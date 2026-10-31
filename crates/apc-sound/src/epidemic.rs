//! Epidemic Sound sound effects through the Partner Content API (API key, Bearer auth):
//! `GET /v0/sound-effects/search?term=…`, `GET /v0/sound-effects/{id}/download`.
//! Endpoint details: developers.epidemicsound.com (checked 2026-10-31).

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, SoundError};
use crate::providers::{SoundCandidate, SoundQuery, SoundSearchProvider};

pub const DEFAULT_BASE: &str = "https://api.epidemicsound.com";

pub struct EpidemicProvider {
    pub api_key: Option<String>,
    pub base_url: String,
    /// Anonymous, stable id for the `x-partner-user-id` header.
    pub user_id: String,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(rename = "soundEffects", default)]
    sound_effects: Vec<SoundEffect>,
}

#[derive(Deserialize)]
struct SoundEffect {
    id: serde_json::Value,
    title: String,
    #[serde(default)]
    length: f64,
}

#[derive(Deserialize)]
struct DownloadResponse {
    url: String,
}

/// A stable anonymous id for this machine's user (no personal data leaves the machine).
pub fn anonymous_user_id() -> String {
    let who = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let h = blake3::hash(format!("{}|{}", apc_core::brand::BUNDLE_ID, who).as_bytes());
    h.to_hex()[..20].to_string()
}

fn http_err(e: ureq::Error) -> SoundError {
    match e {
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => {
            SoundError::Unavailable("epidemic".into(), "the API key was rejected".into())
        }
        other => SoundError::Http(other.to_string()),
    }
}

impl EpidemicProvider {
    pub fn new(api_key: Option<String>) -> Self {
        EpidemicProvider {
            api_key,
            base_url: std::env::var("CUTAWAY_EPIDEMIC_URL")
                .unwrap_or_else(|_| DEFAULT_BASE.to_string()),
            user_id: anonymous_user_id(),
        }
    }

    fn key(&self) -> Result<&str> {
        self.api_key.as_deref().ok_or_else(|| {
            SoundError::Unavailable(
                "epidemic".into(),
                "not connected: add an API key in Settings → Connections".into(),
            )
        })
    }

    fn get(&self, path: &str) -> Result<ureq::http::Response<ureq::Body>> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        ureq::get(&url)
            .header("Authorization", &format!("Bearer {}", self.key()?))
            .header("x-partner-user-id", &self.user_id)
            .call()
            .map_err(http_err)
    }

    /// Checks a key with a minimal search.
    pub fn verify(&self) -> Result<()> {
        self.get("/v0/sound-effects/search?term=door&limit=1")
            .map(|_| ())
    }

    /// Downloads a sound effect (320 kbps MP3) into `dir`. Counts as one download.
    pub fn download(&self, id: &str, title: &str, dir: &Path) -> Result<PathBuf> {
        let mut r = self.get(&format!(
            "/v0/sound-effects/{}/download?quality=high&format=mp3",
            enc(id)
        ))?;
        let d: DownloadResponse = r
            .body_mut()
            .read_json()
            .map_err(|e| SoundError::Http(e.to_string()))?;
        let mut bytes = Vec::new();
        ureq::get(&d.url)
            .call()
            .map_err(http_err)?
            .into_body()
            .into_reader()
            .read_to_end(&mut bytes)?;
        std::fs::create_dir_all(dir)?;
        let safe: String = title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let ext = d
            .url
            .split('?')
            .next()
            .and_then(|p| p.rsplit_once('.'))
            .map(|(_, e)| e.to_lowercase())
            .filter(|e| ["mp3", "wav", "aif", "aiff", "flac"].contains(&e.as_str()))
            .unwrap_or_else(|| "mp3".into());
        let file = dir.join(format!("{} (Epidemic {}).{ext}", safe.trim(), id));
        std::fs::write(&file, bytes)?;
        Ok(file)
    }
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                o.push(b as char)
            }
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

impl SoundSearchProvider for EpidemicProvider {
    fn id(&self) -> &str {
        "epidemic"
    }

    fn search(&self, q: &SoundQuery) -> Result<Vec<SoundCandidate>> {
        let term: String = q.text.chars().take(500).collect();
        let limit = q.max_results.clamp(1, 60);
        let mut r = self.get(&format!(
            "/v0/sound-effects/search?term={}&limit={limit}",
            enc(&term)
        ))?;
        let body: SearchResponse = r
            .body_mut()
            .read_json()
            .map_err(|e| SoundError::Http(e.to_string()))?;
        Ok(body
            .sound_effects
            .into_iter()
            .map(|s| SoundCandidate {
                provider: "epidemic".into(),
                title: s.title,
                asset: None,
                remote_id: Some(match s.id {
                    serde_json::Value::String(x) => x,
                    other => other.to_string(),
                }),
                duration_s: s.length,
                license: Some("Epidemic Sound".into()),
            })
            .collect())
    }
}
