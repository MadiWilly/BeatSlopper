//! The chord palette of the piano roll: a fixed list of chord shapes (intervals in semitones
//! above the root), the most common first, that the CHORDS column beside the roll offers.
//! With a chord armed ([`crate::app::AppState::armed_chord`]) a draw on the roll places every
//! note of the chord at once ([`crate::app::AppState::add_chord`]).

/// One chord shape: a short display name and its intervals in semitones above the root
/// (the root itself, `0`, included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    /// Pill label, e.g. `"Major"`.
    pub name: &'static str,
    /// Semitones above the root, ascending, starting with `0`.
    pub intervals: &'static [u8],
}

/// Every chord the palette offers, most common first.
pub const CHORDS: &[Chord] = &[
    Chord {
        name: "Major",
        intervals: &[0, 4, 7],
    },
    Chord {
        name: "Minor",
        intervals: &[0, 3, 7],
    },
    Chord {
        name: "Power 5",
        intervals: &[0, 7],
    },
    Chord {
        name: "Dom 7",
        intervals: &[0, 4, 7, 10],
    },
    Chord {
        name: "Minor 7",
        intervals: &[0, 3, 7, 10],
    },
    Chord {
        name: "Major 7",
        intervals: &[0, 4, 7, 11],
    },
    Chord {
        name: "Sus 2",
        intervals: &[0, 2, 7],
    },
    Chord {
        name: "Sus 4",
        intervals: &[0, 5, 7],
    },
    Chord {
        name: "Add 9",
        intervals: &[0, 4, 7, 14],
    },
    Chord {
        name: "Minor 9",
        intervals: &[0, 3, 7, 10, 14],
    },
    Chord {
        name: "Dom 9",
        intervals: &[0, 4, 7, 10, 14],
    },
    Chord {
        name: "Major 9",
        intervals: &[0, 4, 7, 11, 14],
    },
    Chord {
        name: "Major 6",
        intervals: &[0, 4, 7, 9],
    },
    Chord {
        name: "Minor 6",
        intervals: &[0, 3, 7, 9],
    },
    Chord {
        name: "Dim",
        intervals: &[0, 3, 6],
    },
    Chord {
        name: "Aug",
        intervals: &[0, 4, 8],
    },
    Chord {
        name: "Dim 7",
        intervals: &[0, 3, 6, 9],
    },
    Chord {
        name: "m7b5",
        intervals: &[0, 3, 6, 10],
    },
    Chord {
        name: "mMaj 7",
        intervals: &[0, 3, 7, 11],
    },
    Chord {
        name: "7 sus 4",
        intervals: &[0, 5, 7, 10],
    },
    Chord {
        name: "Dom 11",
        intervals: &[0, 4, 7, 10, 14, 17],
    },
    Chord {
        name: "Dom 13",
        intervals: &[0, 4, 7, 10, 14, 21],
    },
    Chord {
        name: "Octave",
        intervals: &[0, 12],
    },
];

/// The MIDI pitches of `chord` on `root`, dropping any interval that would land above 127.
pub fn chord_pitches(chord: &Chord, root: u8) -> impl Iterator<Item = u8> + '_ {
    chord
        .intervals
        .iter()
        .filter_map(move |&i| u8::try_from(u16::from(root) + u16::from(i)).ok())
        .filter(|&p| p <= 127)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chord_starts_at_the_root_and_rises() {
        for chord in CHORDS {
            assert_eq!(chord.intervals[0], 0, "{}", chord.name);
            assert!(
                chord.intervals.windows(2).all(|w| w[0] < w[1]),
                "{} intervals must ascend",
                chord.name
            );
            assert!(!chord.name.is_empty());
        }
        let names: Vec<_> = CHORDS.iter().map(|c| c.name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "chord names are unique");
    }

    #[test]
    fn major_and_minor_lead_the_list() {
        assert_eq!(CHORDS[0].name, "Major");
        assert_eq!(CHORDS[1].name, "Minor");
    }

    #[test]
    fn pitches_stop_at_midi_127() {
        let major = &CHORDS[0];
        assert_eq!(
            chord_pitches(major, 60).collect::<Vec<_>>(),
            vec![60, 64, 67]
        );
        assert_eq!(chord_pitches(major, 125).collect::<Vec<_>>(), vec![125]);
        assert_eq!(chord_pitches(major, 127).collect::<Vec<_>>(), vec![127]);
    }
}
