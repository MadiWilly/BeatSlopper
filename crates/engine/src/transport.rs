//! Transport (M3): play/stop, BPM, swing and the 16-step loop.
//!
//! The sample-accurate scheduler lives in [`Engine::render`](crate::Engine::render); this
//! module holds the tempo/swing state and the pure arithmetic it is built on, so the trigger
//! rule can be unit-tested without rendering audio:
//!
//! - one 16th-note step is `samples_per_step = sample_rate * 60 / bpm / 4` samples (kept as
//!   `f64`; exactly 6000 at 120 BPM / 48 kHz),
//! - a bar is [`STEPS_PER_BAR`] steps,
//! - step `n` triggers at `round(n * samples_per_step)` samples into the bar when `n` is even
//!   and at `round((n + swing) * samples_per_step)` when `n` is odd ([`Transport::swing`] in
//!   `0.0..=`[`MAX_SWING`], so the last odd step always stays inside the bar).

/// Lowest allowed tempo.
pub const MIN_BPM: f32 = 60.0;
/// Highest allowed tempo.
pub const MAX_BPM: f32 = 200.0;
/// Tempo of a fresh project.
pub const DEFAULT_BPM: f32 = 120.0;
/// Steps per loop: one bar of 16th notes.
pub const STEPS_PER_BAR: usize = 16;
/// Largest swing amount: odd steps may be delayed by up to three quarters of a step, so step
/// 15 still triggers before the bar wraps.
pub const MAX_SWING: f32 = 0.75;

/// Tempo and loop state. Lives on the audio thread; the UI changes it via messages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transport {
    bpm: f32,
    swing: f32,
    playing: bool,
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport {
    /// Stopped transport at [`DEFAULT_BPM`] with no swing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bpm: DEFAULT_BPM,
            swing: 0.0,
            playing: false,
        }
    }

    /// Current tempo in beats per minute.
    #[must_use]
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Sets the tempo, clamped to [`MIN_BPM`]`..=`[`MAX_BPM`]. Non-finite values are ignored.
    pub fn set_bpm(&mut self, bpm: f32) {
        if bpm.is_finite() {
            self.bpm = bpm.clamp(MIN_BPM, MAX_BPM);
        }
    }

    /// Swing amount `0.0..=`[`MAX_SWING`]: the fraction of a step by which odd steps are delayed
    /// (0.5 is the classic triplet feel).
    #[must_use]
    pub fn swing(&self) -> f32 {
        self.swing
    }

    /// Sets the swing amount, clamped to `0.0..=`[`MAX_SWING`]. Non-finite values are ignored.
    pub fn set_swing(&mut self, swing: f32) {
        if swing.is_finite() {
            self.swing = swing.clamp(0.0, MAX_SWING);
        }
    }

    /// Whether the loop is running.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Starts or stops the loop.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    /// Length of one 16th-note step in samples at the current tempo.
    #[must_use]
    pub fn samples_per_step(&self, sample_rate: f32) -> f64 {
        f64::from(sample_rate) * 60.0 / f64::from(self.bpm) / 4.0
    }

    /// Length of the 16-step bar in samples at the current tempo (exact, unrounded).
    #[must_use]
    pub fn bar_samples(&self, sample_rate: f32) -> f64 {
        self.samples_per_step(sample_rate) * STEPS_PER_BAR as f64
    }

    /// How far step `step` is delayed by swing, in steps: `0.0` for even steps,
    /// [`swing`](Self::swing) for odd ones.
    #[must_use]
    pub fn swing_offset_steps(&self, step: usize) -> f64 {
        if step % 2 == 1 {
            f64::from(self.swing)
        } else {
            0.0
        }
    }

    /// The sample (counted from the start of the bar) at which step `step` triggers:
    /// `round((step + swing_offset) * samples_per_step)`.
    #[must_use]
    pub fn step_trigger_sample(&self, step: usize, samples_per_step: f64) -> u64 {
        let pos = (step as f64 + self.swing_offset_steps(step)) * samples_per_step;
        pos.round().max(0.0) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn step_length_at_120_bpm_48k_is_6000() {
        let t = Transport::new();
        assert_eq!(t.samples_per_step(SR), 6000.0);
        assert_eq!(t.bar_samples(SR), 96_000.0);
    }

    #[test]
    fn step_length_at_90_bpm_is_8000() {
        let mut t = Transport::new();
        t.set_bpm(90.0);
        assert_eq!(t.samples_per_step(SR), 8000.0);
        assert_eq!(t.bar_samples(SR), 128_000.0);
    }

    #[test]
    fn bpm_is_clamped() {
        let mut t = Transport::new();
        t.set_bpm(10.0);
        assert_eq!(t.bpm(), MIN_BPM);
        t.set_bpm(999.0);
        assert_eq!(t.bpm(), MAX_BPM);
        t.set_bpm(f32::NAN);
        assert_eq!(t.bpm(), MAX_BPM);
    }

    #[test]
    fn swing_is_clamped_to_three_quarters() {
        let mut t = Transport::new();
        t.set_swing(2.0);
        assert_eq!(t.swing(), MAX_SWING);
        t.set_swing(-1.0);
        assert_eq!(t.swing(), 0.0);
        t.set_swing(f32::INFINITY);
        assert_eq!(t.swing(), 0.0);
    }

    #[test]
    fn even_steps_trigger_on_the_grid_without_swing() {
        let t = Transport::new();
        let spp = t.samples_per_step(SR);
        for n in 0..STEPS_PER_BAR {
            assert_eq!(t.swing_offset_steps(n), 0.0);
            assert_eq!(t.step_trigger_sample(n, spp), n as u64 * 6000);
        }
    }

    #[test]
    fn swing_delays_odd_steps_only() {
        let mut t = Transport::new();
        t.set_swing(0.5);
        let spp = t.samples_per_step(SR);
        for n in 0..STEPS_PER_BAR {
            let expected = if n % 2 == 1 {
                (n as f64 + 0.5) * 6000.0
            } else {
                n as f64 * 6000.0
            };
            assert_eq!(t.step_trigger_sample(n, spp), expected as u64, "step {n}");
        }
        // Step 15 at maximum swing still lands inside the bar.
        t.set_swing(MAX_SWING);
        assert!(t.step_trigger_sample(15, spp) < t.bar_samples(SR).round() as u64);
    }

    #[test]
    fn fractional_step_lengths_round_to_the_nearest_sample() {
        let mut t = Transport::new();
        t.set_bpm(130.0);
        let spp = t.samples_per_step(SR); // 5538.4615...
        assert_eq!(t.step_trigger_sample(1, spp), 5538);
        assert_eq!(t.step_trigger_sample(2, spp), 11_077);
    }
}
