//! Throughline `.thrl` reader. A `.thrl` file is a Throughline project exported as
//! (usually gzipped) JSON: beat cards whose script is a TipTap document, act cards,
//! characters, settings and arcs. It becomes the same [`ParsedScript`] a Fountain file
//! does, plus the story context the Fountain export leaves out (D54).
//!
//! Block rules follow Throughline's own Fountain export, so a `.thrl` and a Fountain
//! export of the same draft give the same lines: a blank line inside an action starts a
//! new unit, blank lines inside a speech are dropped, a `…` line or a parenthetical
//! splits a speech, and only the active version of a `versionBlock` is read. Underline,
//! italic and bold become Fountain emphasis (`_x_`, `*x*`, `**x**`), as in the export.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::fountain::{split_heading, strip_extension, ParsedScene, ParsedScript, ParsedUnit};
use crate::model::{CharacterProfile, LineKind, SceneStory};
use crate::story::{location_name, StoryCharacter, StoryContext, StoryLocation};

/// Newest format this reader knows; a file whose `minVersion` is newer is refused.
pub const READS_UP_TO: &str = "1.12";

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b)
}

/// True for gzip data or a JSON object (a Fountain file never starts with `{`).
pub fn looks_like_thrl(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return true;
    }
    strip_bom(bytes).trim_ascii_start().starts_with(b"{")
}

/// Parses a `.thrl` file's bytes (gzipped or plain JSON).
pub fn parse(bytes: &[u8]) -> Result<ParsedScript> {
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes).read_to_end(&mut out)?;
        out
    } else {
        bytes.to_vec()
    };
    let v: Value = serde_json::from_slice(strip_bom(&raw))?;
    from_value(&v)
}

fn version(s: &str) -> (u32, u32) {
    let mut it = s.trim().split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn text_of(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn items<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn id_of(v: &Value) -> &str {
    v.get("id").and_then(Value::as_str).unwrap_or_default()
}

fn stacked_on(v: &Value) -> Option<&str> {
    v.get("stackedOn")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Reading order of beat and act-card ids: `storyOrder` when the file has it, else the
/// rule of the format guide (top-level items by `sequence`, each followed by the beats
/// stacked on it by `stackOrder`).
fn story_order(v: &Value) -> Vec<String> {
    let given: Vec<String> = items(v, "storyOrder")
        .iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect();
    if !given.is_empty() {
        return given;
    }
    let beats = items(v, "beats");
    let mut top: Vec<&Value> = beats
        .iter()
        .filter(|b| stacked_on(b).is_none())
        .chain(items(v, "structures").iter())
        .collect();
    top.sort_by(|a, b| num(a, "sequence").total_cmp(&num(b, "sequence")));
    let mut out = Vec::new();
    for t in top {
        out.push(id_of(t).to_string());
        let mut stack: Vec<&Value> = beats
            .iter()
            .filter(|b| stacked_on(b) == Some(id_of(t)))
            .collect();
        stack.sort_by(|a, b| num(a, "stackOrder").total_cmp(&num(b, "stackOrder")));
        out.extend(stack.iter().map(|b| id_of(b).to_string()));
    }
    // beats stacked on something that isn't there: keep them, at the end
    let seen: HashSet<String> = out.iter().cloned().collect();
    out.extend(
        beats
            .iter()
            .map(|b| id_of(b).to_string())
            .filter(|id| !seen.contains(id)),
    );
    out
}

/// A Throughline project as JSON → parsed script with story context.
pub fn from_value(v: &Value) -> Result<ParsedScript> {
    if !v.get("beats").is_some_and(Value::is_array) {
        return Err(CoreError::invalid(
            "not a Throughline project (no beats in the file)",
        ));
    }
    let min = text_of(v, "minVersion");
    if !min.is_empty() && version(&min) > version(READS_UP_TO) {
        return Err(CoreError::invalid(format!(
            "this Throughline file needs a reader for format {min}; Cutaway reads up to {READS_UP_TO}"
        )));
    }

    let beats: HashMap<&str, &Value> = items(v, "beats").iter().map(|b| (id_of(b), b)).collect();
    let structures: HashMap<&str, &Value> = items(v, "structures")
        .iter()
        .map(|s| (id_of(s), s))
        .collect();
    let worlds: HashMap<&str, &Value> = items(v, "worlds").iter().map(|w| (id_of(w), w)).collect();
    let names: HashMap<&str, String> = items(v, "characters")
        .iter()
        .map(|c| (id_of(c), text_of(c, "name")))
        .collect();

    // tone per beat from the default arc (legacy files: `fortune` 0..1 on the beat)
    let mut tone: HashMap<String, f64> = HashMap::new();
    if let Some(arc) = items(v, "arcs")
        .iter()
        .find(|a| a.get("isDefault").and_then(Value::as_bool) == Some(true))
    {
        for p in items(arc, "points") {
            if let Some(c) = p.get("charge").and_then(Value::as_f64) {
                tone.insert(text_of(p, "beatId"), c.clamp(-1.0, 1.0));
            }
        }
    }
    let round = |x: f64| (x * 100.0).round() / 100.0;

    let mut script = ParsedScript::default();
    let project = v.get("project").unwrap_or(&Value::Null);
    let draft = project
        .pointer("/exportSettings/screenplayPdf/dateText")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    for (k, val) in [
        ("Title", text_of(project, "title")),
        ("Credit", text_of(project, "creditLine")),
        ("Author", text_of(project, "authorName")),
        ("Draft date", draft),
    ] {
        if !val.is_empty() {
            script.title.insert(k.to_string(), val);
        }
    }

    let mut acts: HashMap<String, String> = HashMap::new();
    let mut block_no = 0usize;
    for id in story_order(v) {
        let Some(b) = beats.get(id.as_str()) else {
            continue; // an act card
        };
        let act = match stacked_on(b) {
            Some(s) => match structures.get(s) {
                Some(st) => text_of(st, "title"),
                None => acts.get(s).cloned().unwrap_or_default(),
            },
            None => String::new(),
        };
        acts.insert(id.clone(), act.clone());

        let world = b
            .get("sleeveId")
            .and_then(Value::as_str)
            .and_then(|w| worlds.get(w))
            .map(|w| location_name(&text_of(w, "title")))
            .unwrap_or_default();
        let ie = match text_of(b, "intExt").to_uppercase().as_str() {
            "" => String::new(),
            "INT/EXT" | "I/E" | "EXT/INT" => "INT./EXT.".into(),
            x => format!("{}.", x.trim_end_matches('.')),
        };
        let mut heading = [ie.as_str(), world.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        let tod = text_of(b, "timeOfDay");
        if !tod.is_empty() {
            heading = format!("{heading} - {tod}");
        }
        if world.is_empty() {
            heading = text_of(b, "title").to_uppercase();
        }
        let (int_ext, location, time_of_day) = split_heading(&heading);

        let mut blocks = Vec::new();
        flatten(
            items(b.get("contentJson").unwrap_or(&Value::Null), "content"),
            &mut blocks,
        );
        let units = units_of(&blocks, &mut block_no);

        let mut cast: Vec<String> = Vec::new();
        for c in items(b, "chips") {
            let is_char = c
                .get("type")
                .and_then(Value::as_str)
                .is_none_or(|t| t == "character");
            if let Some(n) = names.get(id_of(c)).filter(|_| is_char) {
                let n = n.to_uppercase();
                if n.chars().any(char::is_alphabetic) && !cast.contains(&n) {
                    cast.push(n);
                }
            }
        }
        for u in &units {
            if let Some(c) = &u.character {
                if !cast.contains(c) {
                    cast.push(c.clone());
                }
            }
        }

        let summary = text_of(b, "summary");
        script.scenes.push(ParsedScene {
            heading,
            int_ext,
            location,
            time_of_day,
            number: Some(text_of(b, "sceneNumber")).filter(|n| !n.is_empty()),
            units,
            sections: Vec::new(),
            synopses: if summary.is_empty() {
                vec![]
            } else {
                vec![summary.clone()]
            },
            notes: Vec::new(),
            story: SceneStory {
                title: text_of(b, "title"),
                synopsis: summary,
                act,
                cast,
                tone: tone
                    .get(id.as_str())
                    .copied()
                    .or_else(|| {
                        b.get("fortune")
                            .and_then(Value::as_f64)
                            .map(|f| f * 2.0 - 1.0)
                    })
                    .map(round),
                beat_id: Some(id.clone()),
            },
        });
    }

    script.story = StoryContext {
        characters: items(v, "characters")
            .iter()
            .map(|c| StoryCharacter {
                name: text_of(c, "name"),
                profile: CharacterProfile {
                    summary: text_of(c, "summary"),
                    want: text_of(c, "want"),
                    need: text_of(c, "need"),
                    flaw: text_of(c, "flaw"),
                    notes: text_of(c, "notes"),
                },
            })
            .filter(|c| !c.name.is_empty())
            .collect(),
        locations: items(v, "worlds")
            .iter()
            .map(|w| StoryLocation {
                name: location_name(&text_of(w, "title")),
                description: text_of(w, "description"),
                parent: stacked_on(w)
                    .and_then(|p| worlds.get(p))
                    .map(|p| location_name(&text_of(p, "title"))),
            })
            .filter(|l| !l.name.is_empty())
            .collect(),
    };
    Ok(script)
}

/// One script block: its element type and its text (hard breaks as `\n`).
struct Block {
    kind: String,
    text: String,
}

fn inline_text(n: &Value, out: &mut String) {
    match n.get("type").and_then(Value::as_str) {
        Some("text") => {
            let t = n.get("text").and_then(Value::as_str).unwrap_or_default();
            let has = |m: &str| {
                items(n, "marks")
                    .iter()
                    .any(|x| x.get("type").and_then(Value::as_str) == Some(m))
            };
            let core = t.trim();
            if core.is_empty() {
                out.push_str(t);
                return;
            }
            let stars = match (has("bold"), has("italic")) {
                (true, true) => "***",
                (true, false) => "**",
                (false, true) => "*",
                _ => "",
            };
            let under = if has("underline") { "_" } else { "" };
            let start = t.len() - t.trim_start().len();
            out.push_str(&t[..start]);
            out.push_str(&format!("{under}{stars}{core}{stars}{under}"));
            out.push_str(&t[start + core.len()..]);
        }
        Some("hardBreak") => out.push('\n'),
        _ => {
            for c in items(n, "content") {
                inline_text(c, out);
            }
        }
    }
}

fn flatten(nodes: &[Value], out: &mut Vec<Block>) {
    for n in nodes {
        let kind = n.get("type").and_then(Value::as_str).unwrap_or_default();
        if kind == "versionBlock" {
            // the children are the active version; `attrs.versions` holds the alternates
            flatten(items(n, "content"), out);
            continue;
        }
        let kind = match kind {
            "paragraph" => n
                .pointer("/attrs/screenplayType")
                .and_then(Value::as_str)
                .unwrap_or("action"),
            k => k,
        };
        let mut text = String::new();
        inline_text(n, &mut text);
        out.push(Block {
            kind: kind.to_string(),
            text,
        });
    }
}

/// A line Throughline exports with `!` because Fountain would read it as something else.
fn needs_forcing(line: &str) -> bool {
    line.chars().any(char::is_alphabetic) && line == line.to_uppercase()
}

fn units_of(blocks: &[Block], block_no: &mut usize) -> Vec<ParsedUnit> {
    let mut out: Vec<ParsedUnit> = Vec::new();
    let mut cue: Option<(String, Option<String>)> = None;
    let mut paren: Option<String> = None;
    let mut buf: Vec<String> = Vec::new();
    let mut at = 0usize;
    let flush = |buf: &mut Vec<String>,
                 cue: &Option<(String, Option<String>)>,
                 paren: &Option<String>,
                 out: &mut Vec<ParsedUnit>,
                 at: usize| {
        if let (Some((name, ext)), false) = (cue, buf.is_empty()) {
            out.push(ParsedUnit {
                kind: LineKind::Dialogue,
                character: Some(name.clone()),
                extension: ext.clone(),
                text: buf.join(" "),
                paren: paren.clone(),
                forced: false,
                source_line: at,
            });
        }
        buf.clear();
    };
    for b in blocks {
        *block_no += 1;
        let in_speech = cue.is_some();
        match b.kind.as_str() {
            "character" => {
                flush(&mut buf, &cue, &paren, &mut out, at);
                let (name, ext) = strip_extension(b.text.trim().trim_start_matches('@'));
                cue = Some((name.to_uppercase(), ext));
                paren = None;
                at = *block_no;
            }
            "parenthetical" if in_speech => {
                flush(&mut buf, &cue, &paren, &mut out, at);
                let p = b.text.trim();
                let p = p
                    .strip_prefix('(')
                    .and_then(|p| p.strip_suffix(')'))
                    .unwrap_or(p);
                paren = Some(p.split_whitespace().collect::<Vec<_>>().join(" "));
                at = *block_no;
            }
            "dialogue" if in_speech => {
                for l in b.text.split('\n').map(str::trim).filter(|l| !l.is_empty()) {
                    if l == "\u{2026}" || l == "..." {
                        flush(&mut buf, &cue, &paren, &mut out, at);
                        paren = None;
                        at = *block_no;
                    } else if l.starts_with('(') && l.ends_with(')') && l.len() > 1 {
                        flush(&mut buf, &cue, &paren, &mut out, at);
                        paren = Some(l[1..l.len() - 1].trim().to_string());
                        at = *block_no;
                    } else {
                        if buf.is_empty() {
                            at = *block_no;
                        }
                        buf.push(l.to_string());
                    }
                }
            }
            kind => {
                flush(&mut buf, &cue, &paren, &mut out, at);
                cue = None;
                paren = None;
                if kind == "transition" {
                    let t = b.text.trim().trim_start_matches('>').trim().to_string();
                    if !t.is_empty() {
                        out.push(ParsedUnit {
                            kind: LineKind::Transition,
                            character: None,
                            extension: None,
                            forced: !t.ends_with("TO:"),
                            text: t,
                            paren: None,
                            source_line: *block_no,
                        });
                    }
                    continue;
                }
                // action (and anything unknown): a blank line starts a new unit
                for group in b.text.split("\n\n") {
                    let lines: Vec<&str> = group
                        .split('\n')
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .collect();
                    let text = lines
                        .iter()
                        .map(|l| l.trim_start_matches(['!', '>', '-']).trim())
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    if text.is_empty() {
                        continue;
                    }
                    out.push(ParsedUnit {
                        kind: LineKind::Action,
                        character: None,
                        extension: None,
                        forced: lines.first().is_some_and(|l| {
                            l.starts_with('!') || (lines.len() == 1 && needs_forcing(l))
                        }),
                        text,
                        paren: None,
                        source_line: *block_no,
                    });
                }
            }
        }
    }
    flush(&mut buf, &cue, &paren, &mut out, at);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn node(kind: &str, text: &str) -> Value {
        let mut content = Vec::new();
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                content.push(json!({"type": "hardBreak"}));
            }
            if !part.is_empty() {
                content.push(json!({"type": "text", "text": part}));
            }
        }
        json!({"type": kind, "content": content})
    }

    fn marked() -> Value {
        let under = json!([{"type": "underline"}]);
        let both = json!([{"type": "italic"}, {"type": "bold"}]);
        json!({"type": "dialogue", "content": [
            {"type": "text", "text": "Paul "},
            {"type": "text", "text": "now ", "marks": under},
            {"type": "text", "text": "!", "marks": both}
        ]})
    }

    fn sample() -> Value {
        json!({
            "version": "1.12", "minVersion": "1.6",
            "storyOrder": ["act1", "b1", "b2"],
            "project": {"title": "TEST - Episode 1", "authorName": "A & B", "creditLine": "Written by",
                        "authorPhone": "+49 000", "exportSettings": {"screenplayPdf": {"dateText": "10/30/2026"}}},
            "structures": [{"id": "act1", "title": "Act 1: Setup", "sequence": 0}],
            "characters": [
                {"id": "c1", "name": "Paul", "summary": "Soldier, lazy", "want": "to chill", "need": "", "flaw": "too trusting", "notes": ""},
                {"id": "c2", "name": "The Colonel", "summary": "", "want": "", "need": "", "flaw": "", "notes": ""},
                {"id": "c3", "name": "Stranger", "summary": "", "want": "", "need": "", "flaw": "", "notes": ""}
            ],
            "worlds": [
                {"id": "w1", "title": "TARMAC - HALLWAY", "description": "The base.", "sequence": 0},
                {"id": "w2", "title": "TARMAC \u{2013} CANTEEN", "description": "", "sequence": -1, "stackedOn": "w1", "stackOrder": 0}
            ],
            "arcs": [{"id": "a", "isDefault": true, "points": [{"beatId": "b1", "charge": -0.9200281}]}],
            "sparks": [{"id": "s", "content": "an idea"}],
            "beats": [
                {"id": "b1", "title": "Cold Open", "summary": "Paul sweats.", "intExt": "INT", "timeOfDay": "DAY",
                 "sleeveId": "w2", "stackedOn": "act1", "stackOrder": 0, "sequence": -1,
                 "chips": [{"type": "character", "id": "c1"}, {"type": "character", "id": "c3"}],
                 "contentJson": {"type": "doc", "content": [
                    node("action", "-- PAUL looks into a microscope.\n\nA bell DINGS."),
                    node("character", "SARA (O.S.)"),
                    marked(),
                    node("parenthetical", "turning"),
                    node("dialogue", "Lunch."),
                    {"type": "versionBlock", "attrs": {"versions": [{"id": "old", "content": [node("character", "PAUL"), node("dialogue", "Old line.")]}, {"id": "new"}]},
                     "content": [node("character", "THE COLONEL"), node("dialogue", "You kids.\n\nWith that machine.")]},
                    node("character", "NORBERT"),
                    node("dialogue", "Sir.\n\u{2026}\nYes, sir."),
                    node("dialogue", "Bye."),
                    {"type": "paragraph", "content": [{"type": "text", "text": "ESTABLISHING: a new day."}]},
                    node("transition", "FADE TO BLACK"),
                    node("action", "CUT TO:")
                 ]}},
                {"id": "b2", "title": "No Place", "intExt": "INT", "sequence": 1,
                 "contentJson": {"type": "doc", "content": [node("action", "Quiet.")]}}
            ]
        })
    }

    #[test]
    fn beats_become_scenes_in_story_order() {
        let s = from_value(&sample()).unwrap();
        assert_eq!(s.scenes.len(), 2);
        let s1 = &s.scenes[0];
        assert_eq!(s1.heading, "INT. TARMAC - CANTEEN - DAY");
        assert_eq!(s1.location, "TARMAC - CANTEEN");
        assert_eq!(s1.time_of_day, "DAY");
        assert_eq!(s1.story.title, "Cold Open");
        assert_eq!(s1.story.synopsis, "Paul sweats.");
        assert_eq!(s1.story.act, "Act 1: Setup");
        assert_eq!(s1.story.tone, Some(-0.92));
        assert_eq!(s1.story.beat_id.as_deref(), Some("b1"));
        assert_eq!(
            s1.story.cast,
            vec!["PAUL", "STRANGER", "SARA", "THE COLONEL", "NORBERT"]
        );
        assert_eq!(s.scenes[1].heading, "NO PLACE");
        assert_eq!(s.scenes[1].story.act, "");
        assert_eq!(
            s.title.get("Title").map(String::as_str),
            Some("TEST - Episode 1")
        );
        assert_eq!(
            s.title.get("Draft date").map(String::as_str),
            Some("10/30/2026")
        );
        assert!(!s.title.values().any(|v| v.contains("+49")));
    }

    #[test]
    fn blocks_split_like_the_fountain_export() {
        let s = from_value(&sample()).unwrap();
        let u: Vec<(LineKind, Option<&str>, Option<&str>, &str)> = s.scenes[0]
            .units
            .iter()
            .map(|u| {
                (
                    u.kind,
                    u.character.as_deref(),
                    u.paren.as_deref(),
                    u.text.as_str(),
                )
            })
            .collect();
        use LineKind::*;
        assert_eq!(
            u,
            vec![
                (Action, None, None, "PAUL looks into a microscope."),
                (Action, None, None, "A bell DINGS."),
                (Dialogue, Some("SARA"), None, "Paul _now_ ***!***"),
                (Dialogue, Some("SARA"), Some("turning"), "Lunch."),
                (
                    Dialogue,
                    Some("THE COLONEL"),
                    None,
                    "You kids. With that machine."
                ),
                (Dialogue, Some("NORBERT"), None, "Sir."),
                (Dialogue, Some("NORBERT"), None, "Yes, sir. Bye."),
                (Action, None, None, "ESTABLISHING: a new day."),
                (Transition, None, None, "FADE TO BLACK"),
                (Action, None, None, "CUT TO:"),
            ]
        );
        assert_eq!(s.scenes[0].units[2].extension.as_deref(), Some("O.S."));
        assert!(s.scenes[0].units[8].forced);
        assert!(s.scenes[0].units[9].forced);
    }

    #[test]
    fn story_context_without_sparks() {
        let s = from_value(&sample()).unwrap();
        assert_eq!(s.story.characters.len(), 3);
        assert_eq!(s.story.characters[0].profile.want, "to chill");
        assert_eq!(s.story.locations[1].name, "TARMAC - CANTEEN");
        assert_eq!(
            s.story.locations[1].parent.as_deref(),
            Some("TARMAC - HALLWAY")
        );
        assert_eq!(s.story.locations[0].description, "The base.");
        assert!(!serde_json::to_string(&s).unwrap().contains("an idea"));
    }

    #[test]
    fn gzip_and_plain_json_and_detection() {
        let raw = serde_json::to_vec(&sample()).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&raw).unwrap();
        let gz = gz.finish().unwrap();
        assert!(looks_like_thrl(&gz) && looks_like_thrl(&raw));
        assert!(!looks_like_thrl(b"Title: X\n\nINT. LAB - DAY\n"));
        assert_eq!(parse(&gz).unwrap(), parse(&raw).unwrap());
    }

    #[test]
    fn newer_formats_and_non_projects_are_refused() {
        let mut v = sample();
        v["minVersion"] = json!("2.0");
        assert!(from_value(&v).unwrap_err().to_string().contains("2.0"));
        assert!(from_value(&json!({"a": 1})).is_err());
        v["minVersion"] = json!("1.12");
        assert!(from_value(&v).is_ok());
    }

    #[test]
    fn story_order_without_the_precomputed_list() {
        let mut v = sample();
        v.as_object_mut().unwrap().remove("storyOrder");
        v["beats"][1]["sequence"] = json!(-5); // b2 top-level, before the act card
        let s = from_value(&v).unwrap();
        let ids: Vec<_> = s
            .scenes
            .iter()
            .map(|s| s.story.beat_id.clone().unwrap())
            .collect();
        assert_eq!(ids, vec!["b2", "b1"]);
    }
}
