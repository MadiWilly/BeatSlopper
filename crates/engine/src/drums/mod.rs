//! Procedurally synthesized drum kit (M3): kick, snare, clap, closed hat, open hat.
//!
//! No sample files are used. M3 adds one module per voice with per-row volume and tune.
//! M0 only fixes the row identities so the sequencer and UI can agree on them.

/// The five drum rows of the step sequencer, in display order (top to bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrumRow {
    /// Sine pitch sweep plus a click transient.
    Kick,
    /// Tone plus filtered noise.
    Snare,
    /// Several short noise bursts.
    Clap,
    /// Short high-passed noise / metallic burst.
    ClosedHat,
    /// Longer high-passed noise / metallic burst.
    OpenHat,
}

impl DrumRow {
    /// All rows in display order.
    pub const ALL: [DrumRow; 5] = [
        DrumRow::Kick,
        DrumRow::Snare,
        DrumRow::Clap,
        DrumRow::ClosedHat,
        DrumRow::OpenHat,
    ];

    /// Number of drum rows.
    pub const COUNT: usize = Self::ALL.len();

    /// Index of this row in [`DrumRow::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Short label used in the sequencer grid.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            DrumRow::Kick => "Kick",
            DrumRow::Snare => "Snare",
            DrumRow::Clap => "Clap",
            DrumRow::ClosedHat => "CH",
            DrumRow::OpenHat => "OH",
        }
    }
}

/// The procedural drum kit (M3 stub: silent; the drums subagent implements the voices and
/// keeps this API, see docs/ARCHITECTURE.md "M3 additions / drums").
#[derive(Debug, Clone, PartialEq)]
pub struct DrumKit {
    sample_rate: f32,
    volume: [f32; DrumRow::COUNT],
    tune: [f32; DrumRow::COUNT],
}

impl DrumKit {
    /// Creates a kit for `sample_rate`; everything is pre-allocated here.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        DrumKit {
            sample_rate,
            volume: [1.0; DrumRow::COUNT],
            tune: [0.0; DrumRow::COUNT],
        }
    }

    /// Sample rate the kit runs at.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Fires `row` at `velocity` (`0.0..=1.0`); retriggering restarts the voice.
    pub fn trigger(&mut self, row: DrumRow, velocity: f32) {
        let _ = (row, velocity);
    }

    /// Per-row volume in dB.
    pub fn set_volume(&mut self, row: DrumRow, db: f32) {
        if db.is_finite() {
            self.volume[row.index()] = crate::dsp::db_to_gain(db);
        }
    }

    /// Per-row tune in semitones.
    pub fn set_tune(&mut self, row: DrumRow, semitones: f32) {
        if semitones.is_finite() {
            self.tune[row.index()] = semitones;
        }
    }

    /// Next mono sample of the whole kit.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        0.0
    }

    /// `true` while any drum voice is still sounding.
    #[must_use]
    pub fn is_active(&self) -> bool {
        false
    }

    /// Silences every voice immediately.
    pub fn reset(&mut self) {}
}
