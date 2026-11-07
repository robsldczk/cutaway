//! Project configuration (`[ai]`, `[sound.*]`, `[pacing]`, `[playback]`, `[export]`), stored in the
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
    /// Default pause between lines; negative overlaps them (−1 s … 5 s).
    pub breath_gap_s: f64,
    pub splice_fade_ms: f64,
    /// Fade at the start and end of every line (ms).
    pub edge_fade_ms: f64,
    /// Lead-in and tail: how much audio plays before a line's first sound and after its
    /// last (ms). Heard and exported; the timing of the cut doesn't change.
    pub handle_in_ms: f64,
    pub handle_out_ms: f64,
    pub room_tone: bool,
    /// How much a voice panned away from the centre goes back into the room, 0 … 1 (D126).
    pub pan_room: f64,
}

/// Half way: a hard-panned voice is just audibly further back.
pub const DEFAULT_PAN_ROOM: f64 = 0.5;

impl Default for PacingConfig {
    fn default() -> Self {
        PacingConfig {
            breath_gap_s: 0.3,
            splice_fade_ms: 10.0,
            edge_fade_ms: 20.0,
            handle_in_ms: 100.0,
            handle_out_ms: 150.0,
            room_tone: true,
            pan_room: DEFAULT_PAN_ROOM,
        }
    }
}

/// How the editor hears the cut. Never applied to bounces or exports.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct PlaybackConfig {
    /// Level each recording to `target_lufs` and limit the output, so a hot mic or a loud
    /// sound doesn't blast.
    pub level: bool,
    pub target_lufs: f64,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        PlaybackConfig {
            level: true,
            target_lufs: -23.0,
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
    /// Named settings of the Export page.
    pub presets: Vec<ExportPreset>,
}

/// What an export writes: one file per track, one per clip, a mix per scene, one mix of the whole
/// range, or only the timeline.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportMode {
    #[default]
    Stems,
    Clips,
    Mix,
    /// The range as one mix, scene after scene, in an episode version of its own.
    OneFile,
    Fcpxml,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodec {
    #[default]
    Wav,
    Flac,
    Mp3,
}

impl AudioCodec {
    pub fn ext(self) -> &'static str {
        match self {
            AudioCodec::Wav => "wav",
            AudioCodec::Flac => "flac",
            AudioCodec::Mp3 => "mp3",
        }
    }
}

/// What the clips of an exported FCPXML point at.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FcpxmlSource {
    #[default]
    Original,
    Stems,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CutListFormat {
    Csv,
    Md,
}

/// Where an export goes: the episode's working area, or the production's pipeline folder.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaveTo {
    #[default]
    Episode,
    Pipeline,
}

impl SaveTo {
    pub fn as_str(self) -> &'static str {
        match self {
            SaveTo::Episode => "episode",
            SaveTo::Pipeline => "pipeline",
        }
    }
}

/// The tracks of a stem export. All on except the split by character.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(default)]
pub struct ExportTracks {
    pub dialogue: bool,
    pub room_voices: bool,
    pub effects: bool,
    pub room_effects: bool,
    pub ambience: bool,
    pub music: bool,
    pub mix: bool,
    /// One full-length file per character instead of one dialogue file.
    pub split_dialogue: bool,
    /// The per-character files without pan: each voice in the centre, written as one mono
    /// channel. Off, they keep the scene's pan, in stereo (D160).
    pub split_mono: bool,
}

impl Default for ExportTracks {
    fn default() -> Self {
        ExportTracks {
            dialogue: true,
            room_voices: true,
            effects: true,
            room_effects: true,
            ambience: true,
            music: true,
            mix: true,
            split_dialogue: false,
            split_mono: false,
        }
    }
}

/// Everything the Export page asks. The defaults are the page's defaults.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct ExportSettings {
    pub mode: ExportMode,
    pub tracks: ExportTracks,
    pub codec: AudioCodec,
    pub sample_rate: u32,
    /// 16, 24, or 32 (float; WAV only).
    pub bit_depth: u16,
    /// The mix's target in LUFS; `None` leaves it at mix level. Never applied to stems.
    pub loudness: Option<f64>,
    pub fcpxml: bool,
    pub fcpxml_source: FcpxmlSource,
    pub subtitles: bool,
    pub cut_list: Option<CutListFormat>,
    pub lock_after: bool,
    pub save_to: SaveTo,
}

impl Default for ExportSettings {
    fn default() -> Self {
        ExportSettings {
            mode: ExportMode::Stems,
            tracks: ExportTracks::default(),
            codec: AudioCodec::Wav,
            sample_rate: 48000,
            bit_depth: 24,
            loudness: Some(-16.0),
            fcpxml: true,
            fcpxml_source: FcpxmlSource::Original,
            subtitles: true,
            cut_list: None,
            lock_after: true,
            save_to: SaveTo::Episode,
        }
    }
}

impl ExportSettings {
    /// Audio files at all: FCPXML only renders nothing.
    pub fn renders(&self) -> bool {
        self.mode != ExportMode::Fcpxml
    }

    /// One mix for the whole range, or one per scene: there are no tracks to choose.
    pub fn mixes_only(&self) -> bool {
        matches!(self.mode, ExportMode::Mix | ExportMode::OneFile)
    }

    /// FCPXML only always writes the FCPXML, whatever "Also export" says.
    pub fn writes_fcpxml(&self) -> bool {
        self.fcpxml || self.mode == ExportMode::Fcpxml
    }

    /// Refuses what can't be delivered, in words for the person exporting.
    pub fn check(&self) -> Result<()> {
        if self.mode == ExportMode::Fcpxml {
            return Ok(());
        }
        if self.codec == AudioCodec::Mp3 && !self.mixes_only() {
            return Err(CoreError::invalid(
                "MP3 pads its start and end, so it can't hold a stem or clip to the sample: choose WAV or FLAC, or Mix only or One file",
            ));
        }
        if self.codec == AudioCodec::Flac && self.bit_depth == 32 {
            return Err(CoreError::invalid(
                "FLAC goes up to 24-bit: choose 16 or 24",
            ));
        }
        if self.codec != AudioCodec::Mp3 && ![16, 24, 32].contains(&self.bit_depth) {
            return Err(CoreError::invalid("bit depth is 16, 24 or 32"));
        }
        if ![44100, 48000, 88200, 96000].contains(&self.sample_rate) {
            return Err(CoreError::invalid(
                "sample rate is 44100, 48000, 88200 or 96000",
            ));
        }
        if self.loudness.is_some_and(|l| !(-40.0..=-5.0).contains(&l)) {
            return Err(CoreError::invalid("loudness runs from -40 to -5 LUFS"));
        }
        let t = self.tracks;
        let any = t.dialogue
            || t.room_voices
            || t.effects
            || t.room_effects
            || t.ambience
            || t.music
            || t.mix
            || t.split_dialogue;
        if !self.mixes_only() && !any {
            return Err(CoreError::invalid("no tracks are ticked"));
        }
        Ok(())
    }
}

/// A saved set of export settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ExportPreset {
    pub name: String,
    pub settings: ExportSettings,
}

impl Default for ExportConfig {
    fn default() -> Self {
        ExportConfig {
            handles_s: 1.0,
            fcpxml_version: "1.9".into(),
            loudness_normalize: false,
            target_lufs: -23.0,
            room_tone_track: true,
            presets: Vec::new(),
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
    pub playback: PlaybackConfig,
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
            breath_gap_s: self.pacing.breath_gap_s.clamp(-1.0, 5.0),
            splice_fade_s: self.pacing.splice_fade_ms / 1000.0,
            edge_fade_s: self.pacing.edge_fade_ms.clamp(1.0, 500.0) / 1000.0,
            handle_in_s: self.pacing.handle_in_ms.clamp(0.0, 1000.0) / 1000.0,
            handle_out_s: self.pacing.handle_out_ms.clamp(0.0, 1000.0) / 1000.0,
            pan_room: self.pacing.pan_room.clamp(0.0, 1.0),
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
        assert!(c.playback.level, "playback leveling is on by default");
        let old: ProjectConfig =
            serde_json::from_value(serde_json::json!({ "pacing": {} })).unwrap();
        assert!(old.playback.level, "projects saved before it get it too");
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
