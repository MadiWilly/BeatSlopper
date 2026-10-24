//! Synthesizer building blocks.
//!
//! M2 adds band-limited oscillators (PolyBLEP saw/square/triangle), the SVF filter and the
//! LFO as sibling modules (the amp ADSR lives in [`crate::params::Adsr`]). M0/M1 provide the
//! plain [`SineOsc`] used by the engine's test tone and the M1 voices, plus [`note_to_hz`].

use core::f64::consts::TAU;

/// MIDI note number to frequency in Hz with A4 (note 69) = 440 Hz.
#[inline]
#[must_use]
pub fn note_to_hz(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

/// Naive sine oscillator with a drift-free phase accumulator.
///
/// Phase is kept in turns (`0.0..1.0`) as `f64`, so wrapping is exact and accumulated
/// rounding error over hours of playback stays far below one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SineOsc {
    phase: f64,
    inc: f64,
}

impl SineOsc {
    /// Creates an oscillator at `freq_hz` for the given sample rate, starting at phase 0.
    #[must_use]
    pub fn new(freq_hz: f32, sample_rate: f32) -> Self {
        let mut osc = Self {
            phase: 0.0,
            inc: 0.0,
        };
        osc.set_frequency(freq_hz, sample_rate);
        osc
    }

    /// Changes the frequency without resetting the phase (click-free).
    pub fn set_frequency(&mut self, freq_hz: f32, sample_rate: f32) {
        self.inc = if sample_rate > 0.0 && freq_hz.is_finite() {
            (f64::from(freq_hz) / f64::from(sample_rate)).clamp(0.0, 0.999_999)
        } else {
            0.0
        };
    }

    /// Resets the phase to zero.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Current phase in turns, `0.0..1.0`.
    #[must_use]
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Produces the next sample in `-1.0..=1.0` and advances the phase.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let s = (self.phase * TAU).sin() as f32;
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
    fn a4_is_440() {
        assert!((note_to_hz(69.0) - 440.0).abs() < 1e-3);
        assert!((note_to_hz(81.0) - 880.0).abs() < 1e-2);
    }

    #[test]
    fn phase_stays_in_range() {
        let mut osc = SineOsc::new(12_345.0, 48_000.0);
        for _ in 0..100_000 {
            osc.tick();
            assert!((0.0..1.0).contains(&osc.phase()));
        }
    }
}
