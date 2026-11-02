//! Spot script runs only when asked and skips scenes it has read (unless re-spotting all);
//! Epidemic Sound: bucket searches add 5 at a time, and the free search's results dropped
//! into a bucket stay there across the bucket's own searches. Uses a mock Partner API.

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

/// 40 foil sounds, ids 100..140.
fn mock_epidemic() -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let url = req.url().to_string();
            let resp = if url.starts_with("/v0/sound-effects/search") {
                let limit: usize = query(&url, "limit")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(10);
                let offset: usize = query(&url, "offset")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let items: Vec<Value> = (100..140)
                    .skip(offset)
                    .take(limit)
                    .map(|i| json!({ "id": i.to_string(), "title": format!("Foil rustle {i}"), "length": 2.0 }))
                    .collect();
                tiny_http::Response::from_data(
                    json!({ "soundEffects": items }).to_string().into_bytes(),
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

fn ids(previews: &Value) -> Vec<String> {
    previews
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["remote_id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn spot_script_runs_when_asked_and_skips_unchanged_scenes() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = apc_app::demo::create(dir.path(), "test", true).unwrap();

    // the analysis doesn't spot: no AI buckets, both scenes waiting
    let v = svc.call("buckets", &json!({})).unwrap();
    assert!(v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|b| !b["ai_suggested"].as_bool().unwrap()));
    assert_eq!(v["spot"], json!({ "scenes": 2, "pending": 2 }));

    let r = svc.call("spot_buckets", &json!({})).unwrap();
    assert_eq!(r["scenes"], 2);
    assert!(r["buckets"].as_u64().unwrap() >= 1);
    assert_eq!(
        svc.call("buckets", &json!({})).unwrap()["spot"]["pending"],
        0
    );

    // again: nothing new, no AI calls
    let r = svc.call("spot_buckets", &json!({})).unwrap();
    assert_eq!(r["scenes"], 0);
    assert_eq!(r["skipped"], 2);

    // re-spot all
    let r = svc.call("spot_buckets", &json!({ "all": true })).unwrap();
    assert_eq!(r["scenes"], 2);

    // a changed scene is spotted again, the unchanged one isn't
    let changed = apc_app::demo::SCRIPT.replace(
        "I brought sandwiches for everyone.",
        "I brought a ringing phone for everyone.",
    );
    let f = dir.path().join("v2.fountain");
    std::fs::write(&f, changed).unwrap();
    svc.call("import_script", &json!({ "path": f })).unwrap();
    assert_eq!(
        svc.call("buckets", &json!({})).unwrap()["spot"]["pending"],
        1
    );

    // in the background, as the UI runs it
    let job = svc.call("start_spotting", &json!({})).unwrap();
    let j = wait_job(&mut svc, job["job_id"].as_str().unwrap());
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(j["result"]["scenes"], 1);
    svc.call("buckets", &json!({})).unwrap();
    assert_eq!(
        svc.call("buckets", &json!({})).unwrap()["spot"]["pending"],
        0
    );
}

#[test]
fn free_search_results_dropped_into_buckets_survive_searches() {
    env();
    std::env::set_var("CUTAWAY_EPIDEMIC_URL", mock_epidemic());
    let dir = tempfile::tempdir().unwrap();
    let mut svc = apc_app::demo::create(dir.path(), "test", false).unwrap();
    svc.call(
        "connect_service",
        &json!({ "service": "epidemic", "api_key": "any-key-1234" }),
    )
    .unwrap();

    // the free search: a page of 10, nothing stored
    let s = svc
        .call("search_epidemic", &json!({ "query": "foil" }))
        .unwrap();
    let found = s["results"].as_array().unwrap().clone();
    assert_eq!(found.len(), 10);
    assert_eq!(s["more"], true);
    let s2 = svc
        .call(
            "search_epidemic",
            &json!({ "query": "foil", "offset": s["next_offset"] }),
        )
        .unwrap();
    assert_eq!(s2["results"][0]["remote_id"], "110");

    // dropped on "new bucket": the bucket starts with it
    let first = json!({ "provider": "epidemic", "remote_id": found[0]["remote_id"], "title": found[0]["title"], "duration_s": 2.0 });
    let b = svc
        .call(
            "create_bucket",
            &json!({ "name": "foil", "keywords": ["foil"], "scenes": ["1"], "previews": [first] }),
        )
        .unwrap();
    let bid = b["bucket_id"].as_str().unwrap().to_string();

    // the bucket's own search adds 5, keeps the dropped one, repeats nothing
    let r = svc
        .call("bucket_search", &json!({ "bucket_id": bid }))
        .unwrap();
    let got = ids(&r["previews"]);
    assert_eq!(got.len(), 6, "{got:?}");
    assert_eq!(got[0], "100");
    assert_eq!(got[1..], ["101", "102", "103", "104", "105"]);
    let r = svc
        .call("bucket_search", &json!({ "bucket_id": bid, "more": true }))
        .unwrap();
    assert_eq!(ids(&r["previews"]).len(), 11);
    assert_eq!(ids(&r["previews"])[10], "110", "load more reads on");

    // dropped onto the bucket (twice: once only); a dismissed sound comes back when dropped
    let late = json!({ "provider": "epidemic", "remote_id": "130", "title": "Foil rustle 130", "duration_s": 2.0 });
    for _ in 0..2 {
        let a = svc
            .call(
                "bucket_add_preview",
                &json!({ "bucket_id": bid, "preview": late }),
            )
            .unwrap();
        assert_eq!(a["added"], true);
    }
    svc.call(
        "bucket_dismiss",
        &json!({ "bucket_id": bid, "remote_id": "130" }),
    )
    .unwrap();
    svc.call(
        "bucket_add_preview",
        &json!({ "bucket_id": bid, "preview": late }),
    )
    .unwrap();

    // a fresh search replaces the results but not what was dropped in
    let r = svc
        .call("bucket_search", &json!({ "bucket_id": bid }))
        .unwrap();
    let got = ids(&r["previews"]);
    assert_eq!(got.len(), 7, "{got:?}");
    assert!(got.contains(&"100".to_string()) && got.contains(&"130".to_string()));

    // the free search says where a result already is
    let s = svc
        .call("search_epidemic", &json!({ "query": "foil" }))
        .unwrap();
    assert_eq!(s["results"][0]["buckets"], json!([bid]));
    assert_eq!(s["results"][9]["buckets"], json!([]));

    // a character's vocal bucket takes no outside sounds
    let v = svc.call("buckets", &json!({})).unwrap();
    if let Some(vocal) = v["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| !b["character"].is_null())
    {
        assert!(svc
            .call(
                "bucket_add_preview",
                &json!({ "bucket_id": vocal["id"], "preview": late }),
            )
            .is_err());
    }
}
