//! Role assignment by file name: character names and aliases, room keywords, then a
//! content-based fallback (which character's lines the transcript matches).

use std::collections::BTreeMap;

use serde::Serialize;

use apc_core::Role;

use crate::fuzzy::ratio;

pub const ROOM_KEYWORDS: &[&str] = &["room", "director", "regie", "talkback", "tb"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoleGuess {
    pub file: String,
    pub role: Role,
    pub character: Option<String>,
    pub confidence: f64,
    pub reason: String,
}

/// Lower-case name tokens of a file (camel case split, extension dropped).
pub fn tokens(name: &str) -> Vec<String> {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    // split camel case too: "PaulRoom" -> "Paul Room"
    let mut spaced = String::new();
    let mut prev_lower = false;
    for c in stem.chars() {
        if c.is_uppercase() && prev_lower {
            spaced.push(' ');
        }
        prev_lower = c.is_lowercase();
        spaced.push(c);
    }
    spaced
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

/// The character a file name points to (name, part of the name, a close spelling or an
/// alias), with confidence and reason. Room keywords are ignored, so "Sara-Director_v001"
/// names Sara.
pub fn character_in_name(
    file: &str,
    characters: &[String],
    aliases: &BTreeMap<String, String>,
) -> Option<(String, f64, String)> {
    let toks = tokens(file);
    let mut best: Option<(String, f64, String)> = None;
    let mut consider = |ch: &str, conf: f64, why: String| {
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((ch.to_string(), conf, why));
        }
    };
    for ch in characters {
        let name = ch.to_lowercase();
        let compact: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
        let parts: Vec<&str> = name
            .split_whitespace()
            .filter(|w| *w != "the" && w.len() > 1)
            .collect();
        for t in &toks {
            if *t == compact {
                consider(ch, 0.95, format!("name '{t}'"));
            } else if parts.contains(&t.as_str()) {
                consider(ch, 0.85, format!("part of the name '{t}'"));
            } else if t.len() >= 4 && ratio(t, &compact) >= 85.0 {
                consider(ch, 0.7, format!("close to the name '{t}'"));
            }
        }
    }
    for (alias, ch) in aliases {
        let a = alias.to_lowercase();
        if toks.contains(&a) {
            consider(ch, 0.9, format!("alias '{alias}'"));
        }
    }
    best
}

/// Guesses the role of one file from its name.
pub fn guess_by_name(
    file: &str,
    characters: &[String],
    aliases: &BTreeMap<String, String>,
) -> RoleGuess {
    let toks = tokens(file);
    if let Some(k) = toks.iter().find(|t| ROOM_KEYWORDS.contains(&t.as_str())) {
        return RoleGuess {
            file: file.into(),
            role: Role::Room,
            character: None,
            confidence: 0.95,
            reason: format!("room keyword '{k}'"),
        };
    }
    let best = character_in_name(file, characters, aliases);
    match best {
        Some((ch, conf, why)) => RoleGuess {
            file: file.into(),
            role: Role::Character,
            character: Some(ch),
            confidence: conf,
            reason: why,
        },
        None => RoleGuess {
            file: file.into(),
            role: Role::Unknown,
            character: None,
            confidence: 0.0,
            reason: "no name or keyword in the file name".into(),
        },
    }
}

/// Content fallback: the character whose lines a transcript matches most (by words).
pub fn guess_by_content(hit_words: &BTreeMap<String, usize>) -> Option<(String, f64)> {
    let total: usize = hit_words.values().sum();
    let (ch, n) = hit_words.iter().max_by_key(|(_, n)| **n)?;
    (total > 0).then(|| (ch.clone(), *n as f64 / total as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars() -> Vec<String> {
        ["PAUL", "SARA", "NORBERT", "THE COLONEL"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn fixture_names() {
        let a = BTreeMap::new();
        let g = guess_by_name("Paul-9min_v001_.mp3", &chars(), &a);
        assert_eq!(
            (g.role, g.character.as_deref()),
            (Role::Character, Some("PAUL"))
        );
        let g = guess_by_name("Paul-Room-9min_v001_.mp3", &chars(), &a);
        assert_eq!(g.role, Role::Room);
        let g = guess_by_name("Colonel_take2.wav", &chars(), &a);
        assert_eq!(g.character.as_deref(), Some("THE COLONEL"));
        let g = guess_by_name("Take_03.wav", &chars(), &a);
        assert_eq!(g.role, Role::Unknown);
        let mut al = BTreeMap::new();
        al.insert("Col".to_string(), "THE COLONEL".to_string());
        let g = guess_by_name("Col-01.wav", &chars(), &al);
        assert_eq!(g.character.as_deref(), Some("THE COLONEL"));
        let g = guess_by_name("NorbertTB.wav", &chars(), &a);
        assert_eq!(g.role, Role::Room, "talkback keyword wins");
    }

    #[test]
    fn content_fallback() {
        let mut m = BTreeMap::new();
        m.insert("PAUL".to_string(), 120);
        m.insert("SARA".to_string(), 30);
        let (c, conf) = guess_by_content(&m).unwrap();
        assert_eq!(c, "PAUL");
        assert!((conf - 0.8).abs() < 1e-9);
    }
}
