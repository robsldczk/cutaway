//! The mixer: renders a play schedule into stereo frames. Used by the real-time engine and
//! the offline bounce, so both hear exactly the same thing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use apc_media::decode::to_mono;
use apc_media::AudioReader;

use crate::error::{AudioError, Result};
use crate::schedule::{PlaySchedule, ScheduledClip};

type ClipKey = (String, u64, u64);

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

    /// Mono samples of `[src_in, src_out)` at the mix rate.
    pub fn clip(&self, c: &ScheduledClip) -> Result<Arc<Vec<f32>>> {
        let key = (
            c.source_id.clone(),
            (c.src_in * 1e4) as u64,
            (c.src_out * 1e4) as u64,
        );
        if let Some(v) = self
            .cache
            .lock()
            .map_err(|_| AudioError::Other("cache poisoned".into()))?
            .get(&key)
        {
            return Ok(v.clone());
        }
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
        let mono = resample_linear(&to_mono(&raw, info.channels), info.sample_rate, self.rate);
        let v = Arc::new(mono);
        drop(readers);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() > 512 {
                cache.clear();
            }
            cache.insert(key, v.clone());
        }
        Ok(v)
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
    /// Adds the schedule's audio for frames `[start_frame, start_frame + frames)` into
    /// `out` (stereo interleaved, `frames * 2` samples). Missing sources are skipped.
    pub fn render(s: &PlaySchedule, sources: &SourceSet, start_frame: u64, out: &mut [f32]) {
        let rate = sources.rate() as f64;
        let frames = out.len() / 2;
        let t0 = start_frame as f64 / rate;
        let t1 = (start_frame + frames as u64) as f64 / rate;
        for c in s.active(t0, t1) {
            let Ok(buf) = sources.clip(c) else { continue };
            if buf.is_empty() {
                continue;
            }
            let gain = 10f64.powf(c.gain_db / 20.0) as f32;
            let (gl, gr) = pan_gains(c.pan);
            let len = c.len();
            let first = ((c.start_s - t0) * rate).ceil().max(0.0) as usize;
            for n in first..frames {
                let local = (start_frame + n as u64) as f64 / rate - c.start_s;
                if local >= len {
                    break;
                }
                let mut idx = (local * rate) as usize;
                if c.loop_len_s.is_some() {
                    idx %= buf.len();
                } else if idx >= buf.len() {
                    break;
                }
                let v = buf[idx]
                    * gain
                    * fade_gain(local, len, c.fade_in_s, c.fade_out_s, c.linear_fade);
                out[2 * n] += v * gl;
                out[2 * n + 1] += v * gr;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
