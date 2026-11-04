//! Where an actor said a thing or made a sound: the bucket's character search looks through the
//! words heard on their own mic and hands back stretches cut to the speech (D101). Nothing here
//! is stored: a search reads the transcripts and envelopes as they are.

use apc_core::text::norm_word;

use crate::asr::TWord;
use crate::edges::refine;
use crate::fuzzy::ratio;
use crate::vocals::{collapse, db, interjection_tag, level};

/// This much quiet on both sides, in seconds, and a sound stands alone.
const ALONE_S: f64 = 0.3;
/// How close a misheard word has to be, on a word of at least `CLOSE_MIN_LEN` letters.
const CLOSE_RATIO: f64 = 80.0;
const CLOSE_MIN_LEN: usize = 4;
/// Louder than this in another actor's mic of the session, it's that actor's voice.
const THEIRS_DB: f64 = 3.0;
/// Further than this under the mic's typical speech, it's someone else in the room. Wider than
/// the analysis's bleed margin: a grunt is allowed to be quieter than a line.
const QUIET_DB: f64 = 18.0;
/// Words of the same breath shown around what was found.
const CONTEXT_WORDS: usize = 4;
/// Further apart than this, in seconds, the neighbouring word isn't context.
const CONTEXT_GAP_S: f64 = 1.5;

/// Grunts the recogniser mixes up: asking for one finds the others, when they stand alone.
const ALIKE: &[&[&str]] = &[
    &["huh", "hu", "uh", "hm", "eh"],
    &["hm", "mhm", "mh", "m"],
    &["uh", "um", "er", "ah"],
    &["oh", "ah", "aw", "ooh"],
];

/// One actor mic. `words` are in this file's seconds, as cached transcripts are.
pub struct Mic<'a> {
    /// Session time = file time − offset_s.
    pub offset_s: f64,
    /// RMS per 10 ms frame (mono).
    pub env: &'a [f32],
    pub words: &'a [TWord],
    /// The mic's typical take level, when it has takes.
    pub speech_db: Option<f64>,
    /// Envelope level edges snap to, as for takes.
    pub thr: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// File seconds, cut to the speech.
    pub t0: f64,
    pub t1: f64,
    /// What was heard, as heard.
    pub text: String,
    pub before: String,
    pub after: String,
    /// Heard differently from what was asked ("hm" for "huh").
    pub close: bool,
    /// Quiet on both sides, so it cuts out clean.
    pub alone: bool,
}

/// What kind of sound a searched word asks for, among the sounds the analysis names: the
/// `VocalCandidate` tags.
pub fn sound_kind(word: &str) -> Option<&'static str> {
    Some(match norm_word(word).as_str() {
        "grunt" | "grunts" | "effort" | "efforts" | "strain" => "effort",
        "breath" | "breaths" | "breathe" | "breathing" | "inhale" | "exhale" | "gasp" => "breath",
        "pant" | "panting" | "pants" => "panting",
        "run" | "running" | "steps" | "footsteps" => "running",
        "laugh" | "laughs" | "laughing" | "chuckle" | "giggle" => "laugh",
        "sigh" | "sighs" | "sighing" => "sigh",
        _ => return None,
    })
}

fn grunt(w: &str) -> bool {
    interjection_tag(w).is_some() || ALIKE.iter().any(|g| g.contains(&w))
}

#[derive(PartialEq, Clone, Copy)]
enum Match {
    Exact,
    /// Spelled nearly the same.
    Close,
    /// Another sound of the same kind: only worth showing when it stands alone.
    Alike,
}

/// How a heard word answers a searched one. Both normalized.
fn compare(asked: &str, heard: &TWord) -> Option<Match> {
    let w = heard.w.as_str();
    if asked == w {
        return Some(Match::Exact);
    }
    if grunt(asked) {
        if collapse(asked) == collapse(w) {
            return Some(Match::Exact);
        }
        let kind = sound_kind(asked).or_else(|| interjection_tag(asked));
        let same_kind = kind.is_some() && kind == interjection_tag(&heard.raw);
        let family = ALIKE.iter().any(|g| {
            g.first() == Some(&collapse(asked).as_str()) && g.contains(&collapse(w).as_str())
        });
        return (same_kind || family).then_some(Match::Alike);
    }
    if sound_kind(asked).is_some_and(|k| interjection_tag(&heard.raw) == Some(k)) {
        return Some(Match::Alike);
    }
    (asked.len() >= CLOSE_MIN_LEN && ratio(asked, w) >= CLOSE_RATIO).then_some(Match::Close)
}

fn joined(words: &[TWord]) -> String {
    let raw: Vec<&str> = words.iter().map(|w| w.raw.trim()).collect();
    raw.join(" ")
}

/// Is this the voice of someone else? Louder in another actor's mic, or far under this one's.
fn not_theirs(mic: &Mic, others: &[Mic], t0: f64, t1: f64) -> bool {
    let here = db(level(mic.env, t0, t1));
    let quiet = mic.speech_db.is_some_and(|s| here < s - QUIET_DB);
    quiet
        || others.iter().any(|o| {
            let d = o.offset_s - mic.offset_s; // other file time = this file time + d
            db(level(o.env, t0 + d, t1 + d)) > here + THEIRS_DB
        })
}

/// Every place `asked` (normalized, in order) was heard on `mic`. `others` are the session's
/// other actor mics, to leave out what only bled in.
pub fn find_words(mic: &Mic, others: &[Mic], asked: &[String]) -> Vec<Found> {
    let n = asked.len();
    if n == 0 || mic.words.len() < n {
        return vec![];
    }
    let mut out = Vec::new();
    for i in 0..=mic.words.len() - n {
        let span = &mic.words[i..i + n];
        let matches: Option<Vec<Match>> =
            asked.iter().zip(span).map(|(a, w)| compare(a, w)).collect();
        let Some(matches) = matches else {
            continue;
        };
        let (w0, w1) = (span[0].t0, span[n - 1].t1);
        let prev = i.checked_sub(1).map(|k| &mic.words[k]);
        let next = mic.words.get(i + n);
        let alone =
            prev.is_none_or(|p| w0 - p.t1 >= ALONE_S) && next.is_none_or(|x| x.t0 - w1 >= ALONE_S);
        if matches.contains(&Match::Alike) && !alone {
            continue; // an "uh" inside a sentence is no "huh"
        }
        if not_theirs(mic, others, w0, w1) {
            continue;
        }
        let (a, b) = refine(mic.env, w0, w1, mic.thr);
        // the snapped edges stop at the words around it
        let t0 = prev.map_or(a, |p| a.max(p.t1.min(w0)));
        let t1 = next.map_or(b, |x| b.min(x.t0.max(w1)));
        let near = |w: &&TWord, to: f64| (w.t0 - to).abs() < CONTEXT_GAP_S;
        let before: Vec<TWord> = mic.words[i.saturating_sub(CONTEXT_WORDS)..i]
            .iter()
            .filter(|w| near(w, w0))
            .cloned()
            .collect();
        let after: Vec<TWord> = mic.words[i + n..(i + n + CONTEXT_WORDS).min(mic.words.len())]
            .iter()
            .filter(|w| near(w, w1))
            .cloned()
            .collect();
        out.push(Found {
            t0,
            t1,
            text: joined(span),
            before: joined(&before),
            after: joined(&after),
            close: matches.iter().any(|m| *m != Match::Exact),
            alone,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(raw: &str, t0: f64, t1: f64) -> TWord {
        TWord {
            w: norm_word(raw),
            raw: raw.into(),
            t0,
            t1,
            seg: 0,
        }
    }

    /// Speech at the level given over each stretch, on a quiet floor.
    fn env(len_s: f64, speech: &[(f64, f64, f32)]) -> Vec<f32> {
        let mut e = vec![0.001f32; (len_s * 100.0) as usize];
        for &(a, b, v) in speech {
            for x in &mut e[(a * 100.0) as usize..(b * 100.0) as usize] {
                *x = v;
            }
        }
        e
    }

    fn mic<'a>(env: &'a [f32], words: &'a [TWord]) -> Mic<'a> {
        Mic {
            offset_s: 0.0,
            env,
            words,
            speech_db: None,
            thr: 0.01,
        }
    }

    fn asked(q: &str) -> Vec<String> {
        q.split_whitespace().map(norm_word).collect()
    }

    #[test]
    fn a_grunt_is_cut_to_its_sound() {
        let words = [word("Huh?", 2.0, 2.3)];
        let e = env(5.0, &[(2.05, 2.35, 0.2)]);
        let f = find_words(&mic(&e, &words), &[], &asked("huh"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].text, "Huh?");
        assert!(!f[0].close && f[0].alone);
        assert!((f[0].t0 - 1.97).abs() < 0.011, "{}", f[0].t0);
        assert!((f[0].t1 - 2.5).abs() < 0.011, "{}", f[0].t1);
    }

    #[test]
    fn what_the_recogniser_heard_instead_counts_only_alone() {
        let words = [
            word("Hm.", 1.0, 1.3),
            word("I", 4.0, 4.1),
            word("uh", 4.15, 4.3),
            word("think", 4.35, 4.6),
        ];
        let e = env(6.0, &[(1.0, 1.3, 0.2), (4.0, 4.6, 0.2)]);
        let f = find_words(&mic(&e, &words), &[], &asked("huh"));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].text, "Hm.");
        assert!(f[0].close);
    }

    #[test]
    fn an_exact_word_inside_a_sentence_stays_within_its_neighbours() {
        let words = [
            word("Well", 1.0, 1.3),
            word("huh", 1.35, 1.6),
            word("okay", 1.65, 2.0),
        ];
        let e = env(4.0, &[(1.0, 2.0, 0.2)]);
        let f = find_words(&mic(&e, &words), &[], &asked("huh"));
        assert_eq!(f.len(), 1);
        assert!(!f[0].alone);
        assert!(f[0].t0 >= 1.3 && f[0].t1 <= 1.65, "{f:?}");
        assert_eq!(
            (f[0].before.as_str(), f[0].after.as_str()),
            ("Well", "okay")
        );
    }

    #[test]
    fn a_phrase_is_found_in_order() {
        let words = [
            word("Does", 1.0, 1.2),
            word("it", 1.25, 1.35),
            word("make", 1.4, 1.6),
            word("coffee?", 1.65, 2.0),
            word("it", 3.0, 3.1),
        ];
        let e = env(4.0, &[(1.0, 2.0, 0.2), (3.0, 3.1, 0.2)]);
        let f = find_words(&mic(&e, &words), &[], &asked("make coffee"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].text, "make coffee?");
        assert!(find_words(&mic(&e, &words), &[], &asked("coffee make")).is_empty());
    }

    #[test]
    fn a_misheard_word_is_close() {
        let words = [word("sandwitches", 1.0, 1.6)];
        let e = env(3.0, &[(1.0, 1.6, 0.2)]);
        let f = find_words(&mic(&e, &words), &[], &asked("sandwiches"));
        assert_eq!(f.len(), 1);
        assert!(f[0].close);
    }

    #[test]
    fn another_actors_voice_is_left_out() {
        let words = [word("Huh?", 1.0, 1.3)];
        let bled = env(3.0, &[(1.0, 1.3, 0.02)]);
        let theirs = env(3.0, &[(1.0, 1.3, 0.2)]);
        let other = Mic {
            words: &[],
            ..mic(&theirs, &[])
        };
        assert!(find_words(&mic(&bled, &words), &[other], &asked("huh")).is_empty());
    }

    #[test]
    fn someone_far_under_the_mics_speech_is_left_out() {
        let words = [word("Huh?", 1.0, 1.3)];
        let e = env(3.0, &[(1.0, 1.3, 0.005)]);
        let m = Mic {
            speech_db: Some(-14.0),
            ..mic(&e, &words)
        };
        assert!(find_words(&m, &[], &asked("huh")).is_empty());
    }

    #[test]
    fn a_grunt_is_asked_for_by_its_kind() {
        assert_eq!(sound_kind("Grunt"), Some("effort"));
        assert_eq!(sound_kind("breathing"), Some("breath"));
        assert_eq!(sound_kind("coffee"), None);
        let words = [word("Ugh!", 1.0, 1.3)];
        let e = env(3.0, &[(1.0, 1.3, 0.2)]);
        let f = find_words(&mic(&e, &words), &[], &asked("grunt"));
        assert_eq!(f.len(), 1);
        assert!(f[0].close);
    }
}
