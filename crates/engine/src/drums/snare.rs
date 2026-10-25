//! Snare: two decaying sines (shell) plus band-passed noise (snares).

use super::envelope::Decay;
use super::filter::Svf3;
use super::noise::Noise;
use super::voice::Voice;

/// Fundamental of the shell tone, Hz (at tune 0).
pub const TONE_HZ: f32 = 180.0;
/// Second shell partial, Hz (at tune 0).
pub const TONE2_HZ: f32 = 330.0;
/// Tone decay (T60), seconds.
pub const TONE_SECONDS: f32 = 0.12;
/// Noise decay (T60), seconds.
pub const NOISE_SECONDS: f32 = 0.18;
/// Band-pass centre of the noise, Hz (at tune 0). With `Q = 1` the -3 dB band is about
/// 1.5-4 kHz.
pub const NOISE_CENTRE_HZ: f32 = 2_500.0;
/// Band-pass Q.
const NOISE_Q: f32 = 1.0;
/// Noise attack, seconds.
const NOISE_ATTACK_SECONDS: f32 = 0.000_3;
/// Mix of the second partial relative to the fundamental.
const TONE2_MIX: f32 = 0.6;
/// Tone output gain (calibrated with [`NOISE_LEVEL`] for about -7 dBFS peak).
const TONE_LEVEL: f32 = 0.216;
/// Noise output gain.
const NOISE_LEVEL: f32 = 0.30;
/// Seed of the snare noise.
const SEED: u32 = 0x2468_ACE1;

/// The snare voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snare {
    sample_rate: f32,
    phase1: f32,
    phase2: f32,
    tone_env: Decay,
    noise_env: Decay,
    band: Svf3,
    noise: Noise,
    velocity: f32,
    tune: f32,
}

impl Snare {
    /// Creates an idle snare for `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            phase1: 0.0,
            phase2: 0.0,
            tone_env: Decay::new(sample_rate, 0.0, TONE_SECONDS),
            noise_env: Decay::new(sample_rate, NOISE_ATTACK_SECONDS, NOISE_SECONDS),
            band: Svf3::new(sample_rate, NOISE_CENTRE_HZ, NOISE_Q),
            noise: Noise::new(SEED),
            velocity: 0.0,
            tune: 1.0,
        }
    }
}

#[inline]
fn advance(phase: &mut f32, hz: f32, sample_rate: f32) -> f32 {
    *phase += hz / sample_rate;
    if *phase >= 1.0 {
        *phase -= 1.0;
    }
    (core::f32::consts::TAU * *phase).sin()
}

impl Voice for Snare {
    fn trigger(&mut self, velocity: f32, tune: f32) {
        self.velocity = velocity;
        self.tune = tune;
        self.phase1 = 0.0;
        self.phase2 = 0.0;
        self.band.set(NOISE_CENTRE_HZ * tune, NOISE_Q);
        self.tone_env.reset();
        self.tone_env.trigger();
        self.noise_env.trigger();
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        let t1 = advance(&mut self.phase1, TONE_HZ * self.tune, self.sample_rate);
        let t2 = advance(&mut self.phase2, TONE2_HZ * self.tune, self.sample_rate);
        let tone = (t1 + TONE2_MIX * t2) * self.tone_env.tick() * TONE_LEVEL;
        let raw = self.noise.next_f32() * self.noise_env.tick();
        let noise = self.band.tick(raw).bp * NOISE_LEVEL;
        (tone + noise) * self.velocity
    }

    fn is_active(&self) -> bool {
        self.tone_env.is_active() || self.noise_env.is_active()
    }

    fn reset(&mut self) {
        self.phase1 = 0.0;
        self.phase2 = 0.0;
        self.tone_env.reset();
        self.noise_env.reset();
        self.band.reset();
    }
}
