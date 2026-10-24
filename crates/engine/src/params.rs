//! Parameter handling: smoothing so that UI changes never produce zipper noise.
//!
//! Every audible parameter in later milestones (gains, cutoff, mix amounts, ...) should go
//! through a [`Smoother`] rather than being applied as a step. Later milestones also add the
//! `ParamId` enum used by `Message::SetParam` here.

use crate::dsp::flush_denormal;

/// Default smoothing time used for gain-like parameters (about 10 ms).
pub const DEFAULT_SMOOTH_MS: f32 = 10.0;

/// Relative distance to the target below which the smoother snaps and reports settled.
///
/// Relative to `max(|target|, 1.0)`, so a gain settles within -80 dB of its target. Needed
/// because an `f32` one-pole stalls when `coeff * diff` drops below half an ulp of `current`.
const SNAP_EPSILON: f32 = 1.0e-4;

/// One-pole (exponential) parameter smoother.
///
/// `current += coeff * (target - current)` per sample, where `coeff` is derived from a time
/// constant: after `time_ms` the value has covered about 63 % of a step, after `5 * time_ms`
/// more than 99 %. Real-time safe: `Copy`, no allocation, no panics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smoother {
    current: f32,
    target: f32,
    coeff: f32,
}

impl Smoother {
    /// Creates a smoother resting at `initial` with the given time constant in milliseconds.
    ///
    /// A non-positive `time_ms` or `sample_rate` yields an instant (unsmoothed) parameter.
    #[must_use]
    pub fn new(sample_rate: f32, time_ms: f32, initial: f32) -> Self {
        Self {
            current: initial,
            target: initial,
            coeff: Self::coefficient(sample_rate, time_ms),
        }
    }

    /// Computes the one-pole coefficient for a time constant of `time_ms` at `sample_rate`.
    #[must_use]
    pub fn coefficient(sample_rate: f32, time_ms: f32) -> f32 {
        if !time_ms.is_finite() || time_ms <= 0.0 || !sample_rate.is_finite() || sample_rate <= 0.0
        {
            return 1.0;
        }
        let tau_samples = sample_rate * time_ms * 0.001;
        (1.0 - (-1.0 / tau_samples).exp()).clamp(0.0, 1.0)
    }

    /// Changes the time constant without disturbing the current value.
    pub fn set_time(&mut self, sample_rate: f32, time_ms: f32) {
        self.coeff = Self::coefficient(sample_rate, time_ms);
    }

    /// Sets the value the smoother glides towards. Non-finite targets are ignored.
    pub fn set_target(&mut self, target: f32) {
        if target.is_finite() {
            self.target = target;
        }
    }

    /// Jumps straight to `value` (both current and target), e.g. on project load.
    pub fn snap(&mut self, value: f32) {
        if value.is_finite() {
            self.current = value;
            self.target = value;
        }
    }

    /// The value the smoother is moving towards.
    #[must_use]
    pub fn target(&self) -> f32 {
        self.target
    }

    /// The most recent smoothed value (without advancing).
    #[must_use]
    pub fn current(&self) -> f32 {
        self.current
    }

    /// `true` when the current value has reached the target.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.current == self.target
    }

    /// Advances one sample and returns the new smoothed value.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let diff = self.target - self.current;
        let eps = SNAP_EPSILON * self.target.abs().max(1.0);
        if diff.abs() <= eps {
            self.current = self.target;
        } else {
            self.current = flush_denormal(self.current + self.coeff * diff);
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_about_63_percent_after_one_time_constant() {
        let sr = 48_000.0;
        let mut s = Smoother::new(sr, 10.0, 0.0);
        s.set_target(1.0);
        let n = (sr * 0.010) as usize;
        let mut v = 0.0;
        for _ in 0..n {
            v = s.tick();
        }
        assert!((v - 0.632).abs() < 0.01, "got {v}");
    }

    #[test]
    fn settles_exactly_on_target() {
        let mut s = Smoother::new(48_000.0, 10.0, 1.0);
        s.set_target(0.0);
        for _ in 0..48_000 {
            s.tick();
        }
        assert!(s.is_settled());
        assert_eq!(s.current(), 0.0);
    }

    #[test]
    fn zero_time_is_instant() {
        let mut s = Smoother::new(48_000.0, 0.0, 0.0);
        s.set_target(3.0);
        assert_eq!(s.tick(), 3.0);
    }

    #[test]
    fn ignores_non_finite_targets() {
        let mut s = Smoother::new(48_000.0, 10.0, 0.5);
        s.set_target(f32::NAN);
        assert_eq!(s.target(), 0.5);
        s.snap(f32::INFINITY);
        assert_eq!(s.current(), 0.5);
    }
}
