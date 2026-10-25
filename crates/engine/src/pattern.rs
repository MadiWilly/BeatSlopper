//! The 16-step pattern: five drum rows plus one synth lane. Plain data shared by the engine
//! (scheduler), the UI (editing) and the project file (serde).

use serde::{Deserialize, Serialize};

use crate::drums::DrumRow;

/// Steps per bar (16th notes in one 4/4 bar).
pub const STEPS: usize = 16;

/// Velocity of a normal step.
pub const STEP_VELOCITY: f32 = 0.8;
/// Velocity of an accented step.
pub const ACCENT_VELOCITY: f32 = 1.0;

/// Per-row drum volume range, dB.
pub const DRUM_VOLUME_RANGE: std::ops::RangeInclusive<f32> = -24.0..=6.0;
/// Per-row drum tune range, semitones.
pub const DRUM_TUNE_RANGE: std::ops::RangeInclusive<f32> = -12.0..=12.0;

/// One drum step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrumStep {
    /// Whether the drum fires on this step.
    pub on: bool,
    /// Accented steps play at [`ACCENT_VELOCITY`] instead of [`STEP_VELOCITY`].
    pub accent: bool,
}

impl DrumStep {
    /// Velocity this step plays at, or `None` when off.
    #[must_use]
    pub fn velocity(self) -> Option<f32> {
        self.on.then_some(if self.accent {
            ACCENT_VELOCITY
        } else {
            STEP_VELOCITY
        })
    }
}

/// One synth-lane step: a MIDI note, or rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SynthStep {
    /// MIDI note to play, or `None` for a rest.
    pub note: Option<u8>,
    /// Accented steps play at [`ACCENT_VELOCITY`].
    pub accent: bool,
}

impl SynthStep {
    /// `(note, velocity)` when this step plays a note.
    #[must_use]
    pub fn trigger(self) -> Option<(u8, f32)> {
        self.note.map(|n| {
            (
                n.min(127),
                if self.accent {
                    ACCENT_VELOCITY
                } else {
                    STEP_VELOCITY
                },
            )
        })
    }
}

/// The whole 16-step pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    /// Drum rows in [`DrumRow::ALL`] order, 16 steps each.
    pub drums: [[DrumStep; STEPS]; DrumRow::COUNT],
    /// The bass/melody lane.
    pub synth: [SynthStep; STEPS],
    /// Per-row drum volume in dB ([`DRUM_VOLUME_RANGE`]).
    pub drum_volume: [f32; DrumRow::COUNT],
    /// Per-row drum tune in semitones ([`DRUM_TUNE_RANGE`]).
    pub drum_tune: [f32; DrumRow::COUNT],
}

impl Default for Pattern {
    fn default() -> Self {
        Pattern {
            drums: [[DrumStep::default(); STEPS]; DrumRow::COUNT],
            synth: [SynthStep::default(); STEPS],
            drum_volume: [0.0; DrumRow::COUNT],
            drum_tune: [0.0; DrumRow::COUNT],
        }
    }
}

impl Pattern {
    /// An empty pattern.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The demo beat from the design mock: a four-on-the-floor kick, snare and clap on 2
    /// and 4, eighth-note closed hats, open hats on the off-beats, and a C2 bassline.
    #[must_use]
    pub fn demo() -> Self {
        let mut p = Pattern::default();
        let rows: [(DrumRow, &str); 5] = [
            (DrumRow::Kick, "1000100010001000"),
            (DrumRow::Snare, "0000100000001000"),
            (DrumRow::Clap, "0000100000001010"),
            (DrumRow::ClosedHat, "1010101010101010"),
            (DrumRow::OpenHat, "0010001000100010"),
        ];
        for (row, bits) in rows {
            for (i, b) in bits.bytes().enumerate().take(STEPS) {
                p.drums[row.index()][i].on = b == b'1';
            }
            p.drums[row.index()][0].accent = true;
        }
        // C2 = 36, D#2 = 39, G2 = 43, A#1 = 34, G1 = 31, F1 = 29.
        let bass: [Option<u8>; STEPS] = [
            Some(36),
            None,
            Some(36),
            None,
            Some(39),
            None,
            Some(43),
            None,
            Some(36),
            None,
            None,
            Some(36),
            Some(34),
            None,
            Some(31),
            Some(29),
        ];
        for (i, n) in bass.into_iter().enumerate() {
            p.synth[i].note = n;
        }
        p
    }

    /// Clears every step of one drum row (volume and tune are kept).
    pub fn clear_row(&mut self, row: DrumRow) {
        self.drums[row.index()] = [DrumStep::default(); STEPS];
    }

    /// Clears the synth lane.
    pub fn clear_synth(&mut self) {
        self.synth = [SynthStep::default(); STEPS];
    }

    /// Clears every step of every row and the synth lane.
    pub fn clear_all(&mut self) {
        for row in DrumRow::ALL {
            self.clear_row(row);
        }
        self.clear_synth();
    }

    /// `true` when no step is active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drums.iter().flatten().all(|s| !s.on) && self.synth.iter().all(|s| s.note.is_none())
    }

    /// Sets one drum step (step index wraps into `0..16`).
    pub fn set_drum_step(&mut self, row: DrumRow, step: u8, on: bool, accent: bool) {
        self.drums[row.index()][usize::from(step) % STEPS] = DrumStep { on, accent };
    }

    /// Sets one synth step (step index wraps into `0..16`).
    pub fn set_synth_step(&mut self, step: u8, note: Option<u8>, accent: bool) {
        self.synth[usize::from(step) % STEPS] = SynthStep {
            note: note.map(|n| n.min(127)),
            accent,
        };
    }

    /// Sets a row's volume in dB, clamped to [`DRUM_VOLUME_RANGE`]; non-finite values are ignored.
    pub fn set_drum_volume(&mut self, row: DrumRow, db: f32) {
        if db.is_finite() {
            self.drum_volume[row.index()] =
                db.clamp(*DRUM_VOLUME_RANGE.start(), *DRUM_VOLUME_RANGE.end());
        }
    }

    /// Sets a row's tune in semitones, clamped to [`DRUM_TUNE_RANGE`]; non-finite values are ignored.
    pub fn set_drum_tune(&mut self, row: DrumRow, semitones: f32) {
        if semitones.is_finite() {
            self.drum_tune[row.index()] =
                semitones.clamp(*DRUM_TUNE_RANGE.start(), *DRUM_TUNE_RANGE.end());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_pattern_has_the_expected_hits() {
        let p = Pattern::demo();
        assert!(!p.is_empty());
        assert!(p.drums[DrumRow::Kick.index()][0].on);
        assert!(!p.drums[DrumRow::Kick.index()][1].on);
        assert!(p.drums[DrumRow::Snare.index()][4].on);
        assert_eq!(p.synth[0].note, Some(36));
        assert_eq!(p.synth[1].note, None);
    }

    #[test]
    fn clearing_and_setting_steps() {
        let mut p = Pattern::demo();
        p.clear_row(DrumRow::Kick);
        assert!(p.drums[DrumRow::Kick.index()].iter().all(|s| !s.on));
        p.set_drum_step(DrumRow::Kick, 17, true, true); // wraps to step 1
        assert_eq!(
            p.drums[DrumRow::Kick.index()][1],
            DrumStep {
                on: true,
                accent: true
            }
        );
        p.set_synth_step(3, Some(200), false);
        assert_eq!(p.synth[3].note, Some(127));
        p.set_drum_volume(DrumRow::Snare, 99.0);
        assert_eq!(p.drum_volume[DrumRow::Snare.index()], 6.0);
        p.set_drum_tune(DrumRow::Snare, f32::NAN);
        assert_eq!(p.drum_tune[DrumRow::Snare.index()], 0.0);
        p.clear_all();
        assert!(p.is_empty());
    }

    #[test]
    fn pattern_round_trips_through_json() {
        let p = Pattern::demo();
        let json = serde_json::to_string(&p).unwrap();
        let back: Pattern = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn step_velocities() {
        assert_eq!(
            DrumStep {
                on: false,
                accent: true
            }
            .velocity(),
            None
        );
        assert_eq!(
            DrumStep {
                on: true,
                accent: false
            }
            .velocity(),
            Some(STEP_VELOCITY)
        );
        assert_eq!(
            SynthStep {
                note: Some(60),
                accent: true
            }
            .trigger(),
            Some((60, ACCENT_VELOCITY))
        );
    }
}
