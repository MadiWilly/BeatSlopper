//! Patterns (M3/M5): five drum rows plus one synth lane, 16/32/64 steps long, eight of them
//! in a [`Bank`] together with the song chain. Plain data shared by the engine (scheduler),
//! the UI (editing) and the project file (serde).
//!
//! # Layout
//!
//! - The bar is always [`BAR_STEPS`] (= [`STEPS`]) sixteenth notes. A pattern's
//!   [`length`](Pattern::length) is 16, 32 or 64 steps ([`PatternLength`]), i.e. 1, 2 or 4
//!   bars. Step storage is fixed at [`MAX_STEPS`] per row so the audio thread never
//!   allocates; steps beyond `length` are kept (so shortening and re-lengthening a pattern
//!   loses nothing) but not played.
//! - A [`Bank`] holds [`PATTERN_COUNT`] patterns (A..H), the song chain (up to [`MAX_CHAIN`]
//!   [`ChainEntry`]s, each a pattern index plus a repeat count), the [`PlayMode`] and the
//!   `current` (selected) pattern.
//!
//! # JSON
//!
//! Rows serialise as plain JSON arrays of step objects. A row is written with
//! [`Pattern::used_steps`] entries: its `length`, or more when steps beyond the length hold
//! data, so the file stays compact *and* lossless. On load a row of 16, 32 or 64 entries is
//! accepted as is (shorter rows are padded with empty steps); a row longer than
//! [`MAX_STEPS`] is an error. A version-2 pattern (no `length` field, 16 entries) loads with
//! `length = 16`; when `length` is absent the shortest length covering the longest row is
//! used. The chain is written as an array of `chain_len` entries (`chain_len` itself is
//! implied by the array), `mode` as `"pattern"` / `"song"`.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::drums::DrumRow;

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

/// One pattern: five drum rows plus the synth lane, 16/32/64 steps long.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    /// Length in steps: 16, 32 or 64 (see [`PatternLength`]). Steps at and beyond this
    /// index are kept but not played. Always valid when set through
    /// [`set_length`](Self::set_length) / [`Bank::normalise`]; [`len`](Self::len) is the
    /// defensive accessor.
    pub length: u8,
    /// Drum rows in [`DrumRow::ALL`] order, [`MAX_STEPS`] steps each.
    pub drums: [[DrumStep; MAX_STEPS]; DrumRow::COUNT],
    /// The bass/melody lane, [`MAX_STEPS`] steps.
    pub synth: [SynthStep; MAX_STEPS],
    /// Per-row drum volume in dB ([`DRUM_VOLUME_RANGE`]).
    pub drum_volume: [f32; DrumRow::COUNT],
    /// Per-row drum tune in semitones ([`DRUM_TUNE_RANGE`]).
    pub drum_tune: [f32; DrumRow::COUNT],
}

impl Default for Pattern {
    /// An empty one-bar pattern.
    fn default() -> Self {
        Pattern {
            length: PatternLength::Bar1.steps(),
            drums: [[DrumStep::default(); MAX_STEPS]; DrumRow::COUNT],
            synth: [SynthStep::default(); MAX_STEPS],
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
            p.synth[i].note = n;
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
        for step in p.synth.iter_mut().take(STEPS) {
            step.note = step.note.map(|n| (n + 5).min(127));
        }
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
        let last_synth = self.synth.iter().rposition(|s| *s != SynthStep::default());
        let used = last_drum
            .into_iter()
            .chain(last_synth)
            .map(|i| i + 1)
            .max()
            .unwrap_or(0);
        if used > self.len() {
            usize::from(PatternLength::covering(used).steps())
        } else {
            self.len()
        }
    }

    /// Clamps the length to a valid value, notes to `0..=127` and volume/tune to their
    /// ranges (non-finite values become 0).
    pub fn normalise(&mut self) {
        self.set_length_steps(self.length);
        for step in &mut self.synth {
            step.note = step.note.map(|n| n.min(127));
        }
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

    /// Clears the synth lane, hidden steps included.
    pub fn clear_synth(&mut self) {
        self.synth = [SynthStep::default(); MAX_STEPS];
    }

    /// Clears every step of every row and the synth lane (length, volume and tune are kept).
    pub fn clear_all(&mut self) {
        for row in DrumRow::ALL {
            self.clear_row(row);
        }
        self.clear_synth();
    }

    /// `true` when no step (within the length or beyond it) is active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drums.iter().flatten().all(|s| !s.on) && self.synth.iter().all(|s| s.note.is_none())
    }

    /// Sets one drum step (step index wraps into `0..length`).
    pub fn set_drum_step(&mut self, row: DrumRow, step: u8, on: bool, accent: bool) {
        let i = usize::from(step) % self.len();
        self.drums[row.index()][i] = DrumStep { on, accent };
    }

    /// Sets one synth step (step index wraps into `0..length`).
    pub fn set_synth_step(&mut self, step: u8, note: Option<u8>, accent: bool) {
        let i = usize::from(step) % self.len();
        self.synth[i] = SynthStep {
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

// --- Pattern serde ------------------------------------------------------------------------

/// On-disk shape of a [`Pattern`]: rows are plain arrays of any length up to [`MAX_STEPS`].
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PatternRepr {
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<u8>,
    drums: Vec<Vec<DrumStep>>,
    synth: Vec<SynthStep>,
    drum_volume: [f32; DrumRow::COUNT],
    drum_tune: [f32; DrumRow::COUNT],
}

impl Default for PatternRepr {
    fn default() -> Self {
        Self {
            length: None,
            drums: vec![Vec::new(); DrumRow::COUNT],
            synth: Vec::new(),
            drum_volume: [0.0; DrumRow::COUNT],
            drum_tune: [0.0; DrumRow::COUNT],
        }
    }
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
            synth: self.synth[..n].to_vec(),
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
        fill_row("synth lane", &mut p.synth, &repr.synth)?;
        longest = longest.max(repr.synth.len());
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
        p.set_synth_step(3, Some(200), false);
        assert_eq!(p.synth[3].note, Some(127));
        p.set_drum_volume(DrumRow::Snare, 99.0);
        assert_eq!(p.drum_volume[DrumRow::Snare.index()], 6.0);
        p.set_drum_tune(DrumRow::Snare, f32::NAN);
        assert_eq!(p.drum_tune[DrumRow::Snare.index()], 0.0);
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
        // A 16-step pattern writes 16 entries per row.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["length"], 16);
        assert_eq!(value["drums"][0].as_array().unwrap().len(), 16);
        assert_eq!(value["synth"].as_array().unwrap().len(), 16);
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
        assert_eq!(p.drum_volume[1], -3.0);

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
        p.synth[40].note = Some(1);
        assert_eq!(p.used_steps(), 64);
        p.synth[40].note = None;
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
        b.patterns[2].synth[0].note = Some(255);
        b.patterns[2].drum_volume[0] = f32::INFINITY;
        b.normalise();
        assert_eq!(b.current, 7);
        assert_eq!(b.chain_len as usize, MAX_CHAIN);
        assert_eq!(b.chain[0], ChainEntry::new(7, 1));
        assert_eq!(b.patterns[2].length, 64);
        assert_eq!(b.patterns[2].synth[0].note, Some(127));
        assert_eq!(b.patterns[2].drum_volume[0], 0.0);
    }

    #[test]
    fn bank_round_trips_through_json() {
        let mut b = Bank::demo();
        b.mode = PlayMode::Song;
        b.current = 1;
        b.set_length(1, PatternLength::Bar4);
        b.patterns[1].synth[63].note = Some(50);
        let json = serde_json::to_string_pretty(&b).unwrap();
        let back: Bank = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["mode"], "song");
        assert_eq!(value["current"], 1);
        assert_eq!(value["patterns"].as_array().unwrap().len(), PATTERN_COUNT);
        assert_eq!(value["chain"].as_array().unwrap().len(), 4);
        assert_eq!(value["chain"][2]["pattern"], 1);
        assert_eq!(value["patterns"][1]["synth"].as_array().unwrap().len(), 64);
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
