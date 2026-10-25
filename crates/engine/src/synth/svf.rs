//! State-variable lowpass filter in the TPT ("topology preserving transform", zero-delay
//! feedback) form after Zavalishin.
//!
//! The TPT form is stable for every cutoff below Nyquist and for any damping, and its cutoff
//! and resonance can be modulated per sample without blowing up. Only the lowpass output is
//! exposed (the synth needs nothing else).

use crate::dsp::flush_denormal;

/// Lowest accepted cutoff, Hz.
pub const MIN_CUTOFF_HZ: f32 = 20.0;
/// Highest accepted cutoff, Hz (also capped at `0.45 * sample_rate`).
pub const MAX_CUTOFF_HZ: f32 = 20_000.0;
/// Fraction of the sample rate the cutoff may never exceed.
pub const MAX_CUTOFF_FRACTION: f32 = 0.45;
/// Q at resonance 0 (critically damped: no peak at all).
pub const MIN_Q: f32 = 0.5;
/// Q at resonance 1. High enough to ring audibly, low enough never to self-oscillate.
pub const MAX_Q: f32 = 10.0;

/// Relative cutoff change below which the coefficients are not recomputed.
const CUTOFF_EPSILON: f32 = 1.0e-4;
/// Absolute resonance change below which the coefficients are not recomputed.
const RESONANCE_EPSILON: f32 = 1.0e-4;

/// Highest cutoff usable at `sample_rate`: `min(20 kHz, 0.45 * sample_rate)`.
#[inline]
#[must_use]
pub fn max_cutoff(sample_rate: f32) -> f32 {
    (sample_rate * MAX_CUTOFF_FRACTION).min(MAX_CUTOFF_HZ)
}

/// Maps resonance `0.0..=1.0` to Q, exponentially from [`MIN_Q`] to [`MAX_Q`].
#[inline]
#[must_use]
pub fn resonance_to_q(resonance: f32) -> f32 {
    MIN_Q * (MAX_Q / MIN_Q).powf(resonance.clamp(0.0, 1.0))
}

/// TPT state-variable lowpass filter.
///
/// Real-time safe: `Copy`, no allocation, no panics; the two integrator states are
/// denormal-flushed every sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Svf {
    sample_rate: f32,
    cutoff: f32,
    resonance: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    /// Creates a filter at `cutoff_hz` / `resonance` for `sample_rate` with cleared state.
    #[must_use]
    pub fn new(sample_rate: f32, cutoff_hz: f32, resonance: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut f = Self {
            sample_rate,
            cutoff: -1.0,
            resonance: -1.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1eq: 0.0,
            ic2eq: 0.0,
        };
        f.set(cutoff_hz, resonance);
        f
    }

    /// Current cutoff in Hz (after clamping).
    #[must_use]
    pub fn cutoff(&self) -> f32 {
        self.cutoff
    }

    /// Current resonance in `0.0..=1.0` (after clamping).
    #[must_use]
    pub fn resonance(&self) -> f32 {
        self.resonance
    }

    /// Clears the filter state (not the coefficients).
    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    /// Sets cutoff (Hz, clamped to `20..=min(20 kHz, 0.45 * sample_rate)`) and resonance
    /// (`0.0..=1.0`, see [`resonance_to_q`]). Non-finite values keep the previous setting.
    /// Coefficients are only recomputed when a value actually changed, so calling this every
    /// few samples with an unchanged target costs almost nothing.
    #[inline]
    pub fn set(&mut self, cutoff_hz: f32, resonance: f32) {
        let cutoff = if cutoff_hz.is_finite() {
            cutoff_hz.clamp(MIN_CUTOFF_HZ, max_cutoff(self.sample_rate))
        } else {
            self.cutoff.max(MIN_CUTOFF_HZ)
        };
        let resonance = if resonance.is_finite() {
            resonance.clamp(0.0, 1.0)
        } else {
            self.resonance.max(0.0)
        };
        if (cutoff - self.cutoff).abs() <= self.cutoff * CUTOFF_EPSILON
            && (resonance - self.resonance).abs() <= RESONANCE_EPSILON
        {
            return;
        }
        self.cutoff = cutoff;
        self.resonance = resonance;
        let g = (core::f32::consts::PI * cutoff / self.sample_rate).tan();
        let k = 1.0 / resonance_to_q(resonance);
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Filters one sample and returns the lowpass output.
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let v3 = x - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = flush_denormal(2.0 * v1 - self.ic1eq);
        self.ic2eq = flush_denormal(2.0 * v2 - self.ic2eq);
        v2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (core::f32::consts::TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
    }

    fn filtered_rms(hz: f32, cutoff: f32, res: f32) -> f32 {
        let mut f = Svf::new(SR, cutoff, res);
        let out: Vec<f32> = sine(hz, 48_000).iter().map(|&x| f.tick(x)).collect();
        rms(&out[24_000..])
    }

    #[test]
    fn passes_low_and_attenuates_high() {
        let low = filtered_rms(100.0, 1_000.0, 0.0);
        let high = filtered_rms(10_000.0, 1_000.0, 0.0);
        let unfiltered = 1.0 / 2.0_f32.sqrt();
        assert!((low - unfiltered).abs() < 0.02, "low {low}");
        assert!(high < unfiltered * 0.02, "high {high}");
    }

    #[test]
    fn resonance_boosts_near_cutoff() {
        let flat = filtered_rms(1_000.0, 1_000.0, 0.0);
        let peaky = filtered_rms(1_000.0, 1_000.0, 0.9);
        assert!(peaky > flat * 3.0, "flat {flat} peaky {peaky}");
        // Still bounded: no self-oscillation, gain at most MAX_Q.
        assert!(peaky < MAX_Q, "peaky {peaky}");
    }

    #[test]
    fn clamps_cutoff_and_ignores_non_finite() {
        let mut f = Svf::new(SR, 1.0e9, 5.0);
        assert_eq!(f.cutoff(), max_cutoff(SR));
        assert_eq!(f.resonance(), 1.0);
        f.set(1.0, -1.0);
        assert_eq!(f.cutoff(), MIN_CUTOFF_HZ);
        assert_eq!(f.resonance(), 0.0);
        f.set(f32::NAN, f32::INFINITY);
        assert_eq!(f.cutoff(), MIN_CUTOFF_HZ);
        assert_eq!(f.resonance(), 0.0);
        let low_sr = Svf::new(10_000.0, 20_000.0, 0.0);
        assert_eq!(low_sr.cutoff(), 4_500.0);
    }

    #[test]
    fn decays_to_exact_silence() {
        let mut f = Svf::new(SR, 200.0, 1.0);
        f.tick(1.0);
        for _ in 0..SR as usize {
            f.tick(0.0);
        }
        assert_eq!(f.tick(0.0), 0.0);
    }

    #[test]
    fn stable_under_fast_modulation() {
        let mut f = Svf::new(SR, 200.0, 1.0);
        let mut peak = 0.0_f32;
        for i in 0..200_000 {
            let cutoff = if i % 7 == 0 { 20.0 } else { 20_000.0 };
            f.set(cutoff, (i % 11) as f32 / 10.0);
            let y = f.tick(if i % 2 == 0 { 1.0 } else { -1.0 });
            assert!(y.is_finite());
            peak = peak.max(y.abs());
        }
        assert!(peak < 50.0, "peak {peak}");
    }
}
