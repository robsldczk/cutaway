//! The welcome screen's promises: looking touches nothing, the counts are the consolidated set
//! (what's waiting in the shared folder included), making something doesn't open it, covers
//! live in the project, and a dropped review finds its episode.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use apc_app::{demo, Service, Session};

fn with_mock() {
    static LOCAL: OnceLock<PathBuf> = OnceLock::new();
    let local = LOCAL.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("cutaway-welcome-{}", std::process::id()));
        std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
        std::env::set_var("CUTAWAY_NO_AUDIO", "1");
        std::env::set_var("CUTAWAY_SECRETS", "memory");
        std::env::set_var("CUTAWAY_DATA_DIR", &d);
        d
    });
    assert!(!local.as_os_str().is_empty());
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn episode(path: &Path) -> Value {
    json!({ "path": s(path), "name": "" })
}

fn show(path: &Path) -> Value {
    json!({ "path": s(path), "name": "", "show": true })
}

fn shelf(sess: &mut Session, recents: Vec<Value>) -> Value {
    sess.call("welcome_view", &json!({ "recents": recents }))
        .unwrap()
}

/// Every file under a folder with its bytes.
fn snapshot(root: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for e in std::fs::read_dir(root).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            snapshot(&p, into);
        } else {
            into.insert(p.clone(), std::fs::read(&p).unwrap());
        }
    }
}

/// A small two-colour PNG, wider than tall.
fn png(path: &Path, w: u32, h: u32) {
    let img = image::RgbImage::from_fn(w, h, |x, _| {
        if x < w / 2 {
            image::Rgb([200, 60, 30])
        } else {
            image::Rgb([30, 70, 190])
        }
    });
    img.save(path).unwrap();
}

/// A note on the first line, by someone, written to the review file a package leaves. Who's
/// reviewing is this computer's, so tests take turns.
fn note_in_package(file: &Path, who: &str) {
    static TURN: Mutex<()> = Mutex::new(());
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = Session::new("test");
    s.call("open_review", &json!({ "path": file.to_string_lossy() }))
        .unwrap();
    s.call(
        "set_reviewer",
        &json!({ "name": who, "role": "director", "remember": false }),
    )
    .unwrap();
    s.call("add_note", &json!({ "scene": "1", "text": "louder" }))
        .unwrap();
    s.call("close_project", &json!({})).unwrap();
}

#[test]
fn looking_at_the_shelf_touches_nothing() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let file = svc.paths.file.clone();
    svc.close().unwrap();
    drop(svc);

    let mut before = BTreeMap::new();
    snapshot(dir.path(), &mut before);
    let mut sess = Session::new("test");
    let v = shelf(&mut sess, vec![episode(&file)]);
    let mut after = BTreeMap::new();
    snapshot(dir.path(), &mut after);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    assert!(before == after, "no file changed");

    let e = &v["episodes"][0];
    assert_eq!(e["missing"], false);
    assert!(e["edited"].as_str().is_some(), "{e}");
    assert!(e["stage"].as_u64().unwrap() >= 1, "the demo has takes: {e}");
    assert_eq!(e["open_notes"], 0);
    assert!(sess.service.is_none(), "nothing was opened");
}

#[test]
fn the_stage_follows_the_work() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut sess = Session::new("test");
    let f = sess
        .call(
            "create_episode",
            &json!({ "parent": s(dir.path()), "name": "Fresh", "open": false }),
        )
        .unwrap();
    let fresh = PathBuf::from(f["file"].as_str().unwrap());
    assert_eq!(
        shelf(&mut sess, vec![episode(&fresh)])["episodes"][0]["stage"],
        0
    );

    let mut svc = demo::create(&dir.path().join("demo"), "test", true).unwrap();
    let file = svc.paths.file.clone();
    let item = |svc: &mut Service| {
        svc.call("scene_view", &json!({ "scene": "1" })).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["kind"] == "line")
            .unwrap()["line_id"]
            .clone()
    };
    let line = item(&mut svc);
    let stage =
        |sess: &mut Session| shelf(sess, vec![episode(&file)])["episodes"][0]["stage"].clone();
    // a fade is Edit's work, and a locked scene has been delivered
    svc.call(
        "command",
        &json!({ "cmd": "set_clip_fades", "line_id": line, "fade_in_s": 0.2, "fade_out_s": 0.0 }),
    )
    .unwrap();
    svc.close().unwrap();
    drop(svc);
    assert_eq!(stage(&mut sess), 3);
    let mut svc = Service::open(&file, "test").unwrap();
    svc.call("lock_scene", &json!({ "scene": "1" })).unwrap();
    svc.close().unwrap();
    drop(svc);
    assert_eq!(stage(&mut sess), 4);
}

#[test]
fn counts_are_the_consolidated_set_with_what_waits_in_the_folder() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let file = svc.paths.file.clone();
    svc.close().unwrap();
    drop(svc);

    let mut sess = Session::new("test");
    assert_eq!(
        shelf(&mut sess, vec![episode(&file)])["episodes"][0]["open_notes"],
        0
    );

    // a reviewer's note, not merged yet: the editor sees it here before opening anything
    note_in_package(&file, "Anna Weber");
    let reviews = file.parent().unwrap().join("Reviews");
    assert!(reviews.join("anna-weber.cutawayreview").is_file());
    assert_eq!(
        shelf(&mut sess, vec![episode(&file)])["episodes"][0]["open_notes"],
        1
    );

    // merged and resolved, it's gone from the count; the file still says it was written
    let mut svc = Service::open(&file, "test").unwrap();
    svc.call("review_sync", &json!({})).unwrap();
    let all = svc.call("list_notes", &json!({})).unwrap();
    let id = all["threads"][0]["notes"][0]["id"].clone();
    svc.call(
        "resolve_note",
        &json!({ "note": id, "author": { "name": "Rob", "role": "editor" } }),
    )
    .unwrap();
    svc.close().unwrap();
    drop(svc);
    assert_eq!(
        shelf(&mut sess, vec![episode(&file)])["episodes"][0]["open_notes"],
        0
    );
}

#[test]
fn making_things_opens_nothing() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut sess = Session::new("test");
    let root = dir.path().join("TARMAC");
    sess.call(
        "create_show",
        &json!({ "parent": s(dir.path()), "name": "TARMAC", "open": false }),
    )
    .unwrap();
    assert!(sess.show.is_none(), "the show stays closed");
    let e = sess
        .call(
            "create_episode",
            &json!({ "name": "Lunch Break", "show": s(&root), "open": false }),
        )
        .unwrap();
    assert!(sess.service.is_none(), "the episode stays closed");
    assert!(sess.show.is_none());
    let file = PathBuf::from(e["file"].as_str().unwrap());
    assert_eq!(
        file,
        root.join("Episodes/Lunch Break/Sound/Lunch Break.cutaway")
    );
    let solo = sess
        .call(
            "create_episode",
            &json!({ "name": "Solo", "parent": s(&dir.path().join("New folder")), "open": false }),
        )
        .unwrap();
    let solo = PathBuf::from(solo["file"].as_str().unwrap());
    assert!(solo.is_file());

    let v = shelf(&mut sess, vec![episode(&solo), episode(&file), show(&root)]);
    assert_eq!(v["episodes"][0]["title"], "Solo");
    assert!(v["episodes"][0]["show"].is_null());
    let lunch = &v["episodes"][1];
    assert_eq!(lunch["title"], "Lunch Break");
    assert_eq!(lunch["show_name"], "TARMAC");
    assert_eq!(lunch["folder"], "Lunch Break");
    assert_eq!(
        v["shows"].as_array().unwrap().len(),
        1,
        "one show, seen twice"
    );
    assert_eq!(v["shows"][0]["episodes"][0]["key"], lunch["key"]);
}

#[test]
fn a_shows_episodes_come_with_it_even_without_a_project() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tm-2610");
    std::fs::create_dir_all(root.join("Episodes/Episode 3")).unwrap();
    std::fs::write(
        root.join("Episodes/Episode 3/episode.jetplane.json"),
        r#"{"kind":"episode","name":"Turkey Time","episodeNumber":3}"#,
    )
    .unwrap();
    let mut sess = Session::new("test");
    sess.call("create_show", &json!({ "root": s(&root), "open": false }))
        .unwrap();
    let v = shelf(&mut sess, vec![show(&root)]);
    let e = &v["shows"][0]["episodes"][0];
    assert_eq!(e["title"], "Turkey Time");
    assert_eq!(e["number"], 3);
    assert!(e["file"].is_null(), "no project yet");
    assert_eq!(e["missing"], false);
}

#[test]
fn what_has_gone_says_so_and_keeps_its_name() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("gone/Gone.cutaway");
    let mut sess = Session::new("test");
    let v = shelf(
        &mut sess,
        vec![
            json!({ "path": s(&gone), "name": "Time Burrito" }),
            json!({ "path": s(&dir.path().join("nowhere")), "name": "Old show", "show": true }),
        ],
    );
    assert_eq!(v["episodes"][0]["missing"], true);
    assert_eq!(v["episodes"][0]["title"], "Time Burrito");
    assert_eq!(v["shows"][0]["missing"], true);
    assert_eq!(v["shows"][0]["name"], "Old show");
}

#[test]
fn covers_live_in_the_project_and_fall_back_to_the_show() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut sess = Session::new("test");
    let root = dir.path().join("TARMAC");
    sess.call("create_show", &json!({ "root": s(&root), "open": false }))
        .unwrap();
    let e = sess
        .call(
            "create_episode",
            &json!({ "name": "Pilot", "show": s(&root), "open": false }),
        )
        .unwrap();
    let file = PathBuf::from(e["file"].as_str().unwrap());
    let image = dir.path().join("poster.png");
    png(&image, 3000, 1000);

    let recents = vec![episode(&file), show(&root)];
    let v = shelf(&mut sess, recents.clone());
    assert!(v["episodes"][0]["cover"].is_null(), "a colour wash, then");
    assert!(v["shows"][0]["colors"].as_array().unwrap().is_empty());

    // a show's cover goes in its folder, and gives it colours; its episode wears it too
    sess.call(
        "set_cover",
        &json!({ "target": s(&root), "image": s(&image) }),
    )
    .unwrap();
    assert!(root.join("Cover.jpg").is_file());
    let v = shelf(&mut sess, recents.clone());
    assert_eq!(v["shows"][0]["cover"]["path"], s(&root.join("Cover.jpg")));
    assert_eq!(v["shows"][0]["colors"].as_array().unwrap().len(), 3);
    assert_eq!(v["episodes"][0]["cover"], v["shows"][0]["cover"]);
    assert_eq!(v["episodes"][0]["own_cover"], false);

    // the episode's own wins, and a png sitting there gives way to the new one
    let ep_dir = file.parent().unwrap();
    png(&ep_dir.join("Cover.png"), 40, 20);
    let v = shelf(&mut sess, recents.clone());
    assert_eq!(
        v["episodes"][0]["cover"]["path"],
        s(&ep_dir.join("Cover.png"))
    );
    assert_eq!(v["episodes"][0]["own_cover"], true);
    sess.call(
        "set_cover",
        &json!({ "target": s(&file), "image": s(&image) }),
    )
    .unwrap();
    assert!(ep_dir.join("Cover.jpg").is_file());
    assert!(!ep_dir.join("Cover.png").exists());

    // stored no bigger than 2400 px, and scaled for a tile on the way out
    let stored = image::open(root.join("Cover.jpg")).unwrap();
    assert_eq!(stored.width(), 2400);
    let url = sess
        .call(
            "cover_image",
            &json!({ "path": s(&root.join("Cover.jpg")), "px": 480 }),
        )
        .unwrap();
    assert!(url["url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));

    // not an image: said, and nothing's replaced
    let bad = dir.path().join("notes.txt");
    std::fs::write(&bad, "hello").unwrap();
    let e = sess
        .call(
            "set_cover",
            &json!({ "target": s(&root), "image": s(&bad) }),
        )
        .unwrap_err();
    assert!(e.to_string().contains("image"), "{e}");
    assert!(root.join("Cover.jpg").is_file());
}

#[test]
fn a_shows_cover_is_its_backdrop_until_it_has_one_of_its_own() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("TARMAC");
    let mut sess = Session::new("test");
    sess.call("create_show", &json!({ "root": s(&root) }))
        .unwrap();
    let image = dir.path().join("poster.png");
    png(&image, 64, 32);
    sess.call(
        "set_cover",
        &json!({ "target": s(&root), "image": s(&image) }),
    )
    .unwrap();
    let b = sess.call("show_backdrop", &json!({})).unwrap();
    assert!(
        b["url"].as_str().unwrap().starts_with("data:image/jpeg"),
        "{b}"
    );
    sess.call("close_show", &json!({})).unwrap();

    std::fs::remove_file(root.join("Cover.jpg")).unwrap();
    sess.call("open_show", &json!({ "path": s(&root) }))
        .unwrap();
    sess.call("set_show_backdrop", &json!({ "path": s(&image) }))
        .unwrap();
    sess.call("close_show", &json!({})).unwrap();
    let v = shelf(&mut sess, vec![show(&root)]);
    assert!(v["shows"][0]["cover"]["path"]
        .as_str()
        .unwrap()
        .ends_with("backdrop.jpg"));
}

#[test]
fn a_dropped_review_finds_its_episode_and_leaves_nothing_open() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let file = svc.paths.file.clone();
    svc.close().unwrap();
    drop(svc);
    note_in_package(&file, "Anna Weber");
    let rolling = file
        .parent()
        .unwrap()
        .join("Reviews/anna-weber.cutawayreview");
    let sent = dir.path().join("sent.cutawayreview");
    std::fs::rename(&rolling, &sent).unwrap();

    let mut sess = Session::new("test");
    let r = sess
        .call(
            "add_review_to",
            &json!({ "path": s(&sent), "recents": [episode(&file)] }),
        )
        .unwrap();
    assert_eq!(r["who"], "Anna Weber");
    assert_eq!(r["notes"], 1);
    assert!(sess.service.is_none(), "nothing stays open");
    assert!(!sent.exists(), "archived, as Add review does");
    let v = shelf(&mut sess, vec![episode(&file)]);
    assert_eq!(v["episodes"][0]["open_notes"], 1);

    // a review of something that isn't here
    let other = tempfile::tempdir().unwrap();
    let mut o = demo::create(other.path(), "test", true).unwrap();
    let other_file = o.paths.file.clone();
    o.close().unwrap();
    drop(o);
    note_in_package(&other_file, "Albert");
    let path = other_file
        .parent()
        .unwrap()
        .join("Reviews/albert.cutawayreview");
    let e = sess
        .call(
            "add_review_to",
            &json!({ "path": s(&path), "recents": [episode(&file)] }),
        )
        .unwrap_err();
    assert!(e.to_string().contains("isn't here"), "{e}");
    assert!(path.exists(), "left where it was");
}
