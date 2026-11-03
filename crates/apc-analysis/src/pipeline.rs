//! The analysis pipeline: media → sessions and sync → transcripts → alignment per role
//! track → takes → room classification → note linking → AI refinement → payload.
//!
//! Times: takes are stored in their source file's time; note linking works in session
//! time (file time − the file's sync offset).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use serde::Serialize;

use apc_ai::mock::{condense_gap, pick_strength};
use apc_ai::task::*;
use apc_ai::AiRunner;
use apc_core::project::EpisodePaths;
use apc_core::*;
use apc_media::{bwf, loudness, sync::measure_offset, AudioAnalysis, AudioReader};

use crate::align::{actions_in, active_scenes, align, dialogue_in, units_of, AlignOptions, Unit};
use crate::asr::{AsrProvider, AsrRequest, TWord, Transcript};
use crate::edges::refine;
use crate::error::Result;
use crate::rescue::{rescue_scene, Decision, Hint, MicTake, SceneJob};
use crate::sentences::{sentences, MergeOptions, Mic, Sentence};
use crate::sessions::{group_sessions, pair_rooms_by_name, FileSpan, NamedFile};
use crate::wordmap::{coverage, map_words};

/// Feedback window after a take, direction window before it (seconds). The room talk
/// between two takes is both windows together, read as one (DECISIONS.md D71).
pub const FEEDBACK_WINDOW_S: f64 = 8.0;
pub const DIRECTION_WINDOW_S: f64 = 60.0;
/// Room gaps sent to Claude in one call.
const GAPS_PER_CALL: usize = 40;
/// Takes quieter than this on their own mic are bleed, whatever the mic's gain.
/// (Stage 0 used −38 dB, which fit its fixture's gain but drops real takes from mics
/// recorded a few dB lower; see DECISIONS.md D44.)
pub const BLEED_DB: f64 = -50.0;
/// Takes this far below the mic's median take level are bleed: the director or the
/// reader heard across the room, not the actor on the close mic.
pub const RELATIVE_BLEED_DB: f64 = 10.0;

pub struct Providers<'a> {
    pub asr: &'a dyn AsrProvider,
    pub ai: &'a AiRunner,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub fraction: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RoleReport {
    pub role: String,
    pub source_id: String,
    pub takes: usize,
    pub lines_with_takes: usize,
    pub role_lines_total: usize,
    pub bleed_dropped: usize,
    /// Takes Claude found in speech the matcher left over.
    pub rescued: usize,
    pub unmatched: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MatchReport {
    pub roles: Vec<RoleReport>,
    pub cue_readings: usize,
    pub note_segments: usize,
    pub offsets: Vec<(String, f64)>,
    pub lines_matched: usize,
    pub takes_found: usize,
    pub picks_found: usize,
    pub flagged: Vec<String>,
    pub markdown: String,
}

/// Matching settings (from `[analysis]` in the project config; the learning section
/// tries variants of them).
#[derive(Debug, Clone)]
pub struct Tuning {
    pub merge_gap_s: f64,
    pub bleed_margin_db: f64,
    pub match_offset: f64,
    /// Misheard words: (heard, script), whole words. Fixed in the transcript before
    /// matching when both have the same number of words.
    pub glossary: Vec<(String, String)>,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            merge_gap_s: MergeOptions::default().max_gap,
            bleed_margin_db: RELATIVE_BLEED_DB,
            match_offset: 0.0,
            glossary: vec![],
        }
    }
}

/// Replaces misheard words in a transcript (word for word, so timings stay).
pub fn apply_glossary(tr: &mut Transcript, glossary: &[(String, String)]) -> usize {
    let mut fixed = 0;
    for (heard, script) in glossary {
        let h = apc_core::text::norm_words(heard);
        let sc = apc_core::text::norm_words(script);
        if h.is_empty() || h.len() != sc.len() || h == sc {
            continue;
        }
        let mut i = 0;
        while i + h.len() <= tr.words.len() {
            if tr.words[i..i + h.len()]
                .iter()
                .zip(&h)
                .all(|(w, x)| &w.w == x)
            {
                for (k, x) in sc.iter().enumerate() {
                    tr.words[i + k].w = x.clone();
                }
                fixed += 1;
                i += h.len();
            } else {
                i += 1;
            }
        }
    }
    fixed
}

/// What a run covers. Empty means the whole episode.
#[derive(Debug, Clone, Default)]
pub struct AnalysisScope {
    /// Only these characters' sessions (their session mics and director mics).
    pub characters: Vec<String>,
    /// Only these lines' takes (notes and unmatched speech stay as they are).
    pub line_ids: Vec<String>,
    /// Dev-panel marks: judge only these stretches and return proposals; nothing else.
    pub marks: Vec<DevMark>,
    /// With marks: link the marked stretch to its line as is, without asking Claude.
    pub manual: bool,
    /// Ask Claude about session-mic speech the matcher left over.
    pub rescue: bool,
    /// Worked examples from earlier corrections, for the rescue prompt.
    pub examples: Vec<String>,
    pub tuning: Tuning,
}

impl AnalysisScope {
    pub fn everything() -> Self {
        AnalysisScope {
            rescue: true,
            ..Default::default()
        }
    }

    fn is_scoped(&self) -> bool {
        !self.characters.is_empty() || !self.line_ids.is_empty() || !self.marks.is_empty()
    }
}

pub struct AnalysisOutput {
    pub payload: AnalysisPayload,
    pub report: MatchReport,
    /// With marks in the scope: one proposal per mark id.
    pub proposals: Vec<(String, MarkProposal)>,
    /// Each character's vocal sounds (breaths, "mh-hm", efforts …) for their vocal bucket.
    pub vocals: Vec<crate::vocals::VocalCandidate>,
}

/// Deterministic id from parts (sessions are named by their members).
pub fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let h = blake3::hash(parts.join("|").as_bytes()).to_hex();
    format!("{prefix}_{}", &h[..12])
}

struct SourceData {
    src: SourceFile,
    path: PathBuf,
    analysis: AudioAnalysis,
    /// Transcript in session time.
    tr: Transcript,
}

/// A take before it becomes an entity.
#[derive(Clone)]
struct RawTake {
    unit: usize,
    source: usize,
    score: f64,
    w0: usize,
    w1: usize,
    /// file time
    t0: f64,
    t1: f64,
    /// session time
    s0: f64,
    s1: f64,
    db: f64,
    heard: String,
    bleed: bool,
    before: Vec<usize>,
    after: Vec<usize>,
    origin: TakeOrigin,
    read_as: Option<String>,
}

/// A take candidate on source `k` for words `[w0, w1)`: edges from the segments, snapped
/// to the audio, and its level.
fn raw_take(
    d: &SourceData,
    k: usize,
    unit: usize,
    score: f64,
    w0: usize,
    w1: usize,
    thr: f32,
) -> RawTake {
    let words = &d.tr.words;
    let off = d.src.offset_s;
    let (mut s0, mut s1) = (words[w0].t0, words[w1 - 1].t1);
    let sf = &d.tr.segments[words[w0].seg];
    let sl = &d.tr.segments[words[w1 - 1].seg];
    if w0 == sf.w0 {
        s0 = sf.start;
    }
    if w1 == sl.w1 {
        s1 = sl.end;
    }
    let db = d.analysis.rms_db(s0 + off, s1 + off);
    let (r0, r1) = refine(&d.analysis.env, s0 + off, s1 + off, thr);
    RawTake {
        unit,
        source: k,
        score,
        w0,
        w1,
        t0: r0,
        t1: r1,
        s0: r0 - off,
        s1: r1 - off,
        db,
        heard: words[w0..w1]
            .iter()
            .map(|w| w.raw.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        bleed: db < BLEED_DB,
        before: vec![],
        after: vec![],
        origin: TakeOrigin::Matcher,
        read_as: None,
    }
}

/// A rescued (or hand-linked) take from a checked decision.
fn decided_take(
    d: &SourceData,
    k: usize,
    units: &[Unit],
    dec: &Decision,
    thr: f32,
    origin: TakeOrigin,
) -> Option<RawTake> {
    let u = dec.unit?;
    let heard: Vec<String> = d.tr.words[dec.w0..dec.w1]
        .iter()
        .map(|w| w.w.clone())
        .collect();
    let score = crate::fuzzy::ratio(&units[u].target, &heard.join(" ")).round();
    let mut t = raw_take(d, k, u, score, dec.w0, dec.w1, thr);
    t.bleed = false;
    t.origin = origin;
    t.read_as = (!dec.read_as.is_empty()).then(|| dec.read_as.clone());
    Some(t)
}

/// The takes entity of a raw take (notes are filled in later).
fn build_take(
    r: &RawTake,
    u: &Unit,
    line: &ScriptLine,
    d: &SourceData,
    take_no: u32,
    readers: &mut HashMap<usize, AudioReader>,
) -> Take {
    let off = d.src.offset_s;
    let tw = &d.tr.words[r.w0..r.w1];
    let heard_norm: Vec<String> = tw.iter().map(|w| w.w.clone()).collect();
    // a rescued take was misheard: read Claude's reading against the script when it
    // has the same number of words, so the word map still lines up
    let read: Vec<String> = r
        .read_as
        .as_deref()
        .map(apc_core::text::norm_words)
        .filter(|v| v.len() == heard_norm.len())
        .unwrap_or_else(|| heard_norm.clone());
    let map = map_words(&read, &line.words);
    let cov = coverage(&map, line.words.len());
    let first = map.iter().flatten().next().copied().unwrap_or(0);
    let score_ok = r.score >= 85.0 || r.origin != TakeOrigin::Matcher;
    let take_type = if tw.len() <= 2 && line.words.len() > 3 {
        TakeType::Micro
    } else if first >= 2.max((line.words.len() as f64 * 0.3) as usize) {
        TakeType::Pickup
    } else if !score_ok || cov < 0.6 {
        TakeType::Partial
    } else {
        TakeType::Full
    };
    let lufs = {
        let rd = match readers.entry(r.source) {
            std::collections::hash_map::Entry::Occupied(e) => Some(e.into_mut()),
            std::collections::hash_map::Entry::Vacant(v) => {
                AudioReader::open(&d.path).ok().map(|x| v.insert(x))
            }
        };
        rd.and_then(|rd| rd.read_range(r.t0, r.t1).ok())
            .and_then(|s| {
                loudness::integrated_lufs(&s, d.analysis.info.sample_rate, d.analysis.info.channels)
                    .ok()
                    .flatten()
            })
            .map(|l| (l * 10.0).round() / 10.0)
    };
    Take {
        id: stable_id("tk", &[&u.id, &d.src.id, &format!("{:.1}", r.t0)]),
        line_id: u.id.clone(),
        source_id: d.src.id.clone(),
        take_no,
        take_type,
        t0: (r.t0 * 1000.0).round() / 1000.0,
        t1: (r.t1 * 1000.0).round() / 1000.0,
        words: tw
            .iter()
            .zip(map.iter())
            .map(|(w, m)| TakeWord {
                w: w.w.clone(),
                raw: w.raw.clone(),
                t0: w.t0 + off,
                t1: w.t1 + off,
                script_idx: *m,
            })
            .collect(),
        score: r.score,
        heard: r.heard.clone(),
        lufs,
        db: (r.db * 10.0).round() / 10.0,
        partial: matches!(take_type, TakeType::Partial | TakeType::Micro),
        pick: 0,
        favorite: false,
        favorite_origin: None,
        direction: String::new(),
        feedback: String::new(),
        direction_raw: String::new(),
        feedback_raw: String::new(),
        tags: vec![],
        note_ids: vec![],
        confidence: if r.origin == TakeOrigin::Matcher {
            (r.score / 100.0).min(1.0)
        } else {
            0.9
        },
        needs_rematch: false,
        ai_flag: None,
        origin: r.origin,
        read_as: r.read_as.clone(),
    }
}

struct RoomSeg {
    id: String,
    source_id: String,
    /// session time
    t0: f64,
    t1: f64,
    text: String,
    class: NoteClass,
    line_id: Option<String>,
    take_ids: Vec<String>,
}

fn transcript_cached(
    p: &Providers,
    paths: &EpisodePaths,
    src: &SourceFile,
    audio: &std::path::Path,
    hotwords: &[String],
) -> Result<Transcript> {
    let file = paths
        .cache
        .join("asr")
        .join(format!("{}-{}.json", src.hash, p.asr.id()));
    if let Ok(s) = std::fs::read_to_string(&file) {
        if let Ok(t) = serde_json::from_str::<Transcript>(&s) {
            return Ok(t);
        }
    }
    let t = p.asr.transcribe(&AsrRequest {
        audio: audio.to_path_buf(),
        sidecar: src.transcript.as_ref().map(|s| paths.resolve(s)),
        hotwords: hotwords.to_vec(),
        language: "en".into(),
    })?;
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&file, serde_json::to_string(&t)?)?;
    Ok(t)
}

fn fmt_t(t: f64) -> String {
    format!("{:02}:{:05.2}", (t / 60.0).floor() as i64, t % 60.0)
}

/// Runs the whole analysis over the document's sources.
pub fn analyze(
    doc: &Document,
    paths: &EpisodePaths,
    p: &Providers,
    progress: &dyn Fn(Progress),
) -> Result<AnalysisOutput> {
    analyze_scoped(doc, paths, p, &AnalysisScope::everything(), progress)
}

/// Runs the analysis for a scope: some characters, some lines, or dev-panel marks.
/// The deterministic part always sees everything (it is cheap and keeps ids stable);
/// AI calls and the payload stay inside the scope.
pub fn analyze_scoped(
    doc: &Document,
    paths: &EpisodePaths,
    p: &Providers,
    scope: &AnalysisScope,
    progress: &dyn Fn(Progress),
) -> Result<AnalysisOutput> {
    let say = |stage: &str, fraction: f64, message: String| {
        progress(Progress {
            stage: stage.into(),
            fraction,
            message,
        })
    };
    let units = units_of(doc);
    let opts = AlignOptions {
        threshold_offset: scope.tuning.match_offset,
        ..AlignOptions::default()
    };
    let margin = scope.tuning.bleed_margin_db;
    let mut flagged: Vec<String> = Vec::new();
    let hotwords: Vec<String> = doc.characters.values().map(|c| c.label.clone()).collect();

    // 1. Media analysis (cached by hash) and BWF timecode.
    let mut sources: Vec<SourceData> = Vec::new();
    let srcs: Vec<&SourceFile> = doc
        .sources
        .values()
        .filter(|s| !s.missing && s.role != Role::Unknown)
        .collect();
    for (k, s) in srcs.iter().enumerate() {
        say(
            "media",
            k as f64 / srcs.len().max(1) as f64,
            format!("Reading {}", s.original_name),
        );
        let path = paths.resolve(&s.path);
        let analysis = AudioAnalysis::cached(&path, &s.hash, &paths.cache.join("media"))?;
        let mut src = (*s).clone();
        src.sample_rate = analysis.info.sample_rate;
        src.channels = analysis.info.channels;
        src.duration_s = analysis.info.duration_s;
        if let Ok(Some(b)) = bwf::read_bwf(&path) {
            src.tc_start_s = b.tc_start_s();
        }
        sources.push(SourceData {
            src,
            path,
            analysis,
            tr: Transcript::default(),
        });
    }

    // 2. Sessions and sync.
    let spans: Vec<FileSpan> = sources
        .iter()
        .map(|d| FileSpan {
            id: d.src.id.clone(),
            tc_start_s: d.src.tc_start_s,
            duration_s: d.src.duration_s,
            batch: std::path::Path::new(&d.src.path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })
        .collect();
    // The Import page's match (with the user's room links) wins when it covers every file.
    let matched: Vec<Vec<String>> = doc
        .sessions
        .values()
        .map(|s| s.source_ids.clone())
        .filter(|g| !g.is_empty())
        .collect();
    let covered = sources
        .iter()
        .all(|d| matched.iter().any(|g| g.contains(&d.src.id)));
    let groups = if covered && !matched.is_empty() {
        matched
    } else {
        let named: Vec<NamedFile> = sources
            .iter()
            .map(|d| NamedFile {
                id: d.src.id.clone(),
                name: d.src.original_name.clone(),
                role: d.src.role,
                character: d.src.character.clone(),
            })
            .collect();
        let characters: Vec<String> = doc.characters.keys().cloned().collect();
        let aliases: BTreeMap<String, String> = doc
            .characters
            .values()
            .flat_map(|c| c.aliases.iter().map(move |a| (a.clone(), c.id.clone())))
            .collect();
        pair_rooms_by_name(group_sessions(&spans), &named, &characters, &aliases)
    };
    let mut sessions: Vec<Session> = Vec::new();
    let mut report = MatchReport::default();
    for (gi, g) in groups.iter().enumerate() {
        let mut members: Vec<usize> = g
            .iter()
            .filter_map(|id| sources.iter().position(|d| &d.src.id == id))
            .collect();
        members.sort_by_key(|&i| {
            (
                sources[i].src.role != Role::Character,
                sources[i].src.original_name.clone(),
            )
        });
        let Some(&reference) = members.first() else {
            continue;
        };
        let sid = stable_id("ss", &g.iter().map(String::as_str).collect::<Vec<_>>());
        let name = std::path::Path::new(&sources[reference].src.path)
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Session {}", gi + 1));
        for &m in &members {
            say(
                "sync",
                gi as f64 / groups.len().max(1) as f64,
                format!("Syncing {}", sources[m].src.original_name),
            );
            let (off, conf) = if m == reference {
                (0.0, 1.0)
            } else {
                let r = measure_offset(&sources[reference].analysis, &sources[m].analysis, 3.0);
                (r.offset_s, r.confidence)
            };
            sources[m].src.offset_s = off;
            sources[m].src.offset_confidence = conf;
            sources[m].src.session_id = Some(sid.clone());
            if m != reference {
                report.offsets.push((sources[m].src.id.clone(), off));
            }
        }
        sessions.push(Session {
            id: sid,
            name,
            tc_start_s: sources[reference].src.tc_start_s,
            source_ids: members.iter().map(|&m| sources[m].src.id.clone()).collect(),
        });
    }

    // 3. Transcripts (session time).
    for k in 0..sources.len() {
        say(
            "transcribe",
            k as f64 / sources.len().max(1) as f64,
            format!("Transcribing {}", sources[k].src.original_name),
        );
        let path = sources[k].path.clone();
        let t = transcript_cached(p, paths, &sources[k].src, &path, &hotwords)?;
        sources[k].tr = t.shifted(-sources[k].src.offset_s);
        apply_glossary(&mut sources[k].tr, &scope.tuning.glossary);
    }

    // Scope: which characters' sessions take part beyond the deterministic pass.
    let line_scope: BTreeSet<String> = scope.line_ids.iter().cloned().collect();
    let mut chars: BTreeSet<String> = scope.characters.iter().cloned().collect();
    for lid in &scope.line_ids {
        if let Some(c) = doc.lines.get(lid).and_then(|l| l.character.clone()) {
            chars.insert(c);
        }
    }
    for m in &scope.marks {
        if let Some(c) = doc
            .sources
            .get(&m.source_id)
            .and_then(|s| s.character.clone())
        {
            chars.insert(c);
        }
    }
    let in_chars = |c: &str| chars.is_empty() || chars.contains(c);
    let scoped_sessions: BTreeSet<String> = sources
        .iter()
        .filter(|d| d.src.role == Role::Character)
        .filter(|d| d.src.character.as_deref().is_some_and(in_chars))
        .filter_map(|d| d.src.session_id.clone())
        .collect();
    let in_session = |d: &SourceData| {
        !scope.is_scoped()
            || d.src
                .session_id
                .as_ref()
                .is_some_and(|s| scoped_sessions.contains(s))
    };
    let want_rescue = scope.rescue || !scope.marks.is_empty();
    let merge = MergeOptions {
        max_gap: scope.tuning.merge_gap_s,
        ..MergeOptions::default()
    };
    let sents: Vec<Vec<Sentence>> = sources
        .iter()
        .map(|d| {
            if want_rescue && in_session(d) {
                sentences(&d.tr, &merge)
            } else {
                vec![]
            }
        })
        .collect();
    let slevels: Vec<Vec<f64>> = sources
        .iter()
        .zip(&sents)
        .map(|(d, ss)| {
            ss.iter()
                .map(|s| {
                    d.analysis
                        .rms_db(s.t0 + d.src.offset_s, s.t1 + d.src.offset_s)
                })
                .collect()
        })
        .collect();
    let n_scenes = units.iter().map(|u| u.scene_idx + 1).max().unwrap_or(0);

    // 4. Role tracks: the matcher, then Claude for what it left over.
    let mut raw: Vec<RawTake> = Vec::new();
    let mut unmatched: Vec<Unmatched> = Vec::new();
    let mut near_by_session: HashMap<String, BTreeSet<usize>> = HashMap::new();
    let mut quiet_by_source: HashMap<usize, Option<f64>> = HashMap::new();
    for (k, d) in sources.iter().enumerate() {
        if d.src.role != Role::Character {
            continue;
        }
        let Some(role) = d.src.character.clone() else {
            continue;
        };
        if !in_chars(&role) {
            continue;
        }
        say(
            "align",
            k as f64 / sources.len().max(1) as f64,
            format!("Matching {role}"),
        );
        let words = &d.tr.words;
        let near = active_scenes(&units, words, &role, &opts);
        near_by_session
            .entry(d.src.session_id.clone().unwrap_or_default())
            .or_default()
            .extend(near.iter().copied());
        let speak = dialogue_in(&units, &near);
        let (hits, mut used) = align(&units, &speak, words, &opts);
        let thr = d.analysis.percentile(60.0) * 3.0;
        let off = d.src.offset_s;
        let mut mine: Vec<RawTake> = hits
            .iter()
            .filter(|h| units[h.unit].character.as_deref() == Some(role.as_str()))
            .map(|h| raw_take(d, k, h.unit, h.score, h.w0, h.w1, thr))
            .collect();
        // Bleed: quiet in absolute terms, or far below this mic's typical take level.
        let mut levels: Vec<f64> = mine.iter().map(|t| t.db).collect();
        levels.sort_by(f64::total_cmp);
        let median = levels.get(levels.len() / 2).copied();
        quiet_by_source.insert(k, median);
        let quiet = |db: f64| db < BLEED_DB || median.is_some_and(|m| db < m - margin);
        for t in mine.iter_mut() {
            t.bleed = quiet(t.db);
        }

        // Rescue: Claude reads each scene of this session and judges the left-overs.
        let mut rescued = 0;
        if scope.rescue && scope.marks.is_empty() {
            let (mics, mic_words) = mics_of(&sources, k, &role, &sents, &slevels);
            let role_scenes: BTreeSet<usize> = units
                .iter()
                .filter(|u| {
                    u.kind == LineKind::Dialogue && u.character.as_deref() == Some(role.as_str())
                })
                .filter(|u| line_scope.is_empty() || line_scope.contains(&u.id))
                .map(|u| u.scene_idx)
                .collect();
            for &sc in &role_scenes {
                let Some((lo, hi)) = stretch(&units, &mine, sc, n_scenes) else {
                    continue;
                };
                say(
                    "rescue",
                    k as f64 / sources.len().max(1) as f64,
                    format!("Reading {role}'s takes, scene {}", sc + 1),
                );
                let takes: Vec<MicTake> = mine
                    .iter()
                    .filter(|t| !t.bleed)
                    .map(|t| MicTake {
                        unit: t.unit,
                        w0: t.w0,
                        w1: t.w1,
                    })
                    .collect();
                let job = SceneJob {
                    doc,
                    units: &units,
                    scene_idx: sc,
                    role: &role,
                    mics: &mics,
                    words: &mic_words,
                    takes: &takes,
                    quiet: &quiet,
                    lo,
                    hi,
                    hints: &[],
                    examples: &scope.examples,
                };
                let r = rescue_scene(&job, p.ai);
                flagged.extend(r.flagged);
                for dec in &r.decisions {
                    let Some(u) = dec.unit else { continue };
                    if !line_scope.is_empty() && !line_scope.contains(&units[u].id) {
                        continue;
                    }
                    let clash = mine
                        .iter()
                        .any(|t| !t.bleed && t.w0 < dec.w1 && dec.w0 < t.w1);
                    if clash {
                        continue;
                    }
                    if let Some(t) = decided_take(d, k, &units, dec, thr, TakeOrigin::Rescue) {
                        used.extend(t.w0..t.w1);
                        mine.push(t);
                        rescued += 1;
                    }
                }
            }
        }

        mine.sort_by(|a, b| a.t0.total_cmp(&b.t0));
        for i in 1..mine.len() {
            let (a, b) = (&mine[i - 1], &mine[i]);
            if a.t1 > b.t0 {
                let mid = (words[a.w1 - 1].t1 + words[b.w0].t0) / 2.0 + off;
                let (at1, bt0) = (a.t1.min(mid), b.t0.max(mid));
                mine[i - 1].t1 = at1;
                mine[i - 1].s1 = at1 - off;
                mine[i].t0 = bt0;
                mine[i].s0 = bt0 - off;
            }
        }
        let mut role_report = RoleReport {
            role: role.clone(),
            source_id: d.src.id.clone(),
            bleed_dropped: mine.iter().filter(|t| t.bleed).count(),
            rescued,
            role_lines_total: units
                .iter()
                .filter(|u| u.character.as_deref() == Some(role.as_str()))
                .count(),
            ..Default::default()
        };
        // Unmatched loud speech on the role track.
        for s in &d.tr.segments {
            let n = s.w1 - s.w0;
            if n == 0 {
                continue;
            }
            let cov = (s.w0..s.w1).filter(|w| used.contains(w)).count() as f64 / n as f64;
            let db = d.analysis.rms_db(s.start + off, s.end + off);
            if cov < 0.5 && !quiet(db) {
                unmatched.push(Unmatched {
                    id: stable_id("um", &[&d.src.id, &format!("{:.2}", s.start + off)]),
                    scene_id: None,
                    source_id: d.src.id.clone(),
                    t0: s.start + off,
                    t1: s.end + off,
                    text: s.text.clone(),
                    db: (db * 10.0).round() / 10.0,
                    origin: "analysis".into(),
                    used_as: None,
                });
                role_report.unmatched += 1;
            }
        }
        report.roles.push(role_report);
        raw.extend(mine);
    }

    // Marks from the dev panel: judge those stretches only and return proposals.
    if !scope.marks.is_empty() {
        let proposals = propose_marks(
            doc,
            &units,
            &sources,
            &sents,
            &slevels,
            &raw,
            &quiet_by_source,
            scope,
            p.ai,
            n_scenes,
            &mut flagged,
        );
        report.flagged = flagged;
        return Ok(AnalysisOutput {
            payload: AnalysisPayload::default(),
            report,
            proposals,
            vocals: vec![],
        });
    }

    // 5. Room tracks: cue reading vs notes.
    let mut room: Vec<RoomSeg> = Vec::new();
    for d in sources
        .iter()
        .filter(|d| d.src.role == Role::Room && in_session(d))
    {
        say(
            "room",
            0.0,
            format!("Reading the room track {}", d.src.original_name),
        );
        let near = near_by_session
            .get(&d.src.session_id.clone().unwrap_or_default())
            .cloned()
            .unwrap_or_default();
        let mut subset = dialogue_in(&units, &near);
        subset.extend(actions_in(&units, &near));
        let words = &d.tr.words;
        let (rhits, rused) = align(&units, &subset, words, &opts);
        let mut cue_line: HashMap<usize, String> = HashMap::new();
        for h in &rhits {
            for w in h.w0..h.w1 {
                cue_line.insert(w, units[h.unit].id.clone());
            }
        }
        report.cue_readings += rhits
            .iter()
            .filter(|h| units[h.unit].kind == LineKind::Dialogue)
            .count();
        for s in &d.tr.segments {
            let n = s.w1 - s.w0;
            if n == 0 {
                continue;
            }
            let covered = (s.w0..s.w1).filter(|w| rused.contains(w)).count() as f64 / n as f64;
            let is_cue = covered >= 0.6;
            room.push(RoomSeg {
                id: stable_id("nt", &[&d.src.id, &format!("{:.2}", s.start)]),
                source_id: d.src.id.clone(),
                t0: s.start,
                t1: s.end,
                text: s.text.clone(),
                class: if is_cue {
                    NoteClass::CueReading
                } else {
                    NoteClass::Chatter
                },
                line_id: if is_cue {
                    cue_line.get(&s.w0).cloned()
                } else {
                    None
                },
                take_ids: vec![],
            });
        }
    }
    report.note_segments = room
        .iter()
        .filter(|r| r.class != NoteClass::CueReading)
        .count();

    // 6. Note linking per session (feedback = 8 s after a take, direction = 30 s before).
    let mut real: Vec<usize> = (0..raw.len()).filter(|&i| !raw[i].bleed).collect();
    real.sort_by(|&a, &b| raw[a].s0.total_cmp(&raw[b].s0));
    let session_of = |t: &RawTake| sources[t.source].src.session_id.clone();
    for (pos, &ti) in real.iter().enumerate() {
        let t = raw[ti].clone();
        let same: Vec<usize> = real
            .iter()
            .copied()
            .filter(|&o| session_of(&raw[o]) == session_of(&t))
            .collect();
        let idx = same.iter().position(|&o| o == ti).unwrap_or(pos);
        let prev_end = if idx > 0 { raw[same[idx - 1]].s1 } else { 0.0 };
        let next_start = same.get(idx + 1).map(|&o| raw[o].s0).unwrap_or(f64::MAX);
        let room_in_session = |r: &RoomSeg| {
            sources
                .iter()
                .find(|d| d.src.id == r.source_id)
                .and_then(|d| d.src.session_id.clone())
                == session_of(&t)
        };
        let block = |a: f64, b: f64| -> Vec<usize> {
            room.iter()
                .enumerate()
                .filter(|(_, r)| r.class != NoteClass::CueReading && room_in_session(r))
                .filter(|(_, r)| r.t0 >= a - 0.3 && r.t0 < b - 0.3)
                .map(|(i, _)| i)
                .collect()
        };
        raw[ti].before = block(prev_end.max(t.s0 - DIRECTION_WINDOW_S), t.s0);
        raw[ti].after = block(t.s1, next_start.min(t.s1 + FEEDBACK_WINDOW_S));
    }
    let mut is_after: BTreeSet<usize> = BTreeSet::new();
    let mut is_before: BTreeSet<usize> = BTreeSet::new();
    for &ti in &real {
        is_after.extend(raw[ti].after.iter().copied());
        is_before.extend(raw[ti].before.iter().copied());
    }
    for (i, r) in room.iter_mut().enumerate() {
        if r.class == NoteClass::CueReading {
            continue;
        }
        r.class = if is_after.contains(&i) {
            NoteClass::Feedback
        } else if is_before.contains(&i) {
            NoteClass::Direction
        } else {
            NoteClass::Chatter
        };
    }

    // 7. Takes per line: numbering, ids, word maps, types.
    let mut by_unit: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &ti in &real {
        by_unit.entry(raw[ti].unit).or_default().push(ti);
    }
    let mut takes: Vec<Take> = Vec::new();
    let mut after_texts: HashMap<String, Vec<String>> = HashMap::new();
    let mut readers: HashMap<usize, AudioReader> = HashMap::new();
    for (unit_i, list) in by_unit.iter_mut() {
        list.sort_by(|&a, &b| raw[a].t0.total_cmp(&raw[b].t0));
        let u: &Unit = &units[*unit_i];
        let line = &doc.lines[&u.id];
        for (n, &ti) in list.iter().enumerate() {
            let r = &raw[ti];
            let d = &sources[r.source];
            let t = build_take(r, u, line, d, (n + 1) as u32, &mut readers);
            after_texts.insert(
                t.id.clone(),
                r.after.iter().map(|&i| room[i].text.clone()).collect(),
            );
            takes.push(t);
        }
    }

    // 8. AI refinement per scene (deterministic fallbacks when a task is flagged).
    let scene_of_line = |lid: &str| {
        doc.lines
            .get(lid)
            .map(|l| l.scene_id.clone())
            .unwrap_or_default()
    };
    let mut scenes_with_takes: Vec<String> =
        takes.iter().map(|t| scene_of_line(&t.line_id)).collect();
    scenes_with_takes.sort();
    scenes_with_takes.dedup();
    if !line_scope.is_empty() {
        let keep: BTreeSet<String> = line_scope.iter().map(|l| scene_of_line(l)).collect();
        scenes_with_takes.retain(|s| keep.contains(s));
    }
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    for (si, scene_id) in scenes_with_takes.iter().enumerate() {
        say(
            "notes",
            si as f64 / scenes_with_takes.len().max(1) as f64,
            "Checking matches and finding picks".into(),
        );
        let idx: Vec<usize> = (0..takes.len())
            .filter(|&i| &scene_of_line(&takes[i].line_id) == scene_id)
            .filter(|&i| line_scope.is_empty() || line_scope.contains(&takes[i].line_id))
            .collect();
        let line_text = |t: &Take| {
            doc.lines
                .get(&t.line_id)
                .map(|l| l.text.clone())
                .unwrap_or_default()
        };

        // Ambiguous matches.
        let ambiguous: Vec<usize> = idx
            .iter()
            .copied()
            .filter(|&i| takes[i].score < 85.0 && takes[i].origin == TakeOrigin::Matcher)
            .collect();
        if !ambiguous.is_empty() {
            let input = LineMatchInput {
                segments: ambiguous
                    .iter()
                    .map(|&i| TranscriptSegment {
                        index: i,
                        t0: takes[i].t0,
                        t1: takes[i].t1,
                        text: takes[i].heard.clone(),
                        proposed_line_id: Some(takes[i].line_id.clone()),
                        score: takes[i].score,
                    })
                    .collect(),
                candidates: doc
                    .lines_of_scene(scene_id)
                    .iter()
                    .filter(|l| l.kind == LineKind::Dialogue)
                    .map(|l| CandidateLine {
                        line_id: l.id.clone(),
                        character: l.character.clone().unwrap_or_default(),
                        text: l.text.clone(),
                    })
                    .collect(),
            };
            match p.ai.run(&input) {
                Ok(out) => match out.output {
                    Some(o) => {
                        for m in o.matches {
                            let Some(t) = takes.get_mut(m.index) else {
                                continue;
                            };
                            match &m.line_id {
                                None => {
                                    dropped.insert(t.id.clone());
                                }
                                Some(lid) if lid == &t.line_id => {
                                    if m.take_type == MatchTakeType::Partial {
                                        t.take_type = TakeType::Partial;
                                        t.partial = true;
                                    }
                                }
                                Some(_) => {
                                    t.ai_flag = Some("the AI matched this to another line".into())
                                }
                            }
                        }
                    }
                    None => flagged.extend(out.flagged),
                },
                Err(e) => flagged.push(format!("line_match: {e}")),
            }
        }

        // Room classification: only the director mic of each take's own session, and
        // only around the takes (where notes can attach). Every session clock starts at
        // zero, so a plain time window would pull in every other session too.
        let around: Vec<(Option<String>, f64, f64)> = idx
            .iter()
            .filter_map(|&i| {
                let d = sources.iter().find(|d| d.src.id == takes[i].source_id)?;
                let off = d.src.offset_s;
                Some((
                    d.src.session_id.clone(),
                    takes[i].t0 - off - DIRECTION_WINDOW_S,
                    takes[i].t1 - off + FEEDBACK_WINDOW_S,
                ))
            })
            .collect();
        let session_of_source = |id: &str| {
            sources
                .iter()
                .find(|d| d.src.id == id)
                .and_then(|d| d.src.session_id.clone())
        };
        let segs: Vec<usize> = (0..room.len())
            .filter(|&r| {
                let ses = session_of_source(&room[r].source_id);
                room[r].class != NoteClass::CueReading
                    && around
                        .iter()
                        .any(|(s, a, b)| *s == ses && room[r].t0 >= *a && room[r].t0 <= *b)
            })
            .collect();
        if !segs.is_empty() {
            let to_ai = |c: NoteClass| match c {
                NoteClass::CueReading => RoomClass::CueReading,
                NoteClass::Direction => RoomClass::Direction,
                NoteClass::Feedback => RoomClass::Feedback,
                NoteClass::Chatter => RoomClass::Chatter,
            };
            let input = RoomClassifyInput {
                segments: segs
                    .iter()
                    .map(|&r| RoomSegment {
                        id: room[r].id.clone(),
                        t0: room[r].t0,
                        t1: room[r].t1,
                        text: room[r].text.clone(),
                        hint: to_ai(room[r].class),
                    })
                    .collect(),
                scene_script: doc
                    .lines_of_scene(scene_id)
                    .iter()
                    .map(|l| ScriptItem {
                        line_id: l.id.clone(),
                        kind: format!("{:?}", l.kind).to_lowercase(),
                        character: l.character.clone(),
                        text: l.text.clone(),
                    })
                    .collect(),
            };
            match p.ai.run(&input) {
                Ok(out) => match out.output {
                    Some(o) => {
                        for c in o.segments {
                            if let Some(r) = room.iter_mut().find(|r| r.id == c.id) {
                                r.class = match c.class {
                                    RoomClass::CueReading => NoteClass::CueReading,
                                    RoomClass::Direction => NoteClass::Direction,
                                    RoomClass::Feedback => NoteClass::Feedback,
                                    RoomClass::Chatter => NoteClass::Chatter,
                                };
                            }
                        }
                    }
                    None => flagged.extend(out.flagged),
                },
                Err(e) => flagged.push(format!("room_classify: {e}")),
            }
        }

        // Picks.
        let input = PickDetectInput {
            takes: idx
                .iter()
                .map(|&i| PickCandidate {
                    take_id: takes[i].id.clone(),
                    take_no: takes[i].take_no,
                    line_text: line_text(&takes[i]),
                    feedback_text: first_texts(
                        after_texts
                            .get(&takes[i].id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        4,
                    ),
                })
                .collect(),
        };
        let picks = match p.ai.run(&input) {
            Ok(out) => {
                if out.output.is_none() {
                    flagged.extend(out.flagged);
                }
                out.output
            }
            Err(e) => {
                flagged.push(format!("pick_detect: {e}"));
                None
            }
        };
        for &i in &idx {
            let fb = first_texts(
                after_texts
                    .get(&takes[i].id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                4,
            );
            let strength = picks
                .as_ref()
                .and_then(|o| o.picks.iter().find(|pk| pk.take_id == takes[i].id))
                .map(|pk| pk.strength)
                .unwrap_or_else(|| pick_strength(&fb));
            takes[i].pick = strength;
        }
    }

    // Takes the AI rejected go to the unmatched bin.
    for t in takes.iter().filter(|t| dropped.contains(&t.id)) {
        unmatched.push(Unmatched {
            id: stable_id("um", &[&t.source_id, &format!("{:.2}", t.t0)]),
            scene_id: Some(scene_of_line(&t.line_id)),
            source_id: t.source_id.clone(),
            t0: t.t0,
            t1: t.t1,
            text: t.heard.clone(),
            db: t.db,
            origin: "analysis".into(),
            used_as: None,
        });
    }
    takes.retain(|t| !dropped.contains(&t.id));
    // renumber after drops
    let mut per_line: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, t) in takes.iter().enumerate() {
        per_line.entry(t.line_id.clone()).or_default().push(i);
    }
    for list in per_line.values() {
        let mut l = list.clone();
        l.sort_by(|&a, &b| takes[a].t0.total_cmp(&takes[b].t0));
        for (n, &i) in l.iter().enumerate() {
            takes[i].take_no = (n + 1) as u32;
        }
    }

    // 9. Room talk between takes, read by Claude as one conversation per gap. A gap is
    // the feedback of the take before it and the direction of the take after it.
    let clock: HashMap<String, (Option<String>, f64)> = sources
        .iter()
        .map(|d| (d.src.id.clone(), (d.src.session_id.clone(), d.src.offset_s)))
        .collect();
    let gaps = room_gaps(&takes, &room, &clock);
    let raws: Vec<String> = gaps.iter().map(|g| gap_text(&room, g)).collect();
    let in_scope = |t: &Take| line_scope.is_empty() || line_scope.contains(&t.line_id);
    let gap_take = |i: usize| {
        let t = &takes[i];
        let line = doc.lines.get(&t.line_id);
        GapTake {
            take_id: t.id.clone(),
            character: line
                .and_then(|l| l.character.as_ref())
                .map(|c| doc.characters.get(c).map_or(c, |x| &x.label).clone())
                .unwrap_or_default(),
            line_text: line.map(|l| l.text.clone()).unwrap_or_default(),
        }
    };
    let asked: Vec<RoomGap> = gaps
        .iter()
        .zip(&raws)
        .filter(|(g, text)| {
            !text.is_empty() && g.prev.iter().chain(&g.next).any(|&i| in_scope(&takes[i]))
        })
        .map(|(g, text)| RoomGap {
            gap_id: g.id.clone(),
            previous: g.prev.map(gap_take),
            next: g.next.map(gap_take),
            text: text.clone(),
        })
        .collect();
    let mut gap_notes: HashMap<String, (String, Vec<String>)> = HashMap::new();
    let mut by_rules: BTreeSet<String> = BTreeSet::new();
    let calls = asked.len().div_ceil(GAPS_PER_CALL);
    for (ci, chunk) in asked.chunks(GAPS_PER_CALL).enumerate() {
        say(
            "notes",
            ci as f64 / calls as f64,
            "Reading the room talk between takes".into(),
        );
        let out = match p.ai.run(&NoteCondenseInput {
            gaps: chunk.to_vec(),
        }) {
            Ok(out) => {
                if out.output.is_none() {
                    flagged.extend(out.flagged);
                }
                out.output
            }
            Err(e) => {
                flagged.push(format!("note_condense: {e}"));
                None
            }
        };
        for g in chunk {
            let note = match out
                .as_ref()
                .and_then(|o| o.notes.iter().find(|n| n.gap_id == g.gap_id))
            {
                Some(n) => (n.summary.clone(), n.tags.clone()),
                None => {
                    by_rules.insert(g.gap_id.clone());
                    let rules = condense_gap(&g.text, g.previous.is_some(), g.next.is_some());
                    (rules, vec![])
                }
            };
            gap_notes.insert(g.gap_id.clone(), note);
        }
    }
    for (g, raw) in gaps.iter().zip(&raws) {
        let (note, tags) = gap_notes.get(&g.id).cloned().unwrap_or_default();
        for (k, leads_in) in [(g.prev, false), (g.next, true)] {
            let Some(k) = k else { continue };
            let t = &mut takes[k];
            if leads_in {
                t.direction = note.clone();
                t.direction_raw = raw.clone();
            } else {
                t.feedback = note.clone();
                t.feedback_raw = raw.clone();
            }
            for x in &tags {
                if !t.tags.contains(x) {
                    t.tags.push(x.clone());
                }
            }
            t.note_ids
                .extend(g.segs.iter().map(|&r| room[r].id.clone()));
            if by_rules.contains(&g.id) {
                t.ai_flag
                    .get_or_insert_with(|| "notes condensed by rules".into());
            }
        }
        for &r in &g.segs {
            for &k in g.prev.iter().chain(&g.next) {
                if !room[r].take_ids.contains(&takes[k].id) {
                    room[r].take_ids.push(takes[k].id.clone());
                }
            }
            if let Some(k) = g.next.or(g.prev) {
                room[r]
                    .line_id
                    .get_or_insert_with(|| takes[k].line_id.clone());
            }
        }
    }

    // Unmatched: scene of the nearest take.
    for u in unmatched.iter_mut() {
        if u.scene_id.is_none() {
            u.scene_id = takes
                .iter()
                .filter(|t| t.source_id == u.source_id)
                .min_by(|a, b| (a.t0 - u.t0).abs().total_cmp(&(b.t0 - u.t0).abs()))
                .map(|t| scene_of_line(&t.line_id));
        }
    }

    // Report.
    for rr in report.roles.iter_mut() {
        let mine: Vec<&Take> = takes
            .iter()
            .filter(|t| t.source_id == rr.source_id)
            .collect();
        rr.takes = mine.len();
        rr.lines_with_takes = mine
            .iter()
            .map(|t| &t.line_id)
            .collect::<BTreeSet<_>>()
            .len();
    }
    report.takes_found = takes.len();
    report.lines_matched = takes
        .iter()
        .map(|t| &t.line_id)
        .collect::<BTreeSet<_>>()
        .len();
    report.picks_found = takes.iter().filter(|t| t.pick >= 2).count();
    report.flagged = flagged.clone();
    report.markdown = markdown(doc, &takes, &unmatched, &report);

    let notes: Vec<Note> = room
        .iter()
        .map(|r| {
            let off = sources
                .iter()
                .find(|d| d.src.id == r.source_id)
                .map(|d| d.src.offset_s)
                .unwrap_or(0.0);
            Note {
                id: r.id.clone(),
                source_id: r.source_id.clone(),
                t0: r.t0 + off,
                t1: r.t1 + off,
                text: r.text.clone(),
                class: r.class,
                take_ids: r.take_ids.clone(),
                line_id: r.line_id.clone(),
                summary: String::new(),
                tags: vec![],
            }
        })
        .collect();

    say(
        "done",
        1.0,
        format!(
            "{} takes on {} lines",
            report.takes_found, report.lines_matched
        ),
    );
    // 8. Vocal sounds of each actor, from their own mic.
    let only_interjections = |line_id: &str| {
        doc.lines
            .get(line_id)
            .map(|l| {
                let w = apc_core::text::norm_words(&l.text);
                !w.is_empty()
                    && w.iter()
                        .all(|x| crate::vocals::interjection_tag(x).is_some())
            })
            .unwrap_or(false)
    };
    let tracks: Vec<crate::vocals::VocalTrack> = sources
        .iter()
        .filter(|d| d.src.role == Role::Character)
        .filter_map(|d| {
            Some(crate::vocals::VocalTrack {
                source_id: d.src.id.clone(),
                character: d.src.character.clone()?,
                offset_s: d.src.offset_s,
                session: d.src.session_id.clone(),
                env: &d.analysis.env,
                words: d
                    .tr
                    .words
                    .iter()
                    .map(|w| (w.raw.clone(), w.t0 + d.src.offset_s, w.t1 + d.src.offset_s))
                    .collect(),
                takes: takes
                    .iter()
                    .filter(|t| t.source_id == d.src.id)
                    .map(|t| (t.t0, t.t1, only_interjections(&t.line_id)))
                    .collect(),
            })
        })
        .collect();
    // a scoped run leaves the vocal buckets alone
    let vocals = if scope.is_scoped() {
        vec![]
    } else {
        crate::vocals::find_vocals(&tracks)
    };
    drop(tracks);

    // Payload: inside the scope only.
    let (mut takes, mut notes, mut unmatched) = (takes, notes, unmatched);
    let mut line_scope_out: Vec<String> = vec![];
    let mut source_scope_out: Vec<String> = vec![];
    if !line_scope.is_empty() {
        takes.retain(|t| line_scope.contains(&t.line_id));
        notes.clear();
        unmatched.clear();
        line_scope_out = line_scope.iter().cloned().collect();
    } else if scope.is_scoped() {
        line_scope_out = doc
            .lines
            .values()
            .filter(|l| l.character.as_deref().is_some_and(in_chars))
            .map(|l| l.id.clone())
            .collect();
        source_scope_out = sources
            .iter()
            .filter(|d| in_session(d))
            .map(|d| d.src.id.clone())
            .collect();
        notes.retain(|n| source_scope_out.contains(&n.source_id));
        unmatched.retain(|u| source_scope_out.contains(&u.source_id));
    }
    Ok(AnalysisOutput {
        vocals,
        payload: AnalysisPayload {
            sessions,
            sources: sources.into_iter().map(|d| d.src).collect(),
            takes,
            notes,
            unmatched,
            line_scope: line_scope_out,
            source_scope: source_scope_out,
            assets: vec![],
            buckets: vec![],
        },
        report,
        proposals: vec![],
    })
}

/// The actor's session mic `k` and its session's director mics, with their transcripts.
fn mics_of<'a>(
    sources: &'a [SourceData],
    k: usize,
    role: &str,
    sents: &'a [Vec<Sentence>],
    levels: &'a [Vec<f64>],
) -> (Vec<Mic<'a>>, Vec<&'a [TWord]>) {
    let session = sources[k].src.session_id.clone();
    let mut mics = vec![Mic {
        source_id: &sources[k].src.id,
        session: true,
        speaker: role.to_string(),
        sentences: &sents[k],
        levels: &levels[k],
    }];
    let mut words: Vec<&[TWord]> = vec![&sources[k].tr.words];
    for (i, d) in sources.iter().enumerate() {
        if d.src.role == Role::Room && session.is_some() && d.src.session_id == session {
            mics.push(Mic {
                source_id: &d.src.id,
                session: false,
                speaker: String::new(),
                sentences: &sents[i],
                levels: &levels[i],
            });
            words.push(&d.tr.words);
        }
    }
    (mics, words)
}

/// Where scene `sc` was recorded on one session mic (session time): around the matcher's
/// takes of that scene, or between the neighbouring scenes' takes when it has none.
fn stretch(units: &[Unit], mine: &[RawTake], sc: usize, n_scenes: usize) -> Option<(f64, f64)> {
    let of = |s: usize| {
        mine.iter()
            .filter(move |t| !t.bleed && units[t.unit].scene_idx == s)
    };
    let lo = of(sc).map(|t| t.s0).reduce(f64::min);
    let hi = of(sc).map(|t| t.s1).reduce(f64::max);
    if let (Some(lo), Some(hi)) = (lo, hi) {
        return Some(((lo - 30.0).max(0.0), hi + 15.0));
    }
    let prev = (0..sc)
        .rev()
        .find_map(|p| of(p).map(|t| t.s1).reduce(f64::max));
    let next = (sc + 1..n_scenes).find_map(|n| of(n).map(|t| t.s0).reduce(f64::min));
    match (prev, next) {
        (Some(a), Some(b)) if a < b => Some((a, b)),
        _ => None,
    }
}

/// Proposals for dev-panel marks: the marked stretch is judged again (by Claude, or as
/// linked by hand) and compared with the takes the episode has now.
#[allow(clippy::too_many_arguments)]
fn propose_marks(
    doc: &Document,
    units: &[Unit],
    sources: &[SourceData],
    sents: &[Vec<Sentence>],
    levels: &[Vec<f64>],
    raw: &[RawTake],
    quiet_by_source: &HashMap<usize, Option<f64>>,
    scope: &AnalysisScope,
    ai: &AiRunner,
    n_scenes: usize,
    flagged: &mut Vec<String>,
) -> Vec<(String, MarkProposal)> {
    let mut out = Vec::new();
    let mut readers: HashMap<usize, AudioReader> = HashMap::new();
    for m in &scope.marks {
        let mut prop = MarkProposal::default();
        let Some(k) = sources.iter().position(|d| d.src.id == m.source_id) else {
            prop.summary
                .push("the marked file is not part of the analysis".into());
            out.push((m.id.clone(), prop));
            continue;
        };
        let d = &sources[k];
        let Some(role) = d
            .src
            .character
            .clone()
            .filter(|_| d.src.role == Role::Character)
        else {
            prop.summary.push(
                "marks only work on a character's session mic, never the director mic".into(),
            );
            out.push((m.id.clone(), prop));
            continue;
        };
        let off = d.src.offset_s;
        let (t0, t1) = (m.t0 - off, m.t1 - off);
        let mine: Vec<RawTake> = raw.iter().filter(|t| t.source == k).cloned().collect();
        let unit = m
            .line_id
            .as_ref()
            .and_then(|l| units.iter().position(|u| &u.id == l));
        if m.line_id.is_some() && unit.is_none() {
            prop.summary
                .push("the linked line is not in the script".into());
            out.push((m.id.clone(), prop));
            continue;
        }
        if let Some(u) = unit {
            if units[u].character.as_deref() != Some(role.as_str()) {
                prop.summary
                    .push(format!("{} is not {role}'s line", units[u].id));
                out.push((m.id.clone(), prop));
                continue;
            }
        }
        // the scene: the linked line's, or the one the nearest take belongs to
        let scene = unit.map(|u| units[u].scene_idx).or_else(|| {
            mine.iter()
                .filter(|t| !t.bleed)
                .min_by(|a, b| (a.s0 - t0).abs().total_cmp(&(b.s0 - t0).abs()))
                .map(|t| units[t.unit].scene_idx)
        });
        let Some(sc) = scene else {
            prop.summary
                .push("no scene found for this stretch; link it to a line".into());
            out.push((m.id.clone(), prop));
            continue;
        };
        let thr = d.analysis.percentile(60.0) * 3.0;
        let decisions: Vec<Decision> = if scope.manual {
            let Some(u) = unit else {
                prop.summary
                    .push("link the stretch to a line to make a take by hand".into());
                out.push((m.id.clone(), prop));
                continue;
            };
            let ws: Vec<usize> = (0..d.tr.words.len())
                .filter(|&i| d.tr.words[i].t0 >= t0 - 0.05 && d.tr.words[i].t1 <= t1 + 0.05)
                .collect();
            match (ws.first(), ws.last()) {
                (Some(&a), Some(&b)) => vec![Decision {
                    row_id: "manual".into(),
                    sentence: 0,
                    unit: Some(u),
                    w0: a,
                    w1: b + 1,
                    read_as: String::new(),
                    confidence: RescueConfidence::High,
                }],
                _ => {
                    prop.summary.push("no words in the marked stretch".into());
                    out.push((m.id.clone(), prop));
                    continue;
                }
            }
        } else {
            let (mics, mic_words) = mics_of(sources, k, &role, sents, levels);
            let median = quiet_by_source.get(&k).copied().flatten();
            let margin = scope.tuning.bleed_margin_db;
            let quiet = |db: f64| db < BLEED_DB || median.is_some_and(|x| db < x - margin);
            let (lo, hi) = stretch(units, &mine, sc, n_scenes)
                .map(|(a, b)| (a.min(t0 - 30.0), b.max(t1 + 15.0)))
                .unwrap_or((t0 - 60.0, t1 + 30.0));
            let takes: Vec<MicTake> = mine
                .iter()
                .filter(|t| !t.bleed)
                .map(|t| MicTake {
                    unit: t.unit,
                    w0: t.w0,
                    w1: t.w1,
                })
                .collect();
            let hints = [Hint {
                t0,
                t1,
                unit,
                note: m.note.clone(),
            }];
            let job = SceneJob {
                doc,
                units,
                scene_idx: sc,
                role: &role,
                mics: &mics,
                words: &mic_words,
                takes: &takes,
                quiet: &quiet,
                lo,
                hi,
                hints: &hints,
                examples: &scope.examples,
            };
            let r = rescue_scene(&job, ai);
            flagged.extend(r.flagged.iter().cloned());
            prop.summary
                .extend(r.refused.iter().map(|x| format!("refused: {x}")));
            r.decisions
        };
        let origin = if scope.manual {
            TakeOrigin::Manual
        } else {
            TakeOrigin::Rescue
        };
        for dec in &decisions {
            let (a, b) = (d.tr.words[dec.w0].t0 + off, d.tr.words[dec.w1 - 1].t1 + off);
            let label = dec.unit.map(|u| units[u].id.clone());
            let existing: Vec<&Take> = doc
                .takes
                .values()
                .filter(|t| t.source_id == d.src.id && t.t0 < b && a < t.t1)
                .collect();
            let keeps = existing.iter().any(|t| Some(&t.line_id) == label.as_ref());
            for t in &existing {
                if Some(&t.line_id) != label.as_ref() && !prop.remove.contains(&t.id) {
                    prop.remove.push(t.id.clone());
                }
            }
            match dec.unit {
                Some(u) if !keeps => {
                    if let Some(r) = decided_take(d, k, units, dec, thr, origin) {
                        let line = &doc.lines[&units[u].id];
                        let t = build_take(&r, &units[u], line, d, 0, &mut readers);
                        prop.summary.push(format!(
                            "{} → take of “{}” ({:?}){}",
                            dec.row_id,
                            line.text,
                            dec.confidence,
                            if dec.read_as.is_empty() {
                                String::new()
                            } else {
                                format!(", read as “{}”", dec.read_as)
                            }
                        ));
                        prop.add.push(t);
                    }
                }
                Some(_) => prop
                    .summary
                    .push(format!("{} → already a take of that line", dec.row_id)),
                None => prop.summary.push(format!("{} → not a take", dec.row_id)),
            }
        }
        if decisions.is_empty() && prop.summary.is_empty() {
            prop.summary.push("no answer for the marked stretch".into());
        }
        out.push((m.id.clone(), prop));
    }
    out
}

/// The room talk between two neighbouring takes of a session, or before its first take
/// or after its last.
struct Gap {
    id: String,
    prev: Option<usize>,
    next: Option<usize>,
    /// Room segments (not cue reading), in time order.
    segs: Vec<usize>,
}

/// Cuts each session's room talk at its takes. A gap keeps what was said in the first
/// seconds after the previous take and in the last minute before the next one; a long
/// break in between is left out. `clock` maps a source to its session and sync offset.
fn room_gaps(
    takes: &[Take],
    room: &[RoomSeg],
    clock: &HashMap<String, (Option<String>, f64)>,
) -> Vec<Gap> {
    let at = |source_id: &str| clock.get(source_id).cloned().unwrap_or((None, 0.0));
    let mut by_session: BTreeMap<Option<String>, Vec<usize>> = BTreeMap::new();
    for (i, t) in takes.iter().enumerate() {
        by_session.entry(at(&t.source_id).0).or_default().push(i);
    }
    let mut gaps = Vec::new();
    for (session, mut list) in by_session {
        let start = |i: usize| takes[i].t0 - at(&takes[i].source_id).1;
        let end = |i: usize| takes[i].t1 - at(&takes[i].source_id).1;
        list.sort_by(|&a, &b| start(a).total_cmp(&start(b)));
        let mut talk: Vec<usize> = (0..room.len())
            .filter(|&r| room[r].class != NoteClass::CueReading)
            .filter(|&r| at(&room[r].source_id).0 == session)
            .collect();
        talk.sort_by(|&a, &b| room[a].t0.total_cmp(&room[b].t0));
        let mut prev: Option<usize> = None;
        let mut prev_end = f64::MIN;
        for next in list.iter().copied().map(Some).chain([None]) {
            // the room mic hears the takes too: speech starting just before one is that take
            let segs = talk
                .iter()
                .copied()
                .filter(|&r| {
                    let t = room[r].t0 + 0.3;
                    let inside =
                        (prev.is_none() || t >= prev_end) && next.is_none_or(|n| t < start(n));
                    let after_prev = prev.is_some() && t < prev_end + FEEDBACK_WINDOW_S;
                    let before_next = next.is_some_and(|n| t >= start(n) - DIRECTION_WINDOW_S);
                    inside && (after_prev || before_next)
                })
                .collect();
            let id_of = |k: Option<usize>| k.map(|k| takes[k].id.as_str()).unwrap_or("");
            gaps.push(Gap {
                id: stable_id("gp", &[id_of(prev), id_of(next)]),
                prev,
                next,
                segs,
            });
            if let Some(n) = next {
                prev = Some(n);
                prev_end = prev_end.max(end(n));
            }
        }
    }
    gaps
}

fn gap_text(room: &[RoomSeg], g: &Gap) -> String {
    g.segs
        .iter()
        .map(|&r| room[r].text.trim())
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_texts(texts: &[String], n: usize) -> String {
    texts.iter().take(n).cloned().collect::<Vec<_>>().join(" ")
}

fn markdown(doc: &Document, takes: &[Take], unmatched: &[Unmatched], r: &MatchReport) -> String {
    let mut out = String::new();
    for rr in &r.roles {
        out.push_str(&format!("# Match report: {}\n\n", rr.role));
        out.push_str(&format!(
            "- Takes found: {} on {} script lines (role has {} lines in the episode)\n- Dropped as bleed (quiet): {}\n- Unmatched loud role speech: {}\n",
            rr.takes, rr.lines_with_takes, rr.role_lines_total, rr.bleed_dropped, rr.unmatched
        ));
    }
    out.push_str(&format!(
        "- Room: {} cue readings, {} note segments\n",
        r.cue_readings, r.note_segments
    ));
    for (sid, off) in &r.offsets {
        let name = doc
            .sources
            .get(sid)
            .map(|s| s.original_name.clone())
            .unwrap_or_else(|| sid.clone());
        out.push_str(&format!(
            "- Sync: {name} runs {off:+.3} s against the session reference\n"
        ));
    }
    out.push('\n');
    for sc in doc.scenes_in_order() {
        let lines: Vec<&ScriptLine> = doc
            .lines_of_scene(&sc.id)
            .into_iter()
            .filter(|l| takes.iter().any(|t| t.line_id == l.id))
            .collect();
        if lines.is_empty() {
            continue;
        }
        out.push_str(&format!("## S{} {}\n\n", sc.number, sc.heading));
        for l in lines {
            out.push_str(&format!("**S{}.{}** {}  \n", sc.number, l.number, l.text));
            let mut ts: Vec<&Take> = takes.iter().filter(|t| t.line_id == l.id).collect();
            ts.sort_by_key(|t| t.take_no);
            let def = apc_core::commands::default_take(&ts).map(|t| t.id.clone());
            for t in ts {
                let flag = if Some(&t.id) == def.as_ref() {
                    " DEFAULT"
                } else {
                    ""
                };
                let pk = match t.pick {
                    2 => " PICK",
                    1 => " liked",
                    _ => "",
                };
                let ty = if t.partial {
                    format!(" ({:?})", t.take_type).to_lowercase()
                } else {
                    String::new()
                };
                out.push_str(&format!(
                    "  - T{} {}-{} score {}{}{}{} | heard: {}\n",
                    t.take_no,
                    fmt_t(t.t0),
                    fmt_t(t.t1),
                    t.score,
                    flag,
                    pk,
                    ty,
                    t.heard
                ));
                if !t.feedback.is_empty() || !t.direction.is_empty() {
                    out.push_str(&format!(
                        "    - direction: {} · feedback: {}\n",
                        t.direction, t.feedback
                    ));
                }
            }
            out.push('\n');
        }
    }
    out.push_str("## Unmatched loud role speech\n\n");
    for u in unmatched {
        out.push_str(&format!("- {} {}\n", fmt_t(u.t0), u.text));
    }
    out
}

#[cfg(test)]
mod glossary_tests {
    use super::*;

    #[test]
    fn glossary_fixes_misheard_words_in_place() {
        let mut t = Transcript::from_segments(
            vec![(
                1.0,
                3.0,
                "a little late to get a tricky though".into(),
                None,
            )],
            "t",
        );
        let t0 = t.words[6].t0;
        let n = apply_glossary(
            &mut t,
            &[
                ("tricky".into(), "Turkey".into()),
                ("two words".into(), "one".into()),
            ],
        );
        assert_eq!(n, 1);
        assert_eq!(t.words[6].w, "turkey");
        assert_eq!(t.words[6].raw, "tricky", "what was heard stays visible");
        assert_eq!(t.words[6].t0, t0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(id: &str, source: &str, t0: f64, t1: f64) -> Take {
        serde_json::from_value(serde_json::json!({
            "id": id, "line_id": "l", "source_id": source, "take_no": 1,
            "take_type": "full", "t0": t0, "t1": t1,
        }))
        .unwrap()
    }

    fn seg(source: &str, t0: f64, text: &str, class: NoteClass) -> RoomSeg {
        RoomSeg {
            id: format!("n{t0}"),
            source_id: source.into(),
            t0,
            t1: t0 + 1.0,
            text: text.into(),
            class,
            line_id: None,
            take_ids: vec![],
        }
    }

    fn clock() -> HashMap<String, (Option<String>, f64)> {
        [
            ("paul".to_string(), (Some("s1".to_string()), 0.0)),
            // another actor's mic, 2 s late
            ("sara".to_string(), (Some("s1".to_string()), 2.0)),
            ("room".to_string(), (Some("s1".to_string()), 0.0)),
            ("room2".to_string(), (Some("s2".to_string()), 0.0)),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn talk_between_takes_is_one_shared_gap() {
        let takes = vec![
            take("t1", "paul", 100.0, 105.0),
            take("t2", "paul", 130.0, 134.0),
        ];
        let room = vec![
            seg("room", 106.0, "I like that.", NoteClass::Feedback),
            seg(
                "room",
                115.0,
                "the coffee machine is broken",
                NoteClass::Chatter,
            ),
            seg("room", 125.0, "one more, quieter", NoteClass::Direction),
            seg("room", 120.0, "Lunch.", NoteClass::CueReading),
            seg("room2", 110.0, "another session", NoteClass::Chatter),
        ];
        let gaps = room_gaps(&takes, &room, &clock());
        assert_eq!(gaps.len(), 3, "before, between, after");
        assert_eq!((gaps[0].prev, gaps[0].next), (None, Some(0)));
        assert_eq!((gaps[1].prev, gaps[1].next), (Some(0), Some(1)));
        assert_eq!((gaps[2].prev, gaps[2].next), (Some(1), None));
        // chatter stays in, cue reading and other sessions don't
        assert_eq!(
            gap_text(&room, &gaps[1]),
            "I like that. the coffee machine is broken one more, quieter"
        );
        assert!(gaps[0].segs.is_empty() && gaps[2].segs.is_empty());
    }

    #[test]
    fn a_long_break_keeps_the_reaction_and_the_last_minute() {
        let takes = vec![
            take("t1", "paul", 0.0, 5.0),
            take("t2", "paul", 600.0, 604.0),
        ];
        let room = vec![
            seg("room", 7.0, "that was the one", NoteClass::Feedback),
            seg("room", 200.0, "lunch break talk", NoteClass::Chatter),
            seg(
                "room",
                530.0,
                "back from lunch, scene four",
                NoteClass::Chatter,
            ),
            seg("room", 570.0, "angrier this time", NoteClass::Direction),
            // the take itself heard on the room mic
            seg("room", 599.8, "Lunch", NoteClass::Chatter),
        ];
        let gaps = room_gaps(&takes, &room, &clock());
        assert_eq!(
            gap_text(&room, &gaps[1]),
            "that was the one angrier this time"
        );
    }

    #[test]
    fn takes_on_other_mics_split_the_talk_in_session_time() {
        // Sara's take at file time 52 is session time 50
        let takes = vec![
            take("t1", "paul", 10.0, 20.0),
            take("t2", "sara", 52.0, 60.0),
            take("t3", "paul", 80.0, 85.0),
        ];
        let room = vec![
            seg("room", 30.0, "now Sara", NoteClass::Direction),
            seg("room", 49.0, "go", NoteClass::Direction),
            seg("room", 60.0, "nice", NoteClass::Feedback),
        ];
        let gaps = room_gaps(&takes, &room, &clock());
        let order: Vec<_> = gaps.iter().map(|g| (g.prev, g.next)).collect();
        assert_eq!(
            order,
            vec![
                (None, Some(0)),
                (Some(0), Some(1)),
                (Some(1), Some(2)),
                (Some(2), None)
            ]
        );
        assert_eq!(gap_text(&room, &gaps[1]), "now Sara go");
        assert_eq!(gap_text(&room, &gaps[2]), "nice");
    }
}
