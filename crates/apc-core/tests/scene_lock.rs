//! A locked scene's timing holds still whatever asks to move it (D108): the bus checks the
//! result of every command, so nothing here names the commands it covers.

use apc_core::commands::{ClipEdge, Command};
use apc_core::store::EpisodeStore;
use apc_core::timeline::locked_timing;
use apc_core::*;
use serde_json::{json, Value};

fn line(id: &str, scene: &str, n: u32) -> ScriptLine {
    serde_json::from_value(json!({
        "id": id, "scene_id": scene, "kind": "dialogue", "character": "PAUL", "text": "hello there",
        "tokens": ["hello", "there"], "words": ["hello", "there"], "number": n, "version_added": 1,
    }))
    .unwrap()
}

fn take(id: &str, l: &str, no: u32, t0: f64) -> Take {
    serde_json::from_value(json!({
        "id": id, "line_id": l, "source_id": "src", "take_no": no, "take_type": "full",
        "t0": t0, "t1": t0 + 2.0, "score": 100.0, "confidence": 1.0,
        "words": [
            { "w": "hello", "raw": "hello", "t0": t0 + 0.2, "t1": t0 + 0.8, "script_idx": 0 },
            { "w": "there", "raw": "there", "t0": t0 + 1.0, "t1": t0 + 1.8, "script_idx": 1 },
        ],
    }))
    .unwrap()
}

/// Two scenes of two lines each; `a` has two takes. Sc. 1 holds `a`, `b`; Sc. 2 holds `c`.
fn bus(dir: &std::path::Path) -> CommandBus {
    let store = EpisodeStore::create(&dir.join("t.cutaway")).unwrap();
    let mut bus = CommandBus::new(store, "test").unwrap();
    bus.execute_fn("seed", &Value::Null, |tx| {
        for (id, number, order) in [("s1", 1, vec!["a", "b"]), ("s2", 2, vec!["c"])] {
            let s: Scene = serde_json::from_value(json!({
                "id": id, "number": number, "heading": "INT. LAB", "order": order,
            }))
            .unwrap();
            tx.put(&s)?;
        }
        for (l, s, n, t) in [("a", "s1", 1, 10.0), ("b", "s1", 2, 20.0), ("c", "s2", 1, 30.0)] {
            tx.put(&line(l, s, n))?;
            tx.put(&take(&format!("t{l}"), l, 1, t))?;
            tx.put(&BoxState {
                id: l.into(),
                take_id: Some(format!("t{l}")),
                ..Default::default()
            })?;
        }
        tx.put(&take("ta2", "a", 2, 12.5))?;
        let asset: Asset = serde_json::from_value(json!({
            "id": "as", "file": "x.wav", "hash": "h", "duration_s": 5.0, "source": "imported",
        }))
        .unwrap();
        tx.put(&asset)?;
        let p: Placement = serde_json::from_value(json!({
            "id": "pl", "scene_id": "s1", "asset_id": "as", "lane": "sfx", "at_s": 1.0, "len_s": 2.0,
        }))
        .unwrap();
        tx.put(&p)?;
        Ok(Value::Null)
    })
    .unwrap();
    bus
}

fn lock(bus: &mut CommandBus, scene: &str) {
    bus.execute(&Command::LockScene {
        scene_id: scene.into(),
        version: Some(3),
        at: "2026-11-05".into(),
    })
    .unwrap();
}

fn refused(bus: &mut CommandBus, c: Command) {
    let before = bus.doc().clone();
    let name = c.name();
    match bus.execute(&c) {
        Err(CoreError::SceneLocked(m)) => {
            assert_eq!(
                m,
                "Sc. 1 is locked. Exported as v003 on 5 Nov. Timing can't change; unlock it first.",
                "{name}"
            );
        }
        other => panic!("{name} should be refused, got {other:?}"),
    }
    assert_eq!(bus.doc(), &before, "{name} left nothing behind");
}

#[test]
fn whatever_moves_something_in_a_locked_scene_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut bus = bus(dir.path());
    lock(&mut bus, "s1");
    for c in [
        Command::SelectTake {
            line_id: "a".into(),
            take_id: "ta2".into(),
        },
        Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(2.0),
        },
        Command::SetTrim {
            line_id: "a".into(),
            head_s: 0.2,
            tail_s: 0.0,
        },
        Command::DeleteWords {
            line_id: "a".into(),
            from: 0,
            to: 1,
        },
        Command::MoveItem {
            scene_id: "s1".into(),
            item_id: "b".into(),
            to_index: 0,
        },
        Command::SetSceneTightness {
            scene_id: "s1".into(),
            tightness: 0.5,
        },
        Command::MoveSound {
            placement_id: "pl".into(),
            anchor: None,
            at_s: Some(4.0),
            offset_s: None,
            glue: false,
        },
        Command::TrimPlacement {
            placement_id: "pl".into(),
            edge: ClipEdge::End,
            delta_s: -0.5,
        },
        Command::DeletePlacement {
            placement_id: "pl".into(),
        },
        Command::SlipSound {
            placement_id: "pl".into(),
            in_s: 1.0,
        },
        Command::DeleteLine {
            line_id: "a".into(),
        },
    ] {
        refused(&mut bus, c);
    }
    let p: Placement = serde_json::from_value(json!({
        "id": "new", "scene_id": "s1", "asset_id": "as", "lane": "sfx", "at_s": 2.0, "len_s": 1.0,
    }))
    .unwrap();
    refused(&mut bus, Command::PlaceSound { placement: p });
    // written after the lock, by another hand, and never listed anywhere: still covered
    refused(
        &mut bus,
        Command::DuplicateSounds {
            placement_ids: vec!["pl".into()],
        },
    );
    // one edit of a batch is enough to refuse the batch
    refused(
        &mut bus,
        Command::Batch {
            commands: vec![
                Command::SetGap {
                    line_id: "c".into(),
                    gap_s: Some(1.0),
                },
                Command::SetGap {
                    line_id: "b".into(),
                    gap_s: Some(2.0),
                },
            ],
        },
    );
    // a project-wide setting that moves everything moves the locked scene too
    refused(
        &mut bus,
        Command::SetConfig {
            key: "pacing.breath_gap_s".into(),
            value: json!(1.5),
        },
    );
}

#[test]
fn what_keeps_the_timing_stays_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let mut bus = bus(dir.path());
    lock(&mut bus, "s1");
    for c in [
        Command::SetClipGain {
            line_id: "a".into(),
            gain_db: -3.0,
        },
        Command::SetClipMuted {
            line_id: "b".into(),
            part: None,
            muted: true,
        },
        Command::SetSoundGain {
            placement_id: "pl".into(),
            gain_db: -6.0,
        },
        Command::SetSoundPan {
            placement_id: "pl".into(),
            pan: 0.4,
        },
        Command::SetSoundDepth {
            placement_id: "pl".into(),
            depth: 0.6,
        },
        Command::SetSoundFades {
            placement_id: "pl".into(),
            fade_in_s: Some(0.3),
            fade_out_s: Some(0.2),
            auto: None,
        },
        Command::SetClipFades {
            line_id: "a".into(),
            fade_in_s: 0.1,
            fade_out_s: 0.1,
        },
        Command::ToggleFavorite {
            take_id: "ta".into(),
        },
        Command::SetSceneRoom {
            scene_id: "s1".into(),
            room: SceneRoom {
                size: 0.9,
                damping: 0.2,
                wet: 0.8,
            },
        },
    ] {
        let name = c.name();
        bus.execute(&c).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn other_scenes_stay_free_and_unlocking_frees_this_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut bus = bus(dir.path());
    lock(&mut bus, "s1");
    bus.execute(&Command::SetGap {
        line_id: "c".into(),
        gap_s: Some(1.0),
    })
    .unwrap();
    let gap = Command::SetGap {
        line_id: "b".into(),
        gap_s: Some(2.0),
    };
    assert!(bus.execute(&gap).is_err());
    bus.execute(&Command::UnlockScene {
        scene_id: "s1".into(),
    })
    .unwrap();
    assert!(bus.doc().scenes["s1"].lock.is_none());
    bus.execute(&gap).unwrap();
}

#[test]
fn a_lock_is_one_undo_step() {
    let dir = tempfile::tempdir().unwrap();
    let mut bus = bus(dir.path());
    lock(&mut bus, "s1");
    assert_eq!(
        bus.doc().scenes["s1"].lock.as_ref().unwrap().version,
        Some(3)
    );
    bus.undo().unwrap();
    assert!(bus.doc().scenes["s1"].lock.is_none());
    bus.redo().unwrap();
    assert!(bus.doc().scenes["s1"].lock.is_some());
}

#[test]
fn redo_cant_bring_back_a_timing_edit_past_a_lock() {
    let dir = tempfile::tempdir().unwrap();
    let mut bus = bus(dir.path());
    let gap = Command::SetGap {
        line_id: "b".into(),
        gap_s: Some(2.0),
    };
    bus.execute(&gap).unwrap();
    bus.undo().unwrap();
    let print = |bus: &CommandBus| locked_timing(bus.doc());
    lock(&mut bus, "s1");
    let held = print(&bus);
    assert_eq!(held.len(), 1);
    assert!(bus.redo().unwrap().is_none(), "the lock ended the redo");
    assert_eq!(print(&bus), held);
}

#[test]
fn locked_by_hand_it_says_so_without_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut bus = bus(dir.path());
    bus.execute(&Command::LockScene {
        scene_id: "s1".into(),
        version: None,
        at: "2026-11-05".into(),
    })
    .unwrap();
    let e = bus
        .execute(&Command::SetGap {
            line_id: "b".into(),
            gap_s: Some(2.0),
        })
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "Sc. 1 is locked. Timing can't change; unlock it first."
    );
}
