//! Multi-mode TPT state-variable filter (lowpass, bandpass, highpass) for the drum voices.
//!
//! The synth's [`crate::synth::Svf`] exposes only its lowpass; the drums need band- and
//! highpass with a free `Q`, so this is the same Zavalishin / Simper topology with all three
//! outputs. Stable for any cutoff below Nyquist and any `Q >= 0.1`.

use crate::dsp::flush_denormal;

/// Lowest accepted cutoff, Hz.
pub const MIN_CUTOFF_HZ: f32 = 20.0;
/// Fraction of the sample rate the cutoff may never exceed.
pub const MAX_CUTOFF_FRACTION: f32 = 0.45;
/// Lowest accepted Q.
pub const MIN_Q: f32 = 0.1;
/// Highest accepted Q.
pub const MAX_Q: f32 = 20.0;

/// The three simultaneous outputs of one [`Svf3::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Outputs {
    /// Lowpass.
    pub lp: f32,
    /// Bandpass (peak gain `Q` at the centre frequency).
    pub bp: f32,
    /// Highpass.
    pub hp: f32,
}

/// Three-output TPT state-variable filter.
///
/// Real-time safe: `Copy`, no allocation, no panics; integrator states are denormal-flushed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Svf3 {
    sample_rate: f32,
    cutoff: f32,
    q: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf3 {
    /// Creates a filter at `cutoff_hz` with quality factor `q` for `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut f = Self {
            sample_rate,
            cutoff: 0.0,
            q: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1eq: 0.0,
            ic2eq: 0.0,
        };
        f.set(cutoff_hz, q);
        f
    }

    /// Current cutoff in Hz (after clamping).
    #[must_use]
    pub fn cutoff(&self) -> f32 {
        self.cutoff
    }

    /// Current Q (after clamping).
    #[must_use]
    pub fn q(&self) -> f32 {
        self.q
    }

    /// Clears the filter state (not the coefficients).
    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    /// Sets cutoff (Hz, clamped to `20..=0.45 * sample_rate`) and Q (clamped to `0.1..=20`).
    /// Non-finite values are replaced by the lower clamp limit.
    pub fn set(&mut self, cutoff_hz: f32, q: f32) {
        let max_cutoff = self.sample_rate * MAX_CUTOFF_FRACTION;
        let cutoff = if cutoff_hz.is_finite() {
            cutoff_hz.clamp(MIN_CUTOFF_HZ, max_cutoff)
        } else {
            MIN_CUTOFF_HZ
        };
        let q = if q.is_finite() {
            q.clamp(MIN_Q, MAX_Q)
        } else {
            MIN_Q
        };
        self.cutoff = cutoff;
        self.q = q;
        let g = (core::f32::consts::PI * cutoff / self.sample_rate).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Filters one sample.
    #[inline]
    pub fn tick(&mut self, x: f32) -> Outputs {
        let v3 = x - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = flush_denormal(2.0 * v1 - self.ic1eq);
        self.ic2eq = flush_denormal(2.0 * v2 - self.ic2eq);
        Outputs {
            lp: v2,
            bp: v1,
            hp: x - self.k * v1 - v2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn rms_of(hz: f32, cutoff: f32, q: f32, pick: fn(Outputs) -> f32) -> f32 {
        let mut f = Svf3::new(SR, cutoff, q);
        let mut acc = 0.0_f32;
        let n = 48_000;
        for i in 0..n {
            let x = (core::f32::consts::TAU * hz * i as f32 / SR).sin();
            let y = pick(f.tick(x));
            if i >= n / 2 {
                acc += y * y;
            }
        }
        (acc / (n / 2) as f32).sqrt()
    }

    #[test]
    fn outputs_select_their_bands() {
        let unity = 1.0 / 2.0_f32.sqrt();
        // Lowpass: passes 100 Hz, kills 10 kHz.
        assert!((rms_of(100.0, 1_000.0, 0.707, |o| o.lp) - unity).abs() < 0.02);
        assert!(rms_of(10_000.0, 1_000.0, 0.707, |o| o.lp) < unity * 0.02);
        // Highpass: the mirror image.
        assert!((rms_of(10_000.0, 1_000.0, 0.707, |o| o.hp) - unity).abs() < 0.02);
        assert!(rms_of(100.0, 1_000.0, 0.707, |o| o.hp) < unity * 0.02);
        // Bandpass with Q = 1: unity at the centre, well down two octaves either side.
        assert!((rms_of(2_000.0, 2_000.0, 1.0, |o| o.bp) - unity).abs() < 0.02);
        assert!(rms_of(500.0, 2_000.0, 1.0, |o| o.bp) < unity * 0.3);
        assert!(rms_of(8_000.0, 2_000.0, 1.0, |o| o.bp) < unity * 0.3);
    }

    #[test]
    fn clamps_and_decays_to_silence() {
        let mut f = Svf3::new(SR, 1.0e9, f32::NAN);
        assert_eq!(f.cutoff(), SR * MAX_CUTOFF_FRACTION);
        assert_eq!(f.q(), MIN_Q);
        f.set(1.0, 100.0);
        assert_eq!(f.cutoff(), MIN_CUTOFF_HZ);
        assert_eq!(f.q(), MAX_Q);
        f.set(300.0, 5.0);
        f.tick(1.0);
        for _ in 0..SR as usize {
            f.tick(0.0);
        }
        assert_eq!(f.tick(0.0), Outputs::default());
    }
}
