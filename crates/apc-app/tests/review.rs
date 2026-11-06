//! Review mode's promises, end to end on the demo session: notes that follow the cut, a bus that
//! takes nothing else, and review files that can be moved about without harm.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde_json::{json, Value};

use apc_app::{demo, Service, Session};
use apc_core::bus::BusMode;
use apc_core::review::ReviewFile;

/// One data folder for the whole run: tests run side by side, and it's an environment variable.
fn with_mock() {
    static LOCAL: OnceLock<PathBuf> = OnceLock::new();
    let local = LOCAL.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("cutaway-review-{}", std::process::id()));
        std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
        std::env::set_var("CUTAWAY_NO_AUDIO", "1");
        std::env::set_var("CUTAWAY_SECRETS", "memory");
        std::env::set_var("CUTAWAY_DATA_DIR", &d);
        d
    });
    assert!(!local.as_os_str().is_empty());
}

/// Who's remembered is this computer's, so tests that say who's reviewing take turns.
fn one_reviewer() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(|e| e.into_inner())
}

fn demo_episode(dir: &Path) -> Service {
    with_mock();
    demo::create(dir, "test", true).unwrap()
}

fn anna() -> Value {
    json!({ "name": "Anna Weber", "role": "director" })
}

fn rob() -> Value {
    json!({ "name": "Rob", "role": "editor" })
}

/// The first line of scene 1 with several words, and its id.
fn first_line(svc: &mut Service) -> (String, f64) {
    let v = svc.call("scene_view", &json!({ "scene": "1" })).unwrap();
    let l = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "line" && i["words"].as_array().unwrap().len() > 2)
        .unwrap();
    (
        l["line_id"].as_str().unwrap().to_string(),
        l["start_s"].as_f64().unwrap(),
    )
}

fn note_on_word(svc: &mut Service, line: &str, word: usize, text: &str) -> Value {
    svc.call(
        "add_note",
        &json!({ "line_id": line, "word": word, "text": text, "tag": "performance", "author": anna() }),
    )
    .unwrap()
}

fn find(svc: &mut Service, id: &Value) -> Value {
    let all = svc.call("list_notes", &json!({})).unwrap();
    all["threads"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["notes"].as_array().unwrap().clone())
        .find(|n| &n["id"] == id)
        .unwrap_or_else(|| panic!("note {id} isn't listed"))
}

fn reviews(svc: &Service) -> PathBuf {
    apc_app::review::reviews_dir(&svc.paths)
}

#[test]
fn a_marker_or_a_range_hangs_on_what_is_said_there() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let v = svc.call("scene_view", &json!({ "scene": "1" })).unwrap();
    let lines: Vec<&Value> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "line")
        .collect();
    let l = lines
        .iter()
        .find(|l| l["words"].as_array().unwrap().len() > 2)
        .unwrap();
    let w = &l["words"][1];
    let mid = (w["start_s"].as_f64().unwrap() + w["end_s"].as_f64().unwrap()) / 2.0;

    let t = svc
        .call("note_target", &json!({ "scene": "1", "at_s": mid }))
        .unwrap();
    assert_eq!(t["on"], "word");
    assert_eq!(t["line_id"], l["line_id"]);
    assert_eq!(t["quote"], w["text"]);

    let end = l["words"][2]["end_s"].as_f64().unwrap() - 0.01;
    let t = svc
        .call(
            "note_target",
            &json!({ "scene": "1", "at_s": mid, "to_s": end }),
        )
        .unwrap();
    assert_eq!(t["on"], "words");
    assert!(t["quote"].as_str().unwrap().contains(' '), "{t}");
    assert!(t["time"].as_str().unwrap().contains(" to "), "{t}");

    // between two lines it's the pause
    let (a, b) = (lines[0], lines[1]);
    let gap = (a["end_s"].as_f64().unwrap() + b["start_s"].as_f64().unwrap()) / 2.0;
    let t = svc
        .call("note_target", &json!({ "scene": "1", "at_s": gap }))
        .unwrap();
    assert_eq!(t["on"], "pause");
    assert_eq!(t["line_id"], a["line_id"]);

    let t = svc.call("note_target", &json!({ "scene": "1" })).unwrap();
    assert_eq!(t["on"], "scene");
}

#[test]
fn a_note_follows_its_word_through_gaps_reorders_and_take_swaps() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 1, "bigger");
    let was = n["start_s"].as_f64().unwrap();
    assert_eq!(n["changed"], false);

    // a longer pause before it moves it, and that isn't a change to the line
    svc.call("set_gap", &json!({ "line_id": line, "gap_s": 3.0 }))
        .unwrap();
    let moved = find(&mut svc, &n["id"]);
    assert!(moved["start_s"].as_f64().unwrap() > was + 1.0, "{moved}");
    assert_eq!(moved["changed"], false, "the line itself is as it was");
    assert_ne!(
        moved["time"], n["time"],
        "the time code is worked out afresh"
    );

    // moved to the end of the scene: it goes along
    let v = svc.call("scene_view", &json!({ "scene": "1" })).unwrap();
    let last = v["items"].as_array().unwrap().len() - 1;
    let scene = v["scene"]["id"].clone();
    let at = moved["start_s"].as_f64().unwrap();
    svc.call(
        "command",
        &json!({ "cmd": "move_item", "scene_id": scene, "item_id": line, "to_index": last }),
    )
    .unwrap();
    let reordered = find(&mut svc, &n["id"]);
    assert!(reordered["start_s"].as_f64().unwrap() > at, "{reordered}");
    assert_eq!(reordered["n"], 1, "the only note of the scene");
    assert_eq!(reordered["changed"], false);

    // a new take of the line: still on word 1, and the fix is there to hear
    let takes = svc.call("list_takes", &json!({ "line_id": line })).unwrap();
    let takes = takes.as_array().unwrap();
    assert!(takes.len() > 1, "the demo's first line has two takes");
    let other = takes
        .iter()
        .find(|t| t["selected"] != true)
        .unwrap_or(&takes[0]);
    svc.call(
        "select_take",
        &json!({ "line_id": line, "take_id": other["take_id"] }),
    )
    .unwrap();
    let swapped = find(&mut svc, &n["id"]);
    assert_eq!(swapped["changed"], true, "{swapped}");
    assert_eq!(swapped["detached"], false);
    assert_eq!(swapped["quote"], n["quote"]);
}

#[test]
fn a_deleted_line_leaves_a_detached_note_with_its_words() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "cut this?");
    assert!(!n["quote"].as_str().unwrap().is_empty());
    svc.call("delete_line", &json!({ "line_id": line, "confirm": true }))
        .unwrap();
    let d = find(&mut svc, &n["id"]);
    assert_eq!(d["detached"], true);
    assert_eq!(d["quote"], n["quote"], "it's listed with its words");
    assert_eq!(d["start_s"], Value::Null);
    assert_eq!(d["written_s"], n["start_s"], "and where it last was");
    assert!(n["time"]
        .as_str()
        .unwrap()
        .starts_with(d["time"].as_str().unwrap()));
    // undo brings the line back, and the note with it
    svc.call("undo", &json!({})).unwrap();
    assert_eq!(find(&mut svc, &n["id"])["detached"], false);
}

#[test]
fn a_note_on_a_sound_moves_with_the_sound() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let wav = dir.path().join("door.wav");
    apc_media::wav::write_wav(&wav, 48000, 1, &vec![0.1f32; 48000]).unwrap();
    let r = svc
        .call(
            "import_sounds",
            &json!({ "paths": [wav.to_string_lossy()] }),
        )
        .unwrap();
    let asset = r["imported"][0]["id"]
        .as_str()
        .or(r["ids"][0].as_str())
        .or(r["assets"][0]["id"].as_str())
        .unwrap()
        .to_string();
    svc.call(
        "place_sound",
        &json!({ "scene": "1", "asset_id": asset, "at_s": 2.0 }),
    )
    .unwrap();
    let sounds = svc.call("list_sounds", &json!({ "scene": "1" })).unwrap();
    let id = sounds
        .as_array()
        .map(|a| a[0].clone())
        .unwrap_or_else(|| sounds["sounds"][0].clone())["id"]
        .as_str()
        .unwrap()
        .to_string();
    let n = svc
        .call(
            "add_note",
            &json!({ "placement_id": id, "text": "too loud", "tag": "sound", "author": anna() }),
        )
        .unwrap();
    assert_eq!(n["on"], "sound");
    assert_eq!(n["start_s"], 2.0);
    svc.call(
        "move_sound",
        &json!({ "id": id, "at_s": 5.0, "placement_id": id }),
    )
    .unwrap();
    let m = find(&mut svc, &n["id"]);
    assert!(m["start_s"].as_f64().unwrap() > 4.0, "{m}");
    assert_eq!(m["changed"], true, "the sound moved");
    svc.call("remove_sound", &json!({ "id": id, "placement_id": id }))
        .unwrap();
    assert_eq!(find(&mut svc, &n["id"])["detached"], true);
}

#[test]
fn review_mode_takes_notes_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    svc.call("enter_review", &json!({})).unwrap();

    // through its method, through the raw command, and straight at the bus
    let refused = |r: Result<Value, apc_app::AppError>| {
        let e = r.unwrap_err();
        assert_eq!(e.code(), "review_only", "{e}");
        e.to_string()
    };
    let why = refused(svc.call("set_gap", &json!({ "line_id": line, "gap_s": 1.0 })));
    assert!(why.contains("only takes notes"), "{why}");
    let why = refused(svc.call(
        "command",
        &json!({ "cmd": "set_gap", "line_id": line, "gap_s": 1.0 }),
    ));
    assert!(why.contains("would change the cut"), "{why}");
    refused(svc.call("export_scenes", &json!({})));
    refused(svc.call("start_review_round", &json!({ "scenes": ["1"] })));
    assert!(svc
        .bus
        .execute(&apc_core::Command::DeleteLine {
            line_id: line.clone()
        })
        .is_err());

    // what's before review can't be reached from it
    let undo = svc.call("undo", &json!({})).unwrap();
    assert_eq!(undo["done"], Value::Null);

    let n = note_on_word(&mut svc, &line, 0, "yes");
    svc.call(
        "reply_note",
        &json!({ "note": n["id"], "text": "ok", "author": rob() }),
    )
    .unwrap();
    // and a note can be taken back
    assert_eq!(svc.call("undo", &json!({})).unwrap()["done"], "reply_note");
    assert_eq!(
        find(&mut svc, &n["id"])["replies"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    svc.call("leave_review", &json!({})).unwrap();
    assert_eq!(svc.bus.mode(), BusMode::Edit);
    svc.call("set_gap", &json!({ "line_id": line, "gap_s": 1.0 }))
        .unwrap();
}

#[test]
fn starting_a_round_keeps_the_cut_and_flags_the_scenes() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let r = svc
        .call("start_review_round", &json!({ "scenes": ["1"] }))
        .unwrap();
    assert_eq!(r["number"], 1);
    let scenes = svc.call("list_scenes", &json!({})).unwrap();
    assert_eq!(scenes[0]["status"], "review");
    assert_eq!(scenes[1]["status"], Value::Null);

    let v = svc.call("review_view", &json!({ "scene": "1" })).unwrap();
    assert_eq!(v["round"], r["round"]);
    assert_eq!(v["scenes"][0]["in_round"], true);
    assert_eq!(v["scenes"][1]["in_round"], false);

    // a note goes to the round its scene is in, and undo takes the whole start back
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "hm");
    assert_eq!(n["round"], r["round"]);
    assert_eq!(n["round_number"], 1);
    // an earlier round stays when a new one comes
    let r2 = svc
        .call("start_review_round", &json!({ "scenes": ["1", "2"] }))
        .unwrap();
    let h = svc.call("review_history", &json!({})).unwrap();
    assert_eq!(h["rounds"].as_array().unwrap().len(), 2);
    assert_eq!(h["rounds"][0]["id"], r2["round"]);
    assert_eq!(h["rounds"][1]["summary"]["all"]["open"], 1);
    assert_eq!(h["rounds"][1]["summary"]["people"]["Anna Weber"]["open"], 1);
}

#[test]
fn a_note_moves_to_what_is_said_there_and_undo_puts_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "wrong place");
    let v = svc.call("scene_view", &json!({ "scene": "1" })).unwrap();
    let other = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "line" && i["line_id"] != line.as_str())
        .find(|i| !i["words"].as_array().unwrap().is_empty())
        .unwrap()
        .clone();
    let w = &other["words"][0];
    let mid = (w["start_s"].as_f64().unwrap() + w["end_s"].as_f64().unwrap()) / 2.0;

    let m = svc
        .call(
            "move_note",
            &json!({ "note": n["id"], "scene": "1", "at_s": mid, "author": anna() }),
        )
        .unwrap();
    assert_eq!(m["line_id"], other["line_id"]);
    assert_eq!(m["quote"], w["text"], "it quotes where it is now");
    assert_ne!(m["speaker"], n["speaker"]);
    assert_eq!(m["text"], "wrong place");
    assert_eq!(m["changed"], false, "the new element is as it was");
    // what comes after a move carries the new spot
    let r = svc
        .call(
            "reply_note",
            &json!({ "note": n["id"], "text": "ok", "author": rob() }),
        )
        .unwrap();
    assert_eq!(r["line_id"], other["line_id"]);

    assert_eq!(svc.call("undo", &json!({})).unwrap()["done"], "reply_note");
    assert_eq!(svc.call("undo", &json!({})).unwrap()["done"], "move_note");
    let back = find(&mut svc, &n["id"]);
    assert_eq!(back["line_id"], line.as_str());
    assert_eq!(back["quote"], n["quote"]);
    assert_eq!(svc.call("redo", &json!({})).unwrap()["done"], "move_note");
    assert_eq!(find(&mut svc, &n["id"])["line_id"], other["line_id"]);
}

#[test]
fn a_detached_note_comes_back_by_moving_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "lost");
    svc.call("delete_line", &json!({ "line_id": line, "confirm": true }))
        .unwrap();
    assert_eq!(find(&mut svc, &n["id"])["detached"], true);
    let v = svc.call("scene_view", &json!({ "scene": "1" })).unwrap();
    let left = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "line")
        .unwrap()
        .clone();
    svc.call(
        "move_note",
        &json!({ "note": n["id"], "line_id": left["line_id"], "author": rob() }),
    )
    .unwrap();
    let m = find(&mut svc, &n["id"]);
    assert_eq!(m["detached"], false);
    assert_eq!(m["on"], "line");
}

#[test]
fn only_the_author_or_the_editor_moves_or_removes_a_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "mine");
    let jonas = json!({ "name": "Jonas Brandt", "role": "producer" });
    let e = svc
        .call(
            "remove_note",
            &json!({ "note": n["id"], "author": jonas, "confirm": true }),
        )
        .unwrap_err();
    assert!(e.to_string().contains("Anna Weber's note"), "{e}");
    let e = svc
        .call(
            "move_note",
            &json!({ "note": n["id"], "scene": "1", "at_s": 3.0, "author": jonas }),
        )
        .unwrap_err();
    assert!(e.to_string().contains("only they or the editor"), "{e}");
    // anyone can still answer it
    svc.call("resolve_note", &json!({ "note": n["id"], "author": jonas }))
        .unwrap();

    let e = svc
        .call("remove_note", &json!({ "note": n["id"], "author": anna() }))
        .unwrap_err();
    assert_eq!(e.code(), "needs_confirm");
    svc.call(
        "remove_note",
        &json!({ "note": n["id"], "author": rob(), "confirm": true }),
    )
    .unwrap();
    assert_eq!(svc.call("list_notes", &json!({})).unwrap()["count"], 0);
}

#[test]
fn a_remove_reaches_the_other_machine_and_so_does_its_undo() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "changed my mind");
    let file = reviews(&svc).join("anna-weber.cutawayreview");

    let other_dir = tempfile::tempdir().unwrap();
    let copy = other_dir.path().join("Copy.cutaway");
    apc_core::store::EpisodeStore::copy_file(&svc.paths.file, &copy).unwrap();
    let mut other = Service::open(&copy, "test").unwrap();
    assert_eq!(other.call("list_notes", &json!({})).unwrap()["count"], 1);
    let dest = other.paths.root.join("Reviews/anna-weber.cutawayreview");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();

    svc.call("enter_review", &json!({})).unwrap();
    svc.call(
        "remove_note",
        &json!({ "note": n["id"], "author": anna(), "confirm": true }),
    )
    .unwrap();
    assert_eq!(svc.call("list_notes", &json!({})).unwrap()["count"], 0);
    std::fs::copy(&file, &dest).unwrap();
    other.call("review_sync", &json!({})).unwrap();
    assert_eq!(
        other.call("list_notes", &json!({})).unwrap()["count"],
        0,
        "gone there too"
    );

    assert_eq!(svc.call("undo", &json!({})).unwrap()["done"], "remove_note");
    assert_eq!(find(&mut svc, &n["id"])["text"], "changed my mind");
    std::fs::copy(&file, &dest).unwrap();
    other.call("review_sync", &json!({})).unwrap();
    assert_eq!(
        other.call("list_notes", &json!({})).unwrap()["count"],
        1,
        "and back"
    );
}

#[test]
fn an_undone_round_does_not_come_back_from_the_editors_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    svc.call("start_review_round", &json!({ "scenes": ["1"] }))
        .unwrap();
    note_on_word(&mut svc, &line, 0, "stays");
    // Anna's note holds on to its round: this one is the editor's alone
    let r = svc
        .call("start_review_round", &json!({ "scenes": ["2"] }))
        .unwrap();
    assert_eq!(r["number"], 2);
    assert_eq!(
        svc.call("undo", &json!({})).unwrap()["done"],
        "start_review_round"
    );
    svc.call("review_sync", &json!({})).unwrap();

    let file = svc.paths.file.clone();
    svc.close().unwrap();
    drop(svc);
    let mut again = Service::open(&file, "test").unwrap();
    again.call("review_sync", &json!({})).unwrap();
    let h = again.call("review_history", &json!({})).unwrap();
    assert_eq!(h["rounds"].as_array().unwrap().len(), 1, "{h}");
    assert_eq!(h["rounds"][0]["number"], 1);
}

#[test]
fn search_finds_scenes_lines_and_notes_across_rounds() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    note_on_word(&mut svc, &line, 0, "the canteen hum is too low");
    let r = svc
        .call("review_search", &json!({ "q": "canteen" }))
        .unwrap();
    assert_eq!(r["scenes"].as_array().unwrap().len(), 1);
    assert_eq!(r["notes"].as_array().unwrap().len(), 1);
    let r = svc
        .call("review_search", &json!({ "q": "SANDWICHES" }))
        .unwrap();
    assert_eq!(r["lines"].as_array().unwrap().len(), 1);
    assert_eq!(r["lines"][0]["scene_number"], 2);
}

#[test]
fn a_review_comes_in_once_however_often_it_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 1, "take 3 was better");

    // Anna's rolling file is there, under her name
    let rolling = reviews(&svc).join("anna-weber.cutawayreview");
    assert!(rolling.exists());
    let f = ReviewFile::decode(&std::fs::read(&rolling).unwrap()).unwrap();
    assert_eq!(f.manifest.reviewer.name, "Anna Weber");
    assert_eq!(f.manifest.events, 1);

    // a copy of the episode from before she wrote: the file brings it in
    let other_dir = tempfile::tempdir().unwrap();
    let copy = other_dir.path().join("Copy.cutaway");
    apc_core::store::EpisodeStore::copy_file(&svc.paths.file, &copy).unwrap();
    let mut other = Service::open(&copy, "test").unwrap();
    // it's the same episode, so no id can tell them apart; take her note out first
    let id = n["id"].as_str().unwrap().to_string();
    other
        .bus
        .store()
        .conn()
        .execute("DELETE FROM entity WHERE kind='review_event'", [])
        .unwrap();
    other.bus.reload().unwrap();
    assert!(other.call("list_notes", &json!({})).unwrap()["count"] == 0);

    let sent = other_dir.path().join("anna.cutawayreview");
    std::fs::copy(&rolling, &sent).unwrap();
    let r = other
        .call("add_review", &json!({ "path": sent.to_string_lossy() }))
        .unwrap();
    assert_eq!(r["notes"], 1);
    assert_eq!(other.call("list_notes", &json!({})).unwrap()["count"], 1);
    assert!(!sent.exists(), "moved to the archive");
    let archived = PathBuf::from(r["archived"].as_str().unwrap());
    assert!(archived.exists());
    assert!(archived.parent().unwrap().ends_with("Imported"));
    let rev = other.bus.rev();

    // again, from another copy: nothing changes, not even the revision
    let again = other_dir.path().join("again.cutawayreview");
    std::fs::copy(&rolling, &again).unwrap();
    let r = other
        .call("add_review", &json!({ "path": again.to_string_lossy() }))
        .unwrap();
    assert_eq!(r["notes"], 0);
    assert_eq!(other.bus.rev(), rev);
    assert_eq!(other.call("list_notes", &json!({})).unwrap()["count"], 1);

    // resolved here, reopened there: whichever came last wins, on both sides
    let id = Value::String(id);
    other
        .call("resolve_note", &json!({ "note": id, "author": rob() }))
        .unwrap();
    assert_eq!(find(&mut other, &id)["status"], "resolved");
    svc.call("reopen_note", &json!({ "note": id, "author": anna() }))
        .unwrap(); // not resolved here yet: nothing
    let rob_file = other.paths.root.join("Reviews/rob.cutawayreview");
    assert!(rob_file.exists());
    let keep = other_dir.path().join("rob.cutawayreview");
    std::fs::copy(&rob_file, &keep).unwrap();
    svc.call("add_review", &json!({ "path": keep.to_string_lossy() }))
        .unwrap();
    let there = find(&mut svc, &id);
    assert_eq!(there["status"], "resolved");
    assert_eq!(there["resolved"]["by"]["name"], "Rob");

    let h = svc.call("review_history", &json!({})).unwrap();
    let a = &h["arrivals"][0];
    assert_eq!(a["who"], "Rob");
    assert_eq!(a["via"], "file");
    assert_eq!(a["changes"], 1);
}

#[test]
fn a_merge_is_nobodys_undo_step() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "mine to keep");

    let other_dir = tempfile::tempdir().unwrap();
    let copy = other_dir.path().join("Copy.cutaway");
    apc_core::store::EpisodeStore::copy_file(&svc.paths.file, &copy).unwrap();
    let mut other = Service::open(&copy, "test").unwrap();
    other
        .call(
            "reply_note",
            &json!({ "note": n["id"], "text": "got it", "author": rob() }),
        )
        .unwrap();

    svc.call("set_gap", &json!({ "line_id": line, "gap_s": 2.0 }))
        .unwrap();
    svc.call("undo", &json!({})).unwrap();
    // an undone step survives a merge, which isn't an edit
    let dest = reviews(&svc).join("rob.cutawayreview");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::copy(other.paths.root.join("Reviews/rob.cutawayreview"), &dest).unwrap();
    let r = svc.call("review_sync", &json!({})).unwrap();
    assert_eq!(r["merged"]["replies"], 1);
    assert_eq!(svc.call("redo", &json!({})).unwrap()["done"], "set_gap");
    // the next undo is that gap, then Anna's note: never Rob's reply
    assert_eq!(svc.call("undo", &json!({})).unwrap()["done"], "set_gap");
    assert_eq!(svc.call("undo", &json!({})).unwrap()["done"], "add_note");
}

#[test]
fn an_undone_note_is_taken_back_on_the_other_machine_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    let n = note_on_word(&mut svc, &line, 0, "oops");
    let file = reviews(&svc).join("anna-weber.cutawayreview");

    let other_dir = tempfile::tempdir().unwrap();
    let copy = other_dir.path().join("Copy.cutaway");
    apc_core::store::EpisodeStore::copy_file(&svc.paths.file, &copy).unwrap();
    let mut other = Service::open(&copy, "test").unwrap();
    assert_eq!(other.call("list_notes", &json!({})).unwrap()["count"], 1);

    svc.call("undo", &json!({})).unwrap();
    assert_eq!(svc.call("list_notes", &json!({})).unwrap()["count"], 0);
    let dest = other.paths.root.join("Reviews/anna-weber.cutawayreview");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::copy(&file, &dest).unwrap();
    other.call("review_sync", &json!({})).unwrap();
    assert_eq!(
        other.call("list_notes", &json!({})).unwrap()["count"],
        0,
        "gone there too"
    );

    svc.call("redo", &json!({})).unwrap();
    assert_eq!(find(&mut svc, &n["id"])["text"], "oops");
    std::fs::copy(&file, &dest).unwrap();
    other.call("review_sync", &json!({})).unwrap();
    assert_eq!(
        other.call("list_notes", &json!({})).unwrap()["count"],
        1,
        "and back"
    );
}

#[test]
fn a_file_of_another_project_is_said_to_be_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = demo_episode(dir.path());
    let (line, _) = first_line(&mut a);
    note_on_word(&mut a, &line, 0, "elsewhere");
    let path = reviews(&a).join("anna-weber.cutawayreview");

    let dir2 = tempfile::tempdir().unwrap();
    let mut b = demo::create(dir2.path(), "test", true).unwrap();
    let e = b
        .call("add_review", &json!({ "path": path.to_string_lossy() }))
        .unwrap_err();
    assert!(e.to_string().contains("not "), "{e}");
    assert_eq!(b.call("list_notes", &json!({})).unwrap()["count"], 0);
    assert!(path.exists(), "a refused file stays where it was");

    let r = b
        .call(
            "add_review",
            &json!({ "path": path.to_string_lossy(), "anyway": true }),
        )
        .unwrap();
    assert_eq!(r["detached"], 1);
    assert!(r["notice"].as_str().unwrap().contains("detached"));
    let all = b.call("list_notes", &json!({})).unwrap();
    assert_eq!(all["threads"][0]["notes"][0]["detached"], true);
}

#[test]
fn sending_a_review_writes_one_file_of_that_person() {
    let _turn = one_reviewer();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    note_on_word(&mut svc, &line, 0, "from anna");
    svc.call(
        "add_note",
        &json!({ "scene": "1", "text": "from rob", "author": rob() }),
    )
    .unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();

    let e = svc.call("send_review", &json!({ "path": out.to_string_lossy() }));
    assert!(e.is_err(), "nobody's reviewing yet");

    svc.call(
        "set_reviewer",
        &json!({ "name": "Anna Weber", "role": "director", "remember": false }),
    )
    .unwrap();
    let r = svc
        .call("send_review", &json!({ "path": out.to_string_lossy() }))
        .unwrap();
    assert_eq!(r["notes"], 1);
    let f = ReviewFile::decode(&std::fs::read(r["file"].as_str().unwrap()).unwrap()).unwrap();
    assert!(r["file"]
        .as_str()
        .unwrap()
        .ends_with("anna-weber.cutawayreview"));
    assert_eq!(f.events.len(), 1);
    assert_eq!(f.manifest.reviewer.name, "Anna Weber");
}

#[test]
fn a_reviewer_is_kept_only_if_asked() {
    let _turn = one_reviewer();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    svc.call(
        "set_reviewer",
        &json!({ "name": "Jonas Brandt", "role": "producer", "remember": false }),
    )
    .unwrap();
    let v = svc.call("reviewer", &json!({})).unwrap();
    assert_eq!(v["current"]["initials"], "JB");
    assert_eq!(v["remembered"], false);

    let file = svc.paths.file.clone();
    svc.close().unwrap();
    drop(svc);
    let mut again = Service::open(&file, "test").unwrap();
    let v = again.call("reviewer", &json!({})).unwrap();
    assert_eq!(v["current"], Value::Null, "not remembered");
    assert_eq!(v["last"]["name"], "Jonas Brandt", "but last here");

    again
        .call(
            "set_reviewer",
            &json!({ "name": "Anna Weber", "role": "director", "remember": true }),
        )
        .unwrap();
    again.close().unwrap();
    drop(again);
    let mut third = Service::open(&file, "test").unwrap();
    let v = third.call("reviewer", &json!({})).unwrap();
    assert_eq!(v["current"]["name"], "Anna Weber");
    assert_eq!(v["remembered"], true);
    // it's this computer's, not the episode's: the next test starts clean
    third
        .call(
            "set_reviewer",
            &json!({ "name": "Anna Weber", "role": "director", "remember": false }),
        )
        .unwrap();
}

fn bytes_of(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}

fn listing(p: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in walk(p) {
        out.push(e.strip_prefix(p).unwrap().to_string_lossy().into_owned());
    }
    out.sort();
    out
}

fn walk(p: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(p).unwrap().flatten() {
        let path = e.path();
        if path.is_dir() {
            out.extend(walk(&path));
        }
        out.push(path);
    }
    out
}

#[test]
fn a_package_opened_for_review_is_never_written_to() {
    let _turn = one_reviewer();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    svc.call("start_review_round", &json!({ "scenes": ["1"] }))
        .unwrap();
    let file = svc.paths.file.clone();
    let root = svc.paths.root.clone();
    svc.close().unwrap();
    drop(svc);

    let before = bytes_of(&file);
    let files = listing(&root);
    assert!(!files.iter().any(|f| f.ends_with("-wal")), "closed clean");

    let mut s = Session::new("ui");
    s.call("open_review", &json!({ "path": file.to_string_lossy() }))
        .unwrap();
    let st = s.call("project_status", &json!({})).unwrap();
    assert_eq!(st["review"]["active"], true);
    assert_eq!(st["review"]["package"], true);
    assert_eq!(st["path"], file.to_string_lossy().as_ref());

    // it plays, and it can't be edited
    assert!(s
        .call("play", &json!({ "scene": "1", "from_s": 0.0 }))
        .is_ok());
    s.call("stop", &json!({})).unwrap();
    let e = s
        .call("set_gap", &json!({ "line_id": line, "gap_s": 1.0 }))
        .unwrap_err();
    assert_eq!(e.code(), "review_only");
    let e = s
        .call(
            "create_show",
            &json!({ "root": dir.path().join("x").to_string_lossy() }),
        )
        .unwrap_err();
    assert_eq!(e.code(), "review_only");

    s.call(
        "set_reviewer",
        &json!({ "name": "Albert", "role": "producer", "remember": false }),
    )
    .unwrap();
    let n = s
        .call(
            "add_note",
            &json!({ "line_id": line, "word": 0, "text": "nice" }),
        )
        .unwrap();
    assert_eq!(n["author"]["name"], "Albert");
    s.call("close_project", &json!({})).unwrap();
    drop(s);

    assert_eq!(bytes_of(&file), before, "the episode file is as it was");
    let mut now = listing(&root);
    assert!(
        now.contains(&"Reviews/albert.cutawayreview".to_string()),
        "{now:?}"
    );
    now.retain(|f| !f.starts_with("Reviews"));
    let mut was = files.clone();
    was.retain(|f| !f.starts_with("Reviews"));
    assert_eq!(
        now, was,
        "nothing beside it changed but the reviewer's own file"
    );

    // the editor picks it up from the folder
    let mut ed = Service::open(&file, "test").unwrap();
    assert_eq!(ed.call("list_notes", &json!({})).unwrap()["count"], 0);
    let r = ed.call("review_sync", &json!({})).unwrap();
    assert_eq!(r["merged"]["notes"], 1);
    let h = ed.call("review_history", &json!({})).unwrap();
    assert_eq!(h["arrivals"][0]["who"], "Albert");
    assert_eq!(h["arrivals"][0]["via"], "folder");
    assert_eq!(h["arrivals"][0]["notes"], 1);
    let again = ed.call("review_sync", &json!({})).unwrap();
    assert_eq!(again["merged"]["notes"], 0);
}

#[test]
fn a_review_session_hears_the_editor_and_keeps_what_it_said() {
    let _turn = one_reviewer();
    let dir = tempfile::tempdir().unwrap();
    let mut ed = demo_episode(dir.path());
    let (line, start) = first_line(&mut ed);
    let file = ed.paths.file.clone();

    let mut s = Session::new("ui");
    s.call("open_review", &json!({ "path": file.to_string_lossy() }))
        .unwrap();
    s.call(
        "set_reviewer",
        &json!({ "name": "Anna Weber", "role": "director", "remember": false }),
    )
    .unwrap();
    let n = s
        .call(
            "add_note",
            &json!({ "line_id": line, "word": 0, "text": "tighter" }),
        )
        .unwrap();
    let was = n["start_s"].as_f64().unwrap();
    assert!((was - start).abs() < 0.5);

    // the editor merges it, moves the line, resolves the note
    ed.call("review_sync", &json!({})).unwrap();
    ed.call("set_gap", &json!({ "line_id": line, "gap_s": 4.0 }))
        .unwrap();
    ed.call("resolve_note", &json!({ "note": n["id"], "author": rob() }))
        .unwrap();
    ed.bus.store().checkpoint().unwrap();

    // the session picks up the new cut and the resolve, with its note still there
    let r = s.call("review_sync", &json!({})).unwrap();
    assert_eq!(r["reloaded"], true, "{r}");
    let all = s.call("list_notes", &json!({})).unwrap();
    let got = &all["threads"][0]["notes"][0];
    assert_eq!(got["status"], "resolved");
    assert_eq!(got["resolved"]["by"]["name"], "Rob");
    assert!(got["start_s"].as_f64().unwrap() > was + 1.0, "{got}");

    // reopening there shows as open here
    s.call("reopen_note", &json!({ "note": n["id"] })).unwrap();
    ed.call("review_sync", &json!({})).unwrap();
    assert_eq!(find(&mut ed, &n["id"])["status"], "reopened");
    let v = ed.call("scene_view", &json!({ "scene": "1" })).unwrap();
    assert_eq!(v["notes"].as_array().unwrap().len(), 1);
    assert_eq!(v["scene"]["open_notes"], 1);
}

#[test]
fn the_export_page_says_a_locked_scene_has_open_notes() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    svc.call("lock_scene", &json!({ "scene": "1" })).unwrap();
    let n = note_on_word(&mut svc, &line, 0, "on a locked scene");
    assert_eq!(n["detached"], false, "notes still attach");
    let st = svc.call("export_status", &json!({})).unwrap();
    let w = st["warnings"].as_array().unwrap();
    assert!(
        w.iter()
            .any(|x| x.as_str().unwrap().contains("locked with 1 open note")),
        "{w:?}"
    );
    svc.call("resolve_note", &json!({ "note": n["id"], "author": rob() }))
        .unwrap();
    let st = svc.call("export_status", &json!({})).unwrap();
    assert!(st["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn notes_from_several_people_on_one_word_are_one_thread_with_several_voices() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let (line, _) = first_line(&mut svc);
    note_on_word(&mut svc, &line, 1, "first voice");
    svc.call("add_note", &json!({ "line_id": line, "word": 0, "to_word": 2, "text": "second voice", "author": rob() })).unwrap();
    let all = svc.call("list_notes", &json!({})).unwrap();
    let t = all["threads"].as_array().unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0]["voices"], 2);
    assert_eq!(t[0]["open"], 2);
}

/// Every file under a folder with what's in it, leaving out the review files.
fn contents(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = walk(root)
        .into_iter()
        .filter(|p| p.is_file())
        .map(|p| {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            (rel, std::fs::read(&p).unwrap())
        })
        .filter(|(rel, _)| !rel.starts_with("Reviews"))
        .collect();
    out.sort();
    out
}

#[test]
fn reviewing_a_show_writes_nothing_but_the_reviewers_own_file() {
    let _turn = one_reviewer();
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tm-2610");
    for ep in ["Episode 1", "Episode 2"] {
        std::fs::create_dir_all(root.join("Episodes").join(ep).join("Sound")).unwrap();
    }
    std::fs::write(root.join("library.jetplane.json"), r#"{"kind":"library"}"#).unwrap();
    let script = dir.path().join("script.fountain");
    std::fs::write(&script, demo::SCRIPT).unwrap();

    let mut ed = Session::new("ui");
    let root_s = root.to_string_lossy().into_owned();
    ed.call("create_show", &json!({ "root": root_s })).unwrap();
    for ep in ["Episode 1", "Episode 2"] {
        ed.call("open_episode", &json!({ "episode": ep, "create": true }))
            .unwrap();
        ed.call(
            "import_script",
            &json!({ "path": script.to_string_lossy() }),
        )
        .unwrap();
        ed.call("start_review_round", &json!({ "scenes": ["1"] }))
            .unwrap();
    }
    ed.shutdown();
    ed.call("close_show", &json!({})).unwrap();
    drop(ed);
    let before = contents(&root);

    // Albert comes in by the show, as a reviewer on a synced share does
    let mut s = Session::new("ui");
    let r = s.call("open_review", &json!({ "path": root_s })).unwrap();
    assert!(r["show"]["episodes"].as_array().unwrap().len() >= 2);
    let e = s
        .call("set_show_backdrop", &json!({ "path": "/x.jpg" }))
        .unwrap_err();
    assert_eq!(e.code(), "review_only");
    let e = s.call("rescan_library", &json!({})).unwrap_err();
    assert_eq!(e.code(), "review_only");
    s.call("open_episode", &json!({ "episode": "Episode 2" }))
        .unwrap();
    assert_eq!(
        s.call("project_status", &json!({})).unwrap()["review"]["package"],
        true
    );
    s.call(
        "set_reviewer",
        &json!({ "name": "Albert", "role": "producer", "remember": false }),
    )
    .unwrap();
    s.call("add_note", &json!({ "scene": "1", "text": "on two" }))
        .unwrap();
    assert!(
        !walk(&root)
            .iter()
            .any(|p| p.to_string_lossy().ends_with(".lock")),
        "no lock taken"
    );
    s.call("close_project", &json!({})).unwrap();

    // the show is still open for review: the next episode opens the same way
    s.call("open_episode", &json!({ "episode": "Episode 1" }))
        .unwrap();
    assert_eq!(
        s.call("project_status", &json!({})).unwrap()["review"]["active"],
        true
    );
    s.call(
        "set_reviewer",
        &json!({ "name": "Albert", "role": "producer", "remember": false }),
    )
    .unwrap();
    s.call("add_note", &json!({ "scene": "1", "text": "on one" }))
        .unwrap();
    s.shutdown();
    drop(s);

    assert_eq!(contents(&root), before, "the show is as it was");

    // one rolling file for the person, with both episodes in it
    let rolling = root.join("Reviews/albert.cutawayreview");
    let f = ReviewFile::decode(&std::fs::read(&rolling).unwrap()).unwrap();
    assert_eq!(f.manifest.episodes.len(), 2);
    assert_eq!(f.events.len(), 2);

    // and each episode takes only what's its own
    let mut ed = Session::new("test");
    ed.call("open_show", &json!({ "path": root_s })).unwrap();
    ed.call("open_episode", &json!({ "episode": "Episode 2" }))
        .unwrap();
    ed.call("review_sync", &json!({})).unwrap();
    let all = ed.call("list_notes", &json!({})).unwrap();
    assert_eq!(all["count"], 1);
    assert_eq!(all["threads"][0]["notes"][0]["text"], "on two");

    // the folder can move, and the show remembers where
    let shared = dir.path().join("elsewhere");
    std::fs::create_dir_all(&shared).unwrap();
    let r = ed
        .call(
            "set_reviews_folder",
            &json!({ "path": shared.to_string_lossy() }),
        )
        .unwrap();
    assert_eq!(r["folder"], shared.to_string_lossy().as_ref());
    ed.call(
        "reply_note",
        &json!({ "note": all["threads"][0]["notes"][0]["id"], "text": "ok", "author": rob() }),
    )
    .unwrap();
    assert!(shared.join("rob.cutawayreview").exists());
}
