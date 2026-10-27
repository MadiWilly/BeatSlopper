//! Karplus-Strong plucked string.
//!
//! A delay line one period long with a one-pole lowpass and a feedback gain just below `1.0`
//! in its loop. Plucking adds a burst of noise, one period long, lowpassed by the brightness
//! control and made zero-mean, into the loop; the loop then rings at the delay's frequency
//! while the lowpass eats the harmonics (as a real string does) and the feedback gain sets
//! the decay.
//!
//! - The delay line is allocated **once** in [`Pluck::new`], sized for [`LOWEST_NOTE`] at the
//!   construction sample rate (about 1 500 samples at 48 kHz); nothing allocates afterwards.
//! - The read position is fractional (linear interpolation) and compensated for the phase
//!   delay of the loop lowpass, so the pitch is exact.
//! - Damping maps to both the loop lowpass cutoff (a multiple of the fundamental: 64x at 0 =
//!   open, 1.5x at 1 = dull) and the decay time ([`T60_LONGEST`] seconds at 0 down to
//!   [`T60_SHORTEST`] at 1, independent of pitch). Brightness sets the excitation lowpass the
//!   same way (0.5x the fundamental at 0, raw noise at 1).
//! - Re-plucking adds a new burst without clearing the line (the period it lands on is first
//!   halved, as a pick damps the string), so repeated notes stay smooth and do not pile up.

use crate::dsp::flush_denormal;
use crate::synth::noise::Noise;
use crate::synth::note_to_hz;

/// Lowest MIDI note the delay line is sized for (C1, about 32.7 Hz).
pub const LOWEST_NOTE: f32 = 24.0;
/// Decay time to -60 dB at damping 0, seconds.
pub const T60_LONGEST: f32 = 8.0;
/// Decay time to -60 dB at damping 1, seconds.
pub const T60_SHORTEST: f32 = 0.25;
/// Loop lowpass cutoff as a multiple of the fundamental at damping 0 (practically open) ...
const LOOP_RATIO_OPEN: f32 = 64.0;
/// ... and at damping 1 (dull: the second harmonic is already 3 dB down per circulation).
const LOOP_RATIO_DULL: f32 = 1.5;
/// Excitation lowpass cutoff as a multiple of the fundamental at brightness 0 (dark) ...
const EXC_RATIO_DARK: f32 = 0.5;
/// ... and at brightness 1 (far above the audio band: raw noise).
const EXC_RATIO_RAW: f32 = 2048.0;
/// Cap on the level compensation of a dark excitation burst.
const MAX_EXC_GAIN: f32 = 4.0;
/// RMS a pluck's burst is scaled to, measured after the loop lowpasses of the first
/// [`LEVEL_WINDOW`] seconds (see [`Pluck::pluck`]).
pub const PLUCK_RMS: f32 = 1.0;
/// Most trips through the loop lowpass the level measurement accounts for.
const LEVEL_PASSES: usize = 32;
/// The level measurement accounts for the loop trips of this long a window, seconds (so a
/// high note, which circulates more often and loses its overtones sooner, is measured after
/// more trips than a low one).
const LEVEL_WINDOW: f32 = 0.03;
/// Repetitions of the burst run through the measurement filters (the last one is measured).
const LEVEL_REPEATS: usize = 3;
/// Cap on the level-normalising gain of a burst.
const MAX_LEVEL_GAIN: f32 = 16.0;
/// Ceiling of the soft clip on the level-normalised burst.
const MAX_BURST_PEAK: f32 = 4.0;
/// Gain on the period a new pluck lands on (the pick damps a string that is still ringing).
const PICK_DAMP: f32 = 0.5;
/// Shortest delay (samples) the loop accepts.
const MIN_DELAY: f32 = 2.0;
/// Time constant of the loop's DC blocker, in circulations of the loop. Expressed per period
/// (not in samples) so its phase lead is the same tiny fraction of the period at every pitch
/// (compensated at the fundamental; the second harmonic lands about 0.2 % flat, far below a
/// real string's inharmonicity).
const DC_BLOCK_PERIODS: f32 = 10.0;

/// One plucked string. Real-time safe after construction: no allocation, no panics.
#[derive(Debug, Clone, PartialEq)]
pub struct Pluck {
    buf: Vec<f32>,
    write: usize,
    sample_rate: f32,
    /// Target frequency, Hz.
    hz: f32,
    /// Loop delay in samples (one period), before compensation.
    period: f32,
    /// Delay-line read distance in samples (period minus the loop filter's phase delay).
    line_delay: f32,
    damping: f32,
    bright: f32,
    lp_coeff: f32,
    lp_state: f32,
    /// DC blocker `y = g (x - x1) + r y1` in the loop: a zero-mean burst read through a
    /// fractional loop length still leaves a little DC, which the lowpass would pass forever.
    dc_r: f32,
    /// `(1 + r) / 2`: normalises the blocker's peak gain (at Nyquist) to exactly 1 so the
    /// loop gain never exceeds the feedback.
    dc_g: f32,
    dc_x1: f32,
    dc_y1: f32,
    /// Gain applied once per circulation of the loop.
    feedback: f32,
    exc_coeff: f32,
    exc_gain: f32,
    noise: Noise,
}

impl Pluck {
    /// Allocates a string for `sample_rate` (the delay line fits [`LOWEST_NOTE`]), tuned to
    /// 440 Hz with damping 0.5 and brightness 0.7.
    #[must_use]
    pub fn new(sample_rate: f32, seed: u32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let len = ((sample_rate / note_to_hz(LOWEST_NOTE)).ceil() as usize + 4).max(16);
        let mut p = Self {
            buf: vec![0.0; len],
            write: 0,
            sample_rate,
            hz: 440.0,
            period: 1.0,
            line_delay: 1.0,
            damping: 0.5,
            bright: 0.7,
            lp_coeff: 1.0,
            lp_state: 0.0,
            dc_r: 0.999,
            dc_g: 0.9995,
            dc_x1: 0.0,
            dc_y1: 0.0,
            feedback: 0.99,
            exc_coeff: 1.0,
            exc_gain: 1.0,
            noise: Noise::new(seed),
        };
        p.set_damping(0.5);
        p.set_brightness(0.7);
        p.set_freq(440.0);
        p
    }

    /// Length of the delay line in samples.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Current frequency in Hz (after clamping to the delay line's range).
    #[must_use]
    pub fn freq(&self) -> f32 {
        self.hz
    }

    /// Current loop delay in samples (one period of the string).
    #[must_use]
    pub fn period(&self) -> f32 {
        self.period
    }

    /// Sets the string frequency in Hz. Values below what the delay line can hold (about
    /// [`LOWEST_NOTE`]) or above `sample_rate / 2` are clamped; non-finite values are ignored.
    pub fn set_freq(&mut self, hz: f32) {
        if !hz.is_finite() || hz <= 0.0 {
            return;
        }
        let max_period = (self.buf.len() - 2) as f32;
        self.period = (self.sample_rate / hz).clamp(MIN_DELAY, max_period);
        self.hz = self.sample_rate / self.period;
        self.update_delay();
    }

    /// Sets the damping, `0.0..=1.0` (0 = bright and long, 1 = dull and short).
    pub fn set_damping(&mut self, damping: f32) {
        self.damping = if damping.is_finite() {
            damping.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.update_delay();
    }

    /// Sets the excitation brightness, `0.0..=1.0` (1 = raw noise burst, 0 = dark).
    pub fn set_brightness(&mut self, bright: f32) {
        self.bright = if bright.is_finite() {
            bright.clamp(0.0, 1.0)
        } else {
            0.7
        };
        self.update_delay();
    }

    /// One-pole lowpass coefficient for a cutoff of `ratio` times the fundamental.
    fn coeff_for_ratio(&self, ratio: f32) -> f32 {
        let wc = core::f32::consts::TAU * self.hz * ratio / self.sample_rate;
        (1.0 - (-wc).exp()).clamp(1.0e-4, 1.0)
    }

    /// Magnitude of a one-pole lowpass with coefficient `c` at the fundamental.
    fn one_pole_gain(&self, c: f32) -> f32 {
        let w = core::f32::consts::TAU * self.hz / self.sample_rate;
        let a = 1.0 - c;
        c / (1.0 - 2.0 * a * w.cos() + a * a).sqrt()
    }

    /// Recomputes everything that depends on the pitch, damping or brightness: the loop and
    /// excitation lowpass coefficients (cutoffs are multiples of the fundamental, so the
    /// controls mean the same thing on every note), the DC blocker pole, the read distance
    /// (one period minus the phase delay of the loop lowpass and DC blocker at the
    /// fundamental) and the feedback gain for the current decay time.
    fn update_delay(&mut self) {
        let d = self.damping;
        let loop_ratio = LOOP_RATIO_OPEN * (LOOP_RATIO_DULL / LOOP_RATIO_OPEN).powf(d);
        let c = self.coeff_for_ratio(loop_ratio);
        self.lp_coeff = c;
        let exc_ratio = EXC_RATIO_DARK * (EXC_RATIO_RAW / EXC_RATIO_DARK).powf(self.bright);
        let ce = self.coeff_for_ratio(exc_ratio);
        self.exc_coeff = ce;
        // Lift a dark burst so its fundamental keeps roughly the level of a raw one (capped).
        self.exc_gain = (1.0 / self.one_pole_gain(ce).max(1.0e-3)).min(MAX_EXC_GAIN);

        let w = core::f32::consts::TAU * self.hz / self.sample_rate;
        let (sin_w, cos_w) = w.sin_cos();
        // One-pole lowpass c / (1 - a z^-1): phase lag -> positive delay.
        let a = 1.0 - c;
        let lp_delay = (a * sin_w).atan2(1.0 - a * cos_w) / w;
        // DC blocker (1 - z^-1) / (1 - r z^-1): phase lead -> negative delay.
        let r = (-1.0 / (DC_BLOCK_PERIODS * self.period)).exp();
        self.dc_r = r;
        self.dc_g = 0.5 * (1.0 + r);
        let hp_phase = sin_w.atan2(1.0 - cos_w) - (r * sin_w).atan2(1.0 - r * cos_w);
        let hp_delay = -hp_phase / w;
        let max_delay = (self.buf.len() - 2) as f32;
        self.line_delay = (self.period - lp_delay - hp_delay).clamp(1.0, max_delay);
        let t60 = T60_LONGEST * (T60_SHORTEST / T60_LONGEST).powf(d);
        // -60 dB after t60 * hz circulations.
        self.feedback = 10.0_f32.powf(-3.0 / (t60 * self.hz)).clamp(0.0, 0.999_9);
    }

    /// Plucks the string: adds a noise burst one period long (lowpassed by the brightness,
    /// made zero-mean so the loop carries no DC, level-normalised, see below) onto the most
    /// recently written period of the line. The line is not cleared, so a string that is
    /// still ringing is re-excited smoothly (its last period halved first, see
    /// [`PICK_DAMP`]). Costs a few periods of samples, no allocation.
    ///
    /// **Level:** a raw burst spreads its energy over every harmonic up to Nyquist, and the
    /// loop lowpass removes most of it within a few circulations, so without care a dull or
    /// low string came out 20 dB or more under a bright one, and every pluck drew a
    /// different random level. The burst is therefore scaled so that what survives the
    /// first [`LEVEL_WINDOW`] seconds of trips through the loop lowpass (the part of the note
    /// the ear hears as its level) has the RMS [`PLUCK_RMS`], whatever the damping, brightness, pitch or
    /// noise draw. The gain is capped at [`MAX_LEVEL_GAIN`], and the scaled burst goes through
    /// a soft clip that keeps its peak under [`MAX_BURST_PEAK`].
    pub fn pluck(&mut self) {
        let len = self.buf.len();
        let n = (self.period.round() as usize).clamp(2, len);
        let start = (self.write + len - n) % len;
        // The burst is drawn several times from the same noise state (mean, level, write), so
        // it needs no buffer of its own.
        let seed = self.noise;
        let (exc_coeff, exc_gain, lp_coeff) = (self.exc_coeff, self.exc_gain, self.lp_coeff);
        let burst = |noise: &mut Noise, state: &mut f32| {
            *state += exc_coeff * (noise.white() - *state);
            *state * exc_gain
        };

        let (mut noise, mut state) = (seed, 0.0_f32);
        let mut sum = 0.0_f32;
        for _ in 0..n {
            sum += burst(&mut noise, &mut state);
        }
        let mean = sum / n as f32;

        // Level: the zero-mean burst, repeated (the loop is periodic), through one cascaded copy
        // of the loop lowpass per trip in LEVEL_WINDOW; measured on the last repetition, once the
        // filters have settled.
        let passes = ((self.hz * LEVEL_WINDOW).round() as usize).clamp(1, LEVEL_PASSES);
        let mut lp = [0.0_f32; LEVEL_PASSES];
        let mut energy = 0.0_f32;
        for rep in 0..LEVEL_REPEATS {
            let (mut noise, mut state) = (seed, 0.0_f32);
            for _ in 0..n {
                let mut x = burst(&mut noise, &mut state) - mean;
                for s in &mut lp[..passes] {
                    *s += lp_coeff * (x - *s);
                    x = *s;
                }
                if rep == LEVEL_REPEATS - 1 {
                    energy += x * x;
                }
            }
        }
        let level = (energy / n as f32).sqrt();
        let gain = if level > 1.0e-9 {
            (PLUCK_RMS / level).min(MAX_LEVEL_GAIN)
        } else {
            1.0
        };

        // Write the scaled burst through a soft clip (a bright burst into a damped string is
        // mostly transient, so a large gain would otherwise spike its first period) onto the
        // ringing period, damped first as a pick damps the string (repeated notes would
        // otherwise pile up), then take out the little mean the clip leaves.
        let (mut noise, mut state) = (seed, 0.0_f32);
        let mut sum = 0.0_f32;
        for k in 0..n {
            let x = (burst(&mut noise, &mut state) - mean) * gain;
            let s = MAX_BURST_PEAK * (x / MAX_BURST_PEAK).tanh();
            let i = (start + k) % len;
            self.buf[i] = self.buf[i] * PICK_DAMP + s;
            sum += s;
        }
        self.noise = noise;
        let clip_mean = sum / n as f32;
        for k in 0..n {
            self.buf[(start + k) % len] -= clip_mean;
        }
    }

    /// Silences the string instantly (clears the delay line and filter state). Only for
    /// idle voices: it is the one operation here that could click.
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.lp_state = 0.0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
    }

    /// Produces the next sample of the string.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let len = self.buf.len();
        let d = self.line_delay;
        let di = d as usize;
        let frac = d - di as f32;
        let i0 = (self.write + len - di) % len;
        let i1 = if i0 == 0 { len - 1 } else { i0 - 1 };
        let a = self.buf[i0];
        let b = self.buf[i1];
        let out = a + frac * (b - a);

        self.lp_state = flush_denormal(self.lp_state + self.lp_coeff * (out - self.lp_state));
        let hp = self.dc_g * (self.lp_state - self.dc_x1) + self.dc_r * self.dc_y1;
        self.dc_x1 = self.lp_state;
        self.dc_y1 = flush_denormal(hp);
        self.buf[self.write] = hp * self.feedback;
        self.write += 1;
        if self.write == len {
            self.write = 0;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn render(p: &mut Pluck, n: usize) -> Vec<f32> {
        (0..n).map(|_| p.tick()).collect()
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
    }

    /// Autocorrelation pitch estimate over lags around the expected period.
    fn pitch(buf: &[f32], lo_hz: f32, hi_hz: f32) -> f32 {
        let min_lag = (SR / hi_hz) as usize;
        let max_lag = (SR / lo_hz) as usize;
        let n = buf.len() - max_lag - 1;
        let mut best = (0, f32::MIN);
        for lag in min_lag..=max_lag {
            let c: f32 = (0..n).map(|i| buf[i] * buf[i + lag]).sum();
            if c > best.1 {
                best = (lag, c);
            }
        }
        // Parabolic refinement.
        let lag = best.0;
        let corr = |l: usize| -> f32 { (0..n).map(|i| buf[i] * buf[i + l]).sum() };
        let (c0, c1, c2) = (corr(lag - 1), best.1, corr(lag + 1));
        let denom = c0 - 2.0 * c1 + c2;
        let delta = if denom.abs() > 1e-9 {
            0.5 * (c0 - c2) / denom
        } else {
            0.0
        };
        SR / (lag as f32 + delta)
    }

    #[test]
    fn sized_for_the_lowest_note_at_construction() {
        let p = Pluck::new(SR, 1);
        assert!(
            p.capacity() >= 1468 && p.capacity() < 1600,
            "{}",
            p.capacity()
        );
        let p96 = Pluck::new(96_000.0, 1);
        assert!(p96.capacity() >= 2936);
        assert!(Pluck::new(f32::NAN, 1).capacity() >= 1468);
    }

    #[test]
    fn rings_at_the_requested_pitch_and_decays() {
        for hz in [110.0, 220.0, 440.0, 1000.0] {
            let mut p = Pluck::new(SR, 7);
            p.set_freq(hz);
            p.pluck();
            let out = render(&mut p, (SR * 3.0) as usize);
            assert!(out.iter().all(|s| s.is_finite()));
            let early = rms(&out[..4800]);
            let late = rms(&out[(SR * 2.9) as usize..]);
            assert!(early > 0.05, "{hz} Hz early rms {early}");
            assert!(late < early * 0.01, "{hz} Hz late {late} early {early}");
            let f = pitch(&out[4800..24_000], hz * 0.8, hz * 1.25);
            assert!((f - hz).abs() < hz * 0.01, "{hz} Hz estimated {f}");
        }
    }

    #[test]
    fn damping_shortens_and_brightness_brightens() {
        let decay = |damping: f32| {
            let mut p = Pluck::new(SR, 3);
            p.set_freq(220.0);
            p.set_damping(damping);
            p.pluck();
            let out = render(&mut p, SR as usize);
            rms(&out[40_000..]) / rms(&out[..4800])
        };
        assert!(decay(0.9) < decay(0.1) * 0.2);

        let bright = |b: f32| {
            let mut p = Pluck::new(SR, 3);
            p.set_freq(220.0);
            p.set_brightness(b);
            p.pluck();
            let out = render(&mut p, 2400);
            let hp: Vec<f32> = out.windows(2).map(|w| w[1] - w[0]).collect();
            rms(&hp) / rms(&out)
        };
        assert!(bright(0.0) < bright(1.0) * 0.5);
    }

    /// The burst is level-normalised: across damping, brightness and C2..C5 the first 300 ms
    /// stay within a narrow band (un-normalised, a dull low string came out more than 20 dB
    /// under a bright one), and different noise draws of the same pluck sound equally loud.
    #[test]
    fn pluck_level_is_consistent_across_settings_pitches_and_draws() {
        let level = |damping: f32, bright: f32, note: f32, seed: u32| {
            let mut p = Pluck::new(SR, seed);
            p.set_damping(damping);
            p.set_brightness(bright);
            p.set_freq(note_to_hz(note));
            p.pluck();
            let out = render(&mut p, (SR * 0.3) as usize);
            assert!(
                out.iter()
                    .all(|s| s.is_finite() && s.abs() <= 1.5 * MAX_BURST_PEAK)
            );
            20.0 * rms(&out).log10()
        };
        let mut all = Vec::new();
        for damping in [0.0, 0.5, 0.9] {
            for bright in [0.3, 0.7, 1.0] {
                for note in [36.0, 48.0, 60.0, 72.0] {
                    let draws: Vec<f32> = (1..6)
                        .map(|s| level(damping, bright, note, s * 7919))
                        .collect();
                    let lo = draws.iter().copied().fold(f32::MAX, f32::min);
                    let hi = draws.iter().copied().fold(f32::MIN, f32::max);
                    assert!(
                        hi - lo < 4.5,
                        "damping {damping} bright {bright} note {note}: {draws:?}"
                    );
                    all.extend(draws);
                }
            }
        }
        let lo = all.iter().copied().fold(f32::MAX, f32::min);
        let hi = all.iter().copied().fold(f32::MIN, f32::max);
        assert!(hi - lo < 13.0, "level band {lo:.1} .. {hi:.1} dB");
        assert!(lo > -16.0, "quietest pluck {lo:.1} dB");
    }

    #[test]
    fn replucking_does_not_clear_and_weird_inputs_are_safe() {
        let mut p = Pluck::new(SR, 5);
        p.set_freq(440.0);
        p.pluck();
        let _ = render(&mut p, 2000);
        let before: Vec<f32> = p.buf.clone();
        p.pluck();
        let changed: Vec<usize> = (0..before.len())
            .filter(|&i| p.buf[i] != before[i])
            .collect();
        assert!(
            (100..=110).contains(&changed.len()),
            "burst one period long: {}",
            changed.len()
        );
        assert!(before.iter().any(|s| s.abs() > 1.0e-3), "still ringing");
        // The period the burst lands on is damped by PICK_DAMP; the rest of the line is kept.
        let burst: Vec<f32> = changed
            .iter()
            .map(|&i| p.buf[i] - before[i] * PICK_DAMP)
            .collect();
        let mean = burst.iter().sum::<f32>() / burst.len() as f32;
        assert!(mean.abs() < 1e-4, "burst is zero-mean: {mean}");
        assert!(p.tick().is_finite());
        p.set_freq(f32::NAN);
        p.set_freq(-5.0);
        p.set_freq(1.0);
        assert!(p.period() <= (p.capacity() - 2) as f32);
        p.set_freq(1.0e7);
        assert!(p.period() >= MIN_DELAY);
        p.set_damping(f32::NAN);
        p.set_brightness(f32::INFINITY);
        for _ in 0..10_000 {
            assert!(p.tick().is_finite());
        }
        p.clear();
        assert_eq!(p.tick(), 0.0);
    }
}
