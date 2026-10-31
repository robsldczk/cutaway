//! Media import (copy into the project by default, or reference in place), hashing and
//! relinking missing files by hash. Source files are never modified.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::model::{Document, Stored};
use crate::project::EpisodePaths;

/// BLAKE3 of a file, hex.
pub fn hash_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

#[derive(Debug, Clone)]
pub struct ImportedFile {
    pub stored_path: String,
    pub original: PathBuf,
    pub original_name: String,
    pub hash: String,
    pub size: u64,
    pub stored: Stored,
    /// Transcript sidecar (`<name>.json`) copied or referenced alongside.
    pub sidecar: Option<String>,
}

pub const AUDIO_EXTS: &[&str] = &["wav", "wave", "bwf", "aif", "aiff", "mp3", "flac", "m4a"];

/// Audio files in a folder (non-recursive) or the file itself.
pub fn audio_files_in(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut v: Vec<PathBuf> = std::fs::read_dir(path)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();
    v.sort();
    Ok(v)
}

fn unique_dest(dir: &Path, name: &str, hash: &str) -> Result<(PathBuf, bool)> {
    let p = dir.join(name);
    if !p.exists() {
        return Ok((p, false));
    }
    if hash_file(&p)? == hash {
        return Ok((p, true));
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.to_string(), String::new()),
    };
    for k in 2..1000 {
        let p = dir.join(format!("{stem} ({k}){ext}"));
        if !p.exists() {
            return Ok((p, false));
        }
        if hash_file(&p)? == hash {
            return Ok((p, true));
        }
    }
    Err(CoreError::invalid(format!("too many copies of {name}")))
}

/// Imports one file into `Audio/<session_folder>/` (copy) or references it.
pub fn import_file(
    paths: &EpisodePaths,
    src: &Path,
    session_folder: &str,
    copy: bool,
) -> Result<ImportedFile> {
    let meta = std::fs::metadata(src)?;
    let hash = hash_file(src)?;
    let original_name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sidecar_src = src.with_extension("json");
    let (stored_path, stored, sidecar) = if copy {
        let dir = paths.audio.join(session_folder);
        std::fs::create_dir_all(&dir)?;
        let (dest, exists) = unique_dest(&dir, &original_name, &hash)?;
        if !exists {
            std::fs::copy(src, &dest)?;
        }
        let sidecar = if sidecar_src.exists() {
            let sd = dest.with_extension("json");
            std::fs::copy(&sidecar_src, &sd)?;
            Some(paths.relative(&sd))
        } else {
            None
        };
        (paths.relative(&dest), Stored::Copied, sidecar)
    } else {
        let abs = std::fs::canonicalize(src)?;
        let sidecar = sidecar_src
            .exists()
            .then(|| std::fs::canonicalize(&sidecar_src).ok())
            .flatten()
            .map(|p| p.to_string_lossy().into_owned());
        (
            abs.to_string_lossy().into_owned(),
            Stored::Referenced,
            sidecar,
        )
    };
    Ok(ImportedFile {
        stored_path,
        original: src.to_path_buf(),
        original_name,
        hash,
        size: meta.len(),
        stored,
        sidecar,
    })
}

/// Sources whose file is missing on disk.
pub fn missing_sources(doc: &Document, paths: &EpisodePaths) -> Vec<String> {
    doc.sources
        .values()
        .filter(|s| !paths.resolve(&s.path).exists())
        .map(|s| s.id.clone())
        .collect()
}

/// Finds missing sources again under `search_dir` by size and hash.
/// Returns `(source_id, stored path)` pairs.
pub fn relink_plan(
    doc: &Document,
    paths: &EpisodePaths,
    search_dir: &Path,
) -> Result<Vec<(String, String)>> {
    let missing: Vec<&crate::model::SourceFile> = doc
        .sources
        .values()
        .filter(|s| !paths.resolve(&s.path).exists())
        .collect();
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut stack = vec![search_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            let candidates: Vec<&&crate::model::SourceFile> = missing
                .iter()
                .filter(|s| s.size == m.len() && !out.iter().any(|(id, _)| id == &s.id))
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let h = hash_file(&p)?;
            if let Some(s) = candidates.into_iter().find(|s| s.hash == h) {
                out.push((s.id.clone(), paths.relative(&p)));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::CommandBus;
    use crate::commands::Command;
    use crate::model::{Role, SourceFile};

    #[test]
    fn copy_import_hash_and_relink() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, store) = crate::project::create_episode(dir.path(), "Ep", None).unwrap();
        let mut bus = CommandBus::new(store, "test").unwrap();
        let outside = dir.path().join("session");
        std::fs::create_dir_all(&outside).unwrap();
        let wav = outside.join("Paul.wav");
        std::fs::write(&wav, b"RIFF fake audio bytes").unwrap();
        std::fs::write(outside.join("Paul.json"), b"{\"lines\":[]}").unwrap();

        let f = import_file(&paths, &wav, "Session 1", true).unwrap();
        assert_eq!(f.stored, Stored::Copied);
        assert_eq!(f.stored_path, "Audio/Session 1/Paul.wav");
        assert_eq!(f.sidecar.as_deref(), Some("Audio/Session 1/Paul.json"));
        assert_eq!(f.hash, hash_file(&wav).unwrap());
        // importing the same file twice reuses the copy
        let again = import_file(&paths, &wav, "Session 1", true).unwrap();
        assert_eq!(again.stored_path, f.stored_path);

        let src = SourceFile {
            id: "src_1".into(),
            path: f.stored_path.clone(),
            original_name: f.original_name.clone(),
            stored: f.stored,
            hash: f.hash.clone(),
            size: f.size,
            role: Role::Unknown,
            character: None,
            role_confidence: 0.0,
            session_id: None,
            sample_rate: 0,
            channels: 0,
            duration_s: 0.0,
            tc_start_s: None,
            offset_s: 0.0,
            offset_confidence: 0.0,
            transcript: f.sidecar.clone(),
            missing: false,
        };
        bus.execute(&Command::AddSources { sources: vec![src] })
            .unwrap();

        // move the file away: it is missing, then found again by hash
        let moved_dir = dir.path().join("elsewhere/deeper");
        std::fs::create_dir_all(&moved_dir).unwrap();
        let moved = moved_dir.join("renamed.wav");
        std::fs::rename(paths.resolve(&f.stored_path), &moved).unwrap();
        std::fs::write(moved_dir.join("decoy.wav"), b"RIFF fake audio bytez").unwrap();
        assert_eq!(
            missing_sources(bus.doc(), &paths),
            vec!["src_1".to_string()]
        );
        let plan = relink_plan(bus.doc(), &paths, &dir.path().join("elsewhere")).unwrap();
        assert_eq!(plan.len(), 1);
        assert!(plan[0].1.ends_with("renamed.wav"));
        bus.execute(&Command::RelinkMedia {
            source_id: plan[0].0.clone(),
            path: plan[0].1.clone(),
        })
        .unwrap();
        assert!(missing_sources(bus.doc(), &paths).is_empty());
    }

    #[test]
    fn reference_keeps_the_file_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, _) = crate::project::create_episode(dir.path(), "Ep", None).unwrap();
        let wav = dir.path().join("Room.wav");
        std::fs::write(&wav, b"room").unwrap();
        let f = import_file(&paths, &wav, "S", false).unwrap();
        assert_eq!(f.stored, Stored::Referenced);
        assert!(Path::new(&f.stored_path).is_absolute());
        assert!(!paths.audio.join("S").exists());
    }
}
