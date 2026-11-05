//! The mapping editor (D104): when the matcher picked the wrong stretch, the words on a mic and
//! its waveform let a person mark the right one. A stretch becomes a new take of a line, or a
//! new line. Source media is only read.

use apc_analysis::asr::TWord;
use apc_analysis::wordmap::{coverage, map_words};
use apc_core::ids::new_id;
use apc_core::text::{display_tokens, norm_word};
use apc_core::*;
use apc_media::loudness::integrated_lufs;
use apc_media::AudioReader;
use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::methods::{AddLineP, AddTakeP, MappingOpenP, PeaksP, RecordingP};
use crate::performances::holds;
use crate::service::Service;

/// Shortest stretch that makes a take.
const MIN_TAKE_S: f64 = 0.05;
/// Points of the whole file's shape.
const OVERVIEW: usize = 800;

/// A character's mics that can be listened to.
fn mics_of<'a>(doc: &'a Document, character: &str) -> Vec<&'a SourceFile> {
    doc.sources
        .values()
        .filter(|s| s.role == Role::Character && !s.missing && holds(doc, s, character))
        .collect()
}

/// The take a line plays.
fn picked<'a>(doc: &'a Document, line_id: &str) -> Option<&'a Take> {
    let id = doc.boxes.get(line_id)?.take_id.as_ref()?;
    doc.takes.get(id)
}

/// The nearest line to `at` in the scene's order, among those with a take on one of
/// `character`'s mics: where that recording is, and how far into it.
fn nearby(doc: &Document, scene: &Scene, at: usize, character: &str) -> Option<(String, f64)> {
    let mics = mics_of(doc, character);
    let on_mic = |id: &String| {
        picked(doc, id)
            .filter(|t| mics.iter().any(|m| m.id == t.source_id))
            .map(|t| (t.source_id.clone(), t.t1))
    };
    (0..scene.order.len().max(at + 1)).find_map(|d| {
        let before = at.checked_sub(d).and_then(|i| scene.order.get(i));
        let after = scene.order.get(at + d);
        before.and_then(on_mic).or_else(|| after.and_then(on_mic))
    })
}

/// Who spoke last before position `at` of the scene, or else first after it, among those with
/// a mic: the likeliest voice of a line added there.
fn speaker_near(doc: &Document, scene: &Scene, at: usize) -> Option<String> {
    let voice = |id: &String| {
        let c = doc.lines.get(id)?.character.as_ref()?;
        (!mics_of(doc, c).is_empty()).then(|| c.clone())
    };
    let before = scene.order[..at.min(scene.order.len())].iter().rev();
    let after = scene.order[at.min(scene.order.len())..].iter();
    before.chain(after).find_map(voice)
}

pub fn open(svc: &mut Service, p: &MappingOpenP) -> Result<Value> {
    let doc = svc.doc();
    let mut cast: Vec<&Character> = doc
        .characters
        .values()
        .filter(|c| !mics_of(doc, &c.id).is_empty())
        .collect();
    cast.sort_by_key(|c| c.color_index);
    let cast: Vec<Value> = cast
        .iter()
        .map(|c| {
            let mics: Vec<Value> = mics_of(doc, &c.id)
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id, "name": s.original_name, "duration_s": s.duration_s,
                        "offset_s": s.offset_s, "session_id": s.session_id,
                    })
                })
                .collect();
            json!({ "id": c.id, "label": c.label, "color_index": c.color_index, "mics": mics })
        })
        .collect();

    let (scene, at, character) = match (&p.line_id, &p.scene) {
        (Some(id), _) => {
            let line = doc
                .lines
                .get(id)
                .ok_or_else(|| AppError::Params(format!("no line '{id}'")))?;
            let scene = doc.scenes.get(&line.scene_id);
            let at = scene.and_then(|s| s.order.iter().position(|x| x == id));
            (scene, at.unwrap_or(0), line.character.clone())
        }
        (None, key) => {
            let scene = key.as_deref().and_then(|k| doc.find_scene(k));
            let scene = scene.ok_or_else(|| AppError::Params("no such scene".into()))?;
            let at = p
                .after
                .as_ref()
                .and_then(|a| scene.order.iter().position(|x| x == a))
                .map_or(0, |i| i + 1);
            (Some(scene), at, speaker_near(doc, scene, at))
        }
    };

    // a line with a take opens on it; otherwise near where the talk around it was recorded
    let take = p.line_id.as_ref().and_then(|id| picked(doc, id));
    let place = take.map(|t| (t.source_id.clone(), t.t0)).or_else(|| {
        let who = character.as_deref()?;
        scene
            .and_then(|s| nearby(doc, s, at.saturating_sub(1), who))
            .or_else(|| mics_of(doc, who).first().map(|m| (m.id.clone(), 0.0)))
    });
    let (source_id, at_s) = place.map_or((None, 0.0), |(id, t)| (Some(id), t));
    Ok(json!({
        "cast": cast, "character": character, "source_id": source_id, "at_s": at_s,
        "t0": take.map(|t| t.t0), "t1": take.map(|t| t.t1),
    }))
}

pub fn recording(svc: &mut Service, p: &RecordingP) -> Result<Value> {
    let src = mic(svc, &p.source_id)?;
    let (from, to) = (p.t0.unwrap_or(0.0), p.t1.unwrap_or(f64::INFINITY));
    let transcript = crate::dev::transcript(svc, &src);
    let heard = transcript.is_some();
    let words: Vec<Value> = transcript
        .map(|t| t.words)
        .unwrap_or_default()
        .iter()
        .filter(|w| w.t1 > from && w.t0 < to)
        .map(|w| json!({ "text": w.raw, "t0": w.t0, "t1": w.t1 }))
        .collect();
    let doc = svc.doc();
    let mut takes: Vec<&Take> = doc
        .takes
        .values()
        .filter(|t| t.source_id == src.id && t.t1 > from && t.t0 < to)
        .filter(|t| doc.lines.get(&t.line_id).is_some_and(|l| !l.removed))
        .collect();
    takes.sort_by(|a, b| a.t0.total_cmp(&b.t0));
    let takes: Vec<Value> = takes
        .iter()
        .map(|t| {
            let line = doc.lines.get(&t.line_id);
            json!({
                "id": t.id, "line_id": t.line_id, "take_no": t.take_no, "t0": t.t0, "t1": t.t1,
                "text": line.map_or("", |l| l.text.as_str()),
            })
        })
        .collect();
    let overview = svc.peaks_of(&PeaksP {
        source_id: src.id.clone(),
        t0: 0.0,
        t1: src.duration_s,
        buckets: Some(OVERVIEW),
        normalize: true,
    })?;
    Ok(json!({
        "source_id": src.id, "name": src.original_name, "character": src.character,
        "duration_s": src.duration_s, "heard": heard, "words": words, "takes": takes,
        "overview": overview,
    }))
}

/// A mic that can be read.
fn mic(svc: &Service, source_id: &str) -> Result<SourceFile> {
    let src = svc
        .doc()
        .sources
        .get(source_id)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no recording '{source_id}'")))?;
    if src.role != Role::Character {
        return Err(AppError::Params("a take goes on a character's mic".into()));
    }
    if src.missing {
        return Err(AppError::Params(format!(
            "{} is missing: relink it first",
            src.original_name
        )));
    }
    Ok(src)
}

/// In and out inside the file, and far enough apart.
fn stretch(src: &SourceFile, t0: f64, t1: f64) -> Result<(f64, f64)> {
    let end = if src.duration_s > 0.0 {
        src.duration_s
    } else {
        f64::INFINITY
    };
    let (a, b) = (t0.max(0.0), t1.min(end));
    if b - a < MIN_TAKE_S {
        return Err(AppError::Params(
            "mark an in and an out point further apart".into(),
        ));
    }
    Ok((a, b))
}

/// The words heard between in and out: those with most of their sound inside.
fn heard_in(heard: &[TWord], t0: f64, t1: f64) -> Vec<TWord> {
    heard
        .iter()
        .filter(|w| {
            let mid = (w.t0 + w.t1) / 2.0;
            mid >= t0 && mid < t1
        })
        .cloned()
        .collect()
}

/// A take of the stretch. What was heard is mapped onto the line's words; with nothing heard
/// (a gasp, a mic without a transcript) the line's own words are spread over it.
fn marked_take(
    line_id: &str,
    source_id: &str,
    (t0, t1): (f64, f64),
    heard: &[TWord],
    tokens: &[String],
    script: &[String],
) -> Take {
    let words: Vec<TakeWord> = if heard.is_empty() {
        let n = tokens.len().max(1) as f64;
        let span = t1 - t0;
        tokens
            .iter()
            .zip(script)
            .enumerate()
            .map(|(i, (raw, w))| TakeWord {
                w: w.clone(),
                raw: raw.clone(),
                t0: t0 + span * i as f64 / n,
                t1: t0 + span * (i as f64 + 0.9) / n,
                script_idx: Some(i),
            })
            .collect()
    } else {
        let said: Vec<String> = heard.iter().map(|w| w.w.clone()).collect();
        heard
            .iter()
            .zip(map_words(&said, script))
            .map(|(w, script_idx)| TakeWord {
                w: w.w.clone(),
                raw: w.raw.clone(),
                t0: w.t0.max(t0),
                t1: w.t1.min(t1),
                script_idx,
            })
            .collect()
    };
    let said: Vec<String> = heard.iter().map(|w| w.w.clone()).collect();
    let score = coverage(&map_words(&said, script), script.len()) * 100.0;
    Take {
        id: new_id("tk"),
        line_id: line_id.to_string(),
        source_id: source_id.to_string(),
        take_no: 0,
        take_type: TakeType::Full,
        t0,
        t1,
        words,
        score,
        heard: spoken(heard),
        lufs: None,
        db: 0.0,
        partial: false,
        pick: 0,
        favorite: false,
        favorite_origin: None,
        direction: String::new(),
        feedback: String::new(),
        direction_raw: String::new(),
        feedback_raw: String::new(),
        tags: vec![],
        note_ids: vec![],
        confidence: 1.0,
        needs_rematch: false,
        ai_flag: None,
        origin: TakeOrigin::Marked,
        read_as: None,
    }
}

fn spoken(heard: &[TWord]) -> String {
    let raw: Vec<&str> = heard.iter().map(|w| w.raw.trim()).collect();
    raw.join(" ")
}

/// What the matcher measures of a take, for the stretch: level and loudness.
fn measured(svc: &mut Service, src: &SourceFile, mut take: Take) -> Take {
    if let Some(a) = svc.analysis_of(&src.id) {
        take.db = a.rms_db(take.t0, take.t1);
    }
    take.lufs = AudioReader::open(&svc.paths.resolve(&src.path))
        .ok()
        .and_then(|mut r| {
            let info = r.info();
            let raw = r.read_range(take.t0, take.t1).ok()?;
            integrated_lufs(&raw, info.sample_rate, info.channels)
                .ok()
                .flatten()
        })
        .map(|l| (l * 10.0).round() / 10.0);
    take
}

pub fn add_take(svc: &mut Service, p: &AddTakeP) -> Result<Value> {
    let src = mic(svc, &p.source_id)?;
    let range = stretch(&src, p.t0, p.t1)?;
    let line = svc
        .doc()
        .lines
        .get(&p.line_id)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no line '{}'", p.line_id)))?;
    let words = crate::dev::transcript(svc, &src).map_or(vec![], |t| t.words);
    let heard = heard_in(&words, range.0, range.1);
    let take = marked_take(&line.id, &src.id, range, &heard, &line.tokens, &line.words);
    let take = measured(svc, &src, take);
    Ok(svc.exec(&Command::AddTake { take })?.result)
}

pub fn add_line(svc: &mut Service, scene_id: &str, p: &AddLineP) -> Result<Value> {
    let src = mic(svc, &p.source_id)?;
    let range = stretch(&src, p.t0, p.t1)?;
    let words = crate::dev::transcript(svc, &src).map_or(vec![], |t| t.words);
    let heard = heard_in(&words, range.0, range.1);
    let text = p
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map_or_else(|| spoken(&heard), str::to_string);
    if text.is_empty() {
        return Err(AppError::Params(
            "nothing was heard there: type what's said".into(),
        ));
    }
    let tokens = display_tokens(&text);
    let script: Vec<String> = tokens.iter().map(|t| norm_word(t)).collect();
    let take = marked_take("", &src.id, range, &heard, &tokens, &script);
    let take = measured(svc, &src, take);
    let cmd = Command::AddLine {
        scene_id: scene_id.to_string(),
        after: p.after.clone(),
        character: p.character.clone(),
        text,
        take,
    };
    Ok(svc.exec(&cmd)?.result)
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

    fn script(s: &str) -> (Vec<String>, Vec<String>) {
        let tokens = display_tokens(s);
        let words = tokens.iter().map(|t| norm_word(t)).collect();
        (tokens, words)
    }

    #[test]
    fn a_word_belongs_to_the_side_most_of_it_is_on() {
        let heard = [
            word("cut", 1.0, 1.4),
            word("in", 1.5, 1.9),
            word("half", 2.0, 2.4),
        ];
        let got = heard_in(&heard, 1.3, 2.1);
        let raw: Vec<&str> = got.iter().map(|w| w.raw.as_str()).collect();
        assert_eq!(raw, ["in"]);
    }

    #[test]
    fn a_take_keeps_its_marks_and_maps_what_was_heard() {
        let (tokens, words) = script("Oh wow, really?");
        let heard = [word("oh", 1.9, 2.2), word("wow", 2.3, 2.6)];
        let t = marked_take("ln", "src", (2.0, 2.5), &heard, &tokens, &words);
        assert_eq!((t.t0, t.t1), (2.0, 2.5));
        assert_eq!(t.origin, TakeOrigin::Marked);
        assert_eq!(t.words[0].script_idx, Some(0));
        assert_eq!(t.words[1].script_idx, Some(1));
        // word times stay inside the marks
        assert_eq!(t.words[0].t0, 2.0);
        assert_eq!(t.words[1].t1, 2.5);
        assert!((t.score - 200.0 / 3.0).abs() < 0.01, "{}", t.score);
        assert_eq!(t.heard, "oh wow");
    }

    #[test]
    fn with_nothing_heard_the_line_is_spread_over_the_stretch() {
        let (tokens, words) = script("Ah!");
        let t = marked_take("ln", "src", (5.0, 6.0), &[], &tokens, &words);
        assert_eq!(t.words.len(), 1);
        assert_eq!(t.words[0].script_idx, Some(0));
        assert_eq!(t.words[0].t0, 5.0);
        assert!(t.words[0].t1 <= 6.0);
    }

    #[test]
    fn marks_too_close_are_refused_and_the_file_end_holds() {
        let src = SourceFile {
            duration_s: 10.0,
            ..serde_json::from_value(json!({
                "id": "s", "path": "s.wav", "original_name": "s.wav", "hash": "h",
                "role": "character", "stored": "referenced", "size": 1
            }))
            .unwrap()
        };
        assert!(stretch(&src, 3.0, 3.01).is_err());
        assert_eq!(stretch(&src, -1.0, 12.0).unwrap(), (0.0, 10.0));
    }
}
