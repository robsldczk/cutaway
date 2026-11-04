//! Clean up (D74): the AI reads the names of the loose sound effects in `sfx/` and proposes
//! a folder for each; nothing moves until the moves are confirmed. It runs only when asked,
//! because a move changes the shared pipeline for every machine and episode.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use apc_ai::task::{SortFolder, SortSound, SoundSortInput};
use apc_sound::show_library::folder_slug;

use crate::error::{AppError, Result};
use crate::methods::CleanupMove;
use crate::service::Service;

/// Examples and cues a folder shows the AI: enough to read it, not the whole library.
const SAMPLE: usize = 6;

fn no_show() -> AppError {
    AppError::Params("this episode is not part of a show".into())
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// Where a proposed folder lands: a folder we know, or a new one directly under `sfx/`.
fn target(known: &BTreeSet<String>, sfx: &str, folder: &str) -> Option<String> {
    let f = folder.trim_matches('/');
    if f.is_empty() || f == sfx {
        return None;
    }
    if known.contains(f) {
        return Some(f.to_string());
    }
    Some(format!("{sfx}/{}", folder_slug(file_name(f))))
}

/// Asks the AI where the loose sounds belong. Changes nothing.
pub fn plan(svc: &mut Service) -> Result<Value> {
    let lib = svc.show_lib.clone().ok_or_else(no_show)?;
    let (sfx, sounds, mut folders) = {
        let l = lib
            .lock()
            .map_err(|_| AppError::Other("library lock poisoned".into()))?;
        let sfx = l.loose_folder(false);
        let sounds: Vec<SortSound> = l
            .direct(&sfx)
            .into_iter()
            .map(|e| SortSound {
                file: e.rel.clone(),
                name: file_name(&e.rel).to_string(),
                duration_s: e.asset.duration_s,
            })
            .collect();
        let folders: Vec<SortFolder> = l
            .folders()
            .into_iter()
            .filter(|f| !f.music)
            .map(|f| SortFolder {
                examples: l
                    .direct(&f.rel)
                    .into_iter()
                    .take(SAMPLE)
                    .map(|e| file_name(&e.rel).to_string())
                    .collect(),
                folder: f.rel,
                exists: true,
                cues: vec![],
            })
            .collect();
        (sfx, sounds, folders)
    };
    if sounds.is_empty() {
        return Ok(json!({ "moves": [], "stay": [] }));
    }
    let doc = svc.doc();
    for b in doc.buckets.values() {
        let Some(f) = b
            .folder
            .as_ref()
            .filter(|f| f.starts_with(&format!("{sfx}/")))
        else {
            continue;
        };
        let cues: Vec<String> = b
            .cue_ids
            .iter()
            .filter_map(|c| doc.cues.get(c))
            .take(SAMPLE)
            .map(|c| c.description.clone())
            .collect();
        match folders.iter_mut().find(|x| x.folder == *f) {
            Some(x) => x.cues = cues,
            None if !cues.is_empty() => folders.push(SortFolder {
                folder: f.clone(),
                exists: false,
                examples: vec![],
                cues,
            }),
            None => {}
        }
    }
    let known: BTreeSet<String> = folders.iter().map(|f| f.folder.clone()).collect();
    let on_disk: BTreeSet<String> = folders
        .iter()
        .filter(|f| f.exists)
        .map(|f| f.folder.clone())
        .collect();
    let input = SoundSortInput {
        sounds: sounds.clone(),
        folders,
    };
    let (runner, warn) = svc.ai_runner();
    let out = runner.run(&input)?;
    let Some(o) = out.output else {
        return Err(AppError::Other(
            out.flagged.unwrap_or_else(|| "clean up failed".into()),
        ));
    };
    let mut moves = Vec::new();
    let mut placed: BTreeSet<String> = BTreeSet::new();
    for m in o.moves {
        // only the sounds we asked about, once each
        if !sounds.iter().any(|s| s.file == m.file) || placed.contains(&m.file) {
            continue;
        }
        let Some(to) = m.folder.and_then(|f| target(&known, &sfx, &f)) else {
            continue;
        };
        placed.insert(m.file.clone());
        moves.push(json!({
            "name": crate::views::sound_name(&m.file), "file": m.file, "to": to,
            "new_folder": !on_disk.contains(&to), "reason": m.reason,
        }));
    }
    let stay: Vec<String> = sounds
        .iter()
        .filter(|s| !placed.contains(&s.file))
        .map(|s| crate::views::sound_name(&s.file))
        .collect();
    Ok(json!({ "moves": moves, "stay": stay, "warning": warn }))
}

/// Moves the confirmed sounds. A move that can't be made is reported and the rest go on.
pub fn apply(svc: &mut Service, moves: Vec<CleanupMove>) -> Result<Value> {
    let lib = svc.show_lib.clone().ok_or_else(no_show)?;
    let mut done: Vec<(String, String)> = Vec::new();
    let mut failed = Vec::new();
    let log = {
        let mut l = lib
            .lock()
            .map_err(|_| AppError::Other("library lock poisoned".into()))?;
        for m in &moves {
            match l.move_loose(&m.file, &m.to) {
                Ok(to) => done.push((m.file.clone(), to)),
                Err(e) => failed.push(json!({ "file": m.file, "error": e.to_string() })),
            }
        }
        if done.is_empty() {
            None
        } else {
            let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
            Some(l.log_moves(&stamp, &done)?)
        }
    };
    let buckets = crate::studio::link_folders(svc)?;
    let relinked = crate::studio::relink_sounds(svc)?;
    svc.bus.refresh()?;
    Ok(json!({
        "moved": done.len(), "failed": failed, "new_buckets": buckets, "relinked": relinked,
        "log": log.map(|p| p.to_string_lossy().into_owned()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_stay_directly_under_sfx() {
        let known: BTreeSet<String> = ["sfx/kitchen".to_string()].into();
        assert_eq!(
            target(&known, "sfx", "sfx/kitchen").as_deref(),
            Some("sfx/kitchen")
        );
        assert_eq!(
            target(&known, "sfx", "Key Rings").as_deref(),
            Some("sfx/key-rings")
        );
        assert_eq!(
            target(&known, "sfx", "sfx/a/../../etc").as_deref(),
            Some("sfx/etc")
        );
        assert_eq!(target(&known, "sfx", "sfx/"), None);
        assert_eq!(target(&known, "sfx", ""), None);
    }
}
