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
