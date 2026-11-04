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
