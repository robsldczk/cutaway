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
    let duration = sched.duration_s;
    let use_device = std::env::var("CUTAWAY_NO_AUDIO").is_err();
    if !svc.player.tried && use_device {
        svc.player.tried = true;
        svc.player.engine = Engine::start().ok();
    }
    let until = until_s.unwrap_or(duration).min(duration);
    if let Some(e) = &svc.player.engine {
        let set = Arc::new(source_set(svc.doc(), &svc.paths, e.sample_rate()));
        e.load(sched, set);
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
    if playing && svc.bus.refresh().unwrap_or(false) && svc.player.rev != svc.bus.rev() {
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
    Ok(json!({ "position_s": pos, "playing": playing, "scene_id": svc.player.scene_id }))
}
