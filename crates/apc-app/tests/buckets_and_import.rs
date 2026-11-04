//! Round 2: sound buckets (spot, search with filters and paging, keep, dismiss, place with
//! download on drop) against a mock Partner API, and Import › Files matching (cast view,
//! room links kept on Redo match, what needs fixing).

use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn env() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::remove_var("EPIDEMIC_SOUND_API_KEY");
}

fn query(url: &str, key: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")).map(String::from))
}

/// 40 foil sounds; #105 is a music bed, #107 is very short.
fn mock_epidemic() -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let base2 = base.clone();
    std::thread::spawn(move || {
        let dir = tempfile::tempdir().unwrap();
        // a different file per sound id (the library deduplicates identical files)
        let wav_for = |id: u32| {
            let sr = 48000u32;
            let s: Vec<f32> = (0..sr / 2)
                .map(|i| {
                    0.2 * (i as f32 * (200.0 + id as f32) * std::f32::consts::TAU / sr as f32).sin()
                })
                .collect();
            let wav = dir.path().join(format!("{id}.wav"));
            apc_media::wav::write_wav(&wav, sr, 1, &s).unwrap();
            std::fs::read(&wav).unwrap()
        };
        for req in server.incoming_requests() {
            let url = req.url().to_string();
            let resp = if let Some(f) = url.strip_prefix("/files/") {
                tiny_http::Response::from_data(wav_for(
                    f.trim_end_matches(".wav").parse().unwrap_or(0),
                ))
            } else if url.starts_with("/v0/sound-effects/search") {
                let limit: usize = query(&url, "limit")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(10);
                let offset: usize = query(&url, "offset")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let items: Vec<Value> = (100..140)
                    .skip(offset)
                    .take(limit)
                    .map(|i| {
                        let title = if i == 105 { format!("Foil music bed {i}") } else { format!("Foil rustle {i}") };
                        json!({ "id": i.to_string(), "title": title, "length": if i == 107 { 0.1 } else { 2.0 } })
                    })
                    .collect();
                tiny_http::Response::from_data(
                    json!({ "soundEffects": items }).to_string().into_bytes(),
                )
            } else if url.contains("/download") || url.contains("/preview") {
                let id = url
                    .trim_start_matches("/v0/sound-effects/")
                    .split('/')
                    .next()
                    .unwrap_or("0")
                    .to_string();
                tiny_http::Response::from_data(
                    format!("{{\"url\":\"{base2}/files/{id}.wav\",\"expires\":\"x\"}}")
                        .into_bytes(),
                )
            } else {
                tiny_http::Response::from_data(b"{}".to_vec()).with_status_code(404)
            };
            let _ = req.respond(resp);
        }
    });
    base
}

fn wait_job(svc: &mut apc_app::Service, id: &str) -> Value {
    let t0 = Instant::now();
    loop {
        let jobs = svc.call("job_status", &json!({})).unwrap();
        let j = jobs
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == id)
            .unwrap()
            .clone();
        if j["state"] != "running" {
            return j;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "job timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn buckets_search_keep_dismiss_and_download_on_drop() {
    env();
    std::env::set_var("CUTAWAY_EPIDEMIC_URL", mock_epidemic());
    let dir = tempfile::tempdir().unwrap();
    let mut svc = apc_app::demo::create(dir.path(), "test", true).unwrap();
    svc.call(
        "connect_service",
        &json!({ "service": "epidemic", "api_key": "any-key-1234" }),
    )
    .unwrap();

    // Spot script groups the AI cues into buckets with keywords
    let r = svc.call("spot_buckets", &json!({})).unwrap();
    assert!(r["buckets"].as_u64().unwrap() >= 1);
    let all = svc.call("buckets", &json!({})).unwrap();
    let spotted = &all["buckets"].as_array().unwrap()[0];
    assert!(spotted["ai_suggested"].as_bool().unwrap());
    assert!(!spotted["keywords"].as_array().unwrap().is_empty());
    assert!(!spotted["scenes"].as_array().unwrap().is_empty());
    assert_eq!(all["connected"]["epidemic"], true);

    // a bucket by hand, with "leave out music" and a minimum length
    let b = svc
        .call(
            "create_bucket",
            &json!({ "name": "Foil", "keywords": ["foil rustle"], "scenes": ["1"], "exclude": ["music"], "length_min_s": 0.5, "search": true }),
        )
        .unwrap();
    let bid = b["bucket_id"].as_str().unwrap().to_string();
    let previews = b["search"]["previews"].as_array().unwrap().clone();
    assert_eq!(previews.len(), 5, "one page");
    assert!(
        previews
            .iter()
            .all(|p| !p["title"].as_str().unwrap().contains("music")),
        "left out"
    );
    assert!(
        previews.iter().all(|p| p["remote_id"] != "107"),
        "too short"
    );

    // Load more: no repeats
    let more = svc
        .call("bucket_search", &json!({ "bucket_id": bid, "more": true }))
        .unwrap();
    let ids: Vec<String> = more["previews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["remote_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 10);
    let mut uniq = ids.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), 10, "no duplicates across pages");

    // dismiss one; download another (kept, gold)
    svc.call(
        "bucket_dismiss",
        &json!({ "bucket_id": bid, "remote_id": ids[0] }),
    )
    .unwrap();
    let dl = svc
        .call(
            "bucket_download",
            &json!({ "bucket_id": bid, "remote_id": ids[1] }),
        )
        .unwrap();
    assert_eq!(dl["asset"]["source"], "licensed");
    assert_eq!(dl["asset"]["remote_id"], ids[1].as_str());
    let v = svc
        .call("buckets", &json!({ "scene": "1", "sort": "tag" }))
        .unwrap();
    let foil = v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == bid.as_str())
        .unwrap()
        .clone();
    assert_eq!(foil["kept"].as_array().unwrap().len(), 1);
    assert_eq!(
        foil["previews"].as_array().unwrap().len(),
        8,
        "dismissed and kept are no longer previews"
    );

    // audition a preview (virtual clock without audio)
    svc.call("play_sound", &json!({ "remote_id": ids[2] }))
        .unwrap();
    assert_eq!(
        svc.call("sound_playhead", &json!({})).unwrap()["sound"],
        ids[2].as_str()
    );
    svc.call("stop_sound", &json!({})).unwrap();

    // place the kept sound at once, and a preview after its download (background job)
    let kept = foil["kept"][0]["id"].as_str().unwrap().to_string();
    svc.call(
        "place_from_bucket",
        &json!({ "scene": "1", "asset_id": kept, "at_s": 1.0 }),
    )
    .unwrap();
    let job = svc
        .call(
            "place_from_bucket",
            &json!({ "scene": "1", "bucket_id": bid, "remote_id": ids[3], "at_s": 2.0 }),
        )
        .unwrap();
    let j = wait_job(&mut svc, job["job_id"].as_str().unwrap());
    assert_eq!(j["state"], "done", "{j}");
    let ev = svc.call("edit_view", &json!({ "scene": "1" })).unwrap();
    assert_eq!(ev["placements"].as_array().unwrap().len(), 2);
    // Split tool: one sound becomes two that play the same audio
    let first = ev["placements"][0].clone();
    let at = first["start_s"].as_f64().unwrap() + 0.2;
    svc.call(
        "command",
        &json!({ "cmd": "split_placement", "placement_id": first["id"], "at_s": at }),
    )
    .unwrap();
    let ev = svc.call("edit_view", &json!({ "scene": "1" })).unwrap();
    let pl = ev["placements"].as_array().unwrap();
    assert_eq!(pl.len(), 3);
    let tail = pl
        .iter()
        .find(|p| (p["start_s"].as_f64().unwrap() - at).abs() < 1e-6)
        .unwrap();
    assert!(
        (tail["len_s"].as_f64().unwrap() - (first["len_s"].as_f64().unwrap() - 0.2)).abs() < 1e-6
    );
    svc.call("undo", &json!({})).unwrap();
    // lane mix is part of the edit view
    svc.call(
        "command",
        &json!({ "cmd": "set_lane_mix", "mix": { "id": "sfx1", "volume_db": -4.0, "mute": true } }),
    )
    .unwrap();
    let ev = svc.call("edit_view", &json!({ "scene": "1" })).unwrap();
    assert_eq!(ev["lane_mix"][0]["mute"], true);
    let v = svc.call("buckets", &json!({ "scene": "1" })).unwrap();
    let foil = v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == bid.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        foil["kept"].as_array().unwrap().len(),
        2,
        "a dropped preview is kept"
    );

    // a dismissed sound is skipped by a fresh search; undo brings the preview back
    let fresh = svc
        .call("bucket_search", &json!({ "bucket_id": bid }))
        .unwrap();
    assert!(fresh["previews"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["remote_id"] != ids[0].as_str()));

    svc.call(
        "delete_bucket",
        &json!({ "bucket_id": bid, "confirm": true }),
    )
    .unwrap();
    let v = svc.call("buckets", &json!({})).unwrap();
    assert!(v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["id"] != bid.as_str()));
    // the key never appears in a bucket view
    assert!(!v.to_string().contains("any-key-1234"));
}

#[test]
fn import_matching_keeps_hand_links_and_lists_fixes() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = apc_app::demo::create(dir.path(), "test", true).unwrap();
    svc.call("match_files", &json!({})).unwrap();
    let v = svc.call("import_view", &json!({})).unwrap();
    assert!(v["matched_at"].is_string());
    assert_eq!(v["files"]["recordings"], 2);
    assert_eq!(v["files"]["rooms"], 1);
    let cast = v["cast"].as_array().unwrap();
    let paul = cast.iter().find(|c| c["id"] == "PAUL").unwrap();
    let rec = &paul["recordings"][0];
    assert_eq!(
        rec["rooms"].as_array().unwrap().len(),
        1,
        "the room serves Paul"
    );
    assert_eq!(rec["shared_with"][0], "Sara", "ensemble");
    assert!(v["fixes"].as_array().unwrap().is_empty(), "{}", v["fixes"]);

    // a file nobody can place becomes a fix
    let sources = svc.call("roles", &json!({})).unwrap();
    let sara_src = sources
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["character"] == "SARA")
        .unwrap()["source_id"]
        .as_str()
        .unwrap()
        .to_string();
    let room_src = sources
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["role"] == "room")
        .unwrap()["source_id"]
        .as_str()
        .unwrap()
        .to_string();
    let paul_src = rec["source_id"].as_str().unwrap().to_string();
    svc.call("confirm_roles", &json!({ "assignments": [{ "source_id": sara_src, "role": "unknown" }], "remember": false })).unwrap();
    let v = svc.call("import_view", &json!({})).unwrap();
    let fixes = v["fixes"].as_array().unwrap();
    assert!(fixes
        .iter()
        .any(|f| f["kind"] == "role" && f["source_id"] == sara_src.as_str()));
    assert!(fixes
        .iter()
        .any(|f| f["kind"] == "no_recording" && f["character"] == "SARA"));
    let s = svc
        .call("suggest_character", &json!({ "source_id": sara_src }))
        .unwrap();
    assert_eq!(s["character"], "SARA", "from the file name");
    svc.call("confirm_roles", &json!({ "assignments": [{ "source_id": sara_src, "role": "character", "character": "SARA" }] })).unwrap();

    // link the room to Paul only; Redo match keeps that
    svc.call(
        "link_room",
        &json!({ "room_id": room_src, "recording_ids": [paul_src] }),
    )
    .unwrap();
    let check = |svc: &mut apc_app::Service| {
        let v = svc.call("import_view", &json!({})).unwrap();
        let cast = v["cast"].as_array().unwrap().clone();
        let paul = cast.iter().find(|c| c["id"] == "PAUL").unwrap().clone();
        let sara = cast.iter().find(|c| c["id"] == "SARA").unwrap().clone();
        assert_eq!(paul["recordings"][0]["rooms"][0]["locked"], true);
        assert!(
            sara["recordings"][0]["rooms"]
                .as_array()
                .unwrap()
                .is_empty(),
            "Sara's session has no room now"
        );
    };
    check(&mut svc);
    svc.call("match_files", &json!({})).unwrap();
    check(&mut svc);

    // the analysis uses the matched sessions
    svc.call("run_alignment", &json!({})).unwrap();
    let sessions: Vec<Value> = svc
        .doc()
        .sessions
        .values()
        .map(|s| json!(s.source_ids))
        .collect();
    assert_eq!(sessions.len(), 2, "{sessions:?}");
    // undo the analysis and the link: back to one ensemble session
    svc.call("undo", &json!({})).unwrap();
    svc.call("undo", &json!({})).unwrap();
    svc.call("undo", &json!({})).unwrap();
    let v = svc.call("import_view", &json!({})).unwrap();
    let paul = v["cast"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "PAUL")
        .unwrap()
        .clone();
    assert_eq!(paul["recordings"][0]["rooms"][0]["locked"], false);
}

#[test]
fn closing_snapshots_unsaved_changes_and_returns_to_no_project() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let file = apc_app::demo::create(dir.path(), "test", true)
        .unwrap()
        .paths
        .file
        .clone();
    let mut s = apc_app::Session::with_project(&file, "ui").unwrap();
    s.call(
        "set_config",
        &json!({ "key": "pacing.breath_gap_s", "value": 0.4 }),
    )
    .unwrap();
    let r = s.call("close_project", &json!({})).unwrap();
    assert!(
        r["snapshot"].is_string(),
        "unsaved changes go to Backups/: {r}"
    );
    let err = s.call("project_status", &json!({})).unwrap_err();
    assert_eq!(apc_app::error::ErrorBody::from(&err).code, "no_project");
    // nothing new since: closing again after reopening takes no snapshot
    s.call("open_project", &json!({ "path": file })).unwrap();
    assert_eq!(
        s.call("project_status", &json!({})).unwrap()["breath_gap_s"],
        0.4
    );
    assert!(s.call("close_project", &json!({})).unwrap()["snapshot"].is_null());
}

/// Rob's naming: one folder, no timecode, "Sara-Session_v001" with "Sara-Director_v001".
#[test]
fn director_files_link_only_to_the_recording_they_are_named_after() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let demo = dir.path().join("demo");
    apc_app::demo::write_session(&demo).unwrap();
    let drop = dir.path().join("Session 2026-10-31");
    std::fs::create_dir_all(&drop).unwrap();
    // the recordings are the demo's (BWF timecode, transcripts); the director files are
    // tones without timecode, with the room transcript
    let mut paths = Vec::new();
    for (i, (name, from)) in [
        ("Sara-Session_v001", "Sara"),
        ("Sara-Director_v001", ""),
        ("Paul-Session_v001", "Paul"),
        ("Paul-Director_v001", ""),
    ]
    .iter()
    .enumerate()
    {
        let p = drop.join(format!("{name}.wav"));
        if from.is_empty() {
            let sr = 48000u32;
            let s: Vec<f32> = (0..sr)
                .map(|k| {
                    0.2 * (k as f32 * (220.0 + 60.0 * i as f32) * std::f32::consts::TAU / sr as f32)
                        .sin()
                })
                .collect();
            apc_media::wav::write_wav(&p, sr, 1, &s).unwrap();
            std::fs::copy(demo.join("Room.json"), drop.join(format!("{name}.json"))).unwrap();
        } else {
            std::fs::copy(demo.join(format!("{from}.wav")), &p).unwrap();
            std::fs::copy(
                demo.join(format!("{from}.json")),
                drop.join(format!("{name}.json")),
            )
            .unwrap();
        }
        paths.push(p.to_string_lossy().into_owned());
    }
    let mut s = apc_app::Session::new("test");
    s.call(
        "create_episode",
        &json!({ "parent": dir.path().to_string_lossy(), "name": "Ep" }),
    )
    .unwrap();
    s.call(
        "import_script",
        &json!({ "path": demo.join("Demo Episode.fountain").to_string_lossy() }),
    )
    .unwrap();
    s.call("import_audio", &json!({ "paths": paths, "copy": true }))
        .unwrap();

    let check = |s: &mut apc_app::Session| {
        let v = s.call("import_view", &json!({})).unwrap();
        for (ch, rec, room) in [
            ("SARA", "Sara-Session_v001.wav", "Sara-Director_v001.wav"),
            ("PAUL", "Paul-Session_v001.wav", "Paul-Director_v001.wav"),
        ] {
            let card = v["cast"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == ch)
                .unwrap()
                .clone();
            let r = &card["recordings"][0];
            assert_eq!(r["file"], rec);
            let rooms: Vec<&str> = r["rooms"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["file"].as_str().unwrap())
                .collect();
            assert_eq!(rooms, vec![room], "{ch}");
            assert!(r["shared_with"].as_array().unwrap().is_empty(), "Only {ch}");
        }
        assert!(v["fixes"].as_array().unwrap().is_empty(), "{}", v["fixes"]);
    };
    // the analysis on its own (no match yet) pairs them
    s.call("run_alignment", &json!({})).unwrap();
    check(&mut s);
    // and so does Redo match
    s.call("match_files", &json!({})).unwrap();
    check(&mut s);
}

/// The analysis fills each character's vocal bucket from the actor's own mic; nothing else
/// goes in, a removed vocal stays out, and a vocal is placed on the character's lane.
#[test]
fn the_analysis_fills_character_vocal_buckets_from_the_recordings() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let demo = dir.path().join("session");
    let wavs = apc_app::demo::write_session(&demo).unwrap();
    // Paul says an unscripted "Mh-hm." at 40 s
    let pj = demo.join("Paul.json");
    let mut t: Value = serde_json::from_str(&std::fs::read_to_string(&pj).unwrap()).unwrap();
    t["lines"].as_array_mut().unwrap().push(json!({
        "startTime": "00:00:40,000", "endTime": "00:00:40,600", "speakerDesignation": "Speaker 1", "text": "Mh-hm."
    }));
    std::fs::write(&pj, t.to_string()).unwrap();

    let mut s = apc_app::Session::new("test");
    s.call(
        "create_episode",
        &json!({ "parent": dir.path().to_string_lossy(), "name": "Ep" }),
    )
    .unwrap();
    s.call(
        "import_script",
        &json!({ "path": demo.join("Demo Episode.fountain").to_string_lossy() }),
    )
    .unwrap();
    let paths: Vec<String> = wavs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    s.call("import_audio", &json!({ "paths": paths, "copy": true }))
        .unwrap();
    s.call("run_alignment", &json!({})).unwrap();

    let paul = |s: &mut apc_app::Session| {
        s.call("buckets", &json!({})).unwrap()["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["character"] == "PAUL")
            .cloned()
            .expect("Paul's vocal bucket")
    };
    let b = paul(&mut s);
    assert_eq!(b["name"], "Paul · vocals");
    let bid = b["id"].as_str().unwrap().to_string();
    let mhm = b["kept"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["tags"][0] == "mhm")
        .cloned()
        .expect("the mh-hm");
    assert_eq!(mhm["heard"], "Mh-hm.");
    assert_eq!(mhm["source"], "recorded");
    assert!(
        mhm["file"].as_str().unwrap().starts_with("Vocals/Paul/"),
        "{}",
        mhm["file"]
    );
    assert!(mhm["duration_s"].as_f64().unwrap() > 0.5);
    // the scene filter shows it where Paul speaks
    let sc1 = s.call("buckets", &json!({ "scene": "1" })).unwrap();
    assert!(sc1["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["id"] == bid.as_str()));

    // only the actor's own recordings: no search, no generation, no outside sounds
    assert!(s
        .call(
            "bucket_search",
            &json!({ "bucket_id": bid, "provider": "library" })
        )
        .is_err());
    assert!(s
        .call("bucket_generate", &json!({ "bucket_id": bid }))
        .is_err());

    // placed on Paul's dialogue lane
    let aid = mhm["id"].as_str().unwrap().to_string();
    s.call(
        "place_from_bucket",
        &json!({ "scene": "1", "asset_id": aid, "at_s": 2.0 }),
    )
    .unwrap();
    let ev = s.call("edit_view", &json!({ "scene": "1" })).unwrap();
    let pl = &ev["placements"][0];
    assert_eq!(pl["lane"], "dialogue");
    assert_eq!(pl["character"], "PAUL");

    // removed, it stays out when the analysis runs again
    let n = b["kept"].as_array().unwrap().len();
    s.call(
        "bucket_remove",
        &json!({ "bucket_id": bid, "asset_id": aid }),
    )
    .unwrap();
    s.call("run_alignment", &json!({})).unwrap();
    let again = paul(&mut s);
    assert_eq!(again["kept"].as_array().unwrap().len(), n - 1);
    assert!(again["kept"]
        .as_array()
        .unwrap()
        .iter()
        .all(|k| k["id"] != aid.as_str()));

    // the analysis, vocals included, is one undo step
    let first = {
        let mut f = apc_app::Session::new("test");
        f.call(
            "create_episode",
            &json!({ "parent": dir.path().join("b").to_string_lossy(), "name": "Ep" }),
        )
        .unwrap();
        f.call(
            "import_script",
            &json!({ "path": demo.join("Demo Episode.fountain").to_string_lossy() }),
        )
        .unwrap();
        f.call("import_audio", &json!({ "paths": paths, "copy": true }))
            .unwrap();
        f.call("run_alignment", &json!({})).unwrap();
        assert!(f.call("buckets", &json!({})).unwrap()["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["character"] == "PAUL"));
        f.call("undo", &json!({})).unwrap();
        f.call("buckets", &json!({})).unwrap()
    };
    assert!(first["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|b| b["character"].is_null()));
}
