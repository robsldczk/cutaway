//! Fountain parser tuned for Throughline exports.
//!
//! Handles the title page and the `===` page break, scene headings (also forced `.`),
//! character cues with extensions (`SARA (O.S.)`), parentheticals that split a
//! dialogue block into sub-lines, `…`/`...` lines that split too, forced action `!`,
//! transitions (`>` and `CUT TO:`), centered text `>…<`, `--` action lines, lyrics `~`,
//! and keeps notes `[[ ]]`, sections `#` and synopses `=` as metadata. Synopses become the
//! scene's story synopsis; a top-level section (`# ACT 2`) is the act of the scenes after it.
//! Throughline's native `.thrl` files are read by [`crate::thrl`] into the same types.

use std::collections::BTreeMap;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::model::{LineKind, SceneStory};
use crate::story::StoryContext;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct ParsedUnit {
    pub kind: LineKind,
    pub character: Option<String>,
    pub extension: Option<String>,
    pub text: String,
    pub paren: Option<String>,
    pub forced: bool,
    /// 1-based line number in the source file (first line of the unit).
    pub source_line: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct ParsedScene {
    pub heading: String,
    pub int_ext: String,
    pub location: String,
    pub time_of_day: String,
    /// Forced scene number (`#12#`) if present.
    pub number: Option<String>,
    pub units: Vec<ParsedUnit>,
    /// Sections (`#`) written before this scene.
    pub sections: Vec<String>,
    pub synopses: Vec<String>,
    pub notes: Vec<String>,
    #[serde(default)]
    pub story: SceneStory,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct ParsedScript {
    pub title: BTreeMap<String, String>,
    /// Units before the first scene heading (kept, but not part of any scene).
    pub preamble: Vec<ParsedUnit>,
    pub scenes: Vec<ParsedScene>,
    /// Character bios and settings (Throughline); empty for Fountain.
    #[serde(default)]
    pub story: StoryContext,
}

impl ParsedScript {
    /// All character names that speak, in order of first appearance.
    pub fn characters(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.scenes {
            for u in &s.units {
                if let Some(c) = &u.character {
                    if !out.contains(c) {
                        out.push(c.clone());
                    }
                }
            }
        }
        out
    }
}

const TIMES: &[&str] = &[
    "DAY",
    "NIGHT",
    "LATER",
    "CONTINUOUS",
    "MORNING",
    "EVENING",
    "AFTERNOON",
    "DUSK",
    "DAWN",
    "SAME",
    "SAME TIME",
    "MOMENTS LATER",
    "MOMENTS EARLIER",
    "NOON",
    "MIDNIGHT",
    "SUNSET",
    "SUNRISE",
];

struct Patterns {
    scene: Regex,
    scene_number: Regex,
    note: Regex,
    boneyard: Regex,
}

fn patterns() -> Patterns {
    // These literals are valid regexes; a failure here is a programming error caught by tests.
    let re = |s: &str| Regex::new(s).unwrap_or_else(|e| panic!("bad regex {s}: {e}"));
    Patterns {
        scene: re(r"(?i)^(INT\.?/EXT|INT/EXT|I/E|INT|EXT|EST)[\. ]"),
        scene_number: re(r"\s*#([A-Za-z0-9.\-]+)#\s*$"),
        note: re(r"\[\[(?s:.*?)\]\]"),
        boneyard: re(r"/\*(?s:.*?)\*/"),
    }
}

/// Parses Fountain text.
pub fn parse(source: &str) -> ParsedScript {
    let p = patterns();
    let text = source.replace("\r\n", "\n").replace('\r', "\n");
    let text = p.boneyard.replace_all(&text, "").into_owned();
    let lines: Vec<&str> = text.split('\n').collect();

    let mut script = ParsedScript::default();
    let mut idx = 0;

    // Title page: key: value pairs at the start, up to the first blank line (or ===).
    let has_title = lines
        .first()
        .map(|l| l.contains(':') && !p.scene.is_match(l.trim()))
        .unwrap_or(false);
    if has_title {
        let mut key: Option<String> = None;
        while idx < lines.len() {
            let l = lines[idx];
            if l.trim() == "===" {
                idx += 1;
                break;
            }
            if l.trim().is_empty() {
                // a blank line ends the title page unless a page break follows later
                let rest_has_break = lines[idx..]
                    .iter()
                    .take_while(|x| !p.scene.is_match(x.trim()))
                    .any(|x| x.trim() == "===");
                if !rest_has_break {
                    idx += 1;
                    break;
                }
                idx += 1;
                continue;
            }
            if l.starts_with(' ') || l.starts_with('\t') {
                if let Some(k) = &key {
                    let e = script.title.entry(k.clone()).or_default();
                    if !e.is_empty() {
                        e.push('\n');
                    }
                    e.push_str(l.trim());
                }
            } else if let Some((k, v)) = l.split_once(':') {
                let k = k.trim().to_string();
                script.title.insert(k.clone(), v.trim().to_string());
                key = Some(k);
            }
            idx += 1;
        }
    }

    // Split the rest into blocks separated by blank lines, keeping source line numbers.
    let mut blocks: Vec<(usize, Vec<String>)> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut start = idx + 1;
    for (i, l) in lines.iter().enumerate().skip(idx) {
        if l.trim().is_empty() && !l.starts_with("  ") {
            if !cur.is_empty() {
                blocks.push((start, std::mem::take(&mut cur)));
            }
            start = i + 2;
        } else {
            if cur.is_empty() {
                start = i + 1;
            }
            cur.push(l.trim_end().to_string());
        }
    }
    if !cur.is_empty() {
        blocks.push((start, cur));
    }

    let mut scene: Option<ParsedScene> = None;
    let mut pending_sections: Vec<String> = Vec::new();
    let mut act = String::new();
    for (line_no, block) in blocks {
        let mut lines: Vec<String> = block;
        // metadata lines: notes, sections, synopses, page breaks
        let mut kept: Vec<String> = Vec::new();
        for l in lines.drain(..) {
            let t = l.trim();
            if t == "===" {
                continue;
            }
            if t.starts_with('#') {
                let s = t.trim_start_matches('#').trim().to_string();
                if t.chars().take_while(|c| *c == '#').count() == 1 {
                    act = s.clone();
                }
                pending_sections.push(s);
                continue;
            }
            if t.starts_with('=') && !t.starts_with("==") {
                if let Some(sc) = scene.as_mut() {
                    sc.synopses
                        .push(t.trim_start_matches('=').trim().to_string());
                }
                continue;
            }
            let mut owned = l.clone();
            for m in p.note.find_iter(&l) {
                let n = m
                    .as_str()
                    .trim_start_matches("[[")
                    .trim_end_matches("]]")
                    .trim()
                    .to_string();
                if let Some(sc) = scene.as_mut() {
                    sc.notes.push(n);
                }
                owned = owned.replace(m.as_str(), "");
            }
            if owned.trim().is_empty() && !l.trim().is_empty() {
                continue;
            }
            kept.push(owned);
        }
        if kept.is_empty() {
            continue;
        }
        let first = kept[0].trim().to_string();

        // Scene heading (also forced with a single leading dot)
        let forced_scene = first.starts_with('.') && !first.starts_with("..");
        if p.scene.is_match(&first) || forced_scene {
            if let Some(sc) = scene.take() {
                script.scenes.push(sc);
            }
            let mut heading = if forced_scene {
                first[1..].trim().to_string()
            } else {
                first.clone()
            };
            let mut number = None;
            if let Some(c) = p.scene_number.captures(&heading) {
                number = c.get(1).map(|m| m.as_str().to_string());
                heading = p.scene_number.replace(&heading, "").trim().to_string();
            }
            let (int_ext, location, time_of_day) = split_heading(&heading);
            scene = Some(ParsedScene {
                heading,
                int_ext,
                location,
                time_of_day,
                number,
                sections: std::mem::take(&mut pending_sections),
                story: SceneStory {
                    act: act.clone(),
                    ..Default::default()
                },
                ..Default::default()
            });
            kept.remove(0);
            if kept.is_empty() {
                continue;
            }
        }
        let first = kept[0].trim().to_string();
        let mut units = parse_block(&first, &kept, line_no);
        match scene.as_mut() {
            Some(sc) => sc.units.append(&mut units),
            None => script.preamble.append(&mut units),
        }
    }
    if let Some(sc) = scene.take() {
        script.scenes.push(sc);
    }
    for sc in &mut script.scenes {
        sc.story.synopsis = sc.synopses.join("\n");
    }
    script
}

/// Splits a scene heading into INT/EXT, location and time of day.
pub(crate) fn split_heading(heading: &str) -> (String, String, String) {
    let h = heading.trim();
    let upper = h.to_uppercase();
    let both = [
        "INT./EXT.",
        "INT/EXT.",
        "INT./EXT",
        "INT/EXT",
        "I/E.",
        "I/E",
    ]
    .iter()
    .find(|p| upper.starts_with(*p) && h.get(p.len()..).is_some_and(|r| r.starts_with([' ', '.'])));
    let (int_ext, rest) = match (both, h.find(['.', ' '])) {
        (Some(p), _) => (
            "INT/EXT".to_string(),
            h.get(p.len()..)
                .unwrap_or_default()
                .trim_start_matches('.')
                .trim()
                .to_string(),
        ),
        (None, Some(i)) => {
            let ie = h[..i].trim_end_matches('.').to_string();
            (ie, h[i + 1..].trim_start_matches('.').trim().to_string())
        }
        (None, None) => (String::new(), h.to_string()),
    };
    // split on " - " or " – "
    let parts: Vec<String> = rest
        .replace(" \u{2013} ", " - ")
        .replace(" \u{2014} ", " - ")
        .split(" - ")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() >= 2 {
        let last = parts[parts.len() - 1].to_uppercase();
        if TIMES.contains(&last.as_str()) {
            return (
                int_ext,
                parts[..parts.len() - 1].join(" - "),
                parts[parts.len() - 1].clone(),
            );
        }
    }
    (int_ext, parts.join(" - "), String::new())
}

fn is_character_cue(first: &str, block_len: usize) -> bool {
    if block_len < 2 {
        return false;
    }
    if first.starts_with('@') {
        return true;
    }
    if first.starts_with(['!', '>', '~', '.', '-', '\u{2014}', '\u{2013}']) {
        return false;
    }
    let name = strip_extension(first).0;
    let has_alpha = name.chars().any(|c| c.is_alphabetic());
    has_alpha
        && name == name.to_uppercase()
        && name.chars().count() < 40
        && !name.ends_with("TO:")
        && !name.ends_with(':')
}

pub(crate) fn strip_extension(cue: &str) -> (String, Option<String>) {
    let c = cue.trim().trim_end_matches('^').trim();
    if let Some(open) = c.find('(') {
        if c.ends_with(')') {
            let name = c[..open].trim().to_string();
            let ext = c[open + 1..c.len() - 1].trim().to_string();
            return (name, Some(ext));
        }
    }
    (c.to_string(), None)
}

fn parse_block(first: &str, lines: &[String], line_no: usize) -> Vec<ParsedUnit> {
    let mut out = Vec::new();
    if is_character_cue(first, lines.len()) {
        let (name, extension) = strip_extension(first.trim_start_matches('@'));
        let mut paren: Option<String> = None;
        let mut buf: Vec<String> = Vec::new();
        let mut unit_line = line_no + 1;
        let flush = |buf: &mut Vec<String>,
                     paren: &Option<String>,
                     out: &mut Vec<ParsedUnit>,
                     at: usize| {
            if !buf.is_empty() {
                out.push(ParsedUnit {
                    kind: LineKind::Dialogue,
                    character: Some(name.clone()),
                    extension: extension.clone(),
                    text: buf.join(" "),
                    paren: paren.clone(),
                    forced: false,
                    source_line: at,
                });
                buf.clear();
            }
        };
        for (k, l) in lines.iter().enumerate().skip(1) {
            let s = l.trim();
            if s.starts_with('(') && s.ends_with(')') {
                flush(&mut buf, &paren, &mut out, unit_line);
                paren = Some(s[1..s.len() - 1].trim().to_string());
                unit_line = line_no + k + 1;
            } else if s == "\u{2026}" || s == "..." {
                flush(&mut buf, &paren, &mut out, unit_line);
                paren = None;
                unit_line = line_no + k + 1;
            } else {
                if buf.is_empty() {
                    unit_line = line_no + k;
                }
                buf.push(s.to_string());
            }
        }
        flush(&mut buf, &paren, &mut out, unit_line);
        return out;
    }

    // Transition: "> FADE TO BLACK" (not centered), or an upper-case line ending in "TO:".
    if lines.len() == 1 {
        let t = first.trim();
        if t.starts_with('>') && t.ends_with('<') {
            out.push(ParsedUnit {
                kind: LineKind::Centered,
                character: None,
                extension: None,
                text: t
                    .trim_start_matches('>')
                    .trim_end_matches('<')
                    .trim()
                    .to_string(),
                paren: None,
                forced: true,
                source_line: line_no,
            });
            return out;
        }
        if t.starts_with('>') {
            out.push(ParsedUnit {
                kind: LineKind::Transition,
                character: None,
                extension: None,
                text: t.trim_start_matches('>').trim().to_string(),
                paren: None,
                forced: true,
                source_line: line_no,
            });
            return out;
        }
        // `!` forces action (`!CUT TO:` is an action line, not a transition)
        if !t.starts_with('!')
            && t == t.to_uppercase()
            && t.ends_with("TO:")
            && t.chars().any(|c| c.is_alphabetic())
        {
            out.push(ParsedUnit {
                kind: LineKind::Transition,
                character: None,
                extension: None,
                text: t.to_string(),
                paren: None,
                forced: false,
                source_line: line_no,
            });
            return out;
        }
    }

    if lines.iter().all(|l| l.trim().starts_with('~')) {
        let text = lines
            .iter()
            .map(|l| l.trim().trim_start_matches('~').trim())
            .collect::<Vec<_>>()
            .join(" ");
        out.push(ParsedUnit {
            kind: LineKind::Lyric,
            character: None,
            extension: None,
            text,
            paren: None,
            forced: true,
            source_line: line_no,
        });
        return out;
    }

    // Action (forced `!`, `--` beats and plain text). One unit per block, like Stage 0.
    let forced = first.trim().starts_with('!');
    let text = lines
        .iter()
        .map(|l| {
            l.trim()
                .trim_start_matches(['!', '>', '-'])
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !text.is_empty() {
        out.push(ParsedUnit {
            kind: LineKind::Action,
            character: None,
            extension: None,
            text,
            paren: None,
            forced,
            source_line: line_no,
        });
    }
    out
}

/// Writes a parsed script back as Fountain (used to show a `.thrl` version as text).
/// Sub-lines of one speech go back into one block: a parenthetical before its sub-line,
/// `…` between sub-lines without one, so [`parse`] reads the same units again.
pub fn render(script: &ParsedScript) -> String {
    let mut out = String::new();
    // the usual title page order first, then anything else
    const ORDER: [&str; 6] = [
        "Title",
        "Credit",
        "Author",
        "Source",
        "Draft date",
        "Contact",
    ];
    let mut keys: Vec<&String> = script.title.keys().collect();
    keys.sort_by_key(|k| ORDER.iter().position(|o| o == k).unwrap_or(ORDER.len()));
    for (k, v) in keys.into_iter().map(|k| (k, &script.title[k])) {
        let mut lines = v.lines();
        out.push_str(&format!("{k}: {}\n", lines.next().unwrap_or_default()));
        for l in lines {
            out.push_str(&format!("    {l}\n"));
        }
    }
    if !script.title.is_empty() {
        out.push_str("\n===\n\n");
    }
    let mut act = String::new();
    for sc in &script.scenes {
        if !sc.story.act.is_empty() && sc.story.act != act {
            act = sc.story.act.clone();
            out.push_str(&format!("# {act}\n\n"));
        }
        let heading = if p_scene_like(&sc.heading) {
            sc.heading.clone()
        } else {
            format!(".{}", sc.heading)
        };
        match &sc.number {
            Some(n) => out.push_str(&format!("{heading} #{n}#\n\n")),
            None => out.push_str(&format!("{heading}\n\n")),
        }
        for s in sc.story.synopsis.lines().filter(|s| !s.trim().is_empty()) {
            out.push_str(&format!("= {}\n", s.trim()));
        }
        if !sc.story.synopsis.trim().is_empty() {
            out.push('\n');
        }
        let mut i = 0;
        while i < sc.units.len() {
            let u = &sc.units[i];
            match u.kind {
                LineKind::Dialogue => {
                    let name = u.character.clone().unwrap_or_default();
                    match &u.extension {
                        Some(e) => out.push_str(&format!("{name} ({e})\n")),
                        None => out.push_str(&format!("{name}\n")),
                    }
                    let mut first = true;
                    while i < sc.units.len() {
                        let d = &sc.units[i];
                        if d.kind != LineKind::Dialogue
                            || d.character != u.character
                            || d.extension != u.extension
                        {
                            break;
                        }
                        match &d.paren {
                            Some(p) => out.push_str(&format!("({p})\n")),
                            None if !first => out.push_str("\u{2026}\n"),
                            None => {}
                        }
                        out.push_str(&format!("{}\n", d.text));
                        first = false;
                        i += 1;
                    }
                    out.push('\n');
                    continue;
                }
                LineKind::Action if u.forced => out.push_str(&format!("!{}\n\n", u.text)),
                LineKind::Action => out.push_str(&format!("{}\n\n", u.text)),
                LineKind::Transition if u.text.ends_with("TO:") && !u.forced => {
                    out.push_str(&format!("{}\n\n", u.text))
                }
                LineKind::Transition => out.push_str(&format!("> {}\n\n", u.text)),
                LineKind::Centered => out.push_str(&format!("> {} <\n\n", u.text)),
                LineKind::Lyric => out.push_str(&format!("~{}\n\n", u.text)),
            }
            i += 1;
        }
    }
    out
}

fn p_scene_like(heading: &str) -> bool {
    patterns().scene.is_match(heading.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Title: TEST - Episode 1\nAuthor: Someone\nContact:\n    Line one\n    Line two\n\n===\n\nINT. TARMAC - TIME SPRING COMMAND - DAY\n\nA BLACKBOARD rips down.\n\nPAUL\n(hyping himself)\nSo you don\u{2019}t think I\u{2019}m a scientist, huh? I\u{2019}ll show you SCIENCE!\n\n-- PAUL looks into a microscope.\n\nSARA (O.S.)\nPaul! PAUL!\n\nSARA\nI\u{2019}ve been saying your name.\n(turning to leave)\nLunch is getting cold.\n\n!INTRO\n\nINT. TARMAC - CANTEEN - LATER\n\n!TEXT: 22 FLOORS BELOW\n\nNORBERT\nSir.\n\u{2026}\nYes, sir.\n\n> FADE TO BLACK\n\nEXT. RANCH \u{2013} LIVING ROOM - EVENING\n\nCUT TO:\n\n[[a note]]\n\n= a synopsis\n";

    #[test]
    fn parses_title_scenes_and_units() {
        let s = parse(SAMPLE);
        assert_eq!(
            s.title.get("Title").map(String::as_str),
            Some("TEST - Episode 1")
        );
        assert_eq!(
            s.title.get("Contact").map(String::as_str),
            Some("Line one\nLine two")
        );
        assert_eq!(s.scenes.len(), 3);
        let s1 = &s.scenes[0];
        assert_eq!(s1.location, "TARMAC - TIME SPRING COMMAND");
        assert_eq!(s1.time_of_day, "DAY");
        let kinds: Vec<_> = s1.units.iter().map(|u| u.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LineKind::Action,
                LineKind::Dialogue,
                LineKind::Action,
                LineKind::Dialogue,
                LineKind::Dialogue,
                LineKind::Dialogue,
                LineKind::Action
            ]
        );
        assert_eq!(s1.units[1].paren.as_deref(), Some("hyping himself"));
        assert_eq!(s1.units[2].text, "PAUL looks into a microscope.");
        assert_eq!(s1.units[3].character.as_deref(), Some("SARA"));
        assert_eq!(s1.units[3].extension.as_deref(), Some("O.S."));
        assert_eq!(s1.units[5].paren.as_deref(), Some("turning to leave"));
        assert_eq!(s1.units[5].text, "Lunch is getting cold.");
        assert!(s1.units[6].forced);
        assert_eq!(s1.units[6].text, "INTRO");
    }

    #[test]
    fn ellipsis_lines_split_dialogue_and_transitions_parse() {
        let s = parse(SAMPLE);
        let s2 = &s.scenes[1];
        assert_eq!(s2.units[0].text, "TEXT: 22 FLOORS BELOW");
        assert_eq!(s2.units[1].text, "Sir.");
        assert_eq!(s2.units[2].text, "Yes, sir.");
        assert_eq!(s2.units[3].kind, LineKind::Transition);
        let s3 = &s.scenes[2];
        assert_eq!(s3.location, "RANCH - LIVING ROOM");
        assert_eq!(s3.time_of_day, "EVENING");
        assert_eq!(s3.units[0].kind, LineKind::Transition);
        assert_eq!(s3.notes, vec!["a note".to_string()]);
        let forced = parse("INT. LAB\n\n!CUT TO:\n");
        assert_eq!(forced.scenes[0].units[0].kind, LineKind::Action);
        assert_eq!(forced.scenes[0].units[0].text, "CUT TO:");
        assert_eq!(s3.synopses, vec!["a synopsis".to_string()]);
    }

    #[test]
    fn sections_are_acts_of_the_following_scenes() {
        let s = parse("# ACT ONE\n\n## Cold open\n\nINT. LAB - DAY\n\n= Paul sweats.\n= A bell.\n\nPAUL\nHi.\n\nEXT. FIELD - NIGHT\n\nWind.\n\n# ACT TWO\n\nINT. LAB - DAY\n\nQuiet.\n");
        assert_eq!(s.scenes[0].sections, vec!["ACT ONE", "Cold open"]);
        assert_eq!(s.scenes[0].story.act, "ACT ONE");
        assert_eq!(s.scenes[0].story.synopsis, "Paul sweats.\nA bell.");
        assert_eq!(s.scenes[1].story.act, "ACT ONE");
        assert!(s.scenes[1].sections.is_empty());
        assert_eq!(s.scenes[2].story.act, "ACT TWO");
    }

    #[test]
    fn int_ext_headings() {
        let (ie, loc, t) = split_heading("INT./EXT. RANCH - NIGHT");
        assert_eq!(
            (ie.as_str(), loc.as_str(), t.as_str()),
            ("INT/EXT", "RANCH", "NIGHT")
        );
        let (ie, loc, _) = split_heading("I/E RANCH");
        assert_eq!((ie.as_str(), loc.as_str()), ("INT/EXT", "RANCH"));
        let (ie, loc, _) = split_heading("INT. INTERNATIONAL HALL");
        assert_eq!((ie.as_str(), loc.as_str()), ("INT", "INTERNATIONAL HALL"));
    }

    #[test]
    fn render_reads_back_the_same_units() {
        let a = parse(SAMPLE);
        let b = parse(&render(&a));
        let units = |s: &ParsedScript| {
            s.scenes
                .iter()
                .flat_map(|sc| sc.units.iter())
                .map(|u| {
                    (
                        u.kind,
                        u.character.clone(),
                        u.extension.clone(),
                        u.paren.clone(),
                        u.text.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(units(&a), units(&b));
        assert_eq!(a.title, b.title);
        let headings = |s: &ParsedScript| {
            s.scenes
                .iter()
                .map(|sc| sc.heading.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(headings(&a), headings(&b));
        assert_eq!(b.scenes[2].story.synopsis, "a synopsis");
    }

    #[test]
    fn heading_without_time() {
        let (ie, loc, t) = split_heading("INT. TARMAC - COLONEL\u{2019}S OFFICE");
        assert_eq!(ie, "INT");
        assert_eq!(loc, "TARMAC - COLONEL\u{2019}S OFFICE");
        assert_eq!(t, "");
    }
}
