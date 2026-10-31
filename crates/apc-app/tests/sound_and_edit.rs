//! M7: anchoring, Edit → Options sync, spotting, library import and placement.

use serde_json::{json, Value};

use apc_app::{demo, Service};

fn env() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
}

fn line_item(svc: &mut Service, scene: &str, text: &str) -> Value {
    let v = svc.call("scene_view", &json!({ "scene": scene })).unwrap();
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["text"] == text)
        .unwrap()
        .clone()
}

fn placement_start(svc: &mut Service, id: &str) -> f64 {
    let ev = svc.call("edit_view", &json!({ "scene": "1" })).unwrap();
    ev["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()["start_s"]
        .as_f64()
        .unwrap()
}

fn sound(dir: &std::path::Path, name: &str, secs: f32, freq: f32) -> std::path::PathBuf {
    let sr = 48000u32;
    let s: Vec<f32> = (0..(sr as f32 * secs) as usize)
        .map(|i| 0.2 * (i as f32 * freq * std::f32::consts::TAU / sr as f32).sin())
        .collect();
    let p = dir.join(name);
    apc_media::wav::write_wav(&p, sr, 1, &s).unwrap();
    p
}

#[test]
fn anchored_sounds_follow_their_line_and_pinned_ones_stay() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let door = sound(dir.path(), "Door_close.wav", 1.0, 300.0);
    let r = svc
        .call(
            "import_sounds",
            &json!({ "paths": [door.to_string_lossy()] }),
        )
        .unwrap();
    let asset = r["assets"][0]["id"].as_str().unwrap().to_string();
    assert!(
        r["assets"][0]["file"]
            .as_str()
            .unwrap()
            .starts_with("SFX/Doors/"),
        "sorted into the library"
    );
    // dedup: the same file again is the same asset
    let again = svc
        .call(
            "import_sounds",
            &json!({ "paths": [door.to_string_lossy()] }),
        )
        .unwrap();
    assert_eq!(again["assets"][0]["id"], asset.as_str());

    let lunch = line_item(&mut svc, "1", "Lunch.");
    let lunch_id = lunch["line_id"].as_str().unwrap().to_string();
    let anchored = svc
        .call("place_sound", &json!({ "asset_id": asset, "scene": "1", "anchor_line_id": lunch_id, "offset_s": 0.1 }))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let pinned = svc
        .call(
            "place_sound",
            &json!({ "asset_id": asset, "scene": "1", "at_s": 1.0 }),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let a0 = placement_start(&mut svc, &anchored);
    assert!((a0 - (lunch["start_s"].as_f64().unwrap() + 0.1)).abs() < 1e-6);
    assert_eq!(placement_start(&mut svc, &pinned), 1.0);

    // take swap on an earlier line changes its length: the anchored effect moves, the pinned one stays
    let first = line_item(&mut svc, "1", "So the machine finally works.");
    svc.call(
        "command",
        &json!({ "cmd": "step_take", "line_id": first["line_id"], "delta": 1 }),
    )
    .unwrap();
    let first2 = line_item(&mut svc, "1", "So the machine finally works.");
    let shift = (first2["end_s"].as_f64().unwrap() - first2["start_s"].as_f64().unwrap())
        - (first["end_s"].as_f64().unwrap() - first["start_s"].as_f64().unwrap());
    let a1 = placement_start(&mut svc, &anchored);
    assert!(
        (a1 - a0 - shift).abs() < 1e-6,
        "follows the take swap ({a0} -> {a1}, shift {shift})"
    );
    assert_eq!(placement_start(&mut svc, &pinned), 1.0);

    // gap change before the anchor line
    svc.call("set_gap", &json!({ "line_id": lunch_id, "gap_s": 2.0 }))
        .unwrap();
    let lunch2 = line_item(&mut svc, "1", "Lunch.");
    let a2 = placement_start(&mut svc, &anchored);
    assert!(
        (a2 - (lunch2["start_s"].as_f64().unwrap() + 0.1)).abs() < 1e-6,
        "follows the gap change"
    );
    assert_eq!(placement_start(&mut svc, &pinned), 1.0);

    // unanchor: pinned where it plays now, then gap changes leave it
    svc.call(
        "command",
        &json!({ "cmd": "unanchor_placement", "placement_id": anchored }),
    )
    .unwrap();
    svc.call("set_gap", &json!({ "line_id": lunch_id, "gap_s": 0.5 }))
        .unwrap();
    assert!((placement_start(&mut svc, &anchored) - a2).abs() < 1e-6);

    // the sound is in the bounce
    let mix = svc.call("export_mix", &json!({ "scene": "1" })).unwrap();
    assert!(mix["duration_s"].as_f64().unwrap() > 3.0);
}

#[test]
fn edit_page_moves_show_on_the_options_page() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let lunch = line_item(&mut svc, "1", "Lunch.");
    let id = lunch["line_id"].as_str().unwrap().to_string();
    let gap0 = lunch["gap_before_s"].as_f64().unwrap();
    // drag on the Edit page = move_clip with ripple
    svc.call(
        "command",
        &json!({ "cmd": "move_clip", "line_id": id, "delta_s": 0.7, "ripple": true }),
    )
    .unwrap();
    let after = line_item(&mut svc, "1", "Lunch.");
    assert!(
        (after["gap_before_s"].as_f64().unwrap() - gap0 - 0.7).abs() < 1e-6,
        "Options shows the new spacing"
    );
    assert_eq!(after["gap_overridden"], true, "with the override mark");
    // a pin is something Options can't express: it is marked
    svc.call(
        "command",
        &json!({ "cmd": "pin_clip", "line_id": id, "at_s": 9.0 }),
    )
    .unwrap();
    let pinned = line_item(&mut svc, "1", "Lunch.");
    assert_eq!(pinned["edited_in_edit"], true);
    assert_eq!(pinned["start_s"], 9.0);
    // swapping the take on Options keeps the pin
    svc.call(
        "command",
        &json!({ "cmd": "step_take", "line_id": id, "delta": 1 }),
    )
    .unwrap();
    assert_eq!(line_item(&mut svc, "1", "Lunch.")["start_s"], 9.0);
}

#[test]
fn spotting_writes_anchored_cues_and_sounds_can_be_chosen() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let r = svc
        .call("spot_sound_effects", &json!({ "scene": "1" }))
        .unwrap();
    assert!(r["cues"].as_u64().unwrap() >= 2);
    let bucket = svc.call("list_bucket", &json!({ "scene": "1" })).unwrap();
    let cues = bucket.as_array().unwrap();
    assert!(
        cues.iter()
            .all(|c| c["cue"]["anchor"]["line_id"].is_string()),
        "every cue is anchored"
    );
    assert!(cues
        .iter()
        .any(|c| c["cue"]["description"].as_str().unwrap().contains("door")));
    let door_cue = cues
        .iter()
        .find(|c| c["cue"]["description"].as_str().unwrap().contains("door"))
        .unwrap()["cue"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // a library sound found by search becomes a candidate, then chosen, then placed at its anchor
    let f = sound(dir.path(), "door_creak.wav", 0.8, 220.0);
    svc.call("import_sounds", &json!({ "paths": [f.to_string_lossy()] }))
        .unwrap();
    let s = svc
        .call(
            "search_sounds",
            &json!({ "cue_id": door_cue, "attach": true }),
        )
        .unwrap();
    assert!(!s["results"].as_array().unwrap().is_empty());
    let b = svc.call("list_bucket", &json!({ "scene": "1" })).unwrap();
    let cue = b
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cue"]["id"] == door_cue.as_str())
        .unwrap()
        .clone();
    assert_eq!(cue["cue"]["status"], "candidates");
    let asset = cue["candidates"][0]["id"].as_str().unwrap().to_string();
    svc.call(
        "attach_sound",
        &json!({ "cue_id": door_cue, "asset_id": asset, "choose": true }),
    )
    .unwrap();
    svc.call(
        "place_sound",
        &json!({ "asset_id": asset, "scene": "1", "cue_id": door_cue }),
    )
    .unwrap();
    let b = svc.call("list_bucket", &json!({ "scene": "1" })).unwrap();
    let cue = b
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cue"]["id"] == door_cue.as_str())
        .unwrap()
        .clone();
    assert_eq!(cue["cue"]["status"], "placed");
    let ev = svc.call("edit_view", &json!({ "scene": "1" })).unwrap();
    assert_eq!(ev["placements"][0]["anchored"], true);
    // generation without a key reports why
    let g = svc.call("generate_sound", &json!({ "cue_id": door_cue }));
    if std::env::var("ELEVENLABS_API_KEY").is_err() {
        assert!(g.unwrap_err().to_string().contains("ELEVENLABS_API_KEY"));
    }
    // undo the placement
    svc.call("undo", &json!({})).unwrap();
    let ev = svc.call("edit_view", &json!({ "scene": "1" })).unwrap();
    assert!(ev["placements"].as_array().unwrap().is_empty());
}

#[test]
fn fixture_s2_spotting_with_mock_ai() {
    env();
    let fx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tarmac-ep2");
    if !fx.join("tarmac-episode-2_rob-test.fountain").exists() {
        eprintln!("skipped: fixture not present");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut svc = Service::create(dir.path(), "Ep02", None, "test").unwrap();
    svc.import_script(&fx.join("tarmac-episode-2_rob-test.fountain"), None)
        .unwrap();
    svc.call("spot_sound_effects", &json!({ "scene": "2" }))
        .unwrap();
    let b = svc.call("list_bucket", &json!({ "scene": "2" })).unwrap();
    let cues = b.as_array().unwrap();
    for c in cues {
        eprintln!(
            "{} [{}] @ {}",
            c["cue"]["description"], c["cue"]["cue_type"], c["anchor_text"]
        );
    }
    assert!(cues.len() >= 3);
    assert!(cues.iter().all(|c| c["anchor_text"].is_string()));
    assert!(cues.iter().any(|c| c["cue"]["cue_type"] == "ambience"));
    assert!(cues
        .iter()
        .any(|c| c["anchor_text"].as_str().unwrap().contains("BURRITO")));
}
