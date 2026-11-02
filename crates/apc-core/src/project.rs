//! Episode folders, visible in Finder like a Logic project. An episode stands alone, or
//! lives in a show (studio mode, see `show`): then its folder is the show layout's
//! `Episodes/<Episode>/Sound/`, and its sounds come from the show's library.
//!
//! ```text
//! Ep02/    Ep02.cutaway  Audio/ Script/ Backups/ Exports/ Cache/ Logs/ Library/
//! ```

use std::path::{Path, PathBuf};

use crate::brand;
use crate::error::{CoreError, Result};
use crate::show::Show;
use crate::store::EpisodeStore;

/// Prefix of an asset path in the show's library (`show:sfx/doors/door.wav`); other asset
/// paths are relative to the episode's own `Library/`.
pub const SHOW_PREFIX: &str = "show:";

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
    /// The episode's own library: everything of a standalone episode, a show episode's
    /// vocals (cut from its own recordings).
    pub library: PathBuf,
    /// The show, when the episode lives in one.
    pub show: Option<Show>,
}

impl EpisodePaths {
    /// Paths for an episode file.
    pub fn for_file(file: &Path) -> Result<Self> {
        let root = file
            .parent()
            .ok_or_else(|| CoreError::invalid("episode file has no folder"))?
            .to_path_buf();
        let show = Show::find_for(&root);
        Ok(EpisodePaths {
            file: file.to_path_buf(),
            audio: root.join("Audio"),
            script: root.join("Script"),
            backups: root.join("Backups"),
            exports: root.join("Exports"),
            cache: root.join("Cache"),
            logs: root.join("Logs"),
            library: root.join("Library"),
            show,
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

    /// Where an asset's file is: in the show's library (`show:`), absolute, or in the
    /// episode's own library.
    pub fn asset_path(&self, file: &str) -> PathBuf {
        if let Some(rest) = file.strip_prefix(SHOW_PREFIX) {
            return match &self.show {
                Some(s) => s.sound_dir().join(rest),
                None => self.library.join(rest),
            };
        }
        let p = Path::new(file);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.library.join(p)
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
        if self.show.is_none() {
            for d in ["SFX", "Ambience", "Music"] {
                std::fs::create_dir_all(self.library.join(d))?;
            }
        }
        Ok(())
    }
}

fn first_with_ext(dir: &Path, ext: &str) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(ext))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| !crate::show::ignored_name(n))
                .unwrap_or(false)
        })
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

/// Creates an episode: inside a show (`Episodes/<name>/Sound/`, the folders may already
/// exist in the pipeline) or standalone (`<parent>/<name>/`, with a local `Library/`).
pub fn create_episode(
    parent: &Path,
    name: &str,
    show: Option<&Path>,
) -> Result<(EpisodePaths, EpisodeStore)> {
    let root = match show {
        Some(show_path) => {
            let show = Show::open(show_path)?;
            let root = show.episode_dir(name);
            if let Some(f) = first_with_ext(&root, brand::EPISODE_EXT) {
                return Err(CoreError::invalid(format!(
                    "{name} already has a {} project ({})",
                    brand::DISPLAY_NAME,
                    f.display()
                )));
            }
            root
        }
        None => {
            let root = parent.join(safe_name(name));
            if root.exists() {
                return Err(CoreError::invalid(format!(
                    "{} already exists",
                    root.display()
                )));
            }
            root
        }
    };
    std::fs::create_dir_all(&root)?;
    let file = root.join(format!("{}.{}", safe_name(name), brand::EPISODE_EXT));
    let paths = EpisodePaths::for_file(&file)?;
    paths.ensure_dirs()?;
    let store = EpisodeStore::create(&file)?;
    store.set_meta("name", name)?;
    store.set_meta("episode_id", &crate::ids::new_id("ep"))?;
    Ok((paths, store))
}

pub fn safe_name(name: &str) -> String {
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
    fn episode_inside_a_show() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("TARMAC");
        std::fs::create_dir_all(root.join("Episodes/Ep02/Sound")).unwrap();
        let show = Show::create(&root, None).unwrap();
        let (paths, store) = create_episode(dir.path(), "Ep02", Some(&show.root)).unwrap();
        assert_eq!(paths.root, root.join("Episodes/Ep02/Sound"));
        assert_eq!(paths.file, root.join("Episodes/Ep02/Sound/Ep02.cutaway"));
        for d in ["Audio", "Script", "Backups", "Exports", "Cache", "Logs"] {
            assert!(paths.root.join(d).is_dir(), "{d}");
        }
        assert!(
            !paths.library.join("SFX").exists(),
            "sounds live in the show"
        );
        assert_eq!(paths.show.as_ref().map(|s| &s.root), Some(&root));
        assert_eq!(
            paths.asset_path("show:sfx/doors/door.wav"),
            root.join("Sound/sfx/doors/door.wav")
        );
        assert_eq!(
            paths.asset_path("Vocals/Paul/breath.wav"),
            paths.root.join("Library/Vocals/Paul/breath.wav")
        );
        assert_eq!(store.meta("name").unwrap().as_deref(), Some("Ep02"));
        assert_eq!(find_episode_file(&paths.root).unwrap(), paths.file);
        assert!(
            create_episode(dir.path(), "Ep02", Some(&root)).is_err(),
            "once"
        );
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
        assert!(paths.show.is_none());
        assert!(paths.root.join("Library/SFX").is_dir());
        assert_eq!(
            paths.asset_path("SFX/Misc/door.wav"),
            paths.root.join("Library/SFX/Misc/door.wav")
        );
    }
}
