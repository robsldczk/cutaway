//! The method registry: every operation the UI, CLI and MCP server can call, with a JSON
//! schema for its parameters. MCP tools are generated from this table.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use apc_core::backup::BackupManager;
use apc_core::ids::new_id;
use apc_core::*;
use apc_export::{bounce, cutlist, fcpxml, srt, EpisodeTimeline, ExportScope};

use crate::error::{AppError, Result};
use crate::service::Service;
use crate::sound_edit::{
    BatchSoundsP, MoveSoundP, PlacementP, SceneRoomP, SlipSoundP, SoundDepthP, SoundFadesP,
    SoundGainP, SoundPanP, TrimSoundP,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    /// Needs `"confirm": true`.
    Destructive,
}

pub struct MethodInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub access: Access,
    /// Exposed as an MCP tool.
    pub mcp: bool,
    pub schema: fn() -> Value,
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or_else(|_| json!({"type": "object"}))
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct NoParams {}

#[derive(Deserialize, JsonSchema)]
pub struct SceneP {
    /// Scene id, number ("2") or "S2". Defaults to the first scene.
    #[serde(default)]
    pub scene: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateShowP {
    /// The show's folder: an existing production folder (it is kept as it is) or a new one.
    #[serde(default)]
    pub root: Option<String>,
    /// Instead of `root`: a new folder `parent/name`.
    #[serde(default)]
    pub parent: Option<String>,
    /// The show's name (defaults to the folder's).
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DemoP {
    pub parent: String,
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ShowPathP {
    /// The show's folder or its `.cutawayshow` file.
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct OpenEpisodeP {
    /// Episode folder name in the open show ("Episode 2").
    pub episode: String,
    /// Make the Cutaway project (and the folders) when the episode has none yet.
    #[serde(default = "yes")]
    pub create: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct MigrateP {
    /// A standalone episode file or its folder.
    pub source: String,
    /// Episode folder name in the open show ("Episode 2").
    pub episode: String,
    /// Only report what would be copied.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateEpisodeP {
    /// Folder to create the episode in (standalone), ignored when `show` is set.
    #[serde(default)]
    pub parent: Option<String>,
    pub name: String,
    /// Show folder or show file.
    #[serde(default)]
    pub show: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct OpenP {
    /// Episode file or folder.
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImportScriptP {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathP {
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImportAudioP {
    /// Files or folders.
    pub paths: Vec<String>,
    /// Copy into the project (default) or reference in place.
    #[serde(default = "yes")]
    pub copy: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct RoleItem {
    pub source_id: String,
    pub role: Role,
    #[serde(default)]
    pub character: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ConfirmRolesP {
    pub assignments: Vec<RoleItem>,
    /// Remember name corrections as show aliases ("Remember my fixes for the whole show").
    #[serde(default = "yes")]
    pub remember: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct RelinkP {
    pub search_dir: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct LineP {
    pub line_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SelectTakeP {
    pub line_id: String,
    #[serde(default)]
    pub take_id: Option<String>,
    #[serde(default)]
    pub take_no: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PassageP {
    pub line_ids: Vec<String>,
    pub take_no: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct GapP {
    pub line_id: String,
    /// Pause before the line in seconds; null resets to the scene default.
    #[serde(default)]
    pub gap_s: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TrimP {
    pub line_id: String,
    pub head_s: f64,
    pub tail_s: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct VersionP {
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct RevertP {
    pub backup_id: String,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct HistoryP {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ExportP {
    /// One scene, or the whole episode when omitted.
    #[serde(default)]
    pub scene: Option<String>,
    /// Output file; defaults to `Exports/`.
    #[serde(default)]
    pub path: Option<String>,
    /// Needed to overwrite an existing file.
    #[serde(default)]
    pub confirm: bool,
    /// Cut list: `csv` (default) or `md`. SRT: include character names.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ExportMixP {
    pub scene: String,
    /// `wav` (default) or `mp3`.
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct SpotP {
    pub scene: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddCueP {
    pub scene: String,
    pub description: String,
    pub cue_type: CueType,
    #[serde(default)]
    pub anchor_line_id: Option<String>,
    #[serde(default)]
    pub anchor_word: Option<usize>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateCueP {
    pub cue_id: String,
    pub patch: Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeleteCueP {
    pub cue_id: String,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct MergeCuesP {
    pub cue_ids: Vec<String>,
    pub into_id: String,
    /// Also record it as a show-level cue reused across episodes.
    #[serde(default)]
    pub show_level: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct AttachP {
    pub cue_id: String,
    pub asset_id: String,
    /// Also make it the chosen sound.
    #[serde(default)]
    pub choose: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchSoundsP {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub cue_id: Option<String>,
    /// Attach the results to the cue as candidates.
    #[serde(default)]
    pub attach: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct GenerateP {
    #[serde(default)]
    pub cue_id: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub variations: Option<u32>,
    #[serde(default)]
    pub duration_s: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PlaceSoundP {
    pub asset_id: String,
    pub scene: String,
    #[serde(default)]
    pub cue_id: Option<String>,
    #[serde(default)]
    pub lane: Option<Lane>,
    /// For `lane: dialogue`: whose lane (a character id).
    #[serde(default)]
    pub character: Option<String>,
    /// Anchor to a line (and word); otherwise `at_s` is absolute scene time.
    #[serde(default)]
    pub anchor_line_id: Option<String>,
    #[serde(default)]
    pub anchor_word: Option<usize>,
    /// Which edge of the line or word: `start` (default) or `end`. The pause after a word is
    /// that word's end.
    #[serde(default)]
    pub anchor_edge: Edge,
    #[serde(default)]
    pub at_s: Option<f64>,
    #[serde(default)]
    pub offset_s: f64,
    /// Where it starts in the room: 0 close … 1 far.
    #[serde(default)]
    pub depth: f64,
    /// Set for a drop from a bucket: the drop's length rules apply, with this Loop setting.
    #[serde(skip)]
    pub dropped: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImportSoundsP {
    pub paths: Vec<String>,
    /// Keep the sounds in this bucket (in a show: saved into its folder).
    #[serde(default)]
    pub bucket_id: Option<String>,
    /// Attach the imported sounds to this cue as candidates.
    #[serde(default)]
    pub cue_id: Option<String>,
    /// With `scene` and `at_s`: place the first sound on the timeline.
    #[serde(default)]
    pub scene: Option<String>,
    #[serde(default)]
    pub at_s: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct LibraryP {
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeleteAssetP {
    pub asset_id: String,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct ConfigP {
    pub key: String,
    pub value: Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct PlayP {
    #[serde(default)]
    pub scene: Option<String>,
    #[serde(default)]
    pub from_s: f64,
    #[serde(default)]
    pub until_s: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ConnectP {
    /// `epidemic` or `elevenlabs`.
    pub service: String,
    pub api_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ServiceP {
    pub service: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct FetchP {
    /// Remote id from `search_sounds` (Epidemic Sound).
    pub remote_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cue_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PlayTakeP {
    pub take_id: String,
    #[serde(default)]
    pub from_s: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct LinkRoomP {
    /// The director room file.
    pub room_id: String,
    /// Recordings it serves (one, or several for an ensemble).
    pub recording_ids: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SourceP {
    pub source_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketsP {
    #[serde(default)]
    pub scene: Option<String>,
    /// "scene" (default), "tag" or "show".
    #[serde(default)]
    pub sort: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SpotBucketsP {
    /// One scene; when absent, the scenes not spotted yet or changed since.
    #[serde(default)]
    pub scene: Option<String>,
    /// Re-spot every scene (one AI call per scene), not only new and changed ones.
    #[serde(default)]
    pub all: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct CleanupMove {
    /// The loose file, its path under `Sound/`.
    pub file: String,
    /// The folder under `Sound/` it goes to (`sfx/kitchen`); made if it doesn't exist.
    pub to: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ApplyCleanupP {
    pub moves: Vec<CleanupMove>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchEpidemicP {
    pub query: String,
    /// Results to skip (the `next_offset` of the previous page).
    #[serde(default)]
    pub offset: usize,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketAddPreviewP {
    pub bucket_id: String,
    /// A result of `search_epidemic`.
    pub preview: Preview,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateBucketP {
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub scenes: Vec<String>,
    #[serde(default)]
    pub length_min_s: Option<f64>,
    #[serde(default)]
    pub length_max_s: Option<f64>,
    /// Words to leave out of results ("music", "voices").
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Search Epidemic Sound right away.
    #[serde(default)]
    pub search: bool,
    /// Results of `search_epidemic` to start the bucket with (dropped in by hand).
    #[serde(default)]
    pub previews: Vec<Preview>,
    /// A music bucket (in a show its folder goes under `music/`).
    #[serde(default)]
    pub music: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateBucketP {
    pub bucket_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    #[serde(default)]
    pub scenes: Option<Vec<String>>,
    #[serde(default)]
    pub length_min_s: Option<Option<f64>>,
    #[serde(default)]
    pub length_max_s: Option<Option<f64>>,
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketP {
    pub bucket_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketSearchP {
    pub bucket_id: String,
    /// "epidemic" (default) or "library".
    #[serde(default)]
    pub provider: Option<String>,
    /// Next page ("Load more").
    #[serde(default)]
    pub more: bool,
    /// New keywords (replace the bucket's).
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketRemoteP {
    pub bucket_id: String,
    pub remote_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketAssetP {
    pub bucket_id: String,
    pub asset_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketGenerateP {
    pub bucket_id: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub variations: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PlaceFromBucketP {
    pub scene: String,
    /// A kept sound …
    #[serde(default)]
    pub asset_id: Option<String>,
    /// … or a preview (downloads first, in the background).
    #[serde(default)]
    pub remote_id: Option<String>,
    #[serde(default)]
    pub bucket_id: Option<String>,
    /// The cue it answers.
    #[serde(default)]
    pub cue_id: Option<String>,
    #[serde(default)]
    pub lane: Option<Lane>,
    /// For `lane: dialogue` (vocals default to their character's lane).
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub at_s: Option<f64>,
    #[serde(default)]
    pub anchor_line_id: Option<String>,
    #[serde(default)]
    pub anchor_word: Option<usize>,
    /// Which edge of the line or word: `start` (default) or `end`.
    #[serde(default)]
    pub anchor_edge: Edge,
    /// Seconds after the anchor or time.
    #[serde(default)]
    pub offset_s: f64,
    /// Where it starts in the room: 0 close … 1 far.
    #[serde(default)]
    pub depth: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct BucketLoopP {
    pub bucket_id: String,
    /// A kept sound …
    #[serde(default)]
    pub asset_id: Option<String>,
    /// … or a preview.
    #[serde(default)]
    pub remote_id: Option<String>,
    pub on: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct AmbienceP {
    /// Puts what this scene's cues asked for first.
    #[serde(default)]
    pub scene: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImportAmbienceP {
    pub path: String,
    /// The room it goes straight into; left out it's only offered.
    #[serde(default)]
    pub scene: Option<String>,
    /// As in `set_scene_tone`.
    #[serde(default)]
    pub night: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SuggestBucketP {
    /// A file name or path.
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CharacterWetP {
    pub character: String,
    /// 0 dry … 1.
    pub wet: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct SceneToneP {
    pub scene: String,
    /// A sound of the episode or the show's library; left out, the ambience goes.
    #[serde(default)]
    pub asset_id: Option<String>,
    /// Level in dB; left out it stays, or starts at −28.
    #[serde(default)]
    pub gain_db: Option<f64>,
    /// Only the place's night scenes (true) or the others (false). Left out, all of them.
    #[serde(default)]
    pub night: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PlaySoundP {
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub remote_id: Option<String>,
    #[serde(default)]
    pub from_s: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct PeaksP {
    pub source_id: String,
    pub t0: f64,
    pub t1: f64,
    #[serde(default)]
    pub buckets: Option<usize>,
    /// Scale to the file's loudest peak (as the take waveforms are drawn).
    #[serde(default)]
    pub normalize: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct PeaksBatchP {
    pub ranges: Vec<PeaksP>,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct AnalysisScopeP {
    /// Only these characters' sessions (their session mics and director mics).
    #[serde(default)]
    pub characters: Vec<String>,
    /// Only these lines' takes.
    #[serde(default)]
    pub line_ids: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SourceIdP {
    pub source_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct MarkP {
    /// Updates this mark; a new one is made when absent.
    #[serde(default)]
    pub mark_id: Option<String>,
    /// A character's session mic (the director mic is never a take).
    pub source_id: String,
    /// Source-file seconds.
    pub t0: f64,
    pub t1: f64,
    pub kind: MarkKind,
    /// The script line it belongs to.
    #[serde(default)]
    pub line_id: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct MarkIdP {
    pub mark_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SendMarksP {
    pub mark_ids: Vec<String>,
    /// Link as marked, without asking the AI.
    #[serde(default)]
    pub manual: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct DevEvalP {
    /// A character's session mic.
    pub source_id: String,
    /// Scenes to measure on (reviewed ones count; what is not marked there is right).
    pub scene_ids: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PlaySourceP {
    pub source_id: String,
    /// Source-file seconds.
    pub t0: f64,
    pub t1: f64,
}

macro_rules! registry {
    ($( $name:literal, $P:ty, $access:ident, $mcp:literal, $desc:literal; )*) => {
        /// Every method with its parameter schema.
        pub fn methods() -> Vec<MethodInfo> {
            vec![$( MethodInfo { name: $name, description: $desc, access: Access::$access, mcp: $mcp, schema: schema::<$P> }, )*]
        }
    };
}

registry! {
    "create_show", CreateShowP, Write, true, "Make a folder a show (studio mode): writes <root>/<name>.cutawayshow next to the production's files and adds Episodes/ and Sound/{sfx,music}/ when missing. Nothing that exists is touched. Opens the show.";
    "open_show", ShowPathP, Read, true, "Open a show (its folder or .cutawayshow file): its episodes, library and theme. Then open_episode.";
    "show_view", NoParams, Read, true, "The open show: name, folders, episodes (number, title, has a project) and theme.";
    "show_backdrop", NoParams, Read, false, "The show's backdrop image as a data URL, with its theme colours.";
    "set_show_backdrop", PathP, Write, false, "Set the show's backdrop (once for the show): stored as Sound/Cutaway/backdrop.jpg; the theme colours come from it.";
    "open_episode", OpenEpisodeP, Write, true, "Open an episode of the open show by its folder name; makes its Cutaway project in Episodes/<episode>/Sound/ when it has none.";
    "close_show", NoParams, Read, true, "Close the show (and its open episode).";
    "rescan_library", NoParams, Write, true, "Read the show's sound library again (new files from the pipeline); new folders become buckets.";
    "migrate_episode", MigrateP, Write, true, "Copy a standalone episode into an episode folder of the open show (the source stays as it is). Its sounds go into the show's library, vocals stay with the episode. dry_run reports first.";
    "create_episode", CreateEpisodeP, Write, true, "Create an episode (inside a show, or standalone) and open it.";
    "open_project", OpenP, Read, true, "Open an episode file or folder. Later calls work on it.";
    "close_project", NoParams, Read, true, "Close the open episode (back to the start screen). Unsaved changes are snapshotted into Backups/ first. Refused while an analysis runs.";
    "create_demo", DemoP, Write, false, "Create the synthetic demo session in parent/name, analyze it and open it.";
    "project_status", NoParams, Read, true, "Progress, match rate, flagged items, undo state and AI provider of the open episode.";
    "list_scenes", NoParams, Read, true, "Scenes with heading, place, line counts and flagged items.";
    "import_script", ImportScriptP, Write, true, "Import a script as the next version (vNNN): Fountain, or a Throughline .thrl project with its story context (scene titles and summaries, acts, cast, settings, character bios). Returns the change summary.";
    "import_story", PathP, Write, true, "Take only the story context (character bios, setting descriptions and groups) from a Throughline .thrl file, e.g. the pilot's bios into a later episode. The script stays as it is; non-empty fields replace the stored ones.";
    "import_audio", ImportAudioP, Write, true, "Import role and room recordings (files or folders) and assign roles automatically.";
    "roles", NoParams, Read, true, "Every source file with its role, character and confidence.";
    "confirm_roles", ConfirmRolesP, Write, true, "Set or correct file roles. Corrections are remembered as show aliases.";
    "relink_media", RelinkP, Write, true, "Find missing source files again by hash under a folder.";
    "run_transcription", NoParams, Write, true, "Transcribe every source (transcript sidecars, or local Parakeet). Cached.";
    "run_alignment", NoParams, Write, true, "Run the full analysis: sync, alignment, takes, room notes, picks. One undoable step.";
    "run_notes", NoParams, Write, true, "Re-run room classification, note condensing and pick detection (re-runs the cached pipeline).";
    "job_status", NoParams, Read, true, "Background jobs with progress.";
    "start_analysis", AnalysisScopeP, Write, false, "Run the analysis in the background (the UI polls job_status). With characters or line_ids, only those (dev).";
    "analyze_scope", AnalysisScopeP, Write, true, "Re-run the analysis for some characters' sessions or some lines only; the rest of the episode stays as it is. One undoable step.";
    "dev_sessions", NoParams, Read, true, "Dev panel: each character's session mic with its takes by origin (matcher, rescue, manual) and marks.";
    "dev_session", SourceIdP, Read, true, "Dev panel: one actor session as the AI reads it: every sentence of the session mic and the director mic by time, what each became, the script and the marks.";
    "put_mark", MarkP, Write, true, "Mark a stretch of a character's session mic (missed take, wrong line, not a take, edges) with a note and optionally the script line it belongs to.";
    "delete_mark", MarkIdP, Write, true, "Remove a mark.";
    "send_marks", SendMarksP, Write, true, "Judge marked stretches again, by the AI with the marks as hints (or as linked, with manual), and store a proposal on each. Runs in the background; returns a job id.";
    "apply_mark", MarkIdP, Write, true, "Apply a mark's proposal (adds and removes takes). One undoable step.";
    "discard_mark", MarkIdP, Write, true, "Drop a mark's proposal; the mark stays.";
    "play_source", PlaySourceP, Read, false, "Play a stretch of a source file (dev panel).";
    "dev_learn", SourceIdP, Read, true, "Dev panel learning: scenes to review, worked-example candidates from the marks, misheard-word (glossary) candidates, current matching settings.";
    "dev_eval", DevEvalP, Write, true, "Dev panel learning: try the matching settings variants on these scenes of one session (nothing is applied) and score them against the marks. Runs in the background; returns a job id.";
    "match_report", NoParams, Read, true, "The last match report (Markdown).";
    "list_lines", SceneP, Read, true, "Lines of a scene in timeline order with their selected take and take count.";
    "list_takes", LineP, Read, true, "Takes of a line with type, notes, pick and timing.";
    "select_take", SelectTakeP, Write, true, "Select a take for a line (by take_id or take_no).";
    "step_take", SelectTakeP, Write, false, "Step to the previous/next take (take_no = delta).";
    "apply_take_to_passage", PassageP, Write, true, "Use the same take number for several lines (passage mode).";
    "toggle_favorite", LineP, Write, true, "Toggle the favorite star of the line's selected take.";
    "set_gap", GapP, Write, true, "Set the pause before a line (seconds; negative overlaps; null resets).";
    "trim_line", TrimP, Write, true, "Trim the head and tail of a line (seconds; negative extends into the handle).";
    "command", NoParams, Write, false, "Execute any document command (see the Command schema). Used by the UI.";
    "undo", NoParams, Write, true, "Undo the last change (from any client).";
    "redo", NoParams, Write, true, "Redo the last undone change.";
    "history", HistoryP, Read, true, "Recent commands from every client.";
    "save_version", VersionP, Write, true, "Save a named version (snapshot) of the episode.";
    "list_versions", NoParams, Read, true, "Backups and named versions.";
    "revert", RevertP, Destructive, true, "Revert the episode to a backup or version (a pre-revert snapshot is taken first).";
    "set_config", ConfigP, Write, true, "Set a project setting by dotted key (ai.model, pacing.breath_gap_s, export.handles_s …).";
    "scene_view", SceneP, Read, false, "Everything the Options/Arrange pages show for a scene.";
    "edit_view", SceneP, Read, false, "Sound placements and cues of a scene for the Edit page.";
    "spot_sound_effects", SpotP, Write, true, "AI sound spotting for a scene: adds anchored cues to the bucket and groups them into sound buckets.";
    "list_bucket", SceneP, Read, true, "Sound cues (wish list) of a scene, or of the whole episode.";
    "add_cue", AddCueP, Write, true, "Add a sound cue by hand.";
    "update_cue", UpdateCueP, Write, true, "Change fields of a cue (description, tags, priority, accepted …).";
    "delete_cue", DeleteCueP, Destructive, true, "Delete a cue (its placements stay, unlinked).";
    "merge_cues", MergeCuesP, Write, true, "Merge duplicate cues into one (optionally a show-level cue).";
    "attach_sound", AttachP, Write, true, "Attach a library sound to a cue as a candidate (or choose it).";
    "search_sounds", SearchSoundsP, Read, true, "Search the library (and connected services) for a query or a cue.";
    "generate_sound", GenerateP, Write, true, "Generate sound variations for a cue or prompt (ElevenLabs).";
    "import_sounds", ImportSoundsP, Write, true, "Import files into the library (sorted and tagged), optionally onto a cue or the timeline.";
    "connections", NoParams, Read, true, "Which sound services are connected (keys are never shown).";
    "connect_service", ConnectP, Write, false, "Store an API key for a sound service in the OS keychain (checked first). Not available over MCP.";
    "disconnect_service", ServiceP, Write, false, "Remove a sound service's API key from the keychain.";
    "fetch_sound", FetchP, Write, true, "Download a remote search result (Epidemic Sound) into the library as licensed, and attach it to a cue.";
    "list_library", LibraryP, Read, true, "Library sounds, optionally filtered by a query.";
    "delete_asset", DeleteAssetP, Destructive, true, "Remove a sound from the library (the file moves to Backups/Trash).";
    "place_sound", PlaceSoundP, Write, true, "Place a sound on the timeline, anchored to a line/word or at an absolute time.";
    "export_fcpxml", ExportP, Write, true, "Export FCPXML 1.9 for DaVinci Resolve and Logic (one lane per role).";
    "export_srt", ExportP, Write, true, "Export SRT subtitles from the final word timings.";
    "export_cut_list", ExportP, Write, true, "Export the as-cut transcript / cut list (format csv or md).";
    "export_mix", ExportMixP, Write, true, "Bounce a scene to WAV or MP3.";
    "play", PlayP, Read, false, "Play the scene from a time (audio device, or a virtual clock without one).";
    "play_take", PlayTakeP, Read, false, "Audition one take alone (loudness-matched) from a time within it.";
    "stop", NoParams, Read, false, "Stop playback, the preview player included.";
    "playhead", NoParams, Read, false, "Playhead position and state.";
    "peaks", PeaksP, Read, false, "Waveform peaks of a source range.";
    "peaks_batch", PeaksBatchP, Read, false, "Waveform peaks of several source ranges in one call (the Edit page's visible clips).";
    "import_view", NoParams, Read, false, "Everything the Import › Files page shows: script, cast, room links and what needs fixing.";
    "match_files", NoParams, Write, true, "Redo match: group files into sessions (timecode, else folder) and link director rooms. Hand-set links are kept.";
    "link_room", LinkRoomP, Write, true, "Link a director room file to the recordings it serves (kept on Redo match).";
    "suggest_character", SourceP, Read, true, "Suggest the character of a recording (its name, then AI name matching).";
    "buckets", BucketsP, Read, true, "Sound buckets (all, or a scene's) with keywords, kept sounds and previews.";
    "spot_buckets", SpotBucketsP, Write, true, "Spot script: AI sound spotting (script only, one AI call per scene) grouped into buckets with keywords. A scene, the new and changed scenes, or all with `all`.";
    "start_spotting", SpotBucketsP, Write, false, "Spot script in the background (the UI polls job_status).";
    "plan_cleanup", NoParams, Read, true, "Clean up: AI reads the names of the loose sound effects and proposes a folder for each. Nothing moves; pass the moves you accept to apply_cleanup.";
    "start_cleanup", NoParams, Read, false, "plan_cleanup in the background (the UI polls job_status).";
    "apply_cleanup", ApplyCleanupP, Write, true, "Move loose sound effects into folders of sfx/ in the shared pipeline, and repoint this episode's sounds. Never overwrites. The moves are logged in Sound/Cutaway/moves/.";
    "search_epidemic", SearchEpidemicP, Read, true, "Search Epidemic Sound freely (10 per page). Nothing is stored or downloaded; add a result to a bucket with bucket_add_preview.";
    "bucket_add_preview", BucketAddPreviewP, Write, true, "Add an Epidemic Sound result to a bucket by hand, as a preview a fresh search keeps (not downloaded).";
    "create_bucket", CreateBucketP, Write, true, "Create a sound bucket (name, keywords, scenes, length and leave-out filters).";
    "update_bucket", UpdateBucketP, Write, true, "Change a bucket's name, keywords, scenes or filters.";
    "delete_bucket", BucketP, Destructive, true, "Delete a bucket (its kept sounds stay in the library).";
    "bucket_search", BucketSearchP, Write, true, "Search for a bucket's keywords. Epidemic Sound results become previews (not downloaded).";
    "bucket_download", BucketRemoteP, Write, true, "Download a preview (licensed) and keep it in the bucket. Counts against the plan.";
    "bucket_keep", BucketAssetP, Write, true, "Keep a library sound in a bucket.";
    "bucket_remove", BucketAssetP, Write, true, "Take a kept sound out of a bucket (it stays in the library). A removed vocal stays out on the next analysis.";
    "bucket_dismiss", BucketRemoteP, Write, true, "Dismiss a preview so searches skip it.";
    "bucket_generate", BucketGenerateP, Write, true, "Generate sounds for a bucket (ElevenLabs) and keep them.";
    "place_from_bucket", PlaceFromBucketP, Write, true, "Place a bucket sound on the timeline. A preview downloads first (returns a job_id).";
    "play_sound", PlaySoundP, Read, false, "Audition a bucket sound (kept or preview) on the preview player. The timeline stops; its playhead stays.";
    "stop_sound", NoParams, Read, false, "Stop the preview player.";
    "sound_playhead", NoParams, Read, false, "The preview player's sound, position and state.";
    "list_sounds", SceneP, Read, true, "The sounds placed in a scene with their ids, timing, anchor, gain, pan, depth and fades.";
    "move_sound", MoveSoundP, Write, true, "Move a placed sound: onto an anchor (a word, a line edge, or the pause after a word: anchor_word with anchor_edge end), to free time (at_s), or by offset_s from where it hangs. Anchored sounds follow their line.";
    "trim_sound", TrimSoundP, Write, true, "Trim a placed sound's start or end. Never longer than the source; the automatic fade follows.";
    "set_sound_fades", SoundFadesP, Write, true, "Set a sound's fades. A fade out set by hand turns auto off; auto true hands it back to the length.";
    "set_sound_gain", SoundGainP, Write, true, "Set a sound's loudness, -24 to +6 dB.";
    "set_sound_pan", SoundPanP, Write, true, "Set a sound's pan, -1 left to +1 right.";
    "set_sound_depth", SoundDepthP, Write, true, "Set how far back a sound sits in its scene's room, 0 close to 1 far: wetter, quieter and darker as it goes back.";
    "slip_sound", SlipSoundP, Write, true, "Slip the audio inside a sound's window: a new source in point, same place and length.";
    "remove_sound", PlacementP, Write, true, "Take a placed sound off the timeline. Undoable.";
    "batch_sounds", BatchSoundsP, Write, true, "Several sound edits as one undo step (a multi-select move, gain or depth change); if one fails, none applies. Each edit is {op: move|trim|fades|gain|pan|depth|slip|remove, ...the fields of that method}.";
    "scene_rooms", NoParams, Read, true, "Each scene's room: size, damping, wet, what they come to (decay, pre-delay) and how many sounds play in it.";
    "ambience_sounds", AmbienceP, Read, true, "What can loop under a room: the ambience of the library, what a scene's cues asked for first.";
    "import_ambience", ImportAmbienceP, Write, true, "Bring a file in as an ambience: into the room tone folder, and looping under a scene's location if one is given.";
    "suggest_bucket", SuggestBucketP, Read, true, "The bucket a file name points to, or a name for a new one (bucket_id null). The same name always gives the same answer.";
    "voice_rooms", NoParams, Read, true, "Each character and how much of the scene's room their voice takes.";
    "set_character_wet", CharacterWetP, Write, true, "Set how much of the scene's room a character's voice takes, 0 dry to 1.";
    "set_scene_tone", SceneToneP, Write, true, "Set the ambience looping under a scene's location, from a library sound, and its level: every scene of the place shares it, or give night to set the place's night scenes or the others apart.";
    "bucket_set_loop", BucketLoopP, Write, true, "Set whether a bucket sound loops. Dropped, a sound over 6 s with Loop on goes on the ambience lane and repeats to the end of the scene.";
    "set_scene_room", SceneRoomP, Write, true, "Set a scene's room: size, damping, wet (0 to 1 each; left out stays). Each scene has its own room, shared by all its sounds.";
}

pub fn method(name: &str) -> Option<MethodInfo> {
    methods().into_iter().find(|m| m.name == name)
}

fn parse<T: DeserializeOwned>(params: &Value) -> Result<T> {
    let p = if params.is_null() {
        json!({})
    } else {
        params.clone()
    };
    serde_json::from_value(p).map_err(|e| AppError::Params(e.to_string()))
}

fn need_confirm(name: &str, confirm: bool) -> Result<()> {
    if confirm {
        Ok(())
    } else {
        Err(AppError::NeedsConfirm(name.into()))
    }
}

fn scene_id(svc: &Service, key: Option<&str>) -> Result<String> {
    let doc = svc.doc();
    match key {
        Some(k) => doc
            .find_scene(k)
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Params(format!("no scene '{k}'"))),
        None => doc
            .scenes_in_order()
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Params("no scenes".into())),
    }
}

/// Writes an export, refusing to overwrite without confirmation.
fn write_export(path: &Path, contents: &str, confirm: bool, name: &str) -> Result<Value> {
    if path.exists() && !confirm {
        return Err(AppError::NeedsConfirm(format!(
            "{name} (overwrite {})",
            path.display()
        )));
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, contents)?;
    Ok(json!({ "path": path.to_string_lossy(), "bytes": contents.len() }))
}

fn export_target(svc: &Service, p: &ExportP, ext: &str) -> (ExportScope, PathBuf, String) {
    let scope = match &p.scene {
        Some(s) => ExportScope::Scenes(vec![s.clone()]),
        None => ExportScope::All,
    };
    let title = match &p.scene {
        Some(s) => format!(
            "{} S{}",
            svc.name(),
            svc.doc().find_scene(s).map(|x| x.number).unwrap_or(0)
        ),
        None => svc.name(),
    };
    let path = p
        .path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| svc.paths.exports.join(format!("{title}.{ext}")));
    (scope, path, title)
}

impl Service {
    /// Peaks of a source range: |max| per bucket, at most the cached 100 per second.
    fn peaks_of(&mut self, p: &PeaksP) -> Result<Vec<f32>> {
        let a = self
            .analysis_of(&p.source_id)
            .ok_or_else(|| AppError::Params("unknown source".into()))?;
        let fine = ((p.t1 - p.t0).max(0.0) * apc_media::analysis::PEAK_FPS as f64).ceil() as usize;
        let buckets = p.buckets.unwrap_or(200).clamp(1, fine.max(1));
        let g = if p.normalize {
            a.peaks
                .iter()
                .map(|(lo, hi)| lo.abs().max(hi.abs()))
                .fold(1e-4f32, f32::max)
        } else {
            1.0
        };
        Ok(a.peaks_range(p.t0, p.t1, buckets)
            .into_iter()
            .map(|(lo, hi)| (lo.abs().max(hi.abs()) / g).min(1.0))
            .collect())
    }

    /// Dispatches a method by name.
    pub fn call(&mut self, name: &str, params: &Value) -> Result<Value> {
        let info = method(name).ok_or_else(|| AppError::UnknownMethod(name.into()))?;
        if info.access != Access::Read {
            self.bus.refresh()?;
        }
        match name {
            "project_status" => self.status(),
            "list_scenes" => Ok(serde_json::to_value(crate::views::scene_summaries(self.doc()))?),
            "import_script" => {
                let p: ImportScriptP = parse(params)?;
                self.import_script(Path::new(&p.path), p.name.as_deref())
            }
            "import_story" => {
                let p: PathP = parse(params)?;
                self.import_story(Path::new(&p.path))
            }
            "import_audio" => {
                let p: ImportAudioP = parse(params)?;
                let paths: Vec<PathBuf> = p.paths.iter().map(PathBuf::from).collect();
                Ok(serde_json::to_value(self.import_audio(&paths, p.copy)?)?)
            }
            "roles" => Ok(json!(self
                .doc()
                .sources
                .values()
                .map(|s| json!({
                    "source_id": s.id, "file": s.original_name, "role": s.role, "character": s.character,
                    "confidence": s.role_confidence, "offset_s": s.offset_s, "missing": s.missing,
                    "stored": s.stored, "path": s.path,
                }))
                .collect::<Vec<_>>())),
            "confirm_roles" => {
                let p: ConfirmRolesP = parse(params)?;
                let items: Vec<(String, Role, Option<String>)> =
                    p.assignments.into_iter().map(|a| (a.source_id, a.role, a.character)).collect();
                self.confirm_roles(&items, p.remember)
            }
            "relink_media" => {
                let p: RelinkP = parse(params)?;
                let plan = apc_core::media::relink_plan(self.doc(), &self.paths, Path::new(&p.search_dir))?;
                for (sid, path) in &plan {
                    self.exec(&Command::RelinkMedia { source_id: sid.clone(), path: path.clone() })?;
                }
                Ok(json!({ "relinked": plan.len(), "still_missing": apc_core::media::missing_sources(self.doc(), &self.paths) }))
            }
            "run_transcription" => {
                let asr = self.asr_provider();
                let mut done = 0;
                let sources: Vec<SourceFile> = self.doc().sources.values().filter(|s| s.role != Role::Unknown).cloned().collect();
                for s in sources {
                    let req = apc_analysis::asr::AsrRequest {
                        audio: self.paths.resolve(&s.path),
                        sidecar: s.transcript.as_ref().map(|t| self.paths.resolve(t)),
                        hotwords: vec![],
                        language: "en".into(),
                    };
                    let t = asr.transcribe(&req).map_err(apc_analysis::AnalysisError::from)?;
                    let f = self.paths.cache.join("asr").join(format!("{}-{}.json", s.hash, asr.id()));
                    std::fs::create_dir_all(self.paths.cache.join("asr"))?;
                    std::fs::write(f, serde_json::to_string(&t)?)?;
                    done += 1;
                }
                Ok(json!({ "transcribed": done, "provider": asr.id() }))
            }
            "run_alignment" | "run_notes" => {
                let r = self.analyze(&|_| {})?;
                Ok(json!({
                    "takes_found": r.takes_found, "lines_matched": r.lines_matched, "picks_found": r.picks_found,
                    "cue_readings": r.cue_readings, "note_segments": r.note_segments, "offsets": r.offsets,
                    "flagged": r.flagged,
                }))
            }
            "job_status" => Ok(serde_json::to_value(self.jobs.list())?),
            "start_analysis" => {
                let p: AnalysisScopeP = parse(params)?;
                let scope = (!p.characters.is_empty() || !p.line_ids.is_empty()).then(|| {
                    self.configured(apc_analysis::AnalysisScope {
                        characters: p.characters,
                        line_ids: p.line_ids,
                        ..apc_analysis::AnalysisScope::everything()
                    })
                });
                Ok(json!({ "job_id": self.jobs.start_analysis(self.paths.file.clone(), scope) }))
            }
            "analyze_scope" => {
                let p: AnalysisScopeP = parse(params)?;
                let scope = self.configured(apc_analysis::AnalysisScope {
                    characters: p.characters,
                    line_ids: p.line_ids,
                    ..apc_analysis::AnalysisScope::everything()
                });
                let r = self.analyze_with(scope, &|_| {})?;
                Ok(json!({
                    "takes_found": r.takes_found, "lines_matched": r.lines_matched,
                    "rescued": r.roles.iter().map(|x| x.rescued).sum::<usize>(), "flagged": r.flagged,
                }))
            }
            "dev_sessions" => Ok(crate::dev::sessions(self)),
            "dev_session" => {
                let p: SourceIdP = parse(params)?;
                crate::dev::session_view(self, &p.source_id)
            }
            "put_mark" => {
                let p: MarkP = parse(params)?;
                let src = self
                    .doc()
                    .sources
                    .get(&p.source_id)
                    .cloned()
                    .ok_or_else(|| AppError::Params(format!("unknown source '{}'", p.source_id)))?;
                if src.role != Role::Character {
                    return Err(AppError::Params("marks go on a character's session mic; the director mic is notes only".into()));
                }
                let (t0, t1) = (p.t0.min(p.t1), p.t0.max(p.t1));
                let old = p.mark_id.as_ref().and_then(|id| self.doc().marks.get(id).cloned());
                let mark = DevMark {
                    id: p.mark_id.clone().unwrap_or_else(|| new_id("mk")),
                    source_id: p.source_id,
                    t0,
                    t1,
                    kind: p.kind,
                    line_id: p.line_id,
                    note: p.note,
                    text: crate::dev::heard_between(self, &src, t0, t1),
                    status: MarkStatus::Open,
                    proposal: None,
                    created_at: old.map(|m| m.created_at).unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                };
                Ok(self.exec(&Command::PutMark { mark })?.result)
            }
            "delete_mark" => {
                let p: MarkIdP = parse(params)?;
                Ok(self.exec(&Command::DeleteMark { mark_id: p.mark_id })?.result)
            }
            "send_marks" => {
                let p: SendMarksP = parse(params)?;
                let marks: Vec<DevMark> = p
                    .mark_ids
                    .iter()
                    .map(|id| self.doc().marks.get(id).cloned().ok_or_else(|| AppError::Params(format!("no mark '{id}'"))))
                    .collect::<Result<_>>()?;
                let scope = self.configured(apc_analysis::AnalysisScope {
                    marks,
                    manual: p.manual,
                    ..apc_analysis::AnalysisScope::everything()
                });
                Ok(json!({ "job_id": self.jobs.start_analysis(self.paths.file.clone(), Some(scope)) }))
            }
            "apply_mark" => {
                let p: MarkIdP = parse(params)?;
                Ok(self.exec(&Command::ApplyMark { mark_id: p.mark_id })?.result)
            }
            "discard_mark" => {
                let p: MarkIdP = parse(params)?;
                let mut m = self.doc().marks.get(&p.mark_id).cloned().ok_or_else(|| AppError::Params(format!("no mark '{}'", p.mark_id)))?;
                m.proposal = None;
                m.status = MarkStatus::Discarded;
                Ok(self.exec(&Command::PutMark { mark: m })?.result)
            }
            "dev_learn" => {
                let p: SourceIdP = parse(params)?;
                crate::dev::learn_view(self, &p.source_id)
            }
            "dev_eval" => {
                let p: DevEvalP = parse(params)?;
                let file = self.paths.file.clone();
                Ok(json!({ "job_id": self.jobs.spawn("eval", file, move |svc, progress| {
                    crate::dev::eval(svc, &p.source_id, &p.scene_ids, progress)
                }) }))
            }
            "play_source" => {
                let p: PlaySourceP = parse(params)?;
                crate::player::play_source(self, &p.source_id, p.t0, p.t1)
            }
            "match_report" => {
                let md = self
                    .last_report
                    .as_ref()
                    .map(|r| r.markdown.clone())
                    .or_else(|| std::fs::read_to_string(self.paths.logs.join("match-report.md")).ok())
                    .unwrap_or_default();
                Ok(json!({ "markdown": md }))
            }
            "list_lines" => {
                let p: SceneP = parse(params)?;
                let sid = scene_id(self, p.scene.as_deref())?;
                let doc = self.doc();
                let sc = &doc.scenes[&sid];
                let lines: Vec<Value> = sc
                    .order
                    .iter()
                    .filter_map(|id| doc.lines.get(id))
                    .filter(|l| l.kind == LineKind::Dialogue)
                    .map(|l| {
                        let takes = doc.takes_of_line(&l.id);
                        let sel = doc.boxes.get(&l.id).and_then(|b| b.take_id.clone());
                        json!({
                            "line_id": l.id, "ref": format!("S{}.{}", sc.number, l.number), "character": l.character,
                            "text": l.text, "takes": takes.len(),
                            "selected_take": sel.as_ref().and_then(|s| doc.takes.get(s)).map(|t| t.take_no),
                            "adlib": l.adlib,
                        })
                    })
                    .collect();
                Ok(json!(lines))
            }
            "list_takes" => {
                let p: LineP = parse(params)?;
                let doc = self.doc();
                let sel = doc.boxes.get(&p.line_id).and_then(|b| b.take_id.clone());
                Ok(json!(doc
                    .takes_of_line(&p.line_id)
                    .iter()
                    .map(|t| json!({
                        "take_id": t.id, "take_no": t.take_no, "type": t.take_type, "partial": t.partial,
                        "t0": t.t0, "t1": t.t1, "score": t.score, "heard": t.heard, "pick": t.pick,
                        "favorite": t.favorite, "direction": t.direction, "feedback": t.feedback,
                        "selected": sel.as_deref() == Some(t.id.as_str()),
                    }))
                    .collect::<Vec<_>>()))
            }
            "select_take" => {
                let p: SelectTakeP = parse(params)?;
                let take_id = match (p.take_id, p.take_no) {
                    (Some(t), _) => t,
                    (None, Some(n)) => self
                        .doc()
                        .takes_of_line(&p.line_id)
                        .iter()
                        .find(|t| t.take_no == n)
                        .map(|t| t.id.clone())
                        .ok_or_else(|| AppError::Params(format!("line has no take {n}")))?,
                    _ => return Err(AppError::Params("give take_id or take_no".into())),
                };
                Ok(self.exec(&Command::SelectTake { line_id: p.line_id, take_id })?.result)
            }
            "step_take" => {
                let p: SelectTakeP = parse(params)?;
                Ok(self.exec(&Command::StepTake { line_id: p.line_id, delta: p.take_no.unwrap_or(1) as i32 })?.result)
            }
            "apply_take_to_passage" => {
                let p: PassageP = parse(params)?;
                Ok(self.exec(&Command::ApplyTakeToPassage { line_ids: p.line_ids, take_no: p.take_no })?.result)
            }
            "toggle_favorite" => {
                let p: LineP = parse(params)?;
                let tid = self
                    .doc()
                    .boxes
                    .get(&p.line_id)
                    .and_then(|b| b.take_id.clone())
                    .ok_or_else(|| AppError::Params("the line has no selected take".into()))?;
                Ok(self.exec(&Command::ToggleFavorite { take_id: tid })?.result)
            }
            "set_gap" => {
                let p: GapP = parse(params)?;
                Ok(self.exec(&Command::SetGap { line_id: p.line_id, gap_s: p.gap_s })?.result)
            }
            "trim_line" => {
                let p: TrimP = parse(params)?;
                Ok(self.exec(&Command::SetTrim { line_id: p.line_id, head_s: p.head_s, tail_s: p.tail_s })?.result)
            }
            "command" => {
                let cmd: Command = parse(params)?;
                let r = self.exec(&cmd)?;
                Ok(json!({ "seq": r.seq, "result": r.result, "rev": self.bus.rev() }))
            }
            "undo" | "redo" => {
                let r = if name == "undo" { self.bus.undo()? } else { self.bus.redo()? };
                if r.is_some() {
                    self.after_write()?;
                }
                Ok(json!({ "done": r.as_ref().map(|s| s.name.clone()), "rev": self.bus.rev() }))
            }
            "history" => {
                let p: HistoryP = parse(params)?;
                Ok(serde_json::to_value(self.bus.history(p.limit.unwrap_or(50))?)?)
            }
            "save_version" => {
                let p: VersionP = parse(params)?;
                Ok(serde_json::to_value(self.backups.save_version(self.bus.store(), &p.name)?)?)
            }
            "list_versions" => Ok(serde_json::to_value(BackupManager::list(self.bus.store())?)?),
            "revert" => {
                let p: RevertP = parse(params)?;
                need_confirm("revert", p.confirm)?;
                let pre = self.backups.revert(&mut self.bus, &p.backup_id)?;
                Ok(json!({ "reverted_to": p.backup_id, "pre_revert_backup": pre.id }))
            }
            "set_config" => {
                let p: ConfigP = parse(params)?;
                Ok(self.exec(&Command::SetConfig { key: p.key, value: p.value })?.result)
            }
            "scene_view" => {
                let p: SceneP = parse(params)?;
                let key = p.scene.unwrap_or_default();
                Ok(serde_json::to_value(crate::views::scene_view(self, &key)?)?)
            }
            "edit_view" => {
                let p: SceneP = parse(params)?;
                let sid = scene_id(self, p.scene.as_deref())?;
                Ok(serde_json::to_value(crate::views::edit_view(self, &sid)?)?)
            }
            "spot_sound_effects" => {
                // one scene, grouped into buckets like Spot script (so it counts as spotted)
                let p: SpotP = parse(params)?;
                crate::buckets::spot(self, Some(&p.scene), false, &|_, _| {})
            }
            "list_bucket" => {
                let p: SceneP = parse(params)?;
                let sid = match p.scene {
                    Some(s) => Some(scene_id(self, Some(&s))?),
                    None => None,
                };
                Ok(serde_json::to_value(crate::views::bucket(self.doc(), sid.as_deref()))?)
            }
            "add_cue" => {
                let p: AddCueP = parse(params)?;
                let sid = scene_id(self, Some(&p.scene))?;
                let cue = SoundCue {
                    id: new_id("cue"),
                    scene_id: sid,
                    description: p.description,
                    anchor: p.anchor_line_id.map(|l| Anchor { line_id: l, word: p.anchor_word, edge: Edge::Start }),
                    cue_type: p.cue_type,
                    tags: p.tags,
                    priority: p.priority.unwrap_or(2),
                    status: CueStatus::Wanted,
                    candidates: vec![],
                    chosen: None,
                    show_cue_id: None,
                    ai_suggested: false,
                    accepted: true,
                };
                Ok(self.exec(&Command::AddCue { cue })?.result)
            }
            "update_cue" => {
                let p: UpdateCueP = parse(params)?;
                Ok(self.exec(&Command::UpdateCue { cue_id: p.cue_id, patch: p.patch })?.result)
            }
            "delete_cue" => {
                let p: DeleteCueP = parse(params)?;
                need_confirm("delete_cue", p.confirm)?;
                Ok(self.exec(&Command::DeleteCue { cue_id: p.cue_id })?.result)
            }
            "merge_cues" => {
                let p: MergeCuesP = parse(params)?;
                crate::sound::merge(self, p.cue_ids, p.into_id, p.show_level)
            }
            "attach_sound" => {
                let p: AttachP = parse(params)?;
                crate::sound::ensure_asset_in_doc(self, &p.asset_id)?;
                let cmd = if p.choose {
                    Command::ChooseCandidate { cue_id: p.cue_id, asset_id: p.asset_id }
                } else {
                    Command::AttachCandidate { cue_id: p.cue_id, asset_id: p.asset_id }
                };
                Ok(self.exec(&cmd)?.result)
            }
            "search_sounds" => {
                let p: SearchSoundsP = parse(params)?;
                crate::sound::search(self, p.query, p.cue_id, p.attach)
            }
            "generate_sound" => {
                let p: GenerateP = parse(params)?;
                crate::sound::generate(self, p.cue_id, p.prompt, p.variations, p.duration_s, None)
            }
            "import_sounds" => {
                let p: ImportSoundsP = parse(params)?;
                crate::sound::import(self, &p.paths, p.cue_id, p.scene, p.at_s, p.bucket_id)
            }
            "connections" => crate::sound::connections(self),
            "connect_service" => {
                let p: ConnectP = parse(params)?;
                crate::sound::connect(self, &p.service, &p.api_key)
            }
            "disconnect_service" => {
                let p: ServiceP = parse(params)?;
                crate::sound::disconnect(self, &p.service)
            }
            "fetch_sound" => {
                let p: FetchP = parse(params)?;
                let title = if p.title.is_empty() { p.remote_id.clone() } else { p.title.clone() };
                crate::sound::fetch(self, &p.remote_id, &title, p.cue_id)
            }
            "list_library" => {
                let p: LibraryP = parse(params)?;
                let lib = self.library();
                let local: Vec<Asset> = self.doc().assets.values().cloned().collect();
                let list = match p.query {
                    Some(q) => lib.search(&q, &local)?,
                    None => lib.list(&local)?,
                };
                Ok(serde_json::to_value(list)?)
            }
            "delete_asset" => {
                let p: DeleteAssetP = parse(params)?;
                need_confirm("delete_asset", p.confirm)?;
                crate::sound::delete_asset(self, &p.asset_id)
            }
            "place_sound" => {
                let p: PlaceSoundP = parse(params)?;
                crate::sound::place(self, p)
            }
            "export_fcpxml" => {
                let p: ExportP = parse(params)?;
                let (scope, path, title) = export_target(self, &p, "fcpxml");
                let tl = EpisodeTimeline::build(self.doc(), &scope);
                let xml = fcpxml::write(self.doc(), &self.paths, &tl, &title);
                let mut r = write_export(&path, &xml, p.confirm, "export_fcpxml")?;
                r["clips"] = json!(tl.clips.len());
                r["duration_s"] = json!(tl.duration_s);
                Ok(r)
            }
            "export_srt" => {
                let p: ExportP = parse(params)?;
                let (scope, path, _) = export_target(self, &p, "srt");
                let tl = EpisodeTimeline::build(self.doc(), &scope);
                let names = p.format.as_deref() != Some("plain");
                write_export(&path, &srt::write(&tl, names), p.confirm, "export_srt")
            }
            "export_cut_list" => {
                let p: ExportP = parse(params)?;
                let md = p.format.as_deref() == Some("md");
                let (scope, path, title) = export_target(self, &p, if md { "md" } else { "csv" });
                let tl = EpisodeTimeline::build(self.doc(), &scope);
                let rows = cutlist::rows(self.doc(), &self.paths, &tl);
                let text = if md { cutlist::markdown(&title, &rows) } else { cutlist::csv(&rows) };
                let mut r = write_export(&path, &text, p.confirm, "export_cut_list")?;
                r["rows"] = json!(rows.len());
                Ok(r)
            }
            "export_mix" => {
                let p: ExportMixP = parse(params)?;
                let fmt = if p.format.as_deref() == Some("mp3") { bounce::Format::Mp3 } else { bounce::Format::Wav };
                let sc = self.doc().find_scene(&p.scene).cloned().ok_or_else(|| AppError::Params(format!("no scene '{}'", p.scene)))?;
                let ext = if fmt == bounce::Format::Mp3 { "mp3" } else { "wav" };
                let target = self.paths.exports.join(format!("S{} {}.{ext}", sc.number, sc.place));
                if target.exists() && !p.confirm {
                    return Err(AppError::NeedsConfirm(format!("export_mix (overwrite {})", target.display())));
                }
                let (file, len) = bounce::bounce_scene(self.doc(), &self.paths, &p.scene, fmt, &self.paths.exports)?;
                Ok(json!({ "path": file.to_string_lossy(), "duration_s": len }))
            }
            "play" => {
                let p: PlayP = parse(params)?;
                let sid = scene_id(self, p.scene.as_deref())?;
                crate::player::play(self, &sid, p.from_s, p.until_s)
            }
            "play_take" => {
                let p: PlayTakeP = parse(params)?;
                crate::player::play_take(self, &p.take_id, p.from_s)
            }
            "stop" => crate::player::stop(self),
            "playhead" => crate::player::playhead(self),
            "peaks" => {
                let p: PeaksP = parse(params)?;
                Ok(json!(self.peaks_of(&p)?))
            }
            "peaks_batch" => {
                let p: PeaksBatchP = parse(params)?;
                // a missing file gives an empty list, so one bad range doesn't blank the rest
                let out: Vec<Vec<f32>> = p.ranges.iter().map(|r| self.peaks_of(r).unwrap_or_default()).collect();
                Ok(json!(out))
            }
            "import_view" => crate::import::view(self),
            "match_files" => crate::import::match_files(self),
            "link_room" => {
                let p: LinkRoomP = parse(params)?;
                crate::import::link_room(self, &p.room_id, &p.recording_ids)
            }
            "suggest_character" => {
                let p: SourceP = parse(params)?;
                crate::import::suggest(self, &p.source_id)
            }
            "buckets" => {
                let p: BucketsP = parse(params)?;
                crate::buckets::view(self, p.scene.as_deref(), p.sort.as_deref().unwrap_or("scene"))
            }
            "spot_buckets" => {
                let p: SpotBucketsP = parse(params)?;
                crate::buckets::spot(self, p.scene.as_deref(), p.all, &|_, _| {})
            }
            "start_spotting" => {
                let p: SpotBucketsP = parse(params)?;
                let file = self.paths.file.clone();
                let job = self.jobs.spawn("spotting", file, move |svc, progress| {
                    crate::buckets::spot(svc, p.scene.as_deref(), p.all, progress)
                });
                Ok(json!({ "job_id": job }))
            }
            "plan_cleanup" => crate::cleanup::plan(self),
            "start_cleanup" => {
                let file = self.paths.file.clone();
                let job = self.jobs.spawn("cleanup", file, move |svc, _| crate::cleanup::plan(svc));
                Ok(json!({ "job_id": job }))
            }
            "apply_cleanup" => {
                let p: ApplyCleanupP = parse(params)?;
                crate::cleanup::apply(self, p.moves)
            }
            "search_epidemic" => {
                let p: SearchEpidemicP = parse(params)?;
                crate::buckets::search_free(self, &p.query, p.offset)
            }
            "bucket_add_preview" => {
                let p: BucketAddPreviewP = parse(params)?;
                crate::buckets::add_preview(self, &p.bucket_id, p.preview)
            }
            "create_bucket" => {
                let p: CreateBucketP = parse(params)?;
                let id = crate::buckets::create(self, &p.name, p.keywords, p.scenes, p.length_min_s, p.length_max_s, p.exclude, p.previews, p.music)?;
                let found = if p.search { crate::buckets::search(self, &id, "epidemic", false, None).ok() } else { None };
                Ok(json!({ "bucket_id": id, "search": found }))
            }
            "update_bucket" => {
                let p: UpdateBucketP = parse(params)?;
                crate::buckets::update(self, p)
            }
            "delete_bucket" => {
                let p: BucketP = parse(params)?;
                let folder = self.doc().buckets.get(&p.bucket_id).and_then(|b| {
                    let f = b.folder.clone()?;
                    Some((f, crate::studio::folder_has_files(self, b)))
                });
                if let (Some((f, true)), Some(show)) = (&folder, &self.paths.show) {
                    return Err(AppError::Params(format!(
                        "this bucket is the folder {} of the show's library; it holds sounds other episodes use. Empty or remove the folder in the Finder to remove the bucket.",
                        show.sound_dir().join(f).display()
                    )));
                }
                let r = self.exec(&Command::DeleteBucket { bucket_id: p.bucket_id })?.result;
                // an empty folder made for the bucket goes with it
                if let (Some((f, false)), Some(show)) = (&folder, &self.paths.show) {
                    let _ = std::fs::remove_dir(show.sound_dir().join(f));
                }
                Ok(r)
            }
            "bucket_search" => {
                let p: BucketSearchP = parse(params)?;
                crate::buckets::search(self, &p.bucket_id, p.provider.as_deref().unwrap_or("epidemic"), p.more, p.keywords)
            }
            "bucket_download" => {
                let p: BucketRemoteP = parse(params)?;
                Ok(json!({ "asset": crate::buckets::download(self, &p.bucket_id, &p.remote_id, &|_| {})? }))
            }
            "bucket_keep" => {
                let p: BucketAssetP = parse(params)?;
                let a = crate::sound::ensure_asset_in_doc(self, &p.asset_id)?;
                if let Some(b) = self.doc().buckets.get(&p.bucket_id) {
                    if a.source != AssetSource::Recorded {
                        crate::buckets::only_sourced(b)?;
                    }
                }
                // in a show the bucket's folder gets a copy, so every episode has it there
                if let Some(dest) = crate::studio::bucket_dest(self, &p.bucket_id)? {
                    let path = self.library().path_of(&a);
                    crate::sound::import_into(self, &path, &dest, &a)?;
                    let id = a.id.clone();
                    crate::buckets::unhide(self, &p.bucket_id, &id)?;
                }
                Ok(self.exec(&Command::BucketKeep { bucket_id: p.bucket_id, asset_id: p.asset_id })?.result)
            }
            "bucket_remove" => {
                let p: BucketAssetP = parse(params)?;
                // a sound of the bucket's folder, or matched to it, is hidden in this episode;
                // the file stays
                if let Some(b) = self.doc().buckets.get(&p.bucket_id).cloned() {
                    if crate::studio::bucket_sounds(self, &b).iter().any(|a| a.id == p.asset_id) {
                        return crate::buckets::hide(self, &p.bucket_id, &p.asset_id);
                    }
                }
                Ok(self.exec(&Command::BucketRemove { bucket_id: p.bucket_id, asset_id: p.asset_id })?.result)
            }
            "bucket_dismiss" => {
                let p: BucketRemoteP = parse(params)?;
                Ok(self.exec(&Command::BucketDismiss { bucket_id: p.bucket_id, remote_id: p.remote_id })?.result)
            }
            "bucket_generate" => {
                let p: BucketGenerateP = parse(params)?;
                crate::buckets::generate(self, &p.bucket_id, p.prompt, p.variations)
            }
            "place_from_bucket" => {
                let p: PlaceFromBucketP = parse(params)?;
                crate::buckets::place(self, p)
            }
            "play_sound" => {
                let p: PlaySoundP = parse(params)?;
                crate::buckets::play(self, p.asset_id, p.remote_id, p.from_s)
            }
            "stop_sound" => crate::player::stop_sound(self),
            "sound_playhead" => crate::player::sound_playhead(self),
            "list_sounds" => {
                let p: SceneP = parse(params)?;
                let sc = scene_id(self, p.scene.as_deref())?;
                Ok(serde_json::to_value(crate::sound_edit::list(self, &sc)?)?)
            }
            "move_sound" => self.sound_edit::<MoveSoundP>(params),
            "trim_sound" => self.sound_edit::<TrimSoundP>(params),
            "set_sound_fades" => self.sound_edit::<SoundFadesP>(params),
            "set_sound_gain" => self.sound_edit::<SoundGainP>(params),
            "set_sound_pan" => self.sound_edit::<SoundPanP>(params),
            "set_sound_depth" => self.sound_edit::<SoundDepthP>(params),
            "slip_sound" => self.sound_edit::<SlipSoundP>(params),
            "remove_sound" => self.sound_edit::<PlacementP>(params),
            "batch_sounds" => crate::sound_edit::batch(self, parse(params)?),
            "scene_rooms" => Ok(serde_json::to_value(crate::sound_edit::rooms(self)?)?),
            "set_scene_room" => crate::sound_edit::set_room(self, parse(params)?),
            "set_scene_tone" => crate::sound_edit::set_tone(self, parse(params)?),
            "voice_rooms" => Ok(crate::sound_edit::voices(self)?),
            "ambience_sounds" => {
                let p: AmbienceP = parse(params)?;
                crate::buckets::ambience(self, p.scene.as_deref())
            }
            "import_ambience" => {
                let p: ImportAmbienceP = parse(params)?;
                crate::buckets::import_ambience(self, &p)
            }
            "suggest_bucket" => {
                let p: SuggestBucketP = parse(params)?;
                let id = crate::studio::bucket_by_name(self, &p.name);
                let name = id
                    .as_ref()
                    .and_then(|b| self.doc().buckets.get(b))
                    .map(crate::studio::shown_name)
                    .unwrap_or_else(|| crate::studio::proposed_bucket(&p.name));
                Ok(json!({ "bucket_id": id, "name": name }))
            }
            "set_character_wet" => {
                let p: CharacterWetP = parse(params)?;
                Ok(self
                    .exec(&Command::SetCharacterWet {
                        character_id: p.character,
                        wet: p.wet,
                    })?
                    .result)
            }
            "bucket_set_loop" => {
                let p: BucketLoopP = parse(params)?;
                crate::buckets::set_loop(self, p)
            }
            "rescan_library" => crate::studio::rescan(self),
            "create_show" | "create_episode" | "open_project" | "create_demo" | "open_show"
            | "show_view" | "show_backdrop" | "set_show_backdrop" | "open_episode" | "close_show"
            | "migrate_episode" => {
                Err(AppError::Other(format!("{name} is handled by the session")))
            }
            other => Err(AppError::UnknownMethod(other.into())),
        }
    }

    /// A sound method: its parameters are one command.
    fn sound_edit<P: DeserializeOwned + Into<Command>>(&mut self, params: &Value) -> Result<Value> {
        let p: P = parse(params)?;
        Ok(self.exec(&p.into())?.result)
    }

    fn status(&mut self) -> Result<Value> {
        self.bus.refresh()?;
        let doc = self.doc();
        let dialogue: Vec<&ScriptLine> = doc
            .lines
            .values()
            .filter(|l| l.kind == LineKind::Dialogue && !l.adlib)
            .collect();
        let recorded: std::collections::BTreeSet<&str> = doc
            .sources
            .values()
            .filter(|s| s.role == Role::Character)
            .filter_map(|s| s.character.as_deref())
            .collect();
        let role_lines: Vec<&&ScriptLine> = dialogue
            .iter()
            .filter(|l| {
                l.character
                    .as_deref()
                    .map(|c| recorded.contains(c))
                    .unwrap_or(false)
            })
            .collect();
        let matched = role_lines
            .iter()
            .filter(|l| !doc.takes_of_line(&l.id).is_empty())
            .count();
        let (_, ai_warning) = self.ai_provider();
        let flagged: usize = crate::views::scene_summaries(doc)
            .iter()
            .map(|s| s.flagged)
            .sum();
        Ok(json!({
            "app": apc_core::brand::DISPLAY_NAME,
            "name": self.name(),
            "path": self.paths.file.to_string_lossy(),
            "show": self.paths.show.as_ref().map(|s| json!({
                "name": s.name(), "root": s.root.to_string_lossy(),
                "episode": s.episode_folder_of(&self.paths.root),
                "colors": s.data.theme.colors,
            })),
            "warnings": self.warnings,
            "script_version": doc.current_script_version().map(|v| v.number),
            "scenes": doc.scenes.len(),
            "lines": dialogue.len(),
            "recorded_role_lines": role_lines.len(),
            "lines_with_takes": matched,
            "takes": doc.takes.len(),
            "picks": doc.takes.values().filter(|t| t.pick >= 2).count(),
            "sources": doc.sources.len(),
            "missing_sources": apc_core::media::missing_sources(doc, &self.paths).len(),
            "unmatched": doc.unmatched.len(),
            "flagged": flagged,
            "cues": doc.cues.len(),
            "placements": doc.placements.len(),
            "rev": self.bus.rev(),
            "can_undo": self.bus.can_undo()?,
            "can_redo": self.bus.can_redo()?,
            "ai_provider": self.config().ai.provider,
            "room_tone": self.config().pacing.room_tone,
            "playback_level": self.config().playback.level,
            "breath_gap_s": self.config().pacing.breath_gap_s,
            "handle_in_ms": self.config().pacing.handle_in_ms,
            "handle_out_ms": self.config().pacing.handle_out_ms,
            "edge_fade_ms": self.config().pacing.edge_fade_ms,
            "ai_warning": ai_warning,
            "jobs": self.jobs.list(),
        }))
    }
}

/// A client session: maybe an open show, maybe an open episode, plus the project-level
/// methods.
pub struct Session {
    pub service: Option<Service>,
    /// The open show (studio mode): set by `open_show`, or by opening one of its episodes.
    pub show: Option<apc_core::show::Show>,
    pub client: String,
}

impl Session {
    pub fn new(client: &str) -> Self {
        Session {
            service: None,
            show: None,
            client: client.to_string(),
        }
    }

    /// Opens an episode file or folder, or a show folder or file.
    pub fn with_project(path: &Path, client: &str) -> Result<Self> {
        let mut s = Session::new(client);
        let is_show = path.extension().and_then(|e| e.to_str()) == Some(apc_core::brand::SHOW_EXT)
            || (path.is_dir()
                && apc_core::project::find_episode_file(path).is_err()
                && apc_core::show::Show::open(path).is_ok());
        if is_show {
            s.show = Some(apc_core::show::Show::open(path)?);
        } else {
            let svc = Service::open(path, client)?;
            s.show = svc.paths.show.clone();
            s.service = Some(svc);
        }
        Ok(s)
    }

    fn need_show(&self) -> Result<&apc_core::show::Show> {
        self.show
            .as_ref()
            .ok_or_else(|| AppError::Params("no show is open (open_show)".into()))
    }

    /// Opens an episode, keeping the session's show in step with it.
    fn open_service(&mut self, svc: Service) -> Result<Value> {
        let file = svc.paths.file.to_string_lossy().into_owned();
        let show = svc.paths.show.clone();
        self.replace(svc)?;
        if show.is_some()
            || self
                .show
                .as_ref()
                .map(|s| !Path::new(&file).starts_with(&s.root))
                .unwrap_or(false)
        {
            self.show = show;
        }
        Ok(json!({ "file": file }))
    }

    /// Switches to another episode, closing the open one first (final snapshot).
    fn replace(&mut self, svc: Service) -> Result<()> {
        if let Some(old) = self.service.as_mut() {
            old.close()?;
        }
        self.service = Some(svc);
        Ok(())
    }

    /// Opening another show closes an episode that isn't part of it.
    fn close_episode_outside(&mut self, root: &Path) -> Result<()> {
        let outside = self
            .service
            .as_ref()
            .map(|s| !s.paths.root.starts_with(root))
            .unwrap_or(false);
        if outside {
            if let Some(svc) = self.service.as_mut() {
                svc.close()?;
            }
            self.service = None;
        }
        Ok(())
    }

    /// Closes the open episode (on quit). Errors are reported, not raised: quitting goes on.
    pub fn shutdown(&mut self) {
        if let Some(svc) = self.service.as_mut() {
            if let Err(e) = svc.close() {
                eprintln!("closing the episode: {e}");
            }
        }
        self.service = None;
    }

    pub fn call(&mut self, name: &str, params: &Value) -> Result<Value> {
        match name {
            "create_show" => {
                let p: CreateShowP = parse(params)?;
                let root = match (&p.root, &p.parent, &p.name) {
                    (Some(r), _, _) => PathBuf::from(r),
                    (None, Some(parent), Some(n)) => {
                        Path::new(parent).join(apc_core::project::safe_name(n))
                    }
                    _ => return Err(AppError::Params("give root (or parent and name)".into())),
                };
                let show = apc_core::show::Show::create(&root, p.name.as_deref())?;
                self.close_episode_outside(&show.root)?;
                let v = crate::shows::view(&show, self.service.as_ref());
                self.show = Some(show);
                Ok(v)
            }
            "open_show" => {
                let p: ShowPathP = parse(params)?;
                let show = apc_core::show::Show::open(Path::new(&p.path))?;
                self.close_episode_outside(&show.root)?;
                let v = crate::shows::view(&show, self.service.as_ref());
                self.show = Some(show);
                Ok(v)
            }
            "show_view" => {
                let show = apc_core::show::Show::open(&self.need_show()?.file)?;
                let v = crate::shows::view(&show, self.service.as_ref());
                self.show = Some(show);
                Ok(v)
            }
            "show_backdrop" => crate::shows::backdrop(self.need_show()?),
            "set_show_backdrop" => {
                let p: PathP = parse(params)?;
                let mut show = self.need_show()?.clone();
                let r = crate::shows::set_backdrop(&mut show, Path::new(&p.path))?;
                if let Some(svc) = self.service.as_mut() {
                    if svc
                        .paths
                        .show
                        .as_ref()
                        .map(|s| s.file == show.file)
                        .unwrap_or(false)
                    {
                        svc.paths.show = Some(show.clone());
                    }
                }
                self.show = Some(show);
                Ok(r)
            }
            "open_episode" => {
                let p: OpenEpisodeP = parse(params)?;
                let show = self.need_show()?.clone();
                let info = show.episode_info(&p.episode);
                let svc = match &info.file {
                    Some(f) => Service::open(f, &self.client)?,
                    None if p.create => {
                        Service::create(&show.root, &p.episode, Some(&show.root), &self.client)?
                    }
                    None => {
                        return Err(AppError::Params(format!(
                            "{} has no {} project yet",
                            info.label,
                            apc_core::brand::DISPLAY_NAME
                        )))
                    }
                };
                let created = info.file.is_none();
                let mut r = self.open_service(svc)?;
                r["created"] = json!(created);
                Ok(r)
            }
            "close_show" => {
                if let Some(svc) = self.service.as_mut() {
                    svc.close()?;
                }
                self.service = None;
                self.show = None;
                Ok(json!({ "closed": true }))
            }
            "migrate_episode" => {
                let p: MigrateP = parse(params)?;
                let show = self.need_show()?.clone();
                crate::shows::migrate(&show, Path::new(&p.source), &p.episode, p.dry_run)
            }
            "create_episode" => {
                let p: CreateEpisodeP = parse(params)?;
                let parent = PathBuf::from(p.parent.unwrap_or_else(|| ".".into()));
                let svc = Service::create(
                    &parent,
                    &p.name,
                    p.show.as_deref().map(Path::new),
                    &self.client,
                )?;
                self.open_service(svc)
            }
            "create_demo" => {
                let p: DemoP = parse(params)?;
                let dir = Path::new(&p.parent).join(&p.name);
                std::fs::create_dir_all(&dir)?;
                let svc = crate::demo::create(&dir, &self.client, true)?;
                let file = svc.paths.file.to_string_lossy().into_owned();
                self.replace(svc)?;
                Ok(json!({ "file": file }))
            }
            "open_project" => {
                let p: OpenP = parse(params)?;
                let svc = Service::open(Path::new(&p.path), &self.client)?;
                self.open_service(svc)
            }
            "close_project" => {
                let svc = self.service.as_mut().ok_or(AppError::NoProject)?;
                if svc.jobs.list().iter().any(|j| j.state == "running") {
                    return Err(AppError::Params(
                        "a background job (analysis or a download) is still running; close the episode when it is done".into(),
                    ));
                }
                let r = svc.close()?;
                self.service = None;
                Ok(r)
            }
            "rescan_library" if self.service.is_none() => {
                let show = self.need_show()?;
                let (lib, report) = apc_sound::ShowLibrary::open(show)?;
                Ok(
                    json!({ "files": report.files, "hashed": report.hashed, "removed": report.removed, "folders": lib.folders().len() }),
                )
            }
            _ => self
                .service
                .as_mut()
                .ok_or(AppError::NoProject)?
                .call(name, params),
        }
    }
}
