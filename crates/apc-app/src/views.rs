//! View models for the UI. Read-only: the UI never mutates state directly.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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
    /// Length of the take's source file (how far a clip can be lengthened).
    pub source_len: f64,
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

/// One step of the room strip next to a line, in recording order.
#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoomItem {
    /// What the room said between two takes (or before a session's first, after its last).
    Talk {
        key: String,
        summary: String,
        raw: String,
    },
    Take {
        key: String,
        /// This line's take ("Take 2"), or another line's ("SARA · Where were…").
        label: String,
        own: bool,
    },
    /// Takes of other lines left out between two stretches.
    Break { key: String },
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
    /// The line it runs next to, and how far after that line's start.
    pub beside: Option<Beside>,
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
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    pub head_s: f64,
    pub tail_s: f64,
    pub comp: Vec<CompSegment>,
    pub edits: WordEdits,
    pub gain_db: f64,
    /// How far back in the room the line is spoken, 0 … 1.
    pub depth: f64,
    pub low_confidence: bool,
    pub adlib: bool,
    /// The line's takes and one take either side, with the room talk between them.
    pub room: Vec<RoomItem>,
    // spacers
    pub spacer_enabled: bool,
    pub spacer_duration_s: Option<f64>,
    /// How long it takes: that, or the default for its length.
    pub spacer_s: f64,
}

#[derive(Serialize, Clone)]
pub struct SceneSummary {
    pub id: String,
    pub number: u32,
    pub heading: String,
    pub place: String,
    pub time_of_day: String,
    pub lines: usize,
    pub lines_with_takes: usize,
    pub flagged: usize,
    /// Beat title from the writers (empty for plain Fountain).
    pub title: String,
    pub act: String,
    pub synopsis: String,
    /// Delivered: its timing holds still.
    pub lock: Option<SceneLock>,
    /// How far along the editor has marked it.
    pub status: Option<SceneStatus>,
    /// Review notes on it that nobody has resolved.
    pub open_notes: usize,
}

#[derive(Serialize)]
pub struct CharacterView {
    pub id: String,
    pub label: String,
    pub color_index: usize,
    /// The writers' notes in one line (empty without a bio).
    pub profile: String,
}

/// Where a character of the scene stands (the pan pop-out on Arrange).
#[derive(Serialize)]
pub struct VoicePanView {
    pub character: String,
    pub label: String,
    pub color_index: usize,
    /// What's heard in this scene.
    pub pan: f64,
    /// The value of the room, which a scene inherits.
    pub room: f64,
    /// This scene has a value of its own.
    pub custom: bool,
}

/// Story context of the open scene (Inspector).
#[derive(Serialize)]
pub struct SceneStoryView {
    pub title: String,
    pub act: String,
    pub synopsis: String,
    /// Characters present, by label.
    pub cast: Vec<String>,
    pub tone: Option<f64>,
    pub setting: String,
    pub setting_description: String,
    /// Name of the setting this one belongs to.
    pub setting_parent: Option<String>,
    pub parent_description: String,
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
    /// The scene's room wet: at 0 a line moved back has no room to go into.
    pub room_wet: f64,
    pub duration_s: f64,
    pub items: Vec<ItemView>,
    pub unmatched: Vec<UnmatchedView>,
    pub scenes: Vec<SceneSummary>,
    pub characters: Vec<CharacterView>,
    /// The characters who speak in this scene and where they stand.
    pub pans: Vec<VoicePanView>,
    /// The scene's open review notes, where they are on the cut now.
    pub notes: Vec<crate::review::NoteMark>,
    pub story: SceneStoryView,
    pub rev: i64,
    pub can_undo: bool,
    pub can_redo: bool,
    pub breath_gap_s: f64,
}

/// The characters with a line in a scene, in cast order, and where they stand.
pub fn voice_pans(doc: &Document, scene: &Scene) -> Vec<VoicePanView> {
    let speaking: std::collections::BTreeSet<&str> = doc
        .lines_of_scene(&scene.id)
        .into_iter()
        .filter(|l| l.kind == LineKind::Dialogue && !l.removed)
        .filter_map(|l| l.character.as_deref())
        .collect();
    let mut out: Vec<VoicePanView> = speaking
        .into_iter()
        .filter_map(|id| doc.characters.get(id))
        .map(|c| VoicePanView {
            character: c.id.clone(),
            label: c.label.clone(),
            color_index: c.color_index,
            pan: scene.pan_of(&c.id),
            room: scene.room_pan.get(&c.id).copied().unwrap_or(0.0),
            custom: scene.voice_pan.contains_key(&c.id),
        })
        .collect();
    out.sort_by_key(|p| p.color_index);
    out
}

fn flagged_in(doc: &Document, scene_id: &str) -> usize {
    doc.lines_of_scene(scene_id)
        .iter()
        .filter(|l| !l.removed)
        .filter(|l| {
            let ts = doc.takes_of_line(&l.id);
            ts.iter()
                .any(|t| t.needs_rematch || t.ai_flag.is_some() || t.confidence < 0.8)
                || l.changed
        })
        .count()
}

pub fn scene_summaries(doc: &Document) -> Vec<SceneSummary> {
    let notes = crate::review::counts(doc);
    doc.scenes_in_order()
        .iter()
        .map(|s| {
            let lines: Vec<&ScriptLine> = doc
                .lines_of_scene(&s.id)
                .into_iter()
                .filter(|l| l.kind == LineKind::Dialogue && !l.removed)
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
                title: s.story.title.clone(),
                act: s.story.act.clone(),
                synopsis: s.story.synopsis.clone(),
                lock: s.lock.clone(),
                status: s.status,
                open_notes: notes.get(&s.id).map(|c| c.1).unwrap_or(0),
            }
        })
        .collect()
}

/// Every take in recording order, per session (session time = file time − sync offset).
struct RecordingOrder<'a> {
    sessions: Vec<Vec<&'a Take>>,
    /// Take id → (session, position).
    at: HashMap<&'a str, (usize, usize)>,
}

impl<'a> RecordingOrder<'a> {
    fn of(doc: &'a Document) -> Self {
        let mut by: BTreeMap<Option<String>, Vec<(f64, &Take)>> = BTreeMap::new();
        for t in doc.takes.values() {
            let src = doc.sources.get(&t.source_id);
            let off = src.map(|s| s.offset_s).unwrap_or(0.0);
            by.entry(src.and_then(|s| s.session_id.clone()))
                .or_default()
                .push((t.t0 - off, t));
        }
        let mut sessions = Vec::new();
        let mut at = HashMap::new();
        for mut list in by.into_values() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
            for (i, (_, t)) in list.iter().enumerate() {
                at.insert(t.id.as_str(), (sessions.len(), i));
            }
            sessions.push(list.into_iter().map(|(_, t)| t).collect());
        }
        RecordingOrder { sessions, at }
    }

    /// The room strip of a line: its takes and their neighbours in recording order, with
    /// the talk between them. A gap's note is stored on both takes around it; the take
    /// after it (its direction) is read first.
    fn strip(&self, doc: &Document, own: &[&Take]) -> Vec<RoomItem> {
        let mut want: BTreeSet<(usize, usize)> = BTreeSet::new();
        for t in own {
            if let Some(&(s, i)) = self.at.get(t.id.as_str()) {
                for j in i.saturating_sub(1)..=(i + 1).min(self.sessions[s].len() - 1) {
                    want.insert((s, j));
                }
            }
        }
        let own_ids: HashSet<&str> = own.iter().map(|t| t.id.as_str()).collect();
        let id_at = |s: usize, i: Option<usize>| {
            i.and_then(|i| self.sessions[s].get(i))
                .map(|t| t.id.as_str())
                .unwrap_or("")
        };
        let talk = |s: usize, i: usize, summary: &str, raw: &str| RoomItem::Talk {
            key: format!("{}>{}", id_at(s, i.checked_sub(1)), id_at(s, Some(i))),
            summary: summary.to_string(),
            raw: raw.to_string(),
        };
        let mut out = Vec::new();
        let mut last: Option<(usize, usize)> = None;
        for &(s, i) in &want {
            let t = self.sessions[s][i];
            if i > 0 && last == Some((s, i - 1)) {
                out.push(talk(s, i, &t.direction, &t.direction_raw));
            } else {
                if let Some((ls, li)) = last {
                    let lt = self.sessions[ls][li];
                    out.push(talk(ls, li + 1, &lt.feedback, &lt.feedback_raw));
                    out.push(RoomItem::Break {
                        key: format!("br-{}", t.id),
                    });
                }
                out.push(talk(s, i, &t.direction, &t.direction_raw));
            }
            let label = if own_ids.contains(t.id.as_str()) {
                format!("Take {}", t.take_no)
            } else {
                take_label(doc, t)
            };
            out.push(RoomItem::Take {
                key: t.id.clone(),
                label,
                own: own_ids.contains(t.id.as_str()),
            });
            last = Some((s, i));
        }
        if let Some((ls, li)) = last {
            let lt = self.sessions[ls][li];
            out.push(talk(ls, li + 1, &lt.feedback, &lt.feedback_raw));
        }
        out.retain(|r| !matches!(r, RoomItem::Talk { summary, raw, .. } if summary.is_empty() && raw.is_empty()));
        out
    }
}

/// Another line's take in the room strip: who, and the start of the line.
fn take_label(doc: &Document, t: &Take) -> String {
    let Some(line) = doc.lines.get(&t.line_id) else {
        return format!("Take {}", t.take_no);
    };
    let who = line
        .character
        .as_ref()
        .map(|c| doc.characters.get(c).map_or(c, |x| &x.label).clone())
        .unwrap_or_default();
    let words: Vec<&str> = line.text.split_whitespace().collect();
    let mut text = words.iter().take(4).copied().collect::<Vec<_>>().join(" ");
    if words.len() > 4 {
        text.push('…');
    }
    if who.is_empty() {
        text
    } else {
        format!("{who} · {text}")
    }
}

fn take_view(svc: &mut Service, t: &Take, is_default: bool) -> TakeView {
    let src = svc.doc().sources.get(&t.source_id).cloned();
    let src_len = src.as_ref().map(|s| s.duration_s).unwrap_or(0.0);
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
        source_len: src_len,
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
    let order = RecordingOrder::of(&doc);
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
            beside: li.beside.clone(),
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
            fade_in_s: b.fade_in_s,
            fade_out_s: b.fade_out_s,
            head_s: b.head_s,
            tail_s: b.tail_s,
            comp: b.comp.clone(),
            edits: b.edits.clone(),
            gain_db: b.gain_db,
            depth: b.depth,
            adlib: line.adlib,
            room: order.strip(&doc, &takes),
            spacer_enabled: sp.map(|s| s.enabled).unwrap_or(false),
            spacer_duration_s: sp.and_then(|s| s.duration_s),
            spacer_s: sp
                .and_then(|s| s.duration_s)
                .unwrap_or_else(|| apc_core::timeline::default_spacer_s(line.words.len())),
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
            profile: c.profile.one_line(),
        })
        .collect();
    chars.sort_by_key(|c| c.color_index);
    let summaries = scene_summaries(&doc);
    let this = summaries
        .iter()
        .find(|s| s.id == scene.id)
        .cloned()
        .ok_or_else(|| AppError::Other("scene vanished".into()))?;
    let loc = doc.locations.get(&loc_id);
    let parent = loc
        .and_then(|l| l.parent.as_deref())
        .and_then(|p| doc.locations.get(p));
    let story = SceneStoryView {
        title: scene.story.title.clone(),
        act: scene.story.act.clone(),
        synopsis: scene.story.synopsis.clone(),
        cast: scene
            .story
            .cast
            .iter()
            .map(|c| {
                apc_core::story::character_for(&doc, c)
                    .and_then(|id| doc.characters.get(&id))
                    .map(|ch| ch.label.clone())
                    .unwrap_or_else(|| apc_core::script::title_case(c))
            })
            .collect(),
        tone: scene.story.tone,
        setting: scene.location.clone(),
        setting_description: loc.map(|l| l.description.clone()).unwrap_or_default(),
        setting_parent: parent.map(|p| p.name.clone()),
        parent_description: parent.map(|p| p.description.clone()).unwrap_or_default(),
    };
    Ok(SceneView {
        story,
        episode: svc.name(),
        scene: this,
        location_id: loc_id.clone(),
        tightness: scene.tightness,
        room_wet: scene.room.wet,
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
        pans: voice_pans(&doc, &scene),
        notes: crate::review::marks(&doc, &layout),
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
    /// Its glue is turned off: it stays at its time, also when moved.
    pub unglued: bool,
    /// Seconds after the anchor (or after `at_s` when pinned).
    pub offset_s: f64,
    pub cue_id: Option<String>,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    pub fade_curve: FadeCurve,
    pub gain_db: f64,
    pub muted: bool,
    pub pan: f64,
    /// 0 close … 1 far.
    pub depth: f64,
    pub auto_fade: bool,
    /// Order the sounds were added in (0 for sounds from before it was kept).
    pub added: u64,
    /// Asset in point and the asset's length (trim limits).
    pub in_s: f64,
    pub asset_len_s: f64,
    pub looped: bool,
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
    /// The scene's own room tone (its sound); `None` when it's the location's.
    pub own: Option<String>,
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
            unglued: p.unglued,
            offset_s: p.offset_s,
            cue_id: p.cue_id.clone(),
            fade_in_s: p.fade_in_s,
            fade_out_s: p.fade_out_s,
            fade_curve: p.fade_curve,
            gain_db: p.gain_db,
            muted: p.muted,
            pan: p.pan,
            depth: p.depth,
            auto_fade: p.auto_fade,
            added: p.added,
            looped: p.looped,
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
        .room_tone_for(&scene)
        .filter(|t| t.enabled && t.t1 > t.t0 + 0.05)
        .map(|t| {
            let points = apc_audio::mixer::loop_points(t.t1 - t.t0);
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
                loop_s: points.period_s,
                own: t.asset_id.clone(),
                gain_db: t.gain_db,
                seams: (0..500)
                    .map(|k| points.first_s + k as f64 * points.period_s)
                    .take_while(|t| *t < layout.duration_s)
                    .collect(),
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
    use super::*;

    fn take(id: &str, line: &str, source: &str, t0: f64, direction: &str, feedback: &str) -> Take {
        let mut t: Take = serde_json::from_value(serde_json::json!({
            "id": id, "line_id": line, "source_id": source, "take_no": 1,
            "take_type": "full", "t0": t0, "t1": t0 + 3.0,
        }))
        .unwrap();
        t.direction = direction.into();
        t.feedback = feedback.into();
        t
    }

    fn source(id: &str, offset_s: f64) -> SourceFile {
        serde_json::from_value(serde_json::json!({
            "id": id, "path": id, "original_name": id, "stored": "referenced", "hash": id,
            "size": 0, "role": "character", "session_id": "s1", "offset_s": offset_s,
        }))
        .unwrap()
    }

    fn kinds(strip: &[RoomItem]) -> Vec<String> {
        strip
            .iter()
            .map(|r| match r {
                RoomItem::Talk { summary, .. } => format!("~{summary}"),
                RoomItem::Take { key, own, .. } => format!("{key}{}", if *own { "*" } else { "" }),
                RoomItem::Break { .. } => "|".into(),
            })
            .collect()
    }

    #[test]
    fn room_strip_follows_the_recording_with_shared_gaps() {
        let mut doc = Document::default();
        for s in [source("paul", 0.0), source("sara", 1.0)] {
            doc.sources.insert(s.id.clone(), s);
        }
        // L1 is taken twice, with other lines around it; gap notes sit on both sides
        for t in [
            take("a", "L0", "paul", 0.0, "", "g1"),
            take("b", "L1", "paul", 10.0, "g1", "g2"),
            take("c", "L2", "sara", 21.0, "g2", "g3"),
            take("d", "L3", "paul", 30.0, "g3", "g4"),
            take("e", "L2", "sara", 41.0, "g4", "g5"),
            take("f", "L1", "paul", 50.0, "g5", ""),
        ] {
            doc.takes.insert(t.id.clone(), t);
        }
        let order = RecordingOrder::of(&doc);
        let own: Vec<&Take> = vec![&doc.takes["b"], &doc.takes["f"]];
        assert_eq!(
            kinds(&order.strip(&doc, &own)),
            vec!["a", "~g1", "b*", "~g2", "c", "~g3", "|", "~g4", "e", "~g5", "f*"]
        );
        // keys are the gap itself, whichever take a strip reaches it from
        let keys = |own: Vec<&Take>| -> Vec<String> {
            order
                .strip(&doc, &own)
                .into_iter()
                .filter_map(|r| match r {
                    RoomItem::Talk { key, .. } => Some(key),
                    _ => None,
                })
                .collect()
        };
        assert!(keys(vec![&doc.takes["b"]]).contains(&"b>c".to_string()));
        assert!(keys(vec![&doc.takes["c"]]).contains(&"b>c".to_string()));
    }

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
