//! Take rescue: Claude reads one scene's script and one actor session as clean sentences
//! and says which left-over session-mic sentences are misheard takes.
//!
//! Claude only ever answers with sentence ids and line labels. The ids map back to word
//! ranges here, so timings come from the transcript, never from the model. Only the
//! actor's session mic can give takes; the director mic is context.

use std::collections::{BTreeMap, HashMap};

use apc_ai::{
    AiRunner, RescueConfidence, RescueHint, RescueRow, RescueScriptLine, TakeRescueInput,
};
use apc_core::{Document, LineKind};

use crate::align::Unit;
use crate::asr::TWord;
use crate::sentences::{timeline, Mic, Row, RowTag, Sentence};

/// A take already on the session mic (word range of its transcript).
#[derive(Debug, Clone, Copy)]
pub struct MicTake {
    pub unit: usize,
    pub w0: usize,
    pub w1: usize,
}

/// Something the editor marked: rows in `[t0, t1)` (session time) are judged whatever
/// their tag, with the note and the linked line as a hint.
#[derive(Debug, Clone)]
pub struct Hint {
    pub t0: f64,
    pub t1: f64,
    pub unit: Option<usize>,
    pub note: String,
}

/// One scene of one actor session to judge.
pub struct SceneJob<'a> {
    pub doc: &'a Document,
    pub units: &'a [Unit],
    pub scene_idx: usize,
    pub role: &'a str,
    /// The actor's session mic first, then the session's director mics.
    pub mics: &'a [Mic<'a>],
    /// Words of each mic's transcript (same order as `mics`).
    pub words: &'a [&'a [TWord]],
    pub takes: &'a [MicTake],
    pub quiet: &'a dyn Fn(f64) -> bool,
    pub lo: f64,
    pub hi: f64,
    pub hints: &'a [Hint],
    pub examples: &'a [String],
}

/// Claude's answer for one session-mic sentence, checked and mapped to words.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub row_id: String,
    pub sentence: usize,
    /// The unit performed, or `None`: not a take.
    pub unit: Option<usize>,
    /// Word range on the session mic's transcript.
    pub w0: usize,
    pub w1: usize,
    pub read_as: String,
    pub confidence: RescueConfidence,
}

pub struct SceneResult {
    pub decisions: Vec<Decision>,
    /// One line per answer that was refused, for the report and the dev panel.
    pub refused: Vec<String>,
    pub flagged: Vec<String>,
    /// The timeline sent (for the dev panel).
    pub rows: Vec<Row>,
}

/// Labels `L1…` of the scene's dialogue lines, in order, with their unit index.
pub fn scene_labels(units: &[Unit], scene_idx: usize) -> Vec<(String, usize)> {
    units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.scene_idx == scene_idx && u.kind == LineKind::Dialogue)
        .enumerate()
        .map(|(n, (i, _))| (format!("L{}", n + 1), i))
        .collect()
}

fn norm(s: &str) -> Vec<String> {
    apc_core::text::norm_words(s)
}

/// Narrows a sentence's words to a quoted `from … to` stretch (falls back to all of it).
fn narrow(
    words: &[TWord],
    w0: usize,
    w1: usize,
    from: Option<&str>,
    to: Option<&str>,
) -> (usize, usize) {
    let mut a = w0;
    let mut b = w1;
    if let Some(first) = from.map(norm).and_then(|v| v.into_iter().next()) {
        if let Some(k) = (w0..w1).find(|&k| words[k].w == first) {
            a = k;
        }
    }
    if let Some(last) = to.map(norm).and_then(|v| v.into_iter().last()) {
        if let Some(k) = (a..w1).rev().find(|&k| words[k].w == last) {
            b = k + 1;
        }
    }
    if a < b {
        (a, b)
    } else {
        (w0, w1)
    }
}

/// Builds the scene text, asks Claude, and checks every answer.
pub fn rescue_scene(job: &SceneJob, ai: &AiRunner) -> SceneResult {
    let labels = scene_labels(job.units, job.scene_idx);
    let label_of: HashMap<usize, &str> = labels.iter().map(|(l, u)| (*u, l.as_str())).collect();
    let unit_of: HashMap<&str, usize> = labels.iter().map(|(l, u)| (l.as_str(), *u)).collect();
    let session = &job.mics[0];
    let sw = job.words[0];
    let tag_of = |mi: usize, si: usize| -> RowTag {
        let s = &job.mics[mi].sentences[si];
        let n = (s.w1 - s.w0).max(1);
        if let Some(t) = job.takes.iter().find(|t| {
            let ov = t.w1.min(s.w1).saturating_sub(t.w0.max(s.w0));
            ov * 2 >= n
        }) {
            return RowTag::Take {
                line_id: job.units[t.unit].id.clone(),
            };
        }
        if (job.quiet)(job.mics[mi].levels[si]) {
            RowTag::Bleed
        } else {
            RowTag::Open
        }
    };
    let rows = timeline(job.mics, job.lo, job.hi, &tag_of);
    let forced = |r: &Row| r.mic == 0 && job.hints.iter().any(|h| r.t1 > h.t0 && r.t0 < h.t1);

    let mut recording = Vec::new();
    let mut prev_end: Option<f64> = None;
    for r in &rows {
        let tag = match &r.tag {
            _ if forced(r) => "?".to_string(),
            RowTag::Take { line_id } => job
                .units
                .iter()
                .position(|u| &u.id == line_id)
                .and_then(|u| label_of.get(&u))
                .map(|l| format!("= {l}"))
                .unwrap_or_else(|| "= another scene".into()),
            RowTag::Open => "?".into(),
            RowTag::Bleed => "bleed".into(),
            RowTag::Director => String::new(),
        };
        let pause = prev_end.map(|e| r.t0 - e).filter(|p| *p >= 3.0);
        prev_end = Some(prev_end.map_or(r.t1, |e: f64| e.max(r.t1)));
        recording.push(RescueRow {
            id: r.id.clone(),
            speaker: r.speaker.clone(),
            text: r.text.clone(),
            tag,
            pause_before_s: pause.map(|p| p.round()),
        });
    }
    let mut result = SceneResult {
        decisions: vec![],
        refused: vec![],
        flagged: vec![],
        rows: rows.clone(),
    };
    if !recording.iter().any(|r| r.tag == "?") {
        return result;
    }
    let hints: Vec<RescueHint> = job
        .hints
        .iter()
        .map(|h| RescueHint {
            ids: rows
                .iter()
                .filter(|r| r.mic == 0 && r.t1 > h.t0 && r.t0 < h.t1)
                .map(|r| r.id.clone())
                .collect(),
            line: h.unit.and_then(|u| label_of.get(&u)).map(|l| l.to_string()),
            note: h.note.clone(),
        })
        .collect();
    let scene_name = job
        .units
        .iter()
        .find(|u| u.scene_idx == job.scene_idx)
        .and_then(|u| job.doc.scenes.get(&u.scene_id))
        .map(|s| format!("{} {}", s.number, s.heading))
        .unwrap_or_default();
    let input = TakeRescueInput {
        character: job.role.to_string(),
        scene: scene_name,
        script: labels
            .iter()
            .map(|(l, u)| {
                let line = job.doc.lines.get(&job.units[*u].id);
                RescueScriptLine {
                    label: l.clone(),
                    character: job.units[*u].character.clone().unwrap_or_default(),
                    text: line.map(|l| l.text.clone()).unwrap_or_default(),
                }
            })
            .collect(),
        recording,
        hints,
        examples: job.examples.to_vec(),
    };
    let out = match ai.run(&input) {
        Ok(o) => o,
        Err(e) => {
            result.flagged.push(format!("take_rescue: {e}"));
            return result;
        }
    };
    let Some(answer) = out.output else {
        result.flagged.extend(out.flagged);
        return result;
    };
    let by_id: BTreeMap<&str, &Row> = rows.iter().map(|r| (r.id.as_str(), r)).collect();
    let open: Vec<&str> = input
        .recording
        .iter()
        .filter(|r| r.tag == "?")
        .map(|r| r.id.as_str())
        .collect();
    for a in answer.rescues {
        let refuse =
            |why: &str| format!("{} → {}: {why}", a.id, a.line.as_deref().unwrap_or("none"));
        let Some(row) = by_id.get(a.id.as_str()) else {
            result.refused.push(refuse("not in the recording"));
            continue;
        };
        // the director mic is never a take, and only open (or marked) sentences are judged
        if row.mic != 0 || !session.session {
            result.refused.push(refuse("not the actor's session mic"));
            continue;
        }
        if !open.contains(&a.id.as_str()) {
            result
                .refused
                .push(refuse("not one of the sentences to judge"));
            continue;
        }
        let unit = match a.line.as_deref() {
            None => None,
            Some(l) => match unit_of.get(l) {
                Some(&u) if job.units[u].character.as_deref() == Some(job.role) => Some(u),
                Some(_) => {
                    result.refused.push(refuse("another character's line"));
                    continue;
                }
                None => {
                    result.refused.push(refuse("no such line in this scene"));
                    continue;
                }
            },
        };
        if unit.is_some() && a.confidence == RescueConfidence::Low {
            result.refused.push(refuse("low confidence"));
            continue;
        }
        let s: &Sentence = &session.sentences[row.sentence];
        let (w0, w1) = narrow(sw, s.w0, s.w1, a.from.as_deref(), a.to.as_deref());
        result.decisions.push(Decision {
            row_id: a.id.clone(),
            sentence: row.sentence,
            unit,
            w0,
            w1,
            read_as: a.read_as.clone(),
            confidence: a.confidence,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::Transcript;
    use crate::sentences::{sentences, MergeOptions};
    use apc_core::text::norm_words;
    use std::sync::Arc;

    fn unit(id: &str, scene: usize, ch: &str, text: &str) -> Unit {
        Unit::new(
            id,
            "sc1",
            scene,
            LineKind::Dialogue,
            Some(ch),
            &norm_words(text),
        )
    }

    fn doc_with(lines: &[(&str, &str, &str)]) -> Document {
        let mut doc = Document::default();
        for (id, ch, text) in lines {
            doc.lines.insert(
                id.to_string(),
                serde_json::from_value(serde_json::json!({
                    "id": id, "scene_id": "sc1", "kind": "dialogue", "character": ch,
                    "text": text, "tokens": [], "words": [], "number": 1,
                }))
                .unwrap(),
            );
        }
        doc
    }

    #[test]
    fn misheard_take_is_rescued_and_the_director_never_is() {
        let units = vec![
            unit(
                "l1",
                0,
                "SARA",
                "It's a little bit late to get a Turkey, though",
            ),
            unit("l2", 0, "PAUL", "Germany doesn't have Thanksgiving?"),
        ];
        let doc = doc_with(&[
            (
                "l1",
                "SARA",
                "It's a little bit late to get a Turkey, though…",
            ),
            ("l2", "PAUL", "Germany doesn't have Thanksgiving?"),
        ]);
        let sara = Transcript::from_segments(
            vec![
                (
                    10.0,
                    12.0,
                    "It's a little bit late to get a turkey, though.".into(),
                    None,
                ),
                (
                    20.0,
                    22.0,
                    "it's a lid of a bit late to get a tricky though".into(),
                    None,
                ),
                (30.0, 31.0, "Can I get some water?".into(), None),
            ],
            "t",
        );
        let dir = Transcript::from_segments(
            vec![
                (15.0, 16.0, "Nice. Once more, drier.".into(), None),
                (
                    25.0,
                    27.0,
                    "it's a little bit late to get a turkey though".into(),
                    None,
                ),
            ],
            "t",
        );
        let opts = MergeOptions::default();
        let (ss, ds) = (sentences(&sara, &opts), sentences(&dir, &opts));
        let mics = [
            Mic {
                source_id: "sara",
                session: true,
                speaker: "SARA".into(),
                sentences: &ss,
                levels: &[-30.0, -31.0, -30.0],
            },
            Mic {
                source_id: "dir",
                session: false,
                speaker: String::new(),
                sentences: &ds,
                levels: &[-25.0, -25.0],
            },
        ];
        let words: [&[TWord]; 2] = [&sara.words, &dir.words];
        let takes = [MicTake {
            unit: 0,
            w0: ss[0].w0,
            w1: ss[0].w1,
        }];
        let ai = AiRunner::new(Arc::new(apc_ai::MockProvider::new()), None, None);
        let job = SceneJob {
            doc: &doc,
            units: &units,
            scene_idx: 0,
            role: "SARA",
            mics: &mics,
            words: &words,
            takes: &takes,
            quiet: &|db| db < -50.0,
            lo: 0.0,
            hi: 100.0,
            hints: &[],
            examples: &[],
        };
        let r = rescue_scene(&job, &ai);
        let got: Vec<(&str, Option<usize>)> = r
            .decisions
            .iter()
            .map(|d| (d.row_id.as_str(), d.unit))
            .collect();
        assert_eq!(got, [("s1", Some(0))], "refused: {:?}", r.refused);
        // the rescued words are the session mic's own, with their timings
        let d = &r.decisions[0];
        assert!((sara.words[d.w0].t0 - 20.0).abs() < 1e-9);
        assert!(r
            .rows
            .iter()
            .any(|x| x.id == "d1" && x.tag == RowTag::Director));
    }

    #[test]
    fn quotes_narrow_a_sentence_to_the_take() {
        let t = Transcript::from_segments(
            vec![(
                0.0,
                4.0,
                "Lunch is getting cold. Sorry, again.".into(),
                None,
            )],
            "t",
        );
        let (a, b) = narrow(&t.words, 0, t.words.len(), Some("Lunch is"), Some("cold."));
        assert_eq!((a, b), (0, 4));
    }
}
