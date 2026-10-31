//! API keys for sound services, kept in the OS credential store (macOS Keychain, Windows
//! Credential Manager) under the app's bundle id. Keys never go into project files, logs,
//! the command history or any reply to a client; clients only see "connected" or not.
//! Environment variables still work as a fallback for CI and headless use.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::error::{Result, SoundError};

/// A service that needs an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyedService {
    Epidemic,
    Elevenlabs,
}

impl KeyedService {
    pub const ALL: [KeyedService; 2] = [KeyedService::Epidemic, KeyedService::Elevenlabs];

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().replace([' ', '-', '_'], "").as_str() {
            "epidemic" | "epidemicsound" => Ok(KeyedService::Epidemic),
            "elevenlabs" => Ok(KeyedService::Elevenlabs),
            other => Err(SoundError::Invalid(format!(
                "unknown service '{other}' (epidemic, elevenlabs)"
            ))),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            KeyedService::Epidemic => "epidemic",
            KeyedService::Elevenlabs => "elevenlabs",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            KeyedService::Epidemic => "Epidemic Sound",
            KeyedService::Elevenlabs => "ElevenLabs",
        }
    }

    pub fn env_var(self) -> &'static str {
        match self {
            KeyedService::Epidemic => "EPIDEMIC_SOUND_API_KEY",
            KeyedService::Elevenlabs => "ELEVENLABS_API_KEY",
        }
    }
}

/// Where secrets live.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
}

/// The OS credential store.
pub struct Keychain {
    service: String,
}

impl Keychain {
    pub fn new() -> Self {
        Keychain {
            service: apc_core::brand::BUNDLE_ID.to_string(),
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, account)
            .map_err(|e| SoundError::Invalid(format!("keychain: {e}")))
    }
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>> {
        match self.entry(account)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SoundError::Invalid(format!("keychain: {e}"))),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<()> {
        self.entry(account)?
            .set_password(secret)
            .map_err(|e| SoundError::Invalid(format!("keychain: {e}")))
    }

    fn delete(&self, account: &str) -> Result<()> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SoundError::Invalid(format!("keychain: {e}"))),
        }
    }
}

/// In-memory store for tests (`CUTAWAY_SECRETS=memory`).
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.0.lock().ok().and_then(|m| m.get(account).cloned()))
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        if let Ok(mut m) = self.0.lock() {
            m.insert(account.into(), secret.into());
        }
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        if let Ok(mut m) = self.0.lock() {
            m.remove(account);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectionStatus {
    pub service: KeyedService,
    pub label: &'static str,
    pub connected: bool,
    /// `keychain`, `environment` or null.
    pub source: Option<&'static str>,
}

/// The key for a service: the credential store first, then the environment.
pub fn api_key(store: &dyn SecretStore, s: KeyedService) -> Option<String> {
    if let Ok(Some(k)) = store.get(s.id()) {
        if !k.is_empty() {
            return Some(k);
        }
    }
    std::env::var(s.env_var()).ok().filter(|k| !k.is_empty())
}

pub fn status(store: &dyn SecretStore, s: KeyedService) -> ConnectionStatus {
    let source = match store.get(s.id()) {
        Ok(Some(k)) if !k.is_empty() => Some("keychain"),
        _ if std::env::var(s.env_var())
            .map(|k| !k.is_empty())
            .unwrap_or(false) =>
        {
            Some("environment")
        }
        _ => None,
    };
    ConnectionStatus {
        service: s,
        label: s.label(),
        connected: source.is_some(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_and_status() {
        let m = MemoryStore::default();
        assert!(
            !status(&m, KeyedService::Epidemic).connected
                || std::env::var("EPIDEMIC_SOUND_API_KEY").is_ok()
        );
        m.set("epidemic", "k-123").unwrap();
        let st = status(&m, KeyedService::Epidemic);
        assert!(st.connected);
        assert_eq!(st.source, Some("keychain"));
        assert_eq!(
            api_key(&m, KeyedService::Epidemic).as_deref(),
            Some("k-123")
        );
        // the status never carries the key
        assert!(!serde_json::to_string(&st).unwrap().contains("k-123"));
        m.delete("epidemic").unwrap();
        assert_eq!(m.get("epidemic").unwrap(), None);
        assert_eq!(
            KeyedService::parse("Epidemic Sound").unwrap(),
            KeyedService::Epidemic
        );
    }
}
