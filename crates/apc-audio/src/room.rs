//! The room reverb: a 16-line feedback delay network behind a pre-delay and input diffusers.
//!
//! Every sound of a scene goes through one of these on a shared bus (see `depth` and
//! `render`). Plain Rust with no dependencies, chosen over fundsp and Freeverb on measurements
//! (D75). It is block-size independent and its only time-varying part, the delay modulation,
//! is a function of the absolute frame, so a given stretch of audio sounds the same however
//! it is cut into blocks.

/// Lines in the network. A power of two, for the Hadamard mix.
const LINES: usize = 16;

/// Line lengths at size 1.0, in seconds: unevenly spread, because lines at simple ratios
/// reinforce the same modes and that is what rings.
const LINE_S: [f64; LINES] = [
    0.0297, 0.0371, 0.0411, 0.0437, 0.0467, 0.0503, 0.0533, 0.0571, 0.0607, 0.0641, 0.0677, 0.0719,
    0.0757, 0.0797, 0.0839, 0.0887,
];

/// Input diffusers (allpass length in seconds, per channel): they turn a click into a burst,
/// so the first echoes of a tail are dense rather than a row of discrete taps.
const DIFFUSER_L_S: [f64; 4] = [0.0043, 0.0071, 0.0113, 0.0167];
const DIFFUSER_R_S: [f64; 4] = [0.0047, 0.0067, 0.0121, 0.0157];
const DIFFUSION: f32 = 0.7;

/// Delay modulation: a slow, tiny drift per line breaks up the standing modes that make a
/// static network sound metallic. About 2.5 cents at most: not heard as pitch.
const MOD_DEPTH_S: f64 = 0.000_35;
const MOD_RATE_MIN_HZ: f64 = 0.31;
const MOD_RATE_STEP_HZ: f64 = 0.047;

/// Keeps the feedback path out of denormals (slow on x86), far below hearing.
const ANTI_DENORMAL: f32 = 1e-18;

/// What a room sounds like, derived from the scene's settings. See [`RoomShape::new`].
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct RoomShape {
    /// Scale of the delay lines (0.35 … 1.0).
    pub scale: f64,
    /// Decay to −60 dB at low frequencies.
    pub decay_s: f64,
    /// High frequencies decay this much faster (ratio of the two decay times, 0…1).
    pub hf_ratio: f64,
    pub predelay_s: f64,
    /// Output level of the room (linear).
    pub wet: f64,
}

impl RoomShape {
    /// `size`, `damping` and `wet` run 0…1. The one place the settings become a sound: retune
    /// the curves here.
    pub fn new(size: f64, damping: f64, wet: f64) -> Self {
        let size = clean(size);
        let damping = clean(damping);
        RoomShape {
            scale: 0.35 + 0.65 * size,
            // small booth … large hall, exponential so the slider feels even
            decay_s: 0.25 * (2.8f64 / 0.25).powf(size),
            hf_ratio: 0.85 - 0.65 * damping,
            predelay_s: 0.002 + 0.038 * size,
            wet: clean(wet),
        }
    }

    /// Seconds after the last sound until the tail is out of hearing.
    pub fn tail_s(&self) -> f64 {
        self.decay_s + self.predelay_s
    }

    /// Seconds of audio the room must hear before a point in time to sound as it would have
    /// had it played from the start: −90 dB of whatever came earlier is left out.
    pub fn warmup_s(&self) -> f64 {
        self.decay_s * 1.5 + self.predelay_s
    }
}

fn clean(x: f64) -> f64 {
    if x.is_finite() {
        x.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// A circular delay line with a fractional read.
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Delay {
            buf: vec![0.0; len.max(2)],
            pos: 0,
        }
    }

    fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }

    /// The sample written `d` pushes ago (`d` ≥ 1, below the length).
    fn tap(&self, d: usize) -> f32 {
        let n = self.buf.len();
        let i = self.pos + n - d.min(n);
        self.buf[if i >= n { i - n } else { i }]
    }

    /// Linear read between whole samples.
    fn read(&self, d: f32) -> f32 {
        let whole = d as usize;
        let f = d - whole as f32;
        let a = self.tap(whole);
        let b = self.tap(whole + 1);
        a + (b - a) * f
    }
}

/// Schroeder allpass: flat in level, smears the phase.
struct Allpass {
    line: Delay,
    len: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Allpass {
            line: Delay::new(len + 1),
            len: len.max(1),
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let d = self.line.tap(self.len);
        let v = x + DIFFUSION * d;
        self.line.push(v);
        d - DIFFUSION * v
    }
}

/// One network line: delay, loss and damping (a one-pole lowpass whose pole sets how much
/// faster the highs die), and a triangle drift on its length.
struct Line {
    delay: Delay,
    base: f32,
    gain: f32,
    pole: f32,
    state: f32,
    /// Period of the drift in frames, and where in it this line is.
    period: u64,
    phase: u64,
    depth: f32,
}

impl Line {
    fn drift(&self) -> f32 {
        let half = (self.period / 2).max(1);
        let p = self.phase;
        let up = if p < half { p } else { self.period - p };
        (up as f32 / half as f32 * 2.0 - 1.0) * self.depth
    }
}

/// Fast Hadamard transform, normalised: lossless, and every line feeds every other.
#[inline]
fn hadamard(x: &mut [f32; LINES]) {
    let mut h = 1;
    while h < LINES {
        let mut i = 0;
        while i < LINES {
            for j in i..i + h {
                let (a, b) = (x[j], x[j + h]);
                x[j] = a + b;
                x[j + h] = a - b;
            }
            i += h * 2;
        }
        h *= 2;
    }
    for v in x.iter_mut() {
        *v *= 0.25; // 1/√16
    }
}

/// The running reverb of one room.
pub struct Room {
    shape: RoomShape,
    pre_l: Delay,
    pre_r: Delay,
    pre_len: usize,
    diff_l: [Allpass; 4],
    diff_r: [Allpass; 4],
    lines: Vec<Line>,
    /// Keeps the room's level steady as its decay changes.
    level: f32,
}

impl Room {
    pub fn new(rate: u32, shape: RoomShape) -> Self {
        let fs = f64::from(rate.max(1));
        let samples = |s: f64| (s * fs).round() as usize;
        let pre_len = samples(shape.predelay_s).max(1);
        let lines = LINE_S
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let len_s = s * shape.scale;
                // Loss per pass at DC from the decay time and at Nyquist from the shorter high
                // decay. A one-pole filter through those two points has its pole at
                // (r − 1) / (r + 1): always inside the unit circle, so the loop cannot blow up
                // at any setting (Jot's small-loss approximation of it does in small dark rooms).
                let g = 10f64.powf(-3.0 * len_s / shape.decay_s);
                let g_high = 10f64.powf(-3.0 * len_s / (shape.decay_s * shape.hf_ratio));
                let r = g / g_high;
                let a = (r - 1.0) / (r + 1.0);
                let rate_hz = MOD_RATE_MIN_HZ + MOD_RATE_STEP_HZ * i as f64;
                let depth = MOD_DEPTH_S * shape.scale * fs;
                Line {
                    delay: Delay::new(samples(len_s) + (depth as usize) + 4),
                    base: (len_s * fs) as f32,
                    gain: (g * (1.0 - a)) as f32,
                    pole: a as f32,
                    state: 0.0,
                    period: ((fs / rate_hz) as u64).max(2),
                    phase: 0,
                    depth: depth as f32,
                }
            })
            .collect();
        let diffusers = |lens: &[f64; 4]| -> [Allpass; 4] {
            lens.map(|s| Allpass::new(samples(s * shape.scale)))
        };
        Room {
            shape,
            pre_l: Delay::new(pre_len + 1),
            pre_r: Delay::new(pre_len + 1),
            pre_len,
            diff_l: diffusers(&DIFFUSER_L_S),
            diff_r: diffusers(&DIFFUSER_R_S),
            lines,
            level: (0.17 / shape.decay_s.powf(0.25)) as f32,
        }
    }

    pub fn shape(&self) -> &RoomShape {
        &self.shape
    }

    /// Clears the room and sets the drift to where it is at `frame`, so what follows sounds
    /// the same as if the room had run from frame 0.
    pub fn reset(&mut self, frame: u64) {
        for d in [&mut self.pre_l, &mut self.pre_r] {
            d.buf.fill(0.0);
        }
        for a in self.diff_l.iter_mut().chain(self.diff_r.iter_mut()) {
            a.line.buf.fill(0.0);
        }
        for (i, l) in self.lines.iter_mut().enumerate() {
            l.delay.buf.fill(0.0);
            l.state = 0.0;
            // staggered start so the lines never drift in step
            l.phase = (frame + l.period * i as u64 / LINES as u64) % l.period;
        }
    }

    /// One frame in, one wet frame out (before the room's wet level).
    #[inline]
    fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        // a broken sample must not stay in the feedback path for good
        let in_l = if in_l.is_finite() { in_l } else { 0.0 };
        let in_r = if in_r.is_finite() { in_r } else { 0.0 };
        let mut l = self.pre_l.tap(self.pre_len);
        let mut r = self.pre_r.tap(self.pre_len);
        self.pre_l.push(in_l);
        self.pre_r.push(in_r);
        for a in self.diff_l.iter_mut() {
            l = a.tick(l);
        }
        for a in self.diff_r.iter_mut() {
            r = a.tick(r);
        }

        let mut x = [0.0f32; LINES];
        let (mut out_l, mut out_r) = (0.0f32, 0.0f32);
        for (i, line) in self.lines.iter_mut().enumerate() {
            let v = line.delay.read(line.base + line.drift());
            // independent signs per side keep the two outputs decorrelated
            if i & 1 == 0 {
                out_l += v;
            } else {
                out_l -= v;
            }
            if i & 2 == 0 {
                out_r += v;
            } else {
                out_r -= v;
            }
            line.state = line.gain * v + line.pole * line.state;
            x[i] = line.state;
            line.phase += 1;
            if line.phase >= line.period {
                line.phase = 0;
            }
        }
        hadamard(&mut x);
        for (i, line) in self.lines.iter_mut().enumerate() {
            let feed = if i & 1 == 0 { l } else { r };
            let sign = if i & 2 == 0 { 1.0 } else { -1.0 };
            line.delay.push(x[i] + feed * sign + ANTI_DENORMAL);
        }
        (out_l * self.level, out_r * self.level)
    }

    /// Runs `send` (stereo interleaved) through the room and adds the result to `out`.
    pub fn process(&mut self, send: &[f32], out: &mut [f32]) {
        let wet = self.shape.wet as f32;
        for (s, o) in send.chunks_exact(2).zip(out.chunks_exact_mut(2)) {
            let (l, r) = self.tick(s[0], s[1]);
            o[0] += l * wet;
            o[1] += r * wet;
        }
    }

    /// Runs `send` through the room without keeping the output (warming it up).
    pub fn absorb(&mut self, send: &[f32]) {
        for s in send.chunks_exact(2) {
            self.tick(s[0], s[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48000;

    /// Left channel of the room's answer to a click.
    fn impulse(shape: RoomShape, seconds: f64) -> Vec<f32> {
        let frames = (seconds * f64::from(RATE)) as usize;
        let mut send = vec![0.0f32; frames * 2];
        send[0] = 1.0;
        send[1] = 1.0;
        let mut out = vec![0.0f32; frames * 2];
        let mut room = Room::new(RATE, shape);
        room.reset(0);
        room.process(&send, &mut out);
        out.iter().step_by(2).copied().collect()
    }

    /// Decay to −60 dB, from the slope between −5 and −35 dB of the backward energy integral.
    fn measured_decay_s(ir: &[f32]) -> f64 {
        let mut energy = vec![0.0f64; ir.len()];
        let mut acc = 0.0;
        for i in (0..ir.len()).rev() {
            acc += f64::from(ir[i]).powi(2);
            energy[i] = acc;
        }
        let db = |i: usize| 10.0 * (energy[i] / energy[0]).log10();
        let a = (0..ir.len()).find(|i| db(*i) < -5.0).unwrap();
        let b = (0..ir.len()).find(|i| db(*i) < -35.0).unwrap();
        (b - a) as f64 / f64::from(RATE) * 2.0
    }

    /// Energy of the first difference against the energy: how much of the signal is highs.
    fn brightness(x: &[f32]) -> f64 {
        let e: f64 = x.iter().map(|v| f64::from(*v).powi(2)).sum();
        let d: f64 = x.windows(2).map(|w| f64::from(w[1] - w[0]).powi(2)).sum();
        d / e.max(1e-30)
    }

    #[test]
    fn the_decay_lands_near_what_the_size_asks_for() {
        for size in [0.2, 0.45, 0.8] {
            let shape = RoomShape::new(size, 0.2, 1.0);
            let got = measured_decay_s(&impulse(shape, shape.decay_s * 2.0 + 1.0));
            assert!(
                (got / shape.decay_s - 1.0).abs() < 0.25,
                "size {size}: asked {:.2} s, got {got:.2} s",
                shape.decay_s
            );
        }
    }

    #[test]
    fn a_larger_room_rings_longer_and_starts_later() {
        let (small, large) = (RoomShape::new(0.1, 0.5, 1.0), RoomShape::new(0.9, 0.5, 1.0));
        assert!(large.decay_s > small.decay_s * 3.0);
        assert!(large.predelay_s > small.predelay_s);
        let first = |ir: &[f32]| ir.iter().position(|v| v.abs() > 1e-6).unwrap();
        assert!(first(&impulse(large, 0.5)) > first(&impulse(small, 0.5)));
    }

    #[test]
    fn damping_darkens_the_tail() {
        let tail = |damping| {
            let ir = impulse(RoomShape::new(0.6, damping, 1.0), 1.5);
            brightness(&ir[24000..48000])
        };
        assert!(tail(0.9) < tail(0.1) * 0.7, "{} {}", tail(0.9), tail(0.1));
    }

    #[test]
    fn the_tail_dies_away_and_stays_a_number() {
        for (size, damping) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            let shape = RoomShape::new(size, damping, 1.0);
            let ir = impulse(shape, shape.decay_s * 3.0);
            assert!(ir.iter().all(|v| v.is_finite()));
            let peak = ir.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let n = ir.len();
            let late = ir[n - 2400..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            // three decay times is −180 dB: well under −120 dB even with the highs held back
            assert!(late < peak * 1e-6, "size {size}: {late} of {peak}");
        }
    }

    #[test]
    fn a_long_loud_input_does_not_build_up() {
        let shape = RoomShape::new(1.0, 0.0, 1.0);
        let frames = RATE as usize * 8;
        // a full-level tone at a line's resonance is the worst case for a feedback network
        let hz = 1.0 / LINE_S[0];
        let send: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let v = (i as f64 / f64::from(RATE) * hz * std::f64::consts::TAU).sin() as f32;
                [v, v]
            })
            .collect();
        let mut out = vec![0.0f32; frames * 2];
        let mut room = Room::new(RATE, shape);
        room.reset(0);
        room.process(&send, &mut out);
        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak.is_finite() && peak < 4.0, "peak {peak}");
    }

    #[test]
    fn the_tail_has_no_single_ringing_tones() {
        // Share of the late tail in its strongest line against the median line, by a plain
        // DFT over 200 Hz … 5 kHz. A noise-like tail sits near 12 dB, a metallic one above 20.
        let ir = impulse(RoomShape::new(0.45, 0.5, 1.0), 1.2);
        let n = 8192;
        let tail = &ir[14400..14400 + n];
        let window = |i: usize| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
        let bin_hz = f64::from(RATE) / n as f64;
        let mut power: Vec<f64> = ((200.0 / bin_hz) as usize..(5000.0 / bin_hz) as usize)
            .map(|k| {
                let w = std::f64::consts::TAU * k as f64 / n as f64;
                let (mut re, mut im) = (0.0, 0.0);
                for (i, v) in tail.iter().enumerate() {
                    let x = f64::from(*v) * window(i);
                    re += x * (w * i as f64).cos();
                    im -= x * (w * i as f64).sin();
                }
                re * re + im * im
            })
            .collect();
        power.sort_by(f64::total_cmp);
        let median = power[power.len() / 2];
        let peak = power[power.len() - 1];
        let ratio_db = 10.0 * (peak / median).log10();
        assert!(ratio_db < 18.0, "{ratio_db:.1} dB");
    }

    #[test]
    fn the_two_sides_are_not_the_same_signal() {
        let frames = RATE as usize;
        let mut send = vec![0.0f32; frames * 2];
        send[0] = 1.0;
        send[1] = 1.0;
        let mut out = vec![0.0f32; frames * 2];
        let mut room = Room::new(RATE, RoomShape::new(0.5, 0.5, 1.0));
        room.reset(0);
        room.process(&send, &mut out);
        let (mut ll, mut rr, mut lr) = (0.0f64, 0.0f64, 0.0f64);
        for f in out.chunks_exact(2) {
            let (l, r) = (f64::from(f[0]), f64::from(f[1]));
            ll += l * l;
            rr += r * r;
            lr += l * r;
        }
        assert!((lr / (ll * rr).sqrt()).abs() < 0.2);
    }

    #[test]
    fn the_same_input_gives_the_same_output_bit_for_bit() {
        let shape = RoomShape::new(0.5, 0.5, 0.7);
        assert_eq!(impulse(shape, 1.0), impulse(shape, 1.0));
    }

    #[test]
    fn a_broken_sample_does_not_stay_in_the_room() {
        let mut room = Room::new(RATE, RoomShape::new(0.5, 0.5, 1.0));
        room.reset(0);
        let mut send = vec![0.0f32; 9600];
        send[100] = f32::NAN;
        send[101] = f32::INFINITY;
        send[400] = 1.0;
        let mut out = vec![0.0f32; 9600];
        room.process(&send, &mut out);
        assert!(out.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn wet_sets_the_level_of_the_room() {
        let ir = |wet| impulse(RoomShape::new(0.5, 0.5, wet), 0.5);
        let (half, full) = (ir(0.5), ir(1.0));
        assert!(half
            .iter()
            .zip(&full)
            .all(|(h, f)| (h * 2.0 - f).abs() < 1e-6));
        assert!(ir(0.0).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn it_works_at_every_rate_the_app_meets() {
        for rate in [22050, 44100, 48000, 96000, 192_000] {
            let mut room = Room::new(rate, RoomShape::new(0.5, 0.5, 1.0));
            room.reset(0);
            let mut send = vec![0.0f32; rate as usize];
            send[0] = 1.0;
            let mut out = vec![0.0f32; rate as usize];
            room.process(&send, &mut out);
            assert!(out.iter().all(|v| v.is_finite()));
            assert!(out.iter().any(|v| v.abs() > 1e-4), "{rate}: silent");
        }
    }

    #[test]
    fn settings_out_of_range_are_the_nearest_end() {
        assert_eq!(
            RoomShape::new(9.0, -1.0, 2.0),
            RoomShape::new(1.0, 0.0, 1.0)
        );
        assert_eq!(
            RoomShape::new(f64::NAN, f64::NAN, f64::NAN),
            RoomShape::new(0.0, 0.0, 0.0)
        );
    }
}
