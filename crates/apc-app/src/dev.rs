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
    let merge = MergeOptions {
        max_gap: svc.config().analysis.merge_gap_s,
        ..MergeOptions::default()
    };
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
    let margin = svc.config().analysis.bleed_margin_db;
    let quiet =
        |db: f64| db < apc_analysis::pipeline::BLEED_DB || med.is_some_and(|m| db < m - margin);
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

/// A take as (line, t0, t1) in source-file seconds.
type Span = (String, f64, f64);

/// One setting variant the learning section measures.
#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    /// The config change that adopts it (`analysis.merge_gap_s` …), if any.
    pub adopt: Vec<(String, Value)>,
    pub tuning: apc_analysis::pipeline::Tuning,
    pub rescue: bool,
    pub examples: bool,
}

/// The current settings and one change at a time.
pub fn variants(svc: &Service) -> Vec<Variant> {
    let base = svc
        .configured(apc_analysis::AnalysisScope::everything())
        .tuning;
    let with =
        |name: &str, key: &str, value: f64, f: &dyn Fn(&mut apc_analysis::pipeline::Tuning)| {
            let mut t = base.clone();
            f(&mut t);
            Variant {
                name: name.into(),
                adopt: vec![(key.into(), json!(value))],
                tuning: t,
                rescue: true,
                examples: true,
            }
        };
    let m = base.merge_gap_s;
    let b = base.bleed_margin_db;
    let o = base.match_offset;
    vec![
        Variant {
            name: "current settings".into(),
            adopt: vec![],
            tuning: base.clone(),
            rescue: true,
            examples: true,
        },
        Variant {
            name: "matcher only (no Claude rescue)".into(),
            adopt: vec![("ai.rescue".into(), json!(false))],
            tuning: base.clone(),
            rescue: false,
            examples: false,
        },
        Variant {
            name: "rescue without your examples".into(),
            adopt: vec![],
            tuning: base.clone(),
            rescue: true,
            examples: false,
        },
        with(
            &format!("sentences merge at {:.1} s", (m - 0.3).max(0.2)),
            "analysis.merge_gap_s",
            (m - 0.3).max(0.2),
            &|t| t.merge_gap_s = (m - 0.3).max(0.2),
        ),
        with(
            &format!("sentences merge at {:.1} s", m + 0.3),
            "analysis.merge_gap_s",
            m + 0.3,
            &|t| t.merge_gap_s = m + 0.3,
        ),
        with(
            &format!("bleed margin {:.0} dB", b - 3.0),
            "analysis.bleed_margin_db",
            b - 3.0,
            &|t| t.bleed_margin_db = b - 3.0,
        ),
        with(
            &format!("bleed margin {:.0} dB", b + 4.0),
            "analysis.bleed_margin_db",
            b + 4.0,
            &|t| t.bleed_margin_db = b + 4.0,
        ),
        with(
            &format!("looser matching ({:+.0})", o - 5.0),
            "analysis.match_offset",
            o - 5.0,
            &|t| t.match_offset = o - 5.0,
        ),
        with(
            &format!("stricter matching ({:+.0})", o + 5.0),
            "analysis.match_offset",
            o + 5.0,
            &|t| t.match_offset = o + 5.0,
        ),
    ]
}

/// What is right in a scene on a session mic: the takes it has now, corrected by the
/// marks there (a reviewed scene: what is not marked is right). (line, t0, t1), file time.
pub fn truth(doc: &Document, source_id: &str, scene_id: &str) -> Vec<(String, f64, f64)> {
    let in_scene = |line_id: &str| {
        doc.lines
            .get(line_id)
            .is_some_and(|l| l.scene_id == scene_id)
    };
    let marks: Vec<&DevMark> = doc
        .marks
        .values()
        .filter(|m| m.source_id == source_id)
        .collect();
    let mut out: Vec<(String, f64, f64)> = doc
        .takes
        .values()
        .filter(|t| t.source_id == source_id && in_scene(&t.line_id))
        .filter(|t| {
            !marks.iter().any(|m| {
                matches!(m.kind, MarkKind::NotATake | MarkKind::WrongLine)
                    && m.t0 < t.t1
                    && t.t0 < m.t1
                    && m.line_id.as_deref() != Some(t.line_id.as_str())
            })
        })
        .map(|t| (t.line_id.clone(), t.t0, t.t1))
        .collect();
    for m in &marks {
        let Some(l) = m.line_id.as_deref().filter(|l| in_scene(l)) else {
            continue;
        };
        if matches!(
            m.kind,
            MarkKind::MissedTake | MarkKind::WrongLine | MarkKind::Edges
        ) && !out.iter().any(|(x, a, b)| x == l && *a < m.t1 && m.t0 < *b)
        {
            out.push((l.to_string(), m.t0, m.t1));
        }
    }
    out
}

fn overlaps(a: (f64, f64), b: (f64, f64)) -> bool {
    let ov = a.1.min(b.1) - a.0.max(b.0);
    ov > 0.5 * (a.1 - a.0).min(b.1 - b.0).max(0.05)
}

/// Found / wrong / missed of predicted takes against the truth.
pub fn score(
    truth: &[(String, f64, f64)],
    predicted: &[(String, f64, f64)],
) -> (usize, usize, usize) {
    let mut used = vec![false; truth.len()];
    let mut wrong = 0;
    for (l, a, b) in predicted {
        match truth
            .iter()
            .enumerate()
            .find(|(i, (tl, ta, tb))| !used[*i] && tl == l && overlaps((*a, *b), (*ta, *tb)))
        {
            Some((i, _)) => used[i] = true,
            None => wrong += 1,
        }
    }
    let found = used.iter().filter(|u| **u).count();
    (found, wrong, truth.len() - found)
}

/// Runs every variant on the given scenes of one session (dry: nothing is applied) and
/// scores it against the marks. Written to `Logs/eval-<time>.json` too.
pub fn eval(
    svc: &mut Service,
    source_id: &str,
    scene_ids: &[String],
    progress: &dyn Fn(f64, &str),
) -> Result<Value> {
    svc.bus.refresh()?;
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
    let line_ids: Vec<String> = doc
        .lines
        .values()
        .filter(|l| l.kind == LineKind::Dialogue && l.character.as_deref() == Some(role.as_str()))
        .filter(|l| scene_ids.contains(&l.scene_id))
        .map(|l| l.id.clone())
        .collect();
    if line_ids.is_empty() {
        return Err(AppError::Params(format!(
            "{role} has no lines in those scenes"
        )));
    }
    let truths: Vec<(String, Vec<Span>)> = scene_ids
        .iter()
        .map(|sc| (sc.clone(), truth(&doc, source_id, sc)))
        .collect();
    let base = svc.configured(apc_analysis::AnalysisScope::everything());
    svc.prepare_asr(&|_| {})?;
    let (ai, _) = svc.ai_runner();
    let asr = svc.asr_provider();
    let log = svc.paths.logs.join("ai-calls.jsonl");
    let calls = || {
        std::fs::read_to_string(&log)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    };
    let vs = variants(svc);
    let mut rows = Vec::new();
    for (i, v) in vs.iter().enumerate() {
        progress(i as f64 / vs.len() as f64, &format!("Trying “{}”", v.name));
        let scope = apc_analysis::AnalysisScope {
            line_ids: line_ids.clone(),
            rescue: v.rescue && base.rescue,
            examples: if v.examples {
                base.examples.clone()
            } else {
                vec![]
            },
            tuning: v.tuning.clone(),
            ..apc_analysis::AnalysisScope::everything()
        };
        let before = calls();
        let t = std::time::Instant::now();
        let out = apc_analysis::analyze_scoped(
            &doc,
            &svc.paths,
            &apc_analysis::Providers {
                asr: asr.as_ref(),
                ai: &ai,
            },
            &scope,
            &|_| {},
        )?;
        let secs = t.elapsed().as_secs_f64();
        let scenes: Vec<Value> = truths
            .iter()
            .map(|(sc, tr)| {
                let predicted: Vec<(String, f64, f64)> = out
                    .payload
                    .takes
                    .iter()
                    .filter(|t| t.source_id == source_id)
                    .filter(|t| doc.lines.get(&t.line_id).is_some_and(|l| &l.scene_id == sc))
                    .map(|t| (t.line_id.clone(), t.t0, t.t1))
                    .collect();
                let (found, wrong, missed) = score(tr, &predicted);
                json!({ "scene_id": sc, "truth": tr.len(), "found": found, "wrong": wrong, "missed": missed })
            })
            .collect();
        rows.push(json!({
            "name": v.name,
            "adopt": v.adopt.iter().map(|(k, val)| json!({ "key": k, "value": val })).collect::<Vec<_>>(),
            "scenes": scenes,
            "seconds": (secs * 10.0).round() / 10.0,
            "ai_calls": calls().saturating_sub(before),
            "flagged": out.report.flagged,
        }));
    }
    let result = json!({
        "source_id": source_id, "character": role, "scene_ids": scene_ids,
        "at": chrono::Utc::now().to_rfc3339(), "variants": rows,
    });
    let name = format!("eval-{}.json", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    std::fs::write(
        svc.paths.logs.join(name),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}

/// Where heard words and the script differ inside a take: (heard, script) phrases between
/// words that are the same in both (exact longest common subsequence).
fn differences(heard: &[String], script: &[String]) -> Vec<(String, String)> {
    let (n, m) = (heard.len(), script.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if heard[i] == script[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut anchors = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if heard[i] == script[j] {
            anchors.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    anchors.push((n, m));
    let mut out = Vec::new();
    let (mut h0, mut s0) = (0, 0);
    for (h, s) in anchors {
        let (hg, sg) = (&heard[h0..h], &script[s0..s]);
        if !hg.is_empty() && !sg.is_empty() && hg.len() <= 3 && sg.len() <= 3 {
            out.push((hg.join(" "), sg.join(" ")));
        }
        (h0, s0) = (h + 1, s + 1);
    }
    out
}

/// The learning section: the set-up, worked-example candidates from the marks, and
/// glossary candidates from rescued and hand-made takes.
pub fn learn_view(svc: &Service, source_id: &str) -> Result<Value> {
    let doc = svc.doc();
    let cfg = svc.config();
    let src = doc
        .sources
        .get(source_id)
        .ok_or_else(|| AppError::Params(format!("unknown source '{source_id}'")))?;
    let role = src.character.clone().unwrap_or_default();
    // scenes by how much of this actor there is to check
    let mut scenes: Vec<Value> = doc
        .scenes_in_order()
        .iter()
        .map(|sc| {
            let own: Vec<&ScriptLine> = doc
                .lines_of_scene(&sc.id)
                .into_iter()
                .filter(|l| {
                    l.kind == LineKind::Dialogue && l.character.as_deref() == Some(role.as_str())
                })
                .collect();
            let takes = doc
                .takes
                .values()
                .filter(|t| t.source_id == source_id && own.iter().any(|l| l.id == t.line_id))
                .count();
            json!({
                "id": sc.id, "number": sc.number, "heading": sc.heading,
                "own_lines": own.len(), "takes": takes,
                "reviewed": cfg.analysis.learn.reviewed.contains(&format!("{source_id}/{}", sc.id)),
            })
        })
        .filter(|v| v["own_lines"].as_u64().unwrap_or(0) > 0)
        .collect();
    scenes.sort_by_key(|v| std::cmp::Reverse(v["takes"].as_u64().unwrap_or(0)));

    let line_text = |id: &str| {
        doc.lines
            .get(id)
            .map(|l| l.text.clone())
            .unwrap_or_default()
    };
    let examples: Vec<String> = doc
        .marks
        .values()
        .filter(|m| m.source_id == source_id && m.status != MarkStatus::Discarded)
        .filter_map(|m| {
            let l = m.line_id.as_deref()?;
            let what = match m.kind {
                MarkKind::NotATake => {
                    return Some(format!(
                        "“{}” on {role}'s mic is not a take{}.",
                        m.text,
                        note(&m.note)
                    ))
                }
                MarkKind::WrongLine => "is a take of another line",
                _ => "is a take",
            };
            Some(format!(
                "“{}” on {role}'s mic {what}: “{}”{}.",
                m.text,
                line_text(l),
                note(&m.note)
            ))
        })
        .collect();

    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for t in doc
        .takes
        .values()
        .filter(|t| t.source_id == source_id && t.origin != TakeOrigin::Matcher)
    {
        let Some(l) = doc.lines.get(&t.line_id) else {
            continue;
        };
        let heard: Vec<String> = t.words.iter().map(|w| w.w.clone()).collect();
        for d in differences(&heard, &l.words) {
            *counts.entry(d).or_default() += 1;
        }
    }
    for m in doc.marks.values().filter(|m| m.source_id == source_id) {
        let Some(l) = m.line_id.as_ref().and_then(|l| doc.lines.get(l)) else {
            continue;
        };
        for d in differences(&apc_core::text::norm_words(&m.text), &l.words) {
            *counts.entry(d).or_default() += 1;
        }
    }
    let mut glossary: Vec<(usize, String, String)> =
        counts.into_iter().map(|((h, s), n)| (n, h, s)).collect();
    glossary.sort_by_key(|g| std::cmp::Reverse(g.0));
    glossary.truncate(25);
    Ok(json!({
        "source_id": source_id, "character": role,
        "learn": cfg.analysis.learn,
        "scenes": scenes,
        "examples": { "candidates": examples, "adopted": cfg.ai.rescue_examples },
        "glossary": {
            "candidates": glossary.iter().map(|(n, h, s)| json!({ "heard": h, "script": s, "count": n })).collect::<Vec<_>>(),
            "adopted": cfg.analysis.glossary,
        },
        "settings": { "merge_gap_s": cfg.analysis.merge_gap_s, "bleed_margin_db": cfg.analysis.bleed_margin_db, "match_offset": cfg.analysis.match_offset, "rescue": cfg.ai.rescue },
    }))
}

fn note(n: &str) -> String {
    if n.trim().is_empty() {
        String::new()
    } else {
        format!(" (editor: {})", n.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        apc_core::text::norm_words(s)
    }

    #[test]
    fn misheard_words_become_glossary_candidates() {
        let d = differences(
            &w("it's a little bit late to get a tricky though"),
            &w("It's a little bit late to get a Turkey, though"),
        );
        assert_eq!(d, [("tricky".to_string(), "turkey".to_string())]);
    }

    #[test]
    fn scoring_counts_found_wrong_and_missed() {
        let truth = vec![
            ("l1".to_string(), 10.0, 12.0),
            ("l1".to_string(), 20.0, 22.0),
        ];
        let pred = vec![
            ("l1".to_string(), 10.1, 11.9),
            ("l2".to_string(), 20.0, 22.0),
        ];
        assert_eq!(score(&truth, &pred), (1, 1, 1));
    }
}
