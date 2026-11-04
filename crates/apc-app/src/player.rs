//! Playback for the UI: the cpal engine when an output device exists, otherwise a virtual
//! clock (CI, the browser dev bridge) so the transport still behaves.
//!
//! With `playback.level` on (the default) recordings play leveled and the output runs
//! through a safety limiter. Only the gains sent to the engine change; nothing is stored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use apc_audio::engine::Engine;
use apc_audio::{MixSettings, PlaySchedule};
use apc_core::config::PlaybackConfig;
use apc_export::bounce::source_set;
use apc_media::loudness::{integrated_lufs, match_gain_db};

use crate::error::Result;
use crate::service::Service;

#[derive(Default)]
pub struct Player {
    engine: Option<Engine>,
    /// Take being auditioned alone (positions are then seconds into the take).
    take_id: Option<String>,
    /// Sound auditioned from a bucket.
    sound: Option<String>,
    tried: bool,
    clock: Option<(Instant, f64, f64)>,
    stopped_at: f64,
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
    start(svc, sched, scene_id, from_s, until_s, vec![])
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
    let r = start(
        svc,
        sched,
        &scene_id,
        from_s.clamp(0.0, t.duration()),
        None,
        vec![],
    )?;
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
    let r = start(svc, sched, &scene, 0.0, None, vec![])?;
    svc.player.take_id = None;
    svc.player.sound = Some(format!("source:{source_id}"));
    Ok(r)
}

/// Plays a sound file alone (bucket audition: a library sound or a cached preview).
/// The playhead then reports `sound` = the label, and seconds into the file.
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
    let clip = apc_audio::ScheduledClip {
        source_id: id.clone(),
        src_in: 0.0,
        src_out: duration,
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
    let scene = svc.player.scene_id.clone().unwrap_or_default();
    let r = start(
        svc,
        sched,
        &scene,
        from_s.clamp(0.0, duration),
        None,
        vec![(id, path.to_path_buf())],
    )?;
    svc.player.take_id = None;
    svc.player.sound = Some(label.to_string());
    Ok(r)
}

fn start(
    svc: &mut Service,
    sched: PlaySchedule,
    scene_id: &str,
    from_s: f64,
    until_s: Option<f64>,
    extra: Vec<(String, std::path::PathBuf)>,
) -> Result<Value> {
    // the room rings on past the last clip, as it does in the bounce
    let duration = sched.end_s();
    let use_device = std::env::var("CUTAWAY_NO_AUDIO").is_err();
    if !svc.player.tried && use_device {
        svc.player.tried = true;
        svc.player.engine = Engine::start().ok();
    }
    let until = until_s.unwrap_or(duration).min(duration);
    svc.player.loaded = Some(sched.clone());
    if let Some(e) = &svc.player.engine {
        let mut set = source_set(svc.doc(), &svc.paths, e.sample_rate());
        for (id, p) in extra {
            set.add(id, p);
        }
        e.set_limiter(svc.config().playback.level);
        e.load(sched, Arc::new(set));
        e.play(from_s, Some(until));
    } else {
        svc.player.clock = Some((Instant::now(), from_s, until));
    }
    svc.player.scene_id = Some(scene_id.to_string());
    svc.player.rev = svc.bus.rev();
    svc.player.until = Some(until);
    Ok(
        json!({ "playing": true, "from_s": from_s, "until_s": until, "duration_s": duration, "device": svc.player.engine.is_some() }),
    )
}

pub fn stop(svc: &mut Service) -> Result<Value> {
    let pos = position(svc).0;
    if let Some(e) = &svc.player.engine {
        e.stop();
    }
    svc.player.clock = None;
    svc.player.stopped_at = pos;
    Ok(json!({ "playing": false, "position_s": pos }))
}

fn position(svc: &Service) -> (f64, bool) {
    if let Some(e) = &svc.player.engine {
        return (e.position_s(), e.is_playing());
    }
    match svc.player.clock {
        Some((t0, from, until)) => {
            let p = from + t0.elapsed().as_secs_f64();
            if p >= until {
                (until, false)
            } else {
                (p, true)
            }
        }
        None => (svc.player.stopped_at, false),
    }
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
    let (pos, playing) = position(svc);
    if !playing && svc.player.clock.is_some() {
        svc.player.clock = None;
        svc.player.stopped_at = pos;
    }
    if playing {
        if let Some(s) = changed_schedule(svc) {
            if let Some(e) = &svc.player.engine {
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
        json!({ "position_s": pos, "playing": playing, "scene_id": svc.player.scene_id, "take_id": svc.player.take_id, "sound": svc.player.sound, "level": svc.player.engine.as_ref().map(|e| e.take_level()).unwrap_or(0.0) }),
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

    #[test]
    fn an_audition_stays_what_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let (mut svc, scene, _) = playing(dir.path());
        svc.player.sound = Some("Door".into());
        room(&mut svc, &scene, 0.9);
        assert!(changed_schedule(&mut svc).is_none());
    }
}
