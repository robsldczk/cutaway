//! Streaming decode through symphonia (WAV/BWF, AIFF, FLAC, MP3).

use std::fs::File;
use std::path::{Path, PathBuf};

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

use crate::error::{MediaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MediaInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: Option<u64>,
    pub duration_s: f64,
}

/// Sequential reader of interleaved f32 frames, with seeking.
pub struct AudioReader {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    info: MediaInfo,
    /// Decoded samples not yet handed out (interleaved).
    pending: Vec<f32>,
    pending_pos: usize,
    /// Frame index of the next frame `read` returns.
    position: u64,
    eof: bool,
}

fn err(path: &Path, e: impl std::fmt::Display) -> MediaError {
    MediaError::Decode {
        path: path.display().to_string(),
        msg: e.to_string(),
    }
}

impl AudioReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| err(path, e))?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| err(path, "no audio track"))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| err(path, "no audio codec parameters"))?
            .clone();
        let sample_rate = params
            .sample_rate
            .ok_or_else(|| err(path, "unknown sample rate"))?;
        let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(1) as u16;
        let frames = track.num_frames;
        let track_id = track.id;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| err(path, e))?;
        let duration_s = frames.map(|f| f as f64 / sample_rate as f64).unwrap_or(0.0);
        Ok(AudioReader {
            path: path.to_path_buf(),
            format,
            decoder,
            track_id,
            info: MediaInfo {
                sample_rate,
                channels: channels.max(1),
                frames,
                duration_s,
            },
            pending: Vec::new(),
            pending_pos: 0,
            position: 0,
            eof: false,
        })
    }

    pub fn info(&self) -> MediaInfo {
        self.info
    }

    /// Frame index of the next frame `read` returns.
    pub fn position(&self) -> u64 {
        self.position
    }

    fn fill(&mut self) -> Result<bool> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymError::ResetRequired) => return Ok(false),
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(false)
                }
                Err(e) => return Err(err(&self.path, e)),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    self.pending.clear();
                    self.pending_pos = 0;
                    buf.copy_to_vec_interleaved(&mut self.pending);
                    if !self.pending.is_empty() {
                        return Ok(true);
                    }
                }
                Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue,
                Err(e) => return Err(err(&self.path, e)),
            }
        }
    }

    /// Appends up to `frames` interleaved frames to `out`. Returns frames read (0 at end).
    pub fn read(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<usize> {
        let ch = self.info.channels as usize;
        let mut done = 0;
        while done < frames && !self.eof {
            if self.pending_pos >= self.pending.len() && !self.fill()? {
                self.eof = true;
                break;
            }
            let avail = (self.pending.len() - self.pending_pos) / ch;
            let take = avail.min(frames - done);
            out.extend_from_slice(&self.pending[self.pending_pos..self.pending_pos + take * ch]);
            self.pending_pos += take * ch;
            done += take;
        }
        self.position += done as u64;
        Ok(done)
    }

    /// Seeks so that the next `read` starts at `t_s` (sample accurate).
    pub fn seek(&mut self, t_s: f64) -> Result<()> {
        let t_s = t_s.max(0.0);
        let target = (t_s * self.info.sample_rate as f64).round() as u64;
        if target == self.position {
            return Ok(());
        }
        let time = Time::try_from_secs_f64(t_s).ok_or_else(|| err(&self.path, "bad seek time"))?;
        match self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        ) {
            Ok(seeked) => {
                self.decoder.reset();
                self.pending.clear();
                self.pending_pos = 0;
                self.eof = false;
                self.position = seeked.actual_ts.get().max(0) as u64;
            }
            Err(_) => {
                // Fallback: reopen and decode forward.
                *self = AudioReader::open(&self.path.clone())?;
            }
        }
        // Discard frames up to the target.
        let mut scratch = Vec::new();
        while self.position < target {
            scratch.clear();
            let n = (target - self.position).min(8192) as usize;
            if self.read(n, &mut scratch)? == 0 {
                break;
            }
        }
        Ok(())
    }

    /// Reads `[t0, t1)` as interleaved frames.
    pub fn read_range(&mut self, t0: f64, t1: f64) -> Result<Vec<f32>> {
        self.seek(t0)?;
        let n = ((t1 - t0).max(0.0) * self.info.sample_rate as f64).round() as usize;
        let mut out = Vec::with_capacity(n * self.info.channels as usize);
        self.read(n, &mut out)?;
        Ok(out)
    }
}

/// Probes a file without decoding it all (duration may need a decode pass for MP3).
pub fn probe(path: &Path) -> Result<MediaInfo> {
    Ok(AudioReader::open(path)?.info())
}

/// Mixes interleaved frames down to mono.
pub fn to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect()
}
