//! Searching the performances (D101): what an actor said, found on their own mic, clipped. The
//! mapping editor makes takes of it (D104); nothing is cut or placed from here (D118).

use serde_json::{json, Value};

fn env() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    std::env::set_var("CUTAWAY_SECRETS", "memory");
}

fn demo(dir: &std::path::Path) -> apc_app::Session {
    let svc = apc_app::demo::create(dir, "test", true).unwrap();
    let file = svc.paths.file.clone();
    drop(svc);
    apc_app::Session::with_project(&file, "test").unwrap()
}

fn call(s: &mut apc_app::Session, m: &str, p: Value) -> Value {
    s.call(m, &p).unwrap_or_else(|e| panic!("{m}: {e}"))
}

fn texts(r: &Value) -> Vec<String> {
    r["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["text"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn finds_a_characters_words_on_their_own_mic() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut s = demo(dir.path());

    let empty = call(&mut s, "search_performances", json!({ "query": "" }));
    assert!(empty["hits"].as_array().unwrap().is_empty());
    let cast: Vec<&str> = empty["cast"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert!(cast.contains(&"Paul") && cast.contains(&"Sara"), "{cast:?}");

    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": "paul lunch" }),
    );
    assert_eq!(r["character"], "PAUL");
    assert!(texts(&r).iter().all(|t| t == "Lunch."), "{r}");
    assert!(r["total"].as_u64().unwrap() >= 2, "{r}");
    let h = &r["hits"][0];
    let (t0, t1) = (h["t0"].as_f64().unwrap(), h["t1"].as_f64().unwrap());
    assert!(t1 > t0 && t1 - t0 < 2.0, "{h}");
    assert!(
        h["w0"].as_f64().unwrap() < t0 && h["w1"].as_f64().unwrap() > t1,
        "{h}"
    );
    assert!(!h["peaks"].as_array().unwrap().is_empty());
    assert!(h["at"].as_str().unwrap().contains("take"), "{h}");

    // Sara's line isn't Paul's, though it bleeds onto his mic
    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": "paul sandwiches" }),
    );
    assert_eq!(r["total"], 0, "{r}");
    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": "sara sandwiches" }),
    );
    assert_eq!(r["total"], 1, "{r}");
    // no name: everyone's
    let r = call(&mut s, "search_performances", json!({ "query": "coffee" }));
    assert!(r["character"].is_null());
    assert!(
        r["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["name"] == "Sara"),
        "{r}"
    );
}

#[test]
fn a_stretch_is_no_longer_placed_as_a_sound() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut s = demo(dir.path());
    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": "paul lunch" }),
    );
    let h = &r["hits"][0];
    let clip = json!({ "source_id": h["source_id"], "t0": h["t0"], "t1": h["t1"] });
    let scene = call(&mut s, "list_scenes", json!({}))[0]["id"].clone();
    let placed = s.call(
        "place_from_bucket",
        &json!({ "scene": scene, "clip": clip, "at_s": 1.0 }),
    );
    assert!(placed.is_err(), "{placed:?}");
    assert_eq!(
        call(&mut s, "list_sounds", json!({ "scene": scene })),
        json!([])
    );
}
