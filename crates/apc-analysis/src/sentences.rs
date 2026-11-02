//! Readable text from the ASR: Parakeet's fragments merged into sentences per mic, and the
//! scene timeline that interleaves one actor's session mic with the session's director mic.
//!
//! Every sentence keeps its word range, so anything that points at a sentence (Claude,
//! the dev panel) maps back to exact word timings. Timecodes never travel as text.

use serde::Serialize;

use crate::asr::Transcript;
use crate::fuzzy::ratio;

#[derive(Debug, Clone, Copy)]
pub struct MergeOptions {
    /// Fragments closer than this join (seconds) …
    pub max_gap: f64,
    /// … unless the first one ended a sentence; then only closer than this.
    pub max_gap_after_end: f64,
    pub max_words: usize,
    pub max_len_s: f64,
}

impl Default for MergeOptions {
    fn default() -> Self {
        MergeOptions {
            max_gap: 0.7,
            max_gap_after_end: 0.15,
            max_words: 40,
            max_len_s: 15.0,
        }
    }
}

/// Fragments of one mic merged into a sentence. Times are the transcript's (session time
/// once the transcript is shifted).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sentence {
    /// Word range `[w0, w1)` into the transcript's words.
    pub w0: usize,
    pub w1: usize,
    pub t0: f64,
    pub t1: f64,
    pub text: String,
}

fn ends_sentence(text: &str) -> bool {
    let t = text.trim_end_matches(['"', '\'', ')', '’', '”']);
    t.ends_with(['.', '?', '!', '…'])
}

/// Merges the transcript's segments into sentences.
pub fn sentences(tr: &Transcript, opts: &MergeOptions) -> Vec<Sentence> {
    let mut out: Vec<Sentence> = Vec::new();
    for s in tr.segments.iter().filter(|s| s.w1 > s.w0) {
        let text = s.text.trim();
        let join = out.last().is_some_and(|c| {
            let gap = s.start - c.t1;
            let limit = if ends_sentence(&c.text) {
                opts.max_gap_after_end
            } else {
                opts.max_gap
            };
            gap <= limit
                && (c.w1 - c.w0) + (s.w1 - s.w0) <= opts.max_words
                && s.end - c.t0 <= opts.max_len_s
        });
        match out.last_mut() {
            Some(c) if join => {
                c.w1 = s.w1;
                c.t1 = s.end;
                c.text.push(' ');
                c.text.push_str(text);
            }
            _ => out.push(Sentence {
                w0: s.w0,
                w1: s.w1,
                t0: s.start,
                t1: s.end,
                text: text.to_string(),
            }),
        }
    }
    out
}

/// What a row of the timeline is, for the reader.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowTag {
    /// Session mic, already a take of this line (matcher or rescue).
    Take { line_id: String },
    /// Session mic, loud enough, not a take: Claude may judge it.
    Open,
    /// Session mic, too quiet for the actor or the same speech is the director's.
    Bleed,
    /// Director mic: notes, cues and context, never a take.
    Director,
}

/// One sentence placed on the scene timeline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// `s12` (session mic sentence 12) or `d4` (director mic sentence 4).
    pub id: String,
    /// Index of the mic in the `mics` passed to [`timeline`].
    pub mic: usize,
    /// Index of the sentence in that mic's sentences.
    pub sentence: usize,
    pub t0: f64,
    pub t1: f64,
    pub speaker: String,
    pub text: String,
    pub tag: RowTag,
}

/// One mic of a session with its sentences and their levels (dB).
pub struct Mic<'a> {
    pub source_id: &'a str,
    /// Session mic of `speaker` (false: the director mic).
    pub session: bool,
    pub speaker: String,
    pub sentences: &'a [Sentence],
    pub levels: &'a [f64],
}

fn median(v: &[f64]) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    s.sort_by(f64::total_cmp);
    s.get(s.len() / 2).copied().unwrap_or(-60.0)
}

/// The scene timeline in `[lo, hi)` (session time): the session mic's sentences, tagged by
/// `tag_of` (take or open), and the director mics' sentences, by time.
///
/// Doubles: when a director-mic sentence and a session-mic sentence overlap and read alike,
/// the one louder against its own mic's typical level wins. Actor speech is kept from the
/// session mic only, director speech from the director mic only.
pub fn timeline(
    mics: &[Mic],
    lo: f64,
    hi: f64,
    tag_of: &dyn Fn(usize, usize) -> RowTag,
) -> Vec<Row> {
    let medians: Vec<f64> = mics.iter().map(|m| median(m.levels)).collect();
    let mut rows: Vec<Row> = Vec::new();
    for (mi, m) in mics.iter().enumerate() {
        let prefix = if m.session { "s" } else { "d" };
        for (si, s) in m.sentences.iter().enumerate() {
            if s.t1 <= lo || s.t0 >= hi {
                continue;
            }
            let id = if mics.iter().filter(|o| o.session == m.session).count() > 1 {
                format!("{prefix}{mi}.{si}")
            } else {
                format!("{prefix}{si}")
            };
            rows.push(Row {
                id,
                mic: mi,
                sentence: si,
                t0: s.t0,
                t1: s.t1,
                speaker: if m.session {
                    m.speaker.clone()
                } else {
                    "DIRECTOR".into()
                },
                text: s.text.clone(),
                tag: if m.session {
                    tag_of(mi, si)
                } else {
                    RowTag::Director
                },
            });
        }
    }
    // doubles between a session mic and a director mic
    let mut drop = vec![false; rows.len()];
    for a in 0..rows.len() {
        for b in 0..rows.len() {
            let (ra, rb) = (&rows[a], &rows[b]);
            if !mics[ra.mic].session || mics[rb.mic].session {
                continue;
            }
            let overlap = ra.t1.min(rb.t1) - ra.t0.max(rb.t0);
            let shorter = (ra.t1 - ra.t0).min(rb.t1 - rb.t0).max(0.05);
            if overlap < 0.5 * shorter
                || ratio(&ra.text.to_lowercase(), &rb.text.to_lowercase()) < 60.0
            {
                continue;
            }
            let za = mics[ra.mic].levels[ra.sentence] - medians[ra.mic];
            let zb = mics[rb.mic].levels[rb.sentence] - medians[rb.mic];
            if za >= zb {
                drop[b] = true; // the actor, heard on the director mic too
            } else if !matches!(rows[a].tag, RowTag::Take { .. }) {
                rows[a].tag = RowTag::Bleed; // the director, heard on the actor's mic
            }
        }
    }
    let mut out: Vec<Row> = rows
        .into_iter()
        .zip(drop)
        .filter(|(_, d)| !d)
        .map(|(r, _)| r)
        .collect();
    out.sort_by(|a, b| a.t0.total_cmp(&b.t0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(segs: &[(f64, f64, &str)]) -> Transcript {
        Transcript::from_segments(
            segs.iter()
                .map(|(a, b, t)| (*a, *b, t.to_string(), None))
                .collect(),
            "t",
        )
    }

    #[test]
    fn fragments_merge_into_sentences_and_keep_their_words() {
        let t = tr(&[
            (50.5, 51.0, "D d"),
            (51.3, 51.8, "Dad?"),
            (52.6, 53.0, "Yeah."),
            (53.1, 53.4, "Yeah."),
            (53.7, 55.0, "it's a little bit late"),
            (55.2, 56.4, "to get a turkey, though."),
        ]);
        let s = sentences(&t, &MergeOptions::default());
        let texts: Vec<&str> = s.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "D d Dad?",
                "Yeah. Yeah.",
                "it's a little bit late to get a turkey, though."
            ]
        );
        // a sentence maps back to its exact words
        let last = &s[2];
        assert_eq!(t.words[last.w0].raw, "it's");
        assert_eq!(t.words[last.w1 - 1].raw, "though.");
        assert!((t.words[last.w0].t0 - 53.7).abs() < 1e-9);
    }

    #[test]
    fn a_new_take_after_a_finished_one_stays_its_own_sentence() {
        let t = tr(&[
            (1.0, 3.0, "Lunch is getting cold."),
            (3.4, 5.0, "Lunch is getting cold."),
        ]);
        assert_eq!(sentences(&t, &MergeOptions::default()).len(), 2);
    }

    fn sent(t0: f64, t1: f64, text: &str) -> Sentence {
        Sentence {
            w0: 0,
            w1: 1,
            t0,
            t1,
            text: text.into(),
        }
    }

    #[test]
    fn doubles_keep_the_actor_on_the_session_mic_and_the_director_on_the_director_mic() {
        let actor = [
            sent(10.0, 12.0, "It's a little bit late to get a turkey"),
            sent(14.0, 15.0, "Once more, drier."),
        ];
        let director = [
            sent(10.0, 12.0, "it's a little bit late to get a turkey"),
            sent(14.0, 15.0, "Once more, drier."),
        ];
        // the actor is loud on her mic and quiet on the director's; the director the other way
        let mics = [
            Mic {
                source_id: "sara",
                session: true,
                speaker: "SARA".into(),
                sentences: &actor,
                levels: &[-20.0, -45.0],
            },
            Mic {
                source_id: "dir",
                session: false,
                speaker: String::new(),
                sentences: &director,
                levels: &[-44.0, -22.0],
            },
        ];
        let rows = timeline(&mics, 0.0, 100.0, &|_, _| RowTag::Open);
        let shown: Vec<(&str, &str, &RowTag)> = rows
            .iter()
            .map(|r| (r.id.as_str(), r.speaker.as_str(), &r.tag))
            .collect();
        assert_eq!(
            shown,
            [
                ("s0", "SARA", &RowTag::Open),
                ("s1", "SARA", &RowTag::Bleed),
                ("d1", "DIRECTOR", &RowTag::Director),
            ]
        );
    }
}
