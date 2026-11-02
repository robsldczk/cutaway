//! The dev panel's view under the hood: one actor session as Claude reads it (session mic
//! and director mic as sentences on one timeline), what each sentence became, the script
//! and the marks. Read-only; changes go through `put_mark` / `send_marks` / `apply_mark`.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use apc_analysis::asr::Transcript;
use apc_analysis::fuzzy::ratio;
use apc_analysis::sentences::{sentences, timeline, MergeOptions, Mic, RowTag, Sentence};
use apc_core::*;

use crate::error::{AppError, Result};
use crate::service::Service;

/// The cached transcript of a source (any ASR provider), in file time.
pub fn transcript(svc: &Service, source: &SourceFile) -> Option<Transcript> {
    let dir = svc.paths.cache.join("asr");
    let preferred = dir.join(format!("{}-{}.json", source.hash, svc.asr_provider().id()));
    let file = if preferred.exists() {
        preferred
    } else {
        std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&format!("{}-", source.hash)))
            })?
    };
    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

/// The words of a source in `[t0, t1)` (file time), as heard.
pub fn heard_between(svc: &Service, source: &SourceFile, t0: f64, t1: f64) -> String {
    transcript(svc, source)
        .map(|t| {
            t.words
                .iter()
                .filter(|w| w.t1 > t0 && w.t0 < t1)
                .map(|w| w.raw.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// Character sessions the panel can open: one per character session mic.
pub fn sessions(svc: &Service) -> Value {
    let doc = svc.doc();
    let list: Vec<Value> = doc
        .sources
        .values()
        .filter(|s| s.role == Role::Character)
        .map(|s| {
            let takes = doc.takes.values().filter(|t| t.source_id == s.id);
            let (mut matcher, mut rescue, mut manual) = (0, 0, 0);
            for t in takes {
                match t.origin {
                    TakeOrigin::Matcher => matcher += 1,
                    TakeOrigin::Rescue => rescue += 1,
                    TakeOrigin::Manual => manual += 1,
                }
            }
            json!({
                "source_id": s.id, "name": s.original_name, "character": s.character,
                "takes": { "matcher": matcher, "rescue": rescue, "manual": manual },
                "marks": doc.marks.values().filter(|m| m.source_id == s.id).count(),
            })
        })
        .collect();
    json!({ "sessions": list })
}

fn median(v: &mut [f64]) -> Option<f64> {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied()
}

/// One actor session: every sentence of the session mic and its director mics, by time.
pub fn session_view(svc: &mut Service, source_id: &str) -> Result<Value> {
    let doc = svc.doc().clone();
    let src = doc
        .sources
        .get(source_id)
        .filter(|s| s.role == Role::Character)
        .cloned()
        .ok_or_else(|| {
            AppError::Params(format!("'{source_id}' is not a character's session mic"))
        })?;
    let role = src.character.clone().unwrap_or_default();
    let mut members = vec![src.clone()];
    members.extend(
        doc.sources
            .values()
            .filter(|s| {
                s.role == Role::Room && s.session_id.is_some() && s.session_id == src.session_id
            })
            .cloned(),
    );
    let merge = MergeOptions::default();
    let mut sents: Vec<Vec<Sentence>> = Vec::new();
    let mut levels: Vec<Vec<f64>> = Vec::new();
    let mut texts: Vec<Option<Transcript>> = Vec::new();
    for m in &members {
        let t = transcript(svc, m).map(|t| t.shifted(-m.offset_s));
        let ss = t.as_ref().map(|t| sentences(t, &merge)).unwrap_or_default();
        let a = svc.analysis_of(&m.id);
        levels.push(
            ss.iter()
                .map(|s| {
                    a.as_ref()
                        .map(|a| a.rms_db(s.t0 + m.offset_s, s.t1 + m.offset_s))
                        .unwrap_or(-60.0)
                })
                .collect(),
        );
        sents.push(ss);
        texts.push(t);
    }
    if texts[0].is_none() {
        return Err(AppError::Params(format!(
            "no transcript for {} yet; run the analysis first",
            src.original_name
        )));
    }
    let takes: Vec<&Take> = doc
        .takes
        .values()
        .filter(|t| t.source_id == src.id)
        .collect();
    let mut take_levels: Vec<f64> = takes.iter().map(|t| t.db).collect();
    let med = median(&mut take_levels);
    let quiet = |db: f64| {
        db < apc_analysis::pipeline::BLEED_DB
            || med.is_some_and(|m| db < m - apc_analysis::pipeline::RELATIVE_BLEED_DB)
    };
    let off = src.offset_s;
    let take_at = |s: &Sentence| -> Option<&Take> {
        takes
            .iter()
            .copied()
            .filter(|t| {
                let ov = (t.t1 - off).min(s.t1) - (t.t0 - off).max(s.t0);
                ov > 0.5 * (s.t1 - s.t0).max(0.05)
            })
            .max_by(|a, b| a.duration().total_cmp(&b.duration()))
    };
    let mics: Vec<Mic> = members
        .iter()
        .enumerate()
        .map(|(i, m)| Mic {
            source_id: &m.id,
            session: i == 0,
            speaker: role.clone(),
            sentences: &sents[i],
            levels: &levels[i],
        })
        .collect();
    let tag_of = |mi: usize, si: usize| {
        let s = &sents[mi][si];
        if let Some(t) = take_at(s) {
            RowTag::Take {
                line_id: t.line_id.clone(),
            }
        } else if quiet(levels[mi][si]) {
            RowTag::Bleed
        } else {
            RowTag::Open
        }
    };
    let rows = timeline(&mics, f64::MIN, f64::MAX, &tag_of);

    // the actor's lines, for near misses
    let own: Vec<(&ScriptLine, String)> = doc
        .lines
        .values()
        .filter(|l| l.kind == LineKind::Dialogue && l.character.as_deref() == Some(role.as_str()))
        .map(|l| (l, l.words.join(" ")))
        .collect();
    let out_rows: Vec<Value> = rows
        .iter()
        .map(|r| {
            let m = &members[r.mic];
            let s = &sents[r.mic][r.sentence];
            let (f0, f1) = (r.t0 + m.offset_s, r.t1 + m.offset_s);
            let status = match &r.tag {
                RowTag::Take { .. } => {
                    let t = take_at(s);
                    json!({
                        "kind": "take",
                        "line_id": t.map(|t| t.line_id.clone()),
                        "take_id": t.map(|t| t.id.clone()),
                        "take_no": t.map(|t| t.take_no),
                        "origin": t.map(|t| t.origin),
                        "read_as": t.and_then(|t| t.read_as.clone()),
                        "score": t.map(|t| t.score),
                    })
                }
                RowTag::Open => {
                    let heard = apc_core::text::norm_words(&s.text).join(" ");
                    let best = own
                        .iter()
                        .map(|(l, w)| (ratio(w, &heard), *l))
                        .max_by(|a, b| a.0.total_cmp(&b.0));
                    match best {
                        Some((sc, l)) if sc >= 55.0 => {
                            json!({ "kind": "near_miss", "line_id": l.id, "score": sc.round() })
                        }
                        _ => json!({ "kind": "unmatched" }),
                    }
                }
                RowTag::Bleed => json!({ "kind": "bleed" }),
                RowTag::Director => {
                    let class = doc
                        .notes
                        .values()
                        .filter(|n| n.source_id == m.id && n.t1 > f0 && n.t0 < f1)
                        .map(|n| n.class)
                        .next();
                    json!({ "kind": "director", "class": class })
                }
            };
            let marks: Vec<&str> = doc
                .marks
                .values()
                .filter(|k| k.source_id == m.id && k.t1 > f0 && k.t0 < f1)
                .map(|k| k.id.as_str())
                .collect();
            json!({
                "id": r.id, "source_id": m.id, "mic": if r.mic == 0 { "session" } else { "director" },
                "speaker": r.speaker, "text": r.text, "t0": f0, "t1": f1, "st": r.t0,
                "level_db": (levels[r.mic][r.sentence] * 10.0).round() / 10.0,
                "status": status, "marks": marks,
            })
        })
        .collect();

    // the script, scene by scene, with each line's takes by origin
    let mut by_line: BTreeMap<&str, Vec<&Take>> = BTreeMap::new();
    for t in doc.takes.values().filter(|t| t.source_id == src.id) {
        by_line.entry(t.line_id.as_str()).or_default().push(t);
    }
    let scenes: Vec<Value> = doc
        .scenes_in_order()
        .iter()
        .map(|sc| {
            let lines: Vec<Value> = doc
                .lines_of_scene(&sc.id)
                .iter()
                .filter(|l| l.kind == LineKind::Dialogue)
                .enumerate()
                .map(|(i, l)| {
                    let ts = by_line.get(l.id.as_str()).cloned().unwrap_or_default();
                    json!({
                        "id": l.id, "label": format!("L{}", i + 1), "character": l.character,
                        "text": l.text, "own": l.character.as_deref() == Some(role.as_str()),
                        "takes": ts.iter().map(|t| json!({
                            "id": t.id, "take_no": t.take_no, "origin": t.origin, "t0": t.t0,
                            "t1": t.t1, "heard": t.heard, "read_as": t.read_as,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({ "id": sc.id, "number": sc.number, "heading": sc.heading, "lines": lines })
        })
        .collect();
    let marks: Vec<&DevMark> = doc
        .marks
        .values()
        .filter(|m| members.iter().any(|s| s.id == m.source_id))
        .collect();
    Ok(json!({
        "source_id": src.id, "character": role, "name": src.original_name,
        "director": members.iter().skip(1).map(|m| m.original_name.clone()).collect::<Vec<_>>(),
        "rows": out_rows, "scenes": scenes, "marks": marks,
    }))
}
