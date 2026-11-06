//! The mixer: renders a play schedule into stereo frames. Used by the real-time engine and
//! the offline bounce, so both hear exactly the same thing.
//!
//! Sounds meet on the room bus: `Mixer::mix` fills a dry and a send bus and holds no state, the
//! `Renderer` runs the send through the scene's room. The room has a memory, so a `Renderer`
//! that jumps to a new position first plays the room what it would have heard.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use apc_media::decode::to_mono;
use apc_media::AudioReader;

use crate::depth::{LINE_HIGH_SHELF_HZ, LINE_LOW_SHELF_HZ, LOWPASS_OPEN_HZ};
use crate::error::{AudioError, Result};
use crate::room::Room;
use crate::schedule::{PlaySchedule, ScheduledClip};

/// Source, in and out point, the roll-off (0 = none), the shelves in hundredths of a dB, looped.
type ClipKey = (String, u64, u64, u32, i32, i32, bool);

/// Decoded clip audio (mono, at the mix rate), cached per source range.
pub struct SourceSet {
    rate: u32,
    paths: HashMap<String, PathBuf>,
    readers: Mutex<HashMap<String, AudioReader>>,
    cache: Mutex<HashMap<ClipKey, Arc<ClipAudio>>>,
}

fn resample_linear(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (x.len() as f64 / ratio).floor() as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * ratio;
            let k = p as usize;
            let f = (p - k as f64) as f32;
            let a = x[k.min(x.len() - 1)];
            let b = x[(k + 1).min(x.len() - 1)];
            a + (b - a) * f
        })
        .collect()
}

/// Where a loop turns around, as parts of its file. Files often fade in and out, so it stays
/// clear of both ends: from well before the end back to about the middle (D93).
const LOOP_TURN_AT: f64 = 0.85;
const LOOP_BACK_TO: f64 = 0.45;
/// Longest crossfade at the turn. It's cut from what follows the turn, so the end of the file
/// sets the limit on a short one.
const SEAM_MAX_S: f64 = 3.0;

/// When a loop of a `len_s` file first turns around, and how long each round is after that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopPoints {
    pub first_s: f64,
    pub period_s: f64,
}

pub fn loop_points(len_s: f64) -> LoopPoints {
    LoopPoints {
        first_s: len_s * LOOP_TURN_AT,
        period_s: len_s * (LOOP_TURN_AT - LOOP_BACK_TO),
    }
}

/// A clip's samples, at the mix rate. A looping clip plays through to `turn` and then repeats
/// what follows it.
#[derive(Debug, PartialEq)]
pub struct ClipAudio {
    samples: Vec<f32>,
    turn: usize,
}

impl std::ops::Deref for ClipAudio {
    type Target = Vec<f32>;
    fn deref(&self) -> &Vec<f32> {
        &self.samples
    }
}

impl ClipAudio {
    /// A clip that plays once.
    fn once(samples: Vec<f32>) -> Self {
        let turn = samples.len();
        ClipAudio { samples, turn }
    }

    /// A clip ready to repeat: the file up to the turn, then one round from the middle whose
    /// start crossfades out of what follows the turn, so going round is continuous. Equal
    /// power: room tone and beds don't correlate.
    fn looped(x: &[f32], rate: u32) -> Self {
        let n = x.len();
        let turn = (n as f64 * LOOP_TURN_AT) as usize;
        let back = (n as f64 * LOOP_BACK_TO) as usize;
        let seam = ((SEAM_MAX_S * f64::from(rate)) as usize).min(n - turn);
        if seam == 0 || back >= turn {
            return ClipAudio {
                samples: x.to_vec(),
                turn: 0,
            };
        }
        let mut samples = x[..turn].to_vec();
        samples.extend_from_slice(&x[back..turn]);
        for i in 0..seam {
            let t = (i as f64 + 0.5) / seam as f64 * std::f64::consts::FRAC_PI_2;
            samples[turn + i] = x[back + i] * t.sin() as f32 + x[turn + i] * t.cos() as f32;
        }
        ClipAudio { samples, turn }
    }

    /// The sample for frame `k` of a loop.
    fn wrapped(&self, k: usize) -> Option<f32> {
        let n = self.samples.len();
        if k < n {
            return self.samples.get(k).copied();
        }
        let round = n.checked_sub(self.turn).filter(|r| *r > 0)?;
        self.samples
            .get(self.turn + (k - self.turn) % round)
            .copied()
    }
}

/// The gentle roll-off of a far sound: one pole, run over the whole clip from its first sample,
/// so it sounds the same wherever playback starts.
fn lowpass(x: &[f32], hz: f64, rate: u32) -> Vec<f32> {
    let a = 1.0 - (-std::f64::consts::TAU * hz / f64::from(rate.max(1))).exp();
    let a = a as f32;
    let mut y = 0.0f32;
    x.iter()
        .map(|v| {
            y += a * (v - y);
            y
        })
        .collect()
}

/// A shelf of `db` at `hz`, a biquad with the cookbook's coefficients at a slope of 1. Run over
/// the whole clip from its first sample, like the roll-off.
fn shelf(x: &[f32], hz: f64, db: f64, high: bool, rate: u32) -> Vec<f32> {
    let a = 10f64.powf(db / 40.0);
    let w = std::f64::consts::TAU * hz / f64::from(rate.max(1));
    let (sin, cos) = w.sin_cos();
    let alpha = sin / 2.0 * std::f64::consts::SQRT_2;
    let root = 2.0 * a.sqrt() * alpha;
    // a high shelf is a low one with the sign of the cosine turned
    let c = if high { -cos } else { cos };
    let b0 = a * ((a + 1.0) - (a - 1.0) * c + root);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * c);
    let b2 = a * ((a + 1.0) - (a - 1.0) * c - root);
    let a0 = (a + 1.0) + (a - 1.0) * c + root;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * c);
    let a2 = (a + 1.0) + (a - 1.0) * c - root;
    let turn = if high { -1.0 } else { 1.0 };
    let (b0, b1, b2, a1, a2) = (b0 / a0, turn * b1 / a0, b2 / a0, turn * a1 / a0, a2 / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    x.iter()
        .map(|v| {
            let v = f64::from(*v);
            let y = b0 * v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y as f32
        })
        .collect()
}

impl SourceSet {
    pub fn new(rate: u32) -> Self {
        SourceSet {
            rate,
            paths: HashMap::new(),
            readers: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn add(&mut self, id: impl Into<String>, path: PathBuf) {
        self.paths.insert(id.into(), path);
    }

    /// Mono samples of `[src_in, src_out)` at the mix rate, with the clip's roll-off.
    pub fn clip(&self, c: &ScheduledClip) -> Result<Arc<ClipAudio>> {
        let hz = c
            .space
            .map(|s| s.lowpass_hz)
            .filter(|hz| *hz < LOWPASS_OPEN_HZ);
        let (low_db, high_db) = c
            .space
            .map_or((0.0, 0.0), |s| (s.low_shelf_db, s.high_shelf_db));
        let key = (
            c.source_id.clone(),
            (c.src_in * 1e4) as u64,
            (c.src_out * 1e4) as u64,
            hz.map(|h| h.round() as u32).unwrap_or(0),
            (low_db * 100.0).round() as i32,
            (high_db * 100.0).round() as i32,
            c.loop_len_s.is_some(),
        );
        if let Some(v) = self
            .cache
            .lock()
            .map_err(|_| AudioError::Other("cache poisoned".into()))?
            .get(&key)
        {
            return Ok(v.clone());
        }
        let raw = self.decode(c)?;
        let raw = match hz {
            Some(hz) => lowpass(&raw, hz, self.rate),
            None => raw,
        };
        let raw = match low_db {
            0.0 => raw,
            db => shelf(&raw, LINE_LOW_SHELF_HZ, db, false, self.rate),
        };
        let raw = match high_db {
            0.0 => raw,
            db => shelf(&raw, LINE_HIGH_SHELF_HZ, db, true, self.rate),
        };
        let v = Arc::new(if c.loop_len_s.is_some() {
            ClipAudio::looped(&raw, self.rate)
        } else {
            ClipAudio::once(raw)
        });
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() > 512 {
                cache.clear();
            }
            cache.insert(key, v.clone());
        }
        Ok(v)
    }

    fn decode(&self, c: &ScheduledClip) -> Result<Vec<f32>> {
        let path = self
            .paths
            .get(&c.source_id)
            .ok_or_else(|| AudioError::UnknownSource(c.source_id.clone()))?;
        let mut readers = self
            .readers
            .lock()
            .map_err(|_| AudioError::Other("readers poisoned".into()))?;
        if !readers.contains_key(&c.source_id) {
            readers.insert(c.source_id.clone(), AudioReader::open(path)?);
        }
        let r = readers
            .get_mut(&c.source_id)
            .ok_or_else(|| AudioError::UnknownSource(c.source_id.clone()))?;
        let info = r.info();
        let raw = r.read_range(c.src_in.max(0.0), c.src_out.max(c.src_in))?;
        Ok(resample_linear(
            &to_mono(&raw, info.channels),
            info.sample_rate,
            self.rate,
        ))
    }

    /// Decodes every clip of a schedule ahead of time (offline render, pre-roll).
    pub fn preload(&self, s: &PlaySchedule) -> Result<()> {
        for c in &s.clips {
            self.clip(c)?;
        }
        Ok(())
    }
}

/// Fade envelope at `local` seconds into a clip of length `len` (equal power or linear).
pub fn fade_gain(local: f64, len: f64, fade_in: f64, fade_out: f64, linear: bool) -> f32 {
    let shape = |x: f64| {
        if linear {
            x
        } else {
            (x * std::f64::consts::FRAC_PI_2).sin()
        }
    };
    let mut g = 1.0f64;
    if fade_in > 0.0 && local < fade_in {
        g *= shape(local / fade_in);
    }
    let from_end = len - local;
    if fade_out > 0.0 && from_end < fade_out {
        g *= shape(from_end.max(0.0) / fade_out);
    }
    g.max(0.0) as f32
}

/// Constant-power pan gains (centre = unity on both sides).
pub fn pan_gains(pan: f64) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
    let k = std::f64::consts::SQRT_2;
    ((a.cos() * k) as f32, (a.sin() * k) as f32)
}

pub struct Mixer;

impl Mixer {
    /// Adds the schedule's audio for `frames` frames from `start_frame` into the dry bus and
    /// the room's send bus (stereo interleaved, `frames * 2` samples; leave either out). Holds
    /// no state, so any stretch can be rendered. Missing sources are skipped.
    fn mix(
        s: &PlaySchedule,
        sources: &SourceSet,
        start_frame: u64,
        frames: usize,
        mut dry: Option<&mut [f32]>,
        mut send: Option<&mut [f32]>,
    ) {
        let rate = sources.rate() as f64;
        let t0 = start_frame as f64 / rate;
        let t1 = (start_frame + frames as u64) as f64 / rate;
        for c in s.active(t0, t1) {
            let (dry_gain, send_gain) = c
                .space
                .map(|m| (m.dry_gain as f32, m.send_gain as f32))
                .unwrap_or((1.0, 0.0));
            if dry.is_none() && send_gain == 0.0 {
                continue;
            }
            let Ok(buf) = sources.clip(c) else { continue };
            if buf.is_empty() {
                continue;
            }
            let gain = 10f64.powf(c.gain_db / 20.0) as f32;
            let (gl, gr) = pan_gains(c.pan);
            let len = c.len();
            // Whole frames, not seconds: `floor(t * rate)` lands just under an integer on
            // many samples and repeats/skips them, which is audible as grit.
            let clip_start = (c.start_s * rate).round() as i64;
            let len_frames = (len * rate).round() as i64;
            let first = (clip_start - start_frame as i64).max(0) as usize;
            for n in first..frames {
                let k = start_frame as i64 + n as i64 - clip_start;
                if k >= len_frames {
                    break;
                }
                let local = k as f64 / rate;
                let idx = k as usize;
                let sample = if c.loop_len_s.is_some() {
                    buf.wrapped(idx)
                } else {
                    buf.get(idx).copied()
                };
                let Some(sample) = sample else { break };
                let v =
                    sample * gain * fade_gain(local, len, c.fade_in_s, c.fade_out_s, c.linear_fade);
                if let Some(out) = dry.as_deref_mut() {
                    let d = v * dry_gain;
                    out[2 * n] += d * gl;
                    out[2 * n + 1] += d * gr;
                }
                if send_gain > 0.0 {
                    if let Some(bus) = send.as_deref_mut() {
                        let w = v * send_gain;
                        bus[2 * n] += w * gl;
                        bus[2 * n + 1] += w * gr;
                    }
                }
            }
        }
    }
}

/// Frames the room is warmed up in at a time.
const WARMUP_BLOCK: usize = 4096;

/// Which side of the room a render takes. A stem is one side of some of the clips: the room is
/// linear, so the parts add back up to the whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Direct sound and room, as heard.
    Whole,
    /// The signal before the room send.
    Dry,
    /// The room's 100 % wet return of what was sent to it.
    Wet,
}

/// Renders one schedule from any position, room included. One per loaded schedule: the
/// bounce runs it from frame 0, the engine from wherever playback starts.
pub struct Renderer {
    room: Option<Room>,
    part: Part,
    /// The frame the room has heard up to.
    next_frame: Option<u64>,
    send: Vec<f32>,
}

impl Renderer {
    pub fn new(s: &PlaySchedule, rate: u32) -> Self {
        Self::part(s, rate, Part::Whole)
    }

    /// A renderer of one side of the room.
    pub fn part(s: &PlaySchedule, rate: u32, part: Part) -> Self {
        Renderer {
            room: s
                .room
                .filter(|_| part != Part::Dry)
                .map(|shape| Room::new(rate, shape)),
            part,
            next_frame: None,
            send: Vec::new(),
        }
    }

    /// Adds the schedule's audio for frames `[start_frame, start_frame + frames)` into `out`
    /// (stereo interleaved, `frames * 2` samples). Rendering on from where the last call
    /// stopped sounds the same however the frames are cut into blocks; a jump warms the room.
    pub fn render(
        &mut self,
        s: &PlaySchedule,
        sources: &SourceSet,
        start_frame: u64,
        out: &mut [f32],
    ) {
        let frames = out.len() / 2;
        let Some(room) = self.room.as_mut() else {
            if self.part != Part::Wet {
                Mixer::mix(s, sources, start_frame, frames, Some(out), None);
            }
            return;
        };
        if self.next_frame != Some(start_frame) {
            warm_up(room, s, sources, start_frame, &mut self.send);
        }
        self.send.clear();
        self.send.resize(out.len(), 0.0);
        let dry = (self.part == Part::Whole).then_some(&mut *out);
        Mixer::mix(s, sources, start_frame, frames, dry, Some(&mut self.send));
        room.process(&self.send, out);
        self.next_frame = Some(start_frame + frames as u64);
    }
}

/// Plays the room what it would have heard before `frame`, so a sound that rang out before
/// the playhead still rings. `RoomShape::warmup_s` back is out of hearing.
fn warm_up(
    room: &mut Room,
    s: &PlaySchedule,
    sources: &SourceSet,
    frame: u64,
    send: &mut Vec<f32>,
) {
    let rate = sources.rate() as f64;
    let warm = (room.shape().warmup_s() * rate).ceil() as u64;
    let from = frame.saturating_sub(warm);
    let (t0, t1) = (from as f64 / rate, frame as f64 / rate);
    if !s.active(t0, t1).any(|c| c.has_send()) {
        room.reset(frame);
        return;
    }
    room.reset(from);
    let mut f = from;
    while f < frame {
        let n = (frame - f).min(WARMUP_BLOCK as u64) as usize;
        send.clear();
        send.resize(n * 2, 0.0);
        Mixer::mix(s, sources, f, n, None, Some(send));
        room.absorb(send);
        f += n as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn around(c: &ClipAudio, k: usize) -> f32 {
        c.wrapped(k).unwrap()
    }

    #[test]
    fn a_loop_turns_around_inside_the_file_and_stays_continuous() {
        // a ramp: any jump in the audio is a jump in the value
        let rate = 1000;
        let x: Vec<f32> = (0..20_000).map(|i| i as f32 / 20_000.0).collect();
        let c = ClipAudio::looped(&x, rate);
        let p = loop_points(20.0);
        let turn = (p.first_s * 1000.0).round() as usize;
        let round = (p.period_s * 1000.0).round() as usize;

        assert_eq!(turn, 17_000, "clear of the end");
        assert_eq!(round, 8_000, "back to about the middle");
        assert_eq!(
            around(&c, 16_999),
            x[16_999],
            "the file plays as it is up to the turn"
        );
        // the seam starts on what follows the turn, and ends on what it came back to
        assert!((around(&c, turn) - x[turn]).abs() < 0.001);
        assert!((around(&c, turn + 2_999) - x[9_000 + 2_999]).abs() < 0.001);
        // round after round lands on the same samples, and going round doesn't jump
        assert_eq!(around(&c, turn + round + 5), around(&c, turn + 5));
        let last = around(&c, turn + round - 1);
        assert!((around(&c, turn + round) - last).abs() < 0.001);
    }

    #[test]
    fn a_short_loop_has_a_short_seam() {
        let x = vec![0.5f32; 4000];
        let c = ClipAudio::looped(&x, 1000);
        assert!(around(&c, 100_000).abs() > 0.0);
        let empty = ClipAudio::looped(&[], 48000);
        assert_eq!(empty.wrapped(10), None);
    }

    /// A sine's level through a shelf, in dB against what went in, past the filter's start.
    fn through(hz: f64, shelf_hz: f64, db: f64, high: bool) -> f64 {
        let rate = 48_000u32;
        let x: Vec<f32> = (0..rate)
            .map(|n| (std::f64::consts::TAU * hz * f64::from(n) / f64::from(rate)).sin() as f32)
            .collect();
        let y = shelf(&x, shelf_hz, db, high, rate);
        let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt();
        let n = x.len() / 2;
        20.0 * f64::from(rms(&y[n..]) / rms(&x[n..])).log10()
    }

    #[test]
    fn a_low_shelf_takes_the_chest_and_leaves_the_words() {
        assert!((through(40.0, LINE_LOW_SHELF_HZ, -6.0, false) + 6.0).abs() < 0.3);
        assert!(through(2_000.0, LINE_LOW_SHELF_HZ, -6.0, false).abs() < 0.2);
    }

    #[test]
    fn a_high_shelf_softens_the_top_and_leaves_the_body() {
        assert!((through(16_000.0, LINE_HIGH_SHELF_HZ, -6.0, true) + 6.0).abs() < 0.4);
        assert!(through(400.0, LINE_HIGH_SHELF_HZ, -6.0, true).abs() < 0.2);
        // a shelf, not a roll-off: it stops going down
        let (at8k, at16k) = (
            through(8_000.0, LINE_HIGH_SHELF_HZ, -6.0, true),
            through(16_000.0, LINE_HIGH_SHELF_HZ, -6.0, true),
        );
        assert!((at8k - at16k).abs() < 1.5);
    }

    #[test]
    fn pan_is_constant_power_with_unity_centre() {
        let (l, r) = pan_gains(0.0);
        assert!((l - 1.0).abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let (l, r) = pan_gains(-1.0);
        assert!((l - std::f32::consts::SQRT_2).abs() < 1e-5 && r.abs() < 1e-6);
        let (l, r) = pan_gains(0.3);
        assert!(((l * l + r * r) - 2.0).abs() < 1e-5, "power stays constant");
    }

    #[test]
    fn fades_are_equal_power_or_linear() {
        let eq = fade_gain(0.5, 10.0, 1.0, 0.0, false);
        let lin = fade_gain(0.5, 10.0, 1.0, 0.0, true);
        assert!((lin - 0.5).abs() < 1e-6);
        assert!((eq - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((fade_gain(9.75, 10.0, 0.0, 0.5, true) - 0.5).abs() < 1e-6);
        assert_eq!(fade_gain(5.0, 10.0, 1.0, 1.0, false), 1.0);
    }
}
