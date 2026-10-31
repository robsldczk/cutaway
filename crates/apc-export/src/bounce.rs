//! WAV/MP3 bounce per scene through the audio engine's offline render.

use std::path::{Path, PathBuf};
use std::process::Command;

use apc_audio::{offline, MixSettings, PlaySchedule, SourceSet};
use apc_core::config::ProjectConfig;
use apc_core::project::EpisodePaths;
use apc_core::Document;

use crate::error::{ExportError, Result};
use crate::fcpxml::media_info;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Wav,
    Mp3,
}

/// All sources and library assets the document can play, at `rate`.
pub fn source_set(doc: &Document, paths: &EpisodePaths, rate: u32) -> SourceSet {
    let mut set = SourceSet::new(rate);
    for s in doc.sources.values() {
        set.add(s.id.clone(), paths.resolve(&s.path));
    }
    for a in doc.assets.keys() {
        let id = format!("asset:{a}");
        if let Some(m) = media_info(doc, paths, &id) {
            set.add(id, m.path);
        }
    }
    set
}

/// Mix rate: the session's dialogue sample rate (exports keep the source rate).
pub fn mix_rate(doc: &Document) -> u32 {
    doc.sources
        .values()
        .map(|s| s.sample_rate)
        .find(|r| *r > 0)
        .unwrap_or(48000)
}

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Bounces one scene. Returns the written file and its length.
pub fn bounce_scene(
    doc: &Document,
    paths: &EpisodePaths,
    scene_key: &str,
    format: Format,
    out_dir: &Path,
) -> Result<(PathBuf, f64)> {
    let scene = doc
        .find_scene(scene_key)
        .ok_or_else(|| ExportError::Invalid(format!("no scene '{scene_key}'")))?;
    let cfg = ProjectConfig::from_doc(doc);
    let settings = MixSettings {
        room_tone: cfg.pacing.room_tone,
        ..MixSettings::default()
    };
    let sched = PlaySchedule::from_scene(doc, scene, &cfg.pacing(), &settings);
    let set = source_set(doc, paths, mix_rate(doc));
    std::fs::create_dir_all(out_dir)?;
    let base = format!("S{} {}", scene.number, scene.place);
    let wav = out_dir.join(format!("{base}.wav"));
    let len = offline::render_to_wav(&sched, &set, &wav)?;
    match format {
        Format::Wav => Ok((wav, len)),
        Format::Mp3 => {
            if !ffmpeg_available() {
                return Err(ExportError::Invalid(
                    "MP3 needs ffmpeg on the PATH; the WAV was written".into(),
                ));
            }
            let mp3 = out_dir.join(format!("{base}.mp3"));
            let st = Command::new("ffmpeg")
                .args(["-y", "-loglevel", "error", "-i"])
                .arg(&wav)
                .args(["-codec:a", "libmp3lame", "-b:a", "192k"])
                .arg(&mp3)
                .status()?;
            if !st.success() {
                return Err(ExportError::Invalid(
                    "ffmpeg failed to encode the MP3".into(),
                ));
            }
            std::fs::remove_file(&wav)?;
            Ok((mp3, len))
        }
    }
}
