//! The cut as one list of clips in episode (or scene) time, shared by every export format.

use serde::{Deserialize, Serialize};

use apc_core::config::ProjectConfig;
use apc_core::timeline::{layout_scene, placement_start, SceneLayout};
use apc_core::{Document, Lane, LineKind, Scene};

/// Pause between scenes when several are exported as one timeline.
pub const SCENE_GAP_S: f64 = 2.0;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ExportScope {
    All,
    Scenes(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TimelineClip {
    pub scene_id: String,
    pub scene_number: u32,
    /// Line (dialogue) or placement id.
    pub owner: String,
    /// `S1.3`
    pub line_ref: String,
    pub lane: String,
    pub character: Option<String>,
    pub take_no: Option<u32>,
    /// Part of a comp/edit (1-based) and the number of parts.
    pub part: (usize, usize),
    pub source_id: String,
    pub src_in: f64,
    pub src_out: f64,
    pub rec_in: f64,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    pub gain_db: f64,
    pub direction: String,
    pub feedback: String,
    pub text: String,
    /// Clip mute: exported but disabled.
    #[serde(default)]
    pub muted: bool,
}

impl TimelineClip {
    pub fn len(&self) -> f64 {
        (self.src_out - self.src_in).max(0.0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() <= 0.0
    }
    pub fn rec_out(&self) -> f64 {
        self.rec_in + self.len()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SceneMark {
    pub scene_id: String,
    pub number: u32,
    pub heading: String,
    pub start_s: f64,
    pub end_s: f64,
}

/// A spoken line for subtitles: words in timeline time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cue {
    pub start_s: f64,
    pub end_s: f64,
    pub character: Option<String>,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct EpisodeTimeline {
    pub clips: Vec<TimelineClip>,
    pub scenes: Vec<SceneMark>,
    pub cues: Vec<Cue>,
    pub duration_s: f64,
}

fn scenes_in_scope<'a>(doc: &'a Document, scope: &ExportScope) -> Vec<&'a Scene> {
    let all = doc.scenes_in_order();
    match scope {
        ExportScope::All => all,
        ExportScope::Scenes(keys) => all
            .into_iter()
            .filter(|s| {
                keys.iter()
                    .any(|k| doc.find_scene(k).map(|f| f.id == s.id).unwrap_or(false))
            })
            .collect(),
    }
}

impl EpisodeTimeline {
    pub fn build(doc: &Document, scope: &ExportScope) -> Self {
        let cfg = ProjectConfig::from_doc(doc);
        let pacing = cfg.pacing();
        let mut tl = EpisodeTimeline::default();
        let mut offset = 0.0;
        for scene in scenes_in_scope(doc, scope) {
            let layout = layout_scene(doc, scene, &pacing);
            if layout.duration_s <= 0.0 {
                continue;
            }
            tl.add_scene(doc, scene, &layout, offset);
            offset += layout.duration_s + SCENE_GAP_S;
        }
        tl.duration_s = (offset - SCENE_GAP_S).max(0.0);
        tl
    }

    fn add_scene(&mut self, doc: &Document, scene: &Scene, layout: &SceneLayout, offset: f64) {
        let mut scene_end = offset;
        for item in &layout.items {
            let Some(line) = doc.lines.get(&item.item_id) else {
                continue;
            };
            if line.kind != LineKind::Dialogue || item.pieces.is_empty() {
                continue;
            }
            let take = item.take_id.as_ref().and_then(|t| doc.takes.get(t));
            let n = item.pieces.len();
            for (k, p) in item.pieces.iter().enumerate() {
                let piece_take = doc.takes.get(&p.take_id);
                self.clips.push(TimelineClip {
                    scene_id: scene.id.clone(),
                    scene_number: scene.number,
                    owner: line.id.clone(),
                    line_ref: format!("S{}.{}", scene.number, line.number),
                    lane: line.character.clone().unwrap_or_else(|| "DIALOGUE".into()),
                    character: line.character.clone(),
                    take_no: piece_take.map(|t| t.take_no),
                    part: (k + 1, n),
                    source_id: p.source_id.clone(),
                    src_in: p.src_in,
                    src_out: p.src_out,
                    rec_in: offset + p.start_s,
                    fade_in_s: p.fade_in_s,
                    fade_out_s: p.fade_out_s,
                    gain_db: p.gain_db,
                    direction: take.map(|t| t.direction.clone()).unwrap_or_default(),
                    feedback: take.map(|t| t.feedback.clone()).unwrap_or_default(),
                    text: line.text.clone(),
                    muted: p.muted,
                });
                scene_end = scene_end.max(offset + p.end_s());
            }
            // muted words are not heard, so they get no subtitle
            let words: Vec<&apc_core::timeline::LayoutWord> = item
                .words
                .iter()
                .filter(|w| !w.deleted && !w.muted)
                .collect();
            if let (Some(f), Some(l)) = (words.first(), words.last()) {
                self.cues.push(Cue {
                    start_s: offset + f.start_s,
                    end_s: offset + l.end_s.max(f.start_s + 0.3),
                    character: line.character.clone(),
                    text: words
                        .iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
            } else if !item.muted && !item.words.iter().any(|w| w.muted) {
                self.cues.push(Cue {
                    start_s: offset + item.start_s,
                    end_s: offset + item.end_s,
                    character: line.character.clone(),
                    text: line.text.clone(),
                });
            }
        }
        for p in doc.placements.values().filter(|p| p.scene_id == scene.id) {
            let start = placement_start(layout, p);
            let lane = match p.lane {
                Lane::Sfx => format!("SFX {}", p.layer + 1),
                Lane::Ambience => "AMBIENCE".into(),
                Lane::Music => "MUSIC".into(),
                // vocals go on the character's own track
                Lane::Dialogue => p.character.clone().unwrap_or_else(|| "DIALOGUE".into()),
            };
            let desc = p
                .cue_id
                .as_ref()
                .and_then(|c| doc.cues.get(c))
                .map(|c| c.description.clone())
                .unwrap_or_default();
            self.clips.push(TimelineClip {
                scene_id: scene.id.clone(),
                scene_number: scene.number,
                owner: p.id.clone(),
                line_ref: format!("S{}", scene.number),
                lane,
                character: p.character.clone().filter(|_| p.lane == Lane::Dialogue),
                take_no: None,
                part: (1, 1),
                source_id: format!("asset:{}", p.asset_id),
                src_in: p.in_s,
                src_out: p.in_s + p.len_s,
                rec_in: offset + start,
                fade_in_s: p.fade_in_s,
                fade_out_s: p.fade_out_s,
                gain_db: p.gain_db,
                direction: String::new(),
                feedback: String::new(),
                text: desc,
                muted: p.muted,
            });
        }
        self.scenes.push(SceneMark {
            scene_id: scene.id.clone(),
            number: scene.number,
            heading: scene.heading.clone(),
            start_s: offset,
            end_s: scene_end.max(offset + layout.duration_s),
        });
    }

    /// Lane names in a stable order: characters (script order), then sound lanes.
    pub fn lanes(&self, doc: &Document) -> Vec<String> {
        let mut chars: Vec<&apc_core::Character> = doc.characters.values().collect();
        chars.sort_by_key(|c| c.color_index);
        let mut out: Vec<String> = chars
            .iter()
            .map(|c| c.id.clone())
            .filter(|id| self.clips.iter().any(|c| &c.lane == id))
            .collect();
        let mut rest: Vec<String> = self
            .clips
            .iter()
            .map(|c| c.lane.clone())
            .filter(|l| !out.contains(l))
            .collect();
        rest.sort();
        rest.dedup();
        out.extend(rest);
        out
    }
}
