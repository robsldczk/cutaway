//! The bucket's character search: "paul huh" finds where Paul's mic heard it, pre-clipped, with
//! a stretch of audio either side to trim into (D101). Read-only; a drop goes through
//! `place_from_bucket`'s `clip`.

use std::collections::BTreeSet;
use std::sync::Arc;

use apc_analysis::asr::Transcript;
use apc_analysis::performance::{find_words, sound_kind, Mic};
use apc_analysis::vocals::{every_vocal, find_vocals, VocalCandidate, VocalTrack};
use apc_core::text::{norm_word, norm_words};
use apc_core::*;
use apc_media::AudioAnalysis;
use serde_json::{json, Value};

use crate::error::Result;
use crate::methods::PeaksP;
use crate::service::Service;

/// Stretches a search shows, the likeliest first.
const MAX_HITS: usize = 30;
/// Audio shown either side of a stretch, to trim into.
const WINDOW_S: f64 = 1.5;
/// Points of the waveform a stretch gets.
const PEAKS: usize = 120;

/// A mic with what the search reads of it.
struct Loaded {
    src: SourceFile,
    transcript: Transcript,
    analysis: Arc<AudioAnalysis>,
}

/// A stretch found, before it's dressed for the page.
struct Hit {
    source_id: String,
    character: String,
    t0: f64,
    t1: f64,
    text: String,
    before: String,
    after: String,
    /// The vocal kind, for a sound without words.
    tag: Option<String>,
    close: bool,
    alone: bool,
    level_db: f64,
}

/// Splits off a leading character name. `paul huh` → (PAUL, huh); `the colonel oh` and
/// `colonel oh` both find THE COLONEL.
fn split_name<'a>(doc: &Document, words: &'a [String]) -> (Option<String>, &'a [String]) {
    let mut best: Option<(usize, String)> = None;
    for c in doc.characters.values() {
        let names = [&c.label, &c.id].into_iter().chain(c.aliases.iter());
        for name in names {
            let parts = norm_words(name);
            let short = parts.strip_prefix(&["the".to_string()]).unwrap_or(&parts);
            for p in [parts.as_slice(), short] {
                if !p.is_empty()
                    && words.starts_with(p)
                    && best.as_ref().is_none_or(|(n, _)| p.len() > *n)
                {
                    best = Some((p.len(), c.id.clone()));
                }
            }
        }
    }
    match best {
        Some((n, id)) => (Some(id), &words[n..]),
        None => (None, words),
    }
}

/// Does this mic hold `character`'s voice? Their session mic, or takes of their lines.
fn holds(doc: &Document, s: &SourceFile, character: &str) -> bool {
    s.character.as_deref() == Some(character)
        || doc.takes.values().any(|t| {
            t.source_id == s.id
                && doc
                    .lines
                    .get(&t.line_id)
                    .is_some_and(|l| l.character.as_deref() == Some(character))
        })
}

/// The actor mics to search and those of the same sessions, with transcripts and envelopes.
fn load(svc: &mut Service, character: Option<&str>) -> Vec<Loaded> {
    let doc = svc.doc();
    let all: Vec<SourceFile> = doc
        .sources
        .values()
        .filter(|s| s.role == Role::Character && !s.missing)
        .cloned()
        .collect();
    let sessions: BTreeSet<Option<String>> = all
        .iter()
        .filter(|s| character.is_none_or(|c| holds(doc, s, c)))
        .map(|s| s.session_id.clone())
        .collect();
    all.into_iter()
        .filter(|s| sessions.contains(&s.session_id))
        .filter_map(|src| {
            let transcript = crate::dev::transcript(svc, &src)?;
            let analysis = svc.analysis_of(&src.id)?;
            Some(Loaded {
                src,
                transcript,
                analysis,
            })
        })
        .collect()
}

/// The takes on a mic: (start, end).
fn takes_on(doc: &Document, source_id: &str) -> Vec<(f64, f64)> {
    doc.takes
        .values()
        .filter(|t| t.source_id == source_id)
        .map(|t| (t.t0, t.t1))
        .collect()
}

/// The mic's typical take level, in dB of its envelope.
fn speech_db(env: &[f32], takes: &[(f64, f64)]) -> Option<f64> {
    let fps = apc_media::ENV_FPS as f64;
    let mut levels: Vec<f32> = takes
        .iter()
        .filter_map(|&(a, b)| {
            let (i, j) = ((a * fps) as usize, (b * fps).ceil() as usize);
            env.get(i..j.min(env.len()))?
                .iter()
                .copied()
                .reduce(f32::max)
        })
        .collect();
    levels.sort_by(|a, b| a.total_cmp(b));
    let mid = *levels.get(levels.len() / 2)?;
    Some(20.0 * (mid.max(1e-6) as f64).log10())
}

/// Sounds without words ("paul grunt", or only "paul"): what the analysis would keep, or all
/// of a kind when one is asked for.
fn sounds(
    doc: &Document,
    mics: &[Loaded],
    character: Option<&str>,
    kind: Option<&str>,
) -> Vec<VocalCandidate> {
    let tracks: Vec<VocalTrack> = mics
        .iter()
        .filter_map(|m| {
            Some(VocalTrack {
                source_id: m.src.id.clone(),
                character: m.src.character.clone()?,
                offset_s: m.src.offset_s,
                session: m.src.session_id.clone(),
                env: &m.analysis.env,
                // cached transcripts are in file time, as this wants
                words: m
                    .transcript
                    .words
                    .iter()
                    .map(|w| (w.raw.clone(), w.t0, w.t1))
                    .collect(),
                takes: takes_on(doc, &m.src.id)
                    .into_iter()
                    .map(|(a, b)| (a, b, false))
                    .collect(),
            })
        })
        .collect();
    let found = if kind.is_some() {
        every_vocal(&tracks)
    } else {
        find_vocals(&tracks)
    };
    found
        .into_iter()
        .filter(|c| character.is_none_or(|ch| c.character == ch))
        .filter(|c| kind.is_none_or(|k| c.tag == k))
        .collect()
}

/// Where a stretch sits: its take's scene and number, or between takes.
fn place_of(doc: &Document, h: &Hit) -> String {
    let take = doc
        .takes
        .values()
        .find(|t| t.source_id == h.source_id && h.t0 < t.t1 && t.t0 < h.t1);
    let scene = take
        .and_then(|t| doc.lines.get(&t.line_id))
        .and_then(|l| doc.scenes.get(&l.scene_id));
    match (take, scene) {
        (Some(t), Some(s)) => format!("Sc. {} · take {}", s.number, t.take_no),
        _ => "Between takes".to_string(),
    }
}

/// The audio around a stretch, scaled to its own loudest point: a grunt is small next to the
/// session's shouts.
fn window(svc: &mut Service, h: &Hit, duration_s: f64) -> Value {
    let w0 = (h.t0 - WINDOW_S).max(0.0);
    let w1 = if duration_s > 0.0 {
        (h.t1 + WINDOW_S).min(duration_s)
    } else {
        h.t1 + WINDOW_S
    };
    let peaks = svc
        .peaks_of(&PeaksP {
            source_id: h.source_id.clone(),
            t0: w0,
            t1: w1,
            buckets: Some(PEAKS),
            normalize: false,
        })
        .unwrap_or_default();
    let top = peaks.iter().copied().fold(1e-4f32, f32::max);
    let peaks: Vec<f32> = peaks.iter().map(|p| (p / top).min(1.0)).collect();
    json!({ "w0": w0, "w1": w1, "peaks": peaks })
}

pub fn search(svc: &mut Service, query: &str) -> Result<Value> {
    let doc = svc.doc();
    let cast: Vec<Value> = doc
        .characters
        .values()
        .filter(|c| doc.sources.values().any(|s| holds(doc, s, &c.id)))
        .map(|c| json!({ "id": c.id, "label": c.label, "color_index": c.color_index }))
        .collect();
    let words: Vec<String> = query
        .split_whitespace()
        .map(norm_word)
        .filter(|w| !w.is_empty())
        .collect();
    let (character, asked) = split_name(doc, &words);
    let asked = asked.to_vec();
    if words.is_empty() {
        return Ok(
            json!({ "cast": cast, "character": null, "hits": [], "total": 0, "heard": true }),
        );
    }

    let mics = load(svc, character.as_deref());
    let doc = svc.doc();
    let mut hits: Vec<Hit> = Vec::new();
    for m in &mics {
        let Some(owner) = m.src.character.as_deref() else {
            continue;
        };
        if character.as_deref().is_some_and(|c| !holds(doc, &m.src, c)) {
            continue;
        }
        let others: Vec<Mic> = mics
            .iter()
            .filter(|o| o.src.id != m.src.id && o.src.session_id == m.src.session_id)
            .filter(|o| o.src.session_id.is_some())
            .map(|o| Mic {
                offset_s: o.src.offset_s,
                env: &o.analysis.env,
                words: &[],
                speech_db: None,
                thr: 0.0,
            })
            .collect();
        let mic = Mic {
            offset_s: m.src.offset_s,
            env: &m.analysis.env,
            words: &m.transcript.words,
            speech_db: speech_db(&m.analysis.env, &takes_on(doc, &m.src.id)),
            thr: m.analysis.percentile(60.0) * 3.0,
        };
        for f in find_words(&mic, &others, &asked) {
            hits.push(Hit {
                source_id: m.src.id.clone(),
                character: character.clone().unwrap_or_else(|| owner.to_string()),
                t0: f.t0,
                t1: f.t1,
                text: f.text,
                before: f.before,
                after: f.after,
                tag: None,
                close: f.close,
                alone: f.alone,
                level_db: m.analysis.rms_db(f.t0, f.t1),
            });
        }
    }

    // a sound without words: asked for by kind, or everything of theirs for a name alone
    let kind = match asked.as_slice() {
        [w] => sound_kind(w),
        _ => None,
    };
    if asked.is_empty() || kind.is_some() {
        for c in sounds(doc, &mics, character.as_deref(), kind) {
            if hits
                .iter()
                .any(|h| h.source_id == c.source_id && c.t0 < h.t1 && h.t0 < c.t1)
            {
                continue;
            }
            let level_db = mics
                .iter()
                .find(|m| m.src.id == c.source_id)
                .map_or(f64::MIN, |m| m.analysis.rms_db(c.t0, c.t1));
            hits.push(Hit {
                source_id: c.source_id,
                character: c.character,
                t0: c.t0,
                t1: c.t1,
                text: c.text,
                before: String::new(),
                after: String::new(),
                tag: Some(c.tag),
                close: false,
                alone: true,
                level_db,
            });
        }
    }
    hits.sort_by(|a, b| {
        a.close
            .cmp(&b.close)
            .then(b.alone.cmp(&a.alone))
            .then(b.level_db.total_cmp(&a.level_db))
    });
    let total = hits.len();
    hits.truncate(MAX_HITS);

    let heard = !mics.is_empty();
    let mut out = Vec::with_capacity(hits.len());
    for h in &hits {
        let doc = svc.doc();
        let at = place_of(doc, h);
        let label = doc.characters.get(&h.character);
        let (name, color) = label.map_or((h.character.clone(), 0), |c| {
            (c.label.clone(), c.color_index)
        });
        let duration_s = doc.sources.get(&h.source_id).map_or(0.0, |s| s.duration_s);
        let id = format!("clip:{}:{:.0}", h.source_id, h.t0 * 1000.0);
        let mut v = json!({
            "id": id, "source_id": h.source_id, "character": h.character, "name": name,
            "color_index": color, "t0": h.t0, "t1": h.t1, "text": h.text, "before": h.before,
            "after": h.after, "tag": h.tag, "close": h.close, "alone": h.alone, "at": at,
        });
        let w = window(svc, h, duration_s);
        for k in ["w0", "w1", "peaks"] {
            v[k] = w[k].clone();
        }
        out.push(v);
    }
    Ok(json!({ "cast": cast, "character": character, "hits": out, "total": total, "heard": heard }))
}
