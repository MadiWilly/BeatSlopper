//! FM: a sine modulator that phase-modulates oscillator 1.
//!
//! Classic "FM" synths (DX7 and friends) are really phase modulation: the modulator's sine,
//! scaled by the modulation index (radians), is added to the carrier's phase. The index comes
//! from `FmAmount` (`0.0..=1.0` -> `0.0..=MAX_FM_INDEX` radians) and the modulator runs at
//! `FmRatio` times the carrier frequency, so integer ratios give harmonic (electric piano,
//! organ) tones and non-integer ratios give bells and mallets.

use core::f64::consts::TAU;

/// Modulation index, in radians, at `FmAmount = 1.0`.
pub const MAX_FM_INDEX: f32 = 8.0;

/// Maps the `FmAmount` parameter (`0.0..=1.0`) to a modulation index in radians.
#[inline]
#[must_use]
pub fn fm_index(amount: f32) -> f32 {
    if amount.is_finite() {
        amount.clamp(0.0, 1.0) * MAX_FM_INDEX
    } else {
        0.0
    }
}

/// Sine modulator. [`tick`](Self::tick) returns the carrier phase offset in **turns**
/// (`index * sin(phase) / 2pi`), ready for [`Oscillator::tick_pm`](super::Oscillator::tick_pm).
///
/// Real-time safe: `Copy`, no allocation, no panics; the phase is `f64` turns and never drifts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FmModulator {
    phase: f64,
    inc: f64,
    inv_sample_rate: f64,
    /// Modulation index converted to turns (`radians / 2pi`).
    index_turns: f32,
}

impl FmModulator {
    /// Creates a modulator at `freq_hz` for `sample_rate` with index 0, phase 0.
    #[must_use]
    pub fn new(sample_rate: f32, freq_hz: f32) -> Self {
        let inv_sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            1.0 / f64::from(sample_rate)
        } else {
            1.0 / 48_000.0
        };
        let mut m = Self {
            phase: 0.0,
            inc: 0.0,
            inv_sample_rate,
            index_turns: 0.0,
        };
        m.set_freq(freq_hz);
        m
    }

    /// Sets the modulator frequency in Hz without disturbing the phase. Negative or
    /// non-finite values stop it; the rate is capped below Nyquist.
    #[inline]
    pub fn set_freq(&mut self, freq_hz: f32) {
        self.inc = if freq_hz.is_finite() {
            (f64::from(freq_hz) * self.inv_sample_rate).clamp(0.0, 0.5)
        } else {
            0.0
        };
    }

    /// Current frequency in Hz.
    #[must_use]
    pub fn freq(&self) -> f32 {
        (self.inc / self.inv_sample_rate) as f32
    }

    /// Sets the modulation index in radians (`0.0..=MAX_FM_INDEX`; non-finite -> 0).
    pub fn set_index(&mut self, radians: f32) {
        self.index_turns = if radians.is_finite() {
            (radians.clamp(0.0, MAX_FM_INDEX) / TAU as f32).max(0.0)
        } else {
            0.0
        };
    }

    /// Current modulation index in radians.
    #[must_use]
    pub fn index(&self) -> f32 {
        self.index_turns * TAU as f32
    }

    /// `true` when the index is zero, i.e. the modulator would have no effect.
    #[must_use]
    pub fn is_off(&self) -> bool {
        self.index_turns <= 0.0
    }

    /// Restarts at phase 0 (sine starts at its rising zero crossing).
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Produces the next carrier phase offset in turns and advances the phase.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let s = (self.phase * TAU).sin() as f32 * self.index_turns;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_maps_amount_to_radians() {
        assert_eq!(fm_index(0.0), 0.0);
        assert_eq!(fm_index(1.0), MAX_FM_INDEX);
        assert_eq!(fm_index(0.5), MAX_FM_INDEX * 0.5);
        assert_eq!(fm_index(7.0), MAX_FM_INDEX);
        assert_eq!(fm_index(f32::NAN), 0.0);
    }

    #[test]
    fn offset_is_index_over_two_pi_peak() {
        let mut m = FmModulator::new(48_000.0, 100.0);
        m.set_index(MAX_FM_INDEX);
        assert!((m.index() - MAX_FM_INDEX).abs() < 1e-5);
        let peak = (0..48_000).map(|_| m.tick().abs()).fold(0.0_f32, f32::max);
        let expected = MAX_FM_INDEX / core::f32::consts::TAU;
        assert!((peak - expected).abs() < 1e-3, "peak {peak}");
        m.set_index(0.0);
        assert!(m.is_off());
        assert_eq!(m.tick(), 0.0);
        m.set_freq(f32::NAN);
        assert_eq!(m.freq(), 0.0);
        m.set_index(f32::INFINITY);
        assert!(m.is_off());
    }
}
