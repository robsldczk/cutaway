//! The play schedule: every clip with its source range, timeline position, gain and fades.
//! Built from the shared `SceneLayout`, so playback and export hear the same cut.

use serde::{Deserialize, Serialize};

use apc_core::timeline::{layout_scene, placement_start, Pacing, SceneLayout};
use apc_core::{Document, Lane, Scene};

use crate::depth::{depth_mix, line_mix, LOWPASS_OPEN_HZ};
use crate::room::RoomShape;

/// Where a clip sits in the scene's room (from its depth).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ClipSpace {
    /// Level of the direct sound (linear).
    pub dry_gain: f64,
    /// Level sent to the room (linear).
    pub send_gain: f64,
    /// Roll-off of the clip (dry and send alike).
    pub lowpass_hz: f64,
    /// Shelves of a line moved back (D128); 0 is no filter.
    #[serde(default)]
    pub low_shelf_db: f64,
    #[serde(default)]
    pub high_shelf_db: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScheduledClip {
    /// Source id (`src_…`) or library asset (`asset:<id>`).
    pub source_id: String,
    pub src_in: f64,
    pub src_out: f64,
    /// Timeline position of `src_in`.
    pub start_s: f64,
    pub gain_db: f64,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    /// Lane name: the character for dialogue, `sfx`, `ambience`, `music`, `roomtone`.
    pub lane: String,
    /// Loops `[src_in, src_out)` until `start_s + loop_len_s` (room tone bed).
    pub loop_len_s: Option<f64>,
    /// −1 left … +1 right (constant-power pan).
    #[serde(default)]
    pub pan: f64,
    /// Linear instead of equal-power fades.
    #[serde(default)]
    pub linear_fade: bool,
    /// Line or placement this clip belongs to.
    pub owner: String,
    /// In the scene's room; `None` plays dry (music, room tone, a take auditioned alone).
    #[serde(default)]
    pub space: Option<ClipSpace>,
}

impl ScheduledClip {
    /// The clip feeds the room bus.
    pub fn has_send(&self) -> bool {
        self.space.map(|m| m.send_gain > 0.0).unwrap_or(false)
    }

    pub fn len(&self) -> f64 {
        self.loop_len_s
            .unwrap_or(self.src_out - self.src_in)
            .max(0.0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() <= 0.0
    }
    pub fn end_s(&self) -> f64 {
        self.start_s + self.len()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MixSettings {
    /// Level each recording to `target_lufs` by its typical speech loudness (playback only:
    /// bounces and exports keep this off, so nothing is burned in).
    pub level: bool,
    pub target_lufs: f64,
    pub room_tone: bool,
    pub include_sound: bool,
}

impl Default for MixSettings {
    fn default() -> Self {
        MixSettings {
            level: false,
            target_lufs: -23.0,
            room_tone: true,
            include_sound: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PlaySchedule {
    pub clips: Vec<ScheduledClip>,
    pub duration_s: f64,
    /// The scene's room, when something is sent to it and it is heard.
    #[serde(default)]
    pub room: Option<RoomShape>,
    /// How long the room rings on after `duration_s`.
    #[serde(default)]
    pub tail_s: f64,
}

/// The group of tracks a clip's direct sound belongs to in an export.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StemGroup {
    Dialogue,
    Effects,
    Ambience,
    Music,
}

impl StemGroup {
    /// By lane name (see [`ScheduledClip::lane`]): sound lanes are lower case, what's left is a
    /// voice. Room tone sits under a scene like an ambience (D110).
    pub fn of_lane(lane: &str) -> StemGroup {
        let numbered = |p: &str| {
            lane.strip_prefix(p)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        match lane {
            "ambience" | "roomtone" => StemGroup::Ambience,
            "music" => StemGroup::Music,
            _ if numbered("sfx") => StemGroup::Effects,
            _ => StemGroup::Dialogue,
        }
    }
}

/// Lane key of a clip as the Edit page names lanes (`PAUL`, `sfx1`, `ambience`, …).
pub fn lane_key(c: &ScheduledClip) -> &str {
    &c.lane
}

/// Applies lane volume and pan, drops muted lanes, and keeps only soloed ones when any is soloed.
pub fn apply_lane_mix(doc: &Document, clips: &mut Vec<ScheduledClip>) {
    let any_solo = doc.lane_mix.values().any(|m| m.solo);
    clips.retain(|c| match doc.lane_mix.get(lane_key(c)) {
        Some(m) if m.mute => false,
        Some(m) if any_solo => m.solo,
        _ => !any_solo,
    });
    for c in clips.iter_mut() {
        if let Some(m) = doc.lane_mix.get(lane_key(c)) {
            c.gain_db += m.volume_db;
            // the lane's pan moves the clips in it; a sound keeps its own place on top
            c.pan = (c.pan + m.pan).clamp(-1.0, 1.0);
        }
    }
}

/// A voice in its scene's room: the character's send, and the further it's panned, a little
/// further back. In the centre the direct sound is untouched. A voice at wet 0 stays out of the
/// room however it's panned. `pan` is what's set in the scene; the lane's own pan counts too.
fn voice_space(
    doc: &Document,
    character: Option<&str>,
    pan: f64,
    pacing: &Pacing,
) -> Option<ClipSpace> {
    let id = character?;
    let lane = doc.lane_mix.get(id).map_or(0.0, |m| m.pan);
    let m = crate::depth::voice_mix(
        doc.characters.get(id)?.wet,
        (pan + lane).clamp(-1.0, 1.0),
        pacing.pan_room,
    );
    (m.send_gain > 0.0).then_some(ClipSpace {
        dry_gain: m.dry_gain,
        send_gain: m.send_gain,
        lowpass_hz: m.lowpass_hz,
        low_shelf_db: 0.0,
        high_shelf_db: 0.0,
    })
}

/// Adds a line's clip where it stands in the room. A line moved back is two clips, the direct
/// sound and what reaches the room, because they differ: the direct sound arrives a little late
/// and has its own filter. Each stays one plain clip to the mixer, so a stretch still renders
/// from anywhere with no state to carry.
fn push_voice(clips: &mut Vec<ScheduledClip>, clip: ScheduledClip, depth: f64, predelay_s: f64) {
    let base = clip.space.unwrap_or(ClipSpace {
        dry_gain: 1.0,
        send_gain: 0.0,
        lowpass_hz: LOWPASS_OPEN_HZ,
        low_shelf_db: 0.0,
        high_shelf_db: 0.0,
    });
    let Some(m) = line_mix(depth, base.send_gain) else {
        clips.push(clip);
        return;
    };
    let room = ClipSpace {
        dry_gain: 0.0,
        send_gain: m.send_gain,
        low_shelf_db: m.low_shelf_db,
        ..base
    };
    let direct = ClipSpace {
        dry_gain: base.dry_gain * m.dry_gain,
        send_gain: 0.0,
        low_shelf_db: m.low_shelf_db,
        high_shelf_db: m.high_shelf_db,
        ..base
    };
    clips.push(ScheduledClip {
        space: Some(room),
        ..clip.clone()
    });
    clips.push(ScheduledClip {
        start_s: clip.start_s + m.lag * predelay_s,
        space: Some(direct),
        ..clip
    });
}

impl PlaySchedule {
    pub fn from_scene(
        doc: &Document,
        scene: &Scene,
        pacing: &Pacing,
        settings: &MixSettings,
    ) -> Self {
        let layout = layout_scene(doc, scene, pacing);
        Self::from_layout(doc, scene, &layout, pacing, settings)
    }

    pub fn from_layout(
        doc: &Document,
        scene: &Scene,
        layout: &SceneLayout,
        pacing: &Pacing,
        settings: &MixSettings,
    ) -> Self {
        let levels = if settings.level {
            crate::level::source_lufs(doc)
        } else {
            Default::default()
        };
        // a source without measured takes (sound assets, a bare room mic) stays as it is
        let level = |source_id: &str| {
            levels
                .get(source_id)
                .map(|l| apc_media::loudness::match_gain_db(Some(*l), settings.target_lufs))
                .unwrap_or(0.0)
        };
        let shape = RoomShape::new(scene.room.size, scene.room.damping, scene.room.wet);
        let mut clips = Vec::new();
        for item in &layout.items {
            let voice = item.character.as_deref().map_or(0.0, |c| scene.pan_of(c));
            let depth = doc.boxes.get(&item.item_id).map_or(0.0, |b| b.depth);
            // muted parts keep their time but are not heard; handles are
            for p in item.audio.iter().filter(|p| !p.muted) {
                let clip = ScheduledClip {
                    source_id: p.source_id.clone(),
                    src_in: p.src_in,
                    src_out: p.src_out,
                    start_s: p.start_s,
                    gain_db: p.gain_db + level(&p.source_id),
                    fade_in_s: p.fade_in_s,
                    fade_out_s: p.fade_out_s,
                    lane: item.character.clone().unwrap_or_else(|| "dialogue".into()),
                    loop_len_s: None,
                    pan: voice,
                    linear_fade: false,
                    owner: item.item_id.clone(),
                    space: voice_space(doc, item.character.as_deref(), voice, pacing),
                };
                push_voice(&mut clips, clip, depth, shape.predelay_s);
            }
        }
        // a tail can ring past the last line
        let mut duration = clips
            .iter()
            .map(|c| c.start_s + (c.src_out - c.src_in))
            .fold(layout.duration_s, f64::max);
        if settings.include_sound {
            for p in doc
                .placements
                .values()
                .filter(|p| p.scene_id == scene.id && !p.muted)
            {
                let start = placement_start(layout, p);
                // an actor's breath stands where the actor does
                let pan = (p.pan
                    + p.character
                        .as_deref()
                        .filter(|_| p.lane == Lane::Dialogue)
                        .map_or(0.0, |c| scene.pan_of(c)))
                .clamp(-1.0, 1.0);
                let space = if p.in_room() {
                    let m = depth_mix(p.depth);
                    Some(ClipSpace {
                        dry_gain: m.dry_gain,
                        send_gain: m.send_gain,
                        lowpass_hz: m.lowpass_hz,
                        low_shelf_db: 0.0,
                        high_shelf_db: 0.0,
                    })
                } else if p.lane == Lane::Dialogue {
                    // an actor's breath sits in the room with their lines
                    voice_space(doc, p.character.as_deref(), pan, pacing)
                } else {
                    None
                };
                // a loop repeats the file from its in point, for as long as it's set to
                let file_end = doc
                    .assets
                    .get(&p.asset_id)
                    .map(|a| a.duration_s)
                    .filter(|d| p.looped && *d > p.in_s);
                clips.push(ScheduledClip {
                    source_id: format!("asset:{}", p.asset_id),
                    src_in: p.in_s,
                    src_out: file_end.unwrap_or(p.in_s + p.len_s),
                    start_s: start,
                    gain_db: p.gain_db,
                    fade_in_s: p.fade_in_s.max(0.003),
                    fade_out_s: p.fade_out_s.max(0.003),
                    lane: match p.lane {
                        Lane::Sfx => format!("sfx{}", p.layer + 1),
                        Lane::Ambience => "ambience".into(),
                        Lane::Music => "music".into(),
                        Lane::Dialogue => p.character.clone().unwrap_or_else(|| "dialogue".into()),
                    },
                    loop_len_s: file_end.map(|_| p.len_s),
                    pan,
                    linear_fade: false,
                    owner: p.id.clone(),
                    space,
                });
                if let Some(last) = clips.last_mut() {
                    last.linear_fade = p.fade_curve == apc_core::FadeCurve::Linear;
                }
                duration = duration.max(start + p.len_s);
            }
        }
        if settings.room_tone {
            let loc = apc_core::script::location_id(&scene.location);
            if let Some(tone) = doc.room_tone_for(scene) {
                if tone.enabled && tone.t1 > tone.t0 + 0.2 {
                    let source_id = match (&tone.source_id, &tone.asset_id) {
                        (Some(s), _) => Some(s.clone()),
                        (None, Some(a)) => Some(format!("asset:{a}")),
                        _ => None,
                    };
                    if let Some(source_id) = source_id {
                        let gain_db = tone.gain_db + level(&source_id);
                        clips.push(ScheduledClip {
                            source_id,
                            src_in: tone.t0,
                            src_out: tone.t1,
                            start_s: 0.0,
                            gain_db,
                            fade_in_s: 0.2,
                            fade_out_s: 0.2,
                            lane: "roomtone".into(),
                            loop_len_s: Some(duration),
                            pan: 0.0,
                            linear_fade: false,
                            owner: loc,
                            space: None,
                        });
                    }
                }
            }
        }
        apply_lane_mix(doc, &mut clips);
        clips.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
        // after the lane mix: a muted lane sends nothing and leaves no tail
        let last_send = clips
            .iter()
            .filter(|c| c.has_send())
            .map(ScheduledClip::end_s)
            .fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b))));
        let (room, tail_s) = match last_send {
            Some(end) if shape.wet > 0.0 => {
                (Some(shape), (end + shape.tail_s() - duration).max(0.0))
            }
            _ => (None, 0.0),
        };
        PlaySchedule {
            clips,
            duration_s: duration,
            room,
            tail_s,
        }
    }

    /// When the last sound has rung out: the end of the bounce and of playback.
    pub fn end_s(&self) -> f64 {
        self.duration_s + self.tail_s
    }

    /// The same schedule with only some of its clips: same room, same length, so what it
    /// renders lines up with the whole. The room stays only if something still reaches it.
    pub fn only(&self, keep: impl Fn(&ScheduledClip) -> bool) -> PlaySchedule {
        let clips: Vec<ScheduledClip> = self.clips.iter().filter(|c| keep(c)).cloned().collect();
        let room = self
            .room
            .filter(|_| clips.iter().any(ScheduledClip::has_send));
        PlaySchedule {
            clips,
            room,
            duration_s: self.duration_s,
            tail_s: self.tail_s,
        }
    }

    /// Clips that sound between `t0` and `t1`.
    pub fn active(&self, t0: f64, t1: f64) -> impl Iterator<Item = &ScheduledClip> {
        self.clips
            .iter()
            .filter(move |c| c.start_s < t1 && c.end_s() > t0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apc_core::timeline::{layout_scene, Pacing};

    fn sample_doc() -> (Document, Scene) {
        let (doc, scene) = apc_core_test_doc();
        (doc, scene)
    }

    // A two-line scene: "a" plays 10.0..11.0, "b" plays 20.0..22.0, 0.8 s pause between.
    fn apc_core_test_doc() -> (Document, Scene) {
        use apc_core::*;
        let mut d = Document::default();
        let line = |id: &str, n: u32| ScriptLine {
            id: id.into(),
            scene_id: "sc".into(),
            kind: LineKind::Dialogue,
            character: Some("PAUL".into()),
            extension: None,
            text: "x".into(),
            paren: None,
            tokens: vec!["x".into()],
            words: vec!["x".into()],
            number: n,
            version_added: 1,
            changed: false,
            forced: false,
            adlib: false,
            removed: false,
        };
        d.lines.insert("a".into(), line("a", 1));
        d.lines.insert("b".into(), line("b", 2));
        let take = |id: &str, l: &str, t0: f64, t1: f64, lufs: f64| Take {
            id: id.into(),
            line_id: l.into(),
            source_id: "src".into(),
            take_no: 1,
            take_type: TakeType::Full,
            t0,
            t1,
            words: vec![],
            score: 100.0,
            heard: String::new(),
            lufs: Some(lufs),
            db: 0.0,
            partial: false,
            pick: 0,
            favorite: false,
            favorite_origin: None,
            direction: String::new(),
            feedback: String::new(),
            direction_raw: String::new(),
            feedback_raw: String::new(),
            tags: vec![],
            note_ids: vec![],
            confidence: 1.0,
            needs_rematch: false,
            ai_flag: None,
            origin: Default::default(),
            read_as: None,
        };
        d.takes
            .insert("ta".into(), take("ta", "a", 10.0, 11.0, -20.0));
        d.takes
            .insert("tb".into(), take("tb", "b", 20.0, 22.0, -26.0));
        for (l, t) in [("a", "ta"), ("b", "tb")] {
            d.boxes.insert(
                l.into(),
                BoxState {
                    id: l.into(),
                    take_id: Some(t.into()),
                    gap_s: (l == "b").then_some(0.8),
                    ..Default::default()
                },
            );
        }
        let s = Scene {
            id: "sc".into(),
            number: 1,
            heading: "INT. LAB".into(),
            int_ext: "INT".into(),
            location: "LAB".into(),
            place: "Lab".into(),
            time_of_day: String::new(),
            order: vec!["a".into(), "b".into()],
            tightness: 1.0,
            story: Default::default(),
            room: Default::default(),
            tone: None,
            lock: None,
            status: None,
            room_pan: Default::default(),
            voice_pan: Default::default(),
        };
        (d, s)
    }

    /// Pacing without lead-in and tail: clips play exactly where the layout puts them.
    fn flat() -> Pacing {
        Pacing {
            handle_in_s: 0.0,
            handle_out_s: 0.0,
            ..Pacing::default()
        }
    }

    #[test]
    fn handles_widen_what_is_heard() {
        let (doc, scene) = sample_doc();
        let s = PlaySchedule::from_scene(&doc, &scene, &Pacing::default(), &MixSettings::default());
        let f = PlaySchedule::from_scene(&doc, &scene, &flat(), &MixSettings::default());
        // b starts 100 ms earlier, 100 ms earlier in its source, and rings 150 ms longer
        assert!((f.clips[1].start_s - s.clips[1].start_s - 0.1).abs() < 1e-9);
        assert!((f.clips[1].src_in - s.clips[1].src_in - 0.1).abs() < 1e-9);
        assert!((s.duration_s - f.duration_s - 0.15).abs() < 1e-9);
    }

    #[test]
    fn gaps_ripple_and_leveling_offsets() {
        let (mut doc, scene) = sample_doc();
        let p = flat();
        let leveled = MixSettings {
            level: true,
            ..MixSettings::default()
        };
        assert!(!MixSettings::default().level, "bounces never level");
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &leveled);
        assert_eq!(s.clips.len(), 2);
        assert!((s.clips[1].start_s - 1.8).abs() < 1e-9);
        // the source's takes measure -20 and -26: median -23, so both play as recorded,
        // keeping the difference between the two performances
        assert!(s.clips.iter().all(|c| c.gain_db.abs() < 1e-9));
        assert!((s.duration_s - 3.8).abs() < 1e-9);
        // a source recorded 10 dB hot plays 10 dB down, every take alike
        for t in doc.takes.values_mut() {
            t.lufs = t.lufs.map(|l| l + 10.0);
        }
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &leveled);
        assert!(s.clips.iter().all(|c| (c.gain_db - -10.0).abs() < 1e-9));
        let raw = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        assert!(
            raw.clips.iter().all(|c| c.gain_db == 0.0),
            "off: as recorded"
        );
        // ripple: a longer gap moves b and the end
        doc.boxes.get_mut("b").unwrap().gap_s = Some(1.5);
        let s2 = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        assert!((s2.clips[1].start_s - 2.5).abs() < 1e-9);
        let l = layout_scene(&doc, &scene, &p);
        assert!((s2.duration_s - l.duration_s).abs() < 1e-9);
    }

    #[test]
    fn muted_clips_keep_their_time_but_are_not_scheduled() {
        let (mut doc, scene) = sample_doc();
        let p = flat();
        doc.boxes.get_mut("a").unwrap().muted = true;
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        assert_eq!(s.clips.len(), 1);
        assert_eq!(s.clips[0].owner, "b");
        assert!((s.clips[0].start_s - 1.8).abs() < 1e-9);
        assert!((s.duration_s - 3.8).abs() < 1e-9);
    }

    #[test]
    fn a_voice_stands_where_the_scene_puts_it_and_the_lane_adds_to_that() {
        use apc_core::LaneMix;
        let (mut doc, mut scene) = sample_doc();
        let pan_of = |doc: &Document, scene: &Scene| {
            PlaySchedule::from_scene(doc, scene, &flat(), &MixSettings::default()).clips[0].pan
        };
        assert_eq!(pan_of(&doc, &scene), 0.0);
        scene.room_pan.insert("PAUL".into(), -0.5);
        assert_eq!(pan_of(&doc, &scene), -0.5);
        // the scene's own value wins over the room's
        scene.voice_pan.insert("PAUL".into(), 0.25);
        assert_eq!(pan_of(&doc, &scene), 0.25);
        doc.lane_mix.insert(
            "PAUL".into(),
            LaneMix {
                id: "PAUL".into(),
                volume_db: 0.0,
                pan: 0.25,
                mute: false,
                solo: false,
            },
        );
        assert_eq!(pan_of(&doc, &scene), 0.5);
    }

    #[test]
    fn a_panned_voice_sits_further_back_and_the_setting_turns_it_off() {
        let (mut doc, mut scene) = sample_doc();
        doc.characters.insert(
            "PAUL".into(),
            serde_json::from_value(serde_json::json!({
                "id": "PAUL", "label": "Paul", "color_index": 0, "wet": 0.25,
            }))
            .unwrap(),
        );
        let space_of = |scene: &Scene, pacing: &Pacing| {
            PlaySchedule::from_scene(&doc, scene, pacing, &MixSettings::default()).clips[0]
                .space
                .unwrap()
        };
        let centre = space_of(&scene, &flat());
        assert_eq!(centre.dry_gain, 1.0);

        scene.voice_pan.insert("PAUL".into(), -1.0);
        let side = space_of(&scene, &flat());
        assert!(side.dry_gain < centre.dry_gain && side.send_gain > centre.send_gain);

        let off = Pacing {
            pan_room: 0.0,
            ..flat()
        };
        let plain = space_of(&scene, &off);
        assert_eq!(
            (plain.dry_gain, plain.send_gain),
            (centre.dry_gain, centre.send_gain)
        );
    }

    /// `sample_doc` with Paul in the room and the first line moved back.
    fn moved_back(depth: f64, wet: f64) -> (Document, Scene) {
        let (mut doc, scene) = sample_doc();
        doc.characters.insert(
            "PAUL".into(),
            serde_json::from_value(serde_json::json!({
                "id": "PAUL", "label": "Paul", "color_index": 0, "wet": wet,
            }))
            .unwrap(),
        );
        let first = doc.boxes.keys().min().cloned().unwrap();
        if let Some(b) = doc.boxes.get_mut(&first) {
            b.depth = depth;
        }
        (doc, scene)
    }

    fn schedule(doc: &Document, scene: &Scene) -> PlaySchedule {
        PlaySchedule::from_scene(doc, scene, &flat(), &MixSettings::default())
    }

    #[test]
    fn a_line_nobody_moved_is_the_clip_it_always_was() {
        let (doc, scene) = moved_back(0.0, 0.25);
        let s = schedule(&doc, &scene);
        assert_eq!(s.clips.len(), 2);
        let space = s.clips[0].space.unwrap();
        assert_eq!((space.low_shelf_db, space.high_shelf_db), (0.0, 0.0));
    }

    #[test]
    fn a_line_moved_back_is_a_direct_sound_and_a_room_send() {
        let (doc, scene) = moved_back(0.5, 0.25);
        let plain = schedule(&moved_back(0.0, 0.25).0, &scene);
        let s = schedule(&doc, &scene);
        assert_eq!(s.clips.len(), 3, "the other line stays one clip");
        let (room, direct) = (&s.clips[0], &s.clips[1]);
        assert_eq!(room.owner, direct.owner);
        let (r, d) = (room.space.unwrap(), direct.space.unwrap());
        assert!(!direct.has_send() && room.has_send());
        assert_eq!(r.dry_gain, 0.0);
        assert!(d.dry_gain < 1.0);
        assert!(r.send_gain > plain.clips[0].space.unwrap().send_gain);
        assert!(d.low_shelf_db < 0.0 && d.low_shelf_db == r.low_shelf_db);
        assert!(
            d.high_shelf_db < 0.0 && r.high_shelf_db == 0.0,
            "direct only"
        );
    }

    #[test]
    fn the_direct_sound_arrives_late_but_never_after_the_reflections() {
        let (doc, mut scene) = moved_back(1.0, 0.25);
        for size in [0.0, 0.5, 1.0] {
            scene.room.size = size;
            let s = schedule(&doc, &scene);
            let lag = s.clips[1].start_s - s.clips[0].start_s;
            let pre = RoomShape::new(size, 0.5, 0.5).predelay_s;
            assert!(lag > 0.0 && lag < pre, "at size {size}: {lag} of {pre}");
        }
    }

    #[test]
    fn a_dry_character_moved_back_still_goes_back() {
        let (doc, scene) = moved_back(0.5, 0.0);
        let s = schedule(&doc, &scene);
        assert!(s.clips[0].has_send());
        assert!(s.clips[1].space.unwrap().dry_gain < 1.0);
        // and one left alone keeps D90's guarantee
        let (doc, scene) = moved_back(0.0, 0.0);
        assert!(schedule(&doc, &scene).clips[0].space.is_none());
    }

    #[test]
    fn depth_goes_on_top_of_the_pan() {
        let (doc, mut scene) = moved_back(0.5, 0.25);
        let centre = schedule(&doc, &scene).clips[1].space.unwrap();
        scene.voice_pan.insert("PAUL".into(), -1.0);
        let s = schedule(&doc, &scene);
        let side = s.clips[1].space.unwrap();
        assert!(side.dry_gain < centre.dry_gain);
        assert!(side.lowpass_hz < LOWPASS_OPEN_HZ);
        assert_eq!(s.clips[0].pan, s.clips[1].pan);
    }

    #[test]
    fn lane_mix_mutes_solos_and_sets_volume_and_pan() {
        use apc_core::LaneMix;
        let (mut doc, scene) = sample_doc();
        let p = Pacing::default();
        let mix = |id: &str, volume_db: f64, pan: f64, mute: bool, solo: bool| LaneMix {
            id: id.into(),
            volume_db,
            pan,
            mute,
            solo,
        };
        doc.lane_mix
            .insert("PAUL".into(), mix("PAUL", -6.0, 0.5, false, false));
        doc.takes.get_mut("tb").unwrap().lufs = Some(-20.0);
        let leveled = MixSettings {
            level: true,
            ..MixSettings::default()
        };
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &leveled);
        assert!(
            (s.clips[0].gain_db - -9.0).abs() < 1e-9,
            "-3 leveling, -6 lane"
        );
        assert_eq!(s.clips[0].pan, 0.5);
        doc.lane_mix
            .insert("PAUL".into(), mix("PAUL", 0.0, 0.0, true, false));
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        assert!(s.clips.is_empty(), "muted");
        doc.lane_mix
            .insert("PAUL".into(), mix("PAUL", 0.0, 0.0, false, false));
        doc.lane_mix
            .insert("music".into(), mix("music", 0.0, 0.0, false, true));
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        assert!(s.clips.is_empty(), "another lane is soloed");
    }

    #[test]
    fn anchored_sound_follows_and_room_tone_beds_the_scene() {
        use apc_core::*;
        let (mut doc, scene) = sample_doc();
        doc.assets.insert(
            "as".into(),
            Asset {
                id: "as".into(),
                file: "Library/SFX/x.wav".into(),
                hash: "h".into(),
                category: "SFX/Misc".into(),
                tags: vec![],
                duration_s: 2.0,
                source: AssetSource::Imported,
                license: None,
                prompt: None,
                provider: None,
                lufs: None,
                remote_id: None,
            },
        );
        doc.placements.insert(
            "pl".into(),
            Placement {
                id: "pl".into(),
                scene_id: "sc".into(),
                asset_id: "as".into(),
                cue_id: None,
                lane: Lane::Sfx,
                character: None,
                layer: 0,
                anchor: Some(Anchor {
                    line_id: "b".into(),
                    word: None,
                    edge: Edge::Start,
                }),
                at_s: 0.0,
                offset_s: -0.2,
                in_s: 0.0,
                len_s: 1.0,
                fade_in_s: 0.0,
                fade_out_s: 0.0,
                fade_curve: Default::default(),
                gain_db: 0.0,
                muted: false,
                pan: 0.0,
                depth: 0.0,
                auto_fade: false,
                added: 0,
                looped: false,
                unglued: false,
            },
        );
        doc.locations.insert(
            "loc_lab".into(),
            Location {
                id: "loc_lab".into(),
                name: "LAB".into(),
                room_tone: RoomTone {
                    enabled: true,
                    source_id: Some("src".into()),
                    t0: 1.0,
                    t1: 2.0,
                    asset_id: None,
                    gain_db: -6.0,
                },
                description: String::new(),
                parent: None,
            },
        );
        let p = Pacing::default();
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        let sfx = s.clips.iter().find(|c| c.lane == "sfx1").unwrap();
        assert!((sfx.start_s - 1.6).abs() < 1e-9);
        let tone = s.clips.iter().find(|c| c.lane == "roomtone").unwrap();
        assert_eq!(tone.loop_len_s, Some(s.duration_s));
        // leveling moves the room tone with its source, and leaves library sounds alone
        doc.takes.get_mut("ta").unwrap().lufs = Some(-17.0);
        doc.takes.get_mut("tb").unwrap().lufs = Some(-17.0);
        let lv = MixSettings {
            level: true,
            ..MixSettings::default()
        };
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &lv);
        let tone = s.clips.iter().find(|c| c.lane == "roomtone").unwrap();
        assert!((tone.gain_db - -12.0).abs() < 1e-9, "-6 tone, -6 leveling");
        let sfx = s.clips.iter().find(|c| c.lane == "sfx1").unwrap();
        assert_eq!(sfx.gain_db, 0.0);
        doc.boxes.get_mut("b").unwrap().gap_s = Some(2.0);
        let s2 = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        let sfx2 = s2.clips.iter().find(|c| c.lane == "sfx1").unwrap();
        assert!(
            (sfx2.start_s - 2.8).abs() < 1e-9,
            "the effect moved with its line"
        );
    }
}
