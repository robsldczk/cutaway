//! Real-time playback (cpal). A feeder thread renders ahead into a ring buffer through the
//! shared mixer; the device callback only copies samples. Playhead events at ~60 Hz.
//! The feeder can run the output through the safety limiter (playback leveling).
//! The room runs in the feeder, never in the device callback; after a seek the `Renderer`
//! warms it up, so what plays matches the bounce from the first sample.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::error::{AudioError, Result};
use crate::level::Limiter;
use crate::mixer::{Renderer, SourceSet};
use crate::schedule::PlaySchedule;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct PlayheadEvent {
    pub position_s: f64,
    pub playing: bool,
    /// Output peak since the last event (0..1).
    pub level: f32,
}

struct Shared {
    ring: Mutex<VecDeque<f32>>,
    playing: AtomicBool,
    /// Frames the device has played since `start_frame`.
    played: AtomicU64,
    start_frame: AtomicU64,
    /// Bumped on every play/seek/stop so stale feeder blocks are dropped.
    generation: AtomicU64,
    stop_at: Mutex<Option<f64>>,
    /// Output peak (f32 bits), reset when read.
    peak: AtomicU32,
    /// Safety limiter on the output.
    limit: AtomicBool,
    content: Mutex<Option<(Arc<PlaySchedule>, Arc<SourceSet>)>>,
}

enum Cmd {
    Quit,
}

/// The playback engine. Cheap to share (`Send + Sync`); the cpal stream lives on its own thread.
pub struct Engine {
    shared: Arc<Shared>,
    rate: u32,
    tx: Sender<Cmd>,
    subscribers: Arc<Mutex<Vec<Sender<PlayheadEvent>>>>,
}

const AHEAD_FRAMES: usize = 9600; // 200 ms at 48 kHz
const BLOCK: usize = 1024;

impl Engine {
    /// Opens the default output device. Fails when there is none (e.g. on CI).
    pub fn start() -> Result<Engine> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let shared = Arc::new(Shared {
            ring: Mutex::new(VecDeque::with_capacity(AHEAD_FRAMES * 4)),
            playing: AtomicBool::new(false),
            played: AtomicU64::new(0),
            start_frame: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            stop_at: Mutex::new(None),
            peak: AtomicU32::new(0),
            limit: AtomicBool::new(false),
            content: Mutex::new(None),
        });
        let (tx, rx) = unbounded::<Cmd>();
        let (ready_tx, ready_rx) = unbounded::<std::result::Result<u32, String>>();
        let sh = shared.clone();
        thread::Builder::new()
            .name("cutaway-audio".into())
            .spawn(move || {
                let host = cpal::default_host();
                let Some(device) = host.default_output_device() else {
                    let _ = ready_tx.send(Err("no output device".into()));
                    return;
                };
                let cfg = match device.default_output_config() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let mut config: cpal::StreamConfig = cfg.into();
                config.channels = 2;
                let rate = config.sample_rate;
                let cb_shared = sh.clone();
                let stream = device.build_output_stream(
                    config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut got = 0;
                        let mut peak = 0.0f32;
                        if cb_shared.playing.load(Ordering::Relaxed) {
                            if let Ok(mut ring) = cb_shared.ring.try_lock() {
                                for d in data.iter_mut() {
                                    match ring.pop_front() {
                                        Some(v) => {
                                            *d = v;
                                            peak = peak.max(v.abs());
                                            got += 1;
                                        }
                                        None => *d = 0.0,
                                    }
                                }
                            } else {
                                data.fill(0.0);
                            }
                        } else {
                            data.fill(0.0);
                        }
                        cb_shared
                            .played
                            .fetch_add((got / 2) as u64, Ordering::Relaxed);
                        // keep the loudest block since the meter last read it
                        let _ = cb_shared.peak.fetch_update(
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                            |old| (peak > f32::from_bits(old)).then_some(peak.to_bits()),
                        );
                    },
                    |e| eprintln!("audio stream error: {e}"),
                    None,
                );
                let stream = match stream {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
                let _ = ready_tx.send(Ok(rate));
                // keep the stream alive until told to quit (or the engine is dropped)
                let _ = rx.recv();
                drop(stream);
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let rate = ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| AudioError::Device("audio thread did not start".into()))?
            .map_err(AudioError::Device)?;
        let engine = Engine {
            shared,
            rate,
            tx,
            subscribers: Arc::new(Mutex::new(Vec::new())),
        };
        engine.spawn_feeder();
        engine.spawn_ticker();
        Ok(engine)
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn spawn_feeder(&self) {
        let sh = self.shared.clone();
        let rate = self.rate;
        let mut limiter = Limiter::new(self.rate);
        thread::spawn(move || {
            let mut renderer: Option<(Arc<PlaySchedule>, Renderer)> = None;
            // The feeder counts its own frames within a generation: `played + queued` reads
            // two values the device callback changes one after the other, so it can come out
            // a block short, and frames must reach the room once, in order.
            let mut seen_gen = None;
            let mut next_frame = 0u64;
            loop {
                if Arc::strong_count(&sh) == 1 {
                    break;
                }
                if !sh.playing.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                let content = sh.content.lock().ok().and_then(|c| c.clone());
                let Some((sched, sources)) = content else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let gen = sh.generation.load(Ordering::Relaxed);
                let queued = sh.ring.lock().map(|r| r.len() / 2).unwrap_or(0);
                if seen_gen != Some(gen) {
                    seen_gen = Some(gen);
                    next_frame = sh.start_frame.load(Ordering::Relaxed)
                        + sh.played.load(Ordering::Relaxed)
                        + queued as u64;
                }
                if queued >= AHEAD_FRAMES {
                    thread::sleep(Duration::from_millis(3));
                    continue;
                }
                let current = renderer
                    .as_ref()
                    .is_some_and(|(s, _)| Arc::ptr_eq(s, &sched));
                if !current {
                    renderer = Some((sched.clone(), Renderer::new(&sched, rate)));
                }
                let mut block = vec![0.0f32; BLOCK * 2];
                if let Some((_, r)) = renderer.as_mut() {
                    r.render(&sched, &sources, next_frame, &mut block);
                }
                if sh.limit.load(Ordering::Relaxed) {
                    limiter.process(&mut block);
                }
                if sh.generation.load(Ordering::Relaxed) == gen {
                    if let Ok(mut r) = sh.ring.lock() {
                        r.extend(block);
                        next_frame += BLOCK as u64;
                    }
                }
            }
        });
    }

    fn spawn_ticker(&self) {
        let sh = self.shared.clone();
        let subs = self.subscribers.clone();
        let rate = self.rate as f64;
        thread::spawn(move || loop {
            if Arc::strong_count(&sh) == 1 {
                break;
            }
            thread::sleep(Duration::from_millis(16));
            let playing = sh.playing.load(Ordering::Relaxed);
            let pos = (sh.start_frame.load(Ordering::Relaxed) + sh.played.load(Ordering::Relaxed))
                as f64
                / rate;
            let end = sh
                .content
                .lock()
                .ok()
                .and_then(|c| c.as_ref().map(|(s, _)| s.end_s()))
                .unwrap_or(0.0);
            let stop_at = sh.stop_at.lock().ok().and_then(|s| *s).unwrap_or(end);
            if playing && pos >= stop_at {
                sh.playing.store(false, Ordering::Relaxed);
            }
            if let Ok(mut list) = subs.lock() {
                list.retain(|s| {
                    s.send(PlayheadEvent {
                        position_s: pos,
                        playing,
                        level: f32::from_bits(sh.peak.load(Ordering::Relaxed)),
                    })
                    .is_ok()
                });
            }
        });
    }

    /// Loads what to play. Sources must be at the engine's sample rate.
    pub fn load(&self, schedule: PlaySchedule, sources: Arc<SourceSet>) {
        if let Ok(mut c) = self.shared.content.lock() {
            *c = Some((Arc::new(schedule), sources));
        }
        self.reset_ring();
    }

    fn reset_ring(&self) {
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut r) = self.shared.ring.lock() {
            r.clear();
        }
    }

    /// Starts playback at `from_s`, optionally stopping at `until_s`.
    pub fn play(&self, from_s: f64, until_s: Option<f64>) {
        self.shared.playing.store(false, Ordering::Relaxed);
        self.reset_ring();
        self.shared.start_frame.store(
            (from_s.max(0.0) * self.rate as f64) as u64,
            Ordering::Relaxed,
        );
        self.shared.played.store(0, Ordering::Relaxed);
        if let Ok(mut s) = self.shared.stop_at.lock() {
            *s = until_s;
        }
        // a block the feeder started before the new position was set must not count
        self.reset_ring();
        self.shared.playing.store(true, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        let pos = self.position_s();
        self.shared.playing.store(false, Ordering::Relaxed);
        self.reset_ring();
        self.shared
            .start_frame
            .store((pos * self.rate as f64) as u64, Ordering::Relaxed);
        self.shared.played.store(0, Ordering::Relaxed);
    }

    pub fn seek(&self, to_s: f64) {
        if self.is_playing() {
            self.play(to_s, None);
        } else {
            self.shared
                .start_frame
                .store((to_s.max(0.0) * self.rate as f64) as u64, Ordering::Relaxed);
            self.shared.played.store(0, Ordering::Relaxed);
        }
    }

    /// Turns the output safety limiter on or off.
    pub fn set_limiter(&self, on: bool) {
        self.shared.limit.store(on, Ordering::Relaxed);
    }

    /// Output peak since the last call (0..1).
    pub fn take_level(&self) -> f32 {
        f32::from_bits(self.shared.peak.swap(0, Ordering::Relaxed))
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    pub fn position_s(&self) -> f64 {
        (self.shared.start_frame.load(Ordering::Relaxed)
            + self.shared.played.load(Ordering::Relaxed)) as f64
            / self.rate as f64
    }

    /// Playhead events at ~60 Hz.
    pub fn subscribe(&self) -> Receiver<PlayheadEvent> {
        let (tx, rx) = unbounded();
        if let Ok(mut s) = self.subscribers.lock() {
            s.push(tx);
        }
        rx
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        let _ = self.tx.send(Cmd::Quit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::ScheduledClip;

    /// The timeline and the preview player each hold an engine: two streams on one device.
    /// Silent, so a test run makes no noise (skips without a device).
    #[test]
    fn two_engines_keep_their_own_time() {
        if std::env::var("CI").is_ok() {
            eprintln!("skipped: no audio device on CI");
            return;
        }
        let (Ok(a), Ok(b)) = (Engine::start(), Engine::start()) else {
            eprintln!("skipped: no output device");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let sr = a.sample_rate();
        let p = dir.path().join("s.wav");
        apc_media::wav::write_wav(&p, sr, 1, &vec![0.0; sr as usize * 2]).unwrap();
        for (e, from) in [(&a, 0.0), (&b, 1.0)] {
            let mut set = SourceSet::new(e.sample_rate());
            set.add("s", p.clone());
            let clip = ScheduledClip {
                source_id: "s".into(),
                src_in: 0.0,
                src_out: 2.0,
                start_s: 0.0,
                gain_db: 0.0,
                fade_in_s: 0.0,
                fade_out_s: 0.0,
                lane: "audition".into(),
                loop_len_s: None,
                pan: 0.0,
                linear_fade: false,
                owner: "s".into(),
                space: None,
            };
            let s = PlaySchedule {
                clips: vec![clip],
                duration_s: 2.0,
                ..Default::default()
            };
            e.load(s, Arc::new(set));
            e.play(from, None);
        }
        thread::sleep(Duration::from_millis(400));
        assert!(a.is_playing() && b.is_playing());
        assert!(a.position_s() > 0.1, "a at {}", a.position_s());
        assert!(b.position_s() > 1.1, "b at {}", b.position_s());
        a.stop();
        assert!(b.is_playing(), "stopping one leaves the other");
    }

    /// Plays 300 ms of a test tone on the default device (skips without one).
    #[test]
    fn plays_and_reports_the_playhead() {
        if std::env::var("CI").is_ok() {
            eprintln!("skipped: no audio device on CI");
            return;
        }
        let Ok(engine) = Engine::start() else {
            eprintln!("skipped: no output device");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let sr = engine.sample_rate();
        let tone: Vec<f32> = (0..sr)
            .map(|i| 0.05 * (i as f32 * 440.0 * std::f32::consts::TAU / sr as f32).sin())
            .collect();
        let p = dir.path().join("t.wav");
        apc_media::wav::write_wav(&p, sr, 1, &tone).unwrap();
        let mut set = SourceSet::new(sr);
        set.add("t", p);
        let clip = ScheduledClip {
            source_id: "t".into(),
            src_in: 0.0,
            src_out: 1.0,
            start_s: 0.0,
            gain_db: 0.0,
            fade_in_s: 0.01,
            fade_out_s: 0.01,
            lane: "x".into(),
            loop_len_s: None,
            pan: 0.0,
            linear_fade: false,
            owner: "o".into(),
            space: None,
        };
        engine.load(
            PlaySchedule {
                clips: vec![clip],
                duration_s: 1.0,
                ..Default::default()
            },
            Arc::new(set),
        );
        let rx = engine.subscribe();
        let t0 = std::time::Instant::now();
        engine.play(0.0, Some(0.3));
        assert!(engine.is_playing());
        let mut events = 0;
        while t0.elapsed() < Duration::from_millis(600) {
            if rx.recv_timeout(Duration::from_millis(50)).is_ok() {
                events += 1;
            }
        }
        assert!(events >= 20, "about 60 Hz playhead events, got {events}");
        assert!(!engine.is_playing(), "stops at until_s");
        assert!(engine.position_s() >= 0.25, "{}", engine.position_s());
        engine.seek(0.5);
        assert!((engine.position_s() - 0.5).abs() < 0.01);
    }

    /// Playback runs on past the last clip while the room rings out (skips without a device).
    #[test]
    fn plays_on_into_the_rooms_tail() {
        if std::env::var("CI").is_ok() {
            eprintln!("skipped: no audio device on CI");
            return;
        }
        let Ok(engine) = Engine::start() else {
            eprintln!("skipped: no output device");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let sr = engine.sample_rate();
        let noise: Vec<f32> = (0..sr / 10)
            .map(|i| 0.02 * ((i * 7919 % 101) as f32 / 50.0 - 1.0))
            .collect();
        let p = dir.path().join("n.wav");
        apc_media::wav::write_wav(&p, sr, 1, &noise).unwrap();
        let mut set = SourceSet::new(sr);
        set.add("n", p);
        let clip = ScheduledClip {
            source_id: "n".into(),
            src_in: 0.0,
            src_out: 0.1,
            start_s: 0.0,
            gain_db: 0.0,
            fade_in_s: 0.005,
            fade_out_s: 0.005,
            lane: "sfx1".into(),
            loop_len_s: None,
            pan: 0.0,
            linear_fade: false,
            owner: "o".into(),
            space: Some(crate::schedule::ClipSpace {
                dry_gain: 1.0,
                send_gain: 1.0,
                lowpass_hz: 20_000.0,
                low_shelf_db: 0.0,
                high_shelf_db: 0.0,
            }),
        };
        engine.load(
            PlaySchedule {
                clips: vec![clip],
                duration_s: 0.1,
                room: Some(crate::room::RoomShape::new(0.2, 0.5, 0.5)),
                tail_s: 0.3,
            },
            Arc::new(set),
        );
        engine.play(0.0, None);
        std::thread::sleep(Duration::from_millis(250));
        assert!(engine.is_playing(), "still ringing after the clip's end");
        std::thread::sleep(Duration::from_millis(350));
        assert!(!engine.is_playing(), "stops when the tail is over");
        assert!(engine.position_s() >= 0.35, "{}", engine.position_s());
    }
}
