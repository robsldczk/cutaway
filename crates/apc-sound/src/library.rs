//! The sound library: files under `Library/<category>/`, deduplicated by hash.
//! In a show it lives in the show database; a standalone episode keeps a local library
//! in its own document (the service mirrors used assets into the episode either way).

use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use apc_core::ids::new_id;
use apc_core::media::hash_file;
use apc_core::store::ShowStore;
use apc_core::{Asset, AssetSource};

use crate::error::{Result, SoundError};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LibraryAsset {
    pub asset: Asset,
    /// True when an identical file was already in the library.
    pub deduplicated: bool,
}

/// A library rooted at a `Library/` folder, optionally backed by a show database.
pub struct Library {
    pub root: PathBuf,
    pub show: Option<ShowStore>,
}

fn safe(s: &str) -> String {
    s.chars()
        .map(|c| if "\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect()
}

impl Library {
    pub fn new(root: PathBuf, show: Option<ShowStore>) -> Self {
        Library { root, show }
    }

    /// Path of an asset file.
    pub fn path_of(&self, a: &Asset) -> PathBuf {
        let p = Path::new(&a.file);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    fn find_by_hash(&self, hash: &str, local: &[Asset]) -> Result<Option<Asset>> {
        if let Some(show) = &self.show {
            let row = show
                .conn()
                .query_row(
                    "SELECT id, file, category, duration_s, source, license, prompt, provider, lufs FROM asset WHERE hash=?1",
                    [hash],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, f64>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, Option<String>>(6)?,
                            r.get::<_, Option<String>>(7)?,
                            r.get::<_, Option<f64>>(8)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((id, file, category, duration_s, source, license, prompt, provider, lufs)) =
                row
            {
                let tags = self.tags(&id)?;
                return Ok(Some(Asset {
                    id,
                    file,
                    hash: hash.to_string(),
                    category,
                    tags,
                    duration_s,
                    source: parse_source(&source),
                    license,
                    prompt,
                    provider,
                    lufs,
                }));
            }
            return Ok(None);
        }
        Ok(local.iter().find(|a| a.hash == hash).cloned())
    }

    fn tags(&self, id: &str) -> Result<Vec<String>> {
        let Some(show) = &self.show else {
            return Ok(Vec::new());
        };
        let mut stmt = show
            .conn()
            .prepare("SELECT tag FROM asset_tag WHERE asset_id=?1 ORDER BY tag")?;
        let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Copies a file into `Library/<category>/` (or finds the identical file already there).
    /// `local` are the episode's own assets (standalone libraries).
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
        local: &[Asset],
    ) -> Result<LibraryAsset> {
        let hash = hash_file(src)?;
        if let Some(a) = self.find_by_hash(&hash, local)? {
            return Ok(LibraryAsset {
                asset: a,
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
        let asset = Asset {
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
        };
        if let Some(show) = &self.show {
            show.conn().execute(
                "INSERT INTO asset(id,file,hash,category,duration_s,source,license,prompt,provider,created_at,lufs) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    asset.id, asset.file, asset.hash, asset.category, asset.duration_s, source_str(asset.source),
                    asset.license, asset.prompt, asset.provider, chrono::Utc::now().to_rfc3339(), asset.lufs
                ],
            )?;
            for t in tags {
                show.conn().execute(
                    "INSERT OR IGNORE INTO asset_tag(asset_id,tag) VALUES(?1,?2)",
                    params![asset.id, t],
                )?;
            }
            show.log(
                "library",
                "import_asset",
                &serde_json::json!({"id": asset.id, "file": asset.file}),
            )?;
        }
        Ok(LibraryAsset {
            asset,
            deduplicated: false,
        })
    }

    /// All assets (show library), or the given local ones.
    pub fn list(&self, local: &[Asset]) -> Result<Vec<Asset>> {
        let Some(show) = &self.show else {
            return Ok(local.to_vec());
        };
        let mut stmt = show
            .conn()
            .prepare("SELECT hash FROM asset ORDER BY category, file")?;
        let hashes: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut out = Vec::new();
        for h in hashes {
            if let Some(a) = self.find_by_hash(&h, local)? {
                out.push(a);
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

    pub fn tag(&self, asset_id: &str, tags: &[String]) -> Result<()> {
        if let Some(show) = &self.show {
            for t in tags {
                show.conn().execute(
                    "INSERT OR IGNORE INTO asset_tag(asset_id,tag) VALUES(?1,?2)",
                    params![asset_id, t],
                )?;
            }
            show.log(
                "library",
                "tag_asset",
                &serde_json::json!({"id": asset_id, "tags": tags}),
            )?;
        }
        Ok(())
    }

    /// Removes an asset from the library; its file moves to `Backups/Trash/` next to the library.
    pub fn delete(&self, a: &Asset, trash: &Path) -> Result<()> {
        let p = self.path_of(a);
        if p.exists() {
            std::fs::create_dir_all(trash)?;
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| a.id.clone());
            std::fs::rename(&p, trash.join(format!("{} {}", a.id, name)))?;
        }
        if let Some(show) = &self.show {
            show.conn()
                .execute("DELETE FROM asset WHERE id=?1", [&a.id])?;
            show.log("library", "delete_asset", &serde_json::json!({"id": a.id}))?;
        }
        Ok(())
    }
}

fn source_str(s: AssetSource) -> &'static str {
    match s {
        AssetSource::Imported => "imported",
        AssetSource::Licensed => "licensed",
        AssetSource::Generated => "generated",
    }
}

fn parse_source(s: &str) -> AssetSource {
    match s {
        "licensed" => AssetSource::Licensed,
        "generated" => AssetSource::Generated,
        _ => AssetSource::Imported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_library_dedups_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        let (show, store) = apc_core::project::ShowFolder::create(dir.path(), "TARMAC").unwrap();
        let lib = Library::new(show.root.join("Library"), Some(store));
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
                &[],
            )
            .unwrap();
        assert!(!a.deduplicated);
        assert!(show.root.join("Library/SFX/Doors/door_slam.wav").exists());
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
                &[],
            )
            .unwrap();
        assert!(b.deduplicated);
        assert_eq!(b.asset.id, a.asset.id);
        assert_eq!(lib.list(&[]).unwrap().len(), 1);
        assert_eq!(lib.search("door", &[]).unwrap().len(), 1);
        assert!(lib.search("alarm", &[]).unwrap().is_empty());
        lib.delete(&a.asset, &show.root.join("Backups/Trash"))
            .unwrap();
        assert!(lib.list(&[]).unwrap().is_empty());
        assert_eq!(
            std::fs::read_dir(show.root.join("Backups/Trash"))
                .unwrap()
                .count(),
            1
        );
    }
}
