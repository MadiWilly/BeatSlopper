//! Master-output spectrum tap for the EQ window's analyser (plugins milestone).
//!
//! The audio thread writes every output sample (mono, `(l + r) / 2`) into a lock-free ring of
//! [`SPECTRUM_SIZE`] `AtomicU32` bit patterns ([`SpectrumTap`]); the UI thread snapshots the
//! ring in time order ([`SpectrumReader::snapshot`]) and runs a Hann-windowed radix-2 FFT
//! over it ([`Analyzer`]), with a per-bin peak-hold / decay so the display is smooth at 30 fps.
//! Nothing here allocates on the audio side after construction and nothing locks: a torn
//! read (the UI reading while the audio thread writes) only mixes two consecutive blocks,
//! which the window and the smoothing hide.
//!
//! The FFT is hand-written (iterative Cooley-Tukey, `f32`) so the engine keeps its tiny
//! dependency set; 2048 points cost well under a millisecond.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Number of samples the ring holds and the FFT size (a power of two).
pub const SPECTRUM_SIZE: usize = 2048;
/// Number of magnitude bins the analyser produces (`SPECTRUM_SIZE / 2 + 1`).
pub const SPECTRUM_BINS: usize = SPECTRUM_SIZE / 2 + 1;
/// Floor of the analyser's dB scale (bins below it read as the floor).
pub const SPECTRUM_FLOOR_DB: f32 = -90.0;
/// How fast a displayed bin falls after its peak, in dB per second.
pub const SPECTRUM_DECAY_DB_PER_S: f32 = 40.0;
/// How fast a displayed bin rises towards a louder reading (fraction per update at 30 fps).
pub const SPECTRUM_ATTACK: f32 = 0.6;

/// The shared ring.
#[derive(Debug)]
struct Ring {
    samples: Box<[AtomicU32]>,
    /// Index of the next sample to be written (the oldest sample in the ring).
    write: AtomicUsize,
}

/// Audio-side writer. Created with [`spectrum_pair`]; `push` once per output sample and
/// `publish` once per block.
#[derive(Debug)]
pub struct SpectrumTap {
    ring: Arc<Ring>,
    pos: usize,
}

impl SpectrumTap {
    /// Stores one mono sample (non-finite values are stored as `0.0`). Allocation-free.
    #[inline]
    pub fn push(&mut self, sample: f32) {
        let v = if sample.is_finite() { sample } else { 0.0 };
        // SAFETY-free indexing: `pos` is always wrapped below.
        self.ring.samples[self.pos].store(v.to_bits(), Ordering::Relaxed);
        self.pos += 1;
        if self.pos == SPECTRUM_SIZE {
            self.pos = 0;
        }
    }

    /// Publishes the write position so readers know where the newest sample is.
    #[inline]
    pub fn publish(&self) {
        self.ring.write.store(self.pos, Ordering::Release);
    }
}

/// UI-side reader (cheaply clonable; shares the ring).
#[derive(Debug, Clone)]
pub struct SpectrumReader {
    ring: Arc<Ring>,
}

impl SpectrumReader {
    /// Copies the ring into `out` in time order (oldest first, newest last).
    pub fn snapshot(&self, out: &mut [f32; SPECTRUM_SIZE]) {
        let start = self.ring.write.load(Ordering::Acquire) % SPECTRUM_SIZE;
        for (i, slot) in out.iter_mut().enumerate() {
            let idx = (start + i) % SPECTRUM_SIZE;
            let v = f32::from_bits(self.ring.samples[idx].load(Ordering::Relaxed));
            *slot = if v.is_finite() { v } else { 0.0 };
        }
    }
}

/// Creates a connected tap / reader pair (the one allocation, done at engine construction).
#[must_use]
pub fn spectrum_pair() -> (SpectrumTap, SpectrumReader) {
    let samples: Box<[AtomicU32]> = (0..SPECTRUM_SIZE).map(|_| AtomicU32::new(0)).collect();
    let ring = Arc::new(Ring {
        samples,
        write: AtomicUsize::new(0),
    });
    (
        SpectrumTap {
            ring: Arc::clone(&ring),
            pos: 0,
        },
        SpectrumReader { ring },
    )
}

/// In-place iterative radix-2 FFT of `re` / `im` (both of the same power-of-two length).
/// Anything else is left untouched.
pub fn fft_radix2(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    if n < 2 || !n.is_power_of_two() || im.len() != n {
        return;
    }
    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * core::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        let half = len / 2;
        let mut start = 0;
        while start < n {
            let (mut cr, mut ci) = (1.0_f32, 0.0_f32);
            for k in 0..half {
                let a = start + k;
                let b = a + half;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            start += len;
        }
        len <<= 1;
    }
}

/// Centre frequency of FFT bin `bin` at `sample_rate` (bin 0 = DC).
#[must_use]
pub fn bin_hz(bin: usize, sample_rate: f32) -> f32 {
    bin as f32 * sample_rate / SPECTRUM_SIZE as f32
}

/// Hann-windowed FFT analyser with per-bin attack / decay smoothing for the display.
#[derive(Debug, Clone)]
pub struct Analyzer {
    window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Latest raw magnitudes, dB.
    raw: Vec<f32>,
    /// Smoothed magnitudes, dB (what the UI draws).
    shown: Vec<f32>,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    /// An analyser with every bin at the floor.
    #[must_use]
    pub fn new() -> Self {
        let n = SPECTRUM_SIZE;
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * core::f32::consts::PI * i as f32 / (n - 1) as f32).cos())
            .collect();
        Self {
            window,
            re: vec![0.0; n],
            im: vec![0.0; n],
            raw: vec![SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
            shown: vec![SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
        }
    }

    /// Analyses `samples` and updates the smoothed display bins; `dt_seconds` is the time
    /// since the previous call (drives the decay). Returns the smoothed magnitudes in dB,
    /// one per bin ([`SPECTRUM_BINS`]), each at least [`SPECTRUM_FLOOR_DB`] and normalised
    /// so a full-scale sine reads about 0 dB.
    pub fn analyse(&mut self, samples: &[f32; SPECTRUM_SIZE], dt_seconds: f32) -> &[f32] {
        for (i, (re, im)) in self.re.iter_mut().zip(self.im.iter_mut()).enumerate() {
            *re = samples[i] * self.window[i];
            *im = 0.0;
        }
        fft_radix2(&mut self.re, &mut self.im);
        // A full-scale sine under a Hann window sums to N/4 in its bin.
        let norm = 4.0 / SPECTRUM_SIZE as f32;
        for bin in 0..SPECTRUM_BINS {
            let mag = (self.re[bin] * self.re[bin] + self.im[bin] * self.im[bin]).sqrt() * norm;
            let db = if mag > 0.0 {
                (20.0 * mag.log10()).max(SPECTRUM_FLOOR_DB)
            } else {
                SPECTRUM_FLOOR_DB
            };
            self.raw[bin] = db;
        }
        let fall = SPECTRUM_DECAY_DB_PER_S * dt_seconds.clamp(0.0, 1.0);
        for (shown, &raw) in self.shown.iter_mut().zip(&self.raw) {
            if raw > *shown {
                *shown += (raw - *shown) * SPECTRUM_ATTACK;
            } else {
                *shown = (*shown - fall).max(raw).max(SPECTRUM_FLOOR_DB);
            }
        }
        &self.shown
    }

    /// The latest raw (unsmoothed) magnitudes in dB.
    #[must_use]
    pub fn raw(&self) -> &[f32] {
        &self.raw
    }

    /// The smoothed magnitudes in dB (what the UI draws).
    #[must_use]
    pub fn shown(&self) -> &[f32] {
        &self.shown
    }

    /// Smoothed level in dB at `freq_hz` (linear interpolation between bins).
    #[must_use]
    pub fn level_at(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        if !freq_hz.is_finite() || freq_hz <= 0.0 || sample_rate <= 0.0 {
            return SPECTRUM_FLOOR_DB;
        }
        let pos = freq_hz * SPECTRUM_SIZE as f32 / sample_rate;
        let i = (pos.floor() as usize).min(SPECTRUM_BINS - 1);
        let j = (i + 1).min(SPECTRUM_BINS - 1);
        let t = (pos - i as f32).clamp(0.0, 1.0);
        self.shown[i] + (self.shown[j] - self.shown[i]) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_snapshots_in_time_order() {
        let (mut tap, reader) = spectrum_pair();
        for i in 0..(SPECTRUM_SIZE + 10) {
            tap.push(i as f32);
        }
        tap.publish();
        let mut out = [0.0; SPECTRUM_SIZE];
        reader.snapshot(&mut out);
        assert_eq!(out[0], 10.0);
        assert_eq!(out[SPECTRUM_SIZE - 1], (SPECTRUM_SIZE + 9) as f32);
        tap.push(f32::NAN);
        tap.publish();
        reader.snapshot(&mut out);
        assert_eq!(out[SPECTRUM_SIZE - 1], 0.0);
    }

    #[test]
    fn fft_finds_a_sine_and_ignores_bad_sizes() {
        let sr = 48_000.0;
        let f = bin_hz(43, sr); // an exact bin: ~1007.8 Hz
        let mut samples = [0.0; SPECTRUM_SIZE];
        for (i, s) in samples.iter_mut().enumerate() {
            *s = (2.0 * core::f32::consts::PI * f * i as f32 / sr).sin();
        }
        let mut an = Analyzer::new();
        let shown = an.analyse(&samples, 1.0 / 30.0);
        let (peak_bin, peak_db) = shown
            .iter()
            .enumerate()
            .fold((0, f32::MIN), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
        assert_eq!(peak_bin, 43);
        // The first update rises by the attack fraction from the floor.
        assert!(peak_db > SPECTRUM_FLOOR_DB * (1.0 - SPECTRUM_ATTACK) - 1.0);
        assert!(
            (an.raw()[43]).abs() < 0.5,
            "full-scale sine reads 0 dB: {}",
            an.raw()[43]
        );
        assert!(an.raw()[200] < -60.0);
        for _ in 0..20 {
            an.analyse(&samples, 1.0 / 30.0);
        }
        assert!(an.shown()[43].abs() < 0.5);
        assert!((an.level_at(f, sr) - an.shown()[43]).abs() < 0.5);
        assert_eq!(an.level_at(-1.0, sr), SPECTRUM_FLOOR_DB);
        // Silence decays towards the floor.
        let silence = [0.0; SPECTRUM_SIZE];
        let before = an.shown()[43];
        an.analyse(&silence, 0.1);
        assert!((before - an.shown()[43] - 4.0).abs() < 1e-3);
        let mut re = vec![1.0; 3];
        let mut im = vec![0.0; 3];
        fft_radix2(&mut re, &mut im);
        assert_eq!(re, vec![1.0; 3]);
    }
}
