//! The Export page's methods: what's been exported, what an export will do, doing it, scene
//! locks, presets, and the two destinations. The files themselves are `apc_export::package`.

use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{json, Value};

use apc_core::commands::Command;
use apc_core::config::{ExportMode, ExportPreset, ExportSettings, ProjectConfig, SaveTo};
use apc_core::project::EpisodePaths;
use apc_core::show::Show;
use apc_core::store::EpisodeStore;
use apc_core::{CoreError, Document, Scene, SceneStatus};
use apc_export::bounce::{ffmpeg_available, schedule_of};
use apc_export::package::{self, has_audio, timeline_hash, Target};
use apc_export::versions::{self, EpisodeTag, ExportRecord, History, ManifestScene};

use crate::error::{AppError, Result};
use crate::methods::{ExportRunP, ExportShowP, PresetP};
use crate::service::Service;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The number an episode's exports carry: the pipeline's, or the one in its name.
fn episode_tag(name: &str, paths: &EpisodePaths) -> EpisodeTag {
    let from_show = paths.show.as_ref().and_then(|s| {
        s.episodes()
            .into_iter()
            .find(|e| e.dir == paths.root)
            .and_then(|e| e.number)
    });
    EpisodeTag(from_show.or_else(|| last_number(name)).unwrap_or(1))
}

fn last_number(name: &str) -> Option<u32> {
    name.split(|c: char| !c.is_ascii_digit())
        .rfind(|p| !p.is_empty())
        .and_then(|p| p.parse().ok())
}

/// The folder a destination's scene folders go in: straight in the episode's own `Exports`, or
/// in the pipeline's under the episode, because that one holds them all. The pipeline needs its
/// confirmed folder; the episode's is simply there.
fn base_dir(paths: &EpisodePaths, ep: EpisodeTag, to: SaveTo) -> Result<PathBuf> {
    match to {
        SaveTo::Episode => Ok(paths.exports.clone()),
        SaveTo::Pipeline => {
            let show = paths.show.as_ref().ok_or_else(|| {
                AppError::Params(
                    "this episode isn't in a show, so it has no pipeline folder".into(),
                )
            })?;
            Ok(pipeline_exports(show)?.join(ep.folder()))
        }
    }
}

fn pipeline_exports(show: &Show) -> Result<PathBuf> {
    let dir = show.pipeline_dir().ok_or_else(|| {
        AppError::Params(format!(
            "{} has no pipeline folder yet: confirm one first",
            show.name()
        ))
    })?;
    if !dir.is_dir() {
        return Err(AppError::Params(format!(
            "the pipeline folder {} isn't there",
            dir.display()
        )));
    }
    Ok(exports_in(&dir))
}

/// The `Exports` folder inside a pipeline folder, which may be one itself: choosing
/// `Sound/Exports` shouldn't make `Sound/Exports/Exports`.
fn exports_in(dir: &Path) -> PathBuf {
    if dir.file_name().is_some_and(|n| n == "Exports") {
        dir.to_path_buf()
    } else {
        dir.join("Exports")
    }
}

/// Every version of the episode, in both destinations.
fn history(paths: &EpisodePaths, ep: EpisodeTag) -> History {
    let mut records = versions::scan(&paths.exports);
    if let Some(dir) = paths.show.as_ref().and_then(Show::pipeline_dir) {
        records.extend(versions::scan(&exports_in(&dir).join(ep.folder())));
    }
    History::new(records)
}

/// Has anything of this episode been exported, in either destination?
pub fn any_export(paths: &EpisodePaths, name: &str) -> bool {
    !versions::scan(&paths.exports).is_empty()
        || paths
            .show
            .as_ref()
            .and_then(Show::pipeline_dir)
            .is_some_and(|d| {
                let ep = episode_tag(name, paths);
                !versions::scan(&exports_in(&d).join(ep.folder())).is_empty()
            })
}

/// Brings an export folder to the versions/ and current/ layout (D165), when it's there.
fn tidy(base: &Path) -> Result<apc_export::current::Reorganized> {
    if !base.is_dir() {
        return Ok(Default::default());
    }
    Ok(apc_export::current::reorganize(base)?)
}

/// `reorganize_exports`: both destinations of the open episode.
pub fn reorganize(svc: &Service) -> Result<Value> {
    let ep = episode_tag(&svc.name(), &svc.paths);
    let mut out = vec![
        json!({ "folder": svc.paths.exports.to_string_lossy(), "done": tidy(&svc.paths.exports)? }),
    ];
    if let Some(dir) = svc.paths.show.as_ref().and_then(Show::pipeline_dir) {
        let base = exports_in(&dir).join(ep.folder());
        if base != svc.paths.exports {
            out.push(json!({ "folder": base.to_string_lossy(), "done": tidy(&base)? }));
        }
    }
    Ok(json!({ "folders": out }))
}

/// Every version of the open episode.
pub fn episode_history(svc: &Service) -> History {
    history(&svc.paths, episode_tag(&svc.name(), &svc.paths))
}

fn record_view(r: &ExportRecord) -> Value {
    json!({
        "version": r.manifest.version,
        "date": r.manifest.date,
        "destination": r.manifest.destination,
        "folder": r.folder.to_string_lossy(),
        "files": r.manifest.files.len(),
        "bytes": r.manifest.files.iter().map(|f| f.bytes).sum::<u64>(),
        "locked": r.manifest.locked,
    })
}

fn scene_view(doc: &Document, s: &Scene, h: &History) -> Result<Value> {
    let hash = timeline_hash(doc, s)?;
    let records = h.of_scene(&s.id);
    let last = h.last_of(&s.id);
    let unchanged = h.unchanged_since(&s.id, &hash);
    Ok(json!({
        "id": s.id,
        "number": s.number,
        "place": s.place,
        "heading": s.heading,
        "title": s.story.title,
        "status": s.status,
        "length_s": schedule_of(doc, s).end_s(),
        "has_audio": has_audio(doc, s),
        "lock": s.lock,
        "next_version": h.next_for(&s.id),
        "last": last.map(record_view),
        "history": records.iter().map(|r| record_view(r)).collect::<Vec<_>>(),
        "unchanged_since": unchanged,
        "changed_since": last.filter(|_| unchanged.is_none()).map(|r| r.manifest.version),
    }))
}

/// Lanes muted or soloed on the Edit page shape every export, and are easily forgotten.
fn lane_warning(doc: &Document) -> Option<String> {
    let soloed: Vec<&str> = doc
        .lane_mix
        .values()
        .filter(|m| m.solo)
        .map(|m| m.id.as_str())
        .collect();
    let muted: Vec<&str> = doc
        .lane_mix
        .values()
        .filter(|m| m.mute)
        .map(|m| m.id.as_str())
        .collect();
    if !soloed.is_empty() {
        return Some(format!(
            "Only the soloed lanes will be exported: {}.",
            soloed.join(", ")
        ));
    }
    (!muted.is_empty()).then(|| format!("Muted lanes won't be exported: {}.", muted.join(", ")))
}

/// Everything the Export page shows before anything is chosen.
pub fn status(svc: &Service) -> Result<Value> {
    let doc = svc.doc();
    let ep = episode_tag(&svc.name(), &svc.paths);
    let h = history(&svc.paths, ep);
    let scenes = doc
        .scenes_in_order()
        .iter()
        .map(|s| scene_view(doc, s, &h))
        .collect::<Result<Vec<_>>>()?;
    let cfg = ProjectConfig::from_doc(doc);
    Ok(json!({
        "name": svc.name(),
        "episode": ep.folder(),
        "scenes": scenes,
        "episode_versions": h.episodes().into_iter().map(record_view).collect::<Vec<_>>(),
        "pipeline": svc.paths.show.as_ref().map(pipeline_fields),
        "presets": cfg.export.presets,
        "defaults": ExportSettings::default(),
        "warnings": lane_warning(doc)
            .into_iter()
            .chain(crate::review::locked_with_notes(doc))
            .collect::<Vec<_>>(),
        "ffmpeg": ffmpeg_available(),
    }))
}

fn pipeline_fields(show: &Show) -> Value {
    let path = show.pipeline_dir();
    let proposal = path
        .is_none()
        .then(|| show.propose_pipeline_folder())
        .flatten();
    json!({
        "show": show.name(),
        "path": path.map(|p| p.to_string_lossy().into_owned()),
        "proposal": proposal.map(|p| p.to_string_lossy().into_owned()),
    })
}

pub fn pipeline_view(show: &Show) -> Result<Value> {
    Ok(pipeline_fields(show))
}

/// The settings asked for: a preset by name, or the settings given, or the page's defaults.
fn settings_of(
    doc: &Document,
    preset: Option<&str>,
    settings: Option<ExportSettings>,
) -> Result<ExportSettings> {
    if let Some(name) = preset {
        return ProjectConfig::from_doc(doc)
            .export
            .presets
            .into_iter()
            .find(|p| p.name == name)
            .map(|p| p.settings)
            .ok_or_else(|| AppError::Params(format!("no preset called '{name}'")));
    }
    Ok(settings.unwrap_or_default())
}

/// What's been asked for, checked: settings and scenes in script order.
pub fn resolve<'a>(svc: &'a Service, p: &ExportRunP) -> Result<(ExportSettings, Vec<&'a Scene>)> {
    let doc = svc.doc();
    let settings = settings_of(doc, p.preset.as_deref(), p.settings)?;
    deliverable(&settings)?;
    base_dir(
        &svc.paths,
        episode_tag(&svc.name(), &svc.paths),
        settings.save_to,
    )?;
    let scenes = pick_scenes(doc, p.scenes.as_deref(), p.status.as_deref())?;
    Ok((settings, scenes))
}

/// `check`'s words go straight to the person exporting.
fn deliverable(s: &ExportSettings) -> Result<()> {
    s.check().map_err(|e| match e {
        CoreError::Invalid(m) => AppError::Other(m),
        e => e.into(),
    })
}

/// The scenes asked for by number or id, of those with one of the statuses, in script order.
fn pick_scenes<'a>(
    doc: &'a Document,
    keys: Option<&[String]>,
    status: Option<&[SceneStatus]>,
) -> Result<Vec<&'a Scene>> {
    let named = named_scenes(doc, keys)?;
    let Some(status) = status else {
        return Ok(named);
    };
    let marked: Vec<&Scene> = named
        .into_iter()
        .filter(|s| s.status.is_some_and(|x| status.contains(&x)))
        .collect();
    if marked.is_empty() {
        return Err(AppError::Params("no scenes are marked that way".into()));
    }
    Ok(marked)
}

fn named_scenes<'a>(doc: &'a Document, keys: Option<&[String]>) -> Result<Vec<&'a Scene>> {
    let all = doc.scenes_in_order();
    let Some(keys) = keys else {
        return Ok(all);
    };
    if keys.is_empty() {
        return Err(AppError::Params("no scenes to export".into()));
    }
    let mut wanted = Vec::new();
    for k in keys {
        let s = doc
            .find_scene(k)
            .ok_or_else(|| AppError::Params(format!("no scene '{k}'")))?;
        wanted.push(s.id.as_str());
    }
    Ok(all
        .into_iter()
        .filter(|s| wanted.contains(&s.id.as_str()))
        .collect())
}

/// What an export will write.
pub fn plan(svc: &Service, p: &ExportRunP) -> Result<Value> {
    let (settings, scenes) = resolve(svc, p)?;
    let ep = episode_tag(&svc.name(), &svc.paths);
    let base = base_dir(&svc.paths, ep, settings.save_to)?;
    let title = svc.name();
    let t = Target {
        doc: svc.doc(),
        paths: &svc.paths,
        ep,
        base: &base,
        settings,
        now: Local::now(),
        app_version: APP_VERSION,
        title: &title,
    };
    let h = history(&svc.paths, ep);
    Ok(serde_json::to_value(package::plan(
        &t,
        &scenes,
        whole_episode(p),
        &h,
    )?)?)
}

/// Nothing narrowed the range: the episode gets a version of its own beside its scenes'.
fn whole_episode(p: &ExportRunP) -> bool {
    p.scenes.is_none() && p.status.is_none()
}

/// The range as one mix, in one episode version. The scenes get no versions of their own, so
/// their locks name none, and a lock they already have keeps the version it names.
fn run_one_file(
    svc: &mut Service,
    settings: ExportSettings,
    scenes: Vec<Scene>,
    progress: &dyn Fn(f64, &str),
) -> Result<Value> {
    let ep = episode_tag(&svc.name(), &svc.paths);
    let base = base_dir(&svc.paths, ep, settings.save_to)?;
    tidy(&base)?;
    let h = history(&svc.paths, ep);
    let title = svc.name();
    let now = Local::now();
    let (heard, empty): (Vec<Scene>, Vec<Scene>) =
        scenes.into_iter().partition(|s| has_audio(svc.doc(), s));
    if heard.is_empty() {
        return Err(AppError::Other("these scenes have nothing in them".into()));
    }
    let refs: Vec<&Scene> = heard.iter().collect();
    let locked = settings.lock_after || heard.iter().all(|s| s.lock.is_some());
    let t = Target {
        doc: svc.doc(),
        paths: &svc.paths,
        ep,
        base: &base,
        settings,
        now,
        app_version: APP_VERSION,
        title: &title,
    };
    let record = package::write_one_file(&t, &refs, h.next_episode(), locked, progress)?;
    let mut warnings = record.manifest.warnings.clone();
    if let Some(v) = h.one_file_unchanged_since(&record.manifest.timeline_hash) {
        warnings.push(format!("Unchanged since v{v:03}."));
    }
    if settings.lock_after {
        let at = now.format("%Y-%m-%d").to_string();
        let commands = heard
            .iter()
            .filter(|s| s.lock.is_none())
            .map(|s| Command::LockScene {
                scene_id: s.id.clone(),
                version: None,
                at: at.clone(),
            })
            .collect();
        svc.exec(&Command::Batch { commands })?;
    }
    Ok(json!({
        "scenes": [],
        "episode": record_view(&record),
        "unchanged": [],
        "empty": empty.iter().map(|s| s.number).collect::<Vec<_>>(),
        "locked": settings.lock_after,
        "files": record.manifest.files.len() + 1,
        "bytes": record.manifest.files.iter().map(|f| f.bytes).sum::<u64>(),
        "warnings": warnings,
    }))
}

/// Runs an export. What finished stays and is locked, even when a later scene fails: those
/// versions are complete, and the error says what's missing.
pub fn run(svc: &mut Service, p: &ExportRunP, progress: &dyn Fn(f64, &str)) -> Result<Value> {
    let (settings, scenes) = resolve(svc, p)?;
    let scenes: Vec<Scene> = scenes.into_iter().cloned().collect();
    if settings.mode == ExportMode::OneFile {
        return run_one_file(svc, settings, scenes, progress);
    }
    let ep = episode_tag(&svc.name(), &svc.paths);
    let base = base_dir(&svc.paths, ep, settings.save_to)?;
    // an older layout first, so a new version never lands beside old ones
    tidy(&base)?;
    let h = history(&svc.paths, ep);
    let title = svc.name();
    let now = Local::now();
    let mut done: Vec<ExportRecord> = Vec::new();
    let mut unchanged: Vec<Value> = Vec::new();
    let mut skipped: Vec<u32> = Vec::new();
    let mut failure = None;
    {
        let t = Target {
            doc: svc.doc(),
            paths: &svc.paths,
            ep,
            base: &base,
            settings,
            now,
            app_version: APP_VERSION,
            title: &title,
        };
        let total = scenes.len().max(1) as f64;
        for (i, scene) in scenes.iter().enumerate() {
            if !has_audio(t.doc, scene) {
                skipped.push(scene.number);
                continue;
            }
            let version = h.next_for(&scene.id);
            let locked = settings.lock_after || scene.lock.is_some();
            if let Some(v) = h.unchanged_since(&scene.id, &timeline_hash(t.doc, scene)?) {
                unchanged.push(json!({ "number": scene.number, "since": v }));
            }
            let step = |f: f64, what: &str| {
                progress(
                    (i as f64 + f) / total,
                    &format!("{} · {what}", scene.label()),
                );
            };
            match package::write_scene(&t, scene, version, locked, &step) {
                Ok(r) => done.push(r),
                Err(e) => {
                    failure = Some(format!("{}: {e}", scene.label()));
                    break;
                }
            }
        }
    }
    if settings.lock_after && !done.is_empty() {
        let at = now.format("%Y-%m-%d").to_string();
        let commands = done
            .iter()
            .filter_map(|r| {
                Some(Command::LockScene {
                    scene_id: r.manifest.scene_id.clone()?,
                    version: Some(r.manifest.version),
                    at: at.clone(),
                })
            })
            .collect();
        svc.exec(&Command::Batch { commands })?;
    }
    if let Some(f) = failure {
        let finished: Vec<String> = done
            .iter()
            .map(|r| {
                format!(
                    "Sc. {} v{:03}",
                    r.manifest.scene_number.unwrap_or(0),
                    r.manifest.version
                )
            })
            .collect();
        return Err(AppError::Other(if finished.is_empty() {
            f
        } else {
            format!("{f} ({} finished)", finished.join(", "))
        }));
    }
    let mut episode = None;
    if whole_episode(p) {
        let t = Target {
            doc: svc.doc(),
            paths: &svc.paths,
            ep,
            base: &base,
            settings,
            now,
            app_version: APP_VERSION,
            title: &title,
        };
        let held: Vec<ManifestScene> = done
            .iter()
            .filter_map(|r| {
                Some(ManifestScene {
                    scene_id: r.manifest.scene_id.clone()?,
                    number: r.manifest.scene_number?,
                    version: r.manifest.version,
                })
            })
            .collect();
        episode = Some(package::write_episode(&t, h.next_episode(), &held)?);
    }
    let warnings: Vec<String> = done
        .iter()
        .flat_map(|r| {
            let n = r.manifest.scene_number.unwrap_or(0);
            r.manifest
                .warnings
                .iter()
                .map(move |w| format!("Sc. {n}: {w}"))
        })
        .collect();
    Ok(json!({
        "scenes": done.iter().map(|r| json!({
            "scene_id": r.manifest.scene_id,
            "number": r.manifest.scene_number,
            "version": r.manifest.version,
            "folder": r.folder.to_string_lossy(),
            "files": r.manifest.files.iter().map(|f| &f.name).collect::<Vec<_>>(),
            "frames": r.manifest.frames,
        })).collect::<Vec<_>>(),
        "episode": episode.as_ref().map(record_view),
        "unchanged": unchanged,
        "empty": skipped,
        "locked": settings.lock_after,
        "files": done.iter().map(|r| r.manifest.files.len() + 1).sum::<usize>(),
        "bytes": done.iter().flat_map(|r| &r.manifest.files).map(|f| f.bytes).sum::<u64>(),
        "warnings": warnings,
    }))
}

/// Locks a scene by hand, on its newest version if it has one.
pub fn lock(svc: &mut Service, key: &str) -> Result<Value> {
    let scene = svc
        .doc()
        .find_scene(key)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no scene '{key}'")))?;
    let ep = episode_tag(&svc.name(), &svc.paths);
    let version = history(&svc.paths, ep)
        .last_of(&scene.id)
        .map(|r| r.manifest.version);
    let at = Local::now().format("%Y-%m-%d").to_string();
    Ok(svc
        .exec(&Command::LockScene {
            scene_id: scene.id,
            version,
            at,
        })?
        .result)
}

fn set_presets(svc: &mut Service, presets: Vec<ExportPreset>) -> Result<Value> {
    svc.exec(&Command::SetConfig {
        key: "export.presets".into(),
        value: serde_json::to_value(&presets)?,
    })?;
    Ok(json!({ "presets": presets }))
}

pub fn save_preset(svc: &mut Service, p: PresetP) -> Result<Value> {
    let name = p.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Params("a preset needs a name".into()));
    }
    deliverable(&p.settings)?;
    let mut presets = svc.config().export.presets;
    presets.retain(|x| x.name != name);
    presets.push(ExportPreset {
        name,
        settings: p.settings,
    });
    set_presets(svc, presets)
}

pub fn delete_preset(svc: &mut Service, name: &str) -> Result<Value> {
    let mut presets = svc.config().export.presets;
    let before = presets.len();
    presets.retain(|x| x.name != name);
    if presets.len() == before {
        return Err(AppError::Params(format!("no preset called '{name}'")));
    }
    set_presets(svc, presets)
}

/// The one-time word on opening an episode with locked scenes, after any other warnings.
pub fn with_lock_notice(warnings: &[String], doc: &Document) -> Vec<String> {
    let locks: Vec<_> = doc
        .scenes
        .values()
        .filter_map(|s| s.lock.as_ref())
        .collect();
    let exported = locks.iter().all(|l| l.version.is_some());
    let mut out = warnings.to_vec();
    match (locks.len(), exported) {
        (0, _) => {}
        (1, true) => out.push("1 scene is locked because it was exported.".into()),
        (1, false) => out.push("1 scene is locked.".into()),
        (n, true) => out.push(format!("{n} scenes are locked because they were exported.")),
        (n, false) => out.push(format!("{n} scenes are locked.")),
    }
    out
}

/// A show's episodes as an export reads them: straight from their files, nothing opened for
/// writing, and nothing changed by looking.
fn read_episode(file: &Path) -> Result<(Document, EpisodePaths, String)> {
    let paths = EpisodePaths::for_file(file)?;
    let store = EpisodeStore::open_read_only(file)?;
    let doc = store.load_document()?;
    let name = store.meta("name")?.unwrap_or_default();
    Ok((doc, paths, name))
}

/// Every episode of the show and its scenes.
pub fn show_status(show: &Show) -> Result<Value> {
    let mut episodes = Vec::new();
    for e in show.episodes() {
        let Some(file) = e.file.as_ref() else {
            episodes.push(json!({ "folder": e.folder, "label": e.label, "scenes": [] }));
            continue;
        };
        let (doc, paths, name) = read_episode(file)?;
        let ep = episode_tag(&name, &paths);
        let h = history(&paths, ep);
        let scenes = doc
            .scenes_in_order()
            .iter()
            .map(|s| scene_view(&doc, s, &h))
            .collect::<Result<Vec<_>>>()?;
        episodes.push(json!({
            "folder": e.folder, "label": e.label, "episode": ep.folder(), "scenes": scenes,
        }));
    }
    Ok(json!({
        "name": show.name(),
        "episodes": episodes,
        "pipeline": pipeline_fields(show),
        "defaults": ExportSettings { save_to: SaveTo::Pipeline, ..ExportSettings::default() },
        "ffmpeg": ffmpeg_available(),
    }))
}

/// Refuses a show export that can't go anywhere, before a job starts.
pub fn check_show_export(show: &Show, p: &ExportShowP) -> Result<()> {
    pipeline_exports(show)?;
    if p.episodes.is_empty() {
        return Err(AppError::Params("no episodes to export".into()));
    }
    let eps = show.episodes();
    for e in &p.episodes {
        let found = eps.iter().find(|x| x.folder == e.episode);
        if found.and_then(|x| x.file.as_ref()).is_none() {
            return Err(AppError::Params(format!("{} has no project", e.episode)));
        }
    }
    Ok(())
}

/// The deliverable: each episode's scenes to the pipeline folder, one episode at a time.
pub fn run_show(show: &Show, p: &ExportShowP, progress: &dyn Fn(f64, &str)) -> Result<Value> {
    check_show_export(show, p)?;
    let eps = show.episodes();
    let total = p.episodes.len().max(1) as f64;
    let mut results = Vec::new();
    for (i, e) in p.episodes.iter().enumerate() {
        let file = eps
            .iter()
            .find(|x| x.folder == e.episode)
            .and_then(|x| x.file.clone())
            .ok_or_else(|| AppError::Params(format!("{} has no project", e.episode)))?;
        let mut svc = Service::open(&file, "pipeline")?;
        let mut settings = settings_of(svc.doc(), p.preset.as_deref(), p.settings)?;
        settings.save_to = SaveTo::Pipeline;
        let run_p = ExportRunP {
            scenes: e.scenes.clone(),
            settings: Some(settings),
            ..ExportRunP::default()
        };
        let step = |f: f64, what: &str| {
            progress((i as f64 + f) / total, &format!("{} · {what}", e.episode));
        };
        let r = run(&mut svc, &run_p, &step)?;
        svc.close()?;
        results.push(json!({ "episode": e.episode, "result": r }));
    }
    Ok(json!({ "episodes": results }))
}

/// Opens a version folder in the Finder. Only folders of the export history: this runs a
/// program with a path from the client.
pub fn reveal(svc: Option<&Service>, show: Option<&Show>, folder: &str) -> Result<Value> {
    let folder = PathBuf::from(folder);
    let mut roots: Vec<PathBuf> = svc.iter().map(|s| s.paths.exports.clone()).collect();
    roots.extend(show.and_then(Show::pipeline_dir).map(|d| exports_in(&d)));
    let known = folder.join(versions::MANIFEST).is_file()
        && !folder.components().any(|c| c.as_os_str() == "..")
        && roots.iter().any(|r| folder.starts_with(r));
    if !known {
        return Err(AppError::Params(format!(
            "{} isn't an exported version",
            folder.display()
        )));
    }
    std::process::Command::new("open").arg(&folder).spawn()?;
    Ok(json!({ "folder": folder.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipeline_folder_called_exports_is_used_as_it_is() {
        assert_eq!(
            exports_in(Path::new("/x/Sound")),
            Path::new("/x/Sound/Exports")
        );
        assert_eq!(
            exports_in(Path::new("/x/Sound/Exports")),
            Path::new("/x/Sound/Exports")
        );
    }

    #[test]
    fn an_episode_number_comes_from_its_name() {
        assert_eq!(last_number("TARMAC Ep. 2"), Some(2));
        assert_eq!(last_number("Ep02"), Some(2));
        assert_eq!(last_number("S1 E12 Time Burrito"), Some(12));
        assert_eq!(last_number("Pilot"), None);
    }
}
