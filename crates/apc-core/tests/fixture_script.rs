//! Parses the TARMAC fixture script (skips when `fixtures/` is absent).

use std::path::PathBuf;

use apc_core::fountain;
use apc_core::LineKind;

fn fixture() -> Option<String> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tarmac-ep2/tarmac-episode-2_rob-test.fountain");
    match std::fs::read_to_string(&p) {
        Ok(s) => Some(s),
        Err(_) => {
            eprintln!("skipped: fixture {} not present", p.display());
            None
        }
    }
}

#[test]
fn tarmac_script_parses_like_stage0() {
    let Some(text) = fixture() else { return };
    let s = fountain::parse(&text);
    assert_eq!(
        s.title.get("Title").map(String::as_str),
        Some("TARMAC - Episode 2")
    );
    assert!(
        s.preamble.is_empty(),
        "title page must not leak into the script"
    );
    let s1 = &s.scenes[0];
    let s2 = &s.scenes[1];
    assert_eq!(s1.heading, "INT. TARMAC - TIME SPRING COMMAND - DAY");
    assert_eq!(s2.heading, "INT. TARMAC - CANTEEN - LATER");
    assert_eq!(s2.location, "TARMAC - CANTEEN");
    // Stage 0 unit numbers (1-based within the scene): S1.3, S1.14, S1.16, S1.18, S2.3, S2.12
    let unit = |sc: &fountain::ParsedScene, n: usize| sc.units[n - 1].text.clone();
    assert!(unit(s1, 3).starts_with("So you don"));
    assert!(unit(s1, 14).starts_with("I told you!"));
    assert_eq!(unit(s1, 16), "Lunch.");
    assert_eq!(unit(s1, 18), "Whoooaaah!");
    assert_eq!(unit(s2, 3), "I put this in the time spring two weeks ago.");
    assert_eq!(
        unit(s2, 6),
        "Two weeks elapsed for US, but for this burrito, the transfer was instant."
    );
    assert_eq!(unit(s2, 7), "That\u{2019}s just time slop. Non-toxic.");
    assert_eq!(
        s2.units[6].paren.as_deref(),
        Some("wipes off some goopy slime")
    );
    assert_eq!(unit(s2, 12), "The first time burrito is _EDIBLE_!");
    assert_eq!(s1.units[2].paren.as_deref(), Some("hyping himself"));
    // cue with extension
    let sara_os = s1
        .units
        .iter()
        .find(|u| u.extension.as_deref() == Some("O.S."))
        .unwrap();
    assert_eq!(sara_os.character.as_deref(), Some("SARA"));
    // forced action and transitions
    assert!(s1.units.iter().any(|u| u.forced && u.text == "INTRO"));
    assert!(s
        .scenes
        .iter()
        .flat_map(|s| &s.units)
        .any(|u| u.kind == LineKind::Transition && u.text == "FADE TO BLACK"));
    let chars = s.characters();
    assert_eq!(&chars[..4], &["PAUL", "SARA", "NORBERT", "THE COLONEL"]);
    let paul_lines = s
        .scenes
        .iter()
        .flat_map(|s| &s.units)
        .filter(|u| u.character.as_deref() == Some("PAUL"))
        .count();
    assert_eq!(paul_lines, 27, "Stage 0 counted 27 PAUL lines");
}
