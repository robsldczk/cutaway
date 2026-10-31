//! Parakeet TDT 0.6B v3 through sherpa-onnx (feature `parakeet`).
//!
//! Long recordings are cut into chunks of at most 25 s at the quietest point near each
//! boundary; tokens with TDT timestamps and durations are merged into words.

use std::path::{Path, PathBuf};

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

use apc_core::text::norm_word;
use apc_media::{decode::to_mono, AudioAnalysis, AudioReader};

use crate::asr::{
    AsrError, AsrProvider, AsrRequest, ModelManager, Segment, TWord, Transcript, PARAKEET_TDT_V3,
};

const MAX_CHUNK_S: f64 = 25.0;
const MIN_CHUNK_S: f64 = 8.0;
const DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";

pub struct ParakeetRuntime {
    recognizer: OfflineRecognizer,
}

/// Unpacks the release tarball next to it (once).
fn unpack(model_dir: &Path) -> Result<PathBuf, AsrError> {
    let out = model_dir.join(DIR);
    if out.join("tokens.txt").exists() {
        return Ok(out);
    }
    let tarball = model_dir.join(PARAKEET_TDT_V3.files[0].name);
    let f = std::fs::File::open(&tarball)?;
    let mut ar = tar::Archive::new(bzip2::read::BzDecoder::new(f));
    ar.unpack(model_dir)?;
    if !out.join("tokens.txt").exists() {
        return Err(AsrError::Download(format!(
            "{} has no tokens.txt",
            out.display()
        )));
    }
    Ok(out)
}

fn default_execution_provider() -> String {
    std::env::var("CUTAWAY_ONNX_PROVIDER").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "coreml".into()
        } else if cfg!(target_os = "windows") {
            "directml".into()
        } else {
            "cpu".into()
        }
    })
}

impl ParakeetRuntime {
    pub fn load(
        models: &ModelManager,
        progress: &dyn Fn(&str, u64, Option<u64>),
    ) -> Result<Self, AsrError> {
        let dir = unpack(&models.ensure(&PARAKEET_TDT_V3, progress)?)?;
        let file = |n: &str| Some(dir.join(n).to_string_lossy().into_owned());
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4)
            .min(8);
        let make = |provider: &str| {
            let mut c = OfflineRecognizerConfig::default();
            c.model_config.transducer = OfflineTransducerModelConfig {
                encoder: file("encoder.int8.onnx"),
                decoder: file("decoder.int8.onnx"),
                joiner: file("joiner.int8.onnx"),
            };
            c.model_config.tokens = file("tokens.txt");
            c.model_config.model_type = Some("nemo_transducer".into());
            c.model_config.num_threads = threads;
            c.model_config.provider = Some(provider.into());
            OfflineRecognizer::create(&c)
        };
        let wanted = default_execution_provider();
        let recognizer = make(&wanted).or_else(|| make("cpu")).ok_or_else(|| {
            AsrError::Unavailable("sherpa-onnx could not load the Parakeet model".into())
        })?;
        Ok(ParakeetRuntime { recognizer })
    }

    /// Recognizes one chunk (mono, any sample rate). Returns words with absolute times.
    fn chunk(&self, samples: &[f32], sr: u32, offset: f64) -> Vec<(String, f64, f64)> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(sr as i32, samples);
        self.recognizer.decode(&stream);
        let Some(r) = stream.get_result() else {
            return Vec::new();
        };
        let ts = r.timestamps.unwrap_or_default();
        let durs = r.durations.unwrap_or_default();
        let mut words: Vec<(String, f64, f64)> = Vec::new();
        for (i, tok) in r.tokens.iter().enumerate() {
            let t = ts.get(i).copied().unwrap_or(0.0) as f64 + offset;
            let d = durs.get(i).copied().unwrap_or(0.08).max(0.02) as f64;
            let starts_word =
                tok.starts_with(' ') || tok.starts_with('\u{2581}') || words.is_empty();
            let piece = tok.trim_start_matches([' ', '\u{2581}']);
            if starts_word {
                words.push((piece.to_string(), t, t + d));
            } else if let Some(w) = words.last_mut() {
                w.0.push_str(piece);
                w.2 = t + d;
            }
        }
        words.retain(|w| !norm_word(&w.0).is_empty());
        words
    }
}

/// Chunk boundaries (seconds): every ≤ 25 s, at the quietest 10 ms frame of the last 8 s.
pub fn chunk_points(a: &AudioAnalysis) -> Vec<(f64, f64)> {
    let fps = apc_media::ENV_FPS as f64;
    let total = a.info.duration_s;
    let mut out = Vec::new();
    let mut start = 0.0;
    while start < total - 0.05 {
        let hard_end = (start + MAX_CHUNK_S).min(total);
        if hard_end >= total {
            out.push((start, total));
            break;
        }
        let lo = ((start + MIN_CHUNK_S) * fps) as usize;
        let hi = ((hard_end * fps) as usize).min(a.env.len().saturating_sub(1));
        let cut = (lo..=hi)
            .min_by(|&x, &y| a.env[x].total_cmp(&a.env[y]))
            .map(|i| i as f64 / fps)
            .unwrap_or(hard_end);
        out.push((start, cut));
        start = cut;
    }
    out
}

pub struct ParakeetAsr {
    pub models: ModelManager,
    runtime: std::sync::OnceLock<Result<ParakeetRuntime, String>>,
}

impl ParakeetAsr {
    pub fn new(models: ModelManager) -> Self {
        ParakeetAsr {
            models,
            runtime: std::sync::OnceLock::new(),
        }
    }
}

impl AsrProvider for ParakeetAsr {
    fn id(&self) -> &str {
        // includes the segmenter version, so cached transcripts are rebuilt when it changes
        "parakeet-tdt-0.6b-v3-int8+seg2"
    }

    fn transcribe(&self, req: &AsrRequest) -> Result<Transcript, AsrError> {
        let rt = self.runtime.get_or_init(|| {
            ParakeetRuntime::load(&self.models, &|_, _, _| {}).map_err(|e| e.to_string())
        });
        let rt = rt.as_ref().map_err(|e| AsrError::Unavailable(e.clone()))?;
        let analysis =
            AudioAnalysis::compute(&req.audio).map_err(|e| AsrError::Bad(e.to_string()))?;
        let mut reader = AudioReader::open(&req.audio).map_err(|e| AsrError::Bad(e.to_string()))?;
        let info = reader.info();
        let mut words = Vec::new();
        for (a, b) in chunk_points(&analysis) {
            let raw = reader
                .read_range(a, b)
                .map_err(|e| AsrError::Bad(e.to_string()))?;
            let mono = to_mono(&raw, info.channels);
            words.extend(rt.chunk(&mono, info.sample_rate, a));
        }
        // Segments: sentences (punctuation) or pauses of 0.3 s, like sentence-level transcripts.
        let mut t = Transcript {
            provider: self.id().into(),
            word_level: true,
            ..Default::default()
        };
        let mut seg_start = 0usize;
        for i in 0..words.len() {
            let sentence_end = words[i].0.ends_with(['.', '?', '!']);
            let last = i + 1 == words.len() || sentence_end || words[i + 1].1 - words[i].2 >= 0.3;
            let si = t.segments.len();
            t.words.push(TWord {
                w: norm_word(&words[i].0),
                raw: words[i].0.clone(),
                t0: words[i].1,
                t1: words[i].2,
                seg: si,
            });
            if last {
                let text = words[seg_start..=i]
                    .iter()
                    .map(|w| w.0.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                t.segments.push(Segment {
                    start: words[seg_start].1,
                    end: words[i].2,
                    text,
                    speaker: None,
                    w0: seg_start,
                    w1: i + 1,
                });
                seg_start = i + 1;
            }
        }
        Ok(t)
    }
}
