//! AI tasks with typed, schema-described input and output (see `docs/PLAN.md` §3.1).

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AiTask {
    LineMatch,
    RoomClassify,
    NoteCondense,
    PickDetect,
    SoundSpot,
    SoundTag,
    NameMatch,
}

impl AiTask {
    pub fn as_str(self) -> &'static str {
        match self {
            AiTask::LineMatch => "line_match",
            AiTask::RoomClassify => "room_classify",
            AiTask::NoteCondense => "note_condense",
            AiTask::PickDetect => "pick_detect",
            AiTask::SoundSpot => "sound_spot",
            AiTask::SoundTag => "sound_tag",
            AiTask::NameMatch => "name_match",
        }
    }
}

/// Range and consistency checks beyond the schema.
pub trait Validate {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Ties an input type to its task and output type.
pub trait AiTaskIo: Serialize + JsonSchema {
    const TASK: AiTask;
    type Output: DeserializeOwned + Serialize + JsonSchema + Validate;
}

fn unit(name: &str, v: f64) -> Result<(), String> {
    if (0.0..=1.0).contains(&v) {
        Ok(())
    } else {
        Err(format!("{name} must be within 0..1, got {v}"))
    }
}

// ---- line_match ----
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranscriptSegment {
    pub index: usize,
    pub t0: f64,
    pub t1: f64,
    pub text: String,
    /// The line the deterministic aligner proposes, and its fuzzy score (0..100).
    pub proposed_line_id: Option<String>,
    pub score: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateLine {
    pub line_id: String,
    pub character: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LineMatchInput {
    pub segments: Vec<TranscriptSegment>,
    pub candidates: Vec<CandidateLine>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchTakeType {
    Full,
    Partial,
    Pickup,
    Adlib,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineMatch {
    pub index: usize,
    pub line_id: Option<String>,
    pub take_type: MatchTakeType,
    pub confidence: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineMatchOutput {
    pub matches: Vec<LineMatch>,
}

impl Validate for LineMatchOutput {
    fn validate(&self) -> Result<(), String> {
        self.matches
            .iter()
            .try_for_each(|m| unit("confidence", m.confidence))
    }
}

impl AiTaskIo for LineMatchInput {
    const TASK: AiTask = AiTask::LineMatch;
    type Output = LineMatchOutput;
}

// ---- room_classify ----
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoomClass {
    CueReading,
    Direction,
    Feedback,
    Chatter,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RoomSegment {
    pub id: String,
    pub t0: f64,
    pub t1: f64,
    pub text: String,
    /// The deterministic classification, for the model to confirm or correct.
    pub hint: RoomClass,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ScriptItem {
    pub line_id: String,
    pub kind: String,
    pub character: Option<String>,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RoomClassifyInput {
    pub segments: Vec<RoomSegment>,
    pub scene_script: Vec<ScriptItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoomClassified {
    pub id: String,
    pub class: RoomClass,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoomClassifyOutput {
    pub segments: Vec<RoomClassified>,
}

impl Validate for RoomClassifyOutput {}

impl AiTaskIo for RoomClassifyInput {
    const TASK: AiTask = AiTask::RoomClassify;
    type Output = RoomClassifyOutput;
}

// ---- note_condense ----
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TakeNotes {
    pub take_id: String,
    pub line_text: String,
    pub direction_text: String,
    pub feedback_text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NoteCondenseInput {
    pub takes: Vec<TakeNotes>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CondensedNote {
    pub take_id: String,
    /// A few words: what was asked before the take ("quieter, to yourself").
    /// Models leave out empty notes now and then, so missing means none.
    #[serde(default)]
    pub direction: String,
    /// A few words: the reaction after it ("liked it").
    #[serde(default)]
    pub feedback: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteCondenseOutput {
    pub notes: Vec<CondensedNote>,
}

impl Validate for NoteCondenseOutput {
    fn validate(&self) -> Result<(), String> {
        for n in &self.notes {
            for (k, v) in [("direction", &n.direction), ("feedback", &n.feedback)] {
                if v.split_whitespace().count() > 12 {
                    return Err(format!("{k} of {} is longer than a few words", n.take_id));
                }
            }
        }
        Ok(())
    }
}

impl AiTaskIo for NoteCondenseInput {
    const TASK: AiTask = AiTask::NoteCondense;
    type Output = NoteCondenseOutput;
}

// ---- pick_detect ----
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PickCandidate {
    pub take_id: String,
    pub take_no: u32,
    pub line_text: String,
    pub feedback_text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PickDetectInput {
    pub takes: Vec<PickCandidate>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pick {
    pub take_id: String,
    /// 0 none, 1 liked, 2 explicit pick ("that's the one").
    pub strength: u8,
    pub refers_to: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickDetectOutput {
    pub picks: Vec<Pick>,
}

impl Validate for PickDetectOutput {
    fn validate(&self) -> Result<(), String> {
        match self.picks.iter().find(|p| p.strength > 2) {
            Some(p) => Err(format!("strength of {} must be 0, 1 or 2", p.take_id)),
            None => Ok(()),
        }
    }
}

impl AiTaskIo for PickDetectInput {
    const TASK: AiTask = AiTask::PickDetect;
    type Output = PickDetectOutput;
}

// ---- sound_spot ----
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SpotScene {
    pub id: String,
    pub heading: String,
    pub location: String,
    pub items: Vec<ScriptItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SoundSpotInput {
    pub scene: SpotScene,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpotCueType {
    Hard,
    Foley,
    Ambience,
    Walla,
    Music,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpottedCue {
    pub description: String,
    pub anchor_line_id: String,
    pub anchor_word: Option<usize>,
    pub cue_type: SpotCueType,
    pub tags: Vec<String>,
    /// 1 high … 3 low.
    pub priority: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoundSpotOutput {
    pub cues: Vec<SpottedCue>,
}

impl Validate for SoundSpotOutput {
    fn validate(&self) -> Result<(), String> {
        match self.cues.iter().find(|c| !(1..=3).contains(&c.priority)) {
            Some(c) => Err(format!("priority of '{}' must be 1..3", c.description)),
            None => Ok(()),
        }
    }
}

impl AiTaskIo for SoundSpotInput {
    const TASK: AiTask = AiTask::SoundSpot;
    type Output = SoundSpotOutput;
}

// ---- sound_tag ----
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SoundTagInput {
    pub file_name: String,
    pub duration_s: f64,
    pub cue_description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoundTagOutput {
    pub category: String,
    pub tags: Vec<String>,
}

impl Validate for SoundTagOutput {}

impl AiTaskIo for SoundTagInput {
    const TASK: AiTask = AiTask::SoundTag;
    type Output = SoundTagOutput;
}

// ---- name_match ----
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NameMatchInput {
    pub files: Vec<String>,
    pub characters: Vec<String>,
    pub aliases: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NameRole {
    Character,
    Room,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NameAssignment {
    pub file: String,
    pub role: NameRole,
    pub character: Option<String>,
    pub confidence: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NameMatchOutput {
    pub assignments: Vec<NameAssignment>,
}

impl Validate for NameMatchOutput {
    fn validate(&self) -> Result<(), String> {
        self.assignments
            .iter()
            .try_for_each(|a| unit("confidence", a.confidence))
    }
}

impl AiTaskIo for NameMatchInput {
    const TASK: AiTask = AiTask::NameMatch;
    type Output = NameMatchOutput;
}

/// JSON schema of a task's output.
pub fn output_schema<T: AiTaskIo>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T::Output)).unwrap_or_default()
}
