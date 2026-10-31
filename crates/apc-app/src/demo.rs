//! A synthetic recording session for CI, UI tests and trying the app without real material:
//! a two-scene script, Paul and Sara tracks and a director room track (BWF with timecode,
//! the room running 0.25 s late, bleed on every mic) plus transcript sidecars.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::error::Result;
use crate::service::Service;

pub const SCRIPT: &str = "Title: Demo Episode\nAuthor: Cutaway\n\n===\n\nINT. LAB - DAY\n\nA kettle whistles. PAUL types on a keyboard.\n\nPAUL\n(to himself)\nSo the machine finally works.\n\nSARA\nDoes it make coffee?\n\nPAUL\nLunch.\n\nThe door opens.\n\nINT. CANTEEN - LATER\n\nSARA\nI brought sandwiches for everyone.\n\nPAUL\nWe can send things through time now.\n";

/// (time, track, text): what was said when. Tracks: P = Paul, S = Sara, R = director.
pub const EVENTS: &[(f64, char, &str)] = &[
    (1.0, 'R', "Okay, let's start with the lab scene."),
    (4.0, 'P', "So the machine finally works."),
    (7.0, 'R', "Good, maybe a little quieter."),
    (10.0, 'P', "So the machine finally works."),
    (13.0, 'R', "That was the one I liked."),
    (16.0, 'R', "Does it make coffee?"),
    (18.5, 'P', "Lunch."),
    (20.0, 'R', "One more, annoyed."),
    (23.0, 'P', "Lunch."),
    (25.0, 'R', "I like that."),
    (28.0, 'P', "Can I get some water?"),
    (32.0, 'P', "We can send things through time now."),
    (36.0, 'R', "Perfect, definitely."),
    (44.0, 'S', "Does it make coffee?"),
    (47.0, 'R', "Again, more curious."),
    (50.0, 'S', "Does it make coffee?"),
    (54.0, 'S', "I brought sandwiches for everyone."),
    (58.0, 'R', "Nice."),
];

pub const LENGTH_S: f64 = 62.0;
pub const ROOM_DELAY_S: f64 = 0.25;
const SR: u32 = 48000;
const TC_START: f64 = 36000.0;

fn word_spans(t: f64, text: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut x = t;
    for w in text.split_whitespace() {
        let d = 0.07 * w.chars().filter(|c| c.is_alphanumeric()).count() as f64 + 0.08;
        out.push((x, x + d));
        x += d + 0.05;
    }
    out
}

fn voice(track: char) -> f64 {
    match track {
        'P' => 120.0,
        'S' => 210.0,
        _ => 160.0,
    }
}

/// Speech-like bursts: a few harmonics with a Hann envelope per word, plus breath noise.
fn render(events: &[(f64, char, &str)], gain_for: impl Fn(char) -> f32, delay: f64) -> Vec<f32> {
    let n = (LENGTH_S * SR as f64) as usize;
    let mut s = vec![0.0f32; n];
    let mut seed = 12345u32;
    for v in s.iter_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *v = ((seed >> 9) as f32 / (1u32 << 23) as f32 - 1.0) * 0.0015;
    }
    for &(t, track, text) in events {
        let g = gain_for(track);
        if g <= 0.0 {
            continue;
        }
        let f0 = voice(track);
        for (k, (a, b)) in word_spans(t + delay, text).into_iter().enumerate() {
            let (i0, i1) = ((a * SR as f64) as usize, ((b * SR as f64) as usize).min(n));
            let f = f0 * (1.0 + 0.04 * (k % 3) as f64);
            for (i, out) in s.iter_mut().enumerate().take(i1).skip(i0) {
                let x = (i - i0) as f64 / (i1 - i0).max(1) as f64;
                let env = (std::f64::consts::PI * x).sin().powi(2);
                let ph = std::f64::consts::TAU * f * i as f64 / SR as f64;
                let v = (ph.sin() + 0.5 * (2.0 * ph).sin() + 0.25 * (3.0 * ph).sin()) * env * 0.35;
                *out += (v as f32) * g;
            }
        }
    }
    s
}

fn transcript(events: &[(f64, char, &str)], delay: f64) -> serde_json::Value {
    let tc = |t: f64| {
        let ms = (t * 1000.0).round() as i64;
        format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3_600_000,
            (ms / 60_000) % 60,
            (ms / 1000) % 60,
            ms % 1000
        )
    };
    let lines: Vec<serde_json::Value> = events
        .iter()
        .map(|&(t, track, text)| {
            let spans = word_spans(t + delay, text);
            json!({
                "startTime": tc(spans[0].0),
                "endTime": tc(spans[spans.len() - 1].1),
                "speakerDesignation": format!("Speaker {}", match track { 'P' => 1, 'S' => 2, _ => 3 }),
                "text": text,
            })
        })
        .collect();
    json!({ "lines": lines })
}

/// Writes the session files into `dir` (Paul.wav, Sara.wav, Room.wav + transcripts, script).
pub fn write_session(dir: &Path) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("Demo Episode.fountain"), SCRIPT)?;
    let mut out = Vec::new();
    for (name, own) in [("Paul", 'P'), ("Sara", 'S'), ("Room", 'R')] {
        let room = own == 'R';
        let gain = |t: char| -> f32 {
            if t == own {
                1.0
            } else if room {
                0.25
            } else if t == 'R' {
                0.18
            } else {
                0.03
            }
        };
        let delay = if room { ROOM_DELAY_S } else { 0.0 };
        let audio = render(EVENTS, gain, delay);
        let wav = dir.join(format!("{name}.wav"));
        apc_media::bwf::write_test_bwf(&wav, SR, &audio, TC_START, "DEMO")?;
        // the mic hears its own speech and loud neighbours; quiet bleed is not transcribed
        let heard: Vec<(f64, char, &str)> = EVENTS
            .iter()
            .copied()
            .filter(|e| gain(e.1) >= 0.18)
            .collect();
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_string_pretty(&transcript(&heard, delay))?,
        )?;
        out.push(wav);
    }
    Ok(out)
}

/// Creates a demo episode under `parent` with script and audio imported.
pub fn create(parent: &Path, client: &str, analyze: bool) -> Result<Service> {
    let session = parent.join("Demo session");
    let wavs = write_session(&session)?;
    let mut svc = Service::create(parent, "Demo", None, client)?;
    svc.import_script(&session.join("Demo Episode.fountain"), Some("Demo Episode"))?;
    svc.import_audio(&wavs, true)?;
    if analyze {
        svc.analyze(&|_| {})?;
    }
    Ok(svc)
}
