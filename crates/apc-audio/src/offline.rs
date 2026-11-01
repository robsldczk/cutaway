//! Offline render through the same mixer as playback.

use std::path::Path;

use crate::error::Result;
use crate::mixer::{Mixer, SourceSet};
use crate::schedule::PlaySchedule;

/// Renders the whole schedule to stereo frames at the source set's rate.
pub fn render_to_buffer(s: &PlaySchedule, sources: &SourceSet) -> Result<Vec<f32>> {
    sources.preload(s)?;
    let frames = (s.duration_s * sources.rate() as f64).round() as usize;
    let mut out = vec![0.0f32; frames * 2];
    const BLOCK: usize = 4096;
    let mut f = 0usize;
    while f < frames {
        let n = BLOCK.min(frames - f);
        Mixer::render(s, sources, f as u64, &mut out[2 * f..2 * (f + n)]);
        f += n;
    }
    for v in out.iter_mut() {
        *v = v.clamp(-1.0, 1.0);
    }
    Ok(out)
}

/// Renders to a 24-bit stereo WAV. Returns the length in seconds.
pub fn render_to_wav(s: &PlaySchedule, sources: &SourceSet, path: &Path) -> Result<f64> {
    let buf = render_to_buffer(s, sources)?;
    apc_media::wav::write_wav24(path, sources.rate(), 2, &buf)?;
    Ok(buf.len() as f64 / 2.0 / sources.rate() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::ScheduledClip;

    fn clip(src_in: f64, src_out: f64, start: f64, fade: f64) -> ScheduledClip {
        ScheduledClip {
            source_id: "s".into(),
            src_in,
            src_out,
            start_s: start,
            gain_db: 0.0,
            fade_in_s: fade,
            fade_out_s: fade,
            lane: "PAUL".into(),
            loop_len_s: None,
            pan: 0.0,
            linear_fade: false,
            owner: "l".into(),
        }
    }

    fn sine_source(dir: &Path) -> SourceSet {
        let sr = 48000u32;
        // 4 s of a loud 220 Hz sine: a splice between unrelated phases would click
        let s: Vec<f32> = (0..sr * 4)
            .map(|i| 0.8 * (i as f32 * 220.0 * std::f32::consts::TAU / sr as f32).sin())
            .collect();
        let p = dir.join("sine.wav");
        apc_media::wav::write_wav(&p, sr, 1, &s).unwrap();
        let mut set = SourceSet::new(sr);
        set.add("s", p);
        set
    }

    #[test]
    fn splices_do_not_click() {
        let dir = tempfile::tempdir().unwrap();
        let set = sine_source(dir.path());
        // two pieces from different phases butted together with 10 ms equal-power fades
        let s = PlaySchedule {
            clips: vec![
                clip(0.5013, 1.5013, 0.0, 0.010),
                clip(2.2271, 3.0, 0.99, 0.010),
            ],
            duration_s: 1.99 + 0.7729,
        };
        let out = render_to_buffer(&s, &set).unwrap();
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let max_step = left
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        // the sine itself steps at most 0.8 * 2π·220/48000 ≈ 0.023 per sample
        assert!(max_step < 0.05, "max sample jump {max_step}");
        // without fades the same splice jumps hard
        let hard = PlaySchedule {
            clips: vec![clip(0.5013, 1.5013, 0.0, 0.0), clip(2.2271, 3.0, 1.0, 0.0)],
            duration_s: 1.77,
        };
        let out = render_to_buffer(&hard, &set).unwrap();
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let max_hard = left
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_hard > 0.3,
            "the test signal must expose clicks ({max_hard})"
        );
    }

    #[test]
    fn renders_source_samples_bit_exact() {
        let dir = tempfile::tempdir().unwrap();
        let set = sine_source(dir.path());
        let mut reader = apc_media::AudioReader::open(&dir.path().join("sine.wav")).unwrap();
        let src = reader.read_range(0.5, 1.5).unwrap();
        // whole-sample clip starts used to land between samples and repeat/skip them
        for start in [0.0, 0.25, 1.5, 0.123_456] {
            let s = PlaySchedule {
                clips: vec![clip(0.5, 1.5, start, 0.0)],
                duration_s: start + 1.0,
            };
            let out = render_to_buffer(&s, &set).unwrap();
            let first = (start * 48000.0).round() as usize;
            let left: Vec<f32> = out.iter().step_by(2).skip(first).copied().collect();
            let wrong = left
                .iter()
                .zip(&src)
                .filter(|(a, b)| (*a - *b).abs() > 1e-6)
                .count();
            assert_eq!(wrong, 0, "start {start}: {wrong} samples differ");
        }
    }

    #[test]
    fn render_length_is_exact() {
        let dir = tempfile::tempdir().unwrap();
        let set = sine_source(dir.path());
        let s = PlaySchedule {
            clips: vec![clip(0.0, 1.0, 0.25, 0.005)],
            duration_s: 1.25,
        };
        let p = dir.path().join("out.wav");
        let len = render_to_wav(&s, &set, &p).unwrap();
        assert!((len - 1.25).abs() < 0.001);
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.spec().channels, 2);
        assert_eq!(r.duration(), 60000);
    }
}
