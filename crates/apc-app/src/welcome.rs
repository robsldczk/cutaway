//! The welcome screen (D146–D152): what's on the shelf, read straight from the files. Nothing is
//! opened for writing and nothing is left next to a project by looking at it.
//!
//! The recent list is the UI's (this viewer's); it's passed in, and everything shown about an
//! item comes from the project itself. Covers are `Cover.jpg` or `Cover.png` in the show or
//! episode folder, so they travel with the project.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use apc_core::project::{find_episode_file, EpisodePaths};
use apc_core::review::{fold_notes, unseen, ReviewEvent, ReviewFile};
use apc_core::show::{write_atomic, Show, ShowEpisode};
use apc_core::store::EpisodeStore;
use apc_core::{brand, Document};

use crate::error::{AppError, Result};
use crate::review::{
    episode_id, is_review_file, read_review, reviews_dir, stamp_of, wal_of, AddReviewP, FileStamp,
};
use crate::service::Service;

/// Longest side of a stored cover.
const COVER_PX: u32 = 2400;
/// What a cover is called, in the order they're looked for.
const COVER_NAMES: [&str; 3] = ["Cover.jpg", "Cover.jpeg", "Cover.png"];

/// An item of the UI's recent list.
#[derive(Deserialize, JsonSchema, Clone, Debug)]
pub struct RecentP {
    /// The episode file, or the show's folder.
    pub path: String,
    /// What it was called when it was last opened: all that's left of an item that's gone.
    #[serde(default)]
    pub name: String,
    /// A show, not an episode.
    #[serde(default)]
    pub show: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct WelcomeViewP {
    /// Newest first.
    #[serde(default)]
    pub recents: Vec<RecentP>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CoverImageP {
    /// An image on disk.
    pub path: String,
    /// Longest side wanted, in pixels.
    #[serde(default = "tile_px")]
    pub px: u32,
}

fn tile_px() -> u32 {
    480
}

#[derive(Deserialize, JsonSchema)]
pub struct SetCoverP {
    /// An episode file or folder, or a show's folder or show file.
    pub target: String,
    /// Any JPEG, PNG or WebP.
    pub image: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ShelfReviewP {
    /// A `.cutawayreview` file.
    pub path: String,
    /// Where to look for the episodes it belongs to.
    #[serde(default)]
    pub recents: Vec<RecentP>,
}

/// An image to ask for with `cover_image`. `stamp` changes when the image does.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CoverRef {
    pub path: String,
    pub stamp: String,
}

/// An episode as the shelf shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct EpisodeCard {
    /// Stands for it on the shelf: its file, or its folder until it has a project.
    pub key: String,
    /// The episode file. `None` until the episode has a project, or when it can't be found.
    pub file: Option<String>,
    /// Where its cover goes.
    pub dir: String,
    pub title: String,
    /// The root of its show.
    pub show: Option<String>,
    pub show_name: Option<String>,
    /// Its folder in the show, the key to open it.
    pub folder: Option<String>,
    pub number: Option<u32>,
    /// 0 Import, 1 Options, 2 Arrange, 3 Edit, 4 Exported.
    pub stage: u8,
    /// The last command, as RFC 3339.
    pub edited: Option<String>,
    /// Open notes of the consolidated set, with what's in the review files and not merged yet.
    pub open_notes: usize,
    pub cover: Option<CoverRef>,
    /// Its own image, not its show's.
    pub own_cover: bool,
    /// The file has moved or gone.
    pub missing: bool,
}

/// A show as the shelf shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ShowCard {
    pub key: String,
    pub root: String,
    pub name: String,
    pub colors: Vec<String>,
    pub cover: Option<CoverRef>,
    pub episodes: Vec<EpisodeCard>,
    pub open_notes: usize,
    pub missing: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct WelcomeView {
    /// The recent episodes, newest first.
    pub episodes: Vec<EpisodeCard>,
    /// Shows: the recent ones and the shows of recent episodes, the newest first.
    pub shows: Vec<ShowCard>,
    /// Where new things go unless told otherwise.
    pub folder: String,
}

/// What's read from an episode file.
#[derive(Clone)]
struct Reading {
    id: String,
    name: String,
    stage: u8,
    edited: Option<String>,
    open_notes: usize,
}

/// What's been read, kept until the files change: the shelf is looked at every time the window
/// comes forward, and an episode can take a while to load.
#[derive(Default)]
pub struct Shelf {
    readings: HashMap<PathBuf, (Vec<FileStamp>, Reading)>,
    reviews: HashMap<PathBuf, (FileStamp, Option<ReviewFile>)>,
}

/// Where a new episode or show goes when nobody says: `~/Documents/Cutaway`.
pub fn projects_dir() -> PathBuf {
    std::env::var_os(format!("{}_PROJECTS_DIR", brand::SLUG.to_uppercase()))
        .map(PathBuf::from)
        .or_else(|| dirs::document_dir().map(|d| d.join(brand::DISPLAY_NAME)))
        .unwrap_or_else(|| std::env::temp_dir().join(brand::DISPLAY_NAME))
}

/// How far along an episode is, by what's been done to it. The pages are in working order, so
/// the furthest one with something to show for it (D148).
pub fn stage_of(doc: &Document, exported: bool) -> u8 {
    let boxes = || doc.boxes.values();
    if exported || doc.scenes.values().any(|s| s.lock.is_some()) {
        4
    } else if boxes().any(|b| {
        !b.split_points.is_empty()
            || b.fade_in_s > 0.0
            || b.fade_out_s > 0.0
            || b.gain_db != 0.0
            || b.pinned_s.is_some()
            || b.muted
            || b.dropped
    }) {
        3
    } else if !doc.placements.is_empty()
        || !doc.spacers.is_empty()
        || boxes().any(|b| b.gap_s.is_some() || b.beside.is_some() || b.depth > 0.0)
    {
        2
    } else if !doc.takes.is_empty() {
        1
    } else {
        0
    }
}

/// The episode's open notes: the consolidated set, with what's waiting in the shared folder
/// folded in as a merge would, so feedback shows here before the episode is opened.
fn open_notes(mut doc: Document, files: &[&ReviewFile], ep: &str) -> usize {
    let waiting: Vec<ReviewEvent> = files
        .iter()
        .flat_map(|f| f.events.iter())
        .filter(|e| e.episode == ep)
        .cloned()
        .collect();
    let fresh: Vec<ReviewEvent> = unseen(&doc, &waiting).into_iter().cloned().collect();
    doc.review_events
        .extend(fresh.into_iter().map(|e| (e.id.clone(), e)));
    fold_notes(&doc)
        .iter()
        .filter(|n| n.status.is_open())
        .count()
}

fn cover_ref(path: PathBuf) -> CoverRef {
    let (time, len) = stamp_of(&path);
    let ms = time
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis());
    CoverRef {
        path: path.to_string_lossy().into_owned(),
        stamp: format!("{ms}-{len}"),
    }
}

fn cover_in(dir: &Path) -> Option<PathBuf> {
    COVER_NAMES
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
}

/// A show's cover, or the backdrop it already has (D67).
pub fn show_cover(show: &Show) -> Option<PathBuf> {
    cover_in(&show.root).or_else(|| {
        let rel = show.data.theme.backdrop.as_ref()?;
        Some(show.root.join(rel)).filter(|p| p.is_file())
    })
}

impl Shelf {
    fn review_files(&mut self, dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| is_review_file(p))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        for p in &v {
            let now = stamp_of(p);
            if self.reviews.get(p).is_none_or(|(s, _)| *s != now) {
                // one that can't be read says so when the episode opens
                self.reviews.insert(p.clone(), (now, read_review(p).ok()));
            }
        }
        v
    }

    /// The episode's file as it stands now.
    fn read(&mut self, file: &Path, paths: &EpisodePaths) -> Result<Reading> {
        let files = self.review_files(&reviews_dir(paths));
        let mut stamps = vec![
            stamp_of(file),
            stamp_of(&wal_of(file)),
            stamp_of(&paths.exports),
        ];
        stamps.extend(files.iter().map(|p| stamp_of(p)));
        if let Some((s, r)) = self.readings.get(file) {
            if *s == stamps {
                return Ok(r.clone());
            }
        }
        let store = EpisodeStore::open_read_only(file)?;
        let doc = store.load_document()?;
        let id = episode_id(&store)?;
        let name = store.meta("name")?.unwrap_or_else(|| stem(file));
        let edited = store.history(1)?.into_iter().next().map(|r| r.ts);
        let exported = crate::exports::any_export(paths, &name);
        let stage = stage_of(&doc, exported);
        let waiting: Vec<&ReviewFile> = files
            .iter()
            .filter_map(|p| self.reviews.get(p)?.1.as_ref())
            .collect();
        let open_notes = open_notes(doc, &waiting, &id);
        let r = Reading {
            id,
            name,
            stage,
            edited,
            open_notes,
        };
        self.readings
            .insert(file.to_path_buf(), (stamps, r.clone()));
        Ok(r)
    }

    fn card(&mut self, file: &Path, paths: &EpisodePaths, recent_name: &str) -> EpisodeCard {
        let show = paths.show.as_ref();
        let info: Option<ShowEpisode> = show.and_then(|s| {
            let folder = s.episode_folder_of(&paths.root)?;
            Some(s.episode_info(&folder))
        });
        let reading = self.read(file, paths).ok();
        let own = cover_in(&paths.root);
        let title = info
            .as_ref()
            .map(|i| i.title.clone().unwrap_or_else(|| i.folder.clone()))
            .or_else(|| reading.as_ref().map(|r| r.name.clone()))
            .unwrap_or_else(|| {
                if recent_name.is_empty() {
                    stem(file)
                } else {
                    recent_name.to_string()
                }
            });
        EpisodeCard {
            key: file.to_string_lossy().into_owned(),
            file: Some(file.to_string_lossy().into_owned()),
            dir: paths.root.to_string_lossy().into_owned(),
            title,
            show: show.map(|s| s.root.to_string_lossy().into_owned()),
            show_name: show.map(|s| s.name().to_string()),
            folder: info.as_ref().map(|i| i.folder.clone()),
            number: info.as_ref().and_then(|i| i.number),
            stage: reading.as_ref().map_or(0, |r| r.stage),
            edited: reading.as_ref().and_then(|r| r.edited.clone()),
            open_notes: reading.as_ref().map_or(0, |r| r.open_notes),
            own_cover: own.is_some(),
            cover: own.or_else(|| show.and_then(show_cover)).map(cover_ref),
            missing: false,
        }
    }

    /// An episode folder of a show, which may have no project yet.
    fn show_episode(&mut self, show: &Show, e: &ShowEpisode) -> EpisodeCard {
        if let Some(c) = e
            .file
            .as_ref()
            .and_then(|f| Some((f, EpisodePaths::for_file(f).ok()?)))
            .map(|(f, paths)| self.card(f, &paths, &e.label))
        {
            return c;
        }
        let own = cover_in(&e.dir);
        EpisodeCard {
            key: e.dir.to_string_lossy().into_owned(),
            file: None,
            dir: e.dir.to_string_lossy().into_owned(),
            title: e.title.clone().unwrap_or_else(|| e.folder.clone()),
            show: Some(show.root.to_string_lossy().into_owned()),
            show_name: Some(show.name().to_string()),
            folder: Some(e.folder.clone()),
            number: e.number,
            stage: 0,
            edited: None,
            open_notes: 0,
            own_cover: own.is_some(),
            cover: own.or_else(|| show_cover(show)).map(cover_ref),
            missing: false,
        }
    }

    fn show_card(&mut self, show: &Show) -> ShowCard {
        let episodes: Vec<EpisodeCard> = show
            .episodes()
            .iter()
            .map(|e| self.show_episode(show, e))
            .collect();
        let root = show.root.to_string_lossy().into_owned();
        ShowCard {
            key: root.clone(),
            root,
            name: show.name().to_string(),
            colors: show.data.theme.colors.clone(),
            cover: show_cover(show).map(cover_ref),
            open_notes: episodes.iter().map(|e| e.open_notes).sum(),
            episodes,
            missing: false,
        }
    }

    pub fn view(&mut self, recents: &[RecentP]) -> WelcomeView {
        let mut episodes = Vec::new();
        let mut shows: Vec<ShowCard> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for r in recents {
            let path = Path::new(&r.path);
            let show = if r.show {
                match Show::open(path) {
                    Ok(s) => Some(s),
                    Err(_) => {
                        if seen.insert(path.to_path_buf()) {
                            shows.push(missing_show(r));
                        }
                        None
                    }
                }
            } else {
                match episode_paths(path) {
                    Some((file, paths)) => {
                        let show = paths.show.clone();
                        episodes.push(self.card(&file, &paths, &r.name));
                        show
                    }
                    None => {
                        episodes.push(missing_episode(r));
                        None
                    }
                }
            };
            if let Some(s) = show {
                if seen.insert(s.root.clone()) {
                    shows.push(self.show_card(&s));
                }
            }
        }
        WelcomeView {
            episodes,
            shows,
            folder: projects_dir().to_string_lossy().into_owned(),
        }
    }

    /// The episodes a review file is about, among everything the shelf knows.
    fn matching(&mut self, file: &ReviewFile, recents: &[RecentP]) -> Vec<PathBuf> {
        let ids: HashSet<&str> = file
            .manifest
            .episodes
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        let view = self.view(recents);
        let known = view
            .episodes
            .iter()
            .chain(view.shows.iter().flat_map(|s| s.episodes.iter()));
        let mut found: Vec<PathBuf> = Vec::new();
        for path in known.filter_map(|e| e.file.as_ref()).map(PathBuf::from) {
            if found.contains(&path) {
                continue;
            }
            let id = self.readings.get(&path).map(|(_, r)| r.id.clone());
            if id.is_some_and(|id| ids.contains(id.as_str())) {
                found.push(path);
            }
        }
        found
    }
}

fn episode_paths(path: &Path) -> Option<(PathBuf, EpisodePaths)> {
    let file = find_episode_file(path).ok()?;
    let paths = EpisodePaths::for_file(&file).ok()?;
    Some((file, paths))
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn missing_episode(r: &RecentP) -> EpisodeCard {
    let title = if r.name.is_empty() {
        stem(Path::new(&r.path))
    } else {
        r.name.clone()
    };
    EpisodeCard {
        key: r.path.clone(),
        file: None,
        dir: String::new(),
        title,
        show: None,
        show_name: None,
        folder: None,
        number: None,
        stage: 0,
        edited: None,
        open_notes: 0,
        cover: None,
        own_cover: false,
        missing: true,
    }
}

fn missing_show(r: &RecentP) -> ShowCard {
    ShowCard {
        key: r.path.clone(),
        root: r.path.clone(),
        name: if r.name.is_empty() {
            stem(Path::new(&r.path))
        } else {
            r.name.clone()
        },
        colors: Vec::new(),
        cover: None,
        episodes: Vec::new(),
        open_notes: 0,
        missing: true,
    }
}

// ---- covers ----

fn unreadable(path: &Path, e: impl std::fmt::Display) -> AppError {
    AppError::Params(format!("{} can't be read as an image: {e}", path.display()))
}

/// An image the right way up: a phone's picture says how to turn it, and the JPEG written from
/// it won't.
fn load_image(path: &Path) -> Result<DynamicImage> {
    let mut decoder = ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()
        .map_err(|e| unreadable(path, e))?;
    let turn = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| unreadable(path, e))?;
    img.apply_orientation(turn);
    Ok(img)
}

fn jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(&img.to_rgb8())
        .map_err(|e| AppError::Other(format!("the image could not be written: {e}")))?;
    Ok(out)
}

fn within(img: DynamicImage, px: u32) -> DynamicImage {
    if img.width().max(img.height()) > px {
        img.resize(px, px, image::imageops::FilterType::Triangle)
    } else {
        img
    }
}

/// An image as a data URL (the UI has no file access), no larger than asked. Scaled once: the
/// result is kept in this machine's data folder, under the image's name, time and length.
pub fn cover_image(p: &CoverImageP) -> Result<Value> {
    let path = Path::new(&p.path);
    let px = p.px.clamp(64, COVER_PX);
    let (time, len) = stamp_of(path);
    let key = blake3::hash(format!("{}|{time:?}|{len}|{px}", p.path).as_bytes()).to_hex();
    let kept = apc_sound::data_root()
        .join("covers")
        .join(format!("{}.jpg", &key[..24]));
    let bytes = match std::fs::read(&kept) {
        Ok(b) => b,
        Err(_) => {
            let bytes = jpeg(&within(load_image(path)?, px), 82)?;
            if let Some(dir) = kept.parent() {
                std::fs::create_dir_all(dir)?;
            }
            write_atomic(&kept, &bytes)?;
            bytes
        }
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(json!({ "url": format!("data:image/jpeg;base64,{b64}") }))
}

/// A show's folder or show file, as opposed to an episode.
pub fn is_show_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(brand::SHOW_EXT)
        || (path.is_dir() && find_episode_file(path).is_err() && Show::open(path).is_ok())
}

/// Writes a cover in its folder, in place of any other.
fn write_cover(dir: &Path, img: &DynamicImage) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(COVER_NAMES[0]);
    write_atomic(&dest, &jpeg(img, 85)?)?;
    for n in &COVER_NAMES[1..] {
        let _ = std::fs::remove_file(dir.join(n));
    }
    Ok(dest)
}

/// Sets the cover of a show or an episode: a JPEG copy, at most 2400 px, as `Cover.jpg` in its
/// folder. `target` can be an episode folder that has no project yet. A show without colours of
/// its own takes them from it.
pub fn set_cover(p: &SetCoverP) -> Result<Value> {
    let target = Path::new(&p.target);
    let image = within(load_image(Path::new(&p.image))?, COVER_PX);
    let dest = if is_show_path(target) {
        let mut show = Show::open(target)?;
        let dest = write_cover(&show.root, &image)?;
        if show.data.theme.colors.is_empty() {
            let colors = crate::shows::palette(&image.thumbnail(64, 64).to_rgb8());
            show.update(|d| d.theme.colors = colors)?;
        }
        dest
    } else if let Some((_, paths)) = episode_paths(target) {
        write_cover(&paths.root, &image)?
    } else if target.extension().is_none() {
        write_cover(target, &image)?
    } else {
        return Err(AppError::Params(format!(
            "{} is not an episode or a show",
            target.display()
        )));
    };
    Ok(json!({ "cover": cover_ref(dest) }))
}

/// A review file dropped on the shelf: Add review on each episode it's about, without leaving
/// anything open. The file is archived once, after the last.
pub fn add_review(shelf: &mut Shelf, p: &ShelfReviewP, client: &str) -> Result<Value> {
    let path = Path::new(&p.path);
    let file = read_review(path)?;
    let found = shelf.matching(&file, &p.recents);
    if found.is_empty() {
        return Err(AppError::Params(format!(
            "This review is from {}, which isn't here. Open the episode, then add the review.",
            file.manifest.project
        )));
    }
    let (mut notes, mut replies, mut changes) = (0, 0, 0);
    let mut names: Vec<String> = Vec::new();
    let mut notice: Option<String> = None;
    for (i, ep) in found.iter().enumerate() {
        let mut svc = Service::open(ep, client)?;
        let last = i + 1 == found.len();
        let params = AddReviewP {
            path: p.path.clone(),
            anyway: false,
        };
        let r = crate::review::add_read(&mut svc, &file, &params, last);
        let closed = svc.close();
        let r = r?;
        closed?;
        notes += r["notes"].as_u64().unwrap_or(0);
        replies += r["replies"].as_u64().unwrap_or(0);
        changes += r["changes"].as_u64().unwrap_or(0);
        notice = notice.or_else(|| r["notice"].as_str().map(String::from));
        names.push(stem(ep));
    }
    Ok(json!({
        "who": file.manifest.reviewer.name,
        "notes": notes, "replies": replies, "changes": changes,
        "notice": notice,
        "episodes": names,
        "files": found.iter().map(|f| f.to_string_lossy().into_owned()).collect::<Vec<_>>(),
    }))
}
