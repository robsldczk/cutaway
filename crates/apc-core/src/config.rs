//! Project configuration (`[ai]`, `[sound.*]`, `[pacing]`, `[export]`), stored in the
//! episode as the `config` setting and editable as TOML.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::model::Document;
use crate::timeline::Pacing;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct AiConfig {
    pub provider: String,
    pub model: String,
    pub effort: String,
    pub batch: String,
    pub timeout_s: u64,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            provider: "claude-code".into(),
            model: "claude-sonnet-5-5".into(),
            effort: "high".into(),
            batch: "scene".into(),
            timeout_s: 300,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct SoundSearchConfig {
    pub providers: Vec<String>,
}

impl Default for SoundSearchConfig {
    fn default() -> Self {
        SoundSearchConfig {
            providers: vec!["library".into(), "epidemic".into()],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct SoundGenerateConfig {
    pub providers: Vec<String>,
    pub variations: u32,
}

impl Default for SoundGenerateConfig {
    fn default() -> Self {
        SoundGenerateConfig {
            providers: vec!["elevenlabs".into(), "magnific".into()],
            variations: 3,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct SoundConfig {
    pub search: SoundSearchConfig,
    pub generate: SoundGenerateConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct PacingConfig {
    pub breath_gap_s: f64,
    pub splice_fade_ms: f64,
    pub room_tone: bool,
}

impl Default for PacingConfig {
    fn default() -> Self {
        PacingConfig {
            breath_gap_s: 0.3,
            splice_fade_ms: 10.0,
            room_tone: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct ExportConfig {
    pub handles_s: f64,
    pub fcpxml_version: String,
    pub loudness_normalize: bool,
    pub target_lufs: f64,
    pub room_tone_track: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        ExportConfig {
            handles_s: 1.0,
            fcpxml_version: "1.9".into(),
            loudness_normalize: false,
            target_lufs: -23.0,
            room_tone_track: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct ProjectConfig {
    pub ai: AiConfig,
    pub sound: SoundConfig,
    pub pacing: PacingConfig,
    pub export: ExportConfig,
}

impl ProjectConfig {
    pub fn from_doc(doc: &Document) -> Self {
        doc.setting("config")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    pub fn pacing(&self) -> Pacing {
        Pacing {
            breath_gap_s: self.pacing.breath_gap_s,
            splice_fade_s: self.pacing.splice_fade_ms / 1000.0,
            ..Pacing::default()
        }
    }

    /// Sets a value by dotted path (`ai.model`). Unknown keys are an error.
    pub fn set(&mut self, key: &str, value: Value) -> Result<()> {
        let mut root = serde_json::to_value(&*self)?;
        let mut node = &mut root;
        let parts: Vec<&str> = key.split('.').collect();
        for (i, p) in parts.iter().enumerate() {
            let obj = node
                .as_object_mut()
                .ok_or_else(|| CoreError::invalid(format!("unknown config key '{key}'")))?;
            if !obj.contains_key(*p) {
                return Err(CoreError::invalid(format!("unknown config key '{key}'")));
            }
            if i == parts.len() - 1 {
                obj.insert((*p).to_string(), value.clone());
                break;
            }
            node = obj
                .get_mut(*p)
                .ok_or_else(|| CoreError::invalid(format!("unknown config key '{key}'")))?;
        }
        *self = serde_json::from_value(root)
            .map_err(|e| CoreError::invalid(format!("bad value for '{key}': {e}")))?;
        Ok(())
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self).map_err(|e| CoreError::Toml(e.to_string()))
    }

    pub fn from_toml(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(|e| CoreError::Toml(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_spec() {
        let c = ProjectConfig::default();
        assert_eq!(c.ai.provider, "claude-code");
        assert_eq!(c.ai.model, "claude-sonnet-5-5");
        assert_eq!(c.ai.effort, "high");
        assert_eq!(c.sound.generate.variations, 3);
        let t = c.to_toml().unwrap();
        assert!(t.contains("[ai]"));
        assert_eq!(ProjectConfig::from_toml(&t).unwrap(), c);
    }

    #[test]
    fn dotted_set() {
        let mut c = ProjectConfig::default();
        c.set("ai.model", Value::String("x".into())).unwrap();
        assert_eq!(c.ai.model, "x");
        c.set("pacing.breath_gap_s", serde_json::json!(0.45))
            .unwrap();
        assert_eq!(c.pacing.breath_gap_s, 0.45);
        assert!(c.set("ai.nope", Value::Null).is_err());
        assert!(c
            .set("pacing.breath_gap_s", Value::String("x".into()))
            .is_err());
    }
}
