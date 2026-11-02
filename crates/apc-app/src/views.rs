//! View models for the UI. Read-only: the UI never mutates state directly.

use serde::Serialize;

use apc_core::commands::default_take;
use apc_core::timeline::{layout_scene, placement_start, ItemKind, LayoutPart};
use apc_core::*;

use crate::error::{AppError, Result};
use crate::service::Service;

const PEAKS: usize = 160;

#[derive(Serialize)]
pub struct WordView {
    pub text: String,
    /// Seconds from the take's start.
    pub t0: f64,
    pub t1: f64,
    pub script_idx: Option<usize>,
}

#[derive(Serialize)]
pub struct TakeView {
    pub id: String,
    pub take_no: u32,
    pub take_type: TakeType,
    pub partial: bool,
    pub favorite: bool,
    pub favorite_origin: Option<String>,
    pub pick: u8,
    pub is_default: bool,
    pub direction: String,
    pub feedback: String,
    pub direction_raw: String,
    pub feedback_raw: String,
    pub tags: Vec<String>,
    pub duration: f64,
    pub score: f64,
    pub lufs: Option<f64>,
    pub heard: String,
    pub source_id: String,
    pub source_name: String,
    pub src_in: f64,
    pub src_out: f64,
    /// Source timecode of the take start (seconds since midnight when the file has BWF TC).
    pub tc_in: f64,
    pub words: Vec<WordView>,
    /// Normalized waveform envelope (0..1) across the take.
    pub peaks: Vec<f32>,
    pub needs_rematch: bool,
    pub ai_flag: Option<String>,
}

#[derive(Serialize)]
pub struct LayoutWordView {
    pub idx: usize,
    pub text: String,
    pub take_id: String,
    pub start_s: f64,
    pub end_s: f64,
    pub deleted: bool,
    pub script_idx: Option<usize>,
    pub part: usize,
    pub muted: bool,
}

#[derive(Serialize)]
pub struct PieceView {
    pub take_id: String,
    pub source_id: String,
    pub src_in: f64,
    pub src_out: f64,
    pub start_s: f64,
    pub len_s: f64,
    pub part: usize,
    pub muted: bool,
}

#[derive(Serialize)]
pub struct ItemView {
    pub kind: &'static str,
    pub line_id: String,
    pub number: u32,
    /// Position in the script (for the "Moved" badge).
    pub script_index: usize,
    pub moved: bool,
    pub character: Option<String>,
    pub paren: Option<String>,
    pub text: String,
    pub tokens: Vec<String>,
    pub start_s: f64,
    pub end_s: f64,
    pub gap_before_s: f64,
    pub gap_overridden: bool,
    pub pinned: bool,
    pub edited_in_edit: bool,
    pub take_id: Option<String>,
    pub take_index: Option<usize>,
    pub takes: Vec<TakeView>,
    pub words: Vec<LayoutWordView>,
    pub pieces: Vec<PieceView>,
    /// Parts between splits (one when the line has none), in scene time.
    pub parts: Vec<LayoutPart>,
    /// Every part muted / every part ripple-deleted.
    pub muted: bool,
    pub dropped: bool,
    pub head_s: f64,
    pub tail_s: f64,
    pub comp: Vec<CompSegment>,
    pub edits: WordEdits,
    pub gain_db: f64,
    pub low_confidence: bool,
    pub adlib: bool,
    // spacers
    pub spacer_enabled: bool,
    pub spacer_duration_s: Option<f64>,
}

#[derive(Serialize)]
pub struct SceneSummary {
    pub id: String,
    pub number: u32,
    pub heading: String,
    pub place: String,
    pub time_of_day: String,
    pub lines: usize,
    pub lines_with_takes: usize,
    pub flagged: usize,
}

#[derive(Serialize)]
pub struct CharacterView {
    pub id: String,
    pub label: String,
    pub color_index: usize,
}

#[derive(Serialize)]
pub struct UnmatchedView {
    pub id: String,
    pub text: String,
    pub t0: f64,
    pub t1: f64,
    pub source_id: String,
    pub origin: String,
    pub used: bool,
}

#[derive(Serialize)]
pub struct SceneView {
    pub episode: String,
    pub scene: SceneSummary,
    pub location_id: String,
    pub tightness: f64,
    pub room_tone: RoomTone,
    pub duration_s: f64,
    pub items: Vec<ItemView>,
    pub unmatched: Vec<UnmatchedView>,
    pub scenes: Vec<SceneSummary>,
    pub characters: Vec<CharacterView>,
    pub rev: i64,
    pub can_undo: bool,
    pub can_redo: bool,
    pub breath_gap_s: f64,
}

fn flagged_in(doc: &Document, scene_id: &str) -> usize {
    doc.lines_of_scene(scene_id)
        .iter()
        .filter(|l| {
            let ts = doc.takes_of_line(&l.id);
            ts.iter()
                .any(|t| t.needs_rematch || t.ai_flag.is_some() || t.confidence < 0.8)
                || l.changed
        })
        .count()
}

pub fn scene_summaries(doc: &Document) -> Vec<SceneSummary> {
    doc.scenes_in_order()
        .iter()
        .map(|s| {
            let lines: Vec<&ScriptLine> = doc
                .lines_of_scene(&s.id)
                .into_iter()
                .filter(|l| l.kind == LineKind::Dialogue)
                .collect();
            SceneSummary {
                id: s.id.clone(),
                number: s.number,
                heading: s.heading.clone(),
                place: s.place.clone(),
                time_of_day: s.time_of_day.clone(),
                lines: lines.len(),
                lines_with_takes: lines
                    .iter()
                    .filter(|l| !doc.takes_of_line(&l.id).is_empty())
                    .count(),
                flagged: flagged_in(doc, &s.id),
            }
        })
        .collect()
}

fn take_view(svc: &mut Service, t: &Take, is_default: bool) -> TakeView {
    let src = svc.doc().sources.get(&t.source_id).cloned();
    let peaks = svc
        .analysis_of(&t.source_id)
        .map(|a| {
            let raw = a.peaks_range(t.t0, t.t1, PEAKS);
            let global = a
                .peaks
                .iter()
                .map(|(lo, hi)| lo.abs().max(hi.abs()))
                .fold(1e-4f32, f32::max);
            raw.into_iter()
                .map(|(lo, hi)| (lo.abs().max(hi.abs()) / global).clamp(0.0, 1.0))
                .collect()
        })
        .unwrap_or_default();
    TakeView {
        id: t.id.clone(),
        take_no: t.take_no,
        take_type: t.take_type,
        partial: t.partial,
        favorite: t.favorite,
        favorite_origin: t.favorite_origin.clone(),
        pick: t.pick,
        is_default,
        direction: t.direction.clone(),
        feedback: t.feedback.clone(),
        direction_raw: t.direction_raw.clone(),
        feedback_raw: t.feedback_raw.clone(),
        tags: t.tags.clone(),
        duration: t.duration(),
        score: t.score,
        lufs: t.lufs,
        heard: t.heard.clone(),
        source_id: t.source_id.clone(),
        source_name: src
            .as_ref()
            .map(|s| s.original_name.clone())
            .unwrap_or_default(),
        src_in: t.t0,
        src_out: t.t1,
        tc_in: src.and_then(|s| s.tc_start_s).unwrap_or(0.0) + t.t0,
        words: t
            .words
            .iter()
            .map(|w| WordView {
                text: w.raw.clone(),
                t0: w.t0 - t.t0,
                t1: w.t1 - t.t0,
                script_idx: w.script_idx,
            })
            .collect(),
        peaks,
        needs_rematch: t.needs_rematch,
        ai_flag: t.ai_flag.clone(),
    }
}

pub fn scene_view(svc: &mut Service, scene_key: &str) -> Result<SceneView> {
    svc.bus.refresh()?;
    let doc = svc.doc().clone();
    let scene = doc
        .find_scene(scene_key)
        .cloned()
        .or_else(|| doc.scenes_in_order().first().map(|s| (*s).clone()))
        .ok_or_else(|| AppError::Params("the project has no scenes yet".into()))?;
    let cfg = svc.config();
    let layout = layout_scene(&doc, &scene, &cfg.pacing());
    let script_order: Vec<String> = doc
        .lines_of_scene(&scene.id)
        .iter()
        .map(|l| l.id.clone())
        .collect();
    let mut items = Vec::new();
    let mut prev_script_idx: Option<usize> = None;
    for li in &layout.items {
        let Some(line) = doc.lines.get(&li.item_id) else {
            continue;
        };
        let script_index = script_order
            .iter()
            .position(|x| x == &line.id)
            .unwrap_or(usize::MAX);
        let moved = !line.adlib && prev_script_idx.map(|p| script_index < p).unwrap_or(false);
        if !line.adlib {
            prev_script_idx = Some(script_index);
        }
        let b = doc.boxes.get(&line.id).cloned().unwrap_or_default();
        let takes = doc.takes_of_line(&line.id);
        let def = default_take(&takes).map(|t| t.id.clone());
        let takes_v: Vec<TakeView> = takes
            .iter()
            .map(|t| take_view(svc, t, Some(&t.id) == def.as_ref()))
            .collect();
        let take_index = b
            .take_id
            .as_ref()
            .and_then(|id| takes.iter().position(|t| &t.id == id));
        let sp = doc.spacers.get(&line.id);
        items.push(ItemView {
            kind: if li.kind == ItemKind::Box {
                "line"
            } else {
                "spacer"
            },
            line_id: line.id.clone(),
            number: line.number,
            script_index,
            moved,
            character: line.character.clone(),
            paren: line.paren.clone(),
            text: line.text.clone(),
            tokens: line.tokens.clone(),
            start_s: li.start_s,
            end_s: li.end_s,
            gap_before_s: li.gap_before_s,
            gap_overridden: li.gap_overridden,
            pinned: li.pinned,
            edited_in_edit: b.edited_in_edit(),
            take_id: b.take_id.clone(),
            take_index,
            low_confidence: take_index
                .map(|i| takes[i].confidence < 0.8)
                .unwrap_or(false),
            takes: takes_v,
            words: li
                .words
                .iter()
                .map(|w| LayoutWordView {
                    idx: w.idx,
                    text: w.text.clone(),
                    take_id: w.take_id.clone(),
                    start_s: w.start_s,
                    end_s: w.end_s,
                    deleted: w.deleted,
                    script_idx: w.script_idx,
                    part: w.part,
                    muted: w.muted,
                })
                .collect(),
            pieces: li
                .pieces
                .iter()
                .map(|p| PieceView {
                    take_id: p.take_id.clone(),
                    source_id: p.source_id.clone(),
                    src_in: p.src_in,
                    src_out: p.src_out,
                    start_s: p.start_s,
                    len_s: p.len(),
                    part: p.part,
                    muted: p.muted,
                })
                .collect(),
            parts: li.parts.clone(),
            muted: li.muted,
            dropped: li.dropped,
            head_s: b.head_s,
            tail_s: b.tail_s,
            comp: b.comp.clone(),
            edits: b.edits.clone(),
            gain_db: b.gain_db,
            adlib: line.adlib,
            spacer_enabled: sp.map(|s| s.enabled).unwrap_or(false),
            spacer_duration_s: sp.and_then(|s| s.duration_s),
        });
    }
    let loc_id = apc_core::script::location_id(&scene.location);
    let mut chars: Vec<CharacterView> = doc
        .characters
        .values()
        .map(|c| CharacterView {
            id: c.id.clone(),
            label: c.label.clone(),
            color_index: c.color_index,
        })
        .collect();
    chars.sort_by_key(|c| c.color_index);
    let summaries = scene_summaries(&doc);
    let this = summaries
        .iter()
        .find(|s| s.id == scene.id)
        .map(|s| SceneSummary {
            id: s.id.clone(),
            number: s.number,
            heading: s.heading.clone(),
            place: s.place.clone(),
            time_of_day: s.time_of_day.clone(),
            lines: s.lines,
            lines_with_takes: s.lines_with_takes,
            flagged: s.flagged,
        })
        .ok_or_else(|| AppError::Other("scene vanished".into()))?;
    Ok(SceneView {
        episode: svc.name(),
        scene: this,
        location_id: loc_id.clone(),
        tightness: scene.tightness,
        room_tone: doc
            .locations
            .get(&loc_id)
            .map(|l| l.room_tone.clone())
            .unwrap_or_default(),
        duration_s: layout.duration_s,
        items,
        unmatched: doc
            .unmatched
            .values()
            .filter(|u| u.scene_id.as_deref() == Some(scene.id.as_str()))
            .map(|u| UnmatchedView {
                id: u.id.clone(),
                text: u.text.clone(),
                t0: u.t0,
                t1: u.t1,
                source_id: u.source_id.clone(),
                origin: u.origin.clone(),
                used: u.used_as.is_some(),
            })
            .collect(),
        scenes: summaries,
        characters: chars,
        rev: svc.bus.rev(),
        can_undo: svc.bus.can_undo()?,
        can_redo: svc.bus.can_redo()?,
        breath_gap_s: cfg.pacing.breath_gap_s,
    })
}

/// A sound's display name from its library file: no folder, extension, download id
/// ("(Epidemic 1000)") or duplicate counter ("(2)").
pub fn sound_name(file: &str) -> String {
    let base = file.rsplit('/').next().unwrap_or(file);
    let mut n = base
        .rsplit_once('.')
        .map(|(n, _)| n)
        .unwrap_or(base)
        .trim()
        .to_string();
    while let Some(open) = n.rfind(" (") {
        let tail = &n[open + 2..];
        let drop = tail.ends_with(')')
            && (tail.starts_with("Epidemic ")
                || tail[..tail.len() - 1].chars().all(|c| c.is_ascii_digit()));
        if !drop {
            break;
        }
        n.truncate(open);
    }
    n
}

#[derive(Serialize)]
pub struct PlacementView {
    pub id: String,
    pub asset_id: String,
    pub name: String,
    pub lane: Lane,
    /// Whose dialogue lane (`lane: dialogue`).
    pub character: Option<String>,
    pub layer: u32,
    pub start_s: f64,
    pub len_s: f64,
    pub anchored: bool,
    pub anchor: Option<Anchor>,
    /// Seconds after the anchor (or after `at_s` when pinned).
    pub offset_s: f64,
    pub cue_id: Option<String>,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    pub fade_curve: FadeCurve,
    pub gain_db: f64,
    pub muted: bool,
    /// Asset in point and the asset's length (trim limits).
    pub in_s: f64,
    pub asset_len_s: f64,
    pub peaks: Vec<f32>,
    /// Where the sound came from ("Epidemic Sound", "ElevenLabs", "Library").
    pub origin: String,
}

#[derive(Serialize)]
pub struct CueView {
    pub cue: SoundCue,
    pub anchor_text: Option<String>,
    pub candidates: Vec<Asset>,
}

#[derive(Serialize)]
pub struct EditView {
    pub scene_id: String,
    pub duration_s: f64,
    pub placements: Vec<PlacementView>,
    pub cues: Vec<CueView>,
    /// Volume, pan, mute and solo per lane (dialogue lanes by character id; `sfx1`,
    /// `ambience`, `music`, `roomtone`).
    pub lane_mix: Vec<LaneMix>,
    pub room_tone: Option<RoomToneBed>,
}

/// The scene's room tone loop: where it comes from and where the loop seams fall.
#[derive(Serialize)]
pub struct RoomToneBed {
    pub source: String,
    pub loop_s: f64,
    pub gain_db: f64,
    pub seams: Vec<f64>,
}

pub fn edit_view(svc: &mut Service, scene_key: &str) -> Result<EditView> {
    svc.bus.refresh()?;
    let doc = svc.doc().clone();
    let scene = doc
        .find_scene(scene_key)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no scene '{scene_key}'")))?;
    let layout = layout_scene(&doc, &scene, &svc.config().pacing());
    let mut placements = Vec::new();
    for p in doc.placements.values().filter(|p| p.scene_id == scene.id) {
        let peaks = svc
            .analysis_of(&format!("asset:{}", p.asset_id))
            .map(|a| {
                let g = a
                    .peaks
                    .iter()
                    .map(|(lo, hi)| lo.abs().max(hi.abs()))
                    .fold(1e-4f32, f32::max);
                a.peaks_range(p.in_s, p.in_s + p.len_s, 80)
                    .into_iter()
                    .map(|(lo, hi)| lo.abs().max(hi.abs()) / g)
                    .collect()
            })
            .unwrap_or_default();
        placements.push(PlacementView {
            id: p.id.clone(),
            asset_id: p.asset_id.clone(),
            name: doc
                .assets
                .get(&p.asset_id)
                .map(|a| sound_name(&a.file))
                .unwrap_or_default(),
            lane: p.lane,
            character: p.character.clone(),
            layer: p.layer,
            start_s: placement_start(&layout, p),
            len_s: p.len_s,
            anchored: p.anchor.is_some(),
            anchor: p.anchor.clone(),
            offset_s: p.offset_s,
            cue_id: p.cue_id.clone(),
            fade_in_s: p.fade_in_s,
            fade_out_s: p.fade_out_s,
            fade_curve: p.fade_curve,
            gain_db: p.gain_db,
            muted: p.muted,
            in_s: p.in_s,
            asset_len_s: doc
                .assets
                .get(&p.asset_id)
                .map(|a| a.duration_s)
                .unwrap_or(p.in_s + p.len_s),
            peaks,
            origin: doc
                .assets
                .get(&p.asset_id)
                .map(|a| match (a.source, a.provider.as_deref()) {
                    (_, Some("epidemic")) => "Epidemic Sound".to_string(),
                    (_, Some("elevenlabs")) => "ElevenLabs".to_string(),
                    (AssetSource::Generated, _) => "Generated".to_string(),
                    (AssetSource::Recorded, _) => "Recording".to_string(),
                    _ => "Library".to_string(),
                })
                .unwrap_or_default(),
        });
    }
    placements.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    let room_tone = doc
        .locations
        .get(&apc_core::script::location_id(&scene.location))
        .map(|l| &l.room_tone)
        .filter(|t| t.enabled && t.t1 > t.t0 + 0.05)
        .map(|t| {
            let period = t.t1 - t.t0;
            let n = (layout.duration_s / period).floor() as usize;
            RoomToneBed {
                source: t
                    .source_id
                    .as_ref()
                    .and_then(|s| doc.sources.get(s))
                    .map(|s| s.original_name.clone())
                    .or_else(|| {
                        t.asset_id
                            .as_ref()
                            .and_then(|a| doc.assets.get(a))
                            .map(|a| a.file.clone())
                    })
                    .unwrap_or_default(),
                loop_s: period,
                gain_db: t.gain_db,
                seams: (1..=n.min(500)).map(|k| k as f64 * period).collect(),
            }
        });
    Ok(EditView {
        scene_id: scene.id.clone(),
        duration_s: layout.duration_s,
        placements,
        cues: bucket(&doc, Some(&scene.id)),
        lane_mix: doc.lane_mix.values().cloned().collect(),
        room_tone,
    })
}

pub fn bucket(doc: &Document, scene_id: Option<&str>) -> Vec<CueView> {
    let mut cues: Vec<&SoundCue> = doc
        .cues
        .values()
        .filter(|c| scene_id.map(|s| c.scene_id == s).unwrap_or(true))
        .collect();
    let pos = |c: &SoundCue| {
        c.anchor
            .as_ref()
            .and_then(|a| doc.lines.get(&a.line_id))
            .map(|l| {
                (
                    doc.scenes.get(&l.scene_id).map(|s| s.number).unwrap_or(0),
                    l.number,
                )
            })
            .unwrap_or((u32::MAX, 0))
    };
    cues.sort_by_key(|c| pos(c));
    cues.into_iter()
        .map(|c| CueView {
            cue: c.clone(),
            anchor_text: c
                .anchor
                .as_ref()
                .and_then(|a| doc.lines.get(&a.line_id))
                .map(|l| l.text.clone()),
            candidates: c
                .candidates
                .iter()
                .filter_map(|id| doc.assets.get(id).cloned())
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn sound_names_drop_folders_ids_and_counters() {
        assert_eq!(
            super::sound_name("SFX/Misc/Foil Crinkle Close (Epidemic 1000).mp3"),
            "Foil Crinkle Close"
        );
        assert_eq!(
            super::sound_name("SFX/Doors/Door_close (2).wav"),
            "Door_close"
        );
        assert_eq!(super::sound_name("Hum (live).wav"), "Hum (live)");
    }
}
