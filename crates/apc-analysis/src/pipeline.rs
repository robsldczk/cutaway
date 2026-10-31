//! The analysis pipeline: media → sessions and sync → transcripts → alignment per role
//! track → takes → room classification → note linking → AI refinement → payload.
//!
//! Times: takes are stored in their source file's time; note linking works in session
//! time (file time − the file's sync offset).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use serde::Serialize;

use apc_ai::mock::{condense_direction, condense_feedback, pick_strength};
use apc_ai::task::*;
use apc_ai::AiRunner;
use apc_core::project::EpisodePaths;
use apc_core::*;
use apc_media::{bwf, loudness, sync::measure_offset, AudioAnalysis, AudioReader};

use crate::align::{actions_in, active_scenes, align, dialogue_in, units_of, AlignOptions, Unit};
use crate::asr::{AsrProvider, AsrRequest, Transcript};
use crate::edges::refine;
use crate::error::Result;
use crate::sessions::{group_sessions, pair_rooms_by_name, FileSpan, NamedFile};
use crate::wordmap::{coverage, map_words};

/// Feedback window after a take, direction window before it (seconds).
pub const FEEDBACK_WINDOW_S: f64 = 8.0;
pub const DIRECTION_WINDOW_S: f64 = 30.0;
/// Takes quieter than this on their own mic are bleed.
pub const BLEED_DB: f64 = -38.0;
/// Takes this far below the mic's median take level are bleed too.
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

pub struct AnalysisOutput {
    pub payload: AnalysisPayload,
    pub report: MatchReport,
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
    let say = |stage: &str, fraction: f64, message: String| {
        progress(Progress {
            stage: stage.into(),
            fraction,
            message,
        })
    };
    let units = units_of(doc);
    let opts = AlignOptions::default();
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
    }

    // 4. Role tracks.
    let mut raw: Vec<RawTake> = Vec::new();
    let mut unmatched: Vec<Unmatched> = Vec::new();
    let mut near_by_session: HashMap<String, BTreeSet<usize>> = HashMap::new();
    for (k, d) in sources.iter().enumerate() {
        if d.src.role != Role::Character {
            continue;
        }
        let Some(role) = d.src.character.clone() else {
            continue;
        };
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
        let (hits, used) = align(&units, &speak, words, &opts);
        let thr = d.analysis.percentile(60.0) * 3.0;
        let off = d.src.offset_s;
        let mut mine: Vec<RawTake> = Vec::new();
        for h in &hits {
            let u = &units[h.unit];
            let (mut s0, mut s1) = (words[h.w0].t0, words[h.w1 - 1].t1);
            let sf = &d.tr.segments[words[h.w0].seg];
            let sl = &d.tr.segments[words[h.w1 - 1].seg];
            if h.w0 == sf.w0 {
                s0 = sf.start;
            }
            if h.w1 == sl.w1 {
                s1 = sl.end;
            }
            let db = d.analysis.rms_db(s0 + off, s1 + off);
            let (r0, r1) = refine(&d.analysis.env, s0 + off, s1 + off, thr);
            if u.character.as_deref() != Some(role.as_str()) {
                continue;
            }
            mine.push(RawTake {
                unit: h.unit,
                source: k,
                score: h.score,
                w0: h.w0,
                w1: h.w1,
                t0: r0,
                t1: r1,
                s0: r0 - off,
                s1: r1 - off,
                db,
                heard: words[h.w0..h.w1]
                    .iter()
                    .map(|w| w.raw.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                bleed: db < BLEED_DB,
                before: vec![],
                after: vec![],
            });
        }
        // Bleed: quiet in absolute terms, or far below this mic's typical take level.
        let mut levels: Vec<f64> = mine.iter().map(|t| t.db).collect();
        levels.sort_by(f64::total_cmp);
        if let Some(median) = levels.get(levels.len() / 2).copied() {
            for t in mine.iter_mut() {
                t.bleed = t.db < BLEED_DB || t.db < median - RELATIVE_BLEED_DB;
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
            if cov < 0.5 && db > BLEED_DB {
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

    // 5. Room tracks: cue reading vs notes.
    let mut room: Vec<RoomSeg> = Vec::new();
    for d in sources.iter().filter(|d| d.src.role == Role::Room) {
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
    let mut take_of_raw: HashMap<usize, usize> = HashMap::new();
    let mut after_texts: HashMap<String, Vec<String>> = HashMap::new();
    let mut readers: HashMap<usize, AudioReader> = HashMap::new();
    for (unit_i, list) in by_unit.iter_mut() {
        list.sort_by(|&a, &b| raw[a].t0.total_cmp(&raw[b].t0));
        let u: &Unit = &units[*unit_i];
        let line = &doc.lines[&u.id];
        for (n, &ti) in list.iter().enumerate() {
            let r = &raw[ti];
            let d = &sources[r.source];
            let off = d.src.offset_s;
            let tw = &d.tr.words[r.w0..r.w1];
            let heard_norm: Vec<String> = tw.iter().map(|w| w.w.clone()).collect();
            let map = map_words(&heard_norm, &line.words);
            let cov = coverage(&map, line.words.len());
            let first = map.iter().flatten().next().copied().unwrap_or(0);
            let take_type = if tw.len() <= 2 && line.words.len() > 3 {
                TakeType::Micro
            } else if first >= 2.max((line.words.len() as f64 * 0.3) as usize) {
                TakeType::Pickup
            } else if r.score < 85.0 || cov < 0.6 {
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
                        loudness::integrated_lufs(
                            &s,
                            d.analysis.info.sample_rate,
                            d.analysis.info.channels,
                        )
                        .ok()
                        .flatten()
                    })
                    .map(|l| (l * 10.0).round() / 10.0)
            };
            let id = stable_id("tk", &[&u.id, &d.src.id, &format!("{:.1}", r.t0)]);
            take_of_raw.insert(ti, takes.len());
            after_texts.insert(
                id.clone(),
                r.after.iter().map(|&i| room[i].text.clone()).collect(),
            );
            takes.push(Take {
                id,
                line_id: u.id.clone(),
                source_id: d.src.id.clone(),
                take_no: (n + 1) as u32,
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
                direction_raw: r
                    .before
                    .iter()
                    .map(|&i| room[i].text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                feedback_raw: r
                    .after
                    .iter()
                    .map(|&i| room[i].text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                tags: vec![],
                note_ids: r
                    .before
                    .iter()
                    .chain(r.after.iter())
                    .map(|&i| room[i].id.clone())
                    .collect(),
                confidence: (r.score / 100.0).min(1.0),
                needs_rematch: false,
                ai_flag: None,
            });
        }
    }
    for (ti, k) in &take_of_raw {
        for &ri in raw[*ti].before.iter().chain(raw[*ti].after.iter()) {
            if !room[ri].take_ids.contains(&takes[*k].id) {
                room[ri].take_ids.push(takes[*k].id.clone());
            }
            room[ri]
                .line_id
                .get_or_insert_with(|| takes[*k].line_id.clone());
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
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    for (si, scene_id) in scenes_with_takes.iter().enumerate() {
        say(
            "notes",
            si as f64 / scenes_with_takes.len().max(1) as f64,
            "Condensing notes and finding picks".into(),
        );
        let idx: Vec<usize> = (0..takes.len())
            .filter(|&i| &scene_of_line(&takes[i].line_id) == scene_id)
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
            .filter(|&i| takes[i].score < 85.0)
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

        // Room classification.
        let (lo, hi) = idx.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &i| {
            let off = sources
                .iter()
                .find(|d| d.src.id == takes[i].source_id)
                .map(|d| d.src.offset_s)
                .unwrap_or(0.0);
            (lo.min(takes[i].t0 - off), hi.max(takes[i].t1 - off))
        });
        let segs: Vec<usize> = (0..room.len())
            .filter(|&r| {
                room[r].class != NoteClass::CueReading
                    && room[r].t0 >= lo - DIRECTION_WINDOW_S
                    && room[r].t0 <= hi + FEEDBACK_WINDOW_S
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

        // Notes.
        let input = NoteCondenseInput {
            takes: idx
                .iter()
                .map(|&i| TakeNotes {
                    take_id: takes[i].id.clone(),
                    line_text: line_text(&takes[i]),
                    direction_text: takes[i].direction_raw.clone(),
                    feedback_text: takes[i].feedback_raw.clone(),
                })
                .collect(),
        };
        let notes = match p.ai.run(&input) {
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
        for &i in &idx {
            match notes
                .as_ref()
                .and_then(|o| o.notes.iter().find(|n| n.take_id == takes[i].id))
            {
                Some(n) => {
                    takes[i].direction = n.direction.clone();
                    takes[i].feedback = n.feedback.clone();
                    takes[i].tags = n.tags.clone();
                }
                None => {
                    takes[i].direction = condense_direction(&takes[i].direction_raw);
                    takes[i].feedback = condense_feedback(&takes[i].feedback_raw);
                    takes[i]
                        .ai_flag
                        .get_or_insert_with(|| "notes condensed by rules".into());
                }
            }
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
    Ok(AnalysisOutput {
        payload: AnalysisPayload {
            sessions,
            sources: sources.into_iter().map(|d| d.src).collect(),
            takes,
            notes,
            unmatched,
            line_scope: vec![],
        },
        report,
    })
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
