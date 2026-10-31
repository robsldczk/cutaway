//! The full pipeline on the synthetic demo session (runs everywhere, no fixtures needed).

use serde_json::json;

use apc_app::demo;

fn with_mock() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
}

#[test]
fn demo_session_analyzes_end_to_end() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let doc = svc.doc().clone();
    let takes_of = |text: &str| {
        let l = doc
            .lines
            .values()
            .find(|l| l.text == text)
            .unwrap_or_else(|| panic!("{text}"));
        doc.takes_of_line(&l.id)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(takes_of("So the machine finally works.").len(), 2);
    assert_eq!(takes_of("Lunch.").len(), 2);
    assert_eq!(takes_of("We can send things through time now.").len(), 1);
    assert_eq!(
        takes_of("Does it make coffee?").len(),
        2,
        "Sara's takes, not the room's cue reading"
    );
    assert_eq!(takes_of("I brought sandwiches for everyone.").len(), 1);
    let machine = takes_of("So the machine finally works.");
    assert_eq!(machine[1].pick, 2, "'that was the one I liked'");
    assert_eq!(machine[1].feedback, "the pick");
    let room = doc
        .sources
        .values()
        .find(|s| s.role == apc_core::Role::Room)
        .unwrap();
    assert!(
        (room.offset_s - demo::ROOM_DELAY_S).abs() < 0.05,
        "room offset {}",
        room.offset_s
    );
    assert!(doc.unmatched.values().any(|u| u.text.contains("water")));
    assert_eq!(doc.sessions.len(), 1, "same timecode: one session");

    let status = svc.call("project_status", &json!({})).unwrap();
    assert_eq!(status["lines_with_takes"], 5);
    let view = svc.call("scene_view", &json!({"scene": "1"})).unwrap();
    let items = view["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["kind"] == "spacer"));
    let first = items.iter().find(|i| i["kind"] == "line").unwrap();
    assert_eq!(first["takes"].as_array().unwrap().len(), 2);
    assert_eq!(first["takes"][0]["peaks"].as_array().unwrap().len(), 160);

    // edits through the generic command path, undo
    let line_id = first["line_id"].as_str().unwrap().to_string();
    svc.call(
        "command",
        &json!({"cmd": "delete_words", "line_id": line_id, "from": 0, "to": 0}),
    )
    .unwrap();
    let v = svc.call("scene_view", &json!({"scene": "1"})).unwrap();
    let w = &v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["line_id"] == line_id.as_str())
        .unwrap()["words"];
    assert_eq!(w[0]["deleted"], true);
    svc.call("undo", &json!({})).unwrap();
    let v = svc.call("scene_view", &json!({"scene": "1"})).unwrap();
    let w = &v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["line_id"] == line_id.as_str())
        .unwrap()["words"];
    assert_eq!(w[0]["deleted"], false);

    // exports
    let x = svc.call("export_fcpxml", &json!({})).unwrap();
    assert!(std::path::Path::new(x["path"].as_str().unwrap()).exists());
    assert!(
        svc.call("export_fcpxml", &json!({})).is_err(),
        "no overwrite without confirm"
    );
    svc.call("export_fcpxml", &json!({"confirm": true}))
        .unwrap();
    let mix = svc.call("export_mix", &json!({"scene": "1"})).unwrap();
    assert!(mix["duration_s"].as_f64().unwrap() > 3.0);
    let pl = svc
        .call("play", &json!({"scene": "1", "from_s": 0.0}))
        .unwrap();
    assert_eq!(pl["playing"], true);
    svc.call("stop", &json!({})).unwrap();
}

/// Golden cut list of the demo session (default takes).
#[test]
fn demo_cut_list_golden() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let r = svc.call("export_cut_list", &json!({})).unwrap();
    let csv = std::fs::read_to_string(r["path"].as_str().unwrap()).unwrap();
    let golden_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/demo-cutlist.csv");
    if std::env::var("UPDATE_GOLDEN").is_ok() || !golden_path.exists() {
        std::fs::write(&golden_path, &csv).unwrap();
        eprintln!("wrote {}", golden_path.display());
    }
    let golden = std::fs::read_to_string(&golden_path).unwrap();
    assert_eq!(
        csv, golden,
        "cut list changed; run with UPDATE_GOLDEN=1 if intended"
    );
}

#[test]
fn a_single_take_can_be_auditioned() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let take = svc
        .doc()
        .takes
        .values()
        .find(|t| t.take_no == 1)
        .unwrap()
        .clone();
    let r = svc
        .call("play_take", &json!({ "take_id": take.id, "from_s": 0.2 }))
        .unwrap();
    assert_eq!(r["playing"], true);
    assert!((r["duration_s"].as_f64().unwrap() - take.duration()).abs() < 1e-9);
    let ph = svc.call("playhead", &json!({})).unwrap();
    assert_eq!(ph["take_id"], take.id.as_str());
    assert!(ph["position_s"].as_f64().unwrap() >= 0.2);
    svc.call("stop", &json!({})).unwrap();
    // playing the scene again leaves audition mode
    svc.call("play", &json!({ "scene": "1" })).unwrap();
    assert!(svc.call("playhead", &json!({})).unwrap()["take_id"].is_null());
}
