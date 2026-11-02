//! Every document change the UI, CLI and MCP can make. See `docs/PLAN.md` §2.3.

use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bus::Tx;
use crate::config::ProjectConfig;
use crate::error::{CoreError, Result};
use crate::fountain::ParsedScript;
use crate::ids::new_id;
use crate::model::*;
use crate::timeline::{effective_gap, layout_scene, placement_start};

/// Analysis output applied in one undoable step.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, JsonSchema)]
pub struct AnalysisPayload {
    #[serde(default)]
    pub sessions: Vec<Session>,
    /// Full replacements of source entities (sync offset, session, info).
    #[serde(default)]
    pub sources: Vec<SourceFile>,
    #[serde(default)]
    pub takes: Vec<Take>,
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub unmatched: Vec<Unmatched>,
    /// Only these lines' takes are replaced (all when empty).
    #[serde(default)]
    pub line_scope: Vec<String>,
    /// With a line scope: notes and unmatched speech are replaced only for these sources.
    #[serde(default)]
    pub source_scope: Vec<String>,
    /// Characters' vocal sounds cut from the recordings, and their vocal buckets.
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub buckets: Vec<SoundBucket>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Sets a project config value by dotted path, e.g. `ai.model`, `pacing.breath_gap_s`.
    SetConfig {
        key: String,
        value: Value,
    },
    ImportScriptVersion {
        version: ScriptVersion,
        parsed: ParsedScript,
    },
    /// Story context only (character bios, settings) from another writers' file.
    ImportStoryContext {
        story: crate::story::StoryContext,
    },
    AddSources {
        sources: Vec<SourceFile>,
    },
    SetRole {
        source_id: String,
        role: Role,
        #[serde(default)]
        character: Option<String>,
    },
    RelinkMedia {
        source_id: String,
        path: String,
    },
    ApplyAnalysis {
        payload: AnalysisPayload,
    },
    SelectTake {
        line_id: String,
        take_id: String,
    },
    /// Steps through the takes of a line (Left/Right).
    StepTake {
        line_id: String,
        delta: i32,
    },
    ApplyTakeToPassage {
        line_ids: Vec<String>,
        take_no: u32,
    },
    ToggleFavorite {
        take_id: String,
    },
    SetGap {
        line_id: String,
        #[serde(default)]
        gap_s: Option<f64>,
    },
    NudgeGap {
        line_id: String,
        delta_s: f64,
    },
    SetTrim {
        line_id: String,
        head_s: f64,
        tail_s: f64,
    },
    SetSpacer {
        spacer_id: String,
        #[serde(default)]
        enabled: Option<bool>,
        #[serde(default)]
        duration_s: Option<f64>,
        #[serde(default)]
        reset_duration: bool,
    },
    MoveItem {
        scene_id: String,
        item_id: String,
        to_index: usize,
    },
    SetSceneTightness {
        scene_id: String,
        tightness: f64,
    },
    SetComp {
        line_id: String,
        segments: Vec<CompSegment>,
    },
    ResetComp {
        line_id: String,
    },
    DeleteWords {
        line_id: String,
        from: usize,
        to: usize,
    },
    RestoreWords {
        line_id: String,
        from: usize,
        to: usize,
    },
    SetCutPause {
        line_id: String,
        cut: usize,
        keep: bool,
    },
    SetPauseTrim {
        line_id: String,
        word: usize,
        trim_s: f64,
    },
    PinClip {
        line_id: String,
        #[serde(default)]
        at_s: Option<f64>,
    },
    /// Moves a clip on the Edit page. Ripple moves everything after it; without ripple
    /// the next clip keeps its place (the clip may overlap its neighbours).
    MoveClip {
        line_id: String,
        delta_s: f64,
        #[serde(default = "yes")]
        ripple: bool,
    },
    SetClipGain {
        line_id: String,
        gain_db: f64,
    },
    SplitClip {
        line_id: String,
        at_s: f64,
    },
    ClearSplits {
        line_id: String,
    },
    UseUnmatched {
        unmatched_id: String,
        scene_id: String,
        index: usize,
        #[serde(default)]
        character: Option<String>,
    },
    SetRoomTone {
        location_id: String,
        tone: RoomTone,
    },
    AddCue {
        cue: SoundCue,
    },
    UpdateCue {
        cue_id: String,
        patch: Value,
    },
    DeleteCue {
        cue_id: String,
    },
    MergeCues {
        cue_ids: Vec<String>,
        into_id: String,
    },
    /// Adds AI-suggested cues for a scene (replaces earlier unaccepted suggestions).
    ApplySpotting {
        scene_id: String,
        cues: Vec<SoundCue>,
    },
    AttachCandidate {
        cue_id: String,
        asset_id: String,
    },
    ChooseCandidate {
        cue_id: String,
        asset_id: String,
    },
    AddAsset {
        asset: Asset,
    },
    PlaceSound {
        placement: Placement,
    },
    UpdatePlacement {
        placement_id: String,
        patch: Value,
    },
    /// Pins a placement to absolute scene time (where it currently plays).
    UnanchorPlacement {
        placement_id: String,
    },
    DeletePlacement {
        placement_id: String,
    },
    /// Splits a placed sound at scene time `at_s` into two placements.
    SplitPlacement {
        placement_id: String,
        at_s: f64,
    },
    /// Creates or replaces a sound bucket.
    PutBucket {
        bucket: SoundBucket,
    },
    DeleteBucket {
        bucket_id: String,
    },
    /// Keeps a downloaded/library sound in a bucket (and drops its preview).
    BucketKeep {
        bucket_id: String,
        asset_id: String,
    },
    BucketRemove {
        bucket_id: String,
        asset_id: String,
    },
    /// Hides a search result from a bucket for good.
    BucketDismiss {
        bucket_id: String,
        remote_id: String,
    },
    /// Lane volume, pan, mute and solo on the Edit page.
    SetLaneMix {
        mix: LaneMix,
    },
    /// Stores the import match: links between director room files and recordings.
    ApplyMatch {
        sessions: Vec<Session>,
        sources: Vec<SourceFile>,
        /// When the match ran (shown as "Matched 2 min ago").
        #[serde(default)]
        at: Option<String>,
    },
    /// Adds or replaces a dev-panel mark (a stretch Rob pointed at).
    PutMark {
        mark: DevMark,
    },
    DeleteMark {
        mark_id: String,
    },
    /// Applies a mark's proposal: adds and removes takes, renumbers the lines involved.
    ApplyMark {
        mark_id: String,
    },
}

fn yes() -> bool {
    true
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::SetConfig { .. } => "set_config",
            Command::ImportScriptVersion { .. } => "import_script_version",
            Command::ImportStoryContext { .. } => "import_story",
            Command::AddSources { .. } => "import_media",
            Command::SetRole { .. } => "set_role",
            Command::RelinkMedia { .. } => "relink_media",
            Command::ApplyAnalysis { .. } => "apply_analysis",
            Command::SelectTake { .. } => "select_take",
            Command::StepTake { .. } => "step_take",
            Command::ApplyTakeToPassage { .. } => "apply_take_to_passage",
            Command::ToggleFavorite { .. } => "toggle_favorite",
            Command::SetGap { .. } => "set_gap",
            Command::NudgeGap { .. } => "nudge_gap",
            Command::SetTrim { .. } => "set_trim",
            Command::SetSpacer { .. } => "set_spacer",
            Command::MoveItem { .. } => "move_item",
            Command::SetSceneTightness { .. } => "set_scene_tightness",
            Command::SetComp { .. } => "set_comp",
            Command::ResetComp { .. } => "reset_comp",
            Command::DeleteWords { .. } => "delete_words",
            Command::RestoreWords { .. } => "restore_words",
            Command::SetCutPause { .. } => "set_cut_pause",
            Command::SetPauseTrim { .. } => "set_pause_trim",
            Command::PinClip { .. } => "pin_clip",
            Command::MoveClip { .. } => "move_clip",
            Command::SetClipGain { .. } => "set_clip_gain",
            Command::SplitClip { .. } => "split_clip",
            Command::ClearSplits { .. } => "clear_splits",
            Command::UseUnmatched { .. } => "use_unmatched",
            Command::SetRoomTone { .. } => "set_room_tone",
            Command::AddCue { .. } => "add_cue",
            Command::UpdateCue { .. } => "update_cue",
            Command::DeleteCue { .. } => "delete_cue",
            Command::MergeCues { .. } => "merge_cues",
            Command::ApplySpotting { .. } => "apply_spotting",
            Command::AttachCandidate { .. } => "attach_candidate",
            Command::ChooseCandidate { .. } => "choose_candidate",
            Command::AddAsset { .. } => "add_asset",
            Command::PlaceSound { .. } => "place_sound",
            Command::UpdatePlacement { .. } => "update_placement",
            Command::UnanchorPlacement { .. } => "unanchor_placement",
            Command::DeletePlacement { .. } => "delete_placement",
            Command::SplitPlacement { .. } => "split_placement",
            Command::PutBucket { .. } => "put_bucket",
            Command::DeleteBucket { .. } => "delete_bucket",
            Command::BucketKeep { .. } => "bucket_keep",
            Command::BucketRemove { .. } => "bucket_remove",
            Command::BucketDismiss { .. } => "bucket_dismiss",
            Command::SetLaneMix { .. } => "set_lane_mix",
            Command::ApplyMatch { .. } => "apply_match",
            Command::PutMark { .. } => "put_mark",
            Command::DeleteMark { .. } => "delete_mark",
            Command::ApplyMark { .. } => "apply_mark",
        }
    }

    /// Parameters for the command log; large payloads are summarised.
    pub fn log_params(&self) -> Value {
        match self {
            Command::ImportScriptVersion { version, parsed } => json!({
                "version": version.number, "file": version.file,
                "scenes": parsed.scenes.len(),
            }),
            Command::ApplyAnalysis { payload } => json!({
                "takes": payload.takes.len(), "notes": payload.notes.len(),
                "unmatched": payload.unmatched.len(), "sessions": payload.sessions.len(),
                "vocals": payload.assets.len(),
            }),
            Command::ApplyMatch {
                sessions, sources, ..
            } => json!({
                "sessions": sessions.len(), "sources": sources.len(),
            }),
            Command::AddSources { sources } => json!({
                "files": sources.iter().map(|s| s.original_name.clone()).collect::<Vec<_>>()
            }),
            other => serde_json::to_value(other).unwrap_or(Value::Null),
        }
    }

    pub fn apply(&self, tx: &mut Tx) -> Result<Value> {
        let pacing = ProjectConfig::from_doc(tx.doc()).pacing();
        match self {
            Command::SetConfig { key, value } => {
                let mut cfg = ProjectConfig::from_doc(tx.doc());
                cfg.set(key, value.clone())?;
                tx.put(&Setting {
                    id: "config".into(),
                    value: serde_json::to_value(&cfg)?,
                })?;
                Ok(Value::Null)
            }
            Command::ImportScriptVersion { version, parsed } => {
                let summary = crate::script::apply_version(tx, version, parsed)?;
                Ok(serde_json::to_value(summary)?)
            }
            Command::ImportStoryContext { story } => {
                Ok(serde_json::to_value(crate::story::merge(tx, story)?)?)
            }
            Command::AddSources { sources } => {
                for s in sources {
                    tx.put(s)?;
                }
                Ok(json!({ "ids": sources.iter().map(|s| s.id.clone()).collect::<Vec<_>>() }))
            }
            Command::SetRole {
                source_id,
                role,
                character,
            } => {
                let mut s = source(tx, source_id)?;
                s.role = *role;
                s.character = if *role == Role::Character {
                    character.clone()
                } else {
                    None
                };
                s.role_confidence = 1.0;
                tx.put(&s)?;
                Ok(Value::Null)
            }
            Command::RelinkMedia { source_id, path } => {
                let mut s = source(tx, source_id)?;
                s.path = path.clone();
                s.missing = false;
                tx.put(&s)?;
                Ok(Value::Null)
            }
            Command::ApplyAnalysis { payload } => apply_analysis(tx, payload),
            Command::SelectTake { line_id, take_id } => {
                let t = tx
                    .doc()
                    .takes
                    .get(take_id)
                    .ok_or_else(|| CoreError::not_found("take", take_id))?;
                if &t.line_id != line_id {
                    return Err(CoreError::invalid(format!(
                        "take {take_id} belongs to another line"
                    )));
                }
                let mut b = boxed(tx, line_id)?;
                b.take_id = Some(take_id.clone());
                b.comp.clear();
                b.edits = WordEdits::default();
                tx.put(&b)?;
                Ok(json!({ "take_id": take_id }))
            }
            Command::StepTake { line_id, delta } => {
                let takes: Vec<String> = tx
                    .doc()
                    .takes_of_line(line_id)
                    .iter()
                    .map(|t| t.id.clone())
                    .collect();
                if takes.is_empty() {
                    return Ok(Value::Null);
                }
                let mut b = boxed(tx, line_id)?;
                let cur = b
                    .take_id
                    .as_ref()
                    .and_then(|id| takes.iter().position(|t| t == id))
                    .unwrap_or(0) as i64;
                let n = takes.len() as i64;
                let next = (cur + *delta as i64).rem_euclid(n) as usize;
                b.take_id = Some(takes[next].clone());
                b.comp.clear();
                b.edits = WordEdits::default();
                tx.put(&b)?;
                Ok(json!({ "take_id": takes[next] }))
            }
            Command::ApplyTakeToPassage { line_ids, take_no } => {
                let mut applied = Vec::new();
                for lid in line_ids {
                    let pick = tx
                        .doc()
                        .takes_of_line(lid)
                        .into_iter()
                        .find(|t| t.take_no == *take_no)
                        .map(|t| t.id.clone());
                    if let Some(tid) = pick {
                        let mut b = boxed(tx, lid)?;
                        b.take_id = Some(tid);
                        b.comp.clear();
                        b.edits = WordEdits::default();
                        tx.put(&b)?;
                        applied.push(lid.clone());
                    }
                }
                Ok(json!({ "applied": applied }))
            }
            Command::ToggleFavorite { take_id } => {
                let mut t = tx
                    .doc()
                    .takes
                    .get(take_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("take", take_id))?;
                t.favorite = !t.favorite;
                t.favorite_origin = Some("user".to_string());
                tx.put(&t)?;
                Ok(json!({ "favorite": t.favorite }))
            }
            Command::SetGap { line_id, gap_s } => {
                let mut b = boxed(tx, line_id)?;
                b.gap_s = gap_s.map(|g| g.clamp(-5.0, 30.0));
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::NudgeGap { line_id, delta_s } => {
                let scene = scene_of(tx, line_id)?;
                let cur = effective_gap(tx.doc(), &scene, line_id, &pacing);
                let mut b = boxed(tx, line_id)?;
                let g = ((cur + delta_s) * 1000.0).round() / 1000.0;
                b.gap_s = Some(g.clamp(-5.0, 30.0));
                tx.put(&b)?;
                Ok(json!({ "gap_s": b.gap_s }))
            }
            Command::SetTrim {
                line_id,
                head_s,
                tail_s,
            } => {
                let mut b = boxed(tx, line_id)?;
                if let Some(t) = b.take_id.as_ref().and_then(|id| tx.doc().takes.get(id)) {
                    if head_s + tail_s > t.duration() - 0.05 {
                        return Err(CoreError::invalid("trims leave nothing of the take"));
                    }
                }
                b.head_s = *head_s;
                b.tail_s = *tail_s;
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::SetSpacer {
                spacer_id,
                enabled,
                duration_s,
                reset_duration,
            } => {
                let mut s = tx.doc().spacers.get(spacer_id).cloned().unwrap_or(Spacer {
                    id: spacer_id.clone(),
                    duration_s: None,
                    enabled: true,
                });
                if let Some(e) = enabled {
                    s.enabled = *e;
                }
                if let Some(d) = duration_s {
                    s.duration_s = Some(d.clamp(0.0, 30.0));
                }
                if *reset_duration {
                    s.duration_s = None;
                }
                tx.put(&s)?;
                Ok(Value::Null)
            }
            Command::MoveItem {
                scene_id,
                item_id,
                to_index,
            } => {
                let mut sc = tx
                    .doc()
                    .scenes
                    .get(scene_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("scene", scene_id))?;
                let from = sc
                    .order
                    .iter()
                    .position(|x| x == item_id)
                    .ok_or_else(|| CoreError::not_found("item", item_id))?;
                let item = sc.order.remove(from);
                let to = (*to_index).min(sc.order.len());
                sc.order.insert(to, item);
                tx.put(&sc)?;
                Ok(json!({ "index": to }))
            }
            Command::SetSceneTightness {
                scene_id,
                tightness,
            } => {
                let mut sc = tx
                    .doc()
                    .scenes
                    .get(scene_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("scene", scene_id))?;
                sc.tightness = tightness.clamp(0.1, 3.0);
                tx.put(&sc)?;
                Ok(Value::Null)
            }
            Command::SetComp { line_id, segments } => {
                for s in segments {
                    let t = tx
                        .doc()
                        .takes
                        .get(&s.take_id)
                        .ok_or_else(|| CoreError::not_found("take", &s.take_id))?;
                    if &t.line_id != line_id || s.to_word <= s.from_word {
                        return Err(CoreError::invalid("comp segment doesn't fit the line"));
                    }
                }
                let mut b = boxed(tx, line_id)?;
                let mut segs = segments.clone();
                segs.sort_by_key(|s| s.from_word);
                b.comp = segs;
                b.edits = WordEdits::default();
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::ResetComp { line_id } => {
                let mut b = boxed(tx, line_id)?;
                b.comp.clear();
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::DeleteWords { line_id, from, to } => {
                let mut b = boxed(tx, line_id)?;
                for i in *from..=*to {
                    if !b.edits.deleted.contains(&i) {
                        b.edits.deleted.push(i);
                    }
                }
                b.edits.deleted.sort_unstable();
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::RestoreWords { line_id, from, to } => {
                let mut b = boxed(tx, line_id)?;
                b.edits.deleted.retain(|i| i < from || i > to);
                b.edits.keep_pause.retain(|i| i < from || i > to);
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::SetCutPause { line_id, cut, keep } => {
                let mut b = boxed(tx, line_id)?;
                b.edits.keep_pause.retain(|c| c != cut);
                if *keep {
                    b.edits.keep_pause.push(*cut);
                    b.edits.keep_pause.sort_unstable();
                }
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::SetPauseTrim {
                line_id,
                word,
                trim_s,
            } => {
                let mut b = boxed(tx, line_id)?;
                if *trim_s <= 0.0 {
                    b.edits.pause_trims.remove(word);
                } else {
                    b.edits.pause_trims.insert(*word, *trim_s);
                }
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::PinClip { line_id, at_s } => {
                let mut b = boxed(tx, line_id)?;
                b.pinned_s = *at_s;
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::MoveClip {
                line_id,
                delta_s,
                ripple,
            } => move_clip(tx, line_id, *delta_s, *ripple, &pacing),
            Command::SetClipGain { line_id, gain_db } => {
                let mut b = boxed(tx, line_id)?;
                b.gain_db = gain_db.clamp(-60.0, 24.0);
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::SplitClip { line_id, at_s } => {
                let mut b = boxed(tx, line_id)?;
                b.splits.push(*at_s);
                b.splits.sort_by(f64::total_cmp);
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::ClearSplits { line_id } => {
                let mut b = boxed(tx, line_id)?;
                b.splits.clear();
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::UseUnmatched {
                unmatched_id,
                scene_id,
                index,
                character,
            } => use_unmatched(tx, unmatched_id, scene_id, *index, character.clone()),
            Command::SetRoomTone { location_id, tone } => {
                let mut loc = tx
                    .doc()
                    .locations
                    .get(location_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("location", location_id))?;
                loc.room_tone = tone.clone();
                tx.put(&loc)?;
                Ok(Value::Null)
            }
            Command::AddCue { cue } => {
                tx.put(cue)?;
                Ok(json!({ "id": cue.id }))
            }
            Command::UpdateCue { cue_id, patch } => {
                let cur = tx
                    .doc()
                    .cues
                    .get(cue_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("cue", cue_id))?;
                let updated: SoundCue = merge_patch(&cur, patch)?;
                if updated.id != cur.id {
                    return Err(CoreError::invalid("a cue id can't change"));
                }
                tx.put(&updated)?;
                Ok(Value::Null)
            }
            Command::DeleteCue { cue_id } => {
                let placements: Vec<Placement> = tx
                    .doc()
                    .placements
                    .values()
                    .filter(|p| p.cue_id.as_deref() == Some(cue_id))
                    .cloned()
                    .collect();
                for mut p in placements {
                    p.cue_id = None;
                    tx.put(&p)?;
                }
                tx.delete::<SoundCue>(cue_id)?;
                Ok(Value::Null)
            }
            Command::MergeCues { cue_ids, into_id } => {
                let mut into = tx
                    .doc()
                    .cues
                    .get(into_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("cue", into_id))?;
                for id in cue_ids.iter().filter(|id| *id != into_id) {
                    let Some(c) = tx.doc().cues.get(id).cloned() else {
                        continue;
                    };
                    for cand in c.candidates {
                        if !into.candidates.contains(&cand) {
                            into.candidates.push(cand);
                        }
                    }
                    for t in c.tags {
                        if !into.tags.contains(&t) {
                            into.tags.push(t);
                        }
                    }
                    if into.chosen.is_none() {
                        into.chosen = c.chosen;
                    }
                    let ps: Vec<Placement> = tx
                        .doc()
                        .placements
                        .values()
                        .filter(|p| p.cue_id.as_deref() == Some(id.as_str()))
                        .cloned()
                        .collect();
                    for mut p in ps {
                        p.cue_id = Some(into_id.clone());
                        tx.put(&p)?;
                    }
                    tx.delete::<SoundCue>(id)?;
                }
                into.status = cue_status(tx.doc(), &into);
                tx.put(&into)?;
                Ok(Value::Null)
            }
            Command::ApplySpotting { scene_id, cues } => {
                let stale: Vec<String> = tx
                    .doc()
                    .cues
                    .values()
                    .filter(|c| &c.scene_id == scene_id && c.ai_suggested && !c.accepted)
                    .filter(|c| c.candidates.is_empty() && c.chosen.is_none())
                    .map(|c| c.id.clone())
                    .collect();
                for id in stale {
                    tx.delete::<SoundCue>(&id)?;
                }
                for c in cues {
                    let mut c = c.clone();
                    c.scene_id = scene_id.clone();
                    c.ai_suggested = true;
                    tx.put(&c)?;
                }
                Ok(json!({ "cues": cues.len() }))
            }
            Command::AttachCandidate { cue_id, asset_id } => {
                let mut c = cue(tx, cue_id)?;
                if !c.candidates.contains(asset_id) {
                    c.candidates.push(asset_id.clone());
                }
                c.status = cue_status(tx.doc(), &c);
                tx.put(&c)?;
                Ok(Value::Null)
            }
            Command::ChooseCandidate { cue_id, asset_id } => {
                let mut c = cue(tx, cue_id)?;
                if !c.candidates.contains(asset_id) {
                    c.candidates.push(asset_id.clone());
                }
                c.chosen = Some(asset_id.clone());
                c.status = cue_status(tx.doc(), &c);
                tx.put(&c)?;
                Ok(Value::Null)
            }
            Command::AddAsset { asset } => {
                tx.put(asset)?;
                Ok(json!({ "id": asset.id }))
            }
            Command::PlaceSound { placement } => {
                if !tx.doc().assets.contains_key(&placement.asset_id) {
                    return Err(CoreError::not_found("asset", &placement.asset_id));
                }
                let mut p = placement.clone();
                p.layer = free_layer(tx.doc(), &p, &pacing);
                tx.put(&p)?;
                if let Some(cid) = &p.cue_id {
                    if let Some(mut c) = tx.doc().cues.get(cid).cloned() {
                        if c.chosen.is_none() {
                            c.chosen = Some(p.asset_id.clone());
                        }
                        if !c.candidates.contains(&p.asset_id) {
                            c.candidates.push(p.asset_id.clone());
                        }
                        c.status = cue_status(tx.doc(), &c);
                        tx.put(&c)?;
                    }
                }
                Ok(json!({ "id": p.id, "layer": p.layer }))
            }
            Command::UpdatePlacement {
                placement_id,
                patch,
            } => {
                let cur = placement(tx, placement_id)?;
                let mut updated: Placement = merge_patch(&cur, patch)?;
                if updated.id != cur.id {
                    return Err(CoreError::invalid("a placement id can't change"));
                }
                if patch.get("layer").is_none() {
                    updated.layer = free_layer(tx.doc(), &updated, &pacing);
                }
                tx.put(&updated)?;
                Ok(Value::Null)
            }
            Command::UnanchorPlacement { placement_id } => {
                let mut p = placement(tx, placement_id)?;
                let scene = tx
                    .doc()
                    .scenes
                    .get(&p.scene_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("scene", &p.scene_id))?;
                let layout = layout_scene(tx.doc(), &scene, &pacing);
                let at = placement_start(&layout, &p);
                p.anchor = None;
                p.at_s = at;
                p.offset_s = 0.0;
                tx.put(&p)?;
                Ok(json!({ "at_s": at }))
            }
            Command::SplitPlacement { placement_id, at_s } => {
                let p = placement(tx, placement_id)?;
                let scene = tx
                    .doc()
                    .scenes
                    .get(&p.scene_id)
                    .cloned()
                    .ok_or_else(|| CoreError::not_found("scene", &p.scene_id))?;
                let start = placement_start(&layout_scene(tx.doc(), &scene, &pacing), &p);
                let cut = at_s - start;
                if cut <= 0.01 || cut >= p.len_s - 0.01 {
                    return Err(CoreError::invalid("the split point is outside the sound"));
                }
                let mut a = p.clone();
                a.len_s = cut;
                a.fade_out_s = a.fade_out_s.min(0.01);
                let mut b = p.clone();
                b.id = new_id("pl");
                b.in_s = p.in_s + cut;
                b.len_s = p.len_s - cut;
                b.offset_s = p.offset_s + cut;
                b.fade_in_s = 0.01;
                tx.put(&a)?;
                tx.put(&b)?;
                Ok(json!({ "ids": [a.id, b.id] }))
            }
            Command::PutBucket { bucket } => {
                let mut b = bucket.clone();
                b.keywords.retain(|k| !k.trim().is_empty());
                tx.put(&b)?;
                Ok(json!({ "id": b.id }))
            }
            Command::DeleteBucket { bucket_id } => {
                tx.delete::<SoundBucket>(bucket_id)?;
                Ok(Value::Null)
            }
            Command::BucketKeep {
                bucket_id,
                asset_id,
            } => {
                let mut b = bucket(tx, bucket_id)?;
                let remote = tx
                    .doc()
                    .assets
                    .get(asset_id)
                    .and_then(|a| a.remote_id.clone());
                if !b.kept.contains(asset_id) {
                    b.kept.push(asset_id.clone());
                }
                if let Some(r) = remote {
                    b.previews.retain(|p| p.remote_id != r);
                }
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::BucketRemove {
                bucket_id,
                asset_id,
            } => {
                let mut b = bucket(tx, bucket_id)?;
                b.kept.retain(|a| a != asset_id);
                // a removed vocal stays out when the analysis runs again
                if b.character.is_some() {
                    if let Some(key) = tx
                        .doc()
                        .assets
                        .get(asset_id)
                        .and_then(|a| a.remote_id.clone())
                    {
                        if !b.dismissed.contains(&key) {
                            b.dismissed.push(key);
                        }
                    }
                }
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::BucketDismiss {
                bucket_id,
                remote_id,
            } => {
                let mut b = bucket(tx, bucket_id)?;
                b.previews.retain(|p| &p.remote_id != remote_id);
                if !b.dismissed.contains(remote_id) {
                    b.dismissed.push(remote_id.clone());
                }
                tx.put(&b)?;
                Ok(Value::Null)
            }
            Command::PutMark { mark } => {
                if !tx.doc().sources.contains_key(&mark.source_id) {
                    return Err(CoreError::not_found("source", &mark.source_id));
                }
                tx.put(mark)?;
                Ok(json!({ "mark_id": mark.id }))
            }
            Command::DeleteMark { mark_id } => {
                tx.delete::<DevMark>(mark_id)?;
                Ok(Value::Null)
            }
            Command::ApplyMark { mark_id } => apply_mark(tx, mark_id),
            Command::SetLaneMix { mix } => {
                let mut m = mix.clone();
                m.volume_db = m.volume_db.clamp(-60.0, 12.0);
                m.pan = m.pan.clamp(-1.0, 1.0);
                tx.put(&m)?;
                Ok(Value::Null)
            }
            Command::ApplyMatch {
                sessions,
                sources,
                at,
            } => {
                let keep: HashSet<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
                let stale: Vec<String> = tx
                    .doc()
                    .sessions
                    .keys()
                    .filter(|k| !keep.contains(k.as_str()))
                    .cloned()
                    .collect();
                for id in stale {
                    tx.delete::<Session>(&id)?;
                }
                for s in sessions {
                    tx.put(s)?;
                }
                for s in sources {
                    tx.put(s)?;
                }
                if let Some(at) = at {
                    tx.put(&Setting {
                        id: "match".into(),
                        value: json!({ "at": at }),
                    })?;
                }
                Ok(json!({ "sessions": sessions.len() }))
            }
            Command::DeletePlacement { placement_id } => {
                let p = placement(tx, placement_id)?;
                tx.delete::<Placement>(placement_id)?;
                if let Some(cid) = p.cue_id {
                    if let Some(mut c) = tx.doc().cues.get(&cid).cloned() {
                        c.status = cue_status(tx.doc(), &c);
                        tx.put(&c)?;
                    }
                }
                Ok(Value::Null)
            }
        }
    }
}

fn source(tx: &Tx, id: &str) -> Result<SourceFile> {
    tx.doc()
        .sources
        .get(id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("source", id))
}

fn cue(tx: &Tx, id: &str) -> Result<SoundCue> {
    tx.doc()
        .cues
        .get(id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("cue", id))
}

fn bucket(tx: &Tx, id: &str) -> Result<SoundBucket> {
    tx.doc()
        .buckets
        .get(id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("bucket", id))
}

fn placement(tx: &Tx, id: &str) -> Result<Placement> {
    tx.doc()
        .placements
        .get(id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("placement", id))
}

/// The box of a dialogue line (created on demand).
fn boxed(tx: &Tx, line_id: &str) -> Result<BoxState> {
    let line = tx
        .doc()
        .lines
        .get(line_id)
        .ok_or_else(|| CoreError::not_found("line", line_id))?;
    if line.kind != LineKind::Dialogue {
        return Err(CoreError::invalid(format!(
            "{line_id} is not a dialogue line"
        )));
    }
    Ok(tx.doc().boxes.get(line_id).cloned().unwrap_or(BoxState {
        id: line_id.to_string(),
        ..Default::default()
    }))
}

fn scene_of(tx: &Tx, line_id: &str) -> Result<Scene> {
    let line = tx
        .doc()
        .lines
        .get(line_id)
        .ok_or_else(|| CoreError::not_found("line", line_id))?;
    tx.doc()
        .scenes
        .get(&line.scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", &line.scene_id))
}

fn merge_patch<T: Serialize + serde::de::DeserializeOwned>(cur: &T, patch: &Value) -> Result<T> {
    let mut v = serde_json::to_value(cur)?;
    let (Some(obj), Some(p)) = (v.as_object_mut(), patch.as_object()) else {
        return Err(CoreError::invalid("patch must be a JSON object"));
    };
    for (k, val) in p {
        obj.insert(k.clone(), val.clone());
    }
    Ok(serde_json::from_value(v)?)
}

/// Status from what a cue has: placed > chosen > candidates > wanted.
pub fn cue_status(doc: &Document, c: &SoundCue) -> CueStatus {
    if doc
        .placements
        .values()
        .any(|p| p.cue_id.as_deref() == Some(c.id.as_str()))
    {
        CueStatus::Placed
    } else if c.chosen.is_some() {
        CueStatus::Chosen
    } else if !c.candidates.is_empty() {
        CueStatus::Candidates
    } else {
        CueStatus::Wanted
    }
}

/// Lowest layer of the lane where the placement doesn't overlap another sound.
fn free_layer(doc: &Document, p: &Placement, pacing: &crate::timeline::Pacing) -> u32 {
    let Some(scene) = doc.scenes.get(&p.scene_id) else {
        return p.layer;
    };
    let layout = layout_scene(doc, scene, pacing);
    let start = placement_start(&layout, p);
    let end = start + p.len_s;
    let mut layer = 0;
    loop {
        let clash = doc.placements.values().any(|o| {
            o.id != p.id
                && o.scene_id == p.scene_id
                && o.lane == p.lane
                && o.character == p.character
                && o.layer == layer
                && {
                    let os = placement_start(&layout, o);
                    os < end && start < os + o.len_s
                }
        });
        if !clash {
            return layer;
        }
        layer += 1;
    }
}

fn move_clip(
    tx: &mut Tx,
    line_id: &str,
    delta: f64,
    ripple: bool,
    pacing: &crate::timeline::Pacing,
) -> Result<Value> {
    let scene = scene_of(tx, line_id)?;
    let layout = layout_scene(tx.doc(), &scene, pacing);
    let mut b = boxed(tx, line_id)?;
    if let Some(p) = b.pinned_s {
        b.pinned_s = Some((p + delta).max(0.0));
        tx.put(&b)?;
        return Ok(Value::Null);
    }
    let cur = layout.item(line_id).map(|i| i.gap_before_s).unwrap_or(0.0);
    b.gap_s = Some(((cur + delta) * 1000.0).round() / 1000.0);
    tx.put(&b)?;
    if !ripple {
        // Keep the next box where it was.
        let pos = scene.order.iter().position(|x| x == line_id);
        let next = pos.and_then(|p| {
            scene.order[p + 1..].iter().find(|id| {
                layout
                    .item(id)
                    .map(|i| i.kind == crate::timeline::ItemKind::Box && !i.pieces.is_empty())
                    .unwrap_or(false)
            })
        });
        if let Some(nid) = next {
            let ng = layout.item(nid).map(|i| i.gap_before_s).unwrap_or(0.0);
            let mut nb = boxed(tx, nid)?;
            if nb.pinned_s.is_none() {
                nb.gap_s = Some(((ng - delta) * 1000.0).round() / 1000.0);
                tx.put(&nb)?;
            }
        }
    }
    Ok(Value::Null)
}

fn use_unmatched(
    tx: &mut Tx,
    unmatched_id: &str,
    scene_id: &str,
    index: usize,
    character: Option<String>,
) -> Result<Value> {
    let mut um = tx
        .doc()
        .unmatched
        .get(unmatched_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("unmatched", unmatched_id))?;
    let mut scene = tx
        .doc()
        .scenes
        .get(scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", scene_id))?;
    let src = tx.doc().sources.get(&um.source_id).cloned();
    let character = character
        .or_else(|| src.as_ref().and_then(|s| s.character.clone()))
        .unwrap_or_else(|| "AD-LIB".to_string());
    let tokens = crate::text::display_tokens(&um.text);
    let words: Vec<String> = tokens.iter().map(|t| crate::text::norm_word(t)).collect();
    let line_id = new_id("ln");
    let number = tx
        .doc()
        .lines
        .values()
        .filter(|l| l.scene_id == scene_id)
        .map(|l| l.number)
        .max()
        .unwrap_or(0)
        + 1;
    tx.put(&ScriptLine {
        id: line_id.clone(),
        scene_id: scene_id.to_string(),
        kind: LineKind::Dialogue,
        character: Some(character),
        extension: None,
        text: um.text.clone(),
        paren: None,
        tokens: tokens.clone(),
        words: words.clone(),
        number,
        version_added: 0,
        changed: false,
        forced: false,
        adlib: true,
    })?;
    // Spread word times evenly over the range (good enough for an ad-lib).
    let n = tokens.len().max(1) as f64;
    let span = um.t1 - um.t0;
    let take_words = tokens
        .iter()
        .zip(words.iter())
        .enumerate()
        .map(|(i, (raw, w))| TakeWord {
            w: w.clone(),
            raw: raw.clone(),
            t0: um.t0 + span * i as f64 / n,
            t1: um.t0 + span * (i as f64 + 0.9) / n,
            script_idx: Some(i),
        })
        .collect();
    let take_id = new_id("tk");
    tx.put(&Take {
        id: take_id.clone(),
        line_id: line_id.clone(),
        source_id: um.source_id.clone(),
        take_no: 1,
        take_type: TakeType::Full,
        t0: um.t0,
        t1: um.t1,
        words: take_words,
        score: 0.0,
        heard: um.text.clone(),
        lufs: None,
        db: um.db,
        partial: false,
        pick: 0,
        favorite: false,
        favorite_origin: None,
        direction: String::new(),
        feedback: String::new(),
        direction_raw: String::new(),
        feedback_raw: String::new(),
        tags: vec!["ad-lib".into()],
        note_ids: vec![],
        confidence: 1.0,
        needs_rematch: false,
        ai_flag: None,
        origin: TakeOrigin::Manual,
        read_as: None,
    })?;
    tx.put(&BoxState {
        id: line_id.clone(),
        take_id: Some(take_id),
        ..Default::default()
    })?;
    let at = index.min(scene.order.len());
    scene.order.insert(at, line_id.clone());
    tx.put(&scene)?;
    um.used_as = Some(line_id.clone());
    tx.put(&um)?;
    Ok(json!({ "line_id": line_id }))
}

/// Boxes keep a valid selection, otherwise get the default take. Returns how many were set.
fn refresh_boxes(tx: &mut Tx) -> Result<usize> {
    let after = tx.doc().clone();
    let mut by_line: HashMap<&str, Vec<&Take>> = HashMap::new();
    for t in after.takes.values() {
        by_line.entry(t.line_id.as_str()).or_default().push(t);
    }
    let mut defaults = 0;
    for line in after
        .lines
        .values()
        .filter(|l| l.kind == LineKind::Dialogue)
    {
        let mut b = after.boxes.get(&line.id).cloned().unwrap_or(BoxState {
            id: line.id.clone(),
            ..Default::default()
        });
        let valid = b
            .take_id
            .as_ref()
            .map(|id| after.takes.contains_key(id))
            .unwrap_or(false);
        if !valid {
            let mut takes = by_line.get(line.id.as_str()).cloned().unwrap_or_default();
            takes.sort_by_key(|t| t.take_no);
            b.take_id = default_take(&takes).map(|t| t.id.clone());
            b.comp.clear();
            b.edits = WordEdits::default();
            if b.take_id.is_some() {
                defaults += 1;
            }
        }
        b.comp.retain(|s| after.takes.contains_key(&s.take_id));
        if after.boxes.get(&line.id) != Some(&b) {
            tx.put(&b)?;
        }
    }
    Ok(defaults)
}

fn apply_mark(tx: &mut Tx, mark_id: &str) -> Result<Value> {
    let mut mark = tx
        .doc()
        .marks
        .get(mark_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("mark", mark_id))?;
    let proposal = mark
        .proposal
        .clone()
        .ok_or_else(|| CoreError::invalid("the mark has no proposal to apply"))?;
    let mut lines: HashSet<String> = HashSet::new();
    for id in &proposal.remove {
        if let Some(t) = tx.doc().takes.get(id).cloned() {
            lines.insert(t.line_id.clone());
            tx.delete::<Take>(id)?;
        }
    }
    for t in &proposal.add {
        // the director mic is never a take, whatever proposed it
        match tx.doc().sources.get(&t.source_id) {
            Some(s) if s.role == Role::Character => {}
            _ => {
                return Err(CoreError::invalid(format!(
                    "take {} is not on a character's session mic",
                    t.id
                )))
            }
        }
        if !tx.doc().lines.contains_key(&t.line_id) {
            return Err(CoreError::not_found("line", &t.line_id));
        }
        lines.insert(t.line_id.clone());
        tx.put(t)?;
    }
    for lid in &lines {
        let mut takes: Vec<Take> = tx.doc().takes_of_line(lid).into_iter().cloned().collect();
        takes.sort_by(|a, b| a.t0.total_cmp(&b.t0));
        for (n, t) in takes.iter_mut().enumerate() {
            if t.take_no != (n + 1) as u32 {
                t.take_no = (n + 1) as u32;
                tx.put(t)?;
            }
        }
    }
    refresh_boxes(tx)?;
    mark.status = MarkStatus::Applied;
    tx.put(&mark)?;
    Ok(json!({ "added": proposal.add.len(), "removed": proposal.remove.len() }))
}

/// Default take: strongest director pick, else the last complete take, else the best score.
pub fn default_take<'a>(takes: &[&'a Take]) -> Option<&'a Take> {
    takes.iter().copied().max_by(|a, b| {
        let ka = (a.pick, !a.partial && a.take_type == TakeType::Full);
        let kb = (b.pick, !b.partial && b.take_type == TakeType::Full);
        ka.cmp(&kb)
            .then(a.t0.total_cmp(&b.t0))
            .then(a.score.total_cmp(&b.score))
    })
}

fn apply_analysis(tx: &mut Tx, payload: &AnalysisPayload) -> Result<Value> {
    let doc = tx.doc().clone();
    for a in &payload.assets {
        tx.put(a)?;
    }
    for b in &payload.buckets {
        tx.put(b)?;
    }
    for s in &payload.sessions {
        tx.put(s)?;
    }
    for s in &payload.sources {
        tx.put(s)?;
    }
    let scope: HashSet<&str> = payload.line_scope.iter().map(String::as_str).collect();
    let in_scope = |line_id: &str| scope.is_empty() || scope.contains(line_id);

    // Takes: replace in scope; keep user favorites when the same take id comes back.
    let new_ids: HashSet<&str> = payload.takes.iter().map(|t| t.id.as_str()).collect();
    for t in doc.takes.values() {
        if in_scope(&t.line_id) && !new_ids.contains(t.id.as_str()) {
            if t.favorite && t.favorite_origin.as_deref() == Some("user") {
                // a take starred by hand is never dropped by a re-run; it is flagged
                let mut kept = t.clone();
                kept.needs_rematch = true;
                tx.put(&kept)?;
            } else {
                tx.delete::<Take>(&t.id)?;
            }
        }
    }
    for t in &payload.takes {
        let mut t = t.clone();
        match doc.takes.get(&t.id) {
            Some(old) if old.favorite_origin.as_deref() == Some("user") => {
                t.favorite = old.favorite;
                t.favorite_origin = old.favorite_origin.clone();
            }
            _ => {
                if t.pick >= 2 {
                    t.favorite = true;
                    t.favorite_origin = Some("director".into());
                }
            }
        }
        tx.put(&t)?;
    }

    let sources: HashSet<&str> = payload.source_scope.iter().map(String::as_str).collect();
    let replace_from = |source_id: &str| scope.is_empty() || sources.contains(source_id);
    if scope.is_empty() || !sources.is_empty() {
        for n in doc.notes.values() {
            if replace_from(&n.source_id) {
                tx.delete::<Note>(&n.id)?;
            }
        }
        for u in doc.unmatched.values() {
            if u.origin == "analysis" && u.used_as.is_none() && replace_from(&u.source_id) {
                tx.delete::<Unmatched>(&u.id)?;
            }
        }
    }
    for n in &payload.notes {
        tx.put(n)?;
    }
    for u in &payload.unmatched {
        tx.put(u)?;
    }

    let defaults = refresh_boxes(tx)?;
    Ok(json!({ "takes": payload.takes.len(), "defaults_set": defaults }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::CommandBus;
    use crate::store::EpisodeStore;

    fn setup() -> (tempfile::TempDir, CommandBus) {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::create(&dir.path().join("t.cutaway")).unwrap();
        let mut bus = CommandBus::new(store, "test").unwrap();
        let (doc, scene) = crate::timeline::tests::doc2();
        bus.execute_fn("seed", &Value::Null, |tx| {
            for l in doc.lines.values() {
                let mut l = l.clone();
                l.scene_id = scene.id.clone();
                tx.put(&l)?;
            }
            for t in doc.takes.values() {
                tx.put(t)?;
            }
            for b in doc.boxes.values() {
                tx.put(b)?;
            }
            for s in doc.spacers.values() {
                tx.put(s)?;
            }
            tx.put(&scene)?;
            Ok(Value::Null)
        })
        .unwrap();
        (dir, bus)
    }

    fn source(id: &str, role: &str) -> SourceFile {
        serde_json::from_value(json!({
            "id": id, "path": format!("{id}.wav"), "original_name": format!("{id}.wav"),
            "stored": "copied", "hash": id, "size": 1, "role": role,
        }))
        .unwrap()
    }

    fn mark_with(add: Vec<Take>, remove: Vec<String>, src: &str) -> DevMark {
        DevMark {
            id: "mk1".into(),
            source_id: src.into(),
            t0: 50.0,
            t1: 52.0,
            kind: MarkKind::MissedTake,
            line_id: Some("b".into()),
            note: "misheard".into(),
            text: String::new(),
            status: MarkStatus::Proposed,
            proposal: Some(MarkProposal {
                add,
                remove,
                summary: vec![],
            }),
            created_at: String::new(),
        }
    }

    #[test]
    fn applying_a_mark_adds_removes_and_renumbers() {
        let (_d, mut bus) = setup();
        add_second_take(&mut bus);
        bus.execute(&Command::AddSources {
            sources: vec![source("src", "character"), source("dir", "room")],
        })
        .unwrap();
        // a rescued take of line b between its two takes, and take a1 goes
        let mut t = crate::timeline::tests::take("tr", "b", 30.0, 31.0, vec![]);
        t.origin = TakeOrigin::Rescue;
        let a_take = bus.doc().takes_of_line("a")[0].id.clone();
        bus.execute(&Command::PutMark {
            mark: mark_with(vec![t], vec![a_take.clone()], "src"),
        })
        .unwrap();
        bus.execute(&Command::ApplyMark {
            mark_id: "mk1".into(),
        })
        .unwrap();
        let doc = bus.doc();
        let b: Vec<(&str, u32)> = doc
            .takes_of_line("b")
            .iter()
            .map(|t| (t.id.as_str(), t.take_no))
            .collect();
        assert!(b.contains(&("tr", 2)), "{b:?}");
        assert!(b.contains(&("tb2", 3)), "{b:?}");
        assert!(!doc.takes.contains_key(&a_take));
        assert_eq!(doc.marks["mk1"].status, MarkStatus::Applied);
        // one undo step takes it all back
        bus.undo().unwrap();
        assert!(bus.doc().takes.contains_key(&a_take));
        assert!(!bus.doc().takes.contains_key("tr"));
    }

    #[test]
    fn a_take_on_the_director_mic_is_refused() {
        let (_d, mut bus) = setup();
        bus.execute(&Command::AddSources {
            sources: vec![source("dir", "room")],
        })
        .unwrap();
        let mut t = crate::timeline::tests::take("td", "b", 30.0, 31.0, vec![]);
        t.source_id = "dir".into();
        bus.execute(&Command::PutMark {
            mark: mark_with(vec![t], vec![], "dir"),
        })
        .unwrap();
        let r = bus.execute(&Command::ApplyMark {
            mark_id: "mk1".into(),
        });
        assert!(r.is_err());
        assert!(!bus.doc().takes.contains_key("td"));
    }

    #[test]
    fn a_rerun_keeps_a_hand_starred_take_it_no_longer_finds() {
        let (_d, mut bus) = setup();
        let id = bus.doc().takes_of_line("a")[0].id.clone();
        let mut t = bus.doc().takes[&id].clone();
        t.favorite = true;
        t.favorite_origin = Some("user".into());
        bus.execute_fn("star", &Value::Null, |tx| {
            tx.put(&t)?;
            Ok(Value::Null)
        })
        .unwrap();
        bus.execute(&Command::ApplyAnalysis {
            payload: AnalysisPayload::default(),
        })
        .unwrap();
        let kept = &bus.doc().takes[&id];
        assert!(kept.needs_rematch);
        assert!(
            bus.doc().takes_of_line("b").is_empty(),
            "unstarred takes go"
        );
    }

    #[test]
    fn scoped_analysis_keeps_other_sources_notes() {
        let (_d, mut bus) = setup();
        let note = |id: &str, src: &str| Note {
            id: id.into(),
            source_id: src.into(),
            t0: 0.0,
            t1: 1.0,
            text: "again".into(),
            class: NoteClass::Direction,
            take_ids: vec![],
            line_id: None,
            summary: String::new(),
            tags: vec![],
        };
        bus.execute(&Command::ApplyAnalysis {
            payload: AnalysisPayload {
                notes: vec![note("n_sara", "dir_sara"), note("n_paul", "dir_paul")],
                ..Default::default()
            },
        })
        .unwrap();
        bus.execute(&Command::ApplyAnalysis {
            payload: AnalysisPayload {
                notes: vec![note("n_sara2", "dir_sara")],
                line_scope: vec!["b".into()],
                source_scope: vec!["dir_sara".into()],
                ..Default::default()
            },
        })
        .unwrap();
        let ids: Vec<&str> = bus.doc().notes.keys().map(String::as_str).collect();
        assert_eq!(ids, ["n_paul", "n_sara2"]);
    }

    fn add_second_take(bus: &mut CommandBus) {
        let mut t = crate::timeline::tests::take("tb2", "b", 40.0, 41.0, vec![]);
        t.take_no = 2;
        bus.execute_fn("seed2", &Value::Null, |tx| {
            tx.put(&t)?;
            Ok(Value::Null)
        })
        .unwrap();
    }

    #[test]
    fn undo_redo_select_take() {
        let (_d, mut bus) = setup();
        add_second_take(&mut bus);
        bus.execute(&Command::StepTake {
            line_id: "b".into(),
            delta: 1,
        })
        .unwrap();
        assert_eq!(bus.doc().boxes["b"].take_id.as_deref(), Some("tb2"));
        bus.undo().unwrap();
        assert_eq!(bus.doc().boxes["b"].take_id.as_deref(), Some("tb"));
        bus.redo().unwrap();
        assert_eq!(bus.doc().boxes["b"].take_id.as_deref(), Some("tb2"));
    }

    #[test]
    fn new_command_discards_redo_tail() {
        let (_d, mut bus) = setup();
        bus.execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(1.0),
        })
        .unwrap();
        bus.execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(2.0),
        })
        .unwrap();
        bus.undo().unwrap();
        assert_eq!(bus.doc().boxes["b"].gap_s, Some(1.0));
        bus.execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(3.0),
        })
        .unwrap();
        assert!(!bus.can_redo().unwrap());
        assert!(bus.redo().unwrap().is_none());
        bus.undo().unwrap();
        assert_eq!(bus.doc().boxes["b"].gap_s, Some(1.0));
    }

    #[test]
    fn undo_survives_reopen_and_matches_disk() {
        let (dir, mut bus) = setup();
        bus.execute(&Command::DeleteWords {
            line_id: "b".into(),
            from: 0,
            to: 0,
        })
        .unwrap();
        let path = dir.path().join("t.cutaway");
        drop(bus);
        let mut bus = CommandBus::new(EpisodeStore::open(&path).unwrap(), "test").unwrap();
        assert_eq!(bus.doc().boxes["b"].edits.deleted, vec![0]);
        bus.undo().unwrap();
        assert!(bus.doc().boxes["b"].edits.deleted.is_empty());
        let on_disk = EpisodeStore::open(&path).unwrap().load_document().unwrap();
        assert_eq!(&on_disk, bus.doc());
    }

    #[test]
    fn second_client_sees_changes_and_undoes_them() {
        let (dir, mut a) = setup();
        let path = dir.path().join("t.cutaway");
        let mut b = CommandBus::new(EpisodeStore::open(&path).unwrap(), "mcp").unwrap();
        b.execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(0.9),
        })
        .unwrap();
        assert!(a.refresh().unwrap());
        assert_eq!(a.doc().boxes["b"].gap_s, Some(0.9));
        a.undo().unwrap();
        assert!(b.refresh().unwrap());
        assert_eq!(b.doc().boxes["b"].gap_s, None);
    }

    #[test]
    fn failed_command_leaves_document_untouched() {
        let (_d, mut bus) = setup();
        let before = bus.doc().clone();
        let err = bus.execute(&Command::SetTrim {
            line_id: "b".into(),
            head_s: 1.5,
            tail_s: 1.0,
        });
        assert!(err.is_err());
        assert_eq!(bus.doc(), &before);
        assert!(bus
            .history(10)
            .unwrap()
            .iter()
            .all(|h| h.name != "set_trim"));
    }

    #[test]
    fn move_clip_without_ripple_keeps_the_next_clip() {
        let (_d, mut bus) = setup();
        let mut extra = crate::timeline::tests::line("c", LineKind::Dialogue, 4, "five");
        extra.scene_id = "sc".into();
        let t = crate::timeline::tests::take("tc", "c", 50.0, 51.0, vec![]);
        bus.execute_fn("seed3", &Value::Null, |tx| {
            tx.put(&extra)?;
            tx.put(&t)?;
            tx.put(&BoxState {
                id: "c".into(),
                take_id: Some("tc".into()),
                ..Default::default()
            })?;
            let mut sc = tx.doc().scenes["sc"].clone();
            sc.order.push("c".into());
            tx.put(&sc)?;
            Ok(Value::Null)
        })
        .unwrap();
        let p = crate::timeline::Pacing::default();
        let sc = bus.doc().scenes["sc"].clone();
        let before = layout_scene(bus.doc(), &sc, &p);
        bus.execute(&Command::MoveClip {
            line_id: "b".into(),
            delta_s: 0.5,
            ripple: false,
        })
        .unwrap();
        let after = layout_scene(bus.doc(), &sc, &p);
        assert!(
            (after.item("b").unwrap().start_s - before.item("b").unwrap().start_s - 0.5).abs()
                < 1e-6
        );
        assert!(
            (after.item("c").unwrap().start_s - before.item("c").unwrap().start_s).abs() < 1e-6
        );
        bus.execute(&Command::MoveClip {
            line_id: "b".into(),
            delta_s: 0.5,
            ripple: true,
        })
        .unwrap();
        let rip = layout_scene(bus.doc(), &sc, &p);
        assert!(
            (rip.item("c").unwrap().start_s - after.item("c").unwrap().start_s - 0.5).abs() < 1e-6
        );
    }

    #[test]
    fn default_take_prefers_pick_then_last_complete() {
        let mut a = crate::timeline::tests::take("a", "l", 1.0, 2.0, vec![]);
        let mut b = crate::timeline::tests::take("b", "l", 3.0, 4.0, vec![]);
        let mut c = crate::timeline::tests::take("c", "l", 5.0, 6.0, vec![]);
        c.partial = true;
        assert_eq!(default_take(&[&a, &b, &c]).unwrap().id, "b");
        a.pick = 2;
        assert_eq!(default_take(&[&a, &b, &c]).unwrap().id, "a");
        b.pick = 2;
        assert_eq!(default_take(&[&a, &b, &c]).unwrap().id, "b");
    }
}
