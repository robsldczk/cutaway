//! The play schedule: every clip with its source range, timeline position, gain and fades.
//! Built from the shared `SceneLayout`, so playback and export hear the same cut.

use serde::{Deserialize, Serialize};

use apc_core::timeline::{layout_scene, placement_start, Pacing, SceneLayout};
use apc_core::{Document, Lane, Scene};

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
}

impl ScheduledClip {
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
    /// Match take loudness to this target (playback offset only).
    pub loudness_match: bool,
    pub target_lufs: f64,
    pub room_tone: bool,
    pub include_sound: bool,
}

impl Default for MixSettings {
    fn default() -> Self {
        MixSettings {
            loudness_match: false,
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
            c.pan = m.pan;
        }
    }
}

impl PlaySchedule {
    pub fn from_scene(
        doc: &Document,
        scene: &Scene,
        pacing: &Pacing,
        settings: &MixSettings,
    ) -> Self {
        let layout = layout_scene(doc, scene, pacing);
        Self::from_layout(doc, scene, &layout, settings)
    }

    pub fn from_layout(
        doc: &Document,
        scene: &Scene,
        layout: &SceneLayout,
        settings: &MixSettings,
    ) -> Self {
        let mut clips = Vec::new();
        for item in &layout.items {
            for p in &item.pieces {
                let mut gain = p.gain_db;
                if settings.loudness_match {
                    let lufs = doc.takes.get(&p.take_id).and_then(|t| t.lufs);
                    gain += apc_media::loudness::match_gain_db(lufs, settings.target_lufs);
                }
                clips.push(ScheduledClip {
                    source_id: p.source_id.clone(),
                    src_in: p.src_in,
                    src_out: p.src_out,
                    start_s: p.start_s,
                    gain_db: gain,
                    fade_in_s: p.fade_in_s,
                    fade_out_s: p.fade_out_s,
                    lane: item.character.clone().unwrap_or_else(|| "dialogue".into()),
                    loop_len_s: None,
                    pan: 0.0,
                    linear_fade: false,
                    owner: item.item_id.clone(),
                });
            }
        }
        let mut duration = layout.duration_s;
        if settings.include_sound {
            for p in doc.placements.values().filter(|p| p.scene_id == scene.id) {
                let start = placement_start(layout, p);
                clips.push(ScheduledClip {
                    source_id: format!("asset:{}", p.asset_id),
                    src_in: p.in_s,
                    src_out: p.in_s + p.len_s,
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
                    loop_len_s: None,
                    pan: 0.0,
                    linear_fade: false,
                    owner: p.id.clone(),
                });
                if let Some(last) = clips.last_mut() {
                    last.linear_fade = p.fade_curve == apc_core::FadeCurve::Linear;
                }
                duration = duration.max(start + p.len_s);
            }
        }
        if settings.room_tone {
            let loc = apc_core::script::location_id(&scene.location);
            if let Some(tone) = doc.locations.get(&loc).map(|l| &l.room_tone) {
                if tone.enabled && tone.t1 > tone.t0 + 0.2 {
                    let source_id = match (&tone.source_id, &tone.asset_id) {
                        (Some(s), _) => Some(s.clone()),
                        (None, Some(a)) => Some(format!("asset:{a}")),
                        _ => None,
                    };
                    if let Some(source_id) = source_id {
                        clips.push(ScheduledClip {
                            source_id,
                            src_in: tone.t0,
                            src_out: tone.t1,
                            start_s: 0.0,
                            gain_db: tone.gain_db,
                            fade_in_s: 0.2,
                            fade_out_s: 0.2,
                            lane: "roomtone".into(),
                            loop_len_s: Some(duration),
                            pan: 0.0,
                            linear_fade: false,
                            owner: loc,
                        });
                    }
                }
            }
        }
        apply_lane_mix(doc, &mut clips);
        clips.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
        PlaySchedule {
            clips,
            duration_s: duration,
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
        };
        (d, s)
    }

    #[test]
    fn gaps_ripple_and_loudness_offsets() {
        let (mut doc, scene) = sample_doc();
        let p = Pacing::default();
        let matched = MixSettings {
            loudness_match: true,
            ..MixSettings::default()
        };
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &matched);
        assert_eq!(s.clips.len(), 2);
        assert!((s.clips[1].start_s - 1.8).abs() < 1e-9);
        assert!(
            (s.clips[0].gain_db - -3.0).abs() < 1e-9,
            "-20 LUFS take is turned down to -23"
        );
        assert!((s.clips[1].gain_db - 3.0).abs() < 1e-9);
        assert!((s.duration_s - 3.8).abs() < 1e-9);
        // ripple: a longer gap moves b and the end
        doc.boxes.get_mut("b").unwrap().gap_s = Some(1.5);
        let s2 = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        assert!((s2.clips[1].start_s - 2.5).abs() < 1e-9);
        let l = layout_scene(&doc, &scene, &p);
        assert!((s2.duration_s - l.duration_s).abs() < 1e-9);
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
        let matched = MixSettings {
            loudness_match: true,
            ..MixSettings::default()
        };
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &matched);
        assert!(
            (s.clips[0].gain_db - -9.0).abs() < 1e-9,
            "-3 loudness, -6 lane"
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
            },
        );
        let p = Pacing::default();
        let s = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        let sfx = s.clips.iter().find(|c| c.lane == "sfx1").unwrap();
        assert!((sfx.start_s - 1.6).abs() < 1e-9);
        let tone = s.clips.iter().find(|c| c.lane == "roomtone").unwrap();
        assert_eq!(tone.loop_len_s, Some(s.duration_s));
        doc.boxes.get_mut("b").unwrap().gap_s = Some(2.0);
        let s2 = PlaySchedule::from_scene(&doc, &scene, &p, &MixSettings::default());
        let sfx2 = s2.clips.iter().find(|c| c.lane == "sfx1").unwrap();
        assert!(
            (sfx2.start_s - 2.8).abs() < 1e-9,
            "the effect moved with its line"
        );
    }
}
