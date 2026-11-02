//! The one timing model shared by every page, the audio engine and the export.
//!
//! A scene is a sequence of items (dialogue boxes and action spacers). A box stores its
//! take (or comp), word edits, head/tail trims and the pause before it (`gap_s`), or an
//! absolute pin. Everything after a box ripples when it changes; pinned boxes stay put.

use serde::{Deserialize, Serialize};

use crate::model::{
    Anchor, BoxState, CompSegment, Document, Edge, LineKind, Placement, Scene, Take, TakeWord,
};

/// Pacing parameters (from the project config).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, schemars::JsonSchema)]
pub struct Pacing {
    /// Default pause between two lines.
    pub breath_gap_s: f64,
    /// Short crossfade at every splice.
    pub splice_fade_s: f64,
    /// Fade at the head and tail of every clip.
    pub edge_fade_s: f64,
}

impl Default for Pacing {
    fn default() -> Self {
        Pacing {
            breath_gap_s: 0.3,
            splice_fade_s: 0.010,
            edge_fade_s: 0.005,
        }
    }
}

/// A contiguous stretch of one source file placed on the scene timeline.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClipPiece {
    pub source_id: String,
    pub take_id: String,
    pub src_in: f64,
    pub src_out: f64,
    /// Scene time where `src_in` plays.
    pub start_s: f64,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    pub gain_db: f64,
}

impl ClipPiece {
    pub fn len(&self) -> f64 {
        (self.src_out - self.src_in).max(0.0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() <= 0.0
    }
    pub fn end_s(&self) -> f64 {
        self.start_s + self.len()
    }
}

/// A word of a box as it plays.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutWord {
    pub idx: usize,
    pub text: String,
    pub take_id: String,
    /// Scene time (for deleted words: where they would have been).
    pub start_s: f64,
    pub end_s: f64,
    pub deleted: bool,
    pub script_idx: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Box,
    Spacer,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutItem {
    pub item_id: String,
    pub kind: ItemKind,
    pub start_s: f64,
    pub end_s: f64,
    /// Pause before this box (effective, after tightness); 0 for spacers.
    pub gap_before_s: f64,
    pub gap_overridden: bool,
    pub pinned: bool,
    pub character: Option<String>,
    pub take_id: Option<String>,
    pub pieces: Vec<ClipPiece>,
    pub words: Vec<LayoutWord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SceneLayout {
    pub scene_id: String,
    pub items: Vec<LayoutItem>,
    pub duration_s: f64,
}

impl SceneLayout {
    pub fn item(&self, id: &str) -> Option<&LayoutItem> {
        self.items.iter().find(|i| i.item_id == id)
    }

    pub fn pieces(&self) -> impl Iterator<Item = &ClipPiece> {
        self.items.iter().flat_map(|i| i.pieces.iter())
    }
}

/// Default spacer length for an action line: a beat that grows with its length.
pub fn default_spacer_s(word_count: usize) -> f64 {
    (0.6 + 0.04 * word_count as f64).min(2.5)
}

/// A source range with its words, before head/tail trims and edits.
#[derive(Clone, Debug)]
struct Span {
    source_id: String,
    take_id: String,
    src_in: f64,
    src_out: f64,
    words: Vec<TakeWord>,
}

fn take_words_in(take: &Take, from: usize, to: usize) -> Vec<TakeWord> {
    let mapped: Vec<TakeWord> = take
        .words
        .iter()
        .filter(|w| w.script_idx.map(|i| i >= from && i < to).unwrap_or(false))
        .cloned()
        .collect();
    if !mapped.is_empty() {
        return mapped;
    }
    // Unmapped take: fall back to proportional positions.
    let n = take.words.len();
    if n == 0 {
        return Vec::new();
    }
    let total = take
        .words
        .iter()
        .filter_map(|w| w.script_idx)
        .max()
        .map(|m| m + 1)
        .unwrap_or(n)
        .max(1);
    let a = from * n / total;
    let b = (to * n).div_ceil(total).min(n);
    take.words[a.min(n)..b.max(a).min(n)].to_vec()
}

/// Source spans for a box: the whole take, or one span per comp segment.
fn spans_for(doc: &Document, b: &BoxState) -> Vec<Span> {
    let Some(take) = b.take_id.as_ref().and_then(|id| doc.takes.get(id)) else {
        return Vec::new();
    };
    if b.comp.is_empty() {
        return vec![Span {
            source_id: take.source_id.clone(),
            take_id: take.id.clone(),
            src_in: take.t0,
            src_out: take.t1,
            words: take.words.clone(),
        }];
    }
    let mut segs: Vec<&CompSegment> = b.comp.iter().collect();
    segs.sort_by_key(|s| s.from_word);
    let last = segs.len() - 1;
    let mut out = Vec::new();
    for (i, seg) in segs.iter().enumerate() {
        let Some(t) = doc.takes.get(&seg.take_id) else {
            continue;
        };
        let words = take_words_in(t, seg.from_word, seg.to_word);
        let (mut a, mut z) = match (words.first(), words.last()) {
            (Some(f), Some(l)) => (f.t0, l.t1),
            _ => continue,
        };
        // Splice in the middle of the pauses around the words, inside the take.
        let before = t.words.iter().rev().find(|w| w.t1 <= a + 1e-6 && w.t0 < a);
        let after = t.words.iter().find(|w| w.t0 >= z - 1e-6 && w.t1 > z);
        a = if i == 0 {
            t.t0.min(a)
        } else {
            match before {
                Some(w) => (w.t1 + a) / 2.0,
                None => (a - 0.05).max(t.t0),
            }
        };
        z = if i == last {
            t.t1.max(z)
        } else {
            match after {
                Some(w) => (z + w.t0) / 2.0,
                None => (z + 0.05).min(t.t1),
            }
        };
        out.push(Span {
            source_id: t.source_id.clone(),
            take_id: t.id.clone(),
            src_in: a - seg.nudge_s,
            src_out: z - seg.nudge_s,
            words: words
                .into_iter()
                .map(|mut w| {
                    w.t0 -= seg.nudge_s;
                    w.t1 -= seg.nudge_s;
                    w
                })
                .collect(),
        });
    }
    out
}

/// Builds the clip pieces and word list of one box, in box time starting at 0.
fn build_box(
    doc: &Document,
    b: &BoxState,
    pacing: &Pacing,
) -> (Vec<ClipPiece>, Vec<LayoutWord>, f64) {
    let mut spans = spans_for(doc, b);
    if spans.is_empty() {
        return (Vec::new(), Vec::new(), 0.0);
    }
    // Head and tail trims apply to the first and last spans.
    if let Some(f) = spans.first_mut() {
        f.src_in += b.head_s;
    }
    if let Some(l) = spans.last_mut() {
        l.src_out -= b.tail_s;
    }

    // Word list across spans.
    struct W {
        span: usize,
        t0: f64,
        t1: f64,
        text: String,
        script_idx: Option<usize>,
    }
    let mut words: Vec<W> = Vec::new();
    for (si, s) in spans.iter().enumerate() {
        for w in &s.words {
            words.push(W {
                span: si,
                t0: w.t0,
                t1: w.t1,
                text: w.raw.clone(),
                script_idx: w.script_idx,
            });
        }
    }

    // Cuts (source ranges per span) from deleted words and pause trims.
    let mut cuts: Vec<(usize, f64, f64)> = Vec::new();
    let deleted: Vec<bool> = (0..words.len())
        .map(|i| b.edits.deleted.contains(&i))
        .collect();
    let mut i = 0;
    while i < words.len() {
        if !deleted[i] {
            i += 1;
            continue;
        }
        let run_start = i;
        while i < words.len() && deleted[i] && words[i].span == words[run_start].span {
            i += 1;
        }
        let run_end = i - 1;
        let span = words[run_start].span;
        let keep = b.edits.keep_pause.contains(&run_start);
        let prev =
            (run_start > 0 && words[run_start - 1].span == span).then(|| &words[run_start - 1]);
        let next = (i < words.len() && words[i].span == span).then(|| &words[i]);
        let (a, z) = if keep {
            (words[run_start].t0, words[run_end].t1)
        } else {
            let a = match prev {
                Some(p) => p.t1 + 0.04,
                None => spans[span].src_in,
            };
            let z = match next {
                Some(n) => n.t0 - 0.04,
                None => spans[span].src_out,
            };
            (a.min(words[run_start].t0), z.max(words[run_end].t1))
        };
        cuts.push((span, a, z));
    }
    for (&wi, &trim) in &b.edits.pause_trims {
        if wi + 1 >= words.len() || trim <= 0.0 {
            continue;
        }
        let (w, n) = (&words[wi], &words[wi + 1]);
        if w.span != n.span {
            continue;
        }
        let pause = n.t0 - w.t1;
        let t = trim.min((pause - 0.02).max(0.0));
        if t > 0.0 {
            let mid = w.t1 + pause / 2.0;
            cuts.push((w.span, mid - t / 2.0, mid + t / 2.0));
        }
    }

    // Split each span by its cuts into pieces and lay them end to end.
    let mut pieces: Vec<ClipPiece> = Vec::new();
    let mut piece_ranges: Vec<(usize, f64, f64, f64)> = Vec::new(); // span, in, out, box start
    let mut cursor = 0.0;
    for (si, s) in spans.iter().enumerate() {
        let mut segs = vec![(s.src_in, s.src_out)];
        let mut span_cuts: Vec<(f64, f64)> = cuts
            .iter()
            .filter(|c| c.0 == si)
            .map(|c| (c.1, c.2))
            .collect();
        span_cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (ca, cz) in span_cuts {
            let mut next = Vec::new();
            for (a, z) in segs {
                if cz <= a || ca >= z {
                    next.push((a, z));
                    continue;
                }
                if ca > a {
                    next.push((a, ca));
                }
                if cz < z {
                    next.push((cz, z));
                }
            }
            segs = next;
        }
        for (a, z) in segs {
            if z - a < 0.005 {
                continue;
            }
            let overlap = if pieces.is_empty() {
                0.0
            } else {
                pacing.splice_fade_s
            };
            let start = (cursor - overlap).max(0.0);
            pieces.push(ClipPiece {
                source_id: s.source_id.clone(),
                take_id: s.take_id.clone(),
                src_in: a,
                src_out: z,
                start_s: start,
                fade_in_s: if pieces.is_empty() {
                    pacing.edge_fade_s
                } else {
                    pacing.splice_fade_s
                },
                fade_out_s: pacing.splice_fade_s,
                gain_db: b.gain_db,
            });
            piece_ranges.push((si, a, z, start));
            cursor = start + (z - a);
        }
    }
    if let Some(l) = pieces.last_mut() {
        l.fade_out_s = pacing.edge_fade_s;
    }

    // Place words in box time.
    let mut out_words = Vec::new();
    for (idx, w) in words.iter().enumerate() {
        let hit = piece_ranges
            .iter()
            .find(|(s, a, z, _)| *s == w.span && w.t0 >= *a - 1e-6 && w.t0 < *z);
        let (start, end) = match hit {
            Some((_, a, z, st)) => (st + (w.t0 - a), st + (w.t1.min(*z) - a)),
            None => {
                // deleted or trimmed away: place it at the nearest cut point
                let prev = piece_ranges
                    .iter()
                    .rfind(|(s, a, _, _)| *s < w.span || (*s == w.span && *a <= w.t0));
                let t = prev.map(|(_, a, z, st)| st + (z - a)).unwrap_or(0.0);
                (t, t)
            }
        };
        out_words.push(LayoutWord {
            idx,
            text: w.text.clone(),
            take_id: spans[w.span].take_id.clone(),
            start_s: start,
            end_s: end,
            deleted: deleted[idx] || hit.is_none(),
            script_idx: w.script_idx,
        });
    }
    (pieces, out_words, cursor)
}

/// Lays out one scene.
pub fn layout_scene(doc: &Document, scene: &Scene, pacing: &Pacing) -> SceneLayout {
    let mut items = Vec::new();
    let mut cursor = 0.0;
    let mut pending_spacers = 0.0;
    let mut first_box = true;
    for item_id in &scene.order {
        let Some(line) = doc.lines.get(item_id) else {
            continue;
        };
        if line.kind != LineKind::Dialogue {
            let enabled = doc.spacers.get(item_id).map(|s| s.enabled).unwrap_or(false);
            let dur = doc
                .spacers
                .get(item_id)
                .and_then(|s| s.duration_s)
                .unwrap_or_else(|| default_spacer_s(line.words.len()));
            if enabled && !first_box {
                pending_spacers += dur;
            }
            items.push(LayoutItem {
                item_id: item_id.clone(),
                kind: ItemKind::Spacer,
                start_s: cursor,
                end_s: cursor,
                gap_before_s: 0.0,
                gap_overridden: false,
                pinned: false,
                character: None,
                take_id: None,
                pieces: Vec::new(),
                words: Vec::new(),
            });
            continue;
        }
        let b = doc.boxes.get(item_id).cloned().unwrap_or(BoxState {
            id: item_id.clone(),
            ..Default::default()
        });
        let (mut pieces, mut words, dur) = build_box(doc, &b, pacing);
        if pieces.is_empty() {
            items.push(LayoutItem {
                item_id: item_id.clone(),
                kind: ItemKind::Box,
                start_s: cursor,
                end_s: cursor,
                gap_before_s: 0.0,
                gap_overridden: false,
                pinned: false,
                character: line.character.clone(),
                take_id: None,
                pieces,
                words,
            });
            continue;
        }
        let (gap, overridden) = if first_box {
            (0.0, b.gap_s.is_some())
        } else {
            match b.gap_s {
                Some(g) => (g, true),
                None => (
                    (pacing.breath_gap_s + pending_spacers) * scene.tightness,
                    false,
                ),
            }
        };
        let start = match b.pinned_s {
            Some(p) => p,
            None => (cursor + gap).max(0.0),
        };
        for p in pieces.iter_mut() {
            p.start_s += start;
        }
        for w in words.iter_mut() {
            w.start_s += start;
            w.end_s += start;
        }
        let end = start + dur;
        items.push(LayoutItem {
            item_id: item_id.clone(),
            kind: ItemKind::Box,
            start_s: start,
            end_s: end,
            gap_before_s: if first_box { 0.0 } else { start - cursor },
            gap_overridden: overridden,
            pinned: b.pinned_s.is_some(),
            character: line.character.clone(),
            take_id: b.take_id.clone(),
            pieces,
            words,
        });
        // Pinned boxes don't push the ripple cursor backwards.
        cursor = if b.pinned_s.is_some() {
            cursor.max(end)
        } else {
            end
        };
        pending_spacers = 0.0;
        first_box = false;
    }
    let duration_s = items.iter().map(|i| i.end_s).fold(0.0_f64, f64::max);
    SceneLayout {
        scene_id: scene.id.clone(),
        items,
        duration_s,
    }
}

/// The effective pause before a box (what `set_gap` would need to keep the timing).
pub fn effective_gap(doc: &Document, scene: &Scene, line_id: &str, pacing: &Pacing) -> f64 {
    layout_scene(doc, scene, pacing)
        .item(line_id)
        .map(|i| i.gap_before_s)
        .unwrap_or(pacing.breath_gap_s)
}

/// Scene time of an anchor, or `None` when its line isn't on the timeline.
pub fn resolve_anchor(layout: &SceneLayout, anchor: &Anchor) -> Option<f64> {
    let item = layout.item(&anchor.line_id)?;
    if item.pieces.is_empty() && item.kind == ItemKind::Box {
        return None;
    }
    match anchor.word {
        Some(w) => {
            let word = item
                .words
                .iter()
                .find(|x| x.script_idx == Some(w) && !x.deleted)
                .or_else(|| item.words.iter().find(|x| x.idx == w))?;
            Some(match anchor.edge {
                Edge::Start => word.start_s,
                Edge::End => word.end_s,
            })
        }
        None => Some(match anchor.edge {
            Edge::Start => item.start_s,
            Edge::End => item.end_s,
        }),
    }
}

/// Scene time where a placement starts (anchor + offset, or its absolute time).
pub fn placement_start(layout: &SceneLayout, p: &Placement) -> f64 {
    match &p.anchor {
        Some(a) => resolve_anchor(layout, a).unwrap_or(p.at_s) + p.offset_s,
        None => p.at_s + p.offset_s,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::*;

    pub(crate) fn word(w: &str, t0: f64, t1: f64, idx: usize) -> TakeWord {
        TakeWord {
            w: w.to_lowercase(),
            raw: w.into(),
            t0,
            t1,
            script_idx: Some(idx),
        }
    }

    pub(crate) fn take(id: &str, line: &str, t0: f64, t1: f64, words: Vec<TakeWord>) -> Take {
        Take {
            id: id.into(),
            line_id: line.into(),
            source_id: "src".into(),
            take_no: 1,
            take_type: TakeType::Full,
            t0,
            t1,
            words,
            score: 100.0,
            heard: String::new(),
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
            origin: Default::default(),
            read_as: None,
        }
    }

    pub(crate) fn line(id: &str, kind: LineKind, n: u32, text: &str) -> ScriptLine {
        ScriptLine {
            id: id.into(),
            scene_id: "sc".into(),
            kind,
            character: (kind == LineKind::Dialogue).then(|| "PAUL".to_string()),
            extension: None,
            text: text.into(),
            paren: None,
            tokens: text.split(' ').map(String::from).collect(),
            words: text.split(' ').map(|w| w.to_lowercase()).collect(),
            number: n,
            version_added: 1,
            changed: false,
            forced: false,
            adlib: false,
        }
    }

    /// Two lines with one take each and an action line between them.
    pub(crate) fn doc2() -> (Document, Scene) {
        let mut d = Document::default();
        d.lines
            .insert("a".into(), line("a", LineKind::Dialogue, 1, "one two"));
        d.lines
            .insert("x".into(), line("x", LineKind::Action, 2, "he walks"));
        d.lines
            .insert("b".into(), line("b", LineKind::Dialogue, 3, "three four"));
        d.takes.insert(
            "ta".into(),
            take(
                "ta",
                "a",
                10.0,
                11.0,
                vec![word("one", 10.1, 10.4, 0), word("two", 10.6, 10.9, 1)],
            ),
        );
        d.takes.insert(
            "tb".into(),
            take(
                "tb",
                "b",
                20.0,
                22.0,
                vec![word("three", 20.2, 20.8, 0), word("four", 21.2, 21.8, 1)],
            ),
        );
        for (id, t) in [("a", "ta"), ("b", "tb")] {
            d.boxes.insert(
                id.into(),
                BoxState {
                    id: id.into(),
                    take_id: Some(t.into()),
                    ..Default::default()
                },
            );
        }
        d.spacers.insert(
            "x".into(),
            Spacer {
                id: "x".into(),
                duration_s: Some(0.5),
                enabled: true,
            },
        );
        let sc = Scene {
            id: "sc".into(),
            number: 1,
            heading: "INT. LAB".into(),
            int_ext: "INT".into(),
            location: "LAB".into(),
            place: "Lab".into(),
            time_of_day: String::new(),
            order: vec!["a".into(), "x".into(), "b".into()],
            tightness: 1.0,
        };
        (d, sc)
    }

    #[test]
    fn gaps_and_spacers_add_up() {
        let (d, sc) = doc2();
        let p = Pacing::default();
        let l = layout_scene(&d, &sc, &p);
        let b = l.item("b").unwrap();
        assert!((l.item("a").unwrap().end_s - 1.0).abs() < 1e-9);
        assert!((b.start_s - (1.0 + 0.3 + 0.5)).abs() < 1e-9);
        assert!((l.duration_s - (1.8 + 2.0)).abs() < 1e-9);
    }

    #[test]
    fn take_swap_ripples_and_pins_stay() {
        let (mut d, sc) = doc2();
        let p = Pacing::default();
        d.takes
            .insert("ta2".into(), take("ta2", "a", 30.0, 32.0, vec![]));
        d.boxes.get_mut("a").unwrap().take_id = Some("ta2".into());
        let l = layout_scene(&d, &sc, &p);
        assert!((l.item("b").unwrap().start_s - (2.0 + 0.8)).abs() < 1e-9);
        d.boxes.get_mut("b").unwrap().pinned_s = Some(5.0);
        let l = layout_scene(&d, &sc, &p);
        assert!((l.item("b").unwrap().start_s - 5.0).abs() < 1e-9);
    }

    #[test]
    fn tightness_scales_default_gaps_but_not_overrides() {
        let (mut d, mut sc) = doc2();
        let p = Pacing::default();
        sc.tightness = 0.5;
        let l = layout_scene(&d, &sc, &p);
        assert!((l.item("b").unwrap().gap_before_s - 0.4).abs() < 1e-9);
        d.boxes.get_mut("b").unwrap().gap_s = Some(1.0);
        let l = layout_scene(&d, &sc, &p);
        assert!((l.item("b").unwrap().gap_before_s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn negative_gap_overlaps() {
        let (mut d, sc) = doc2();
        d.boxes.get_mut("b").unwrap().gap_s = Some(-0.25);
        let l = layout_scene(&d, &sc, &Pacing::default());
        assert!((l.item("b").unwrap().start_s - 0.75).abs() < 1e-9);
    }

    #[test]
    fn deleting_a_word_closes_the_pause() {
        let (mut d, sc) = doc2();
        d.boxes.get_mut("b").unwrap().edits.deleted = vec![0];
        let l = layout_scene(&d, &sc, &Pacing::default());
        let b = l.item("b").unwrap();
        assert!(b.words[0].deleted);
        assert!(!b.words[1].deleted);
        // "three" and its pause are gone: the box starts 40 ms before "four"
        assert_eq!(b.pieces.len(), 1);
        assert!((b.pieces[0].src_in - 21.16).abs() < 1e-6);
        assert!((b.end_s - b.start_s - (22.0 - 21.16)).abs() < 1e-6);
    }

    #[test]
    fn comp_takes_words_from_two_takes() {
        let (mut d, sc) = doc2();
        d.takes.insert(
            "tb2".into(),
            take(
                "tb2",
                "b",
                40.0,
                42.0,
                vec![word("three", 40.1, 40.5, 0), word("four", 41.0, 41.6, 1)],
            ),
        );
        d.boxes.get_mut("b").unwrap().comp = vec![
            CompSegment {
                take_id: "tb".into(),
                from_word: 0,
                to_word: 1,
                nudge_s: 0.0,
            },
            CompSegment {
                take_id: "tb2".into(),
                from_word: 1,
                to_word: 2,
                nudge_s: 0.0,
            },
        ];
        let l = layout_scene(&d, &sc, &Pacing::default());
        let b = l.item("b").unwrap();
        assert_eq!(b.pieces.len(), 2);
        assert_eq!(b.pieces[0].take_id, "tb");
        assert_eq!(b.pieces[1].take_id, "tb2");
        // splice in the middle of the pauses: tb 20.0..21.0, tb2 40.75..42.0
        assert!((b.pieces[0].src_out - 21.0).abs() < 1e-6);
        assert!((b.pieces[1].src_in - 40.75).abs() < 1e-6);
        assert_eq!(b.words.len(), 2);
    }

    #[test]
    fn anchors_follow_their_line() {
        let (mut d, sc) = doc2();
        let p = Pacing::default();
        let pl = Placement {
            id: "p".into(),
            scene_id: "sc".into(),
            asset_id: "as".into(),
            cue_id: None,
            lane: Lane::Sfx,
            character: None,
            layer: 0,
            anchor: Some(Anchor {
                line_id: "b".into(),
                word: Some(1),
                edge: Edge::Start,
            }),
            at_s: 0.0,
            offset_s: 0.1,
            in_s: 0.0,
            len_s: 1.0,
            fade_in_s: 0.0,
            fade_out_s: 0.0,
            fade_curve: Default::default(),
            gain_db: 0.0,
        };
        let before = placement_start(&layout_scene(&d, &sc, &p), &pl);
        d.boxes.get_mut("b").unwrap().gap_s = Some(2.0);
        let after = placement_start(&layout_scene(&d, &sc, &p), &pl);
        assert!((after - before - (2.0 - 0.8)).abs() < 1e-9);
        let mut pinned = pl.clone();
        pinned.anchor = None;
        pinned.at_s = 3.0;
        assert!((placement_start(&layout_scene(&d, &sc, &p), &pinned) - 3.1).abs() < 1e-9);
    }
}
