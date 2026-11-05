//! An export of the Export page: one version folder per scene, with its stems, clips, mix and
//! timeline files, and a manifest. Built on the same schedule as playback and the bounce, and
//! the same writers as the single exports.
//!
//! A version is written into a hidden folder and renamed when it's complete, so a failed export
//! leaves nothing that looks delivered, and a sync tool never picks up half of one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::Serialize;

use apc_audio::offline::{frames_of, render_part};
use apc_audio::{Part, PlaySchedule, SourceSet, StemGroup};
use apc_core::config::{
    AudioCodec, CutListFormat, ExportMode, ExportSettings, FcpxmlSource, ProjectConfig,
};
use apc_core::project::EpisodePaths;
use apc_core::{Document, Scene};

use crate::bounce::{encode, ffmpeg_available, schedule_of, source_set};
use crate::error::{ExportError, Result};
use crate::fcpxml::{self, media_info, MediaInfo, Options, ROOM_EFFECTS_LANE, ROOM_VOICES_LANE};
use crate::timeline::{EpisodeTimeline, ExportScope, TimelineClip};
use crate::versions::{
    checksum, scene_folder, version_folder, EpisodeTag, ExportRecord, History, Manifest,
    ManifestFile, ManifestScene, EPISODE_FOLDER, MANIFEST, MANIFEST_SCHEMA,
};
use crate::{cutlist, srt};

/// The mix stops short of full scale when a loudness target would push it over.
const CEILING_DB: f64 = -1.0;

/// Where and as what an export writes.
pub struct Target<'a> {
    pub doc: &'a Document,
    pub paths: &'a EpisodePaths,
    pub ep: EpisodeTag,
    /// The `Exports` folder of the destination.
    pub exports: &'a Path,
    pub settings: ExportSettings,
    pub now: DateTime<Local>,
    pub app_version: &'a str,
    pub title: &'a str,
}

impl Target<'_> {
    fn date(&self) -> String {
        self.now.format("%Y-%m-%d").to_string()
    }
}

/// One full-length file of a scene.
#[derive(Clone, Debug, PartialEq)]
enum Track {
    Group(StemGroup),
    /// The dry signal of these lanes: one character.
    Voice(Vec<String>),
    RoomVoices,
    RoomEffects,
    Mix,
}

#[derive(Clone, Debug, PartialEq)]
struct TrackFile {
    /// `Dialogue`, `Dialogue_PAUL`, `RoomVoices` …
    suffix: String,
    track: Track,
}

fn is_voice(lane: &str) -> bool {
    StemGroup::of_lane(lane) == StemGroup::Dialogue
}

fn token(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// One file per character (adlibs together), or one for the dialogue when nobody speaks: the
/// stack stays complete either way.
fn voice_files(sched: &PlaySchedule) -> Vec<TrackFile> {
    let mut voices: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in sched.clips.iter().filter(|c| is_voice(&c.lane)) {
        let name = if c.lane.starts_with("adlib") {
            "Adlibs".to_string()
        } else {
            token(&c.lane)
        };
        voices.entry(name).or_default().insert(c.lane.clone());
    }
    if voices.is_empty() {
        return vec![TrackFile {
            suffix: "Dialogue".into(),
            track: Track::Group(StemGroup::Dialogue),
        }];
    }
    voices
        .into_iter()
        .map(|(name, lanes)| TrackFile {
            suffix: format!("Dialogue_{name}"),
            track: Track::Voice(lanes.into_iter().collect()),
        })
        .collect()
}

/// The full-length files of a scene, in stack order.
fn track_files(s: &ExportSettings, sched: &PlaySchedule) -> Vec<TrackFile> {
    let t = s.tracks;
    let file = |on: bool, suffix: &str, track: Track| {
        on.then(|| TrackFile {
            suffix: suffix.into(),
            track,
        })
    };
    let mix = file(t.mix, "Mix", Track::Mix);
    let room_voices = file(t.room_voices, "RoomVoices", Track::RoomVoices);
    let room_effects = file(t.room_effects, "RoomEffects", Track::RoomEffects);
    match s.mode {
        ExportMode::Fcpxml => Vec::new(),
        ExportMode::Mix => vec![TrackFile {
            suffix: "Mix".into(),
            track: Track::Mix,
        }],
        // the clips are the dry tracks; what they can't be still renders whole
        ExportMode::Clips => [room_voices, room_effects, mix]
            .into_iter()
            .flatten()
            .collect(),
        ExportMode::Stems => {
            let mut out = if t.split_dialogue {
                voice_files(sched)
            } else {
                file(t.dialogue, "Dialogue", Track::Group(StemGroup::Dialogue))
                    .into_iter()
                    .collect()
            };
            out.extend(room_voices);
            out.extend(file(t.effects, "Effects", Track::Group(StemGroup::Effects)));
            out.extend(room_effects);
            out.extend(file(
                t.ambience,
                "Ambience",
                Track::Group(StemGroup::Ambience),
            ));
            out.extend(file(t.music, "Music", Track::Group(StemGroup::Music)));
            out.extend(mix);
            out
        }
    }
}

fn render(track: &Track, sched: &PlaySchedule, set: &SourceSet, frames: usize) -> Result<Vec<f32>> {
    let group = |g: StemGroup| sched.only(move |c| StemGroup::of_lane(&c.lane) == g);
    let (part_of, part) = match track {
        Track::Mix => return Ok(render_part(sched, set, frames, Part::Whole)?),
        Track::Group(g) => (group(*g), Part::Dry),
        Track::Voice(lanes) => (sched.only(|c| lanes.contains(&c.lane)), Part::Dry),
        Track::RoomVoices => (sched.only(|c| is_voice(&c.lane)), Part::Wet),
        // an ambience in the room rings with the effects: the sum has to come out whole
        Track::RoomEffects => (sched.only(|c| !is_voice(&c.lane)), Part::Wet),
    };
    Ok(render_part(&part_of, set, frames, part)?)
}

/// Brings a mix to its target, short of full scale. Loudness is for the mix alone, so the
/// stems stay at mix level and still add up.
fn level_mix(
    buf: &mut [f32],
    rate: u32,
    target: f64,
) -> Result<(f64, Option<f64>, Option<String>)> {
    let Some(lufs) = apc_media::loudness::integrated_lufs(buf, rate, 2)? else {
        return Ok((0.0, None, None));
    };
    let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let wanted = target - lufs;
    let room = if peak > 0.0 {
        CEILING_DB - 20.0 * f64::from(peak).log10()
    } else {
        wanted
    };
    let gain_db = wanted.min(room);
    let g = 10f64.powf(gain_db / 20.0) as f32;
    for v in buf.iter_mut() {
        *v *= g;
    }
    let reached = lufs + gain_db;
    let warning = (wanted > room)
        .then(|| format!("The mix stops at {reached:.1} LUFS: {target:.0} would clip its peaks."));
    Ok((gain_db, Some(reached), warning))
}

/// An exported clip of a scene: which clip, its file name, and the stretch of its source.
#[derive(Clone, Debug)]
struct ClipFile {
    clip: usize,
    name: String,
    from_s: f64,
    to_s: f64,
}

fn group_of_clip(c: &TimelineClip) -> StemGroup {
    if c.lane.starts_with("SFX") {
        StemGroup::Effects
    } else if c.lane == "AMBIENCE" {
        StemGroup::Ambience
    } else if c.lane == "MUSIC" {
        StemGroup::Music
    } else {
        StemGroup::Dialogue
    }
}

fn clip_wanted(s: &ExportSettings, c: &TimelineClip) -> bool {
    let t = s.tracks;
    match group_of_clip(c) {
        StemGroup::Dialogue => t.dialogue || t.split_dialogue,
        StemGroup::Effects => t.effects,
        StemGroup::Ambience => t.ambience,
        StemGroup::Music => t.music,
    }
}

/// `E02_S02_v003_L014_PAUL_T3`, `E02_S02_v003_L014_PAUL_T3_p2`, `E02_S02_v003_SFX1_003`.
fn clip_name(stem: &str, c: &TimelineClip, sound_no: usize) -> String {
    let part = if c.part.1 > 1 {
        format!("_p{}", c.part.0)
    } else {
        String::new()
    };
    match c.take_no {
        Some(take) => {
            let line = c
                .line_ref
                .rsplit('.')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0);
            let who = token(c.character.as_deref().unwrap_or(&c.lane));
            format!("{stem}_L{line:03}_{who}_T{take}{part}")
        }
        None => format!(
            "{stem}_{}_{sound_no:03}{part}",
            token(&c.lane).replace('_', "")
        ),
    }
}

/// The clips an export writes, each with its handles inside the source file.
fn clip_files(
    doc: &Document,
    paths: &EpisodePaths,
    tl: &EpisodeTimeline,
    s: &ExportSettings,
    stem: &str,
) -> Vec<ClipFile> {
    if s.mode != ExportMode::Clips {
        return Vec::new();
    }
    let handles = ProjectConfig::from_doc(doc).export.handles_s.max(0.0);
    let mut sounds = 0usize;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for (i, c) in tl.clips.iter().enumerate() {
        if !clip_wanted(s, c) {
            continue;
        }
        if c.take_no.is_none() && c.part.0 == 1 {
            sounds += 1;
        }
        let base = clip_name(stem, c, sounds);
        // the same take can come back in one line: a name is never used twice
        let mut name = base.clone();
        let mut n = 2;
        while !seen.insert(name.clone()) {
            name = format!("{base}_{n}");
            n += 1;
        }
        let end = media_info(doc, paths, &c.source_id)
            .map(|m| m.duration_s)
            .filter(|d| *d > 0.0)
            .unwrap_or(c.src_out);
        out.push(ClipFile {
            clip: i,
            name,
            from_s: (c.src_in - handles).max(0.0),
            to_s: (c.src_out + handles).min(end.max(c.src_out)),
        });
    }
    out
}

/// What the audio of a version came to, for the timeline that points at it.
struct Written<'a> {
    rendered: &'a [Rendered],
    clips: &'a [ClipFile],
    /// Where the files will be, not where they're being written.
    folder: &'a Path,
    rate: u32,
    len_s: f64,
}

/// A rendered file that a timeline can point at.
struct Rendered {
    file: TrackFile,
    path: PathBuf,
    name: String,
}

fn scene_timeline(doc: &Document, scene: &Scene) -> EpisodeTimeline {
    EpisodeTimeline::build(doc, &ExportScope::Scenes(vec![scene.id.clone()]))
}

/// What a scene is, to tell one export from the next: what's heard, and what the timeline files
/// say about it (cues, notes, take names).
pub fn timeline_hash(doc: &Document, scene: &Scene) -> Result<String> {
    let mut h = blake3::Hasher::new();
    h.update(&serde_json::to_vec(&schedule_of(doc, scene))?);
    h.update(&serde_json::to_vec(&scene_timeline(doc, scene))?);
    Ok(h.finalize().to_hex().to_string())
}

/// A clip standing for a whole rendered file: from 0:00, on its own lane.
fn bed(scene: &Scene, lane: &str, label: &str, id: &str, len_s: f64) -> TimelineClip {
    TimelineClip {
        scene_id: scene.id.clone(),
        scene_number: scene.number,
        owner: String::new(),
        line_ref: format!("S{}", scene.number),
        lane: lane.into(),
        character: None,
        take_no: None,
        part: (1, 1),
        source_id: id.into(),
        src_in: 0.0,
        src_out: len_s,
        rec_in: 0.0,
        fade_in_s: 0.0,
        fade_out_s: 0.0,
        gain_db: 0.0,
        direction: String::new(),
        feedback: String::new(),
        text: label.into(),
        muted: false,
    }
}

fn lane_of(doc: &Document, f: &TrackFile) -> Option<(String, String)> {
    let named = |lane: &str, label: &str| Some((lane.to_string(), label.to_string()));
    match &f.track {
        Track::Mix => None,
        Track::RoomVoices => named(ROOM_VOICES_LANE, "Room · voices"),
        Track::RoomEffects => named(ROOM_EFFECTS_LANE, "Room · effects"),
        Track::Group(StemGroup::Dialogue) => named("DIALOGUE", "Dialogue"),
        Track::Group(StemGroup::Effects) => named("EFFECTS", "Effects"),
        Track::Group(StemGroup::Ambience) => named("AMBIENCE", "Ambience"),
        Track::Group(StemGroup::Music) => named("MUSIC", "Music"),
        Track::Voice(lanes) => {
            let lane = lanes.first()?;
            let label = doc
                .characters
                .get(lane)
                .map(|c| c.label.clone())
                .unwrap_or_else(|| f.suffix.replace("Dialogue_", ""));
            Some((
                if doc.characters.contains_key(lane) {
                    lane.clone()
                } else {
                    "ADLIB".into()
                },
                label,
            ))
        }
    }
}

/// The scene's FCPXML: the dry character lanes and the two room lanes, on the original recordings,
/// the exported clips or the rendered stems. Without rendered room tracks there's no room lane:
/// FCPXML only renders nothing (D111).
fn scene_fcpxml(
    t: &Target,
    scene: &Scene,
    tl: &EpisodeTimeline,
    written: &Written,
    name: &str,
) -> String {
    let Written {
        rendered,
        clips,
        folder: final_dir,
        rate,
        len_s,
    } = *written;
    let info =
        |path: PathBuf, name: &str, duration_s: f64, channels: u16, sample_rate: u32| MediaInfo {
            name: name.to_string(),
            path,
            duration_s,
            tc_start_s: 0.0,
            channels,
            sample_rate,
        };
    let mut media = BTreeMap::new();
    let mut out = tl.clone();
    let stems_only =
        t.settings.mode == ExportMode::Stems && t.settings.fcpxml_source == FcpxmlSource::Stems;
    if stems_only {
        out.clips.clear();
    }
    if t.settings.mode == ExportMode::Clips {
        let wanted: BTreeMap<usize, &ClipFile> = clips.iter().map(|c| (c.clip, c)).collect();
        out.clips = Vec::new();
        for (i, c) in tl.clips.iter().enumerate() {
            let Some(f) = wanted.get(&i) else { continue };
            let id = format!("clip:{}", f.name);
            let mut moved = c.clone();
            moved.source_id = id.clone();
            moved.src_in = c.src_in - f.from_s;
            moved.src_out = c.src_out - f.from_s;
            out.clips.push(moved);
            // the source's own file format comes through a clip unchanged
            let src = media_info(t.doc, t.paths, &c.source_id);
            let file = format!("{}.{}", f.name, t.settings.codec.ext());
            media.insert(
                id,
                info(
                    final_dir.join(file),
                    &f.name,
                    f.to_s - f.from_s,
                    src.as_ref().map(|m| m.channels).unwrap_or(1),
                    src.as_ref().map(|m| m.sample_rate).unwrap_or(rate),
                ),
            );
        }
    }
    for r in rendered {
        let room = matches!(r.file.track, Track::RoomVoices | Track::RoomEffects);
        if !(room || stems_only) {
            continue;
        }
        let Some((lane, label)) = lane_of(t.doc, &r.file) else {
            continue;
        };
        let id = format!("bed:{}", r.name);
        out.clips.push(bed(scene, &lane, &label, &id, len_s));
        media.insert(id, info(r.path.clone(), &r.name, len_s, 2, rate));
    }
    out.duration_s = out.duration_s.max(len_s);
    let options = Options {
        media,
        marks: Some(tl),
    };
    fcpxml::write_with(t.doc, t.paths, &out, name, &options)
}

/// A file already in the version folder, for the manifest.
fn listed(path: &Path, track: &str, audio: Option<(u64, u32)>) -> Result<ManifestFile> {
    Ok(ManifestFile {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        track: track.into(),
        duration_s: audio.map(|(frames, rate)| frames as f64 / f64::from(rate)),
        frames: audio.map(|(frames, _)| frames),
        bytes: std::fs::metadata(path)?.len(),
        checksum: checksum(path)?,
    })
}

/// Writes interleaved samples as a file of the export's format, and says where it went.
fn write_audio(
    dir: &Path,
    name: &str,
    samples: &[f32],
    rate: u32,
    channels: u16,
    s: &ExportSettings,
) -> Result<PathBuf> {
    let out = dir.join(format!("{name}.{}", s.codec.ext()));
    if s.codec == AudioCodec::Wav {
        apc_media::wav::write_wav_bits(&out, rate, channels, samples, s.bit_depth)?;
        return Ok(out);
    }
    let bits = if s.codec == AudioCodec::Mp3 {
        24
    } else {
        s.bit_depth
    };
    let wav = dir.join(format!("{name}.encoding.wav"));
    apc_media::wav::write_wav_bits(&wav, rate, channels, samples, bits)?;
    encode(&wav, &out, s.codec)?;
    Ok(out)
}

fn text_file(dir: &Path, name: String, track: &str, text: &str) -> Result<ManifestFile> {
    let path = dir.join(name);
    std::fs::write(&path, text)?;
    listed(&path, track, None)
}

/// Checks what can only be found out here, before anything renders.
fn preflight(s: &ExportSettings) -> Result<()> {
    s.check().map_err(|e| ExportError::Invalid(e.to_string()))?;
    if s.renders() && s.codec != AudioCodec::Wav && !ffmpeg_available() {
        return Err(ExportError::Invalid(format!(
            "{} needs ffmpeg on the PATH; WAV doesn't",
            s.codec.ext().to_uppercase()
        )));
    }
    Ok(())
}

/// Runs `work` in a hidden folder beside `folder`, and gives it `folder`'s name once it's done.
fn into_folder<T>(folder: &Path, work: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    if folder.exists() {
        return Err(ExportError::Invalid(format!(
            "{} already exists, and exports never overwrite",
            folder.display()
        )));
    }
    let parent = folder
        .parent()
        .ok_or_else(|| ExportError::Invalid("the export has no folder".into()))?;
    std::fs::create_dir_all(parent)?;
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let hidden = parent.join(format!(".{name} writing {}", std::process::id()));
    if hidden.exists() {
        std::fs::remove_dir_all(&hidden)?;
    }
    std::fs::create_dir_all(&hidden)?;
    let done = work(&hidden).and_then(|v| {
        std::fs::rename(&hidden, folder)?;
        Ok(v)
    });
    if done.is_err() {
        let _ = std::fs::remove_dir_all(&hidden);
    }
    done
}

/// Writes one version of a scene. `locked` says what the manifest records.
pub fn write_scene(
    t: &Target,
    scene: &Scene,
    version: u32,
    locked: bool,
    progress: &dyn Fn(f64, &str),
) -> Result<ExportRecord> {
    preflight(&t.settings)?;
    let s = t.settings;
    let folder = t
        .exports
        .join(t.ep.folder())
        .join(scene_folder(scene))
        .join(version_folder(version, &t.date()));
    let stem = t.ep.stem(scene.number, version);
    let sched = schedule_of(t.doc, scene);
    let tl = scene_timeline(t.doc, scene);
    let rate = s.sample_rate;
    let frames = frames_of(&sched, rate);
    let len_s = frames as f64 / f64::from(rate);
    let tracks = track_files(&s, &sched);
    let clips = clip_files(t.doc, t.paths, &tl, &s, &stem);

    let manifest = into_folder(&folder, |dir| {
        let mut files = Vec::new();
        let mut rendered = Vec::new();
        let mut warnings = Vec::new();
        let (mut mix_gain_db, mut mix_lufs) = (None, None);
        let steps = (tracks.len() + clips.len()).max(1) as f64;
        let set = source_set(t.doc, t.paths, rate);
        for (i, f) in tracks.iter().enumerate() {
            progress(i as f64 / steps, &f.suffix);
            let mut buf = render(&f.track, &sched, &set, frames)?;
            if f.track == Track::Mix {
                if let Some(target) = s.loudness {
                    let (gain, lufs, warning) = level_mix(&mut buf, rate, target)?;
                    mix_gain_db = Some(gain);
                    mix_lufs = lufs;
                    warnings.extend(warning);
                } else if buf.iter().any(|v| v.abs() > 1.0) && s.bit_depth != 32 {
                    warnings.push("The mix goes over full scale and clips.".to_string());
                }
            }
            let name = format!("{stem}_{}", f.suffix);
            let path = write_audio(dir, &name, &buf, rate, 2, &s)?;
            files.push(listed(&path, &f.suffix, Some((frames as u64, rate)))?);
            rendered.push(Rendered {
                file: f.clone(),
                path: folder.join(format!("{name}.{}", s.codec.ext())),
                name,
            });
        }
        for (i, f) in clips.iter().enumerate() {
            progress((tracks.len() + i) as f64 / steps, &f.name);
            files.push(write_clip(t, &tl.clips[f.clip], f, dir)?);
        }
        progress(1.0, "Timeline files");
        if s.writes_fcpxml() {
            let written = Written {
                rendered: &rendered,
                clips: &clips,
                folder: &folder,
                rate,
                len_s,
            };
            let xml = scene_fcpxml(t, scene, &tl, &written, &stem);
            files.push(text_file(dir, format!("{stem}.fcpxml"), "fcpxml", &xml)?);
        }
        if s.subtitles {
            files.push(text_file(
                dir,
                format!("{stem}.srt"),
                "srt",
                &srt::write(&tl, true),
            )?);
        }
        if let Some(format) = s.cut_list {
            files.push(cut_list(t, &tl, format, dir, &stem)?);
        }
        let manifest = Manifest {
            schema: MANIFEST_SCHEMA,
            version,
            date: t.now.to_rfc3339(),
            app_version: t.app_version.into(),
            settings: s,
            destination: s.save_to,
            scene_id: Some(scene.id.clone()),
            scene_number: Some(scene.number),
            heading: scene.heading.clone(),
            timeline_hash: timeline_hash(t.doc, scene)?,
            locked,
            frames: frames as u64,
            files,
            mix_gain_db,
            mix_lufs,
            warnings,
            scenes: Vec::new(),
        };
        std::fs::write(dir.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
        Ok(manifest)
    })?;
    Ok(ExportRecord { manifest, folder })
}

fn cut_list(
    t: &Target,
    tl: &EpisodeTimeline,
    format: CutListFormat,
    dir: &Path,
    stem: &str,
) -> Result<ManifestFile> {
    let rows = cutlist::rows(t.doc, t.paths, tl);
    let (text, ext) = match format {
        CutListFormat::Csv => (cutlist::csv(&rows), "csv"),
        CutListFormat::Md => (cutlist::markdown(t.title, &rows), "md"),
    };
    text_file(dir, format!("{stem}.{ext}"), "cutlist", &text)
}

/// One clip as it was recorded, handles and all: no gain, no fades, its own rate and channels.
/// Those stay on the clip in the FCPXML.
fn write_clip(t: &Target, c: &TimelineClip, f: &ClipFile, dir: &Path) -> Result<ManifestFile> {
    let m = media_info(t.doc, t.paths, &c.source_id)
        .ok_or_else(|| ExportError::Invalid(format!("the source of {} is missing", f.name)))?;
    let mut reader = apc_media::AudioReader::open(&m.path)?;
    let info = reader.info();
    let samples = reader.read_range(f.from_s, f.to_s)?;
    let path = write_audio(
        dir,
        &f.name,
        &samples,
        info.sample_rate,
        info.channels,
        &t.settings,
    )?;
    let frames = samples.len() as u64 / u64::from(info.channels.max(1));
    listed(&path, "clip", Some((frames, info.sample_rate)))
}

/// The whole-episode version: which scene versions it holds, and the episode's timeline files.
pub fn write_episode(t: &Target, version: u32, scenes: &[ManifestScene]) -> Result<ExportRecord> {
    let s = t.settings;
    let folder = t
        .exports
        .join(t.ep.folder())
        .join(EPISODE_FOLDER)
        .join(version_folder(version, &t.date()));
    let stem = t.ep.episode_stem(version);
    let tl = EpisodeTimeline::build(t.doc, &ExportScope::All);
    let manifest = into_folder(&folder, |dir| {
        let mut files = Vec::new();
        if s.writes_fcpxml() {
            let xml = fcpxml::write(t.doc, t.paths, &tl, t.title);
            files.push(text_file(dir, format!("{stem}.fcpxml"), "fcpxml", &xml)?);
        }
        if s.subtitles {
            files.push(text_file(
                dir,
                format!("{stem}.srt"),
                "srt",
                &srt::write(&tl, true),
            )?);
        }
        if let Some(format) = s.cut_list {
            files.push(cut_list(t, &tl, format, dir, &stem)?);
        }
        let manifest = Manifest {
            schema: MANIFEST_SCHEMA,
            version,
            date: t.now.to_rfc3339(),
            app_version: t.app_version.into(),
            settings: s,
            destination: s.save_to,
            scene_id: None,
            scene_number: None,
            heading: String::new(),
            timeline_hash: String::new(),
            locked: false,
            frames: 0,
            files,
            mix_gain_db: None,
            mix_lufs: None,
            warnings: Vec::new(),
            scenes: scenes.to_vec(),
        };
        std::fs::write(dir.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
        Ok(manifest)
    })?;
    Ok(ExportRecord { manifest, folder })
}

/// What an export of one scene will do.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ScenePlan {
    pub scene_id: String,
    pub number: u32,
    pub place: String,
    pub version: u32,
    /// `Sc02 v003`
    pub label: String,
    /// `Ep02/Sc02 Canteen/v003 2026-11-05`
    pub folder: String,
    pub files: Vec<String>,
    pub bytes: u64,
    pub length_s: f64,
    /// The same as this version: exporting again delivers nothing new.
    pub unchanged_since: Option<u32>,
    pub locked: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Plan {
    pub scenes: Vec<ScenePlan>,
    /// Scenes with nothing in them, left out.
    pub empty: Vec<u32>,
    /// A whole-episode export's own version.
    pub episode_version: Option<u32>,
    pub files: usize,
    pub bytes: u64,
}

/// What a second of a file takes up. FLAC is a guess, and the footer says "about".
fn bytes_per_s(s: &ExportSettings, channels: u64) -> f64 {
    let raw = f64::from(s.sample_rate) * channels as f64 * f64::from(s.bit_depth / 8);
    match s.codec {
        AudioCodec::Wav => raw,
        AudioCodec::Flac => raw * 0.6,
        AudioCodec::Mp3 => 24_000.0,
    }
}

fn plan_scene(t: &Target, scene: &Scene, history: &History) -> Result<ScenePlan> {
    let (doc, paths, s, ep) = (t.doc, t.paths, &t.settings, t.ep);
    let date = t.date();
    let version = history.next_for(&scene.id);
    let stem = ep.stem(scene.number, version);
    let sched = schedule_of(doc, scene);
    let tl = scene_timeline(doc, scene);
    let length_s = frames_of(&sched, s.sample_rate) as f64 / f64::from(s.sample_rate);
    let ext = s.codec.ext();
    let mut files = Vec::new();
    let mut bytes = 0.0;
    for f in track_files(s, &sched) {
        files.push(format!("{stem}_{}.{ext}", f.suffix));
        bytes += bytes_per_s(s, 2) * length_s;
    }
    for f in clip_files(doc, paths, &tl, s, &stem) {
        files.push(format!("{}.{ext}", f.name));
        bytes += bytes_per_s(s, 1) * (f.to_s - f.from_s);
    }
    if s.writes_fcpxml() {
        files.push(format!("{stem}.fcpxml"));
    }
    if s.subtitles {
        files.push(format!("{stem}.srt"));
    }
    match s.cut_list {
        Some(CutListFormat::Csv) => files.push(format!("{stem}.csv")),
        Some(CutListFormat::Md) => files.push(format!("{stem}.md")),
        None => {}
    }
    files.push(MANIFEST.to_string());
    Ok(ScenePlan {
        scene_id: scene.id.clone(),
        number: scene.number,
        place: scene.place.clone(),
        version,
        label: format!("Sc{:02} v{version:03}", scene.number),
        folder: format!(
            "{}/{}/{}",
            ep.folder(),
            scene_folder(scene),
            version_folder(version, &date)
        ),
        files,
        bytes: bytes as u64,
        length_s,
        unchanged_since: history.unchanged_since(&scene.id, &timeline_hash(doc, scene)?),
        locked: scene.lock.is_some(),
    })
}

/// Has the scene anything to export?
pub fn has_audio(doc: &Document, scene: &Scene) -> bool {
    schedule_of(doc, scene).end_s() > 0.0
}

/// What an export will write, without writing it.
pub fn plan(t: &Target, scenes: &[&Scene], whole_episode: bool, history: &History) -> Result<Plan> {
    let mut p = Plan {
        episode_version: whole_episode.then(|| history.next_episode()),
        ..Plan::default()
    };
    for scene in scenes {
        if !has_audio(t.doc, scene) {
            p.empty.push(scene.number);
            continue;
        }
        let sp = plan_scene(t, scene, history)?;
        p.files += sp.files.len();
        p.bytes += sp.bytes;
        p.scenes.push(sp);
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_named_like_a_lane_keeps_a_safe_file_name() {
        assert_eq!(token("Dr. Who/2"), "Dr__Who_2");
        assert_eq!(token("ÉMILE"), "ÉMILE");
    }

    #[test]
    fn a_mix_reaches_its_target_and_stops_short_of_clipping() {
        let rate = 48000;
        let tone = |a: f32| -> Vec<f32> {
            (0..rate * 2)
                .flat_map(|i| {
                    let v = a * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin();
                    [v, v]
                })
                .collect()
        };
        let mut quiet = tone(0.05);
        let (gain, lufs, warning) = level_mix(&mut quiet, rate, -23.0).unwrap();
        assert!(gain > 0.0);
        assert!((lufs.unwrap() - -23.0).abs() < 0.01);
        assert!(warning.is_none());
        let measured = apc_media::loudness::integrated_lufs(&quiet, rate, 2)
            .unwrap()
            .unwrap();
        assert!((measured - -23.0).abs() < 0.2, "{measured}");

        // quiet with one loud crack: the crack is what stops it
        let mut peaky = tone(0.02);
        peaky[48_000] = 0.9;
        let (gain, lufs, warning) = level_mix(&mut peaky, rate, -16.0).unwrap();
        let peak = peaky.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.8913).abs() < 0.001, "{peak} at {gain}");
        assert!(lufs.unwrap() < -16.0);
        assert!(warning.unwrap().contains("would clip"));
    }

    #[test]
    fn silence_has_no_loudness_to_reach() {
        let mut buf = vec![0.0f32; 96000];
        let (gain, lufs, warning) = level_mix(&mut buf, 48000, -16.0).unwrap();
        assert_eq!((gain, lufs, warning), (0.0, None, None));
    }
}
