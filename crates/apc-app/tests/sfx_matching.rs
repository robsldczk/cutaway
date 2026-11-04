//! Show sounds reach the buckets by file name (D74): matched to what a bucket's cues or
//! keywords ask for, loose sounds nothing wants in their own bucket, show-wide buckets in
//! every scene, and the clean up that moves loose sounds into folders on request.

use std::path::Path;

use serde_json::{json, Value};

/// Each file gets its own pitch: the same audio would be the same sound in the library.
fn wav(p: &Path, secs: f32) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let sr = 48000u32;
    let hz = 200.0 + p.to_string_lossy().bytes().map(f32::from).sum::<f32>() % 900.0;
    let s: Vec<f32> = (0..(sr as f32 * secs) as u32)
        .map(|i| 0.2 * (i as f32 * hz * std::f32::consts::TAU / sr as f32).sin())
        .collect();
    apc_media::wav::write_wav(p, sr, 1, &s).unwrap();
}

fn call(s: &mut apc_app::Session, m: &str, p: Value) -> Value {
    s.call(m, &p).unwrap_or_else(|e| panic!("{m}: {e}"))
}

fn named<'a>(v: &'a Value, name: &str) -> &'a Value {
    v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == name)
        .unwrap_or_else(|| panic!("no bucket {name}: {v}"))
}

fn has(v: &Value, name: &str) -> bool {
    v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["name"] == name)
}

fn sounds(b: &Value) -> Vec<String> {
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

const KEYS: &str = "ES_Objects, Keys, Item Pick Up - Epidemic Sound.wav";
const JACKET: &str = "ES_Cloth, Puffer Jacket - Epidemic Sound.wav";
const HISS: &str = "ES_Steam, Pipe Hiss - Epidemic Sound.wav";

#[test]
fn show_sounds_reach_the_buckets_and_the_clean_up_files_them() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::set_var("CUTAWAY_DATA_DIR", dir.path().join("local"));

    let root = dir.path().join("tm-2610");
    std::fs::create_dir_all(root.join("Episodes/Episode 2/Sound")).unwrap();
    let sfx = root.join("Sound/sfx");
    wav(
        &sfx.join("kitchen/ES_Drawers, Wood, Close - Epidemic Sound.wav"),
        0.2,
    );
    wav(
        &sfx.join("radio/ES_Communications, Radio, Static - Epidemic Sound.wav"),
        0.2,
    );
    wav(&sfx.join(KEYS), 0.2);
    wav(&sfx.join(JACKET), 0.2);
    wav(&sfx.join(HISS), 0.2);
    wav(
        &sfx.join("ES_Kitchen, Fridge Hum, Bed - Epidemic Sound.wav"),
        11.0,
    );
    wav(
        &sfx.join("ES_Kitchen, Spoon Drop - Epidemic Sound.wav"),
        0.2,
    );

    let mut s = apc_app::Session::new("ui");
    call(
        &mut s,
        "create_show",
        json!({ "root": root.to_string_lossy() }),
    );
    call(&mut s, "open_episode", json!({ "episode": "Episode 2" }));
    let demo = dir.path().join("session");
    apc_app::demo::write_session(&demo).unwrap();
    call(
        &mut s,
        "import_script",
        json!({ "path": demo.join("Demo Episode.fountain").to_string_lossy() }),
    );

    // nothing asks for the loose sounds yet: they all wait in their own bucket
    let b = call(&mut s, "buckets", json!({}));
    assert_eq!(b["loose"], 5);
    assert_eq!(
        sounds(named(&b, "Loose SFX")).len(),
        3,
        "kitchen's keyword claims two: {b}"
    );
    let k = sounds(named(&b, "Kitchen"));
    assert!(
        k.iter().any(|n| n.contains("Spoon Drop")),
        "a folder bucket's keyword: {k:?}"
    );

    // a show sound can be heard from its bucket, and listening adds nothing to the episode
    let spoon = named(&b, "Kitchen")["kept"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["name"].as_str().unwrap().contains("Spoon Drop"))
        .unwrap()["id"]
        .clone();
    let rev = call(&mut s, "project_status", json!({}))["rev"].clone();
    let played = call(&mut s, "play_sound", json!({ "asset_id": spoon }));
    assert_eq!(played["playing"], true);
    call(&mut s, "stop", json!({}));
    assert_eq!(call(&mut s, "project_status", json!({}))["rev"], rev);
    let gone = s.call("play_sound", &json!({ "asset_id": "nothing" }));
    assert!(gone.is_err(), "an unknown sound still says so");

    // and it can loop under a scene: it joins the episode for that
    call(
        &mut s,
        "set_scene_tone",
        json!({ "scene": "1", "asset_id": spoon }),
    );
    let rooms = call(&mut s, "scene_rooms", json!({}));
    assert_eq!(rooms[0]["tone"]["asset_id"], spoon);
    assert!(rooms[0]["tone"]["name"]
        .as_str()
        .unwrap()
        .contains("Spoon Drop"));
    call(&mut s, "set_scene_tone", json!({ "scene": "1" }));

    // a hand-made bucket's keyword picks a loose sound out, without touching the file
    let id = call(
        &mut s,
        "create_bucket",
        json!({ "name": "Key jingles", "keywords": ["keys"] }),
    )["bucket_id"]
        .as_str()
        .unwrap()
        .to_string();
    let b = call(&mut s, "buckets", json!({}));
    let keys = named(&b, "Key jingles");
    assert_eq!(sounds(keys), vec![KEYS]);
    assert_eq!(keys["kept"][0]["matched"], true);
    assert!(!sounds(named(&b, "Loose SFX")).contains(&KEYS.to_string()));
    assert!(sfx.join(KEYS).is_file() && !sfx.join("key-jingles").join(KEYS).exists());

    // taking a match out hides it in this episode; the file stays, and it is loose again
    let asset = keys["kept"][0]["id"].as_str().unwrap().to_string();
    call(
        &mut s,
        "bucket_remove",
        json!({ "bucket_id": id, "asset_id": asset }),
    );
    let b = call(&mut s, "buckets", json!({}));
    assert!(sounds(named(&b, "Key jingles")).is_empty());
    assert!(sounds(named(&b, "Loose SFX")).contains(&KEYS.to_string()));
    assert!(sfx.join(KEYS).is_file());

    // a show-wide bucket with sounds is offered in every scene; an empty one isn't
    let scene = call(&mut s, "buckets", json!({ "scene": "1" }));
    assert_eq!(named(&scene, "Radio")["show_wide"], true);
    assert!(!has(&scene, "Key jingles"));

    // the clean up proposes first and moves nothing
    let plan = call(&mut s, "plan_cleanup", json!({}));
    assert!(sfx.join(KEYS).is_file(), "planning moves nothing");
    let moves = plan["moves"].as_array().unwrap();
    assert!(!moves.is_empty(), "{plan}");
    assert!(moves
        .iter()
        .all(|m| m["to"].as_str().unwrap().starts_with("sfx/")));
    assert!(plan["stay"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n.as_str().unwrap().contains("Jacket")));

    // a placed sound follows its file (`relinked` below)
    let line = call(&mut s, "buckets", json!({}));
    let spoon = named(&line, "Kitchen")["kept"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["file"].as_str().unwrap().contains("Spoon Drop"))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut s,
        "place_from_bucket",
        json!({ "scene": "1", "asset_id": spoon, "at_s": 1.0 }),
    );

    // only what was confirmed moves; a bad move is reported and the rest go on
    let spoon_file = "ES_Kitchen, Spoon Drop - Epidemic Sound.wav";
    let r = call(
        &mut s,
        "apply_cleanup",
        json!({ "moves": [
            { "file": format!("sfx/{spoon_file}"), "to": "sfx/kitchen" },
            { "file": format!("sfx/{HISS}"), "to": "sfx/steam" },
            { "file": format!("sfx/{JACKET}"), "to": "../outside" },
        ] }),
    );
    assert_eq!(r["moved"], 2);
    assert_eq!(r["failed"].as_array().unwrap().len(), 1);
    assert_eq!(r["relinked"], 1);
    assert_eq!(r["new_buckets"], 1, "the new folder is a bucket: {r}");
    assert!(sfx.join("kitchen").join(spoon_file).is_file());
    assert!(!sfx.join(spoon_file).exists());
    assert!(sfx.join(JACKET).is_file(), "the refused move left it");
    assert!(!dir.path().join("outside").exists());
    let log = std::fs::read_to_string(r["log"].as_str().unwrap()).unwrap();
    assert!(log.contains("sfx/kitchen/ES_Kitchen, Spoon Drop"), "{log}");

    let b = call(&mut s, "buckets", json!({}));
    assert_eq!(b["loose"], 3);
    assert_eq!(sounds(named(&b, "Steam")), vec![HISS]);
    assert!(sounds(named(&b, "Kitchen")).contains(&spoon_file.to_string()));
}
