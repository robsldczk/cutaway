//! The room bus end to end: a document with placed sounds, through the schedule, into the
//! same render the bounce and the engine use.

use std::path::Path;

use apc_audio::offline::render_to_buffer;
use apc_audio::{MixSettings, PlaySchedule, Renderer, SourceSet};
use apc_core::config::ProjectConfig;
use apc_core::*;

const RATE: u32 = 48000;

/// White noise, 0.5 s: full of highs, so darkening shows.
fn noise_set(dir: &Path) -> SourceSet {
    let mut x = 0x1234_5678u32;
    let s: Vec<f32> = (0..RATE / 2)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((x >> 8) as f32 / 8_388_608.0 - 1.0) * 0.5
        })
        .collect();
    let p = dir.join("noise.wav");
    apc_media::wav::write_wav(&p, RATE, 1, &s).unwrap();
    let mut set = SourceSet::new(RATE);
    set.add("asset:noise", p);
    set
}

fn scene(id: &str, number: u32, room: SceneRoom) -> Scene {
    Scene {
        id: id.into(),
        number,
        heading: format!("INT. ROOM {number}"),
        int_ext: "INT".into(),
        location: format!("ROOM {number}"),
        place: "Room".into(),
        time_of_day: String::new(),
        order: vec![],
        tightness: 1.0,
        story: Default::default(),
        room,
        tone: None,
        lock: None,
    }
}

fn sound(id: &str, scene_id: &str, at_s: f64, depth: f64) -> Placement {
    Placement {
        id: id.into(),
        scene_id: scene_id.into(),
        asset_id: "noise".into(),
        cue_id: None,
        lane: Lane::Sfx,
        character: None,
        layer: 0,
        anchor: None,
        at_s,
        offset_s: 0.0,
        in_s: 0.0,
        len_s: 0.5,
        fade_in_s: 0.005,
        fade_out_s: 0.005,
        fade_curve: FadeCurve::EqualPower,
        gain_db: 0.0,
        muted: false,
        pan: 0.0,
        depth,
        auto_fade: false,
        added: 0,
        looped: false,
    }
}

fn doc_with(sounds: Vec<Placement>, room: SceneRoom) -> Document {
    let mut d = Document::default();
    d.scenes.insert("s1".into(), scene("s1", 1, room));
    d.scenes
        .insert("s2".into(), scene("s2", 2, SceneRoom::default()));
    for p in sounds {
        d.placements.insert(p.id.clone(), p);
    }
    d
}

fn schedule(d: &Document, scene: &str) -> PlaySchedule {
    let settings = MixSettings {
        room_tone: false,
        ..Default::default()
    };
    PlaySchedule::from_scene(
        d,
        &d.scenes[scene],
        &ProjectConfig::from_doc(d).pacing(),
        &settings,
    )
}

fn left(out: &[f32]) -> Vec<f32> {
    out.iter().step_by(2).copied().collect()
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| f64::from(*v).powi(2)).sum()
}

/// Energy of the first difference against the energy: how much of the signal is highs.
fn brightness(x: &[f32]) -> f64 {
    let d: f64 = x.windows(2).map(|w| f64::from(w[1] - w[0]).powi(2)).sum();
    d / energy(x).max(1e-30)
}

fn db(ratio: f64) -> f64 {
    10.0 * ratio.max(1e-30).log10()
}

fn big_room() -> SceneRoom {
    SceneRoom {
        size: 0.6,
        damping: 0.4,
        wet: 0.8,
    }
}

fn dry_room() -> SceneRoom {
    SceneRoom {
        wet: 0.0,
        ..big_room()
    }
}

/// Plays from `from` in blocks of `block`, as the engine's feeder does.
fn play(s: &PlaySchedule, set: &SourceSet, from: u64, block: usize, frames: usize) -> Vec<f32> {
    let mut r = Renderer::new(s, RATE);
    let mut out = vec![0.0f32; frames * 2];
    let mut f = 0;
    while f < frames {
        let n = block.min(frames - f);
        r.render(s, set, from + f as u64, &mut out[2 * f..2 * (f + n)]);
        f += n;
    }
    out
}

#[test]
fn a_far_sound_is_wetter_and_darker_than_a_close_one() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let render = |depth| {
        let d = doc_with(vec![sound("p", "s1", 0.0, depth)], big_room());
        left(&render_to_buffer(&schedule(&d, "s1"), &set).unwrap())
    };
    let (close, far) = (render(0.0), render(1.0));
    let body = RATE as usize / 2;
    // after the sound has stopped, only the room is left
    let wet_ratio = |x: &[f32]| db(energy(&x[body..]) / energy(&x[..body]));
    println!(
        "room against body: close {:.1} dB, far {:.1} dB; body level far {:.1} dB; brightness close {:.3}, far {:.3}",
        wet_ratio(&close),
        wet_ratio(&far),
        db(energy(&far[..body]) / energy(&close[..body])),
        brightness(&close[..body]),
        brightness(&far[..body]),
    );
    assert!(
        wet_ratio(&far) - wet_ratio(&close) > 15.0,
        "far is much wetter: {:.1} against {:.1} dB",
        wet_ratio(&far),
        wet_ratio(&close)
    );
    assert!(
        brightness(&far[..body]) < brightness(&close[..body]) * 0.5,
        "far is darker"
    );
    assert!(
        energy(&far[..body]) < energy(&close[..body]) * 0.25,
        "far is further back"
    );
}

#[test]
fn a_close_sound_keeps_its_direct_signal_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let mut d = doc_with(vec![sound("p", "s1", 0.0, 0.0)], big_room());
    let in_room = left(&render_to_buffer(&schedule(&d, "s1"), &set).unwrap());
    d.scenes.get_mut("s1").unwrap().room.wet = 0.0;
    let dry = left(&render_to_buffer(&schedule(&d, "s1"), &set).unwrap());
    // before the pre-delay is over nothing of the room is heard yet
    let first = 200;
    assert_eq!(in_room[..first], dry[..first]);
    assert_ne!(in_room[..dry.len()], dry[..]);
}

#[test]
fn the_same_input_gives_the_same_bounce() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(
        vec![sound("p", "s1", 0.1, 0.7), sound("q", "s1", 0.4, 0.2)],
        big_room(),
    );
    let s = schedule(&d, "s1");
    let first = render_to_buffer(&s, &set).unwrap();
    assert_eq!(first, render_to_buffer(&s, &set).unwrap());
    let fresh = noise_set(dir.path());
    assert_eq!(
        first,
        render_to_buffer(&s, &fresh).unwrap(),
        "a fresh source set too"
    );
}

#[test]
fn playback_from_the_start_is_the_bounce_bit_for_bit() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(
        vec![sound("p", "s1", 0.1, 0.7), sound("q", "s1", 0.4, 0.2)],
        big_room(),
    );
    let s = schedule(&d, "s1");
    let bounce = render_to_buffer(&s, &set).unwrap();
    let frames = bounce.len() / 2;
    for block in [1024, 777, 64] {
        let mut played = play(&s, &set, 0, block, frames);
        for v in played.iter_mut() {
            *v = v.clamp(-1.0, 1.0);
        }
        assert_eq!(played, bounce, "block {block}");
    }
}

#[test]
fn playback_started_in_the_tail_is_what_the_bounce_has_there() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(vec![sound("p", "s1", 0.0, 0.8)], big_room());
    let s = schedule(&d, "s1");
    let bounce = render_to_buffer(&s, &set).unwrap();
    // inside the warm-up window the room hears everything, so it is exact
    let from = (f64::from(RATE) * 0.7) as u64;
    let frames = bounce.len() / 2 - from as usize;
    let played = play(&s, &set, from, 1024, frames);
    let tail = &bounce[from as usize * 2..];
    assert!(energy(tail) > 1e-3, "there is a tail to hear");
    assert_eq!(&played[..], tail);
}

#[test]
fn playback_started_long_after_a_sound_misses_nothing_you_could_hear() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    // the first sound lies just outside the warm-up window of the start point
    let d = doc_with(
        vec![sound("p", "s1", 0.0, 0.8), sound("q", "s1", 8.0, 0.8)],
        big_room(),
    );
    let s = schedule(&d, "s1");
    let warm = s.room.unwrap().warmup_s();
    let bounce = render_to_buffer(&s, &set).unwrap();
    let from = ((0.5 + warm + 0.01) * f64::from(RATE)) as u64;
    assert!(from < u64::from(RATE) * 8, "starts before the second sound");
    let frames = bounce.len() / 2 - from as usize;
    let played = play(&s, &set, from, 1024, frames);
    let want = &bounce[from as usize * 2..];
    let diff: Vec<f32> = played.iter().zip(want).map(|(a, b)| a - b).collect();
    let rel = db(energy(&diff) / energy(want));
    assert!(rel < -80.0, "{rel:.1} dB");
}

#[test]
fn a_jump_back_sounds_like_starting_there() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(vec![sound("p", "s1", 0.0, 0.8)], big_room());
    let s = schedule(&d, "s1");
    let mut r = Renderer::new(&s, RATE);
    let mut skip = vec![0.0f32; 4096];
    r.render(&s, &set, 30_000, &mut skip);
    r.render(&s, &set, 32_048, &mut skip);
    let mut jumped = vec![0.0f32; 4096];
    r.render(&s, &set, 10_000, &mut jumped);
    assert_eq!(jumped, play(&s, &set, 10_000, 2048, 2048));
}

#[test]
fn changing_one_scenes_room_leaves_the_other_alone() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let sounds = vec![sound("a", "s1", 0.0, 0.6), sound("b", "s2", 0.0, 0.6)];
    let mut d = doc_with(sounds, SceneRoom::default());
    let one = render_to_buffer(&schedule(&d, "s1"), &set).unwrap();
    let two = render_to_buffer(&schedule(&d, "s2"), &set).unwrap();

    d.scenes.get_mut("s1").unwrap().room = big_room();
    let one_after = render_to_buffer(&schedule(&d, "s1"), &set).unwrap();
    let two_after = render_to_buffer(&schedule(&d, "s2"), &set).unwrap();

    assert_ne!(one, one_after, "scene 1 changed");
    assert_eq!(two, two_after, "scene 2 did not");
}

#[test]
fn a_rooms_settings_are_heard() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let tail = |room: SceneRoom| {
        let d = doc_with(vec![sound("p", "s1", 0.0, 0.8)], room);
        let out = left(&render_to_buffer(&schedule(&d, "s1"), &set).unwrap());
        out[RATE as usize / 2..].to_vec()
    };
    let base = SceneRoom {
        size: 0.5,
        damping: 0.5,
        wet: 0.5,
    };
    let louder = SceneRoom { wet: 1.0, ..base };
    assert!(energy(&tail(louder)) > energy(&tail(base)) * 3.0, "wet");
    let larger = SceneRoom { size: 0.9, ..base };
    assert!(tail(larger).len() > tail(base).len(), "size");
    let darker = SceneRoom {
        damping: 1.0,
        ..base
    };
    assert!(
        brightness(&tail(darker)) < brightness(&tail(base)),
        "damping"
    );
}

#[test]
fn every_sound_of_a_scene_shares_one_room() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let solo = |id: &str, at| {
        let d = doc_with(vec![sound(id, "s1", at, 0.5)], big_room());
        schedule(&d, "s1")
    };
    let both = doc_with(
        vec![sound("p", "s1", 0.0, 0.5), sound("q", "s1", 0.25, 0.5)],
        big_room(),
    );
    // one linear room: the sum of the sounds through it is the sum of each through it
    let mix = render_to_buffer(&schedule(&both, "s1"), &set).unwrap();
    let a = render_to_buffer(&solo("p", 0.0), &set).unwrap();
    let b = render_to_buffer(&solo("q", 0.25), &set).unwrap();
    let n = mix.len().min(a.len()).min(b.len());
    let diff: f64 = (0..n)
        .map(|i| f64::from(mix[i] - a[i] - b[i]).powi(2))
        .sum();
    assert!(db(diff / energy(&mix[..n])) < -60.0);
}

#[test]
fn a_bounce_lets_the_room_ring_out() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(vec![sound("p", "s1", 0.0, 0.5)], big_room());
    let s = schedule(&d, "s1");
    assert_eq!(s.duration_s, 0.5);
    assert!((s.tail_s - s.room.unwrap().tail_s()).abs() < 1e-9);
    let out = render_to_buffer(&s, &set).unwrap();
    assert_eq!(
        out.len() / 2,
        (s.end_s() * f64::from(RATE)).round() as usize
    );
}

#[test]
fn a_dry_room_adds_nothing_to_the_length() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(vec![sound("p", "s1", 0.0, 0.5)], dry_room());
    let s = schedule(&d, "s1");
    assert_eq!((s.room, s.tail_s), (None, 0.0));
    assert_eq!(render_to_buffer(&s, &set).unwrap().len(), RATE as usize);
}

#[test]
fn a_scene_without_sounds_has_no_room_at_all() {
    let d = doc_with(vec![], big_room());
    let s = schedule(&d, "s1");
    assert_eq!((s.room, s.tail_s, s.duration_s), (None, 0.0, 0.0));
}

#[test]
fn music_and_vocals_stay_out_of_the_room() {
    let mut music = sound("m", "s1", 0.0, 1.0);
    music.lane = Lane::Music;
    let mut vocal = sound("v", "s1", 0.0, 1.0);
    vocal.lane = Lane::Dialogue;
    vocal.character = Some("PAUL".into());
    let d = doc_with(vec![music, vocal], big_room());
    let s = schedule(&d, "s1");
    assert_eq!(s.room, None);
    assert!(s.clips.iter().all(|c| c.space.is_none()));
}

#[test]
fn ambience_is_in_the_room() {
    let mut bed = sound("a", "s1", 0.0, 0.3);
    bed.lane = Lane::Ambience;
    let d = doc_with(vec![bed], big_room());
    let s = schedule(&d, "s1");
    assert!(s.room.is_some());
    assert!(s.clips[0].has_send());
}

#[test]
fn a_muted_sound_or_lane_sends_nothing() {
    let mut p = sound("p", "s1", 0.0, 0.8);
    p.muted = true;
    let d = doc_with(vec![p], big_room());
    assert_eq!(schedule(&d, "s1").room, None);

    let mut d = doc_with(vec![sound("p", "s1", 0.0, 0.8)], big_room());
    d.lane_mix.insert(
        "sfx1".into(),
        LaneMix {
            id: "sfx1".into(),
            volume_db: 0.0,
            pan: 0.0,
            mute: true,
            solo: false,
        },
    );
    let s = schedule(&d, "s1");
    assert_eq!((s.room, s.tail_s), (None, 0.0));
}

#[test]
fn pan_is_heard_and_the_lane_adds_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let mut p = sound("p", "s1", 0.0, 0.0);
    p.pan = -1.0;
    let mut d = doc_with(vec![p], dry_room());
    let out = render_to_buffer(&schedule(&d, "s1"), &set).unwrap();
    let right: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
    assert!(energy(&left(&out)) > 1.0 && energy(&right) < 1e-9);

    d.lane_mix.insert(
        "sfx1".into(),
        LaneMix {
            id: "sfx1".into(),
            volume_db: 0.0,
            pan: 0.5,
            mute: false,
            solo: false,
        },
    );
    assert_eq!(schedule(&d, "s1").clips[0].pan, -0.5);
}

#[test]
fn two_depths_of_one_sound_do_not_share_a_cache_entry() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(
        vec![sound("p", "s1", 0.0, 0.0), sound("q", "s1", 2.0, 1.0)],
        big_room(),
    );
    let s = schedule(&d, "s1");
    assert_ne!(
        set.clip(&s.clips[0]).unwrap(),
        set.clip(&s.clips[1]).unwrap()
    );
}

#[test]
fn a_far_sound_sounds_the_same_from_any_point_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let set = noise_set(dir.path());
    let d = doc_with(vec![sound("p", "s1", 0.0, 1.0)], dry_room());
    let s = schedule(&d, "s1");
    let whole = render_to_buffer(&s, &set).unwrap();
    let from = 12_345usize;
    let part = play(&s, &set, from as u64, 1000, 2000);
    assert_eq!(&part[..], &whole[from * 2..(from + 2000) * 2]);
}
