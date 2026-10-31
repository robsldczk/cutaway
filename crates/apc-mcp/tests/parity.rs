//! M4 acceptance: the pipeline through the CLI and through MCP tools (in-process client)
//! gives identical results. Runs on the demo session, and on the TARMAC fixture when present.

use std::path::{Path, PathBuf};

use clap::Parser;
use rmcp::model::CallToolRequestParams;
use rmcp::ServiceExt;
use serde_json::{json, Value};

use apc_app::Session;
use apc_core::Document;
use apc_mcp::CutawayMcp;

fn env() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
}

fn cli(args: &[&str]) -> Value {
    let mut v = vec!["cutaway"];
    v.extend_from_slice(args);
    apc_cli::run(apc_cli::Cli::parse_from(v)).unwrap()
}

/// Canonical summary of a document: independent of random ids.
fn summary(doc: &Document) -> Value {
    let text = |lid: &str| {
        doc.lines
            .get(lid)
            .map(|l| l.text.clone())
            .unwrap_or_default()
    };
    let mut takes: Vec<Value> = doc
        .takes
        .values()
        .map(|t| {
            json!([
                text(&t.line_id),
                t.take_no,
                (t.t0 * 1000.0).round(),
                (t.t1 * 1000.0).round(),
                t.pick,
                t.partial,
                t.feedback
            ])
        })
        .collect();
    takes.sort_by_key(|v| v.to_string());
    let mut selected: Vec<Value> = doc
        .boxes
        .iter()
        .map(|(lid, b)| {
            json!([
                text(lid),
                b.take_id
                    .as_ref()
                    .and_then(|t| doc.takes.get(t))
                    .map(|t| t.take_no)
            ])
        })
        .collect();
    selected.sort_by_key(|v| v.to_string());
    let mut unmatched: Vec<String> = doc.unmatched.values().map(|u| u.text.clone()).collect();
    unmatched.sort();
    let offsets: Vec<i64> = {
        let mut o: Vec<i64> = doc
            .sources
            .values()
            .map(|s| (s.offset_s * 1000.0).round() as i64)
            .collect();
        o.sort();
        o
    };
    json!({ "takes": takes, "selected": selected, "unmatched": unmatched, "offsets": offsets })
}

fn via_cli(dir: &Path, script: &Path, audio: &[PathBuf]) -> Document {
    let r = cli(&["new", "Ep", "--parent", dir.to_str().unwrap()]);
    let file = r["file"].as_str().unwrap().to_string();
    cli(&["-p", &file, "import-script", script.to_str().unwrap()]);
    let mut a = vec!["-p", &file, "import-audio"];
    let paths: Vec<String> = audio
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    a.extend(paths.iter().map(String::as_str));
    cli(&a);
    let st = cli(&["-p", &file, "align"]);
    assert!(st["takes_found"].as_u64().unwrap() > 0);
    apc_core::store::EpisodeStore::open(Path::new(&file))
        .unwrap()
        .load_document()
        .unwrap()
}

async fn tool(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    args: Value,
) -> Value {
    let Value::Object(map) = args else { panic!() };
    let r = client
        .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(map))
        .await
        .unwrap();
    assert_ne!(r.is_error, Some(true), "{name} failed: {:?}", r.content);
    r.structured_content.unwrap()["result"].clone()
}

async fn via_mcp(dir: &Path, script: &Path, audio: &[PathBuf]) -> (Document, Vec<String>) {
    let (a, b) = tokio::io::duplex(1 << 20);
    let server = CutawayMcp::new(Session::new("mcp"));
    tokio::spawn(async move {
        let s = server.serve(a).await.unwrap();
        let _ = s.waiting().await;
    });
    let client = ().serve(b).await.unwrap();
    let tools: Vec<String> = client
        .list_tools(None)
        .await
        .unwrap()
        .tools
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    let r = tool(
        &client,
        "create_episode",
        json!({ "parent": dir.to_string_lossy(), "name": "Ep" }),
    )
    .await;
    let file = r["file"].as_str().unwrap().to_string();
    tool(
        &client,
        "import_script",
        json!({ "path": script.to_string_lossy() }),
    )
    .await;
    tool(
        &client,
        "import_audio",
        json!({ "paths": audio.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>() }),
    )
    .await;
    tool(&client, "run_alignment", json!({})).await;
    // destructive tools need confirm
    let refused = client
        .call_tool(
            CallToolRequestParams::new("revert")
                .with_arguments(json!({"backup_id": "x"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(refused.is_error, Some(true));
    // resources
    let res = client.list_resources(None).await.unwrap();
    assert!(res.resources.iter().any(|r| r.uri == "cutaway://cutlist"));
    let report = client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            "cutaway://report",
        ))
        .await
        .unwrap();
    assert!(!report.contents.is_empty());
    let _ = client.cancel().await;
    (
        apc_core::store::EpisodeStore::open(Path::new(&file))
            .unwrap()
            .load_document()
            .unwrap(),
        tools,
    )
}

fn check(script: PathBuf, audio: Vec<PathBuf>) {
    env();
    let d1 = tempfile::tempdir().unwrap();
    let d2 = tempfile::tempdir().unwrap();
    let from_cli = via_cli(d1.path(), &script, &audio);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (from_mcp, tools) = rt.block_on(via_mcp(d2.path(), &script, &audio));
    for t in [
        "project_status",
        "list_scenes",
        "import_script",
        "import_audio",
        "confirm_roles",
        "run_transcription",
        "run_alignment",
        "run_notes",
        "job_status",
        "list_lines",
        "list_takes",
        "select_take",
        "apply_take_to_passage",
        "set_gap",
        "trim_line",
        "undo",
        "spot_sound_effects",
        "list_bucket",
        "add_cue",
        "attach_sound",
        "search_sounds",
        "generate_sound",
        "place_sound",
        "export_fcpxml",
        "export_mix",
        "export_srt",
        "export_cut_list",
        "open_project",
    ] {
        assert!(tools.contains(&t.to_string()), "missing MCP tool {t}");
    }
    assert!(!from_cli.takes.is_empty());
    assert_eq!(summary(&from_cli), summary(&from_mcp));
}

#[test]
fn cli_and_mcp_agree_on_the_demo_session() {
    let dir = tempfile::tempdir().unwrap();
    let wavs = apc_app::demo::write_session(dir.path()).unwrap();
    check(dir.path().join("Demo Episode.fountain"), wavs);
}

#[test]
fn cli_and_mcp_agree_on_the_fixture() {
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tarmac-ep2");
    if !fx.join("Paul-9min_v001_.mp3").exists() {
        eprintln!("skipped: fixture not present");
        return;
    }
    check(
        fx.join("tarmac-episode-2_rob-test.fountain"),
        vec![
            fx.join("Paul-9min_v001_.mp3"),
            fx.join("Paul-Room-9min_v001_.mp3"),
        ],
    );
}
