//! Small numeric helpers shared by all DSP modules.
//!
//! Everything here is `#[inline]`, allocation-free and panic-free so it can be used inside
//! [`crate::Engine::render`].

/// Magnitude below which a value is treated as zero by [`flush_denormal`].
///
/// `f32` denormals start below about `1.2e-38`; flushing well above that keeps recursive
/// filters and smoothers from ever reaching the (very slow) denormal range.
pub const DENORMAL_THRESHOLD: f32 = 1.0e-18;

/// Tiny DC offset that can be added into a feedback path (delay line, reverb comb, filter
/// state) so the signal decays into this constant instead of into denormals.
///
/// Inaudible: about -360 dBFS.
pub const DENORMAL_OFFSET: f32 = 1.0e-18;

/// Flushes values with magnitude below [`DENORMAL_THRESHOLD`] to exactly `0.0`.
///
/// Use on every feedback path (filter state, delay feedback, smoother state) so that decaying
/// signals never enter the denormal range, where arithmetic is orders of magnitude slower.
#[inline]
#[must_use]
pub fn flush_denormal(x: f32) -> f32 {
    if x.abs() < DENORMAL_THRESHOLD { 0.0 } else { x }
}

/// Converts a level in decibels (relative to full scale) to a linear gain.
#[inline]
#[must_use]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a linear gain to decibels. Gains at or below zero return `-inf`.
#[inline]
#[must_use]
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * gain.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_zeroes_tiny_values_and_keeps_normal_ones() {
        assert_eq!(flush_denormal(1.0e-30), 0.0);
        assert_eq!(flush_denormal(-1.0e-30), 0.0);
        assert_eq!(flush_denormal(0.5), 0.5);
        assert_eq!(flush_denormal(-0.5), -0.5);
    }

    #[test]
    fn db_round_trips() {
        let g = db_to_gain(-12.0);
        assert!((g - 0.251_188_6).abs() < 1e-6);
        assert!((gain_to_db(g) + 12.0).abs() < 1e-4);
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
    }
}
