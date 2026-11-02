//! Script alignment, ported from Stage 0 (`candidates`, `greedy`, two passes).
//!
//! Transcript windows are fuzzy-matched against *all* characters' lines, so speech that
//! bleeds onto a mic matches its own character's line and drops out. Matching is greedy
//! and non-overlapping, longer lines weigh more. One- and two-word lines only match
//! when they stand alone or open a segment.

use std::collections::BTreeSet;

use apc_core::{Document, LineKind};

use crate::asr::TWord;
use crate::fuzzy::Pattern;

/// A script unit prepared for matching.
pub struct Unit {
    pub id: String,
    pub scene_id: String,
    /// Scene position in script order (0-based).
    pub scene_idx: usize,
    pub kind: LineKind,
    pub character: Option<String>,
    pub n_words: usize,
    pub target: String,
    pattern: Pattern,
}

impl Unit {
    pub fn new(
        id: &str,
        scene_id: &str,
        scene_idx: usize,
        kind: LineKind,
        character: Option<&str>,
        words: &[String],
    ) -> Self {
        let target = words.join(" ");
        Unit {
            id: id.to_string(),
            scene_id: scene_id.to_string(),
            scene_idx,
            kind,
            character: character.map(String::from),
            n_words: words.len(),
            pattern: Pattern::new(&target),
            target,
        }
    }
}

/// All units of the document in script order.
pub fn units_of(doc: &Document) -> Vec<Unit> {
    let mut out = Vec::new();
    for (si, sc) in doc.scenes_in_order().iter().enumerate() {
        for l in doc.lines_of_scene(&sc.id) {
            if l.adlib || l.words.is_empty() {
                continue;
            }
            if l.kind == LineKind::Dialogue || l.kind == LineKind::Action {
                out.push(Unit::new(
                    &l.id,
                    &sc.id,
                    si,
                    l.kind,
                    l.character.as_deref(),
                    &l.words,
                ));
            }
        }
    }
    out
}

/// A matched window: words `[w0, w1)` perform `unit`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub unit: usize,
    pub score: f64,
    pub w0: usize,
    pub w1: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct AlignOptions {
    /// A pause longer than this splits windows (seconds).
    pub max_gap: f64,
    /// Added to every similarity threshold (negative is looser).
    pub threshold_offset: f64,
}

impl Default for AlignOptions {
    fn default() -> Self {
        AlignOptions {
            max_gap: 1.6,
            threshold_offset: 0.0,
        }
    }
}

struct Cand {
    pr: f64,
    sc: f64,
    unit: usize,
    i: usize,
    j: usize,
}

fn threshold(n: usize) -> f64 {
    if n <= 2 {
        88.0
    } else if n <= 4 {
        80.0
    } else {
        72.0
    }
}

fn candidates(
    units: &[&Unit],
    unit_index: &[usize],
    words: &[TWord],
    opts: &AlignOptions,
) -> Vec<Cand> {
    let w = words.len();
    let seg_start: Vec<bool> = (0..w)
        .map(|k| k == 0 || words[k - 1].seg != words[k].seg)
        .collect();
    let seg_end: Vec<bool> = (0..w)
        .map(|k| k == w - 1 || words[k + 1].seg != words[k].seg)
        .collect();
    // last word index a window starting at i may include (before the first long pause)
    let mut reach = vec![0usize; w];
    for i in (0..w).rev() {
        reach[i] = if i + 1 < w && words[i + 1].t0 - words[i].t1 <= opts.max_gap {
            reach[i + 1]
        } else {
            i
        };
    }
    let mut out = Vec::new();
    let mut buf = String::new();
    for (ui, u) in units.iter().enumerate() {
        let n = u.n_words;
        if n == 0 {
            continue;
        }
        let thr = threshold(n) + opts.threshold_offset;
        let weight = 1.0 + 0.04 * n.min(10) as f64;
        for i in 0..w {
            let lo = n.saturating_sub(3).max(1);
            for len in lo..n + 4 {
                let j = i + len;
                if j > w || j - 1 > reach[i] {
                    break;
                }
                if n <= 2 {
                    let whole = seg_start[i] && seg_end[j - 1];
                    let lead = seg_start[i] && u.target.len() >= 4;
                    if !(whole || lead) {
                        continue;
                    }
                }
                buf.clear();
                for (k, wd) in words[i..j].iter().enumerate() {
                    if k > 0 {
                        buf.push(' ');
                    }
                    buf.push_str(&wd.w);
                }
                if u.pattern.max_ratio(buf.len()) < thr {
                    continue;
                }
                let sc = u.pattern.ratio(&buf);
                if sc >= thr {
                    out.push(Cand {
                        pr: sc * weight,
                        sc,
                        unit: unit_index[ui],
                        i,
                        j,
                    });
                }
            }
        }
    }
    out
}

fn greedy(mut cands: Vec<Cand>) -> (Vec<Hit>, BTreeSet<usize>) {
    cands.sort_by(|a, b| b.pr.total_cmp(&a.pr));
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut hits = Vec::new();
    for c in cands {
        if (c.i..c.j).any(|k| used.contains(&k)) {
            continue;
        }
        used.extend(c.i..c.j);
        hits.push(Hit {
            unit: c.unit,
            score: c.sc.round(),
            w0: c.i,
            w1: c.j,
        });
    }
    (hits, used)
}

/// Matches `words` against the given units (indices into `all`).
pub fn align(
    all: &[Unit],
    subset: &[usize],
    words: &[TWord],
    opts: &AlignOptions,
) -> (Vec<Hit>, BTreeSet<usize>) {
    if words.is_empty() || subset.is_empty() {
        return (Vec::new(), BTreeSet::new());
    }
    let units: Vec<&Unit> = subset.iter().map(|&i| &all[i]).collect();
    greedy(candidates(&units, subset, words, opts))
}

/// Scenes active for a role: pass 1 matches long dialogue lines across the whole script;
/// the scenes where the role's lines hit, plus their neighbours, are kept.
pub fn active_scenes(
    all: &[Unit],
    words: &[TWord],
    role: &str,
    opts: &AlignOptions,
) -> BTreeSet<usize> {
    let long: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|(_, u)| u.kind == LineKind::Dialogue && u.n_words >= 4)
        .map(|(i, _)| i)
        .collect();
    let (pass1, _) = align(all, &long, words, opts);
    let mut active: BTreeSet<usize> = pass1
        .iter()
        .filter(|h| all[h.unit].character.as_deref() == Some(role))
        .map(|h| all[h.unit].scene_idx)
        .collect();
    // Pass 1 can't see scenes where the role only has short lines ("D…d…dad?!"): keep them.
    let mut has_long: std::collections::BTreeMap<usize, bool> = Default::default();
    for u in all
        .iter()
        .filter(|u| u.kind == LineKind::Dialogue && u.character.as_deref() == Some(role))
    {
        *has_long.entry(u.scene_idx).or_default() |= u.n_words >= 4;
    }
    active.extend(has_long.into_iter().filter(|(_, l)| !l).map(|(s, _)| s));
    let n_scenes = all.iter().map(|u| u.scene_idx + 1).max().unwrap_or(0);
    let mut near = BTreeSet::new();
    for s in active {
        for k in s.saturating_sub(1)..=(s + 1).min(n_scenes.saturating_sub(1)) {
            near.insert(k);
        }
    }
    near
}

/// Pass 2: all dialogue lines (every character) within the given scenes.
pub fn dialogue_in(all: &[Unit], scenes: &BTreeSet<usize>) -> Vec<usize> {
    all.iter()
        .enumerate()
        .filter(|(_, u)| u.kind == LineKind::Dialogue && scenes.contains(&u.scene_idx))
        .map(|(i, _)| i)
        .collect()
}

pub fn actions_in(all: &[Unit], scenes: &BTreeSet<usize>) -> Vec<usize> {
    all.iter()
        .enumerate()
        .filter(|(_, u)| u.kind == LineKind::Action && scenes.contains(&u.scene_idx))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::Transcript;
    use apc_core::text::norm_words;

    fn unit(id: &str, scene: usize, ch: &str, text: &str) -> Unit {
        Unit::new(
            id,
            &format!("s{scene}"),
            scene,
            LineKind::Dialogue,
            Some(ch),
            &norm_words(text),
        )
    }

    #[test]
    fn bleed_matches_its_own_character_and_short_lines_need_context() {
        let all = vec![
            unit("p1", 0, "PAUL", "So you don't think I'm a scientist, huh?"),
            unit("s1", 0, "SARA", "Lunch is getting cold."),
            unit("p2", 0, "PAUL", "Lunch."),
        ];
        let t = Transcript::from_segments(
            vec![
                (
                    1.0,
                    3.0,
                    "So you don't think I'm a scientist huh".into(),
                    None,
                ),
                (4.0, 5.5, "Lunch is getting cold.".into(), None),
                (6.0, 6.5, "Lunch.".into(), None),
                (8.0, 9.0, "and lunch then".into(), None),
            ],
            "t",
        );
        let (hits, _) = align(&all, &[0, 1, 2], &t.words, &AlignOptions::default());
        let ids: Vec<(&str, usize)> = hits
            .iter()
            .map(|h| (all[h.unit].id.as_str(), h.w0))
            .collect();
        assert!(ids.contains(&("p1", 0)));
        assert!(
            ids.contains(&("s1", 8)),
            "Sara's line on Paul's mic matches Sara"
        );
        assert!(
            ids.contains(&("p2", 12)),
            "a one-word line alone in its segment"
        );
        // "and lunch then": "lunch" doesn't open the segment, so it is not a take
        assert_eq!(hits.iter().filter(|h| all[h.unit].id == "p2").count(), 1);
    }

    #[test]
    fn a_role_with_only_short_lines_keeps_its_scenes() {
        let all = vec![
            unit("d1", 0, "DAD", "Nina, who is it at the door?"),
            unit("n1", 3, "NINA", "D... d... dad?!"),
        ];
        // what Parakeet heard on Nina's mic
        let t = Transcript::from_segments(
            vec![
                (50.5, 51.0, "D d".into(), None),
                (51.3, 51.8, "Dad?".into(), None),
            ],
            "t",
        );
        let near = active_scenes(&all, &t.words, "NINA", &AlignOptions::default());
        assert!(near.contains(&3), "{near:?}");
        let (hits, _) = align(
            &all,
            &dialogue_in(&all, &near),
            &t.words,
            &AlignOptions::default(),
        );
        assert_eq!(hits.len(), 1);
    }
}
