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
    TakeRescue,
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
            AiTask::TakeRescue => "take_rescue",
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
    /// Plain-text rendering sent instead of the JSON input, when reading matters more than
    /// structure (a scene's script and recording).
    fn text(&self) -> Option<String> {
        None
    }
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
/// A take on one side of a stretch of room talk.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GapTake {
    pub take_id: String,
    pub character: String,
    pub line_text: String,
}

/// Everything the room said between two takes of a session (or before the first, or
/// after the last), chatter included.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RoomGap {
    pub gap_id: String,
    /// The take the talk follows.
    pub previous: Option<GapTake>,
    /// The take the talk leads into.
    pub next: Option<GapTake>,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NoteCondenseInput {
    pub gaps: Vec<RoomGap>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GapNote {
    pub gap_id: String,
    /// One or two full sentences on what the room meant. Models leave out empty notes
    /// now and then, so missing means none.
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteCondenseOutput {
    pub notes: Vec<GapNote>,
}

impl Validate for NoteCondenseOutput {
    fn validate(&self) -> Result<(), String> {
        // one or two sentences are asked for; only a copied transcript is refused
        for n in &self.notes {
            if n.summary.split_whitespace().count() > 80 {
                return Err(format!(
                    "the note of {} reads like a transcript, not a summary",
                    n.gap_id
                ));
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
    /// What the writers know about the scene (background only, see the prompt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<SpotContext>,
}

fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// Story context for spotting; empty fields are left out to keep the call small.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct SpotContext {
    #[serde(skip_serializing_if = "is_blank")]
    pub title: String,
    #[serde(skip_serializing_if = "is_blank")]
    pub act: String,
    /// The writers' summary of the scene.
    #[serde(skip_serializing_if = "is_blank")]
    pub synopsis: String,
    /// -1 (lowest point of the story) … +1 (highest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<f64>,
    /// Characters present, also the silent ones, with a one-line note each.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cast: Vec<CastNote>,
    #[serde(skip_serializing_if = "is_blank")]
    pub setting: String,
    /// The larger place the setting belongs to (Canteen → TARMAC base).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_of: Option<String>,
}

impl SpotContext {
    pub fn is_empty(&self) -> bool {
        self == &SpotContext::default()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CastNote {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_blank")]
    pub note: String,
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
    /// The writers' one-line notes per character (full names, roles).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub character_notes: std::collections::BTreeMap<String, String>,
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

// ---- take_rescue ----
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RescueScriptLine {
    /// `L1`, `L2` … in scene order.
    pub label: String,
    pub character: String,
    pub text: String,
}

/// One sentence of the scene's recording.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RescueRow {
    /// `s…` session mic (may be a take), `d…` director mic (never a take).
    pub id: String,
    pub speaker: String,
    pub text: String,
    /// `= L3` already a take of L3 · `?` to judge · `bleed` not the actor · empty for the
    /// director.
    #[serde(default)]
    pub tag: String,
    /// Silence before this sentence, when long enough to matter (seconds).
    #[serde(default)]
    pub pause_before_s: Option<f64>,
}

/// A stretch the editor marked in the dev panel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RescueHint {
    pub ids: Vec<String>,
    /// The line the editor linked it to.
    #[serde(default)]
    pub line: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TakeRescueInput {
    /// The actor whose session mic is judged.
    pub character: String,
    pub scene: String,
    /// The whole scene, every character.
    pub script: Vec<RescueScriptLine>,
    pub recording: Vec<RescueRow>,
    #[serde(default)]
    pub hints: Vec<RescueHint>,
    /// Worked examples from the editor's earlier corrections.
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RescueConfidence {
    High,
    Medium,
    Low,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rescue {
    /// A session-mic id from the recording (`s…`).
    pub id: String,
    /// The script line it performs (`L3`), or null: not a take.
    pub line: Option<String>,
    /// What the actor actually said, as you read it.
    #[serde(default)]
    pub read_as: String,
    pub confidence: RescueConfidence,
    /// When only part of the sentence is the take: its first and last words, quoted
    /// from the recording.
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TakeRescueOutput {
    pub rescues: Vec<Rescue>,
}

impl Validate for TakeRescueOutput {
    fn validate(&self) -> Result<(), String> {
        for r in &self.rescues {
            if !r.id.starts_with('s') {
                return Err(format!(
                    "{} is not a session-mic sentence; only s… ids can be takes",
                    r.id
                ));
            }
            if let Some(l) = &r.line {
                if !l.starts_with('L') {
                    return Err(format!("line of {} must be a label like L3, got {l}", r.id));
                }
            }
        }
        Ok(())
    }
}

impl AiTaskIo for TakeRescueInput {
    const TASK: AiTask = AiTask::TakeRescue;
    type Output = TakeRescueOutput;

    fn text(&self) -> Option<String> {
        use std::fmt::Write;
        let mut t = String::new();
        let _ = writeln!(
            t,
            "CHARACTER: {}\nSCENE: {}\n\nSCRIPT",
            self.character, self.scene
        );
        for l in &self.script {
            let _ = writeln!(t, "{} {}: {}", l.label, l.character, l.text);
        }
        let _ = writeln!(
            t,
            "\nRECORDING (s = {}'s session mic, may be takes · d = director mic, notes and cues only, never a take)",
            self.character
        );
        for r in &self.recording {
            if let Some(p) = r.pause_before_s {
                let _ = writeln!(t, "— {p:.0} s —");
            }
            let tag = if r.tag.is_empty() {
                String::new()
            } else {
                format!(" [{}]", r.tag)
            };
            let _ = writeln!(t, "{} {}{}: {}", r.id, r.speaker, tag, r.text);
        }
        if !self.hints.is_empty() {
            let _ = writeln!(
                t,
                "\nMARKED BY THE EDITOR (judge these ids too, whatever their tag)"
            );
            for h in &self.hints {
                let line = h
                    .line
                    .as_deref()
                    .map(|l| format!(" → {l}"))
                    .unwrap_or_default();
                let _ = writeln!(t, "{}{}: {}", h.ids.join(", "), line, h.note);
            }
        }
        if !self.examples.is_empty() {
            let _ = writeln!(t, "\nEARLIER CORRECTIONS BY THE EDITOR");
            for e in &self.examples {
                let _ = writeln!(t, "- {e}");
            }
        }
        Some(t)
    }
}

/// JSON schema of a task's output.
pub fn output_schema<T: AiTaskIo>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T::Output)).unwrap_or_default()
}
