//! Transport (M3): play/stop, BPM, swing and the 16-step loop.
//!
//! Sample-accurate step scheduling runs inside the audio thread in M3. M0 only fixes the
//! ranges and the samples-per-step arithmetic that the scheduler and its tests depend on
//! (at 120 BPM / 48 kHz a 16th-note step is exactly 6000 samples).

/// Lowest allowed tempo.
pub const MIN_BPM: f32 = 60.0;
/// Highest allowed tempo.
pub const MAX_BPM: f32 = 200.0;
/// Tempo of a fresh project.
pub const DEFAULT_BPM: f32 = 120.0;
/// Steps per loop: one bar of 16th notes.
pub const STEPS_PER_BAR: usize = 16;

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

    /// Swing amount `0.0..=1.0` (fraction of a step by which odd steps are delayed).
    #[must_use]
    pub fn swing(&self) -> f32 {
        self.swing
    }

    /// Sets the swing amount, clamped to `0.0..=1.0`. Non-finite values are ignored.
    pub fn set_swing(&mut self, swing: f32) {
        if swing.is_finite() {
            self.swing = swing.clamp(0.0, 1.0);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_length_at_120_bpm_48k_is_6000() {
        let t = Transport::new();
        assert_eq!(t.samples_per_step(48_000.0), 6000.0);
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
}
