//! EBU R128 integrated loudness, used as a playback offset so takes compare at equal level.

use crate::error::{MediaError, Result};

/// Integrated LUFS of interleaved samples. `None` for silence or very short clips.
pub fn integrated_lufs(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Option<f64>> {
    if samples.len() < (sample_rate as usize * channels as usize) / 2 {
        // shorter than 400 ms gating blocks: fall back to RMS-based estimate
        return Ok(rms_lufs(samples));
    }
    let mut m = ebur128::EbuR128::new(channels as u32, sample_rate, ebur128::Mode::I)
        .map_err(|e| MediaError::Loudness(e.to_string()))?;
    m.add_frames_f32(samples)
        .map_err(|e| MediaError::Loudness(e.to_string()))?;
    let l = m
        .loudness_global()
        .map_err(|e| MediaError::Loudness(e.to_string()))?;
    Ok(l.is_finite().then_some(l))
}

fn rms_lufs(samples: &[f32]) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let ms: f64 = samples.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / samples.len() as f64;
    (ms > 1e-10).then(|| -0.691 + 10.0 * ms.log10())
}

/// Gain (dB) that brings `lufs` to `target`, limited to ±12 dB.
pub fn match_gain_db(lufs: Option<f64>, target: f64) -> f64 {
    lufs.map(|l| (target - l).clamp(-12.0, 12.0)).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn louder_tone_measures_louder() {
        let sr = 48000;
        let tone = |a: f32| -> Vec<f32> {
            (0..sr * 2)
                .map(|i| a * (i as f32 * 1000.0 * std::f32::consts::TAU / sr as f32).sin())
                .collect()
        };
        let quiet = integrated_lufs(&tone(0.1), sr as u32, 1).unwrap().unwrap();
        let loud = integrated_lufs(&tone(0.4), sr as u32, 1).unwrap().unwrap();
        assert!((loud - quiet - 12.04).abs() < 0.2, "{quiet} {loud}");
        assert_eq!(match_gain_db(Some(-30.0), -23.0), 7.0);
        assert_eq!(match_gain_db(None, -23.0), 0.0);
    }
}
