//! What a drop from a bucket makes of a sound, loops, and a scene's room tone.

use std::path::Path;

use apc_audio::{MixSettings, PlaySchedule};
use serde_json::{json, Value};

fn wav(path: &Path, seconds: f64) {
    apc_media::wav::write_wav(path, 48000, 1, &vec![0.1; (48000.0 * seconds) as usize]).unwrap();
}

struct Fix {
    _dir: tempfile::TempDir,
    svc: apc_app::Service,
    bucket: String,
    root: std::path::PathBuf,
}

fn fix() -> Fix {
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut svc = apc_app::demo::create(dir.path(), "test", true).unwrap();
    let bucket = svc
        .call(
            "create_bucket",
            &json!({ "name": "Drops", "keywords": ["x"] }),
        )
        .unwrap()["bucket_id"]
        .as_str()
        .unwrap()
        .to_string();
    Fix {
        _dir: dir,
        svc,
        bucket,
        root,
    }
}

impl Fix {
    fn sound(&mut self, name: &str, seconds: f64) -> String {
        let file = self.root.join(name);
        wav(&file, seconds);
        let r = self
            .svc
            .call(
                "import_sounds",
                &json!({ "paths": [file.to_string_lossy()], "bucket_id": self.bucket }),
            )
            .unwrap();
        r["assets"][0]["id"].as_str().unwrap().to_string()
    }

    fn drop(&mut self, asset: &str, extra: Value) -> Value {
        let mut p =
            json!({ "scene": "1", "asset_id": asset, "bucket_id": self.bucket, "at_s": 0.5 });
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        let id = self.svc.call("place_from_bucket", &p).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let all = self
            .svc
            .call("list_sounds", &json!({ "scene": "1" }))
            .unwrap();
        all.as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id.as_str())
            .unwrap()
            .clone()
    }

    fn schedule(&self) -> PlaySchedule {
        let doc = self.svc.doc();
        let scene = doc.find_scene("1").unwrap();
        let cfg = self.svc.config();
        PlaySchedule::from_scene(doc, scene, &cfg.pacing(), &MixSettings::default())
    }
}

fn len(s: &Value) -> f64 {
    s["len_s"].as_f64().unwrap()
}

#[test]
fn a_short_sound_keeps_its_length_and_a_long_one_is_cropped() {
    let mut f = fix();
    for (name, src, want) in [
        ("short.wav", 3.0, 3.0),
        ("mark.wav", 6.0, 6.0),
        ("between.wav", 8.0, 8.0),
        ("long.wav", 40.0, 10.0),
    ] {
        let a = f.sound(name, src);
        // a long import counts as ambience, so it loops until told otherwise
        let bucket = f.bucket.clone();
        f.svc
            .call(
                "bucket_set_loop",
                &json!({ "bucket_id": bucket, "asset_id": a, "on": false }),
            )
            .unwrap();
        let s = f.drop(&a, json!({}));
        assert!((len(&s) - want).abs() < 1e-3, "{name}: {s}");
        assert_eq!(s["looped"], false, "{name}");
    }
}

#[test]
fn a_long_sound_with_loop_on_is_a_bed_to_the_end_of_the_scene() {
    let mut f = fix();
    let a = f.sound("bed.wav", 7.0);
    let bucket = f.bucket.clone();
    f.svc
        .call(
            "bucket_set_loop",
            &json!({ "bucket_id": bucket, "asset_id": a, "on": true }),
        )
        .unwrap();
    // even dropped on an effects lane
    let s = f.drop(&a, json!({ "lane": "sfx" }));
    assert_eq!(s["lane"], "ambience");
    assert_eq!(s["looped"], true);

    let sched = f.schedule();
    let clip = sched
        .clips
        .iter()
        .find(|c| c.owner == s["id"].as_str().unwrap())
        .unwrap();
    assert!((clip.src_out - 7.0).abs() < 1e-3, "the whole file repeats");
    assert_eq!(clip.loop_len_s, Some(len(&s)));
    assert!(len(&s) > 7.0 || sched.duration_s < 8.0, "outlasts its file");

    // the rules leave it alone, and a trim still works
    let id = s["id"].clone();
    f.svc
        .call(
            "trim_sound",
            &json!({ "placement_id": id, "edge": "end", "delta_s": 2.0 }),
        )
        .unwrap();
    let again = f.svc.call("list_sounds", &json!({ "scene": "1" })).unwrap();
    assert!((len(&again[0]) - len(&s) - 2.0).abs() < 1e-3, "{again}");
}

#[test]
fn a_short_sound_with_loop_on_stays_a_short_sound() {
    let mut f = fix();
    let a = f.sound("tick.wav", 4.0);
    let bucket = f.bucket.clone();
    f.svc
        .call(
            "bucket_set_loop",
            &json!({ "bucket_id": bucket, "asset_id": a, "on": true }),
        )
        .unwrap();
    let s = f.drop(&a, json!({}));
    assert_eq!(s["lane"], "sfx");
    assert_eq!(s["looped"], false);
    assert!((len(&s) - 4.0).abs() < 1e-3);
}

#[test]
fn music_keeps_its_length() {
    let mut f = fix();
    let a = f.sound("score.wav", 40.0);
    let s = f.drop(&a, json!({ "lane": "music" }));
    assert_eq!(s["lane"], "music");
    assert!((len(&s) - 40.0).abs() < 1e-3);
}

#[test]
fn only_a_drop_crops() {
    let mut f = fix();
    let a = f.sound("cue.wav", 40.0);
    let r = f
        .svc
        .call(
            "place_sound",
            &json!({ "scene": "1", "asset_id": a, "at_s": 0.0 }),
        )
        .unwrap();
    let all = f.svc.call("list_sounds", &json!({ "scene": "1" })).unwrap();
    let s = all
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == r["id"])
        .unwrap();
    assert!((len(s) - 40.0).abs() < 1e-3);
}

#[test]
fn the_bucket_says_what_loops_and_ambience_loops_by_default() {
    let mut f = fix();
    let a = f.sound("dull.wav", 8.0);
    let bucket = f.bucket.clone();
    let looped = |svc: &mut apc_app::Service, id: &str| {
        let v = svc.call("buckets", &json!({})).unwrap();
        v["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|b| b["kept"].as_array().unwrap())
            .find(|k| k["id"] == id)
            .unwrap()["loop"]
            .clone()
    };
    assert_eq!(looped(&mut f.svc, &a), false);
    f.svc
        .call(
            "bucket_set_loop",
            &json!({ "bucket_id": bucket, "asset_id": a, "on": true }),
        )
        .unwrap();
    assert_eq!(looped(&mut f.svc, &a), true);
}

#[test]
fn a_scene_loops_its_own_room_tone() {
    let mut f = fix();
    let a = f.sound("tone.wav", 5.0);
    let tone_of = |s: &PlaySchedule| s.clips.iter().find(|c| c.lane == "roomtone").cloned();
    assert!(tone_of(&f.schedule()).is_none());

    let rev = f.svc.call("project_status", &json!({})).unwrap()["rev"].clone();
    f.svc
        .call(
            "set_scene_tone",
            &json!({ "scene": "1", "asset_id": a, "gain_db": -18.0 }),
        )
        .unwrap();
    assert_ne!(
        f.svc.call("project_status", &json!({})).unwrap()["rev"],
        rev
    );
    let sched = f.schedule();
    let clip = tone_of(&sched).expect("the tone plays");
    assert_eq!(clip.source_id, format!("asset:{a}"));
    assert_eq!(clip.gain_db, -18.0);
    assert_eq!(
        clip.loop_len_s,
        Some(sched.duration_s),
        "under the whole scene"
    );

    // another scene has its own
    let rooms = f.svc.call("scene_rooms", &json!({})).unwrap();
    assert_eq!(rooms[0]["tone"]["gain_db"], -18.0);
    assert!(rooms[1]["tone"].is_null());

    // the level alone keeps the sound, and undo brings the last one back
    f.svc
        .call(
            "set_scene_tone",
            &json!({ "scene": "1", "asset_id": a, "gain_db": -12.0 }),
        )
        .unwrap();
    f.svc.call("undo", &json!({})).unwrap();
    assert_eq!(tone_of(&f.schedule()).unwrap().gain_db, -18.0);

    // no sound: back to the location's, here none
    f.svc
        .call("set_scene_tone", &json!({ "scene": "1" }))
        .unwrap();
    assert!(tone_of(&f.schedule()).is_none());
}

#[test]
fn an_unknown_sound_is_no_room_tone() {
    let mut f = fix();
    let r = f.svc.call(
        "set_scene_tone",
        &json!({ "scene": "1", "asset_id": "nothing" }),
    );
    assert!(r.is_err());
}
