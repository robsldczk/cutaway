//! The sound library. A standalone episode keeps its sounds in its own `Library/<category>/`
//! (mirrored into the episode document). In a show, sounds live in the show's library
//! (`show_library`, paths `show:sfx/doors/…`), and the episode's own `Library/` keeps only
//! the vocals older builds cut from its recordings (D118).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

use apc_core::ids::new_id;
use apc_core::media::hash_file;
use apc_core::project::SHOW_PREFIX;
use apc_core::{Asset, AssetSource};

use crate::error::{Result, SoundError};
use crate::show_library::{ShowLibrary, SoundMeta};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LibraryAsset {
    pub asset: Asset,
    /// True when an identical file was already in the library.
    pub deduplicated: bool,
}

/// The show's library, shared by the service and the libraries it hands out.
pub type SharedShowLibrary = Arc<Mutex<ShowLibrary>>;

/// An episode's library: its own folder, plus the show's library in a show.
pub struct Library {
    /// The episode's own `Library/` folder.
    pub root: PathBuf,
    pub show: Option<SharedShowLibrary>,
}

fn safe(s: &str) -> String {
    s.chars()
        .map(|c| if "\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect()
}

impl Library {
    pub fn new(root: PathBuf, show: Option<SharedShowLibrary>) -> Self {
        Library { root, show }
    }

    fn show_lib(&self) -> Option<MutexGuard<'_, ShowLibrary>> {
        self.show.as_ref().and_then(|s| s.lock().ok())
    }

    /// Path of an asset file. A show sound that was moved in the pipeline is found again
    /// by its hash.
    pub fn path_of(&self, a: &Asset) -> PathBuf {
        if let Some(rel) = a.file.strip_prefix(SHOW_PREFIX) {
            if let Some(show) = self.show_lib() {
                let p = show.path_of(rel);
                if !p.exists() {
                    if let Some(e) = show.by_hash(&a.hash) {
                        return show.path_of(&e.rel);
                    }
                }
                return p;
            }
            return self.root.join(rel);
        }
        let p = Path::new(&a.file);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    /// Copies a file into the library (or finds the identical file already there).
    /// `category` is a folder of the episode's own library (`SFX/Doors`), or
    /// a folder of the show's library with the `show:` prefix (`show:sfx/doors`).
    /// `local` are the episode's own assets.
    #[allow(clippy::too_many_arguments)]
    pub fn import(
        &self,
        src: &Path,
        category: &str,
        tags: &[String],
        source: AssetSource,
        duration_s: f64,
        prompt: Option<String>,
        provider: Option<String>,
        remote_id: Option<String>,
        local: &[Asset],
    ) -> Result<LibraryAsset> {
        if let Some(folder) = category.strip_prefix(SHOW_PREFIX) {
            let mut show = self
                .show_lib()
                .ok_or_else(|| SoundError::Invalid("this episode is not part of a show".into()))?;
            let meta = SoundMeta {
                source: (source != AssetSource::Imported).then_some(source),
                license: None,
                provider,
                remote_id,
                prompt,
                tags: Vec::new(),
            };
            let (mut asset, existed) = show.import(src, folder, meta)?;
            if asset.duration_s <= 0.0 {
                asset.duration_s = duration_s;
            }
            return Ok(LibraryAsset {
                asset,
                deduplicated: existed,
            });
        }
        let hash = hash_file(src)?;
        if let Some(a) = local.iter().find(|a| a.hash == hash) {
            return Ok(LibraryAsset {
                asset: a.clone(),
                deduplicated: true,
            });
        }
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| SoundError::Invalid("file has no name".into()))?;
        let cat = safe(category.trim_matches('/'));
        let dir = self.root.join(&cat);
        std::fs::create_dir_all(&dir)?;
        let mut dest = dir.join(&name);
        let mut k = 2;
        while dest.exists() {
            let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
            dest = dir.join(format!("{stem} ({k}).{ext}"));
            k += 1;
        }
        std::fs::copy(src, &dest)?;
        let rel = dest
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| dest.to_string_lossy().into_owned());
        Ok(LibraryAsset {
            asset: Asset {
                id: new_id("as"),
                file: rel,
                hash,
                category: cat,
                tags: tags.to_vec(),
                duration_s,
                source,
                license: None,
                prompt,
                provider,
                lufs: None,
                remote_id,
            },
            deduplicated: false,
        })
    }

    /// The episode's own assets, then the show's sounds.
    pub fn list(&self, local: &[Asset]) -> Result<Vec<Asset>> {
        let mut out = local.to_vec();
        if let Some(show) = self.show_lib() {
            for a in show.assets() {
                if !out.iter().any(|x| x.id == a.id || x.hash == a.hash) {
                    out.push(a);
                }
            }
        }
        Ok(out)
    }

    /// Text search over file name, category and tags.
    pub fn search(&self, query: &str, local: &[Asset]) -> Result<Vec<Asset>> {
        let words: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        Ok(self
            .list(local)?
            .into_iter()
            .filter(|a| {
                let hay = format!("{} {} {}", a.file, a.category, a.tags.join(" ")).to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .collect())
    }

    /// Tags a show sound (shared through the show); an episode's own sounds carry their
    /// tags in the document.
    pub fn tag(&self, a: &Asset, tags: &[String]) -> Result<()> {
        if a.file.starts_with(SHOW_PREFIX) {
            if let Some(mut show) = self.show_lib() {
                show.record(
                    &a.hash,
                    SoundMeta {
                        tags: tags.to_vec(),
                        ..Default::default()
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Removes a sound: a show sound goes to `Sound/Cutaway/Trash/`, an episode's own to
    /// `trash`.
    pub fn delete(&self, a: &Asset, trash: &Path) -> Result<()> {
        if a.file.starts_with(SHOW_PREFIX) {
            if let Some(mut show) = self.show_lib() {
                return show.delete(&a.id);
            }
        }
        let p = self.path_of(a);
        if p.exists() {
            std::fs::create_dir_all(trash)?;
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| a.id.clone());
            std::fs::rename(&p, trash.join(format!("{} {}", a.id, name)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_library_dedups_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::new(dir.path().join("Library"), None);
        let f = dir.path().join("door_slam.wav");
        std::fs::write(&f, b"RIFF door").unwrap();
        let a = lib
            .import(
                &f,
                "SFX/Doors",
                &["door".into()],
                AssetSource::Imported,
                1.0,
                None,
                None,
                None,
                &[],
            )
            .unwrap();
        assert!(!a.deduplicated);
        assert!(dir.path().join("Library/SFX/Doors/door_slam.wav").exists());
        let local = vec![a.asset.clone()];
        let copy = dir.path().join("same_again.wav");
        std::fs::write(&copy, b"RIFF door").unwrap();
        let b = lib
            .import(
                &copy,
                "SFX/Misc",
                &[],
                AssetSource::Imported,
                1.0,
                None,
                None,
                None,
                &local,
            )
            .unwrap();
        assert!(b.deduplicated);
        assert_eq!(b.asset.id, a.asset.id);
        assert_eq!(lib.list(&local).unwrap().len(), 1);
        assert_eq!(lib.search("door", &local).unwrap().len(), 1);
        assert!(lib.search("alarm", &local).unwrap().is_empty());
        assert!(lib
            .import(
                &copy,
                "show:sfx/doors",
                &[],
                AssetSource::Imported,
                1.0,
                None,
                None,
                None,
                &[]
            )
            .is_err());
        lib.delete(&a.asset, &dir.path().join("Backups/Trash"))
            .unwrap();
        assert_eq!(
            std::fs::read_dir(dir.path().join("Backups/Trash"))
                .unwrap()
                .count(),
            1
        );
    }
}
