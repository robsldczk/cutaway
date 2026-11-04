//! Matches library sounds to what a bucket wants by file name alone (D74): no AI and no
//! audio. Epidemic names read `ES_Category, Sub, Detail - Epidemic Sound.wav`, so the words
//! of a name say most of what a cue's tags ask for. Scored per query (one cue's tags, or one
//! keyword), so a wide bucket like "Props" only gets what one of its cues asks for.

use std::collections::BTreeSet;

/// A sound below this doesn't match. One specific tag is enough, a weak word alone never is.
pub const MIN_SCORE: u32 = 3;
/// Sounds one query adds at most, the best first.
pub const PER_QUERY: usize = 5;

/// Says nothing about what a sound is: naming habits and how a sound is performed.
const NOISE: &[&str] = &[
    "and",
    "background",
    "big",
    "close",
    "comedic",
    "designed",
    "distant",
    "down",
    "dry",
    "epidemic",
    "exterior",
    "faint",
    "fast",
    "filtered",
    "from",
    "general",
    "group",
    "hard",
    "heavy",
    "high",
    "interior",
    "large",
    "light",
    "long",
    "loud",
    "low",
    "medium",
    "misc",
    "muffled",
    "new",
    "old",
    "one",
    "open",
    "perspective",
    "quick",
    "quiet",
    "sharp",
    "short",
    "slow",
    "small",
    "soft",
    "sound",
    "source",
    "subtle",
    "take",
    "tension",
    "the",
    "three",
    "two",
    "various",
    "wet",
    "with",
];

/// Materials and wide categories: they back up a match but don't make one ("metal" alone
/// would give a telescope every metal impact).
const WEAK: &[&str] = &[
    "animal", "drink", "floor", "food", "glass", "hand", "human", "impact", "metal", "object",
    "plastic", "room", "run", "skin", "table", "wood",
];

/// A word's plain form: `doors` → `door`, `running` → `run`, `batteries` → `battery`.
fn stem(w: &str) -> String {
    if w.len() > 4 && w.ends_with("ing") {
        let b = &w[..w.len() - 3];
        let mut end = b.chars().rev();
        return match (end.next(), end.next()) {
            (Some(x), Some(y)) if x == y && b.len() > 2 => b[..b.len() - 1].to_string(),
            _ => b.to_string(),
        };
    }
    if w.len() > 3 && w.ends_with("ies") {
        return format!("{}y", &w[..w.len() - 3]);
    }
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
        return w[..w.len() - 1].to_string();
    }
    w.to_string()
}

/// The words of a text that can carry a match.
fn signal(text: &str) -> Vec<String> {
    let low = text
        .to_lowercase()
        .replace("sci-fi", "scifi")
        .replace("sci fi", "scifi");
    low.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !w.chars().all(|c| c.is_ascii_digit()))
        .map(stem)
        .filter(|w| !NOISE.contains(&w.as_str()))
        .collect()
}

/// What a library file's path says about it.
#[derive(Clone, Debug, Default)]
pub struct SoundWords {
    words: BTreeSet<String>,
    /// The name's first part (Epidemic's category) and the folder: what the sound *is*, as
    /// against what it mentions.
    lead: BTreeSet<String>,
}

impl SoundWords {
    /// From a path under `Sound/` (`sfx/doors/ES_Doors, Wood, Slam - Epidemic Sound.wav`).
    pub fn new(rel: &str) -> SoundWords {
        let parts: Vec<&str> = rel.split('/').collect();
        let file = parts.last().copied().unwrap_or(rel);
        let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
        let name = stem.strip_prefix("ES_").unwrap_or(stem);
        let name = name.strip_suffix(" - Epidemic Sound").unwrap_or(name);
        let mut lead: BTreeSet<String> = signal(name.split(',').next().unwrap_or(name))
            .into_iter()
            .collect();
        let mut words: BTreeSet<String> = signal(name).into_iter().collect();
        if parts.len() > 2 {
            for w in signal(parts[1]) {
                lead.insert(w.clone());
                words.insert(w);
            }
        }
        SoundWords { words, lead }
    }

    /// Does the name or folder hold this word (in its plain form)?
    pub fn has(&self, word: &str) -> bool {
        self.words.contains(&stem(&word.to_lowercase()))
    }
}

/// How well a sound answers a query: tags in order of importance, the first being what the
/// cue is mostly about. A tag of several words needs them all and counts for more.
pub fn score(tags: &[String], s: &SoundWords) -> u32 {
    let mut total = 0;
    for (i, tag) in tags.iter().enumerate() {
        let need = signal(tag);
        if need.is_empty() || !need.iter().all(|w| s.words.contains(w)) {
            continue;
        }
        if need.len() == 1 && WEAK.contains(&need[0].as_str()) {
            total += 1;
            continue;
        }
        total += if i == 0 { 3 } else { 2 };
        total += if need.len() > 1 { 2 } else { 0 };
        total += u32::from(need.iter().any(|w| s.lead.contains(w)));
    }
    total
}

/// The sounds that answer a query, the best first, at most [`PER_QUERY`].
pub fn best<'a, T>(tags: &[String], sounds: &[(&'a T, &SoundWords)]) -> Vec<&'a T> {
    let mut hits: Vec<(u32, &'a T)> = sounds
        .iter()
        .map(|(t, w)| (score(tags, w), *t))
        .filter(|(n, _)| *n >= MIN_SCORE)
        .collect();
    hits.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    hits.into_iter().take(PER_QUERY).map(|(_, t)| t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    fn hit(t: &[&str], rel: &str) -> bool {
        score(&tags(t), &SoundWords::new(rel)) >= MIN_SCORE
    }

    #[test]
    fn plain_forms() {
        assert_eq!(stem("doors"), "door");
        assert_eq!(stem("running"), "run");
        assert_eq!(stem("ringing"), "ring");
        assert_eq!(stem("ring"), "ring");
        assert_eq!(stem("batteries"), "battery");
        assert_eq!(stem("glass"), "glass");
    }

    #[test]
    fn a_cue_finds_loose_and_foldered_sounds_by_name() {
        let t = ["door", "screen door", "boots"];
        assert!(hit(
            &t,
            "sfx/ES_Doors, Wood, Slam, Squeak - Epidemic Sound.wav"
        ));
        assert!(hit(
            &t,
            "sfx/kitchen/ES_Doors, Cabinet, Close - Epidemic Sound.wav"
        ));
        let t = ["coffee machine", "pour"];
        assert!(hit(
            &t,
            "sfx/ES_Machines, Appliance, Coffee Maker - Epidemic Sound.wav"
        ));
        assert!(
            !hit(
                &t,
                "sfx/kitchen/ES_Glassware, Coffee Cup, Put Down - Epidemic Sound.wav"
            ),
            "a tag of two words needs both"
        );
    }

    #[test]
    fn the_folder_counts_as_what_a_sound_is() {
        assert!(hit(
            &["ufo", "whoosh"],
            "sfx/ufo/ES_Metal, Impact, Huge Reverb - Epidemic Sound.wav"
        ));
        assert!(hit(
            &["time spring", "charge"],
            "sfx/time-spring/ES_Water, Bubbles, Giant - Epidemic Sound.wav"
        ));
    }

    #[test]
    fn weak_words_and_noise_never_match_alone() {
        let barrel = "sfx/ES_Metal, Tonal, Impact, Barrel, Hit - Epidemic Sound.wav";
        assert!(!hit(&["telescope", "metal", "adjust"], barrel));
        assert!(hit(&["barrels", "metal", "clunk"], barrel));
        assert!(!hit(
            &["distant", "heavy"],
            "sfx/ES_Ambience, Rural, Distant Plane, Heavy - Epidemic Sound.wav"
        ));
    }

    #[test]
    fn sci_fi_is_one_word_however_it_is_written() {
        assert!(hit(
            &["sci-fi", "hum"],
            "sfx/ES_Scifi, Spaceship, Interior - Epidemic Sound.wav"
        ));
    }

    #[test]
    fn best_keeps_the_strongest_few() {
        let names: Vec<String> = (0..8)
            .map(|i| format!("sfx/ES_Doors, Wood, Take {i} - Epidemic Sound.wav"))
            .chain(["sfx/ES_Doors, Screen, Bang - Epidemic Sound.wav".to_string()])
            .collect();
        let words: Vec<SoundWords> = names.iter().map(|n| SoundWords::new(n)).collect();
        let list: Vec<(&String, &SoundWords)> = names.iter().zip(words.iter()).collect();
        let got = best(&tags(&["door", "bang"]), &list);
        assert_eq!(got.len(), PER_QUERY);
        assert!(got[0].contains("Bang"), "the closest match leads");
    }
}
