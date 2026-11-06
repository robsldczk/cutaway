//! Review mode: who's reviewing, rounds, notes on the live cut, and the review files that carry
//! them between machines. The folded notes themselves are `apc_core::review`.
//!
//! Two ways to be in review. The editor enters it on the open episode: same database, the bus
//! taking notes only. A package opened for review is someone else's project, often on a synced
//! share, so it's never written to: the session runs on a private copy, refreshed when the
//! package changes, and what the reviewer says leaves through their own file (D140).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use apc_core::bus::BusMode;
use apc_core::commands::Command;
use apc_core::ids::new_id;
use apc_core::project::{find_episode_file, EpisodePaths};
use apc_core::review::*;
use apc_core::show::{is_conflict_copy, write_atomic};
use apc_core::store::EpisodeStore;
use apc_core::timeline::{layout_scene, SceneLayout};
use apc_core::*;
use apc_export::package::timeline_hash;

use crate::error::{AppError, Result};
use crate::exports;
use crate::methods::scene_id;
use crate::service::Service;
use crate::studio::host_name;

/// What review mode lets through besides reading. The note commands are checked again on the
/// bus; these are the methods that lead to them, or that leave the cut alone.
pub const REVIEW_METHODS: &[&str] = &[
    "add_note",
    "reply_note",
    "edit_note",
    "tag_note",
    "resolve_note",
    "reopen_note",
    "move_note",
    "remove_note",
    "set_reviewer",
    "send_review",
    "review_sync",
    "leave_review",
    "enter_review",
    "command",
    "undo",
    "redo",
];

/// The review state of an open episode. None of it is in the document.
#[derive(Default)]
pub struct ReviewLocal {
    /// Set from the very start of a session on a private copy, before anything can write:
    /// no backups, no checkpoint, no lock. The package is never touched.
    pub private: bool,
    /// A package opened for review: the session runs on a private copy of it.
    pub package: Option<Package>,
    /// Who's writing, in this session.
    pub reviewer: Option<Reviewer>,
    /// The review files read so far, by when they last changed.
    seen: HashMap<PathBuf, FileStamp>,
    /// What's wrong with files in the folder, kept until they change.
    troubles: BTreeMap<PathBuf, String>,
    /// The revision this machine's files were last brought up to.
    published: Option<i64>,
}

impl ReviewLocal {
    pub(crate) fn new(private: bool) -> Self {
        ReviewLocal {
            private,
            ..Default::default()
        }
    }

    /// Forgets which files were read: a revert took their events with it.
    pub fn forget_files(&mut self) {
        self.seen.clear();
    }
}

type FileStamp = (Option<SystemTime>, u64);

fn stamp_of(path: &Path) -> FileStamp {
    match std::fs::metadata(path) {
        Ok(m) => (m.modified().ok(), m.len()),
        Err(_) => (None, 0),
    }
}

/// The package and how it stood when it was copied.
pub struct Package {
    source: PathBuf,
    stamp: (FileStamp, FileStamp),
    dir: PathBuf,
}

fn wal_of(file: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", file.display()))
}

fn package_stamp(file: &Path) -> (FileStamp, FileStamp) {
    (stamp_of(file), stamp_of(&wal_of(file)))
}

// ---- who's reviewing ----

/// What this computer remembers. It's a file, not the web view's storage, so the CLI and MCP
/// know the same person.
#[derive(Serialize, Deserialize, Default, Clone)]
struct Remembered {
    #[serde(default)]
    me: Option<Reviewer>,
    /// Whoever was here last, remembered or not.
    #[serde(default)]
    last: Option<Reviewer>,
}

fn identity_file() -> PathBuf {
    apc_sound::data_root().join("reviewer.json")
}

fn remembered() -> Remembered {
    std::fs::read(identity_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn keep_identity(r: &Remembered) -> Result<()> {
    let path = identity_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_atomic(
        &path,
        &serde_json::to_vec_pretty(&serde_json::to_value(r.clone())?)?,
    )?;
    Ok(())
}

/// Who writes here: the session's person, or the one this computer remembers.
pub fn current(svc: &Service) -> Option<Reviewer> {
    svc.review.reviewer.clone().or_else(|| remembered().me)
}

#[derive(Deserialize, JsonSchema)]
pub struct AuthorP {
    pub name: String,
    #[serde(default)]
    pub role: ReviewRole,
}

fn author_of(svc: &Service, given: Option<AuthorP>) -> Result<Reviewer> {
    match given {
        Some(a) if !a.name.trim().is_empty() => Ok(Reviewer::new(&a.name, a.role)),
        _ => current(svc).ok_or_else(|| {
            AppError::Params("say who's writing first (set_reviewer, or give author)".into())
        }),
    }
}

pub fn reviewer_view(svc: &Service) -> Value {
    let r = remembered();
    json!({
        "current": current(svc),
        "last": r.last,
        "remembered": r.me.is_some() && r.me == current(svc),
    })
}

#[derive(Deserialize, JsonSchema)]
pub struct SetReviewerP {
    pub name: String,
    pub role: ReviewRole,
    /// Keep this person on this computer.
    #[serde(default)]
    pub remember: bool,
}

pub fn set_reviewer(svc: &mut Service, p: SetReviewerP) -> Result<Value> {
    if p.name.trim().is_empty() {
        return Err(AppError::Params("a reviewer needs a name".into()));
    }
    let who = Reviewer::new(&p.name, p.role);
    let mut r = remembered();
    r.last = Some(who.clone());
    if p.remember {
        r.me = Some(who.clone());
    } else if r.me.as_ref().is_some_and(|m| m.slug() == who.slug()) {
        r.me = None;
    }
    keep_identity(&r)?;
    svc.review.reviewer = Some(who);
    Ok(reviewer_view(svc))
}

// ---- the episode's identity ----

/// The same for every copy of an episode: it's made from when the file was created, which the
/// package carries, so a reviewer's read-only copy needn't write anything down to have one.
pub fn episode_id(store: &EpisodeStore) -> Result<String> {
    let made = store.meta("created_at")?.unwrap_or_default();
    let hash = blake3::hash(format!("episode {made}").as_bytes());
    Ok(format!("ep_{}", &hash.to_hex()[..16]))
}

fn project_name(svc: &Service) -> String {
    svc.paths
        .show
        .as_ref()
        .map(|s| s.name().to_string())
        .unwrap_or_else(|| svc.name())
}

/// The shared folder: the show's, or one beside a standalone episode.
pub fn reviews_dir(paths: &EpisodePaths) -> PathBuf {
    match &paths.show {
        Some(show) => show.reviews_dir(),
        None => paths.root.join("Reviews"),
    }
}

fn ext() -> &'static str {
    apc_core::brand::REVIEW_EXT
}

// ---- entering and leaving ----

pub fn enter(svc: &mut Service) -> Result<Value> {
    svc.bus.set_mode(BusMode::Review);
    Ok(mode_view(svc))
}

pub fn leave(svc: &mut Service) -> Result<Value> {
    if svc.review.package.is_some() {
        return Err(AppError::Other(
            "this package was opened for review: close it to leave".into(),
        ));
    }
    svc.bus.set_mode(BusMode::Edit);
    Ok(mode_view(svc))
}

/// What `project_status` says about review.
pub fn mode_view(svc: &Service) -> Value {
    let doc = svc.doc();
    let notes = fold_notes(doc);
    json!({
        "active": svc.in_review(),
        "package": svc.review.package.is_some(),
        "open_notes": notes.iter().filter(|n| n.status.is_open()).count(),
        "notes": notes.len(),
        "rounds": doc.review_rounds.len(),
    })
}

// ---- a package opened for review ----

/// The private folder of a package, the same each time it's opened.
fn private_dir(file: &Path) -> PathBuf {
    let key = blake3::hash(file.to_string_lossy().as_bytes()).to_hex();
    apc_sound::data_root().join("review").join(&key[..16])
}

fn copies_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(brand::EPISODE_EXT))
                .collect()
        })
        .unwrap_or_default()
}

fn drop_copy(path: &Path) {
    for p in [
        path.to_path_buf(),
        wal_of(path),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        let _ = std::fs::remove_file(p);
    }
}

/// A fresh private copy of the package. The undo log is the editor's, so it stays behind.
fn take_copy(source: &Path, dir: &Path) -> Result<EpisodeStore> {
    std::fs::create_dir_all(dir)?;
    let copy = dir.join(format!("{}.{}", new_id("copy"), brand::EPISODE_EXT));
    EpisodeStore::copy_file(source, &copy)?;
    let store = EpisodeStore::open(&copy)?;
    store.clear_log()?;
    Ok(store)
}

/// What was said in the session a copy held: nothing of it is lost when the copy goes.
fn said_in(path: &Path) -> Vec<ReviewEvent> {
    EpisodeStore::open_read_only(path)
        .and_then(|s| s.load_document())
        .map(|d| d.review_events.into_values().collect())
        .unwrap_or_default()
}

/// Opens an episode for review without writing to it: nothing next to the file changes.
pub fn open_package(path: &Path, client: &str) -> Result<Service> {
    let file = find_episode_file(path)?;
    let mut paths = EpisodePaths::for_file(&file)?;
    let dir = private_dir(&file);
    // caches are a working file too: the package may be read-only, and it's synced
    paths.cache = dir.join("Cache");
    std::fs::create_dir_all(paths.cache.join("media"))?;
    let older = copies_in(&dir);
    let stamp = package_stamp(&file);
    let store = take_copy(&file, &dir)?;
    let mut svc = Service::from_private(paths, store, client)?;
    for old in older {
        carry(&mut svc, said_in(&old))?;
        drop_copy(&old);
    }
    svc.review.package = Some(Package {
        source: file,
        stamp,
        dir,
    });
    svc.bus.set_mode(BusMode::Review);
    sync(&mut svc)?;
    Ok(svc)
}

/// Rounds aren't carried: they're the editor's, and come with the package or the editor's file.
fn carry(svc: &mut Service, events: Vec<ReviewEvent>) -> Result<()> {
    let events: Vec<ReviewEvent> = unseen(svc.doc(), &events).into_iter().cloned().collect();
    let count = events.len();
    svc.bus.merge_review(&json!({ "carried": count }), |tx| {
        for e in &events {
            tx.put(e)?;
        }
        Ok(())
    })?;
    Ok(())
}

/// The package moved on: takes a new copy, with everything said here. Playback carries on from
/// the document it had; the next play hears the new cut.
fn refresh_package(svc: &mut Service) -> Result<bool> {
    let Some(p) = &svc.review.package else {
        return Ok(false);
    };
    let now = package_stamp(&p.source);
    if now == p.stamp {
        return Ok(false);
    }
    let (source, dir) = (p.source.clone(), p.dir.clone());
    // a sync tool can be mid-write: the next look tries again
    let Ok(store) = take_copy(&source, &dir) else {
        return Ok(false);
    };
    let before = svc.bus.store().path().to_path_buf();
    let events: Vec<ReviewEvent> = svc.doc().review_events.values().cloned().collect();
    let client = svc.client.clone();
    svc.bus = CommandBus::new(store, client)?;
    svc.bus.set_mode(BusMode::Review);
    carry(svc, events)?;
    drop_copy(&before);
    if let Some(p) = svc.review.package.as_mut() {
        p.stamp = now;
    }
    Ok(true)
}

// ---- rounds ----

#[derive(Deserialize, JsonSchema)]
pub struct StartRoundP {
    /// Scene ids, numbers or "S2".
    pub scenes: Vec<String>,
}

fn editor(svc: &Service) -> Reviewer {
    current(svc).unwrap_or_else(|| {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "Editor".into());
        Reviewer::new(&user, ReviewRole::Editor)
    })
}

pub fn start_round(svc: &mut Service, p: StartRoundP) -> Result<Value> {
    if p.scenes.is_empty() {
        return Err(AppError::Params("pick the scenes to review".into()));
    }
    let history = exports::episode_history(svc);
    let mut scenes = Vec::new();
    for key in &p.scenes {
        let id = scene_id(svc, Some(key))?;
        let doc = svc.doc();
        let s = doc
            .scenes
            .get(&id)
            .ok_or_else(|| AppError::Params(format!("no scene '{key}'")))?;
        scenes.push(RoundScene {
            scene_id: id.clone(),
            version: history.last_of(&id).map(|r| r.manifest.version),
            timeline_hash: timeline_hash(doc, s)?,
        });
    }
    let doc = svc.doc();
    let number = doc
        .review_rounds
        .values()
        .map(|r| r.number)
        .max()
        .unwrap_or(0)
        + 1;
    let round = ReviewRound {
        id: new_id("rnd"),
        episode: episode_id(svc.bus.store())?,
        number,
        created_by: editor(svc),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        scenes,
        status: RoundStatus::Open,
    };
    let r = svc.exec(&Command::StartReviewRound {
        round: round.clone(),
    })?;
    let _ = publish(svc);
    Ok(json!({ "round": round.id, "number": number, "seq": r.seq }))
}

/// The newest open round, the one new notes go to.
pub fn current_round(doc: &Document) -> Option<&ReviewRound> {
    doc.review_rounds
        .values()
        .filter(|r| r.status == RoundStatus::Open)
        .max_by_key(|r| r.number)
}

fn round_of_scene<'a>(doc: &'a Document, scene_id: &str) -> Option<&'a ReviewRound> {
    let mut open: Vec<&ReviewRound> = doc
        .review_rounds
        .values()
        .filter(|r| r.status == RoundStatus::Open)
        .collect();
    open.sort_by_key(|r| std::cmp::Reverse(r.number));
    open.iter()
        .find(|r| r.scenes.iter().any(|s| s.scene_id == scene_id))
        .copied()
        .or_else(|| open.first().copied())
}

/// `v004`: the newest export among a round's scenes, or nothing for a cut never delivered.
fn cut_of(r: &ReviewRound) -> Option<String> {
    r.scenes
        .iter()
        .filter_map(|s| s.version)
        .max()
        .map(|v| format!("v{v:03}"))
}

// ---- writing notes ----

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// A new event, later than everything seen.
fn event(
    svc: &Service,
    note: &str,
    round: Option<String>,
    author: Reviewer,
    anchor: NoteAnchor,
    body: ReviewBody,
) -> Result<ReviewEvent> {
    let doc = svc.doc();
    Ok(ReviewEvent {
        id: new_id("ev"),
        episode: episode_id(svc.bus.store())?,
        note: note.to_string(),
        round,
        author,
        device: host_name().to_string(),
        clock: Hlc::next(latest_clock(doc), now_ms(), host_name()).encode(),
        anchor,
        body,
    })
}

fn layouts(svc: &Service) -> HashMap<String, SceneLayout> {
    let pacing = svc.config().pacing();
    let doc = svc.doc();
    doc.scenes
        .values()
        .map(|s| (s.id.clone(), layout_scene(doc, s, &pacing)))
        .collect()
}

fn layout_of(svc: &Service, scene_id: &str) -> Result<SceneLayout> {
    let doc = svc.doc();
    let s = doc
        .scenes
        .get(scene_id)
        .ok_or_else(|| AppError::Params(format!("no scene '{scene_id}'")))?;
    Ok(layout_scene(doc, s, &svc.config().pacing()))
}

/// Where a note goes. `at_s` and `to_s` are a marker or a range on the scene; `line_id` (with
/// `word`, `to_word`) and `placement_id` name an element outright; none of them is the scene.
#[derive(Deserialize, JsonSchema, Default)]
pub struct TargetP {
    /// Scene id, number or "S2". Defaults to the first scene.
    #[serde(default)]
    pub scene: Option<String>,
    #[serde(default)]
    pub at_s: Option<f64>,
    #[serde(default)]
    pub to_s: Option<f64>,
    #[serde(default)]
    pub line_id: Option<String>,
    /// A word of the line, numbered from 0.
    #[serde(default)]
    pub word: Option<usize>,
    #[serde(default)]
    pub to_word: Option<usize>,
    #[serde(default)]
    pub placement_id: Option<String>,
}

fn anchor_of(svc: &Service, p: &TargetP) -> Result<NoteAnchor> {
    let doc = svc.doc();
    // a line or a sound says which scene it's in
    let scene = match (&p.line_id, &p.placement_id) {
        (Some(l), _) => doc
            .lines
            .get(l)
            .map(|l| l.scene_id.clone())
            .ok_or_else(|| AppError::Params(format!("no line '{l}'")))?,
        (_, Some(id)) => doc
            .placements
            .get(id)
            .map(|x| x.scene_id.clone())
            .ok_or_else(|| AppError::Params(format!("no sound '{id}'")))?,
        _ => scene_id(svc, p.scene.as_deref())?,
    };
    if let Some(placement_id) = &p.placement_id {
        return Ok(NoteAnchor::Sound {
            scene_id: scene,
            placement_id: placement_id.clone(),
        });
    }
    if let Some(line_id) = &p.line_id {
        return Ok(match p.word {
            Some(from) => NoteAnchor::Words {
                scene_id: scene,
                line_id: line_id.clone(),
                from,
                to: p.to_word.unwrap_or(from).max(from),
                to_line: None,
            },
            None => NoteAnchor::Line {
                scene_id: scene,
                line_id: line_id.clone(),
            },
        });
    }
    match p.at_s {
        Some(t) => Ok(anchor_at(&layout_of(svc, &scene)?, t, p.to_s)),
        None => Ok(NoteAnchor::Scene { scene_id: scene }),
    }
}

/// `00:48.6`
fn clock_label(t: f64) -> String {
    let t = t.max(0.0);
    let m = (t / 60.0).floor();
    format!("{:02}:{:04.1}", m as u32, t - m * 60.0)
}

/// `00:48.6`, or `00:48.6 to 49.8`; the minutes come back when they change.
fn spot_label(s: &NoteSpot) -> String {
    let a = clock_label(s.start_s);
    match s.end_s {
        Some(e) if e > s.start_s => {
            let b = clock_label(e);
            match (a.split_once(':'), b.split_once(':')) {
                (Some((am, _)), Some((bm, sec))) if am == bm => format!("{a} to {sec}"),
                _ => format!("{a} to {b}"),
            }
        }
        _ => a,
    }
}

/// What a note would hang on, for the note being written.
pub fn target_view(svc: &Service, p: &TargetP) -> Result<Value> {
    let anchor = anchor_of(svc, p)?;
    let layout = layout_of(svc, anchor.scene_id())?;
    let doc = svc.doc();
    let spot = locate(doc, &layout, &anchor);
    Ok(json!({
        "on": anchor.kind(),
        "line_id": anchor.line_id(),
        "placement_id": anchor.placement_id(),
        "quote": quote_of(doc, &layout, &anchor),
        "speaker": speaker_of(doc, &anchor),
        "start_s": spot.map(|s| s.start_s),
        "end_s": spot.and_then(|s| s.end_s),
        "time": spot.as_ref().map(spot_label),
    }))
}

#[derive(Deserialize, JsonSchema)]
pub struct AddNoteP {
    #[serde(flatten)]
    pub target: TargetP,
    pub text: String,
    /// performance, timing, sound or music.
    #[serde(default)]
    pub tag: Option<NoteTag>,
    /// The round it's for; defaults to the newest open round with this scene in it.
    #[serde(default)]
    pub round: Option<String>,
    /// Who's writing, when it isn't the current reviewer.
    #[serde(default)]
    pub author: Option<AuthorP>,
}

pub fn add_note(svc: &mut Service, p: AddNoteP) -> Result<Value> {
    if p.text.trim().is_empty() {
        return Err(AppError::Params("a note needs some text".into()));
    }
    let anchor = anchor_of(svc, &p.target)?;
    let layout = layout_of(svc, anchor.scene_id())?;
    let doc = svc.doc();
    let spot = locate(doc, &layout, &anchor);
    let round = match p.round {
        Some(r) if !doc.review_rounds.contains_key(&r) => {
            return Err(AppError::Params(format!("no round '{r}'")))
        }
        Some(r) => Some(r),
        None => round_of_scene(doc, anchor.scene_id()).map(|r| r.id.clone()),
    };
    let body = ReviewBody::Note {
        text: p.text.trim().to_string(),
        tag: p.tag,
        quote: quote_of(doc, &layout, &anchor),
        speaker: speaker_of(doc, &anchor),
        print: print_of(doc, &layout, &anchor),
        written_s: spot.map(|s| s.start_s).unwrap_or(0.0),
    };
    let author = author_of(svc, p.author)?;
    let note = new_id("note");
    let e = event(svc, &note, round, author, anchor, body)?;
    write_event(svc, e)?;
    note_view(svc, &note)
}

fn write_event(svc: &mut Service, e: ReviewEvent) -> Result<()> {
    svc.exec(&Command::ReviewNote { event: e })?;
    // the note is safe in the document; the file catches up on the next look if it can't now
    let _ = publish(svc);
    Ok(())
}

fn folded(svc: &Service, id: &str) -> Result<FoldedNote> {
    fold_notes(svc.doc())
        .into_iter()
        .find(|n| n.id == id)
        .ok_or_else(|| AppError::Params(format!("no note '{id}'")))
}

/// Everything that follows a note carries its anchor and round, so an event can be read alone.
fn follow_up(
    svc: &mut Service,
    id: &str,
    author: Option<AuthorP>,
    body: ReviewBody,
) -> Result<Value> {
    let n = folded(svc, id)?;
    let author = author_of(svc, author)?;
    let e = event(svc, &n.id, n.round, author, n.anchor, body)?;
    write_event(svc, e)?;
    note_view(svc, id)
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteTextP {
    pub note: String,
    pub text: String,
    #[serde(default)]
    pub author: Option<AuthorP>,
}

pub fn reply_note(svc: &mut Service, p: NoteTextP) -> Result<Value> {
    if p.text.trim().is_empty() {
        return Err(AppError::Params("a reply needs some text".into()));
    }
    let text = p.text.trim().to_string();
    follow_up(svc, &p.note, p.author, ReviewBody::Reply { text })
}

pub fn edit_note(svc: &mut Service, p: NoteTextP) -> Result<Value> {
    if p.text.trim().is_empty() {
        return Err(AppError::Params("a note needs some text".into()));
    }
    let text = p.text.trim().to_string();
    follow_up(svc, &p.note, p.author, ReviewBody::Edit { text })
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteTagP {
    pub note: String,
    #[serde(default)]
    pub tag: Option<NoteTag>,
    #[serde(default)]
    pub author: Option<AuthorP>,
}

pub fn tag_note(svc: &mut Service, p: NoteTagP) -> Result<Value> {
    follow_up(svc, &p.note, p.author, ReviewBody::Tag { tag: p.tag })
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteP {
    pub note: String,
    #[serde(default)]
    pub author: Option<AuthorP>,
}

pub fn resolve_note(svc: &mut Service, p: NoteP) -> Result<Value> {
    if folded(svc, &p.note)?.status == NoteStatus::Resolved {
        return note_view(svc, &p.note);
    }
    follow_up(svc, &p.note, p.author, ReviewBody::Resolve)
}

pub fn reopen_note(svc: &mut Service, p: NoteP) -> Result<Value> {
    if folded(svc, &p.note)?.status != NoteStatus::Resolved {
        return note_view(svc, &p.note);
    }
    follow_up(svc, &p.note, p.author, ReviewBody::Reopen)
}

/// Moving and removing change what someone said, so they're the author's, or the editor's, who
/// has to clear the list. Replies, resolves and reopens stay anyone's (D141).
fn may_change(note: &FoldedNote, who: &Reviewer) -> Result<()> {
    if who.slug() == note.author.slug() || who.role == ReviewRole::Editor {
        return Ok(());
    }
    Err(AppError::Other(format!(
        "This is {}'s note: only they or the editor can move or remove it.",
        note.author.name
    )))
}

#[derive(Deserialize, JsonSchema)]
pub struct MoveNoteP {
    pub note: String,
    /// Where it goes, as for add_note.
    #[serde(flatten)]
    pub target: TargetP,
    #[serde(default)]
    pub author: Option<AuthorP>,
}

/// Hangs a note on another spot. A detached note comes back into the cut this way.
pub fn move_note(svc: &mut Service, p: MoveNoteP) -> Result<Value> {
    let n = folded(svc, &p.note)?;
    let author = author_of(svc, p.author)?;
    may_change(&n, &author)?;
    let anchor = anchor_of(svc, &p.target)?;
    if anchor == n.anchor {
        return note_view(svc, &p.note);
    }
    let layout = layout_of(svc, anchor.scene_id())?;
    let doc = svc.doc();
    let body = ReviewBody::Move {
        quote: quote_of(doc, &layout, &anchor),
        speaker: speaker_of(doc, &anchor),
        print: print_of(doc, &layout, &anchor),
        written_s: locate(doc, &layout, &anchor)
            .map(|s| s.start_s)
            .unwrap_or(0.0),
    };
    let e = event(svc, &n.id, n.round, author, anchor, body)?;
    write_event(svc, e)?;
    note_view(svc, &p.note)
}

#[derive(Deserialize, JsonSchema)]
pub struct RemoveNoteP {
    pub note: String,
    #[serde(default)]
    pub author: Option<AuthorP>,
    #[serde(default)]
    pub confirm: bool,
}

pub fn remove_note(svc: &mut Service, p: RemoveNoteP) -> Result<Value> {
    let n = folded(svc, &p.note)?;
    let author = author_of(svc, p.author)?;
    may_change(&n, &author)?;
    let e = event(svc, &n.id, n.round, author, n.anchor, ReviewBody::Remove)?;
    write_event(svc, e)?;
    Ok(json!({ "removed": p.note }))
}

/// After an undo or a redo of a note command: others may already have what it took back, and
/// events are never unsaid, so the change goes out as an event of its own (D137).
pub fn after_step(svc: &mut Service, seq: i64, undone: bool) -> Result<()> {
    let mut said = Vec::new();
    for c in svc.bus.changes_of(seq)? {
        if c.kind != Kind::ReviewEvent {
            continue;
        }
        let side = c.after.as_ref().or(c.before.as_ref());
        let Some(e) = side.and_then(|v| serde_json::from_value::<ReviewEvent>(v.clone()).ok())
        else {
            continue;
        };
        let mut r = event(
            svc,
            &e.note,
            e.round.clone(),
            e.author.clone(),
            e.anchor.clone(),
            ReviewBody::Retract {
                target: e.id.clone(),
                on: undone,
            },
        )?;
        if let Some(last) = said.last() {
            let l: &ReviewEvent = last;
            r.clock = Hlc::next(Some(&l.clock), now_ms(), host_name()).encode();
        }
        said.push(r);
    }
    if said.is_empty() {
        return Ok(());
    }
    svc.bus
        .merge_review(&json!({ "retracted": said.len() }), |tx| {
            for e in &said {
                tx.put(e)?;
            }
            Ok(())
        })?;
    let _ = publish(svc);
    Ok(())
}

// ---- reading notes ----

/// A note as a page shows it: where it is now, and what's become of its element.
#[derive(Serialize, Clone, Debug)]
pub struct NoteView {
    pub id: String,
    /// Its number in its scene, in cut order.
    pub n: usize,
    pub round: Option<String>,
    pub round_number: Option<u32>,
    pub scene_id: String,
    pub scene_number: Option<u32>,
    pub author: Reviewer,
    pub mine: bool,
    /// Whoever's here may move and remove it: it's theirs, or they're the editor.
    pub can_change: bool,
    pub tag: Option<NoteTag>,
    pub text: String,
    pub status: NoteStatus,
    /// `word`, `words`, `line`, `pause`, `sound` or `scene`.
    pub on: &'static str,
    pub quote: String,
    pub speaker: String,
    pub line_id: Option<String>,
    pub placement_id: Option<String>,
    /// Scene seconds, worked out from the element. Absent when it's detached.
    pub start_s: Option<f64>,
    pub end_s: Option<f64>,
    /// `00:48.6 to 49.8`
    pub time: String,
    /// Its element has left the cut.
    pub detached: bool,
    /// Where it was when it was written, for a detached note.
    pub written_s: f64,
    /// Its element has changed since it was written.
    pub changed: bool,
    pub created_at: String,
    pub resolved: Option<Stamp>,
    pub reopened: Option<Stamp>,
    pub replies: Vec<FoldedReply>,
    /// Notes with the same thread are about the same thing: the id of its first note.
    pub thread: String,
}

fn view_all(
    doc: &Document,
    layouts: &HashMap<String, SceneLayout>,
    me: Option<&Reviewer>,
) -> Vec<NoteView> {
    let notes = fold_notes(doc);
    let thread_of = threads(&notes);
    let mut out: Vec<NoteView> = notes
        .iter()
        .zip(&thread_of)
        .map(|(n, t)| {
            let scene_id = n.anchor.scene_id().to_string();
            let layout = layouts.get(&scene_id);
            let spot = layout.and_then(|l| locate(doc, l, &n.anchor));
            let print = layout
                .map(|l| print_of(doc, l, &n.anchor))
                .unwrap_or_default();
            let round = n.round.as_ref().and_then(|r| doc.review_rounds.get(r));
            NoteView {
                id: n.id.clone(),
                n: 0,
                round: n.round.clone(),
                round_number: round.map(|r| r.number),
                scene_number: doc.scenes.get(&scene_id).map(|s| s.number),
                scene_id,
                author: n.author.clone(),
                mine: me.is_some_and(|m| m.slug() == n.author.slug()),
                can_change: me.is_some_and(|m| may_change(n, m).is_ok()),
                tag: n.tag,
                text: n.text.clone(),
                status: n.status,
                on: n.anchor.kind(),
                quote: n.quote.clone(),
                speaker: n.speaker.clone(),
                line_id: n.anchor.line_id().map(str::to_string),
                placement_id: n.anchor.placement_id().map(str::to_string),
                start_s: spot.map(|s| s.start_s),
                end_s: spot.and_then(|s| s.end_s),
                time: match &spot {
                    Some(_) if n.anchor.kind() == "scene" => "Whole scene".to_string(),
                    Some(s) => spot_label(s),
                    None => clock_label(n.written_s),
                },
                detached: spot.is_none(),
                written_s: n.written_s,
                changed: spot.is_some() && !n.print.is_empty() && print != n.print,
                created_at: n.created_at.clone(),
                resolved: n.resolved.clone(),
                reopened: n.reopened.clone(),
                replies: n.replies.clone(),
                thread: notes[*t].id.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        (a.scene_number, a.detached)
            .cmp(&(b.scene_number, b.detached))
            .then(
                a.start_s
                    .unwrap_or(a.written_s)
                    .total_cmp(&b.start_s.unwrap_or(b.written_s)),
            )
            .then(a.created_at.cmp(&b.created_at))
    });
    let mut counts: HashMap<String, usize> = HashMap::new();
    for n in &mut out {
        let c = counts.entry(n.scene_id.clone()).or_default();
        *c += 1;
        n.n = *c;
    }
    out
}

pub fn all_notes(svc: &Service) -> Vec<NoteView> {
    let me = current(svc);
    view_all(svc.doc(), &layouts(svc), me.as_ref())
}

fn note_view(svc: &Service, id: &str) -> Result<Value> {
    let n = all_notes(svc)
        .into_iter()
        .find(|n| n.id == id)
        .ok_or_else(|| AppError::Params(format!("no note '{id}'")))?;
    Ok(serde_json::to_value(n)?)
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct ListNotesP {
    /// Scene id, number or "S2"; every scene without it.
    #[serde(default)]
    pub scene: Option<String>,
    /// A round's id.
    #[serde(default)]
    pub round: Option<String>,
    /// `open` (open and reopened), `resolved`, or leave it out for all.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tag: Option<NoteTag>,
    /// Part of an author's name.
    #[serde(default)]
    pub author: Option<String>,
}

/// The one feedback set: every note of every reviewer and round, worked out afresh, in threads.
pub fn list_notes(svc: &Service, p: ListNotesP) -> Result<Value> {
    let scene = match &p.scene {
        Some(s) => Some(scene_id(svc, Some(s))?),
        None => None,
    };
    let who = p.author.as_ref().map(|a| a.to_lowercase());
    let notes: Vec<NoteView> = all_notes(svc)
        .into_iter()
        .filter(|n| scene.as_ref().is_none_or(|s| &n.scene_id == s))
        .filter(|n| p.round.is_none() || n.round == p.round)
        .filter(|n| match p.status.as_deref() {
            Some("open") => n.status.is_open(),
            Some("resolved") => !n.status.is_open(),
            _ => true,
        })
        .filter(|n| p.tag.is_none() || n.tag == p.tag)
        .filter(|n| {
            who.as_ref()
                .is_none_or(|w| n.author.name.to_lowercase().contains(w))
        })
        .collect();
    let mut threads: Vec<(String, Vec<&NoteView>)> = Vec::new();
    for n in &notes {
        match threads.iter_mut().find(|(id, _)| *id == n.thread) {
            Some((_, t)) => t.push(n),
            None => threads.push((n.thread.clone(), vec![n])),
        }
    }
    let threads: Vec<Value> = threads
        .iter()
        .map(|(id, t)| {
            json!({
                "id": id,
                "voices": t.iter().map(|n| n.author.name.clone()).collect::<HashSet<_>>().len(),
                "open": t.iter().filter(|n| n.status.is_open()).count(),
                "notes": t,
            })
        })
        .collect();
    Ok(json!({ "count": notes.len(), "threads": threads }))
}

/// What a page marks on a scene: its open notes.
#[derive(Serialize, Clone, Debug)]
pub struct NoteMark {
    pub id: String,
    pub n: usize,
    pub line_id: Option<String>,
    pub placement_id: Option<String>,
    pub start_s: f64,
    pub end_s: Option<f64>,
    pub tag: Option<NoteTag>,
    pub text: String,
    pub who: String,
    pub changed: bool,
}

/// How many notes each scene holds: (all, open).
pub fn counts(doc: &Document) -> HashMap<String, (usize, usize)> {
    let mut out: HashMap<String, (usize, usize)> = HashMap::new();
    for n in fold_notes(doc) {
        let c = out.entry(n.anchor.scene_id().to_string()).or_default();
        c.0 += 1;
        if n.status.is_open() {
            c.1 += 1;
        }
    }
    out
}

/// A scene's open notes, for the markers on Options, Arrange and Edit.
pub fn marks(doc: &Document, layout: &SceneLayout) -> Vec<NoteMark> {
    let layouts = HashMap::from([(layout.scene_id.clone(), layout.clone())]);
    view_all(doc, &layouts, None)
        .into_iter()
        .filter(|n| n.scene_id == layout.scene_id && n.status.is_open())
        .filter_map(|n| {
            Some(NoteMark {
                start_s: n.start_s?,
                id: n.id,
                n: n.n,
                line_id: n.line_id,
                placement_id: n.placement_id,
                end_s: n.end_s,
                tag: n.tag,
                text: n.text,
                who: n.author.name,
                changed: n.changed,
            })
        })
        .collect()
}

fn round_view(doc: &Document, r: &ReviewRound, notes: &[NoteView]) -> Value {
    let mine: Vec<&NoteView> = notes
        .iter()
        .filter(|n| n.round.as_deref() == Some(&r.id))
        .collect();
    let scenes: Vec<Value> = r
        .scenes
        .iter()
        .filter_map(|s| {
            let sc = doc.scenes.get(&s.scene_id)?;
            Some(json!({
                "id": sc.id, "number": sc.number, "place": sc.place,
                "version": s.version,
                "changed": timeline_hash(doc, sc).map(|h| h != s.timeline_hash).unwrap_or(false),
            }))
        })
        .collect();
    json!({
        "id": r.id,
        "number": r.number,
        "created_at": r.created_at,
        "created_by": r.created_by,
        "status": r.status,
        "cut": cut_of(r),
        "scenes": scenes,
        "notes": mine.len(),
        "open": mine.iter().filter(|n| n.status.is_open()).count(),
    })
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct ReviewViewP {
    /// Scene id, number or "S2". Defaults to the first scene.
    #[serde(default)]
    pub scene: Option<String>,
    /// The round looked at; the current one without it.
    #[serde(default)]
    pub round: Option<String>,
}

/// Everything the review screen shows around the scene the app already has.
pub fn review_view(svc: &Service, p: ReviewViewP) -> Result<Value> {
    let notes = all_notes(svc);
    let doc = svc.doc();
    let scene = scene_id(svc, p.scene.as_deref())?;
    let mut rounds: Vec<&ReviewRound> = doc.review_rounds.values().collect();
    rounds.sort_by_key(|r| std::cmp::Reverse(r.number));
    let current = current_round(doc).map(|r| r.id.clone());
    let picked = p
        .round
        .clone()
        .filter(|r| doc.review_rounds.contains_key(r))
        .or_else(|| current.clone())
        .or_else(|| rounds.first().map(|r| r.id.clone()));
    let round = picked.as_ref().and_then(|r| doc.review_rounds.get(r));
    let in_round: HashSet<&str> = round
        .map(|r| r.scenes.iter().map(|s| s.scene_id.as_str()).collect())
        .unwrap_or_default();
    let tally = counts(doc);
    let scenes: Vec<Value> = doc
        .scenes_in_order()
        .iter()
        .map(|s| {
            let (all, open) = tally.get(&s.id).copied().unwrap_or((0, 0));
            json!({
                "id": s.id,
                "in_round": round.is_none() || in_round.contains(s.id.as_str()),
                "notes": all,
                "open": open,
            })
        })
        .collect();
    let here: Vec<&NoteView> = notes.iter().filter(|n| n.scene_id == scene).collect();
    Ok(json!({
        "reviewer": reviewer_view(svc),
        "project": project_name(svc),
        "episode": svc.name(),
        "package": svc.review.package.is_some(),
        "rounds": rounds.iter().map(|r| round_view(doc, r, &notes)).collect::<Vec<_>>(),
        "current_round": current,
        "round": picked,
        "scenes": scenes,
        "notes": here,
    }))
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchP {
    pub q: String,
}

/// One field for scenes, lines and notes, across every round.
pub fn search(svc: &Service, q: &str) -> Result<Value> {
    const EACH: usize = 6;
    let words: Vec<String> = q
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    if words.is_empty() {
        return Ok(json!({ "scenes": [], "lines": [], "notes": [] }));
    }
    let hit = |text: &str| {
        let t = text.to_lowercase();
        words.iter().all(|w| t.contains(w))
    };
    let doc = svc.doc();
    let tally = counts(doc);
    let order = doc.scenes_in_order();
    let scenes: Vec<Value> = order
        .iter()
        .filter(|s| {
            hit(&format!(
                "{} {} {} {}",
                s.heading, s.place, s.story.title, s.story.synopsis
            ))
        })
        .take(EACH)
        .map(|s| {
            json!({
                "id": s.id, "number": s.number, "place": s.place, "title": s.story.title,
                "text": if s.story.synopsis.is_empty() { &s.heading } else { &s.story.synopsis },
                "open": tally.get(&s.id).map(|c| c.1).unwrap_or(0),
            })
        })
        .collect();
    let mut lines = Vec::new();
    'scenes: for s in &order {
        for l in doc.lines_of_scene(&s.id) {
            if l.kind == LineKind::Dialogue && !l.removed && hit(&l.text) {
                if lines.len() == EACH {
                    break 'scenes;
                }
                let who = l
                    .character
                    .as_ref()
                    .and_then(|c| doc.characters.get(c))
                    .map(|c| c.label.clone())
                    .unwrap_or_default();
                lines.push(json!({
                    "id": l.id, "scene_id": s.id, "scene_number": s.number,
                    "who": who, "text": l.text,
                }));
            }
        }
    }
    let notes: Vec<NoteView> = all_notes(svc)
        .into_iter()
        .filter(|n| {
            let replies: Vec<&str> = n.replies.iter().map(|r| r.text.as_str()).collect();
            hit(&format!(
                "{} {} {} {}",
                n.text,
                n.quote,
                n.author.name,
                replies.join(" ")
            ))
        })
        .take(EACH)
        .collect();
    Ok(json!({ "scenes": scenes, "lines": lines, "notes": notes }))
}

#[derive(Default, Serialize, Clone)]
struct Tally {
    open: usize,
    resolved: usize,
    reopened: usize,
}

impl Tally {
    fn add(&mut self, s: NoteStatus) {
        match s {
            NoteStatus::Open => self.open += 1,
            NoteStatus::Resolved => self.resolved += 1,
            NoteStatus::Reopened => self.reopened += 1,
        }
    }
}

/// What the review history shows: built from the merged events, with no file opened.
pub fn history(svc: &Service) -> Result<Value> {
    let notes = all_notes(svc);
    let doc = svc.doc();
    let mut rounds: Vec<&ReviewRound> = doc.review_rounds.values().collect();
    rounds.sort_by_key(|r| std::cmp::Reverse(r.number));
    let summary = |round: Option<&str>| {
        let (mut all, mut scenes, mut people) = (
            Tally::default(),
            BTreeMap::<u32, Tally>::new(),
            BTreeMap::<String, Tally>::new(),
        );
        for n in notes.iter().filter(|n| n.round.as_deref() == round) {
            all.add(n.status);
            scenes
                .entry(n.scene_number.unwrap_or(0))
                .or_default()
                .add(n.status);
            people
                .entry(n.author.name.clone())
                .or_default()
                .add(n.status);
        }
        json!({ "all": all, "scenes": scenes, "people": people })
    };
    let rounds: Vec<Value> = rounds
        .iter()
        .map(|r| {
            let mut v = round_view(doc, r, &notes);
            v["summary"] = summary(Some(&r.id));
            let reviewers: HashSet<&str> = notes
                .iter()
                .filter(|n| n.round.as_deref() == Some(&r.id))
                .map(|n| n.author.name.as_str())
                .collect();
            v["reviewers"] = json!(reviewers);
            v
        })
        .collect();
    let mut arrivals: Vec<&ReviewArrival> = doc.review_arrivals.values().collect();
    arrivals.sort_by(|a, b| b.at.cmp(&a.at));
    let detached: Vec<&NoteView> = notes.iter().filter(|n| n.detached).collect();
    Ok(json!({
        "rounds": rounds,
        "outside": summary(None),
        "arrivals": arrivals,
        "detached": detached.len(),
        "folder": reviews_dir(&svc.paths).to_string_lossy(),
        "troubles": svc.review.troubles.iter().map(|(p, t)| json!({
            "file": p.file_name().map(|n| n.to_string_lossy().into_owned()),
            "text": t,
        })).collect::<Vec<_>>(),
    }))
}

// ---- files ----

fn read_review(path: &Path) -> Result<ReviewFile> {
    let bytes = std::fs::read(path)?;
    Ok(ReviewFile::decode(&bytes)?)
}

fn manifest_for(svc: &Service, who: &Reviewer, rounds: Vec<ReviewRound>) -> Result<ReviewManifest> {
    Ok(ReviewManifest {
        format: REVIEW_FORMAT,
        project: project_name(svc),
        episodes: vec![ManifestEpisode {
            id: episode_id(svc.bus.store())?,
            name: svc.name(),
        }],
        rounds,
        reviewer: who.clone(),
        device: host_name().to_string(),
        events: 0,
        checksum: String::new(),
    })
}

/// The rounds some events belong to.
fn rounds_of(doc: &Document, events: &[ReviewEvent]) -> Vec<ReviewRound> {
    let ids: HashSet<&str> = events.iter().filter_map(|e| e.round.as_deref()).collect();
    doc.review_rounds
        .values()
        .filter(|r| ids.contains(r.id.as_str()))
        .cloned()
        .collect()
}

/// Brings this machine's rolling files up to date: one per person who wrote here, and never
/// anyone else's. A file only ever grows, and it holds every episode of the show that person
/// reviewed, so what's in it stays. Nothing is touched when there's nothing new, so a sync tool
/// has nothing to carry.
pub fn publish(svc: &mut Service) -> Result<usize> {
    let doc = svc.doc();
    let ep = episode_id(svc.bus.store())?;
    let mut by: BTreeMap<String, (Reviewer, Vec<ReviewEvent>)> = BTreeMap::new();
    for e in doc.review_events.values() {
        if e.device == host_name() && e.episode == ep {
            by.entry(e.author.slug())
                .or_insert_with(|| (e.author.clone(), Vec::new()))
                .1
                .push(e.clone());
        }
    }
    // the editor's rounds travel even before anyone has said anything
    let me = editor(svc);
    if doc
        .review_rounds
        .values()
        .any(|r| r.created_by.slug() == me.slug())
    {
        by.entry(me.slug()).or_insert_with(|| (me, Vec::new()));
    }
    if by.is_empty() {
        svc.review.published = Some(svc.bus.rev());
        return Ok(0);
    }
    let dir = reviews_dir(&svc.paths);
    let mut written = 0;
    let mut stamps = Vec::new();
    for (slug, (who, events)) in by {
        let path = dir.join(format!("{slug}.{}", ext()));
        let old = read_review(&path).ok();
        let (old_events, old_rounds, old_episodes) = match old {
            Some(f) => (f.events, f.manifest.rounds, f.manifest.episodes),
            None => Default::default(),
        };
        let have: HashSet<&str> = old_events.iter().map(|e| e.id.as_str()).collect();
        let fresh: Vec<ReviewEvent> = events
            .into_iter()
            .filter(|e| !have.contains(e.id.as_str()))
            .collect();
        let mut all = old_events;
        all.extend(fresh.iter().cloned());
        // the file says what this episode's rounds are now: one undone doesn't come back from it
        let mut rounds: Vec<ReviewRound> = rounds_of(svc.doc(), &all)
            .into_iter()
            .chain(
                svc.doc()
                    .review_rounds
                    .values()
                    .filter(|r| r.created_by.slug() == slug)
                    .cloned(),
            )
            .collect();
        rounds.sort_by(|a, b| a.id.cmp(&b.id));
        rounds.dedup_by(|a, b| a.id == b.id);
        let (ours, others): (Vec<ReviewRound>, Vec<ReviewRound>) =
            old_rounds.into_iter().partition(|r| r.episode == ep);
        if fresh.is_empty() && ours == rounds {
            continue;
        }
        rounds.extend(others);
        let mut manifest = manifest_for(svc, &who, rounds)?;
        for e in old_episodes {
            if !manifest.episodes.iter().any(|x| x.id == e.id) {
                manifest.episodes.push(e);
            }
        }
        std::fs::create_dir_all(&dir)?;
        write_atomic(&path, &ReviewFile::new(manifest, all).encode()?)?;
        stamps.push((path.clone(), stamp_of(&path)));
        written += 1;
    }
    svc.review.seen.extend(stamps);
    svc.review.published = Some(svc.bus.rev());
    Ok(written)
}

/// What a merge brought.
#[derive(Default, Serialize, Debug, Clone)]
pub struct Merged {
    pub notes: usize,
    pub replies: usize,
    pub changes: usize,
    pub rounds: usize,
}

impl Merged {
    fn any(&self) -> bool {
        self.notes + self.replies + self.changes + self.rounds > 0
    }
}

/// The one merge, for the shared folder and for Add review alike: the union of events by id.
/// `episode` is the episode to read from the file.
fn merge(svc: &mut Service, file: &ReviewFile, ep: &str, via: ArrivalVia) -> Result<Merged> {
    let doc = svc.doc();
    let mine = episode_id(svc.bus.store())?;
    let wanted: Vec<ReviewEvent> = file
        .events
        .iter()
        .filter(|e| e.episode == ep)
        .map(|e| ReviewEvent {
            episode: mine.clone(),
            ..e.clone()
        })
        .collect();
    let fresh: Vec<ReviewEvent> = unseen(doc, &wanted).into_iter().cloned().collect();
    let rounds: Vec<ReviewRound> = file
        .manifest
        .rounds
        .iter()
        .filter(|r| match doc.review_rounds.get(&r.id) {
            // a round closed there is closed here
            Some(have) => have != *r && r.status == RoundStatus::Closed,
            None => true,
        })
        .filter(|r| r.episode == ep || r.episode.is_empty())
        .filter(|r| {
            r.scenes
                .iter()
                .any(|s| doc.scenes.contains_key(&s.scene_id))
        })
        .cloned()
        .collect();
    let mut got = Merged {
        rounds: rounds.len(),
        ..Merged::default()
    };
    // what someone else said, for the history. One's own file coming back is no arrival
    let me = current(svc).map(|m| m.slug());
    let mut by: BTreeMap<String, Merged> = BTreeMap::new();
    for e in &fresh {
        let m = by.entry(e.author.name.clone()).or_default();
        match e.body {
            ReviewBody::Note { .. } => {
                got.notes += 1;
                m.notes += 1;
            }
            ReviewBody::Reply { .. } => {
                got.replies += 1;
                m.replies += 1;
            }
            ReviewBody::Retract { .. } => got.changes += 1,
            _ => {
                got.changes += 1;
                m.changes += 1;
            }
        }
    }
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let arrivals: Vec<ReviewArrival> = by
        .iter()
        .filter(|(who, m)| {
            m.any()
                && fresh
                    .iter()
                    .any(|e| &e.author.name == *who && Some(e.author.slug()) != me)
        })
        .map(|(who, m)| ReviewArrival {
            id: new_id("arr"),
            at: at.clone(),
            who: who.clone(),
            via,
            notes: m.notes,
            replies: m.replies,
            changes: m.changes,
        })
        .collect();
    let params = json!({ "events": fresh.len(), "from": file.manifest.reviewer.name });
    svc.bus.merge_review(&params, |tx| {
        for e in &fresh {
            tx.put(e)?;
        }
        for r in &rounds {
            tx.put(r)?;
        }
        for a in &arrivals {
            tx.put(a)?;
        }
        Ok(())
    })?;
    Ok(got)
}

fn is_review_file(p: &Path) -> bool {
    p.is_file()
        && p.extension().and_then(|e| e.to_str()) == Some(ext())
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| !n.starts_with('.'))
}

/// One look at the world: a package that moved on, and what's new in the shared folder. The app
/// asks every tick; a file that hasn't changed isn't opened. A conflict copy from a sync tool
/// merges like any other, since a merge can't hurt.
pub fn sync(svc: &mut Service) -> Result<Value> {
    let reloaded = refresh_package(svc)?;
    let ep = episode_id(svc.bus.store())?;
    let dir = reviews_dir(&svc.paths);
    let mut got = Merged::default();
    let files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| is_review_file(p))
                .collect()
        })
        .unwrap_or_default();
    for path in &files {
        let now = stamp_of(path);
        if svc.review.seen.get(path) == Some(&now) && !reloaded {
            continue;
        }
        match read_review(path) {
            Ok(f) => {
                let m = merge(svc, &f, &ep, ArrivalVia::Folder)?;
                got.notes += m.notes;
                got.replies += m.replies;
                got.changes += m.changes;
                got.rounds += m.rounds;
                svc.review.troubles.remove(path);
            }
            // Syncthing writes to a temp name first, so this is a real problem with the file;
            // it's said in the history and tried again when the file changes
            Err(e) if !conflict_name(path) => {
                svc.review.troubles.insert(path.clone(), e.to_string());
            }
            Err(_) => {}
        }
        svc.review.seen.insert(path.clone(), now);
    }
    // what couldn't be written earlier (a share offline) goes out now
    let wrote = if svc.review.published == Some(svc.bus.rev()) {
        0
    } else {
        publish(svc).unwrap_or(0)
    };
    Ok(json!({ "merged": got, "reloaded": reloaded, "wrote": wrote, "rev": svc.bus.rev() }))
}

fn conflict_name(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(is_conflict_copy)
}

#[derive(Deserialize, JsonSchema)]
pub struct AddReviewP {
    pub path: String,
    /// Add it even though it belongs to another project: its notes stay detached where their
    /// elements are missing.
    #[serde(default)]
    pub anyway: bool,
}

/// `Reviews/Imported/2026-11-06 anna-weber.cutawayreview`, never over one already there.
fn archive(svc: &Service, from: &Path) -> Result<Option<PathBuf>> {
    let dir = reviews_dir(&svc.paths);
    // a rolling file stays where its owner writes it
    if from.parent() == Some(dir.as_path()) {
        return Ok(None);
    }
    let dest_dir = dir.join("Imported");
    if from.parent() == Some(dest_dir.as_path()) {
        return Ok(None);
    }
    std::fs::create_dir_all(&dest_dir)?;
    let day = chrono::Local::now().format("%Y-%m-%d");
    let name = from
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "review".into());
    let mut dest = dest_dir.join(format!("{day} {name}.{}", ext()));
    let mut n = 2;
    while dest.exists() {
        dest = dest_dir.join(format!("{day} {name} {n}.{}", ext()));
        n += 1;
    }
    if std::fs::rename(from, &dest).is_err() {
        std::fs::copy(from, &dest)?;
        std::fs::remove_file(from)?;
    }
    Ok(Some(dest))
}

/// Add review: the same file and the same merge as the shared folder, by hand.
pub fn add_review(svc: &mut Service, p: AddReviewP) -> Result<Value> {
    let path = PathBuf::from(&p.path);
    let file = read_review(&path)?;
    let ep = episode_id(svc.bus.store())?;
    let own = file.manifest.episodes.iter().any(|e| e.id == ep);
    let read_as = if own {
        ep
    } else {
        if !p.anyway {
            let names: Vec<&str> = file
                .manifest
                .episodes
                .iter()
                .map(|e| e.name.as_str())
                .collect();
            return Err(AppError::Other(format!(
                "This review is from {} ({}), not {}. Its notes would come in detached.",
                file.manifest.project,
                names.join(", "),
                svc.name()
            )));
        }
        file.manifest
            .episodes
            .first()
            .map(|e| e.id.clone())
            .unwrap_or_default()
    };
    let lost: Vec<String> = file
        .manifest
        .rounds
        .iter()
        .filter(|r| r.episode == read_as)
        .filter(|r| {
            r.scenes
                .iter()
                .any(|s| !svc.doc().scenes.contains_key(&s.scene_id))
                || !svc.doc().review_rounds.contains_key(&r.id)
        })
        .map(|r| format!("round {}", r.number))
        .collect();
    let m = merge(svc, &file, &read_as, ArrivalVia::File)?;
    let ids: HashSet<&str> = file.events.iter().map(|e| e.note.as_str()).collect();
    let notes = all_notes(svc);
    let detached = notes
        .iter()
        .filter(|n| ids.contains(n.id.as_str()) && n.detached)
        .count();
    let archived = archive(svc, &path)?;
    let notice = (!lost.is_empty() || detached > 0).then(|| {
        let cut = if lost.is_empty() {
            String::new()
        } else {
            format!(
                "It refers to a cut this project no longer has ({}). ",
                lost.join(", ")
            )
        };
        let stray = if detached > 0 {
            format!("{detached} of its notes are detached: their elements are gone.")
        } else {
            String::new()
        };
        format!("{cut}{stray}").trim().to_string()
    });
    Ok(json!({
        "who": file.manifest.reviewer.name,
        "notes": m.notes, "replies": m.replies, "changes": m.changes,
        "detached": detached,
        "notice": notice,
        "archived": archived.map(|p| p.to_string_lossy().into_owned()),
    }))
}

#[derive(Deserialize, JsonSchema)]
pub struct SendReviewP {
    /// The file to write, or the folder to write it in.
    pub path: String,
}

/// Send review: this person's events as one file, ready to hand over.
pub fn send_review(svc: &mut Service, p: SendReviewP) -> Result<Value> {
    let who = current(svc)
        .ok_or_else(|| AppError::Params("say who's reviewing first (set_reviewer)".into()))?;
    let ep = episode_id(svc.bus.store())?;
    let doc = svc.doc();
    let events: Vec<ReviewEvent> = doc
        .review_events
        .values()
        .filter(|e| e.episode == ep && e.author.slug() == who.slug())
        .cloned()
        .collect();
    if events.is_empty() {
        return Err(AppError::Other(format!(
            "{} has no notes to send yet",
            who.name
        )));
    }
    let rounds = rounds_of(doc, &events);
    let mut path = PathBuf::from(&p.path);
    if path.is_dir() {
        path = path.join(format!("{}.{}", who.slug(), ext()));
    } else if path.extension().and_then(|e| e.to_str()) != Some(ext()) {
        path.set_extension(ext());
    }
    let count = events.len();
    let notes = events
        .iter()
        .filter(|e| matches!(e.body, ReviewBody::Note { .. }))
        .count();
    let manifest = manifest_for(svc, &who, rounds)?;
    write_atomic(&path, &ReviewFile::new(manifest, events).encode()?)?;
    Ok(json!({ "file": path.to_string_lossy(), "events": count, "notes": notes }))
}

/// What the Export page says about a locked scene someone has written on.
pub fn locked_with_notes(doc: &Document) -> Vec<String> {
    let tally = counts(doc);
    doc.scenes_in_order()
        .iter()
        .filter(|s| s.lock.is_some())
        .filter_map(|s| {
            let open = tally.get(&s.id).map(|c| c.1).unwrap_or(0);
            (open > 0).then(|| {
                let notes = if open == 1 { "1 open note" } else { "open notes" };
                let n = if open == 1 { String::new() } else { format!("{open} ") };
                format!(
                    "{} is locked with {n}{notes}: unlock it and export again once they're dealt with.",
                    s.label()
                )
            })
        })
        .collect()
}

#[derive(Deserialize, JsonSchema)]
pub struct CloseRoundP {
    pub round: String,
}
