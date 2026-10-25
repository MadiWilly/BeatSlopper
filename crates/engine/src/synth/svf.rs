//! State-variable lowpass filter in the TPT ("topology preserving transform", zero-delay
//! feedback) form after Zavalishin.
//!
//! The TPT form is stable for every cutoff below Nyquist and for any damping, and its cutoff
//! and resonance can be modulated per sample without blowing up. The structure yields the
//! lowpass, bandpass and highpass outputs in one pass; [`Svf::set_type`] picks which one
//! [`Svf::tick`] returns (M8).

use crate::dsp::flush_denormal;

/// Which output of the state-variable filter is used. The index order matches
/// `ParamId::FilterType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FilterType {
    /// 12 dB/octave lowpass.
    #[default]
    Lowpass,
    /// Bandpass, unity gain at the cutoff for every resonance (resonance narrows it).
    Bandpass,
    /// 12 dB/octave highpass.
    Highpass,
}

impl FilterType {
    /// Every type in parameter-index order.
    pub const ALL: [FilterType; 3] = [
        FilterType::Lowpass,
        FilterType::Bandpass,
        FilterType::Highpass,
    ];

    /// Type for a parameter index; out-of-range indices clamp to the last type.
    #[must_use]
    pub const fn from_index(index: usize) -> FilterType {
        match index {
            0 => FilterType::Lowpass,
            1 => FilterType::Bandpass,
            _ => FilterType::Highpass,
        }
    }

    /// Type for a parameter value (rounded, clamped). Non-finite values give the default.
    #[must_use]
    pub fn from_param(value: f32) -> FilterType {
        if !value.is_finite() {
            return FilterType::default();
        }
        FilterType::from_index(value.round().clamp(0.0, 2.0) as usize)
    }

    /// Parameter index of this type.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Short display name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            FilterType::Lowpass => "LP",
            FilterType::Bandpass => "BP",
            FilterType::Highpass => "HP",
        }
    }
}

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

/// TPT state-variable filter (lowpass by default; see [`FilterType`]).
///
/// Real-time safe: `Copy`, no allocation, no panics; the two integrator states are
/// denormal-flushed every sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Svf {
    sample_rate: f32,
    cutoff: f32,
    resonance: f32,
    filter_type: FilterType,
    a1: f32,
    a2: f32,
    a3: f32,
    /// Damping `1 / Q` (needed by the bandpass and highpass outputs).
    k: f32,
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
            filter_type: FilterType::Lowpass,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            k: 2.0,
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

    /// Which output [`tick`](Self::tick) returns.
    #[must_use]
    pub fn filter_type(&self) -> FilterType {
        self.filter_type
    }

    /// Selects the output of [`tick`](Self::tick). The state is shared by all three outputs,
    /// so switching is click-free.
    pub fn set_type(&mut self, filter_type: FilterType) {
        self.filter_type = filter_type;
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
        self.k = k;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Filters one sample and returns the selected output (lowpass unless
    /// [`set_type`](Self::set_type) chose otherwise).
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let v3 = x - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = flush_denormal(2.0 * v1 - self.ic1eq);
        self.ic2eq = flush_denormal(2.0 * v2 - self.ic2eq);
        match self.filter_type {
            FilterType::Lowpass => v2,
            FilterType::Bandpass => self.k * v1,
            FilterType::Highpass => x - self.k * v1 - v2,
        }
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
        typed_rms(FilterType::Lowpass, hz, cutoff, res)
    }

    fn typed_rms(t: FilterType, hz: f32, cutoff: f32, res: f32) -> f32 {
        let mut f = Svf::new(SR, cutoff, res);
        f.set_type(t);
        let out: Vec<f32> = sine(hz, 48_000).iter().map(|&x| f.tick(x)).collect();
        rms(&out[24_000..])
    }

    #[test]
    fn type_indices_round_trip() {
        for (i, t) in FilterType::ALL.iter().enumerate() {
            assert_eq!(t.index(), i);
            assert_eq!(FilterType::from_index(i), *t);
            assert_eq!(FilterType::from_param(i as f32 - 0.3), *t);
            assert!(!t.label().is_empty());
        }
        assert_eq!(FilterType::from_param(f32::NAN), FilterType::Lowpass);
        assert_eq!(FilterType::from_index(9), FilterType::Highpass);
        let mut f = Svf::new(SR, 1000.0, 0.0);
        assert_eq!(f.filter_type(), FilterType::Lowpass);
        f.set_type(FilterType::Highpass);
        assert_eq!(f.filter_type(), FilterType::Highpass);
    }

    #[test]
    fn bandpass_and_highpass_shape_the_spectrum() {
        let unfiltered = 1.0 / 2.0_f32.sqrt();
        let hp_low = typed_rms(FilterType::Highpass, 50.0, 1_000.0, 0.0);
        let hp_high = typed_rms(FilterType::Highpass, 10_000.0, 1_000.0, 0.0);
        assert!(hp_low < unfiltered * 0.01, "hp low {hp_low}");
        assert!((hp_high - unfiltered).abs() < 0.03, "hp high {hp_high}");

        let bp_low = typed_rms(FilterType::Bandpass, 100.0, 1_000.0, 0.0);
        let bp_mid = typed_rms(FilterType::Bandpass, 1_000.0, 1_000.0, 0.0);
        let bp_high = typed_rms(FilterType::Bandpass, 10_000.0, 1_000.0, 0.0);
        assert!((bp_mid - unfiltered).abs() < 0.03, "bp mid {bp_mid}");
        assert!(bp_low < bp_mid * 0.25, "bp low {bp_low} mid {bp_mid}");
        assert!(bp_high < bp_mid * 0.25, "bp high {bp_high} mid {bp_mid}");
        // Resonance narrows the bandpass without raising its centre gain.
        let bp_mid_res = typed_rms(FilterType::Bandpass, 1_000.0, 1_000.0, 0.9);
        let bp_near_res = typed_rms(FilterType::Bandpass, 700.0, 1_000.0, 0.9);
        let bp_near = typed_rms(FilterType::Bandpass, 700.0, 1_000.0, 0.0);
        assert!((bp_mid_res - unfiltered).abs() < 0.05, "{bp_mid_res}");
        assert!(bp_near_res < bp_near * 0.6, "{bp_near_res} vs {bp_near}");
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
