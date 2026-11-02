//! A show's sound library in studio mode (D65): the files under the show's `Sound/sfx` and
//! `Sound/music` folders are the library. Nothing is registered by hand: a scan finds them,
//! a local index remembers each file's hash and length (by path, size and modification
//! time), and `Sound/Cutaway/library.json` keeps what a file can't tell (licence, provider
//! id, prompt), shared through the pipeline. The first-level folders are the buckets.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use apc_core::media::{hash_file, AUDIO_EXTS};
use apc_core::project::SHOW_PREFIX;
use apc_core::show::{ignored_name, is_conflict_copy, write_atomic, Show};
use apc_core::{Asset, AssetSource};

use crate::error::{Result, SoundError};

/// What a file can't say about itself; kept in `library.json` by content hash.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SoundMeta {
    pub source: Option<AssetSource>,
    pub license: Option<String>,
    pub provider: Option<String>,
    pub remote_id: Option<String>,
    pub prompt: Option<String>,
    pub tags: Vec<String>,
}

impl SoundMeta {
    fn is_empty(&self) -> bool {
        *self == SoundMeta::default()
    }

    /// Fills what `self` lacks from `other` (a conflict copy, a second machine).
    fn merge(&mut self, other: &SoundMeta) {
        self.source = self.source.or(other.source);
        self.license = self.license.clone().or_else(|| other.license.clone());
        self.provider = self.provider.clone().or_else(|| other.provider.clone());
        self.remote_id = self.remote_id.clone().or_else(|| other.remote_id.clone());
        self.prompt = self.prompt.clone().or_else(|| other.prompt.clone());
        for t in &other.tags {
            if !self.tags.contains(t) {
                self.tags.push(t.clone());
            }
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MetaFile {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    sounds: BTreeMap<String, SoundMeta>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Indexed {
    size: u64,
    mtime_ms: i64,
    hash: String,
    duration_s: f64,
}

#[derive(Serialize, Deserialize, Default)]
struct IndexFile {
    #[serde(default)]
    files: BTreeMap<String, Indexed>,
}

/// One file of the library.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Path under `Sound/` with forward slashes (`sfx/doors/door.wav`).
    pub rel: String,
    pub asset: Asset,
}

/// A first-level folder of `sfx/` or `music/`: a bucket.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct BucketFolder {
    /// `sfx/doors`.
    pub rel: String,
    /// The folder's own name (`doors`).
    pub name: String,
    pub music: bool,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct ScanReport {
    pub files: usize,
    /// Files hashed in this scan (new or changed since the last one).
    pub hashed: usize,
    pub removed: usize,
    pub folders: usize,
}

/// The id of a show sound: the same file has the same id on every machine and episode.
pub fn asset_id(hash: &str) -> String {
    format!("as_{}", &hash[..hash.len().min(12)])
}

/// A bucket name for a folder (`room-tone` → `Room tone`).
pub fn bucket_name(folder: &str) -> String {
    let words = folder.replace(['-', '_'], " ");
    let mut c = words.trim().chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => folder.to_string(),
    }
}

/// A folder name for a bucket (`Doors and hatches` → `doors-and-hatches`), in the
/// pipeline's style.
pub fn folder_slug(name: &str) -> String {
    let mut out = String::new();
    for w in name
        .to_lowercase()
        .replace(['\'', '’'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(w);
    }
    if out.is_empty() {
        "sounds".into()
    } else {
        out
    }
}

fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn mtime_ms(m: &std::fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| p.to_string_lossy().into_owned())
}

/// Where a show keeps its machine-local files (`CUTAWAY_DATA_DIR` overrides; tests use it).
pub fn local_dir(show: &Show) -> PathBuf {
    let base = std::env::var_os(format!("{}_DATA_DIR", apc_core::brand::SLUG.to_uppercase()))
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join(apc_core::brand::SLUG)))
        .unwrap_or_else(|| std::env::temp_dir().join(apc_core::brand::SLUG));
    base.join("shows").join(&show.data.id)
}

/// A track's stems folder (`ES_Home Straight - Jon Bjork/` next to the `.wav` or `.zip` of
/// the same name, or holding `… STEMS …` files) is part of the track, not a bucket.
fn is_track_folder(dir: &Path) -> bool {
    let Some(name) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return false;
    };
    let sibling = dir
        .parent()
        .and_then(|p| std::fs::read_dir(p).ok())
        .map(|rd| {
            rd.filter_map(|e| e.ok()).any(|e| {
                e.path().is_file()
                    && e.path()
                        .file_stem()
                        .map(|s| s.to_string_lossy() == name.as_str())
                        .unwrap_or(false)
            })
        });
    if sibling == Some(true) {
        return true;
    }
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| e.file_name().to_string_lossy().contains(" STEMS "))
        })
        .unwrap_or(false)
}

pub struct ShowLibrary {
    /// The show's `Sound/` folder.
    pub root: PathBuf,
    sfx: String,
    music: String,
    meta_file: PathBuf,
    index_file: PathBuf,
    trash: PathBuf,
    entries: Vec<Entry>,
    index: IndexFile,
    meta: MetaFile,
}

impl ShowLibrary {
    /// Opens the show's library and brings the index up to date.
    pub fn open(show: &Show) -> Result<(ShowLibrary, ScanReport)> {
        Self::open_with(show, &local_dir(show))
    }

    /// Opens with the machine-local files in `local`.
    pub fn open_with(show: &Show, local: &Path) -> Result<(ShowLibrary, ScanReport)> {
        let mut lib = ShowLibrary {
            root: show.sound_dir(),
            sfx: show.data.layout.sfx.clone(),
            music: show.data.layout.music.clone(),
            meta_file: show.cutaway_dir().join("library.json"),
            index_file: local.join("library-index.json"),
            trash: show.cutaway_dir().join("Trash"),
            entries: Vec::new(),
            index: std::fs::read_to_string(local.join("library-index.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            meta: MetaFile::default(),
        };
        let report = lib.scan()?;
        Ok((lib, report))
    }

    fn top_dirs(&self) -> [(String, bool); 2] {
        [(self.sfx.clone(), false), (self.music.clone(), true)]
    }

    /// Reads the folders again: new and changed files are hashed, gone ones dropped.
    pub fn scan(&mut self) -> Result<ScanReport> {
        self.meta = self.read_meta();
        let mut files: Vec<PathBuf> = Vec::new();
        for (top, _) in self.top_dirs() {
            walk(&self.root.join(&top), &mut files);
        }
        let mut report = ScanReport::default();
        let mut index = IndexFile::default();
        let mut entries = Vec::new();
        for p in files {
            let rel = rel_of(&self.root, &p);
            let Ok(m) = std::fs::metadata(&p) else {
                continue;
            };
            let (size, mt) = (m.len(), mtime_ms(&m));
            let known = self
                .index
                .files
                .get(&rel)
                .filter(|i| i.size == size && i.mtime_ms == mt)
                .cloned();
            let ix = match known {
                Some(i) => i,
                None => {
                    let Ok(hash) = hash_file(&p) else {
                        continue; // still being written (a sync in progress); the next scan
                    };
                    report.hashed += 1;
                    let duration_s = apc_media::decode::probe(&p)
                        .map(|i| i.duration_s)
                        .unwrap_or(0.0);
                    Indexed {
                        size,
                        mtime_ms: mt,
                        hash,
                        duration_s,
                    }
                }
            };
            entries.push(self.entry(&rel, &ix));
            index.files.insert(rel, ix);
        }
        report.removed = self
            .index
            .files
            .keys()
            .filter(|k| !index.files.contains_key(*k))
            .count();
        report.files = entries.len();
        report.folders = self.folders().len();
        self.entries = entries;
        self.index = index;
        self.save_index();
        Ok(report)
    }

    /// The index is a cache: failing to write it only costs a re-hash next time.
    fn save_index(&self) {
        if let Some(d) = self.index_file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string(&self.index) {
            let _ = std::fs::write(&self.index_file, s);
        }
    }

    /// `library.json` merged with any sync-conflict copies of it.
    fn read_meta(&self) -> MetaFile {
        let read = |p: &Path| -> Option<MetaFile> {
            serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
        };
        let mut main = read(&self.meta_file).unwrap_or_default();
        let dir = self.meta_file.parent().map(Path::to_path_buf);
        if let Some(rd) = dir.and_then(|d| std::fs::read_dir(d).ok()) {
            for e in rd.filter_map(|e| e.ok()) {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with("library") && n.ends_with(".json") && is_conflict_copy(&n) {
                    if let Some(other) = read(&e.path()) {
                        for (h, m) in other.sounds {
                            main.sounds.entry(h).or_default().merge(&m);
                        }
                    }
                }
            }
        }
        main
    }

    /// Records what a file can't tell about itself (read again first: another machine may
    /// have written meanwhile).
    pub fn record(&mut self, hash: &str, meta: SoundMeta) -> Result<()> {
        if meta.is_empty() {
            return Ok(());
        }
        let mut file = self.read_meta();
        let mut m = meta;
        if let Some(old) = file.sounds.get(hash) {
            m.merge(old);
        }
        file.sounds.insert(hash.to_string(), m);
        file.schema_version = 1;
        file.kind = "cutaway-library".into();
        if let Some(d) = self.meta_file.parent() {
            std::fs::create_dir_all(d)?;
        }
        let json =
            serde_json::to_string_pretty(&file).map_err(|e| SoundError::Invalid(e.to_string()))?;
        write_atomic(&self.meta_file, json.as_bytes())?;
        self.meta = file;
        let rels: Vec<(String, Indexed)> = self
            .index
            .files
            .iter()
            .filter(|(_, i)| i.hash == hash)
            .map(|(r, i)| (r.clone(), i.clone()))
            .collect();
        for (rel, ix) in rels {
            let e = self.entry(&rel, &ix);
            if let Some(x) = self.entries.iter_mut().find(|x| x.rel == rel) {
                *x = e;
            }
        }
        Ok(())
    }

    fn entry(&self, rel: &str, ix: &Indexed) -> Entry {
        let parts: Vec<&str> = rel.split('/').collect();
        let top = parts.first().copied().unwrap_or("");
        let folder = (parts.len() > 2).then(|| parts[1]);
        let file_name = parts.last().copied().unwrap_or(rel);
        let (named, mut tags) = crate::tagging::tag_from_name(file_name);
        let ambient = |s: &str| {
            let s = s.to_lowercase();
            [
                "ambience",
                "ambient",
                "room-tone",
                "roomtone",
                "room tone",
                "atmo",
            ]
            .iter()
            .any(|k| s.contains(k))
        };
        let category = if top == self.music {
            match folder {
                Some(f) => format!("Music/{f}"),
                None => "Music".into(),
            }
        } else {
            match folder {
                Some(f) if ambient(f) => format!("Ambience/{f}"),
                Some(f) => format!("SFX/{f}"),
                None if named.as_deref().map(ambient).unwrap_or(false) => "Ambience".into(),
                None => "SFX".into(),
            }
        };
        if let Some(f) = folder {
            for w in f.split(['-', '_', ' ']).filter(|w| w.len() > 1) {
                let w = w.to_lowercase();
                if !tags.contains(&w) {
                    tags.insert(0, w);
                }
            }
        }
        let meta = self.meta.sounds.get(&ix.hash).cloned().unwrap_or_default();
        for t in &meta.tags {
            if !tags.contains(t) {
                tags.push(t.clone());
            }
        }
        Entry {
            rel: rel.to_string(),
            asset: Asset {
                id: asset_id(&ix.hash),
                file: format!("{SHOW_PREFIX}{rel}"),
                hash: ix.hash.clone(),
                category,
                tags,
                duration_s: ix.duration_s,
                source: meta.source.unwrap_or(AssetSource::Imported),
                license: meta.license,
                prompt: meta.prompt,
                provider: meta.provider,
                lufs: None,
                remote_id: meta.remote_id,
            },
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Every sound once (the same file in two folders is one sound).
    pub fn assets(&self) -> Vec<Asset> {
        let mut seen = std::collections::BTreeSet::new();
        self.entries
            .iter()
            .filter(|e| seen.insert(e.asset.id.clone()))
            .map(|e| e.asset.clone())
            .collect()
    }

    pub fn by_hash(&self, hash: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.asset.hash == hash)
    }

    pub fn by_id(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.asset.id == id)
    }

    pub fn by_remote(&self, remote_id: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.asset.remote_id.as_deref() == Some(remote_id))
    }

    /// The sounds in a folder (`sfx/doors`), its subfolders included.
    pub fn in_folder(&self, folder: &str) -> Vec<Asset> {
        let prefix = format!("{}/", folder.trim_end_matches('/'));
        let mut seen = std::collections::BTreeSet::new();
        self.entries
            .iter()
            .filter(|e| e.rel.starts_with(&prefix))
            .filter(|e| seen.insert(e.asset.id.clone()))
            .map(|e| e.asset.clone())
            .collect()
    }

    /// The bucket folders: first-level folders of `sfx/` and `music/`, without track stems.
    pub fn folders(&self) -> Vec<BucketFolder> {
        let mut out = Vec::new();
        for (top, music) in self.top_dirs() {
            let Ok(rd) = std::fs::read_dir(self.root.join(&top)) else {
                continue;
            };
            let mut dirs: Vec<PathBuf> = rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            for d in dirs {
                let name = d
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ignored_name(&name) || is_track_folder(&d) {
                    continue;
                }
                out.push(BucketFolder {
                    rel: format!("{top}/{name}"),
                    name,
                    music,
                });
            }
        }
        out
    }

    /// The folder (`sfx/<slug>` or `music/<slug>`) a new bucket gets.
    pub fn folder_for(&self, name: &str, music: bool) -> String {
        let top = if music { &self.music } else { &self.sfx };
        let slug = folder_slug(name);
        // a bucket named after the top folder ("Music") is that folder
        if slug == folder_slug(top) {
            top.clone()
        } else {
            format!("{top}/{slug}")
        }
    }

    /// Loose files of a kind go straight into `sfx/` or `music/`.
    pub fn loose_folder(&self, music: bool) -> String {
        if music {
            self.music.clone()
        } else {
            self.sfx.clone()
        }
    }

    pub fn path_of(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Copies a file into a folder of the library (`sfx/doors`). The same file already in
    /// that folder is reused; elsewhere in the library it is still copied, so the folder
    /// (the bucket) holds it for every episode. Returns the sound and whether it existed.
    pub fn import(&mut self, src: &Path, folder: &str, meta: SoundMeta) -> Result<(Asset, bool)> {
        let hash = hash_file(src)?;
        let folder = folder.trim_matches('/');
        let in_folder = self.entries.iter().find(|e| {
            e.asset.hash == hash && e.rel.rsplit_once('/').map(|(d, _)| d) == Some(folder)
        });
        if let Some(e) = in_folder.cloned() {
            self.record(&hash, meta)?;
            let asset = self
                .by_id(&e.asset.id)
                .map(|x| x.asset.clone())
                .unwrap_or(e.asset);
            return Ok((asset, true));
        }
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| SoundError::Invalid("file has no name".into()))?;
        let dir = self.root.join(folder);
        std::fs::create_dir_all(&dir)?;
        let mut dest = dir.join(&name);
        let mut k = 2;
        while dest.exists() {
            let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
            dest = dir.join(format!("{stem} ({k}).{ext}"));
            k += 1;
        }
        std::fs::copy(src, &dest)?;
        let m = std::fs::metadata(&dest)?;
        let duration_s = apc_media::decode::probe(&dest)
            .map(|i| i.duration_s)
            .unwrap_or(0.0);
        let rel = rel_of(&self.root, &dest);
        let ix = Indexed {
            size: m.len(),
            mtime_ms: mtime_ms(&m),
            hash: hash.clone(),
            duration_s,
        };
        self.index.files.insert(rel.clone(), ix.clone());
        self.entries.push(self.entry(&rel, &ix));
        self.save_index();
        self.record(&hash, meta)?;
        let asset = self
            .entries
            .iter()
            .find(|e| e.rel == rel)
            .map(|e| e.asset.clone())
            .ok_or_else(|| SoundError::Invalid("imported file not indexed".into()))?;
        Ok((asset, false))
    }

    /// Creates a bucket's folder (nothing happens if it exists).
    pub fn make_folder(&self, folder: &str) -> Result<()> {
        std::fs::create_dir_all(self.root.join(folder.trim_matches('/')))?;
        Ok(())
    }

    /// Moves every copy of a sound to `Sound/Cutaway/Trash/`.
    pub fn delete(&mut self, id: &str) -> Result<()> {
        let rels: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.asset.id == id)
            .map(|e| e.rel.clone())
            .collect();
        for rel in &rels {
            let p = self.root.join(rel);
            if p.exists() {
                std::fs::create_dir_all(&self.trash)?;
                let name = rel.replace('/', " ");
                std::fs::rename(&p, self.trash.join(format!("{id} {name}")))?;
            }
            self.index.files.remove(rel);
        }
        self.entries.retain(|e| e.asset.id != id);
        self.save_index();
        Ok(())
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    v.sort();
    for p in v {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if ignored_name(&name) {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else if is_audio(&p) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(p: &Path, n: usize) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let samples: Vec<f32> = (0..n).map(|i| ((i as f32) * 0.01).sin() * 0.2).collect();
        apc_media::wav::write_wav(p, 48_000, 1, &samples).unwrap();
    }

    fn show(dir: &Path) -> Show {
        let root = dir.join("tm");
        let s = root.join("Sound");
        wav(
            &s.join("sfx/doors/ES_Doors, Wood, Slam - Epidemic Sound.wav"),
            4800,
        );
        wav(&s.join("sfx/room-tone/office.wav"), 9600);
        wav(
            &s.join("sfx/ES_Ambience, Lakeside, Night - Epidemic Sound.wav"),
            2400,
        );
        wav(&s.join("music/ES_Home Straight - Jon Bjork.wav"), 1200);
        wav(
            &s.join(
                "music/ES_Home Straight - Jon Bjork/ES_Home Straight STEMS DRUMS - Jon Bjork.wav",
            ),
            1300,
        );
        wav(&s.join("music/Stings/horror sting.wav"), 1400);
        std::fs::write(s.join("music/ES_Home Straight - Jon Bjork.zip"), b"zip").unwrap();
        std::fs::write(s.join("sfx/doors/.syncthing.creak.wav.tmp"), b"half").unwrap();
        wav(
            &s.join("sfx/doors/creak.sync-conflict-20261012-113350-IO3PGJU.wav"),
            100,
        );
        Show::create(&root, None).unwrap()
    }

    #[test]
    fn folders_are_buckets_and_files_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let show = show(dir.path());
        let (mut lib, r) = ShowLibrary::open_with(&show, &dir.path().join("local")).unwrap();
        assert_eq!(r.files, 6, "{:?}", lib.entries());
        assert_eq!(r.hashed, 6);
        let folders: Vec<String> = lib.folders().into_iter().map(|f| f.rel).collect();
        assert_eq!(folders, vec!["sfx/doors", "sfx/room-tone", "music/Stings"]);
        let door = &lib.in_folder("sfx/doors")[0];
        assert_eq!(
            door.file,
            "show:sfx/doors/ES_Doors, Wood, Slam - Epidemic Sound.wav"
        );
        assert_eq!(door.category, "SFX/doors");
        assert!(door.tags.contains(&"doors".to_string()));
        assert!((door.duration_s - 0.1).abs() < 1e-6);
        assert_eq!(
            lib.in_folder("sfx/room-tone")[0].category,
            "Ambience/room-tone"
        );
        let lake = lib
            .entries()
            .iter()
            .find(|e| e.rel.contains("Lakeside"))
            .unwrap();
        assert_eq!(lake.asset.category, "Ambience");
        assert_eq!(lib.in_folder("music/Stings")[0].category, "Music/Stings");

        // a second scan reads nothing again
        let again = lib.scan().unwrap();
        assert_eq!((again.files, again.hashed), (6, 0));
        // nor does a new session on this machine
        let (_, cold) = ShowLibrary::open_with(&show, &dir.path().join("local")).unwrap();
        assert_eq!(cold.hashed, 0);
    }

    #[test]
    fn imports_land_in_bucket_folders_with_their_licence() {
        let dir = tempfile::tempdir().unwrap();
        let show = show(dir.path());
        let (mut lib, _) = ShowLibrary::open_with(&show, &dir.path().join("local")).unwrap();
        let src = dir
            .path()
            .join("download/ES_Hatch, Metal - Epidemic Sound.wav");
        wav(&src, 3000);
        let meta = SoundMeta {
            source: Some(AssetSource::Licensed),
            license: Some("Epidemic Sound, sound effect 42".into()),
            provider: Some("epidemic".into()),
            remote_id: Some("42".into()),
            ..Default::default()
        };
        assert_eq!(lib.folder_for("Music", true), "music");
        let folder = lib.folder_for("Doors and hatches", false);
        assert_eq!(folder, "sfx/doors-and-hatches");
        let (a, existed) = lib.import(&src, &folder, meta.clone()).unwrap();
        assert!(!existed);
        assert!(show
            .sound_dir()
            .join("sfx/doors-and-hatches/ES_Hatch, Metal - Epidemic Sound.wav")
            .exists());
        assert_eq!(a.source, AssetSource::Licensed);
        assert_eq!(lib.by_remote("42").unwrap().asset.id, a.id);
        // the same file into the same folder is not copied twice
        let (b, existed) = lib.import(&src, &folder, SoundMeta::default()).unwrap();
        assert!(existed);
        assert_eq!(b.id, a.id);
        // another machine (a fresh index) knows the licence from library.json
        let (other, r) = ShowLibrary::open_with(&show, &dir.path().join("other-machine")).unwrap();
        assert_eq!(r.hashed, 7);
        let e = other.by_remote("42").unwrap();
        assert_eq!(
            e.asset.license.as_deref(),
            Some("Epidemic Sound, sound effect 42")
        );
        assert_eq!(e.asset.id, a.id, "same id on every machine");
    }

    #[test]
    fn a_conflict_copy_of_the_metadata_is_merged() {
        let dir = tempfile::tempdir().unwrap();
        let show = show(dir.path());
        let (mut lib, _) = ShowLibrary::open_with(&show, &dir.path().join("local")).unwrap();
        let h = lib.in_folder("sfx/doors")[0].hash.clone();
        lib.record(
            &h,
            SoundMeta {
                license: Some("A".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let other = MetaFile {
            schema_version: 1,
            kind: "cutaway-library".into(),
            sounds: [(
                h.clone(),
                SoundMeta {
                    prompt: Some("slam".into()),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        };
        std::fs::write(
            show.cutaway_dir()
                .join("library.sync-conflict-20261102-101010-ABC.json"),
            serde_json::to_string(&other).unwrap(),
        )
        .unwrap();
        lib.scan().unwrap();
        let a = &lib.in_folder("sfx/doors")[0];
        assert_eq!(a.license.as_deref(), Some("A"));
        assert_eq!(a.prompt.as_deref(), Some("slam"));
    }

    #[test]
    fn names_and_slugs() {
        assert_eq!(bucket_name("room-tone"), "Room tone");
        assert_eq!(folder_slug("Doors and hatches"), "doors-and-hatches");
        assert_eq!(folder_slug("Kitchenette ambience"), "kitchenette-ambience");
        assert_eq!(folder_slug("!!"), "sounds");
        assert_eq!(
            folder_slug("Colonel’s Office ambience"),
            "colonels-office-ambience"
        );
    }
}
