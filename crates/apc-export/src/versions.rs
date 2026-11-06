//! Export versions: nothing is overwritten. Each scene counts up on its own, every version is a
//! folder with a manifest, and the manifests are the whole history: it survives undo and revert
//! because it isn't in the document, and it can't disagree with what's on disk (D112).
//!
//! ```text
//! Exports/Sc02_Canteen/v003_2026-11-05/  E02_S02_v003_Dialogue.wav … manifest.json
//! Exports/Episode/v002_2026-11-05/       E02_v002.fcpxml … manifest.json
//! ```
//!
//! That's the episode's own `Exports` folder. A pipeline folder holds every episode, so there
//! the same tree sits under `Exports/Ep02/` (D117). The names stay machine readable, with no
//! spaces or punctuation (D132): `_` separates the parts of a name and `-` holds a part together
//! (D135). Versions saved under older names still count, because the manifests are what's read.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use deunicode::deunicode;

use apc_core::config::{ExportSettings, SaveTo};
use apc_core::Scene;

use crate::error::Result;

pub const MANIFEST: &str = "manifest.json";
pub const MANIFEST_SCHEMA: u32 = 1;
/// The folder of whole-episode versions, beside the scenes'.
pub const EPISODE_FOLDER: &str = "Episode";

/// An episode as exports name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpisodeTag(pub u32);

impl EpisodeTag {
    /// `Ep02`
    pub fn folder(self) -> String {
        format!("Ep{:02}", self.0)
    }

    /// `E02`
    pub fn code(self) -> String {
        format!("E{:02}", self.0)
    }

    /// `E02_S02_v003`
    pub fn stem(self, scene: u32, version: u32) -> String {
        format!("{}_S{scene:02}_v{version:03}", self.code())
    }

    /// `E02_v002`
    pub fn episode_stem(self, version: u32) -> String {
        format!("{}_v{version:03}", self.code())
    }

    /// `E02 v002`
    pub fn episode_label(self, version: u32) -> String {
        format!("{} v{version:03}", self.code())
    }
}

/// One part of a name: its words held together by hyphens, so `Old Man` is `Old-Man`. `_` is what
/// separates the parts, so it never appears inside one. An apostrophe belongs to its word.
pub fn token(name: &str) -> String {
    name.replace(['\'', '’'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// A part of a folder's name, which a script can take without quoting: ASCII letters, digits and
/// hyphens, so `Colonel’s Office` is `Colonels-Office`.
pub fn machine_name(name: &str) -> String {
    token(&deunicode(name))
}

/// `Sc03_Colonels-Office`
pub fn scene_folder(scene: &Scene) -> String {
    let name = machine_name(&scene.place);
    if name.is_empty() {
        format!("Sc{:02}", scene.number)
    } else {
        format!("Sc{:02}_{name}", scene.number)
    }
}

/// `v003_2026-11-05`
pub fn version_folder(version: u32, date: &str) -> String {
    format!("v{version:03}_{date}")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ManifestFile {
    pub name: String,
    /// What it is: `Dialogue`, `RoomVoices`, `Mix`, `clip`, `fcpxml`, `srt`, `cutlist`.
    pub track: String,
    /// Audio only.
    #[serde(default)]
    pub duration_s: Option<f64>,
    #[serde(default)]
    pub frames: Option<u64>,
    pub bytes: u64,
    /// `blake3:<hex>`
    pub checksum: String,
}

/// A scene as a whole-episode version holds it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ManifestScene {
    pub scene_id: String,
    pub number: u32,
    /// The scene's own version; 0 in a one-file export, which renders it as it stands and gives
    /// it no version of its own.
    pub version: u32,
}

/// One version of a scene, or of the whole episode (`scene_id` is then `None`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub version: u32,
    /// RFC 3339.
    pub date: String,
    pub app_version: String,
    pub settings: ExportSettings,
    pub destination: SaveTo,
    #[serde(default)]
    pub scene_id: Option<String>,
    #[serde(default)]
    pub scene_number: Option<u32>,
    #[serde(default)]
    pub heading: String,
    /// What the scene was when exported; equal hashes sound and read the same.
    #[serde(default)]
    pub timeline_hash: String,
    /// The scene was locked by this export (or already was).
    pub locked: bool,
    /// Scene length in frames at `settings.sample_rate`.
    #[serde(default)]
    pub frames: u64,
    pub files: Vec<ManifestFile>,
    /// Gain the mix got on top of the stems' level, and what it measures after it.
    #[serde(default)]
    pub mix_gain_db: Option<f64>,
    #[serde(default)]
    pub mix_lufs: Option<f64>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// A whole-episode version: which scene versions it holds.
    #[serde(default)]
    pub scenes: Vec<ManifestScene>,
}

/// A version found on disk.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ExportRecord {
    pub manifest: Manifest,
    pub folder: PathBuf,
}

/// Every version under the folder an episode's scene folders are in, oldest first. Folders still
/// being written start with a dot; an unreadable manifest doesn't hide the rest.
pub fn scan(base: &Path) -> Vec<ExportRecord> {
    let mut out = Vec::new();
    for scene_dir in subfolders(base) {
        for folder in subfolders(&scene_dir) {
            let Ok(bytes) = std::fs::read(folder.join(MANIFEST)) else {
                continue;
            };
            if let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) {
                out.push(ExportRecord { manifest, folder });
            }
        }
    }
    out.sort_by_key(|r| (r.manifest.scene_number, r.manifest.version));
    out
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.'))
        })
        .collect();
    v.sort();
    v
}

/// What the page needs to know about a set of versions.
#[derive(Default, Clone, Debug)]
pub struct History {
    records: Vec<ExportRecord>,
}

impl History {
    /// From every destination: the counters run across both, so a version number means one
    /// thing wherever its files landed.
    pub fn new(mut records: Vec<ExportRecord>) -> Self {
        records.sort_by(|a, b| a.manifest.date.cmp(&b.manifest.date));
        History { records }
    }

    pub fn of_scene(&self, scene_id: &str) -> Vec<&ExportRecord> {
        self.records
            .iter()
            .filter(|r| r.manifest.scene_id.as_deref() == Some(scene_id))
            .collect()
    }

    /// The newest version of a scene.
    pub fn last_of(&self, scene_id: &str) -> Option<&ExportRecord> {
        self.of_scene(scene_id)
            .into_iter()
            .max_by_key(|r| r.manifest.version)
    }

    pub fn next_for(&self, scene_id: &str) -> u32 {
        self.last_of(scene_id)
            .map(|r| r.manifest.version + 1)
            .unwrap_or(1)
    }

    pub fn episodes(&self) -> Vec<&ExportRecord> {
        self.records
            .iter()
            .filter(|r| r.manifest.scene_id.is_none())
            .collect()
    }

    pub fn next_episode(&self) -> u32 {
        self.episodes()
            .iter()
            .map(|r| r.manifest.version + 1)
            .max()
            .unwrap_or(1)
    }

    /// The newest one-file version, when `hash` is what it holds. Only those have a hash.
    pub fn one_file_unchanged_since(&self, hash: &str) -> Option<u32> {
        self.episodes()
            .into_iter()
            .filter(|r| !r.manifest.timeline_hash.is_empty())
            .max_by_key(|r| r.manifest.version)
            .filter(|r| r.manifest.timeline_hash == hash)
            .map(|r| r.manifest.version)
    }

    /// The newest version whose hash is `hash`, when that's also the scene's newest: exporting
    /// again would deliver the same thing.
    pub fn unchanged_since(&self, scene_id: &str, hash: &str) -> Option<u32> {
        self.last_of(scene_id)
            .filter(|r| r.manifest.timeline_hash == hash)
            .map(|r| r.manifest.version)
    }
}

/// `blake3:<hex>` of a file, read in pieces: stems can be large.
pub fn checksum(path: &Path) -> Result<String> {
    let mut h = blake3::Hasher::new();
    let mut f = std::fs::File::open(path)?;
    std::io::copy(&mut f, &mut h)?;
    Ok(format!("blake3:{}", h.finalize().to_hex()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(scene: Option<&str>, version: u32, hash: &str) -> Manifest {
        Manifest {
            schema: MANIFEST_SCHEMA,
            version,
            date: format!("2026-11-0{version}T10:00:00+01:00"),
            app_version: "0.1.0".into(),
            settings: ExportSettings::default(),
            destination: SaveTo::Episode,
            scene_id: scene.map(String::from),
            scene_number: scene.map(|_| 2),
            heading: String::new(),
            timeline_hash: hash.into(),
            locked: false,
            frames: 0,
            files: vec![],
            mix_gain_db: None,
            mix_lufs: None,
            warnings: vec![],
            scenes: vec![],
        }
    }

    fn record(m: Manifest) -> ExportRecord {
        ExportRecord {
            manifest: m,
            folder: PathBuf::new(),
        }
    }

    #[test]
    fn names_follow_the_folder_scheme() {
        let ep = EpisodeTag(2);
        assert_eq!(ep.folder(), "Ep02");
        assert_eq!(ep.stem(2, 3), "E02_S02_v003");
        assert_eq!(ep.episode_stem(12), "E02_v012");
        assert_eq!(version_folder(3, "2026-11-05"), "v003_2026-11-05");
    }

    #[test]
    fn a_scene_folder_separates_with_underscores_and_holds_together_with_hyphens() {
        let scene = |number, place: &str| -> Scene {
            serde_json::from_value(serde_json::json!({
                "id": "sc", "number": number, "heading": "", "place": place
            }))
            .unwrap()
        };
        assert_eq!(
            scene_folder(&scene(3, "Colonel’s Office")),
            "Sc03_Colonels-Office"
        );
        assert_eq!(scene_folder(&scene(2, "Canteen")), "Sc02_Canteen");
        assert_eq!(scene_folder(&scene(7, "  ?  ")), "Sc07");
    }

    #[test]
    fn a_folder_name_needs_no_quoting() {
        for (place, name) in [
            ("Canteen", "Canteen"),
            ("Time Spring Command", "Time-Spring-Command"),
            ("Colonel’s Office", "Colonels-Office"),
            ("Nina's Bedroom", "Ninas-Bedroom"),
            ("Canteen – Later", "Canteen-Later"),
            ("Canteen · Later (Night)", "Canteen-Later-Night"),
            ("Küche/Flur", "Kuche-Flur"),
            ("ÉMILE'S  CAFÉ", "EMILES-CAFE"),
            ("Top-Floor Lab", "Top-Floor-Lab"),
            ("Snake_Case  Room", "Snake-Case-Room"),
            ("  ?  ", ""),
        ] {
            assert_eq!(machine_name(place), name, "{place}");
        }
    }

    #[test]
    fn versions_under_the_old_names_still_count() {
        let dir = tempfile::tempdir().unwrap();
        for (version, folder) in [
            (1, "Sc03 Colonel’s Office/v001 2026-11-06"),
            (2, "Sc03_Colonels_Office/v002_2026-11-06"),
        ] {
            let old = dir.path().join(folder);
            std::fs::create_dir_all(&old).unwrap();
            std::fs::write(
                old.join(MANIFEST),
                serde_json::to_vec(&manifest(Some("a"), version, "x")).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(History::new(scan(dir.path())).next_for("a"), 3);
    }

    #[test]
    fn counters_run_per_scene_and_across_destinations() {
        let h = History::new(vec![
            record(manifest(Some("a"), 1, "x")),
            record(manifest(Some("a"), 3, "y")),
            record(manifest(Some("b"), 1, "z")),
            record(manifest(None, 4, "")),
        ]);
        assert_eq!(h.next_for("a"), 4);
        assert_eq!(h.next_for("b"), 2);
        assert_eq!(h.next_for("new"), 1);
        assert_eq!(h.next_episode(), 5);
    }

    #[test]
    fn only_the_newest_version_can_be_unchanged() {
        let h = History::new(vec![
            record(manifest(Some("a"), 1, "x")),
            record(manifest(Some("a"), 2, "y")),
        ]);
        assert_eq!(h.unchanged_since("a", "y"), Some(2));
        assert_eq!(h.unchanged_since("a", "x"), None, "changed back isn't v002");
    }

    #[test]
    fn a_half_written_or_broken_version_is_not_history() {
        let dir = tempfile::tempdir().unwrap();
        let scene = dir.path().join("Sc02 Canteen");
        let good = scene.join("v001 2026-11-05");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(
            good.join(MANIFEST),
            serde_json::to_vec(&manifest(Some("a"), 1, "x")).unwrap(),
        )
        .unwrap();
        let hidden = scene.join(".v002 writing");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join(MANIFEST), b"{}").unwrap();
        let broken = scene.join("v003 2026-11-05");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join(MANIFEST), b"not json").unwrap();
        let found = scan(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].manifest.version, 1);
    }
}
