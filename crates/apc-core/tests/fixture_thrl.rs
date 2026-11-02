//! Reads the TARMAC Throughline projects (skips when `fixtures/` is absent): the EP02
//! `.thrl` must give the same lines as Throughline's own Fountain export of that draft.

use std::path::{Path, PathBuf};

use apc_core::fountain::{self, ParsedScript};
use apc_core::script::read_script_file;

fn fixture(name: &str) -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tarmac-ep2")
        .join(name);
    if p.exists() {
        Some(p)
    } else {
        eprintln!("skipped: fixture {} not present", p.display());
        None
    }
}

type Row = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

fn rows(s: &ParsedScript) -> Vec<Row> {
    let mut out = Vec::new();
    for sc in &s.scenes {
        for u in &sc.units {
            out.push((
                sc.location.clone(),
                format!("{:?}", u.kind),
                u.character.clone(),
                u.extension.clone(),
                u.paren.clone(),
                u.text.split_whitespace().collect::<Vec<_>>().join(" "),
            ));
        }
    }
    out
}

fn read(p: &Path) -> ParsedScript {
    read_script_file(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn ep2_thrl_matches_its_fountain_export() {
    let (Some(thrl), Some(fnt)) = (
        fixture("tarmac-episode-2.thrl"),
        fixture("tarmac-episode-2_thrl-export.fountain"),
    ) else {
        return;
    };
    let a = read(&thrl);
    let b = fountain::parse(&std::fs::read_to_string(&fnt).unwrap());
    let headings = |s: &ParsedScript| -> Vec<(String, String, String)> {
        s.scenes
            .iter()
            .map(|sc| {
                (
                    sc.int_ext.clone(),
                    sc.location.clone(),
                    sc.time_of_day.clone(),
                )
            })
            .collect()
    };
    assert_eq!(headings(&a), headings(&b));
    let (ra, rb) = (rows(&a), rows(&b));
    for (i, (x, y)) in ra.iter().zip(rb.iter()).enumerate() {
        assert_eq!(x, y, "unit {i} differs");
    }
    assert_eq!(ra.len(), rb.len());

    // what the Fountain export leaves out
    let s1 = &a.scenes[0];
    assert_eq!(s1.story.title, "Cold Open \u{2013} Burrito");
    assert!(s1.story.synopsis.contains("Paul sweating"));
    assert!(s1.story.tone.is_some());
    assert!(a.scenes.iter().any(|s| s.story.act.starts_with("Act 2")));
    let canteen = a
        .story
        .locations
        .iter()
        .find(|l| l.name == "TARMAC - CANTEEN")
        .expect("canteen setting");
    assert_eq!(canteen.parent.as_deref(), Some("TARMAC - HALLWAY"));
}

#[test]
fn pilot_thrl_carries_the_character_bios() {
    let Some(p) = fixture("tarmac-pilot.thrl") else {
        return;
    };
    let s = read(&p);
    let paul = s
        .story
        .characters
        .iter()
        .find(|c| c.name == "Paul")
        .expect("Paul");
    assert!(paul.profile.summary.contains("Soldier"));
    assert!(!paul.profile.want.is_empty());
    assert!(s.scenes.iter().any(|sc| sc
        .units
        .iter()
        .any(|u| u.character.as_deref() == Some("PAUL"))));
}
