//! Sound providers: search (library, Epidemic Sound) and generate (ElevenLabs, MCP).
//! New services plug in behind these two traits.

use std::io::Read;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use apc_core::Asset;

use crate::error::{Result, SoundError};
use crate::library::Library;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundQuery {
    pub text: String,
    pub tags: Vec<String>,
    pub max_results: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundCandidate {
    pub provider: String,
    pub title: String,
    /// Library asset (local results) or a remote id to fetch.
    pub asset: Option<Asset>,
    pub remote_id: Option<String>,
    pub duration_s: f64,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerateRequest {
    pub prompt: String,
    pub duration_s: Option<f64>,
    pub variations: u32,
    /// Where generated files are written before import.
    pub out_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneratedSound {
    pub provider: String,
    pub file: PathBuf,
    pub prompt: String,
}

pub trait SoundSearchProvider {
    fn id(&self) -> &str;
    fn search(&self, q: &SoundQuery) -> Result<Vec<SoundCandidate>>;
}

pub trait SoundGenerateProvider {
    fn id(&self) -> &str;
    fn generate(&self, req: &GenerateRequest) -> Result<Vec<GeneratedSound>>;
}

/// Searches the show (or local) library.
pub struct LibrarySearchProvider<'a> {
    pub library: &'a Library,
    pub local: Vec<Asset>,
}

impl SoundSearchProvider for LibrarySearchProvider<'_> {
    fn id(&self) -> &str {
        "library"
    }

    fn search(&self, q: &SoundQuery) -> Result<Vec<SoundCandidate>> {
        let mut text = q.text.clone();
        for t in &q.tags {
            text.push(' ');
            text.push_str(t);
        }
        // try all words, then any significant word
        let mut hits = self.library.search(&text, &self.local)?;
        if hits.is_empty() {
            for w in q.text.split_whitespace().filter(|w| w.len() > 3) {
                for a in self.library.search(w, &self.local)? {
                    if !hits.iter().any(|h| h.id == a.id) {
                        hits.push(a);
                    }
                }
            }
        }
        Ok(hits
            .into_iter()
            .take(q.max_results.max(1))
            .map(|a| SoundCandidate {
                provider: "library".into(),
                title: a.file.clone(),
                duration_s: a.duration_s,
                license: a.license.clone(),
                remote_id: None,
                asset: Some(a),
            })
            .collect())
    }
}

/// ElevenLabs sound effects (`ELEVENLABS_API_KEY`).
pub struct ElevenLabsProvider {
    pub api_key: Option<String>,
    pub endpoint: String,
}

impl ElevenLabsProvider {
    pub fn from_env() -> Self {
        Self::with_key(
            std::env::var("ELEVENLABS_API_KEY")
                .ok()
                .filter(|k| !k.is_empty()),
        )
    }

    pub fn with_key(api_key: Option<String>) -> Self {
        ElevenLabsProvider {
            api_key,
            endpoint: "https://api.elevenlabs.io/v1/sound-generation".into(),
        }
    }
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() > 40 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

impl SoundGenerateProvider for ElevenLabsProvider {
    fn id(&self) -> &str {
        "elevenlabs"
    }

    fn generate(&self, req: &GenerateRequest) -> Result<Vec<GeneratedSound>> {
        let key = self.api_key.as_ref().ok_or_else(|| {
            SoundError::Unavailable("elevenlabs".into(), "not connected: add an API key in Settings → Connections (or set ELEVENLABS_API_KEY)".into())
        })?;
        std::fs::create_dir_all(&req.out_dir)?;
        let mut out = Vec::new();
        for k in 0..req.variations.max(1) {
            let mut body = serde_json::json!({ "text": req.prompt, "prompt_influence": 0.4 });
            if let Some(d) = req.duration_s {
                body["duration_seconds"] = serde_json::json!(d.clamp(0.5, 22.0));
            }
            let resp = ureq::post(&self.endpoint)
                .header("xi-api-key", key)
                .header("Accept", "audio/mpeg")
                .send_json(&body)
                .map_err(|e| SoundError::Http(e.to_string()))?;
            let mut bytes = Vec::new();
            resp.into_body().into_reader().read_to_end(&mut bytes)?;
            let file = req
                .out_dir
                .join(format!("{}-{}.mp3", slug(&req.prompt), k + 1));
            std::fs::write(&file, bytes)?;
            out.push(GeneratedSound {
                provider: "elevenlabs".into(),
                file,
                prompt: req.prompt.clone(),
            });
        }
        Ok(out)
    }
}

/// Generation through a configured MCP server (e.g. Magnific). Stub (DECISIONS.md D28).
pub struct McpGenerateProvider {
    pub server: Option<String>,
}

impl SoundGenerateProvider for McpGenerateProvider {
    fn id(&self) -> &str {
        "magnific"
    }

    fn generate(&self, _req: &GenerateRequest) -> Result<Vec<GeneratedSound>> {
        Err(SoundError::Unavailable(
            "magnific".into(),
            "MCP client provider not configured; set up the server in the app settings".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_without_keys_report_unavailable() {
        let e = ElevenLabsProvider {
            api_key: None,
            endpoint: String::new(),
        };
        let r = e.generate(&GenerateRequest {
            prompt: "door".into(),
            duration_s: None,
            variations: 1,
            out_dir: std::env::temp_dir(),
        });
        assert!(matches!(r, Err(SoundError::Unavailable(..))));
        assert!(matches!(
            crate::epidemic::EpidemicProvider::new(None).search(&SoundQuery {
                text: "x".into(),
                tags: vec![],
                max_results: 3
            }),
            Err(SoundError::Unavailable(..))
        ));
        assert_eq!(slug("Aluminum foil, burrito!"), "aluminum-foil-burrito");
    }

    #[test]
    #[ignore = "live ElevenLabs call; needs ELEVENLABS_API_KEY"]
    fn live_elevenlabs() {
        let p = ElevenLabsProvider::from_env();
        let dir = tempfile::tempdir().unwrap();
        let r = p
            .generate(&GenerateRequest {
                prompt: "a wooden door closing".into(),
                duration_s: Some(2.0),
                variations: 1,
                out_dir: dir.path().into(),
            })
            .unwrap();
        assert!(std::fs::metadata(&r[0].file).unwrap().len() > 1000);
    }
}
