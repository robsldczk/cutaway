//! Review notes: what people who don't edit say about the cut.
//!
//! Notes aren't rows that change. Every note, reply, edit, tag, resolve and reopen is one event,
//! and a note's state is folded from them (D136). So the same events can arrive by a shared
//! folder or a file, twice or out of order, and settle on the same state: merging is the union
//! by id, and for each field the latest clock wins.
//!
//! A note hangs on an element of the cut, never on a time: where it is now is worked out from
//! the layout, so it follows reordering, take swaps and gap changes as a glued sound does.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{CoreError, Result};
use crate::model::{Anchor, Document, Edge, Entity, Kind};
use crate::timeline::{placement_start, resolve_anchor, ItemKind, LayoutItem, SceneLayout};

/// The `.cutawayreview` format this build writes and reads.
pub const REVIEW_FORMAT: u32 = 1;

/// What a person does on the production, as they say it themselves.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRole {
    Director,
    Producer,
    Writer,
    Actor,
    Client,
    /// Whoever cuts the show: replies and resolves from the editing pages.
    Editor,
    #[default]
    Other,
}

impl ReviewRole {
    pub fn label(self) -> &'static str {
        match self {
            ReviewRole::Director => "Director",
            ReviewRole::Producer => "Producer",
            ReviewRole::Writer => "Writer",
            ReviewRole::Actor => "Actor",
            ReviewRole::Client => "Client",
            ReviewRole::Editor => "Editor",
            ReviewRole::Other => "Other",
        }
    }
}

/// Who wrote something. Every event carries it whole, so a note keeps its author wherever it goes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Reviewer {
    pub name: String,
    #[serde(default)]
    pub role: ReviewRole,
    #[serde(default)]
    pub initials: String,
}

impl Reviewer {
    pub fn new(name: &str, role: ReviewRole) -> Self {
        let name = name.trim().to_string();
        let initials = initials_of(&name);
        Reviewer {
            name,
            role,
            initials,
        }
    }

    /// The name of this person's rolling file: `anna-weber`.
    pub fn slug(&self) -> String {
        let mut out = String::new();
        for c in self.name.to_lowercase().chars() {
            if c.is_alphanumeric() {
                out.push(c);
            } else if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        }
        let out = out.trim_end_matches('-').to_string();
        if out.is_empty() {
            "reviewer".into()
        } else {
            out
        }
    }
}

/// `AW` for Anna Weber, `AL` for Albert.
pub fn initials_of(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = |w: &str| w.chars().next().map(|c| c.to_uppercase().to_string());
    match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect::<String>().to_uppercase(),
        [a, .., b] => format!(
            "{}{}",
            first(a).unwrap_or_default(),
            first(b).unwrap_or_default()
        ),
    }
}

/// What a note is about. Kept short on purpose: these are the filters above the notes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteTag {
    Performance,
    Timing,
    Sound,
    Music,
}

/// What a note hangs on. Word numbers are the script's, so they hold across takes, exactly as
/// a glued sound's do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(tag = "on", rename_all = "snake_case")]
pub enum NoteAnchor {
    Scene {
        scene_id: String,
    },
    Line {
        scene_id: String,
        line_id: String,
    },
    /// One word (`from` = `to`), or a run of them. A run dragged over several lines ends on
    /// `to_line`.
    Words {
        scene_id: String,
        line_id: String,
        from: usize,
        to: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_line: Option<String>,
    },
    /// The pause after a line; `at` is how far into it, 0 … 1.
    Pause {
        scene_id: String,
        line_id: String,
        #[serde(default)]
        at: f64,
    },
    Sound {
        scene_id: String,
        placement_id: String,
    },
}

impl NoteAnchor {
    pub fn scene_id(&self) -> &str {
        match self {
            NoteAnchor::Scene { scene_id }
            | NoteAnchor::Line { scene_id, .. }
            | NoteAnchor::Words { scene_id, .. }
            | NoteAnchor::Pause { scene_id, .. }
            | NoteAnchor::Sound { scene_id, .. } => scene_id,
        }
    }

    pub fn line_id(&self) -> Option<&str> {
        match self {
            NoteAnchor::Line { line_id, .. }
            | NoteAnchor::Words { line_id, .. }
            | NoteAnchor::Pause { line_id, .. } => Some(line_id),
            _ => None,
        }
    }

    pub fn placement_id(&self) -> Option<&str> {
        match self {
            NoteAnchor::Sound { placement_id, .. } => Some(placement_id),
            _ => None,
        }
    }

    /// `word`, `words`, `line`, `pause`, `sound` or `scene`.
    pub fn kind(&self) -> &'static str {
        match self {
            NoteAnchor::Scene { .. } => "scene",
            NoteAnchor::Line { .. } => "line",
            NoteAnchor::Words {
                from, to, to_line, ..
            } if from == to && to_line.is_none() => "word",
            NoteAnchor::Words { .. } => "words",
            NoteAnchor::Pause { .. } => "pause",
            NoteAnchor::Sound { .. } => "sound",
        }
    }

    /// Two anchors on the same element, with ranges that touch: their notes are one thread.
    pub fn shares_element(&self, other: &NoteAnchor) -> bool {
        use NoteAnchor::*;
        let span = |a: &NoteAnchor| match a {
            Words {
                from, to, to_line, ..
            } => (*from, if to_line.is_some() { usize::MAX } else { *to }),
            _ => (0, usize::MAX),
        };
        match (self, other) {
            (Scene { scene_id: a }, Scene { scene_id: b }) => a == b,
            (
                Sound {
                    placement_id: a, ..
                },
                Sound {
                    placement_id: b, ..
                },
            ) => a == b,
            (Pause { line_id: a, .. }, Pause { line_id: b, .. }) => a == b,
            (
                Line { line_id: a, .. } | Words { line_id: a, .. },
                Line { line_id: b, .. } | Words { line_id: b, .. },
            ) => {
                let ((a0, a1), (b0, b1)) = (span(self), span(other));
                a == b && a0 <= b1 && b0 <= a1
            }
            _ => false,
        }
    }
}

/// What happened. A file from a newer build may hold kinds this one doesn't know: those fail
/// the whole file, with a message, rather than being half read.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewBody {
    /// A new note. `quote`, `speaker` and `written_s` are what's left of it if its element goes;
    /// `print` is how the element stood, for *changed since note*.
    Note {
        text: String,
        #[serde(default)]
        tag: Option<NoteTag>,
        #[serde(default)]
        quote: String,
        #[serde(default)]
        speaker: String,
        #[serde(default)]
        print: String,
        #[serde(default)]
        written_s: f64,
    },
    Reply {
        text: String,
    },
    Edit {
        text: String,
    },
    Tag {
        #[serde(default)]
        tag: Option<NoteTag>,
    },
    Resolve,
    Reopen,
    /// An undo (`on`) or a redo of an event that may already be on other machines: events are
    /// never taken back, so what's undone is said (D137).
    Retract {
        target: String,
        on: bool,
    },
}

/// One thing that happened to a note. Never changed once written.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReviewEvent {
    pub id: String,
    /// The episode it belongs to. A show's rolling file holds every episode a person reviewed.
    pub episode: String,
    pub note: String,
    #[serde(default)]
    pub round: Option<String>,
    pub author: Reviewer,
    #[serde(default)]
    pub device: String,
    /// Hybrid logical clock, see [`Hlc`].
    pub clock: String,
    pub anchor: NoteAnchor,
    #[serde(flatten)]
    pub body: ReviewBody,
}

impl Entity for ReviewEvent {
    const KIND: Kind = Kind::ReviewEvent;
    fn id(&self) -> &str {
        &self.id
    }
    fn parent(&self) -> Option<&str> {
        Some(&self.note)
    }
}

/// A scene as it stood when a round began.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RoundScene {
    pub scene_id: String,
    /// The export this scene was last delivered as, if any.
    #[serde(default)]
    pub version: Option<u32>,
    pub timeline_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    #[default]
    Open,
    Closed,
}

/// A wave of review: which scenes, on which cut. Earlier ones stay.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReviewRound {
    pub id: String,
    /// The episode it belongs to: a show's rolling file carries rounds of several.
    #[serde(default)]
    pub episode: String,
    pub number: u32,
    pub created_by: Reviewer,
    /// RFC 3339.
    pub created_at: String,
    pub scenes: Vec<RoundScene>,
    #[serde(default)]
    pub status: RoundStatus,
}

impl Entity for ReviewRound {
    const KIND: Kind = Kind::ReviewRound;
    fn id(&self) -> &str {
        &self.id
    }
    fn ord(&self) -> f64 {
        self.number as f64
    }
}

/// How a review came in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalVia {
    Folder,
    File,
}

/// New events arriving from someone, for the Review history.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReviewArrival {
    pub id: String,
    /// RFC 3339.
    pub at: String,
    pub who: String,
    pub via: ArrivalVia,
    pub notes: usize,
    pub replies: usize,
    /// Edits, tags, resolves, reopens.
    pub changes: usize,
}

impl Entity for ReviewArrival {
    const KIND: Kind = Kind::ReviewArrival;
    fn id(&self) -> &str {
        &self.id
    }
}

// ---- the clock ----

/// A hybrid logical clock: wall time, a counter for what happens within one millisecond or behind
/// a clock that runs late, and the device, so two events never tie. It's kept as a string that
/// sorts in clock order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hlc {
    pub ms: u64,
    pub count: u32,
    pub device: String,
}

impl Hlc {
    pub fn parse(s: &str) -> Option<Hlc> {
        let mut p = s.splitn(3, '.');
        Some(Hlc {
            ms: p.next()?.parse().ok()?,
            count: p.next()?.parse().ok()?,
            device: p.next().unwrap_or("").to_string(),
        })
    }

    pub fn encode(&self) -> String {
        format!("{:015}.{:06}.{}", self.ms, self.count, self.device)
    }

    /// The next tick after everything seen so far.
    pub fn next(seen: Option<&str>, now_ms: u64, device: &str) -> Hlc {
        let last = seen.and_then(Hlc::parse);
        match last {
            Some(l) if l.ms >= now_ms => Hlc {
                ms: l.ms,
                count: l.count + 1,
                device: device.to_string(),
            },
            _ => Hlc {
                ms: now_ms,
                count: 0,
                device: device.to_string(),
            },
        }
    }
}

/// The wall time in a clock, as RFC 3339.
pub fn clock_time(clock: &str) -> String {
    Hlc::parse(clock)
        .and_then(|h| chrono::DateTime::from_timestamp_millis(h.ms as i64))
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// The newest clock in the document: the next event has to come after it.
pub fn latest_clock(doc: &Document) -> Option<&str> {
    doc.review_events.values().map(|e| e.clock.as_str()).max()
}

// ---- folding ----

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteStatus {
    Open,
    Resolved,
    Reopened,
}

impl NoteStatus {
    pub fn is_open(self) -> bool {
        self != NoteStatus::Resolved
    }
}

/// Who did something, and when.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Stamp {
    pub by: Reviewer,
    /// RFC 3339.
    pub at: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FoldedReply {
    pub id: String,
    pub author: Reviewer,
    pub text: String,
    pub at: String,
}

/// A note as all its events add up.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FoldedNote {
    pub id: String,
    pub round: Option<String>,
    pub author: Reviewer,
    pub anchor: NoteAnchor,
    pub text: String,
    pub tag: Option<NoteTag>,
    pub status: NoteStatus,
    pub quote: String,
    pub speaker: String,
    pub print: String,
    pub written_s: f64,
    pub created_at: String,
    pub resolved: Option<Stamp>,
    pub reopened: Option<Stamp>,
    pub replies: Vec<FoldedReply>,
}

fn stamp(e: &ReviewEvent) -> Stamp {
    Stamp {
        by: e.author.clone(),
        at: clock_time(&e.clock),
    }
}

/// Every note, oldest first. A note whose first event hasn't arrived yet waits for it.
pub fn fold_notes(doc: &Document) -> Vec<FoldedNote> {
    let mut by_clock: Vec<&ReviewEvent> = doc.review_events.values().collect();
    by_clock.sort_by(|a, b| (&a.clock, &a.id).cmp(&(&b.clock, &b.id)));

    // the last word on each event: undone, or brought back
    let mut retracted: HashMap<&str, bool> = HashMap::new();
    for e in &by_clock {
        if let ReviewBody::Retract { target, on } = &e.body {
            retracted.insert(target, *on);
        }
    }
    let live = |e: &&&ReviewEvent| !retracted.get(e.id.as_str()).copied().unwrap_or(false);

    let mut notes: BTreeMap<&str, FoldedNote> = BTreeMap::new();
    for e in by_clock.iter().filter(live) {
        if let ReviewBody::Note {
            text,
            tag,
            quote,
            speaker,
            print,
            written_s,
        } = &e.body
        {
            notes.insert(
                &e.note,
                FoldedNote {
                    id: e.note.clone(),
                    round: e.round.clone(),
                    author: e.author.clone(),
                    anchor: e.anchor.clone(),
                    text: text.clone(),
                    tag: *tag,
                    status: NoteStatus::Open,
                    quote: quote.clone(),
                    speaker: speaker.clone(),
                    print: print.clone(),
                    written_s: *written_s,
                    created_at: clock_time(&e.clock),
                    resolved: None,
                    reopened: None,
                    replies: Vec::new(),
                },
            );
        }
    }
    for e in by_clock.iter().filter(live) {
        let Some(n) = notes.get_mut(e.note.as_str()) else {
            continue;
        };
        match &e.body {
            ReviewBody::Reply { text } => n.replies.push(FoldedReply {
                id: e.id.clone(),
                author: e.author.clone(),
                text: text.clone(),
                at: clock_time(&e.clock),
            }),
            ReviewBody::Edit { text } => n.text = text.clone(),
            ReviewBody::Tag { tag } => n.tag = *tag,
            ReviewBody::Resolve => {
                n.status = NoteStatus::Resolved;
                n.resolved = Some(stamp(e));
            }
            ReviewBody::Reopen => {
                n.status = NoteStatus::Reopened;
                n.reopened = Some(stamp(e));
            }
            ReviewBody::Note { .. } | ReviewBody::Retract { .. } => {}
        }
    }
    let mut out: Vec<FoldedNote> = notes.into_values().collect();
    out.sort_by(|a, b| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)));
    out
}

/// Notes on the same element, or on ranges of it that touch, as thread numbers: index `i` of the
/// result is the thread of `notes[i]`, named by its first note.
pub fn threads(notes: &[FoldedNote]) -> Vec<usize> {
    let mut of: Vec<usize> = (0..notes.len()).collect();
    fn root(of: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while of[r] != r {
            r = of[r];
        }
        of[i] = r;
        r
    }
    for i in 0..notes.len() {
        for j in 0..i {
            if notes[i].anchor.shares_element(&notes[j].anchor) {
                let (a, b) = (root(&mut of, i), root(&mut of, j));
                let (lo, hi) = (a.min(b), a.max(b));
                of[hi] = lo;
            }
        }
    }
    (0..notes.len()).map(|i| root(&mut of, i)).collect()
}

// ---- where a note is now ----

/// A note's place on the cut, in scene seconds. `end` is set for a range.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct NoteSpot {
    pub start_s: f64,
    pub end_s: Option<f64>,
}

fn word_edge(layout: &SceneLayout, line_id: &str, word: usize, edge: Edge) -> Option<f64> {
    resolve_anchor(
        layout,
        &Anchor {
            line_id: line_id.to_string(),
            word: Some(word),
            edge,
        },
    )
}

/// The next spoken line after this one, in layout order.
fn following<'a>(layout: &'a SceneLayout, line_id: &str) -> Option<&'a LayoutItem> {
    let i = layout.items.iter().position(|x| x.item_id == line_id)?;
    layout.items[i + 1..]
        .iter()
        .find(|x| x.kind == ItemKind::Box && !x.pieces.is_empty())
}

/// Where a note is now, or `None` when its element has left the cut: it's detached.
pub fn locate(doc: &Document, layout: &SceneLayout, anchor: &NoteAnchor) -> Option<NoteSpot> {
    match anchor {
        NoteAnchor::Scene { .. } => Some(NoteSpot {
            start_s: 0.0,
            end_s: None,
        }),
        NoteAnchor::Line { line_id, .. } => {
            let i = spoken(layout, line_id)?;
            Some(NoteSpot {
                start_s: i.start_s,
                end_s: Some(i.end_s),
            })
        }
        NoteAnchor::Words {
            line_id,
            from,
            to,
            to_line,
            ..
        } => {
            spoken(layout, line_id)?;
            let start_s = word_edge(layout, line_id, *from, Edge::Start)?;
            // one word is a marker: a place, not a stretch
            if from == to && to_line.is_none() {
                return Some(NoteSpot {
                    start_s,
                    end_s: None,
                });
            }
            // the far end can leave the cut on its own: what's left of the run still stands
            let last = to_line.as_deref().unwrap_or(line_id);
            let end_s = word_edge(layout, last, *to, Edge::End)
                .or_else(|| word_edge(layout, line_id, *to, Edge::End));
            Some(NoteSpot { start_s, end_s })
        }
        NoteAnchor::Pause { line_id, at, .. } => {
            let i = spoken(layout, line_id)?;
            let until = following(layout, line_id)
                .map(|n| n.start_s)
                .unwrap_or(layout.duration_s);
            Some(NoteSpot {
                start_s: i.end_s + (until - i.end_s).max(0.0) * at.clamp(0.0, 1.0),
                end_s: None,
            })
        }
        NoteAnchor::Sound { placement_id, .. } => {
            let p = doc.placements.get(placement_id)?;
            let start_s = placement_start(layout, p);
            Some(NoteSpot {
                start_s,
                end_s: Some(start_s + p.len_s),
            })
        }
    }
}

fn spoken<'a>(layout: &'a SceneLayout, line_id: &str) -> Option<&'a LayoutItem> {
    layout
        .item(line_id)
        .filter(|i| i.kind == ItemKind::Box && !i.pieces.is_empty() && !i.dropped)
}

/// What a moment or a stretch of the scene is, as a note would hang on it: the word being said,
/// the run of words between two points, or the pause they fall in.
pub fn anchor_at(layout: &SceneLayout, t0: f64, t1: Option<f64>) -> NoteAnchor {
    let scene_id = layout.scene_id.clone();
    let lines: Vec<&LayoutItem> = layout
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Box && !i.pieces.is_empty() && i.end_s > i.start_s)
        .collect();
    let (a, b) = match t1 {
        Some(t) if (t - t0).abs() > 1e-3 => (t0.min(t), t0.max(t)),
        _ => (t0, t0),
    };
    if a < b {
        // the lines it touches; a stretch that's all pause is a moment of that pause
        let touched: Vec<&&LayoutItem> = lines
            .iter()
            .filter(|l| l.start_s < b && a < l.end_s)
            .collect();
        if let (Some(first), Some(last)) = (touched.first(), touched.last()) {
            let from = first_word(first, a);
            let to = last_word(last, b);
            return match (from, to) {
                (Some(from), Some(to)) => NoteAnchor::Words {
                    scene_id,
                    line_id: first.item_id.clone(),
                    from,
                    to,
                    to_line: (first.item_id != last.item_id).then(|| last.item_id.clone()),
                },
                _ => NoteAnchor::Line {
                    scene_id,
                    line_id: first.item_id.clone(),
                },
            };
        }
    }
    if let Some(line) = lines
        .iter()
        .filter(|l| l.start_s <= a && a < l.end_s)
        .max_by(|x, y| x.start_s.total_cmp(&y.start_s))
    {
        return match first_word_at(line, a) {
            Some(w) => NoteAnchor::Words {
                scene_id,
                line_id: line.item_id.clone(),
                from: w,
                to: w,
                to_line: None,
            },
            None => NoteAnchor::Line {
                scene_id,
                line_id: line.item_id.clone(),
            },
        };
    }
    let before = lines
        .iter()
        .filter(|l| l.end_s <= a)
        .max_by(|x, y| x.end_s.total_cmp(&y.end_s));
    match before {
        Some(l) => {
            let until = following(layout, &l.item_id)
                .map(|n| n.start_s)
                .unwrap_or(layout.duration_s);
            let len = until - l.end_s;
            NoteAnchor::Pause {
                scene_id,
                line_id: l.item_id.clone(),
                at: if len > 1e-6 {
                    ((a - l.end_s) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                },
            }
        }
        // before the first line there's nothing to hang on but the scene
        None => NoteAnchor::Scene { scene_id },
    }
}

fn word_number(w: &crate::timeline::LayoutWord) -> usize {
    w.script_idx.unwrap_or(w.idx)
}

/// The word being said at `t`, or the one just said.
fn first_word_at(line: &LayoutItem, t: f64) -> Option<usize> {
    line.words
        .iter()
        .filter(|w| !w.deleted && w.start_s <= t)
        .max_by(|a, b| a.start_s.total_cmp(&b.start_s))
        .or_else(|| line.words.iter().find(|w| !w.deleted))
        .map(word_number)
}

/// The first word still being said at `t` or later.
fn first_word(line: &LayoutItem, t: f64) -> Option<usize> {
    line.words
        .iter()
        .filter(|w| !w.deleted && w.end_s > t)
        .min_by(|a, b| a.start_s.total_cmp(&b.start_s))
        .map(word_number)
}

/// The last word begun before `t`.
fn last_word(line: &LayoutItem, t: f64) -> Option<usize> {
    line.words
        .iter()
        .filter(|w| !w.deleted && w.start_s < t)
        .max_by(|a, b| a.start_s.total_cmp(&b.start_s))
        .map(word_number)
}

/// The words a note quotes, as they stand now.
pub fn quote_of(doc: &Document, layout: &SceneLayout, anchor: &NoteAnchor) -> String {
    let said = |line_id: &str, from: usize, to: usize| -> Vec<String> {
        layout
            .item(line_id)
            .map(|i| {
                i.words
                    .iter()
                    .filter(|w| !w.deleted && (from..=to).contains(&word_number(w)))
                    .map(|w| w.text.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    match anchor {
        NoteAnchor::Words {
            line_id,
            from,
            to,
            to_line,
            ..
        } => {
            let mut w = match to_line {
                Some(l) => {
                    let mut w = said(line_id, *from, usize::MAX);
                    w.extend(said(l, 0, *to));
                    w
                }
                None => said(line_id, *from, *to),
            };
            w.retain(|x| !x.is_empty());
            w.join(" ")
        }
        NoteAnchor::Line { line_id, .. } => doc
            .lines
            .get(line_id)
            .map(|l| l.text.clone())
            .unwrap_or_default(),
        NoteAnchor::Pause { line_id, .. } => {
            let w = said(line_id, 0, usize::MAX);
            w.last()
                .map(|x| format!("after {x}"))
                .unwrap_or_else(|| "a pause".into())
        }
        NoteAnchor::Sound { placement_id, .. } => doc
            .placements
            .get(placement_id)
            .and_then(|p| doc.assets.get(&p.asset_id))
            .map(|a| {
                let f = a.file.rsplit('/').next().unwrap_or(&a.file);
                f.rsplit_once('.').map(|(n, _)| n).unwrap_or(f).to_string()
            })
            .unwrap_or_default(),
        NoteAnchor::Scene { .. } => String::new(),
    }
}

/// Who speaks at the anchor, by name.
pub fn speaker_of(doc: &Document, anchor: &NoteAnchor) -> String {
    anchor
        .line_id()
        .and_then(|l| doc.lines.get(l))
        .and_then(|l| l.character.as_ref())
        .and_then(|c| doc.characters.get(c))
        .map(|c| c.label.clone())
        .unwrap_or_default()
}

/// How the element stands: a change here is what *changed since note* says. What only moves it
/// (order, the pause before it) isn't in here: the note goes with it (D138).
pub fn print_of(doc: &Document, layout: &SceneLayout, anchor: &NoteAnchor) -> String {
    let v = match anchor {
        NoteAnchor::Scene { .. } => return String::new(),
        NoteAnchor::Line { line_id, .. } | NoteAnchor::Words { line_id, .. } => {
            let Some(b) = doc.boxes.get(line_id) else {
                return String::new();
            };
            json!([
                b.take_id,
                b.comp,
                b.edits,
                b.head_s,
                b.tail_s,
                b.gain_db,
                b.depth,
                b.muted,
                b.split_points,
                b.fade_in_s,
                b.fade_out_s
            ])
        }
        NoteAnchor::Pause { line_id, .. } => {
            let Some(next) = following(layout, line_id) else {
                return String::new();
            };
            json!(((next.gap_before_s) * 100.0).round())
        }
        NoteAnchor::Sound { placement_id, .. } => {
            let Some(p) = doc.placements.get(placement_id) else {
                return String::new();
            };
            json!([
                p.asset_id,
                p.anchor,
                p.at_s,
                p.offset_s,
                p.in_s,
                p.len_s,
                p.gain_db,
                p.pan,
                p.depth,
                p.muted,
                p.fade_in_s,
                p.fade_out_s
            ])
        }
    };
    blake3::hash(v.to_string().as_bytes()).to_hex()[..16].to_string()
}

// ---- the `.cutawayreview` file ----

/// The episode a file holds events of.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ManifestEpisode {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReviewManifest {
    pub format: u32,
    /// The show, or the episode when it stands alone.
    pub project: String,
    pub episodes: Vec<ManifestEpisode>,
    /// The rounds its events belong to, each with the cut it refers to.
    pub rounds: Vec<ReviewRound>,
    pub reviewer: Reviewer,
    pub device: String,
    pub events: usize,
    /// blake3 of the events.
    pub checksum: String,
}

/// A review on its way: everything a person said, in one gzip-compressed JSON file.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewFile {
    pub manifest: ReviewManifest,
    pub events: Vec<ReviewEvent>,
}

fn checksum(events: &Value) -> String {
    blake3::hash(events.to_string().as_bytes())
        .to_hex()
        .to_string()
}

impl ReviewFile {
    /// `events` go in clock order. The count and the checksum are set here.
    pub fn new(mut manifest: ReviewManifest, mut events: Vec<ReviewEvent>) -> Self {
        events.sort_by(|a, b| (&a.clock, &a.id).cmp(&(&b.clock, &b.id)));
        manifest.format = REVIEW_FORMAT;
        manifest.events = events.len();
        ReviewFile { manifest, events }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let events = serde_json::to_value(&self.events)?;
        let mut manifest = self.manifest.clone();
        manifest.checksum = checksum(&events);
        let body = json!({ "manifest": manifest, "events": events });
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(body.to_string().as_bytes())?;
        Ok(gz.finish()?)
    }

    pub fn decode(bytes: &[u8]) -> Result<ReviewFile> {
        let bad = |why: &str| CoreError::invalid(format!("this isn't a review file: {why}"));
        let mut text = String::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_string(&mut text)
            .map_err(|_| bad("it can't be unpacked"))?;
        let v: Value = serde_json::from_str(&text).map_err(|_| bad("it can't be read"))?;
        let format = v
            .pointer("/manifest/format")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("it has no manifest"))?;
        if format > REVIEW_FORMAT as u64 {
            return Err(CoreError::invalid(format!(
                "this review was written by a newer {} (format {format}); update to add it",
                crate::brand::DISPLAY_NAME
            )));
        }
        let manifest: ReviewManifest = serde_json::from_value(v["manifest"].clone())
            .map_err(|e| bad(&format!("its manifest is broken ({e})")))?;
        if checksum(&v["events"]) != manifest.checksum {
            return Err(CoreError::invalid(
                "this review file is damaged: its checksum doesn't match. Ask for it again.",
            ));
        }
        let events: Vec<ReviewEvent> = serde_json::from_value(v["events"].clone())
            .map_err(|e| bad(&format!("its events are broken ({e})")))?;
        Ok(ReviewFile { manifest, events })
    }
}

/// What a merge has to do: the events it doesn't have yet, in `incoming`.
pub fn unseen<'a>(doc: &Document, incoming: &'a [ReviewEvent]) -> Vec<&'a ReviewEvent> {
    let mut seen: HashSet<&str> = HashSet::new();
    incoming
        .iter()
        .filter(|e| !doc.review_events.contains_key(&e.id) && seen.insert(&e.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(name: &str) -> Reviewer {
        Reviewer::new(name, ReviewRole::Director)
    }

    fn at(id: &str, note: &str, ms: u64, body: ReviewBody) -> ReviewEvent {
        ReviewEvent {
            id: id.into(),
            episode: "ep".into(),
            note: note.into(),
            round: None,
            author: who("Anna Weber"),
            device: "a".into(),
            clock: Hlc {
                ms,
                count: 0,
                device: "a".into(),
            }
            .encode(),
            anchor: NoteAnchor::Line {
                scene_id: "s".into(),
                line_id: "l".into(),
            },
            body,
        }
    }

    fn note(id: &str, ms: u64) -> ReviewEvent {
        at(
            id,
            id,
            ms,
            ReviewBody::Note {
                text: "louder".into(),
                tag: Some(NoteTag::Performance),
                quote: String::new(),
                speaker: String::new(),
                print: String::new(),
                written_s: 0.0,
            },
        )
    }

    fn doc_of(events: &[ReviewEvent]) -> Document {
        let mut d = Document::default();
        for e in events {
            d.review_events.insert(e.id.clone(), e.clone());
        }
        d
    }

    #[test]
    fn initials_and_file_names_come_from_the_name() {
        assert_eq!(
            Reviewer::new("Anna Weber", ReviewRole::Director).initials,
            "AW"
        );
        assert_eq!(Reviewer::new("Albert", ReviewRole::Producer).initials, "AL");
        assert_eq!(who("Anna  Weber ").slug(), "anna-weber");
        assert_eq!(who("Jürgen O'Neil").slug(), "jürgen-o-neil");
        assert_eq!(who("…").slug(), "reviewer");
    }

    #[test]
    fn a_clock_sorts_as_text_and_passes_what_it_has_seen() {
        let a = Hlc::next(None, 1000, "a").encode();
        // a machine whose clock runs behind still comes after what it has seen
        let b = Hlc::next(Some(&a), 900, "b").encode();
        let c = Hlc::next(Some(&b), 5000, "a").encode();
        assert!(a < b && b < c, "{a} {b} {c}");
        assert_eq!(Hlc::parse(&b).unwrap().ms, 1000);
        // over a digit boundary the text still sorts
        assert!(
            Hlc {
                ms: 999,
                count: 0,
                device: "z".into()
            }
            .encode()
                < Hlc {
                    ms: 1000,
                    count: 0,
                    device: "a".into()
                }
                .encode()
        );
    }

    #[test]
    fn the_latest_clock_settles_resolve_and_reopen_in_either_order() {
        let n = note("n", 1);
        let resolve = at("r", "n", 2, ReviewBody::Resolve);
        let reopen = at("o", "n", 3, ReviewBody::Reopen);
        for order in [
            vec![n.clone(), resolve.clone(), reopen.clone()],
            vec![reopen.clone(), resolve.clone(), n.clone()],
        ] {
            let f = fold_notes(&doc_of(&order));
            assert_eq!(f[0].status, NoteStatus::Reopened);
            assert!(f[0].resolved.is_some() && f[0].reopened.is_some());
        }
        // and the other way round it ends resolved
        let later = at("r2", "n", 4, ReviewBody::Resolve);
        let f = fold_notes(&doc_of(&[n, resolve, reopen, later]));
        assert_eq!(f[0].status, NoteStatus::Resolved);
    }

    #[test]
    fn the_latest_edit_and_tag_win_and_replies_keep_their_order() {
        let events = [
            note("n", 1),
            at("e2", "n", 5, ReviewBody::Edit { text: "new".into() }),
            at("e1", "n", 3, ReviewBody::Edit { text: "old".into() }),
            at(
                "t",
                "n",
                4,
                ReviewBody::Tag {
                    tag: Some(NoteTag::Sound),
                },
            ),
            at(
                "b",
                "n",
                7,
                ReviewBody::Reply {
                    text: "second".into(),
                },
            ),
            at(
                "a",
                "n",
                6,
                ReviewBody::Reply {
                    text: "first".into(),
                },
            ),
        ];
        let f = fold_notes(&doc_of(&events));
        assert_eq!(f[0].text, "new");
        assert_eq!(f[0].tag, Some(NoteTag::Sound));
        let r: Vec<&str> = f[0].replies.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(r, ["first", "second"]);
    }

    #[test]
    fn a_reply_that_arrives_before_its_note_waits_for_it() {
        let reply = at("r", "n", 2, ReviewBody::Reply { text: "yes".into() });
        assert!(fold_notes(&doc_of(std::slice::from_ref(&reply))).is_empty());
        let f = fold_notes(&doc_of(&[reply, note("n", 1)]));
        assert_eq!(f[0].replies.len(), 1);
    }

    #[test]
    fn what_was_undone_elsewhere_goes_and_a_redo_brings_it_back() {
        let n = note("n", 1);
        let off = at(
            "x",
            "n",
            2,
            ReviewBody::Retract {
                target: "n".into(),
                on: true,
            },
        );
        assert!(fold_notes(&doc_of(&[n.clone(), off.clone()])).is_empty());
        let on = at(
            "y",
            "n",
            3,
            ReviewBody::Retract {
                target: "n".into(),
                on: false,
            },
        );
        assert_eq!(fold_notes(&doc_of(&[n, off, on])).len(), 1);
    }

    #[test]
    fn notes_on_touching_ranges_of_one_element_are_one_thread() {
        let words = |line: &str, from, to| NoteAnchor::Words {
            scene_id: "s".into(),
            line_id: line.into(),
            from,
            to,
            to_line: None,
        };
        let with = |id: &str, ms, anchor: NoteAnchor| {
            let mut e = note(id, ms);
            e.anchor = anchor;
            e
        };
        let line = NoteAnchor::Line {
            scene_id: "s".into(),
            line_id: "b".into(),
        };
        let d = doc_of(&[
            with("n1", 1, words("a", 2, 5)),
            with("n2", 2, words("a", 5, 7)),
            with("n3", 3, words("a", 8, 9)),
            with("n4", 4, words("b", 0, 0)),
            with("n5", 5, line),
        ]);
        let f = fold_notes(&d);
        let t = threads(&f);
        assert_eq!(t[0], t[1], "they share word 5");
        assert_ne!(t[0], t[2], "no word in common");
        assert_eq!(t[3], t[4], "a whole line takes in its words");
    }

    #[test]
    fn a_review_file_comes_back_as_it_went() {
        let manifest = ReviewManifest {
            format: 0,
            project: "TARMAC".into(),
            episodes: vec![ManifestEpisode {
                id: "ep".into(),
                name: "Ep 2".into(),
            }],
            rounds: vec![],
            reviewer: who("Anna Weber"),
            device: "a".into(),
            events: 0,
            checksum: String::new(),
        };
        let f = ReviewFile::new(manifest, vec![note("b", 2), note("a", 1)]);
        let bytes = f.encode().unwrap();
        assert_eq!(&bytes[..2], &[0x1f, 0x8b], "gzip");
        let back = ReviewFile::decode(&bytes).unwrap();
        assert_eq!(back.events, f.events);
        assert_eq!(back.manifest.events, 2);
        assert_eq!(back.events[0].id, "a", "in clock order");
    }

    #[test]
    fn a_damaged_or_newer_file_is_turned_away_with_a_reason() {
        let pack = |v: Value| {
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gz.write_all(v.to_string().as_bytes()).unwrap();
            gz.finish().unwrap()
        };
        let e = ReviewFile::decode(b"not gzip").unwrap_err().to_string();
        assert!(e.contains("isn't a review file"), "{e}");
        let newer = pack(json!({ "manifest": { "format": REVIEW_FORMAT + 1 }, "events": [] }));
        let e = ReviewFile::decode(&newer).unwrap_err().to_string();
        assert!(e.contains("newer"), "{e}");
        let f = ReviewFile::new(
            ReviewManifest {
                format: 0,
                project: "p".into(),
                episodes: vec![],
                rounds: vec![],
                reviewer: who("A"),
                device: "a".into(),
                events: 0,
                checksum: String::new(),
            },
            vec![note("a", 1)],
        );
        let mut text = String::new();
        flate2::read::GzDecoder::new(f.encode().unwrap().as_slice())
            .read_to_string(&mut text)
            .unwrap();
        let tampered = pack(serde_json::from_str(&text.replace("louder", "softer")).unwrap());
        let e = ReviewFile::decode(&tampered).unwrap_err().to_string();
        assert!(e.contains("damaged"), "{e}");
    }

    #[test]
    fn an_event_keeps_its_shape_on_disk() {
        let e = note("a", 1);
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "note");
        assert_eq!(v["anchor"]["on"], "line");
        assert_eq!(serde_json::from_value::<ReviewEvent>(v).unwrap(), e);
    }
}
