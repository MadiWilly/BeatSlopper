//! Freeverb-style reverb: per channel, 8 parallel lowpass-feedback combs feeding 4 series
//! allpasses, with the right channel's delay lines offset by [`STEREO_SPREAD`] samples.
//!
//! The classic tunings are given for 44.1 kHz and scaled to the actual sample rate at
//! construction, so the room sounds the same size at 48 kHz or 96 kHz.
//!
//! - `size` maps to comb feedback `ROOM_OFFSET + size * ROOM_SCALE` (`0.7..=0.98`).
//! - `damping` maps to the comb lowpass coefficient `damping * DAMP_SCALE` (`0..=0.4`).
//!
//! Real-time safe after construction: `process` never allocates or panics; every feedback
//! path is denormal-flushed.

use crate::dsp::flush_denormal;

/// Reference sample rate of the comb/allpass tunings.
pub const REFERENCE_SAMPLE_RATE: f32 = 44_100.0;

/// Comb delay lengths at 44.1 kHz (left channel).
pub const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];

/// Allpass delay lengths at 44.1 kHz (left channel).
pub const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];

/// Extra samples (at 44.1 kHz) added to every right-channel delay line.
pub const STEREO_SPREAD: usize = 23;

/// Input attenuation before the comb bank.
pub const FIXED_GAIN: f32 = 0.015;

/// Comb feedback at `size = 0.0`.
pub const ROOM_OFFSET: f32 = 0.7;

/// Comb feedback range covered by `size`.
pub const ROOM_SCALE: f32 = 0.28;

/// Comb lowpass coefficient range covered by `damping`.
pub const DAMP_SCALE: f32 = 0.4;

/// Allpass feedback (diffusion).
pub const ALLPASS_FEEDBACK: f32 = 0.5;

/// Gain applied to the summed wet signal.
pub const WET_GAIN: f32 = 1.0;

/// Scales a 44.1 kHz tuning to `sample_rate` (at least one sample).
fn scaled(len: usize, sample_rate: f32) -> usize {
    let n = (len as f32 * sample_rate / REFERENCE_SAMPLE_RATE).round();
    (n as usize).max(1)
}

/// Lowpass-feedback comb filter.
#[derive(Debug, Clone, PartialEq)]
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    filter: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            filter: 0.0,
            feedback: ROOM_OFFSET,
            damp1: 0.0,
            damp2: 1.0,
        }
    }

    fn set(&mut self, feedback: f32, damp: f32) {
        self.feedback = feedback;
        self.damp1 = damp;
        self.damp2 = 1.0 - damp;
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
        self.filter = 0.0;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.idx];
        self.filter = flush_denormal(out * self.damp2 + self.filter * self.damp1);
        self.buf[self.idx] = flush_denormal(input + self.filter * self.feedback);
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

/// Schroeder allpass with fixed feedback.
#[derive(Debug, Clone, PartialEq)]
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buf[self.idx];
        self.buf[self.idx] = flush_denormal(input + bufout * ALLPASS_FEEDBACK);
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        bufout - input
    }
}

/// One channel of the reverb network.
#[derive(Debug, Clone, PartialEq)]
struct Channel {
    combs: [Comb; 8],
    allpasses: [Allpass; 4],
}

impl Channel {
    fn new(sample_rate: f32, spread: usize) -> Self {
        Self {
            combs: core::array::from_fn(|i| {
                Comb::new(scaled(COMB_TUNINGS[i] + spread, sample_rate))
            }),
            allpasses: core::array::from_fn(|i| {
                Allpass::new(scaled(ALLPASS_TUNINGS[i] + spread, sample_rate))
            }),
        }
    }

    fn set(&mut self, feedback: f32, damp: f32) {
        for c in &mut self.combs {
            c.set(feedback, damp);
        }
    }

    fn reset(&mut self) {
        for c in &mut self.combs {
            c.reset();
        }
        for a in &mut self.allpasses {
            a.reset();
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let mut out = 0.0;
        for c in &mut self.combs {
            out += c.process(input);
        }
        for a in &mut self.allpasses {
            out = a.process(out);
        }
        out
    }
}

/// Stereo Freeverb.
#[derive(Debug, Clone, PartialEq)]
pub struct Reverb {
    left: Channel,
    right: Channel,
    size: f32,
    damping: f32,
    mix: f32,
    sample_rate: f32,
}

impl Reverb {
    /// Pre-allocates the delay lines for `sample_rate` (non-finite or non-positive rates fall
    /// back to 48 kHz). Starts with size `0.0`, damping `0.0` and a fully dry mix.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut r = Self {
            left: Channel::new(sample_rate, 0),
            right: Channel::new(sample_rate, STEREO_SPREAD),
            size: -1.0,
            damping: -1.0,
            mix: 0.0,
            sample_rate,
        };
        r.set_size(0.0);
        r.set_damping(0.0);
        r
    }

    /// Sample rate the delay lines were scaled for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    fn update(&mut self) {
        let feedback = ROOM_OFFSET + self.size * ROOM_SCALE;
        let damp = self.damping * DAMP_SCALE;
        self.left.set(feedback, damp);
        self.right.set(feedback, damp);
    }

    /// Sets the room size in `0.0..=1.0` (clamped; non-finite ignored). Cheap when unchanged.
    pub fn set_size(&mut self, size: f32) {
        if !size.is_finite() {
            return;
        }
        let size = size.clamp(0.0, 1.0);
        if size != self.size {
            self.size = size;
            self.update();
        }
    }

    /// Sets the damping in `0.0..=1.0` (clamped; non-finite ignored). Cheap when unchanged.
    pub fn set_damping(&mut self, damping: f32) {
        if !damping.is_finite() {
            return;
        }
        let damping = damping.clamp(0.0, 1.0);
        if damping != self.damping {
            self.damping = damping;
            self.update();
        }
    }

    /// Sets the dry/wet mix in `0.0..=1.0` (clamped; non-finite ignored).
    pub fn set_mix(&mut self, mix: f32) {
        if mix.is_finite() {
            self.mix = mix.clamp(0.0, 1.0);
        }
    }

    /// Current room size.
    #[must_use]
    pub fn size(&self) -> f32 {
        self.size
    }

    /// Current damping.
    #[must_use]
    pub fn damping(&self) -> f32 {
        self.damping
    }

    /// Current mix.
    #[must_use]
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Comb feedback currently in use (`0.7..=0.98`).
    #[must_use]
    pub fn comb_feedback(&self) -> f32 {
        ROOM_OFFSET + self.size * ROOM_SCALE
    }

    /// Clears every delay line (kills the tail instantly).
    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    /// Processes one stereo frame. Non-finite inputs are treated as `0.0`.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let input = (l + r) * FIXED_GAIN;
        let wet_l = self.left.process(input) * WET_GAIN;
        let wet_r = self.right.process(input) * WET_GAIN;
        (l + (wet_l - l) * self.mix, r + (wet_r - r) * self.mix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn tail(rev: &mut Reverb, len: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(rev.process(x, x).0);
        }
        out
    }

    fn energy(buf: &[f32]) -> f64 {
        buf.iter().map(|&v| f64::from(v) * f64::from(v)).sum()
    }

    #[test]
    fn tunings_scale_with_sample_rate() {
        assert_eq!(scaled(1116, 44_100.0), 1116);
        assert_eq!(scaled(1116, 88_200.0), 2232);
        assert_eq!(scaled(1116, 48_000.0), 1215);
        let r = Reverb::new(SR);
        assert_eq!(r.left.combs[0].buf.len(), 1215);
        assert_eq!(r.right.combs[0].buf.len(), scaled(1116 + 23, SR));
        assert_eq!(r.left.allpasses[3].buf.len(), scaled(225, SR));
    }

    #[test]
    fn size_maps_to_comb_feedback() {
        let mut r = Reverb::new(SR);
        assert!((r.comb_feedback() - 0.7).abs() < 1e-6);
        r.set_size(1.0);
        assert!((r.comb_feedback() - 0.98).abs() < 1e-6);
        r.set_size(5.0);
        assert_eq!(r.size(), 1.0);
        r.set_damping(1.0);
        assert!((r.left.combs[0].damp1 - 0.4).abs() < 1e-6);
        r.set_damping(f32::NAN);
        assert_eq!(r.damping(), 1.0);
    }

    #[test]
    fn bigger_room_rings_longer() {
        let n = (SR * 2.0) as usize;
        let after = (SR * 0.5) as usize;
        let mut small = Reverb::new(SR);
        small.set_size(0.1);
        small.set_mix(1.0);
        let mut big = Reverb::new(SR);
        big.set_size(0.9);
        big.set_mix(1.0);
        let ts = tail(&mut small, n);
        let tb = tail(&mut big, n);
        assert!(ts.iter().chain(tb.iter()).all(|v| v.is_finite()));
        assert!(energy(&tb[after..]) > energy(&ts[after..]) * 10.0);
    }

    #[test]
    fn dry_mix_is_identity_and_reset_kills_tail() {
        let mut r = Reverb::new(SR);
        r.set_size(1.0);
        r.set_mix(0.0);
        assert_eq!(r.process(0.4, -0.2), (0.4, -0.2));
        r.set_mix(1.0);
        r.reset();
        for _ in 0..10_000 {
            assert_eq!(r.process(0.0, 0.0), (0.0, 0.0));
        }
    }

    #[test]
    fn non_finite_input_is_ignored() {
        let mut r = Reverb::new(SR);
        r.set_mix(1.0);
        assert_eq!(r.process(f32::NAN, f32::NEG_INFINITY), (0.0, 0.0));
        for _ in 0..5000 {
            assert_eq!(r.process(0.0, 0.0), (0.0, 0.0));
        }
    }
}
