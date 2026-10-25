//! Percussive envelope: an optional short linear attack, then an exponential decay that snaps
//! to exactly zero at a floor so voices go idle and never produce denormals.

use crate::dsp::flush_denormal;

/// Level below which a decaying envelope snaps to `0.0` and reports idle (-100 dBFS).
pub const FLOOR: f32 = 1.0e-5;

/// `ln(1000)`: the number of time constants in a 60 dB decay.
const LN_1000: f32 = 6.907_755_3;

/// Exponential percussive envelope.
///
/// The decay time is a *T60*: the time for the level to fall by 60 dB (to `1e-3`), which is
/// the perceived "length" of a drum hit; the level keeps decaying and reaches [`FLOOR`] after
/// about 1.67 T60. Real-time safe: `Copy`, no allocation, no panics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decay {
    level: f32,
    attack_step: f32,
    attacking: bool,
    mul: f32,
}

fn sane_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    }
}

impl Decay {
    /// Creates an idle envelope with `attack_s` seconds of linear attack (`0.0` = instant) and
    /// a 60 dB decay time of `t60_s` seconds.
    #[must_use]
    pub fn new(sample_rate: f32, attack_s: f32, t60_s: f32) -> Self {
        let sr = sane_rate(sample_rate);
        let t60_samples = (t60_s.max(0.0) * sr).max(1.0);
        let attack_samples = attack_s.max(0.0) * sr;
        Self {
            level: 0.0,
            attack_step: if attack_samples >= 1.0 {
                1.0 / attack_samples
            } else {
                1.0
            },
            attacking: false,
            mul: (-LN_1000 / t60_samples).exp(),
        }
    }

    /// Creates an idle envelope decaying with a plain time constant `tau_s` (63 % per tau),
    /// e.g. for a pitch sweep.
    #[must_use]
    pub fn with_time_constant(sample_rate: f32, tau_s: f32) -> Self {
        let sr = sane_rate(sample_rate);
        let tau_samples = (tau_s.max(0.0) * sr).max(1.0);
        Self {
            level: 0.0,
            attack_step: 1.0,
            attacking: false,
            mul: (-1.0 / tau_samples).exp(),
        }
    }

    /// Per-sample decay multiplier.
    #[must_use]
    pub fn multiplier(&self) -> f32 {
        self.mul
    }

    /// Starts the attack **from the current level** (never from zero when retriggered).
    pub fn trigger(&mut self) {
        self.attacking = true;
    }

    /// Jumps to silence.
    pub fn reset(&mut self) {
        self.level = 0.0;
        self.attacking = false;
    }

    /// Current level without advancing.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.level
    }

    /// `true` until the level has decayed to [`FLOOR`].
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.attacking || self.level > 0.0
    }

    /// Returns the level for this sample and advances.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.attacking {
            self.level += self.attack_step;
            // Snap on the last step so rounding never leaves the ramp an ulp short of 1.0.
            if self.level >= 1.0 - 0.5 * self.attack_step {
                self.level = 1.0;
                self.attacking = false;
            }
            return self.level;
        }
        let out = self.level;
        let next = flush_denormal(self.level * self.mul);
        self.level = if next < FLOOR { 0.0 } else { next };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn instant_attack_starts_at_one_and_hits_minus_60_db_at_t60() {
        let mut env = Decay::new(SR, 0.0, 0.1);
        env.trigger();
        assert_eq!(env.tick(), 1.0);
        let mut last = 1.0;
        for _ in 1..4_800 {
            last = env.tick();
        }
        // Sample 4799 is one step short of 100 ms: 1e-3 within rounding.
        assert!((last - 1.0e-3).abs() < 2.0e-5, "level at T60 {last}");
        assert!(env.is_active());
    }

    #[test]
    fn snaps_to_zero_at_the_floor_and_goes_idle() {
        let mut env = Decay::new(SR, 0.0, 0.05);
        env.trigger();
        let mut n = 0;
        while env.is_active() {
            let v = env.tick();
            assert!(v == 0.0 || v >= FLOOR, "{v}");
            n += 1;
            assert!(n < 48_000, "never went idle");
        }
        assert_eq!(env.level(), 0.0);
        assert_eq!(env.tick(), 0.0);
        // 50 ms T60 reaches -100 dB after 5/3 of the time: about 83 ms.
        assert!((3_800..4_200).contains(&n), "{n} samples");
    }

    #[test]
    fn linear_attack_ramps_from_current_level() {
        let mut env = Decay::new(SR, 0.001, 1.0);
        env.trigger();
        let first = env.tick();
        assert!((first - 1.0 / 48.0).abs() < 1.0e-6, "{first}");
        for _ in 0..47 {
            env.tick();
        }
        assert_eq!(env.level(), 1.0);
        // Decay a bit, retrigger: the attack continues upward from the current level.
        for _ in 0..24_000 {
            env.tick();
        }
        let mid = env.level();
        assert!(mid > 0.0 && mid < 0.1, "{mid}");
        env.trigger();
        let step = env.tick();
        assert!(step > mid && step < mid + 0.03, "{step}");
    }

    #[test]
    fn time_constant_variant_decays_63_percent_per_tau() {
        let mut env = Decay::with_time_constant(SR, 0.05);
        env.trigger();
        env.tick();
        for _ in 0..2_400 {
            env.tick();
        }
        let e = (-1.0_f32).exp();
        assert!((env.level() - e).abs() < 1.0e-3, "{}", env.level());
    }
}
