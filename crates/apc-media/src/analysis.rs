//! One streaming pass over a file: RMS envelope (100 fps), fine envelope (1 kHz) for sync,
//! and min/max peaks (100 per second) for waveforms. Cached in `Cache/` by file hash.

use std::io::{Read, Write};
use std::path::Path;

use crate::decode::{AudioReader, MediaInfo};
use crate::error::{MediaError, Result};

pub const ENV_FPS: u32 = 100;
pub const FINE_FPS: u32 = 1000;
pub const PEAK_FPS: u32 = 100;
const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioAnalysis {
    pub info: MediaInfo,
    /// RMS per 10 ms frame (mono mix).
    pub env: Vec<f32>,
    /// RMS per 1 ms frame (mono mix).
    pub fine: Vec<f32>,
    /// (min, max) per 10 ms frame.
    pub peaks: Vec<(f32, f32)>,
}

impl AudioAnalysis {
    pub fn compute(path: &Path) -> Result<Self> {
        let mut r = AudioReader::open(path)?;
        let mut info = r.info();
        let sr = info.sample_rate as usize;
        let ch = info.channels as usize;
        let hop = (sr / ENV_FPS as usize).max(1);
        let fine_hop = (sr / FINE_FPS as usize).max(1);
        let mut env = Vec::new();
        let mut fine = Vec::new();
        let mut peaks = Vec::new();
        let (mut acc, mut n, mut lo, mut hi) = (0.0f64, 0usize, 0.0f32, 0.0f32);
        let (mut facc, mut fnum) = (0.0f64, 0usize);
        let mut buf = Vec::with_capacity(8192 * ch);
        let mut total = 0u64;
        loop {
            buf.clear();
            let got = r.read(8192, &mut buf)?;
            if got == 0 {
                break;
            }
            total += got as u64;
            for f in buf.chunks_exact(ch) {
                let m = f.iter().sum::<f32>() / ch as f32;
                let sq = (m as f64) * (m as f64);
                acc += sq;
                facc += sq;
                n += 1;
                fnum += 1;
                lo = lo.min(m);
                hi = hi.max(m);
                if fnum == fine_hop {
                    fine.push((facc / fnum as f64).sqrt() as f32);
                    facc = 0.0;
                    fnum = 0;
                }
                if n == hop {
                    env.push((acc / n as f64).sqrt() as f32);
                    peaks.push((lo, hi));
                    acc = 0.0;
                    n = 0;
                    lo = 0.0;
                    hi = 0.0;
                }
            }
        }
        if n > 0 {
            env.push((acc / n as f64).sqrt() as f32);
            peaks.push((lo, hi));
        }
        info.frames = Some(total);
        info.duration_s = total as f64 / sr as f64;
        Ok(AudioAnalysis {
            info,
            env,
            fine,
            peaks,
        })
    }

    /// Loads from `cache_dir/<hash>.env` or computes and stores it.
    pub fn cached(path: &Path, hash: &str, cache_dir: &Path) -> Result<Self> {
        let file = cache_dir.join(format!("{hash}.env"));
        if let Ok(a) = Self::load(&file) {
            return Ok(a);
        }
        let a = Self::compute(path)?;
        std::fs::create_dir_all(cache_dir)?;
        a.save(&file)?;
        Ok(a)
    }

    fn save(&self, file: &Path) -> Result<()> {
        let mut out = Vec::with_capacity(32 + 4 * (self.env.len() * 3 + self.fine.len()));
        for v in [
            CACHE_VERSION,
            self.info.sample_rate,
            self.info.channels as u32,
            self.env.len() as u32,
            self.fine.len() as u32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.info.frames.unwrap_or(0).to_le_bytes());
        for v in &self.env {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in &self.fine {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (a, b) in &self.peaks {
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
        let tmp = file.with_extension("env.tmp");
        std::fs::File::create(&tmp)?.write_all(&out)?;
        std::fs::rename(tmp, file)?;
        Ok(())
    }

    fn load(file: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(file)?.read_to_end(&mut bytes)?;
        let bad = || MediaError::Invalid("bad envelope cache".into());
        let u32_at = |i: usize| -> Result<u32> {
            Ok(u32::from_le_bytes(
                bytes
                    .get(i..i + 4)
                    .ok_or_else(bad)?
                    .try_into()
                    .map_err(|_| bad())?,
            ))
        };
        if u32_at(0)? != CACHE_VERSION {
            return Err(bad());
        }
        let sr = u32_at(4)?;
        let ch = u32_at(8)? as u16;
        let ne = u32_at(12)? as usize;
        let nf = u32_at(16)? as usize;
        let frames = u64::from_le_bytes(
            bytes
                .get(20..28)
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
        );
        let mut pos = 28;
        let mut f32s = |count: usize| -> Result<Vec<f32>> {
            let end = pos + count * 4;
            let s = bytes.get(pos..end).ok_or_else(bad)?;
            pos = end;
            Ok(s.chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect())
        };
        let env = f32s(ne)?;
        let fine = f32s(nf)?;
        let pk = f32s(ne * 2)?;
        Ok(AudioAnalysis {
            info: MediaInfo {
                sample_rate: sr,
                channels: ch,
                frames: Some(frames),
                duration_s: frames as f64 / sr.max(1) as f64,
            },
            env,
            fine,
            peaks: pk.chunks_exact(2).map(|c| (c[0], c[1])).collect(),
        })
    }

    /// RMS level in dB over `[t0, t1)` (Stage 0 `rms_db`).
    pub fn rms_db(&self, t0: f64, t1: f64) -> f64 {
        let a = ((t0 * ENV_FPS as f64) as usize).min(self.env.len());
        let b = ((t1 * ENV_FPS as f64) as usize).min(self.env.len());
        if b <= a {
            return -99.0;
        }
        let mean_sq: f64 = self.env[a..b]
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>()
            / (b - a) as f64;
        20.0 * (mean_sq.sqrt() + 1e-9).log10()
    }

    /// Envelope value at the given percentile (0..100).
    pub fn percentile(&self, p: f64) -> f32 {
        if self.env.is_empty() {
            return 0.0;
        }
        let mut v = self.env.clone();
        v.sort_by(f32::total_cmp);
        let i = ((p / 100.0) * (v.len() - 1) as f64).round() as usize;
        v[i.min(v.len() - 1)]
    }

    /// Waveform peaks over `[t0, t1)` resampled to `buckets` (min, max) pairs.
    pub fn peaks_range(&self, t0: f64, t1: f64, buckets: usize) -> Vec<(f32, f32)> {
        let n = self.peaks.len();
        let a = (t0 * PEAK_FPS as f64).max(0.0);
        let b = (t1 * PEAK_FPS as f64).max(a + 1.0);
        (0..buckets)
            .map(|k| {
                let s = (a + (b - a) * k as f64 / buckets as f64) as usize;
                let e = ((a + (b - a) * (k + 1) as f64 / buckets as f64) as usize).max(s + 1);
                let (mut lo, mut hi) = (0.0f32, 0.0f32);
                for i in s.min(n)..e.min(n) {
                    lo = lo.min(self.peaks[i].0);
                    hi = hi.max(self.peaks[i].1);
                }
                (lo, hi)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_of_a_tone_burst_and_cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.wav");
        let sr = 48000u32;
        let mut s = vec![0.0f32; sr as usize * 2];
        for (i, v) in s
            .iter_mut()
            .enumerate()
            .skip(sr as usize / 2)
            .take(sr as usize / 2)
        {
            *v = 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / sr as f32).sin();
        }
        crate::wav::write_wav(&p, sr, 1, &s).unwrap();
        let a = AudioAnalysis::compute(&p).unwrap();
        assert_eq!(a.info.sample_rate, sr);
        assert_eq!(a.env.len(), 200);
        assert_eq!(a.fine.len(), 2000);
        assert!(a.env[10] < 1e-4);
        assert!((a.env[70] - 0.5 / 2f32.sqrt()).abs() < 0.01);
        assert!(a.rms_db(0.6, 0.9) > -10.0 && a.rms_db(0.0, 0.4) < -60.0);
        let pk = a.peaks_range(0.5, 1.0, 10);
        assert!(pk.iter().all(|(lo, hi)| *hi > 0.45 && *lo < -0.45));
        let c = AudioAnalysis::cached(&p, "h", dir.path()).unwrap();
        let c2 = AudioAnalysis::cached(&p, "h", dir.path()).unwrap();
        assert_eq!(c, c2);
        assert_eq!(c.env, a.env);
    }

    #[test]
    fn range_read_is_sample_accurate() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("ramp.wav");
        let s: Vec<f32> = (0..48000).map(|i| i as f32 / 48000.0).collect();
        crate::wav::write_wav(&p, 48000, 1, &s).unwrap();
        let mut r = AudioReader::open(&p).unwrap();
        let v = r.read_range(0.5, 0.6).unwrap();
        assert_eq!(v.len(), 4800);
        assert!((v[0] - 0.5).abs() < 1e-3, "{}", v[0]);
        let v = r.read_range(0.25, 0.26).unwrap();
        assert!((v[0] - 0.25).abs() < 1e-3);
    }
}
