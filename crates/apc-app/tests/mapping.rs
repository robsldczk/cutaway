//! The mapping editor (D104) on the demo session: a stretch marked by hand becomes a take or a
//! line, a line can be deleted, and undo, a re-run and the sources all stay honest.

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

fn lines(s: &mut apc_app::Session) -> Vec<Value> {
    let v = call(s, "scene_view", json!({ "scene": "1" }));
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "line")
        .cloned()
        .collect()
}

/// The first line with a take, and the picked take.
fn first_take(s: &mut apc_app::Session) -> (Value, Value) {
    let ls = lines(s);
    let l = ls
        .iter()
        .find(|l| !l["takes"].as_array().unwrap().is_empty());
    let l = l.unwrap().clone();
    let t = l["takes"][l["take_index"].as_u64().unwrap() as usize].clone();
    (l, t)
}

#[test]
fn the_editor_opens_on_the_lines_take_with_the_words_of_its_mic() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let (l, t) = first_take(&mut s);
    let o = call(&mut s, "mapping_open", json!({ "line_id": l["line_id"] }));
    assert_eq!(o["character"], l["character"]);
    assert_eq!(o["source_id"], t["source_id"]);
    assert_eq!(o["t0"], t["src_in"]);
    assert_eq!(o["t1"], t["src_out"]);
    assert!(!o["cast"].as_array().unwrap().is_empty());

    let r = call(&mut s, "recording", json!({ "source_id": o["source_id"] }));
    assert_eq!(r["heard"], true);
    assert!(!r["words"].as_array().unwrap().is_empty());
    assert!(r["takes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k["id"] == t["id"]));
    assert!(r["overview"].as_array().unwrap().len() > 10);

    // a stretch only
    let t0 = t["src_in"].as_f64().unwrap();
    let part = call(
        &mut s,
        "recording",
        json!({ "source_id": o["source_id"], "t0": t0, "t1": t0 + 0.5 }),
    );
    assert!(part["words"].as_array().unwrap().len() < r["words"].as_array().unwrap().len());
}

#[test]
fn a_marked_take_is_added_picked_exact_and_undone_in_one_step() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let (l, t) = first_take(&mut s);
    let id = l["line_id"].clone();
    let n = l["takes"].as_array().unwrap().len();
    let (a, b) = (
        t["src_in"].as_f64().unwrap() + 0.1,
        t["src_out"].as_f64().unwrap() - 0.1,
    );
    let r = call(
        &mut s,
        "add_take",
        json!({ "line_id": id, "source_id": t["source_id"], "t0": a, "t1": b }),
    );
    assert_eq!(r["take_no"], n + 1);
    let after = lines(&mut s)
        .into_iter()
        .find(|x| x["line_id"] == id)
        .unwrap();
    assert_eq!(after["takes"].as_array().unwrap().len(), n + 1);
    assert_eq!(after["take_id"], r["take_id"]);
    let new = &after["takes"][n];
    assert_eq!(new["src_in"].as_f64().unwrap(), a);
    assert_eq!(new["src_out"].as_f64().unwrap(), b);
    assert!(new["lufs"].as_f64().is_some(), "measured like any take");
    assert!(!new["words"].as_array().unwrap().is_empty());

    call(&mut s, "undo", json!({}));
    let back = lines(&mut s)
        .into_iter()
        .find(|x| x["line_id"] == id)
        .unwrap();
    assert_eq!(back["takes"].as_array().unwrap().len(), n);
    assert_eq!(back["take_id"], t["id"]);
}

#[test]
fn a_line_is_added_from_a_stretch_with_what_was_heard_as_its_text() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let (l, t) = first_take(&mut s);
    let before = lines(&mut s).len();
    let r = call(
        &mut s,
        "add_line",
        json!({
            "scene": "1", "after": l["line_id"], "character": l["character"],
            "source_id": t["source_id"], "t0": t["src_in"], "t1": t["src_out"],
        }),
    );
    let now = lines(&mut s);
    assert_eq!(now.len(), before + 1);
    let at = now
        .iter()
        .position(|x| x["line_id"] == l["line_id"])
        .unwrap();
    assert_eq!(now[at + 1]["line_id"], r["line_id"]);
    assert_eq!(now[at + 1]["adlib"], true);
    assert!(!now[at + 1]["text"].as_str().unwrap().is_empty());

    // typed text wins
    let r = call(
        &mut s,
        "add_line",
        json!({
            "scene": "1", "character": l["character"], "source_id": t["source_id"],
            "t0": t["src_in"], "t1": t["src_out"], "text": "Oh wow.",
        }),
    );
    let now = lines(&mut s);
    assert_eq!(now[0]["line_id"], r["line_id"]);
    assert_eq!(now[0]["text"], "Oh wow.");
}

#[test]
fn deleting_needs_a_yes_leaves_the_exports_and_comes_back_on_undo() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let ls = lines(&mut s);
    let id = ls[1]["line_id"].clone();
    let text = ls[1]["text"].as_str().unwrap().to_string();
    let scenes = call(&mut s, "list_scenes", json!({}));
    let count = scenes[0]["lines"].as_u64().unwrap();

    assert!(s.call("delete_line", &json!({ "line_id": id })).is_err());
    call(
        &mut s,
        "delete_line",
        json!({ "line_id": id, "confirm": true }),
    );
    let now = lines(&mut s);
    assert_eq!(now.len(), ls.len() - 1);
    assert!(now.iter().all(|x| x["line_id"] != id));
    let scenes = call(&mut s, "list_scenes", json!({}));
    assert_eq!(scenes[0]["lines"].as_u64().unwrap(), count - 1);

    let srt = d.path().join("out.srt");
    call(&mut s, "export_srt", json!({ "path": srt }));
    assert!(!std::fs::read_to_string(&srt).unwrap().contains(&text));

    call(&mut s, "undo", json!({}));
    assert_eq!(lines(&mut s).len(), ls.len());
}

#[test]
fn a_sentence_search_stays_on_the_character_asked_for() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let (l, t) = first_take(&mut s);
    let said = t["heard"].as_str().unwrap().to_string();
    let r = call(
        &mut s,
        "search_performances",
        json!({ "query": said, "character": l["character"] }),
    );
    assert_eq!(r["character"], l["character"]);
    let hits = r["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "{said}");
    assert!(hits.iter().all(|h| h["character"] == l["character"]));
    let a = t["src_in"].as_f64().unwrap();
    let z = t["src_out"].as_f64().unwrap();
    assert!(
        hits.iter()
            .any(|h| h["t0"].as_f64().unwrap() < z && h["t1"].as_f64().unwrap() > a),
        "the take itself is among the hits"
    );
}

fn take_of(s: &mut apc_app::Session, line: &Value, id: &Value) -> Value {
    let l = lines(s)
        .into_iter()
        .find(|x| x["line_id"] == line["line_id"])
        .unwrap();
    l["takes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == *id)
        .unwrap()
        .clone()
}

fn words_of(t: &Value) -> Vec<String> {
    t["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["text"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_take_edited_in_place_keeps_its_number_and_says_what_was_typed() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let (l, t) = first_take(&mut s);
    let n = l["takes"].as_array().unwrap().len();

    // text alone: the marks stay, the words are corrected
    let heard = words_of(&t);
    let mut fixed = heard.clone();
    fixed[1] = "Corrected".to_string();
    call(
        &mut s,
        "edit_take",
        json!({ "take_id": t["id"], "text": fixed.join(" ") }),
    );
    let e = take_of(&mut s, &l, &t["id"]);
    assert_eq!(words_of(&e), fixed);
    assert_eq!(e["src_in"], t["src_in"]);
    assert_eq!(e["src_out"], t["src_out"]);
    assert_eq!(e["take_no"], t["take_no"]);
    assert_eq!(e["lufs"], t["lufs"]);

    // what plays says it too
    let now = lines(&mut s)
        .into_iter()
        .find(|x| x["line_id"] == l["line_id"])
        .unwrap();
    let shown: Vec<&str> = now["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["text"].as_str().unwrap())
        .collect();
    assert!(shown.contains(&"Corrected"), "{shown:?}");

    // new marks: same take, shorter
    let a = t["src_in"].as_f64().unwrap() + 0.2;
    call(&mut s, "edit_take", json!({ "take_id": t["id"], "t0": a }));
    let e = take_of(&mut s, &l, &t["id"]);
    assert_eq!(e["src_in"].as_f64().unwrap(), a);
    assert_eq!(
        lines(&mut s)
            .iter()
            .find(|x| x["line_id"] == l["line_id"])
            .unwrap()["takes"]
            .as_array()
            .unwrap()
            .len(),
        n
    );

    call(&mut s, "undo", json!({}));
    call(&mut s, "undo", json!({}));
    let e = take_of(&mut s, &l, &t["id"]);
    assert_eq!(words_of(&e), heard);
    assert_eq!(e["src_in"], t["src_in"]);
}

#[test]
fn a_lengthened_crop_says_all_of_it() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let (l, t) = first_take(&mut s);
    let r = call(&mut s, "recording", json!({ "source_id": t["source_id"] }));
    let all = r["words"].as_array().unwrap();
    let (a, z) = (
        all[0]["t0"].as_f64().unwrap(),
        all[all.len() - 1]["t1"].as_f64().unwrap(),
    );
    let made = call(
        &mut s,
        "add_take",
        json!({ "line_id": l["line_id"], "source_id": t["source_id"], "t0": a - 0.05, "t1": z + 0.05 }),
    );
    let e = take_of(&mut s, &l, &made["take_id"]);
    assert_eq!(
        words_of(&e).len(),
        all.len(),
        "not only what's in the script"
    );
}

#[test]
fn a_take_deleted_needs_a_yes_leaves_the_line_and_comes_back_on_undo() {
    env();
    let d = tempfile::tempdir().unwrap();
    let mut s = demo(d.path());
    let (l, t) = first_take(&mut s);
    let n = l["takes"].as_array().unwrap().len();
    let count = lines(&mut s).len();
    assert!(s
        .call("delete_take", &json!({ "take_id": t["id"] }))
        .is_err());
    call(
        &mut s,
        "delete_take",
        json!({ "take_id": t["id"], "confirm": true }),
    );
    let ls = lines(&mut s);
    assert_eq!(ls.len(), count, "the line stays");
    let now = ls.iter().find(|x| x["line_id"] == l["line_id"]).unwrap();
    assert_eq!(now["takes"].as_array().unwrap().len(), n - 1);
    assert_ne!(now["take_id"], t["id"]);
    call(&mut s, "undo", json!({}));
    let now = lines(&mut s)
        .into_iter()
        .find(|x| x["line_id"] == l["line_id"])
        .unwrap();
    assert_eq!(now["take_id"], t["id"]);
}
