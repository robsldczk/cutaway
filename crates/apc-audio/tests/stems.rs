//! Stems add up to the mix: dry signals and room returns taken from one render path, so a stack
//! in Resolve or Logic sums to what the director heard.

use std::path::Path;

use apc_audio::offline::{frames_of, render_part};
use apc_audio::{MixSettings, Part, PlaySchedule, SourceSet, StemGroup};
use apc_core::config::ProjectConfig;
use apc_core::*;
use serde_json::json;

const RATE: u32 = 48000;

fn noise(dir: &Path, name: &str, seed: u32, len_s: f64) -> std::path::PathBuf {
    let mut x = seed;
    let s: Vec<f32> = (0..(f64::from(RATE) * len_s) as usize)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((x >> 8) as f32 / 8_388_608.0 - 1.0) * 0.3
        })
        .collect();
    let p = dir.join(name);
    apc_media::wav::write_wav(&p, RATE, 1, &s).unwrap();
    p
}

fn sound(id: &str, lane: &str, extra: serde_json::Value) -> Placement {
    let mut v = json!({
        "id": id, "scene_id": "s1", "asset_id": id, "lane": lane,
        "at_s": 0.3, "len_s": 0.8, "fade_in_s": 0.01, "fade_out_s": 0.01,
    });
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    serde_json::from_value(v).unwrap()
}

/// Something on every lane: a voice, an adlib, effects near and far, a bed, a music cue.
fn scene(dir: &Path) -> (Document, SourceSet) {
    let mut d = Document::default();
    let s: Scene = serde_json::from_value(json!({
        "id": "s1", "number": 1, "heading": "INT. HALL",
        "room": { "size": 0.7, "damping": 0.4, "wet": 0.8 },
    }))
    .unwrap();
    d.scenes.insert("s1".into(), s);
    let c: Character = serde_json::from_value(json!({
        "id": "PAUL", "label": "Paul", "color_index": 0, "wet": 0.6,
    }))
    .unwrap();
    d.characters.insert("PAUL".into(), c);
    let mut set = SourceSet::new(RATE);
    let sounds = [
        sound("voice", "dialogue", json!({ "character": "PAUL" })),
        sound(
            "adlib",
            "adlib",
            json!({ "character": "PAUL", "at_s": 0.5, "depth": 0.3 }),
        ),
        sound("door", "sfx", json!({ "pan": -0.6, "depth": 0.1 })),
        sound(
            "far",
            "sfx",
            json!({ "layer": 1, "at_s": 0.9, "depth": 0.9, "gain_db": -3.0 }),
        ),
        sound(
            "bed",
            "ambience",
            json!({ "at_s": 0.0, "len_s": 1.8, "depth": 0.4 }),
        ),
        sound("cue", "music", json!({ "at_s": 0.2, "pan": 0.3 })),
    ];
    for (i, p) in sounds.into_iter().enumerate() {
        let id = p.id.clone();
        let a: Asset = serde_json::from_value(json!({
            "id": id, "file": format!("{id}.wav"), "hash": id, "duration_s": 2.0, "source": "imported",
        }))
        .unwrap();
        d.assets.insert(id.clone(), a);
        set.add(
            format!("asset:{id}"),
            noise(dir, &format!("{id}.wav"), 77 + i as u32, 2.0),
        );
        d.placements.insert(id, p);
    }
    (d, set)
}

fn schedule(d: &Document) -> PlaySchedule {
    PlaySchedule::from_scene(
        d,
        &d.scenes["s1"],
        &ProjectConfig::from_doc(d).pacing(),
        &MixSettings {
            room_tone: false,
            ..Default::default()
        },
    )
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn voice(lane: &str) -> bool {
    StemGroup::of_lane(lane) == StemGroup::Dialogue
}

#[test]
fn dry_stems_and_room_returns_null_against_the_mix() {
    let dir = tempfile::tempdir().unwrap();
    let (doc, set) = scene(dir.path());
    let s = schedule(&doc);
    assert!(s.room.is_some() && s.tail_s > 0.0, "the room rings on");
    let frames = frames_of(&s, RATE);
    let mix = render_part(&s, &set, frames, Part::Whole).unwrap();
    assert!(peak(&mix) > 0.1, "the mix isn't silent");

    let group = |g: StemGroup| s.only(move |c| StemGroup::of_lane(&c.lane) == g);
    let stems = [
        (group(StemGroup::Dialogue), Part::Dry),
        (s.only(|c| voice(&c.lane)), Part::Wet),
        (group(StemGroup::Effects), Part::Dry),
        (s.only(|c| !voice(&c.lane)), Part::Wet),
        (group(StemGroup::Ambience), Part::Dry),
        (group(StemGroup::Music), Part::Dry),
    ];
    let mut sum = vec![0.0f32; frames * 2];
    for (stem, part) in &stems {
        let out = render_part(stem, &set, frames, *part).unwrap();
        assert_eq!(
            out.len(),
            mix.len(),
            "every stem is exactly as long as the mix"
        );
        assert!(peak(&out) > 0.0, "{part:?}: something on every stem");
        for (a, b) in sum.iter_mut().zip(&out) {
            *a += b;
        }
    }
    let residual = sum
        .iter()
        .zip(&mix)
        .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    let db = 20.0 * (residual / peak(&mix)).log10();
    assert!(db < -100.0, "the stems null against the mix: {db:.1} dB");
}

#[test]
fn dry_has_no_room_and_wet_has_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let (doc, set) = scene(dir.path());
    let s = schedule(&doc);
    let frames = frames_of(&s, RATE);
    // past the last clip only the room is left
    let last_clip = s.clips.iter().map(|c| c.end_s()).fold(0.0, f64::max);
    let after = (last_clip * f64::from(RATE)) as usize * 2 + 2;
    let dry = render_part(&s, &set, frames, Part::Dry).unwrap();
    let wet = render_part(&s, &set, frames, Part::Wet).unwrap();
    assert_eq!(peak(&dry[after..]), 0.0, "no tail on the dry side");
    assert!(peak(&wet[after..]) > 1e-4, "the tail is all room");
    // before the first clip's pre-delay is up there's no room at all
    assert_eq!(peak(&wet[..64]), 0.0, "a wet return of nothing yet");
}

#[test]
fn an_empty_stem_is_silence_of_the_full_length() {
    let dir = tempfile::tempdir().unwrap();
    let (doc, set) = scene(dir.path());
    let s = schedule(&doc);
    let frames = frames_of(&s, RATE);
    let nothing = s.only(|_| false);
    assert!(nothing.room.is_none(), "nothing reaches the room");
    for part in [Part::Dry, Part::Wet] {
        let out = render_part(&nothing, &set, frames, part).unwrap();
        assert_eq!(out.len(), frames * 2);
        assert_eq!(peak(&out), 0.0);
    }
}

#[test]
fn lanes_fall_into_their_groups() {
    use StemGroup::*;
    for (lane, g) in [
        ("PAUL", Dialogue),
        ("dialogue", Dialogue),
        ("adlib1", Dialogue),
        ("sfx1", Effects),
        ("sfx12", Effects),
        ("ambience", Ambience),
        ("roomtone", Ambience),
        ("music", Music),
        // a character can be called anything
        ("SFX1", Dialogue),
        ("sfxman", Dialogue),
    ] {
        assert_eq!(StemGroup::of_lane(lane), g, "{lane}");
    }
}
