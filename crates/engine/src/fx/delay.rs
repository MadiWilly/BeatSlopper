//! Stereo delay: a circular buffer per channel, feedback with a gentle one-pole lowpass in
//! the loop, and a dry/wet mix.
//!
//! Delay-time changes are click-free: the output crossfades linearly from the old read tap to
//! the new one over [`FADE_MS`]. A change that arrives mid-fade is queued and applied as soon
//! as the running fade finishes, so the output is always a blend of at most two taps and
//! never jumps. Once a fade has completed the delay is exact to the sample, which is what the
//! BPM-sync tests rely on.
//!
//! Real-time safe after construction: `process` never allocates or panics; the feedback path
//! is denormal-flushed.

use crate::dsp::flush_denormal;

/// Length of the crossfade between the old and new read tap on a time change.
pub const FADE_MS: f32 = 20.0;

/// Highest feedback the delay accepts (`-0.45 dB` per repeat); more would never die out.
pub const MAX_FEEDBACK: f32 = 0.95;

/// Cutoff of the one-pole lowpass in the feedback loop, Hz. Repeats get a little darker
/// each time round, like a tape delay.
pub const FEEDBACK_LOWPASS_HZ: f32 = 8_000.0;

/// Stereo delay line with feedback, tone and mix.
#[derive(Debug, Clone, PartialEq)]
pub struct StereoDelay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write: usize,
    /// Current (target) read distance in samples.
    delay: usize,
    /// Previous read distance, still audible while `fade < 1.0`.
    old_delay: usize,
    /// Crossfade position from `old_delay` (0) to `delay` (1).
    fade: f32,
    fade_inc: f32,
    /// A time change requested while a fade was running.
    pending: Option<usize>,
    feedback: f32,
    mix: f32,
    lp_coeff: f32,
    lp_l: f32,
    lp_r: f32,
    sample_rate: f32,
}

impl StereoDelay {
    /// Pre-allocates a delay line able to hold `max_seconds` at `sample_rate`.
    ///
    /// Non-finite or non-positive arguments fall back to 48 kHz and 1 s. Starts with a delay
    /// of one sample, no feedback and a fully dry mix.
    #[must_use]
    pub fn new(sample_rate: f32, max_seconds: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let max_seconds = if max_seconds.is_finite() && max_seconds > 0.0 {
            max_seconds
        } else {
            1.0
        };
        // One extra slot so that a delay of exactly `max_seconds` is representable: a delay
        // of `d` samples needs a ring of at least `d + 1`.
        let len = (sample_rate * max_seconds).ceil() as usize + 1;
        let fade_samples = (sample_rate * FADE_MS * 0.001).round().max(1.0);
        Self {
            buf_l: vec![0.0; len],
            buf_r: vec![0.0; len],
            write: 0,
            delay: 1,
            old_delay: 1,
            fade: 1.0,
            fade_inc: 1.0 / fade_samples,
            pending: None,
            feedback: 0.0,
            mix: 0.0,
            lp_coeff: lowpass_coefficient(sample_rate, FEEDBACK_LOWPASS_HZ),
            lp_l: 0.0,
            lp_r: 0.0,
            sample_rate,
        }
    }

    /// Longest delay this line can produce, in samples.
    #[must_use]
    pub fn max_delay_samples(&self) -> usize {
        self.buf_l.len() - 1
    }

    /// Sample rate the line was built for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Sets the delay time in whole samples (clamped to `1..=max_delay_samples()`).
    ///
    /// The change is crossfaded in over [`FADE_MS`]; see the module docs.
    pub fn set_delay_samples(&mut self, samples: usize) {
        let d = samples.clamp(1, self.max_delay_samples());
        if d == self.delay {
            self.pending = None;
            return;
        }
        if self.fade < 1.0 {
            self.pending = Some(d);
        } else {
            self.old_delay = self.delay;
            self.delay = d;
            self.fade = 0.0;
        }
    }

    /// Sets the delay time in seconds (rounded to the nearest sample, then clamped).
    pub fn set_delay_seconds(&mut self, seconds: f32) {
        if !seconds.is_finite() {
            return;
        }
        let samples = (seconds.max(0.0) * self.sample_rate).round();
        // `as usize` saturates, so huge values simply clamp to the maximum.
        self.set_delay_samples(samples as usize);
    }

    /// Delay time the line is (fading) towards, in samples.
    #[must_use]
    pub fn delay_samples(&self) -> usize {
        self.pending.unwrap_or(self.delay)
    }

    /// `true` while a time change is still crossfading.
    #[must_use]
    pub fn is_fading(&self) -> bool {
        self.fade < 1.0
    }

    /// Sets the feedback in `0.0..=MAX_FEEDBACK` (clamped; non-finite ignored).
    pub fn set_feedback(&mut self, feedback: f32) {
        if feedback.is_finite() {
            self.feedback = feedback.clamp(0.0, MAX_FEEDBACK);
        }
    }

    /// Sets the dry/wet mix in `0.0..=1.0` (clamped; non-finite ignored).
    pub fn set_mix(&mut self, mix: f32) {
        if mix.is_finite() {
            self.mix = mix.clamp(0.0, 1.0);
        }
    }

    /// Current feedback.
    #[must_use]
    pub fn feedback(&self) -> f32 {
        self.feedback
    }

    /// Current mix.
    #[must_use]
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Clears the buffers and the tone filter, and completes any running fade instantly.
    pub fn reset(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write = 0;
        self.lp_l = 0.0;
        self.lp_r = 0.0;
        if let Some(p) = self.pending.take() {
            self.delay = p;
        }
        self.old_delay = self.delay;
        self.fade = 1.0;
    }

    #[inline]
    fn tap(&self, distance: usize) -> (f32, f32) {
        let n = self.buf_l.len();
        let i = (self.write + n - distance) % n;
        (self.buf_l[i], self.buf_r[i])
    }

    /// Processes one stereo frame. Non-finite inputs are treated as `0.0`.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };

        let (mut tl, mut tr) = self.tap(self.delay);
        if self.fade < 1.0 {
            let (ol, or) = self.tap(self.old_delay);
            tl = ol + (tl - ol) * self.fade;
            tr = or + (tr - or) * self.fade;
            self.fade += self.fade_inc;
            if self.fade >= 1.0 {
                self.fade = 1.0;
                if let Some(p) = self.pending.take() {
                    self.old_delay = self.delay;
                    self.delay = p;
                    self.fade = 0.0;
                }
            }
        }

        self.lp_l = flush_denormal(self.lp_l + self.lp_coeff * (tl - self.lp_l));
        self.lp_r = flush_denormal(self.lp_r + self.lp_coeff * (tr - self.lp_r));
        self.buf_l[self.write] = flush_denormal(l + self.feedback * self.lp_l);
        self.buf_r[self.write] = flush_denormal(r + self.feedback * self.lp_r);
        self.write += 1;
        if self.write == self.buf_l.len() {
            self.write = 0;
        }

        (l + (tl - l) * self.mix, r + (tr - r) * self.mix)
    }
}

/// One-pole lowpass coefficient for `cutoff_hz` at `sample_rate` (`1.0` = no filtering).
fn lowpass_coefficient(sample_rate: f32, cutoff_hz: f32) -> f32 {
    if sample_rate <= 0.0 || cutoff_hz <= 0.0 {
        return 1.0;
    }
    (1.0 - (-core::f32::consts::TAU * cutoff_hz / sample_rate).exp()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn impulse_response(d: &mut StereoDelay, len: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(d.process(x, x).0);
        }
        out
    }

    #[test]
    fn capacity_covers_max_seconds() {
        let d = StereoDelay::new(SR, 4.0);
        assert_eq!(d.max_delay_samples(), 192_000);
        let mut d2 = StereoDelay::new(SR, 4.0);
        d2.set_delay_seconds(4.0);
        assert_eq!(d2.delay_samples(), 192_000);
        d2.set_delay_seconds(100.0);
        assert_eq!(d2.delay_samples(), 192_000);
        d2.set_delay_seconds(0.0);
        assert_eq!(d2.delay_samples(), 1);
        d2.set_delay_seconds(f32::NAN);
        assert_eq!(d2.delay_samples(), 1);
    }

    #[test]
    fn echo_lands_exactly_on_the_delay_time() {
        let mut d = StereoDelay::new(SR, 1.0);
        d.set_delay_samples(1000);
        d.set_mix(1.0);
        d.reset();
        let out = impulse_response(&mut d, 3000);
        let (idx, peak) = out
            .iter()
            .enumerate()
            .fold((0, 0.0_f32), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
        assert_eq!(idx, 1000);
        assert!((peak - 1.0).abs() < 1e-6);
        assert_eq!(out[0], 0.0, "wet-only output has no dry signal");
    }

    #[test]
    fn feedback_repeats_decay() {
        let mut d = StereoDelay::new(SR, 1.0);
        d.set_delay_samples(500);
        d.set_mix(1.0);
        d.set_feedback(0.5);
        d.reset();
        let out = impulse_response(&mut d, 2600);
        let window = |c: usize| -> f32 { out[c - 20..c + 60].iter().map(|v| v.abs()).sum() };
        let first = window(500);
        let second = window(1000);
        let third = window(1500);
        assert!((first - 1.0).abs() < 1e-5);
        assert!((second - 0.5).abs() < 0.02, "second {second}");
        assert!((third - 0.25).abs() < 0.02, "third {third}");
        assert!(d.feedback() == 0.5);
        d.set_feedback(2.0);
        assert_eq!(d.feedback(), MAX_FEEDBACK);
    }

    #[test]
    fn time_change_crossfades_without_jumps() {
        let mut d = StereoDelay::new(SR, 1.0);
        d.set_delay_samples(4000);
        d.set_mix(1.0);
        // No feedback: the wet signal is then a pure delayed copy of the input, so any step
        // beyond the input's own 1.6 can only come from the tap change.
        d.set_feedback(0.0);
        // Fill with a loud square-ish signal so an abrupt tap switch would show up.
        let mut prev = 0.0_f32;
        let mut max_step = 0.0_f32;
        for i in 0..40_000 {
            if i == 10_000 {
                d.set_delay_samples(1234);
            }
            if i == 10_100 {
                // Mid-fade change: queued, not applied abruptly.
                d.set_delay_samples(7000);
                assert!(d.is_fading());
                assert_eq!(d.delay_samples(), 7000);
            }
            let x = if (i / 37) % 2 == 0 { 0.8 } else { -0.8 };
            let (y, _) = d.process(x, x);
            assert!(y.is_finite());
            if i > 0 {
                max_step = max_step.max((y - prev).abs());
            }
            prev = y;
        }
        // The input itself steps by 1.6; the crossfade must not add bigger discontinuities.
        assert!(max_step <= 1.6 + 1e-3, "max step {max_step}");
        assert!(!d.is_fading());
        assert_eq!(d.delay_samples(), 7000);
    }

    #[test]
    fn reset_clears_tail_and_finishes_fade() {
        let mut d = StereoDelay::new(SR, 1.0);
        d.set_delay_samples(100);
        d.set_mix(1.0);
        d.set_feedback(0.9);
        d.process(1.0, 1.0);
        d.set_delay_samples(200);
        d.process(0.0, 0.0);
        d.set_delay_samples(300);
        assert!(d.is_fading());
        d.reset();
        assert!(!d.is_fading());
        assert_eq!(d.delay_samples(), 300);
        for _ in 0..1000 {
            assert_eq!(d.process(0.0, 0.0), (0.0, 0.0));
        }
    }

    #[test]
    fn non_finite_input_does_not_poison_the_buffer() {
        let mut d = StereoDelay::new(SR, 0.1);
        d.set_delay_samples(10);
        d.set_mix(1.0);
        d.set_feedback(0.5);
        d.process(f32::NAN, f32::INFINITY);
        for _ in 0..100 {
            let (l, r) = d.process(0.0, 0.0);
            assert_eq!((l, r), (0.0, 0.0));
        }
    }

    #[test]
    fn weird_construction_arguments_fall_back() {
        let d = StereoDelay::new(f32::NAN, -3.0);
        assert_eq!(d.sample_rate(), 48_000.0);
        assert_eq!(d.max_delay_samples(), 48_000);
    }
}
