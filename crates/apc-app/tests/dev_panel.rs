//! Scoped re-analysis, dev-panel marks and the session view, on the synthetic demo session
//! (mock AI, runs everywhere).

use serde_json::json;

use apc_app::demo;
use apc_core::{MarkStatus, Role, TakeOrigin};

fn with_mock() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
}

#[test]
fn a_scoped_run_leaves_other_characters_alone() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let before = svc.doc().clone();
    let chars: Vec<String> = before
        .sources
        .values()
        .filter(|s| s.role == Role::Character)
        .filter_map(|s| s.character.clone())
        .collect();
    assert!(chars.len() >= 2, "{chars:?}");
    let (me, other) = (&chars[0], &chars[1]);
    let theirs = |doc: &apc_core::Document, ch: &str| -> Vec<String> {
        let mut v: Vec<String> = doc
            .takes
            .values()
            .filter(|t| doc.lines[&t.line_id].character.as_deref() == Some(ch))
            .map(|t| t.id.clone())
            .collect();
        v.sort();
        v
    };
    // star the other character's selected take by hand: it must survive
    let fav_line = before
        .boxes
        .values()
        .find(|b| {
            b.take_id
                .as_ref()
                .is_some_and(|t| theirs(&before, other).contains(t) && !before.takes[t].favorite)
        })
        .unwrap()
        .clone();
    let fav = fav_line.take_id.clone().unwrap();
    svc.call("toggle_favorite", &json!({ "line_id": fav_line.id }))
        .unwrap();
    assert!(svc.doc().takes[&fav].favorite);
    let r = svc
        .call("analyze_scope", &json!({ "characters": [me] }))
        .unwrap();
    assert!(r["takes_found"].as_u64().unwrap() > 0, "{r}");
    let after = svc.doc().clone();
    assert_eq!(theirs(&after, other), theirs(&before, other));
    assert_eq!(
        theirs(&after, me),
        theirs(&before, me),
        "same input, same takes"
    );
    assert!(after.takes[&fav].favorite);
    // one undo step
    svc.call("undo", &json!({})).unwrap();
}

#[test]
fn marks_propose_and_apply_and_the_director_mic_is_refused() {
    with_mock();
    let dir = tempfile::tempdir().unwrap();
    let mut svc = demo::create(dir.path(), "test", true).unwrap();
    let doc = svc.doc().clone();
    let room = doc
        .sources
        .values()
        .find(|s| s.role == Role::Room)
        .unwrap()
        .clone();
    let err = svc.call(
        "put_mark",
        &json!({ "source_id": room.id, "t0": 1.0, "t1": 2.0, "kind": "missed_take" }),
    );
    assert!(err.is_err(), "the director mic is never a take");

    // the unmatched "water" question on an actor's mic, linked by hand to one of their lines
    let um = doc
        .unmatched
        .values()
        .find(|u| u.text.contains("water"))
        .unwrap()
        .clone();
    let src = doc.sources[&um.source_id].clone();
    let line = doc
        .lines
        .values()
        .find(|l| l.character == src.character && l.kind == apc_core::LineKind::Dialogue)
        .unwrap()
        .clone();
    let r = svc
        .call(
            "put_mark",
            &json!({ "source_id": src.id, "t0": um.t0, "t1": um.t1, "kind": "missed_take",
                     "line_id": line.id, "note": "test link" }),
        )
        .unwrap();
    let mark_id = r["mark_id"].as_str().unwrap().to_string();
    assert!(svc.doc().marks[&mark_id].text.contains("water"));

    // propose as linked (the job runs the same code; call it directly here)
    let mark = svc.doc().marks[&mark_id].clone();
    svc.analyze_with(
        apc_analysis::AnalysisScope {
            marks: vec![mark],
            manual: true,
            ..apc_analysis::AnalysisScope::everything()
        },
        &|_| {},
    )
    .unwrap();
    let m = svc.doc().marks[&mark_id].clone();
    assert_eq!(m.status, MarkStatus::Proposed);
    let prop = m.proposal.clone().unwrap();
    assert_eq!(prop.add.len(), 1, "{:?}", prop.summary);
    assert_eq!(prop.add[0].origin, TakeOrigin::Manual);
    assert_eq!(prop.add[0].line_id, line.id);
    let n_before = svc.doc().takes_of_line(&line.id).len();
    svc.call("apply_mark", &json!({ "mark_id": mark_id }))
        .unwrap();
    let takes = svc.doc().takes_of_line(&line.id);
    assert_eq!(takes.len(), n_before + 1);
    let nos: Vec<u32> = takes.iter().map(|t| t.take_no).collect();
    assert_eq!(
        nos,
        (1..=takes.len() as u32).collect::<Vec<_>>(),
        "renumbered"
    );

    // the session view shows the new take and never a director-mic take
    let v = svc
        .call("dev_session", &json!({ "source_id": src.id }))
        .unwrap();
    let rows = v["rows"].as_array().unwrap();
    assert!(rows.iter().any(
        |r| r["status"]["origin"] == "manual" && r["text"].as_str().unwrap().contains("water")
    ));
    assert!(rows
        .iter()
        .filter(|r| r["mic"] == "director")
        .all(|r| r["status"]["kind"] == "director"));
    assert!(v["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| !s["lines"].as_array().unwrap().is_empty()));
}
