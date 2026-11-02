//! Playback for the UI: the cpal engine when an output device exists, otherwise a virtual
//! clock (CI, the browser dev bridge) so the transport still behaves.

use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use apc_audio::engine::Engine;
use apc_audio::{MixSettings, PlaySchedule};
use apc_export::bounce::source_set;

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
    until: Option<f64>,
}

fn schedule(svc: &Service, scene_id: &str) -> Option<PlaySchedule> {
    let scene = svc.doc().scenes.get(scene_id)?;
    let cfg = svc.config();
    let settings = MixSettings {
        room_tone: cfg.pacing.room_tone,
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
    let clip = apc_audio::ScheduledClip {
        source_id: t.source_id.clone(),
        src_in: t.t0,
        src_out: t.t1,
        start_s: 0.0,
        gain_db: 0.0,
        fade_in_s: 0.005,
        fade_out_s: 0.005,
        lane: "audition".into(),
        loop_len_s: None,
        pan: 0.0,
        linear_fade: false,
        owner: t.line_id.clone(),
    };
    let sched = PlaySchedule {
        clips: vec![clip],
        duration_s: t.duration(),
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
    let clip = apc_audio::ScheduledClip {
        source_id: id.clone(),
        src_in: 0.0,
        src_out: duration,
        start_s: 0.0,
        gain_db: 0.0,
        fade_in_s: 0.003,
        fade_out_s: 0.01,
        lane: "audition".into(),
        loop_len_s: None,
        pan: 0.0,
        linear_fade: false,
        owner: label.to_string(),
    };
    let sched = PlaySchedule {
        clips: vec![clip],
        duration_s: duration,
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
    let duration = sched.duration_s;
    let use_device = std::env::var("CUTAWAY_NO_AUDIO").is_err();
    if !svc.player.tried && use_device {
        svc.player.tried = true;
        svc.player.engine = Engine::start().ok();
    }
    let until = until_s.unwrap_or(duration).min(duration);
    if let Some(e) = &svc.player.engine {
        let mut set = source_set(svc.doc(), &svc.paths, e.sample_rate());
        for (id, p) in extra {
            set.add(id, p);
        }
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

/// Playhead; reloads the schedule when the document changed during playback.
pub fn playhead(svc: &mut Service) -> Result<Value> {
    let (pos, playing) = position(svc);
    if !playing && svc.player.clock.is_some() {
        svc.player.clock = None;
        svc.player.stopped_at = pos;
    }
    if playing
        && svc.player.take_id.is_none()
        && svc.bus.refresh().unwrap_or(false)
        && svc.player.rev != svc.bus.rev()
    {
        if let (Some(sid), Some(e)) = (svc.player.scene_id.clone(), &svc.player.engine) {
            if let Some(s) = schedule(svc, &sid) {
                e.load(
                    s,
                    Arc::new(source_set(svc.doc(), &svc.paths, e.sample_rate())),
                );
                e.play(pos, svc.player.until);
            }
        }
        svc.player.rev = svc.bus.rev();
    }
    Ok(
        json!({ "position_s": pos, "playing": playing, "scene_id": svc.player.scene_id, "take_id": svc.player.take_id, "sound": svc.player.sound, "level": svc.player.engine.as_ref().map(|e| e.take_level()).unwrap_or(0.0) }),
    )
}
