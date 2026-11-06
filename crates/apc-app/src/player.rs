//! Playback for the UI: the cpal engine when an output device exists, otherwise a virtual
//! clock (CI, the browser dev bridge) so the transport still behaves.
//!
//! Two decks: the timeline's, and the buckets' preview player. A sound auditioned from a
//! bucket isn't on the timeline, so it has no business moving the playhead. One plays at a
//! time: starting either stops the other, and the timeline's playhead stays where it stopped.
//!
//! With `playback.level` on (the default) recordings play leveled and the output runs
//! through a safety limiter. Only the gains sent to the engine change; nothing is stored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use apc_audio::engine::Engine;
use apc_audio::mixer::SourceSet;
use apc_audio::{MixSettings, PlaySchedule};
use apc_core::config::PlaybackConfig;
use apc_export::bounce::source_set;
use apc_media::loudness::{integrated_lufs, match_gain_db};

use crate::error::Result;
use crate::service::Service;

/// One way to the speakers.
#[derive(Default)]
struct Deck {
    engine: Option<Engine>,
    tried: bool,
    clock: Option<(Instant, f64, f64)>,
    stopped_at: f64,
}

impl Deck {
    /// Opens the device the first time it's needed.
    fn open(&mut self) {
        if !self.tried && std::env::var("CUTAWAY_NO_AUDIO").is_err() {
            self.tried = true;
            self.engine = Engine::start().ok();
        }
    }

    fn position(&self) -> (f64, bool) {
        if let Some(e) = &self.engine {
            return (e.position_s(), e.is_playing());
        }
        match self.clock {
            Some((t0, from, until)) => {
                let p = from + t0.elapsed().as_secs_f64();
                if p >= until {
                    (until, false)
                } else {
                    (p, true)
                }
            }
            None => (self.stopped_at, false),
        }
    }

    /// The position, and whether it's still playing; a virtual clock that ran out rests.
    fn poll(&mut self) -> (f64, bool) {
        let (pos, playing) = self.position();
        if !playing && self.clock.is_some() {
            self.clock = None;
            self.stopped_at = pos;
        }
        (pos, playing)
    }

    fn stop(&mut self) -> f64 {
        let pos = self.position().0;
        if let Some(e) = &self.engine {
            e.stop();
        }
        self.clock = None;
        self.stopped_at = pos;
        pos
    }
}

/// The buckets' preview player.
#[derive(Default)]
struct Preview {
    deck: Deck,
    /// What it was last given (asset or remote id).
    sound: Option<String>,
    duration_s: f64,
}

#[derive(Default)]
pub struct Player {
    deck: Deck,
    preview: Preview,
    /// Take being auditioned alone (positions are then seconds into the take).
    take_id: Option<String>,
    /// Stretch of a source played alone (dev panel).
    sound: Option<String>,
    scene_id: Option<String>,
    rev: i64,
    /// What is playing: an edit that wouldn't change it isn't worth a restart.
    loaded: Option<PlaySchedule>,
    until: Option<f64>,
    /// Measured loudness of auditioned sound files.
    file_lufs: HashMap<PathBuf, Option<f64>>,
}

/// Leveling gain (dB) for a whole source: its typical take loudness to the target.
fn source_gain(svc: &Service, cfg: &PlaybackConfig, source_id: &str) -> f64 {
    if !cfg.level {
        return 0.0;
    }
    let lufs = apc_audio::level::source_lufs(svc.doc())
        .get(source_id)
        .copied();
    match_gain_db(lufs, cfg.target_lufs)
}

/// Loudness of a sound file from its first 30 s (enough for a preview, quick for a long
/// music track). `None` when it can't be read or is silent.
fn measure_file(path: &Path) -> Option<f64> {
    let mut r = apc_media::AudioReader::open(path).ok()?;
    let info = r.info();
    let raw = r.read_range(0.0, 30.0).ok()?;
    integrated_lufs(&raw, info.sample_rate, info.channels)
        .ok()
        .flatten()
}

fn schedule(svc: &Service, scene_id: &str) -> Option<PlaySchedule> {
    let scene = svc.doc().scenes.get(scene_id)?;
    let cfg = svc.config();
    let settings = MixSettings {
        room_tone: cfg.pacing.room_tone,
        level: cfg.playback.level,
        target_lufs: cfg.playback.target_lufs,
        ..MixSettings::default()
    };
    Some(PlaySchedule::from_scene(
        svc.doc(),
        scene,
        &cfg.pacing(),
        &settings,
    ))
}

pub fn play(svc: &mut Service, scene_id: &str, from_s: f64, until_s: Option<f64>) -> Result<Value> {
    let Some(sched) = schedule(svc, scene_id) else {
        return Ok(json!({ "playing": false }));
    };
    svc.player.take_id = None;
    svc.player.sound = None;
    start(svc, sched, scene_id, from_s, until_s)
}

/// Plays one take alone, loudness-matched, from `from_s` seconds into it.
pub fn play_take(svc: &mut Service, take_id: &str, from_s: f64) -> Result<Value> {
    let t = svc
        .doc()
        .takes
        .get(take_id)
        .cloned()
        .ok_or_else(|| crate::AppError::Params(format!("no take '{take_id}'")))?;
    let scene_id = svc
        .doc()
        .lines
        .get(&t.line_id)
        .map(|l| l.scene_id.clone())
        .unwrap_or_default();
    // takes are compared at equal loudness, each take to the target on its own
    let cfg = svc.config().playback;
    let gain_db = if cfg.level {
        match_gain_db(t.lufs, cfg.target_lufs)
    } else {
        0.0
    };
    let clip = apc_audio::ScheduledClip {
        source_id: t.source_id.clone(),
        src_in: t.t0,
        src_out: t.t1,
        start_s: 0.0,
        gain_db,
        fade_in_s: 0.005,
        fade_out_s: 0.005,
        lane: "audition".into(),
        loop_len_s: None,
        pan: 0.0,
        linear_fade: false,
        owner: t.line_id.clone(),
        space: None,
    };
    let sched = PlaySchedule {
        clips: vec![clip],
        duration_s: t.duration(),
        ..Default::default()
    };
    let r = start(svc, sched, &scene_id, from_s.clamp(0.0, t.duration()), None)?;
    svc.player.take_id = Some(take_id.to_string());
    Ok(r)
}

/// Plays a stretch of a source file alone, as recorded apart from leveling (dev panel). The
/// playhead then reports seconds into the stretch.
pub fn play_source(svc: &mut Service, source_id: &str, t0: f64, t1: f64) -> Result<Value> {
    if !svc.doc().sources.contains_key(source_id) {
        return Err(crate::AppError::Params(format!(
            "unknown source '{source_id}'"
        )));
    }
    let (t0, t1) = (t0.max(0.0), t1.max(t0));
    let gain_db = source_gain(svc, &svc.config().playback, source_id);
    let clip = apc_audio::ScheduledClip {
        source_id: source_id.to_string(),
        src_in: t0,
        src_out: t1,
        start_s: 0.0,
        gain_db,
        fade_in_s: 0.005,
        fade_out_s: 0.005,
        lane: "audition".into(),
        loop_len_s: None,
        pan: 0.0,
        linear_fade: false,
        owner: source_id.to_string(),
        space: None,
    };
    let sched = PlaySchedule {
        clips: vec![clip],
        duration_s: t1 - t0,
        ..Default::default()
    };
    let scene = svc.player.scene_id.clone().unwrap_or_default();
    let r = start(svc, sched, &scene, 0.0, None)?;
    svc.player.take_id = None;
    svc.player.sound = Some(format!("source:{source_id}"));
    Ok(r)
}

/// Plays a sound file on the preview player (bucket audition: a library sound or a cached
/// preview). `sound_playhead` then reports `sound` = the label, and seconds into the file.
pub fn play_file(
    svc: &mut Service,
    path: &std::path::Path,
    label: &str,
    from_s: f64,
) -> Result<Value> {
    let info = apc_media::decode::probe(path)?;
    let duration = if info.duration_s > 0.0 {
        info.duration_s
    } else {
        apc_media::AudioAnalysis::compute(path)?.info.duration_s
    };
    let id = format!("file:{label}");
    // sounds audition loudness-matched, like takes
    let cfg = svc.config().playback;
    let gain_db = if cfg.level {
        let lufs = *svc
            .player
            .file_lufs
            .entry(path.to_path_buf())
            .or_insert_with(|| measure_file(path));
        match_gain_db(lufs, cfg.target_lufs)
    } else {
        0.0
    };
    on_preview(svc, id, path, label, (0.0, duration), gain_db, from_s)
}

/// Plays a stretch of a source on the preview player (the mapping editor).
/// `sound_playhead` then reports seconds into the stretch.
pub fn play_stretch(svc: &mut Service, clip: &crate::methods::ClipP, from_s: f64) -> Result<Value> {
    let Some(src) = svc.doc().sources.get(&clip.source_id).cloned() else {
        return Err(crate::AppError::Params(format!(
            "unknown source '{}'",
            clip.source_id
        )));
    };
    let gain_db = source_gain(svc, &svc.config().playback, &src.id);
    let path = svc.paths.resolve(&src.path);
    let id = format!("source:{}", src.id);
    let range = (clip.t0.max(0.0), clip.t1.max(clip.t0));
    on_preview(svc, id.clone(), &path, &id, range, gain_db, from_s)
}

/// Loads `path` from `range.0` to `range.1` on the preview deck and plays it from `from_s`
/// (seconds into the range).
fn on_preview(
    svc: &mut Service,
    id: String,
    path: &std::path::Path,
    label: &str,
    range: (f64, f64),
    gain_db: f64,
    from_s: f64,
) -> Result<Value> {
    let duration = range.1 - range.0;
    let clip = apc_audio::ScheduledClip {
        source_id: id.clone(),
        src_in: range.0,
        src_out: range.1,
        start_s: 0.0,
        gain_db,
        fade_in_s: 0.003,
        fade_out_s: 0.01,
        lane: "audition".into(),
        loop_len_s: None,
        pan: 0.0,
        linear_fade: false,
        owner: label.to_string(),
        space: None,
    };
    let sched = PlaySchedule {
        clips: vec![clip],
        duration_s: duration,
        ..Default::default()
    };
    let from_s = from_s.clamp(0.0, duration);
    let level = svc.config().playback.level;
    stop_timeline(svc);
    let p = &mut svc.player.preview;
    p.deck.open();
    if let Some(e) = &p.deck.engine {
        let mut set = SourceSet::new(e.sample_rate());
        set.add(id, path.to_path_buf());
        e.set_limiter(level);
        e.load(sched, Arc::new(set));
        e.play(from_s, Some(duration));
    } else {
        p.deck.clock = Some((Instant::now(), from_s, duration));
    }
    p.sound = Some(label.to_string());
    p.duration_s = duration;
    Ok(
        json!({ "playing": true, "from_s": from_s, "until_s": duration, "duration_s": duration, "device": p.deck.engine.is_some() }),
    )
}

fn start(
    svc: &mut Service,
    sched: PlaySchedule,
    scene_id: &str,
    from_s: f64,
    until_s: Option<f64>,
) -> Result<Value> {
    svc.player.preview.deck.stop();
    // the room rings on past the last clip, as it does in the bounce
    let duration = sched.end_s();
    svc.player.deck.open();
    let until = until_s.unwrap_or(duration).min(duration);
    svc.player.loaded = Some(sched.clone());
    if let Some(e) = &svc.player.deck.engine {
        let set = source_set(svc.doc(), &svc.paths, e.sample_rate());
        e.set_limiter(svc.config().playback.level);
        e.load(sched, Arc::new(set));
        e.play(from_s, Some(until));
    } else {
        svc.player.deck.clock = Some((Instant::now(), from_s, until));
    }
    svc.player.scene_id = Some(scene_id.to_string());
    svc.player.rev = svc.bus.rev();
    svc.player.until = Some(until);
    Ok(
        json!({ "playing": true, "from_s": from_s, "until_s": until, "duration_s": duration, "device": svc.player.deck.engine.is_some() }),
    )
}

fn stop_timeline(svc: &mut Service) -> f64 {
    svc.player.deck.stop()
}

/// Stops whatever plays.
pub fn stop(svc: &mut Service) -> Result<Value> {
    svc.player.preview.deck.stop();
    let pos = stop_timeline(svc);
    Ok(json!({ "playing": false, "position_s": pos }))
}

/// Stops the preview player alone.
pub fn stop_sound(svc: &mut Service) -> Result<Value> {
    let pos = svc.player.preview.deck.stop();
    Ok(json!({ "playing": false, "position_s": pos }))
}

/// Where the preview player is: seconds into its sound.
pub fn sound_playhead(svc: &mut Service) -> Result<Value> {
    let p = &mut svc.player.preview;
    let (pos, playing) = p.deck.poll();
    Ok(
        json!({ "sound": p.sound, "position_s": pos, "playing": playing, "duration_s": p.duration_s }),
    )
}

/// The scene's schedule, if the document changed during playback in a way that can be heard.
/// A change here (the room, a sound) is in the document already; another process's arrives
/// through `refresh`. A favorite or a note changes nothing heard, so it doesn't restart anything.
fn changed_schedule(svc: &mut Service) -> Option<PlaySchedule> {
    svc.bus.refresh().ok();
    if svc.player.take_id.is_some() || svc.player.sound.is_some() || svc.player.rev == svc.bus.rev()
    {
        return None;
    }
    svc.player.rev = svc.bus.rev();
    let sid = svc.player.scene_id.clone()?;
    schedule(svc, &sid).filter(|s| svc.player.loaded.as_ref() != Some(s))
}

/// Playhead; reloads the schedule when the document changed during playback.
pub fn playhead(svc: &mut Service) -> Result<Value> {
    let (pos, playing) = svc.player.deck.poll();
    if playing {
        if let Some(s) = changed_schedule(svc) {
            if let Some(e) = &svc.player.deck.engine {
                e.set_limiter(svc.config().playback.level);
                e.load(
                    s.clone(),
                    Arc::new(source_set(svc.doc(), &svc.paths, e.sample_rate())),
                );
                e.play(pos, svc.player.until);
            }
            svc.player.loaded = Some(s);
        }
    }
    Ok(
        json!({ "position_s": pos, "playing": playing, "scene_id": svc.player.scene_id, "take_id": svc.player.take_id, "sound": svc.player.sound, "level": svc.player.deck.engine.as_ref().map(|e| e.take_level()).unwrap_or(0.0) }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A demo scene with one sound, playing on the virtual clock.
    fn playing(dir: &Path) -> (Service, String, String) {
        std::env::set_var("CUTAWAY_SECRETS", "memory");
        std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
        std::env::set_var("CUTAWAY_NO_AUDIO", "1");
        let mut svc = crate::demo::create(dir, "test", true).unwrap();
        let file = dir.join("Door.wav");
        apc_media::wav::write_wav(&file, 48000, 1, &vec![0.1; 48000]).unwrap();
        let r = svc
            .call(
                "import_sounds",
                &json!({ "paths": [file.to_string_lossy()] }),
            )
            .unwrap();
        let asset = r["assets"][0]["id"].as_str().unwrap().to_string();
        let placed = svc
            .call(
                "place_sound",
                &json!({ "asset_id": asset, "scene": "1", "at_s": 0.5 }),
            )
            .unwrap();
        let id = placed["id"].as_str().unwrap().to_string();
        let scene = svc.doc().find_scene("1").unwrap().id.clone();
        play(&mut svc, &scene, 0.0, None).unwrap();
        (svc, scene, id)
    }

    fn room(svc: &mut Service, scene: &str, size: f64) {
        svc.call(
            "command",
            &json!({ "cmd": "set_scene_room", "scene_id": scene, "room": { "size": size, "damping": 0.5, "wet": 0.5 } }),
        )
        .unwrap();
    }

    #[test]
    fn a_change_of_room_during_playback_reaches_the_schedule() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, scene, _) = playing(dir.path());
        assert!(changed_schedule(&mut svc).is_none(), "nothing changed yet");
        room(&mut svc, &scene, 0.9);
        let s = changed_schedule(&mut svc).expect("heard while it plays");
        assert_ne!(Some(&s), svc.player.loaded.as_ref());
        // and undo is heard the same way
        svc.player.loaded = Some(s);
        svc.call("undo", &json!({})).unwrap();
        assert!(changed_schedule(&mut svc).is_some());
    }

    #[test]
    fn a_change_to_a_sound_during_playback_reaches_the_schedule() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, _, id) = playing(dir.path());
        svc.call(
            "command",
            &json!({ "cmd": "set_sound_gain", "placement_id": id, "gain_db": -6.0 }),
        )
        .unwrap();
        assert!(changed_schedule(&mut svc).is_some());
    }

    #[test]
    fn an_edit_nobody_hears_restarts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, _, _) = playing(dir.path());
        let take = svc.doc().takes.keys().next().unwrap().clone();
        let rev = svc.bus.rev();
        svc.call(
            "command",
            &json!({ "cmd": "toggle_favorite", "take_id": take }),
        )
        .unwrap();
        assert_ne!(svc.bus.rev(), rev, "the document did change");
        assert!(changed_schedule(&mut svc).is_none());
    }

    fn door(dir: &Path) -> PathBuf {
        dir.join("Door.wav")
    }

    #[test]
    fn a_preview_leaves_the_timeline_where_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, scene, _) = playing(dir.path());
        let loaded = svc.player.loaded.clone();
        let at = stop(&mut svc).unwrap()["position_s"].as_f64().unwrap();

        play_file(&mut svc, &door(dir.path()), "Door", 0.25).unwrap();

        let head = playhead(&mut svc).unwrap();
        assert_eq!(head["playing"], false);
        assert_eq!(head["position_s"].as_f64().unwrap(), at);
        assert_eq!(head["scene_id"], scene.as_str());
        assert_eq!(svc.player.loaded, loaded, "the timeline's schedule stays");
        let heard = sound_playhead(&mut svc).unwrap();
        assert_eq!(heard["playing"], true);
        assert_eq!(heard["sound"], "Door");
        assert!(heard["position_s"].as_f64().unwrap() >= 0.25);
        assert!((heard["duration_s"].as_f64().unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_preview_stops_the_timeline_and_the_timeline_stops_a_preview() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, scene, _) = playing(dir.path());
        assert_eq!(playhead(&mut svc).unwrap()["playing"], true);
        play_file(&mut svc, &door(dir.path()), "Door", 0.0).unwrap();
        assert_eq!(playhead(&mut svc).unwrap()["playing"], false);
        play(&mut svc, &scene, 0.0, None).unwrap();
        assert_eq!(sound_playhead(&mut svc).unwrap()["playing"], false);
        assert_eq!(playhead(&mut svc).unwrap()["playing"], true);
    }

    #[test]
    fn stopping_a_preview_is_all_it_stops() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, _, _) = playing(dir.path());
        let at = stop(&mut svc).unwrap()["position_s"].as_f64().unwrap();
        play_file(&mut svc, &door(dir.path()), "Door", 0.0).unwrap();
        stop_sound(&mut svc).unwrap();
        assert_eq!(sound_playhead(&mut svc).unwrap()["playing"], false);
        assert_eq!(
            playhead(&mut svc).unwrap()["position_s"].as_f64().unwrap(),
            at
        );
        // and a plain stop reaches it too: closing an episode must leave nothing playing
        play_file(&mut svc, &door(dir.path()), "Door", 0.0).unwrap();
        stop(&mut svc).unwrap();
        assert_eq!(sound_playhead(&mut svc).unwrap()["playing"], false);
    }

    #[test]
    fn an_audition_stays_what_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, scene, _) = playing(dir.path());
        svc.player.sound = Some("Door".into());
        room(&mut svc, &scene, 0.9);
        assert!(changed_schedule(&mut svc).is_none());
    }
}
