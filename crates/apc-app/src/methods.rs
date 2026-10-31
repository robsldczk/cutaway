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
    pub parent: String,
    pub name: String,
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
    /// Anchor to a line (and word); otherwise `at_s` is absolute scene time.
    #[serde(default)]
    pub anchor_line_id: Option<String>,
    #[serde(default)]
    pub anchor_word: Option<usize>,
    #[serde(default)]
    pub at_s: Option<f64>,
    #[serde(default)]
    pub offset_s: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImportSoundsP {
    pub paths: Vec<String>,
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
pub struct PeaksP {
    pub source_id: String,
    pub t0: f64,
    pub t1: f64,
    #[serde(default)]
    pub buckets: Option<usize>,
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
    "create_show", CreateShowP, Write, true, "Create a show folder with its library (TARMAC/TARMAC.cutawayshow, Library/, Episodes/).";
    "create_episode", CreateEpisodeP, Write, true, "Create an episode (inside a show, or standalone) and open it.";
    "open_project", OpenP, Read, true, "Open an episode file or folder. Later calls work on it.";
    "project_status", NoParams, Read, true, "Progress, match rate, flagged items, undo state and AI provider of the open episode.";
    "list_scenes", NoParams, Read, true, "Scenes with heading, place, line counts and flagged items.";
    "import_script", ImportScriptP, Write, true, "Import a Fountain/Throughline script as the next version (vNNN). Returns the change summary.";
    "import_audio", ImportAudioP, Write, true, "Import role and room recordings (files or folders) and assign roles automatically.";
    "roles", NoParams, Read, true, "Every source file with its role, character and confidence.";
    "confirm_roles", ConfirmRolesP, Write, true, "Set or correct file roles. Corrections are remembered as show aliases.";
    "relink_media", RelinkP, Write, true, "Find missing source files again by hash under a folder.";
    "run_transcription", NoParams, Write, true, "Transcribe every source (transcript sidecars, or local Parakeet). Cached.";
    "run_alignment", NoParams, Write, true, "Run the full analysis: sync, alignment, takes, room notes, picks. One undoable step.";
    "run_notes", NoParams, Write, true, "Re-run room classification, note condensing and pick detection (re-runs the cached pipeline).";
    "job_status", NoParams, Read, true, "Background jobs with progress.";
    "start_analysis", NoParams, Write, false, "Run the analysis in the background (the UI polls job_status).";
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
    "spot_sound_effects", SpotP, Write, true, "AI sound spotting for a scene: adds anchored cues to the bucket.";
    "list_bucket", SceneP, Read, true, "Sound cues (wish list) of a scene, or of the whole episode.";
    "add_cue", AddCueP, Write, true, "Add a sound cue by hand.";
    "update_cue", UpdateCueP, Write, true, "Change fields of a cue (description, tags, priority, accepted …).";
    "delete_cue", DeleteCueP, Destructive, true, "Delete a cue (its placements stay, unlinked).";
    "merge_cues", MergeCuesP, Write, true, "Merge duplicate cues into one (optionally a show-level cue).";
    "attach_sound", AttachP, Write, true, "Attach a library sound to a cue as a candidate (or choose it).";
    "search_sounds", SearchSoundsP, Read, true, "Search the library (and connected services) for a query or a cue.";
    "generate_sound", GenerateP, Write, true, "Generate sound variations for a cue or prompt (ElevenLabs).";
    "import_sounds", ImportSoundsP, Write, true, "Import files into the library (sorted and tagged), optionally onto a cue or the timeline.";
    "list_library", LibraryP, Read, true, "Library sounds, optionally filtered by a query.";
    "delete_asset", DeleteAssetP, Destructive, true, "Remove a sound from the library (the file moves to Backups/Trash).";
    "place_sound", PlaceSoundP, Write, true, "Place a sound on the timeline, anchored to a line/word or at an absolute time.";
    "export_fcpxml", ExportP, Write, true, "Export FCPXML 1.9 for DaVinci Resolve and Logic (one lane per role).";
    "export_srt", ExportP, Write, true, "Export SRT subtitles from the final word timings.";
    "export_cut_list", ExportP, Write, true, "Export the as-cut transcript / cut list (format csv or md).";
    "export_mix", ExportMixP, Write, true, "Bounce a scene to WAV or MP3.";
    "play", PlayP, Read, false, "Play the scene from a time (audio device, or a virtual clock without one).";
    "stop", NoParams, Read, false, "Stop playback.";
    "playhead", NoParams, Read, false, "Playhead position and state.";
    "peaks", PeaksP, Read, false, "Waveform peaks of a source range.";
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
                self.confirm_roles(&items)
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
            "start_analysis" => Ok(json!({ "job_id": self.jobs.start_analysis(self.paths.file.clone()) })),
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
                let p: SpotP = parse(params)?;
                crate::sound::spot(self, &p.scene)
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
                crate::sound::generate(self, p.cue_id, p.prompt, p.variations, p.duration_s)
            }
            "import_sounds" => {
                let p: ImportSoundsP = parse(params)?;
                crate::sound::import(self, &p.paths, p.cue_id, p.scene, p.at_s)
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
            "stop" => crate::player::stop(self),
            "playhead" => crate::player::playhead(self),
            "peaks" => {
                let p: PeaksP = parse(params)?;
                let a = self.analysis_of(&p.source_id).ok_or_else(|| AppError::Params("unknown source".into()))?;
                let pk: Vec<f32> = a.peaks_range(p.t0, p.t1, p.buckets.unwrap_or(200)).into_iter().map(|(lo, hi)| lo.abs().max(hi.abs())).collect();
                Ok(json!(pk))
            }
            "create_show" | "create_episode" | "open_project" => {
                Err(AppError::Other(format!("{name} is handled by the session")))
            }
            other => Err(AppError::UnknownMethod(other.into())),
        }
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
            "show": self.paths.show_file.as_ref().map(|p| p.to_string_lossy().into_owned()),
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
            "ai_warning": ai_warning,
            "jobs": self.jobs.list(),
        }))
    }
}

/// A client session: maybe an open episode, plus the project-level methods.
pub struct Session {
    pub service: Option<Service>,
    pub client: String,
}

impl Session {
    pub fn new(client: &str) -> Self {
        Session {
            service: None,
            client: client.to_string(),
        }
    }

    pub fn with_project(path: &Path, client: &str) -> Result<Self> {
        Ok(Session {
            service: Some(Service::open(path, client)?),
            client: client.to_string(),
        })
    }

    pub fn call(&mut self, name: &str, params: &Value) -> Result<Value> {
        match name {
            "create_show" => {
                let p: CreateShowP = parse(params)?;
                let (sf, _) = apc_core::project::ShowFolder::create(Path::new(&p.parent), &p.name)?;
                Ok(json!({ "path": sf.root.to_string_lossy(), "file": sf.file.to_string_lossy() }))
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
                let file = svc.paths.file.to_string_lossy().into_owned();
                self.service = Some(svc);
                Ok(json!({ "file": file }))
            }
            "open_project" => {
                let p: OpenP = parse(params)?;
                let svc = Service::open(Path::new(&p.path), &self.client)?;
                let file = svc.paths.file.to_string_lossy().into_owned();
                self.service = Some(svc);
                Ok(json!({ "file": file }))
            }
            _ => self
                .service
                .as_mut()
                .ok_or(AppError::NoProject)?
                .call(name, params),
        }
    }
}
