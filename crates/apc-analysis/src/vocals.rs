//! Character vocals: the small sounds an actor makes besides the scripted lines ("mh-hm",
//! "ah", laughs, efforts, breaths, panting, running steps), found in the actor's own mic
//! during the analysis. They fill the character's vocal bucket; nothing is generated or
//! searched for.
//!
//! Two sources:
//! - **Words** the recogniser heard that are interjections, outside any take (or the takes
//!   of a line that is only an interjection, like "Mh-hm…").
//! - **Sounds without words** between takes: short bursts above the noise floor that are
//!   louder in this mic than in the other actors' mics (not bleed), told apart by length,
//!   level and rhythm.

use apc_core::text::norm_word;
use apc_media::ENV_FPS;
use serde::Serialize;

/// One actor mic, prepared by the pipeline. Times are in this file's seconds.
pub struct VocalTrack<'a> {
    pub source_id: String,
    pub character: String,
    /// Session time = file time − offset_s.
    pub offset_s: f64,
    pub session: Option<String>,
    /// RMS per 10 ms frame (mono).
    pub env: &'a [f32],
    /// Recognised words (raw text, start, end).
    pub words: Vec<(String, f64, f64)>,
    /// Takes on this file (start, end, the line is only interjections).
    pub takes: Vec<(f64, f64, bool)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VocalCandidate {
    pub source_id: String,
    pub character: String,
    /// mhm, ah, whoa, laugh, effort, huh, sigh, breath, panting, running
    pub tag: String,
    /// File seconds, with a little air around the sound.
    pub t0: f64,
    pub t1: f64,
    /// What was heard ("mh-hm"), empty for sounds without words.
    pub text: String,
    /// Higher is clearer (level over the noise floor, dB).
    pub score: f64,
}

/// The vocal tag of a word, if it is an interjection ("Mh-hm" → mhm, "Whoooaaah" → whoa).
pub fn interjection_tag(word: &str) -> Option<&'static str> {
    let w = collapse(&norm_word(word));
    // real words that only count when stretched: "of" is not "oof", "he" is not "hehe"
    let letters: String = word
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    if AMBIGUOUS.contains(&w.as_str()) && letters == w {
        return None;
    }
    let tag = match w.as_str() {
        "mhm" | "mh" | "mhmh" | "hm" | "m" | "mhmhm" | "uhuh" | "uhu" | "mhum" => "mhm",
        "ah" | "aha" | "oh" | "o" | "ohoh" | "ahah" => "ah",
        "whoa" | "woah" | "whoah" | "wow" | "wo" | "whoo" | "wohoo" | "wohou" => "whoa",
        "ha" | "haha" | "hahaha" | "he" | "hehe" | "heh" | "hah" | "hihi" => "laugh",
        "ugh" | "argh" | "arg" | "ow" | "ouch" | "of" | "hngh" | "ngh" | "gah" | "uf" => "effort",
        "huh" | "eh" | "hu" => "huh",
        "phew" | "pf" | "psh" | "tsk" | "sigh" => "sigh",
        _ => return None,
    };
    Some(tag)
}

/// Collapsed forms that are also ordinary words.
const AMBIGUOUS: &[&str] = &["of", "he", "o", "hu", "wo", "uf", "arg", "eh"];

/// Repeated letters once: "whoooaaah" → "whoah", "mmhmm" → "mhm", "oof" → "of".
pub(crate) fn collapse(w: &str) -> String {
    let mut out = String::new();
    for c in w.chars() {
        if !out.ends_with(c) {
            out.push(c);
        }
    }
    out
}

/// How many of each tag one character keeps (the clearest ones).
fn cap(tag: &str) -> usize {
    match tag {
        "panting" | "running" => 4,
        "breath" => 6,
        _ => 8,
    }
}

const PAD_BEFORE: f64 = 0.08;
const PAD_AFTER: f64 = 0.15;

pub(crate) fn db(x: f32) -> f64 {
    20.0 * (x.max(1e-6) as f64).log10()
}

pub(crate) fn level(env: &[f32], t0: f64, t1: f64) -> f32 {
    let fps = ENV_FPS as f64;
    let a = ((t0 * fps).max(0.0) as usize).min(env.len());
    let b = ((t1 * fps).ceil().max(0.0) as usize).min(env.len());
    env[a..b].iter().copied().fold(0.0, f32::max)
}

/// Bursts above the floor: (start, end) in seconds, short gaps merged.
fn bursts(env: &[f32], floor: f32) -> Vec<(f64, f64)> {
    let fps = ENV_FPS as f64;
    let on = floor * 3.16; // +10 dB
    let stay = floor * 1.78; // +5 dB
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &v) in env.iter().enumerate() {
        match start {
            None if v >= on => start = Some(i),
            Some(s) if v < stay => {
                out.push((s as f64 / fps, i as f64 / fps));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s as f64 / fps, env.len() as f64 / fps));
    }
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for b in out {
        match merged.last_mut() {
            Some(m) if b.0 - m.1 < 0.06 => m.1 = b.1,
            _ => merged.push(b),
        }
    }
    merged
}

/// Finds every character's vocal sounds. `tracks` are the actor mics of the episode; mics
/// in the same session are compared so another actor's bleed isn't taken for this one.
pub fn find_vocals(tracks: &[VocalTrack]) -> Vec<VocalCandidate> {
    vocals_up_to(tracks, cap)
}

/// All of them, for a search: the bucket's caps keep it short, a search wants what's there.
pub fn every_vocal(tracks: &[VocalTrack]) -> Vec<VocalCandidate> {
    vocals_up_to(tracks, |_| usize::MAX)
}

fn vocals_up_to(tracks: &[VocalTrack], cap: impl Fn(&str) -> usize) -> Vec<VocalCandidate> {
    let mut all: Vec<VocalCandidate> = Vec::new();
    for (ti, tr) in tracks.iter().enumerate() {
        let near = |t0: f64, t1: f64, pad: f64| {
            tr.takes
                .iter()
                .any(|&(a, b, _)| t0 < b + pad && a - pad < t1)
        };
        let mut found: Vec<VocalCandidate> = Vec::new();

        // 1. interjections the recogniser heard
        for (raw, t0, t1) in &tr.words {
            let Some(tag) = interjection_tag(raw) else {
                continue;
            };
            let in_take = tr.takes.iter().find(|&&(a, b, _)| *t0 < b && a < *t1);
            if let Some(&(_, _, only_interjection)) = in_take {
                if !only_interjection {
                    continue; // part of a scripted line
                }
            }
            // "Mh-hm" arrives as "Mh" + "hm": back-to-back interjections are one sound
            // (a word ending in punctuation, like "Mm.", is a sound of its own)
            if let Some(prev) = found.last_mut() {
                let joined = !prev.text.ends_with(['.', ',', '!', '?', '…', ';', ':']);
                if joined && prev.tag == tag && *t0 - (prev.t1 - PAD_AFTER) < 0.25 {
                    prev.t1 = t1 + PAD_AFTER;
                    prev.text = format!(
                        "{}-{}",
                        prev.text.trim_end_matches(['.', ',', '!', '?']),
                        raw.trim()
                    );
                    continue;
                }
            }
            found.push(VocalCandidate {
                source_id: tr.source_id.clone(),
                character: tr.character.clone(),
                tag: tag.into(),
                t0: (t0 - PAD_BEFORE).max(0.0),
                t1: t1 + PAD_AFTER,
                text: raw.trim().to_string(),
                score: 0.0,
            });
        }

        // 2. sounds without words between takes
        let mut sorted: Vec<f32> = tr.env.iter().copied().filter(|v| *v > 0.0).collect();
        if sorted.len() < 50 {
            all.extend(found);
            continue;
        }
        sorted.sort_by(|a, b| a.total_cmp(b));
        let floor = sorted[sorted.len() / 10].max(1e-5);
        let speech: Vec<f32> = tr
            .takes
            .iter()
            .map(|&(a, b, _)| level(tr.env, a, b))
            .collect();
        let speech_db = if speech.is_empty() {
            db(sorted[sorted.len() * 95 / 100])
        } else {
            let mut s = speech.clone();
            s.sort_by(|a, b| a.total_cmp(b));
            db(s[s.len() / 2])
        };
        let word_near = |t0: f64, t1: f64| {
            tr.words
                .iter()
                .any(|(_, a, b)| t0 < b + 0.1 && a - 0.1 < t1)
        };
        // the same moment in the other actors' mics of this session
        let louder_here = |t0: f64, t1: f64| {
            let here = db(level(tr.env, t0, t1));
            tracks.iter().enumerate().all(|(oi, o)| {
                if oi == ti || o.session.is_none() || o.session != tr.session {
                    return true;
                }
                let d = o.offset_s - tr.offset_s; // other file time = this file time + d
                here >= db(level(o.env, t0 + d, t1 + d)) + 6.0
            })
        };
        let events: Vec<(f64, f64, f64)> = bursts(tr.env, floor)
            .into_iter()
            .filter(|&(a, b)| {
                b - a >= 0.03
                    && b - a <= 3.0
                    && !near(a, b, 0.25)
                    && !word_near(a, b)
                    && louder_here(a, b)
            })
            .map(|(a, b)| (a, b, db(level(tr.env, a, b)) - db(floor)))
            .collect();

        // rhythmic runs first: steps (short clicks) or panting (breaths) at a steady pace
        let mut used = vec![false; events.len()];
        let mut i = 0;
        while i < events.len() {
            let short = |e: &(f64, f64, f64)| e.1 - e.0 <= 0.15;
            let breathy = |e: &(f64, f64, f64)| (0.15..=1.0).contains(&(e.1 - e.0));
            let kind_of = |e: &(f64, f64, f64)| {
                if short(e) {
                    0
                } else if breathy(e) {
                    1
                } else {
                    2
                }
            };
            let k = kind_of(&events[i]);
            let mut j = i;
            while j + 1 < events.len() && kind_of(&events[j + 1]) == k && {
                let gap = events[j + 1].0 - events[j].1;
                if k == 0 {
                    (0.1..=0.7).contains(&gap)
                } else {
                    (0.05..=0.6).contains(&gap)
                }
            } {
                j += 1;
            }
            let steady = {
                let starts: Vec<f64> = events[i..=j].iter().map(|e| e.0).collect();
                let iv: Vec<f64> = starts.windows(2).map(|w| w[1] - w[0]).collect();
                let mean = iv.iter().sum::<f64>() / iv.len().max(1) as f64;
                let var =
                    iv.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / iv.len().max(1) as f64;
                !iv.is_empty() && var.sqrt() <= 0.3 * mean
            };
            let need = if k == 0 { 6 } else { 4 };
            if k < 2 && j - i + 1 >= need && steady {
                let (a, b) = (events[i].0, events[j].1);
                let score = events[i..=j].iter().map(|e| e.2).sum::<f64>() / (j - i + 1) as f64;
                found.push(VocalCandidate {
                    source_id: tr.source_id.clone(),
                    character: tr.character.clone(),
                    tag: if k == 0 { "running" } else { "panting" }.into(),
                    t0: (a - PAD_BEFORE).max(0.0),
                    t1: b + PAD_AFTER,
                    text: String::new(),
                    score,
                });
                for u in used.iter_mut().take(j + 1).skip(i) {
                    *u = true;
                }
            }
            i = j + 1;
        }
        // single sounds: a breath is well under the voice, an effort near it
        for (n, &(a, b, over)) in events.iter().enumerate() {
            if used[n] || b - a < 0.15 || over < 8.0 {
                continue;
            }
            let lvl = db(level(tr.env, a, b));
            let tag = if lvl >= speech_db - 6.0 {
                if b - a > 1.2 {
                    continue; // long and loud without words: likely talk the recogniser missed
                }
                "effort"
            } else if b - a <= 1.5 {
                "breath"
            } else {
                continue;
            };
            found.push(VocalCandidate {
                source_id: tr.source_id.clone(),
                character: tr.character.clone(),
                tag: tag.into(),
                t0: (a - PAD_BEFORE).max(0.0),
                t1: b + PAD_AFTER,
                text: String::new(),
                score: over,
            });
        }

        // the clearest few of each kind
        let level_of = |c: &VocalCandidate| db(level(tr.env, c.t0, c.t1)) - db(floor);
        for c in found.iter_mut().filter(|c| c.score == 0.0) {
            c.score = level_of(c);
        }
        found.sort_by(|a, b| a.tag.cmp(&b.tag).then(b.score.total_cmp(&a.score)));
        let mut kept: Vec<VocalCandidate> = Vec::new();
        for c in found {
            let same = kept.iter().filter(|k| k.tag == c.tag).count();
            let overlaps = kept.iter().any(|k| c.t0 < k.t1 && k.t0 < c.t1);
            if same < cap(&c.tag) && !overlaps {
                kept.push(c);
            }
        }
        kept.sort_by(|a, b| a.t0.total_cmp(&b.t0));
        all.extend(kept);
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interjections_are_tagged() {
        assert_eq!(interjection_tag("Mh-hm…"), Some("mhm"));
        assert_eq!(interjection_tag("Mmhmm"), Some("mhm"));
        assert_eq!(interjection_tag("Whoooaaah!"), Some("whoa"));
        assert_eq!(interjection_tag("Ah,"), Some("ah"));
        assert_eq!(interjection_tag("haha"), Some("laugh"));
        assert_eq!(interjection_tag("Oof"), Some("effort"));
        assert_eq!(interjection_tag("science"), None);
        assert_eq!(interjection_tag("of"), None);
        assert_eq!(interjection_tag("Oof!"), Some("effort"));
        assert_eq!(interjection_tag("he"), None);
        assert_eq!(interjection_tag("Mm"), Some("mhm"));
        assert_eq!(interjection_tag("who"), None);
    }

    /// 60 s at a quiet floor with a take (10–14 s, line "I'm done"), an "mh-hm" word
    /// outside it, a breath, a panting run and a running run.
    fn track(env: &mut Vec<f32>) -> VocalTrack<'_> {
        let set = |env: &mut Vec<f32>, a: f64, b: f64, v: f32| {
            for e in &mut env[(a * 100.0) as usize..(b * 100.0) as usize] {
                *e = v;
            }
        };
        env.resize(6000, 0.001);
        set(env, 10.0, 14.0, 0.2); // the take
        set(env, 20.0, 20.5, 0.15); // "mh-hm"
        set(env, 25.0, 25.4, 0.02); // a breath (well under the voice)
        for k in 0..6 {
            let t = 30.0 + k as f64 * 0.6;
            set(env, t, t + 0.3, 0.03); // panting
        }
        for k in 0..6 {
            let t = 40.0 + k as f64 * 0.4;
            set(env, t, t + 0.08, 0.05); // steps
        }
        VocalTrack {
            source_id: "paul".into(),
            character: "PAUL".into(),
            offset_s: 0.0,
            session: None,
            env,
            words: vec![
                ("I'm".into(), 10.1, 10.4),
                ("done".into(), 10.5, 11.0),
                ("Mh-hm".into(), 20.0, 20.5),
            ],
            takes: vec![(10.0, 14.0, false)],
        }
    }

    #[test]
    fn finds_words_breaths_panting_and_steps_but_not_dialogue() {
        let mut env = Vec::new();
        let v = find_vocals(&[track(&mut env)]);
        let tags: Vec<&str> = v.iter().map(|c| c.tag.as_str()).collect();
        assert_eq!(tags, vec!["mhm", "breath", "panting", "running"], "{v:#?}");
        assert_eq!(v[0].text, "Mh-hm");
        assert!(
            v.iter().all(|c| c.t1 <= 10.0 || c.t0 >= 14.0),
            "nothing from the take"
        );
        assert!((v[2].t0 - (30.0 - PAD_BEFORE)).abs() < 0.02 && v[2].t1 > 33.3);
    }

    #[test]
    fn a_split_mh_hm_is_one_sound_but_separate_mms_stay_apart() {
        let env = vec![0.001f32; 1000];
        let t = VocalTrack {
            source_id: "p".into(),
            character: "PAUL".into(),
            offset_s: 0.0,
            session: None,
            env: &env,
            words: vec![
                ("Mh".into(), 1.0, 1.2),
                ("hm.".into(), 1.25, 1.5),
                ("Mm.".into(), 3.0, 3.2),
                ("Mm.".into(), 3.25, 3.5),
            ],
            takes: vec![],
        };
        let v = find_vocals(&[t]);
        let heard: Vec<&str> = v.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            heard,
            vec!["Mh-hm.", "Mm."],
            "the second Mm overlaps the first's air: {v:#?}"
        );
    }

    #[test]
    fn another_actors_bleed_is_not_a_vocal() {
        let mut env = Vec::new();
        let mut paul = track(&mut env);
        paul.session = Some("s".into());
        // Sara's mic hears the same breath louder: it is hers
        let mut sara_env = vec![0.001f32; 6000];
        for e in sara_env.iter_mut().take(2540).skip(2500) {
            *e = 0.2;
        }
        let sara = VocalTrack {
            source_id: "sara".into(),
            character: "SARA".into(),
            offset_s: 0.0,
            session: Some("s".into()),
            env: &sara_env,
            words: vec![],
            takes: vec![],
        };
        let v = find_vocals(&[paul, sara]);
        assert!(
            !v.iter().any(|c| c.character == "PAUL" && c.tag == "breath"),
            "{v:#?}"
        );
    }
}
