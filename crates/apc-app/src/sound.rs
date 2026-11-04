//! Sound methods: spotting, bucket, library import, search, generation and placement.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use apc_ai::task::{
    CastNote, ScriptItem, SoundSpotInput, SoundTagInput, SpotContext, SpotCueType, SpotScene,
};
use apc_core::ids::new_id;
use apc_core::project::SHOW_PREFIX;
use apc_core::sounds::{DROP_CROP_S, DROP_SHORT_S};
use apc_core::timeline::{layout_scene, placement_start};
use apc_core::*;
use apc_sound::epidemic::EpidemicProvider;
use apc_sound::providers::{ElevenLabsProvider, LibrarySearchProvider};
use apc_sound::secrets::KeyedService;
use apc_sound::{GenerateRequest, SoundGenerateProvider, SoundQuery, SoundSearchProvider};

use crate::error::{AppError, Result};
use crate::methods::PlaceSoundP;
use crate::service::Service;

fn scene_of(svc: &Service, key: &str) -> Result<Scene> {
    svc.doc()
        .find_scene(key)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("no scene '{key}'")))
}

/// What the writers know about a scene, for the spotting prompt.
pub fn spot_context(doc: &Document, scene: &Scene) -> SpotContext {
    let st = &scene.story;
    let loc = doc
        .locations
        .get(&apc_core::script::location_id(&scene.location));
    let with_desc = |name: &str, desc: &str| {
        if desc.trim().is_empty() {
            name.to_string()
        } else {
            format!("{name}: {}", desc.trim())
        }
    };
    SpotContext {
        title: st.title.clone(),
        act: st.act.clone(),
        synopsis: st.synopsis.clone(),
        tone: st.tone,
        cast: st
            .cast
            .iter()
            .map(|name| {
                let c = apc_core::story::character_for(doc, name)
                    .and_then(|id| doc.characters.get(&id));
                CastNote {
                    name: name.clone(),
                    note: c.map(|c| c.profile.summary.clone()).unwrap_or_default(),
                }
            })
            .collect(),
        setting: loc
            .filter(|l| !l.description.trim().is_empty())
            .map(|l| with_desc(&l.name, &l.description))
            .unwrap_or_default(),
        part_of: loc
            .and_then(|l| l.parent.as_deref())
            .and_then(|p| doc.locations.get(p))
            .map(|p| with_desc(&p.name, &p.description)),
    }
}

/// AI sound spotting for one scene → anchored cues in the bucket.
pub fn spot(svc: &mut Service, scene_key: &str) -> Result<Value> {
    let scene = scene_of(svc, scene_key)?;
    let doc = svc.doc();
    let items: Vec<ScriptItem> = doc
        .lines_of_scene(&scene.id)
        .iter()
        .map(|l| ScriptItem {
            line_id: l.id.clone(),
            kind: format!("{:?}", l.kind).to_lowercase(),
            character: l.character.clone(),
            text: l.text.clone(),
        })
        .collect();
    let context = if svc.config().ai.story_context {
        Some(spot_context(doc, &scene)).filter(|c| !c.is_empty())
    } else {
        None
    };
    let input = SoundSpotInput {
        scene: SpotScene {
            id: scene.id.clone(),
            heading: scene.heading.clone(),
            location: scene.location.clone(),
            items,
        },
        context,
    };
    let (runner, warn) = svc.ai_runner();
    let out = runner.run(&input)?;
    let Some(o) = out.output else {
        return Err(AppError::Other(
            out.flagged.unwrap_or_else(|| "spotting failed".into()),
        ));
    };
    let cues: Vec<SoundCue> = o
        .cues
        .into_iter()
        .filter(|c| svc.doc().lines.contains_key(&c.anchor_line_id))
        .map(|c| SoundCue {
            id: new_id("cue"),
            scene_id: scene.id.clone(),
            description: c.description,
            anchor: Some(Anchor {
                line_id: c.anchor_line_id,
                word: c.anchor_word,
                edge: Edge::Start,
            }),
            cue_type: match c.cue_type {
                SpotCueType::Hard => CueType::Hard,
                SpotCueType::Foley => CueType::Foley,
                SpotCueType::Ambience => CueType::Ambience,
                SpotCueType::Walla => CueType::Walla,
                SpotCueType::Music => CueType::Music,
            },
            tags: c.tags,
            priority: c.priority,
            status: CueStatus::Wanted,
            candidates: vec![],
            chosen: None,
            show_cue_id: None,
            ai_suggested: true,
            accepted: false,
        })
        .collect();
    let n = cues.len();
    svc.exec(&Command::ApplySpotting {
        scene_id: scene.id.clone(),
        cues,
    })?;
    Ok(json!({ "cues": n, "provider": runner.provider_id(), "warning": warn }))
}

/// Merges cues; with `show_level` records the result as a show cue.
pub fn merge(svc: &mut Service, ids: Vec<String>, into: String, show_level: bool) -> Result<Value> {
    svc.exec(&Command::MergeCues {
        cue_ids: ids,
        into_id: into.clone(),
    })?;
    if show_level {
        let cue = svc
            .doc()
            .cues
            .get(&into)
            .cloned()
            .ok_or_else(|| AppError::Params("cue vanished".into()))?;
        if svc.paths.show.is_some() {
            let sid = cue.show_cue_id.clone().unwrap_or_else(|| new_id("scue"));
            svc.exec(&Command::UpdateCue {
                cue_id: into.clone(),
                patch: json!({ "show_cue_id": sid }),
            })?;
        }
    }
    Ok(json!({ "into": into }))
}

/// Makes sure a library asset is mirrored into the episode document (placements need it).
/// A sound of the episode or of the show's library. A bucket lists both, and a show's sound
/// only joins the episode when it is placed.
pub fn find_asset(svc: &Service, asset_id: &str) -> Result<Option<Asset>> {
    if let Some(a) = svc.doc().assets.get(asset_id) {
        return Ok(Some(a.clone()));
    }
    let local: Vec<Asset> = svc.doc().assets.values().cloned().collect();
    Ok(svc
        .library()
        .list(&local)?
        .into_iter()
        .find(|a| a.id == asset_id))
}

pub fn ensure_asset_in_doc(svc: &mut Service, asset_id: &str) -> Result<Asset> {
    let a = find_asset(svc, asset_id)?
        .ok_or_else(|| AppError::Params(format!("no library sound '{asset_id}'")))?;
    if !svc.doc().assets.contains_key(asset_id) {
        svc.exec(&Command::AddAsset { asset: a.clone() })?;
    }
    Ok(a)
}

pub fn search(
    svc: &mut Service,
    query: Option<String>,
    cue_id: Option<String>,
    attach: bool,
) -> Result<Value> {
    let cue = cue_id.as_ref().and_then(|c| svc.doc().cues.get(c).cloned());
    let text = query
        .or_else(|| cue.as_ref().map(|c| c.description.clone()))
        .unwrap_or_default();
    let q = SoundQuery {
        text,
        tags: cue.as_ref().map(|c| c.tags.clone()).unwrap_or_default(),
        max_results: 8,
    };
    let lib = svc.library();
    let local: Vec<Asset> = svc.doc().assets.values().cloned().collect();
    let mut results = Vec::new();
    let mut errors = Vec::new();
    let cfg = svc.config();
    for p in &cfg.sound.search.providers {
        let r = match p.as_str() {
            "library" => LibrarySearchProvider {
                library: &lib,
                local: local.clone(),
            }
            .search(&q),
            "epidemic" => EpidemicProvider::new(svc.api_key(KeyedService::Epidemic)).search(&q),
            other => Err(apc_sound::SoundError::Unavailable(
                other.into(),
                "unknown provider".into(),
            )),
        };
        match r {
            Ok(mut v) => results.append(&mut v),
            Err(e) => errors.push(e.to_string()),
        }
    }
    drop(lib);
    if attach {
        if let Some(c) = &cue {
            for r in &results {
                if let Some(a) = &r.asset {
                    ensure_asset_in_doc(svc, &a.id)?;
                    svc.exec(&Command::AttachCandidate {
                        cue_id: c.id.clone(),
                        asset_id: a.id.clone(),
                    })?;
                }
            }
        }
    }
    Ok(json!({ "results": results, "unavailable": errors }))
}

/// Imports one file. `dest` is a bucket's folder in the show (`show:sfx/doors`); without
/// it a show sound goes loose into `sfx/` or `music/`, and a standalone episode's into
/// `Library/<category>/` (from the name, or the AI when the name says nothing).
#[allow(clippy::too_many_arguments)]
fn import_one(
    svc: &mut Service,
    path: &Path,
    source: AssetSource,
    prompt: Option<String>,
    provider: Option<String>,
    cue_desc: Option<String>,
    remote_id: Option<String>,
    dest: Option<String>,
) -> Result<Asset> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let duration = apc_media::decode::probe(path)
        .map(|i| i.duration_s)
        .unwrap_or(0.0);
    let duration = if duration > 0.0 {
        duration
    } else {
        apc_media::AudioAnalysis::compute(path)
            .map(|a| a.info.duration_s)
            .unwrap_or(0.0)
    };
    let (cat, tags) = apc_sound::tagging::tag_from_name(&name);
    let loose = svc.show_lib.as_ref().and_then(|l| {
        let music = cat
            .as_deref()
            .map(|c| c.starts_with("Music"))
            .unwrap_or(false);
        l.lock()
            .ok()
            .map(|l| format!("{SHOW_PREFIX}{}", l.loose_folder(music)))
    });
    let cat = match (dest, loose, cat) {
        (Some(d), _, _) => d,
        (None, Some(l), _) => l,
        (None, None, Some(c)) => c,
        (None, None, None) => {
            let (runner, _) = svc.ai_runner();
            runner
                .run(&SoundTagInput {
                    file_name: name.clone(),
                    duration_s: duration,
                    cue_description: cue_desc,
                })
                .ok()
                .and_then(|o| o.output)
                .map(|o| o.category)
                .unwrap_or_else(|| "SFX/Misc".into())
        }
    };
    let local: Vec<Asset> = svc.doc().assets.values().cloned().collect();
    let la = svc.library().import(
        path, &cat, &tags, source, duration, prompt, provider, remote_id, &local,
    )?;
    if !svc.doc().assets.contains_key(&la.asset.id) {
        svc.exec(&Command::AddAsset {
            asset: la.asset.clone(),
        })?;
    }
    Ok(la.asset)
}

pub fn generate(
    svc: &mut Service,
    cue_id: Option<String>,
    prompt: Option<String>,
    variations: Option<u32>,
    duration_s: Option<f64>,
    dest: Option<String>,
) -> Result<Value> {
    let cue = cue_id.as_ref().and_then(|c| svc.doc().cues.get(c).cloned());
    let prompt = prompt
        .or_else(|| cue.as_ref().map(|c| c.description.clone()))
        .ok_or_else(|| AppError::Params("give a prompt or a cue".into()))?;
    let cfg = svc.config();
    let req = GenerateRequest {
        prompt: prompt.clone(),
        duration_s,
        variations: variations.unwrap_or(cfg.sound.generate.variations),
        out_dir: svc.paths.cache.join("generated"),
    };
    let mut errors = Vec::new();
    let mut made = Vec::new();
    for p in &cfg.sound.generate.providers {
        let r = match p.as_str() {
            "elevenlabs" => {
                ElevenLabsProvider::with_key(svc.api_key(KeyedService::Elevenlabs)).generate(&req)
            }
            "magnific" => apc_sound::providers::McpGenerateProvider { server: None }.generate(&req),
            other => Err(apc_sound::SoundError::Unavailable(
                other.into(),
                "unknown provider".into(),
            )),
        };
        match r {
            Ok(v) => {
                made = v;
                break;
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    let mut assets = Vec::new();
    for g in made {
        let a = import_one(
            svc,
            &g.file,
            AssetSource::Generated,
            Some(g.prompt.clone()),
            Some(g.provider.clone()),
            Some(prompt.clone()),
            None,
            dest.clone(),
        )?;
        if let Some(c) = &cue {
            svc.exec(&Command::AttachCandidate {
                cue_id: c.id.clone(),
                asset_id: a.id.clone(),
            })?;
        }
        assets.push(a);
    }
    if assets.is_empty() {
        return Err(AppError::Other(format!(
            "no generation provider available: {}",
            errors.join("; ")
        )));
    }
    Ok(json!({ "assets": assets }))
}

pub fn import(
    svc: &mut Service,
    paths: &[String],
    cue_id: Option<String>,
    scene: Option<String>,
    at_s: Option<f64>,
    bucket_id: Option<String>,
) -> Result<Value> {
    let dest = match &bucket_id {
        Some(b) => crate::studio::bucket_dest(svc, b)?,
        None => None,
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for p in paths {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            files.extend(apc_core::media::audio_files_in(&pb)?);
        } else {
            files.push(pb);
        }
    }
    let cue_desc = cue_id
        .as_ref()
        .and_then(|c| svc.doc().cues.get(c))
        .map(|c| c.description.clone());
    let mut assets = Vec::new();
    for f in &files {
        let a = import_one(
            svc,
            f,
            AssetSource::Imported,
            None,
            None,
            cue_desc.clone(),
            None,
            dest.clone(),
        )?;
        if let Some(c) = &cue_id {
            svc.exec(&Command::AttachCandidate {
                cue_id: c.clone(),
                asset_id: a.id.clone(),
            })?;
        }
        if let Some(b) = &bucket_id {
            svc.exec(&Command::BucketKeep {
                bucket_id: b.clone(),
                asset_id: a.id.clone(),
            })?;
        }
        assets.push(a);
    }
    let mut placed = None;
    if let (Some(sc), Some(t), Some(first)) = (scene, at_s, assets.first()) {
        let r = place(
            svc,
            PlaceSoundP {
                asset_id: first.id.clone(),
                scene: sc,
                cue_id: cue_id.clone(),
                lane: None,
                character: None,
                anchor_line_id: None,
                anchor_word: None,
                anchor_edge: Edge::Start,
                at_s: Some(t),
                offset_s: 0.0,
                depth: 0.0,
                dropped: Some(first.loops_by_default()),
            },
        )?;
        placed = Some(r);
    }
    Ok(json!({ "assets": assets, "placed": placed }))
}

pub fn delete_asset(svc: &mut Service, asset_id: &str) -> Result<Value> {
    let local: Vec<Asset> = svc.doc().assets.values().cloned().collect();
    let lib = svc.library();
    let a = lib
        .list(&local)?
        .into_iter()
        .find(|a| a.id == asset_id)
        .ok_or_else(|| AppError::Params(format!("no sound '{asset_id}'")))?;
    if svc
        .doc()
        .placements
        .values()
        .any(|p| p.asset_id == asset_id)
    {
        return Err(AppError::Params(
            "the sound is placed on the timeline; remove the placements first".into(),
        ));
    }
    let trash = lib.root.parent().unwrap_or(&lib.root).join("Backups/Trash");
    lib.delete(&a, &trash)?;
    Ok(json!({ "deleted": asset_id }))
}

/// Places a sound. With a cue anchor (or an explicit line) it hangs on the dialogue;
/// otherwise it sits at `at_s`.
/// What a drop from a bucket makes of a sound (D85). Short sounds stay as they are, and music
/// and vocals aren't sound design. Past the short mark a sound is cropped, or, with Loop on,
/// becomes a bed: the ambience lane, repeating to the scene's end.
fn shape_drop(svc: &Service, scene: &Scene, looping: bool, p: &mut Placement) {
    if matches!(p.lane, Lane::Dialogue | Lane::Music) || p.len_s <= DROP_SHORT_S {
        return;
    }
    if !looping {
        p.len_s = p.len_s.min(DROP_CROP_S);
        return;
    }
    let layout = layout_scene(svc.doc(), scene, &svc.config().pacing());
    p.lane = Lane::Ambience;
    p.looped = true;
    p.len_s = (layout.duration_s - placement_start(&layout, p)).max(DROP_CROP_S);
    p.fade_in_s = 1.0;
}

pub fn place(svc: &mut Service, p: PlaceSoundP) -> Result<Value> {
    let a = ensure_asset_in_doc(svc, &p.asset_id)?;
    let scene = scene_of(svc, &p.scene)?;
    let cue = p
        .cue_id
        .as_ref()
        .and_then(|c| svc.doc().cues.get(c).cloned());
    let anchor = match (&p.anchor_line_id, p.at_s) {
        (Some(l), _) => Some(Anchor {
            line_id: l.clone(),
            word: p.anchor_word,
            edge: p.anchor_edge,
        }),
        (None, None) => cue.as_ref().and_then(|c| c.anchor.clone()),
        (None, Some(_)) => None,
    };
    let lane = p.lane.unwrap_or(match cue.as_ref().map(|c| c.cue_type) {
        _ if p.character.is_some() && a.category.starts_with("Vocals") => Lane::Dialogue,
        Some(CueType::Ambience) => Lane::Ambience,
        Some(CueType::Music) => Lane::Music,
        _ if a.category.starts_with("Ambience") => Lane::Ambience,
        _ if a.category.starts_with("Music") => Lane::Music,
        _ => Lane::Sfx,
    });
    let character = match lane {
        Lane::Dialogue => Some(p.character.clone().ok_or_else(|| {
            AppError::Params("a sound on a dialogue lane needs its character".into())
        })?),
        _ => None,
    };
    // a breath or "mh-hm" is cut to the actor's sound: it keeps its short edges
    let vocal = lane == Lane::Dialogue;
    let placement = Placement {
        id: new_id("pl"),
        scene_id: scene.id.clone(),
        asset_id: a.id.clone(),
        cue_id: p.cue_id.clone(),
        lane,
        character,
        layer: 0,
        anchor,
        at_s: p.at_s.unwrap_or(0.0),
        offset_s: p.offset_s,
        in_s: 0.0,
        len_s: if a.duration_s > 0.0 {
            a.duration_s
        } else {
            1.0
        },
        fade_in_s: if vocal { 0.01 } else { 0.02 },
        fade_out_s: 0.05,
        fade_curve: Default::default(),
        gain_db: 0.0,
        muted: false,
        pan: 0.0,
        depth: p.depth,
        auto_fade: !vocal,
        added: 0,
        looped: false,
    };
    let mut placement = placement;
    if let Some(looping) = p.dropped {
        shape_drop(svc, &scene, looping, &mut placement);
    }
    Ok(svc.exec(&Command::PlaceSound { placement })?.result)
}

/// Stores an API key in the credential store after checking it (Epidemic: a test search).
pub fn connect(svc: &mut Service, service: &str, api_key: &str) -> Result<Value> {
    let s = KeyedService::parse(service)?;
    let key = api_key.trim();
    if key.len() < 8 {
        return Err(AppError::Params("that doesn't look like an API key".into()));
    }
    if s == KeyedService::Epidemic {
        EpidemicProvider::new(Some(key.to_string())).verify()?;
    }
    svc.secrets.set(s.id(), key)?;
    Ok(serde_json::to_value(apc_sound::secrets::status(
        svc.secrets.as_ref(),
        s,
    ))?)
}

pub fn disconnect(svc: &mut Service, service: &str) -> Result<Value> {
    let s = KeyedService::parse(service)?;
    svc.secrets.delete(s.id())?;
    Ok(serde_json::to_value(apc_sound::secrets::status(
        svc.secrets.as_ref(),
        s,
    ))?)
}

pub fn connections(svc: &Service) -> Result<Value> {
    Ok(serde_json::to_value(
        KeyedService::ALL
            .iter()
            .map(|s| apc_sound::secrets::status(svc.secrets.as_ref(), *s))
            .collect::<Vec<_>>(),
    )?)
}

/// Downloads a remote search result (Epidemic Sound) into the library, licensed, and
/// attaches it to the cue. Counts as one download against the plan.
pub fn fetch(
    svc: &mut Service,
    remote_id: &str,
    title: &str,
    cue_id: Option<String>,
) -> Result<Value> {
    let ep = EpidemicProvider::new(svc.api_key(KeyedService::Epidemic));
    let file = ep.download(remote_id, title, &svc.paths.cache.join("downloads"))?;
    let cue_desc = cue_id
        .as_ref()
        .and_then(|c| svc.doc().cues.get(c))
        .map(|c| c.description.clone());
    let a = import_licensed(svc, &file, remote_id, cue_desc, None)?;
    if let Some(c) = &cue_id {
        svc.exec(&Command::AttachCandidate {
            cue_id: c.clone(),
            asset_id: a.id.clone(),
        })?;
    }
    Ok(json!({ "asset": a }))
}

/// Imports a downloaded Epidemic Sound file into the library as licensed.
pub fn import_licensed(
    svc: &mut Service,
    file: &Path,
    remote_id: &str,
    describe: Option<String>,
    dest: Option<String>,
) -> Result<Asset> {
    let mut a = import_one(
        svc,
        file,
        AssetSource::Licensed,
        None,
        Some("epidemic".into()),
        describe,
        Some(remote_id.to_string()),
        dest,
    )?;
    if a.license.is_none() {
        let license = format!("Epidemic Sound, sound effect {remote_id}");
        record_show_meta(
            svc,
            &a,
            apc_sound::show_library::SoundMeta {
                license: Some(license.clone()),
                ..Default::default()
            },
        )?;
        a.license = Some(license);
        svc.exec(&Command::AddAsset { asset: a.clone() })?;
    }
    Ok(a)
}

/// Shares what a show sound's file can't say (licence, provider id) with the other
/// episodes and machines.
fn record_show_meta(
    svc: &Service,
    a: &Asset,
    meta: apc_sound::show_library::SoundMeta,
) -> Result<()> {
    if !a.file.starts_with(SHOW_PREFIX) {
        return Ok(());
    }
    if let Some(lib) = &svc.show_lib {
        if let Ok(mut l) = lib.lock() {
            l.record(&a.hash, meta)?;
        }
    }
    Ok(())
}

/// Copies a known sound (from elsewhere in the show) into a bucket's folder, keeping what
/// is known about it (licence, provider id, prompt).
pub fn import_into(svc: &mut Service, file: &Path, dest: &str, known: &Asset) -> Result<Asset> {
    let la = svc.library().import(
        file,
        dest,
        &known.tags,
        known.source,
        known.duration_s,
        known.prompt.clone(),
        known.provider.clone(),
        known.remote_id.clone(),
        &[],
    )?;
    let mut a = la.asset;
    if a.license.is_none() && known.license.is_some() {
        record_show_meta(
            svc,
            &a,
            apc_sound::show_library::SoundMeta {
                license: known.license.clone(),
                ..Default::default()
            },
        )?;
        a.license = known.license.clone();
    }
    Ok(a)
}
