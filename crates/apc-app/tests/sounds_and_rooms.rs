//! Sound clips and rooms through the service, on the demo session: the methods the CLI and
//! MCP use, undo across them, and what the real bounce makes of depth and room settings.

use std::path::Path;

use serde_json::{json, Value};

use apc_app::{demo, Service};

fn env() {
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
}

/// White noise, so the room and the roll-off have something to work on.
fn noise_file(dir: &Path, secs: f32) -> std::path::PathBuf {
    let sr = 48000u32;
    let mut x = 77u32;
    let s: Vec<f32> = (0..(sr as f32 * secs) as usize)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((x >> 8) as f32 / 8_388_608.0 - 1.0) * 0.4
        })
        .collect();
    let p = dir.join("Foil_crackle.wav");
    apc_media::wav::write_wav(&p, sr, 1, &s).unwrap();
    p
}

/// A demo episode with a 2 s sound in the library.
fn episode(dir: &Path) -> (Service, String) {
    let mut svc = demo::create(dir, "test", true).unwrap();
    let p = noise_file(dir, 2.0);
    let r = svc
        .call("import_sounds", &json!({ "paths": [p.to_string_lossy()] }))
        .unwrap();
    let asset = r["assets"][0]["id"].as_str().unwrap().to_string();
    (svc, asset)
}

fn place(svc: &mut Service, asset: &str, depth: f64) -> String {
    let r = svc
        .call(
            "place_sound",
            &json!({ "asset_id": asset, "scene": "1", "lane": "sfx", "at_s": 0.5, "depth": depth }),
        )
        .unwrap();
    r["id"].as_str().unwrap().to_string()
}

fn sound(svc: &mut Service, id: &str) -> Value {
    let v = svc.call("list_sounds", &json!({ "scene": "1" })).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

/// The scene's bounce as one channel, with its length.
fn bounce(svc: &mut Service) -> Vec<f32> {
    let r = svc
        .call("export_mix", &json!({ "scene": "1", "confirm": true }))
        .unwrap();
    let p = r["path"].as_str().unwrap();
    let mut reader = apc_media::AudioReader::open(Path::new(p)).unwrap();
    let info = reader.info();
    let raw = reader.read_range(0.0, info.duration_s).unwrap();
    raw.iter()
        .step_by(info.channels as usize)
        .copied()
        .collect()
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| f64::from(*v).powi(2)).sum()
}

#[test]
fn a_new_sound_starts_with_its_depth_and_an_automatic_fade() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let id = place(&mut svc, &asset, 0.6);
    let s = sound(&mut svc, &id);
    assert_eq!(s["depth"], json!(0.6));
    assert_eq!(s["auto_fade"], json!(true));
    assert_eq!(s["fade_out_s"], json!(0.3));
    assert_eq!(s["in_room"], json!(true));
    assert_eq!(s["source_len_s"], json!(2.0));
}

#[test]
fn a_sound_can_be_placed_from_a_bucket_with_a_depth_on_an_edge() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let line = svc.call("list_lines", &json!({ "scene": "1" })).unwrap()[0]["line_id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = svc
        .call(
            "place_from_bucket",
            &json!({
                "scene": "1", "asset_id": asset, "depth": 0.35,
                "anchor_line_id": line, "anchor_edge": "end",
            }),
        )
        .unwrap();
    let s = sound(&mut svc, r["id"].as_str().unwrap());
    assert_eq!(s["depth"], json!(0.35));
    assert_eq!(s["anchor"]["edge"], json!("end"));
    assert_eq!(s["anchor"]["line_id"], json!(line));
}

#[test]
fn every_sound_method_is_one_undo_step() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let id = place(&mut svc, &asset, 0.0);
    let line = svc.call("list_lines", &json!({ "scene": "1" })).unwrap()[0]["line_id"]
        .as_str()
        .unwrap()
        .to_string();
    let steps = [
        ("move_sound", json!({ "placement_id": id, "at_s": 1.25 })),
        (
            "move_sound",
            json!({ "placement_id": id, "anchor_line_id": line, "anchor_edge": "end" }),
        ),
        (
            "trim_sound",
            json!({ "placement_id": id, "edge": "end", "delta_s": -0.5 }),
        ),
        (
            "set_sound_fades",
            json!({ "placement_id": id, "fade_out_s": 0.1 }),
        ),
        (
            "set_sound_gain",
            json!({ "placement_id": id, "gain_db": -7.5 }),
        ),
        ("set_sound_pan", json!({ "placement_id": id, "pan": 0.4 })),
        (
            "set_sound_depth",
            json!({ "placement_id": id, "depth": 0.9 }),
        ),
        ("slip_sound", json!({ "placement_id": id, "in_s": 0.2 })),
        ("remove_sound", json!({ "placement_id": id })),
    ];
    for (name, params) in steps {
        let before = sound(&mut svc, &id);
        svc.call(name, &params).unwrap();
        let after = sound(&mut svc, &id);
        assert_ne!(before, after, "{name} changed nothing");
        svc.call("undo", &json!({})).unwrap();
        assert_eq!(sound(&mut svc, &id), before, "{name}: undo");
        svc.call("redo", &json!({})).unwrap();
        assert_eq!(sound(&mut svc, &id), after, "{name}: redo");
    }
}

#[test]
fn a_batch_of_edits_is_one_undo_step() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let a = place(&mut svc, &asset, 0.1);
    let b = place(&mut svc, &asset, 0.2);
    let before = (sound(&mut svc, &a), sound(&mut svc, &b));
    svc.call(
        "batch_sounds",
        &json!({ "edits": [
            { "op": "gain", "placement_id": a, "gain_db": -3.0 },
            { "op": "gain", "placement_id": b, "gain_db": -3.0 },
            { "op": "depth", "placement_id": a, "depth": 0.5 },
            { "op": "depth", "placement_id": b, "depth": 0.6 },
            { "op": "move", "placement_id": a, "at_s": 3.0 },
            { "op": "move", "placement_id": b, "at_s": 4.0 },
        ] }),
    )
    .unwrap();
    assert_eq!(sound(&mut svc, &b)["depth"], json!(0.6));
    svc.call("undo", &json!({})).unwrap();
    assert_eq!((sound(&mut svc, &a), sound(&mut svc, &b)), before);
}

#[test]
fn a_batch_is_all_or_nothing_and_needs_something_to_do() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let a = place(&mut svc, &asset, 0.1);
    let before = sound(&mut svc, &a);
    let bad = svc.call(
        "batch_sounds",
        &json!({ "edits": [
            { "op": "gain", "placement_id": a, "gain_db": -3.0 },
            { "op": "remove", "placement_id": "nope" },
        ] }),
    );
    assert!(bad.is_err());
    assert_eq!(sound(&mut svc, &a), before);
    assert!(svc.call("batch_sounds", &json!({ "edits": [] })).is_err());
    assert!(svc
        .call("batch_sounds", &json!({ "edits": [{ "op": "explode" }] }))
        .is_err());
}

#[test]
fn the_edit_view_shows_the_same_clip() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let id = place(&mut svc, &asset, 0.0);
    svc.call(
        "batch_sounds",
        &json!({ "edits": [
            { "op": "pan", "placement_id": id, "pan": -0.25 },
            { "op": "depth", "placement_id": id, "depth": 0.75 },
            { "op": "gain", "placement_id": id, "gain_db": -2.0 },
        ] }),
    )
    .unwrap();
    let ev = svc.call("edit_view", &json!({ "scene": "1" })).unwrap();
    let p = ev["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(id))
        .unwrap();
    assert_eq!(
        (&p["pan"], &p["depth"], &p["gain_db"], &p["auto_fade"]),
        (&json!(-0.25), &json!(0.75), &json!(-2.0), &json!(true))
    );
}

#[test]
fn a_far_sound_differs_from_a_close_one_in_the_real_bounce() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let id = place(&mut svc, &asset, 0.0);
    let close = bounce(&mut svc);
    svc.call(
        "set_sound_depth",
        &json!({ "placement_id": id, "depth": 1.0 }),
    )
    .unwrap();
    let far = bounce(&mut svc);
    let n = close.len().min(far.len());
    let diff: Vec<f32> = close[..n]
        .iter()
        .zip(&far[..n])
        .map(|(a, b)| a - b)
        .collect();
    assert!(
        energy(&diff) > energy(&close[..n]) * 0.01,
        "audibly different"
    );
    svc.call("undo", &json!({})).unwrap();
    assert_eq!(bounce(&mut svc), close, "undo brings the same bounce back");
}

#[test]
fn room_settings_are_set_per_scene_and_are_heard() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    place(&mut svc, &asset, 0.8);
    let before = bounce(&mut svc);

    let rooms = svc.call("scene_rooms", &json!({})).unwrap();
    assert_eq!(rooms[0]["sounds"], json!(1));
    let decay = rooms[0]["decay_s"].as_f64().unwrap();

    // what is left out stays
    svc.call("set_scene_room", &json!({ "scene": "1", "size": 0.9 }))
        .unwrap();
    let after = svc.call("scene_rooms", &json!({})).unwrap();
    assert_eq!(after[0]["room"]["size"], json!(0.9));
    assert_eq!(after[0]["room"]["wet"], rooms[0]["room"]["wet"]);
    assert!(after[0]["decay_s"].as_f64().unwrap() > decay);
    for other in after.as_array().unwrap().iter().skip(1) {
        assert_eq!(other["room"], rooms[1]["room"], "the others stay");
    }
    assert_ne!(bounce(&mut svc), before, "heard");

    svc.call("undo", &json!({})).unwrap();
    assert_eq!(
        svc.call("scene_rooms", &json!({})).unwrap()[0]["room"],
        rooms[0]["room"]
    );
    assert_eq!(bounce(&mut svc), before);
}

#[test]
fn a_scene_with_no_sound_bounces_as_it_always_did() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, _) = episode(dir.path());
    let plain = bounce(&mut svc);
    svc.call(
        "set_scene_room",
        &json!({ "scene": "1", "size": 1.0, "wet": 1.0 }),
    )
    .unwrap();
    assert_eq!(bounce(&mut svc), plain, "dialogue never touches the room");
}

#[test]
fn bad_input_is_refused_with_a_message() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let (mut svc, asset) = episode(dir.path());
    let id = place(&mut svc, &asset, 0.0);
    for (name, params) in [
        (
            "set_sound_gain",
            json!({ "placement_id": "nope", "gain_db": 0.0 }),
        ),
        ("set_sound_gain", json!({ "placement_id": id })),
        ("move_sound", json!({ "placement_id": id })),
        (
            "move_sound",
            json!({ "placement_id": id, "anchor_line_id": "nope" }),
        ),
        ("set_scene_room", json!({ "scene": "99", "wet": 0.5 })),
        ("list_sounds", json!({ "scene": "99" })),
    ] {
        assert!(svc.call(name, &params).is_err(), "{name} {params}");
    }
}
