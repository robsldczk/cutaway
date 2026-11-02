//! M11: a Throughline `.thrl` script with its story context, the pilot's bios on top,
//! and the context in views and spotting (skips when `fixtures/` is absent).

use serde_json::json;

use apc_app::Service;

fn env() {
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
}

#[test]
fn thrl_import_story_notes_and_spotting_context() {
    env();
    let fx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tarmac-ep2");
    let (ep2, pilot, fountain) = (
        fx.join("tarmac-episode-2.thrl"),
        fx.join("tarmac-pilot.thrl"),
        fx.join("tarmac-episode-2_thrl-export.fountain"),
    );
    if !(ep2.exists() && pilot.exists() && fountain.exists()) {
        eprintln!("skipped: Throughline fixtures not present");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut svc = Service::create(dir.path(), "Ep02", None, "test").unwrap();

    // v001 from the Fountain export, v002 the same draft as .thrl: no line changes
    svc.import_script(&fountain, None).unwrap();
    let r = svc
        .call("import_script", &json!({ "path": ep2.to_string_lossy() }))
        .unwrap();
    assert_eq!(r["version"], 2);
    assert_eq!(r["changed"].as_array().unwrap().len(), 0, "{r}");
    assert_eq!(r["inserted"].as_array().unwrap().len(), 0);
    assert_eq!(r["deleted"].as_array().unwrap().len(), 0);
    let v = svc.doc().current_script_version().unwrap().clone();
    assert!(v.file.ends_with(".thrl"), "{}", v.file);
    assert!(dir.path().join("Ep02").exists() || !v.file.is_empty());

    // EP02 itself has no bios; the pilot's come in as story notes
    assert!(svc.doc().characters["PAUL"].profile.is_empty());
    let s = svc
        .call("import_story", &json!({ "path": pilot.to_string_lossy() }))
        .unwrap();
    let updated: Vec<String> = serde_json::from_value(s["characters_updated"].clone()).unwrap();
    assert!(updated.contains(&"PAUL".to_string()), "{s}");
    assert!(updated.contains(&"THE COLONEL".to_string()), "{s}");
    assert!(svc.doc().characters["SARA"].profile.want.contains("NASA"));

    // views: titles and acts in the scene list, the scene's story for the inspector
    let sv = svc.call("scene_view", &json!({ "scene": "2" })).unwrap();
    assert_eq!(sv["scene"]["title"], "Lunch");
    assert!(sv["scenes"][0]["act"]
        .as_str()
        .unwrap()
        .starts_with("Act 1"));
    assert_eq!(sv["story"]["setting"], "TARMAC - CANTEEN");
    assert_eq!(sv["story"]["setting_parent"], "TARMAC - HALLWAY");
    let paul = sv["characters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "PAUL")
        .unwrap();
    assert!(paul["profile"].as_str().unwrap().contains("Soldier"));

    // spotting gets the context, and still works with the mock
    let doc = svc.doc().clone();
    let scene = doc.find_scene("1").unwrap().clone();
    let ctx = apc_app::sound::spot_context(&doc, &scene);
    assert!(ctx.synopsis.contains("Paul sweating"));
    assert!(ctx
        .cast
        .iter()
        .any(|c| c.name == "PAUL" && c.note.contains("Soldier")));
    assert!(serde_json::to_string(&ctx).unwrap().len() < 2000);
    svc.call("spot_sound_effects", &json!({ "scene": "1" }))
        .unwrap();
    assert!(!svc
        .call("list_bucket", &json!({ "scene": "1" }))
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());

    // the MCP script resource shows a .thrl version as Fountain text
    let text = apc_core::script::script_text(&svc.paths.resolve(&v.file)).unwrap();
    assert!(text.starts_with("Title: TARMAC - Episode 2"));
    assert!(text.contains("INT. TARMAC - CANTEEN"));
}
