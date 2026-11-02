//! Playback leveling: evens out recordings made at different levels, and a safety limiter
//! on the device output. Playback only; bounces and exports never go through it.

use std::collections::HashMap;

use apc_core::Document;

/// Typical speech loudness of each source: the median LUFS of the takes on it. Takes are
/// the role's own lines, so silence, bleed and room talk don't pull the number down.
pub fn source_lufs(doc: &Document) -> HashMap<String, f64> {
    let mut per: HashMap<&str, Vec<f64>> = HashMap::new();
    for t in doc.takes.values() {
        if let Some(l) = t.lufs.filter(|l| l.is_finite()) {
            per.entry(t.source_id.as_str()).or_default().push(l);
        }
    }
    per.into_iter()
        .map(|(id, mut v)| {
            v.sort_by(f64::total_cmp);
            let m = v.len() / 2;
            let median = if v.len() % 2 == 0 {
                (v[m - 1] + v[m]) / 2.0
            } else {
                v[m]
            };
            (id.to_string(), median)
        })
        .collect()
}

/// Output ceiling of the safety limiter: −1 dBFS.
pub const CEILING: f32 = 0.891;

/// Peak limiter with instant attack and a 150 ms release. Keeps a blast (a scream on a hot
/// mic, a sound mastered loud) under the ceiling instead of clipping into the ears.
pub struct Limiter {
    gain: f32,
    release: f32,
}

impl Limiter {
    pub fn new(rate: u32) -> Self {
        Limiter {
            gain: 1.0,
            release: 1.0 - (-1.0 / (0.15 * rate.max(1) as f32)).exp(),
        }
    }

    /// Limits stereo interleaved samples in place.
    pub fn process(&mut self, block: &mut [f32]) {
        for f in block.chunks_exact_mut(2) {
            if !(f[0].is_finite() && f[1].is_finite()) {
                f.fill(0.0);
                continue;
            }
            let peak = f[0].abs().max(f[1].abs());
            self.gain += (1.0 - self.gain) * self.release;
            if peak * self.gain > CEILING {
                self.gain = CEILING / peak;
            }
            f[0] *= self.gain;
            f[1] *= self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_holds_the_ceiling_and_recovers() {
        let rate = 48000;
        let mut lim = Limiter::new(rate);
        let mut quiet = vec![0.2f32; 200];
        lim.process(&mut quiet);
        assert!(
            quiet.iter().all(|v| (*v - 0.2).abs() < 1e-6),
            "quiet passes untouched"
        );
        let mut blast: Vec<f32> = (0..9600)
            .map(|i| if i % 2 == 0 { 3.0 } else { -2.0 })
            .collect();
        lim.process(&mut blast);
        assert!(blast.iter().all(|v| v.abs() <= CEILING + 1e-6));
        // half a second after the blast the gain is back near unity
        let mut after = vec![0.2f32; rate as usize];
        lim.process(&mut after);
        assert!((after[after.len() - 1] - 0.2).abs() < 0.01);
        let mut bad = vec![f32::NAN, 0.5];
        lim.process(&mut bad);
        assert_eq!(bad, vec![0.0, 0.0]);
    }
}
