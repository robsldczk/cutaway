//! ASR providers. Transcripts are word streams with timings; ownership of speech comes
//! from script matching and loudness, never from diarization labels.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use apc_core::text::{display_tokens, norm_word};

#[derive(Debug, Error)]
pub enum AsrError {
    #[error("ASR provider unavailable: {0}")]
    Unavailable(String),
    #[error("no transcript for {0}")]
    NoTranscript(String),
    #[error("bad transcript: {0}")]
    Bad(String),
    #[error("model download failed: {0}")]
    Download(String),
    #[error("checksum mismatch for {0}")]
    Checksum(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub speaker: Option<String>,
    /// Word index range `[w0, w1)` into `Transcript::words`.
    pub w0: usize,
    pub w1: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TWord {
    pub w: String,
    pub raw: String,
    pub t0: f64,
    pub t1: f64,
    pub seg: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Transcript {
    pub segments: Vec<Segment>,
    pub words: Vec<TWord>,
    /// True when word times come from the model (not spread across segments).
    pub word_level: bool,
    pub provider: String,
}

impl Transcript {
    /// Builds a transcript from segments, spreading word times by character length
    /// (Stage 0 `load_stream`).
    pub fn from_segments(segs: Vec<(f64, f64, String, Option<String>)>, provider: &str) -> Self {
        let mut t = Transcript {
            provider: provider.to_string(),
            ..Default::default()
        };
        for (si, (st, en, text, spk)) in segs.into_iter().enumerate() {
            let raw = display_tokens(&text);
            let w0 = t.words.len();
            if !raw.is_empty() {
                let lens: Vec<f64> = raw.iter().map(|w| (w.chars().count() + 1) as f64).collect();
                let total: f64 = lens.iter().sum();
                let mut acc = 0.0;
                for (k, w) in raw.iter().enumerate() {
                    let a = st + acc / total * (en - st);
                    acc += lens[k];
                    let b = st + acc / total * (en - st);
                    t.words.push(TWord {
                        w: norm_word(w),
                        raw: w.clone(),
                        t0: a,
                        t1: b,
                        seg: si,
                    });
                }
            }
            t.segments.push(Segment {
                start: st,
                end: en,
                text,
                speaker: spk,
                w0,
                w1: t.words.len(),
            });
        }
        t
    }

    /// Shifts all times (sync correction: session time = file time − offset).
    pub fn shifted(&self, by: f64) -> Transcript {
        let mut t = self.clone();
        for s in &mut t.segments {
            s.start += by;
            s.end += by;
        }
        for w in &mut t.words {
            w.t0 += by;
            w.t1 += by;
        }
        t
    }
}

pub struct AsrRequest {
    pub audio: PathBuf,
    /// Transcript sidecar (fixture JSON), used by the import provider.
    pub sidecar: Option<PathBuf>,
    pub hotwords: Vec<String>,
    pub language: String,
}

pub trait AsrProvider: Send + Sync {
    fn id(&self) -> &str;
    fn transcribe(&self, req: &AsrRequest) -> Result<Transcript, AsrError>;
}

/// Reads segment-level transcripts (`{"lines":[{startTime, endTime, text, speakerDesignation}]}`,
/// SRT-style `HH:MM:SS,mmm` times) and spreads word times across each segment.
pub struct TranscriptImportProvider;

pub fn parse_tc(s: &str) -> Option<f64> {
    let s = s.trim().replace('.', ",");
    let (hms, ms) = s.split_once(',').unwrap_or((&s, "0"));
    let parts: Vec<&str> = hms.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (
            h.parse::<f64>().ok()?,
            m.parse::<f64>().ok()?,
            s.parse::<f64>().ok()?,
        ),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        _ => return None,
    };
    let frac: f64 = format!("0.{ms}").parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec + frac)
}

/// Parses the fixture transcript JSON.
pub fn parse_transcript_json(text: &str) -> Result<Transcript, AsrError> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| AsrError::Bad(e.to_string()))?;
    let lines = v
        .get("lines")
        .and_then(|l| l.as_array())
        .ok_or_else(|| AsrError::Bad("missing 'lines' array".into()))?;
    let mut segs = Vec::with_capacity(lines.len());
    for l in lines {
        let st = l
            .get("startTime")
            .and_then(|x| x.as_str())
            .and_then(parse_tc);
        let en = l.get("endTime").and_then(|x| x.as_str()).and_then(parse_tc);
        let text = l
            .get("text")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let spk = l
            .get("speakerDesignation")
            .and_then(|x| x.as_str())
            .map(String::from);
        match (st, en) {
            (Some(a), Some(b)) => segs.push((a, b, text, spk)),
            _ => return Err(AsrError::Bad(format!("bad times in segment '{text}'"))),
        }
    }
    Ok(Transcript::from_segments(segs, "transcript-import"))
}

impl AsrProvider for TranscriptImportProvider {
    fn id(&self) -> &str {
        "transcript-import"
    }

    fn transcribe(&self, req: &AsrRequest) -> Result<Transcript, AsrError> {
        let sidecar = req
            .sidecar
            .clone()
            .or_else(|| {
                let p = req.audio.with_extension("json");
                p.exists().then_some(p)
            })
            .ok_or_else(|| AsrError::NoTranscript(req.audio.display().to_string()))?;
        let text = std::fs::read_to_string(&sidecar)?;
        parse_transcript_json(&text)
    }
}

/// A downloadable model file.
#[derive(Clone, Debug)]
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    /// BLAKE3 hex, or empty to record it on first download.
    pub blake3: &'static str,
}

#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub id: &'static str,
    pub files: &'static [ModelFile],
}

/// Parakeet TDT 0.6B v3 (int8) as published for sherpa-onnx.
pub const PARAKEET_TDT_V3: ModelSpec = ModelSpec {
    id: "parakeet-tdt-0.6b-v3-int8",
    files: &[
        ModelFile {
            name: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
            blake3: "",
        },
    ],
};

/// Downloads models once into the app-support folder, verifies checksums and caches them.
pub struct ModelManager {
    pub root: PathBuf,
}

impl ModelManager {
    /// `<app support>/<slug>/models` (macOS `~/Library/Application Support`, Windows `%APPDATA%`).
    pub fn default_root() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(apc_core::brand::SLUG)
            .join("models")
    }

    pub fn new(root: PathBuf) -> Self {
        ModelManager { root }
    }

    pub fn model_dir(&self, spec: &ModelSpec) -> PathBuf {
        self.root.join(spec.id)
    }

    pub fn is_installed(&self, spec: &ModelSpec) -> bool {
        spec.files
            .iter()
            .all(|f| self.model_dir(spec).join(f.name).exists())
    }

    /// Ensures every file is present and verified; downloads what is missing.
    pub fn ensure(
        &self,
        spec: &ModelSpec,
        progress: &dyn Fn(&str, u64, Option<u64>),
    ) -> Result<PathBuf, AsrError> {
        let dir = self.model_dir(spec);
        std::fs::create_dir_all(&dir)?;
        for f in spec.files {
            let dest = dir.join(f.name);
            if !dest.exists() {
                download(f.url, &dest, &|got, total| progress(f.name, got, total))?;
            }
            verify(&dest, f)?;
        }
        Ok(dir)
    }
}

fn verify(path: &Path, f: &ModelFile) -> Result<(), AsrError> {
    let sum_file = path.with_extension("blake3");
    let actual = apc_core::media::hash_file(path).map_err(|e| AsrError::Download(e.to_string()))?;
    let expected = if !f.blake3.is_empty() {
        f.blake3.to_string()
    } else if let Ok(s) = std::fs::read_to_string(&sum_file) {
        s.trim().to_string()
    } else {
        // First download of a model without a pinned checksum: record it (trust on first use).
        std::fs::write(&sum_file, &actual)?;
        actual.clone()
    };
    if actual != expected {
        return Err(AsrError::Checksum(path.display().to_string()));
    }
    Ok(())
}

fn download(url: &str, dest: &Path, progress: &dyn Fn(u64, Option<u64>)) -> Result<(), AsrError> {
    use std::io::{Read, Write};
    let resp = ureq::get(url)
        .call()
        .map_err(|e| AsrError::Download(format!("{url}: {e}")))?;
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = resp.into_body().into_reader();
    let tmp = dest.with_extension("part");
    let mut out = std::fs::File::create(&tmp)?;
    let mut buf = vec![0u8; 1 << 16];
    let mut got = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        got += n as u64;
        progress(got, total);
    }
    out.flush()?;
    std::fs::rename(tmp, dest)?;
    Ok(())
}

/// Parakeet TDT 0.6B v3 through ONNX Runtime (sherpa-onnx). Built only with the
/// `parakeet` feature; otherwise it reports itself unavailable (see DECISIONS.md D23).
pub struct ParakeetProvider {
    pub models: ModelManager,
}

impl AsrProvider for ParakeetProvider {
    fn id(&self) -> &str {
        PARAKEET_TDT_V3.id
    }

    fn transcribe(&self, _req: &AsrRequest) -> Result<Transcript, AsrError> {
        Err(AsrError::Unavailable(
            "this build has no Parakeet runtime (enable the `parakeet` feature); use transcript import".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_format_and_spreads_words() {
        let json = r#"{"lines":[{"startTime":"00:01:00,880","endTime":"00:01:03,600","speakerDesignation":"Speaker 3","text":"So, you don't think I'm a scientist, huh?"},{"startTime":"00:01:03,600","endTime":"00:01:05,520","text":"I'll show you science"}]}"#;
        let t = parse_transcript_json(json).unwrap();
        assert_eq!(t.segments.len(), 2);
        assert!((t.segments[0].start - 60.88).abs() < 1e-9);
        assert_eq!(t.words.len(), 8 + 4);
        assert_eq!(t.words[0].w, "so");
        assert!((t.words[0].t0 - 60.88).abs() < 1e-9);
        assert!((t.words[7].t1 - 63.6).abs() < 1e-9);
        assert_eq!(t.segments[1].w0, 8);
        assert!(t.words.windows(2).all(|w| w[0].t1 <= w[1].t0 + 1e-9));
        assert_eq!(parse_tc("01:02:03.250"), Some(3723.25));
    }

    #[test]
    fn parakeet_stub_reports_unavailable() {
        let p = ParakeetProvider {
            models: ModelManager::new(std::env::temp_dir()),
        };
        let r = p.transcribe(&AsrRequest {
            audio: "x.wav".into(),
            sidecar: None,
            hotwords: vec![],
            language: "en".into(),
        });
        assert!(matches!(r, Err(AsrError::Unavailable(_))));
    }
}
