//! Chorus: two modulated delay taps that turn the mono synth channel into stereo.
//!
//! One [`ModDelay`] holds the input; two taps read it back at `BASE_MS +- depth`, where
//! `depth = ChorusDepth x `[`DEPTH_MS`] and the modulation is a sine LFO at `ChorusRate`.
//! The left tap follows `sin(phase)`, the right tap `cos(phase)` (90 degrees apart), so the
//! two channels never move in step and the image widens. Each output is the classic chorus
//! blend `x + (tap - x) * mix / 2`: at `ChorusMix = 1.0` dry and wet are equal, at `0.0` the
//! output is exactly `(x, x)`.
//!
//! `ChorusMix` is smoothed over [`DEFAULT_SMOOTH_MS`]; `ChorusDepth` over [`DEPTH_SMOOTH_MS`]
//! because moving the tap is itself a pitch shift. The rate is applied directly: the phase
//! accumulator stays continuous so a rate change is click-free.
//!
//! Real-time safe after [`Chorus::new`]: the delay line is the only heap object and is only
//! indexed afterwards. Non-finite input is treated as silence; the output is always finite.

use core::f32::consts::TAU;

use crate::params::{DEFAULT_SMOOTH_MS, ParamId, Smoother};

use super::mod_delay::ModDelay;

/// Centre delay of both taps, milliseconds.
pub const BASE_MS: f32 = 12.0;

/// Modulation depth at `ChorusDepth = 1.0`, milliseconds (so the taps sweep 4..=20 ms).
pub const DEPTH_MS: f32 = 8.0;

/// Smoothing time of the depth parameter, milliseconds.
pub const DEPTH_SMOOTH_MS: f32 = 50.0;

/// Stereo chorus from a mono input. See the module docs.
#[derive(Debug, Clone, PartialEq)]
pub struct Chorus {
    sample_rate: f32,
    /// Clamped targets of `ChorusRate`, `ChorusDepth`, `ChorusMix`.
    values: [f32; 3],
    delay: ModDelay,
    phase: f32,
    phase_inc: f32,
    base_samples: f32,
    depth_samples: f32,
    depth: Smoother,
    mix: Smoother,
}

impl Chorus {
    /// The parameters this stage owns, in [`ParamId::ALL`] order.
    pub const IDS: [ParamId; 3] = [
        ParamId::ChorusRate,
        ParamId::ChorusDepth,
        ParamId::ChorusMix,
    ];

    /// Pre-allocates the chorus for `sample_rate` (non-finite or non-positive rates fall back
    /// to 48 kHz) with every `Chorus*` parameter at its [`ParamId::default_value`].
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let max_samples = (sample_rate * (BASE_MS + DEPTH_MS) * 0.001).ceil() as usize;
        let mut chorus = Self {
            sample_rate,
            values: [0.0; 3],
            delay: ModDelay::new(max_samples),
            phase: 0.0,
            phase_inc: 0.0,
            base_samples: sample_rate * BASE_MS * 0.001,
            depth_samples: sample_rate * DEPTH_MS * 0.001,
            depth: Smoother::new(sample_rate, DEPTH_SMOOTH_MS, 0.0),
            mix: Smoother::new(sample_rate, DEFAULT_SMOOTH_MS, 0.0),
        };
        for id in Self::IDS {
            chorus.set_param(id, id.default_value());
        }
        chorus.reset();
        chorus
    }

    fn slot(id: ParamId) -> Option<usize> {
        Self::IDS.iter().position(|&i| i == id)
    }

    /// Sample rate the chorus was built for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Sets one of the `Chorus*` parameters (clamped with [`ParamId::clamp`]); other ids are
    /// ignored.
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let Some(slot) = Self::slot(id) else {
            return;
        };
        let v = id.clamp(value);
        self.values[slot] = v;
        match id {
            ParamId::ChorusRate => self.phase_inc = v / self.sample_rate,
            ParamId::ChorusDepth => self.depth.set_target(v),
            ParamId::ChorusMix => self.mix.set_target(v),
            _ => {}
        }
    }

    /// Current (clamped target) value of a `Chorus*` parameter; other ids return `0.0`.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        Self::slot(id).map_or(0.0, |s| self.values[s])
    }

    /// Current LFO phase in cycles, `0.0..1.0`.
    #[must_use]
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Clears the delay line, restarts the LFO and snaps the smoothers to their targets.
    pub fn reset(&mut self) {
        self.delay.reset();
        self.phase = 0.0;
        for s in [&mut self.depth, &mut self.mix] {
            s.snap(s.target());
        }
    }

    /// Processes one mono sample into a stereo pair. Non-finite input is treated as `0.0`;
    /// the output is finite. With `ChorusMix` at `0.0` the output is exactly `(x, x)`.
    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let x = if x.is_finite() { x } else { 0.0 };
        let depth = self.depth.tick() * self.depth_samples;
        let m = 0.5 * self.mix.tick();
        let (s, c) = (TAU * self.phase).sin_cos();
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        self.delay.push(x);
        let a = self.delay.read(self.base_samples + depth * s);
        let b = self.delay.read(self.base_samples + depth * c);
        (x + (a - x) * m, x + (b - x) * m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn defaults_and_routing() {
        let c = Chorus::new(SR);
        for id in Chorus::IDS {
            assert_eq!(c.param(id), id.default_value(), "{id:?}");
        }
        assert_eq!(c.param(ParamId::LofiMix), 0.0);
        let mut c = Chorus::new(SR);
        c.set_param(ParamId::ChorusRate, 100.0);
        assert_eq!(c.param(ParamId::ChorusRate), 5.0);
        let before = c.clone();
        c.set_param(ParamId::DriveAmount, 1.0);
        assert_eq!(c, before);
        assert_eq!(Chorus::new(-1.0).sample_rate(), 48_000.0);
    }

    #[test]
    fn mix_zero_is_identity() {
        let mut c = Chorus::new(SR);
        c.set_param(ParamId::ChorusDepth, 1.0);
        c.reset();
        for i in 0..10_000 {
            let x = (i as f32 * 0.05).sin() * 0.6;
            assert_eq!(c.process(x), (x, x));
        }
    }

    #[test]
    fn mix_one_is_an_equal_blend_at_the_centre_delay() {
        // Depth 0: both taps sit exactly at the centre delay, so an impulse comes back at
        // half amplitude BASE_MS later, on both sides.
        let mut c = Chorus::new(SR);
        c.set_param(ParamId::ChorusDepth, 0.0);
        c.set_param(ParamId::ChorusMix, 1.0);
        c.reset();
        let n = (SR * BASE_MS * 0.001) as usize;
        for i in 0..=n + 5 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, r) = c.process(x);
            if i == 0 {
                assert_eq!((l, r), (0.5, 0.5));
            } else if i == n {
                assert!((l - 0.5).abs() < 1e-6 && (r - 0.5).abs() < 1e-6, "{l} {r}");
            } else {
                assert!(l.abs() < 1e-6 && r.abs() < 1e-6, "sample {i}: {l} {r}");
            }
        }
    }

    #[test]
    fn lfo_phase_wraps_and_input_is_cleaned() {
        let mut c = Chorus::new(SR);
        c.set_param(ParamId::ChorusRate, 5.0);
        c.set_param(ParamId::ChorusMix, 1.0);
        c.reset();
        for _ in 0..(SR as usize) {
            let (l, r) = c.process(f32::NAN);
            assert_eq!((l, r), (0.0, 0.0));
            assert!((0.0..1.0).contains(&c.phase()));
        }
    }
}
