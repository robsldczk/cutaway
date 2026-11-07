//! `current/`: the newest export of a scene, or of the episode, under names that don't change
//! (D165). Another app points at `Exports/Sc02_Canteen/current/E02_S02_Mix.wav` once and always
//! gets the newest. Every version stays in `versions/`, as written; `current/` is a copy.
//!
//! The newest of each track, not the newest folder (D166): a version that exported only the
//! dialogue again leaves the mix of the version before in `current/`, as long as the scene is
//! still as it was then.
//!
//! ```text
//! Sc02_Canteen/versions/v003_2026-11-07/E02_S02_v003_Mix.wav   the version, as exported
//! Sc02_Canteen/current/E02_S02_Mix.wav                         the same file, its newest
//! ```
//!
//! The copy's timeline files (FCPXML, cut list) point at the copies, and its manifest lists them
//! with their checksums. On APFS a copy is a clone and takes no room on this disk; a sync tool
//! still carries it as a file of its own.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{ExportError, Result};
use crate::versions::{
    checksum, groups, subfolders, version_dirs, ExportRecord, Manifest, CURRENT_FOLDER, MANIFEST,
    VERSIONS_FOLDER,
};

/// `_v003`, as names carry a version.
fn tag(version: u32) -> String {
    format!("_v{version:03}")
}

/// A version's file name without the version: `E02_S02_v003_Mix.wav` is `E02_S02_Mix.wav`,
/// `E02_v002.fcpxml` is `E02.fcpxml`. The version is the first `_v003` that ends a part.
pub fn unversioned(name: &str, version: u32) -> String {
    let t = tag(version);
    let mut from = 0;
    while let Some(i) = name[from..].find(&t) {
        let at = from + i;
        let rest = &name[at + t.len()..];
        if rest.is_empty() || rest.starts_with('_') || rest.starts_with('.') {
            return format!("{}{rest}", &name[..at]);
        }
        from = at + 1;
    }
    name.to_string()
}

/// The stem every file of a version starts with (`E02_S02_v003`).
fn stem_of(m: &Manifest) -> Option<String> {
    let t = tag(m.version);
    m.files.iter().find_map(|f| {
        let i = f.name.find(&t)?;
        Some(f.name[..i + t.len()].to_string())
    })
}

/// The files that name other files: timeline files, subtitles, cut lists.
fn is_text(name: &str) -> bool {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "fcpxml" | "xml" | "srt" | "csv" | "md" | "txt"
    )
}

/// A folder name as a `file://` URL spells it (`fcpxml::file_url`).
fn url_part(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Replaces each `from` with its `to`.
fn replaced(text: &str, pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .filter(|(from, to)| !from.is_empty() && from != to)
        .fold(text.to_string(), |t, (from, to)| t.replace(from, to))
}

/// The scene's or the episode's folder a version folder is in, either layout.
fn group_of(version_dir: &Path) -> Result<PathBuf> {
    let parent = version_dir
        .parent()
        .ok_or_else(|| ExportError::Invalid("a version folder has no parent".into()))?;
    let group = if name_of(parent) == VERSIONS_FOLDER {
        parent.parent()
    } else {
        Some(parent)
    };
    group
        .map(Path::to_path_buf)
        .ok_or_else(|| ExportError::Invalid("a version folder has no scene folder".into()))
}

/// What a file is, so the same track from two versions is one entry of `current/`: the track
/// in lower case with `_` and `-` alike (`Dialogue_THE_COLONEL`, written before D135, is
/// `Dialogue_THE-COLONEL`), whatever its format.
fn key_of(track: &str) -> String {
    track.to_lowercase().replace('_', "-")
}

/// Kinds of file that come as a set: the newest version with any of them gives all of them.
/// The dialogue as one file and per character are one family, so it's never there twice;
/// individual clips are another, so clips of takes no longer in the cut don't stay.
fn family_of(track: &str) -> Option<&'static str> {
    let k = key_of(track);
    if k == "dialogue" || k.starts_with("dialogue-") {
        Some("dialogue")
    } else if k == "clip" {
        Some("clips")
    } else {
        None
    }
}

/// One version of a group, as found on disk.
struct Found {
    dir: PathBuf,
    manifest: Manifest,
}

/// A file of `current/`: where it comes from and what it's called there.
struct Pick {
    from: PathBuf,
    version: u32,
    /// The version's own folder and stem, which its text files name.
    pairs: Vec<(String, String)>,
    file: crate::versions::ManifestFile,
}

/// Every version of a scene's (or the episode's) folder, newest first.
fn versions_in(group: &Path) -> Vec<Found> {
    let mut out: Vec<Found> = version_dirs(group)
        .into_iter()
        .filter_map(|dir| read_manifest(&dir).map(|manifest| Found { dir, manifest }))
        .collect();
    out.sort_by(|a, b| {
        (b.manifest.version, &b.manifest.date).cmp(&(a.manifest.version, &a.manifest.date))
    });
    out
}

/// What `current/` should hold (D166): of each track, the newest version that has it. An older
/// version counts only while the scene is as it was then (the same timeline hash): once the cut
/// moved, its files would no longer line up with the newer ones. They stay in `versions/`.
fn picks(group: &Path) -> Vec<Pick> {
    let found = versions_in(group);
    let Some(newest) = found.first() else {
        return Vec::new();
    };
    let hash = newest.manifest.timeline_hash.clone();
    let (g, current) = (url_part(&name_of(group)), CURRENT_FOLDER);
    let mut taken_keys = std::collections::HashSet::new();
    let mut taken_families = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, f) in found.iter().enumerate() {
        let m = &f.manifest;
        if i > 0 && (hash.is_empty() || m.timeline_hash != hash) {
            continue;
        }
        let v = url_part(&name_of(&f.dir));
        let mut pairs = vec![
            (
                format!("/{g}/{VERSIONS_FOLDER}/{v}/"),
                format!("/{g}/{current}/"),
            ),
            (format!("/{g}/{v}/"), format!("/{g}/{current}/")),
        ];
        if let Some(stem) = stem_of(m) {
            pairs.push((stem.clone(), unversioned(&stem, m.version)));
        }
        // a family this version has is decided here, by its newest version
        let families: std::collections::HashSet<&str> =
            m.files.iter().filter_map(|x| family_of(&x.track)).collect();
        for x in &m.files {
            let key = key_of(&x.track);
            let take = match family_of(&x.track) {
                Some(fam) => !taken_families.contains(fam),
                None => !taken_keys.contains(&key),
            };
            if take {
                taken_keys.insert(key);
                out.push(Pick {
                    from: f.dir.join(&x.name),
                    version: m.version,
                    pairs: pairs.clone(),
                    file: x.clone(),
                });
            }
        }
        taken_families.extend(families);
    }
    out
}

/// `current/` already holds these picks: the same names from the same versions.
fn holds(current: &Path, picks: &[Pick]) -> bool {
    let Some(m) = read_manifest(current) else {
        return false;
    };
    let mut have: Vec<(String, Option<u32>)> = m
        .files
        .iter()
        .map(|f| (f.name.clone(), f.version))
        .collect();
    let mut want: Vec<(String, Option<u32>)> = picks
        .iter()
        .map(|p| (unversioned(&p.file.name, p.version), Some(p.version)))
        .collect();
    have.sort();
    want.sort();
    have == want && want.iter().all(|(n, _)| current.join(n).is_file())
}

/// Writes `current/` for a scene's (or the episode's) folder: the picks copied under names
/// without their version, the text files pointing at the copies. It's built beside the old one
/// and swapped in, so whatever reads `current/` finds the old set or the new one, never half.
/// Gives the newest version, or `None` when there's nothing to write or it's already in place.
pub fn refresh(group: &Path) -> Result<Option<u32>> {
    let picks = picks(group);
    let current = group.join(CURRENT_FOLDER);
    let Some(newest) = versions_in(group).into_iter().next() else {
        return Ok(None);
    };
    if picks.is_empty() || holds(&current, &picks) {
        return Ok(None);
    }
    let pid = std::process::id();
    let building = group.join(format!(".{CURRENT_FOLDER}_writing_{pid}"));
    if building.exists() {
        std::fs::remove_dir_all(&building)?;
    }
    std::fs::create_dir_all(&building)?;
    let made = (|| -> Result<()> {
        let mut files = Vec::new();
        for p in &picks {
            let mut f = p.file.clone();
            f.name = unversioned(&f.name, p.version);
            f.version = Some(p.version);
            let to = building.join(&f.name);
            if is_text(&f.name) {
                std::fs::write(&to, replaced(&std::fs::read_to_string(&p.from)?, &p.pairs))?;
                f.bytes = std::fs::metadata(&to)?.len();
                f.checksum = checksum(&to)?;
            } else {
                std::fs::copy(&p.from, &to)?;
            }
            files.push(f);
        }
        let out = Manifest {
            files,
            ..newest.manifest.clone()
        };
        std::fs::write(building.join(MANIFEST), serde_json::to_vec_pretty(&out)?)?;
        // the swap: the old set steps aside, the new one takes its name, the old one goes
        let old = group.join(format!(".{CURRENT_FOLDER}_old_{pid}"));
        if current.exists() {
            if old.exists() {
                std::fs::remove_dir_all(&old)?;
            }
            std::fs::rename(&current, &old)?;
        }
        std::fs::rename(&building, &current)?;
        if old.exists() {
            std::fs::remove_dir_all(&old)?;
        }
        Ok(())
    })();
    if made.is_err() {
        let _ = std::fs::remove_dir_all(&building);
    }
    made.map(|()| Some(newest.manifest.version))
}

/// After an export: `current/` of the version's folder, with what it brought.
pub fn publish(record: &ExportRecord) -> Result<PathBuf> {
    let group = group_of(&record.folder)?;
    refresh(&group)?;
    Ok(group.join(CURRENT_FOLDER))
}

/// What `reorganize` did, by folder relative to the base.
#[derive(Serialize, Default, Debug, PartialEq)]
pub struct Reorganized {
    /// Versions moved into their `versions/`.
    pub moved: Vec<String>,
    /// `current/` folders written, and the version each holds.
    pub current: Vec<(String, u32)>,
    /// Text files whose paths were brought along.
    pub rewritten: usize,
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    serde_json::from_slice(&std::fs::read(dir.join(MANIFEST)).ok()?).ok()
}

/// Moves a version written before D165 into `versions/`, its text files' paths and the
/// manifest's checksums with it. Gives how many files were rewritten.
fn move_into_versions(group: &Path, legacy: &Path) -> Result<(PathBuf, usize)> {
    let to = group.join(VERSIONS_FOLDER).join(name_of(legacy));
    if to.exists() {
        return Err(ExportError::Invalid(format!(
            "{} is in the way of {}",
            to.display(),
            legacy.display()
        )));
    }
    std::fs::create_dir_all(group.join(VERSIONS_FOLDER))?;
    std::fs::rename(legacy, &to)?;
    let Some(mut m) = read_manifest(&to) else {
        return Ok((to, 0));
    };
    let (g, v) = (url_part(&name_of(group)), url_part(&name_of(legacy)));
    let pairs = [(format!("/{g}/{v}/"), format!("/{g}/{VERSIONS_FOLDER}/{v}/"))];
    let mut rewritten = 0;
    for f in m.files.iter_mut().filter(|f| is_text(&f.name)) {
        let p = to.join(&f.name);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let new = replaced(&text, &pairs);
        if new != text {
            std::fs::write(&p, new)?;
            f.bytes = std::fs::metadata(&p)?.len();
            f.checksum = checksum(&p)?;
            rewritten += 1;
        }
    }
    if rewritten > 0 {
        std::fs::write(to.join(MANIFEST), serde_json::to_vec_pretty(&m)?)?;
    }
    Ok((to, rewritten))
}

/// Brings an export folder to the D165 layout. Versions written straight into a scene's folder
/// move into its `versions/`, and each `current/` is made the newest version. Run again, it
/// changes nothing. Folders being written (a leading dot) are left alone.
pub fn reorganize(base: &Path) -> Result<Reorganized> {
    let mut done = Reorganized::default();
    let rel = |p: &Path| {
        p.strip_prefix(base)
            .unwrap_or(p)
            .to_string_lossy()
            .into_owned()
    };
    for group in groups(base) {
        for legacy in subfolders(&group) {
            let n = name_of(&legacy);
            if n == VERSIONS_FOLDER || n == CURRENT_FOLDER || read_manifest(&legacy).is_none() {
                continue;
            }
            let (to, rewritten) = move_into_versions(&group, &legacy)?;
            done.moved.push(rel(&to));
            done.rewritten += rewritten;
        }
        if let Some(version) = refresh(&group)? {
            done.current
                .push((rel(&group.join(CURRENT_FOLDER)), version));
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_leaves_the_names() {
        assert_eq!(unversioned("E02_S02_v003_Mix.wav", 3), "E02_S02_Mix.wav");
        assert_eq!(unversioned("E02_S02_v003.fcpxml", 3), "E02_S02.fcpxml");
        assert_eq!(unversioned("E02_v012_Mix.mp3", 12), "E02_Mix.mp3");
        assert_eq!(
            unversioned("E02_S02_v003_L014_PAUL_T3_p2.wav", 3),
            "E02_S02_L014_PAUL_T3_p2.wav"
        );
        // only a whole part: `_v0031` isn't version 3
        assert_eq!(
            unversioned("E02_S02_v0031_Mix.wav", 3),
            "E02_S02_v0031_Mix.wav"
        );
        assert_eq!(unversioned("manifest.json", 3), "manifest.json");
    }

    #[test]
    fn a_folder_name_is_spelled_as_urls_spell_it() {
        assert_eq!(url_part("Sc02_Canteen"), "Sc02_Canteen");
        assert_eq!(url_part("Episode 2"), "Episode%202");
    }
}
