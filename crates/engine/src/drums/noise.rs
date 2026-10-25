//! Tiny xorshift32 pseudo-random noise source for the drum voices.
//!
//! Deterministic (fixed seeds), 4 bytes of state, no dependency on `rand`. The quality is more
//! than enough for percussion noise: a period of `2^32 - 1` and a flat spectrum.

/// Default seed used by the kit; any non-zero value works.
pub const DEFAULT_SEED: u32 = 0x9E37_79B9;

/// xorshift32 white-noise generator.
///
/// Real-time safe: `Copy`, no allocation, no panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Noise {
    state: u32,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(DEFAULT_SEED)
    }
}

impl Noise {
    /// Creates a generator from `seed`. A zero seed (which would lock xorshift at zero forever)
    /// is replaced by [`DEFAULT_SEED`].
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        let state = if seed == 0 { DEFAULT_SEED } else { seed };
        Self { state }
    }

    /// Next raw 32-bit value.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Next white-noise sample, uniformly distributed in `-1.0..=1.0`.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        // 24 significant bits so every value is exactly representable and the result never
        // leaves [-1, 1] through rounding.
        let bits = (self.next_u32() >> 8) as f32;
        bits * (2.0 / 16_777_215.0) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_in_range_and_covers_both_halves() {
        let mut n = Noise::default();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        let mut sum = 0.0_f64;
        for _ in 0..100_000 {
            let v = n.next_f32();
            assert!((-1.0..=1.0).contains(&v), "{v}");
            lo = lo.min(v);
            hi = hi.max(v);
            sum += f64::from(v);
        }
        assert!(lo < -0.99, "min {lo}");
        assert!(hi > 0.99, "max {hi}");
        let mean = sum / 100_000.0;
        assert!(mean.abs() < 0.02, "mean {mean}");
    }

    #[test]
    fn is_not_constant_and_is_deterministic() {
        let mut a = Noise::new(42);
        let mut b = Noise::new(42);
        let first: Vec<f32> = (0..64).map(|_| a.next_f32()).collect();
        let again: Vec<f32> = (0..64).map(|_| b.next_f32()).collect();
        assert_eq!(first, again);
        let distinct = first.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(distinct > 60, "distinct {distinct}");
    }

    #[test]
    fn zero_seed_is_replaced() {
        let mut n = Noise::new(0);
        assert_ne!(n.next_u32(), 0);
        assert_ne!(n.next_u32(), 0);
    }
}
