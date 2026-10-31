//! Clip edges snapped to speech in the role track's loudness envelope (Stage 0 `refine`).

use apc_media::ENV_FPS;

/// Snaps `[t0, t1]` to speech onset/offset; returns the new edges with 80 ms pre-roll and
/// 150 ms tail.
pub fn refine(env: &[f32], t0: f64, t1: f64, thr: f32) -> (f64, f64) {
    let fps = ENV_FPS as f64;
    let n = env.len();
    if n == 0 {
        return (t0, t1);
    }
    let at = |i: usize| env[i.min(n - 1)];
    let a = (t0 * fps).max(0.0) as usize;
    let b = (t1 * fps).max(0.0) as usize;
    let mut s = a;
    if at(s) > thr {
        let lim = a.saturating_sub(50);
        while s > lim && at(s - 1) > thr * 0.5 {
            s -= 1;
        }
    } else {
        let lim = (n - 1).min(b);
        while s < lim && at(s) <= thr {
            s += 1;
        }
    }
    let mut e = b;
    if at(e) > thr {
        let lim = (n - 1).min(b + 60);
        while e < lim && at(e) > thr * 0.5 {
            e += 1;
        }
    } else {
        let lim = s + 10;
        while e > lim && at(e - 1) <= thr {
            e -= 1;
        }
    }
    ((s as f64 / fps - 0.08).max(0.0), e as f64 / fps + 0.15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_to_the_burst() {
        // silence, speech from 1.00 s to 2.00 s, silence
        let env: Vec<f32> = (0..300)
            .map(|i| if (100..200).contains(&i) { 0.2 } else { 0.001 })
            .collect();
        let (a, b) = refine(&env, 0.9, 1.9, 0.05);
        assert!((a - 0.92).abs() < 1e-9, "{a}");
        assert!((b - 2.15).abs() < 1e-9, "{b}");
        let (a, b) = refine(&env, 1.2, 2.3, 0.05);
        assert!((a - 0.92).abs() < 1e-9, "{a}");
        assert!((b - 2.15).abs() < 1e-9, "{b}");
    }
}
