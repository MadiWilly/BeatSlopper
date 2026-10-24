//! Master soft limiter: a cheap soft clipper that guarantees `|y| <= 1.0`.
//!
//! Signals below the knee pass through untouched; above it the excess is squashed with a
//! `tanh` curve that approaches (but never exceeds) full scale. The transfer function is
//! continuous with a continuous first derivative at the knee, so there is no audible seam.

/// Default knee: levels up to -6 dBFS are completely transparent.
pub const DEFAULT_KNEE: f32 = 0.5;

/// Memoryless soft clipper used as the final stage of the master bus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limiter {
    knee: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Limiter {
    /// Creates a limiter with [`DEFAULT_KNEE`].
    #[must_use]
    pub const fn new() -> Self {
        Self { knee: DEFAULT_KNEE }
    }

    /// Creates a limiter whose linear region ends at `knee` (clamped to `0.0..=0.99`).
    #[must_use]
    pub fn with_knee(knee: f32) -> Self {
        let knee = if knee.is_finite() {
            knee.clamp(0.0, 0.99)
        } else {
            DEFAULT_KNEE
        };
        Self { knee }
    }

    /// Upper bound of the transparent (linear) region.
    #[must_use]
    pub fn knee(&self) -> f32 {
        self.knee
    }

    /// Soft-clips one sample. Output is always finite and within `-1.0..=1.0`;
    /// NaN and infinite inputs become `0.0`.
    #[inline]
    #[must_use]
    pub fn process(&self, x: f32) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        let mag = x.abs();
        if mag <= self.knee {
            return x;
        }
        let room = 1.0 - self.knee;
        let y = self.knee + room * ((mag - self.knee) / room).tanh();
        y.min(1.0).copysign(x)
    }

    /// Soft-clips a buffer in place.
    pub fn process_buffer(&self, buf: &mut [f32]) {
        for s in buf {
            *s = self.process(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_below_knee() {
        let l = Limiter::new();
        assert_eq!(l.process(0.3), 0.3);
        assert_eq!(l.process(-0.5), -0.5);
        assert_eq!(l.process(0.0), 0.0);
    }

    #[test]
    fn never_exceeds_full_scale() {
        let l = Limiter::new();
        for i in 0..2000 {
            let x = (i as f32 - 1000.0) * 0.05;
            let y = l.process(x);
            assert!(y.abs() <= 1.0, "x={x} y={y}");
            assert!(y.is_finite());
            assert_eq!(y.signum(), x.signum());
        }
        assert!(l.process(1.0e30).abs() <= 1.0);
    }

    #[test]
    fn non_finite_input_is_silenced() {
        let l = Limiter::new();
        assert_eq!(l.process(f32::NAN), 0.0);
        assert_eq!(l.process(f32::INFINITY), 0.0);
        assert_eq!(l.process(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn monotonic() {
        let l = Limiter::new();
        let mut prev = l.process(0.0);
        for i in 1..1000 {
            let y = l.process(i as f32 * 0.01);
            assert!(y >= prev);
            prev = y;
        }
    }
}
