//! Clap: a few short band-passed noise bursts a few milliseconds apart, then a longer tail.

use super::envelope::Decay;
use super::filter::Svf3;
use super::noise::Noise;
use super::voice::Voice;

/// Number of bursts (the last one also starts the tail).
pub const BURSTS: u32 = 4;
/// Spacing between bursts, seconds.
pub const BURST_SPACING_SECONDS: f32 = 0.010;
/// Decay (T60) of each burst, seconds.
pub const BURST_SECONDS: f32 = 0.012;
/// Tail decay (T60), seconds.
pub const TAIL_SECONDS: f32 = 0.15;
/// Band-pass centre, Hz (at tune 0). With `Q = 1.2` the -3 dB band is about 0.9-2 kHz.
pub const CENTRE_HZ: f32 = 1_300.0;
/// Band-pass Q.
const Q: f32 = 1.2;
/// Attack of each burst / the tail, seconds.
const ATTACK_SECONDS: f32 = 0.000_3;
/// Tail level relative to a burst.
const TAIL_MIX: f32 = 0.7;
/// Output gain (calibrated for about -7 dBFS peak at velocity 1).
const LEVEL: f32 = 1.19;
/// Seed of the clap noise.
const SEED: u32 = 0x0F1E_2D3C;

/// The clap voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clap {
    /// Samples since the trigger (saturating), or `u32::MAX` when idle.
    elapsed: u32,
    spacing: u32,
    burst: Decay,
    tail: Decay,
    band: Svf3,
    noise: Noise,
    velocity: f32,
}

impl Clap {
    /// Creates an idle clap for `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            elapsed: u32::MAX,
            spacing: (sr * BURST_SPACING_SECONDS).round().max(1.0) as u32,
            burst: Decay::new(sr, ATTACK_SECONDS, BURST_SECONDS),
            tail: Decay::new(sr, ATTACK_SECONDS, TAIL_SECONDS),
            band: Svf3::new(sr, CENTRE_HZ, Q),
            noise: Noise::new(SEED),
            velocity: 0.0,
        }
    }

    /// Sample index at which the last burst (and the tail) starts.
    fn tail_start(&self) -> u32 {
        self.spacing * (BURSTS - 1)
    }
}

impl Voice for Clap {
    fn trigger(&mut self, velocity: f32, tune: f32) {
        self.velocity = velocity;
        self.band.set(CENTRE_HZ * tune, Q);
        self.elapsed = 0;
        self.burst.trigger();
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        if self.elapsed == u32::MAX {
            return 0.0;
        }
        if self.elapsed > 0
            && self.elapsed <= self.tail_start()
            && self.elapsed.is_multiple_of(self.spacing)
        {
            self.burst.trigger();
            if self.elapsed == self.tail_start() {
                self.tail.trigger();
            }
        }
        self.elapsed = self.elapsed.saturating_add(1).min(u32::MAX - 1);
        let env = self.burst.tick() + TAIL_MIX * self.tail.tick();
        let raw = self.noise.next_f32() * env;
        self.band.tick(raw).bp * LEVEL * self.velocity
    }

    fn is_active(&self) -> bool {
        self.elapsed != u32::MAX
            && (self.elapsed <= self.tail_start()
                || self.burst.is_active()
                || self.tail.is_active())
    }

    fn reset(&mut self) {
        self.elapsed = u32::MAX;
        self.burst.reset();
        self.tail.reset();
        self.band.reset();
    }
}
