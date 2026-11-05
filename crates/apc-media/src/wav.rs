//! WAV writing (exports, bounces, test signals).

use std::path::Path;

use crate::error::Result;

/// Writes 32-bit float WAV.
pub fn write_wav(path: &Path, sample_rate: u32, channels: u16, interleaved: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in interleaved {
        w.write_sample(*s)?;
    }
    w.finalize()?;
    Ok(())
}

/// Writes a WAV at a bit depth: 16 or 24-bit PCM, or 32-bit float. Samples past full scale clip,
/// except in float. No dither at 16 bits: a silent stem has to stay digital silence (D109).
pub fn write_wav_bits(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    interleaved: &[f32],
    bits: u16,
) -> Result<()> {
    match bits {
        32 => write_wav(path, sample_rate, channels, interleaved),
        24 => write_wav24(path, sample_rate, channels, interleaved),
        _ => write_wav16(path, sample_rate, channels, interleaved),
    }
}

fn write_wav16(path: &Path, sample_rate: u32, channels: u16, interleaved: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in interleaved {
        w.write_sample((s.clamp(-1.0, 1.0) * 32_767.0).round() as i16)?;
    }
    w.finalize()?;
    Ok(())
}

/// Writes 24-bit PCM WAV (export default: keeps typical session depth).
pub fn write_wav24(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    interleaved: &[f32],
) -> Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in interleaved {
        w.write_sample((s.clamp(-1.0, 1.0) * 8_388_607.0) as i32)?;
    }
    w.finalize()?;
    Ok(())
}
