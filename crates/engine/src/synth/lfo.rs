//! Low-frequency sine oscillator shared by all voices.

use core::f64::consts::TAU;

/// What the LFO modulates. The index order matches `ParamId::LfoTarget`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LfoTarget {
    /// Pitch: +-1 semitone at depth 1.
    Pitch,
    /// Filter cutoff: +-2 octaves at depth 1.
    #[default]
    Cutoff,
}

impl LfoTarget {
    /// Every target in parameter-index order.
    pub const ALL: [LfoTarget; 2] = [LfoTarget::Pitch, LfoTarget::Cutoff];

    /// Target for a parameter index; out-of-range indices clamp to the last target.
    #[must_use]
    pub const fn from_index(index: usize) -> LfoTarget {
        match index {
            0 => LfoTarget::Pitch,
            _ => LfoTarget::Cutoff,
        }
    }

    /// Target for a parameter value (rounded, clamped). Non-finite values give the default.
    #[must_use]
    pub fn from_param(value: f32) -> LfoTarget {
        if !value.is_finite() {
            return LfoTarget::default();
        }
        LfoTarget::from_index(value.round().clamp(0.0, 1.0) as usize)
    }

    /// Parameter index of this target.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Short display name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            LfoTarget::Pitch => "Pitch",
            LfoTarget::Cutoff => "Cutoff",
        }
    }
}

/// Sine LFO. `tick` returns `-1.0..=1.0`; the phase is `f64` turns and never drifts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lfo {
    phase: f64,
    inc: f64,
    inv_sample_rate: f64,
    value: f32,
}

impl Lfo {
    /// Creates an LFO at `rate_hz` for `sample_rate`, starting at phase 0 (value 0, rising).
    #[must_use]
    pub fn new(sample_rate: f32, rate_hz: f32) -> Self {
        let inv_sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            1.0 / f64::from(sample_rate)
        } else {
            1.0 / 48_000.0
        };
        let mut lfo = Self {
            phase: 0.0,
            inc: 0.0,
            inv_sample_rate,
            value: 0.0,
        };
        lfo.set_rate(rate_hz);
        lfo
    }

    /// Sets the rate in Hz without disturbing the phase. Negative or non-finite rates stop
    /// the LFO; the rate is capped below Nyquist.
    pub fn set_rate(&mut self, rate_hz: f32) {
        self.inc = if rate_hz.is_finite() {
            (f64::from(rate_hz) * self.inv_sample_rate).clamp(0.0, 0.5)
        } else {
            0.0
        };
    }

    /// Current rate in Hz.
    #[must_use]
    pub fn rate(&self) -> f32 {
        (self.inc / self.inv_sample_rate) as f32
    }

    /// Current phase in turns, `0.0..1.0`.
    #[must_use]
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// The value returned by the most recent [`tick`](Self::tick).
    #[must_use]
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Restarts at phase 0.
    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.value = 0.0;
    }

    /// Produces the next value in `-1.0..=1.0` and advances the phase.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.value = (self.phase * TAU).sin() as f32;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_indices_round_trip() {
        for (i, t) in LfoTarget::ALL.iter().enumerate() {
            assert_eq!(t.index(), i);
            assert_eq!(LfoTarget::from_index(i), *t);
            assert_eq!(LfoTarget::from_param(i as f32 - 0.2), *t);
        }
        assert_eq!(LfoTarget::from_param(f32::NAN), LfoTarget::default());
        assert_eq!(LfoTarget::from_index(7), LfoTarget::Cutoff);
    }

    #[test]
    fn one_hertz_completes_a_cycle_in_one_second() {
        let mut lfo = Lfo::new(48_000.0, 1.0);
        let mut max = f32::MIN;
        let mut min = f32::MAX;
        for _ in 0..48_000 {
            let v = lfo.tick();
            assert!((-1.0..=1.0).contains(&v));
            max = max.max(v);
            min = min.min(v);
        }
        assert!(max > 0.999 && min < -0.999);
        assert!(lfo.phase() < 1.0e-6 || lfo.phase() > 1.0 - 1.0e-6);
        assert_eq!(lfo.rate(), 1.0);
    }

    #[test]
    fn weird_rates_do_not_panic() {
        let mut lfo = Lfo::new(48_000.0, f32::NAN);
        assert_eq!(lfo.tick(), 0.0);
        lfo.set_rate(-3.0);
        assert_eq!(lfo.rate(), 0.0);
        lfo.set_rate(1.0e9);
        assert!(lfo.tick().is_finite());
        lfo.reset();
        assert_eq!(lfo.value(), 0.0);
    }
}
