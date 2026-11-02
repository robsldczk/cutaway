//! Studio mode for an open episode (D64–D66): the lock file and sync warnings of an episode
//! in a shared pipeline, and buckets as folders of the show's library.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::json;

use apc_core::ids::new_id;
use apc_core::project::{EpisodePaths, SHOW_PREFIX};
use apc_core::show::is_conflict_copy;
use apc_core::*;
use apc_sound::show_library::{bucket_name, BucketFolder};

use crate::error::Result;
use crate::service::Service;

/// A lock older than this is a leftover (a crash, a machine that went to sleep).
const LOCK_STALE_H: i64 = 12;

#[derive(Serialize, Deserialize)]
struct Lock {
    host: String,
    user: String,
    since: String,
}

/// This machine's name, as other machines will see it in a lock.
pub fn host_name() -> &'static str {
    static HOST: OnceLock<String> = OnceLock::new();
    HOST.get_or_init(|| {
        let from_cmd = |cmd: &str, args: &[&str]| {
            std::process::Command::new(cmd)
                .args(args)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        };
        #[cfg(target_os = "macos")]
        let pretty = from_cmd("scutil", &["--get", "ComputerName"]);
        #[cfg(not(target_os = "macos"))]
        let pretty: Option<String> = None;
        pretty
            .or_else(|| from_cmd("hostname", &[]))
            .or_else(|| std::env::var("COMPUTERNAME").ok())
            .unwrap_or_else(|| "this computer".into())
    })
}

fn lock_path(paths: &EpisodePaths) -> PathBuf {
    let name = paths
        .file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    paths.root.join(format!(".{name}.lock"))
}

/// Takes the episode's lock (a small file the pipeline syncs to the other machines).
/// Returns a warning when another machine holds it.
pub fn take_lock(paths: &EpisodePaths) -> Result<Option<String>> {
    let p = lock_path(paths);
    let mut warning = None;
    if let Some(l) = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<Lock>(&s).ok())
    {
        let fresh = chrono::DateTime::parse_from_rfc3339(&l.since)
            .map(|t| chrono::Utc::now().signed_duration_since(t).num_hours() < LOCK_STALE_H)
            .unwrap_or(false);
        if l.host != host_name() && fresh {
            let since = chrono::DateTime::parse_from_rfc3339(&l.since)
                .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_default();
            warning = Some(format!(
                "This episode is open on {} ({}, since {since}). Changes made on both machines at once end in a sync conflict.",
                l.host, l.user
            ));
        }
    }
    let lock = Lock {
        host: host_name().to_string(),
        user: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_default(),
        since: chrono::Utc::now().to_rfc3339(),
    };
    apc_core::show::write_atomic(&p, serde_json::to_string(&lock)?.as_bytes())?;
    Ok(warning)
}

/// Gives the lock back (only our own).
pub fn release_lock(paths: &EpisodePaths) {
    let p = lock_path(paths);
    let ours = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<Lock>(&s).ok())
        .map(|l| l.host == host_name())
        .unwrap_or(false);
    if ours {
        let _ = std::fs::remove_file(p);
    }
}

/// Sync-conflict copies of the episode file next to it.
pub fn sync_warnings(paths: &EpisodePaths) -> Vec<String> {
    let ext = paths
        .file
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut v: Vec<String> = std::fs::read_dir(&paths.root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| is_conflict_copy(n) && n.ends_with(&format!(".{ext}")))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.into_iter()
        .map(|n| format!("Sync conflict: {n} is another machine's version of this episode. Open it to compare; this one stays as it is."))
        .collect()
}

fn singular(w: &str) -> String {
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
        w[..w.len() - 1].to_string()
    } else {
        w.to_string()
    }
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(singular)
        .collect()
}

/// The library folder a bucket named `name` (with `keywords`) belongs in: the folder whose
/// words all appear in them, the most specific first ("Doors and hatches" → `doors`).
pub fn match_folder(
    folders: &[BucketFolder],
    name: &str,
    keywords: &[String],
    music: bool,
) -> Option<String> {
    let have: BTreeSet<String> = words(name)
        .into_iter()
        .chain(keywords.iter().flat_map(|k| words(k)))
        .collect();
    folders
        .iter()
        .filter(|f| f.music == music)
        .filter_map(|f| {
            let need = words(&f.name);
            (!need.is_empty() && need.iter().all(|w| have.contains(w)))
                .then(|| (need.len(), f.rel.clone()))
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, rel)| rel)
}

/// A bucket of music (its folder goes under `music/`).
pub fn is_music(doc: &Document, b: &SoundBucket) -> bool {
    b.name.eq_ignore_ascii_case("music")
        || b.cue_ids
            .iter()
            .filter_map(|c| doc.cues.get(c))
            .any(|c| c.cue_type == CueType::Music)
}

/// Gives every folder of the show's library a bucket, and every bucket without a folder
/// its folder (an existing one it names, or its own, made with its first sound). One
/// undoable step, only when something changes.
pub fn link_folders(svc: &mut Service) -> Result<usize> {
    let Some(lib) = svc.show_lib.clone() else {
        return Ok(0);
    };
    let (folders, planned) = {
        let Ok(l) = lib.lock() else {
            return Ok(0);
        };
        let doc = svc.doc();
        let folders = l.folders();
        let mut taken: BTreeSet<String> = doc
            .buckets
            .values()
            .filter_map(|b| b.folder.clone())
            .collect();
        let mut planned: Vec<SoundBucket> = Vec::new();
        let mut open: Vec<&SoundBucket> = doc
            .buckets
            .values()
            .filter(|b| b.character.is_none() && b.folder.is_none())
            .collect();
        open.sort_by_key(|b| b.order);
        for b in open {
            let music = is_music(doc, b);
            let f = match_folder(&folders, &b.name, &b.keywords, music)
                .filter(|f| !taken.contains(f))
                .unwrap_or_else(|| l.folder_for(&b.name, music));
            taken.insert(f.clone());
            planned.push(SoundBucket {
                folder: Some(f),
                ..b.clone()
            });
        }
        let mut order = doc.buckets.len() as u32;
        for f in &folders {
            if taken.contains(&f.rel) {
                continue;
            }
            planned.push(SoundBucket {
                id: new_id("bk"),
                name: bucket_name(&f.name),
                keywords: vec![f.name.replace(['-', '_'], " ").to_lowercase()],
                scene_ids: vec![],
                cue_ids: vec![],
                kept: vec![],
                previews: vec![],
                dismissed: vec![],
                length_min_s: None,
                length_max_s: None,
                exclude: vec![],
                ai_suggested: false,
                order,
                character: None,
                search_offset: 0,
                folder: Some(f.rel.clone()),
            });
            order += 1;
        }
        (folders.len(), planned)
    };
    if planned.is_empty() {
        return Ok(0);
    }
    let n = planned.len();
    let r = svc.bus.execute_fn(
        "link_show_folders",
        &json!({ "buckets": n, "folders": folders }),
        |tx| {
            for b in &planned {
                tx.put(b)?;
            }
            Ok(json!({ "linked": n }))
        },
    )?;
    if r.seq.is_some() {
        svc.after_write()?;
    }
    Ok(n)
}

/// The sounds in a bucket's folder, without those taken out in this episode.
pub fn folder_sounds(svc: &Service, b: &SoundBucket) -> Vec<Asset> {
    let (Some(f), Some(lib)) = (&b.folder, &svc.show_lib) else {
        return Vec::new();
    };
    lib.lock()
        .map(|l| l.in_folder(f))
        .unwrap_or_default()
        .into_iter()
        .filter(|a| !b.dismissed.contains(&a.id))
        .collect()
}

/// Provider ids of the sounds in a bucket's folder (a search skips what is there).
pub fn folder_remote(svc: &Service, b: &SoundBucket) -> Vec<String> {
    folder_sounds(svc, b)
        .into_iter()
        .filter_map(|a| a.remote_id)
        .collect()
}

/// Is any file in the bucket's folder (hidden ones too)?
pub fn folder_has_files(svc: &Service, b: &SoundBucket) -> bool {
    match (&b.folder, &svc.show_lib) {
        (Some(f), Some(lib)) => lib
            .lock()
            .map(|l| !l.in_folder(f).is_empty())
            .unwrap_or(false),
        _ => false,
    }
}

/// Where a bucket's new sounds go: its folder in the show's library (`show:sfx/doors`),
/// made now if it doesn't exist yet. `None` outside a show and for vocal buckets.
pub fn bucket_dest(svc: &mut Service, bucket_id: &str) -> Result<Option<String>> {
    let Some(lib) = svc.show_lib.clone() else {
        return Ok(None);
    };
    let Some(b) = svc.doc().buckets.get(bucket_id).cloned() else {
        return Ok(None);
    };
    if b.character.is_some() {
        return Ok(None);
    }
    let folder = match &b.folder {
        Some(f) => f.clone(),
        None => {
            let music = is_music(svc.doc(), &b);
            let f = lib
                .lock()
                .map(|l| l.folder_for(&b.name, music))
                .unwrap_or_else(|_| apc_sound::show_library::folder_slug(&b.name));
            let r = svc.bus.execute_fn(
                "edit_bucket",
                &json!({ "bucket_id": b.id, "folder": f }),
                |tx| {
                    if let Some(mut x) = tx.doc().buckets.get(&b.id).cloned() {
                        x.folder = Some(f.clone());
                        tx.put(&x)?;
                    }
                    Ok(serde_json::Value::Null)
                },
            )?;
            if r.seq.is_some() {
                svc.after_write()?;
            }
            f
        }
    };
    if let Ok(l) = lib.lock() {
        l.make_folder(&folder)?;
    }
    Ok(Some(format!("{SHOW_PREFIX}{folder}")))
}

/// Re-reads the show's library (new files from the pipeline) and links new folders.
pub fn rescan(svc: &mut Service) -> Result<serde_json::Value> {
    let Some(lib) = svc.show_lib.clone() else {
        return Err(crate::error::AppError::Params(
            "this episode is not part of a show".into(),
        ));
    };
    let report = lib
        .lock()
        .map_err(|_| crate::error::AppError::Other("library lock poisoned".into()))?
        .scan()?;
    let linked = link_folders(svc)?;
    svc.bus.refresh()?;
    Ok(
        json!({ "files": report.files, "hashed": report.hashed, "removed": report.removed, "folders": report.folders, "new_buckets": linked }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(rel: &str, music: bool) -> BucketFolder {
        BucketFolder {
            rel: rel.into(),
            name: rel.rsplit('/').next().unwrap().into(),
            music,
        }
    }

    #[test]
    fn spotted_groups_find_their_folders() {
        let folders = vec![
            f("sfx/doors", false),
            f("sfx/footsteps", false),
            f("sfx/room-tone", false),
            f("sfx/time-spring", false),
            f("music/Stings", true),
        ];
        let m = |n: &str, k: &[&str]| {
            match_folder(
                &folders,
                n,
                &k.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                false,
            )
        };
        assert_eq!(m("Doors and hatches", &[]).as_deref(), Some("sfx/doors"));
        assert_eq!(m("Footsteps", &[]).as_deref(), Some("sfx/footsteps"));
        assert_eq!(m("Props", &["room tone"]).as_deref(), Some("sfx/room-tone"));
        assert_eq!(
            m("Machines", &["time spring hum"]).as_deref(),
            Some("sfx/time-spring")
        );
        assert_eq!(m("Sci-fi", &["beam"]), None);
        assert_eq!(m("Stings", &[]), None, "music folders only for music");
    }
}
