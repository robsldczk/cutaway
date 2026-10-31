//! Epidemic Sound: connecting stores the key (checked first), search and download work, and
//! the key never comes back through any method. Uses a local mock of the Partner API.

use serde_json::{json, Value};

fn mock_epidemic() -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let base2 = base.clone();
    std::thread::spawn(move || {
        let sr = 48000u32;
        let s: Vec<f32> = (0..sr)
            .map(|i| 0.2 * (i as f32 * 440.0 * std::f32::consts::TAU / sr as f32).sin())
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("door.wav");
        apc_media::wav::write_wav(&wav, sr, 1, &s).unwrap();
        let bytes = std::fs::read(&wav).unwrap();
        for mut req in server.incoming_requests() {
            let mut b = String::new();
            let _ = req.as_reader().read_to_string(&mut b);
            let auth_ok = req.headers().iter().any(|h| {
                h.field.equiv("Authorization") && h.value.as_str() == "Bearer good-key-123"
            });
            let user_ok = req
                .headers()
                .iter()
                .any(|h| h.field.equiv("x-partner-user-id") && !h.value.as_str().is_empty());
            let url = req.url().to_string();
            let resp = if url.starts_with("/files/") {
                tiny_http::Response::from_data(bytes.clone())
            } else if !auth_ok || !user_ok {
                tiny_http::Response::from_data(b"{\"error\":\"unauthorized\"}".to_vec())
                    .with_status_code(401)
            } else if url.starts_with("/v0/sound-effects/search") {
                tiny_http::Response::from_data(
                    br#"{"soundEffects":[{"id":"9001","title":"Door Slam Heavy","length":2,"added":"2024-01-01"}],"pagination":{},"links":{}}"#.to_vec(),
                )
            } else if url.starts_with("/v0/sound-effects/9001/download") {
                tiny_http::Response::from_data(
                    format!("{{\"url\":\"{base2}/files/door.wav\",\"expires\":\"x\"}}")
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

#[test]
fn connect_search_and_fetch_without_leaking_the_key() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::remove_var("EPIDEMIC_SOUND_API_KEY");
    std::env::set_var("CUTAWAY_EPIDEMIC_URL", mock_epidemic());
    let dir = tempfile::tempdir().unwrap();
    let mut svc = apc_app::demo::create(dir.path(), "test", true).unwrap();

    let c: Value = svc.call("connections", &json!({})).unwrap();
    assert_eq!(c[0]["service"], "epidemic");
    assert_eq!(c[0]["connected"], false);
    // a wrong key is refused and not stored
    let bad = svc.call(
        "connect_service",
        &json!({ "service": "epidemic", "api_key": "wrong-key-999" }),
    );
    assert!(bad.unwrap_err().to_string().contains("rejected"));
    assert_eq!(
        svc.call("connections", &json!({})).unwrap()[0]["connected"],
        false
    );
    let ok = svc
        .call(
            "connect_service",
            &json!({ "service": "epidemic", "api_key": "good-key-123" }),
        )
        .unwrap();
    assert_eq!(ok["connected"], true);
    assert_eq!(ok["source"], "keychain");

    svc.call("spot_sound_effects", &json!({ "scene": "1" }))
        .unwrap();
    let cue = svc.call("list_bucket", &json!({ "scene": "1" })).unwrap()[1]["cue"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = svc
        .call("search_sounds", &json!({ "query": "door slam" }))
        .unwrap();
    let remote = r["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["provider"] == "epidemic")
        .unwrap()
        .clone();
    assert_eq!(remote["remote_id"], "9001");
    let f = svc
        .call(
            "fetch_sound",
            &json!({ "remote_id": "9001", "title": remote["title"], "cue_id": cue }),
        )
        .unwrap();
    assert_eq!(f["asset"]["source"], "licensed");
    assert_eq!(f["asset"]["provider"], "epidemic");
    let bucket = svc.call("list_bucket", &json!({ "scene": "1" })).unwrap();
    let c = bucket
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cue"]["id"] == cue.as_str())
        .unwrap();
    assert_eq!(c["cue"]["status"], "candidates");

    // the key is nowhere in what clients can read
    for m in ["connections", "project_status", "history", "list_library"] {
        let v = svc.call(m, &json!({})).unwrap();
        assert!(!v.to_string().contains("good-key-123"), "{m} leaks the key");
    }
    let db = std::fs::read(&svc.paths.file).unwrap();
    assert!(
        !String::from_utf8_lossy(&db).contains("good-key-123"),
        "the key is not in the project file"
    );
    // connect is not an MCP tool
    assert!(apc_app::methods()
        .iter()
        .any(|m| m.name == "connect_service" && !m.mcp));

    svc.call("disconnect_service", &json!({ "service": "epidemic" }))
        .unwrap();
    assert_eq!(
        svc.call("connections", &json!({})).unwrap()[0]["connected"],
        false
    );
}
