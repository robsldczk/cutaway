//! Show-level methods (studio mode, D64–D68): the show home's view, the backdrop that themes
//! it, and copying a standalone episode into a show.

use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::{json, Value};

use apc_core::project::{find_episode_file, safe_name, SHOW_PREFIX};
use apc_core::show::{ignored_name, Show};
use apc_core::store::EpisodeStore;
use apc_core::*;

use crate::error::{AppError, Result};
use crate::service::Service;

/// Longest side of the stored backdrop.
const BACKDROP_PX: u32 = 2400;

/// Everything the show home shows.
pub fn view(show: &Show, open: Option<&Service>) -> Value {
    let open_file = open.map(|s| s.paths.file.clone());
    let episodes: Vec<Value> = show
        .episodes()
        .into_iter()
        .map(|e| {
            json!({
                "folder": e.folder, "number": e.number, "title": e.title, "label": e.label,
                "dir": e.dir.to_string_lossy(), "file": e.file.as_ref().map(|f| f.to_string_lossy().into_owned()),
                "open": e.file.is_some() && e.file == open_file,
            })
        })
        .collect();
    let backdrop = show
        .data
        .theme
        .backdrop
        .as_ref()
        .map(|b| show.root.join(b))
        .filter(|p| p.exists());
    json!({
        "name": show.name(),
        "root": show.root.to_string_lossy(),
        "file": show.file.to_string_lossy(),
        "sound": show.sound_dir().to_string_lossy(),
        "episodes": episodes,
        "theme": {
            "colors": show.data.theme.colors,
            "backdrop": backdrop.map(|p| p.to_string_lossy().into_owned()),
        },
    })
}

/// The backdrop as a data URL (the UI has no file access).
pub fn backdrop(show: &Show) -> Result<Value> {
    let Some(rel) = &show.data.theme.backdrop else {
        return Ok(json!({ "url": null }));
    };
    let p = show.root.join(rel);
    let Ok(bytes) = std::fs::read(&p) else {
        return Ok(json!({ "url": null }));
    };
    let mime = match p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(json!({ "url": format!("data:{mime};base64,{b64}"), "colors": show.data.theme.colors }))
}

/// Sets the show's backdrop (set once): a JPEG copy, at most 2400 px, goes to
/// `Sound/Cutaway/backdrop.jpg`, and the theme colours are taken from it.
pub fn set_backdrop(show: &mut Show, image: &Path) -> Result<Value> {
    let img = image::ImageReader::open(image)?
        .with_guessed_format()?
        .decode()
        .map_err(|e| {
            AppError::Params(format!(
                "{} can't be read as an image: {e}",
                image.display()
            ))
        })?;
    let img = if img.width().max(img.height()) > BACKDROP_PX {
        img.resize(
            BACKDROP_PX,
            BACKDROP_PX,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let colors = palette(&img.thumbnail(64, 64).to_rgb8());
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode_image(&img.to_rgb8())
        .map_err(|e| AppError::Other(format!("the backdrop could not be written: {e}")))?;
    let dest = show.cutaway_dir().join("backdrop.jpg");
    std::fs::create_dir_all(show.cutaway_dir())?;
    apc_core::show::write_atomic(&dest, &jpeg)?;
    let rel = dest
        .strip_prefix(&show.root)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| dest.to_string_lossy().into_owned());
    show.update(|d| {
        d.theme.backdrop = Some(rel.clone());
        d.theme.colors = colors.clone();
    })?;
    Ok(json!({ "backdrop": rel, "colors": colors, "width": img.width(), "height": img.height() }))
}

/// Three colours that carry an image: the most present saturated hues, each averaged,
/// then the image's overall tone when it has fewer.
pub fn palette(img: &image::RgbImage) -> Vec<String> {
    const BINS: usize = 12;
    let mut sum = [[0f64; 4]; BINS]; // r, g, b, weight
    let mut all = [0f64; 4];
    for p in img.pixels() {
        let [r, g, b] = p.0.map(|c| c as f64 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let (v, sat) = (max, if max > 0.0 { (max - min) / max } else { 0.0 });
        all = [all[0] + r, all[1] + g, all[2] + b, all[3] + 1.0];
        if sat < 0.18 || !(0.12..=0.97).contains(&v) {
            continue;
        }
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } / 6.0;
        let i = ((h * BINS as f64) as usize).min(BINS - 1);
        let w = sat * v;
        sum[i] = [
            sum[i][0] + r * w,
            sum[i][1] + g * w,
            sum[i][2] + b * w,
            sum[i][3] + w,
        ];
    }
    let mut bins: Vec<[f64; 4]> = sum.into_iter().filter(|s| s[3] > 0.0).collect();
    bins.sort_by(|a, b| b[3].total_cmp(&a[3]));
    let hex = |r: f64, g: f64, b: f64| {
        format!(
            "#{:02x}{:02x}{:02x}",
            (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (b.clamp(0.0, 1.0) * 255.0).round() as u8
        )
    };
    let mut out: Vec<String> = bins
        .iter()
        .take(3)
        .map(|s| hex(s[0] / s[3], s[1] / s[3], s[2] / s[3]))
        .collect();
    if all[3] > 0.0 {
        let avg = hex(all[0] / all[3], all[1] / all[3], all[2] / all[3]);
        while out.len() < 3 {
            out.push(avg.clone());
        }
    }
    out
}

/// Files of a standalone episode folder to copy: everything except the open database
/// (copied through SQLite), sync leftovers and Finder files.
fn files_to_copy(root: &Path, episode_file: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let ep_name = episode_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if ignored_name(&name) || name.starts_with(&ep_name) {
            continue;
        }
        if p.is_dir() {
            files_to_copy(&p, episode_file, out);
        } else {
            out.push(p);
        }
    }
}

/// Copies a standalone episode into a show's episode folder (the source stays as it is).
/// The episode's own sounds that aren't vocals move into the show's library; vocals stay
/// with the episode (D68). `dry_run` only reports.
pub fn migrate(show: &Show, source: &Path, episode: &str, dry_run: bool) -> Result<Value> {
    let src_file = find_episode_file(source)?;
    let src_root = src_file
        .parent()
        .ok_or_else(|| AppError::Params("episode file has no folder".into()))?
        .to_path_buf();
    let target = show.episode_dir(episode);
    if let Ok(f) = find_episode_file(&target) {
        return Err(AppError::Params(format!(
            "{episode} already has a {} project ({})",
            brand::DISPLAY_NAME,
            f.display()
        )));
    }
    if target.starts_with(&src_root) {
        return Err(AppError::Params(
            "the show folder is inside the episode".into(),
        ));
    }
    let target_file = target.join(format!("{}.{}", safe_name(episode), brand::EPISODE_EXT));
    let doc = EpisodeStore::open_read_only(&src_file)?.load_document()?;
    // the episode's own sounds that belong in the show's library (not its vocals)
    let moving: Vec<&Asset> = doc
        .assets
        .values()
        .filter(|a| !a.category.starts_with("Vocals") && !a.file.starts_with(SHOW_PREFIX))
        .filter(|a| !Path::new(&a.file).is_absolute())
        .collect();
    let src_library = src_root.join("Library");
    let moving_files: Vec<PathBuf> = moving.iter().map(|a| src_library.join(&a.file)).collect();
    let mut files = Vec::new();
    files_to_copy(&src_root, &src_file, &mut files);
    files.retain(|f| !moving_files.contains(f));
    files.sort();
    let bytes: u64 = files
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum::<u64>()
        + std::fs::metadata(&src_file).map(|m| m.len()).unwrap_or(0);

    let mut warnings: Vec<String> = Vec::new();
    for s in doc.sources.values() {
        if Path::new(&s.path).is_absolute() {
            warnings.push(format!(
                "{} is referenced outside the episode folder ({}); it only plays on this machine. Import it again to copy it in.",
                s.original_name, s.path
            ));
        }
    }
    let plan = json!({
        "source": src_file.to_string_lossy(),
        "target": target_file.to_string_lossy(),
        "files": files.len() + 1,
        "bytes": bytes,
        "sounds_to_show_library": moving.iter().map(|a| a.file.clone()).collect::<Vec<_>>(),
        "vocals_stay": doc.assets.values().filter(|a| a.category.starts_with("Vocals")).count(),
        "warnings": warnings,
    });
    if dry_run {
        return Ok(json!({ "dry_run": true, "plan": plan }));
    }

    std::fs::create_dir_all(&target)?;
    for f in &files {
        let rel = f.strip_prefix(&src_root).unwrap_or(f);
        let dest = target.join(rel);
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::copy(f, &dest)?;
    }
    EpisodeStore::copy_file(&src_file, &target_file)?;
    {
        let store = EpisodeStore::open(&target_file)?;
        store.set_meta("name", episode)?;
        store.set_meta("migrated_from", &src_file.to_string_lossy())?;
    }

    // sounds into the show's library: a bucket's sounds into its folder, others loose
    let mut svc = Service::open(&target_file, "pipeline")?;
    let mut moved = 0;
    for a in moving {
        let path = src_library.join(&a.file);
        if !path.exists() {
            continue;
        }
        let bucket = svc
            .doc()
            .buckets
            .values()
            .find(|b| b.kept.contains(&a.id))
            .map(|b| b.id.clone());
        let dest = match bucket {
            Some(b) => crate::studio::bucket_dest(&mut svc, &b)?,
            None => None,
        };
        let dest = match dest {
            Some(d) => d,
            None => {
                let music = a.category.starts_with("Music");
                let lib = svc
                    .show_lib
                    .as_ref()
                    .ok_or_else(|| AppError::Other("the show library is not open".into()))?;
                let loose = lib
                    .lock()
                    .map_err(|_| AppError::Other("library lock poisoned".into()))?
                    .loose_folder(music);
                format!("{SHOW_PREFIX}{loose}")
            }
        };
        let copied = crate::sound::import_into(&mut svc, &path, &dest, a)?;
        // the episode keeps its asset id (placements and buckets point at it); only the file
        // moves into the show
        let mut updated = a.clone();
        updated.file = copied.file;
        svc.exec(&Command::AddAsset { asset: updated })?;
        moved += 1;
    }
    let missing_sources = apc_core::media::missing_sources(svc.doc(), &svc.paths).len();
    let lib = svc.library();
    let missing_sounds: Vec<String> = svc
        .doc()
        .assets
        .values()
        .filter(|a| !lib.path_of(a).exists())
        .map(|a| a.file.clone())
        .collect();
    drop(lib);
    svc.close()?;
    Ok(json!({
        "dry_run": false,
        "plan": plan,
        "file": target_file.to_string_lossy(),
        "sounds_moved": moved,
        "missing_sources": missing_sources,
        "missing_sounds": missing_sounds,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_finds_the_main_hues() {
        let mut img = image::RgbImage::new(10, 10);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 6 {
                image::Rgb([200, 40, 30])
            } else if x < 9 {
                image::Rgb([20, 60, 190])
            } else {
                image::Rgb([128, 128, 128])
            };
        }
        let c = palette(&img);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], "#c8281e");
        assert_eq!(c[1], "#143cbe");
    }
}
