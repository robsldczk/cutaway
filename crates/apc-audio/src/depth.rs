//! Depth: how far back a sound sits in its scene's room, as one macro. This is the only place
//! the curve lives; retune the constants here (starting values and why: D76).

/// Level lost at full depth, on dry and send alike: the sound is simply further away.
const LEVEL_FAR_DB: f64 = -6.0;
/// Dry signal lost on top of that at full depth: the balance tips towards the room.
const DRY_FAR_DB: f64 = -8.0;
/// Send to the room when close and at full depth, before the level offset.
const SEND_CLOSE_DB: f64 = -20.0;
const SEND_FAR_DB: f64 = 4.0;
/// High-frequency roll-off (one pole, so gentle). Open means no filter at all.
pub const LOWPASS_OPEN_HZ: f64 = 20_000.0;
const LOWPASS_FAR_HZ: f64 = 3_000.0;

/// A voice's send to its scene's room at the bottom and top of its wet setting.
const VOICE_SEND_MIN_DB: f64 = -30.0;
const VOICE_SEND_MAX_DB: f64 = -6.0;

/// A voice's send to the room (linear) for its wet setting, 0 … 1. Only the send: a voice stays
/// at full level and full brightness, however much room it takes.
pub fn voice_send(wet: f64) -> f64 {
    if !wet.is_finite() || wet <= 0.0 {
        return 0.0;
    }
    gain(VOICE_SEND_MIN_DB + (VOICE_SEND_MAX_DB - VOICE_SEND_MIN_DB) * wet.min(1.0))
}

/// How far back a hard-panned voice goes with its setting at full, on the depth scale. Small on
/// purpose: at the default of half that's 1.4 dB less direct sound and 1.8 dB more room.
const PAN_BACK_FULL: f64 = 0.2;

/// How far back a voice sits for its pan and `amount` (0 … 1). Squared, so it stays close around
/// the centre and only the sides go back.
fn pan_back(pan: f64, amount: f64) -> f64 {
    if !pan.is_finite() || !amount.is_finite() {
        return 0.0;
    }
    amount.clamp(0.0, 1.0) * PAN_BACK_FULL * pan.clamp(-1.0, 1.0).powi(2)
}

/// A voice in the room: its send for its wet setting, moved back along the depth curve the further
/// it's panned. In the centre that's `voice_send` alone, and full level and brightness.
pub fn voice_mix(wet: f64, pan: f64, amount: f64) -> DepthMix {
    let close = depth_mix(0.0);
    let back = depth_mix(pan_back(pan, amount));
    DepthMix {
        dry_gain: back.dry_gain,
        send_gain: voice_send(wet) * back.send_gain / close.send_gain,
        lowpass_hz: back.lowpass_hz,
    }
}

/// What a depth does to a sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthMix {
    /// Level of the direct sound (linear).
    pub dry_gain: f64,
    /// Level sent to the room (linear).
    pub send_gain: f64,
    /// Cutoff of the roll-off; `LOWPASS_OPEN_HZ` and above is no filter.
    pub lowpass_hz: f64,
}

fn gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// `depth` runs 0 (close) … 1 (far). Moves the sound two ways at once: the balance goes from
/// dry towards the room, and it sits further back (quieter, softer highs).
pub fn depth_mix(depth: f64) -> DepthMix {
    let d = if depth.is_finite() {
        depth.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let level_db = LEVEL_FAR_DB * d;
    DepthMix {
        dry_gain: gain(level_db + DRY_FAR_DB * d),
        send_gain: gain(level_db + SEND_CLOSE_DB + (SEND_FAR_DB - SEND_CLOSE_DB) * d),
        lowpass_hz: LOWPASS_OPEN_HZ * (LOWPASS_FAR_HZ / LOWPASS_OPEN_HZ).powf(d),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_takes_less_of_the_room_than_a_close_sound_by_default() {
        assert_eq!(voice_send(0.0), 0.0, "dry is dry");
        let a_little = voice_send(0.25);
        assert!(a_little > 0.0 && a_little < depth_mix(0.0).send_gain);
        assert!(voice_send(1.0) > a_little);
    }

    #[test]
    fn a_centred_voice_is_exactly_what_it_was() {
        let m = voice_mix(0.25, 0.0, 1.0);
        assert_eq!(m.send_gain, voice_send(0.25));
        assert_eq!((m.dry_gain, m.lowpass_hz), (1.0, LOWPASS_OPEN_HZ));
    }

    #[test]
    fn a_panned_voice_goes_back_a_little_and_the_same_on_both_sides() {
        let (centre, side) = (voice_mix(0.25, 0.0, 0.5), voice_mix(0.25, -1.0, 0.5));
        assert!(side.send_gain > centre.send_gain, "more room");
        assert!(side.dry_gain < centre.dry_gain, "less direct sound");
        assert!(side.lowpass_hz < centre.lowpass_hz, "softer highs");
        assert_eq!(side, voice_mix(0.25, 1.0, 0.5));
        let db = |x: f64| 20.0 * x.log10();
        assert!((db(side.dry_gain) + 1.4).abs() < 0.01, "only a touch");
        assert!((db(side.send_gain / centre.send_gain) - 1.8).abs() < 0.01);
    }

    #[test]
    fn the_sides_go_back_and_the_middle_hardly_does() {
        let drop = |pan| 1.0 - voice_mix(0.25, pan, 1.0).dry_gain;
        assert!(drop(0.25) < drop(1.0) / 10.0);
        let mut last = voice_mix(0.25, 0.0, 1.0);
        for i in 1..=100 {
            let m = voice_mix(0.25, i as f64 / 100.0, 1.0);
            assert!(m.send_gain > last.send_gain && m.dry_gain < last.dry_gain);
            last = m;
        }
    }

    #[test]
    fn the_setting_scales_it_and_off_is_off() {
        assert_eq!(voice_mix(0.25, 1.0, 0.0), voice_mix(0.25, 0.0, 0.0));
        assert!(voice_mix(0.25, 1.0, 1.0).send_gain > voice_mix(0.25, 1.0, 0.5).send_gain);
        assert_eq!(voice_mix(0.25, f64::NAN, 0.5), voice_mix(0.25, 0.0, 0.5));
        assert_eq!(voice_mix(0.25, 1.0, 9.0), voice_mix(0.25, 1.0, 1.0));
    }

    #[test]
    fn close_is_the_sound_as_it_is() {
        let m = depth_mix(0.0);
        assert_eq!(m.dry_gain, 1.0, "full level, untouched");
        assert_eq!(m.lowpass_hz, LOWPASS_OPEN_HZ, "no filter");
        assert!(m.send_gain > 0.0, "even a close sound is in the room");
    }

    #[test]
    fn the_curve_is_monotonic_all_the_way_back() {
        let mut last = depth_mix(0.0);
        for i in 1..=1000 {
            let m = depth_mix(i as f64 / 1000.0);
            assert!(m.dry_gain < last.dry_gain, "dry falls at {i}");
            assert!(m.send_gain > last.send_gain, "send rises at {i}");
            assert!(m.lowpass_hz < last.lowpass_hz, "highs close at {i}");
            last = m;
        }
    }

    #[test]
    fn far_means_more_room_than_direct_sound() {
        let (near, far) = (depth_mix(0.0), depth_mix(1.0));
        assert!(near.dry_gain / near.send_gain > 5.0);
        assert!(far.send_gain > far.dry_gain);
    }

    #[test]
    fn out_of_range_values_are_the_nearest_end() {
        assert_eq!(depth_mix(-3.0), depth_mix(0.0));
        assert_eq!(depth_mix(7.0), depth_mix(1.0));
        assert_eq!(depth_mix(f64::NAN), depth_mix(0.0));
    }
}
