//! The Import › Files page: script and audio, the cast (recording → character, director
//! room → recordings), "Needs you" fixes and Redo match. Matching groups files into
//! sessions by timecode, else by the folder they were dropped in, then pairs a room file
//! named after a character with that character's recordings. Links the user set by hand
//! (`link_locked`) survive a redo.

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};

use apc_analysis::pipeline::stable_id;
use apc_analysis::sessions::{group_sessions, pair_rooms_by_name, FileSpan, NamedFile};
use apc_core::*;

use crate::error::{AppError, Result};
use crate::service::Service;

fn batch_of(s: &SourceFile) -> String {
    std::path::Path::new(&s.path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Members of each source's current session (for keeping hand-set links).
fn partners(doc: &Document) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    for s in doc.sessions.values() {
        for m in &s.source_ids {
            out.insert(
                m.clone(),
                s.source_ids.iter().filter(|o| *o != m).cloned().collect(),
            );
        }
    }
    out
}

/// Builds sessions from groups of source ids (recordings first, then rooms).
fn sessions_of(doc: &Document, groups: Vec<Vec<String>>) -> (Vec<Session>, Vec<SourceFile>) {
    let mut sessions = Vec::new();
    let mut changed = Vec::new();
    for (gi, mut g) in groups.into_iter().enumerate() {
        g.sort_by_key(|id| {
            let s = doc.sources.get(id);
            (
                s.map(|s| s.role != Role::Character).unwrap_or(true),
                s.map(|s| s.original_name.clone()).unwrap_or_default(),
            )
        });
        let Some(first) = g.first().and_then(|id| doc.sources.get(id)) else {
            continue;
        };
        let id = stable_id("ss", &g.iter().map(String::as_str).collect::<Vec<_>>());
        let name = std::path::Path::new(&first.path)
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Session {}", gi + 1));
        let tc = g
            .iter()
            .filter_map(|m| doc.sources.get(m))
            .filter_map(|s| s.tc_start_s)
            .reduce(f64::min);
        for m in &g {
            if let Some(s) = doc.sources.get(m) {
                if s.session_id.as_deref() != Some(id.as_str()) {
                    let mut s = s.clone();
                    s.session_id = Some(id.clone());
                    changed.push(s);
                }
            }
        }
        sessions.push(Session {
            id,
            name,
            tc_start_s: tc,
            source_ids: g,
        });
    }
    (sessions, changed)
}

/// Redo match: regroups the files into sessions, keeping hand-set room links.
pub fn match_files(svc: &mut Service) -> Result<Value> {
    svc.bus.refresh()?;
    let doc = svc.doc().clone();
    let old = partners(&doc);
    let files: Vec<&SourceFile> = doc.sources.values().filter(|s| !s.missing).collect();
    let spans: Vec<FileSpan> = files
        .iter()
        .filter(|s| !s.link_locked)
        .map(|s| FileSpan {
            id: s.id.clone(),
            tc_start_s: s.tc_start_s,
            duration_s: s.duration_s,
            batch: batch_of(s),
        })
        .collect();
    let named: Vec<NamedFile> = files
        .iter()
        .map(|s| NamedFile {
            id: s.id.clone(),
            name: s.original_name.clone(),
            role: s.role,
            character: s.character.clone(),
        })
        .collect();
    let characters: Vec<String> = doc.characters.keys().cloned().collect();
    let mut groups =
        pair_rooms_by_name(group_sessions(&spans), &named, &characters, &svc.aliases());
    // hand-linked files stay together as they were
    let mut locked: Vec<Vec<String>> = Vec::new();
    for s in files.iter().filter(|s| s.link_locked) {
        let p = old.get(&s.id).cloned().unwrap_or_default();
        match locked.iter_mut().find(|g| g.iter().any(|m| p.contains(m))) {
            Some(g) => g.push(s.id.clone()),
            None => locked.push(vec![s.id.clone()]),
        }
    }
    groups.extend(locked);
    let (sessions, sources) = sessions_of(&doc, groups);
    let n = sessions.len();
    svc.exec(&Command::ApplyMatch {
        sessions,
        sources,
        at: Some(chrono::Utc::now().to_rfc3339()),
    })?;
    Ok(json!({ "sessions": n }))
}

/// Links a director room file to exactly these recordings: together they become one session,
/// set by hand and kept on Redo match.
pub fn link_room(svc: &mut Service, room_id: &str, recording_ids: &[String]) -> Result<Value> {
    svc.bus.refresh()?;
    let doc = svc.doc().clone();
    if !doc.sources.contains_key(room_id) {
        return Err(AppError::Params(format!("no file '{room_id}'")));
    }
    if recording_ids.is_empty() {
        return Err(AppError::Params("pick at least one recording".into()));
    }
    // the room and its recordings leave their sessions and form one of their own
    let mut linked: Vec<String> = recording_ids.to_vec();
    linked.push(room_id.to_string());
    let mut groups: Vec<Vec<String>> = doc
        .sessions
        .values()
        .map(|s| {
            s.source_ids
                .iter()
                .filter(|m| !linked.contains(m))
                .cloned()
                .collect()
        })
        .collect();
    groups.retain(|g: &Vec<String>| !g.is_empty());
    groups.push(linked.clone());
    let (sessions, mut sources) = sessions_of(&doc, groups);
    for id in &linked {
        match sources.iter_mut().find(|s| &s.id == id) {
            Some(s) => s.link_locked = true,
            None => {
                if let Some(mut s) = doc.sources.get(id).cloned() {
                    s.link_locked = true;
                    sources.push(s);
                }
            }
        }
    }
    let at = doc
        .settings
        .get("match")
        .and_then(|s| s.value["at"].as_str().map(String::from));
    svc.exec(&Command::ApplyMatch {
        sessions,
        sources,
        at,
    })?;
    Ok(json!({ "linked": recording_ids.len() }))
}

/// Suggests the character of a file: its name first, then AI name matching.
pub fn suggest(svc: &mut Service, source_id: &str) -> Result<Value> {
    let s = svc
        .doc()
        .sources
        .get(source_id)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no file '{source_id}'")))?;
    let characters: Vec<String> = svc.doc().characters.keys().cloned().collect();
    let aliases = svc.aliases();
    let mut g = apc_analysis::roles::guess_by_name(&s.original_name, &characters, &aliases);
    if g.role != Role::Character {
        if let Some(ai) = svc.ai_name_guess(&s.original_name, &characters, &aliases) {
            g = ai;
        }
    }
    Ok(
        json!({ "role": g.role, "character": g.character, "confidence": g.confidence, "reason": g.reason }),
    )
}

fn peaks_if_cached(svc: &mut Service, s: &SourceFile, n: usize) -> Vec<f32> {
    let cached = svc
        .paths
        .cache
        .join("media")
        .join(format!("{}.env", s.hash))
        .exists();
    if !cached {
        return Vec::new();
    }
    svc.analysis_of(&s.id)
        .map(|a| {
            let p = a.peaks_range(0.0, a.info.duration_s, n);
            let g = p
                .iter()
                .map(|(lo, hi)| lo.abs().max(hi.abs()))
                .fold(1e-4f32, f32::max);
            p.into_iter()
                .map(|(lo, hi)| lo.abs().max(hi.abs()) / g)
                .collect()
        })
        .unwrap_or_default()
}

fn file_view(svc: &mut Service, s: &SourceFile) -> Value {
    json!({
        "source_id": s.id, "file": s.original_name, "duration_s": s.duration_s, "role": s.role,
        "character": s.character, "confidence": s.role_confidence, "locked": s.link_locked,
        "tc_start_s": s.tc_start_s, "peaks": peaks_if_cached(svc, s, 48),
    })
}

/// Everything the Import › Files page shows.
pub fn view(svc: &mut Service) -> Result<Value> {
    svc.bus.refresh()?;
    let doc = svc.doc().clone();
    let script = doc
        .script_versions
        .values()
        .max_by_key(|v| v.number)
        .map(|v| {
            json!({ "number": v.number, "name": v.name, "file": v.file, "summary": v.summary,
                "characters": doc.characters.len(), "lines": doc.lines.len() })
        });
    let files: Vec<&SourceFile> = doc.sources.values().collect();
    let session_of = |id: &str| {
        doc.sessions
            .values()
            .find(|s| s.source_ids.iter().any(|m| m == id))
    };
    let mut fixes: Vec<Value> = Vec::new();
    for s in &files {
        match (s.role, &s.character) {
            (Role::Unknown, _) => fixes.push(
                json!({ "kind": "role", "source_id": s.id, "file": s.original_name,
                "message": "Recording or director room?" }),
            ),
            (Role::Character, None) => fixes.push(
                json!({ "kind": "character", "source_id": s.id, "file": s.original_name,
                "message": "No character" }),
            ),
            (Role::Room, _) => {
                let linked = session_of(&s.id)
                    .map(|ss| {
                        ss.source_ids.iter().any(|m| {
                            doc.sources
                                .get(m)
                                .map(|o| o.role == Role::Character)
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
                if !linked {
                    fixes.push(
                        json!({ "kind": "link", "source_id": s.id, "file": s.original_name,
                        "message": "Not linked to any recording" }),
                    );
                }
            }
            _ => {}
        }
        if s.missing {
            fixes.push(json!({ "kind": "missing_file", "source_id": s.id, "file": s.original_name, "message": "File is missing" }));
        }
    }
    let mut lines_by: BTreeMap<String, usize> = BTreeMap::new();
    for l in doc.lines.values() {
        if let Some(c) = &l.character {
            *lines_by.entry(c.clone()).or_default() += 1;
        }
    }
    let mut chars: Vec<&Character> = doc.characters.values().collect();
    chars.sort_by_key(|c| c.color_index);
    let mut cast = Vec::new();
    for c in chars {
        let recs: Vec<SourceFile> = files
            .iter()
            .filter(|s| s.role == Role::Character && s.character.as_deref() == Some(c.id.as_str()))
            .map(|s| (*s).clone())
            .collect();
        if recs.is_empty() {
            fixes.push(json!({ "kind": "no_recording", "character": c.id, "label": c.label, "message": "No recording found" }));
        }
        let mut rec_views = Vec::new();
        for r in &recs {
            let mut v = file_view(svc, r);
            let ss = session_of(&r.id);
            let others: Vec<String> = ss
                .map(|ss| {
                    let mut o: Vec<String> = ss
                        .source_ids
                        .iter()
                        .filter_map(|m| doc.sources.get(m))
                        .filter(|o| o.role == Role::Character && o.id != r.id)
                        .filter_map(|o| {
                            o.character
                                .as_ref()
                                .and_then(|ch| doc.characters.get(ch))
                                .map(|ch| ch.label.clone())
                        })
                        .collect();
                    o.sort();
                    o.dedup();
                    o
                })
                .unwrap_or_default();
            let rooms: Vec<Value> = ss
                .map(|ss| {
                    ss.source_ids
                        .iter()
                        .filter_map(|m| doc.sources.get(m))
                        .filter(|o| o.role == Role::Room)
                        .map(|o| {
                            let sync = (o.offset_confidence > 0.0 && r.offset_confidence > 0.0).then_some(o.offset_s - r.offset_s);
                            json!({ "source_id": o.id, "file": o.original_name, "duration_s": o.duration_s,
                                    "locked": o.link_locked, "sync_s": sync,
                                    "by": if o.tc_start_s.is_some() && r.tc_start_s.is_some() { "timecode" } else { "folder" } })
                        })
                        .collect()
                })
                .unwrap_or_default();
            v["rooms"] = json!(rooms);
            v["shared_with"] = json!(others);
            v["session"] = json!(ss.map(|s| s.name.clone()));
            rec_views.push(v);
        }
        cast.push(json!({ "id": c.id, "label": c.label, "color_index": c.color_index,
                          "lines": lines_by.get(&c.id).copied().unwrap_or(0), "recordings": rec_views }));
    }
    let unassigned: Vec<Value> = files
        .iter()
        .filter(|s| {
            s.role != Role::Room
                && (s.role == Role::Unknown
                    || s.character
                        .as_ref()
                        .map(|c| !doc.characters.contains_key(c))
                        .unwrap_or(true))
        })
        .map(|s| (*s).clone())
        .collect::<Vec<_>>()
        .iter()
        .map(|s| file_view(svc, s))
        .collect();
    let rooms: Vec<Value> = files
        .iter()
        .filter(|s| s.role == Role::Room)
        .map(|s| (*s).clone())
        .collect::<Vec<_>>()
        .iter()
        .map(|s| file_view(svc, s))
        .collect();
    Ok(json!({
        "script": script,
        "files": {
            "total": files.len(),
            "recordings": files.iter().filter(|s| s.role == Role::Character).count(),
            "rooms": files.iter().filter(|s| s.role == Role::Room).count(),
            "unknown": files.iter().filter(|s| s.role == Role::Unknown).count(),
        },
        "matched_at": doc.settings.get("match").and_then(|s| s.value["at"].as_str().map(String::from)),
        "analyzed": !doc.takes.is_empty(),
        "fixes": fixes,
        "cast": cast,
        "unassigned": unassigned,
        "room_files": rooms,
    }))
}
