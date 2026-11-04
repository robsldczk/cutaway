//! Shaping placed sounds and setting rooms from the outside: the parameters of the service
//! methods (CLI and MCP) and what they read back. The UI sends the same commands directly.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use apc_audio::RoomShape;
use apc_core::commands::ClipEdge;
use apc_core::timeline::{layout_scene, placement_start};
use apc_core::*;

use crate::error::{AppError, Result};
use crate::methods::SceneToneP;
use crate::service::Service;

#[derive(Deserialize, JsonSchema)]
pub struct PlacementP {
    pub placement_id: String,
}

/// Where to: an anchor (`anchor_line_id`), free scene time (`at_s`), or only `offset_s` from
/// where the sound hangs now.
#[derive(Deserialize, JsonSchema)]
pub struct MoveSoundP {
    pub placement_id: String,
    /// Anchor to this line: the sound follows it when takes, gaps or the order change.
    #[serde(default)]
    pub anchor_line_id: Option<String>,
    /// Anchor to this word of the line (script word index).
    #[serde(default)]
    pub anchor_word: Option<usize>,
    /// `start` or `end` of the line or word. The pause after a word is that word's end.
    #[serde(default)]
    pub anchor_edge: Edge,
    /// Free scene time in seconds; drops any anchor.
    #[serde(default)]
    pub at_s: Option<f64>,
    /// Seconds after the anchor or time.
    #[serde(default)]
    pub offset_s: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TrimSoundP {
    pub placement_id: String,
    /// Which end moves: `start` or `end`.
    pub edge: ClipEdge,
    /// How far, in seconds (positive moves the edge later). Stops at the source's ends.
    pub delta_s: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct SoundFadesP {
    pub placement_id: String,
    #[serde(default)]
    pub fade_in_s: Option<f64>,
    /// Turns the automatic fade out off.
    #[serde(default)]
    pub fade_out_s: Option<f64>,
    /// true: the fade out follows the length again (min(0.3 s, 30 %), 2 s for beds over 8 s).
    #[serde(default)]
    pub auto: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SoundGainP {
    pub placement_id: String,
    /// −24 … +6 dB.
    pub gain_db: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct SoundPanP {
    pub placement_id: String,
    /// −1 left … +1 right.
    pub pan: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct SoundDepthP {
    pub placement_id: String,
    /// 0 close … 1 far.
    pub depth: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct SlipSoundP {
    pub placement_id: String,
    /// New source in point in seconds. Place and length stay.
    pub in_s: f64,
}

/// One edit of a batch.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SoundEdit {
    Move(MoveSoundP),
    Trim(TrimSoundP),
    Fades(SoundFadesP),
    Gain(SoundGainP),
    Pan(SoundPanP),
    Depth(SoundDepthP),
    Slip(SlipSoundP),
    Remove(PlacementP),
}

#[derive(Deserialize, JsonSchema)]
pub struct BatchSoundsP {
    /// Applied in order, as one undo step; if one fails, none applies.
    pub edits: Vec<SoundEdit>,
}

/// Settings left out stay as they are.
#[derive(Deserialize, JsonSchema)]
pub struct SceneRoomP {
    /// Scene id, number ("2") or "S2".
    pub scene: String,
    /// 0 small … 1 large (decay time and pre-delay).
    #[serde(default)]
    pub size: Option<f64>,
    /// 0 bright … 1 dark.
    #[serde(default)]
    pub damping: Option<f64>,
    /// 0 dry … 1 full: overall level of the room.
    #[serde(default)]
    pub wet: Option<f64>,
}

impl From<MoveSoundP> for Command {
    fn from(p: MoveSoundP) -> Command {
        Command::MoveSound {
            placement_id: p.placement_id,
            anchor: p.anchor_line_id.map(|line_id| Anchor {
                line_id,
                word: p.anchor_word,
                edge: p.anchor_edge,
            }),
            at_s: p.at_s,
            offset_s: p.offset_s,
        }
    }
}

impl From<TrimSoundP> for Command {
    fn from(p: TrimSoundP) -> Command {
        Command::TrimPlacement {
            placement_id: p.placement_id,
            edge: p.edge,
            delta_s: p.delta_s,
        }
    }
}

impl From<SoundFadesP> for Command {
    fn from(p: SoundFadesP) -> Command {
        Command::SetSoundFades {
            placement_id: p.placement_id,
            fade_in_s: p.fade_in_s,
            fade_out_s: p.fade_out_s,
            auto: p.auto,
        }
    }
}

impl From<SoundGainP> for Command {
    fn from(p: SoundGainP) -> Command {
        Command::SetSoundGain {
            placement_id: p.placement_id,
            gain_db: p.gain_db,
        }
    }
}

impl From<SoundPanP> for Command {
    fn from(p: SoundPanP) -> Command {
        Command::SetSoundPan {
            placement_id: p.placement_id,
            pan: p.pan,
        }
    }
}

impl From<SoundDepthP> for Command {
    fn from(p: SoundDepthP) -> Command {
        Command::SetSoundDepth {
            placement_id: p.placement_id,
            depth: p.depth,
        }
    }
}

impl From<SlipSoundP> for Command {
    fn from(p: SlipSoundP) -> Command {
        Command::SlipSound {
            placement_id: p.placement_id,
            in_s: p.in_s,
        }
    }
}

impl From<PlacementP> for Command {
    fn from(p: PlacementP) -> Command {
        Command::DeletePlacement {
            placement_id: p.placement_id,
        }
    }
}

impl From<SoundEdit> for Command {
    fn from(e: SoundEdit) -> Command {
        match e {
            SoundEdit::Move(p) => p.into(),
            SoundEdit::Trim(p) => p.into(),
            SoundEdit::Fades(p) => p.into(),
            SoundEdit::Gain(p) => p.into(),
            SoundEdit::Pan(p) => p.into(),
            SoundEdit::Depth(p) => p.into(),
            SoundEdit::Slip(p) => p.into(),
            SoundEdit::Remove(p) => p.into(),
        }
    }
}

/// Several sounds in one undo step (a multi-select move, gain or depth change).
pub fn batch(svc: &mut Service, p: BatchSoundsP) -> Result<Value> {
    if p.edits.is_empty() {
        return Err(AppError::Params("no edits given".into()));
    }
    let commands = p.edits.into_iter().map(Command::from).collect();
    Ok(svc.exec(&Command::Batch { commands })?.result)
}

pub fn set_room(svc: &mut Service, p: SceneRoomP) -> Result<Value> {
    let scene = svc
        .doc()
        .find_scene(&p.scene)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no scene '{}'", p.scene)))?;
    let room = SceneRoom {
        size: p.size.unwrap_or(scene.room.size),
        damping: p.damping.unwrap_or(scene.room.damping),
        wet: p.wet.unwrap_or(scene.room.wet),
    };
    let r = svc.exec(&Command::SetSceneRoom {
        scene_id: scene.id,
        room,
    })?;
    Ok(r.result)
}

/// A scene's room as the Sounds tab shows it.
#[derive(Serialize)]
pub struct RoomView {
    pub scene_id: String,
    pub number: u32,
    pub heading: String,
    pub place: String,
    pub room: SceneRoom,
    /// What the settings come to, from the same curves the audio uses.
    pub decay_s: f64,
    pub predelay_ms: f64,
    /// The ambience the scene plays, its own or what its place shares.
    pub tone: Option<ToneView>,
    /// It's set on this scene.
    pub own_tone: bool,
    /// Scenes of one location are one room.
    pub location_id: String,
    pub night: bool,
    /// Without one it plays its location's, if that has any.
    pub place_tone: bool,
    /// Sounds of the scene that play in this room.
    pub sounds: usize,
}

#[derive(Serialize)]
pub struct ToneView {
    pub asset_id: String,
    pub name: String,
    pub gain_db: f64,
}

/// The cast with their wet settings, in cast order.
pub fn voices(svc: &mut Service) -> Result<Value> {
    svc.bus.refresh()?;
    let mut cast: Vec<&Character> = svc.doc().characters.values().collect();
    cast.sort_by_key(|c| c.color_index);
    Ok(serde_json::json!(cast
        .iter()
        .map(|c| serde_json::json!({
            "id": c.id, "label": c.label, "color_index": c.color_index, "wet": c.wet,
        }))
        .collect::<Vec<_>>()))
}

/// Sets a scene's room tone from a library sound, which joins the episode for it.
pub fn set_tone(svc: &mut Service, p: SceneToneP) -> Result<Value> {
    let scene = svc
        .doc()
        .find_scene(&p.scene)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no scene '{}'", p.scene)))?;
    let tone = match p.asset_id {
        Some(id) => {
            let a = crate::sound::ensure_asset_in_doc(svc, &id)?;
            let was = svc
                .doc()
                .room_tone_for(&scene)
                .filter(|t| t.asset_id.is_some())
                .map(|t| t.gain_db);
            Some(SceneTone {
                asset_id: a.id,
                gain_db: p.gain_db.or(was).unwrap_or(apc_core::DEFAULT_TONE_DB),
            })
        }
        None => None,
    };
    let r = svc.exec(&Command::SetRoomAmbience {
        scene_id: scene.id,
        night: p.night,
        tone,
    })?;
    Ok(r.result)
}

pub fn rooms(svc: &mut Service) -> Result<Vec<RoomView>> {
    svc.bus.refresh()?;
    let doc = svc.doc();
    Ok(doc
        .scenes_in_order()
        .into_iter()
        .map(|s| {
            let shape = RoomShape::new(s.room.size, s.room.damping, s.room.wet);
            RoomView {
                scene_id: s.id.clone(),
                number: s.number,
                heading: s.heading.clone(),
                place: s.place.clone(),
                room: s.room,
                decay_s: shape.decay_s,
                predelay_ms: shape.predelay_s * 1000.0,
                tone: doc.room_tone_for(s).and_then(|t| {
                    let a = doc.assets.get(t.asset_id.as_ref()?)?;
                    Some(ToneView {
                        asset_id: a.id.clone(),
                        name: crate::views::sound_name(&a.file),
                        gain_db: t.gain_db,
                    })
                }),
                own_tone: s.tone.is_some(),
                location_id: apc_core::script::location_id(&s.location),
                night: s.is_night(),
                place_tone: doc
                    .room_tone_of(&apc_core::script::location_id(&s.location))
                    .is_some_and(|t| t.enabled),
                sounds: doc
                    .placements
                    .values()
                    .filter(|p| p.scene_id == s.id && p.in_room())
                    .count(),
            }
        })
        .collect())
}

/// A placed sound with everything the commands can change.
#[derive(Serialize)]
pub struct SoundView {
    pub id: String,
    pub file: String,
    pub lane: Lane,
    pub layer: u32,
    /// Where it plays now, in scene seconds.
    pub start_s: f64,
    pub anchor: Option<Anchor>,
    pub offset_s: f64,
    pub len_s: f64,
    pub in_s: f64,
    /// Length of the file, 0 when unknown.
    pub source_len_s: f64,
    pub gain_db: f64,
    pub pan: f64,
    pub depth: f64,
    /// False for music and vocal sounds: they play outside the room and depth does nothing.
    pub in_room: bool,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    pub auto_fade: bool,
    pub looped: bool,
    pub muted: bool,
}

pub fn list(svc: &mut Service, scene_key: &str) -> Result<Vec<SoundView>> {
    svc.bus.refresh()?;
    let pacing = svc.config().pacing();
    let doc = svc.doc();
    let scene = doc
        .find_scene(scene_key)
        .ok_or_else(|| AppError::Params(format!("no scene '{scene_key}'")))?;
    let layout = layout_scene(doc, scene, &pacing);
    let mut out: Vec<SoundView> = doc
        .placements
        .values()
        .filter(|p| p.scene_id == scene.id)
        .map(|p| {
            let asset = doc.assets.get(&p.asset_id);
            SoundView {
                id: p.id.clone(),
                file: asset.map(|a| a.file.clone()).unwrap_or_default(),
                lane: p.lane,
                layer: p.layer,
                start_s: placement_start(&layout, p),
                anchor: p.anchor.clone(),
                offset_s: p.offset_s,
                len_s: p.len_s,
                in_s: p.in_s,
                source_len_s: asset.map(|a| a.duration_s).unwrap_or(0.0),
                gain_db: p.gain_db,
                pan: p.pan,
                depth: p.depth,
                in_room: p.in_room(),
                fade_in_s: p.fade_in_s,
                fade_out_s: p.fade_out_s,
                auto_fade: p.auto_fade,
                looped: p.looped,
                muted: p.muted,
            }
        })
        .collect();
    out.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    Ok(out)
}
