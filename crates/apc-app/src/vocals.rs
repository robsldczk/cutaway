//! Character vocal buckets: the analysis finds each actor's small sounds (breaths, "mh-hm",
//! efforts, panting …) in their own mic; here they are cut into the show library
//! (`Library/Vocals/<Character>/`) and kept in that character's bucket. Only the actor's
//! own recordings go in: no search, no generation. A removed vocal stays out on the next
//! analysis. The episode side (assets, buckets) rides in the analysis command, so one
//! undo takes the whole analysis back.

use std::collections::BTreeMap;

use apc_analysis::vocals::{interjection_tag, VocalCandidate};
use apc_core::*;
use serde_json::Value;

use crate::error::{AppError, Result};
use crate::methods::{ClipP, PlaceSoundP};
use crate::service::Service;

/// The vocal bucket id of a character (stable, so a new analysis updates the same bucket).
pub fn bucket_id(character: &str) -> String {
    let slug: String = character
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("bk_vocals_{slug}")
}

fn label_of(svc: &Service, character: &str) -> String {
    svc.doc()
        .characters
        .get(character)
        .map(|c| c.label.clone())
        .unwrap_or_else(|| character.to_string())
}

/// A character's vocal bucket as it stands, or a new one.
fn bucket_of(svc: &Service, character: &str) -> SoundBucket {
    let id = bucket_id(character);
    svc.doc()
        .buckets
        .get(&id)
        .cloned()
        .unwrap_or_else(|| SoundBucket {
            id,
            name: format!("{} · vocals", label_of(svc, character)),
            keywords: vec![],
            scene_ids: vec![],
            cue_ids: vec![],
            kept: vec![],
            previews: vec![],
            dismissed: vec![],
            length_min_s: None,
            length_max_s: None,
            exclude: vec![],
            ai_suggested: false,
            order: svc.doc().buckets.len() as u32,
            character: Some(character.to_string()),
            search_offset: 0,
            folder: None,
            loops: Default::default(),
        })
}

/// Cuts the candidates into the library and returns the vocal assets and the updated
/// vocal buckets for the analysis command.
pub fn prepare(
    svc: &mut Service,
    found: &[VocalCandidate],
) -> Result<(Vec<Asset>, Vec<SoundBucket>)> {
    let mut assets: Vec<Asset> = Vec::new();
    let mut buckets: Vec<SoundBucket> = Vec::new();
    let mut by_char: BTreeMap<&str, Vec<&VocalCandidate>> = BTreeMap::new();
    for c in found {
        by_char.entry(c.character.as_str()).or_default().push(c);
    }
    for (ch, list) in by_char {
        let label = label_of(svc, ch);
        let mut b = bucket_of(svc, ch);
        for c in list {
            let Some(src) = svc.doc().sources.get(&c.source_id).cloned() else {
                continue;
            };
            let key = format!("rec:{}:{:.2}", &src.hash[..src.hash.len().min(12)], c.t0);
            if b.dismissed.contains(&key) {
                continue;
            }
            if let Some(a) = svc
                .doc()
                .assets
                .values()
                .find(|a| a.remote_id.as_deref() == Some(key.as_str()))
            {
                if !b.kept.contains(&a.id) {
                    b.kept.push(a.id.clone());
                }
                continue;
            }
            let n = b
                .kept
                .iter()
                .filter_map(|a| {
                    svc.doc()
                        .assets
                        .get(a)
                        .or_else(|| assets.iter().find(|x| &x.id == a))
                })
                .filter(|a| a.tags.iter().any(|t| t == &c.tag))
                .count()
                + 1;
            let file = svc
                .paths
                .cache
                .join("vocals")
                .join(format!("{label} {} {n:02}.wav", c.tag));
            let Ok(dur) = cut(&svc.paths.resolve(&src.path), c.t0, c.t1, &file) else {
                continue; // the source can't be read right now; the next analysis tries again
            };
            let local: Vec<Asset> = svc
                .doc()
                .assets
                .values()
                .cloned()
                .chain(assets.iter().cloned())
                .collect();
            let tags = vec![c.tag.clone(), "vocal".to_string(), label.to_lowercase()];
            let la = svc.library().import(
                &file,
                &format!("Vocals/{label}"),
                &tags,
                AssetSource::Recorded,
                dur,
                (!c.text.is_empty()).then(|| c.text.clone()),
                Some("recording".into()),
                Some(key),
                &local,
            )?;
            let _ = std::fs::remove_file(&file);
            if !b.kept.contains(&la.asset.id) {
                b.kept.push(la.asset.id.clone());
            }
            if !svc.doc().assets.contains_key(&la.asset.id)
                && !assets.iter().any(|a| a.id == la.asset.id)
            {
                assets.push(la.asset);
            }
        }
        // the tags present are the bucket's chips
        let mut tags: Vec<String> = b
            .kept
            .iter()
            .filter_map(|a| {
                svc.doc()
                    .assets
                    .get(a)
                    .or_else(|| assets.iter().find(|x| &x.id == a))
            })
            .filter_map(|a| a.tags.first().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        b.keywords = tags;
        buckets.push(b);
    }
    Ok((assets, buckets))
}

/// Cuts a stretch found by the character search, trimmed by hand, into the character's vocals
/// and places it. Cut, bucket and placement are one undo step. The same stretch again is the
/// same sound.
pub fn place_clip(svc: &mut Service, clip: &ClipP, mut p: PlaceSoundP) -> Result<Value> {
    let src = svc
        .doc()
        .sources
        .get(&clip.source_id)
        .cloned()
        .ok_or_else(|| AppError::Params(format!("unknown source '{}'", clip.source_id)))?;
    if clip.t0 < 0.0 || clip.t1 <= clip.t0 {
        return Err(AppError::Params("a clip ends after it starts".into()));
    }
    let character = p
        .character
        .clone()
        .or_else(|| src.character.clone())
        .ok_or_else(|| AppError::Params("this recording has no character".into()))?;
    p.character = Some(character.clone());
    p.lane = p.lane.or(Some(Lane::Dialogue));

    let label = label_of(svc, &character);
    let key = format!(
        "rec:{}:{:.2}-{:.2}",
        &src.hash[..src.hash.len().min(12)],
        clip.t0,
        clip.t1
    );
    let known = svc
        .doc()
        .assets
        .values()
        .find(|a| a.remote_id.as_deref() == Some(key.as_str()))
        .cloned();
    let asset = match known {
        Some(a) => a,
        None => {
            let said: String = clip
                .text
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .take(32)
                .collect();
            let name = format!("{label} {} {:.0}.wav", said.trim(), clip.t0 * 100.0);
            let file = svc.paths.cache.join("vocals").join(name);
            let dur = cut(&svc.paths.resolve(&src.path), clip.t0, clip.t1, &file)?;
            let tag = interjection_tag(&clip.text).unwrap_or("word");
            let local: Vec<Asset> = svc.doc().assets.values().cloned().collect();
            let la = svc.library().import(
                &file,
                &format!("Vocals/{label}"),
                &[tag.to_string(), "vocal".to_string(), label.to_lowercase()],
                AssetSource::Recorded,
                dur,
                (!clip.text.is_empty()).then(|| clip.text.clone()),
                Some("recording".into()),
                Some(key),
                &local,
            )?;
            let _ = std::fs::remove_file(&file);
            la.asset
        }
    };
    p.asset_id = asset.id.clone();
    let placement = crate::sound::placement_of(svc, &asset, &p)?;
    let mut bucket = bucket_of(svc, &character);
    if !bucket.kept.contains(&asset.id) {
        bucket.kept.push(asset.id.clone());
    }
    let mut commands = vec![];
    if !svc.doc().assets.contains_key(&asset.id) {
        commands.push(Command::AddAsset { asset });
    }
    commands.push(Command::PutBucket { bucket });
    commands.push(Command::PlaceSound { placement });
    let r = svc.exec(&Command::Batch { commands })?.result;
    Ok(r.as_array()
        .and_then(|a| a.last())
        .cloned()
        .unwrap_or_default())
}

/// Writes `[t0, t1)` of a source as a mono WAV; returns its length.
fn cut(src: &std::path::Path, t0: f64, t1: f64, out: &std::path::Path) -> Result<f64> {
    let mut r = apc_media::AudioReader::open(src)?;
    let info = r.info();
    let mono = apc_media::decode::to_mono(&r.read_range(t0, t1)?, info.channels);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    apc_media::wav::write_wav(out, info.sample_rate, 1, &mono)?;
    Ok(mono.len() as f64 / info.sample_rate.max(1) as f64)
}
