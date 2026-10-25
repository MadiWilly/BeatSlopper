//! Patterns (M3/M5/M7): five drum rows plus a polyphonic note list for the synth, 16/32/64
//! steps long, eight of them in a [`Bank`] together with the song chain. Plain data shared by
//! the engine (scheduler), the UI (editing) and the project file (serde).
//!
//! # Layout
//!
//! - The bar is always [`BAR_STEPS`] (= [`STEPS`]) sixteenth notes. A pattern's
//!   [`length`](Pattern::length) is 16, 32 or 64 steps ([`PatternLength`]), i.e. 1, 2 or 4
//!   bars. Step storage is fixed at [`MAX_STEPS`] per row so the audio thread never
//!   allocates; steps beyond `length` are kept (so shortening and re-lengthening a pattern
//!   loses nothing) but not played.
//! - The synth part of a pattern is one note list **per instrument track** (M10):
//!   [`Pattern::tracks`] holds [`MAX_TRACKS`](crate::track::MAX_TRACKS) [`NoteTrack`]s, each a
//!   list of up to [`MAX_NOTES`] [`Note`]s (`start` step, `length` in steps, MIDI `pitch`,
//!   `velocity`) kept sorted by start then pitch in a fixed array. Notes may overlap (chords,
//!   held notes); a note that starts at or beyond the pattern length is kept but never
//!   reached by the scheduler, and one that runs past the end is released at the wrap. Every
//!   note helper takes the `track` it edits.
//! - The step grid's bass lane, [`Pattern::synth`], is a **derived view** of *one* track's
//!   notes, [`Pattern::lane_track`] (the engine's and the UI's edit track): after every note
//!   edit the lane is rebuilt so `synth[s]` is the lowest-pitched note of that track starting
//!   at step `s` (accent = velocity >= [`ACCENT_THRESHOLD`]). [`Pattern::rebuild_lane`]
//!   switches the lane to another track. Grid editing keeps working through
//!   [`Pattern::set_synth_step`], which upserts a length-1 note. Writing to `synth` directly is
//!   not supported: the next rebuild overwrites it. `synth` and `lane_track` are views, so
//!   they do not take part in `Pattern` equality.
//! - A [`Bank`] holds [`PATTERN_COUNT`] patterns (A..H), the song chain (up to [`MAX_CHAIN`]
//!   [`ChainEntry`]s, each a pattern index plus a repeat count), the [`PlayMode`] and the
//!   `current` (selected) pattern.
//!
//! # JSON
//!
//! Drum rows serialise as plain JSON arrays of step objects. A row is written with
//! [`Pattern::used_steps`] entries: its `length`, or more when steps beyond the length hold
//! data, so the file stays compact *and* lossless. On load a row of 16, 32 or 64 entries is
//! accepted as is (shorter rows are padded with empty steps); a row longer than
//! [`MAX_STEPS`] is an error. The notes are written as a `tracks` array holding one
//! `{"track": n, "notes": [...]}` entry per **non-empty** track (format version 7), each note
//! an object `{"start", "length", "pitch", "velocity"}`; the derived `synth` lane is not
//! written and is rebuilt on load (for track 0). A pattern with a `notes` array instead
//! (format versions 5 and 6) loads it into track 0; a pattern with neither (version 4 and
//! older) converts its `synth` lane: every step with a note becomes a length-1 note at
//! [`ACCENT_VELOCITY`] when accented, [`STEP_VELOCITY`] otherwise. A version-2 pattern (no
//! `length` field, 16 entries) loads with `length = 16`; when `length` is absent the shortest
//! length covering the longest row (or note) is used. The chain is written as an array of
//! `chain_len` entries (`chain_len` itself is implied by the array), `mode` as `"pattern"` /
//! `"song"`.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::drums::DrumRow;
use crate::track::{MAX_TRACKS, TrackId, track_index};

/// Steps per bar (16th notes in one 4/4 bar). The scheduler's bar is always this long.
pub const STEPS: usize = 16;
/// Steps per bar, the M5 name for [`STEPS`].
pub const BAR_STEPS: usize = STEPS;
/// Longest pattern, in steps (4 bars). Step storage is fixed at this size.
pub const MAX_STEPS: usize = 64;
/// Patterns per bank (A..H).
pub const PATTERN_COUNT: usize = 8;
/// Longest song chain, in entries.
pub const MAX_CHAIN: usize = 64;
/// Largest repeat count of one chain entry.
pub const MAX_REPEATS: u8 = 8;

/// Velocity of a normal step.
pub const STEP_VELOCITY: f32 = 0.8;
/// Velocity of an accented step.
pub const ACCENT_VELOCITY: f32 = 1.0;
/// Notes at or above this velocity show as accented in the derived step lane.
pub const ACCENT_THRESHOLD: f32 = 0.95;
/// Smallest note velocity; quieter notes are clamped up to it.
pub const MIN_VELOCITY: f32 = 0.05;
/// Most notes one pattern holds.
pub const MAX_NOTES: usize = 64;

/// One synth note of a pattern (M7): a held, possibly overlapping note on the step grid.
///
/// `start` is the step the note begins on (`0..MAX_STEPS`, counted from the pattern start, so
/// step 20 is bar 1, step 4), `length` its duration in steps (`1..=MAX_STEPS`; a note that
/// runs past the pattern end is released when the pattern wraps or switches), `pitch` the
/// MIDI note (`0..=127`) and `velocity` the level in `MIN_VELOCITY..=1`. [`Note::normalise`]
/// clamps every field into range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    /// First step of the note, `0..MAX_STEPS`.
    pub start: u8,
    /// Duration in steps, `1..=MAX_STEPS`.
    pub length: u8,
    /// MIDI note number, `0..=127`.
    pub pitch: u8,
    /// Velocity, `MIN_VELOCITY..=1.0`; the lane shows `>= ACCENT_THRESHOLD` as accented.
    pub velocity: f32,
}

impl Default for Note {
    /// A one-step note at step 0, pitch 0, normal velocity (the empty-slot value).
    fn default() -> Self {
        Self::EMPTY_SLOT
    }
}

impl Note {
    /// The value of an unused note slot (same as [`Note::default`]).
    pub const EMPTY_SLOT: Note = Note {
        start: 0,
        length: 1,
        pitch: 0,
        velocity: STEP_VELOCITY,
    };

    /// A note with every field clamped into range (see [`Note::normalise`]).
    #[must_use]
    pub fn new(start: u8, length: u8, pitch: u8, velocity: f32) -> Self {
        let mut n = Self {
            start,
            length,
            pitch,
            velocity,
        };
        n.normalise();
        n
    }

    /// A length-1 note at `step`, as the step grid writes it: [`ACCENT_VELOCITY`] when
    /// `accent`, [`STEP_VELOCITY`] otherwise.
    #[must_use]
    pub fn step(step: u8, pitch: u8, accent: bool) -> Self {
        Self::new(
            step,
            1,
            pitch,
            if accent {
                ACCENT_VELOCITY
            } else {
                STEP_VELOCITY
            },
        )
    }

    /// First step after the note (`start + length`), which may lie beyond the pattern.
    #[must_use]
    pub fn end(self) -> usize {
        usize::from(self.start) + usize::from(self.length.max(1))
    }

    /// `true` when the lane shows this note as accented (`velocity >= ACCENT_THRESHOLD`).
    #[must_use]
    pub fn accent(self) -> bool {
        self.velocity >= ACCENT_THRESHOLD
    }

    /// The lane cell this note produces when it is the lowest note starting at its step.
    #[must_use]
    pub fn lane_step(self) -> SynthStep {
        SynthStep {
            note: Some(self.pitch.min(127)),
            accent: self.accent(),
        }
    }

    /// Clamps `start` to `0..MAX_STEPS`, `length` to `1..=MAX_STEPS`, `pitch` to `0..=127`
    /// and `velocity` to `MIN_VELOCITY..=1` (non-finite becomes [`STEP_VELOCITY`]).
    pub fn normalise(&mut self) {
        self.start = self.start.min(MAX_STEPS as u8 - 1);
        self.length = self.length.clamp(1, MAX_STEPS as u8);
        self.pitch = self.pitch.min(127);
        self.velocity = if self.velocity.is_finite() {
            self.velocity.clamp(MIN_VELOCITY, 1.0)
        } else {
            STEP_VELOCITY
        };
    }

    /// Total order used to keep a pattern's notes sorted: start, then pitch, then length,
    /// then velocity.
    fn sort_key(self) -> (u8, u8, u8, u32) {
        (self.start, self.pitch, self.length, self.velocity.to_bits())
    }
}

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

/// The three allowed pattern lengths: 1, 2 or 4 bars of [`BAR_STEPS`] steps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatternLength {
    /// 16 steps, one bar.
    #[default]
    Bar1,
    /// 32 steps, two bars.
    Bar2,
    /// 64 steps, four bars.
    Bar4,
}

impl PatternLength {
    /// Every length, shortest first.
    pub const ALL: [PatternLength; 3] = [
        PatternLength::Bar1,
        PatternLength::Bar2,
        PatternLength::Bar4,
    ];

    /// Length in steps (16, 32 or 64).
    #[must_use]
    pub const fn steps(self) -> u8 {
        match self {
            PatternLength::Bar1 => 16,
            PatternLength::Bar2 => 32,
            PatternLength::Bar4 => 64,
        }
    }

    /// Length in bars (1, 2 or 4).
    #[must_use]
    pub const fn bars(self) -> u8 {
        match self {
            PatternLength::Bar1 => 1,
            PatternLength::Bar2 => 2,
            PatternLength::Bar4 => 4,
        }
    }

    /// The length with exactly `steps` steps, or `None` for anything but 16, 32 or 64.
    #[must_use]
    pub const fn from_steps(steps: u8) -> Option<Self> {
        match steps {
            16 => Some(PatternLength::Bar1),
            32 => Some(PatternLength::Bar2),
            64 => Some(PatternLength::Bar4),
            _ => None,
        }
    }

    /// The shortest length that holds `steps` steps (anything above 32 becomes four bars).
    #[must_use]
    pub const fn covering(steps: usize) -> Self {
        if steps <= 16 {
            PatternLength::Bar1
        } else if steps <= 32 {
            PatternLength::Bar2
        } else {
            PatternLength::Bar4
        }
    }

    /// Label for the UI (`"1 bar"`, `"2 bars"`, `"4 bars"`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            PatternLength::Bar1 => "1 bar",
            PatternLength::Bar2 => "2 bars",
            PatternLength::Bar4 => "4 bars",
        }
    }
}

/// The note list of one instrument track within a pattern (M10): up to [`MAX_NOTES`] notes
/// sorted by start then pitch in a fixed array.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteTrack {
    /// The notes, sorted by [`Note`] start then pitch; only the first `note_count` slots are
    /// valid (the rest are kept at `Note::default()`). Edit through the helpers so the order
    /// stays consistent, or call [`normalise`](Self::normalise) afterwards.
    pub notes: [Note; MAX_NOTES],
    /// Number of valid entries in `notes`, `0..=MAX_NOTES`.
    pub note_count: u8,
}

impl Default for NoteTrack {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl NoteTrack {
    /// No notes.
    pub const EMPTY: NoteTrack = NoteTrack {
        notes: [Note::EMPTY_SLOT; MAX_NOTES],
        note_count: 0,
    };

    /// The valid notes, sorted by start then pitch.
    #[must_use]
    pub fn notes(&self) -> &[Note] {
        &self.notes[..usize::from(self.note_count).min(MAX_NOTES)]
    }

    /// `true` when the track holds no notes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.note_count == 0
    }

    /// Index of the first note starting at `step` with MIDI `pitch`, if any.
    #[must_use]
    pub fn note_at(&self, step: u8, pitch: u8) -> Option<usize> {
        self.notes()
            .iter()
            .position(|n| n.start == step && n.pitch == pitch)
    }

    /// Removes every note.
    pub fn clear(&mut self) {
        *self = Self::EMPTY;
    }

    /// Clamps the count and every note into range, clears the unused slots and sorts.
    pub fn normalise(&mut self) {
        let count = usize::from(self.note_count).min(MAX_NOTES);
        self.note_count = count as u8;
        for note in &mut self.notes[..count] {
            note.normalise();
        }
        for slot in &mut self.notes[count..] {
            *slot = Note::default();
        }
        self.sort();
    }

    /// Inserts a clamped note at its sorted position and returns its index. Rejects a full
    /// list and `length == 0` (returns `None`). No pattern-length check: use
    /// [`Pattern::add_note`] for that.
    pub fn insert(&mut self, mut note: Note) -> Option<usize> {
        let count = self.notes().len();
        if count >= MAX_NOTES || note.length == 0 {
            return None;
        }
        note.normalise();
        let key = note.sort_key();
        let index = self.notes[..count]
            .iter()
            .position(|n| n.sort_key() > key)
            .unwrap_or(count);
        self.notes[index..=count].rotate_right(1);
        self.notes[index] = note;
        self.note_count = count as u8 + 1;
        Some(index)
    }

    /// Removes note `index` (`0..note_count`), shifting later notes down. Returns `false`
    /// when `index` is out of range.
    pub fn remove(&mut self, index: usize) -> bool {
        let count = self.notes().len();
        if index >= count {
            return false;
        }
        self.notes[index..count].rotate_left(1);
        self.notes[count - 1] = Note::default();
        self.note_count = count as u8 - 1;
        true
    }

    /// Removes every note starting at `step`.
    pub fn remove_starting_at(&mut self, step: u8) {
        let count = self.notes().len();
        let mut kept = 0;
        for i in 0..count {
            if self.notes[i].start != step {
                self.notes[kept] = self.notes[i];
                kept += 1;
            }
        }
        for slot in &mut self.notes[kept..count] {
            *slot = Note::default();
        }
        self.note_count = kept as u8;
    }

    /// `start + 1` of the note starting last, or `None` when empty.
    fn last_used_step(&self) -> Option<usize> {
        self.notes().iter().map(|n| usize::from(n.start) + 1).max()
    }

    /// Sorts the valid notes by [`Note::sort_key`] (allocation-free).
    fn sort(&mut self) {
        let count = self.notes().len();
        self.notes[..count].sort_unstable_by_key(|n| n.sort_key());
    }
}

/// One pattern: five drum rows plus one note list per instrument track (and the derived step
/// lane of one of them), 16/32/64 steps long.
///
/// Equality compares the content (length, drums, every track's notes, volume, tune); the
/// derived `synth` lane and `lane_track` are views and are ignored.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Length in steps: 16, 32 or 64 (see [`PatternLength`]). Steps at and beyond this
    /// index are kept but not played. Always valid when set through
    /// [`set_length`](Self::set_length) / [`Bank::normalise`]; [`len`](Self::len) is the
    /// defensive accessor.
    pub length: u8,
    /// Drum rows in [`DrumRow::ALL`] order, [`MAX_STEPS`] steps each.
    pub drums: [[DrumStep; MAX_STEPS]; DrumRow::COUNT],
    /// The bass/melody lane, [`MAX_STEPS`] steps: a derived view of track
    /// [`lane_track`](Self::lane_track)'s notes (the lowest note starting at each step),
    /// rebuilt after every note edit and by [`rebuild_lane`](Self::rebuild_lane). Read it;
    /// edit through [`set_synth_step`](Self::set_synth_step) or the note helpers.
    pub synth: [SynthStep; MAX_STEPS],
    /// The track whose notes `synth` mirrors (`0..MAX_TRACKS`); the engine keeps it at its
    /// edit track. Not serialised (a loaded pattern shows track 0).
    pub lane_track: TrackId,
    /// One note list per instrument track, indexed by track slot.
    pub tracks: [NoteTrack; MAX_TRACKS],
    /// Per-row drum volume in dB ([`DRUM_VOLUME_RANGE`]).
    pub drum_volume: [f32; DrumRow::COUNT],
    /// Per-row drum tune in semitones ([`DRUM_TUNE_RANGE`]).
    pub drum_tune: [f32; DrumRow::COUNT],
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.length == other.length
            && self.drums == other.drums
            && self.tracks == other.tracks
            && self.drum_volume == other.drum_volume
            && self.drum_tune == other.drum_tune
    }
}

impl Default for Pattern {
    /// An empty one-bar pattern.
    fn default() -> Self {
        Pattern {
            length: PatternLength::Bar1.steps(),
            drums: [[DrumStep::default(); MAX_STEPS]; DrumRow::COUNT],
            synth: [SynthStep::default(); MAX_STEPS],
            lane_track: 0,
            tracks: [NoteTrack::EMPTY; MAX_TRACKS],
            drum_volume: [0.0; DrumRow::COUNT],
            drum_tune: [0.0; DrumRow::COUNT],
        }
    }
}

impl Pattern {
    /// An empty one-bar pattern.
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
            if let Some(n) = n {
                p.set_synth_step(0, i as u8, Some(n), false);
            }
        }
        p
    }

    /// A variation of [`demo`](Self::demo) for pattern B of [`Bank::demo`]: the same kick,
    /// a snare on 2 and 4 with a ghost note before the fourth beat, sixteenth-note closed
    /// hats, and the bassline a fourth up (F2).
    #[must_use]
    pub fn demo_variation() -> Self {
        let mut p = Pattern::demo();
        let rows: [(DrumRow, &str); 4] = [
            (DrumRow::Snare, "0000100000001001"),
            (DrumRow::Clap, "0000000000001000"),
            (DrumRow::ClosedHat, "1111111111111111"),
            (DrumRow::OpenHat, "0000001000000010"),
        ];
        for (row, bits) in rows {
            for (i, b) in bits.bytes().enumerate().take(STEPS) {
                p.drums[row.index()][i].on = b == b'1';
            }
        }
        let count = p.tracks[0].notes().len();
        for note in &mut p.tracks[0].notes[..count] {
            note.pitch = (note.pitch + 5).min(127);
        }
        p.tracks[0].sort();
        p.refresh_lane();
        p
    }

    /// Length in steps, clamped into `1..=MAX_STEPS` (defensive: `length` is a public field).
    #[must_use]
    pub fn len(&self) -> usize {
        usize::from(self.length).clamp(1, MAX_STEPS)
    }

    /// Number of 16-step bars this pattern spans (`length / 16`, at least 1).
    #[must_use]
    pub fn bars(&self) -> usize {
        (self.len() / BAR_STEPS).max(1)
    }

    /// The pattern's [`PatternLength`] (the shortest one covering `length`).
    #[must_use]
    pub fn pattern_length(&self) -> PatternLength {
        PatternLength::covering(self.len())
    }

    /// Sets the length to 1, 2 or 4 bars. Steps beyond the new length are kept.
    pub fn set_length(&mut self, length: PatternLength) {
        self.length = length.steps();
    }

    /// Sets the length in steps; anything but 16/32/64 rounds up to the next allowed length
    /// (above 64 clamps to 64).
    pub fn set_length_steps(&mut self, steps: u8) {
        self.set_length(
            PatternLength::from_steps(steps)
                .unwrap_or_else(|| PatternLength::covering(usize::from(steps.max(1)))),
        );
    }

    /// Steps worth serialising: the length, or the shortest [`PatternLength`] covering the
    /// last non-empty step when data lives beyond the length.
    #[must_use]
    pub fn used_steps(&self) -> usize {
        let last_drum = self
            .drums
            .iter()
            .flat_map(|row| row.iter().rposition(|s| *s != DrumStep::default()))
            .max();
        let last_note = self
            .tracks
            .iter()
            .filter_map(NoteTrack::last_used_step)
            .max();
        let used = last_drum
            .map(|i| i + 1)
            .into_iter()
            .chain(last_note)
            .max()
            .unwrap_or(0);
        if used > self.len() {
            usize::from(PatternLength::covering(used).steps())
        } else {
            self.len()
        }
    }

    /// Clamps the length to a valid value, every note into range (sorted, unused slots
    /// cleared, the lane rebuilt) and volume/tune to their ranges (non-finite values become
    /// 0).
    pub fn normalise(&mut self) {
        self.set_length_steps(self.length);
        for track in &mut self.tracks {
            track.normalise();
        }
        self.lane_track = track_index(self.lane_track) as u8;
        self.refresh_lane();
        for i in 0..DrumRow::COUNT {
            let v = self.drum_volume[i];
            self.drum_volume[i] = if v.is_finite() {
                v.clamp(*DRUM_VOLUME_RANGE.start(), *DRUM_VOLUME_RANGE.end())
            } else {
                0.0
            };
            let t = self.drum_tune[i];
            self.drum_tune[i] = if t.is_finite() {
                t.clamp(*DRUM_TUNE_RANGE.start(), *DRUM_TUNE_RANGE.end())
            } else {
                0.0
            };
        }
    }

    /// Clears every step of one drum row, hidden steps included (volume and tune are kept).
    pub fn clear_row(&mut self, row: DrumRow) {
        self.drums[row.index()] = [DrumStep::default(); MAX_STEPS];
    }

    /// Clears every step of every drum row, hidden steps included (notes, length, volume and
    /// tune are kept).
    pub fn clear_rows(&mut self) {
        for row in DrumRow::ALL {
            self.clear_row(row);
        }
    }

    /// Clears every step of every row and every track's notes (length, volume and tune are
    /// kept). The grid's "clear all" button clears the rows plus the *edit* track only; see
    /// `Engine::clear_all`.
    pub fn clear_all(&mut self) {
        self.clear_rows();
        self.clear_all_notes();
    }

    /// `true` when no drum step (within the length or beyond it) is active and no track holds
    /// a note.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drums.iter().flatten().all(|s| !s.on) && self.tracks.iter().all(NoteTrack::is_empty)
    }

    /// Sets one drum step (step index wraps into `0..length`).
    pub fn set_drum_step(&mut self, row: DrumRow, step: u8, on: bool, accent: bool) {
        let i = usize::from(step) % self.len();
        self.drums[row.index()][i] = DrumStep { on, accent };
    }

    /// Sets one synth step of track `track`'s grid lane (step index wraps into `0..length`):
    /// every note of that track starting at that step is removed, then `note` (if any) is
    /// added as a length-1 note at [`ACCENT_VELOCITY`] / [`STEP_VELOCITY`], and the lane is
    /// rebuilt so `synth[step] == SynthStep { note, accent }` afterwards when `track` is the
    /// lane track.
    pub fn set_synth_step(&mut self, track: TrackId, step: u8, note: Option<u8>, accent: bool) {
        let i = (usize::from(step) % self.len()) as u8;
        let t = &mut self.tracks[track_index(track)];
        t.remove_starting_at(i);
        if let Some(pitch) = note {
            t.insert(Note::step(i, pitch, accent));
        }
        self.refresh_lane();
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

    // --- notes (M7, per track since M10) --------------------------------------------------

    /// The note list of track `track` (wraps into `0..MAX_TRACKS`).
    #[must_use]
    pub fn track(&self, track: TrackId) -> &NoteTrack {
        &self.tracks[track_index(track)]
    }

    /// The valid notes of track `track`, sorted by start then pitch.
    #[must_use]
    pub fn notes(&self, track: TrackId) -> &[Note] {
        self.track(track).notes()
    }

    /// Total number of notes over every track.
    #[must_use]
    pub fn note_count(&self) -> usize {
        self.tracks.iter().map(|t| t.notes().len()).sum()
    }

    /// Adds a note to track `track` (clamped with [`Note::normalise`]) and returns its index
    /// in that track's sorted list, or `None` when the track already holds [`MAX_NOTES`]
    /// notes, the note has `length == 0`, or it starts at or beyond the pattern
    /// [`length`](Self::length). The lane is rebuilt.
    pub fn add_note(&mut self, track: TrackId, note: Note) -> Option<usize> {
        if usize::from(note.start) >= self.len() {
            return None;
        }
        let index = self.tracks[track_index(track)].insert(note)?;
        self.refresh_lane();
        Some(index)
    }

    /// Removes note `index` (`0..note_count`) of track `track`, shifting later notes down.
    /// Returns `false` when `index` is out of range. The lane is rebuilt.
    pub fn remove_note(&mut self, track: TrackId, index: usize) -> bool {
        let removed = self.tracks[track_index(track)].remove(index);
        if removed {
            self.refresh_lane();
        }
        removed
    }

    /// Replaces note `index` (`0..note_count`) of track `track` with `note` (clamped) and
    /// returns the note's new index after re-sorting, or `None` (nothing changes) when `index`
    /// is out of range or `note` is invalid (`length == 0` or `start >= length` of the
    /// pattern). The lane is rebuilt.
    pub fn set_note(&mut self, track: TrackId, index: usize, note: Note) -> Option<usize> {
        let len = self.len();
        let t = &mut self.tracks[track_index(track)];
        let count = t.notes().len();
        if index >= count || note.length == 0 || usize::from(note.start) >= len {
            return None;
        }
        // Remove then re-insert; the list cannot be full after the removal.
        t.remove(index);
        let new_index = t.insert(note);
        self.refresh_lane();
        new_index
    }

    /// Index of the first note of track `track` starting at `step` with MIDI `pitch`, if any.
    #[must_use]
    pub fn note_at(&self, track: TrackId, step: u8, pitch: u8) -> Option<usize> {
        self.track(track).note_at(step, pitch)
    }

    /// Removes every note of track `track` (and clears the lane when it is the lane track).
    pub fn clear_notes(&mut self, track: TrackId) {
        self.tracks[track_index(track)].clear();
        self.refresh_lane();
    }

    /// Removes every note of every track and clears the lane.
    pub fn clear_all_notes(&mut self) {
        self.tracks = [NoteTrack::EMPTY; MAX_TRACKS];
        self.refresh_lane();
    }

    /// Swaps the note lists of two tracks (the lane follows `lane_track`).
    pub fn swap_tracks(&mut self, a: TrackId, b: TrackId) {
        self.tracks.swap(track_index(a), track_index(b));
        self.refresh_lane();
    }

    /// Makes `track` the lane track and recomputes the grid lane from its notes: `synth[s]`
    /// is the lowest-pitched note starting at step `s` (accent when its velocity is at least
    /// [`ACCENT_THRESHOLD`]), a rest where no note starts. Every note helper rebuilds the lane
    /// for the current lane track; call this to switch tracks or after writing a
    /// [`NoteTrack`]'s fields by hand (prefer [`normalise`](Self::normalise), which also
    /// sorts).
    pub fn rebuild_lane(&mut self, track: TrackId) {
        self.lane_track = track_index(track) as u8;
        self.refresh_lane();
    }

    /// Rebuilds the lane for the current lane track.
    fn refresh_lane(&mut self) {
        self.synth = [SynthStep::default(); MAX_STEPS];
        let t = &self.tracks[track_index(self.lane_track)];
        // Independent of the note order (by-hand edits may be unsorted): the lowest pitch
        // starting at a step wins, ties keep the first (lowest sort key) note.
        let count = usize::from(t.note_count).min(MAX_NOTES);
        for i in 0..count {
            let note = t.notes[i];
            let s = usize::from(note.start);
            if s < MAX_STEPS && self.synth[s].note.is_none_or(|p| note.pitch.min(127) < p) {
                self.synth[s] = note.lane_step();
            }
        }
    }
}

// --- Pattern serde ------------------------------------------------------------------------

/// On-disk shape of a [`Pattern`]: drum rows are plain arrays of any length up to
/// [`MAX_STEPS`]; `tracks` holds one entry per non-empty track (format version 7). `notes`
/// (versions 5 and 6, track 0) and `synth` (version 4 and older) are only read.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PatternRepr {
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<u8>,
    drums: Vec<Vec<DrumStep>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    synth: Vec<SynthStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<Vec<Note>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tracks: Vec<NoteTrackRepr>,
    drum_volume: [f32; DrumRow::COUNT],
    drum_tune: [f32; DrumRow::COUNT],
}

/// One `tracks` entry of a serialised pattern: the slot and its notes.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct NoteTrackRepr {
    track: u8,
    notes: Vec<Note>,
}

impl Default for PatternRepr {
    fn default() -> Self {
        Self {
            length: None,
            drums: vec![Vec::new(); DrumRow::COUNT],
            synth: Vec::new(),
            notes: None,
            tracks: Vec::new(),
            drum_volume: [0.0; DrumRow::COUNT],
            drum_tune: [0.0; DrumRow::COUNT],
        }
    }
}

/// Validates and inserts a serialised note list into one track; returns the step after the
/// last note start (for the inferred length).
fn load_notes<E: serde::de::Error>(track: &mut NoteTrack, notes: Vec<Note>) -> Result<usize, E> {
    if notes.len() + track.notes().len() > MAX_NOTES {
        return Err(E::custom(format!(
            "pattern track has {} notes, at most {MAX_NOTES} are allowed",
            notes.len() + track.notes().len()
        )));
    }
    let mut longest = 0;
    for note in notes {
        if note.length == 0 {
            return Err(E::custom("note length must be at least 1"));
        }
        if usize::from(note.start) >= MAX_STEPS {
            return Err(E::custom(format!(
                "note starts at step {}, the last step is {}",
                note.start,
                MAX_STEPS - 1
            )));
        }
        longest = longest.max(usize::from(note.start) + 1);
        // Cannot fail: the count was checked and the length is non-zero.
        track.insert(note);
    }
    Ok(longest)
}

/// Copies `src` into the front of a fixed-size row; the rest stays empty.
fn fill_row<T: Copy + Default, E: serde::de::Error>(
    what: &str,
    dst: &mut [T; MAX_STEPS],
    src: &[T],
) -> Result<(), E> {
    if src.len() > MAX_STEPS {
        return Err(E::custom(format!(
            "{what} has {} steps, at most {MAX_STEPS} are allowed",
            src.len()
        )));
    }
    *dst = [T::default(); MAX_STEPS];
    dst[..src.len()].copy_from_slice(src);
    Ok(())
}

impl Serialize for Pattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let n = self.used_steps();
        PatternRepr {
            length: Some(self.len() as u8),
            drums: self.drums.iter().map(|row| row[..n].to_vec()).collect(),
            synth: Vec::new(),
            notes: None,
            tracks: self
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.is_empty())
                .map(|(i, t)| NoteTrackRepr {
                    track: i as u8,
                    notes: t.notes().to_vec(),
                })
                .collect(),
            drum_volume: self.drum_volume,
            drum_tune: self.drum_tune,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = PatternRepr::deserialize(deserializer)?;
        if repr.drums.len() > DrumRow::COUNT {
            return Err(D::Error::custom(format!(
                "pattern has {} drum rows, at most {} are allowed",
                repr.drums.len(),
                DrumRow::COUNT
            )));
        }
        let mut p = Pattern {
            drum_volume: repr.drum_volume,
            drum_tune: repr.drum_tune,
            ..Pattern::default()
        };
        let mut longest = 0;
        for (i, row) in repr.drums.iter().enumerate() {
            fill_row(DrumRow::ALL[i].label(), &mut p.drums[i], row)?;
            longest = longest.max(row.len());
        }
        if !repr.tracks.is_empty() {
            // Version 7: one entry per non-empty track.
            for entry in repr.tracks {
                if usize::from(entry.track) >= MAX_TRACKS {
                    return Err(D::Error::custom(format!(
                        "pattern track {} is out of range, the last track is {}",
                        entry.track,
                        MAX_TRACKS - 1
                    )));
                }
                let used = load_notes(&mut p.tracks[usize::from(entry.track)], entry.notes)?;
                longest = longest.max(used);
            }
        } else if let Some(notes) = repr.notes {
            // Versions 5 and 6: the single synth's notes become track 0.
            longest = longest.max(load_notes(&mut p.tracks[0], notes)?);
        } else {
            // Version 4 and older: the lane is the data. Each step with a note becomes a
            // length-1 note (accent -> ACCENT_VELOCITY, else STEP_VELOCITY) on track 0.
            let mut lane = [SynthStep::default(); MAX_STEPS];
            fill_row("synth lane", &mut lane, &repr.synth)?;
            longest = longest.max(repr.synth.len());
            for (step, cell) in lane.iter().enumerate() {
                if let Some(pitch) = cell.note {
                    p.tracks[0].insert(Note::step(step as u8, pitch, cell.accent));
                }
            }
        }
        match repr.length {
            Some(steps) => p.set_length_steps(steps),
            None => p.set_length(PatternLength::covering(longest)),
        }
        p.normalise();
        Ok(p)
    }
}

// --- Bank ---------------------------------------------------------------------------------

/// Pattern mode loops one pattern; song mode follows the chain.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayMode {
    /// Loop the current pattern (switches are queued to the next bar).
    #[default]
    Pattern,
    /// Play the chain, entry by entry, wrapping at the end.
    Song,
}

impl PlayMode {
    /// The other mode.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            PlayMode::Pattern => PlayMode::Song,
            PlayMode::Song => PlayMode::Pattern,
        }
    }
}

/// One step of the song chain: play pattern `pattern` `repeats` times.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ChainEntry {
    /// Pattern index `0..PATTERN_COUNT` (A = 0).
    pub pattern: u8,
    /// Repeat count `1..=MAX_REPEATS`.
    pub repeats: u8,
}

impl Default for ChainEntry {
    /// Pattern A once.
    fn default() -> Self {
        Self {
            pattern: 0,
            repeats: 1,
        }
    }
}

impl ChainEntry {
    /// `pattern` played `repeats` times (both clamped into range).
    #[must_use]
    pub fn new(pattern: u8, repeats: u8) -> Self {
        let mut e = Self { pattern, repeats };
        e.normalise();
        e
    }

    /// Clamps the pattern index and repeat count into range.
    pub fn normalise(&mut self) {
        self.pattern = self.pattern.min(PATTERN_COUNT as u8 - 1);
        self.repeats = self.repeats.clamp(1, MAX_REPEATS);
    }

    /// Letter of the pattern (`'A'..='H'`).
    #[must_use]
    pub fn letter(self) -> char {
        pattern_letter(self.pattern)
    }
}

/// Letter of pattern index `index` (`'A'..='H'`; out-of-range indices wrap).
#[must_use]
pub fn pattern_letter(index: u8) -> char {
    char::from(b'A' + index % PATTERN_COUNT as u8)
}

/// Eight patterns, the song chain, the play mode and the selected pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Bank {
    /// Patterns A..H.
    pub patterns: [Pattern; PATTERN_COUNT],
    /// The song chain; only the first `chain_len` entries are used.
    pub chain: [ChainEntry; MAX_CHAIN],
    /// Number of valid chain entries, `0..=MAX_CHAIN`.
    pub chain_len: u8,
    /// Pattern or song mode.
    pub mode: PlayMode,
    /// The selected pattern `0..PATTERN_COUNT`: what pattern mode plays (and loops when the
    /// chain is empty).
    pub current: u8,
}

impl Default for Bank {
    /// The demo beat in A, B..H empty, chain `[A x1]`, pattern mode, A selected.
    fn default() -> Self {
        Self::from_pattern(Pattern::demo())
    }
}

impl Bank {
    /// `pattern` in A, B..H empty, chain `[A x1]`, pattern mode, A selected. This is also the
    /// version-2 project migration.
    #[must_use]
    pub fn from_pattern(pattern: Pattern) -> Self {
        let mut patterns: [Pattern; PATTERN_COUNT] = Default::default();
        patterns[0] = pattern;
        let mut bank = Self {
            patterns,
            chain: [ChainEntry::default(); MAX_CHAIN],
            chain_len: 1,
            mode: PlayMode::Pattern,
            current: 0,
        };
        bank.normalise();
        bank
    }

    /// Every pattern empty, chain `[A x1]`, pattern mode, A selected.
    #[must_use]
    pub fn empty() -> Self {
        Self::from_pattern(Pattern::empty())
    }

    /// [`Bank::default`] plus a variation in B and the chain `A A B A` (each once).
    #[must_use]
    pub fn demo() -> Self {
        let mut bank = Self::default();
        bank.patterns[1] = Pattern::demo_variation();
        bank.chain_len = 0;
        for p in [0, 0, 1, 0] {
            bank.push(ChainEntry::new(p, 1));
        }
        bank
    }

    /// Pattern `index` (wraps into `0..PATTERN_COUNT`).
    #[must_use]
    pub fn pattern(&self, index: u8) -> &Pattern {
        &self.patterns[usize::from(index) % PATTERN_COUNT]
    }

    /// Pattern `index`, mutably (wraps into `0..PATTERN_COUNT`).
    pub fn pattern_mut(&mut self, index: u8) -> &mut Pattern {
        &mut self.patterns[usize::from(index) % PATTERN_COUNT]
    }

    /// The selected pattern.
    #[must_use]
    pub fn current_pattern(&self) -> &Pattern {
        self.pattern(self.current)
    }

    /// The selected pattern, mutably.
    pub fn current_pattern_mut(&mut self) -> &mut Pattern {
        self.pattern_mut(self.current)
    }

    /// Copies pattern `from` over pattern `to` (length, steps, volume and tune).
    pub fn copy_pattern(&mut self, from: u8, to: u8) {
        let (from, to) = (
            usize::from(from) % PATTERN_COUNT,
            usize::from(to) % PATTERN_COUNT,
        );
        if from != to {
            self.patterns[to] = self.patterns[from].clone();
        }
    }

    /// Clears every step of pattern `index` (length, volume and tune are kept).
    pub fn clear_pattern(&mut self, index: u8) {
        self.pattern_mut(index).clear_all();
    }

    /// Sets the length of pattern `index`.
    pub fn set_length(&mut self, index: u8, length: PatternLength) {
        self.pattern_mut(index).set_length(length);
    }

    /// The valid part of the chain.
    #[must_use]
    pub fn chain_entries(&self) -> &[ChainEntry] {
        &self.chain[..usize::from(self.chain_len).min(MAX_CHAIN)]
    }

    /// Appends a chain entry. Returns `false` when the chain is full.
    pub fn push(&mut self, entry: ChainEntry) -> bool {
        let len = usize::from(self.chain_len).min(MAX_CHAIN);
        if len >= MAX_CHAIN {
            return false;
        }
        let mut entry = entry;
        entry.normalise();
        self.chain[len] = entry;
        self.chain_len = len as u8 + 1;
        true
    }

    /// Sets chain entry `index` (`0..MAX_CHAIN`), extending `chain_len` to cover it (any gap
    /// is filled with `A x1`). Returns `false` for an out-of-range index.
    pub fn set_entry(&mut self, index: u8, entry: ChainEntry) -> bool {
        let index = usize::from(index);
        if index >= MAX_CHAIN {
            return false;
        }
        let mut entry = entry;
        entry.normalise();
        self.chain[index] = entry;
        let len = usize::from(self.chain_len).min(MAX_CHAIN);
        if index >= len {
            for e in &mut self.chain[len..index] {
                *e = ChainEntry::default();
            }
            self.chain_len = index as u8 + 1;
        }
        true
    }

    /// Shortens the chain to `len` entries (no-op when it is already shorter).
    pub fn truncate(&mut self, len: u8) {
        self.chain_len = self.chain_len.min(len).min(MAX_CHAIN as u8);
    }

    /// Removes chain entry `index`, shifting later entries down. Returns `false` when
    /// `index` is past the end.
    pub fn remove_entry(&mut self, index: u8) -> bool {
        let len = usize::from(self.chain_len).min(MAX_CHAIN);
        let index = usize::from(index);
        if index >= len {
            return false;
        }
        self.chain[index..len].rotate_left(1);
        self.chain_len = len as u8 - 1;
        true
    }

    /// Bars in one pass of the song: the sum over the chain of `repeats * bars(pattern)`,
    /// or the current pattern's bars when the chain is empty.
    #[must_use]
    pub fn song_bars(&self) -> usize {
        if self.chain_len == 0 {
            return self.current_pattern().bars();
        }
        self.chain_entries()
            .iter()
            .map(|e| usize::from(e.repeats.max(1)) * self.pattern(e.pattern).bars())
            .sum()
    }

    /// Steps in one pass of the song (`song_bars() * 16`).
    #[must_use]
    pub fn song_steps(&self) -> usize {
        self.song_bars() * BAR_STEPS
    }

    /// Clamps every index, repeat count and pattern length into range.
    pub fn normalise(&mut self) {
        for p in &mut self.patterns {
            p.normalise();
        }
        for e in &mut self.chain {
            e.normalise();
        }
        self.chain_len = self.chain_len.min(MAX_CHAIN as u8);
        self.current = self.current.min(PATTERN_COUNT as u8 - 1);
    }
}

/// On-disk shape of a [`Bank`]: `chain` holds only the valid entries.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct BankRepr {
    patterns: Vec<Pattern>,
    chain: Vec<ChainEntry>,
    mode: PlayMode,
    current: u8,
}

impl Default for BankRepr {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            chain: vec![ChainEntry::default()],
            mode: PlayMode::Pattern,
            current: 0,
        }
    }
}

impl Serialize for Bank {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BankRepr {
            patterns: self.patterns.to_vec(),
            chain: self.chain_entries().to_vec(),
            mode: self.mode,
            current: self.current,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bank {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = BankRepr::deserialize(deserializer)?;
        if repr.patterns.len() > PATTERN_COUNT {
            return Err(D::Error::custom(format!(
                "bank has {} patterns, at most {PATTERN_COUNT} are allowed",
                repr.patterns.len()
            )));
        }
        if repr.chain.len() > MAX_CHAIN {
            return Err(D::Error::custom(format!(
                "chain has {} entries, at most {MAX_CHAIN} are allowed",
                repr.chain.len()
            )));
        }
        let mut bank = Bank::empty();
        for (slot, p) in bank.patterns.iter_mut().zip(repr.patterns) {
            *slot = p;
        }
        bank.chain_len = 0;
        for e in repr.chain {
            bank.push(e);
        }
        bank.mode = repr.mode;
        bank.current = repr.current;
        bank.normalise();
        Ok(bank)
    }
}

impl fmt::Display for PlayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PlayMode::Pattern => "Pattern",
            PlayMode::Song => "Song",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_pattern_has_the_expected_hits() {
        let p = Pattern::demo();
        assert!(!p.is_empty());
        assert_eq!(p.len(), 16);
        assert_eq!(p.bars(), 1);
        assert!(p.drums[DrumRow::Kick.index()][0].on);
        assert!(!p.drums[DrumRow::Kick.index()][1].on);
        assert!(p.drums[DrumRow::Snare.index()][4].on);
        assert_eq!(p.synth[0].note, Some(36));
        assert_eq!(p.synth[1].note, None);
        assert_eq!(p.notes(0).len(), 9);
        assert!(p.tracks[1..].iter().all(NoteTrack::is_empty));
        assert_eq!(p.note_count(), 9);
        let v = Pattern::demo_variation();
        assert_ne!(v, p);
        assert_eq!(v.synth[0].note, Some(41));
    }

    #[test]
    fn clearing_and_setting_steps() {
        let mut p = Pattern::demo();
        p.clear_row(DrumRow::Kick);
        assert!(p.drums[DrumRow::Kick.index()].iter().all(|s| !s.on));
        p.set_drum_step(DrumRow::Kick, 17, true, true); // wraps to step 1 (length 16)
        assert_eq!(
            p.drums[DrumRow::Kick.index()][1],
            DrumStep {
                on: true,
                accent: true
            }
        );
        p.set_length(PatternLength::Bar2);
        p.set_drum_step(DrumRow::Kick, 17, true, false); // now a real step
        assert!(p.drums[DrumRow::Kick.index()][17].on);
        p.set_synth_step(0, 3, Some(200), false);
        assert_eq!(p.synth[3].note, Some(127));
        p.set_drum_volume(DrumRow::Snare, 99.0);
        assert_eq!(p.drum_volume[DrumRow::Snare.index()], 6.0);
        p.set_drum_tune(DrumRow::Snare, f32::NAN);
        assert_eq!(p.drum_tune[DrumRow::Snare.index()], 0.0);
        p.add_note(2, Note::new(1, 1, 50, 0.8)).unwrap();
        p.clear_rows();
        assert!(p.drums.iter().flatten().all(|s| !s.on));
        assert_eq!(p.note_count(), 11, "clear_rows keeps the notes");
        p.clear_all();
        assert!(p.is_empty());
        assert_eq!(p.len(), 32, "clear keeps the length");
    }

    #[test]
    fn lengths() {
        assert_eq!(PatternLength::from_steps(32), Some(PatternLength::Bar2));
        assert_eq!(PatternLength::from_steps(20), None);
        assert_eq!(PatternLength::covering(0), PatternLength::Bar1);
        assert_eq!(PatternLength::covering(17), PatternLength::Bar2);
        assert_eq!(PatternLength::covering(33), PatternLength::Bar4);
        assert_eq!(PatternLength::covering(999), PatternLength::Bar4);
        let mut p = Pattern::default();
        p.set_length_steps(20);
        assert_eq!(p.length, 32);
        p.set_length_steps(0);
        assert_eq!(p.length, 16);
        p.set_length_steps(200);
        assert_eq!(p.length, 64);
        p.length = 0;
        assert_eq!(p.len(), 1);
        p.normalise();
        assert_eq!(p.length, 16);
        assert_eq!(PatternLength::Bar4.bars(), 4);
        assert_eq!(PatternLength::Bar2.label(), "2 bars");
    }

    #[test]
    fn pattern_round_trips_through_json() {
        let p = Pattern::demo();
        let json = serde_json::to_string(&p).unwrap();
        let back: Pattern = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
        // A 16-step pattern writes 16 entries per drum row; the notes are written per
        // non-empty track and the derived lane is not written.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["length"], 16);
        assert_eq!(value["drums"][0].as_array().unwrap().len(), 16);
        assert!(value.get("synth").is_none(), "{json}");
        assert!(value.get("notes").is_none(), "{json}");
        let tracks = value["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 1, "only the non-empty track is written");
        assert_eq!(tracks[0]["track"], 0);
        let notes = tracks[0]["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 9, "the demo bassline has nine notes");
        assert_eq!(
            notes[0],
            serde_json::json!({ "start": 0, "length": 1, "pitch": 36, "velocity": 0.8 })
        );
        // An empty pattern writes no `tracks` at all.
        let value = serde_json::to_value(Pattern::default()).unwrap();
        assert!(value.get("tracks").is_none());
    }

    #[test]
    fn tracks_round_trip_and_validate() {
        let mut p = Pattern::demo();
        p.add_note(3, Note::new(2, 4, 72, 0.6)).unwrap();
        p.add_note(3, Note::new(2, 4, 76, 0.6)).unwrap();
        p.add_note(7, Note::new(15, 1, 40, 1.0)).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let tracks = value["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[1]["track"], 3);
        assert_eq!(tracks[1]["notes"].as_array().unwrap().len(), 2);
        assert_eq!(tracks[2]["track"], 7);
        let back: Pattern = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.notes(3), p.notes(3));
        assert_eq!(back.lane_track, 0, "a loaded pattern shows track 0");
        assert_eq!(back.synth[0].note, Some(36));

        // A v6 `notes` list lands on track 0; `tracks` wins over a stray `notes`.
        let v = serde_json::json!({ "notes": [{ "start": 1, "length": 1, "pitch": 60 }] });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p.notes(0), &[Note::new(1, 1, 60, STEP_VELOCITY)]);
        let v = serde_json::json!({
            "notes": [{ "start": 1, "length": 1, "pitch": 60 }],
            "tracks": [{ "track": 2, "notes": [{ "start": 5, "length": 2, "pitch": 70 }] }]
        });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert!(p.notes(0).is_empty());
        assert_eq!(p.notes(2), &[Note::new(5, 2, 70, STEP_VELOCITY)]);
        assert_eq!(p.synth[5].note, None, "the lane shows track 0");

        // Duplicate entries for one track merge; an out-of-range track or too many notes
        // in total is an error.
        let v = serde_json::json!({ "tracks": [
            { "track": 1, "notes": [{ "start": 0, "length": 1, "pitch": 60 }] },
            { "track": 1, "notes": [{ "start": 0, "length": 1, "pitch": 64 }] }
        ] });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p.notes(1).len(), 2);
        let v = serde_json::json!({ "tracks": [{ "track": 8, "notes": [] }] });
        assert!(serde_json::from_value::<Pattern>(v).is_err());
        let many: Vec<Note> = vec![Note::default(); MAX_NOTES];
        let v = serde_json::json!({ "tracks": [
            { "track": 1, "notes": many },
            { "track": 1, "notes": [Note::default()] }
        ] });
        assert!(serde_json::from_value::<Pattern>(v).is_err());
    }

    #[test]
    fn hidden_steps_beyond_the_length_survive_a_round_trip() {
        let mut p = Pattern::demo();
        p.set_length(PatternLength::Bar2);
        p.set_drum_step(DrumRow::Kick, 30, true, false);
        p.set_length(PatternLength::Bar1);
        let json = serde_json::to_string(&p).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["length"], 16);
        assert_eq!(value["drums"][0].as_array().unwrap().len(), 32);
        let back: Pattern = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.len(), 16);
        assert!(back.drums[DrumRow::Kick.index()][30].on);
    }

    #[test]
    fn rows_are_padded_inferred_and_bounded() {
        // Version-2 shape: no length, 16 entries.
        let v2 = serde_json::json!({
            "drums": [[{"on": true, "accent": false}], [], [], [], []],
            "synth": [{"note": 60, "accent": true}],
            "drum_volume": [0.0, -3.0, 0.0, 0.0, 0.0],
            "drum_tune": [0.0, 0.0, 0.0, 0.0, 0.0]
        });
        let p: Pattern = serde_json::from_value(v2).unwrap();
        assert_eq!(p.len(), 16);
        assert!(p.drums[0][0].on);
        assert!(!p.drums[0][1].on);
        assert_eq!(p.synth[0].note, Some(60));
        assert!(p.synth[0].accent);
        assert_eq!(p.notes(0), &[Note::new(0, 1, 60, ACCENT_VELOCITY)]);
        assert_eq!(p.drum_volume[1], -3.0);

        // A lane of 20 entries with no length (v4 shape) covers two bars; `notes` wins over a
        // stray `synth`.
        let lane: Vec<SynthStep> = (0..20)
            .map(|i| SynthStep {
                note: (i == 19).then_some(40),
                accent: false,
            })
            .collect();
        let v = serde_json::json!({ "drums": [], "synth": lane });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p.len(), 32);
        assert_eq!(p.notes(0), &[Note::new(19, 1, 40, STEP_VELOCITY)]);
        let v = serde_json::json!({
            "drums": [],
            "synth": [{ "note": 60, "accent": true }],
            "notes": [{ "start": 33, "length": 2, "pitch": 70, "velocity": 0.5 }]
        });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p.notes(0), &[Note::new(33, 2, 70, 0.5)]);
        assert_eq!(p.synth[0].note, None);
        assert_eq!(p.len(), 64, "a note at step 33 needs four bars");

        // Too many notes, a zero length or a start beyond the grid are errors; other
        // out-of-range fields are clamped.
        let many: Vec<Note> = vec![Note::default(); MAX_NOTES + 1];
        assert!(serde_json::from_value::<Pattern>(serde_json::json!({ "notes": many })).is_err());
        let v = serde_json::json!({ "notes": [{ "start": 0, "length": 0, "pitch": 1 }] });
        assert!(serde_json::from_value::<Pattern>(v).is_err());
        let v = serde_json::json!({ "notes": [{ "start": 64, "length": 1, "pitch": 1 }] });
        assert!(serde_json::from_value::<Pattern>(v).is_err());
        let v = serde_json::json!({
            "notes": [{ "start": 3, "length": 200, "pitch": 200, "velocity": 7.0 }]
        });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p.notes(0), &[Note::new(3, 64, 127, 1.0)]);

        // No length but 40 entries: the covering length is 64.
        let long: Vec<DrumStep> = vec![DrumStep::default(); 40];
        let v = serde_json::json!({ "drums": [long], "synth": [] });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p.len(), 64);

        // Longer than MAX_STEPS is an error, as are too many rows.
        let too_long: Vec<DrumStep> = vec![DrumStep::default(); 65];
        let v = serde_json::json!({ "drums": [too_long] });
        assert!(serde_json::from_value::<Pattern>(v).is_err());
        let v = serde_json::json!({ "drums": [[], [], [], [], [], []] });
        assert!(serde_json::from_value::<Pattern>(v).is_err());

        // A given length wins over the row count and is normalised.
        let v = serde_json::json!({ "length": 20, "drums": [], "synth": [] });
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p.len(), 32);
        let v = serde_json::json!({});
        let p: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(p, Pattern::default());
    }

    #[test]
    fn used_steps_follows_the_data() {
        let mut p = Pattern::default();
        assert_eq!(p.used_steps(), 16);
        // A note left on a hidden step (added at four bars, then shortened) still counts,
        // whichever track it is on.
        p.set_length(PatternLength::Bar4);
        p.set_synth_step(5, 40, Some(1), false);
        p.set_length(PatternLength::Bar1);
        assert_eq!(p.used_steps(), 64);
        p.clear_notes(5);
        p.drums[2][17].accent = true;
        assert_eq!(p.used_steps(), 32);
        p.set_length(PatternLength::Bar4);
        assert_eq!(p.used_steps(), 64);
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

    #[test]
    fn notes_are_sorted_bounded_and_validated() {
        let mut p = Pattern::default();
        assert!(p.notes(0).is_empty());
        assert_eq!(p.add_note(0, Note::new(4, 2, 72, 0.8)), Some(0));
        assert_eq!(
            p.add_note(0, Note::new(0, 1, 60, 1.0)),
            Some(0),
            "sorted by start"
        );
        assert_eq!(
            p.add_note(0, Note::new(4, 1, 60, 0.8)),
            Some(1),
            "then by pitch"
        );
        assert_eq!(p.notes(0).len(), 3);
        assert_eq!(p.note_at(0, 4, 72), Some(2));
        assert_eq!(p.note_at(0, 4, 61), None);
        // Invalid notes are rejected and change nothing.
        assert_eq!(
            p.add_note(0, Note::new(16, 1, 60, 0.8)),
            None,
            "start >= length"
        );
        assert_eq!(
            p.add_note(
                0,
                Note {
                    start: 1,
                    length: 0,
                    pitch: 60,
                    velocity: 0.8
                }
            ),
            None,
            "zero length"
        );
        assert_eq!(p.notes(0).len(), 3);
        // Out-of-range fields clamp.
        assert_eq!(p.add_note(0, Note::new(2, 200, 200, 9.0)), Some(1));
        assert_eq!(p.notes(0)[1], Note::new(2, 64, 127, 1.0));
        assert_eq!(Note::new(1, 1, 1, f32::NAN).velocity, STEP_VELOCITY);
        assert_eq!(Note::new(1, 1, 1, 0.0).velocity, MIN_VELOCITY);
        assert_eq!(Note::new(2, 3, 1, 0.5).end(), 5);
        assert!(Note::new(0, 1, 1, 0.95).accent());
        assert!(!Note::new(0, 1, 1, 0.9).accent());

        // set_note re-sorts and reports the new index; remove shifts down.
        assert_eq!(p.set_note(0, 3, Note::new(1, 1, 40, 0.8)), Some(1));
        assert_eq!(p.notes(0)[1], Note::new(1, 1, 40, 0.8));
        assert_eq!(p.set_note(0, 9, Note::new(1, 1, 40, 0.8)), None);
        assert_eq!(p.set_note(0, 0, Note::new(20, 1, 40, 0.8)), None, "invalid");
        assert_eq!(p.notes(0)[0], Note::new(0, 1, 60, 1.0), "unchanged");
        assert!(p.remove_note(0, 0));
        assert_eq!(p.notes(0)[0], Note::new(1, 1, 40, 0.8));
        assert!(!p.remove_note(0, 3));
        assert_eq!(p.notes(0).len(), 3);
        assert_eq!(
            p.tracks[0].notes[3],
            Note::default(),
            "unused slots stay empty"
        );

        // Capacity is per track.
        let mut full = Pattern::default();
        full.set_length(PatternLength::Bar4);
        for i in 0..MAX_NOTES {
            assert!(
                full.add_note(0, Note::new((i % 64) as u8, 1, 60, 0.8))
                    .is_some()
            );
        }
        assert_eq!(full.notes(0).len(), MAX_NOTES);
        assert_eq!(full.add_note(0, Note::new(0, 1, 61, 0.8)), None, "full");
        assert_eq!(
            full.add_note(1, Note::new(0, 1, 61, 0.8)),
            Some(0),
            "another track has room"
        );
        full.set_synth_step(0, 0, Some(62), false);
        assert_eq!(
            full.notes(0).len(),
            MAX_NOTES,
            "the upsert replaced one note"
        );
        assert_eq!(full.synth[0].note, Some(62));
        full.clear_notes(0);
        assert!(full.notes(0).is_empty());
        assert!(full.synth.iter().all(|s| *s == SynthStep::default()));
        assert!(!full.is_empty(), "track 1 still has its note");
        full.clear_all_notes();
        assert!(full.is_empty());
    }

    #[test]
    fn lane_is_derived_from_the_lowest_note_per_step() {
        let mut p = Pattern::default();
        p.add_note(0, Note::new(4, 2, 72, 0.8)).unwrap();
        p.add_note(0, Note::new(4, 1, 65, 1.0)).unwrap();
        p.add_note(0, Note::new(4, 3, 60, 0.8)).unwrap();
        p.add_note(0, Note::new(7, 1, 50, 0.96)).unwrap();
        assert_eq!(p.synth[4].note, Some(60), "lowest pitch wins");
        assert!(
            !p.synth[4].accent,
            "the lowest note's velocity decides the accent"
        );
        assert_eq!(
            p.synth[5],
            SynthStep::default(),
            "held steps are rests in the lane"
        );
        assert_eq!(
            p.synth[7],
            SynthStep {
                note: Some(50),
                accent: true
            }
        );
        p.remove_note(0, p.note_at(0, 4, 60).unwrap());
        assert_eq!(
            p.synth[4],
            SynthStep {
                note: Some(65),
                accent: true
            }
        );

        // set_synth_step upserts a length-1 note and removes every other note at that step.
        p.set_synth_step(0, 4, Some(48), false);
        assert_eq!(
            p.notes(0),
            &[Note::step(4, 48, false), Note::new(7, 1, 50, 0.96)]
        );
        assert_eq!(
            p.synth[4],
            SynthStep {
                note: Some(48),
                accent: false
            }
        );
        p.set_synth_step(0, 4, Some(48), true);
        assert_eq!(p.notes(0)[0], Note::step(4, 48, true));
        assert!(p.synth[4].accent);
        p.set_synth_step(0, 20, None, false); // wraps to step 4 in a one-bar pattern
        assert_eq!(p.notes(0), &[Note::new(7, 1, 50, 0.96)]);
        assert_eq!(p.synth[4], SynthStep::default());
        // set_synth_step on a hidden step (after shortening) keeps working, like drum steps.
        p.set_length(PatternLength::Bar2);
        p.set_synth_step(0, 30, Some(33), false);
        p.set_length(PatternLength::Bar1);
        assert_eq!(p.synth[30].note, Some(33), "hidden notes stay in the lane");
        assert_eq!(p.notes(0).len(), 2);
        p.clear_notes(0);
        assert!(p.notes(0).is_empty());

        // rebuild_lane after a by-hand edit.
        p.tracks[0].notes[0] = Note::new(2, 1, 70, 1.0);
        p.tracks[0].note_count = 1;
        assert_eq!(p.synth[2].note, None);
        p.rebuild_lane(0);
        assert_eq!(p.synth[2].note, Some(70));
        assert!(p.synth[2].accent);
    }

    #[test]
    fn lane_follows_the_lane_track() {
        let mut p = Pattern::default();
        p.add_note(0, Note::new(0, 1, 36, 0.8)).unwrap();
        p.add_note(2, Note::new(0, 1, 60, 1.0)).unwrap();
        p.add_note(2, Note::new(3, 1, 64, 0.8)).unwrap();
        assert_eq!(p.synth[0].note, Some(36), "track 0 by default");
        assert_eq!(p.synth[3].note, None);
        p.rebuild_lane(2);
        assert_eq!(p.lane_track, 2);
        assert_eq!(p.synth[0].note, Some(60));
        assert!(p.synth[0].accent);
        assert_eq!(p.synth[3].note, Some(64));
        // Edits on another track leave the lane alone; edits on the lane track update it.
        p.add_note(0, Note::new(3, 1, 40, 0.8)).unwrap();
        assert_eq!(p.synth[3].note, Some(64));
        p.set_synth_step(2, 3, None, false);
        assert_eq!(p.synth[3].note, None);
        assert_eq!(p.notes(0).len(), 2, "track 0 untouched by the lane edit");
        // Grid edits through set_synth_step go to the given track, whatever the lane shows.
        p.set_synth_step(5, 1, Some(70), false);
        assert_eq!(p.notes(5), &[Note::step(1, 70, false)]);
        assert_eq!(p.synth[1].note, None);
        // swap_tracks moves the notes and the lane keeps showing its slot.
        p.swap_tracks(2, 5);
        assert_eq!(p.notes(5)[0].pitch, 60);
        assert_eq!(p.notes(2), &[Note::step(1, 70, false)]);
        assert_eq!(p.synth[1].note, Some(70));
        assert_eq!(p.synth[0].note, None);
        // The lane track wraps and survives normalise; equality ignores the view.
        p.rebuild_lane(10);
        assert_eq!(p.lane_track, 2);
        let mut q = p.clone();
        q.rebuild_lane(0);
        assert_eq!(p, q, "same content, different view");
        assert_ne!(p.synth, q.synth);
        q.lane_track = 200;
        q.normalise();
        assert_eq!(q.lane_track, 0);
    }

    #[test]
    fn copying_a_pattern_carries_its_notes() {
        let mut b = Bank::empty();
        b.patterns[0].add_note(0, Note::new(1, 4, 60, 0.7)).unwrap();
        b.patterns[0].add_note(3, Note::new(1, 4, 64, 0.7)).unwrap();
        b.copy_pattern(0, 2);
        assert_eq!(b.patterns[2].tracks, b.patterns[0].tracks);
        assert_eq!(b.patterns[2].synth[1].note, Some(60));
        b.clear_pattern(2);
        assert!(b.patterns[2].is_empty());
        assert_eq!(b.patterns[0].note_count(), 2);
    }

    #[test]
    fn bank_defaults_and_demo() {
        let b = Bank::default();
        assert_eq!(b.patterns[0], Pattern::demo());
        assert!(b.patterns[1..].iter().all(Pattern::is_empty));
        assert_eq!(b.chain_entries(), &[ChainEntry::new(0, 1)]);
        assert_eq!(b.mode, PlayMode::Pattern);
        assert_eq!(b.current, 0);
        assert_eq!(b.song_bars(), 1);

        let d = Bank::demo();
        assert_eq!(d.patterns[1], Pattern::demo_variation());
        assert_eq!(
            d.chain_entries()
                .iter()
                .map(|e| e.letter())
                .collect::<String>(),
            "AABA"
        );
        assert_eq!(d.song_bars(), 4);
        assert_eq!(d.song_steps(), 64);
        assert_eq!(pattern_letter(7), 'H');
        assert_eq!(pattern_letter(8), 'A');
        assert_eq!(PlayMode::Song.toggled(), PlayMode::Pattern);
        assert_eq!(PlayMode::Song.to_string(), "Song");
    }

    #[test]
    fn bank_pattern_helpers() {
        let mut b = Bank::default();
        b.copy_pattern(0, 3);
        assert_eq!(b.patterns[3], Pattern::demo());
        b.set_length(3, PatternLength::Bar4);
        assert_eq!(b.pattern(3).len(), 64);
        b.copy_pattern(3, 3);
        assert_eq!(b.pattern(3).len(), 64);
        b.clear_pattern(3);
        assert!(b.pattern(3).is_empty());
        assert_eq!(b.pattern(3).len(), 64);
        assert_eq!(b.pattern(11).len(), 64, "indices wrap");
        b.current = 3;
        assert_eq!(b.current_pattern().len(), 64);
        b.current_pattern_mut().set_length(PatternLength::Bar2);
        assert_eq!(b.pattern(3).bars(), 2);
    }

    #[test]
    fn chain_helpers() {
        let mut b = Bank::default();
        b.truncate(0);
        assert!(b.chain_entries().is_empty());
        assert_eq!(b.song_bars(), 1, "an empty chain plays the current pattern");
        assert!(b.push(ChainEntry::new(1, 2)));
        assert!(b.push(ChainEntry::new(9, 0)));
        assert_eq!(
            b.chain_entries(),
            &[ChainEntry::new(1, 2), ChainEntry::new(7, 1)]
        );
        assert!(b.set_entry(4, ChainEntry::new(2, 3)));
        assert_eq!(b.chain_len, 5);
        assert_eq!(b.chain[2], ChainEntry::default());
        assert_eq!(b.chain[4], ChainEntry::new(2, 3));
        assert!(!b.set_entry(64, ChainEntry::default()));
        assert!(b.remove_entry(0));
        assert_eq!(b.chain_len, 4);
        assert_eq!(b.chain[0], ChainEntry::new(7, 1));
        assert_eq!(b.chain[3], ChainEntry::new(2, 3));
        assert!(!b.remove_entry(4));
        b.truncate(2);
        assert_eq!(b.chain_len, 2);
        b.set_length(7, PatternLength::Bar2);
        assert_eq!(b.song_bars(), 2 + 1);
        for _ in 0..70 {
            b.push(ChainEntry::default());
        }
        assert_eq!(b.chain_len as usize, MAX_CHAIN);
        assert!(!b.push(ChainEntry::default()));
    }

    #[test]
    fn bank_normalise_clamps_everything() {
        let mut b = Bank {
            current: 200,
            chain_len: 255,
            ..Bank::default()
        };
        b.chain[0] = ChainEntry {
            pattern: 42,
            repeats: 0,
        };
        b.patterns[2].length = 48;
        // Hand-written notes: out of range, unsorted, with a stale slot beyond the count.
        let t = &mut b.patterns[2].tracks[4];
        t.notes[0] = Note {
            start: 5,
            length: 0,
            pitch: 255,
            velocity: f32::NAN,
        };
        t.notes[1] = Note {
            start: 2,
            length: 99,
            pitch: 60,
            velocity: -1.0,
        };
        t.notes[2] = Note::new(9, 1, 9, 1.0);
        t.note_count = 2;
        b.patterns[2].lane_track = 4;
        b.patterns[2].drum_volume[0] = f32::INFINITY;
        b.normalise();
        assert_eq!(b.current, 7);
        assert_eq!(b.chain_len as usize, MAX_CHAIN);
        assert_eq!(b.chain[0], ChainEntry::new(7, 1));
        assert_eq!(b.patterns[2].length, 64);
        assert_eq!(
            b.patterns[2].notes(4),
            &[
                Note::new(2, 64, 60, MIN_VELOCITY),
                Note::new(5, 1, 127, STEP_VELOCITY)
            ]
        );
        assert_eq!(
            b.patterns[2].tracks[4].notes[2],
            Note::default(),
            "stale slot cleared"
        );
        assert_eq!(b.patterns[2].synth[5].note, Some(127));
        assert_eq!(b.patterns[2].synth[2].note, Some(60));
        assert_eq!(b.patterns[2].synth[9].note, None);
        assert_eq!(b.patterns[2].drum_volume[0], 0.0);
    }

    #[test]
    fn bank_round_trips_through_json() {
        let mut b = Bank::demo();
        b.mode = PlayMode::Song;
        b.current = 1;
        b.set_length(1, PatternLength::Bar4);
        b.patterns[1].set_synth_step(0, 63, Some(50), false);
        b.patterns[1]
            .add_note(0, Note::new(63, 4, 55, 0.6))
            .unwrap();
        b.patterns[1].add_note(6, Note::new(0, 4, 72, 0.6)).unwrap();
        let json = serde_json::to_string_pretty(&b).unwrap();
        let back: Bank = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
        assert_eq!(back.patterns[1].synth[63].note, Some(50));
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["mode"], "song");
        assert_eq!(value["current"], 1);
        assert_eq!(value["patterns"].as_array().unwrap().len(), PATTERN_COUNT);
        assert_eq!(value["chain"].as_array().unwrap().len(), 4);
        assert_eq!(value["chain"][2]["pattern"], 1);
        assert_eq!(
            value["patterns"][1]["drums"][0].as_array().unwrap().len(),
            64
        );
        assert!(value["patterns"][1].get("synth").is_none());
        assert!(value["patterns"][1].get("notes").is_none());
        let tracks = value["patterns"][1]["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0]["notes"].as_array().unwrap().len(), 9 + 2);
        assert_eq!(tracks[1]["track"], 6);
        assert!(value.get("chain_len").is_none());

        // Short or empty banks pad; too many patterns or entries fail.
        let small: Bank = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(small, Bank::empty());
        let v = serde_json::json!({ "patterns": [Pattern::demo()], "chain": [], "mode": "song" });
        let b: Bank = serde_json::from_value(v).unwrap();
        assert_eq!(b.patterns[0], Pattern::demo());
        assert!(b.patterns[1].is_empty());
        assert_eq!(b.chain_len, 0);
        assert_eq!(b.mode, PlayMode::Song);
        let nine: Vec<Pattern> = vec![Pattern::default(); 9];
        assert!(serde_json::from_value::<Bank>(serde_json::json!({ "patterns": nine })).is_err());
        let chain: Vec<ChainEntry> = vec![ChainEntry::default(); 65];
        assert!(serde_json::from_value::<Bank>(serde_json::json!({ "chain": chain })).is_err());
    }
}
