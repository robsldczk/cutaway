//! The bucket's character search (D101): what an actor said, found on their own mic, clipped,
//! and placed as a vocal sound, the source left as it was.

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

/// How many sounds Paul's vocal bucket keeps.
fn paul_kept(s: &mut apc_app::Session) -> usize {
    let b = call(s, "buckets", json!({}));
    b["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["character"] == "PAUL")
        .map_or(0, |b| b["kept"].as_array().unwrap().len())
}

/// The demo's recordings, wherever the project keeps them.
fn wavs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(wavs(&p));
        } else if p
            .file_name()
            .is_some_and(|n| n == "Paul.wav" || n == "Sara.wav")
        {
            out.push(p);
        }
    }
    out
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
fn a_trimmed_clip_is_cut_placed_and_undone_in_one_step() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut s = demo(dir.path());
    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": "paul lunch" }),
    );
    let h = &r["hits"][0];
    let source = h["source_id"].as_str().unwrap();
    let t0 = h["t0"].as_f64().unwrap();
    let clip = json!({ "source_id": source, "t0": t0, "t1": t0 + 0.3, "text": "Lunch." });
    let scene = call(&mut s, "list_scenes", json!({}))[0]["id"].clone();

    let recordings = wavs(dir.path());
    assert!(!recordings.is_empty());
    let before: Vec<Vec<u8>> = recordings
        .iter()
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    let kept_before = paul_kept(&mut s);

    let placed = call(
        &mut s,
        "place_from_bucket",
        json!({ "scene": scene, "clip": clip, "at_s": 1.0 }),
    );
    assert!(placed["id"].is_string(), "{placed}");
    let sounds = call(&mut s, "list_sounds", json!({ "scene": scene }));
    let p = sounds
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == placed["id"])
        .unwrap_or_else(|| panic!("{sounds}"));
    assert!((p["len_s"].as_f64().unwrap() - 0.3).abs() < 0.01, "{p}");
    assert_eq!(p["lane"], "adlib", "{p}");
    assert_eq!(p["in_room"], true, "{p}");

    let kept = paul_kept(&mut s);
    assert_eq!(kept, kept_before + 1);

    // the same stretch again is the same sound
    let again = call(
        &mut s,
        "place_from_bucket",
        json!({ "scene": scene, "clip": clip, "at_s": 3.0 }),
    );
    assert_eq!(paul_kept(&mut s), kept);

    // one undo per drop, and the recordings as they were
    call(&mut s, "undo", json!({}));
    call(&mut s, "undo", json!({}));
    assert_eq!(paul_kept(&mut s), kept_before);
    let after: Vec<Vec<u8>> = recordings
        .iter()
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    assert!(before == after);
    let sounds = call(&mut s, "list_sounds", json!({ "scene": scene }));
    assert!(
        !sounds
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == placed["id"] || p["id"] == again["id"]),
        "{sounds}"
    );
}

#[test]
fn a_clip_must_end_after_it_starts() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut s = demo(dir.path());
    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": "paul lunch" }),
    );
    let source = r["hits"][0]["source_id"].clone();
    let scene = call(&mut s, "list_scenes", json!({}))[0]["id"].clone();
    let bad = json!({ "source_id": source, "t0": 2.0, "t1": 1.0 });
    let e = s.call(
        "place_from_bucket",
        &json!({ "scene": scene, "clip": bad, "at_s": 0.0 }),
    );
    assert!(e.is_err());
}

#[test]
fn an_adlib_exports_and_bounces_on_its_own_track() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut s = demo(dir.path());
    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": "paul lunch" }),
    );
    let h = &r["hits"][0];
    let clip =
        json!({ "source_id": h["source_id"], "t0": h["t0"], "t1": h["t1"], "text": "Lunch." });
    let scene = call(&mut s, "list_scenes", json!({}))[0]["id"].clone();
    call(
        &mut s,
        "place_from_bucket",
        json!({ "scene": scene, "clip": clip, "at_s": 1.0, "depth": 0.5 }),
    );

    let out = dir.path().join("adlib.fcpxml");
    call(
        &mut s,
        "export_fcpxml",
        json!({ "path": out.to_string_lossy() }),
    );
    let xml = std::fs::read_to_string(&out).unwrap();
    assert!(xml.contains("audioRole=\"dialogue.Adlibs\""), "{xml}");

    let mix = call(&mut s, "export_mix", json!({ "scene": scene }));
    assert!(mix["duration_s"].as_f64().unwrap() > 1.0, "{mix}");
}
