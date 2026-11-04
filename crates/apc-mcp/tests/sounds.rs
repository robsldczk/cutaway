//! Sound clips and rooms from the CLI and over MCP: the same steps give the same document.

use std::path::{Path, PathBuf};

use clap::Parser;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{json, Value};

use apc_app::{demo, Session};
use apc_mcp::CutawayMcp;

fn env() {
    std::env::set_var("CUTAWAY_SECRETS", "memory");
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
}

fn cli(args: &[&str]) -> Value {
    let mut v = vec!["cutaway"];
    v.extend_from_slice(args);
    apc_cli::run(apc_cli::Cli::parse_from(v)).unwrap()
}

/// A demo episode with a 2 s sound file next to it.
fn episode(dir: &Path) -> (PathBuf, PathBuf) {
    let file = {
        let svc = demo::create(dir, "ep", true).unwrap();
        svc.paths.file.clone()
    };
    let wav = dir.join("Foil_crackle.wav");
    let s: Vec<f32> = (0..96000).map(|i| 0.2 * (i as f32 * 0.05).sin()).collect();
    apc_media::wav::write_wav(&wav, 48000, 1, &s).unwrap();
    (file, wav)
}

type Client = RunningService<RoleClient, ()>;

async fn tool(client: &Client, name: &str, args: Value) -> Value {
    let Value::Object(map) = args else { panic!() };
    let r = client
        .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(map))
        .await
        .unwrap();
    assert_ne!(r.is_error, Some(true), "{name} failed: {:?}", r.content);
    r.structured_content.unwrap()["result"].clone()
}

/// What the steps should leave, without the ids that differ between runs.
fn settled(sounds: &Value, rooms: &Value) -> Value {
    let mut s = sounds[0].clone();
    if let Some(o) = s.as_object_mut() {
        o.remove("id");
        o.remove("file");
    }
    json!({ "sound": s, "count": sounds.as_array().unwrap().len(), "room": rooms[0]["room"] })
}

fn via_cli(dir: &Path) -> Value {
    let (file, wav) = episode(dir);
    let f = file.to_string_lossy().to_string();
    let run = |args: &[&str]| {
        let mut v = vec!["-p", &f];
        v.extend_from_slice(args);
        cli(&v)
    };
    let imported = run(&["sfx", "import", &wav.to_string_lossy()]);
    let asset = imported["assets"][0]["id"].as_str().unwrap().to_string();
    run(&[
        "sfx", "add", &asset, "--scene", "1", "--depth", "0.25", "--at", "0.5",
    ]);
    let list = run(&["sfx", "list", "--scene", "1"]);
    let id = list[0]["id"].as_str().unwrap().to_string();
    run(&["sfx", "gain", &id, "-6.5"]);
    run(&["sfx", "pan", &id, "-0.5"]);
    run(&["sfx", "depth", &id, "0.8"]);
    run(&["sfx", "fade", &id, "--in", "0.05", "--out", "0.2"]);
    run(&["sfx", "trim", &id, "end", "-0.5"]);
    run(&["sfx", "fade", &id, "--auto"]);
    run(&["sfx", "slip", &id, "0.25"]);
    run(&["sfx", "move", &id, "--at", "1.5", "--offset", "-0.25"]);
    let edits = json!([
        { "op": "gain", "placement_id": id, "gain_db": -3.0 },
        { "op": "depth", "placement_id": id, "depth": 0.6 },
    ])
    .to_string();
    run(&["sfx", "batch", &edits]);
    run(&["sfx", "gain", &id, "-12"]);
    run(&["undo"]);
    run(&["sfx", "room", "1", "--size", "0.8", "--wet", "0.7"]);
    settled(
        &run(&["sfx", "list", "--scene", "1"]),
        &run(&["sfx", "rooms"]),
    )
}

async fn via_mcp(dir: &Path) -> Value {
    let (file, wav) = episode(dir);
    let (a, b) = tokio::io::duplex(1 << 20);
    let server = CutawayMcp::new(Session::new("mcp"));
    tokio::spawn(async move {
        let s = server.serve(a).await.unwrap();
        let _ = s.waiting().await;
    });
    let client: Client = ().serve(b).await.unwrap();
    let c = &client;
    tool(c, "open_project", json!({ "path": file.to_string_lossy() })).await;
    let imported = tool(
        c,
        "import_sounds",
        json!({ "paths": [wav.to_string_lossy()] }),
    )
    .await;
    let asset = imported["assets"][0]["id"].as_str().unwrap().to_string();
    tool(
        c,
        "place_from_bucket",
        json!({ "scene": "1", "asset_id": asset, "depth": 0.25, "at_s": 0.5 }),
    )
    .await;
    let list = tool(c, "list_sounds", json!({ "scene": "1" })).await;
    let id = list[0]["id"].as_str().unwrap().to_string();
    let p = |extra: Value| {
        let mut v = json!({ "placement_id": id });
        for (k, val) in extra.as_object().unwrap() {
            v[k] = val.clone();
        }
        v
    };
    tool(c, "set_sound_gain", p(json!({ "gain_db": -6.5 }))).await;
    tool(c, "set_sound_pan", p(json!({ "pan": -0.5 }))).await;
    tool(c, "set_sound_depth", p(json!({ "depth": 0.8 }))).await;
    tool(
        c,
        "set_sound_fades",
        p(json!({ "fade_in_s": 0.05, "fade_out_s": 0.2 })),
    )
    .await;
    tool(
        c,
        "trim_sound",
        p(json!({ "edge": "end", "delta_s": -0.5 })),
    )
    .await;
    tool(c, "set_sound_fades", p(json!({ "auto": true }))).await;
    tool(c, "slip_sound", p(json!({ "in_s": 0.25 }))).await;
    tool(
        c,
        "move_sound",
        p(json!({ "at_s": 1.5, "offset_s": -0.25 })),
    )
    .await;
    tool(
        c,
        "batch_sounds",
        json!({ "edits": [
            { "op": "gain", "placement_id": id, "gain_db": -3.0 },
            { "op": "depth", "placement_id": id, "depth": 0.6 },
        ] }),
    )
    .await;
    tool(c, "set_sound_gain", p(json!({ "gain_db": -12.0 }))).await;
    tool(c, "undo", json!({})).await;
    tool(
        c,
        "set_scene_room",
        json!({ "scene": "1", "size": 0.8, "wet": 0.7 }),
    )
    .await;
    let out = settled(
        &tool(c, "list_sounds", json!({ "scene": "1" })).await,
        &tool(c, "scene_rooms", json!({})).await,
    );
    let _ = client.cancel().await;
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_and_mcp_shape_a_sound_the_same_way() {
    env();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let p1 = d1.path().to_path_buf();
    let cli_result = tokio::task::spawn_blocking(move || via_cli(&p1))
        .await
        .unwrap();
    let mcp_result = via_mcp(d2.path()).await;
    assert_eq!(cli_result, mcp_result);
    // and the steps did what they say
    let s = &cli_result["sound"];
    assert_eq!(s["gain_db"], json!(-3.0), "the -12 was undone");
    assert_eq!(s["depth"], json!(0.6));
    assert_eq!(s["pan"], json!(-0.5));
    assert_eq!(s["in_s"], json!(0.25));
    assert_eq!(s["len_s"], json!(1.5));
    assert_eq!(s["start_s"], json!(1.25));
    assert_eq!(s["auto_fade"], json!(true));
    assert_eq!(s["fade_out_s"], json!(0.3));
    assert_eq!(cli_result["room"]["size"], json!(0.8));
    assert_eq!(cli_result["room"]["wet"], json!(0.7));
}

#[test]
fn every_sound_action_is_a_tool() {
    let tools: Vec<String> = apc_mcp::tools()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    for name in [
        "list_sounds",
        "place_sound",
        "place_from_bucket",
        "move_sound",
        "trim_sound",
        "set_sound_fades",
        "set_sound_gain",
        "set_sound_pan",
        "set_sound_depth",
        "slip_sound",
        "remove_sound",
        "batch_sounds",
        "scene_rooms",
        "set_scene_room",
    ] {
        assert!(tools.iter().any(|t| t == name), "{name} is missing");
    }
}

#[test]
fn the_tool_schemas_describe_their_parameters() {
    let tools = apc_mcp::tools();
    let schema = |name: &str| {
        let t = tools.iter().find(|t| t.name == name).unwrap();
        serde_json::to_value(t.input_schema.as_ref()).unwrap()
    };
    assert!(schema("set_sound_depth")["properties"]["depth"].is_object());
    let edits = schema("batch_sounds").to_string();
    assert!(edits.contains("gain_db") && edits.contains("anchor_line_id"));
}
