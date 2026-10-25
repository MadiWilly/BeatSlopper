//! Band-limited oscillator: PolyBLEP saw and square, triangle by integrating the PolyBLEP
//! square, plain sine.
//!
//! PolyBLEP (polynomial band-limited step) replaces the hard edge of a naive saw/square with a
//! two-sample polynomial correction, which removes most of the aliasing for a tiny cost. The
//! triangle is a leaky integration of the band-limited square, so it inherits the same
//! band-limiting. Phase is kept in turns (`0.0..1.0`) as `f64` so it never drifts.
//!
//! M8: the square has a variable pulse width ([`Oscillator::set_pulse_width`], both edges
//! PolyBLEP-corrected, DC removed so narrow pulses do not thump through the amp envelope; the
//! triangle always integrates the 50 % square) and a phase-modulation input
//! ([`Oscillator::tick_pm`]) for FM.

use core::f64::consts::TAU;

use crate::dsp::flush_denormal;

/// Oscillator waveform. The index order matches `ParamId::Osc1Wave` / `Osc2Wave`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Wave {
    /// Plain sine (no band-limiting needed).
    Sine,
    /// Rising sawtooth, PolyBLEP band-limited.
    #[default]
    Saw,
    /// 50 % square, PolyBLEP band-limited on both edges.
    Square,
    /// Triangle, integrated from the band-limited square.
    Triangle,
}

impl Wave {
    /// Every waveform in parameter-index order.
    pub const ALL: [Wave; 4] = [Wave::Sine, Wave::Saw, Wave::Square, Wave::Triangle];

    /// Waveform for a parameter index; out-of-range indices clamp to the last waveform.
    #[must_use]
    pub const fn from_index(index: usize) -> Wave {
        match index {
            0 => Wave::Sine,
            1 => Wave::Saw,
            2 => Wave::Square,
            _ => Wave::Triangle,
        }
    }

    /// Waveform for a parameter value (rounded to the nearest index, clamped). Non-finite
    /// values give the default waveform.
    #[must_use]
    pub fn from_param(value: f32) -> Wave {
        if !value.is_finite() {
            return Wave::default();
        }
        Wave::from_index(value.round().clamp(0.0, 3.0) as usize)
    }

    /// Parameter index of this waveform.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Short display name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Wave::Sine => "Sine",
            Wave::Saw => "Saw",
            Wave::Square => "Square",
            Wave::Triangle => "Tri",
        }
    }

    /// Phase (in turns) at which the waveform crosses zero going up. A reset starts a note
    /// here so there is no DC step at note-on (the square has no such point).
    const fn start_phase(self) -> f64 {
        match self {
            Wave::Sine | Wave::Square => 0.0,
            Wave::Saw => 0.5,
            Wave::Triangle => 0.25,
        }
    }
}

/// Largest phase increment accepted (turns per sample). Above about 0.45 the PolyBLEP
/// corrections of the two edges overlap and the waveform degenerates, so the frequency is
/// clamped to `0.45 * sample_rate`.
pub const MAX_PHASE_INC: f64 = 0.45;

/// Leak of the triangle integrator per sample: a one-pole high-pass a few Hz wide that removes
/// any DC the integration accumulates (time constant about 2 000 samples).
const TRI_LEAK: f32 = 0.9995;

/// Narrowest pulse width accepted (fraction of the period the square spends high).
pub const MIN_PULSE_WIDTH: f32 = 0.05;
/// Widest pulse width accepted.
pub const MAX_PULSE_WIDTH: f32 = 0.95;

/// PolyBLEP residual for a unit step at phase 0 (`t` in turns, `dt` = phase increment).
#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Band-limited 50 % square at phase `t` (both edges corrected).
#[inline]
fn blep_square(t: f32, dt: f32) -> f32 {
    let mut s = if t < 0.5 { 1.0 } else { -1.0 };
    s += poly_blep(t, dt);
    let t2 = if t >= 0.5 { t - 0.5 } else { t + 0.5 };
    s - poly_blep(t2, dt)
}

/// Band-limited pulse at phase `t` with width `pw` (high for `t < pw`), both edges corrected,
/// DC removed (`2 pw - 1` subtracted) so the average is zero at every width. At `pw = 0.5`
/// this is exactly [`blep_square`].
#[inline]
fn blep_pulse(t: f32, dt: f32, pw: f32) -> f32 {
    let mut s = if t < pw { 1.0 } else { -1.0 };
    s += poly_blep(t, dt);
    let t2 = if t >= pw { t - pw } else { t + 1.0 - pw };
    s - poly_blep(t2, dt) - (2.0 * pw - 1.0)
}

/// Naive (not band-limited) triangle at phase `t`: `-1` at 0, `+1` at 0.5.
#[inline]
fn naive_triangle(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t - 1.0
    } else {
        3.0 - 4.0 * t
    }
}

/// Band-limited oscillator with selectable [`Wave`].
///
/// Real-time safe: `Copy`, no allocation, no panics. Output is nominally `-1.0..=1.0`; the
/// PolyBLEP corrections may overshoot by a few percent right at an edge, and a square with a
/// pulse width away from 0.5 swings to `2 * max(pw, 1 - pw)` on its short side because its DC
/// is removed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oscillator {
    wave: Wave,
    /// Phase in turns, `0.0..1.0`.
    phase: f64,
    /// Phase increment per sample (turns).
    inc: f64,
    inv_sample_rate: f64,
    /// Integrator state for the triangle.
    tri: f32,
    /// Pulse width of the square, `MIN_PULSE_WIDTH..=MAX_PULSE_WIDTH`.
    pulse_width: f32,
}

impl Oscillator {
    /// Creates an oscillator at `freq_hz` for `sample_rate`, reset to the start phase of `wave`.
    #[must_use]
    pub fn new(wave: Wave, freq_hz: f32, sample_rate: f32) -> Self {
        let inv_sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            1.0 / f64::from(sample_rate)
        } else {
            1.0 / 48_000.0
        };
        let mut osc = Self {
            wave,
            phase: 0.0,
            inc: 0.0,
            inv_sample_rate,
            tri: 0.0,
            pulse_width: 0.5,
        };
        osc.set_freq(freq_hz);
        osc.reset();
        osc
    }

    /// Pulse width of the square wave (fraction of the period spent high).
    #[must_use]
    pub fn pulse_width(&self) -> f32 {
        self.pulse_width
    }

    /// Sets the square's pulse width, clamped to `MIN_PULSE_WIDTH..=MAX_PULSE_WIDTH`
    /// (non-finite -> 0.5). Only the square uses it; the triangle stays symmetric.
    pub fn set_pulse_width(&mut self, width: f32) {
        self.pulse_width = if width.is_finite() {
            width.clamp(MIN_PULSE_WIDTH, MAX_PULSE_WIDTH)
        } else {
            0.5
        };
    }

    /// Current waveform.
    #[must_use]
    pub fn wave(&self) -> Wave {
        self.wave
    }

    /// Switches the waveform without disturbing the phase (the triangle integrator is
    /// re-seeded from the current phase so it does not start with a DC offset).
    pub fn set_wave(&mut self, wave: Wave) {
        if wave != self.wave {
            self.wave = wave;
            self.tri = naive_triangle(self.phase as f32);
        }
    }

    /// Sets the frequency in Hz without resetting the phase (click-free). Negative or
    /// non-finite values give silence (zero increment); the frequency is clamped to
    /// `MAX_PHASE_INC * sample_rate`.
    #[inline]
    pub fn set_freq(&mut self, freq_hz: f32) {
        self.inc = if freq_hz.is_finite() {
            (f64::from(freq_hz) * self.inv_sample_rate).clamp(0.0, MAX_PHASE_INC)
        } else {
            0.0
        };
    }

    /// Current frequency in Hz.
    #[must_use]
    pub fn freq(&self) -> f32 {
        (self.inc / self.inv_sample_rate) as f32
    }

    /// Current phase in turns, `0.0..1.0`.
    #[must_use]
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Restarts the waveform at its zero-crossing start phase (see [`Wave`]).
    pub fn reset(&mut self) {
        self.phase = self.wave.start_phase();
        self.tri = naive_triangle(self.phase as f32);
    }

    /// Jumps to `phase` (turns, wrapped into `0.0..1.0`; non-finite -> 0) and re-seeds the
    /// triangle integrator. Clicks if the voice is audible; meant for note starts.
    pub fn set_phase(&mut self, phase: f64) {
        let mut t = phase - phase.floor();
        if !t.is_finite() || !(0.0..1.0).contains(&t) {
            t = 0.0;
        }
        self.phase = t;
        self.tri = naive_triangle(t as f32);
    }

    /// Produces the next sample and advances the phase.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let s = self.sample_at(self.phase);
        self.advance();
        s
    }

    /// Produces the next sample with `phase_offset` (in turns, any sign) added to the phase
    /// for this sample only, then advances the phase as usual: the phase-modulation input
    /// used for FM. `tick_pm(0.0)` is bit-identical to [`tick`](Self::tick).
    #[inline]
    pub fn tick_pm(&mut self, phase_offset: f32) -> f32 {
        let mut t = self.phase + f64::from(phase_offset);
        if !(0.0..1.0).contains(&t) {
            t -= t.floor();
            if !t.is_finite() {
                t = 0.0;
            }
        }
        let s = self.sample_at(t);
        self.advance();
        s
    }

    /// Waveform value at phase `phase` (turns, `0.0..1.0`); updates the triangle integrator.
    #[inline]
    fn sample_at(&mut self, phase: f64) -> f32 {
        let t = phase as f32;
        let dt = self.inc as f32;
        match self.wave {
            Wave::Sine => (phase * TAU).sin() as f32,
            Wave::Saw => 2.0 * t - 1.0 - poly_blep(t, dt),
            Wave::Square => blep_pulse(t, dt, self.pulse_width),
            Wave::Triangle => {
                // Integrate the band-limited square: over half a period (0.5 / dt samples) the
                // state travels 4 * dt * 0.5 / dt = 2, i.e. from -1 to +1.
                // The clamp only acts near Nyquist, where a few coarse integration steps per
                // period would otherwise overshoot.
                let sq = blep_square(t, dt);
                self.tri = flush_denormal((self.tri * TRI_LEAK + 4.0 * dt * sq).clamp(-1.0, 1.0));
                self.tri
            }
        }
    }

    #[inline]
    fn advance(&mut self) {
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn render(osc: &mut Oscillator, n: usize) -> Vec<f32> {
        (0..n).map(|_| osc.tick()).collect()
    }

    #[test]
    fn wave_indices_round_trip() {
        for (i, w) in Wave::ALL.iter().enumerate() {
            assert_eq!(w.index(), i);
            assert_eq!(Wave::from_index(i), *w);
            assert_eq!(Wave::from_param(i as f32 + 0.3), *w);
            assert!(!w.label().is_empty());
        }
        assert_eq!(Wave::from_index(99), Wave::Triangle);
        assert_eq!(Wave::from_param(f32::NAN), Wave::default());
        assert_eq!(Wave::from_param(-5.0), Wave::Sine);
    }

    #[test]
    fn every_wave_is_bounded_and_finite() {
        for w in Wave::ALL {
            for hz in [20.0, 440.0, 5_000.0, 15_000.0] {
                let mut osc = Oscillator::new(w, hz, SR);
                let out = render(&mut osc, 48_000);
                assert!(out.iter().all(|s| s.is_finite()), "{w:?} {hz}");
                let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                assert!(peak <= 1.1, "{w:?} at {hz} Hz peaks at {peak}");
                // Near Nyquist a band-limited saw/square is reduced to (less than) its
                // fundamental, so only ask for full level where harmonics fit.
                let min_peak = if hz > 10_000.0 { 0.3 } else { 0.5 };
                assert!(peak > min_peak, "{w:?} at {hz} Hz is quiet: {peak}");
            }
        }
    }

    #[test]
    fn starts_at_zero_crossing_except_square() {
        for w in [Wave::Sine, Wave::Saw, Wave::Triangle] {
            let mut osc = Oscillator::new(w, 440.0, SR);
            let first = osc.tick();
            assert!(first.abs() < 0.05, "{w:?} first sample {first}");
            let later = osc.tick();
            assert!(later > first, "{w:?} should rise from the start");
        }
    }

    #[test]
    fn triangle_matches_naive_shape_at_low_frequency() {
        let mut osc = Oscillator::new(Wave::Triangle, 100.0, SR);
        let out = render(&mut osc, 4_800);
        let max = out.iter().cloned().fold(f32::MIN, f32::max);
        let min = out.iter().cloned().fold(f32::MAX, f32::min);
        assert!((max - 1.0).abs() < 0.08, "max {max}");
        assert!((min + 1.0).abs() < 0.08, "min {min}");
    }

    #[test]
    fn pulse_width_changes_brightness_keeps_pitch_and_half_is_the_square() {
        let brightness = |out: &[f32]| {
            let rms = |b: &[f32]| (b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32).sqrt();
            let d: Vec<f32> = out.windows(2).map(|w| w[1] - w[0]).collect();
            rms(&d) / rms(out)
        };
        let mut square = Oscillator::new(Wave::Square, 440.0, SR);
        let half = render(&mut square, 4800);
        let mut pulse = Oscillator::new(Wave::Square, 440.0, SR);
        pulse.set_pulse_width(0.5);
        assert_eq!(render(&mut pulse, 4800), half, "pw 0.5 is the plain square");
        pulse.reset();
        pulse.set_pulse_width(0.1);
        assert_eq!(pulse.pulse_width(), 0.1);
        let narrow = render(&mut pulse, 4800);
        assert!(narrow.iter().all(|s| s.is_finite()));
        assert!(brightness(&narrow) > brightness(&half) * 1.3);
        let mean = narrow.iter().sum::<f32>() / narrow.len() as f32;
        assert!(mean.abs() < 0.02, "DC removed: mean {mean}");
        let crossings = |b: &[f32]| b.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert_eq!(crossings(&narrow), crossings(&half));
        pulse.set_pulse_width(2.0);
        assert_eq!(pulse.pulse_width(), MAX_PULSE_WIDTH);
        pulse.set_pulse_width(f32::NAN);
        assert_eq!(pulse.pulse_width(), 0.5);
        // The triangle ignores the pulse width.
        let mut tri = Oscillator::new(Wave::Triangle, 100.0, SR);
        let plain = render(&mut tri, 2400);
        let mut tri_pw = Oscillator::new(Wave::Triangle, 100.0, SR);
        tri_pw.set_pulse_width(0.1);
        assert_eq!(render(&mut tri_pw, 2400), plain);
    }

    #[test]
    fn phase_modulation_input_is_transparent_at_zero_and_wraps() {
        for w in Wave::ALL {
            let mut a = Oscillator::new(w, 440.0, SR);
            let mut b = Oscillator::new(w, 440.0, SR);
            for _ in 0..4800 {
                assert_eq!(a.tick(), b.tick_pm(0.0), "{w:?}");
            }
        }
        let mut osc = Oscillator::new(Wave::Sine, 440.0, SR);
        for i in 0..4800 {
            let off = (i as f32 * 0.37).sin() * 3.0;
            assert!(osc.tick_pm(off).is_finite());
        }
        assert!(osc.tick_pm(f32::NAN).is_finite());
        assert!(osc.tick_pm(f32::INFINITY).is_finite());
        assert!((0.0..1.0).contains(&osc.phase()));
    }

    #[test]
    fn weird_frequencies_do_not_panic() {
        let mut osc = Oscillator::new(Wave::Saw, f32::NAN, SR);
        assert_eq!(osc.freq(), 0.0);
        osc.set_freq(-100.0);
        osc.set_freq(1.0e9);
        assert!(osc.freq() <= SR * MAX_PHASE_INC as f32 + 1.0);
        for _ in 0..1000 {
            assert!(osc.tick().is_finite());
        }
        let mut bad_sr = Oscillator::new(Wave::Square, 440.0, 0.0);
        assert!(bad_sr.tick().is_finite());
    }
}
