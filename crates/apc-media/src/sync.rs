//! Sync offset between two recordings of the same session by envelope cross-correlation.
//! Coarse at 100 fps over ±max_lag, then fine at 1 kHz around the best coarse lag.

use crate::analysis::{AudioAnalysis, ENV_FPS, FINE_FPS};

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SyncResult {
    /// `b` runs this many seconds late against `a` (b(t + offset) ≈ a(t)).
    pub offset_s: f64,
    /// Normalized correlation at the best lag (0..1).
    pub confidence: f64,
}

/// Log-energy onset curve: half-wave rectified derivative of the dB envelope.
fn onset(env: &[f32]) -> Vec<f32> {
    let db: Vec<f32> = env.iter().map(|v| 20.0 * (v + 1e-5).log10()).collect();
    let mut floor: Vec<f32> = db.clone();
    floor.sort_by(f32::total_cmp);
    let f = floor.get(floor.len() / 5).copied().unwrap_or(-100.0);
    let c: Vec<f32> = db.iter().map(|v| v.max(f) - f).collect();
    let mut out = vec![0.0; c.len()];
    for i in 1..c.len() {
        out[i] = (c[i] - c[i - 1]).max(0.0);
    }
    out
}

fn ncc_at(a: &[f32], b: &[f32], lag: i64) -> f64 {
    // correlate a[i] with b[i + lag]
    let n = a.len() as i64;
    let m = b.len() as i64;
    let start = 0.max(-lag);
    let end = n.min(m - lag);
    if end - start < 10 {
        return 0.0;
    }
    let (mut sab, mut saa, mut sbb) = (0.0f64, 0.0f64, 0.0f64);
    for i in start..end {
        let x = a[i as usize] as f64;
        let y = b[(i + lag) as usize] as f64;
        sab += x * y;
        saa += x * x;
        sbb += y * y;
    }
    if saa <= 0.0 || sbb <= 0.0 {
        return 0.0;
    }
    sab / (saa.sqrt() * sbb.sqrt())
}

fn best_lag(a: &[f32], b: &[f32], center: i64, radius: i64) -> (i64, f64) {
    let mut best = (center, f64::MIN);
    for lag in center - radius..=center + radius {
        let c = ncc_at(a, b, lag);
        if c > best.1 {
            best = (lag, c);
        }
    }
    best
}

/// Measures how late `b` runs against `a`, searching ±`max_lag_s`.
pub fn measure_offset(a: &AudioAnalysis, b: &AudioAnalysis, max_lag_s: f64) -> SyncResult {
    let (oa, ob) = (onset(&a.env), onset(&b.env));
    let radius = (max_lag_s * ENV_FPS as f64) as i64;
    let (coarse, conf) = best_lag(&oa, &ob, 0, radius);
    let (fa, fb) = (onset(&a.fine), onset(&b.fine));
    if fa.is_empty() || fb.is_empty() {
        return SyncResult {
            offset_s: coarse as f64 / ENV_FPS as f64,
            confidence: conf.max(0.0),
        };
    }
    let k = (FINE_FPS / ENV_FPS) as i64;
    let (fine, _) = best_lag(&fa, &fb, coarse * k, 2 * k);
    SyncResult {
        offset_s: fine as f64 / FINE_FPS as f64,
        confidence: conf.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::MediaInfo;

    fn analysis_from(env_fn: impl Fn(f64) -> f32, secs: f64) -> AudioAnalysis {
        let env = (0..(secs * 100.0) as usize)
            .map(|i| env_fn(i as f64 / 100.0))
            .collect();
        let fine = (0..(secs * 1000.0) as usize)
            .map(|i| env_fn(i as f64 / 1000.0))
            .collect();
        AudioAnalysis {
            info: MediaInfo {
                sample_rate: 48000,
                channels: 1,
                frames: None,
                duration_s: secs,
            },
            env,
            fine,
            peaks: vec![],
        }
    }

    #[test]
    fn finds_a_quarter_second_delay() {
        // bursts at pseudo-random times
        let bursts: Vec<f64> = (0..40)
            .map(|i| 1.0 + i as f64 * 1.37 + ((i * 7) % 5) as f64 * 0.21)
            .collect();
        let sig = |delay: f64, gain: f32| {
            let bursts = bursts.clone();
            move |t: f64| {
                let on = bursts.iter().any(|b| t >= b + delay && t < b + delay + 0.3);
                if on {
                    gain
                } else {
                    0.001
                }
            }
        };
        let a = analysis_from(sig(0.0, 0.3), 60.0);
        let b = analysis_from(sig(0.25, 0.05), 60.0);
        let r = measure_offset(&a, &b, 2.0);
        assert!((r.offset_s - 0.25).abs() < 0.005, "{r:?}");
        assert!(r.confidence > 0.5);
    }
}
