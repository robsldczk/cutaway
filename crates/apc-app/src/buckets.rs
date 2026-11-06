//! Sound buckets (Import › Sounds and the Edit page's docked bucket): named groups of sounds
//! whose keywords are the search query. Previews are search results; a download happens
//! only when a sound is kept or dropped on the timeline.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use apc_core::ids::new_id;
use apc_core::*;
use apc_sound::epidemic::EpidemicProvider;
use apc_sound::secrets::KeyedService;

use crate::error::{AppError, Result};
use crate::service::Service;

/// Previews a bucket search adds per page ("Load more").
pub const PAGE: usize = 5;
/// Results per page of the free Epidemic search.
pub const FREE_PAGE: usize = 10;

/// Bucket a spotted cue belongs to, from its words (deterministic grouping of AI cues).
pub fn category_of(c: &SoundCue, place: &str) -> String {
    if c.cue_type == CueType::Ambience {
        return format!("{place} ambience").trim().to_string();
    }
    if c.cue_type == CueType::Walla {
        return "Walla and crowd".into();
    }
    if c.cue_type == CueType::Music {
        return "Music".into();
    }
    let d = format!("{} {}", c.description, c.tags.join(" ")).to_lowercase();
    let rules: &[(&[&str], &str)] = &[
        (
            &[
                "foil", "burrito", "bite", "steam", "sizzle", "food", "set down", "table",
                "coffee", "cup",
            ],
            "Food and foil",
        ),
        (
            &["door", "knock", "hatch", "cage", "squeak"],
            "Doors and hatches",
        ),
        (&["phone", "dial", "ring", "hang"], "Phones"),
        (&["ufo", "beam", "sci-fi", "ship"], "Sci-fi"),
        (
            &[
                "machine", "hum", "computer", "beep", "power", "button", "lever", "alarm", "bell",
            ],
            "Machines",
        ),
        (&["footstep", "walk", "run"], "Footsteps"),
        (&["walla", "crowd", "chatter"], "Walla and crowd"),
    ];
    rules
        .iter()
        .find(|(ks, _)| ks.iter().any(|k| d.contains(k)))
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| "Props".into())
}

/// A short keyword from a cue description ("aluminum foil rustle" → "foil rustle").
fn keyword_of(desc: &str) -> String {
    let stop = [
        "a", "an", "the", "on", "of", "in", "into", "against", "down",
    ];
    let words: Vec<&str> = desc
        .split_whitespace()
        .filter(|w| !stop.contains(&w.to_lowercase().as_str()))
        .collect();
    words[words.len().saturating_sub(2)..]
        .join(" ")
        .to_lowercase()
}

/// Scenes that have script lines, and those of them not spotted yet (or changed since).
pub fn spot_status(doc: &Document) -> (Vec<String>, Vec<String>) {
    let with_lines: Vec<String> = doc
        .scenes_in_order()
        .into_iter()
        .filter(|s| !doc.lines_of_scene(&s.id).is_empty())
        .map(|s| s.id.clone())
        .collect();
    let pending = with_lines
        .iter()
        .filter(|id| !doc.scene_spotted(id))
        .cloned()
        .collect();
    (with_lines, pending)
}

/// Changes one bucket inside a transaction on the latest document, so writers that run
/// at the same time (a spotting job, a search waiting on the network, a drop) don't
/// overwrite each other's changes the way a `PutBucket` of an older copy would.
fn edit_bucket(
    svc: &mut Service,
    id: &str,
    f: impl Fn(&Document, &mut SoundBucket),
) -> Result<SoundBucket> {
    let r = svc
        .bus
        .execute_fn("edit_bucket", &json!({ "bucket_id": id }), |tx| {
            let mut b = tx
                .doc()
                .buckets
                .get(id)
                .cloned()
                .ok_or_else(|| CoreError::not_found("bucket", id))?;
            f(tx.doc(), &mut b);
            tx.put(&b)?;
            Ok(serde_json::to_value(&b)?)
        })?;
    if r.seq.is_some() {
        svc.after_write()?;
    }
    Ok(serde_json::from_value(r.result)?)
}

/// Keywords a bucket's live cues give.
fn cue_keywords(doc: &Document, b: &SoundBucket) -> BTreeSet<String> {
    b.cue_ids
        .iter()
        .filter_map(|id| doc.cues.get(id))
        .map(|c| keyword_of(&c.description))
        .collect()
}

/// Brings a bucket up to date with a scene's spotted cues: replaced cues leave, keywords
/// that only came from them go (keywords typed by hand stay), the new cues' keywords come.
fn merge_spotted(
    doc: &Document,
    b: &mut SoundBucket,
    scene_id: &str,
    cues: &[SoundCue],
    stale_keywords: &BTreeSet<String>,
) {
    if !b.scene_ids.iter().any(|s| s == scene_id) {
        b.scene_ids.push(scene_id.to_string());
    }
    b.cue_ids.retain(|id| doc.cues.contains_key(id));
    for c in cues {
        if !b.cue_ids.contains(&c.id) {
            b.cue_ids.push(c.id.clone());
        }
    }
    let live = cue_keywords(doc, b);
    b.keywords
        .retain(|k| !stale_keywords.contains(k) || live.contains(k));
    for c in cues {
        let k = keyword_of(&c.description);
        if !k.is_empty() && !b.keywords.contains(&k) && b.keywords.len() < 6 {
            b.keywords.push(k);
        }
    }
    if b.cue_ids.iter().any(|id| {
        doc.cues
            .get(id)
            .map(|c| c.cue_type == CueType::Ambience)
            .unwrap_or(false)
    }) {
        b.length_min_s.get_or_insert(10.0);
    }
}

/// An AI bucket a re-spot of `scene_id` no longer fills: it lets go of the scene, and goes
/// away when nothing is left in it (no cues, nothing kept, nothing dropped in by hand).
fn release_scene(
    svc: &mut Service,
    id: &str,
    scene_id: &str,
    stale_keywords: &BTreeSet<String>,
) -> Result<()> {
    let has_files = svc
        .doc()
        .buckets
        .get(id)
        .map(|b| crate::studio::folder_has_files(svc, b))
        .unwrap_or(false);
    let r = svc.bus.execute_fn(
        "edit_bucket",
        &json!({ "bucket_id": id, "release_scene": scene_id }),
        |tx| {
            let Some(mut b) = tx.doc().buckets.get(id).cloned() else {
                return Ok(Value::Null);
            };
            let doc = tx.doc();
            b.cue_ids.retain(|c| doc.cues.contains_key(c));
            let live = cue_keywords(doc, &b);
            b.keywords
                .retain(|k| !stale_keywords.contains(k) || live.contains(k));
            let still_here = b
                .cue_ids
                .iter()
                .filter_map(|c| doc.cues.get(c))
                .any(|c| c.scene_id == scene_id);
            if !still_here {
                b.scene_ids.retain(|s| s != scene_id);
            }
            if b.cue_ids.is_empty()
                && b.kept.is_empty()
                && !has_files
                && !b.previews.iter().any(|p| p.pinned)
            {
                tx.delete::<SoundBucket>(id)?;
            } else {
                tx.put(&b)?;
            }
            Ok(Value::Null)
        },
    )?;
    if r.seq.is_some() {
        svc.after_write()?;
    }
    Ok(())
}

/// "Spot script": AI sound spotting (the script only, no audio), grouped into buckets.
/// Runs only when asked: one AI call per scene. Without `scene` it spots the scenes not
/// spotted yet or changed since, or every scene with `all` ("Re-spot"). A scene that
/// fails is reported and the others go on.
pub fn spot(
    svc: &mut Service,
    scene: Option<&str>,
    all: bool,
    progress: &dyn Fn(f64, &str),
) -> Result<Value> {
    let (with_lines, pending) = spot_status(svc.doc());
    let ids: Vec<String> = match scene {
        Some(k) => vec![svc
            .doc()
            .find_scene(k)
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Params(format!("no scene '{k}'")))?],
        None if all => with_lines.clone(),
        None => pending,
    };
    let scenes: Vec<Scene> = ids
        .iter()
        .filter_map(|id| svc.doc().scenes.get(id).cloned())
        .filter(|s| !svc.doc().lines_of_scene(&s.id).is_empty())
        .collect();
    let mut touched = 0;
    let mut spotted = 0;
    let mut cue_count = 0;
    let mut provider = Value::Null;
    let mut warning = Value::Null;
    let mut failed: Vec<Value> = Vec::new();
    for (i, sc) in scenes.iter().enumerate() {
        progress(
            i as f64 / scenes.len() as f64,
            &format!("Scene {} ({} of {})", sc.number, i + 1, scenes.len()),
        );
        // keywords the last spotting of this scene gave: they go unless they come back
        let stale: BTreeSet<String> = svc
            .doc()
            .cues
            .values()
            .filter(|c| c.scene_id == sc.id && c.ai_suggested)
            .map(|c| keyword_of(&c.description))
            .collect();
        let r = match crate::sound::spot(svc, &sc.id) {
            Ok(r) => r,
            Err(e) => {
                failed.push(json!({ "scene": sc.number, "error": e.to_string() }));
                continue;
            }
        };
        spotted += 1;
        cue_count += r["cues"].as_u64().unwrap_or(0);
        provider = r["provider"].clone();
        if warning.is_null() {
            warning = r["warning"].clone();
        }
        let cues: Vec<SoundCue> = svc
            .doc()
            .cues
            .values()
            .filter(|c| c.scene_id == sc.id && c.ai_suggested)
            .cloned()
            .collect();
        let mut groups: BTreeMap<String, Vec<SoundCue>> = BTreeMap::new();
        for c in cues {
            groups
                .entry(category_of(&c, &sc.place))
                .or_default()
                .push(c);
        }
        let folders = svc
            .show_lib
            .as_ref()
            .and_then(|l| l.lock().ok().map(|l| l.folders()))
            .unwrap_or_default();
        for (name, cs) in &groups {
            // in a show, a group that names a library folder goes into that folder's bucket
            let music = name.eq_ignore_ascii_case("music");
            let folder = crate::studio::match_folder(&folders, name, music);
            let existing = svc
                .doc()
                .buckets
                .values()
                .find(|b| b.name.eq_ignore_ascii_case(name))
                .or_else(|| {
                    folder.as_ref().and_then(|f| {
                        svc.doc()
                            .buckets
                            .values()
                            .find(|b| b.folder.as_ref() == Some(f))
                    })
                })
                .map(|b| b.id.clone());
            match existing {
                Some(id) => {
                    edit_bucket(svc, &id, |doc, b| merge_spotted(doc, b, &sc.id, cs, &stale))?;
                }
                None => {
                    let mut b = SoundBucket {
                        id: new_id("bk"),
                        name: name.clone(),
                        keywords: vec![],
                        scene_ids: vec![],
                        cue_ids: vec![],
                        kept: vec![],
                        previews: vec![],
                        dismissed: vec![],
                        length_min_s: None,
                        length_max_s: None,
                        exclude: vec!["music".into(), "voices".into()],
                        ai_suggested: true,
                        order: svc.doc().buckets.len() as u32,
                        search_offset: 0,
                        folder,
                        loops: Default::default(),
                    };
                    merge_spotted(svc.doc(), &mut b, &sc.id, cs, &stale);
                    svc.exec(&Command::PutBucket { bucket: b })?;
                }
            }
            touched += 1;
        }
        // AI buckets of this scene the new spotting didn't fill
        let left: Vec<String> = svc
            .doc()
            .buckets
            .values()
            .filter(|b| b.ai_suggested)
            .filter(|b| b.scene_ids.contains(&sc.id))
            .filter(|b| !groups.keys().any(|n| n.eq_ignore_ascii_case(&b.name)))
            .map(|b| b.id.clone())
            .collect();
        for id in left {
            release_scene(svc, &id, &sc.id, &stale)?;
        }
    }
    // new AI buckets get their folder names (the folders come with their first sounds)
    crate::studio::link_folders(svc)?;
    if spotted == 0 && !failed.is_empty() {
        return Err(AppError::Other(format!(
            "spotting failed: {}",
            failed[0]["error"].as_str().unwrap_or("unknown error")
        )));
    }
    Ok(json!({
        "buckets": touched,
        "scenes": spotted,
        "cues": cue_count,
        "skipped": if scene.is_some() { 0 } else { with_lines.len().saturating_sub(scenes.len()) },
        "failed": failed,
        "provider": provider,
        "warning": warning,
    }))
}

fn scene_labels(doc: &Document, b: &SoundBucket) -> Vec<String> {
    let mut v: Vec<u32> = b
        .scene_ids
        .iter()
        .filter_map(|s| doc.scenes.get(s))
        .map(|s| s.number)
        .collect();
    v.sort_unstable();
    v.into_iter().map(|n| format!("Sc. {n}")).collect()
}

fn asset_view(svc: &mut Service, a: &Asset) -> Value {
    let peaks: Vec<f32> = svc
        .analysis_of(&format!("asset:{}", a.id))
        .map(|an| {
            let g = an
                .peaks
                .iter()
                .map(|(lo, hi)| lo.abs().max(hi.abs()))
                .fold(1e-4f32, f32::max);
            an.peaks_range(0.0, an.info.duration_s, 60)
                .into_iter()
                .map(|(lo, hi)| lo.abs().max(hi.abs()) / g)
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": a.id, "name": crate::views::sound_name(&a.file),
        "file": a.file, "duration_s": a.duration_s, "provider": a.provider, "license": a.license,
        "source": a.source, "category": a.category, "peaks": peaks,
        "tags": a.tags,
    })
}

/// All buckets (or a scene's), sorted by scene, tag (name) or show (widest first).
pub fn view(svc: &mut Service, scene: Option<&str>, sort: &str) -> Result<Value> {
    svc.bus.refresh()?;
    let doc = svc.doc().clone();
    let sid = match scene {
        Some(k) => Some(
            doc.find_scene(k)
                .map(|s| s.id.clone())
                .ok_or_else(|| AppError::Params(format!("no scene '{k}'")))?,
        ),
        None => None,
    };
    let mut list: Vec<&SoundBucket> = doc
        .buckets
        .values()
        .filter(|b| !crate::studio::is_ambience_bucket(svc, b))
        // a bucket of no scene is for the whole show, so every scene can use it
        .filter(|b| {
            sid.as_ref()
                .is_none_or(|s| b.scene_ids.contains(s) || b.scene_ids.is_empty())
        })
        .collect();
    let matched = crate::studio::matches(svc, sid.as_deref());
    let first_scene = |b: &SoundBucket| {
        b.scene_ids
            .iter()
            .filter_map(|s| doc.scenes.get(s))
            .map(|s| s.number)
            .min()
            .unwrap_or(u32::MAX)
    };
    match sort {
        "tag" => list.sort_by_key(|b| b.name.to_lowercase()),
        "show" => {
            list.sort_by_key(|b| (std::cmp::Reverse(b.scene_ids.len()), b.name.to_lowercase()))
        }
        _ => list.sort_by_key(|b| (first_scene(b), b.order)),
    }
    let mut out = Vec::new();
    for b in list {
        let mut sounds: Vec<Asset> = b
            .kept
            .iter()
            .filter_map(|id| doc.assets.get(id))
            .cloned()
            .collect();
        // in a show, the files in the bucket's folder are its sounds too
        let loose = crate::studio::is_loose_bucket(svc, b);
        for a in crate::studio::folder_sounds(svc, b) {
            // a loose sound that matched shows in the bucket that wanted it, and an ambience
            // is a room's, in Import › Rooms
            if loose && (matched.claimed(&a.id) || a.loops_by_default()) {
                continue;
            }
            if !sounds.iter().any(|x| x.id == a.id || x.hash == a.hash) {
                sounds.push(a);
            }
        }
        let own = sounds.len();
        // and so are the sounds whose file names answer its cues
        for a in matched.of(&b.id) {
            if !sounds.iter().any(|x| x.id == a.id || x.hash == a.hash) {
                sounds.push(a.clone());
            }
        }
        let show_wide = b.scene_ids.is_empty();
        if sounds.is_empty() && (loose || (sid.is_some() && show_wide)) {
            continue;
        }
        let kept: Vec<Value> = sounds
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let mut v = asset_view(svc, a);
                v["matched"] = json!(i >= own);
                v["loop"] = json!(loops(&doc, Some(&b.id), a));
                v
            })
            .collect();
        let previews: Vec<Value> = b
            .previews
            .iter()
            .map(|p| {
                let mut v = json!(p);
                v["loop"] = json!(b.loops.get(&p.remote_id).copied().unwrap_or(false));
                v
            })
            .collect();
        out.push(json!({
            "id": b.id, "name": crate::studio::shown_name(b), "keywords": b.keywords, "scenes": scene_labels(&doc, b), "scene_ids": b.scene_ids,
            "kept": kept, "previews": previews, "length_min_s": b.length_min_s, "length_max_s": b.length_max_s,
            "exclude": b.exclude, "ai_suggested": b.ai_suggested, "folder": b.folder,
            "show_wide": show_wide,
            "anchors": b.cue_ids.iter().filter_map(|c| doc.cues.get(c)).filter_map(|c| c.anchor.clone()).collect::<Vec<_>>(),
        }));
    }
    let epidemic = svc.api_key(KeyedService::Epidemic).is_some();
    let elevenlabs = svc.api_key(KeyedService::Elevenlabs).is_some();
    let (scenes, pending) = spot_status(&doc);
    let loose = svc.show_lib.as_ref().map_or(0, |l| {
        l.lock()
            .map_or(0, |l| l.direct(&l.loose_folder(false)).len())
    });
    Ok(json!({
        "buckets": out,
        "loose": loose,
        "show": svc.paths.show.as_ref().map(|s| s.name().to_string()),
        "connected": { "epidemic": epidemic, "elevenlabs": elevenlabs },
        "spot": { "scenes": scenes.len(), "pending": pending.len() },
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    svc: &mut Service,
    name: &str,
    keywords: Vec<String>,
    scenes: Vec<String>,
    length_min_s: Option<f64>,
    length_max_s: Option<f64>,
    exclude: Vec<String>,
    previews: Vec<Preview>,
    music: bool,
) -> Result<String> {
    if name.trim().is_empty() {
        return Err(AppError::Params("a bucket needs a name".into()));
    }
    // in a show a bucket made by hand is a folder of the library at once (D65)
    let folder = match &svc.show_lib {
        Some(lib) => {
            let l = lib
                .lock()
                .map_err(|_| AppError::Other("library lock poisoned".into()))?;
            let existing = crate::studio::match_folder(&l.folders(), name, music).filter(|f| {
                apc_sound::show_library::folder_slug(f.rsplit('/').next().unwrap_or(""))
                    == apc_sound::show_library::folder_slug(name)
            });
            let f = existing.unwrap_or_else(|| l.folder_for(name, music));
            if let Some(b) = svc
                .doc()
                .buckets
                .values()
                .find(|b| b.folder.as_deref() == Some(f.as_str()))
            {
                return Err(AppError::Params(format!(
                    "the bucket “{}” is already the folder {f}",
                    b.name
                )));
            }
            l.make_folder(&f)?;
            Some(f)
        }
        None => None,
    };
    let scene_ids = scenes
        .iter()
        .map(|k| {
            svc.doc()
                .find_scene(k)
                .map(|s| s.id.clone())
                .ok_or_else(|| AppError::Params(format!("no scene '{k}'")))
        })
        .collect::<Result<Vec<_>>>()?;
    let b = SoundBucket {
        id: new_id("bk"),
        name: name.trim().to_string(),
        keywords: keywords
            .into_iter()
            .map(|k| k.trim().to_lowercase())
            .filter(|k| !k.is_empty())
            .collect(),
        scene_ids,
        cue_ids: vec![],
        kept: vec![],
        previews: vec![],
        dismissed: vec![],
        length_min_s,
        length_max_s,
        exclude,
        ai_suggested: false,
        order: svc.doc().buckets.len() as u32,
        search_offset: 0,
        folder,
        loops: Default::default(),
    };
    let mut b = b;
    for p in previews {
        if !b.previews.iter().any(|x| x.remote_id == p.remote_id) {
            b.previews.push(Preview { pinned: true, ..p });
        }
    }
    let id = b.id.clone();
    svc.exec(&Command::PutBucket { bucket: b })?;
    Ok(id)
}

pub fn update(svc: &mut Service, p: crate::methods::UpdateBucketP) -> Result<Value> {
    let scene_ids = match &p.scenes {
        Some(sc) => Some(
            sc.iter()
                .map(|k| {
                    svc.doc()
                        .find_scene(k)
                        .map(|s| s.id.clone())
                        .ok_or_else(|| AppError::Params(format!("no scene '{k}'")))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        None => None,
    };
    let b = edit_bucket(svc, &p.bucket_id, |_, b| {
        if let Some(n) = p.name.as_ref().filter(|n| !n.trim().is_empty()) {
            b.name = n.trim().to_string();
        }
        if let Some(k) = &p.keywords {
            b.keywords = normal_keywords(k);
        }
        if let Some(ids) = &scene_ids {
            b.scene_ids = ids.clone();
        }
        if let Some(v) = p.length_min_s {
            b.length_min_s = v;
        }
        if let Some(v) = p.length_max_s {
            b.length_max_s = v;
        }
        if let Some(x) = &p.exclude {
            b.exclude = x.clone();
        }
    })?;
    Ok(json!({ "id": b.id }))
}

fn normal_keywords(k: &[String]) -> Vec<String> {
    k.iter()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .collect()
}

fn bucket(svc: &Service, id: &str) -> Result<SoundBucket> {
    svc.doc()
        .buckets
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no bucket '{id}'")))
}

/// Searches a provider with the bucket's keywords. Epidemic results become previews (more
/// pages with `more`); library results are returned to pick from.
pub fn search(
    svc: &mut Service,
    bucket_id: &str,
    provider: &str,
    more: bool,
    query: Option<Vec<String>>,
) -> Result<Value> {
    let snap = bucket(svc, bucket_id)?;
    let new_keywords = query.as_deref().map(normal_keywords);
    let keywords = new_keywords
        .clone()
        .unwrap_or_else(|| snap.keywords.clone());
    let term = keywords.join(" ");
    if term.trim().is_empty() {
        return Err(AppError::Params("add keywords first".into()));
    }
    match provider {
        "library" => {
            let lib = svc.library();
            let local: Vec<Asset> = svc.doc().assets.values().cloned().collect();
            let in_folder: Vec<String> = crate::studio::bucket_sounds(svc, &snap)
                .into_iter()
                .map(|a| a.id)
                .collect();
            let mut hits: Vec<Asset> = Vec::new();
            for k in &keywords {
                for a in lib.search(k, &local)? {
                    if !hits.iter().any(|h| h.id == a.id)
                        && !snap.kept.contains(&a.id)
                        && !in_folder.contains(&a.id)
                    {
                        hits.push(a);
                    }
                }
            }
            drop(lib);
            if let Some(k) = &new_keywords {
                edit_bucket(svc, bucket_id, |_, b| b.keywords = k.clone())?;
            }
            let views: Vec<Value> = hits.iter().map(|a| asset_view(svc, a)).collect();
            Ok(json!({ "library": views }))
        }
        _ => {
            let ep = EpidemicProvider::new(svc.api_key(KeyedService::Epidemic));
            // the network part works on a copy; the result is applied to the bucket as it
            // is by then (a spotting job may have changed it meanwhile)
            let mut work = snap.clone();
            if !more {
                work.previews.retain(|p| p.pinned);
                work.search_offset = 0;
            }
            let folder_remote = crate::studio::folder_remote(svc, &snap);
            let mut kept_remote = kept_remote_of(svc.doc(), &work);
            kept_remote.extend(folder_remote.iter().cloned());
            let mut found: Vec<Preview> = Vec::new();
            let mut exhausted = false;
            // read on until a page is filled: results the filters rule out don't count
            'pages: for _ in 0..4 {
                let ask = PAGE * 3;
                let results = ep.search_page(&term, ask, work.search_offset)?;
                let got = results.len();
                for r in results {
                    work.search_offset += 1;
                    if let Some(p) = keep_result(&work, &kept_remote, r) {
                        work.previews.push(p.clone());
                        found.push(p);
                        if found.len() >= PAGE {
                            break 'pages;
                        }
                    }
                }
                if got < ask {
                    exhausted = true;
                    break;
                }
            }
            let offset = work.search_offset;
            let b = edit_bucket(svc, bucket_id, |doc, b| {
                if let Some(k) = &new_keywords {
                    b.keywords = k.clone();
                }
                if !more {
                    b.previews.retain(|p| p.pinned);
                }
                let mut kept_remote = kept_remote_of(doc, b);
                kept_remote.extend(folder_remote.iter().cloned());
                for p in &found {
                    if admits(b, &kept_remote, &p.remote_id) {
                        b.previews.push(p.clone());
                    }
                }
                b.search_offset = offset;
            })?;
            Ok(json!({ "added": found.len(), "previews": b.previews, "more": !exhausted }))
        }
    }
}

/// Provider ids of the sounds a bucket keeps.
fn kept_remote_of(doc: &Document, b: &SoundBucket) -> Vec<String> {
    b.kept
        .iter()
        .filter_map(|id| doc.assets.get(id))
        .filter_map(|a| a.remote_id.clone())
        .collect()
}

/// Not dismissed, not kept and not shown yet.
fn admits(b: &SoundBucket, kept_remote: &[String], rid: &str) -> bool {
    !b.dismissed.iter().any(|d| d == rid)
        && !kept_remote.iter().any(|k| k == rid)
        && !b.previews.iter().any(|p| p.remote_id == rid)
}

/// A search result as a new preview, unless the bucket's filters or earlier choices rule
/// it out (left-out words, length, dismissed, kept, already shown).
fn keep_result(
    b: &SoundBucket,
    kept_remote: &[String],
    r: apc_sound::SoundCandidate,
) -> Option<Preview> {
    let rid = r.remote_id?;
    let title = r.title.to_lowercase();
    let excluded = b
        .exclude
        .iter()
        .any(|x| !x.is_empty() && title.contains(&x.to_lowercase()));
    let too_short = b
        .length_min_s
        .map(|m| r.duration_s > 0.0 && r.duration_s < m)
        .unwrap_or(false);
    let too_long = b.length_max_s.map(|m| r.duration_s > m).unwrap_or(false);
    if excluded || too_short || too_long || !admits(b, kept_remote, &rid) {
        return None;
    }
    Some(Preview {
        provider: "epidemic".into(),
        remote_id: rid,
        title: r.title,
        duration_s: r.duration_s,
        pinned: false,
    })
}

/// Free search of Epidemic Sound (the search bar of Import › Sounds): results to audition
/// and drag into a bucket. Nothing is stored or downloaded.
pub fn search_free(svc: &mut Service, query: &str, offset: usize) -> Result<Value> {
    let term = query.trim();
    if term.is_empty() {
        return Err(AppError::Params("type something to search for".into()));
    }
    let ep = EpidemicProvider::new(svc.api_key(KeyedService::Epidemic));
    // one extra result says whether there is a next page
    let mut results = ep.search_page(term, FREE_PAGE + 1, offset)?;
    let more = results.len() > FREE_PAGE;
    results.truncate(FREE_PAGE);
    let doc = svc.doc();
    let previews: Vec<Value> = results
        .into_iter()
        .filter_map(|r| {
            let rid = r.remote_id?;
            // where it already is, so a drop can say so
            let buckets: Vec<String> = doc
                .buckets
                .values()
                .filter(|b| {
                    b.previews.iter().any(|p| p.remote_id == rid)
                        || b.kept.iter().any(|id| {
                            doc.assets.get(id).and_then(|a| a.remote_id.as_deref())
                                == Some(rid.as_str())
                        })
                })
                .map(|b| b.id.clone())
                .collect();
            Some(json!({
                "provider": "epidemic", "remote_id": rid, "title": r.title,
                "duration_s": r.duration_s, "buckets": buckets,
            }))
        })
        .collect();
    Ok(json!({ "results": previews, "next_offset": offset + FREE_PAGE, "more": more }))
}

/// Adds a search result to a bucket by hand (dropped in from the free search). It stays a
/// preview (nothing is downloaded) and survives the bucket's own searches.
pub fn add_preview(svc: &mut Service, bucket_id: &str, p: Preview) -> Result<Value> {
    let snap = bucket(svc, bucket_id)?;
    let folder_remote = crate::studio::folder_remote(svc, &snap);
    let added = Cell::new(false);
    edit_bucket(svc, bucket_id, |doc, b| {
        added.set(false);
        if kept_remote_of(doc, b).contains(&p.remote_id) || folder_remote.contains(&p.remote_id) {
            return;
        }
        b.dismissed.retain(|d| d != &p.remote_id);
        match b.previews.iter_mut().find(|x| x.remote_id == p.remote_id) {
            Some(x) => x.pinned = true,
            None => b.previews.insert(
                0,
                Preview {
                    pinned: true,
                    ..p.clone()
                },
            ),
        }
        added.set(true);
    })?;
    Ok(if added.get() {
        json!({ "added": true })
    } else {
        json!({ "added": false, "reason": "already kept in this bucket" })
    })
}

/// Downloads a preview into the library (licensed) and keeps it in the bucket.
pub fn download(
    svc: &mut Service,
    bucket_id: &str,
    remote_id: &str,
    progress: &dyn Fn(f64),
) -> Result<Asset> {
    let b = bucket(svc, bucket_id)?;
    let dest = crate::studio::bucket_dest(svc, bucket_id)?;
    // downloaded before (in this episode, or anywhere in the show): no second download
    let known = svc
        .doc()
        .assets
        .values()
        .find(|a| a.remote_id.as_deref() == Some(remote_id))
        .cloned()
        .or_else(|| {
            svc.show_lib
                .as_ref()
                .and_then(|l| l.lock().ok()?.by_remote(remote_id).map(|e| e.asset.clone()))
        });
    if let Some(a) = known {
        let a = match &dest {
            // a copy in the bucket's folder, so the bucket holds it in every episode
            Some(d) => crate::sound::import_into(svc, &svc.library().path_of(&a), d, &a)?,
            None => a,
        };
        crate::sound::ensure_asset_in_doc(svc, &a.id)?;
        svc.exec(&Command::BucketKeep {
            bucket_id: b.id.clone(),
            asset_id: a.id.clone(),
        })?;
        return Ok(a);
    }
    let title = b
        .previews
        .iter()
        .find(|p| p.remote_id == remote_id)
        .map(|p| p.title.clone())
        .unwrap_or_else(|| remote_id.to_string());
    let ep = EpidemicProvider::new(svc.api_key(KeyedService::Epidemic));
    let file = ep.download_with_progress(
        remote_id,
        &title,
        &svc.paths.cache.join("downloads"),
        progress,
    )?;
    let a = crate::sound::import_licensed(svc, &file, remote_id, Some(b.name.clone()), dest)?;
    svc.exec(&Command::BucketKeep {
        bucket_id: b.id.clone(),
        asset_id: a.id.clone(),
    })?;
    Ok(a)
}

/// ElevenLabs: generates variations from a prompt (or the keywords) and keeps them.
pub fn generate(
    svc: &mut Service,
    bucket_id: &str,
    prompt: Option<String>,
    variations: Option<u32>,
) -> Result<Value> {
    let b = bucket(svc, bucket_id)?;
    let prompt = prompt
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| b.keywords.join(", "));
    let dest = crate::studio::bucket_dest(svc, bucket_id)?;
    let r = crate::sound::generate(svc, None, Some(prompt), variations, None, dest)?;
    let ids: Vec<String> = r["assets"]
        .as_array()
        .map(|v| {
            v.iter()
                .filter_map(|a| a["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    for id in &ids {
        svc.exec(&Command::BucketKeep {
            bucket_id: bucket_id.to_string(),
            asset_id: id.clone(),
        })?;
    }
    Ok(json!({ "kept": ids }))
}

/// What can loop under a room: the ambience buckets' sounds, those a scene's cues asked for
/// first, then every other ambience in the library.
pub fn ambience(svc: &mut Service, scene: Option<&str>) -> Result<Value> {
    svc.bus.refresh()?;
    let doc = svc.doc().clone();
    let sid = scene.and_then(|k| doc.find_scene(k)).map(|s| s.id.clone());
    let row = |a: &Asset| json!({ "id": a.id, "name": crate::views::sound_name(&a.file), "duration_s": a.duration_s });
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut groups: Vec<(bool, Value)> = Vec::new();
    let mut buckets: Vec<&SoundBucket> = doc.buckets.values().collect();
    buckets.sort_by_key(|b| (b.order, b.id.clone()));
    for b in buckets {
        if !crate::studio::is_ambience_bucket(svc, b) {
            continue;
        }
        let mut sounds: Vec<Asset> = b
            .kept
            .iter()
            .filter_map(|id| doc.assets.get(id).cloned())
            .collect();
        sounds.extend(crate::studio::bucket_sounds(svc, b));
        let rows: Vec<Value> = sounds
            .iter()
            .filter(|a| seen.insert(a.id.clone()))
            .map(row)
            .collect();
        if rows.is_empty() {
            continue;
        }
        let here = sid.as_ref().is_some_and(|s| b.scene_ids.contains(s));
        groups.push((
            here,
            json!({ "name": b.name, "here": here, "sounds": rows }),
        ));
    }
    let local: Vec<Asset> = doc.assets.values().cloned().collect();
    let rest: Vec<Value> = svc
        .library()
        .list(&local)?
        .iter()
        .filter(|a| a.loops_by_default() && seen.insert(a.id.clone()))
        .map(row)
        .collect();
    if !rest.is_empty() {
        groups.push((
            false,
            json!({ "name": "Ambience", "here": false, "sounds": rest }),
        ));
    }
    groups.sort_by_key(|(here, _)| !here);
    Ok(json!(groups
        .into_iter()
        .map(|(_, g)| g)
        .collect::<Vec<_>>()))
}

/// A file for a room's ambience: into the show's room tone folder, and straight into the room.
pub fn import_ambience(svc: &mut Service, p: &crate::methods::ImportAmbienceP) -> Result<Value> {
    let bucket = svc
        .doc()
        .buckets
        .values()
        .find(|b| {
            b.folder
                .as_deref()
                .is_some_and(|f| f.ends_with("room-tone"))
        })
        .map(|b| b.id.clone());
    let r = crate::sound::import(svc, std::slice::from_ref(&p.path), None, None, None, bucket)?;
    let id = r["assets"][0]["id"]
        .as_str()
        .ok_or_else(|| AppError::Params(format!("no sound in '{}'", p.path)))?
        .to_string();
    let Some(scene) = &p.scene else {
        return Ok(json!({ "asset_id": id }));
    };
    crate::sound_edit::set_tone(
        svc,
        crate::methods::SceneToneP {
            scene: scene.clone(),
            asset_id: Some(id),
            gain_db: None,
            night: p.night,
        },
    )
}

/// Does this sound loop? What its bucket says, else what kind of sound it is.
fn loops(doc: &Document, bucket_id: Option<&str>, a: &Asset) -> bool {
    bucket_id
        .and_then(|b| doc.buckets.get(b))
        .and_then(|b| {
            b.loops
                .get(&a.id)
                .or_else(|| a.remote_id.as_ref().and_then(|r| b.loops.get(r)))
        })
        .copied()
        .unwrap_or_else(|| a.loops_by_default())
}

/// Sets Loop on a bucket's sound or preview.
pub fn set_loop(svc: &mut Service, p: crate::methods::BucketLoopP) -> Result<Value> {
    let id = p
        .asset_id
        .or(p.remote_id)
        .ok_or_else(|| AppError::Params("give asset_id or remote_id".into()))?;
    edit_bucket(svc, &p.bucket_id, |_, b| {
        b.loops.insert(id.clone(), p.on);
    })?;
    Ok(json!({ "loop": p.on }))
}

/// Auditions a bucket sound: a kept asset, or a preview (fetched into the cache).
pub fn play(
    svc: &mut Service,
    asset_id: Option<String>,
    remote_id: Option<String>,
    clip: Option<crate::methods::ClipP>,
    from_s: f64,
) -> Result<Value> {
    if let Some(c) = clip {
        return crate::player::play_stretch(svc, &c, from_s);
    }
    if let Some(aid) = asset_id {
        let a = crate::sound::find_asset(svc, &aid)?
            .ok_or_else(|| AppError::Params(format!("no sound '{aid}'")))?;
        let path = svc.library().path_of(&a);
        return crate::player::play_file(svc, &path, &aid, from_s);
    }
    let rid = remote_id.ok_or_else(|| AppError::Params("give asset_id or remote_id".into()))?;
    let ep = EpidemicProvider::new(svc.api_key(KeyedService::Epidemic));
    let file = ep.preview_file(&rid, &svc.paths.cache.join("previews"))?;
    crate::player::play_file(svc, &file, &rid, from_s)
}

/// Places a bucket sound on the timeline. A preview downloads first (a background job with
/// progress, "downloads on drop"); a kept sound is placed at once.
pub fn place(svc: &mut Service, q: crate::methods::PlaceFromBucketP) -> Result<Value> {
    let crate::methods::PlaceFromBucketP {
        scene,
        asset_id,
        remote_id,
        bucket_id,
        cue_id,
        lane,
        character,
        at_s,
        anchor_line_id,
        anchor_word,
        anchor_edge,
        offset_s,
        glue,
        depth,
    } = q;
    let p = crate::methods::PlaceSoundP {
        asset_id: String::new(),
        scene,
        cue_id,
        lane,
        character,
        anchor_line_id,
        anchor_word,
        anchor_edge,
        at_s,
        offset_s,
        glue,
        depth,
        dropped: None,
    };
    if let Some(aid) = asset_id {
        let a = crate::sound::find_asset(svc, &aid)?;
        let dropped = Some(a.is_some_and(|a| loops(svc.doc(), bucket_id.as_deref(), &a)));
        return crate::sound::place(
            svc,
            crate::methods::PlaceSoundP {
                asset_id: aid,
                dropped,
                ..p
            },
        );
    }
    let rid = remote_id.ok_or_else(|| AppError::Params("give asset_id or remote_id".into()))?;
    let bid = bucket_id.ok_or_else(|| AppError::Params("a preview needs its bucket".into()))?;
    let file = svc.paths.file.clone();
    let secrets = svc.secrets.clone();
    let job = svc.jobs.spawn("download", file, move |svc, progress| {
        svc.secrets = secrets;
        let a = download(svc, &bid, &rid, &|f| progress(f * 0.95, "downloading"))?;
        let r = crate::sound::place(
            svc,
            crate::methods::PlaceSoundP {
                asset_id: a.id.clone(),
                dropped: Some(loops(svc.doc(), Some(&bid), &a)),
                ..p
            },
        )?;
        Ok(json!({ "asset_id": a.id, "placement": r }))
    });
    Ok(json!({ "job_id": job }))
}

/// Takes a sound of the bucket's folder out of this episode's bucket: hidden here, the
/// file stays in the show's library for the other episodes.
pub fn hide(svc: &mut Service, bucket_id: &str, asset_id: &str) -> Result<Value> {
    edit_bucket(svc, bucket_id, |_, b| {
        b.kept.retain(|a| a != asset_id);
        if !b.dismissed.iter().any(|d| d == asset_id) {
            b.dismissed.push(asset_id.to_string());
        }
    })?;
    Ok(json!({ "hidden": asset_id }))
}

/// Shows a hidden folder sound again (it was kept by hand).
pub fn unhide(svc: &mut Service, bucket_id: &str, asset_id: &str) -> Result<()> {
    if bucket(svc, bucket_id)?
        .dismissed
        .iter()
        .any(|d| d == asset_id)
    {
        edit_bucket(svc, bucket_id, |_, b| b.dismissed.retain(|d| d != asset_id))?;
    }
    Ok(())
}
