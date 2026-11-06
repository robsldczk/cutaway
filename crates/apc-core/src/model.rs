//! Typed document entities. Each entity is stored as one row of the `entity` table
//! (see `docs/PLAN.md`, schema) and changed only through the command bus.

use std::collections::{BTreeMap, HashSet};

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
    ReviewEvent,
    ReviewRound,
    ReviewArrival,
}

impl Kind {
    pub const ALL: [Kind; 22] = [
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
        Kind::ReviewEvent,
        Kind::ReviewRound,
        Kind::ReviewArrival,
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
            Kind::ReviewEvent => "review_event",
            Kind::ReviewRound => "review_round",
            Kind::ReviewArrival => "review_arrival",
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
    /// Multiplies the scene's default pauses and spacers (1.0 = as set, 0.5 = tighter);
    /// a negative default gap (overlap) is divided by it instead, so tighter overlaps more.
    #[serde(default = "one")]
    pub tightness: f64,
    /// What the writers know about the scene (Throughline beat, Fountain synopsis).
    #[serde(default)]
    pub story: SceneStory,
    /// The room the scene's sounds share (each scene has its own, D76).
    #[serde(default)]
    pub room: SceneRoom,
    /// The ambience looping under this scene. Without one it plays what its location has (D97).
    #[serde(default)]
    pub tone: Option<SceneTone>,
    /// Delivered: its timing holds still until it's unlocked (D108).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<SceneLock>,
    /// How far along the editor says it is. `None` until they say (D119).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<SceneStatus>,
    /// Where the characters stand in this room, by character id, −1 left … +1 right. Every scene
    /// of a location holds the same values (D122).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub room_pan: BTreeMap<String, f64>,
    /// This scene's own values, over the room's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub voice_pan: BTreeMap<String, f64>,
}

impl Scene {
    /// Where a character's voice sits in this scene: its own value, or the room's.
    pub fn pan_of(&self, character: &str) -> f64 {
        self.voice_pan
            .get(character)
            .or_else(|| self.room_pan.get(character))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Where a scene stands, as the editor marks it on the scene rail.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SceneStatus {
    InProgress,
    Review,
    Finished,
}

/// Why a scene's timing is held, and since when. See `timeline::timing_print` for what it holds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct SceneLock {
    /// The export the lock protects; `None` for a scene locked by hand before any export.
    pub version: Option<u32>,
    /// The day it was locked, `2026-11-05`.
    pub at: String,
    /// A newer script version changed lines inside the scene while it was locked.
    pub script_changed: bool,
}

impl SceneLock {
    /// `5 Nov`
    pub fn day(&self) -> String {
        chrono::NaiveDate::parse_from_str(&self.at, "%Y-%m-%d")
            .map(|d| d.format("%-d %b").to_string())
            .unwrap_or_else(|_| self.at.clone())
    }

    /// `Exported as v003 on 5 Nov.`, or nothing for a scene never exported.
    pub fn story(&self) -> Option<String> {
        self.version
            .map(|v| format!("Exported as v{v:03} on {}.", self.day()))
    }
}

impl Scene {
    /// `Sc. 2`
    pub fn label(&self) -> String {
        format!("Sc. {}", self.number)
    }

    /// What an edit that would move something here is told.
    pub fn lock_refusal(&self) -> Option<String> {
        let lock = self.lock.as_ref()?;
        let story = lock.story().map(|s| format!(" {s}")).unwrap_or_default();
        Some(format!(
            "{} is locked.{story} Timing can't change; unlock it first.",
            self.label()
        ))
    }

    /// Dark outside: the time of day a place may have a second ambience for.
    pub fn is_night(&self) -> bool {
        let t = self.time_of_day.to_lowercase();
        ["night", "evening", "dusk", "dark"]
            .iter()
            .any(|w| t.contains(w))
    }
}

/// A library sound looping under a whole scene.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct SceneTone {
    pub asset_id: String,
    pub gain_db: f64,
}

/// The one room all sound effects of a scene play in: they meet on a shared reverb bus, so
/// they sit in the same space. `apc_audio::room::RoomShape` turns these into a sound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, schemars::JsonSchema)]
#[serde(default)]
pub struct SceneRoom {
    /// 0 small … 1 large: sets the decay time and the pre-delay.
    pub size: f64,
    /// 0 bright … 1 dark: how much faster the highs die away.
    pub damping: f64,
    /// 0 dry … 1 full: overall level of the room.
    pub wet: f64,
}

impl Default for SceneRoom {
    /// A small furnished room: present, but no one would call it a hall.
    fn default() -> Self {
        SceneRoom {
            size: 0.35,
            damping: 0.5,
            wet: 0.5,
        }
    }
}

/// Story context of a scene, replaced by each script version. Read-only in the app.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct SceneStory {
    /// Beat title, e.g. "Cold Open – Burrito".
    pub title: String,
    /// The writers' summary of the scene.
    pub synopsis: String,
    /// Act or top-level section, e.g. "Act 1: Setup".
    pub act: String,
    /// Characters present, speaking or not (upper case, like cues).
    pub cast: Vec<String>,
    /// Emotional charge from the story arc, -1 (bad fortune) … +1 (good fortune).
    pub tone: Option<f64>,
    /// Throughline beat id: keeps the scene's identity when its heading changes.
    pub beat_id: Option<String>,
}

impl SceneStory {
    pub fn is_empty(&self) -> bool {
        self == &SceneStory::default()
    }
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
    /// Not in the script: from the unmatched bin, or added by hand in the mapping editor.
    #[serde(default)]
    pub adlib: bool,
    /// Deleted from the cut. It stays here, off the scene's order, so the script keeps its
    /// shape: the matcher still claims its audio and a new script version doesn't bring it back.
    #[serde(default)]
    pub removed: bool,
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
    /// The writers' notes on the character (Throughline), newest import wins.
    #[serde(default)]
    pub profile: CharacterProfile,
    /// How much of the scene's room the voice takes, 0 dry … 1 (D90).
    #[serde(default = "default_voice_wet")]
    pub wet: f64,
}
entity!(Character, Character);

/// A little: the voice stays close and whole, and sits in the same space as its sounds.
pub const DEFAULT_VOICE_WET: f64 = 0.25;

fn default_voice_wet() -> f64 {
    DEFAULT_VOICE_WET
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct CharacterProfile {
    pub summary: String,
    pub want: String,
    pub need: String,
    pub flaw: String,
    pub notes: String,
}

impl CharacterProfile {
    pub fn is_empty(&self) -> bool {
        self == &CharacterProfile::default()
    }

    /// One line for prompts and tooltips: "Soldier, lazy … Wants to chill. Flaw: too trusting."
    pub fn one_line(&self) -> String {
        let flat = |s: &str| {
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim_end_matches('.')
                .to_string()
        };
        let mut parts: Vec<String> = Vec::new();
        for (label, v) in [
            ("", &self.summary),
            ("Wants ", &self.want),
            ("Needs ", &self.need),
            ("Flaw: ", &self.flaw),
        ] {
            let v = flat(v);
            if !v.is_empty() {
                parts.push(format!("{label}{v}"));
            }
        }
        parts.join(". ")
    }
}

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

/// Where an ambience starts: under the voices.
pub const DEFAULT_TONE_DB: f64 = -28.0;

fn default_tone_gain() -> f64 {
    DEFAULT_TONE_DB
}

/// A script location (from scene headings); carries its room tone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Location {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub room_tone: RoomTone,
    /// The writers' description of the setting (Throughline), newest import wins.
    #[serde(default)]
    pub description: String,
    /// The setting this one belongs to (location id), e.g. Canteen → TARMAC.
    #[serde(default)]
    pub parent: Option<String>,
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
    /// In and out marked by hand in the mapping editor: heard exactly as marked, and a re-run
    /// of the analysis leaves it alone.
    Marked,
    /// Found by the matcher, its text corrected by hand. It keeps its edges and handles, and a
    /// re-run leaves it alone.
    Corrected,
}

impl TakeOrigin {
    /// A person settled this take: a re-run of the analysis doesn't replace it.
    pub fn by_hand(self) -> bool {
        matches!(self, TakeOrigin::Marked | TakeOrigin::Corrected)
    }
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

/// A line running next to the one before it, so the two are heard together. It only counts while
/// that line really comes right before it on the timeline; anything else leaves it in the chain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Beside {
    pub line_id: String,
    /// How long after that line's start this one starts; negative starts it first.
    #[serde(default)]
    pub offset_s: f64,
}

fn is_close(depth: &f64) -> bool {
    *depth == 0.0
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
    /// Runs next to the line before it instead of after it (D125).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beside: Option<Beside>,
    #[serde(default)]
    pub gain_db: f64,
    /// How far back in the room this line is spoken, 0 (where the character stands) … 1 (far).
    /// It stays when the take changes: staging belongs to the line (D129).
    #[serde(default, skip_serializing_if = "is_close")]
    pub depth: f64,
    /// Split points (Edit page), in source order once laid out.
    #[serde(default)]
    pub split_points: Vec<SplitPoint>,
    /// The first part (the whole line without splits) is silent but keeps its time.
    #[serde(default)]
    pub muted: bool,
    /// The first part is left out and the time closes up (ripple delete).
    #[serde(default)]
    pub dropped: bool,
    /// Fades at the start and end of the line (the edge fade is the minimum).
    #[serde(default)]
    pub fade_in_s: f64,
    #[serde(default)]
    pub fade_out_s: f64,
    /// Old pick kept as a suggestion after a script change.
    #[serde(default)]
    pub suggested_take: Option<String>,
    /// Takes deleted by hand: a re-run of the analysis doesn't bring them back.
    #[serde(default)]
    pub dismissed: Vec<DismissedTake>,
}

/// What's remembered of a deleted take.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct DismissedTake {
    pub take_id: String,
    pub source_id: String,
    pub t0: f64,
    pub t1: f64,
}

impl DismissedTake {
    /// Is this the performance that was deleted? The same id, or most of the same stretch:
    /// a re-run can place its edges a little differently.
    pub fn covers(&self, t: &Take) -> bool {
        if t.id == self.take_id {
            return true;
        }
        let shared = self.t1.min(t.t1) - self.t0.max(t.t0);
        let shorter = (self.t1 - self.t0).min(t.t1 - t.t0);
        t.source_id == self.source_id && shorter > 0.0 && shared > shorter / 2.0
    }
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
    /// A character's dialogue lane (`Placement::character`). A sound on the adlib track, which
    /// is gone, reads as being on its character's lane (D118).
    #[serde(alias = "adlib")]
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
    /// Whose voice it is (`Lane::Dialogue`).
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
    /// −1 left … +1 right.
    #[serde(default)]
    pub pan: f64,
    /// 0 close … 1 far: how far back the sound sits in its scene's room.
    #[serde(default)]
    pub depth: f64,
    /// The fade out follows the length (`auto_fade_out_s`). Setting it by hand turns this
    /// off. Off in projects from before it existed, so their fades stay as they were set.
    #[serde(default)]
    pub auto_fade: bool,
    /// Order the sounds were added in (1, 2, …): breaks ties between sounds that start
    /// together. 0 for sounds from before it existed.
    #[serde(default)]
    pub added: u64,
    /// Repeats its file: the one kind of sound that may outlast its source.
    #[serde(default)]
    pub looped: bool,
    /// Glue turned off by hand: it stays at its time, and moving it doesn't hang it on a line
    /// again (D123).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unglued: bool,
}
entity!(Placement, Placement, parent = scene_id);

impl Placement {
    /// The automatic fade out of a sound of this length: short sounds get
    /// `min(0.3 s, 30 %)`, beds over 8 s a slow 2 s.
    pub fn auto_fade_out_s(len_s: f64) -> f64 {
        if len_s > 8.0 {
            2.0
        } else {
            (len_s * 0.3).clamp(0.0, 0.3)
        }
    }

    /// Whether the sound plays in the scene's room. Music is outside the story's space and
    /// vocal sounds on a dialogue lane belong with the dialogue, so depth means nothing for
    /// them (D76).
    pub fn in_room(&self) -> bool {
        matches!(self.lane, Lane::Sfx | Lane::Ambience)
    }
}

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
    /// Cut from the episode's own recordings. Nothing is cut any more; the ones still used in
    /// an older episode stay this (D118).
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

impl Asset {
    /// Beds and room tones are made to repeat.
    pub fn loops_by_default(&self) -> bool {
        self.category.starts_with("Ambience")
    }
}

/// A search result that hasn't been downloaded yet.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Preview {
    pub provider: String,
    pub remote_id: String,
    pub title: String,
    #[serde(default)]
    pub duration_s: f64,
    /// Added by hand (dropped in from a search); a fresh keyword search keeps it.
    #[serde(default)]
    pub pinned: bool,
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
    /// How far into the provider's results the keyword search has read ("Load more").
    #[serde(default)]
    pub search_offset: usize,
    /// In a show: the bucket's folder in the show's library (`sfx/doors`). Its files are the
    /// bucket's sounds; new sounds of the bucket are saved there (D65).
    #[serde(default)]
    pub folder: Option<String>,
    /// Loop set by hand, by asset or provider id. Others follow `Asset::loops_by_default`.
    #[serde(default)]
    pub loops: BTreeMap<String, bool>,
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
    pub review_events: BTreeMap<String, crate::review::ReviewEvent>,
    pub review_rounds: BTreeMap<String, crate::review::ReviewRound>,
    pub review_arrivals: BTreeMap<String, crate::review::ReviewArrival>,
}

/// A character's vocal bucket, in a file from before they were removed. Undo and backups can
/// bring one back as well as a load, so it's turned away here (D118).
fn is_vocal_bucket(v: &Value) -> bool {
    v.get("character").is_some_and(|c| !c.is_null())
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
                let value = value.filter(|v| !(kind == Kind::Bucket && is_vocal_bucket(v)));
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
    ReviewEvent => review_events,
    ReviewRound => review_rounds,
    ReviewArrival => review_arrivals,
);

impl Document {
    /// Takes the sounds the analysis used to cut for the vocal buckets out of a loaded document:
    /// nothing lists them any more. One still in use stays, and the rows and files are left as
    /// they are (D118).
    pub(crate) fn drop_unused_cuts(&mut self) {
        let mut used: HashSet<&str> = self
            .placements
            .values()
            .map(|p| p.asset_id.as_str())
            .collect();
        used.extend(
            self.buckets
                .values()
                .flat_map(|b| b.kept.iter().map(String::as_str)),
        );
        used.extend(
            self.scenes
                .values()
                .filter_map(|s| s.tone.as_ref())
                .map(|t| t.asset_id.as_str()),
        );
        used.extend(
            self.locations
                .values()
                .filter_map(|l| l.room_tone.asset_id.as_deref()),
        );
        let unused: Vec<String> = self
            .assets
            .values()
            .filter(|a| a.source == AssetSource::Recorded && !used.contains(a.id.as_str()))
            .map(|a| a.id.clone())
            .collect();
        for id in unused {
            self.assets.remove(&id);
        }
    }

    /// The ambience a scene plays. Places share: without one of its own a scene plays what
    /// its location has, for its time of day if that's set apart, then what the setting it
    /// belongs to has, then the location's room tone (D97).
    pub fn room_tone_for(&self, scene: &Scene) -> Option<RoomTone> {
        let tone = scene.tone.as_ref().or_else(|| self.shared_tone(scene));
        let own = tone.and_then(|t| {
            let a = self.assets.get(&t.asset_id)?;
            Some(RoomTone {
                enabled: true,
                source_id: None,
                t0: 0.0,
                t1: a.duration_s,
                asset_id: Some(a.id.clone()),
                gain_db: t.gain_db,
            })
        });
        own.or_else(|| {
            self.room_tone_of(&crate::script::location_id(&scene.location))
                .cloned()
        })
    }

    /// What the rest of a scene's place plays, nearest first.
    fn shared_tone(&self, scene: &Scene) -> Option<&SceneTone> {
        let mut loc = crate::script::location_id(&scene.location);
        // settings stack one level deep in Throughline; the bound guards against loops
        for _ in 0..8 {
            let mut here: Vec<&Scene> = self
                .scenes
                .values()
                .filter(|s| s.tone.is_some() && crate::script::location_id(&s.location) == loc)
                .collect();
            here.sort_by_key(|s| s.number);
            let found = here
                .iter()
                .find(|s| s.is_night() == scene.is_night())
                .or(here.first())
                .and_then(|s| s.tone.as_ref());
            if found.is_some() {
                return found;
            }
            loc = self.locations.get(&loc)?.parent.clone()?;
        }
        None
    }

    /// The room tone a location plays: its own when enabled, else the nearest parent's
    /// that is (Canteen → TARMAC), else its own (disabled) one.
    pub fn room_tone_of(&self, location_id: &str) -> Option<&RoomTone> {
        let own = self.locations.get(location_id)?;
        let mut cur = own;
        // settings stack one level deep in Throughline; the bound guards against loops
        for _ in 0..8 {
            if cur.room_tone.enabled {
                return Some(&cur.room_tone);
            }
            match cur.parent.as_deref().and_then(|p| self.locations.get(p)) {
                Some(p) => cur = p,
                None => break,
            }
        }
        Some(&own.room_tone)
    }

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

    /// Fingerprint of what sound spotting reads of a scene (heading, location, lines), so
    /// a scene is spotted again only when its script changed.
    pub fn scene_script_hash(&self, scene_id: &str) -> String {
        let mut h = blake3::Hasher::new();
        if let Some(s) = self.scenes.get(scene_id) {
            h.update(s.heading.as_bytes());
            h.update(b"\0");
            h.update(s.location.as_bytes());
        }
        for l in self.lines_of_scene(scene_id) {
            h.update(b"\n");
            h.update(l.id.as_bytes());
            h.update(format!("|{:?}|", l.kind).as_bytes());
            h.update(l.character.as_deref().unwrap_or("").as_bytes());
            h.update(b"|");
            h.update(l.text.as_bytes());
        }
        h.finalize().to_hex()[..16].to_string()
    }

    /// Setting key under which spotting records the scene hash it last read.
    pub fn spot_key(scene_id: &str) -> String {
        format!("spot:{scene_id}")
    }

    /// The scene was spotted and its script hasn't changed since.
    pub fn scene_spotted(&self, scene_id: &str) -> bool {
        self.setting(&Self::spot_key(scene_id))
            .and_then(|v| v["hash"].as_str())
            .map(|h| h == self.scene_script_hash(scene_id))
            .unwrap_or(false)
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
