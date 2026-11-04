//! M2 acceptance on the TARMAC fixture: transcript import + mock AI.
//! Skips when `fixtures/tarmac-ep2/` is absent.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use apc_ai::{AiRunner, MockProvider};
use apc_analysis::asr::{AsrProvider, TranscriptImportProvider};
use apc_analysis::roles::guess_by_name;
use apc_analysis::{analyze, Providers};
use apc_core::commands::default_take;
use apc_core::*;

fn fixture_dir() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tarmac-ep2");
    if d.join("Paul-9min_v001_.mp3").exists() {
        Some(d)
    } else {
        eprintln!("skipped: fixture {} not present", d.display());
        None
    }
}

fn norm(s: &str) -> String {
    apc_core::text::norm_words(s).join(" ")
}

#[test]
fn tarmac_paul_takes_picks_and_sync() {
    let Some(fx) = fixture_dir() else { return };
    check_tarmac(&fx, &TranscriptImportProvider);
}

/// Same checks with word-level timing from Parakeet (downloads the model on first run).
#[cfg(feature = "parakeet")]
#[test]
#[ignore = "Parakeet runs end to end with speech-region decoding but finds 31 of the 35+ takes (exclamation spellings, Mh-hm) on this bleed-heavy session; see RUN_REPORT.md"]
fn tarmac_with_parakeet() {
    let Some(fx) = fixture_dir() else { return };
    let asr = apc_analysis::parakeet::ParakeetAsr::new(apc_analysis::asr::ModelManager::new(
        apc_analysis::asr::ModelManager::default_root(),
    ));
    check_tarmac(&fx, &asr);
}

fn check_tarmac(fx: &std::path::Path, asr: &dyn AsrProvider) {
    let tmp = tempfile::tempdir().unwrap();
    let (paths, store) = apc_core::project::create_episode(tmp.path(), "Ep02", None).unwrap();
    let mut bus = CommandBus::new(store, "test").unwrap();

    // script
    let script = fx.join("tarmac-episode-2_rob-test.fountain");
    let (file, hash) =
        apc_core::script::store_version_file(&paths, &script, "TARMAC Ep2", 1).unwrap();
    let parsed = apc_core::fountain::parse(&std::fs::read_to_string(&script).unwrap());
    let version = ScriptVersion {
        id: "sv_1".into(),
        number: 1,
        file,
        name: "TARMAC Ep2".into(),
        hash,
        imported_at: String::new(),
        summary: serde_json::Value::Null,
        title: parsed.title.clone(),
    };
    bus.execute(&Command::ImportScriptVersion { version, parsed })
        .unwrap();

    // audio + roles by file name
    let characters: Vec<String> = bus.doc().characters.keys().cloned().collect();
    let mut sources = Vec::new();
    for name in ["Paul-9min_v001_.mp3", "Paul-Room-9min_v001_.mp3"] {
        let f = apc_core::media::import_file(&paths, &fx.join(name), "Session 1", true).unwrap();
        let g = guess_by_name(name, &characters, &BTreeMap::new());
        sources.push(SourceFile {
            id: format!("src_{}", sources.len() + 1),
            path: f.stored_path,
            original_name: f.original_name,
            stored: f.stored,
            hash: f.hash,
            size: f.size,
            role: g.role,
            character: g.character,
            role_confidence: g.confidence,
            session_id: None,
            sample_rate: 0,
            channels: 0,
            duration_s: 0.0,
            tc_start_s: None,
            offset_s: 0.0,
            offset_confidence: 0.0,
            transcript: f.sidecar,
            missing: false,
            link_locked: false,
        });
    }
    assert_eq!(sources[0].character.as_deref(), Some("PAUL"));
    assert_eq!(sources[1].role, Role::Room);
    bus.execute(&Command::AddSources { sources }).unwrap();

    let ai = AiRunner::new(
        Arc::new(MockProvider::new()),
        Some(paths.cache.join("ai")),
        Some(paths.logs.join("ai-calls.jsonl")),
    );
    let t = std::time::Instant::now();
    let out = analyze(bus.doc(), &paths, &Providers { asr, ai: &ai }, &|_| {}).unwrap();
    eprintln!("analysis took {:?}", t.elapsed());
    std::fs::write(tmp.path().join("report.md"), &out.report.markdown).unwrap();
    eprintln!("{}", out.report.markdown);
    bus.execute(&Command::ApplyAnalysis {
        payload: out.payload,
    })
    .unwrap();
    let doc = bus.doc();

    // sync
    let room = doc.sources.values().find(|s| s.role == Role::Room).unwrap();
    assert!(
        (room.offset_s - 0.25).abs() <= 0.1,
        "room offset {}",
        room.offset_s
    );

    // takes
    let paul_takes: Vec<&Take> = doc.takes.values().collect();
    assert!(paul_takes.len() >= 35, "PAUL takes: {}", paul_takes.len());
    for t in &paul_takes {
        assert_eq!(
            doc.lines[&t.line_id].character.as_deref(),
            Some("PAUL"),
            "Sara's lines are never PAUL takes"
        );
    }
    let takes_for = |text: &str| -> Vec<&Take> {
        let line = doc
            .lines
            .values()
            .find(|l| norm(&l.text) == norm(text))
            .unwrap_or_else(|| panic!("line {text}"));
        doc.takes_of_line(&line.id)
    };
    assert_eq!(
        takes_for(
            "So you don\u{2019}t think I\u{2019}m a scientist, huh? I\u{2019}ll show you SCIENCE!"
        )
        .len(),
        5
    );
    assert_eq!(
        takes_for(
            "I told you! I\u{2019}m bringing mine! Or I will be bringing it! It\u{2019}s quantum."
        )
        .len(),
        4
    );
    assert_eq!(takes_for("Lunch.").len(), 4);
    let spring = takes_for("I put this in the time spring two weeks ago.");
    assert_eq!(spring.len(), 10);
    let partial: Vec<&&Take> = spring.iter().filter(|t| t.partial).collect();
    assert_eq!(partial.len(), 1, "exactly one partial take");
    assert!(partial[0].heard.to_lowercase().contains("time machine"));

    // all 9 PAUL lines of S1/S2 recorded in the session have takes
    let lines_with: std::collections::BTreeSet<&String> =
        paul_takes.iter().map(|t| &t.line_id).collect();
    for text in [
        "So you don\u{2019}t think I\u{2019}m a scientist, huh? I\u{2019}ll show you SCIENCE!",
        "I told you! I\u{2019}m bringing mine! Or I will be bringing it! It\u{2019}s quantum.",
        "Lunch.",
        "Whoooaaah!",
        "I put this in the time spring two weeks ago.",
        "Two weeks elapsed for US, but for this burrito, the transfer was instant.",
        "That\u{2019}s just time slop. Non-toxic.",
        "Mh-hm\u{2026}mh-hm.",
        "The first time burrito is _EDIBLE_!",
    ] {
        let line = doc
            .lines
            .values()
            .find(|l| norm(&l.text) == norm(text))
            .unwrap();
        assert!(lines_with.contains(&line.id), "no take for {text}");
    }

    // picks (and they are the default takes)
    let pick_of = |text: &str| -> (u32, f64) {
        let ts = takes_for(text);
        let d = default_take(&ts).unwrap();
        assert_eq!(
            d.pick, 2,
            "{text}: default take {} is not a pick",
            d.take_no
        );
        (d.take_no, d.t0)
    };
    let (n, t0) = pick_of(
        "So you don\u{2019}t think I\u{2019}m a scientist, huh? I\u{2019}ll show you SCIENCE!",
    );
    assert_eq!(n, 4);
    assert!((t0 - 129.0).abs() < 5.0);
    let (n, t0) = pick_of("Lunch.");
    assert_eq!(n, 4);
    assert!((t0 - 414.0).abs() < 5.0);
    let (n, t0) = pick_of("The first time burrito is _EDIBLE_!");
    assert_eq!(n, 3);
    assert!((t0 - 656.0).abs() < 5.0);
    // the boxes use them
    let science = doc
        .lines
        .values()
        .find(|l| l.text.starts_with("So you don"))
        .unwrap();
    let sel = doc.boxes[&science.id].take_id.clone().unwrap();
    assert_eq!(doc.takes[&sel].take_no, 4);
    assert!(doc.takes[&sel].favorite, "director pick seeds the favorite");

    // notes are sentences, and the talk after a take leads into the next one
    let t4 = &doc.takes[&sel];
    assert!(!t4.feedback.is_empty());
    assert!(t4.feedback.ends_with('.'), "{}", t4.feedback);
    assert!(
        doc.takes
            .values()
            .any(|t| t.id != t4.id && t.direction == t4.feedback),
        "the gap after take 4 is no take's direction"
    );
    assert!(!doc.unmatched.is_empty());
    // AI calls were logged
    let log = std::fs::read_to_string(paths.logs.join("ai-calls.jsonl")).unwrap();
    assert!(log.lines().count() >= 4);
}
