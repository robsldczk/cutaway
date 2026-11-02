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
    /// Ask the AI about session-mic speech the matcher left over (misheard takes).
    pub rescue: bool,
    /// Worked examples from the editor's corrections, added to the rescue prompt.
    pub rescue_examples: Vec<String>,
    /// Send the writers' story context (scene summaries, bios, settings) with spotting and
    /// file-name matching.
    pub story_context: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            provider: "claude-code".into(),
            model: "claude-sonnet-5-5".into(),
            effort: "medium".into(),
            batch: "scene".into(),
            timeout_s: 300,
            rescue: true,
            rescue_examples: vec![],
            story_context: true,
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

/// A word the recogniser keeps mishearing, and what the script says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct GlossaryEntry {
    pub heard: String,
    pub script: String,
}

/// The dev panel's learning set: the scenes Rob reviewed against.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct LearnConfig {
    /// The session mic the learning set belongs to.
    pub source_id: String,
    pub reference_scene: String,
    pub holdout_scene: String,
    /// Scenes Rob went through completely (`source_id/scene_id`): what is not marked
    /// there is right.
    pub reviewed: Vec<String>,
}

/// Matching settings the learning section can tune.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct AnalysisConfig {
    /// Speech fragments closer than this merge into one sentence (seconds).
    pub merge_gap_s: f64,
    /// A take more than this far under the mic's typical take level is bleed (dB).
    pub bleed_margin_db: f64,
    /// Added to the matcher's similarity thresholds (negative is looser).
    pub match_offset: f64,
    /// Words the recogniser mishears; fixed before matching and told to the AI.
    pub glossary: Vec<GlossaryEntry>,
    pub learn: LearnConfig,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        AnalysisConfig {
            merge_gap_s: 0.7,
            bleed_margin_db: 10.0,
            match_offset: 0.0,
            glossary: vec![],
            learn: LearnConfig::default(),
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
    pub analysis: AnalysisConfig,
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
        assert_eq!(c.ai.effort, "medium");
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
