//! Show and episode folders, visible in Finder like a Logic project.
//!
//! ```text
//! TARMAC/  TARMAC.cutawayshow  Library/{SFX,Ambience,Music}/  Episodes/Ep02/
//! Ep02/    Ep02.cutaway  Audio/ Script/ Backups/ Exports/ Cache/ Logs/ [Library/]
//! ```

use std::path::{Path, PathBuf};

use crate::brand;
use crate::error::{CoreError, Result};
use crate::store::{EpisodeStore, ShowStore};

/// Folders of one episode.
#[derive(Clone, Debug)]
pub struct EpisodePaths {
    pub root: PathBuf,
    pub file: PathBuf,
    pub audio: PathBuf,
    pub script: PathBuf,
    pub backups: PathBuf,
    pub exports: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    /// Local library of a standalone episode, or the show's library.
    pub library: PathBuf,
    /// The show file when the episode lives in a show.
    pub show_file: Option<PathBuf>,
}

impl EpisodePaths {
    /// Paths for an episode file.
    pub fn for_file(file: &Path) -> Result<Self> {
        let root = file
            .parent()
            .ok_or_else(|| CoreError::invalid("episode file has no folder"))?
            .to_path_buf();
        let show_file = find_show_file(&root);
        let library = match &show_file {
            Some(sf) => sf
                .parent()
                .map(|p| p.join("Library"))
                .unwrap_or_else(|| root.join("Library")),
            None => root.join("Library"),
        };
        Ok(EpisodePaths {
            file: file.to_path_buf(),
            audio: root.join("Audio"),
            script: root.join("Script"),
            backups: root.join("Backups"),
            exports: root.join("Exports"),
            cache: root.join("Cache"),
            logs: root.join("Logs"),
            library,
            show_file,
            root,
        })
    }

    /// Path relative to the episode folder when inside it, absolute otherwise.
    pub fn relative(&self, p: &Path) -> String {
        match p.strip_prefix(&self.root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => p.to_string_lossy().into_owned(),
        }
    }

    /// Resolves a stored path.
    pub fn resolve(&self, stored: &str) -> PathBuf {
        let p = Path::new(stored);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for d in [
            &self.audio,
            &self.script,
            &self.backups,
            &self.exports,
            &self.cache,
            &self.logs,
        ] {
            std::fs::create_dir_all(d)?;
        }
        if self.show_file.is_none() {
            create_library(&self.library)?;
        }
        Ok(())
    }
}

fn create_library(lib: &Path) -> Result<()> {
    for d in ["SFX", "Ambience", "Music"] {
        std::fs::create_dir_all(lib.join(d))?;
    }
    Ok(())
}

/// Looks for `<Show>/<Show>.cutawayshow` two levels up (`<Show>/Episodes/<Ep>/`).
fn find_show_file(episode_root: &Path) -> Option<PathBuf> {
    let episodes = episode_root.parent()?;
    if episodes.file_name()?.to_str()? != "Episodes" {
        return None;
    }
    let show = episodes.parent()?;
    first_with_ext(show, brand::SHOW_EXT)
}

fn first_with_ext(dir: &Path, ext: &str) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(ext))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Finds the episode file in a folder (or returns the path if it is one).
pub fn find_episode_file(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    first_with_ext(path, brand::EPISODE_EXT).ok_or_else(|| {
        CoreError::not_found(
            "episode file",
            format!("*.{} in {}", brand::EPISODE_EXT, path.display()),
        )
    })
}

/// Finds the show file in a folder (or returns the path if it is one).
pub fn find_show_file_in(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    first_with_ext(path, brand::SHOW_EXT).ok_or_else(|| {
        CoreError::not_found(
            "show file",
            format!("*.{} in {}", brand::SHOW_EXT, path.display()),
        )
    })
}

/// A show folder.
pub struct ShowFolder {
    pub root: PathBuf,
    pub file: PathBuf,
}

impl ShowFolder {
    /// Creates `<parent>/<name>/` with the show file, `Library/` and `Episodes/`.
    pub fn create(parent: &Path, name: &str) -> Result<(ShowFolder, ShowStore)> {
        let root = parent.join(safe_name(name));
        if root.exists() {
            return Err(CoreError::invalid(format!(
                "{} already exists",
                root.display()
            )));
        }
        std::fs::create_dir_all(root.join("Episodes"))?;
        create_library(&root.join("Library"))?;
        let file = root.join(format!("{}.{}", safe_name(name), brand::SHOW_EXT));
        let store = ShowStore::create(&file)?;
        store
            .conn()
            .execute("INSERT INTO meta(key,value) VALUES('name',?1)", [name])?;
        Ok((ShowFolder { root, file }, store))
    }

    pub fn open(path: &Path) -> Result<(ShowFolder, ShowStore)> {
        let file = find_show_file_in(path)?;
        let root = file
            .parent()
            .ok_or_else(|| CoreError::invalid("show file has no folder"))?
            .to_path_buf();
        let store = ShowStore::open(&file)?;
        Ok((ShowFolder { root, file }, store))
    }
}

/// Creates an episode folder: inside a show (`<show>/Episodes/<name>/`) or standalone
/// (`<parent>/<name>/`, with a local `Library/`).
pub fn create_episode(
    parent: &Path,
    name: &str,
    show: Option<&Path>,
) -> Result<(EpisodePaths, EpisodeStore)> {
    let root = match show {
        Some(show_path) => {
            let (sf, _) = ShowFolder::open(show_path)?;
            sf.root.join("Episodes").join(safe_name(name))
        }
        None => parent.join(safe_name(name)),
    };
    if root.exists() {
        return Err(CoreError::invalid(format!(
            "{} already exists",
            root.display()
        )));
    }
    std::fs::create_dir_all(&root)?;
    let file = root.join(format!("{}.{}", safe_name(name), brand::EPISODE_EXT));
    let paths = EpisodePaths::for_file(&file)?;
    paths.ensure_dirs()?;
    let store = EpisodeStore::create(&file)?;
    store.set_meta("name", name)?;
    if let Some(sf) = &paths.show_file {
        let show = ShowStore::open(sf)?;
        let id = crate::ids::new_id("ep");
        store.set_meta("episode_id", &id)?;
        show.register_episode(&id, name, &format!("Episodes/{}", safe_name(name)))?;
    }
    Ok((paths, store))
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_and_episode_folders() {
        let dir = tempfile::tempdir().unwrap();
        let (show, _store) = ShowFolder::create(dir.path(), "TARMAC").unwrap();
        assert!(show.file.ends_with(format!("TARMAC.{}", brand::SHOW_EXT)));
        for d in [
            "Library/SFX",
            "Library/Ambience",
            "Library/Music",
            "Episodes",
        ] {
            assert!(show.root.join(d).is_dir(), "{d}");
        }
        let (paths, store) = create_episode(dir.path(), "Ep02", Some(&show.root)).unwrap();
        assert_eq!(paths.root, show.root.join("Episodes/Ep02"));
        for d in ["Audio", "Script", "Backups", "Exports", "Cache", "Logs"] {
            assert!(paths.root.join(d).is_dir(), "{d}");
        }
        assert_eq!(paths.show_file.as_deref(), Some(show.file.as_path()));
        assert_eq!(paths.library, show.root.join("Library"));
        assert_eq!(store.meta("name").unwrap().as_deref(), Some("Ep02"));
        let (_, show_store) = ShowFolder::open(&show.root).unwrap();
        assert_eq!(show_store.episodes().unwrap().len(), 1);
        assert_eq!(find_episode_file(&paths.root).unwrap(), paths.file);
        let mode: String = store
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn standalone_episode_has_local_library() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, _) = create_episode(dir.path(), "Solo", None).unwrap();
        assert!(paths.show_file.is_none());
        assert!(paths.root.join("Library/SFX").is_dir());
    }
}
