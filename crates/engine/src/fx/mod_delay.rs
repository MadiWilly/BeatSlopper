//! A short delay line with a fractional, linearly interpolated read tap, shared by the
//! modulation effects ([`Lofi`](super::Lofi) wobble and [`Chorus`](super::Chorus)).
//!
//! The ring buffer is allocated once in [`ModDelay::new`]; [`push`](ModDelay::push) and
//! [`read`](ModDelay::read) only index into it, so the per-sample path is allocation-free and
//! panic-free. A read at distance `0.0` returns the sample just pushed bit-exactly (the
//! interpolation weight of the neighbour is `0.0`), which is what lets the wobble stage be
//! transparent at depth zero.

/// Ring buffer with a fractional read tap.
#[derive(Debug, Clone, PartialEq)]
pub struct ModDelay {
    buf: Vec<f32>,
    /// Index the next `push` writes to.
    write: usize,
    /// Largest read distance in samples (`buf.len() - 2`).
    max_delay: f32,
}

impl ModDelay {
    /// Allocates a line able to delay by up to `max_samples` whole samples (plus one more for
    /// the interpolation neighbour). `max_samples` is clamped to at least `1`.
    #[must_use]
    pub fn new(max_samples: usize) -> Self {
        let max_samples = max_samples.max(1);
        Self {
            buf: vec![0.0; max_samples + 2],
            write: 0,
            max_delay: max_samples as f32,
        }
    }

    /// Largest read distance in samples.
    #[must_use]
    pub fn max_delay(&self) -> f32 {
        self.max_delay
    }

    /// Zeroes the buffer.
    pub fn reset(&mut self) {
        self.buf.fill(0.0);
        self.write = 0;
    }

    /// Writes `x` as the newest sample. Non-finite values are stored as `0.0`.
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.write] = if x.is_finite() { x } else { 0.0 };
        self.write += 1;
        if self.write == self.buf.len() {
            self.write = 0;
        }
    }

    /// Reads `delay` samples behind the newest sample with linear interpolation; `0.0` is the
    /// newest sample itself. The distance is clamped to `0.0..=max_delay()`; a non-finite
    /// distance reads the newest sample.
    #[inline]
    #[must_use]
    pub fn read(&self, delay: f32) -> f32 {
        let d = if delay.is_finite() {
            delay.clamp(0.0, self.max_delay)
        } else {
            0.0
        };
        let whole = d as usize;
        let frac = d - whole as f32;
        let n = self.buf.len();
        // Newest sample sits at `write - 1`.
        let i0 = (self.write + n - 1 - whole) % n;
        let i1 = if i0 == 0 { n - 1 } else { i0 - 1 };
        let s0 = self.buf[i0];
        let s1 = self.buf[i1];
        s0 + (s1 - s0) * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_distance_is_bit_exact() {
        let mut d = ModDelay::new(10);
        for i in 0..100 {
            let x = (i as f32 * 0.37).sin() * 0.8;
            d.push(x);
            assert_eq!(d.read(0.0), x);
        }
    }

    #[test]
    fn whole_and_fractional_reads() {
        let mut d = ModDelay::new(8);
        for i in 0..20 {
            d.push(i as f32);
        }
        // Newest is 19.
        assert_eq!(d.read(0.0), 19.0);
        assert_eq!(d.read(3.0), 16.0);
        assert!((d.read(2.5) - 16.5).abs() < 1e-6);
        assert_eq!(d.read(8.0), 11.0);
        // Clamps: beyond the maximum reads the oldest reachable sample.
        assert_eq!(d.read(50.0), 11.0);
        assert_eq!(d.read(-1.0), 19.0);
        assert_eq!(d.read(f32::NAN), 19.0);
    }

    #[test]
    fn reset_and_non_finite() {
        let mut d = ModDelay::new(4);
        d.push(f32::NAN);
        d.push(f32::INFINITY);
        assert_eq!(d.read(0.0), 0.0);
        assert_eq!(d.read(1.0), 0.0);
        d.push(1.0);
        d.reset();
        for k in 0..5 {
            assert_eq!(d.read(k as f32), 0.0);
        }
        assert_eq!(ModDelay::new(0).max_delay(), 1.0);
    }
}
