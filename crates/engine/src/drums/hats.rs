//! Hi-hats: six detuned square oscillators at inharmonic ratios plus a little white noise,
//! high-passed with a resonant peak. Closed and open hats share the source and differ in
//! decay; the open hat can be choked (fast fade) by the closed hat.

use super::envelope::Decay;
use super::filter::Svf3;
use super::noise::Noise;
use super::voice::Voice;

/// Oscillator frequencies, Hz (at tune 0): the classic inharmonic 808 hat cluster.
pub const OSC_HZ: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
/// High-pass cutoff, Hz (at tune 0).
pub const HIGHPASS_HZ: f32 = 6_000.0;
/// Centre of the resonant "sizzle" peak, Hz (at tune 0).
pub const PEAK_HZ: f32 = 9_500.0;
/// Closed-hat decay (T60), seconds.
pub const CLOSED_SECONDS: f32 = 0.06;
/// Open-hat decay (T60), seconds.
pub const OPEN_SECONDS: f32 = 0.40;
/// Length of the choke fade, seconds.
pub const CHOKE_SECONDS: f32 = 0.005;
/// Attack of the hat envelope, seconds.
const ATTACK_SECONDS: f32 = 0.000_25;
/// High-pass Q (slight bump at the cutoff).
const HIGHPASS_Q: f32 = 1.2;
/// Peak filter Q.
const PEAK_Q: f32 = 2.5;
/// White-noise mix added to the metallic source.
const NOISE_MIX: f32 = 0.35;
/// Peak output relative to the high-pass output.
const PEAK_MIX: f32 = 0.5;
/// Closed-hat output gain (calibrated for about -7 dBFS peak at velocity 1; the short
/// envelope decays before the filters fully build up, so it needs more gain than the open hat).
const CLOSED_LEVEL: f32 = 0.76;
/// Open-hat output gain.
const OPEN_LEVEL: f32 = 0.506;

/// A hi-hat voice (closed or open depending on the decay given to [`Hat::new`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hat {
    sample_rate: f32,
    phases: [f32; 6],
    env: Decay,
    highpass: Svf3,
    peak: Svf3,
    noise: Noise,
    level: f32,
    velocity: f32,
    tune: f32,
    choke_gain: f32,
    choke_step: f32,
    choking: bool,
}

impl Hat {
    /// Creates an idle hat with the given T60 `decay_seconds` and output gain `level`.
    #[must_use]
    pub fn new(sample_rate: f32, decay_seconds: f32, level: f32, seed: u32) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            sample_rate: sr,
            // Spread the starting phases so the squares do not all flip together.
            phases: [0.0, 0.17, 0.31, 0.46, 0.63, 0.82],
            env: Decay::new(sr, ATTACK_SECONDS, decay_seconds),
            highpass: Svf3::new(sr, HIGHPASS_HZ, HIGHPASS_Q),
            peak: Svf3::new(sr, PEAK_HZ, PEAK_Q),
            noise: Noise::new(seed),
            level,
            velocity: 0.0,
            tune: 1.0,
            choke_gain: 1.0,
            choke_step: 1.0 / (sr * CHOKE_SECONDS).max(1.0),
            choking: false,
        }
    }

    /// Closed hat ([`CLOSED_SECONDS`]).
    #[must_use]
    pub fn closed(sample_rate: f32) -> Self {
        Self::new(sample_rate, CLOSED_SECONDS, CLOSED_LEVEL, 0x7A3B_9C1D)
    }

    /// Open hat ([`OPEN_SECONDS`]).
    #[must_use]
    pub fn open(sample_rate: f32) -> Self {
        Self::new(sample_rate, OPEN_SECONDS, OPEN_LEVEL, 0x5EED_BEEF)
    }
}

impl Voice for Hat {
    fn trigger(&mut self, velocity: f32, tune: f32) {
        self.velocity = velocity;
        self.tune = tune;
        self.highpass.set(HIGHPASS_HZ * tune, HIGHPASS_Q);
        self.peak.set(PEAK_HZ * tune, PEAK_Q);
        self.choking = false;
        self.choke_gain = 1.0;
        self.env.trigger();
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        let mut metal = 0.0;
        for (phase, hz) in self.phases.iter_mut().zip(OSC_HZ) {
            *phase += hz * self.tune / self.sample_rate;
            if *phase >= 1.0 {
                *phase -= 1.0;
            }
            metal += if *phase < 0.5 { 1.0 } else { -1.0 };
        }
        let source = metal * (1.0 / 6.0) + self.noise.next_f32() * NOISE_MIX;
        let shaped = source * self.env.tick();
        let bright = self.highpass.tick(shaped).hp + self.peak.tick(shaped).bp * PEAK_MIX;
        let mut out = bright * self.level * self.velocity * self.choke_gain;
        if self.choking {
            self.choke_gain -= self.choke_step;
            if self.choke_gain <= 0.0 {
                out = 0.0;
                self.reset();
            }
        }
        out
    }

    fn is_active(&self) -> bool {
        self.env.is_active()
    }

    fn reset(&mut self) {
        self.env.reset();
        self.highpass.reset();
        self.peak.reset();
        self.choking = false;
        self.choke_gain = 1.0;
    }

    fn choke(&mut self) {
        if self.env.is_active() && !self.choking {
            self.choking = true;
        }
    }
}
