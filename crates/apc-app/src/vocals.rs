//! Character vocal buckets: the analysis finds each actor's small sounds (breaths, "mh-hm",
//! efforts, panting …) in their own mic; here they are cut into the show library
//! (`Library/Vocals/<Character>/`) and kept in that character's bucket. Only the actor's
//! own recordings go in: no search, no generation. A removed vocal stays out on the next
//! analysis. The episode side (assets, buckets) rides in the analysis command, so one
//! undo takes the whole analysis back.

use std::collections::BTreeMap;

use apc_analysis::vocals::VocalCandidate;
use apc_core::*;

use crate::error::Result;
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
        let label = svc
            .doc()
            .characters
            .get(ch)
            .map(|c| c.label.clone())
            .unwrap_or_else(|| ch.to_string());
        let id = bucket_id(ch);
        let order = svc.doc().buckets.len() as u32;
        let mut b = svc.doc().buckets.get(&id).cloned().unwrap_or(SoundBucket {
            id: id.clone(),
            name: format!("{label} · vocals"),
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
            order,
            character: Some(ch.to_string()),
            search_offset: 0,
        });
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
