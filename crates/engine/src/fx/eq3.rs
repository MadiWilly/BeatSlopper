//! Three-band insert EQ (M11): low shelf at [`LOW_SHELF_HZ`], a peaking mid at a chosen
//! frequency (Q [`MID_Q`]) and a high shelf at [`HIGH_SHELF_HZ`], FL's per-insert track EQ.
//!
//! [`Eq3State`] is the plain data (serde, `Copy`): three gains in dB (`-15..=15`) and the mid
//! frequency. [`Eq3`] is the DSP: the three RBJ biquads from [`super::eq`], one pair per
//! band for stereo, with every value smoothed over [`EQ3_SMOOTH_MS`] and the coefficients
//! recomputed only while a smoother is moving, so a knob move never zippers. A flat state
//! (every gain at 0 dB) is bit-transparent: the biquads are skipped entirely and their
//! memories cleared, so an insert at the defaults reproduces its input exactly.
//!
//! Real-time safe after [`Eq3::new`]: `Copy`, no allocation, no panics, non-finite input
//! treated as silence, every feedback path denormal-flushed.

use serde::{Deserialize, Serialize};

use crate::params::{DEFAULT_SMOOTH_MS, Smoother};

use super::eq::{BiquadState, Coeffs, DEFAULT_Q, EqKind};

/// Corner frequency of the low shelf, Hz.
pub const LOW_SHELF_HZ: f32 = 200.0;
/// Corner frequency of the high shelf, Hz.
pub const HIGH_SHELF_HZ: f32 = 4000.0;
/// Q of the peaking mid band.
pub const MID_Q: f32 = 0.7;
/// Lowest band gain, dB.
pub const MIN_GAIN_DB: f32 = -15.0;
/// Highest band gain, dB.
pub const MAX_GAIN_DB: f32 = 15.0;
/// Lowest mid frequency, Hz.
pub const MIN_MID_HZ: f32 = 100.0;
/// Highest mid frequency, Hz.
pub const MAX_MID_HZ: f32 = 10_000.0;
/// Default mid frequency, Hz.
pub const DEFAULT_MID_HZ: f32 = 1000.0;
/// Smoothing time constant of the gains and the mid frequency, milliseconds.
pub const EQ3_SMOOTH_MS: f32 = DEFAULT_SMOOTH_MS;

/// Clamps `v` into `min..=max`; non-finite values become `fallback`.
fn clamp_or(v: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        fallback
    }
}

/// Clamps a band gain into `MIN_GAIN_DB..=MAX_GAIN_DB` (non-finite = 0 dB).
#[must_use]
pub fn clamp_gain_db(db: f32) -> f32 {
    clamp_or(db, MIN_GAIN_DB, MAX_GAIN_DB, 0.0)
}

/// Clamps a mid frequency into `MIN_MID_HZ..=MAX_MID_HZ` (non-finite = the default).
#[must_use]
pub fn clamp_mid_hz(hz: f32) -> f32 {
    clamp_or(hz, MIN_MID_HZ, MAX_MID_HZ, DEFAULT_MID_HZ)
}

/// The three-band EQ's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Eq3State {
    /// Low shelf gain, dB (`-15..=15`).
    pub low_gain_db: f32,
    /// Mid peak gain, dB (`-15..=15`).
    pub mid_gain_db: f32,
    /// Mid peak centre frequency, Hz (`100..=10000`).
    pub mid_freq_hz: f32,
    /// High shelf gain, dB (`-15..=15`).
    pub high_gain_db: f32,
}

impl Default for Eq3State {
    /// [`Eq3State::FLAT`].
    fn default() -> Self {
        Self::FLAT
    }
}

impl Eq3State {
    /// Every band at 0 dB, mid at [`DEFAULT_MID_HZ`].
    pub const FLAT: Eq3State = Eq3State {
        low_gain_db: 0.0,
        mid_gain_db: 0.0,
        mid_freq_hz: DEFAULT_MID_HZ,
        high_gain_db: 0.0,
    };

    /// A state with every value given (clamped lazily by [`normalise`](Self::normalise)).
    #[must_use]
    pub const fn new(
        low_gain_db: f32,
        mid_gain_db: f32,
        mid_freq_hz: f32,
        high_gain_db: f32,
    ) -> Self {
        Self {
            low_gain_db,
            mid_gain_db,
            mid_freq_hz,
            high_gain_db,
        }
    }

    /// Clamps every value into range (non-finite values go to the defaults).
    pub fn normalise(&mut self) {
        self.low_gain_db = clamp_gain_db(self.low_gain_db);
        self.mid_gain_db = clamp_gain_db(self.mid_gain_db);
        self.mid_freq_hz = clamp_mid_hz(self.mid_freq_hz);
        self.high_gain_db = clamp_gain_db(self.high_gain_db);
    }

    /// `true` when every gain is 0 dB (the EQ changes nothing).
    #[must_use]
    pub fn is_flat(&self) -> bool {
        self.low_gain_db == 0.0 && self.mid_gain_db == 0.0 && self.high_gain_db == 0.0
    }

    /// Magnitude of the whole EQ at `freq_hz` in dB (the sum of the three bands), for a UI
    /// curve. Uses the target (unsmoothed) values.
    #[must_use]
    pub fn response_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let (low, mid, high) = coefficients(self, sample_rate);
        low.response_db(freq_hz, sample_rate)
            + mid.response_db(freq_hz, sample_rate)
            + high.response_db(freq_hz, sample_rate)
    }
}

/// The three biquads of `state` at `sample_rate`.
fn coefficients(state: &Eq3State, sample_rate: f32) -> (Coeffs, Coeffs, Coeffs) {
    (
        Coeffs::compute(
            EqKind::LowShelf,
            LOW_SHELF_HZ,
            state.low_gain_db,
            DEFAULT_Q,
            sample_rate,
        ),
        Coeffs::compute(
            EqKind::Peak,
            state.mid_freq_hz,
            state.mid_gain_db,
            MID_Q,
            sample_rate,
        ),
        Coeffs::compute(
            EqKind::HighShelf,
            HIGH_SHELF_HZ,
            state.high_gain_db,
            DEFAULT_Q,
            sample_rate,
        ),
    )
}

/// The three-band EQ DSP. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eq3 {
    sample_rate: f32,
    state: Eq3State,
    low_gain: Smoother,
    mid_gain: Smoother,
    mid_freq: Smoother,
    high_gain: Smoother,
    low: Coeffs,
    mid: Coeffs,
    high: Coeffs,
    /// Biquad memories: `[low L, low R, mid L, mid R, high L, high R]`.
    mem: [BiquadState; 6],
}

impl Eq3 {
    /// A flat EQ for `sample_rate` (non-finite or non-positive rates fall back to 48 kHz).
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let state = Eq3State::FLAT;
        let s = |v: f32| Smoother::new(sample_rate, EQ3_SMOOTH_MS, v);
        let mut eq = Self {
            sample_rate,
            state,
            low_gain: s(0.0),
            mid_gain: s(0.0),
            mid_freq: s(state.mid_freq_hz),
            high_gain: s(0.0),
            low: Coeffs::IDENTITY,
            mid: Coeffs::IDENTITY,
            high: Coeffs::IDENTITY,
            mem: [BiquadState::default(); 6],
        };
        eq.recompute();
        eq
    }

    /// Sample rate the EQ runs at.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// The target state (what the UI mirrors).
    #[must_use]
    pub fn state(&self) -> &Eq3State {
        &self.state
    }

    /// Recomputes the coefficients from the smoothers' current values.
    fn recompute(&mut self) {
        let current = Eq3State {
            low_gain_db: self.low_gain.current(),
            mid_gain_db: self.mid_gain.current(),
            mid_freq_hz: self.mid_freq.current(),
            high_gain_db: self.high_gain.current(),
        };
        let (low, mid, high) = coefficients(&current, self.sample_rate);
        self.low = low;
        self.mid = mid;
        self.high = high;
    }

    /// Points the smoothers at `state` (normalised); the change glides over
    /// [`EQ3_SMOOTH_MS`].
    pub fn retarget(&mut self, state: &Eq3State) {
        let mut state = *state;
        state.normalise();
        self.state = state;
        self.low_gain.set_target(state.low_gain_db);
        self.mid_gain.set_target(state.mid_gain_db);
        self.mid_freq.set_target(state.mid_freq_hz);
        self.high_gain.set_target(state.high_gain_db);
    }

    /// Jumps straight to `state` (normalised) and clears the filter memories (project load).
    pub fn snap(&mut self, state: &Eq3State) {
        self.retarget(state);
        self.low_gain.snap(self.state.low_gain_db);
        self.mid_gain.snap(self.state.mid_gain_db);
        self.mid_freq.snap(self.state.mid_freq_hz);
        self.high_gain.snap(self.state.high_gain_db);
        self.recompute();
        self.mem = [BiquadState::default(); 6];
    }

    /// Clears the filter memories and snaps the smoothers to their targets.
    pub fn reset(&mut self) {
        let state = self.state;
        self.snap(&state);
    }

    /// `true` while the EQ passes its input through untouched (every smoothed gain at 0 dB).
    #[must_use]
    pub fn is_transparent(&self) -> bool {
        self.low_gain.current() == 0.0
            && self.mid_gain.current() == 0.0
            && self.high_gain.current() == 0.0
            && self.low_gain.is_settled()
            && self.mid_gain.is_settled()
            && self.high_gain.is_settled()
    }

    /// Processes one stereo frame. Non-finite inputs are treated as `0.0`; the output is
    /// always finite, and exactly the input while flat.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let moving = !self.low_gain.is_settled()
            || !self.mid_gain.is_settled()
            || !self.mid_freq.is_settled()
            || !self.high_gain.is_settled();
        let lg = self.low_gain.tick();
        let mg = self.mid_gain.tick();
        self.mid_freq.tick();
        let hg = self.high_gain.tick();
        if moving {
            self.recompute();
        }
        if lg == 0.0 && mg == 0.0 && hg == 0.0 {
            // Flat: bypass and keep the memories clear so a later boost starts clean.
            self.mem = [BiquadState::default(); 6];
            return (l, r);
        }
        let yl = self.mem[0].tick(l, &self.low);
        let yr = self.mem[1].tick(r, &self.low);
        let yl = self.mem[2].tick(yl, &self.mid);
        let yr = self.mem[3].tick(yr, &self.mid);
        let yl = self.mem[4].tick(yl, &self.high);
        let yr = self.mem[5].tick(yr, &self.high);
        (yl, yr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(n: usize, hz: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * core::f32::consts::TAU * hz / SR).sin() * 0.5)
            .collect()
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum::<f64>()
            / buf.len() as f64)
            .sqrt() as f32
    }

    /// Steady-state gain of `eq` on a sine at `hz`, dB (snapped state, the first 100 ms
    /// skipped).
    fn measure_db(eq: &mut Eq3, hz: f32) -> f32 {
        let input = sine(24_000, hz);
        let out: Vec<f32> = input.iter().map(|&x| eq.process(x, x).0).collect();
        20.0 * (rms(&out[4_800..]) / rms(&input[4_800..])).log10()
    }

    #[test]
    fn state_normalises_and_defaults_are_flat() {
        let mut s = Eq3State::new(40.0, f32::NAN, 5.0, -40.0);
        s.normalise();
        assert_eq!(s, Eq3State::new(15.0, 0.0, 100.0, -15.0));
        assert!(Eq3State::default().is_flat());
        assert!(!s.is_flat());
        assert_eq!(Eq3State::FLAT.response_db(1000.0, SR), 0.0);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Eq3State>(&json).unwrap(), s);
        let partial: Eq3State = serde_json::from_str(r#"{ "low_gain_db": -3.0 }"#).unwrap();
        assert_eq!(partial.low_gain_db, -3.0);
        assert_eq!(partial.mid_freq_hz, DEFAULT_MID_HZ);
    }

    #[test]
    fn flat_is_bit_transparent_and_shelves_cut() {
        let mut eq = Eq3::new(SR);
        assert!(eq.is_transparent());
        for i in 0..4_800 {
            let x = (i as f32 * 0.05).sin() * 0.7;
            assert_eq!(eq.process(x, -x), (x, -x));
        }
        // Low shelf -15 dB: 100 Hz drops by about the analytic response, 2 kHz stays put.
        let state = Eq3State::new(-15.0, 0.0, 1000.0, 0.0);
        eq.snap(&state);
        assert!(!eq.is_transparent());
        let low = measure_db(&mut eq, 100.0);
        let expected = state.response_db(100.0, SR);
        assert!(
            (low - expected).abs() < 0.3,
            "100 Hz: {low} dB, analytic {expected}"
        );
        assert!(low < -10.0, "100 Hz must drop by more than 10 dB: {low}");
        eq.reset();
        let mid = measure_db(&mut eq, 2000.0);
        assert!(mid.abs() < 1.0, "2 kHz: {mid} dB");
        // High shelf -15 dB: 8 kHz drops, 500 Hz stays.
        let state = Eq3State::new(0.0, 0.0, 1000.0, -15.0);
        eq.snap(&state);
        let high = measure_db(&mut eq, 8000.0);
        let expected = state.response_db(8000.0, SR);
        assert!(
            (high - expected).abs() < 0.3,
            "8 kHz: {high} dB, analytic {expected}"
        );
        assert!(high < -10.0, "8 kHz must drop by more than 10 dB: {high}");
        eq.reset();
        let low = measure_db(&mut eq, 500.0);
        assert!(low.abs() < 1.0, "500 Hz: {low} dB");
        // Mid boost at 1 kHz.
        eq.snap(&Eq3State::new(0.0, 6.0, 1000.0, 0.0));
        let peak = measure_db(&mut eq, 1000.0);
        assert!((peak - 6.0).abs() < 0.3, "1 kHz: {peak} dB");
    }

    #[test]
    fn retarget_glides_without_a_step_and_input_is_cleaned() {
        let mut eq = Eq3::new(SR);
        let input = sine(9_600, 200.0);
        let mut out = Vec::with_capacity(input.len());
        for (i, &x) in input.iter().enumerate() {
            if i == 4_800 {
                eq.retarget(&Eq3State::new(12.0, 0.0, 1000.0, 0.0));
            }
            out.push(eq.process(x, x).0);
        }
        let signal_step = input
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        let out_step = out
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(
            out_step < signal_step * 4.5,
            "step {out_step} vs {signal_step}"
        );
        assert!(
            rms(&out[7_200..]) > rms(&input[7_200..]) * 1.5,
            "the boost arrived"
        );
        assert_eq!(eq.state().low_gain_db, 12.0);
        let (l, r) = eq.process(f32::NAN, f32::INFINITY);
        assert!(l.is_finite() && r.is_finite());
        assert_eq!(Eq3::new(f32::NAN).sample_rate(), 48_000.0);
    }
}
