//! Lo-fi insert: wobble -> rate reducer -> bit crusher -> vinyl noise -> high cut, with a
//! dry/wet mix. Mono in, mono out; sits on the synth channel after the drive.
//!
//! Stages, in processing order:
//!
//! - **Wobble** (`LofiWobble`, wow and flutter): a slow random LFO drives a short modulated
//!   delay line ([`ModDelay`], up to [`MAX_WOBBLE_MS`]). The LFO is a smoothed random walk:
//!   each segment picks a random rate in [`WOBBLE_MIN_HZ`]`..=`[`WOBBLE_MAX_HZ`] and a random
//!   target delay, and glides there with a raised cosine over half a cycle. The segment's step
//!   is sized so that the *slope* of the delay (which is what shifts the pitch) peaks at a
//!   random 50..100 % of `+-20 cents x LofiWobble` ([`MAX_WOBBLE_CENTS`]); the walk is kept
//!   inside `0..=MAX_WOBBLE_MS` by reversing a step that would leave it. With the wobble at
//!   `0.0` the walk glides back to `0.0` (at no more than 20 cents of pitch slope) and then
//!   reads the delay line at distance zero, which is bit-exact, so the stage is transparent.
//!   The wobble amount is picked up at the start of each segment (at most ~1 s later) rather
//!   than smoothed per sample, because a per-sample depth change would itself be a pitch
//!   jump.
//! - **Rate reducer** (`LofiRate`): sample-and-hold, every output held for `N` samples
//!   (`1` = off).
//! - **Bit crusher** (`LofiBits`): mid-tread quantisation, `round(x * 2^(bits-1)) /
//!   2^(bits-1)`, i.e. `2^bits` levels across `-1..=1` (fractional bit depths are allowed and
//!   simply use a fractional scale).
//! - **Vinyl noise** (`LofiNoise`): a pink-ish hiss (white noise through Paul Kellet's
//!   economy pink filter, scaled to about -40 dBFS RMS at `1.0`) plus sparse crackle: random
//!   impulses at `LofiNoise x `[`CRACKLE_RATE_HZ`] per second, each a one-sample hit of
//!   random polarity and -28..-16 dBFS amplitude decaying with a [`CRACKLE_DECAY_MS`] time
//!   constant, which stands well above the hiss floor. The level is smoothed; at `0.0` the
//!   stage adds exactly zero.
//! - **High cut** (`LofiTone`): one-pole lowpass; at the top of the range (20 kHz) it is
//!   bypassed exactly so the default is transparent. The coefficient is smoothed.
//! - **Mix** (`LofiMix`, smoothed): `x + (wet - x) * mix`; at `0.0` the output is the input
//!   bit for bit.
//!
//! Real-time safe after [`Lofi::new`]: the only heap object is the delay line, allocated in
//! `new` and only indexed afterwards; nothing here locks, logs or panics, and every feedback
//! path is denormal-flushed. Non-finite input is treated as silence; the output is always
//! finite.

use core::f32::consts::PI;

use crate::drums::noise::Noise;
use crate::dsp::flush_denormal;
use crate::params::{DEFAULT_SMOOTH_MS, ParamId, Smoother};

use super::mod_delay::ModDelay;

/// Longest wobble delay in milliseconds (the walk lives in `0..=MAX_WOBBLE_MS`).
pub const MAX_WOBBLE_MS: f32 = 20.0;

/// Peak pitch deviation of the wobble at `LofiWobble = 1.0`, in cents.
pub const MAX_WOBBLE_CENTS: f32 = 20.0;

/// Slowest random-walk segment rate, Hz.
pub const WOBBLE_MIN_HZ: f32 = 0.5;

/// Fastest random-walk segment rate, Hz.
pub const WOBBLE_MAX_HZ: f32 = 3.0;

/// Crackle impulses per second at `LofiNoise = 1.0` (scales linearly with the level).
pub const CRACKLE_RATE_HZ: f32 = 16.0;

/// Decay time constant of one crackle impulse, milliseconds.
pub const CRACKLE_DECAY_MS: f32 = 0.5;

/// Level of the whole noise bed (hiss plus crackle) at `LofiNoise = 1.0`, dBFS RMS
/// (approximately; the hiss alone sits about 3 dB lower, see [`HISS_GAIN`]).
pub const NOISE_DBFS: f32 = -40.0;

/// Gain applied to the Kellet pink filter output. Measured on uniform white noise (RMS
/// 0.577) the three correlated poles give the filter an RMS gain of about 4.0, so this
/// lands the hiss near -43 dBFS; the crackle (-28..-16 dBFS hits, [`CRACKLE_RATE_HZ`] per
/// second, decaying over [`CRACKLE_DECAY_MS`]) adds about as much energy again, for a bed
/// of about [`NOISE_DBFS`].
pub const HISS_GAIN: f32 = 0.003;

/// Top of the `LofiTone` range; at or above it the high cut is bypassed exactly.
pub const TONE_BYPASS_HZ: f32 = 20_000.0;

/// Smoothing time for the noise level and tone coefficient, ms.
const SLOW_SMOOTH_MS: f32 = 30.0;

/// Seed of the vinyl-noise generator (distinct from the drum kit's).
const NOISE_SEED: u32 = 0x5EED_1F1A;

/// Pitch ratio deviation for [`MAX_WOBBLE_CENTS`]: `2^(cents/1200) - 1`.
fn max_wobble_ratio() -> f32 {
    2.0_f32.powf(MAX_WOBBLE_CENTS / 1200.0) - 1.0
}

/// One-pole lowpass coefficient for `cutoff_hz`; `1.0` (exact bypass) at or above
/// [`TONE_BYPASS_HZ`].
fn tone_coefficient(sample_rate: f32, cutoff_hz: f32) -> f32 {
    if !cutoff_hz.is_finite() || cutoff_hz >= TONE_BYPASS_HZ || sample_rate <= 0.0 {
        return 1.0;
    }
    if cutoff_hz <= 0.0 {
        return 0.0;
    }
    (1.0 - (-core::f32::consts::TAU * cutoff_hz / sample_rate).exp()).clamp(0.0, 1.0)
}

/// The lo-fi insert. See the module docs for the signal path.
#[derive(Debug, Clone, PartialEq)]
pub struct Lofi {
    sample_rate: f32,
    /// Clamped target values of the six `Lofi*` ids, in `ParamId::ALL` order from `LofiBits`.
    values: [f32; 6],
    // Bit crusher.
    /// `2^(bits - 1)`.
    scale: f32,
    // Rate reducer.
    hold_len: usize,
    hold_count: usize,
    held: f32,
    // Wobble.
    delay: ModDelay,
    wobble: f32,
    /// Random-walk segment: from `walk_a` to `walk_b` (samples) as `walk_phase` runs 0..1.
    walk_a: f32,
    walk_b: f32,
    walk_phase: f32,
    walk_inc: f32,
    // Vinyl noise.
    rng: Noise,
    pink: [f32; 3],
    crackle: f32,
    crackle_decay: f32,
    noise_level: Smoother,
    // High cut.
    tone_coeff: Smoother,
    lp: f32,
    // Mix.
    mix: Smoother,
}

impl Lofi {
    /// Pre-allocates the insert for `sample_rate` (non-finite or non-positive rates fall back
    /// to 48 kHz) with every `Lofi*` parameter at its [`ParamId::default_value`].
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let max_samples = (sample_rate * MAX_WOBBLE_MS * 0.001).ceil() as usize;
        let mut lofi = Self {
            sample_rate,
            values: [0.0; 6],
            scale: 1.0,
            hold_len: 1,
            hold_count: 0,
            held: 0.0,
            delay: ModDelay::new(max_samples),
            wobble: 0.0,
            walk_a: 0.0,
            walk_b: 0.0,
            walk_phase: 1.0,
            walk_inc: 0.0,
            rng: Noise::new(NOISE_SEED),
            pink: [0.0; 3],
            crackle: 0.0,
            crackle_decay: (-1.0 / (sample_rate * CRACKLE_DECAY_MS * 0.001)).exp(),
            noise_level: Smoother::new(sample_rate, SLOW_SMOOTH_MS, 0.0),
            tone_coeff: Smoother::new(sample_rate, SLOW_SMOOTH_MS, 1.0),
            lp: 0.0,
            mix: Smoother::new(sample_rate, DEFAULT_SMOOTH_MS, 0.0),
        };
        for id in Self::IDS {
            lofi.set_param(id, id.default_value());
        }
        lofi.reset();
        lofi
    }

    /// The parameters this stage owns, in [`ParamId::ALL`] order.
    pub const IDS: [ParamId; 6] = [
        ParamId::LofiBits,
        ParamId::LofiRate,
        ParamId::LofiWobble,
        ParamId::LofiNoise,
        ParamId::LofiTone,
        ParamId::LofiMix,
    ];

    fn slot(id: ParamId) -> Option<usize> {
        Self::IDS.iter().position(|&i| i == id)
    }

    /// Sample rate the insert was built for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Sets one of the `Lofi*` parameters (clamped with [`ParamId::clamp`]); other ids are
    /// ignored. Noise level, tone and mix glide; bits and rate step (they are quantisers);
    /// the wobble depth is taken up at the next random-walk segment.
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let Some(slot) = Self::slot(id) else {
            return;
        };
        let v = id.clamp(value);
        self.values[slot] = v;
        match id {
            ParamId::LofiBits => self.scale = 2.0_f32.powf(v - 1.0),
            // `as usize` saturates; the clamp already keeps it in 1..=16.
            ParamId::LofiRate => self.hold_len = (v.round() as usize).max(1),
            ParamId::LofiWobble => self.wobble = v,
            ParamId::LofiNoise => self.noise_level.set_target(v),
            ParamId::LofiTone => self
                .tone_coeff
                .set_target(tone_coefficient(self.sample_rate, v)),
            ParamId::LofiMix => self.mix.set_target(v),
            _ => {}
        }
    }

    /// Current (clamped target) value of a `Lofi*` parameter; other ids return `0.0`.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        Self::slot(id).map_or(0.0, |s| self.values[s])
    }

    /// Current wobble delay in samples (for tests and meters).
    #[must_use]
    pub fn wobble_delay_samples(&self) -> f32 {
        self.walk_value()
    }

    /// Clears the delay line, filters, hold and noise states, returns the wobble walk to
    /// zero and snaps the smoothers to their targets. May click; use between notes or on
    /// load.
    pub fn reset(&mut self) {
        self.delay.reset();
        self.hold_count = 0;
        self.held = 0.0;
        self.walk_a = 0.0;
        self.walk_b = 0.0;
        self.walk_phase = 1.0;
        self.walk_inc = 0.0;
        self.pink = [0.0; 3];
        self.crackle = 0.0;
        self.lp = 0.0;
        for s in [&mut self.noise_level, &mut self.tone_coeff, &mut self.mix] {
            s.snap(s.target());
        }
    }

    /// Raised-cosine interpolation of the current walk segment.
    #[inline]
    fn walk_value(&self) -> f32 {
        let t = 0.5 * (1.0 - (PI * self.walk_phase.min(1.0)).cos());
        self.walk_a + (self.walk_b - self.walk_a) * t
    }

    /// Uniform random in `0.0..1.0`.
    #[inline]
    fn rand01(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Starts a new random-walk segment from the current end point.
    fn new_segment(&mut self) {
        let a = self.walk_b;
        self.walk_a = a;
        self.walk_phase = 0.0;
        let sr = self.sample_rate;
        let dev = max_wobble_ratio();
        let hz;
        if self.wobble <= 0.0 {
            // Head home at a pitch slope no steeper than the full wobble, then idle.
            self.walk_b = 0.0;
            hz = if a > 0.0 {
                (dev * sr / (PI * a)).clamp(0.05, WOBBLE_MAX_HZ)
            } else {
                1.0
            };
        } else {
            hz = WOBBLE_MIN_HZ + (WOBBLE_MAX_HZ - WOBBLE_MIN_HZ) * self.rand01();
            let strength = 0.5 + 0.5 * self.rand01();
            // Peak slope of a raised cosine from a to b over half a cycle is (b - a) * pi * hz
            // (in samples per sample when the step is in samples and hz in cycles/s / sr).
            let mut step = dev * self.wobble * strength * sr / (PI * hz);
            if self.rng.next_u32() & 1 == 1 {
                step = -step;
            }
            let max = self.delay.max_delay();
            if a + step < 0.0 || a + step > max {
                step = -step;
            }
            self.walk_b = (a + step).clamp(0.0, max);
        }
        // Half a cycle at `hz` spans sr / (2 hz) samples.
        self.walk_inc = 2.0 * hz / sr;
    }

    /// Wobble stage: pushes `x` into the delay line and reads it back at the walk's delay.
    #[inline]
    fn wobble_stage(&mut self, x: f32) -> f32 {
        let idle = self.walk_a == 0.0 && self.walk_b == 0.0;
        if self.walk_phase >= 1.0 || (idle && self.wobble > 0.0) {
            self.new_segment();
        }
        let d = self.walk_value();
        self.walk_phase += self.walk_inc;
        self.delay.push(x);
        self.delay.read(d)
    }

    /// Vinyl noise for this sample at the current (smoothed) level.
    #[inline]
    fn noise_stage(&mut self) -> f32 {
        let level = self.noise_level.tick();
        if level <= 0.0 {
            // Nothing to add; skip the generator and filters.
            return 0.0;
        }
        let w = self.rng.next_f32();
        let [b0, b1, b2] = self.pink;
        let b0 = flush_denormal(0.997_65 * b0 + w * 0.099_046);
        let b1 = flush_denormal(0.963 * b1 + w * 0.296_516_4);
        let b2 = flush_denormal(0.57 * b2 + w * 1.052_691_3);
        self.pink = [b0, b1, b2];
        let hiss = (b0 + b1 + b2 + w * 0.1848) * HISS_GAIN;

        self.crackle = flush_denormal(self.crackle * self.crackle_decay);
        let target = self.values[3];
        let p = target * CRACKLE_RATE_HZ / self.sample_rate;
        if self.rand01() < p {
            // -28..-16 dBFS, random polarity.
            let amp = 0.04 + 0.12 * self.rand01();
            let sign = if self.rng.next_u32() & 1 == 1 {
                -1.0
            } else {
                1.0
            };
            self.crackle += amp * sign;
        }
        level * (hiss + self.crackle)
    }

    /// Processes one sample. Non-finite input is treated as `0.0`; the output is finite. With
    /// `LofiMix` at `0.0` the output equals the input exactly (the stages still run so that
    /// turning the mix up is seamless).
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let mix = self.mix.tick();

        let mut y = self.wobble_stage(x);

        // Sample-and-hold.
        if self.hold_len > 1 {
            if self.hold_count == 0 {
                self.held = y;
            }
            self.hold_count += 1;
            if self.hold_count >= self.hold_len {
                self.hold_count = 0;
            }
            y = self.held;
        }

        // Mid-tread bit crusher.
        y = (y * self.scale).round() / self.scale;

        y += self.noise_stage();

        // High cut.
        let c = self.tone_coeff.tick();
        if c >= 1.0 {
            self.lp = y;
        } else {
            self.lp = flush_denormal(self.lp + c * (y - self.lp));
        }
        let wet = self.lp;

        let out = x + (wet - x) * mix;
        if out.is_finite() { out } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(n: usize, hz: f32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * core::f32::consts::TAU * hz / SR).sin() * amp)
            .collect()
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum::<f64>()
            / buf.len() as f64)
            .sqrt() as f32
    }

    #[test]
    fn defaults_and_param_routing() {
        let l = Lofi::new(SR);
        for id in Lofi::IDS {
            assert_eq!(l.param(id), id.default_value(), "{id:?}");
        }
        assert_eq!(l.param(ParamId::DriveMix), 0.0);
        let mut l = Lofi::new(SR);
        l.set_param(ParamId::LofiBits, 1.0);
        assert_eq!(l.param(ParamId::LofiBits), 4.0);
        l.set_param(ParamId::LofiRate, 99.0);
        assert_eq!(l.param(ParamId::LofiRate), 16.0);
        let before = l.clone();
        l.set_param(ParamId::ChorusMix, 1.0);
        assert_eq!(l, before);
        assert_eq!(Lofi::new(f32::NAN).sample_rate(), 48_000.0);
    }

    #[test]
    fn mix_zero_is_bit_transparent_whatever_the_settings() {
        let mut l = Lofi::new(SR);
        l.set_param(ParamId::LofiBits, 4.0);
        l.set_param(ParamId::LofiRate, 8.0);
        l.set_param(ParamId::LofiWobble, 1.0);
        l.set_param(ParamId::LofiNoise, 1.0);
        l.set_param(ParamId::LofiTone, 1000.0);
        l.set_param(ParamId::LofiMix, 0.0);
        l.reset();
        for (i, &x) in sine(20_000, 440.0, 0.7).iter().enumerate() {
            assert_eq!(l.process(x), x, "sample {i}");
        }
    }

    #[test]
    fn default_wet_path_is_near_transparent() {
        let mut l = Lofi::new(SR);
        l.set_param(ParamId::LofiMix, 1.0);
        l.reset();
        let input = sine(20_000, 440.0, 0.5);
        let diff: Vec<f32> = input.iter().map(|&x| l.process(x) - x).collect();
        assert!(rms(&diff) < 1e-3, "rms {}", rms(&diff));
        // 16-bit quantisation error is bounded by half a step.
        assert!(diff.iter().all(|d| d.abs() <= 1.0 / 32_768.0));
    }

    #[test]
    fn tone_coefficient_bypasses_at_the_top() {
        assert_eq!(tone_coefficient(SR, 20_000.0), 1.0);
        assert_eq!(tone_coefficient(SR, 25_000.0), 1.0);
        assert!(tone_coefficient(SR, 2_000.0) < 0.5);
        assert_eq!(tone_coefficient(SR, 0.0), 0.0);
    }

    #[test]
    fn non_finite_input_is_silenced() {
        let mut l = Lofi::new(SR);
        l.set_param(ParamId::LofiMix, 1.0);
        l.set_param(ParamId::LofiWobble, 1.0);
        l.reset();
        assert_eq!(l.process(f32::NAN), 0.0);
        assert_eq!(l.process(f32::INFINITY), 0.0);
        for _ in 0..1000 {
            assert_eq!(l.process(0.0), 0.0);
        }
    }

    #[test]
    fn wobble_walk_stays_in_range_and_returns_home() {
        let mut l = Lofi::new(SR);
        l.set_param(ParamId::LofiMix, 1.0);
        l.set_param(ParamId::LofiWobble, 1.0);
        l.reset();
        let max = l.delay.max_delay();
        let mut moved = false;
        for _ in 0..(SR as usize * 4) {
            let _ = l.process(0.3);
            let d = l.wobble_delay_samples();
            assert!((0.0..=max).contains(&d), "{d}");
            moved |= d > 1.0;
        }
        assert!(moved);
        l.set_param(ParamId::LofiWobble, 0.0);
        for _ in 0..(SR as usize * 4) {
            let _ = l.process(0.3);
        }
        assert_eq!(l.wobble_delay_samples(), 0.0);
        // Back home the stage is bit-exact again.
        for (i, &x) in sine(2000, 440.0, 0.5).iter().enumerate() {
            let y = l.process(x);
            assert!((y - x).abs() <= 1.0 / 32_768.0, "sample {i}: {y} vs {x}");
        }
    }
}
