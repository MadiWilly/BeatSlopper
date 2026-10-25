//! Noise source: white (xorshift32) and pink (Paul Kellet's three-pole approximation).
//!
//! Pink noise falls off at 3 dB per octave; Kellet's "economy" filter gets within about
//! +-0.5 dB of that from 10 Hz to 20 kHz with three one-pole sections. The output is scaled so
//! that pink and white have roughly the same RMS; pink peaks may exceed `1.0` occasionally
//! (its distribution is close to Gaussian), which the voice level and the master limiter
//! absorb.

use crate::dsp::flush_denormal;

/// Noise colour. The index order matches `ParamId::NoiseColor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NoiseColor {
    /// Flat spectrum.
    #[default]
    White,
    /// 3 dB/octave roll-off (equal energy per octave).
    Pink,
}

impl NoiseColor {
    /// Every colour in parameter-index order.
    pub const ALL: [NoiseColor; 2] = [NoiseColor::White, NoiseColor::Pink];

    /// Colour for a parameter index; out-of-range indices clamp to the last colour.
    #[must_use]
    pub const fn from_index(index: usize) -> NoiseColor {
        match index {
            0 => NoiseColor::White,
            _ => NoiseColor::Pink,
        }
    }

    /// Colour for a parameter value (rounded, clamped). Non-finite values give the default.
    #[must_use]
    pub fn from_param(value: f32) -> NoiseColor {
        if !value.is_finite() {
            return NoiseColor::default();
        }
        NoiseColor::from_index(value.round().clamp(0.0, 1.0) as usize)
    }

    /// Parameter index of this colour.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Short display name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            NoiseColor::White => "White",
            NoiseColor::Pink => "Pink",
        }
    }
}

/// Scale applied to the pink filter's output so its RMS roughly matches white noise.
const PINK_GAIN: f32 = 0.33;

/// Seed used when `0` is requested (xorshift must never hold a zero state).
const FALLBACK_SEED: u32 = 0x9E37_79B9;

/// White/pink noise generator. Real-time safe: `Copy`, no allocation, no panics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Noise {
    state: u32,
    seed: u32,
    color: NoiseColor,
    b0: f32,
    b1: f32,
    b2: f32,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Noise {
    /// Creates a white noise generator from `seed` (a zero seed is replaced by a constant).
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let seed = if seed == 0 { FALLBACK_SEED } else { seed };
        Self {
            state: seed,
            seed,
            color: NoiseColor::White,
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
        }
    }

    /// Current colour.
    #[must_use]
    pub fn color(&self) -> NoiseColor {
        self.color
    }

    /// Switches the colour; the pink filter keeps its state so the switch is click-free.
    pub fn set_color(&mut self, color: NoiseColor) {
        self.color = color;
    }

    /// Restarts the sequence from the seed and clears the pink filter.
    pub fn reset(&mut self) {
        self.state = self.seed;
        self.b0 = 0.0;
        self.b1 = 0.0;
        self.b2 = 0.0;
    }

    /// Next white sample in `-1.0..1.0` (uniform).
    #[inline]
    pub fn white(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        // Top 24 bits -> [0, 1) exactly representable, then centred.
        (x >> 8) as f32 * (1.0 / 8_388_608.0) - 1.0
    }

    /// Next sample of the current colour.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let white = self.white();
        match self.color {
            NoiseColor::White => white,
            NoiseColor::Pink => {
                self.b0 = flush_denormal(0.997_65 * self.b0 + white * 0.099_046);
                self.b1 = flush_denormal(0.963_00 * self.b1 + white * 0.296_516_4);
                self.b2 = flush_denormal(0.570_00 * self.b2 + white * 1.052_691_3);
                (self.b0 + self.b1 + self.b2 + white * 0.184_8) * PINK_GAIN
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(buf: &[f32]) -> (f32, f32, f32) {
        let mean = buf.iter().sum::<f32>() / buf.len() as f32;
        let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        let hp: Vec<f32> = buf.windows(2).map(|w| w[1] - w[0]).collect();
        let hp_rms = (hp.iter().map(|s| s * s).sum::<f32>() / hp.len() as f32).sqrt();
        (mean, rms, hp_rms / rms)
    }

    #[test]
    fn color_indices_round_trip() {
        for (i, c) in NoiseColor::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
            assert_eq!(NoiseColor::from_index(i), *c);
            assert_eq!(NoiseColor::from_param(i as f32 + 0.2), *c);
            assert!(!c.label().is_empty());
        }
        assert_eq!(NoiseColor::from_param(f32::NAN), NoiseColor::White);
        assert_eq!(NoiseColor::from_index(9), NoiseColor::Pink);
    }

    #[test]
    fn white_is_uniform_and_pink_is_darker_with_similar_level() {
        let mut n = Noise::new(42);
        let white: Vec<f32> = (0..200_000).map(|_| n.tick()).collect();
        n.set_color(NoiseColor::Pink);
        let pink: Vec<f32> = (0..200_000).map(|_| n.tick()).collect();
        assert!(
            white
                .iter()
                .all(|s| s.is_finite() && (-1.0..1.0).contains(s))
        );
        assert!(pink.iter().all(|s| s.is_finite()));
        let (wm, wr, wb) = stats(&white);
        let (pm, pr, pb) = stats(&pink);
        assert!(wm.abs() < 0.01, "white mean {wm}");
        assert!((wr - 0.577).abs() < 0.02, "white rms {wr}");
        assert!(pm.abs() < 0.05, "pink mean {pm}");
        assert!(
            pr > wr * 0.6 && pr < wr * 1.6,
            "pink rms {pr} vs white {wr}"
        );
        assert!(pb < wb * 0.5, "pink brightness {pb} vs white {wb}");
    }

    #[test]
    fn seeds_differ_and_reset_repeats() {
        let mut a = Noise::new(1);
        let mut b = Noise::new(2);
        let mut z = Noise::new(0);
        let sa: Vec<f32> = (0..16).map(|_| a.tick()).collect();
        let sb: Vec<f32> = (0..16).map(|_| b.tick()).collect();
        assert_ne!(sa, sb);
        assert!(z.tick().is_finite());
        a.reset();
        let again: Vec<f32> = (0..16).map(|_| a.tick()).collect();
        assert_eq!(sa, again);
    }
}
