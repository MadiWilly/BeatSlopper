//! Kick: a sine with an exponential pitch sweep, soft saturation and a short noise click.

use super::envelope::Decay;
use super::noise::Noise;
use super::voice::Voice;

/// Sweep start frequency, Hz (at tune 0).
pub const START_HZ: f32 = 150.0;
/// Sweep end (body) frequency, Hz (at tune 0).
pub const END_HZ: f32 = 50.0;
/// Time constant of the exponential pitch sweep, seconds.
pub const SWEEP_TAU_SECONDS: f32 = 0.05;
/// Amplitude decay (T60), seconds.
pub const DECAY_SECONDS: f32 = 0.35;
/// Click (noise burst) decay (T60), seconds.
pub const CLICK_SECONDS: f32 = 0.002;
/// Click attack, seconds: just enough to avoid a hard first sample.
const CLICK_ATTACK_SECONDS: f32 = 0.000_2;
/// Drive into the soft clipper.
const DRIVE: f32 = 1.4;
/// Body gain after the clipper (calibrated for about -7 dBFS peak at velocity 1, so that
/// several drums hitting together stay well under full scale before the master limiter).
const BODY_LEVEL: f32 = 0.513;
/// Click gain.
const CLICK_LEVEL: f32 = 0.25;
/// Seed of the click noise.
const SEED: u32 = 0x1234_5679;

/// The kick voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kick {
    sample_rate: f32,
    phase: f32,
    sweep: Decay,
    amp: Decay,
    click: Decay,
    noise: Noise,
    velocity: f32,
    tune: f32,
}

impl Kick {
    /// Creates an idle kick for `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            phase: 0.0,
            sweep: Decay::with_time_constant(sample_rate, SWEEP_TAU_SECONDS),
            amp: Decay::new(sample_rate, 0.0, DECAY_SECONDS),
            click: Decay::new(sample_rate, CLICK_ATTACK_SECONDS, CLICK_SECONDS),
            noise: Noise::new(SEED),
            velocity: 0.0,
            tune: 1.0,
        }
    }
}

/// Cheap, monotonic soft clipper: `tanh`-like, exact for small inputs.
#[inline]
fn soft_clip(x: f32) -> f32 {
    x.tanh()
}

impl Voice for Kick {
    fn trigger(&mut self, velocity: f32, tune: f32) {
        self.velocity = velocity;
        self.tune = tune;
        self.phase = 0.0;
        self.sweep.reset();
        self.sweep.trigger();
        self.amp.reset();
        self.amp.trigger();
        self.click.trigger();
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        let sweep = self.sweep.tick();
        let hz = (END_HZ + (START_HZ - END_HZ) * sweep) * self.tune;
        self.phase += hz / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let body = (core::f32::consts::TAU * self.phase).sin() * self.amp.tick();
        let body = soft_clip(body * DRIVE) * BODY_LEVEL;
        let click = self.noise.next_f32() * self.click.tick() * CLICK_LEVEL;
        (body + click) * self.velocity
    }

    fn is_active(&self) -> bool {
        self.amp.is_active() || self.click.is_active()
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.sweep.reset();
        self.amp.reset();
        self.click.reset();
    }
}
