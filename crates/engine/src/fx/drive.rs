//! Drive: a `tanh` waveshaper with level compensation and a dry/wet mix.
//!
//! The pre-gain runs from 1x (amount `0.0`) to 24x (amount `1.0`). The shaped signal is
//! divided by `tanh(gain)` so that a full-scale input always peaks at exactly `1.0` whatever
//! the amount: turning the knob changes the *colour*, not the loudness of hot signals. Because
//! `|tanh(g * x)| <= tanh(g)` for `|x| <= 1`, the wet output never exceeds `1.0` for inputs
//! within full scale.
//!
//! Memoryless, so there is nothing to reset. Real-time safe: `Copy`, no allocation, no
//! panics.

/// Pre-gain at amount `1.0` (`1 + MAX_EXTRA_GAIN`).
pub const MAX_EXTRA_GAIN: f32 = 23.0;

/// `tanh` waveshaper with gain compensation and dry/wet mix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drive {
    amount: f32,
    gain: f32,
    /// `1 / tanh(gain)`.
    comp: f32,
    mix: f32,
}

impl Default for Drive {
    fn default() -> Self {
        Self::new()
    }
}

impl Drive {
    /// Creates a drive stage with amount `0.0` and mix `0.0` (fully dry).
    #[must_use]
    pub fn new() -> Self {
        let mut d = Self {
            amount: -1.0,
            gain: 1.0,
            comp: 1.0,
            mix: 0.0,
        };
        d.set_amount(0.0);
        d
    }

    /// Sets the drive amount in `0.0..=1.0` (clamped; non-finite values are ignored).
    ///
    /// Cheap when the value is unchanged, so it can be fed from a smoother every sample.
    pub fn set_amount(&mut self, amount: f32) {
        if !amount.is_finite() {
            return;
        }
        let amount = amount.clamp(0.0, 1.0);
        if amount == self.amount {
            return;
        }
        self.amount = amount;
        self.gain = 1.0 + amount * MAX_EXTRA_GAIN;
        self.comp = 1.0 / self.gain.tanh();
    }

    /// Sets the dry/wet mix in `0.0..=1.0` (clamped; non-finite values are ignored).
    pub fn set_mix(&mut self, mix: f32) {
        if mix.is_finite() {
            self.mix = mix.clamp(0.0, 1.0);
        }
    }

    /// Current drive amount.
    #[must_use]
    pub fn amount(&self) -> f32 {
        self.amount
    }

    /// Current pre-gain (`1.0..=24.0`).
    #[must_use]
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Current dry/wet mix.
    #[must_use]
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Shapes one sample. Non-finite inputs become `0.0`.
    #[inline]
    #[must_use]
    pub fn process(&self, x: f32) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        let wet = (self.gain * x).tanh() * self.comp;
        x + (wet - x) * self.mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_follows_amount() {
        let mut d = Drive::new();
        assert_eq!(d.gain(), 1.0);
        d.set_amount(1.0);
        assert_eq!(d.gain(), 24.0);
        d.set_amount(7.0);
        assert_eq!(d.amount(), 1.0);
        d.set_amount(f32::NAN);
        assert_eq!(d.amount(), 1.0);
    }

    #[test]
    fn full_scale_peaks_at_one_for_any_amount() {
        for i in 0..=10 {
            let mut d = Drive::new();
            d.set_amount(i as f32 * 0.1);
            d.set_mix(1.0);
            let y = d.process(1.0);
            assert!((y - 1.0).abs() < 1e-5, "amount {i} peak {y}");
            assert!((d.process(-1.0) + 1.0).abs() < 1e-5);
            for k in 0..200 {
                let x = (k as f32 - 100.0) / 100.0;
                let y = d.process(x);
                assert!(y.abs() <= 1.0 + 1e-6, "x={x} y={y}");
                assert!(y.is_finite());
            }
        }
    }

    #[test]
    fn dry_mix_is_identity() {
        let mut d = Drive::new();
        d.set_amount(1.0);
        d.set_mix(0.0);
        assert_eq!(d.process(0.3), 0.3);
        assert_eq!(d.process(-0.7), -0.7);
    }

    #[test]
    fn non_finite_input_is_silenced() {
        let d = Drive::new();
        assert_eq!(d.process(f32::NAN), 0.0);
        assert_eq!(d.process(f32::INFINITY), 0.0);
    }

    #[test]
    fn monotonic_and_odd() {
        let mut d = Drive::new();
        d.set_amount(0.5);
        d.set_mix(1.0);
        let mut prev = d.process(-1.0);
        for k in 1..=200 {
            let x = -1.0 + k as f32 * 0.01;
            let y = d.process(x);
            assert!(y >= prev);
            assert!((y + d.process(-x)).abs() < 1e-6);
            prev = y;
        }
    }
}
