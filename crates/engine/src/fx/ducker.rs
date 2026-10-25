//! Sidechain ducker: the EDM "pump". Every [`trigger`](Ducker::trigger) (the engine calls it
//! when the kick fires) dips the gain to `1 - amount` within [`ATTACK_MS`] (linearly from
//! wherever the gain currently is) and then lets it recover exponentially towards `1.0` with
//! a time constant of `release / 3`, so the gain is back within 5 % (`e^-3`) of unity at
//! `release` seconds. A retrigger restarts the attack from the current gain.
//!
//! The state kept is the *dip* `1 - gain`, which decays towards zero: an `f32` one-pole
//! heading for `1.0` would stall a few ulps short of it, whereas a value decaying towards
//! zero keeps its full relative precision until it is flushed below [`SETTLE_EPSILON`].
//!
//! The gain is applied by the engine to the synth channel after the chorus (see
//! [`FxChain::process_synth_insert`](super::FxChain::process_synth_insert)).
//!
//! Real-time safe: `Copy`, no allocation, no panics. [`tick`](Ducker::tick) is one multiply
//! per sample.

/// Time the gain takes to fall to the floor after a trigger, milliseconds.
pub const ATTACK_MS: f32 = 2.0;

/// Shortest release accepted by [`Ducker::set`], seconds.
pub const MIN_RELEASE_SECONDS: f32 = 0.001;

/// Dip below which the gain snaps to exactly `1.0` and the dip ends (-0.001 dB).
pub const SETTLE_EPSILON: f32 = 1.0e-4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Release,
}

/// Kick-triggered gain dip. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ducker {
    sample_rate: f32,
    amount: f32,
    release: f32,
    /// `1 - gain`: `0.0` at rest, `amount` at the bottom of a dip.
    dip: f32,
    stage: Stage,
    /// Dip change per sample during the attack (positive).
    attack_step: f32,
    attack_samples: f32,
    /// Per-sample multiplier of the dip during the release, `exp(-3 / (release * sr))`.
    release_decay: f32,
}

impl Ducker {
    /// Creates a ducker at `sample_rate` (non-finite or non-positive rates fall back to
    /// 48 kHz) with amount `0.0` and release `0.25` s, resting at unity gain.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut d = Self {
            sample_rate,
            amount: 0.0,
            release: 0.25,
            dip: 0.0,
            stage: Stage::Idle,
            attack_step: 0.0,
            attack_samples: (sample_rate * ATTACK_MS * 0.001).round().max(1.0),
            release_decay: 0.0,
        };
        d.set(0.0, 0.25);
        d
    }

    /// Sets the dip depth (`0.0..=1.0`, clamped) and the release time in seconds (clamped to
    /// at least [`MIN_RELEASE_SECONDS`]). Non-finite values leave the respective setting
    /// unchanged. Takes effect immediately, including on a running dip.
    pub fn set(&mut self, amount: f32, release: f32) {
        if amount.is_finite() {
            self.amount = amount.clamp(0.0, 1.0);
        }
        if release.is_finite() {
            self.release = release.max(MIN_RELEASE_SECONDS);
            let tau_samples = self.sample_rate * self.release / 3.0;
            self.release_decay = (-1.0 / tau_samples).exp().clamp(0.0, 1.0);
        }
    }

    /// Current dip depth.
    #[must_use]
    pub fn amount(&self) -> f32 {
        self.amount
    }

    /// Current release time, seconds.
    #[must_use]
    pub fn release(&self) -> f32 {
        self.release
    }

    /// Sample rate the ducker was built for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Gain as of the last [`tick`](Self::tick) (`1.0` at rest).
    #[must_use]
    pub fn gain(&self) -> f32 {
        1.0 - self.dip
    }

    /// `true` while a dip is in progress (attack or not yet recovered).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Starts (or restarts) a dip: the gain heads for `1 - amount` over [`ATTACK_MS`] from
    /// its current value, then releases.
    pub fn trigger(&mut self) {
        if self.dip >= self.amount {
            self.dip = self.amount;
            self.stage = Stage::Release;
        } else {
            self.attack_step = (self.amount - self.dip) / self.attack_samples;
            self.stage = Stage::Attack;
        }
    }

    /// Returns the gain to unity instantly and cancels any dip.
    pub fn reset(&mut self) {
        self.dip = 0.0;
        self.stage = Stage::Idle;
    }

    /// Advances one sample and returns the gain to apply.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.dip += self.attack_step;
                if self.dip >= self.amount {
                    self.dip = self.amount;
                    self.stage = Stage::Release;
                }
            }
            Stage::Release => {
                self.dip *= self.release_decay;
                if self.dip <= SETTLE_EPSILON {
                    self.dip = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        1.0 - self.dip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn rests_at_unity() {
        let mut d = Ducker::new(SR);
        assert!(!d.is_active());
        for _ in 0..100 {
            assert_eq!(d.tick(), 1.0);
        }
        assert_eq!(Ducker::new(0.0).sample_rate(), 48_000.0);
    }

    #[test]
    fn set_clamps() {
        let mut d = Ducker::new(SR);
        d.set(3.0, 0.0);
        assert_eq!(d.amount(), 1.0);
        assert_eq!(d.release(), MIN_RELEASE_SECONDS);
        d.set(f32::NAN, f32::INFINITY);
        assert_eq!(d.amount(), 1.0);
        assert_eq!(d.release(), MIN_RELEASE_SECONDS);
    }

    #[test]
    fn dips_and_recovers_on_schedule() {
        for release in [0.05, 0.25, 1.0] {
            let mut d = Ducker::new(SR);
            d.set(0.8, release);
            d.trigger();
            assert!(d.is_active());
            let attack = (SR * 0.002) as usize;
            let mut g = 1.0;
            for _ in 0..attack {
                g = d.tick();
            }
            assert!((g - 0.2).abs() < 1e-5, "floor {g}");
            // Within 5 % of unity at `release`.
            let release_samples = (SR * release) as usize;
            for _ in 0..release_samples {
                g = d.tick();
            }
            assert!(g > 0.95 && g < 0.97, "release {release}: at release {g}");
            for _ in 0..release_samples * 3 {
                d.tick();
            }
            assert!(!d.is_active(), "release {release}");
            assert_eq!(d.gain(), 1.0);
        }
    }

    #[test]
    fn retrigger_restarts_the_attack_and_zero_amount_is_a_no_op() {
        let mut d = Ducker::new(SR);
        d.set(0.5, 0.1);
        d.trigger();
        for _ in 0..(SR * 0.05) as usize {
            d.tick();
        }
        let mid = d.gain();
        assert!(mid > 0.5 && mid < 1.0);
        d.trigger();
        let g = d.tick();
        assert!(g < mid, "attack resumes downward from the current gain");
        // One attack tick is already done; the remaining 95 land exactly on the floor.
        for _ in 1..(SR * 0.002) as usize {
            d.tick();
        }
        assert!((d.gain() - 0.5).abs() < 1e-5, "{}", d.gain());
        d.reset();
        assert_eq!(d.gain(), 1.0);
        d.set(0.0, 0.1);
        d.trigger();
        assert_eq!(d.tick(), 1.0);
        assert!(!d.is_active());
    }
}
