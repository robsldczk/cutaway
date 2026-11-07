//! The Export page's promises, end to end on the demo session: real audio through the real
//! service, then the files read back from disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use apc_app::{demo, methods, Access, Service, Session};
use apc_core::config::{ExportMode, ExportSettings};
use apc_export::timeline::SCENE_GAP_S;
use apc_export::versions::Manifest;
use apc_export::{EpisodeTimeline, ExportScope};

fn with_mock(local: &Path) {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::set_var("CUTAWAY_DATA_DIR", local);
}

fn demo_episode(dir: &Path) -> Service {
    with_mock(&dir.join("local"));
    demo::create(dir, "test", true).unwrap()
}

/// A sound on the room's bus and a voice in it, so the room tracks have something to say.
fn with_sound(svc: &mut Service, dir: &Path) {
    let wav = dir.join("door.wav");
    let s: Vec<f32> = (0..48000)
        .map(|i| 0.3 * (i as f32 * 900.0 * std::f32::consts::TAU / 48000.0).sin())
        .collect();
    apc_media::wav::write_wav(&wav, 48000, 1, &s).unwrap();
    let r = svc
        .call(
            "import_sounds",
            &json!({ "paths": [wav.to_string_lossy()] }),
        )
        .unwrap();
    let asset = r["imported"][0]["id"]
        .as_str()
        .or_else(|| r["ids"][0].as_str())
        .or_else(|| r["assets"][0]["id"].as_str())
        .unwrap_or_else(|| panic!("no asset id in {r}"))
        .to_string();
    // something on every lane that has a room or a stem of its own
    for extra in [
        json!({ "lane": "sfx", "at_s": 1.0, "depth": 0.5 }),
        json!({ "lane": "ambience", "at_s": 0.0, "depth": 0.4 }),
        json!({ "lane": "music", "at_s": 0.5 }),
        json!({ "lane": "dialogue", "character": "PAUL", "at_s": 1.5 }),
    ] {
        let mut p = json!({ "scene": "1", "asset_id": asset });
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        svc.call("place_sound", &p).unwrap();
    }
}

fn settings(f: impl FnOnce(&mut ExportSettings)) -> Value {
    let mut s = ExportSettings {
        lock_after: false,
        ..Default::default()
    };
    f(&mut s);
    serde_json::to_value(s).unwrap()
}

fn export(svc: &mut Service, scenes: Value, s: Value) -> Value {
    svc.call("export_scenes", &json!({ "scenes": scenes, "settings": s }))
        .unwrap()
}

fn folder(r: &Value, i: usize) -> PathBuf {
    PathBuf::from(r["scenes"][i]["folder"].as_str().unwrap())
}

fn manifest(dir: &Path) -> Manifest {
    serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Frames and samples of a WAV, whatever its depth, as floats.
fn read(p: &Path) -> (u32, Vec<f32>) {
    let mut r = hound::WavReader::open(p).unwrap();
    let spec = r.spec();
    let v: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let full = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.unwrap() as f32 / full)
                .collect()
        }
    };
    (spec.sample_rate, v)
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn stems_of(dir: &Path) -> BTreeMap<String, Vec<f32>> {
    names(dir)
        .into_iter()
        .filter(|n| n.ends_with(".wav"))
        .map(|n| {
            let key = n
                .rsplit('_')
                .next()
                .unwrap()
                .trim_end_matches(".wav")
                .to_string();
            (key, read(&dir.join(&n)).1)
        })
        .collect()
}

#[test]
fn in_stem_mode_every_file_is_the_scene_to_the_sample_silent_ones_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    with_sound(&mut svc, dir.path());
    let r = export(&mut svc, json!(["1"]), settings(|_| {}));
    let f = folder(&r, 0);
    let m = manifest(&f);
    assert_eq!(m.version, 1);
    let stems = stems_of(&f);
    assert_eq!(
        stems.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "Ambience",
            "Dialogue",
            "Effects",
            "Mix",
            "Music",
            "RoomEffects",
            "RoomVoices"
        ],
    );
    for (name, s) in &stems {
        assert_eq!(s.len() as u64, m.frames * 2, "{name} is the scene's length");
    }
    assert!(peak(&stems["Music"]) > 0.01, "the music is on its stem");
    assert!(peak(&stems["Ambience"]) > 0.01, "the ambience on its");
    assert!(peak(&stems["Dialogue"]) > 0.01, "the dialogue is there");
    assert!(peak(&stems["Effects"]) > 0.01, "and the effect");
    assert!(peak(&stems["RoomEffects"]) > 0.0, "and its room");
    for file in m.files.iter().filter(|f| f.frames.is_some()) {
        assert_eq!(file.frames, Some(m.frames), "{}", file.name);
        assert_eq!(file.checksum.len(), "blake3:".len() + 64);
    }
    let meant = "E01_S01_v001";
    assert!(names(&f)
        .iter()
        .all(|n| n.starts_with(meant) || n == "manifest.json"));
    assert!(f.ends_with(format!("v001_{}", chrono::Local::now().format("%Y-%m-%d"))));
}

#[test]
fn a_track_with_nothing_on_it_is_a_silent_file_of_the_full_length() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let r = export(&mut svc, json!(["1"]), settings(|_| {}));
    let f = folder(&r, 0);
    let frames = manifest(&f).frames;
    let stems = stems_of(&f);
    for name in ["Effects", "Ambience", "Music", "RoomEffects"] {
        assert_eq!(peak(&stems[name]), 0.0, "{name} is silent");
        assert_eq!(
            stems[name].len() as u64,
            frames * 2,
            "{name} is full length"
        );
    }
}

#[test]
fn the_stems_add_up_to_the_mix_before_loudness() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    with_sound(&mut svc, dir.path());
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| {
            s.loudness = None;
            s.bit_depth = 32;
        }),
    );
    let stems = stems_of(&folder(&r, 0));
    let mix = &stems["Mix"];
    let mut sum = vec![0.0f32; mix.len()];
    for (name, s) in &stems {
        if name != "Mix" {
            for (a, b) in sum.iter_mut().zip(s) {
                *a += b;
            }
        }
    }
    let residual = sum
        .iter()
        .zip(mix)
        .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(peak(mix) > 0.01);
    let db = 20.0 * (residual / peak(mix)).log10();
    assert!(db < -100.0, "residual {db:.1} dB");
}

#[test]
fn loudness_moves_the_mix_and_leaves_the_stems_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let plain = export(&mut svc, json!(["1"]), settings(|s| s.loudness = None));
    let loud = export(
        &mut svc,
        json!(["1"]),
        settings(|s| s.loudness = Some(-23.0)),
    );
    let (a, b) = (stems_of(&folder(&plain, 0)), stems_of(&folder(&loud, 0)));
    assert_eq!(a["Dialogue"], b["Dialogue"], "stems stay at mix level");
    assert_ne!(a["Mix"], b["Mix"]);
    let m = manifest(&folder(&loud, 0));
    assert!(m.mix_gain_db.is_some(), "the manifest says what it got");
    let lufs = apc_media::loudness::integrated_lufs(&b["Mix"], 48000, 2)
        .unwrap()
        .unwrap();
    assert!(
        (lufs - m.mix_lufs.unwrap()).abs() < 0.3,
        "{lufs} {:?}",
        m.mix_lufs
    );
}

#[test]
fn fcpxml_only_writes_the_timeline_without_rendering() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    // no audio can be read: a render would fail
    for s in svc.doc().sources.values().cloned().collect::<Vec<_>>() {
        std::fs::remove_file(svc.paths.resolve(&s.path)).unwrap();
    }
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| {
            s.mode = apc_core::config::ExportMode::Fcpxml;
            s.fcpxml = false;
            s.subtitles = true;
        }),
    );
    assert_eq!(
        names(&folder(&r, 0)),
        ["E01_S01_v001.fcpxml", "E01_S01_v001.srt", "manifest.json"],
    );
}

#[test]
fn a_second_export_is_the_next_version_and_touches_nothing_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let first = export(&mut svc, json!(["1"]), settings(|_| {}));
    let f1 = folder(&first, 0);
    let before = std::fs::read(f1.join("manifest.json")).unwrap();

    let status = svc.call("export_status", &json!({})).unwrap();
    let s1 = &status["scenes"][0];
    assert_eq!(s1["last"]["version"], 1);
    assert_eq!(s1["next_version"], 2);
    assert_eq!(s1["unchanged_since"], 1, "Unchanged since v001");
    let plan = svc
        .call("export_plan", &json!({ "scenes": ["1"] }))
        .unwrap();
    assert_eq!(plan["scenes"][0]["version"], 2);
    assert_eq!(plan["scenes"][0]["unchanged_since"], 1);
    assert_eq!(plan["scenes"][0]["label"], "Sc01 v002");

    let second = export(&mut svc, json!(["1"]), settings(|_| {}));
    assert_eq!(
        second["scenes"][0]["version"], 2,
        "still allowed, the next version"
    );
    assert_eq!(second["unchanged"][0]["since"], 1, "and it says so");
    assert_ne!(folder(&second, 0), f1);
    assert_eq!(std::fs::read(f1.join("manifest.json")).unwrap(), before);
    // the other scene counts on its own
    let other = export(&mut svc, json!(["2"]), settings(|_| {}));
    assert_eq!(other["scenes"][0]["version"], 1);

    // a change is noticed by the hash, and a gain doesn't need the lock to be lifted
    let line = svc
        .doc()
        .lines_of_scene(&svc.doc().scenes_in_order()[0].id)
        .iter()
        .find(|l| l.kind == apc_core::LineKind::Dialogue)
        .unwrap()
        .id
        .clone();
    svc.call(
        "command",
        &json!({ "cmd": "set_clip_gain", "line_id": line, "gain_db": -6.0 }),
    )
    .unwrap();
    let status = svc.call("export_status", &json!({})).unwrap();
    assert_eq!(status["scenes"][0]["changed_since"], 2);
    assert_eq!(status["scenes"][0]["unchanged_since"], Value::Null);
}

#[test]
fn the_whole_episode_has_its_own_counter_and_says_which_versions_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    export(&mut svc, json!(["1"]), settings(|_| {}));
    let r = svc
        .call("export_scenes", &json!({ "settings": settings(|_| {}) }))
        .unwrap();
    assert_eq!(r["episode"]["version"], 1);
    let f = PathBuf::from(r["episode"]["folder"].as_str().unwrap());
    assert!(names(&f).contains(&"E01_v001.fcpxml".to_string()));
    let held: Vec<(u32, u32)> = manifest(&f)
        .scenes
        .iter()
        .map(|s| (s.number, s.version))
        .collect();
    assert_eq!(
        held,
        [(1, 2), (2, 1)],
        "Sc. 1 was at v002 and Sc. 2 at v001"
    );
}

fn one_file(f: impl FnOnce(&mut ExportSettings)) -> Value {
    settings(|s| {
        s.mode = ExportMode::OneFile;
        s.fcpxml = false;
        s.subtitles = false;
        f(s);
    })
}

fn episode_folder(r: &Value) -> PathBuf {
    PathBuf::from(r["episode"]["folder"].as_str().unwrap())
}

/// Each scene's own mix, at mix level.
fn scene_mixes(svc: &mut Service) -> Vec<Vec<f32>> {
    let r = export(
        svc,
        json!(["1", "2"]),
        settings(|s| {
            s.mode = ExportMode::Mix;
            s.loudness = None;
            s.fcpxml = false;
            s.subtitles = false;
        }),
    );
    (0..2)
        .map(|i| {
            let dir = folder(&r, i);
            let wav = names(&dir).into_iter().find(|n| n.ends_with("_Mix.wav"));
            read(&dir.join(wav.unwrap())).1
        })
        .collect()
}

#[test]
fn one_file_is_each_scenes_mix_in_order_with_the_pause_between_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    with_sound(&mut svc, dir.path());
    let mixes = scene_mixes(&mut svc);

    let r = svc
        .call(
            "export_scenes",
            &json!({ "settings": one_file(|s| s.loudness = None) }),
        )
        .unwrap();
    assert_eq!(r["scenes"], json!([]), "no scene versions of its own");
    let f = episode_folder(&r);
    let (rate, joined) = read(&f.join("E01_v001_Mix.wav"));
    assert_eq!(rate, 48000);

    let gap = (SCENE_GAP_S * 48000.0) as usize * 2;
    assert_eq!(joined.len(), mixes[0].len() + gap + mixes[1].len());
    let close = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
    assert!(close(&joined[..mixes[0].len()], &mixes[0]));
    assert!(joined[mixes[0].len()..mixes[0].len() + gap]
        .iter()
        .all(|v| *v == 0.0));
    assert!(close(&joined[mixes[0].len() + gap..], &mixes[1]));
    assert_eq!(manifest(&f).frames as usize * 2, joined.len());
}

#[test]
fn a_sound_past_the_last_line_keeps_the_next_scene_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    with_sound(&mut svc, dir.path());
    let doc = svc.doc();
    let scene = doc.find_scene("1").unwrap();
    let line_end =
        EpisodeTimeline::build(doc, &ExportScope::Scenes(vec![scene.id.clone()])).duration_s;
    let asset = doc.assets.keys().next().unwrap().clone();
    svc.call(
        "place_sound",
        &json!({ "scene": "1", "asset_id": asset, "lane": "sfx", "at_s": line_end + 5.0 }),
    )
    .unwrap();
    let mixes = scene_mixes(&mut svc);

    let r = svc
        .call(
            "export_scenes",
            &json!({ "settings": one_file(|s| s.loudness = None) }),
        )
        .unwrap();
    let f = episode_folder(&r);
    let (_, joined) = read(&f.join("E01_v001_Mix.wav"));
    let gap = (SCENE_GAP_S * 48000.0) as usize * 2;
    let second = &joined[mixes[0].len() + gap..];
    assert_eq!(second.len(), mixes[1].len());
    assert!(second
        .iter()
        .zip(&mixes[1])
        .all(|(a, b)| (a - b).abs() < 1e-5));
}

#[test]
fn the_subtitles_of_one_file_land_on_its_second_scene() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let mixes = scene_mixes(&mut svc);
    let r = svc
        .call(
            "export_scenes",
            &json!({ "settings": settings(|s| {
                s.mode = ExportMode::OneFile;
                s.loudness = None;
            }) }),
        )
        .unwrap();
    let f = episode_folder(&r);
    let expected = [
        "E01_v001.fcpxml",
        "E01_v001.srt",
        "E01_v001_Mix.wav",
        "manifest.json",
    ];
    assert_eq!(names(&f), expected);

    let starts: Vec<f64> = EpisodeTimeline::joined(svc.doc(), &ExportScope::All)
        .scenes
        .iter()
        .map(|m| m.start_s)
        .collect();
    let first = mixes[0].len() as f64 / 2.0 / 48000.0;
    assert!(
        (starts[1] - (first + SCENE_GAP_S)).abs() < 0.001,
        "{starts:?}"
    );
    let srt = std::fs::read_to_string(f.join("E01_v001.srt")).unwrap();
    let cue = EpisodeTimeline::joined(svc.doc(), &ExportScope::All)
        .cues
        .into_iter()
        .find(|c| c.start_s >= starts[1])
        .unwrap();
    let at = cue.start_s as u64;
    let stamp = format!("00:{:02}:{:02},", at / 60, at % 60);
    assert!(srt.contains(&stamp), "{stamp} isn't in\n{srt}");
}

#[test]
fn loudness_reaches_the_whole_file_and_keeps_the_scenes_balance() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let r = svc
        .call(
            "export_scenes",
            &json!({ "settings": one_file(|s| s.loudness = Some(-23.0)) }),
        )
        .unwrap();
    let f = episode_folder(&r);
    let m = manifest(&f);
    assert!(
        (m.mix_lufs.unwrap() - -23.0).abs() < 0.2,
        "{:?}",
        m.mix_lufs
    );
    let (_, joined) = read(&f.join("E01_v001_Mix.wav"));
    let measured = apc_media::loudness::integrated_lufs(&joined, 48000, 2)
        .unwrap()
        .unwrap();
    assert!((measured - -23.0).abs() < 0.3, "{measured}");
}

#[test]
fn one_file_counts_with_the_episode_and_says_when_nothing_changed() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let plan = svc
        .call("export_plan", &json!({ "settings": one_file(|_| {}) }))
        .unwrap();
    assert_eq!(plan["one_file"]["label"], "E01 v001");
    assert_eq!(plan["episode_version"], 1);
    assert_eq!(plan["scenes"], json!([]));
    assert_eq!(plan["files"], 2, "the mix and the manifest");
    assert_eq!(plan["one_file"]["unchanged_since"], Value::Null);
    let folder_name = plan["one_file"]["folder"].as_str().unwrap();
    assert!(
        folder_name.starts_with("Episode/versions/v001_"),
        "{folder_name}"
    );
    assert!(!folder_name.contains(' '), "{folder_name}");

    let first = svc
        .call("export_scenes", &json!({ "settings": one_file(|_| {}) }))
        .unwrap();
    assert_eq!(first["episode"]["version"], 1);
    let f = episode_folder(&first);
    let held: Vec<(u32, u32)> = manifest(&f)
        .scenes
        .iter()
        .map(|s| (s.number, s.version))
        .collect();
    assert_eq!(held, [(1, 0), (2, 0)]);

    let status = svc.call("export_status", &json!({})).unwrap();
    assert_eq!(status["episode_versions"][0]["version"], 1);
    assert_eq!(status["scenes"][0]["next_version"], 1, "scenes don't count");

    let plan = svc
        .call("export_plan", &json!({ "settings": one_file(|_| {}) }))
        .unwrap();
    assert_eq!(plan["one_file"]["unchanged_since"], 1);
    let second = svc
        .call("export_scenes", &json!({ "settings": one_file(|_| {}) }))
        .unwrap();
    assert_eq!(second["episode"]["version"], 2);
    assert_eq!(second["warnings"], json!(["Unchanged since v001."]));
}

#[test]
fn one_file_takes_the_scenes_picked_and_only_those() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let mixes = scene_mixes(&mut svc);
    let r = export(&mut svc, json!(["2"]), one_file(|s| s.loudness = None));
    let f = episode_folder(&r);
    let (_, joined) = read(&f.join("E01_v001_Mix.wav"));
    assert_eq!(joined.len(), mixes[1].len());
    let held: Vec<u32> = manifest(&f).scenes.iter().map(|s| s.number).collect();
    assert_eq!(held, [2]);
}

#[test]
fn one_file_locks_its_scenes_and_a_lock_keeps_the_version_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    export(&mut svc, json!(["1"]), settings(|s| s.lock_after = true));
    let r = svc
        .call(
            "export_scenes",
            &json!({ "settings": one_file(|s| s.lock_after = true) }),
        )
        .unwrap();
    assert!(manifest(&episode_folder(&r)).locked);
    let locks: Vec<(Option<u32>, bool)> = svc
        .doc()
        .scenes_in_order()
        .iter()
        .map(|s| (s.lock.as_ref().and_then(|l| l.version), s.lock.is_some()))
        .collect();
    assert_eq!(locks, [(Some(1), true), (None, true)]);
}

#[test]
fn a_failed_one_file_leaves_no_version_and_no_lock() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    for s in svc.doc().sources.values().cloned().collect::<Vec<_>>() {
        std::fs::remove_file(svc.paths.resolve(&s.path)).unwrap();
    }
    svc.call(
        "export_scenes",
        &json!({ "settings": one_file(|s| s.lock_after = true) }),
    )
    .unwrap_err();
    assert!(svc.doc().scenes.values().all(|s| s.lock.is_none()));
    let exports = svc.paths.exports.clone();
    let left: Vec<_> = walk(&exports).into_iter().filter(|p| p.is_file()).collect();
    assert!(left.is_empty(), "nothing half written: {left:?}");
}

#[test]
fn scenes_with_nothing_in_them_cant_go_out_as_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let lines: Vec<String> = svc.doc().lines.keys().cloned().collect();
    for id in lines {
        svc.call("delete_line", &json!({ "line_id": id, "confirm": true }))
            .unwrap();
    }
    let plan = svc
        .call("export_plan", &json!({ "settings": one_file(|_| {}) }))
        .unwrap();
    assert_eq!(plan["one_file"], Value::Null);
    assert_eq!(plan["empty"], json!([1, 2]));
    let e = svc
        .call("export_scenes", &json!({ "settings": one_file(|_| {}) }))
        .unwrap_err();
    assert!(e.to_string().contains("nothing in them"), "{e}");
}

#[test]
fn scenes_can_be_picked_by_how_far_along_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let mark = |svc: &mut Service, scene: &str, status: &str| {
        svc.call(
            "set_scene_status",
            &json!({ "scene": scene, "status": status }),
        )
        .unwrap();
    };
    mark(&mut svc, "2", "finished");
    mark(&mut svc, "1", "review");
    let status = svc.call("export_status", &json!({})).unwrap();
    assert_eq!(status["scenes"][1]["status"], "finished");

    let s = settings(|_| {});
    let plan = svc
        .call(
            "export_plan",
            &json!({ "status": ["finished"], "settings": s }),
        )
        .unwrap();
    let numbers: Vec<u64> = plan["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["number"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, [2]);
    assert_eq!(
        plan["episode_version"],
        Value::Null,
        "that's not the episode"
    );

    let plan = svc
        .call(
            "export_plan",
            &json!({ "status": ["finished", "review"], "settings": s }),
        )
        .unwrap();
    assert_eq!(plan["scenes"].as_array().unwrap().len(), 2);
    assert_eq!(plan["episode_version"], Value::Null);

    let none = svc.call(
        "export_plan",
        &json!({ "status": ["in_progress"], "settings": s }),
    );
    assert!(none.unwrap_err().to_string().contains("marked"));
}

#[test]
fn an_exported_scene_is_locked_and_refuses_timing_edits_with_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let s = settings(|s| s.lock_after = true);
    export(&mut svc, json!(["1"]), s);
    let sc = svc.doc().scenes_in_order()[0].clone();
    assert_eq!(sc.lock.as_ref().unwrap().version, Some(1));
    assert!(
        manifest(
            &svc.call("export_status", &json!({})).unwrap()["scenes"][0]["last"]["folder"]
                .as_str()
                .map(PathBuf::from)
                .unwrap()
        )
        .locked
    );
    let line = sc.order[1].clone();
    let e = svc
        .call("set_gap", &json!({ "line_id": line, "gap_s": 3.0 }))
        .unwrap_err();
    assert_eq!(e.code(), "scene_locked");
    assert!(
        e.to_string()
            .starts_with("Sc. 1 is locked. Exported as v001 on "),
        "{e}"
    );

    let status = svc.call("project_status", &json!({})).unwrap();
    assert_eq!(
        status["warnings"][0],
        "1 scene is locked because it was exported."
    );
    // unlocking asks first, like every destructive tool
    let e = svc
        .call("unlock_scene", &json!({ "scene": "1" }))
        .unwrap_err();
    assert_eq!(e.code(), "needs_confirm");
    svc.call("unlock_scene", &json!({ "scene": "1", "confirm": true }))
        .unwrap();
    svc.call("set_gap", &json!({ "line_id": line, "gap_s": 3.0 }))
        .unwrap();
    // a whole re-analysis would only fail at its end: it's refused at the start
    svc.call("lock_scene", &json!({ "scene": "1" })).unwrap();
    assert_eq!(
        svc.doc().scenes_in_order()[0]
            .lock
            .as_ref()
            .unwrap()
            .version,
        Some(1)
    );
    let e = svc.call("run_alignment", &json!({})).unwrap_err();
    assert_eq!(e.code(), "scene_locked");
}

#[test]
fn a_locked_scene_can_still_be_exported_and_the_lock_moves_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    export(&mut svc, json!(["1"]), settings(|s| s.lock_after = true));
    export(&mut svc, json!(["1"]), settings(|s| s.lock_after = true));
    assert_eq!(
        svc.doc().scenes_in_order()[0]
            .lock
            .as_ref()
            .unwrap()
            .version,
        Some(2)
    );
    // and turning the box off doesn't quietly unlock it
    let r = export(&mut svc, json!(["1"]), settings(|_| {}));
    assert!(svc.doc().scenes_in_order()[0].lock.is_some());
    assert!(manifest(&folder(&r, 0)).locked);
}

#[test]
fn locking_after_an_export_is_one_undo_step_for_all_its_scenes() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    export(
        &mut svc,
        json!(["1", "2"]),
        settings(|s| s.lock_after = true),
    );
    assert!(svc.doc().scenes.values().all(|s| s.lock.is_some()));
    svc.call("undo", &json!({})).unwrap();
    assert!(svc.doc().scenes.values().all(|s| s.lock.is_none()));
}

#[test]
fn a_failed_export_leaves_no_version_and_no_lock() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    for s in svc.doc().sources.values().cloned().collect::<Vec<_>>() {
        std::fs::remove_file(svc.paths.resolve(&s.path)).unwrap();
    }
    let e = svc
        .call(
            "export_scenes",
            &json!({ "scenes": ["1"], "settings": settings(|s| s.lock_after = true) }),
        )
        .unwrap_err();
    assert!(e.to_string().starts_with("Sc. 1: "), "{e}");
    assert!(svc.doc().scenes.values().all(|s| s.lock.is_none()));
    let scene_dir = svc.paths.exports.clone();
    let left: Vec<_> = walk(&scene_dir)
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    assert!(left.is_empty(), "nothing half written: {left:?}");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            out.push(e.path());
            out.extend(walk(&e.path()));
        }
    }
    out
}

#[test]
fn split_dialogue_writes_one_full_length_file_per_character() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| s.tracks.split_dialogue = true),
    );
    let f = folder(&r, 0);
    let frames = manifest(&f).frames;
    let wavs: Vec<String> = names(&f)
        .into_iter()
        .filter(|n| n.contains("Dialogue"))
        .collect();
    assert_eq!(
        wavs,
        [
            "E01_S01_v001_Dialogue_PAUL.wav",
            "E01_S01_v001_Dialogue_SARA.wav"
        ]
    );
    for n in &wavs {
        let (_, s) = read(&f.join(n));
        assert_eq!(s.len() as u64, frames * 2);
        assert!(peak(&s) > 0.01, "{n}");
    }
}

/// PAUL stands hard left in scene 1; the split with and without pan.
fn split_paul(mono: bool) -> (u64, u16, Vec<f32>) {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    svc.call(
        "set_character_pan",
        &json!({ "scene": "1", "character": "PAUL", "pan": -1.0 }),
    )
    .unwrap();
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| {
            s.tracks.split_dialogue = true;
            s.tracks.split_mono = mono;
        }),
    );
    let f = folder(&r, 0);
    let frames = manifest(&f).frames;
    let p = f.join("E01_S01_v001_Dialogue_PAUL.wav");
    let channels = hound::WavReader::open(&p).unwrap().spec().channels;
    let (_, samples) = read(&p);
    (frames, channels, samples)
}

#[test]
fn a_split_without_pan_is_one_centred_channel_per_character() {
    let (frames, channels, panned) = split_paul(false);
    assert_eq!(channels, 2);
    let right: Vec<f32> = panned.iter().skip(1).step_by(2).copied().collect();
    let left: Vec<f32> = panned.iter().step_by(2).copied().collect();
    assert!(peak(&left) > 0.01);
    assert!(peak(&right) < 1e-4, "hard left: {}", peak(&right));

    let (mono_frames, channels, mono) = split_paul(true);
    assert_eq!(channels, 1);
    assert_eq!(mono_frames, frames);
    assert_eq!(mono.len() as u64, frames, "one sample a frame");
    assert!(peak(&mono) > 0.01);
    // centred, the voice is as loud as on the side it was panned to (constant power, unity
    // in the centre), give or take what panning took from the direct sound
    let ratio = peak(&mono) / peak(&left);
    assert!((0.5..=1.05).contains(&ratio), "{ratio}");
}

#[test]
fn individual_clips_have_handles_and_the_fcpxml_points_at_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| {
            s.mode = apc_core::config::ExportMode::Clips;
            s.tracks.mix = false;
            s.tracks.room_voices = false;
            s.tracks.room_effects = false;
        }),
    );
    let f = folder(&r, 0);
    let clips: Vec<String> = names(&f)
        .into_iter()
        .filter(|n| n.ends_with(".wav"))
        .collect();
    assert!(clips.len() >= 3, "{clips:?}");
    assert!(
        clips.iter().all(|n| n.starts_with("E01_S01_v001_L0")),
        "{clips:?}"
    );
    assert!(clips.iter().any(|n| n.contains("_PAUL_T")), "{clips:?}");
    let xml = std::fs::read_to_string(f.join("E01_S01_v001.fcpxml")).unwrap();
    let plain = xml.replace("<!DOCTYPE fcpxml>", "");
    let doc = roxmltree::Document::parse(&plain).unwrap();
    let reps: Vec<&str> = doc
        .descendants()
        .filter(|n| n.has_tag_name("media-rep"))
        .filter_map(|n| n.attribute("src"))
        .collect();
    assert_eq!(reps.len(), clips.len());
    assert!(
        reps.iter().all(|s| s.contains("E01_S01_v001_L0")),
        "the clips, not the recordings"
    );
    // the handle sits before what plays: a clip starts inside its file
    let starts: Vec<&str> = doc
        .descendants()
        .filter(|n| n.has_tag_name("asset-clip"))
        .filter_map(|n| n.attribute("start"))
        .collect();
    assert!(starts.iter().any(|s| *s != "0s"), "{starts:?}");
}

#[test]
fn the_fcpxml_has_a_lane_per_character_and_the_two_room_lanes() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    with_sound(&mut svc, dir.path());
    let r = export(&mut svc, json!(["1"]), settings(|_| {}));
    let f = folder(&r, 0);
    let xml = std::fs::read_to_string(f.join("E01_S01_v001.fcpxml")).unwrap();
    assert!(xml.contains("audioRole=\"dialogue.Room\""));
    assert!(xml.contains("audioRole=\"effects.Room\""));
    assert!(xml.contains("E01_S01_v001_RoomVoices.wav"));
    assert!(xml.contains(".wav\"/>"));
    assert!(
        xml.contains("dialogue.Paul") || xml.contains("dialogue.PAUL"),
        "dry character lanes"
    );

    // from the rendered stems: nothing of the recordings is left, the markers are
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| s.fcpxml_source = apc_core::config::FcpxmlSource::Stems),
    );
    let f = folder(&r, 0);
    let xml = std::fs::read_to_string(f.join("E01_S01_v002.fcpxml")).unwrap();
    let plain = xml.replace("<!DOCTYPE fcpxml>", "");
    let doc = roxmltree::Document::parse(&plain).unwrap();
    let reps: Vec<&str> = doc
        .descendants()
        .filter(|n| n.has_tag_name("media-rep"))
        .filter_map(|n| n.attribute("src"))
        .collect();
    assert_eq!(reps.len(), 6, "every stem but the mix: {reps:?}");
    assert!(reps.iter().all(|s| s.contains("E01_S01_v002_")));
    assert!(!xml.contains("_Mix.wav"), "the mix would double everything");
    assert!(doc.descendants().any(|n| n.has_tag_name("marker")));
}

#[test]
fn what_cant_be_delivered_is_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let refused = |svc: &mut Service, s: Value, word: &str| {
        let e = svc
            .call("export_scenes", &json!({ "scenes": ["1"], "settings": s }))
            .unwrap_err();
        assert!(e.to_string().contains(word), "{e}");
    };
    use apc_core::config::AudioCodec;
    refused(&mut svc, settings(|s| s.codec = AudioCodec::Mp3), "MP3");
    refused(
        &mut svc,
        settings(|s| {
            s.codec = AudioCodec::Flac;
            s.bit_depth = 32;
        }),
        "FLAC",
    );
    refused(&mut svc, settings(|s| s.sample_rate = 12345), "sample rate");
    refused(
        &mut svc,
        settings(|s| s.save_to = apc_core::config::SaveTo::Pipeline),
        "show",
    );
    let e = svc
        .call("export_scenes", &json!({ "scenes": ["99"] }))
        .unwrap_err();
    assert!(e.to_string().contains("no scene '99'"), "{e}");
    assert!(walk(&svc.paths.exports).is_empty());
    // mix only may be MP3, when there's ffmpeg to make one
    if apc_export::bounce::ffmpeg_available() {
        let r = export(
            &mut svc,
            json!(["1"]),
            settings(|s| {
                s.mode = ExportMode::Mix;
                s.codec = AudioCodec::Mp3;
            }),
        );
        assert!(names(&folder(&r, 0)).contains(&"E01_S01_v001_Mix.mp3".to_string()));
    } else {
        eprintln!("skipped the MP3 export: no ffmpeg");
    }
}

#[test]
fn flac_stems_stay_exact() {
    if !apc_export::bounce::ffmpeg_available() {
        eprintln!("skipped: no ffmpeg");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| s.codec = apc_core::config::AudioCodec::Flac),
    );
    let f = folder(&r, 0);
    let m = manifest(&f);
    let flacs: Vec<String> = names(&f)
        .into_iter()
        .filter(|n| n.ends_with(".flac"))
        .collect();
    assert_eq!(flacs.len(), 7);
    assert!(
        !names(&f).iter().any(|n| n.ends_with(".wav")),
        "no leftovers"
    );
    for n in &flacs {
        let mut reader = apc_media::AudioReader::open(&f.join(n)).unwrap();
        let info = reader.info();
        let all = reader.read_range(0.0, info.duration_s + 1.0).unwrap();
        assert_eq!(all.len() as u64 / u64::from(info.channels), m.frames, "{n}");
    }
}

#[test]
fn another_sample_rate_keeps_every_file_the_same_length() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let r = export(
        &mut svc,
        json!(["1"]),
        settings(|s| {
            s.sample_rate = 44100;
            s.bit_depth = 16;
        }),
    );
    let f = folder(&r, 0);
    let m = manifest(&f);
    for n in names(&f).iter().filter(|n| n.ends_with(".wav")) {
        let (rate, s) = read(&f.join(n));
        assert_eq!(rate, 44100);
        assert_eq!(s.len() as u64, m.frames * 2);
    }
}

#[test]
fn presets_are_kept_by_name_and_can_run_an_export() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    let s = settings(|s| {
        s.mode = apc_core::config::ExportMode::Fcpxml;
        s.subtitles = false;
    });
    svc.call(
        "save_export_preset",
        &json!({ "name": "Timeline", "settings": s }),
    )
    .unwrap();
    assert_eq!(
        svc.call("export_status", &json!({})).unwrap()["presets"][0]["name"],
        "Timeline"
    );
    let r = svc
        .call(
            "export_scenes",
            &json!({ "scenes": ["1"], "preset": "Timeline" }),
        )
        .unwrap();
    assert_eq!(
        names(&folder(&r, 0)),
        ["E01_S01_v001.fcpxml", "manifest.json"]
    );
    svc.call("delete_export_preset", &json!({ "name": "Timeline" }))
        .unwrap();
    let e = svc
        .call("export_scenes", &json!({ "preset": "Timeline" }))
        .unwrap_err();
    assert!(e.to_string().contains("no preset"), "{e}");
}

#[test]
fn a_soloed_lane_is_pointed_out_before_it_empties_the_stack() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    svc.call(
        "command",
        &json!({ "cmd": "set_lane_mix", "mix": { "id": "PAUL", "solo": true } }),
    )
    .unwrap();
    let w = &svc.call("export_status", &json!({})).unwrap()["warnings"];
    assert_eq!(w[0], "Only the soloed lanes will be exported: PAUL.");
}

/// A pipeline with a sound folder, the demo episode moved into it.
fn show(dir: &Path) -> (Session, PathBuf) {
    with_mock(&dir.join("local"));
    let root = dir.join("tm-2610");
    std::fs::create_dir_all(root.join("Episodes/Episode 2/Sound")).unwrap();
    std::fs::write(
        root.join("Episodes/Episode 2/episode.jetplane.json"),
        r#"{"schemaVersion":1,"kind":"episode","name":"Turkey Time","episodeNumber":2}"#,
    )
    .unwrap();
    std::fs::write(root.join("library.jetplane.json"), r#"{"kind":"library"}"#).unwrap();
    let mut sess = Session::new("test");
    sess.call("create_show", &json!({ "root": root.to_string_lossy() }))
        .unwrap();
    let src = demo::create(dir, "source", true).unwrap();
    let file = src.paths.file.to_string_lossy().into_owned();
    drop(src);
    sess.call(
        "migrate_episode",
        &json!({ "source": file, "episode": "Episode 2" }),
    )
    .unwrap();
    (sess, root)
}

#[test]
fn a_show_export_goes_only_to_the_confirmed_pipeline_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (mut sess, root) = show(dir.path());

    let view = sess.call("pipeline_folder", &json!({})).unwrap();
    assert_eq!(view["path"], Value::Null, "nothing stored yet");
    let proposal = view["proposal"].as_str().unwrap().to_string();
    assert_eq!(
        Path::new(&proposal),
        root.join("Sound"),
        "the pipeline's own sound folder"
    );

    let run = json!({ "episodes": [{ "episode": "Episode 2", "scenes": ["1"] }], "settings": settings(|_| {}) });
    let e = sess.call("export_show", &run).unwrap_err();
    assert!(e.to_string().contains("no pipeline folder"), "{e}");
    assert!(!root.join("Sound/Exports").exists(), "never a guessed path");

    // an agent can't confirm its own proposal: it's the editor's word
    let confirmed = root.join("Sound");
    let e = sess
        .call(
            "set_pipeline_folder",
            &json!({ "path": confirmed.to_string_lossy() }),
        )
        .unwrap_err();
    assert_eq!(e.code(), "needs_confirm");
    sess.call(
        "set_pipeline_folder",
        &json!({ "path": confirmed.to_string_lossy(), "confirm": true }),
    )
    .unwrap();
    let stored = std::fs::read_to_string(root.join("tm-2610.cutawayshow")).unwrap();
    assert!(
        stored.contains("\"pipelineFolder\": \"Sound\""),
        "relative, for the other machines"
    );

    let r = sess.call("export_show", &run).unwrap();
    let f = PathBuf::from(
        r["episodes"][0]["result"]["scenes"][0]["folder"]
            .as_str()
            .unwrap(),
    );
    assert!(
        f.starts_with(root.join("Sound/Exports/Ep02/Sc01_Lab")),
        "{f:?}"
    );
    assert!(
        names(&f).contains(&"E02_S01_v001_Mix.wav".to_string()),
        "the pipeline's episode number"
    );

    // and it counts in both places: the episode's own folder is where the next one goes
    let status = sess.call("show_export_status", &json!({})).unwrap();
    let ep = status["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["folder"] == "Episode 2")
        .unwrap();
    assert_eq!(ep["scenes"][0]["next_version"], 2);
    assert_eq!(ep["scenes"][0]["last"]["destination"], "pipeline");
    sess.call("open_episode", &json!({ "episode": "Episode 2" }))
        .unwrap();
    let r = sess
        .call(
            "export_scenes",
            &json!({ "scenes": ["1"], "settings": settings(|_| {}) }),
        )
        .unwrap();
    assert_eq!(r["scenes"][0]["version"], 2);
    assert!(folder(&r, 0)
        .to_string_lossy()
        .contains("Episodes/Episode 2/Sound/Exports/Sc01_Lab"));
}

#[test]
fn a_show_can_take_an_episode_as_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let (mut sess, root) = show(dir.path());
    sess.call(
        "set_pipeline_folder",
        &json!({ "path": root.join("Sound").to_string_lossy(), "confirm": true }),
    )
    .unwrap();
    let r = sess
        .call(
            "export_show",
            &json!({ "episodes": [{ "episode": "Episode 2" }], "settings": one_file(|_| {}) }),
        )
        .unwrap();
    let f = episode_folder(&r["episodes"][0]["result"]);
    assert!(
        f.starts_with(root.join("Sound/Exports/Ep02/Episode")),
        "{f:?}"
    );
    assert!(names(&f).contains(&"E02_v001_Mix.wav".to_string()));
}

#[test]
fn the_default_goes_straight_into_the_episodes_exports_with_no_folder_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let (mut sess, root) = show(dir.path());
    sess.call("open_episode", &json!({ "episode": "Episode 2" }))
        .unwrap();
    assert_eq!(
        sess.call("pipeline_folder", &json!({})).unwrap()["path"],
        Value::Null
    );

    // no settings at all: nothing asked, nothing chosen
    let r = sess
        .call("export_scenes", &json!({ "scenes": ["1"] }))
        .unwrap();
    let exports = root.join("Episodes/Episode 2/Sound/Exports");
    let f = folder(&r, 0);
    assert_eq!(
        f.parent().unwrap(),
        exports.join("Sc01_Lab/versions"),
        "no Ep02 inside Episode 2"
    );
    // the files still say which episode they're from, once they leave the folder
    assert!(names(&f).contains(&"E02_S01_v001_Mix.wav".to_string()));
    let plan = sess
        .call("export_plan", &json!({ "scenes": ["1"] }))
        .unwrap();
    assert!(plan["scenes"][0]["folder"]
        .as_str()
        .unwrap()
        .starts_with("Sc01_Lab/versions/v002_"));
}

#[test]
fn a_custom_folder_gets_its_exports_folder_and_keeps_the_episodes_apart() {
    let dir = tempfile::tempdir().unwrap();
    let (mut sess, root) = show(dir.path());
    let custom = root.join("Deliveries");
    std::fs::create_dir_all(&custom).unwrap();
    sess.call(
        "set_pipeline_folder",
        &json!({ "path": custom.to_string_lossy(), "confirm": true }),
    )
    .unwrap();
    let r = sess
        .call(
            "export_show",
            &json!({ "episodes": [{ "episode": "Episode 2", "scenes": ["1"] }] }),
        )
        .unwrap();
    let f = PathBuf::from(
        r["episodes"][0]["result"]["scenes"][0]["folder"]
            .as_str()
            .unwrap(),
    );
    assert!(f.starts_with(custom.join("Exports/Ep02/Sc01_Lab")), "{f:?}");
}

#[test]
fn a_locked_episode_still_opens_in_its_show() {
    let dir = tempfile::tempdir().unwrap();
    let (mut sess, root) = show(dir.path());
    sess.call("open_episode", &json!({ "episode": "Episode 2" }))
        .unwrap();
    sess.call("lock_scene", &json!({ "scene": "1" })).unwrap();
    sess.call("lock_scene", &json!({ "scene": "2" })).unwrap();
    sess.call("close_project", &json!({})).unwrap();
    // what opening does on its own (linking folders, relinking sounds) must never be refused
    sess.call("open_episode", &json!({ "episode": "Episode 2" }))
        .unwrap();
    let status = sess.call("project_status", &json!({})).unwrap();
    assert!(status["warnings"]
        .to_string()
        .contains("2 scenes are locked."));
    assert!(root.exists());
}

#[test]
fn the_new_methods_reach_agents_and_unlocking_is_marked_destructive() {
    let all = methods();
    let find = |n: &str| {
        all.iter()
            .find(|m| m.name == n)
            .unwrap_or_else(|| panic!("{n}"))
    };
    for n in [
        "export_status",
        "export_plan",
        "export_scenes",
        "lock_scene",
        "unlock_scene",
        "save_export_preset",
        "delete_export_preset",
        "pipeline_folder",
        "set_pipeline_folder",
        "show_export_status",
        "export_show",
    ] {
        assert!(find(n).mcp, "{n} is on MCP");
    }
    assert_eq!(find("unlock_scene").access, Access::Destructive);
    assert_eq!(find("set_pipeline_folder").access, Access::Destructive);
}

/// The files an FCPXML points at, as paths.
fn media_paths(xml: &str) -> Vec<PathBuf> {
    let plain = xml.replace("<!DOCTYPE fcpxml>", "");
    let doc = roxmltree::Document::parse(&plain).unwrap();
    doc.descendants()
        .filter(|n| n.has_tag_name("media-rep"))
        .filter_map(|n| n.attribute("src"))
        .map(|src| {
            let raw = src.trim_start_matches("file://").as_bytes();
            let mut out = Vec::new();
            let mut i = 0;
            while i < raw.len() {
                if raw[i] == b'%' && i + 2 < raw.len() {
                    let hex = std::str::from_utf8(&raw[i + 1..i + 3]).unwrap();
                    out.push(u8::from_str_radix(hex, 16).unwrap());
                    i += 3;
                } else {
                    out.push(raw[i]);
                    i += 1;
                }
            }
            PathBuf::from(String::from_utf8(out).unwrap())
        })
        .collect()
}

/// Every file a manifest lists is there, with its size and checksum.
fn manifest_holds(dir: &Path) {
    for f in manifest(dir).files {
        let p = dir.join(&f.name);
        assert!(p.is_file(), "{} is missing", p.display());
        assert_eq!(std::fs::metadata(&p).unwrap().len(), f.bytes, "{}", f.name);
        assert_eq!(
            apc_export::versions::checksum(&p).unwrap(),
            f.checksum,
            "{}",
            f.name
        );
    }
}

#[test]
fn current_holds_the_newest_version_under_names_that_dont_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    with_sound(&mut svc, dir.path());
    let stems = |s: &mut ExportSettings| s.fcpxml_source = apc_core::config::FcpxmlSource::Stems;
    let r = export(&mut svc, json!(["1"]), settings(stems));
    let v1 = folder(&r, 0);
    assert_eq!(v1.parent().unwrap().file_name().unwrap(), "versions");
    let scene = v1.parent().unwrap().parent().unwrap().to_path_buf();
    let current = scene.join("current");
    assert_eq!(
        names(&scene),
        ["current", "versions"],
        "nothing else beside them"
    );

    // the same files, without the version in their names
    let want: Vec<String> = names(&v1).iter().map(|n| n.replace("_v001", "")).collect();
    assert_eq!(names(&current), want);
    assert!(names(&current).contains(&"E01_S01_Mix.wav".to_string()));
    assert_eq!(
        std::fs::read(current.join("E01_S01_Mix.wav")).unwrap(),
        std::fs::read(v1.join("E01_S01_v001_Mix.wav")).unwrap()
    );
    assert_eq!(manifest(&current).version, 1);
    manifest_holds(&current);
    manifest_holds(&v1);

    // its FCPXML points at its own files; the version's at the version's
    let xml = std::fs::read_to_string(current.join("E01_S01.fcpxml")).unwrap();
    let paths = media_paths(&xml);
    assert!(!paths.is_empty());
    for p in &paths {
        assert!(
            p.starts_with(current.canonicalize().unwrap()) || p.starts_with(&current),
            "{}",
            p.display()
        );
        assert!(p.is_file(), "{}", p.display());
        assert!(!p.to_string_lossy().contains("_v001"), "{}", p.display());
    }
    let old = std::fs::read_to_string(v1.join("E01_S01_v001.fcpxml")).unwrap();
    assert!(media_paths(&old)
        .iter()
        .all(|p| p.starts_with(&v1) && p.is_file()));

    // the next version takes its place there, and the history counts versions only
    let r = export(&mut svc, json!(["1"]), settings(stems));
    let v2 = folder(&r, 0);
    assert_eq!(manifest(&current).version, 2);
    assert_eq!(names(&current), want, "the same names");
    assert_eq!(
        std::fs::read(current.join("E01_S01_Mix.wav")).unwrap(),
        std::fs::read(v2.join("E01_S01_v002_Mix.wav")).unwrap()
    );
    manifest_holds(&current);
    let plan = svc
        .call(
            "export_plan",
            &json!({ "scenes": ["1"], "settings": settings(stems) }),
        )
        .unwrap();
    assert_eq!(plan["scenes"][0]["version"], 3);
    assert!(
        !names(&scene).iter().any(|n| n.starts_with('.')),
        "nothing half written left: {:?}",
        names(&scene)
    );
}

#[test]
fn the_whole_episode_has_its_current_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    svc.call("export_scenes", &json!({ "settings": one_file(|_| {}) }))
        .unwrap();
    let ep = svc.paths.exports.join("Episode");
    assert_eq!(names(&ep), ["current", "versions"]);
    assert!(names(&ep.join("current")).contains(&"E01_Mix.wav".to_string()));
    manifest_holds(&ep.join("current"));
}

#[test]
fn versions_from_before_move_into_versions_with_their_paths() {
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo_episode(dir.path());
    with_sound(&mut svc, dir.path());
    let stems = |s: &mut ExportSettings| s.fcpxml_source = apc_core::config::FcpxmlSource::Stems;
    let mut old = Vec::new();
    for _ in 0..2 {
        let r = export(&mut svc, json!(["1"]), settings(stems));
        old.push(folder(&r, 0));
    }
    let scene = old[0].parent().unwrap().parent().unwrap().to_path_buf();

    // put them back as they were before: straight in the scene's folder, no current/
    std::fs::remove_dir_all(scene.join("current")).unwrap();
    for v in &old {
        let name = v.file_name().unwrap().to_string_lossy().into_owned();
        let to = scene.join(&name);
        std::fs::rename(v, &to).unwrap();
        let mut m = manifest(&to);
        for f in m.files.iter_mut().filter(|f| f.name.ends_with(".fcpxml")) {
            let p = to.join(&f.name);
            let xml = std::fs::read_to_string(&p).unwrap();
            std::fs::write(
                &p,
                xml.replace(&format!("/versions/{name}/"), &format!("/{name}/")),
            )
            .unwrap();
            f.bytes = std::fs::metadata(&p).unwrap().len();
            f.checksum = apc_export::versions::checksum(&p).unwrap();
        }
        std::fs::write(
            to.join("manifest.json"),
            serde_json::to_vec_pretty(&m).unwrap(),
        )
        .unwrap();
        let xml = names(&to)
            .into_iter()
            .find(|n| n.ends_with(".fcpxml"))
            .unwrap();
        let paths = media_paths(&std::fs::read_to_string(to.join(xml)).unwrap());
        assert!(
            paths.iter().all(|p| p.starts_with(&to) && p.is_file()),
            "{paths:?}"
        );
    }
    std::fs::remove_dir(scene.join("versions")).unwrap();
    // the old layout still counts
    let plan = svc
        .call(
            "export_plan",
            &json!({ "scenes": ["1"], "settings": settings(stems) }),
        )
        .unwrap();
    assert_eq!(plan["scenes"][0]["version"], 3);

    let r = svc.call("reorganize_exports", &json!({})).unwrap();
    let done = &r["folders"][0]["done"];
    assert_eq!(done["moved"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(done["rewritten"], 2, "{r}");
    assert_eq!(done["current"][0][1], 2, "{r}");
    assert_eq!(names(&scene), ["current", "versions"]);
    for v in &old {
        assert!(v.is_dir(), "{} is back in versions/", v.display());
        manifest_holds(v);
        let xml = names(v)
            .into_iter()
            .find(|n| n.ends_with(".fcpxml"))
            .unwrap();
        let paths = media_paths(&std::fs::read_to_string(v.join(xml)).unwrap());
        assert!(
            paths.iter().all(|p| p.starts_with(v) && p.is_file()),
            "{paths:?}"
        );
    }
    manifest_holds(&scene.join("current"));
    assert_eq!(manifest(&scene.join("current")).version, 2);

    // again: nothing to do
    let again = svc.call("reorganize_exports", &json!({})).unwrap();
    assert_eq!(again["folders"][0]["done"]["moved"], json!([]));
    assert_eq!(again["folders"][0]["done"]["current"], json!([]));
}
