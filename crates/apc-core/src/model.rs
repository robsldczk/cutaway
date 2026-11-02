//! Typed document entities. Each entity is stored as one row of the `entity` table
//! (see `docs/PLAN.md`, schema) and changed only through the command bus.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CoreError, Result};

/// Entity kinds, as stored in the `entity.kind` column.
#[derive(
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Setting,
    ScriptVersion,
    Scene,
    Line,
    Character,
    Location,
    Session,
    Source,
    Take,
    Note,
    Unmatched,
    Box,
    Spacer,
    Cue,
    Placement,
    Asset,
    Bucket,
    LaneMix,
    Mark,
}

impl Kind {
    pub const ALL: [Kind; 19] = [
        Kind::Setting,
        Kind::ScriptVersion,
        Kind::Scene,
        Kind::Line,
        Kind::Character,
        Kind::Location,
        Kind::Session,
        Kind::Source,
        Kind::Take,
        Kind::Note,
        Kind::Unmatched,
        Kind::Box,
        Kind::Spacer,
        Kind::Cue,
        Kind::Placement,
        Kind::Asset,
        Kind::Bucket,
        Kind::LaneMix,
        Kind::Mark,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Setting => "setting",
            Kind::ScriptVersion => "script_version",
            Kind::Scene => "scene",
            Kind::Line => "line",
            Kind::Character => "character",
            Kind::Location => "location",
            Kind::Session => "session",
            Kind::Source => "source",
            Kind::Take => "take",
            Kind::Note => "note",
            Kind::Unmatched => "unmatched",
            Kind::Box => "box",
            Kind::Spacer => "spacer",
            Kind::Cue => "cue",
            Kind::Placement => "placement",
            Kind::Asset => "asset",
            Kind::Bucket => "bucket",
            Kind::LaneMix => "lane_mix",
            Kind::Mark => "mark",
        }
    }

    pub fn parse(s: &str) -> Result<Kind> {
        Kind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| CoreError::invalid(format!("unknown entity kind '{s}'")))
    }
}

/// Common behaviour of all stored entities.
pub trait Entity: Serialize + DeserializeOwned + Clone {
    const KIND: Kind;
    fn id(&self) -> &str;
    fn parent(&self) -> Option<&str> {
        None
    }
    fn ord(&self) -> f64 {
        0.0
    }
}

macro_rules! entity {
    ($ty:ty, $kind:ident) => {
        impl Entity for $ty {
            const KIND: Kind = Kind::$kind;
            fn id(&self) -> &str {
                &self.id
            }
        }
    };
    ($ty:ty, $kind:ident, parent = $parent:ident) => {
        impl Entity for $ty {
            const KIND: Kind = Kind::$kind;
            fn id(&self) -> &str {
                &self.id
            }
            fn parent(&self) -> Option<&str> {
                Some(&self.$parent)
            }
        }
    };
    ($ty:ty, $kind:ident, parent = $parent:ident, ord = $ord:ident) => {
        impl Entity for $ty {
            const KIND: Kind = Kind::$kind;
            fn id(&self) -> &str {
                &self.id
            }
            fn parent(&self) -> Option<&str> {
                Some(&self.$parent)
            }
            fn ord(&self) -> f64 {
                self.$ord as f64
            }
        }
    };
}

/// A project setting (key = id). The project config lives under id `config`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Setting {
    pub id: String,
    pub value: Value,
}
entity!(Setting, Setting);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct ScriptVersion {
    pub id: String,
    pub number: u32,
    /// Path relative to the episode folder (`Script/v001 Name.fountain`).
    pub file: String,
    pub name: String,
    pub hash: String,
    pub imported_at: String,
    #[serde(default)]
    pub summary: Value,
    #[serde(default)]
    pub title: BTreeMap<String, String>,
}
entity!(ScriptVersion, ScriptVersion);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Scene {
    pub id: String,
    pub number: u32,
    pub heading: String,
    #[serde(default)]
    pub int_ext: String,
    /// Full location, e.g. "TARMAC - CANTEEN".
    #[serde(default)]
    pub location: String,
    /// Short place name, e.g. "Canteen".
    #[serde(default)]
    pub place: String,
    #[serde(default)]
    pub time_of_day: String,
    /// Timeline items in order: line ids (dialogue boxes and action spacers).
    #[serde(default)]
    pub order: Vec<String>,
    /// Scales every gap and spacer of the scene (1.0 = as set).
    #[serde(default = "one")]
    pub tightness: f64,
}

fn one() -> f64 {
    1.0
}

impl Entity for Scene {
    const KIND: Kind = Kind::Scene;
    fn id(&self) -> &str {
        &self.id
    }
    fn ord(&self) -> f64 {
        self.number as f64
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Dialogue,
    Action,
    Transition,
    Lyric,
    Centered,
}

/// One script unit: a dialogue sub-line (split at parentheticals) or an action line.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct ScriptLine {
    pub id: String,
    pub scene_id: String,
    pub kind: LineKind,
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub extension: Option<String>,
    pub text: String,
    #[serde(default)]
    pub paren: Option<String>,
    /// Display tokens of `text` (what the UI shows word by word).
    #[serde(default)]
    pub tokens: Vec<String>,
    /// Normalized words, one per token.
    #[serde(default)]
    pub words: Vec<String>,
    /// Position within the scene in script order (1-based, like Stage 0 unit numbers).
    pub number: u32,
    #[serde(default)]
    pub version_added: u32,
    /// Text changed in the latest script version (takes need re-matching).
    #[serde(default)]
    pub changed: bool,
    #[serde(default)]
    pub forced: bool,
    /// Created from the unmatched bin (not in the script).
    #[serde(default)]
    pub adlib: bool,
}
entity!(ScriptLine, Line, parent = scene_id, ord = number);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Character {
    /// Upper-case script name, e.g. "THE COLONEL".
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub color_index: usize,
}
entity!(Character, Character);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct RoomTone {
    pub enabled: bool,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub t0: f64,
    #[serde(default)]
    pub t1: f64,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default = "default_tone_gain")]
    pub gain_db: f64,
}

fn default_tone_gain() -> f64 {
    -6.0
}

/// A script location (from scene headings); carries its room tone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Location {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub room_tone: RoomTone,
}
entity!(Location, Location);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Session {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tc_start_s: Option<f64>,
    #[serde(default)]
    pub source_ids: Vec<String>,
}
entity!(Session, Session);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stored {
    Copied,
    Referenced,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Character,
    Room,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct SourceFile {
    pub id: String,
    /// Relative to the episode folder when inside it, absolute otherwise.
    pub path: String,
    pub original_name: String,
    pub stored: Stored,
    pub hash: String,
    pub size: u64,
    pub role: Role,
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub role_confidence: f64,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default)]
    pub channels: u16,
    #[serde(default)]
    pub duration_s: f64,
    #[serde(default)]
    pub tc_start_s: Option<f64>,
    /// Measured sync: this file runs `offset_s` late against the session reference,
    /// so session time = file time − offset_s.
    #[serde(default)]
    pub offset_s: f64,
    #[serde(default)]
    pub offset_confidence: f64,
    /// Transcript sidecar (fixture JSON) when present.
    #[serde(default)]
    pub transcript: Option<String>,
    #[serde(default)]
    pub missing: bool,
    /// The user set this file's room link by hand; "Redo match" keeps it.
    #[serde(default)]
    pub link_locked: bool,
}
entity!(SourceFile, Source);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TakeType {
    Full,
    Partial,
    Pickup,
    Micro,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct TakeWord {
    /// Normalized word.
    pub w: String,
    /// As heard.
    pub raw: String,
    pub t0: f64,
    pub t1: f64,
    /// Index of the script word this word maps to (`ScriptLine::words`).
    #[serde(default)]
    pub script_idx: Option<usize>,
}

/// One performance of one line: a source range with word timings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Take {
    pub id: String,
    pub line_id: String,
    pub source_id: String,
    pub take_no: u32,
    pub take_type: TakeType,
    /// Source-file seconds (edges snapped to speech).
    pub t0: f64,
    pub t1: f64,
    #[serde(default)]
    pub words: Vec<TakeWord>,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub heard: String,
    #[serde(default)]
    pub lufs: Option<f64>,
    #[serde(default)]
    pub db: f64,
    #[serde(default)]
    pub partial: bool,
    /// Director pick strength: 0 none, 1 liked, 2 explicit pick.
    #[serde(default)]
    pub pick: u8,
    #[serde(default)]
    pub favorite: bool,
    /// "director" or "user".
    #[serde(default)]
    pub favorite_origin: Option<String>,
    /// Condensed notes (a few words).
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub feedback: String,
    /// The room speech the notes were condensed from.
    #[serde(default)]
    pub direction_raw: String,
    #[serde(default)]
    pub feedback_raw: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note_ids: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub needs_rematch: bool,
    #[serde(default)]
    pub ai_flag: Option<String>,
    /// Who found it. Shown only in the dev panel; the normal UI treats all takes alike.
    #[serde(default)]
    pub origin: TakeOrigin,
    /// What the actor actually said, as Claude read it (rescued takes; dev panel only).
    #[serde(default)]
    pub read_as: Option<String>,
}
entity!(Take, Take, parent = line_id, ord = take_no);

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TakeOrigin {
    /// The deterministic script matcher.
    #[default]
    Matcher,
    /// Claude, from session-mic speech the matcher left over (misheard by the ASR).
    Rescue,
    /// Linked by hand in the dev panel.
    Manual,
}

impl Take {
    pub fn duration(&self) -> f64 {
        (self.t1 - self.t0).max(0.0)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteClass {
    CueReading,
    Direction,
    Feedback,
    Chatter,
}

/// A room-mic speech segment.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Note {
    pub id: String,
    pub source_id: String,
    pub t0: f64,
    pub t1: f64,
    pub text: String,
    pub class: NoteClass,
    #[serde(default)]
    pub take_ids: Vec<String>,
    #[serde(default)]
    pub line_id: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
}
entity!(Note, Note);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    MissedTake,
    WrongLine,
    NotATake,
    Edges,
    Other,
}

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MarkStatus {
    #[default]
    Open,
    /// Claude answered; the proposal waits for Apply or Discard.
    Proposed,
    Applied,
    Discarded,
}

/// What applying a mark would change.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, schemars::JsonSchema)]
pub struct MarkProposal {
    #[serde(default)]
    pub add: Vec<Take>,
    /// Take ids to remove.
    #[serde(default)]
    pub remove: Vec<String>,
    /// One line per decision, for the dev panel.
    #[serde(default)]
    pub summary: Vec<String>,
}

/// A stretch of a session mic Rob pointed at in the dev panel ("this is a take of L3").
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct DevMark {
    pub id: String,
    pub source_id: String,
    /// Source-file seconds.
    pub t0: f64,
    pub t1: f64,
    pub kind: MarkKind,
    #[serde(default)]
    pub line_id: Option<String>,
    #[serde(default)]
    pub note: String,
    /// The transcript text of the stretch when it was marked.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub status: MarkStatus,
    #[serde(default)]
    pub proposal: Option<MarkProposal>,
    #[serde(default)]
    pub created_at: String,
}
entity!(DevMark, Mark);

/// Loud role-track speech that matches no line (ad-libs, questions, chatter).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Unmatched {
    pub id: String,
    #[serde(default)]
    pub scene_id: Option<String>,
    pub source_id: String,
    pub t0: f64,
    pub t1: f64,
    pub text: String,
    #[serde(default)]
    pub db: f64,
    /// "analysis" or "deleted_line".
    pub origin: String,
    #[serde(default)]
    pub used_as: Option<String>,
}
entity!(Unmatched, Unmatched);

/// A comp segment: script words `[from_word, to_word)` taken from `take_id`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct CompSegment {
    pub take_id: String,
    pub from_word: usize,
    pub to_word: usize,
    #[serde(default)]
    pub nudge_s: f64,
}

/// Word-level edits of a box, indexed into the box's rendered word list.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct WordEdits {
    #[serde(default)]
    pub deleted: Vec<usize>,
    /// Cuts (identified by the first deleted word of the run) that keep their pause.
    #[serde(default)]
    pub keep_pause: Vec<usize>,
    /// Pause shortening (seconds) after word index.
    #[serde(default)]
    pub pause_trims: BTreeMap<usize, f64>,
}

impl WordEdits {
    pub fn is_empty(&self) -> bool {
        self.deleted.is_empty() && self.keep_pause.is_empty() && self.pause_trims.is_empty()
    }
}

/// Edit state of one dialogue line on the timeline (id = line id).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct BoxState {
    pub id: String,
    #[serde(default)]
    pub take_id: Option<String>,
    #[serde(default)]
    pub comp: Vec<CompSegment>,
    #[serde(default)]
    pub edits: WordEdits,
    /// Seconds trimmed from the head (negative extends into the handle).
    #[serde(default)]
    pub head_s: f64,
    #[serde(default)]
    pub tail_s: f64,
    /// Pause after the previous item; `None` uses the scene's breath gap.
    #[serde(default)]
    pub gap_s: Option<f64>,
    /// Absolute scene time (Edit page pin); overrides the gap.
    #[serde(default)]
    pub pinned_s: Option<f64>,
    #[serde(default)]
    pub gain_db: f64,
    /// Split points (Edit page), in source order once laid out.
    #[serde(default)]
    pub split_points: Vec<SplitPoint>,
    /// The first part (the whole line without splits) is silent but keeps its time.
    #[serde(default)]
    pub muted: bool,
    /// The first part is left out and the time closes up (ripple delete).
    #[serde(default)]
    pub dropped: bool,
    /// Old pick kept as a suggestion after a script change.
    #[serde(default)]
    pub suggested_take: Option<String>,
}

impl Entity for BoxState {
    const KIND: Kind = Kind::Box;
    fn id(&self) -> &str {
        &self.id
    }
    fn parent(&self) -> Option<&str> {
        Some(&self.id)
    }
}

impl BoxState {
    /// True when the box holds edits the Options/Arrange pages cannot show.
    pub fn edited_in_edit(&self) -> bool {
        self.pinned_s.is_some() || !self.split_points.is_empty()
    }

    /// Number of parts the splits cut the line into.
    pub fn parts(&self) -> usize {
        self.split_points.len() + 1
    }

    /// Mute state of every part.
    pub fn all_muted(&self) -> bool {
        self.muted && self.split_points.iter().all(|s| s.muted)
    }

    /// Drop state of every part.
    pub fn all_dropped(&self) -> bool {
        self.dropped && self.split_points.iter().all(|s| s.dropped)
    }
}

/// A split of a dialogue clip, by source time so it survives trims and word edits.
/// It holds the state of the part after it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct SplitPoint {
    pub take_id: String,
    /// Source time of the split.
    pub src_s: f64,
    /// Extra time before the part after the split (negative overlaps the part before).
    #[serde(default)]
    pub offset_s: f64,
    /// Trimmed from the end of the part before the split (≥ 0).
    #[serde(default)]
    pub trim_before_s: f64,
    /// Trimmed from the start of the part after the split (≥ 0).
    #[serde(default)]
    pub trim_after_s: f64,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub dropped: bool,
}

/// Spacer for an action line (id = action line id).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Spacer {
    pub id: String,
    #[serde(default)]
    pub duration_s: Option<f64>,
    #[serde(default = "yes")]
    pub enabled: bool,
}
entity!(Spacer, Spacer);

fn yes() -> bool {
    true
}

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    #[default]
    Start,
    End,
}

/// Where a sound hangs on the dialogue: a line (or a word of it) plus an edge.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Anchor {
    pub line_id: String,
    #[serde(default)]
    pub word: Option<usize>,
    #[serde(default)]
    pub edge: Edge,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CueType {
    Hard,
    Foley,
    Ambience,
    Walla,
    Music,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CueStatus {
    Wanted,
    Candidates,
    Chosen,
    Placed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct SoundCue {
    pub id: String,
    pub scene_id: String,
    pub description: String,
    #[serde(default)]
    pub anchor: Option<Anchor>,
    pub cue_type: CueType,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "two")]
    pub priority: u8,
    pub status: CueStatus,
    #[serde(default)]
    pub candidates: Vec<String>,
    #[serde(default)]
    pub chosen: Option<String>,
    #[serde(default)]
    pub show_cue_id: Option<String>,
    #[serde(default)]
    pub ai_suggested: bool,
    #[serde(default)]
    pub accepted: bool,
}
entity!(SoundCue, Cue, parent = scene_id);

fn two() -> u8 {
    2
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Sfx,
    Ambience,
    Music,
    /// A character's dialogue lane (`Placement::character`): vocal sounds from the actor's
    /// own recordings (breaths, "mh-hm", efforts) sit with that character's lines.
    Dialogue,
}

/// A sound placed on the timeline.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Placement {
    pub id: String,
    pub scene_id: String,
    pub asset_id: String,
    #[serde(default)]
    pub cue_id: Option<String>,
    pub lane: Lane,
    /// The character whose dialogue lane holds this sound (`Lane::Dialogue`).
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub layer: u32,
    /// Anchored placements follow their line; otherwise `at_s` is absolute scene time.
    #[serde(default)]
    pub anchor: Option<Anchor>,
    #[serde(default)]
    pub at_s: f64,
    #[serde(default)]
    pub offset_s: f64,
    /// Asset in point and length (seconds).
    #[serde(default)]
    pub in_s: f64,
    pub len_s: f64,
    #[serde(default)]
    pub fade_in_s: f64,
    #[serde(default)]
    pub fade_out_s: f64,
    #[serde(default)]
    pub fade_curve: FadeCurve,
    #[serde(default)]
    pub gain_db: f64,
    /// Silent but kept in place (Edit page clip mute).
    #[serde(default)]
    pub muted: bool,
}
entity!(Placement, Placement, parent = scene_id);

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FadeCurve {
    #[default]
    EqualPower,
    Linear,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetSource {
    Imported,
    Licensed,
    Generated,
    /// Cut from the episode's own recordings (a character's vocal sounds).
    Recorded,
}

/// A library sound (show library, or the local library of a standalone episode).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Asset {
    pub id: String,
    pub file: String,
    pub hash: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub duration_s: f64,
    pub source: AssetSource,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub lufs: Option<f64>,
    /// Id at the provider (Epidemic Sound), so search results that are already kept are known.
    #[serde(default)]
    pub remote_id: Option<String>,
}
entity!(Asset, Asset);

/// A search result that hasn't been downloaded yet.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Preview {
    pub provider: String,
    pub remote_id: String,
    pub title: String,
    #[serde(default)]
    pub duration_s: f64,
}

/// A named group of sounds for one or more scenes: its keywords are the search query.
/// Kept sounds are downloaded library assets; previews are search results not downloaded yet.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct SoundBucket {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub scene_ids: Vec<String>,
    /// Spotted cues this bucket serves (their anchors are where its sounds snap).
    #[serde(default)]
    pub cue_ids: Vec<String>,
    #[serde(default)]
    pub kept: Vec<String>,
    #[serde(default)]
    pub previews: Vec<Preview>,
    #[serde(default)]
    pub dismissed: Vec<String>,
    #[serde(default)]
    pub length_min_s: Option<f64>,
    #[serde(default)]
    pub length_max_s: Option<f64>,
    /// Words that rule a result out ("music", "voices").
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub ai_suggested: bool,
    #[serde(default)]
    pub order: u32,
    /// A character's vocal bucket: filled by the analysis from the actor's own recordings
    /// only (no search, no generation).
    #[serde(default)]
    pub character: Option<String>,
}

impl Entity for SoundBucket {
    const KIND: Kind = Kind::Bucket;
    fn id(&self) -> &str {
        &self.id
    }
    fn ord(&self) -> f64 {
        self.order as f64
    }
}

/// Mix settings of one Edit-page lane (`PAUL`, `sfx1`, `ambience`, `roomtone`, `music`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct LaneMix {
    pub id: String,
    #[serde(default)]
    pub volume_db: f64,
    /// −1 left … +1 right.
    #[serde(default)]
    pub pan: f64,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
}
entity!(LaneMix, LaneMix);

/// The whole episode document, loaded from the `entity` table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub settings: BTreeMap<String, Setting>,
    pub script_versions: BTreeMap<String, ScriptVersion>,
    pub scenes: BTreeMap<String, Scene>,
    pub lines: BTreeMap<String, ScriptLine>,
    pub characters: BTreeMap<String, Character>,
    pub locations: BTreeMap<String, Location>,
    pub sessions: BTreeMap<String, Session>,
    pub sources: BTreeMap<String, SourceFile>,
    pub takes: BTreeMap<String, Take>,
    pub notes: BTreeMap<String, Note>,
    pub unmatched: BTreeMap<String, Unmatched>,
    pub boxes: BTreeMap<String, BoxState>,
    pub spacers: BTreeMap<String, Spacer>,
    pub cues: BTreeMap<String, SoundCue>,
    pub placements: BTreeMap<String, Placement>,
    pub assets: BTreeMap<String, Asset>,
    pub buckets: BTreeMap<String, SoundBucket>,
    pub lane_mix: BTreeMap<String, LaneMix>,
    pub marks: BTreeMap<String, DevMark>,
}

macro_rules! dispatch {
    ($($kind:ident => $field:ident),* $(,)?) => {
        impl Document {
            /// The JSON of one entity, if present.
            pub fn get_value(&self, kind: Kind, id: &str) -> Result<Option<Value>> {
                Ok(match kind {
                    $(Kind::$kind => match self.$field.get(id) {
                        Some(e) => Some(serde_json::to_value(e)?),
                        None => None,
                    },)*
                })
            }

            /// Inserts, replaces (`Some`) or removes (`None`) one entity from JSON.
            pub fn put_value(&mut self, kind: Kind, id: &str, value: Option<&Value>) -> Result<()> {
                match kind {
                    $(Kind::$kind => match value {
                        Some(v) => {
                            let e = serde_json::from_value(v.clone())?;
                            self.$field.insert(id.to_string(), e);
                        }
                        None => {
                            self.$field.remove(id);
                        }
                    },)*
                }
                Ok(())
            }

            /// Number of entities of a kind.
            pub fn count(&self, kind: Kind) -> usize {
                match kind {
                    $(Kind::$kind => self.$field.len(),)*
                }
            }
        }
    };
}

dispatch!(
    Setting => settings,
    ScriptVersion => script_versions,
    Scene => scenes,
    Line => lines,
    Character => characters,
    Location => locations,
    Session => sessions,
    Source => sources,
    Take => takes,
    Note => notes,
    Unmatched => unmatched,
    Box => boxes,
    Spacer => spacers,
    Cue => cues,
    Placement => placements,
    Asset => assets,
    Bucket => buckets,
    LaneMix => lane_mix,
    Mark => marks,
);

impl Document {
    /// Scenes in script order.
    pub fn scenes_in_order(&self) -> Vec<&Scene> {
        let mut v: Vec<&Scene> = self.scenes.values().collect();
        v.sort_by_key(|s| s.number);
        v
    }

    /// Lines of a scene in script order (not timeline order).
    pub fn lines_of_scene(&self, scene_id: &str) -> Vec<&ScriptLine> {
        let mut v: Vec<&ScriptLine> = self
            .lines
            .values()
            .filter(|l| l.scene_id == scene_id)
            .collect();
        v.sort_by_key(|l| l.number);
        v
    }

    /// Takes of a line ordered by take number.
    pub fn takes_of_line(&self, line_id: &str) -> Vec<&Take> {
        let mut v: Vec<&Take> = self
            .takes
            .values()
            .filter(|t| t.line_id == line_id)
            .collect();
        v.sort_by_key(|t| t.take_no);
        v
    }

    pub fn current_script_version(&self) -> Option<&ScriptVersion> {
        self.script_versions.values().max_by_key(|v| v.number)
    }

    /// Finds a scene by id, by number ("2", "S2") or by heading.
    pub fn find_scene(&self, key: &str) -> Option<&Scene> {
        if let Some(s) = self.scenes.get(key) {
            return Some(s);
        }
        let k = key.trim().trim_start_matches(['S', 's']);
        if let Ok(n) = k.parse::<u32>() {
            return self.scenes.values().find(|s| s.number == n);
        }
        self.scenes
            .values()
            .find(|s| s.heading.eq_ignore_ascii_case(key.trim()))
    }

    pub fn setting(&self, key: &str) -> Option<&Value> {
        self.settings.get(key).map(|s| &s.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_get_roundtrip() {
        let mut doc = Document::default();
        let sp = Spacer {
            id: "ln_1".into(),
            duration_s: Some(1.5),
            enabled: true,
        };
        let v = serde_json::to_value(&sp).unwrap();
        doc.put_value(Kind::Spacer, "ln_1", Some(&v)).unwrap();
        assert_eq!(doc.spacers["ln_1"], sp);
        assert_eq!(doc.get_value(Kind::Spacer, "ln_1").unwrap(), Some(v));
        doc.put_value(Kind::Spacer, "ln_1", None).unwrap();
        assert_eq!(doc.count(Kind::Spacer), 0);
    }

    #[test]
    fn kinds_parse_back() {
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.as_str()).unwrap(), k);
        }
    }
}
