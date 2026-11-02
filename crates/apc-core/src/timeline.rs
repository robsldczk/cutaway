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
    /// Part of the line (0 before the first split).
    #[serde(default)]
    pub part: usize,
    /// Silent but in place (clip mute).
    #[serde(default)]
    pub muted: bool,
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
    #[serde(default)]
    pub part: usize,
    #[serde(default)]
    pub muted: bool,
}

/// A part of a line between splits (the whole line when it has none), in scene time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutPart {
    pub index: usize,
    /// Index in `BoxState::split_points` of the split before this part (`None` for part 0).
    pub split: Option<usize>,
    pub start_s: f64,
    pub end_s: f64,
    pub muted: bool,
    /// Ripple-deleted: no length, sits where it was cut out.
    pub dropped: bool,
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
    #[serde(default)]
    pub parts: Vec<LayoutPart>,
    /// Every part muted.
    #[serde(default)]
    pub muted: bool,
    /// Every part dropped: the line is out of the cut.
    #[serde(default)]
    pub dropped: bool,
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
struct BuiltBox {
    pieces: Vec<ClipPiece>,
    words: Vec<LayoutWord>,
    parts: Vec<LayoutPart>,
    dur: f64,
}

/// Shortest part a split may leave.
pub const MIN_PART_S: f64 = 0.01;

fn build_box(doc: &Document, b: &BoxState, pacing: &Pacing) -> BuiltBox {
    let mut spans = spans_for(doc, b);
    if spans.is_empty() {
        return BuiltBox {
            pieces: Vec::new(),
            words: Vec::new(),
            parts: Vec::new(),
            dur: 0.0,
        };
    }
    // Head and tail trims apply to the first and last spans.
    if let Some(f) = spans.first_mut() {
        f.src_in += b.head_s;
    }
    if let Some(l) = spans.last_mut() {
        l.src_out -= b.tail_s;
    }

    // Split points inside a span, in play order: (span, source time, split index).
    let mut bounds: Vec<(usize, f64, usize)> = Vec::new();
    for (k, sp) in b.split_points.iter().enumerate() {
        if let Some(si) = spans.iter().position(|s| {
            s.take_id == sp.take_id
                && sp.src_s > s.src_in + MIN_PART_S
                && sp.src_s < s.src_out - MIN_PART_S
        }) {
            bounds.push((si, sp.src_s, k));
        }
    }
    bounds.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    bounds.dedup_by(|a, b| a.0 == b.0 && (a.1 - b.1).abs() < MIN_PART_S);
    // (offset, muted, dropped) of a part
    let state = |part: usize| -> (f64, bool, bool) {
        match part.checked_sub(1).and_then(|k| bounds.get(k)) {
            Some(&(_, _, k)) => {
                let sp = &b.split_points[k];
                (sp.offset_s, sp.muted, sp.dropped)
            }
            None => (0.0, b.muted, b.dropped),
        }
    };
    let part_of = |si: usize, t: f64| {
        bounds
            .iter()
            .filter(|(bs, bt, _)| *bs < si || (*bs == si && *bt <= t + 1e-9))
            .count()
    };

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

    // Trims at a split cut audio on either side of it.
    for &(si, t, k) in &bounds {
        let sp = &b.split_points[k];
        if sp.trim_before_s > 0.0 {
            cuts.push((si, (t - sp.trim_before_s).max(spans[si].src_in), t));
        }
        if sp.trim_after_s > 0.0 {
            cuts.push((si, t, (t + sp.trim_after_s).min(spans[si].src_out)));
        }
    }

    // Split each span by its cuts (and split points) into pieces and lay them end to end.
    let mut pieces: Vec<ClipPiece> = Vec::new();
    let mut piece_span: Vec<usize> = Vec::new();
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
        for &(_, bt, _) in bounds.iter().filter(|x| x.0 == si) {
            segs = segs
                .into_iter()
                .flat_map(|(a, z)| {
                    if bt > a + 1e-9 && bt < z - 1e-9 {
                        vec![(a, bt), (bt, z)]
                    } else {
                        vec![(a, z)]
                    }
                })
                .collect();
        }
        for (a, z) in segs {
            if z - a < 0.005 {
                continue;
            }
            let part = part_of(si, a);
            // parts meet end to end; the part pass below places them
            let overlap = match pieces.last() {
                Some(l) if l.part == part => pacing.splice_fade_s,
                _ => 0.0,
            };
            let start = (cursor - overlap).max(0.0);
            pieces.push(ClipPiece {
                source_id: s.source_id.clone(),
                take_id: s.take_id.clone(),
                src_in: a,
                src_out: z,
                start_s: start,
                fade_in_s: pacing.splice_fade_s,
                fade_out_s: pacing.splice_fade_s,
                gain_db: b.gain_db,
                part,
                muted: state(part).1,
            });
            piece_span.push(si);
            cursor = start + (z - a);
        }
    }

    // Parts: each starts at the end of the last kept part plus its offset; dropped parts
    // close up. Pieces are in part order, so one pass does it.
    let n_parts = bounds.len() + 1;
    let mut kept: Vec<ClipPiece> = Vec::new();
    let mut kept_span: Vec<usize> = Vec::new();
    let mut parts = Vec::with_capacity(n_parts);
    let mut end_prev: Option<f64> = None;
    for part in 0..n_parts {
        let (offset, muted, dropped) = state(part);
        let own: Vec<usize> = (0..pieces.len())
            .filter(|&i| pieces[i].part == part)
            .collect();
        let at = end_prev.unwrap_or(0.0);
        let split = part.checked_sub(1).map(|k| bounds[k].2);
        let Some(&first) = own.first() else {
            parts.push(LayoutPart {
                index: part,
                split,
                start_s: at,
                end_s: at,
                muted,
                dropped,
            });
            continue;
        };
        if dropped {
            parts.push(LayoutPart {
                index: part,
                split,
                start_s: at,
                end_s: at,
                muted,
                dropped,
            });
            continue;
        }
        let new_start = match end_prev {
            None => 0.0,
            Some(e) => (e + offset).max(0.0),
        };
        let delta = new_start - pieces[first].start_s;
        let mut end = new_start;
        for i in own {
            let mut p = pieces[i].clone();
            p.start_s += delta;
            end = end.max(p.end_s());
            kept.push(p);
            kept_span.push(piece_span[i]);
        }
        parts.push(LayoutPart {
            index: part,
            split,
            start_s: new_start,
            end_s: end,
            muted,
            dropped,
        });
        end_prev = Some(end);
    }
    // Fades: edges of the line, and part edges unless the parts still play as one.
    for i in 0..kept.len() {
        if i == 0 {
            kept[i].fade_in_s = pacing.edge_fade_s;
        }
        if i + 1 == kept.len() {
            kept[i].fade_out_s = pacing.edge_fade_s;
            continue;
        }
        let (p, n) = (&kept[i], &kept[i + 1]);
        if p.part == n.part {
            continue;
        }
        let seamless = p.source_id == n.source_id
            && (p.src_out - n.src_in).abs() < 1e-6
            && (p.end_s() - n.start_s).abs() < 1e-6
            && p.muted == n.muted;
        let f = if seamless { 0.0 } else { pacing.edge_fade_s };
        kept[i].fade_out_s = f;
        kept[i + 1].fade_in_s = f;
    }
    // The line's own fades at its first and last piece.
    if let Some(f) = kept.first_mut() {
        f.fade_in_s = f.fade_in_s.max(b.fade_in_s.min(f.len() / 2.0));
    }
    if let Some(l) = kept.last_mut() {
        l.fade_out_s = l.fade_out_s.max(b.fade_out_s.min(l.len() / 2.0));
    }
    let dur = kept.iter().map(|p| p.end_s()).fold(0.0_f64, f64::max);

    // Place words in box time.
    let mut out_words = Vec::new();
    for (idx, w) in words.iter().enumerate() {
        let hit = (0..kept.len())
            .find(|&j| {
                kept_span[j] == w.span && w.t0 >= kept[j].src_in - 1e-6 && w.t0 < kept[j].src_out
            })
            .map(|j| &kept[j]);
        let (start, end) = match hit {
            Some(p) => (
                p.start_s + (w.t0 - p.src_in),
                p.start_s + (w.t1.min(p.src_out) - p.src_in),
            ),
            None => {
                // deleted, trimmed or dropped: place it at the nearest cut point
                let prev = (0..kept.len()).rfind(|&j| {
                    kept_span[j] < w.span || (kept_span[j] == w.span && kept[j].src_in <= w.t0)
                });
                let t = prev.map(|j| kept[j].end_s()).unwrap_or(0.0);
                (t, t)
            }
        };
        let part = part_of(w.span, w.t0);
        out_words.push(LayoutWord {
            idx,
            text: w.text.clone(),
            take_id: spans[w.span].take_id.clone(),
            start_s: start,
            end_s: end,
            deleted: deleted[idx] || hit.is_none(),
            script_idx: w.script_idx,
            part,
            muted: state(part).1,
        });
    }
    BuiltBox {
        pieces: kept,
        words: out_words,
        parts,
        dur,
    }
}

/// Lays out one scene.
pub fn layout_scene(doc: &Document, scene: &Scene, pacing: &Pacing) -> SceneLayout {
    let mut items = Vec::new();
    let mut cursor = 0.0;
    let mut pending_spacers = 0.0;
    let mut first_box = true;
    // start of the last chained box: a default overlap never reaches back past it
    let mut prev_start = 0.0;
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
                parts: Vec::new(),
                muted: false,
                dropped: false,
            });
            continue;
        }
        let b = doc.boxes.get(item_id).cloned().unwrap_or(BoxState {
            id: item_id.clone(),
            ..Default::default()
        });
        let BuiltBox {
            mut pieces,
            mut words,
            mut parts,
            dur,
        } = build_box(doc, &b, pacing);
        let dropped = !parts.is_empty() && parts.iter().all(|p| p.dropped);
        let muted = parts.iter().any(|p| !p.dropped) && parts.iter().all(|p| p.dropped || p.muted);
        if pieces.is_empty() {
            for p in parts.iter_mut() {
                p.start_s = cursor;
                p.end_s = cursor;
            }
            for w in words.iter_mut() {
                w.start_s = cursor;
                w.end_s = cursor;
            }
            items.push(LayoutItem {
                item_id: item_id.clone(),
                kind: ItemKind::Box,
                start_s: cursor,
                end_s: cursor,
                gap_before_s: 0.0,
                gap_overridden: false,
                pinned: false,
                character: line.character.clone(),
                take_id: if dropped { b.take_id.clone() } else { None },
                pieces,
                words,
                parts,
                muted,
                dropped,
            });
            continue;
        }
        let (gap, overridden) = if first_box {
            // the first locked line starts the scene unless it was moved later
            (b.gap_s.unwrap_or(0.0).max(0.0), b.gap_s.is_some())
        } else {
            match b.gap_s {
                Some(g) => (g, true),
                None => {
                    // tighter = shorter pauses and, below zero, deeper overlaps
                    let base = pacing.breath_gap_s + pending_spacers;
                    let t = scene.tightness.max(0.1);
                    let g = if base >= 0.0 { base * t } else { base / t };
                    (g.max(prev_start - cursor), false)
                }
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
        for p in parts.iter_mut() {
            p.start_s += start;
            p.end_s += start;
        }
        let end = start + dur;
        items.push(LayoutItem {
            item_id: item_id.clone(),
            kind: ItemKind::Box,
            start_s: start,
            end_s: end,
            gap_before_s: start - cursor,
            gap_overridden: overridden,
            pinned: b.pinned_s.is_some(),
            character: line.character.clone(),
            take_id: b.take_id.clone(),
            pieces,
            words,
            parts,
            muted,
            dropped,
        });
        // Pinned (unlocked) boxes are out of the chain: the next box follows the last
        // locked one, and spacers before a pinned box still count for it.
        if b.pinned_s.is_none() {
            prev_start = start;
            cursor = end;
            pending_spacers = 0.0;
            first_box = false;
        }
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
            story: Default::default(),
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
    fn negative_breath_gap_overlaps_more_when_tighter() {
        let (mut d, mut sc) = doc2();
        sc.order = vec!["a".into(), "b".into()];
        d.spacers.clear();
        let p = Pacing {
            breath_gap_s: -0.2,
            ..Pacing::default()
        };
        let gap = |sc: &Scene| layout_scene(&d, sc, &p).item("b").unwrap().gap_before_s;
        assert!((gap(&sc) + 0.2).abs() < 1e-9);
        sc.tightness = 0.5;
        assert!((gap(&sc) + 0.4).abs() < 1e-9);
        // never back past the start of the line before ("a" is 1 s long)
        sc.tightness = 0.1;
        assert!((gap(&sc) + 1.0).abs() < 1e-9);
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

    fn split_b(d: &mut Document, src_s: f64) -> &mut SplitPoint {
        let b = d.boxes.get_mut("b").unwrap();
        b.split_points.push(SplitPoint {
            take_id: "tb".into(),
            src_s,
            ..Default::default()
        });
        b.split_points.last_mut().unwrap()
    }

    #[test]
    fn a_split_alone_plays_the_same() {
        let (mut d, sc) = doc2();
        let p = Pacing::default();
        let before = layout_scene(&d, &sc, &p).item("b").unwrap().clone();
        split_b(&mut d, 21.0);
        let l = layout_scene(&d, &sc, &p);
        let b = l.item("b").unwrap();
        assert_eq!(b.pieces.len(), 2);
        assert_eq!(b.parts.len(), 2);
        assert!((b.pieces[0].src_out - 21.0).abs() < 1e-9);
        assert!((b.pieces[1].start_s - b.pieces[0].end_s()).abs() < 1e-9);
        // one stretch of audio: no fades at the split
        assert_eq!(b.pieces[0].fade_out_s, 0.0);
        assert_eq!(b.pieces[1].fade_in_s, 0.0);
        assert!((b.end_s - before.end_s).abs() < 1e-9);
        assert_eq!(b.words[1].part, 1);
        assert!((b.words[1].start_s - before.words[1].start_s).abs() < 1e-9);
    }

    #[test]
    fn parts_move_by_their_offset_and_close_up_when_dropped() {
        let (mut d, sc) = doc2();
        let p = Pacing::default();
        let before = layout_scene(&d, &sc, &p).item("b").unwrap().clone();
        split_b(&mut d, 21.0).offset_s = 0.5;
        let b = layout_scene(&d, &sc, &p).item("b").unwrap().clone();
        assert!((b.end_s - before.end_s - 0.5).abs() < 1e-9);
        assert!((b.words[1].start_s - before.words[1].start_s - 0.5).abs() < 1e-9);
        assert!(b.pieces[1].fade_in_s > 0.0);
        d.boxes.get_mut("b").unwrap().split_points[0].dropped = true;
        let b = layout_scene(&d, &sc, &p).item("b").unwrap().clone();
        assert_eq!(b.pieces.len(), 1);
        assert!((b.end_s - b.start_s - 1.0).abs() < 1e-9);
        assert!(b.words[1].deleted);
        assert!(b.parts[1].dropped);
        assert!(!b.dropped);
        d.boxes.get_mut("b").unwrap().dropped = true;
        let l = layout_scene(&d, &sc, &p);
        let b = l.item("b").unwrap();
        assert!(b.dropped && b.pieces.is_empty());
        assert!((l.duration_s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn splits_survive_a_head_trim() {
        let (mut d, sc) = doc2();
        split_b(&mut d, 21.0);
        d.boxes.get_mut("b").unwrap().head_s = 0.15;
        let l = layout_scene(&d, &sc, &Pacing::default());
        let b = l.item("b").unwrap();
        assert!((b.pieces[0].src_in - 20.15).abs() < 1e-9);
        assert!((b.pieces[1].src_in - 21.0).abs() < 1e-9);
    }

    #[test]
    fn trims_at_a_split_cut_audio_on_either_side() {
        let (mut d, sc) = doc2();
        let sp = split_b(&mut d, 21.0);
        sp.trim_before_s = 0.1;
        sp.trim_after_s = 0.1;
        let l = layout_scene(&d, &sc, &Pacing::default());
        let b = l.item("b").unwrap();
        assert!((b.pieces[0].src_out - 20.9).abs() < 1e-9);
        assert!((b.pieces[1].src_in - 21.1).abs() < 1e-9);
        assert!((b.end_s - b.start_s - 1.8).abs() < 1e-9);
    }

    #[test]
    fn muted_parts_keep_their_time() {
        let (mut d, sc) = doc2();
        let p = Pacing::default();
        let before = layout_scene(&d, &sc, &p);
        split_b(&mut d, 21.0).muted = true;
        let l = layout_scene(&d, &sc, &p);
        let b = l.item("b").unwrap();
        assert!(!b.pieces[0].muted && b.pieces[1].muted);
        assert!(b.words[1].muted && !b.words[1].deleted);
        assert!(!b.muted);
        assert!((l.duration_s - before.duration_s).abs() < 1e-9);
        d.boxes.get_mut("b").unwrap().muted = true;
        assert!(layout_scene(&d, &sc, &p).item("b").unwrap().muted);
    }

    #[test]
    fn pinned_clips_are_out_of_the_chain() {
        let (mut d, sc) = doc2();
        let p = Pacing::default();
        d.boxes.get_mut("a").unwrap().pinned_s = Some(6.0);
        let l = layout_scene(&d, &sc, &p);
        // b now opens the chain
        assert!(l.item("b").unwrap().start_s.abs() < 1e-9);
        assert!((l.item("a").unwrap().start_s - 6.0).abs() < 1e-9);
        assert!((l.duration_s - 7.0).abs() < 1e-9);
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
            muted: false,
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
