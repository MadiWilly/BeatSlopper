//! Spectrum analyser for the visualizers: a [`FFT_SIZE`]-point Hann-windowed radix-2 FFT of
//! the scope window, folded into [`BINS`] log-spaced bands from [`MIN_HZ`] to [`MAX_HZ`],
//! each `0..=1` on a dB scale ([`FLOOR_DB`] = 0, 0 dBFS sine = 1), with instant attack and a
//! [`RELEASE_SECONDS`] exponential release so the bars fall smoothly.
//!
//! Hand-written (no FFT crate): the sizes are fixed, so an iterative Cooley-Tukey with a
//! precomputed twiddle table is a few dozen lines, allocation-free after construction.

/// FFT length (the scope window length).
pub const FFT_SIZE: usize = 2048;
/// Number of output bands.
pub const BINS: usize = 64;
/// Lowest band edge, in Hz.
pub const MIN_HZ: f32 = 30.0;
/// Highest band edge, in Hz.
pub const MAX_HZ: f32 = 16_000.0;
/// Level mapped to 0 (a 0 dBFS sine maps to 1).
pub const FLOOR_DB: f32 = -60.0;
/// Release time constant of the band smoothing, in seconds.
pub const RELEASE_SECONDS: f32 = 0.2;
/// Smoothed values below this snap to 0 (so the UI can tell silence).
pub const SILENT: f32 = 1e-3;

/// The analyser state; one per visualizer feed.
#[derive(Debug, Clone)]
pub struct Spectrum {
    sample_rate: f32,
    window: Vec<f32>,
    twiddle_re: Vec<f32>,
    twiddle_im: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    /// FFT bin range `lo..hi` of each band.
    bands: [(usize, usize); BINS],
    raw: [f32; BINS],
    smooth: [f32; BINS],
}

impl Spectrum {
    /// Builds the window, the twiddle table and the band layout for `sample_rate` Hz.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let n = FFT_SIZE;
        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 * (1.0 - (std::f32::consts::TAU * i as f32 / n as f32).cos()))
            .collect();
        let twiddle_re: Vec<f32> = (0..n / 2)
            .map(|k| (-std::f32::consts::TAU * k as f32 / n as f32).cos())
            .collect();
        let twiddle_im: Vec<f32> = (0..n / 2)
            .map(|k| (-std::f32::consts::TAU * k as f32 / n as f32).sin())
            .collect();
        let mut bands = [(0, 0); BINS];
        let max_bin = n / 2;
        for (k, band) in bands.iter_mut().enumerate() {
            let (lo_hz, hi_hz) = band_edges_hz(k);
            let lo = ((lo_hz * n as f32 / sample_rate).round() as usize).clamp(1, max_bin - 1);
            let hi = ((hi_hz * n as f32 / sample_rate).round() as usize).clamp(lo + 1, max_bin);
            *band = (lo, hi);
        }
        Self {
            sample_rate,
            window,
            twiddle_re,
            twiddle_im,
            re: vec![0.0; n],
            im: vec![0.0; n],
            bands,
            raw: [0.0; BINS],
            smooth: [0.0; BINS],
        }
    }

    /// The analyser's sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Analyses the newest [`FFT_SIZE`] samples of `samples` (zero padded at the front when
    /// shorter) and advances the smoothing by `dt_seconds`.
    pub fn analyse(&mut self, samples: &[f32], dt_seconds: f32) {
        let n = FFT_SIZE;
        let start = samples.len().saturating_sub(n);
        let tail = &samples[start..];
        let pad = n - tail.len();
        for i in 0..pad {
            self.re[i] = 0.0;
            self.im[i] = 0.0;
        }
        for (i, s) in tail.iter().enumerate() {
            let s = if s.is_finite() { *s } else { 0.0 };
            self.re[pad + i] = s * self.window[pad + i];
            self.im[pad + i] = 0.0;
        }
        fft_in_place(
            &mut self.re,
            &mut self.im,
            &self.twiddle_re,
            &self.twiddle_im,
        );
        // A full-scale sine under a Hann window peaks at N/4 in magnitude.
        let norm = 4.0 / n as f32;
        for (k, &(lo, hi)) in self.bands.iter().enumerate() {
            let mut peak = 0.0_f32;
            for i in lo..hi {
                let m = (self.re[i] * self.re[i] + self.im[i] * self.im[i]).sqrt() * norm;
                if m > peak {
                    peak = m;
                }
            }
            self.raw[k] = level_to_unit(peak);
        }
        let dt = if dt_seconds.is_finite() {
            dt_seconds.max(0.0)
        } else {
            0.0
        };
        let release = (-dt / RELEASE_SECONDS).exp();
        for (s, &r) in self.smooth.iter_mut().zip(&self.raw) {
            let v = if r >= *s { r } else { r + (*s - r) * release };
            *s = if v < SILENT { 0.0 } else { v };
        }
    }

    /// The smoothed bands, `0..=1` each, [`BINS`] long.
    #[must_use]
    pub fn bins(&self) -> &[f32] {
        &self.smooth
    }

    /// This frame's unsmoothed bands.
    #[must_use]
    pub fn raw(&self) -> &[f32] {
        &self.raw
    }

    /// Index of the loudest smoothed band, if anything is above silence.
    #[must_use]
    pub fn peak_bin(&self) -> Option<usize> {
        let (i, v) = self
            .smooth
            .iter()
            .enumerate()
            .fold(
                (0, 0.0_f32),
                |best, (i, &v)| if v > best.1 { (i, v) } else { best },
            );
        (v > 0.0).then_some(i)
    }

    /// `true` while any band is above silence (the UI keeps repainting).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.smooth.iter().any(|v| *v > 0.0)
    }

    /// Resets the smoothed bands to silence.
    pub fn clear(&mut self) {
        self.raw = [0.0; BINS];
        self.smooth = [0.0; BINS];
    }

    /// The band that contains `hz`, if any.
    #[must_use]
    pub fn band_of(hz: f32) -> Option<usize> {
        (0..BINS).find(|&k| {
            let (lo, hi) = band_edges_hz(k);
            hz >= lo && hz < hi
        })
    }
}

/// Frequency edges `(lo, hi)` of band `k`: log spaced from [`MIN_HZ`] to [`MAX_HZ`].
#[must_use]
pub fn band_edges_hz(k: usize) -> (f32, f32) {
    let ratio = MAX_HZ / MIN_HZ;
    let lo = MIN_HZ * ratio.powf(k as f32 / BINS as f32);
    let hi = MIN_HZ * ratio.powf((k + 1) as f32 / BINS as f32);
    (lo, hi)
}

/// Maps a linear magnitude to `0..=1` on the dB scale ([`FLOOR_DB`] -> 0, 0 dB -> 1).
#[must_use]
pub fn level_to_unit(level: f32) -> f32 {
    if !level.is_finite() || level <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * level.log10();
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// In-place iterative radix-2 FFT. `re.len() == im.len()` must be a power of two and the
/// twiddle tables hold `cos`/`sin(-2 pi k / n)` for `k < n / 2`.
pub fn fft_in_place(re: &mut [f32], im: &mut [f32], twiddle_re: &[f32], twiddle_im: &[f32]) {
    let n = re.len().min(im.len());
    if n < 2 || !n.is_power_of_two() || twiddle_re.len() < n / 2 || twiddle_im.len() < n / 2 {
        return;
    }
    // Bit-reversal permutation.
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..half {
                let (wr, wi) = (twiddle_re[k * step], twiddle_im[k * step]);
                let (a, b) = (start + k, start + k + half);
                let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, amplitude: f32, sample_rate: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amplitude * (std::f32::consts::TAU * hz * i as f32 / sample_rate).sin())
            .collect()
    }

    #[test]
    fn fft_of_a_bin_centred_sine_lands_on_that_bin() {
        let n = 64;
        let tw_re: Vec<f32> = (0..n / 2)
            .map(|k| (-std::f32::consts::TAU * k as f32 / n as f32).cos())
            .collect();
        let tw_im: Vec<f32> = (0..n / 2)
            .map(|k| (-std::f32::consts::TAU * k as f32 / n as f32).sin())
            .collect();
        // 5 cycles over 64 samples: energy in bin 5 (and its mirror 59).
        let mut re: Vec<f32> = (0..n)
            .map(|i| (std::f32::consts::TAU * 5.0 * i as f32 / n as f32).cos())
            .collect();
        let mut im = vec![0.0; n];
        fft_in_place(&mut re, &mut im, &tw_re, &tw_im);
        let mags: Vec<f32> = re
            .iter()
            .zip(&im)
            .map(|(r, i)| (r * r + i * i).sqrt())
            .collect();
        assert!((mags[5] - 32.0).abs() < 1e-3, "{}", mags[5]);
        assert!((mags[59] - 32.0).abs() < 1e-3);
        for (k, m) in mags.iter().enumerate() {
            if k != 5 && k != 59 {
                assert!(*m < 1e-3, "bin {k} = {m}");
            }
        }
    }

    #[test]
    fn a_one_khz_sine_peaks_in_the_right_band_at_full_scale() {
        let sr = 48_000.0;
        let mut spectrum = Spectrum::new(sr);
        spectrum.analyse(&sine(1000.0, 1.0, sr, FFT_SIZE), 1.0 / 60.0);
        let expected = Spectrum::band_of(1000.0).expect("1 kHz is inside the range");
        assert_eq!(spectrum.peak_bin(), Some(expected));
        let v = spectrum.bins()[expected];
        assert!(v > 0.95, "full scale should read about 1, got {v}");
        // Far-away bands are much quieter (Hann sidelobes are below -30 dB).
        assert!(spectrum.bins()[expected + 12] < 0.5);
        assert!(spectrum.bins()[expected.saturating_sub(12)] < 0.5);
        assert!(spectrum.is_active());

        // A -20 dBFS sine reads about two thirds of the scale.
        let mut quiet = Spectrum::new(sr);
        quiet.analyse(&sine(1000.0, 0.1, sr, FFT_SIZE), 0.0);
        let q = quiet.bins()[expected];
        assert!((q - 2.0 / 3.0).abs() < 0.05, "{q}");
    }

    #[test]
    fn silence_gives_zeros_and_short_or_bad_input_is_tolerated() {
        let mut spectrum = Spectrum::new(48_000.0);
        spectrum.analyse(&[0.0; FFT_SIZE], 0.02);
        assert!(spectrum.bins().iter().all(|v| *v == 0.0));
        assert_eq!(spectrum.peak_bin(), None);
        assert!(!spectrum.is_active());
        spectrum.analyse(&[], 0.02);
        assert!(spectrum.bins().iter().all(|v| *v == 0.0));
        spectrum.analyse(&[f32::NAN, f32::INFINITY, 0.5], f32::NAN);
        assert!(spectrum.bins().iter().all(|v| v.is_finite()));
        let short = sine(1000.0, 1.0, 48_000.0, 500);
        spectrum.analyse(&short, 0.0);
        assert!(
            spectrum.is_active(),
            "a short window is zero padded, not dropped"
        );
    }

    #[test]
    fn release_falls_over_time_and_attack_is_instant() {
        let sr = 48_000.0;
        let mut spectrum = Spectrum::new(sr);
        let tone = sine(1000.0, 1.0, sr, FFT_SIZE);
        spectrum.analyse(&tone, 0.0);
        let band = Spectrum::band_of(1000.0).unwrap();
        let loud = spectrum.bins()[band];
        spectrum.analyse(&[0.0; FFT_SIZE], 0.2);
        let after = spectrum.bins()[band];
        assert!(
            after > 0.0 && after < loud,
            "falls but does not vanish: {after}"
        );
        assert!((after - loud * (-1.0_f32).exp()).abs() < 1e-3);
        for _ in 0..30 {
            spectrum.analyse(&[0.0; FFT_SIZE], 0.2);
        }
        assert!(!spectrum.is_active(), "silent after a few seconds");
        spectrum.analyse(&tone, 0.0);
        assert!(
            (spectrum.bins()[band] - loud).abs() < 1e-6,
            "attack is instant"
        );
        spectrum.clear();
        assert!(!spectrum.is_active());
    }

    #[test]
    fn bands_are_log_spaced_and_cover_the_range() {
        let (lo0, _) = band_edges_hz(0);
        let (_, hi_last) = band_edges_hz(BINS - 1);
        assert!((lo0 - MIN_HZ).abs() < 1e-3);
        assert!((hi_last - MAX_HZ).abs() < 1e-1);
        let ratio0 = band_edges_hz(0).1 / band_edges_hz(0).0;
        let ratio40 = band_edges_hz(40).1 / band_edges_hz(40).0;
        assert!((ratio0 - ratio40).abs() < 1e-4, "constant ratio per band");
        assert_eq!(Spectrum::band_of(10.0), None);
        assert_eq!(Spectrum::band_of(20_000.0), None);
        let spectrum = Spectrum::new(48_000.0);
        for (lo, hi) in spectrum.bands {
            assert!(lo < hi && hi <= FFT_SIZE / 2);
        }
        assert_eq!(level_to_unit(1.0), 1.0);
        assert_eq!(level_to_unit(0.0), 0.0);
        assert!((level_to_unit(0.001) - 0.0).abs() < 1e-6);
        assert!((level_to_unit(0.1) - 2.0 / 3.0).abs() < 1e-5);
    }
}
