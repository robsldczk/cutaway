//! Shows in a production pipeline (studio mode, D64). A show is an existing folder (a
//! Syncthing share, say) that Cutaway joins with one JSON file at its root, next to the
//! pipeline's own files:
//!
//! ```text
//! tm-2610/  tm-2610.cutawayshow  Episodes/<Episode>/Sound/  Sound/{sfx,music}/<bucket>/
//! ```
//!
//! The layout (folder names) lives in the show file, so nothing here knows tm-2610.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::brand;
use crate::error::{CoreError, Result};

pub const SHOW_KIND: &str = "cutaway-show";
pub const SHOW_SCHEMA_VERSION: u32 = 1;

/// Folder names of a show. The defaults are the tm-2610 pipeline's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ShowLayout {
    /// The folder holding one folder per episode.
    pub episodes: String,
    /// Cutaway's folder inside an episode folder ("" for the episode folder itself).
    pub episode_folder: String,
    /// The show's sound library.
    pub sound: String,
    /// Sound effects and music inside the library; their subfolders are buckets.
    pub sfx: String,
    pub music: String,
}

impl Default for ShowLayout {
    fn default() -> Self {
        ShowLayout {
            episodes: "Episodes".into(),
            episode_folder: "Sound".into(),
            sound: "Sound".into(),
            sfx: "sfx".into(),
            music: "music".into(),
        }
    }
}

/// The show's look: a backdrop image (set once) and colours taken from it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ShowTheme {
    /// Relative to the show root.
    pub backdrop: Option<String>,
    /// `#rrggbb`, most characteristic first.
    pub colors: Vec<String>,
}

/// The show file (`<root>/<name>.cutawayshow`, JSON).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFile {
    pub schema_version: u32,
    pub kind: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub layout: ShowLayout,
    #[serde(default)]
    pub theme: ShowTheme,
    /// File-name aliases of characters, shared by every episode (alias → character id).
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// Where deliverables go, confirmed by the editor: relative to the root when inside the
    /// show, so it holds on the other machines of a sync share. Unset means never exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_folder: Option<String>,
}

/// An open show: its root folder, file and contents.
#[derive(Clone, Debug, PartialEq)]
pub struct Show {
    pub root: PathBuf,
    pub file: PathBuf,
    pub data: ShowFile,
}

/// An episode folder of a show.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ShowEpisode {
    /// Folder name under the episodes folder ("Episode 2"); the key to open it.
    pub folder: String,
    pub number: Option<u32>,
    /// Title from the pipeline (`episode.jetplane.json`), if any.
    pub title: Option<String>,
    /// "Episode 2 · Turkey Time".
    pub label: String,
    /// Cutaway's folder of the episode.
    pub dir: PathBuf,
    /// The episode file, when the episode has a Cutaway project.
    pub file: Option<PathBuf>,
}

/// Names a scan skips: hidden files, Syncthing temp and conflict copies, other sync tools'
/// conflict copies.
pub fn ignored_name(name: &str) -> bool {
    name.starts_with('.')
        || name.contains(".sync-conflict-")
        || name.contains("(conflicted copy")
        || name.contains("(Conflict ")
        || name.ends_with(".tmp")
        || name == "Thumbs.db"
        || name == "desktop.ini"
}

/// What a sound folder of a production pipeline is called.
const SOUND_FOLDERS: &[&str] = &["sound", "sounds", "audio", "ton"];
/// Folders looked into at most when guessing the pipeline folder: it runs on a click, and the
/// production folders above a show can be large.
const PIPELINE_SEARCH_DIRS: usize = 4000;

/// A folder that holds a production: a show or pipeline file, or an episodes folder.
fn looks_like_pipeline(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.filter_map(|e| e.ok()).any(|e| {
        let name = e.file_name().to_string_lossy().to_lowercase();
        name.ends_with(&format!(".{}", brand::SHOW_EXT))
            || name.ends_with(".jetplane.json")
            || name == "episodes"
    })
}

/// Is this a Syncthing (or similar) conflict copy?
pub fn is_conflict_copy(name: &str) -> bool {
    name.contains(".sync-conflict-")
        || name.contains("(conflicted copy")
        || name.contains("(Conflict ")
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Writes a file through a temporary sibling and a rename, so a sync tool or a crash never
/// sees half of it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| CoreError::invalid("file has no folder"))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}-tmp", brand::SLUG));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn show_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(brand::SHOW_EXT))
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| !ignored_name(n))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

impl Show {
    /// Makes an existing folder a show: writes the show file and the folders the layout
    /// needs that aren't there yet. Nothing that exists is touched.
    pub fn create(root: &Path, name: Option<&str>) -> Result<Show> {
        std::fs::create_dir_all(root)?;
        if let Some(f) = show_files_in(root).first() {
            return Err(CoreError::invalid(format!(
                "{} is already a show ({})",
                root.display(),
                f.display()
            )));
        }
        let folder_name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Show".into());
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(String::from)
            .unwrap_or(folder_name);
        let file = root.join(format!("{}.{}", safe_name(&name), brand::SHOW_EXT));
        let show = Show {
            root: root.to_path_buf(),
            file,
            data: ShowFile {
                schema_version: SHOW_SCHEMA_VERSION,
                kind: SHOW_KIND.into(),
                id: uuid::Uuid::new_v4().to_string(),
                name,
                layout: ShowLayout::default(),
                theme: ShowTheme::default(),
                aliases: BTreeMap::new(),
                pipeline_folder: None,
            },
        };
        std::fs::create_dir_all(show.episodes_dir())?;
        std::fs::create_dir_all(show.sfx_dir())?;
        std::fs::create_dir_all(show.music_dir())?;
        show.save()?;
        Ok(show)
    }

    /// Opens a show from its root folder or its show file.
    pub fn open(path: &Path) -> Result<Show> {
        let file = if path.is_file() {
            path.to_path_buf()
        } else {
            show_files_in(path).into_iter().next().ok_or_else(|| {
                CoreError::not_found(
                    "show file",
                    format!("*.{} in {}", brand::SHOW_EXT, path.display()),
                )
            })?
        };
        let bytes = std::fs::read(&file)?;
        if bytes.starts_with(b"SQLite format 3") {
            return Err(CoreError::invalid(format!(
                "{} is a show file of an older {} (a database); make the folder a show again",
                file.display(),
                brand::DISPLAY_NAME
            )));
        }
        let data: ShowFile = serde_json::from_slice(&bytes)
            .map_err(|e| CoreError::invalid(format!("{}: {e}", file.display())))?;
        if data.kind != SHOW_KIND {
            return Err(CoreError::invalid(format!(
                "{} is not a {} show file",
                file.display(),
                brand::DISPLAY_NAME
            )));
        }
        let root = file
            .parent()
            .ok_or_else(|| CoreError::invalid("show file has no folder"))?
            .to_path_buf();
        Ok(Show { root, file, data })
    }

    /// The show an episode folder belongs to: a show file a few folders up whose layout puts
    /// episodes exactly here.
    pub fn find_for(episode_root: &Path) -> Option<Show> {
        for dir in episode_root.ancestors().skip(1).take(4) {
            if show_files_in(dir).is_empty() {
                continue;
            }
            let show = Show::open(dir).ok()?;
            let inside = show.episode_folder_of(episode_root).is_some();
            return inside.then_some(show);
        }
        None
    }

    /// Writes the show file (after a change to `data`).
    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.data)?;
        write_atomic(&self.file, json.as_bytes())
    }

    /// Re-reads the show file (another machine may have changed it), applies `f` and saves.
    pub fn update(&mut self, f: impl FnOnce(&mut ShowFile)) -> Result<()> {
        if let Ok(fresh) = Show::open(&self.file) {
            self.data = fresh.data;
        }
        f(&mut self.data);
        self.save()
    }

    pub fn name(&self) -> &str {
        &self.data.name
    }

    pub fn episodes_dir(&self) -> PathBuf {
        self.root.join(&self.data.layout.episodes)
    }

    /// The show's sound library (`Sound/`).
    pub fn sound_dir(&self) -> PathBuf {
        self.root.join(&self.data.layout.sound)
    }

    pub fn sfx_dir(&self) -> PathBuf {
        self.sound_dir().join(&self.data.layout.sfx)
    }

    pub fn music_dir(&self) -> PathBuf {
        self.sound_dir().join(&self.data.layout.music)
    }

    /// The confirmed folder for deliverables. Nothing else is ever written to.
    pub fn pipeline_dir(&self) -> Option<PathBuf> {
        let stored = self.data.pipeline_folder.as_ref()?;
        let p = Path::new(stored);
        Some(if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        })
    }

    /// Stores the folder the editor confirmed.
    pub fn set_pipeline_folder(&mut self, folder: &Path) -> Result<()> {
        if !folder.is_dir() {
            return Err(CoreError::invalid(format!(
                "{} isn't a folder",
                folder.display()
            )));
        }
        let stored = folder
            .strip_prefix(&self.root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| folder.to_string_lossy().into_owned());
        self.update(|d| d.pipeline_folder = Some(stored))
    }

    /// The best guess at the pipeline's sound folder, in the show and the production folders above
    /// it. Only a proposal: nothing is written there until the editor confirms it (D114).
    pub fn propose_pipeline_folder(&self) -> Option<PathBuf> {
        let mut found: Vec<(i32, PathBuf)> = Vec::new();
        let mut budget = PIPELINE_SEARCH_DIRS;
        for (above, top) in self.root.ancestors().take(3).enumerate() {
            let depth = if above == 0 { 3 } else { 2 };
            self.sound_folders(top, depth, &mut budget, &mut found, above as i32);
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        found.into_iter().next().map(|(_, p)| p)
    }

    fn sound_folders(
        &self,
        dir: &Path,
        depth: u32,
        budget: &mut usize,
        found: &mut Vec<(i32, PathBuf)>,
        above: i32,
    ) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut subs: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        subs.sort();
        let pipeline = looks_like_pipeline(dir);
        for p in subs {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // an episode's Sound folder is Cutaway's own project, not the pipeline's
            if *budget == 0 || ignored_name(&name) || p == self.episodes_dir() {
                continue;
            }
            *budget -= 1;
            if SOUND_FOLDERS.contains(&name.to_lowercase().as_str()) {
                let score = depth as i32 * 2 + if pipeline { 10 } else { 0 } - above * 3;
                found.push((score, p));
            } else if depth > 1 {
                self.sound_folders(&p, depth - 1, budget, found, above);
            }
        }
    }

    /// Cutaway's own files in the library (backdrop, library metadata, trash).
    pub fn cutaway_dir(&self) -> PathBuf {
        self.sound_dir().join(brand::DISPLAY_NAME)
    }

    /// Cutaway's folder of an episode folder.
    pub fn episode_dir(&self, folder: &str) -> PathBuf {
        let ep = self.episodes_dir().join(safe_name(folder));
        if self.data.layout.episode_folder.is_empty() {
            ep
        } else {
            ep.join(&self.data.layout.episode_folder)
        }
    }

    /// The episode folder name of a Cutaway episode folder inside this show.
    pub fn episode_folder_of(&self, episode_root: &Path) -> Option<String> {
        let rel = episode_root.strip_prefix(self.episodes_dir()).ok()?;
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        match (parts.as_slice(), self.data.layout.episode_folder.as_str()) {
            ([ep], "") => Some(ep.clone()),
            ([ep, sub], want) if sub == want => Some(ep.clone()),
            _ => None,
        }
    }

    /// Episode folders, by number then name, with the pipeline's titles.
    pub fn episodes(&self) -> Vec<ShowEpisode> {
        let mut v: Vec<ShowEpisode> = std::fs::read_dir(self.episodes_dir())
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| {
                        let folder = e.file_name().to_string_lossy().into_owned();
                        (!ignored_name(&folder)).then(|| self.episode_info(&folder))
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| {
            (a.number.unwrap_or(u32::MAX), a.folder.to_lowercase())
                .cmp(&(b.number.unwrap_or(u32::MAX), b.folder.to_lowercase()))
        });
        v
    }

    /// One episode folder (it may not exist yet).
    pub fn episode_info(&self, folder: &str) -> ShowEpisode {
        let ep_folder = self.episodes_dir().join(folder);
        let meta: Option<serde_json::Value> =
            std::fs::read_to_string(ep_folder.join("episode.jetplane.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
        let number = meta
            .as_ref()
            .and_then(|m| m["episodeNumber"].as_u64())
            .map(|n| n as u32)
            .or_else(|| trailing_number(folder));
        let title = meta
            .as_ref()
            .and_then(|m| m["name"].as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case(folder))
            .map(String::from);
        let label = match &title {
            Some(t) => format!("{folder} · {t}"),
            None => folder.to_string(),
        };
        let dir = self.episode_dir(folder);
        let file = crate::project::find_episode_file(&dir).ok();
        ShowEpisode {
            folder: folder.to_string(),
            number,
            title,
            label,
            dir,
            file,
        }
    }
}

fn trailing_number(s: &str) -> Option<u32> {
    let digits: String = s
        .chars()
        .rev()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.chars().rev().collect::<String>().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder shaped like the tm-2610 pipeline.
    pub fn pipeline(dir: &Path) -> PathBuf {
        let root = dir.join("tm-2610");
        for d in [
            "Episodes/Episode 2/Screenplay",
            "Episodes/Episode 2/Sound",
            "Episodes/Episode 0/Scenes",
            "Sound/sfx/doors",
            "Sound/music",
            "Characters/Paul",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(
            root.join("Episodes/Episode 2/episode.jetplane.json"),
            r#"{"kind":"episode","name":"Turkey Time","episodeNumber":2}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("Episodes/Episode 0/episode.jetplane.json"),
            r#"{"kind":"episode","name":"","episodeNumber":0}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("Episodes/.sync-conflict-x")).unwrap();
        std::fs::write(root.join("library.jetplane.json"), "{}").unwrap();
        root
    }

    #[test]
    fn an_existing_pipeline_becomes_a_show() {
        let dir = tempfile::tempdir().unwrap();
        let root = pipeline(dir.path());
        let show = Show::create(&root, None).unwrap();
        assert_eq!(show.file, root.join("tm-2610.cutawayshow"));
        assert_eq!(show.name(), "tm-2610");
        // the pipeline's own files are untouched
        assert_eq!(
            std::fs::read_to_string(root.join("library.jetplane.json")).unwrap(),
            "{}"
        );
        assert!(Show::create(&root, None).is_err(), "only once");
        let again = Show::open(&root).unwrap();
        assert_eq!(again, show);

        let eps = show.episodes();
        let labels: Vec<&str> = eps.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, vec!["Episode 0", "Episode 2 · Turkey Time"]);
        assert_eq!(eps[1].number, Some(2));
        assert_eq!(eps[1].dir, root.join("Episodes/Episode 2/Sound"));
        assert!(eps[1].file.is_none());

        let ep_root = root.join("Episodes/Episode 2/Sound");
        assert_eq!(Show::find_for(&ep_root).unwrap().root, root);
        assert_eq!(
            show.episode_folder_of(&ep_root).as_deref(),
            Some("Episode 2")
        );
        // a folder elsewhere in the pipeline is not an episode
        assert!(Show::find_for(&root.join("Episodes/Episode 2/Screenplay")).is_none());
    }

    #[test]
    fn an_old_database_show_file_is_refused_clearly() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Old.cutawayshow"), b"SQLite format 3\0...").unwrap();
        let e = Show::open(dir.path()).unwrap_err().to_string();
        assert!(e.contains("older"), "{e}");
    }

    #[test]
    fn update_keeps_what_another_machine_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = Show::create(dir.path(), Some("TARMAC")).unwrap();
        let mut b = Show::open(dir.path()).unwrap();
        b.update(|d| {
            d.aliases.insert("Bert".into(), "NORBERT".into());
        })
        .unwrap();
        a.update(|d| d.theme.colors = vec!["#112233".into()])
            .unwrap();
        let c = Show::open(dir.path()).unwrap();
        assert_eq!(
            c.data.aliases.get("Bert").map(String::as_str),
            Some("NORBERT")
        );
        assert_eq!(c.data.theme.colors, vec!["#112233"]);
    }

    #[test]
    fn scans_skip_sync_leftovers() {
        assert!(ignored_name(".DS_Store"));
        assert!(ignored_name(
            "door.sync-conflict-20261012-113350-IO3PGJU.wav"
        ));
        assert!(ignored_name(".syncthing.door.wav.tmp"));
        assert!(!ignored_name("ES_Doors, Wood, Slam - Epidemic Sound.wav"));
        assert_eq!(trailing_number("Episode 12"), Some(12));
        assert_eq!(trailing_number("Pilot"), None);
    }
}
