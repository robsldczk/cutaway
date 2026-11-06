//! The rules of a placed sound and the commands that shape it: move and anchor, fades, gain,
//! pan, depth, slip, and the room of a scene. One gesture is one command; several sounds at
//! once are a `Command::Batch`, so a multi-select change is one undo step too.
//!
//! Every value is absolute, not a delta from the last mouse move: a UI previews locally and
//! commits once on release, and running a command twice lands in the same place.

use serde_json::{json, Value};

use crate::bus::Tx;
use crate::commands::{free_layer, ms, placement};
use crate::error::{CoreError, Result};
use crate::model::*;
use crate::timeline::{
    glue_point, layout_scene, placement_start, resolve_anchor, Pacing, SceneLayout,
};

/// Loudness range of a sound. The Edit page's gain tool reaches further (D77).
pub const SOUND_GAIN_MIN_DB: f64 = -24.0;
pub const SOUND_GAIN_MAX_DB: f64 = 6.0;
/// Shortest sound the length rules leave.
pub const MIN_SOUND_S: f64 = 0.01;
/// A sound dropped from a bucket keeps its length up to here …
pub const DROP_SHORT_S: f64 = 6.0;
/// … and past it is cropped to this, or loops as ambience if it's set to (D85).
pub const DROP_CROP_S: f64 = 10.0;
/// Room tone level range.
const TONE_GAIN_MIN_DB: f64 = -60.0;
const TONE_GAIN_MAX_DB: f64 = 12.0;

/// A value from outside; `NaN` and infinity would poison the document and the mix.
pub(crate) fn finite(name: &str, x: f64) -> Result<f64> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(CoreError::invalid(format!("{name} must be a number")))
    }
}

/// Length of the sound's file, when known.
fn source_len(doc: &Document, p: &Placement) -> Option<f64> {
    doc.assets
        .get(&p.asset_id)
        .map(|a| a.duration_s)
        .filter(|d| *d > 0.0)
}

/// Puts a sound back inside its rules after any change: pan and depth in range, the length
/// within the source, fades within the length, the automatic fade following the length.
pub fn settle(doc: &Document, p: &mut Placement) {
    p.pan = p.pan.clamp(-1.0, 1.0);
    p.depth = p.depth.clamp(0.0, 1.0);
    p.in_s = p.in_s.max(0.0);
    if let Some(src) = source_len(doc, p) {
        p.in_s = p.in_s.min((src - MIN_SOUND_S).max(0.0));
        if !p.looped {
            p.len_s = p.len_s.min(src - p.in_s);
        }
    }
    p.len_s = p.len_s.max(MIN_SOUND_S);
    if p.auto_fade {
        p.fade_out_s = Placement::auto_fade_out_s(p.len_s);
    }
    p.fade_out_s = p.fade_out_s.clamp(0.0, p.len_s);
    p.fade_in_s = p.fade_in_s.clamp(0.0, p.len_s - p.fade_out_s);
}

/// The next value of `Placement::added`.
pub(crate) fn next_added(doc: &Document) -> u64 {
    doc.placements.values().map(|p| p.added).max().unwrap_or(0) + 1
}

/// Checks a sound about to be placed and puts it inside the rules.
pub(crate) fn check_new(doc: &Document, p: &mut Placement) -> Result<()> {
    for (name, x) in [
        ("at_s", p.at_s),
        ("offset_s", p.offset_s),
        ("in_s", p.in_s),
        ("len_s", p.len_s),
        ("fade_in_s", p.fade_in_s),
        ("fade_out_s", p.fade_out_s),
        ("gain_db", p.gain_db),
        ("pan", p.pan),
        ("depth", p.depth),
    ] {
        finite(name, x)?;
    }
    p.gain_db = p.gain_db.clamp(SOUND_GAIN_MIN_DB, SOUND_GAIN_MAX_DB);
    p.added = next_added(doc);
    settle(doc, p);
    Ok(())
}

/// An anchor must point at a line of the sound's own scene and, with a word, at a word of it.
fn check_anchor(doc: &Document, p: &Placement, a: &Anchor) -> Result<()> {
    let line = doc
        .lines
        .get(&a.line_id)
        .ok_or_else(|| CoreError::not_found("line", &a.line_id))?;
    if line.scene_id != p.scene_id {
        return Err(CoreError::invalid(format!(
            "{} is not in the sound's scene",
            a.line_id
        )));
    }
    if let Some(w) = a.word {
        if w >= line.words.len() {
            return Err(CoreError::invalid(format!("{} has no word {w}", a.line_id)));
        }
    }
    Ok(())
}

/// A sound that follows the dialogue when it's put down. Beds don't: ambience and music run under
/// the scene, and a loop lasts to its end. Glue turned off by hand stays off.
pub fn takes_glue(p: &Placement) -> bool {
    !p.unglued && !p.looped && matches!(p.lane, Lane::Sfx | Lane::Dialogue)
}

/// Hangs a sound on the moment `t`, exactly: the offset is what's left after the anchor. Says
/// whether there was anything to hang on.
pub fn glue_to(p: &mut Placement, layout: &SceneLayout, t: f64) -> bool {
    let Some((anchor, offset)) = glue_point(layout, t) else {
        return false;
    };
    // the fallback if the line ever leaves the timeline: stay about here
    p.at_s = ms(t - offset);
    p.offset_s = offset;
    p.anchor = Some(anchor);
    true
}

/// Turns a sound's glue on, where it stands, or off, which leaves it at its time for good.
pub(crate) fn set_glue(
    tx: &mut Tx,
    placement_id: &str,
    glued: bool,
    pacing: &Pacing,
) -> Result<Value> {
    let mut p = placement(tx, placement_id)?;
    let scene = tx
        .doc()
        .scenes
        .get(&p.scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", &p.scene_id))?;
    let layout = layout_scene(tx.doc(), &scene, pacing);
    let start = placement_start(&layout, &p);
    p.unglued = !glued;
    if !glued {
        p.anchor = None;
        p.at_s = start;
        p.offset_s = 0.0;
    } else if p.anchor.is_none() && !glue_to(&mut p, &layout, start) {
        return Err(CoreError::invalid(
            "this scene has nothing to glue a sound to",
        ));
    }
    tx.put(&p)?;
    Ok(json!({ "glued": p.anchor.is_some(), "start_s": start }))
}

pub(crate) fn move_sound(
    tx: &mut Tx,
    placement_id: &str,
    anchor: Option<&Anchor>,
    at_s: Option<f64>,
    offset_s: Option<f64>,
    glue: bool,
    pacing: &Pacing,
) -> Result<Value> {
    let mut p = placement(tx, placement_id)?;
    let scene = tx
        .doc()
        .scenes
        .get(&p.scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", &p.scene_id))?;
    let layout = layout_scene(tx.doc(), &scene, pacing);
    let offset = offset_s.map(|o| finite("offset_s", o)).transpose()?;
    match (anchor, at_s) {
        (Some(_), Some(_)) => {
            return Err(CoreError::invalid("give an anchor or a time, not both"));
        }
        (Some(a), None) => {
            check_anchor(tx.doc(), &p, a)?;
            // the fallback if the line ever leaves the timeline: stay about here
            if let Some(t) = resolve_anchor(&layout, a) {
                p.at_s = ms(t);
            }
            p.anchor = Some(a.clone());
            p.offset_s = ms(offset.unwrap_or(0.0));
            p.unglued = false;
        }
        (None, Some(t)) => {
            p.anchor = None;
            p.at_s = ms(finite("at_s", t)?.max(0.0));
            p.offset_s = ms(offset.unwrap_or(0.0));
            if glue && takes_glue(&p) {
                let at = p.at_s + p.offset_s;
                glue_to(&mut p, &layout, at);
            }
        }
        (None, None) => {
            let Some(o) = offset else {
                return Err(CoreError::invalid("say where to: anchor, at_s or offset_s"));
            };
            p.offset_s = ms(o);
        }
    }
    p.layer = free_layer(tx.doc(), &p, pacing);
    let start = placement_start(&layout, &p);
    tx.put(&p)?;
    Ok(json!({ "start_s": start, "layer": p.layer }))
}

pub(crate) fn set_fades(
    tx: &mut Tx,
    placement_id: &str,
    fade_in_s: Option<f64>,
    fade_out_s: Option<f64>,
    auto: Option<bool>,
) -> Result<Value> {
    if fade_in_s.is_none() && fade_out_s.is_none() && auto.is_none() {
        return Err(CoreError::invalid(
            "say which fade: fade_in_s, fade_out_s or auto",
        ));
    }
    let mut p = placement(tx, placement_id)?;
    if let Some(f) = fade_in_s {
        p.fade_in_s = ms(finite("fade_in_s", f)?.max(0.0));
    }
    if let Some(f) = fade_out_s {
        p.fade_out_s = ms(finite("fade_out_s", f)?.max(0.0));
        p.auto_fade = false;
        // a fade set by hand wins over the other one if they would overlap
        p.fade_in_s = p.fade_in_s.min((p.len_s - p.fade_out_s).max(0.0));
    }
    if let Some(a) = auto {
        p.auto_fade = a;
    }
    settle(tx.doc(), &mut p);
    tx.put(&p)?;
    Ok(json!({
        "fade_in_s": p.fade_in_s, "fade_out_s": p.fade_out_s, "auto": p.auto_fade,
    }))
}

pub(crate) fn set_gain(tx: &mut Tx, placement_id: &str, gain_db: f64) -> Result<Value> {
    let mut p = placement(tx, placement_id)?;
    p.gain_db = finite("gain_db", gain_db)?.clamp(SOUND_GAIN_MIN_DB, SOUND_GAIN_MAX_DB);
    tx.put(&p)?;
    Ok(json!({ "gain_db": p.gain_db }))
}

pub(crate) fn set_pan(tx: &mut Tx, placement_id: &str, pan: f64) -> Result<Value> {
    let mut p = placement(tx, placement_id)?;
    p.pan = finite("pan", pan)?.clamp(-1.0, 1.0);
    tx.put(&p)?;
    Ok(json!({ "pan": p.pan }))
}

pub(crate) fn set_depth(tx: &mut Tx, placement_id: &str, depth: f64) -> Result<Value> {
    let mut p = placement(tx, placement_id)?;
    p.depth = finite("depth", depth)?.clamp(0.0, 1.0);
    tx.put(&p)?;
    // music and vocals keep the value but play outside the room: say so to CLI and MCP
    Ok(json!({ "depth": p.depth, "in_room": p.in_room() }))
}

pub(crate) fn slip(tx: &mut Tx, placement_id: &str, in_s: f64) -> Result<Value> {
    let mut p = placement(tx, placement_id)?;
    let last = source_len(tx.doc(), &p)
        .map(|src| (src - if p.looped { MIN_SOUND_S } else { p.len_s }).max(0.0))
        .unwrap_or(f64::MAX);
    p.in_s = ms(finite("in_s", in_s)?.clamp(0.0, last));
    tx.put(&p)?;
    Ok(json!({ "in_s": p.in_s }))
}

pub(crate) fn set_room(tx: &mut Tx, scene_id: &str, room: &SceneRoom) -> Result<Value> {
    let mut scene = tx
        .doc()
        .scenes
        .get(scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", scene_id))?;
    scene.room = SceneRoom {
        size: finite("size", room.size)?.clamp(0.0, 1.0),
        damping: finite("damping", room.damping)?.clamp(0.0, 1.0),
        wet: finite("wet", room.wet)?.clamp(0.0, 1.0),
    };
    // a place is one room, however many scenes happen in it (D97)
    for mut other in same_place(tx, &scene) {
        other.room = scene.room;
        tx.put(&other)?;
    }
    tx.put(&scene)?;
    Ok(serde_json::to_value(scene.room)?)
}

/// A character's pan: this scene's own, or with `room` the value of its whole room, which the
/// other scenes of the location inherit. `None` takes the value away. A scene's own value stays
/// when the room's changes: that's what makes it its own.
pub(crate) fn set_voice_pan(
    tx: &mut Tx,
    scene_id: &str,
    character: &str,
    pan: Option<f64>,
    room: bool,
) -> Result<Value> {
    if !tx.doc().characters.contains_key(character) {
        return Err(CoreError::not_found("character", character));
    }
    let mut scene = tx
        .doc()
        .scenes
        .get(scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", scene_id))?;
    let pan = pan
        .map(|p| finite("pan", p).map(|p| p.clamp(-1.0, 1.0)))
        .transpose()?;
    let write = |values: &mut std::collections::BTreeMap<String, f64>| match pan {
        Some(p) => {
            values.insert(character.to_string(), p);
        }
        None => {
            values.remove(character);
        }
    };
    if room {
        for mut other in same_place(tx, &scene) {
            write(&mut other.room_pan);
            tx.put(&other)?;
        }
        write(&mut scene.room_pan);
    } else {
        write(&mut scene.voice_pan);
    }
    tx.put(&scene)?;
    Ok(json!({ "pan": scene.pan_of(character) }))
}

/// The other scenes of a scene's location.
fn same_place(tx: &Tx, scene: &Scene) -> Vec<Scene> {
    tx.doc()
        .scenes
        .values()
        .filter(|s| s.id != scene.id && s.location == scene.location)
        .cloned()
        .collect()
}

pub(crate) fn set_ambience(
    tx: &mut Tx,
    scene_id: &str,
    night: Option<bool>,
    tone: &Option<SceneTone>,
) -> Result<Value> {
    let scene = tx
        .doc()
        .scenes
        .get(scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", scene_id))?;
    let tone = checked_tone(tx, tone)?;
    let mut scenes = same_place(tx, &scene);
    scenes.push(scene);
    for mut s in scenes {
        if night.is_none_or(|n| s.is_night() == n) {
            s.tone = tone.clone();
            tx.put(&s)?;
        }
    }
    Ok(serde_json::to_value(&tone)?)
}

fn checked_tone(tx: &Tx, tone: &Option<SceneTone>) -> Result<Option<SceneTone>> {
    let Some(t) = tone else { return Ok(None) };
    if !tx.doc().assets.contains_key(&t.asset_id) {
        return Err(CoreError::not_found("asset", &t.asset_id));
    }
    Ok(Some(SceneTone {
        asset_id: t.asset_id.clone(),
        gain_db: finite("gain_db", t.gain_db)?.clamp(TONE_GAIN_MIN_DB, TONE_GAIN_MAX_DB),
    }))
}

pub(crate) fn set_voice_wet(tx: &mut Tx, character_id: &str, wet: f64) -> Result<Value> {
    let mut c = tx
        .doc()
        .characters
        .get(character_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("character", character_id))?;
    c.wet = finite("wet", wet)?.clamp(0.0, 1.0);
    tx.put(&c)?;
    Ok(json!({ "wet": c.wet }))
}

pub(crate) fn set_tone(tx: &mut Tx, scene_id: &str, tone: &Option<SceneTone>) -> Result<Value> {
    let mut scene = tx
        .doc()
        .scenes
        .get(scene_id)
        .cloned()
        .ok_or_else(|| CoreError::not_found("scene", scene_id))?;
    scene.tone = checked_tone(tx, tone)?;
    tx.put(&scene)?;
    Ok(serde_json::to_value(&scene.tone)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::CommandBus;
    use crate::commands::{ClipEdge, Command};
    use crate::store::EpisodeStore;

    /// The two-line scene of the timeline tests, a second scene, a 2 s and a 20 s sound.
    fn setup() -> (tempfile::TempDir, CommandBus) {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::create(&dir.path().join("t.cutaway")).unwrap();
        let mut bus = CommandBus::new(store, "test").unwrap();
        let (doc, scene) = crate::timeline::tests::doc2();
        let other = Scene {
            id: "other".into(),
            number: scene.number + 1,
            location: "OTHER PLACE".into(),
            ..scene.clone()
        };
        let asset = |id: &str, len: f64| -> Asset {
            serde_json::from_value(json!({
                "id": id, "file": format!("{id}.wav"), "hash": id, "duration_s": len,
                "source": "imported",
            }))
            .unwrap()
        };
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
            tx.put(&scene)?;
            tx.put(&other)?;
            tx.put(&asset("door", 2.0))?;
            tx.put(&asset("bed", 20.0))?;
            tx.put(&sound("p", "door", 1.0))?;
            tx.put(&sound("q", "door", 5.0))?;
            Ok(Value::Null)
        })
        .unwrap();
        (dir, bus)
    }

    fn sound(id: &str, asset: &str, at_s: f64) -> Placement {
        Placement {
            id: id.into(),
            scene_id: "sc".into(),
            asset_id: asset.into(),
            cue_id: None,
            lane: Lane::Sfx,
            character: None,
            layer: 0,
            anchor: None,
            at_s,
            offset_s: 0.0,
            in_s: 0.0,
            len_s: 1.0,
            fade_in_s: 0.02,
            fade_out_s: 0.3,
            fade_curve: FadeCurve::EqualPower,
            gain_db: 0.0,
            muted: false,
            pan: 0.0,
            depth: 0.0,
            auto_fade: true,
            added: 0,
            looped: false,
            unglued: false,
        }
    }

    /// The command changes the sound in one undo step; undo and redo restore it exactly.
    fn one_step(bus: &mut CommandBus, cmd: Command) -> Placement {
        let id = "p";
        let before = bus.doc().placements[id].clone();
        let r = bus.execute(&cmd).unwrap();
        assert!(r.seq.is_some(), "{} changed nothing", r.name);
        let after = bus.doc().placements[id].clone();
        assert_ne!(before, after);
        assert_eq!(bus.undo().unwrap().unwrap().name, r.name);
        assert_eq!(bus.doc().placements[id], before, "undo restores exactly");
        bus.redo().unwrap();
        assert_eq!(bus.doc().placements[id], after, "redo restores exactly");
        after
    }

    fn gain(id: &str, gain_db: f64) -> Command {
        Command::SetSoundGain {
            placement_id: id.into(),
            gain_db,
        }
    }

    #[test]
    fn moving_to_free_time_drops_the_anchor() {
        let (_d, mut bus) = setup();
        bus.execute(&Command::MoveSound {
            placement_id: "p".into(),
            anchor: Some(Anchor {
                line_id: "a".into(),
                word: None,
                edge: Edge::End,
            }),
            at_s: None,
            offset_s: None,
            glue: false,
        })
        .unwrap();
        let p = one_step(
            &mut bus,
            Command::MoveSound {
                placement_id: "p".into(),
                anchor: None,
                at_s: Some(3.25),
                offset_s: None,
                glue: false,
            },
        );
        assert_eq!(p.anchor, None);
        assert_eq!(p.at_s, 3.25);
    }

    #[test]
    fn a_sound_anchors_to_a_word_a_line_edge_and_the_pause_after_a_word() {
        let (_d, mut bus) = setup();
        let scene = bus.doc().scenes["sc"].clone();
        let pacing = crate::config::ProjectConfig::from_doc(bus.doc()).pacing();
        let layout = layout_scene(bus.doc(), &scene, &pacing);
        let item = layout.item("b").unwrap().clone();
        let word = item.words[0].clone();
        for (anchor, want) in [
            (
                Anchor {
                    line_id: "b".into(),
                    word: Some(0),
                    edge: Edge::Start,
                },
                word.start_s,
            ),
            // the pause after a word is that word's end
            (
                Anchor {
                    line_id: "b".into(),
                    word: Some(0),
                    edge: Edge::End,
                },
                word.end_s,
            ),
            (
                Anchor {
                    line_id: "b".into(),
                    word: None,
                    edge: Edge::End,
                },
                item.end_s,
            ),
        ] {
            let r = bus
                .execute(&Command::MoveSound {
                    placement_id: "p".into(),
                    anchor: Some(anchor.clone()),
                    at_s: None,
                    offset_s: Some(0.25),
                    glue: false,
                })
                .unwrap();
            assert!((r.result["start_s"].as_f64().unwrap() - want - 0.25).abs() < 1e-9);
            let p = &bus.doc().placements["p"];
            assert_eq!(p.anchor.as_ref(), Some(&anchor));
            assert_eq!(p.offset_s, 0.25);
            assert!((p.at_s - want).abs() < 1e-3, "fallback stays about here");
        }
        one_step(
            &mut bus,
            Command::MoveSound {
                placement_id: "p".into(),
                anchor: None,
                at_s: None,
                offset_s: Some(0.5),
                glue: false,
            },
        );
    }

    #[test]
    fn an_anchored_sound_follows_its_line() {
        let (_d, mut bus) = setup();
        let start = |bus: &CommandBus| {
            let scene = bus.doc().scenes["sc"].clone();
            let pacing = crate::config::ProjectConfig::from_doc(bus.doc()).pacing();
            placement_start(
                &layout_scene(bus.doc(), &scene, &pacing),
                &bus.doc().placements["p"],
            )
        };
        bus.execute(&Command::MoveSound {
            placement_id: "p".into(),
            anchor: Some(Anchor {
                line_id: "b".into(),
                word: None,
                edge: Edge::Start,
            }),
            at_s: None,
            offset_s: None,
            glue: false,
        })
        .unwrap();
        let before = start(&bus);
        bus.execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(3.0),
        })
        .unwrap();
        assert!(start(&bus) > before + 1.0, "the sound went with the line");
    }

    #[test]
    fn moves_are_refused_when_they_cannot_be_right() {
        let (_d, mut bus) = setup();
        let anchor = |line: &str, word| {
            Some(Anchor {
                line_id: line.into(),
                word,
                edge: Edge::Start,
            })
        };
        let bad = [
            (None, None, None),
            (anchor("a", None), Some(1.0), None),
            (anchor("nope", None), None, None),
            (anchor("a", Some(9999)), None, None),
            (None, Some(f64::NAN), None),
            (None, None, Some(f64::INFINITY)),
        ];
        let before = bus.doc().placements["p"].clone();
        for (anchor, at_s, offset_s) in bad {
            assert!(bus
                .execute(&Command::MoveSound {
                    placement_id: "p".into(),
                    anchor,
                    at_s,
                    offset_s,
                    glue: false,
                })
                .is_err());
        }
        assert_eq!(bus.doc().placements["p"], before);
        let mut line = bus.doc().lines["a"].clone();
        line.scene_id = "other".into();
        bus.execute_fn("x", &Value::Null, |tx| tx.put(&line).map(|_| Value::Null))
            .unwrap();
        let elsewhere = bus.execute(&Command::MoveSound {
            placement_id: "p".into(),
            anchor: anchor("a", None),
            at_s: None,
            offset_s: None,
            glue: false,
        });
        assert!(elsewhere.is_err(), "a line of another scene");
    }

    #[test]
    fn free_time_stops_at_zero() {
        let (_d, mut bus) = setup();
        let p = one_step(
            &mut bus,
            Command::MoveSound {
                placement_id: "p".into(),
                anchor: None,
                at_s: Some(-4.0),
                offset_s: None,
                glue: false,
            },
        );
        assert_eq!(p.at_s, 0.0);
    }

    #[test]
    fn the_auto_fade_follows_the_rule() {
        assert_eq!(Placement::auto_fade_out_s(0.5), 0.15);
        assert_eq!(Placement::auto_fade_out_s(1.0), 0.3);
        assert_eq!(Placement::auto_fade_out_s(8.0), 0.3);
        assert_eq!(Placement::auto_fade_out_s(8.01), 2.0);
        assert_eq!(Placement::auto_fade_out_s(0.0), 0.0);
    }

    #[test]
    fn trimming_keeps_the_auto_fade_in_step() {
        let (_d, mut bus) = setup();
        let p = one_step(
            &mut bus,
            Command::TrimPlacement {
                placement_id: "p".into(),
                edge: ClipEdge::End,
                delta_s: -0.5,
            },
        );
        assert_eq!(p.len_s, 0.5);
        assert_eq!(p.fade_out_s, 0.15);
    }

    #[test]
    fn a_bed_over_eight_seconds_fades_for_two() {
        let (_d, mut bus) = setup();
        let mut bed = sound("bed1", "bed", 0.0);
        bed.len_s = 12.0;
        bus.execute(&Command::PlaceSound { placement: bed })
            .unwrap();
        assert_eq!(bus.doc().placements["bed1"].fade_out_s, 2.0);
    }

    #[test]
    fn the_length_never_passes_the_source() {
        let (_d, mut bus) = setup();
        // a 2 s file, in point 1.5: 0.5 s are left, whatever is asked for
        let mut long = sound("long", "door", 0.0);
        long.in_s = 1.5;
        long.len_s = 9.0;
        bus.execute(&Command::PlaceSound { placement: long })
            .unwrap();
        assert_eq!(bus.doc().placements["long"].len_s, 0.5);

        bus.execute(&Command::TrimPlacement {
            placement_id: "p".into(),
            edge: ClipEdge::End,
            delta_s: 50.0,
        })
        .unwrap();
        assert_eq!(bus.doc().placements["p"].len_s, 2.0);

        bus.execute(&Command::UpdatePlacement {
            placement_id: "q".into(),
            patch: json!({ "in_s": 1.5, "len_s": 4.0 }),
        })
        .unwrap();
        let q = &bus.doc().placements["q"];
        assert!(q.in_s + q.len_s <= 2.0 + 1e-9, "{} + {}", q.in_s, q.len_s);
    }

    #[test]
    fn a_sound_of_unknown_length_is_left_alone() {
        let (_d, mut bus) = setup();
        let mut a = bus.doc().assets["door"].clone();
        a.id = "unknown".into();
        a.duration_s = 0.0;
        bus.execute(&Command::AddAsset { asset: a }).unwrap();
        let mut p = sound("u", "unknown", 0.0);
        p.len_s = 7.0;
        bus.execute(&Command::PlaceSound { placement: p }).unwrap();
        assert_eq!(bus.doc().placements["u"].len_s, 7.0);
    }

    #[test]
    fn setting_the_fade_out_by_hand_turns_auto_off() {
        let (_d, mut bus) = setup();
        let p = one_step(
            &mut bus,
            Command::SetSoundFades {
                placement_id: "p".into(),
                fade_in_s: None,
                fade_out_s: Some(0.5),
                auto: None,
            },
        );
        assert_eq!(p.fade_out_s, 0.5);
        assert!(!p.auto_fade);
        // a hand-set fade stays through a trim that leaves it room
        bus.execute(&Command::TrimPlacement {
            placement_id: "p".into(),
            edge: ClipEdge::End,
            delta_s: 0.5,
        })
        .unwrap();
        assert_eq!(bus.doc().placements["p"].fade_out_s, 0.5);
        // Auto fade hands it back
        let p = one_step(
            &mut bus,
            Command::SetSoundFades {
                placement_id: "p".into(),
                fade_in_s: None,
                fade_out_s: None,
                auto: Some(true),
            },
        );
        assert!(p.auto_fade);
        assert_eq!(p.fade_out_s, 0.3);
    }

    #[test]
    fn the_fade_in_leaves_auto_on() {
        let (_d, mut bus) = setup();
        let p = one_step(
            &mut bus,
            Command::SetSoundFades {
                placement_id: "p".into(),
                fade_in_s: Some(0.1),
                fade_out_s: None,
                auto: None,
            },
        );
        assert_eq!(p.fade_in_s, 0.1);
        assert!(p.auto_fade);
    }

    #[test]
    fn fades_stay_within_the_length() {
        let (_d, mut bus) = setup();
        bus.execute(&Command::SetSoundFades {
            placement_id: "p".into(),
            fade_in_s: Some(0.8),
            fade_out_s: None,
            auto: None,
        })
        .unwrap();
        // 0.8 + 0.3 auto would pass the 1 s sound
        assert!((bus.doc().placements["p"].fade_in_s - 0.7).abs() < 1e-9);
        bus.execute(&Command::SetSoundFades {
            placement_id: "p".into(),
            fade_in_s: None,
            fade_out_s: Some(5.0),
            auto: None,
        })
        .unwrap();
        let p = &bus.doc().placements["p"];
        assert_eq!((p.fade_in_s, p.fade_out_s), (0.0, 1.0));
    }

    #[test]
    fn a_fade_command_must_say_something() {
        let (_d, mut bus) = setup();
        assert!(bus
            .execute(&Command::SetSoundFades {
                placement_id: "p".into(),
                fade_in_s: None,
                fade_out_s: None,
                auto: None,
            })
            .is_err());
    }

    #[test]
    fn the_edit_page_patch_follows_the_same_fade_rule() {
        let (_d, mut bus) = setup();
        // its fade drag sends both values: an unchanged fade out is not a hand-set one
        bus.execute(&Command::UpdatePlacement {
            placement_id: "p".into(),
            patch: json!({ "fade_in_s": 0.2, "fade_out_s": 0.3 }),
        })
        .unwrap();
        assert!(bus.doc().placements["p"].auto_fade);
        bus.execute(&Command::UpdatePlacement {
            placement_id: "p".into(),
            patch: json!({ "fade_in_s": 0.2, "fade_out_s": 0.4 }),
        })
        .unwrap();
        let p = &bus.doc().placements["p"];
        assert!(!p.auto_fade);
        assert_eq!(p.fade_out_s, 0.4);
    }

    #[test]
    fn splitting_a_sound_keeps_its_place_in_the_room() {
        let (_d, mut bus) = setup();
        for c in [
            Command::SetSoundDepth {
                placement_id: "p".into(),
                depth: 0.7,
            },
            Command::SetSoundPan {
                placement_id: "p".into(),
                pan: -0.5,
            },
        ] {
            bus.execute(&c).unwrap();
        }
        let r = bus
            .execute(&Command::SplitPlacement {
                placement_id: "p".into(),
                at_s: 1.5,
            })
            .unwrap();
        let ids = r.result["ids"].as_array().unwrap();
        let a = &bus.doc().placements[ids[0].as_str().unwrap()];
        let b = &bus.doc().placements[ids[1].as_str().unwrap()];
        assert_eq!((a.depth, b.depth), (0.7, 0.7));
        assert_eq!((a.pan, b.pan), (-0.5, -0.5));
        assert!(a.fade_out_s <= 0.01 && !a.auto_fade, "no fade at the cut");
        assert!(b.auto_fade);
        assert_eq!(b.fade_out_s, 0.15, "the tail fades for its own length");
    }

    #[test]
    fn gain_stays_in_range() {
        let (_d, mut bus) = setup();
        assert_eq!(one_step(&mut bus, gain("p", -6.5)).gain_db, -6.5);
        assert_eq!(one_step(&mut bus, gain("p", 40.0)).gain_db, 6.0);
        assert_eq!(one_step(&mut bus, gain("p", -80.0)).gain_db, -24.0);
        assert!(bus.execute(&gain("p", f64::NAN)).is_err());
        assert_eq!(bus.doc().placements["p"].gain_db, -24.0);
    }

    #[test]
    fn pan_stays_in_range() {
        let (_d, mut bus) = setup();
        let pan = |pan| Command::SetSoundPan {
            placement_id: "p".into(),
            pan,
        };
        assert_eq!(one_step(&mut bus, pan(0.25)).pan, 0.25);
        assert_eq!(one_step(&mut bus, pan(3.0)).pan, 1.0);
        assert_eq!(one_step(&mut bus, pan(-3.0)).pan, -1.0);
        assert!(bus.execute(&pan(f64::NAN)).is_err());
    }

    #[test]
    fn depth_stays_in_range_and_says_when_it_means_nothing() {
        let (_d, mut bus) = setup();
        let depth = |depth| Command::SetSoundDepth {
            placement_id: "p".into(),
            depth,
        };
        assert_eq!(one_step(&mut bus, depth(0.4)).depth, 0.4);
        assert_eq!(one_step(&mut bus, depth(2.0)).depth, 1.0);
        assert_eq!(one_step(&mut bus, depth(-1.0)).depth, 0.0);
        assert!(bus.execute(&depth(f64::NAN)).is_err());
        let r = bus.execute(&depth(0.5)).unwrap();
        assert_eq!(r.result["in_room"], json!(true));
        bus.execute(&Command::UpdatePlacement {
            placement_id: "p".into(),
            patch: json!({ "lane": "music" }),
        })
        .unwrap();
        let r = bus.execute(&depth(0.6)).unwrap();
        assert_eq!(r.result["in_room"], json!(false));
    }

    #[test]
    fn a_slip_keeps_place_and_length_and_stays_inside_the_source() {
        let (_d, mut bus) = setup();
        let slip = |in_s| Command::SlipSound {
            placement_id: "p".into(),
            in_s,
        };
        let p = one_step(&mut bus, slip(0.75));
        assert_eq!((p.in_s, p.len_s, p.at_s), (0.75, 1.0, 1.0));
        // 2 s file, 1 s window: the in point stops at 1.0
        assert_eq!(one_step(&mut bus, slip(9.0)).in_s, 1.0);
        assert_eq!(one_step(&mut bus, slip(-1.0)).in_s, 0.0);
        assert!(bus.execute(&slip(f64::NAN)).is_err());
    }

    #[test]
    fn a_placed_sound_can_start_with_a_depth() {
        let (_d, mut bus) = setup();
        let mut p = sound("new", "door", 0.0);
        p.depth = 0.6;
        p.len_s = 0.5;
        p.fade_out_s = 0.0;
        let r = bus.execute(&Command::PlaceSound { placement: p }).unwrap();
        assert!(r.seq.is_some());
        let n = &bus.doc().placements["new"];
        assert_eq!(n.depth, 0.6);
        assert_eq!(n.fade_out_s, 0.15, "auto fade from the start");
        bus.undo().unwrap();
        assert!(!bus.doc().placements.contains_key("new"));
        bus.redo().unwrap();
        assert_eq!(bus.doc().placements["new"].depth, 0.6);
    }

    #[test]
    fn sounds_remember_the_order_they_were_added_in() {
        let (_d, mut bus) = setup();
        for id in ["first", "second"] {
            bus.execute(&Command::PlaceSound {
                placement: sound(id, "door", 9.0),
            })
            .unwrap();
        }
        let added = |bus: &CommandBus, id: &str| bus.doc().placements[id].added;
        assert_eq!(added(&bus, "second"), added(&bus, "first") + 1);
        // a value sent with the sound doesn't count, and undo gives the number back
        let mut p = sound("third", "door", 9.0);
        p.added = 1;
        let n = added(&bus, "second");
        bus.execute(&Command::PlaceSound { placement: p }).unwrap();
        assert_eq!(added(&bus, "third"), n + 1);
        bus.undo().unwrap();
        bus.execute(&Command::PlaceSound {
            placement: sound("again", "door", 9.0),
        })
        .unwrap();
        assert_eq!(added(&bus, "again"), n + 1);
    }

    #[test]
    fn the_tail_of_a_split_counts_as_added_after_the_head() {
        let (_d, mut bus) = setup();
        bus.execute(&Command::PlaceSound {
            placement: sound("s", "door", 9.0),
        })
        .unwrap();
        let r = bus
            .execute(&Command::SplitPlacement {
                placement_id: "s".into(),
                at_s: 9.5,
            })
            .unwrap();
        let ids = r.result["ids"].as_array().unwrap();
        let (a, b) = (
            bus.doc().placements[ids[0].as_str().unwrap()].added,
            bus.doc().placements[ids[1].as_str().unwrap()].added,
        );
        assert!(b > a, "{a} {b}");
    }

    #[test]
    fn placing_refuses_values_that_are_not_numbers() {
        let (_d, mut bus) = setup();
        let mut p = sound("new", "door", 0.0);
        p.depth = f64::NAN;
        assert!(bus.execute(&Command::PlaceSound { placement: p }).is_err());
        assert!(!bus.doc().placements.contains_key("new"));
    }

    #[test]
    fn removing_a_sound_is_one_undo_step() {
        let (_d, mut bus) = setup();
        let before = bus.doc().placements["p"].clone();
        bus.execute(&Command::DeletePlacement {
            placement_id: "p".into(),
        })
        .unwrap();
        assert!(!bus.doc().placements.contains_key("p"));
        bus.undo().unwrap();
        assert_eq!(bus.doc().placements["p"], before);
        bus.redo().unwrap();
        assert!(!bus.doc().placements.contains_key("p"));
    }

    #[test]
    fn a_multi_select_change_is_one_undo_step() {
        let (_d, mut bus) = setup();
        let (p0, q0) = (
            bus.doc().placements["p"].clone(),
            bus.doc().placements["q"].clone(),
        );
        let undoable = bus.history(100).unwrap().len();
        bus.execute(&Command::Batch {
            commands: vec![
                gain("p", -3.0),
                gain("q", -9.0),
                Command::SetSoundDepth {
                    placement_id: "p".into(),
                    depth: 0.8,
                },
                Command::SetSoundDepth {
                    placement_id: "q".into(),
                    depth: 0.9,
                },
                Command::MoveSound {
                    placement_id: "p".into(),
                    anchor: None,
                    at_s: Some(2.0),
                    offset_s: None,
                    glue: false,
                },
                Command::MoveSound {
                    placement_id: "q".into(),
                    anchor: None,
                    at_s: Some(6.0),
                    offset_s: None,
                    glue: false,
                },
            ],
        })
        .unwrap();
        assert_eq!(bus.history(100).unwrap().len(), undoable + 1);
        let (p1, q1) = (
            bus.doc().placements["p"].clone(),
            bus.doc().placements["q"].clone(),
        );
        assert_eq!((p1.gain_db, q1.gain_db), (-3.0, -9.0));
        assert_eq!((p1.at_s, q1.at_s), (2.0, 6.0));
        bus.undo().unwrap();
        assert_eq!(bus.doc().placements["p"], p0);
        assert_eq!(bus.doc().placements["q"], q0);
        bus.redo().unwrap();
        assert_eq!(bus.doc().placements["p"], p1);
        assert_eq!(bus.doc().placements["q"], q1);
    }

    #[test]
    fn a_batch_with_a_bad_command_changes_nothing() {
        let (_d, mut bus) = setup();
        let p0 = bus.doc().placements["p"].clone();
        let log = bus.history(100).unwrap().len();
        assert!(bus
            .execute(&Command::Batch {
                commands: vec![gain("p", -3.0), gain("nope", -9.0)],
            })
            .is_err());
        assert_eq!(bus.doc().placements["p"], p0);
        assert_eq!(bus.history(100).unwrap().len(), log);
    }

    #[test]
    fn a_batch_touching_one_sound_twice_undoes_to_the_start() {
        let (_d, mut bus) = setup();
        let p0 = bus.doc().placements["p"].clone();
        bus.execute(&Command::Batch {
            commands: vec![gain("p", -3.0), gain("p", -9.0)],
        })
        .unwrap();
        assert_eq!(bus.doc().placements["p"].gain_db, -9.0);
        bus.undo().unwrap();
        assert_eq!(bus.doc().placements["p"], p0);
    }

    #[test]
    fn setting_the_same_value_makes_no_undo_step() {
        let (_d, mut bus) = setup();
        bus.execute(&gain("p", -3.0)).unwrap();
        let r = bus.execute(&gain("p", -3.0)).unwrap();
        assert_eq!(r.seq, None);
    }

    /// Undo reads the old value back from the log as text. These drift by a bit on the way
    /// without serde_json's `float_roundtrip`: a dragged value would not come back exact.
    const AWKWARD: [f64; 3] = [-12.170959849309899, -7.300000000000001, 0.1 + 0.2];

    #[test]
    fn undo_gives_back_exactly_the_value_that_was_there() {
        let (_d, mut bus) = setup();
        for v in AWKWARD {
            bus.execute(&gain("p", v)).unwrap();
            bus.execute(&gain("p", 1.0)).unwrap();
            bus.undo().unwrap();
            assert_eq!(bus.doc().placements["p"].gain_db.to_bits(), v.to_bits());
        }
    }

    #[test]
    fn values_come_back_exact_from_the_file() {
        let (d, mut bus) = setup();
        for (id, v) in [("p", AWKWARD[0]), ("q", AWKWARD[1])] {
            bus.execute(&gain(id, v)).unwrap();
        }
        drop(bus);
        let store = EpisodeStore::open(&d.path().join("t.cutaway")).unwrap();
        let bus = CommandBus::new(store, "test").unwrap();
        assert_eq!(bus.doc().placements["p"].gain_db, AWKWARD[0]);
        assert_eq!(bus.doc().placements["q"].gain_db, AWKWARD[1]);
    }

    #[test]
    fn a_room_belongs_to_its_scene() {
        let (_d, mut bus) = setup();
        let other = bus.doc().scenes["other"].clone();
        let room = SceneRoom {
            size: 0.9,
            damping: 0.2,
            wet: 0.75,
        };
        let before = bus.doc().scenes["sc"].room;
        bus.execute(&Command::SetSceneRoom {
            scene_id: "sc".into(),
            room,
        })
        .unwrap();
        assert_eq!(bus.doc().scenes["sc"].room, room);
        assert_eq!(bus.doc().scenes["other"], other, "scene 2 is untouched");
        bus.undo().unwrap();
        assert_eq!(bus.doc().scenes["sc"].room, before);
        bus.redo().unwrap();
        assert_eq!(bus.doc().scenes["sc"].room, room);
    }

    /// A second scene in the same place, at the given time of day.
    fn twin(bus: &mut CommandBus, time_of_day: &str) -> Scene {
        let scene = Scene {
            id: "twin".into(),
            number: 9,
            order: vec![],
            time_of_day: time_of_day.into(),
            ..bus.doc().scenes["sc"].clone()
        };
        let put = scene.clone();
        bus.execute_fn("seed", &Value::Null, |tx| {
            tx.put(&put)?;
            Ok(Value::Null)
        })
        .unwrap();
        scene
    }

    fn tone(asset_id: &str, gain_db: f64) -> Option<SceneTone> {
        Some(SceneTone {
            asset_id: asset_id.into(),
            gain_db,
        })
    }

    /// Which sound a scene plays under it.
    fn heard(bus: &CommandBus, id: &str) -> Option<String> {
        let d = bus.doc();
        d.room_tone_for(&d.scenes[id]).and_then(|t| t.asset_id)
    }

    #[test]
    fn a_place_is_one_room() {
        let (_d, mut bus) = setup();
        twin(&mut bus, "");
        let room = SceneRoom {
            size: 0.9,
            damping: 0.1,
            wet: 0.2,
        };
        bus.execute(&Command::SetSceneRoom {
            scene_id: "sc".into(),
            room,
        })
        .unwrap();
        assert_eq!(bus.doc().scenes["twin"].room, room, "the same place");
        assert_ne!(bus.doc().scenes["other"].room, room, "another place");
    }

    #[test]
    fn scenes_of_a_place_share_what_any_of_them_has() {
        let (_d, mut bus) = setup();
        twin(&mut bus, "");
        // as an episode from before sharing has it: set on one scene only
        bus.execute(&Command::SetSceneTone {
            scene_id: "sc".into(),
            tone: tone("door", -34.0),
        })
        .unwrap();
        assert_eq!(heard(&bus, "twin").as_deref(), Some("door"));
        assert_eq!(heard(&bus, "other"), None, "not across places");
        let d = bus.doc();
        assert_eq!(d.room_tone_for(&d.scenes["twin"]).unwrap().gain_db, -34.0);
    }

    #[test]
    fn one_choice_reaches_every_scene_of_the_place_in_one_step() {
        let (_d, mut bus) = setup();
        twin(&mut bus, "");
        bus.execute(&Command::SetRoomAmbience {
            scene_id: "twin".into(),
            night: None,
            tone: tone("bed", -28.0),
        })
        .unwrap();
        assert_eq!(bus.doc().scenes["sc"].tone, tone("bed", -28.0));
        assert_eq!(bus.doc().scenes["twin"].tone, tone("bed", -28.0));
        assert_eq!(bus.doc().scenes["other"].tone, None);
        bus.undo().unwrap();
        assert_eq!(bus.doc().scenes["sc"].tone, None);
        assert_eq!(bus.doc().scenes["twin"].tone, None);
    }

    #[test]
    fn night_can_have_its_own_and_falls_back_on_the_day() {
        let (_d, mut bus) = setup();
        twin(&mut bus, "Night");
        bus.execute(&Command::SetRoomAmbience {
            scene_id: "sc".into(),
            night: Some(false),
            tone: tone("door", -28.0),
        })
        .unwrap();
        assert_eq!(heard(&bus, "twin").as_deref(), Some("door"), "optional");
        bus.execute(&Command::SetRoomAmbience {
            scene_id: "sc".into(),
            night: Some(true),
            tone: tone("bed", -30.0),
        })
        .unwrap();
        assert_eq!(heard(&bus, "twin").as_deref(), Some("bed"));
        assert_eq!(
            heard(&bus, "sc").as_deref(),
            Some("door"),
            "the day keeps its own"
        );
    }

    #[test]
    fn a_setting_without_an_ambience_plays_its_parents() {
        let (_d, mut bus) = setup();
        let inner = Location {
            id: "loc_other-place".into(),
            name: "OTHER PLACE".into(),
            room_tone: Default::default(),
            description: String::new(),
            parent: Some("loc_lab".into()),
        };
        let outer = Location {
            id: "loc_lab".into(),
            name: "LAB".into(),
            parent: None,
            ..inner.clone()
        };
        bus.execute_fn("seed", &Value::Null, |tx| {
            tx.put(&inner)?;
            tx.put(&outer)?;
            Ok(Value::Null)
        })
        .unwrap();
        bus.execute(&Command::SetRoomAmbience {
            scene_id: "sc".into(),
            night: None,
            tone: tone("door", -28.0),
        })
        .unwrap();
        assert_eq!(heard(&bus, "other").as_deref(), Some("door"));
    }

    #[test]
    fn room_settings_stay_in_range() {
        let (_d, mut bus) = setup();
        bus.execute(&Command::SetSceneRoom {
            scene_id: "sc".into(),
            room: SceneRoom {
                size: 4.0,
                damping: -1.0,
                wet: 2.0,
            },
        })
        .unwrap();
        let r = bus.doc().scenes["sc"].room;
        assert_eq!((r.size, r.damping, r.wet), (1.0, 0.0, 1.0));
        assert!(bus
            .execute(&Command::SetSceneRoom {
                scene_id: "sc".into(),
                room: SceneRoom {
                    size: f64::NAN,
                    ..r
                },
            })
            .is_err());
        assert!(bus
            .execute(&Command::SetSceneRoom {
                scene_id: "nope".into(),
                room: r,
            })
            .is_err());
    }

    #[test]
    fn a_new_script_version_keeps_the_room() {
        let (_d, mut bus) = setup();
        let room = SceneRoom {
            size: 0.9,
            damping: 0.2,
            wet: 0.75,
        };
        let sc = |bus: &CommandBus, heading: &str| {
            bus.doc()
                .scenes
                .values()
                .find(|s| s.heading == heading)
                .cloned()
        };
        let parsed = crate::fountain::parse("INT. CANTEEN - DAY\n\nPAUL\nHello there.\n");
        let version = |n: u32| ScriptVersion {
            id: format!("v{n}"),
            number: n,
            file: format!("v{n}.fountain"),
            name: "x".into(),
            hash: format!("h{n}"),
            imported_at: String::new(),
            summary: Value::Null,
            title: Default::default(),
        };
        bus.execute(&Command::ImportScriptVersion {
            version: version(1),
            parsed: parsed.clone(),
        })
        .unwrap();
        let heading = "INT. CANTEEN - DAY";
        let id = sc(&bus, heading).unwrap().id;
        assert_eq!(sc(&bus, heading).unwrap().room, SceneRoom::default());
        bus.execute(&Command::SetSceneRoom { scene_id: id, room })
            .unwrap();
        bus.execute(&Command::ImportScriptVersion {
            version: version(2),
            parsed,
        })
        .unwrap();
        assert_eq!(sc(&bus, heading).unwrap().room, room);
    }

    fn start_of(bus: &CommandBus, id: &str) -> f64 {
        let doc = bus.doc();
        let p = &doc.placements[id];
        let layout = layout_scene(doc, &doc.scenes[&p.scene_id], &Pacing::default());
        placement_start(&layout, p)
    }

    /// Where a word of a line is said.
    fn word_at(bus: &CommandBus, line: &str, word: usize) -> f64 {
        let doc = bus.doc();
        let scene = &doc.scenes[&doc.lines[line].scene_id];
        let layout = layout_scene(doc, scene, &Pacing::default());
        let item = layout.item(line).unwrap();
        item.words
            .iter()
            .find(|w| w.script_idx == Some(word))
            .unwrap()
            .start_s
    }

    fn glue_at(placement_id: &str, at_s: f64) -> Command {
        Command::MoveSound {
            placement_id: placement_id.into(),
            anchor: None,
            at_s: Some(at_s),
            offset_s: None,
            glue: true,
        }
    }

    #[test]
    fn glue_keeps_the_exact_moment_in_the_word_through_later_changes() {
        let (_d, mut bus) = setup();
        // a little way into the second word of b
        let t = word_at(&bus, "b", 1) + 0.137;
        bus.execute(&glue_at("p", t)).unwrap();
        let p = bus.doc().placements["p"].clone();
        let a = p.anchor.clone().unwrap();
        assert_eq!((a.line_id.as_str(), a.word), ("b", Some(1)));
        assert!((p.offset_s - 0.137).abs() < 1e-9);
        assert!((start_of(&bus, "p") - t).abs() < 1e-9);

        // the pause before it grows: the sound stays on the same spot of the word
        let from_word = |bus: &CommandBus| start_of(bus, "p") - word_at(bus, "b", 1);
        bus.execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(3.0),
        })
        .unwrap();
        assert!(start_of(&bus, "p") > t + 1.0);
        assert!((from_word(&bus) - 0.137).abs() < 1e-9);

        // so does a trim of the line's head
        bus.execute(&Command::SetTrim {
            line_id: "b".into(),
            head_s: 0.1,
            tail_s: 0.0,
        })
        .unwrap();
        assert!((from_word(&bus) - 0.137).abs() < 1e-9);
    }

    #[test]
    fn between_lines_a_sound_hangs_on_the_nearest_edge() {
        let (_d, mut bus) = setup();
        let doc = bus.doc();
        let layout = layout_scene(doc, doc.scenes.values().next().unwrap(), &Pacing::default());
        let (a_end, b_start) = (
            layout.item("a").unwrap().end_s,
            layout.item("b").unwrap().start_s,
        );
        assert!(b_start - a_end > 0.1);
        let edge = |bus: &CommandBus| {
            let a = bus.doc().placements["p"].anchor.clone().unwrap();
            (a.line_id, a.edge)
        };
        bus.execute(&glue_at("p", a_end + 0.01)).unwrap();
        assert_eq!(edge(&bus), ("a".to_string(), Edge::End));
        bus.execute(&glue_at("p", b_start - 0.01)).unwrap();
        assert_eq!(edge(&bus), ("b".to_string(), Edge::Start));
        assert!((start_of(&bus, "p") - (b_start - 0.01)).abs() < 1e-6);
    }

    #[test]
    fn glue_off_stays_off_through_moves_and_on_again_where_it_stands() {
        let (_d, mut bus) = setup();
        bus.execute(&glue_at("p", word_at(&bus, "b", 0))).unwrap();
        let t = start_of(&bus, "p");
        let glue = |glued| Command::SetSoundGlue {
            placement_id: "p".into(),
            glued,
        };

        let p = one_step(&mut bus, glue(false));
        assert!(p.anchor.is_none() && p.unglued);
        assert!((start_of(&bus, "p") - t).abs() < 1e-9);

        // the dialogue moves, the sound doesn't, and a move doesn't hang it on anything
        bus.execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(3.0),
        })
        .unwrap();
        assert!((start_of(&bus, "p") - t).abs() < 1e-9);
        bus.execute(&glue_at("p", t + 0.5)).unwrap();
        assert!(bus.doc().placements["p"].anchor.is_none());

        let at = start_of(&bus, "p");
        let p = one_step(&mut bus, glue(true));
        assert!(p.anchor.is_some() && !p.unglued);
        assert!(
            (start_of(&bus, "p") - at).abs() < 1e-9,
            "it stays where it is"
        );
    }

    #[test]
    fn beds_do_not_take_glue() {
        let (_d, mut bus) = setup();
        bus.execute_fn("seed", &Value::Null, |tx| {
            let mut p = tx.doc().placements["p"].clone();
            p.lane = Lane::Ambience;
            tx.put(&p)?;
            Ok(Value::Null)
        })
        .unwrap();
        bus.execute(&glue_at("p", 1.0)).unwrap();
        assert!(bus.doc().placements["p"].anchor.is_none());
    }

    #[test]
    fn a_word_this_take_lacks_keeps_the_sound_on_its_line() {
        let (_d, mut bus) = setup();
        bus.execute(&Command::MoveSound {
            placement_id: "p".into(),
            anchor: Some(Anchor {
                line_id: "b".into(),
                word: Some(1),
                edge: Edge::Start,
            }),
            at_s: None,
            offset_s: Some(0.0),
            glue: false,
        })
        .unwrap();
        // the take is recut so the word isn't in it any more
        bus.execute_fn("seed", &Value::Null, |tx| {
            let mut t = tx.doc().takes["tb"].clone();
            t.words.truncate(1);
            tx.put(&t)?;
            Ok(Value::Null)
        })
        .unwrap();
        assert!((start_of(&bus, "p") - word_at(&bus, "b", 0)).abs() < 1e-9);
    }

    /// Scenes s1 and s2 in one place, s3 in another, and a character.
    fn setup_two_rooms() -> (tempfile::TempDir, CommandBus) {
        let (d, mut bus) = setup();
        let scene = bus.doc().scenes.values().next().cloned().unwrap();
        bus.execute_fn("seed", &Value::Null, |tx| {
            for (id, location) in [("s1", "LAB"), ("s2", "LAB"), ("s3", "ROOF")] {
                tx.put(&Scene {
                    id: id.into(),
                    location: location.into(),
                    ..scene.clone()
                })?;
            }
            tx.put(&serde_json::from_value::<Character>(json!({
                "id": "PAUL", "label": "Paul", "color_index": 0, "aliases": [],
            }))?)?;
            Ok(Value::Null)
        })
        .unwrap();
        (d, bus)
    }

    #[test]
    fn a_scene_inherits_its_rooms_pan_until_it_has_its_own() {
        let (_d, mut bus) = setup_two_rooms();
        let pan = |scene: &str, pan: Option<f64>, room: bool| Command::SetCharacterPan {
            scene_id: scene.into(),
            character_id: "PAUL".into(),
            pan,
            room,
        };
        // the room's value reaches the other scene of the place, not another place
        bus.execute(&pan("s1", Some(-0.4), true)).unwrap();
        let of = |bus: &CommandBus, s: &str| bus.doc().scenes[s].pan_of("PAUL");
        assert_eq!(of(&bus, "s1"), -0.4);
        assert_eq!(of(&bus, "s2"), -0.4);
        assert_eq!(of(&bus, "s3"), 0.0);

        // a scene's own value wins, and a later room value leaves it alone
        bus.execute(&pan("s2", Some(0.7), false)).unwrap();
        bus.execute(&pan("s1", Some(0.1), true)).unwrap();
        assert_eq!(of(&bus, "s1"), 0.1);
        assert_eq!(of(&bus, "s2"), 0.7);

        // taken away, it goes back to the room
        bus.execute(&pan("s2", None, false)).unwrap();
        assert_eq!(of(&bus, "s2"), 0.1);

        // out of range is brought back, nonsense is refused, undo is one step
        bus.execute(&pan("s3", Some(4.0), false)).unwrap();
        assert_eq!(of(&bus, "s3"), 1.0);
        assert!(bus.execute(&pan("s3", Some(f64::NAN), false)).is_err());
        bus.undo().unwrap();
        assert_eq!(of(&bus, "s3"), 0.0);
    }
}
