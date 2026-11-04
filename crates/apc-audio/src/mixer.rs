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

use crate::depth::LOWPASS_OPEN_HZ;
use crate::error::{AudioError, Result};
use crate::room::Room;
use crate::schedule::{PlaySchedule, ScheduledClip};

/// Source, in and out point, and the roll-off (0 = none).
type ClipKey = (String, u64, u64, u32, bool);

/// Decoded clip audio (mono, at the mix rate), cached per source range.
pub struct SourceSet {
    rate: u32,
    paths: HashMap<String, PathBuf>,
    readers: Mutex<HashMap<String, AudioReader>>,
    cache: Mutex<HashMap<ClipKey, Arc<Vec<f32>>>>,
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

/// Longest crossfade at a loop's seam.
const SEAM_S: f64 = 0.5;

fn seam_s(len_s: f64) -> f64 {
    SEAM_S.min(len_s / 4.0)
}

/// How often a loop of `len_s` repeats: its seam overlaps, so a little less than its length.
pub fn loop_period_s(len_s: f64) -> f64 {
    len_s - seam_s(len_s)
}

/// A clip ready to repeat: its tail crossfades into its head, so the seam of a file that was
/// never cut to loop doesn't click. Equal power: room tone and beds don't correlate.
fn seamless(x: &[f32], rate: u32) -> Vec<f32> {
    let n = (seam_s(x.len() as f64 / f64::from(rate.max(1))) * f64::from(rate)) as usize;
    if n == 0 || x.len() < 2 * n {
        return x.to_vec();
    }
    let period = x.len() - n;
    let mut y = x[..period].to_vec();
    for i in 0..n {
        let t = (i as f64 + 0.5) / n as f64 * std::f64::consts::FRAC_PI_2;
        y[i] = x[i] * t.sin() as f32 + x[period + i] * t.cos() as f32;
    }
    y
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
    pub fn clip(&self, c: &ScheduledClip) -> Result<Arc<Vec<f32>>> {
        let hz = c
            .space
            .map(|s| s.lowpass_hz)
            .filter(|hz| *hz < LOWPASS_OPEN_HZ);
        let key = (
            c.source_id.clone(),
            (c.src_in * 1e4) as u64,
            (c.src_out * 1e4) as u64,
            hz.map(|h| h.round() as u32).unwrap_or(0),
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
        let v = Arc::new(if c.loop_len_s.is_some() {
            seamless(&raw, self.rate)
        } else {
            raw
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
                let mut idx = k as usize;
                if c.loop_len_s.is_some() {
                    idx %= buf.len();
                } else if idx >= buf.len() {
                    break;
                }
                let v = buf[idx]
                    * gain
                    * fade_gain(local, len, c.fade_in_s, c.fade_out_s, c.linear_fade);
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

/// Renders one schedule from any position, room included. One per loaded schedule: the
/// bounce runs it from frame 0, the engine from wherever playback starts.
pub struct Renderer {
    room: Option<Room>,
    /// The frame the room has heard up to.
    next_frame: Option<u64>,
    send: Vec<f32>,
}

impl Renderer {
    pub fn new(s: &PlaySchedule, rate: u32) -> Self {
        Renderer {
            room: s.room.map(|shape| Room::new(rate, shape)),
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
            Mixer::mix(s, sources, start_frame, frames, Some(out), None);
            return;
        };
        if self.next_frame != Some(start_frame) {
            warm_up(room, s, sources, start_frame, &mut self.send);
        }
        self.send.clear();
        self.send.resize(out.len(), 0.0);
        Mixer::mix(
            s,
            sources,
            start_frame,
            frames,
            Some(out),
            Some(&mut self.send),
        );
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

    #[test]
    fn a_loop_seam_is_continuous() {
        // a ramp jumps from 1 back to 0 at the end of the file
        let rate = 1000;
        let x: Vec<f32> = (0..4000).map(|i| i as f32 / 4000.0).collect();
        let y = seamless(&x, rate);
        assert_eq!(y.len(), 3500, "the seam overlaps by half a second");
        let jump = (y[0] - y[y.len() - 1]).abs();
        assert!(jump < 0.02, "wrapping around jumps by {jump}");
        assert_eq!(y[2000], x[2000], "the middle stays the file");
        assert!((loop_period_s(4.0) - 3.5).abs() < 1e-9);
    }

    #[test]
    fn a_short_loop_has_a_short_seam() {
        assert!((loop_period_s(1.0) - 0.75).abs() < 1e-9);
        assert_eq!(seamless(&[], 48000), Vec::<f32>::new());
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
