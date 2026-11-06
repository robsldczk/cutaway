//! Studio mode (D64–D68): a show inside a production pipeline shaped like tm-2610, its
//! episodes in `Episodes/<Episode>/Sound/`, buckets as folders of the show's `Sound/`
//! library, the lock and sync warnings, and copying a standalone episode in.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn env(local: &Path) {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::set_var("CUTAWAY_DATA_DIR", local);
    std::env::remove_var("EPIDEMIC_SOUND_API_KEY");
}

fn wav(p: &Path, hz: f32) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let sr = 48000u32;
    let s: Vec<f32> = (0..sr / 4)
        .map(|i| 0.2 * (i as f32 * hz * std::f32::consts::TAU / sr as f32).sin())
        .collect();
    apc_media::wav::write_wav(p, sr, 1, &s).unwrap();
}

/// A production folder like tm-2610: episodes with pipeline metadata, a sound library
/// with bucket folders, loose files and a music track with its stems.
fn pipeline(dir: &Path) -> PathBuf {
    let root = dir.join("tm-2610");
    for d in [
        "Episodes/Episode 2/Screenplay",
        "Episodes/Episode 2/Sound",
        "Episodes/Episode 0/Scenes",
        "Characters/Paul",
    ] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(
        root.join("Episodes/Episode 2/episode.jetplane.json"),
        r#"{"schemaVersion":1,"kind":"episode","name":"Turkey Time","episodeNumber":2}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("Episodes/Episode 0/episode.jetplane.json"),
        r#"{"schemaVersion":1,"kind":"episode","name":"","episodeNumber":0}"#,
    )
    .unwrap();
    std::fs::write(root.join("library.jetplane.json"), r#"{"kind":"library"}"#).unwrap();
    let s = root.join("Sound");
    wav(
        &s.join("sfx/doors/ES_Doors, Wood, Slam, Squeak - Epidemic Sound.wav"),
        300.0,
    );
    wav(&s.join("sfx/doors/door creak.wav"), 310.0);
    wav(
        &s.join("sfx/ES_Objects, Keys, Item Pick Up - Epidemic Sound.wav"),
        320.0,
    );
    wav(
        &s.join("music/Stings/ES_Horror Sting 01 - Epidemic Sound.wav"),
        330.0,
    );
    wav(&s.join("music/ES_Home Straight - Jon Bjork.wav"), 340.0);
    wav(
        &s.join("music/ES_Home Straight - Jon Bjork/ES_Home Straight STEMS DRUMS - Jon Bjork.wav"),
        350.0,
    );
    std::fs::write(s.join("music/ES_Home Straight - Jon Bjork.zip"), b"zip").unwrap();
    root
}

fn call(s: &mut apc_app::Session, m: &str, p: Value) -> Value {
    s.call(m, &p).unwrap_or_else(|e| panic!("{m}: {e}"))
}

fn bucket<'a>(v: &'a Value, folder: &str) -> &'a Value {
    v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["folder"] == folder)
        .unwrap_or_else(|| panic!("no bucket for {folder}: {v}"))
}

fn kept_names(b: &Value) -> Vec<String> {
    let mut v: Vec<String> = b["kept"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| {
            k["file"]
                .as_str()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap()
                .to_string()
        })
        .collect();
    v.sort();
    v
}

#[test]
fn a_show_in_a_pipeline_with_folder_buckets() {
    let dir = tempfile::tempdir().unwrap();
    env(&dir.path().join("local"));
    let root = pipeline(dir.path());
    let mut s = apc_app::Session::new("ui");

    // the pipeline becomes a show; its own files stay
    let v = call(
        &mut s,
        "create_show",
        json!({ "root": root.to_string_lossy() }),
    );
    assert_eq!(v["name"], "tm-2610");
    assert!(root.join("tm-2610.cutawayshow").is_file());
    assert_eq!(
        std::fs::read_to_string(root.join("library.jetplane.json")).unwrap(),
        r#"{"kind":"library"}"#
    );
    let labels: Vec<&str> = v["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, vec!["Episode 0", "Episode 2 · Turkey Time"]);
    assert!(v["episodes"][1]["file"].is_null());

    // opening an episode without a project makes one in its Sound folder
    let r = call(&mut s, "open_episode", json!({ "episode": "Episode 2" }));
    assert_eq!(r["created"], true);
    let ep_dir = root.join("Episodes/Episode 2/Sound");
    assert!(ep_dir.join("Episode 2.cutaway").is_file());
    let st = call(&mut s, "project_status", json!({}));
    assert_eq!(st["show"]["name"], "tm-2610");
    assert_eq!(st["show"]["episode"], "Episode 2");
    assert!(st["warnings"].as_array().unwrap().is_empty());
    // the UI holds the episode's lock
    assert!(ep_dir.join(".Episode 2.cutaway.lock").is_file());

    // recordings are copied into the episode's folder
    let demo = dir.path().join("session");
    let wavs = apc_app::demo::write_session(&demo).unwrap();
    call(
        &mut s,
        "import_script",
        json!({ "path": demo.join("Demo Episode.fountain").to_string_lossy() }),
    );
    call(
        &mut s,
        "import_audio",
        json!({ "paths": wavs.iter().map(|w| w.to_string_lossy()).collect::<Vec<_>>(), "copy": false }),
    );
    let copied = walk(&ep_dir.join("Audio"));
    assert_eq!(
        copied
            .iter()
            .filter(|p| p.extension().map(|e| e == "wav").unwrap_or(false))
            .count(),
        3,
        "copied, though asked to reference"
    );
    call(&mut s, "run_alignment", json!({}));

    // the library folders are buckets; a track's stems folder is not
    let b = call(&mut s, "buckets", json!({}));
    assert_eq!(b["show"], "tm-2610");
    let doors = bucket(&b, "sfx/doors");
    assert_eq!(doors["name"], "Doors");
    assert_eq!(
        kept_names(doors),
        vec![
            "ES_Doors, Wood, Slam, Squeak - Epidemic Sound.wav",
            "door creak.wav"
        ]
    );
    assert_eq!(kept_names(bucket(&b, "music/Stings")).len(), 1);
    assert!(b["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| !x["folder"].as_str().unwrap_or("").contains("Home Straight")));

    // a bucket made by hand is a new folder; sounds dropped on it land there
    let creaks = call(&mut s, "create_bucket", json!({ "name": "Creaky floor" }))["bucket_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(root.join("Sound/sfx/creaky-floor").is_dir());
    let drop = dir.path().join("drops/floor creak 01.wav");
    wav(&drop, 400.0);
    call(
        &mut s,
        "import_sounds",
        json!({ "paths": [drop.to_string_lossy()], "bucket_id": creaks }),
    );
    assert!(root
        .join("Sound/sfx/creaky-floor/floor creak 01.wav")
        .is_file());
    // a sound dropped into the library without a bucket goes loose into sfx/
    let loose = dir
        .path()
        .join("drops/ES_Footsteps, Gravel - Epidemic Sound.wav");
    wav(&loose, 410.0);
    call(
        &mut s,
        "import_sounds",
        json!({ "paths": [loose.to_string_lossy()] }),
    );
    assert!(root
        .join("Sound/sfx/ES_Footsteps, Gravel - Epidemic Sound.wav")
        .is_file());

    // keeping a library sound in a bucket puts a copy in its folder
    let lib = call(&mut s, "list_library", json!({ "query": "keys" }));
    let keys = lib[0]["id"].as_str().unwrap().to_string();
    let doors_id = bucket(&call(&mut s, "buckets", json!({})), "sfx/doors")["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut s,
        "bucket_keep",
        json!({ "bucket_id": doors_id, "asset_id": keys }),
    );
    assert!(root
        .join("Sound/sfx/doors/ES_Objects, Keys, Item Pick Up - Epidemic Sound.wav")
        .is_file());

    // taking a folder sound out hides it in this episode only; the file stays
    let b = call(&mut s, "buckets", json!({}));
    let creak = bucket(&b, "sfx/doors")["kept"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["file"].as_str().unwrap().ends_with("door creak.wav"))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut s,
        "bucket_remove",
        json!({ "bucket_id": doors_id, "asset_id": creak }),
    );
    let b = call(&mut s, "buckets", json!({}));
    assert_eq!(kept_names(bucket(&b, "sfx/doors")).len(), 2);
    assert!(root.join("Sound/sfx/doors/door creak.wav").is_file());

    // a folder bucket with sounds can't be deleted from an episode; an empty one can
    let e = s
        .call(
            "delete_bucket",
            &json!({ "bucket_id": doors_id, "confirm": true }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("Finder"), "{e}");
    let empty = call(&mut s, "create_bucket", json!({ "name": "Spare" }))["bucket_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(root.join("Sound/sfx/spare").is_dir());
    call(
        &mut s,
        "delete_bucket",
        json!({ "bucket_id": empty, "confirm": true }),
    );
    assert!(!root.join("Sound/sfx/spare").exists());

    // a folder sound placed on the timeline plays and exports from the show's library
    let door = bucket(&call(&mut s, "buckets", json!({})), "sfx/doors")["kept"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut s,
        "place_from_bucket",
        json!({ "scene": "1", "asset_id": door, "bucket_id": doors_id, "at_s": 1.0 }),
    );
    let x = call(&mut s, "export_fcpxml", json!({ "confirm": true }));
    let xml = std::fs::read_to_string(x["path"].as_str().unwrap()).unwrap();
    assert!(
        xml.contains("tm-2610/Sound/sfx/"),
        "the export points into the show library: {}",
        &xml[..xml.len().min(3000)]
    );

    // new folders from the pipeline become buckets on a rescan
    wav(&root.join("Sound/sfx/whooshes/whoosh 01.wav"), 500.0);
    let r = call(&mut s, "rescan_library", json!({}));
    assert_eq!(r["new_buckets"], 1);
    let b = call(&mut s, "buckets", json!({}));
    assert_eq!(bucket(&b, "sfx/whooshes")["name"], "Whooshes");

    // another episode sees the same buckets, with the sounds saved in this one
    call(&mut s, "close_project", json!({}));
    assert!(
        !ep_dir.join(".Episode 2.cutaway.lock").exists(),
        "lock given back"
    );
    assert!(
        !ep_dir.join("Episode 2.cutaway-wal").exists()
            || std::fs::metadata(ep_dir.join("Episode 2.cutaway-wal"))
                .unwrap()
                .len()
                == 0,
        "log folded in"
    );
    call(&mut s, "open_episode", json!({ "episode": "Episode 0" }));
    assert!(root
        .join("Episodes/Episode 0/Sound/Episode 0.cutaway")
        .is_file());
    let b = call(&mut s, "buckets", json!({}));
    assert_eq!(
        kept_names(bucket(&b, "sfx/creaky-floor")),
        vec!["floor creak 01.wav"]
    );
    assert_eq!(
        kept_names(bucket(&b, "sfx/doors")).len(),
        3,
        "hidden only in Episode 2"
    );

    // the show view marks which episodes have a project
    let v = call(&mut s, "show_view", json!({}));
    assert!(v["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| !e["file"].is_null()));
    call(&mut s, "close_show", json!({}));
    assert!(s.call("project_status", &json!({})).is_err());
}

#[test]
fn another_machine_holding_the_episode_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    env(&dir.path().join("local"));
    let root = pipeline(dir.path());
    let mut s = apc_app::Session::new("ui");
    call(
        &mut s,
        "create_show",
        json!({ "root": root.to_string_lossy() }),
    );
    call(&mut s, "open_episode", json!({ "episode": "Episode 2" }));
    call(&mut s, "close_project", json!({}));
    let ep = root.join("Episodes/Episode 2/Sound");
    std::fs::write(
        ep.join(".Episode 2.cutaway.lock"),
        json!({ "host": "Studio iMac", "user": "rob", "since": chrono::Utc::now().to_rfc3339() })
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        ep.join("Episode 2.sync-conflict-20261102-101010-ABCDEFG.cutaway"),
        b"x",
    )
    .unwrap();
    call(
        &mut s,
        "open_episode",
        json!({ "episode": "Episode 2", "create": false }),
    );
    let st = call(&mut s, "project_status", json!({}));
    let w: Vec<&str> = st["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(w[0].contains("Studio iMac"));
    assert!(w[1].contains("sync-conflict"));
    // a CLI next to the UI shares the episode without a warning or a lock of its own
    let cli = apc_app::Session::with_project(&ep, "cli").unwrap();
    assert!(cli.show.is_some());
}

#[test]
fn a_standalone_episode_is_copied_into_the_show() {
    let dir = tempfile::tempdir().unwrap();
    env(&dir.path().join("local"));
    let root = pipeline(dir.path());

    // a standalone episode with recordings, an analysis and a library sound
    let standalone = dir.path().join("Cutaway");
    let mut s = apc_app::Session::new("test");
    call(
        &mut s,
        "create_episode",
        json!({ "parent": standalone.to_string_lossy(), "name": "Tarmac-EP02" }),
    );
    let demo = dir.path().join("session");
    let wavs = apc_app::demo::write_session(&demo).unwrap();
    call(
        &mut s,
        "import_script",
        json!({ "path": demo.join("Demo Episode.fountain").to_string_lossy() }),
    );
    call(
        &mut s,
        "import_audio",
        json!({ "paths": wavs.iter().map(|w| w.to_string_lossy()).collect::<Vec<_>>(), "copy": true }),
    );
    call(&mut s, "run_alignment", json!({}));
    let hatch = call(&mut s, "create_bucket", json!({ "name": "Hatches" }))["bucket_id"]
        .as_str()
        .unwrap()
        .to_string();
    let snd = dir.path().join("drops/hatch slam.wav");
    wav(&snd, 600.0);
    call(
        &mut s,
        "import_sounds",
        json!({ "paths": [snd.to_string_lossy()], "bucket_id": hatch }),
    );
    let src_root = standalone.join("Tarmac-EP02");
    assert!(src_root.join("Library").read_dir().unwrap().count() > 0);
    call(&mut s, "close_project", json!({}));
    let src_file = src_root.join("Tarmac-EP02.cutaway");
    let before = std::fs::read(&src_file).unwrap();
    let listing = |p: &Path| {
        let mut v: Vec<String> = walk(p)
            .into_iter()
            .map(|f| f.strip_prefix(p).unwrap().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    let src_listing = listing(&src_root);

    let e = s
        .call("open_show", &json!({ "path": root.to_string_lossy() }))
        .unwrap_err();
    assert!(e.to_string().contains("show file"), "not a show yet: {e}");
    call(
        &mut s,
        "create_show",
        json!({ "root": root.to_string_lossy() }),
    );

    // a dry run first
    let dry = call(
        &mut s,
        "migrate_episode",
        json!({ "source": src_root.to_string_lossy(), "episode": "Episode 2", "dry_run": true }),
    );
    assert_eq!(dry["dry_run"], true);
    assert_eq!(
        dry["plan"]["sounds_to_show_library"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(!root
        .join("Episodes/Episode 2/Sound/Episode 2.cutaway")
        .exists());

    let r = call(
        &mut s,
        "migrate_episode",
        json!({ "source": src_root.to_string_lossy(), "episode": "Episode 2" }),
    );
    assert_eq!(r["missing_sources"], 0, "{r}");
    assert!(r["missing_sounds"].as_array().unwrap().is_empty(), "{r}");
    assert_eq!(r["sounds_moved"], 1);
    let target = root.join("Episodes/Episode 2/Sound");
    assert!(target.join("Episode 2.cutaway").is_file());
    assert!(target.join("Audio").read_dir().unwrap().count() > 0);
    // the bucket's sound is in the show's library, in the bucket's folder
    assert!(root.join("Sound/sfx/hatches/hatch slam.wav").is_file());
    // the source is untouched
    assert_eq!(std::fs::read(&src_file).unwrap(), before);
    assert_eq!(listing(&src_root), src_listing);
    // a second copy into the same episode is refused
    assert!(s
        .call(
            "migrate_episode",
            &json!({ "source": src_root.to_string_lossy(), "episode": "Episode 2" })
        )
        .is_err());

    // the copy opens as a show episode with its takes and the bucket in its folder
    call(
        &mut s,
        "open_episode",
        json!({ "episode": "Episode 2", "create": false }),
    );
    let st = call(&mut s, "project_status", json!({}));
    assert_eq!(st["name"], "Episode 2");
    assert_eq!(st["missing_sources"], 0);
    assert!(st["takes"].as_u64().unwrap() > 0);
    let b = call(&mut s, "buckets", json!({}));
    let h = b["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Hatches")
        .unwrap();
    assert_eq!(h["folder"], "sfx/hatches");
    assert_eq!(kept_names(h), vec!["hatch slam.wav"]);
    assert_eq!(
        h["kept"].as_array().unwrap().len(),
        1,
        "one sound, not a copy per place"
    );
}

fn walk(p: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let q = e.path();
            if q.is_dir() {
                out.extend(walk(&q));
            } else {
                out.push(q);
            }
        }
    }
    out
}
