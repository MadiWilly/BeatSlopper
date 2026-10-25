//! The piano roll (M7): time on X in steps, pitch on Y, notes as rounded accent bars.
//!
//! Layout: a [`KEY_COLUMN_WIDTH`] key column on the left (one key per row, see [`key_rect`]:
//! white keys as lit surface tiles labelled "D3" etc., black keys as shorter `key_black`
//! stubs; a key in `model.held` paints accent and its grid row gets a faint accent wash),
//! then `steps` columns of `step_width` by `pitches` rows of `row_height`.
//! The bottom row is `low_pitch`; pitch rises upwards. With `model.black_keys` off the
//! black-key rows are left out altogether: every row is a white key, an octave is seven rows
//! and notes on black keys are off-screen until it is turned back on ([`PianoRollModel`]
//! maps rows to pitches either way, see [`PianoRollModel::pitch_at_row`]).
//! The whole widget is one inset-m surface; black-key rows get a faint shadow-dark tint, beat
//! lines fall every [`BEAT`] steps (1 pt) and bar lines every [`BAR`] steps (2 pt), the
//! playhead is a 2 pt accent line at a fractional step. Accented notes (velocity >=
//! [`ACCENT_THRESHOLD`]) carry a [`ACCENT_DOT`] (6 pt) on-accent dot near their left end on
//! top of the `accent_hover` fill, like the step cell's accent marker, so the state does not
//! rest on the 1.2:1 accent / accent-hover difference alone. While the roll has keyboard focus
//! a 1.5 pt accent ring (the shared [`crate::shadow::paint_focus_rect`]) is drawn 1 pt outside
//! the whole roll.
//!
//! As-built fill (not in `design/WINNER.md`): while idle, the grid cell under the pointer is
//! highlighted with a half-strength `shadow_highlight` wash so the draw target reads before
//! the press. It is a hover-only cue and conveys no state. A note removed with the right
//! button "pops": its outline grows and fades over [`POP_SECONDS`].
//!
//! Interaction is one `Sense::click_and_drag` response with a small gesture state machine kept
//! in egui's temporary memory under `response.id` (see [`piano_roll`] for the rules). The
//! widget never edits the notes itself: it reports the edits in [`PianoRollOutput`] and the
//! host applies them. It does update `model.low_pitch` (wheel) and the selection
//! (`model.selection` / `model.selected`, clicks and the marquee) in place, because those
//! are view state.
//!
//! Selection: `model.selection` holds every selected note index and `model.selected` the
//! primary one (the last clicked, always a member). A plain click selects one note,
//! Ctrl-click toggles a note in and out, Ctrl-drag on empty space draws a marquee that
//! selects the notes it touches, Ctrl+A selects them all. Dragging a selected note moves (or,
//! from its right edge, resizes) the whole selection together; the arrow keys nudge the
//! selection by a row or a step, Ctrl+Up / Ctrl+Down by an octave; Delete removes it.
//!
//! All geometry is public and unit-tested: [`grid_rect`], [`cell_at`], [`note_rect`],
//! [`row_rect`], [`key_rect`], [`is_black_key`].

use egui::{
    Align2, Color32, CursorIcon, Id, Key, Modifiers, Pos2, Rect, Response, Role, Sense, Stroke, Ui,
    Vec2, WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::piano_keys::note_name;
use crate::shadow::{Depth, paint_focus_rect, paint_soft_rect, paint_soft_rect_with_fill};
use crate::text::{self, TextKind};

/// Width of the key column on the left of the grid.
pub const KEY_COLUMN_WIDTH: f32 = 48.0;
/// Corner radius of a note bar.
pub const NOTE_RADIUS: f32 = 6.0;
/// Vertical inset of a note bar inside its row.
pub const NOTE_INSET_Y: f32 = 1.0;
/// Width, from a note's right edge inwards, of the zone where a press starts a resize.
pub const RESIZE_HANDLE: f32 = 6.0;
/// Distance of the painted 1 pt handle line from the note's right edge.
pub const HANDLE_INSET: f32 = 4.0;
/// Steps per beat (beat lines, 1 pt).
pub const BEAT: usize = 4;
/// Steps per bar (bar lines, 2 pt).
pub const BAR: usize = 16;
/// Width of the playhead line.
pub const PLAYHEAD_WIDTH: f32 = 2.0;
/// Velocity of a freshly drawn note, and the "unaccented" half of the shift-click toggle.
pub const DEFAULT_VELOCITY: f32 = 0.8;
/// Velocity of an accented note (the other half of the shift-click toggle).
pub const ACCENT_VELOCITY: f32 = 1.0;
/// Notes at or above this velocity paint in `accent_hover` (the grid lane's accent rule).
pub const ACCENT_THRESHOLD: f32 = 0.95;
/// Diameter of the on-accent dot on an accented note (the step cell's accent marker).
pub const ACCENT_DOT: f32 = 6.0;
/// Distance from a note's left edge to the centre of its accent dot.
pub const ACCENT_DOT_INSET: f32 = 8.0;
/// Corner radius of the roll's inset background.
pub const ROLL_RADIUS: f32 = 10.0;
/// How far the surface is lerped towards shadow-dark on black-key rows (6 %).
pub const BLACK_ROW_TINT: f32 = 0.06;
/// Size of the "C3" key tile in the key column (the M7 look; kept for layouts that still
/// measure it).
pub const KEY_TILE: Vec2 = vec2(40.0, 20.0);
/// Horizontal inset of a key tile inside the key column.
pub const KEY_INSET_X: f32 = 3.0;
/// Vertical inset of a key tile inside its row.
pub const KEY_INSET_Y: f32 = 0.5;
/// Width of a black key as a fraction of a white key's width.
pub const BLACK_KEY_FRACTION: f32 = 0.6;
/// How far a white key is lerped from the surface towards shadow-highlight (it reads as lit).
pub const WHITE_KEY_LIFT: f32 = 0.5;
/// Distance from a white key's right edge to the end of its note name.
pub const KEY_LABEL_INSET: f32 = 4.0;
/// Opacity of the accent wash across the grid row of a held key.
pub const HELD_ROW_WASH: f32 = 0.12;
/// Opacity of the accent wash inside the selection marquee.
pub const MARQUEE_WASH: f32 = 0.15;
/// How long the pop of a right-click-removed note lasts.
pub const POP_SECONDS: f64 = 0.18;
/// How much a popping note's outline grows (as a fraction of its size) by the end.
pub const POP_GROWTH: f32 = 0.6;
/// Number of white keys in MIDI 0..=127 (C-1 .. G9).
pub const WHITE_KEYS: usize = 75;
/// Rows in an octave with the black keys shown / hidden.
pub const OCTAVE_ROWS: usize = 12;
pub const OCTAVE_WHITE_ROWS: usize = 7;

/// Number of white keys strictly below each semitone of an octave.
const WHITE_BELOW: [usize; 12] = [0, 1, 1, 2, 2, 3, 4, 4, 5, 5, 6, 6];
/// Semitone of each white key of an octave.
const WHITE_SEMITONES: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];

/// One note on the roll, in grid units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RollNote {
    /// First step (0-based).
    pub start: u8,
    /// Length in steps (at least 1).
    pub length: u8,
    /// MIDI pitch 0..=127.
    pub pitch: u8,
    /// 0..=1; `>= 0.95` paints as accented.
    pub velocity: f32,
    /// Painted with the on-glow treatment (the host's own selection flag; the widget also
    /// honours `PianoRollModel::selection`).
    pub selected: bool,
}

/// View state of the roll: grid extent, scroll position, cell size, playhead, selection,
/// held keys and whether the black-key rows are shown.
#[derive(Debug, Clone, PartialEq)]
pub struct PianoRollModel {
    /// Number of step columns (16 / 32 / 64).
    pub steps: usize,
    /// MIDI pitch of the bottom row (a white key when `black_keys` is off, see
    /// [`Self::normalise`]).
    pub low_pitch: u8,
    /// Number of visible rows.
    pub pitches: usize,
    /// Column width in points.
    pub step_width: f32,
    /// Row height in points.
    pub row_height: f32,
    /// Playhead position in (fractional) steps; `None` hides it.
    pub playhead_step: Option<f32>,
    /// The primary selected note (the last one clicked), if any; always in `selection`.
    pub selected: Option<usize>,
    /// Every selected note, in selection order (no duplicates).
    pub selection: Vec<usize>,
    /// Pitches currently sounding (the host's held keys): their key in the key column paints
    /// accent and their grid row gets a faint accent wash. View state only, never an edit.
    pub held: [bool; 128],
    /// Whether the black-key rows are shown (`true`) or left out so every row is a white key.
    pub black_keys: bool,
}

impl Default for PianoRollModel {
    /// 16 steps, three octaves (36 rows) from C2 (36), 24 x 14 cells, no playhead, nothing
    /// selected, no held keys, black keys shown.
    fn default() -> Self {
        Self {
            steps: 16,
            low_pitch: 36,
            pitches: 36,
            step_width: 24.0,
            row_height: 14.0,
            playhead_step: None,
            selected: None,
            selection: Vec::new(),
            held: [false; 128],
            black_keys: true,
        }
    }
}

impl PianoRollModel {
    /// Size of the widget: the key column plus `steps` x `step_width` by `pitches` x
    /// `row_height`.
    pub fn size(&self) -> Vec2 {
        vec2(
            KEY_COLUMN_WIDTH + self.steps as f32 * self.step_width,
            self.pitches as f32 * self.row_height,
        )
    }

    /// Index of the highest row-able pitch: 127 with the black keys, the last white key
    /// (G9, index 74) without.
    pub fn max_index(&self) -> usize {
        if self.black_keys { 127 } else { WHITE_KEYS - 1 }
    }

    /// Position of `pitch` in the row scale: the pitch itself with the black keys shown, its
    /// white-key index without (a black key shares the index of the white key below it).
    pub fn scale_index(&self, pitch: u8) -> usize {
        if self.black_keys {
            usize::from(pitch)
        } else {
            white_index(pitch)
        }
    }

    /// The pitch at `index` of the row scale (clamped to [`Self::max_index`]).
    pub fn pitch_of_index(&self, index: usize) -> u8 {
        let index = index.min(self.max_index());
        if self.black_keys {
            index as u8
        } else {
            white_pitch(index)
        }
    }

    /// Rows between the bottom row and `pitch`'s row (negative below the grid, `>= pitches`
    /// above it). A hidden black key reports the row of the white key below it.
    pub fn row_of(&self, pitch: u8) -> i64 {
        self.scale_index(pitch) as i64 - self.scale_index(self.low_pitch) as i64
    }

    /// The pitch of visible row `row` (0 is the bottom row), or `None` past the top of MIDI.
    pub fn pitch_at_row(&self, row: usize) -> Option<u8> {
        let index = self.scale_index(self.low_pitch) + row;
        (index <= self.max_index()).then(|| self.pitch_of_index(index))
    }

    /// `pitch` moved by `rows` rows of this scale (a semitone, or a white key when the black
    /// keys are hidden), clamped to MIDI.
    pub fn shift_pitch(&self, pitch: u8, rows: i64) -> u8 {
        let index = (self.scale_index(pitch) as i64 + rows).clamp(0, self.max_index() as i64);
        self.pitch_of_index(index as usize)
    }

    /// Rows in an octave: 12, or 7 without the black keys.
    pub fn octave_rows(&self) -> i64 {
        if self.black_keys {
            OCTAVE_ROWS as i64
        } else {
            OCTAVE_WHITE_ROWS as i64
        }
    }

    /// The highest `low_pitch` the wheel scrolls to: `127 - pitches` (0 when more than the
    /// whole MIDI range is visible); the white key `pitches` rows under G9 without the black
    /// keys.
    pub fn max_low_pitch(&self) -> u8 {
        self.pitch_of_index(self.max_index().saturating_sub(self.pitches))
    }

    /// Whether `pitch` has a row of its own on screen (a hidden black key never has).
    pub fn is_visible(&self, pitch: u8) -> bool {
        if !self.black_keys && is_black_key(pitch) {
            return false;
        }
        let row = self.row_of(pitch);
        row >= 0 && (row as usize) < self.pitches
    }

    /// Puts the view state right: a hidden black key as the bottom row moves up to the next
    /// white key, `low_pitch` is clamped to [`Self::max_low_pitch`], the selection loses
    /// duplicates and indices past `notes` and `selected` is one of its members (the last).
    pub fn normalise(&mut self, notes: usize) {
        if !self.black_keys && is_black_key(self.low_pitch) {
            self.low_pitch += 1;
        }
        self.low_pitch = self.low_pitch.min(self.max_low_pitch());
        let mut seen = Vec::with_capacity(self.selection.len());
        self.selection.retain(|&i| {
            let keep = i < notes && !seen.contains(&i);
            if keep {
                seen.push(i);
            }
            keep
        });
        // A primary set on its own (hosts that only know `selected`) joins the selection; one
        // past the notes goes.
        match self.selected {
            Some(s) if s >= notes => self.selected = None,
            Some(s) if !self.selection.contains(&s) => self.selection.push(s),
            _ => {}
        }
        if self.selected.is_none() {
            self.selected = self.selection.last().copied();
        }
    }

    /// Replaces the selection; the last entry becomes the primary note.
    pub fn set_selection(&mut self, selection: Vec<usize>) {
        self.selection = Vec::with_capacity(selection.len());
        for i in selection {
            if !self.selection.contains(&i) {
                self.selection.push(i);
            }
        }
        self.selected = self.selection.last().copied();
    }

    /// Adds `index` to the selection (or just makes it primary when it is there already).
    pub fn select_also(&mut self, index: usize) {
        if !self.selection.contains(&index) {
            self.selection.push(index);
        }
        self.selected = Some(index);
    }

    /// Removes `index` from the selection; the primary falls back to the last member.
    pub fn deselect(&mut self, index: usize) {
        self.selection.retain(|&i| i != index);
        if self.selected == Some(index) {
            self.selected = self.selection.last().copied();
        }
    }

    /// Whether `index` is selected (primary or not).
    pub fn is_selected(&self, index: usize) -> bool {
        self.selection.contains(&index)
    }
}

/// What the roll reported this frame. At most one kind of edit (`add`, `moved`,
/// `moved_many`, `removed`, `removed_many`) is set; the indices refer to the `notes` the
/// widget was given.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PianoRollOutput {
    /// A note was drawn (released a draw gesture or clicked an empty cell).
    pub add: Option<RollNote>,
    /// Note `index` should become this note (moved, resized, nudged or velocity toggled).
    pub moved: Option<(usize, RollNote)>,
    /// Several notes (a selection) should become these notes, all at once.
    pub moved_many: Vec<(usize, RollNote)>,
    /// Note `index` should be removed (double-click, right-click, Delete / Backspace).
    pub removed: Option<usize>,
    /// Several notes (a selection) should be removed, all at once.
    pub removed_many: Vec<usize>,
    /// Note `index` became the primary selection by a click this frame.
    pub selected: Option<usize>,
    /// `model.low_pitch` changed from the wheel this frame.
    pub scrolled: bool,
}

impl PianoRollOutput {
    /// `true` when an edit of any kind was reported.
    pub fn has_edit(&self) -> bool {
        self.add.is_some()
            || self.moved.is_some()
            || !self.moved_many.is_empty()
            || self.removed.is_some()
            || !self.removed_many.is_empty()
    }
}

/// Index of `pitch` among the white keys (0 = C-1, 74 = G9); a black key gets the index of
/// the white key below it.
pub fn white_index(pitch: u8) -> usize {
    let octave = usize::from(pitch / 12);
    let semi = usize::from(pitch % 12);
    let white = usize::from(!is_black_key(pitch));
    octave * OCTAVE_WHITE_ROWS + WHITE_BELOW[semi] + white - 1
}

/// The white key with [`white_index`] `index` (clamped to the last, G9 = 127).
pub fn white_pitch(index: usize) -> u8 {
    let index = index.min(WHITE_KEYS - 1);
    let octave = (index / OCTAVE_WHITE_ROWS) as u8;
    (octave * 12 + WHITE_SEMITONES[index % OCTAVE_WHITE_ROWS]).min(127)
}

/// The grid part of a roll allocated in `outer`: everything right of the key column.
pub fn grid_rect(outer: Rect) -> Rect {
    Rect::from_min_max(pos2(outer.min.x + KEY_COLUMN_WIDTH, outer.min.y), outer.max)
}

/// The `(step, pitch)` cell under `pos`, or `None` outside the grid (or above the top of
/// MIDI).
pub fn cell_at(model: &PianoRollModel, grid: Rect, pos: Pos2) -> Option<(usize, u8)> {
    if model.steps == 0 || model.pitches == 0 || !grid.contains(pos) {
        return None;
    }
    let (step, pitch) = clamped_cell_at(model, grid, pos);
    // Rows above the top of MIDI are empty space, not a cell.
    let row = ((grid.max.y - pos.y) / model.row_height).floor().max(0.0) as usize;
    model.pitch_at_row(row).is_some().then_some((step, pitch))
}

/// The cell nearest `pos`, clamping positions outside the grid onto its edge rows / columns
/// and the pitch to MIDI. `model.steps` and `model.pitches` must be non-zero.
fn clamped_cell_at(model: &PianoRollModel, grid: Rect, pos: Pos2) -> (usize, u8) {
    let col = ((pos.x - grid.min.x) / model.step_width).floor();
    let step = (col.max(0.0) as usize).min(model.steps - 1);
    let row_f = ((grid.max.y - pos.y) / model.row_height).floor();
    let row = (row_f.max(0.0) as usize).min(model.pitches - 1);
    let pitch = model
        .pitch_at_row(row)
        .unwrap_or_else(|| model.pitch_of_index(model.max_index()));
    (step, pitch)
}

/// The full-height rect of the row for `pitch` (may lie outside `grid` for pitches
/// off-screen). A hidden black key gets a zero-height rect on the line above the white key
/// below it.
pub fn row_rect(model: &PianoRollModel, grid: Rect, pitch: u8) -> Rect {
    let row = model.row_of(pitch);
    if !model.black_keys && is_black_key(pitch) {
        let y = grid.max.y - (row + 1) as f32 * model.row_height;
        return Rect::from_min_max(pos2(grid.min.x, y), pos2(grid.max.x, y));
    }
    let bottom = grid.max.y - row as f32 * model.row_height;
    Rect::from_min_max(
        pos2(grid.min.x, bottom - model.row_height),
        pos2(grid.max.x, bottom),
    )
}

/// The key tile painted for `pitch` in the key column of a roll allocated in `outer`: its row
/// inset [`KEY_INSET_Y`] vertically and [`KEY_INSET_X`] horizontally; a black key is only
/// [`BLACK_KEY_FRACTION`] of that width, against the left edge. Not clipped to `outer`.
pub fn key_rect(model: &PianoRollModel, outer: Rect, pitch: u8) -> Rect {
    let row = row_rect(model, grid_rect(outer), pitch);
    let full = KEY_COLUMN_WIDTH - 2.0 * KEY_INSET_X;
    let width = if is_black_key(pitch) {
        full * BLACK_KEY_FRACTION
    } else {
        full
    };
    Rect::from_min_max(
        pos2(outer.min.x + KEY_INSET_X, row.min.y + KEY_INSET_Y),
        pos2(outer.min.x + KEY_INSET_X + width, row.max.y - KEY_INSET_Y),
    )
}

/// The bar painted for `note`: `start * step_width` wide `length` steps, inset
/// [`NOTE_INSET_Y`] vertically in its row. Not clipped to the grid.
pub fn note_rect(model: &PianoRollModel, grid: Rect, note: &RollNote) -> Rect {
    let row = row_rect(model, grid, note.pitch);
    let x0 = grid.min.x + f32::from(note.start) * model.step_width;
    let x1 = x0 + f32::from(note.length.max(1)) * model.step_width;
    Rect::from_min_max(
        pos2(x0, row.min.y + NOTE_INSET_Y),
        pos2(x1, row.max.y - NOTE_INSET_Y),
    )
}

/// `true` for the sharps / flats (C# D# F# G# A#).
pub fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// The surface tinted [`BLACK_ROW_TINT`] towards shadow-dark: the black-key row fill.
pub fn black_row_tint(theme: &Theme) -> Color32 {
    theme
        .surface
        .lerp_to_gamma(theme.shadow_dark, BLACK_ROW_TINT)
}

/// Which gesture a press started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gesture {
    /// Press on an empty cell: the note grows from the press cell to the pointer.
    Draw,
    /// Press on a note body: the note (or the whole selection) follows the pointer by cells.
    Move,
    /// Press within [`RESIZE_HANDLE`] of a note's right edge: the length(s) follow the pointer.
    Resize,
    /// Ctrl-press on an empty cell: a marquee selects the notes it touches on release.
    Select,
}

/// The in-progress gesture, kept in temp memory between the press and the release.
#[derive(Debug, Clone)]
struct Drag {
    gesture: Gesture,
    /// The note under the press (`Move` / `Resize`).
    note: Option<usize>,
    /// That note as it was at the press (a default note for `Draw`).
    original: RollNote,
    /// `Move` / `Resize` of a selection of two or more: every member as it was at the press
    /// (the pressed note included). Empty for a single note.
    group: Vec<(usize, RollNote)>,
    /// Ctrl was held at the press (a click then toggles the note in the selection).
    ctrl: bool,
    /// Cell of the press.
    press: (usize, u8),
    /// Cell under the pointer now (clamped to the grid).
    current: (usize, u8),
    /// Pointer position at the press and now (the marquee corners).
    press_pos: Pos2,
    current_pos: Pos2,
}

impl Drag {
    /// Starts the gesture for a press at `cell` / `pos` given the notes under the pointer and
    /// the current selection (which it may replace, see [`piano_roll`]).
    fn begin(
        model: &mut PianoRollModel,
        grid: Rect,
        notes: &[RollNote],
        cell: (usize, u8),
        pos: Pos2,
        ctrl: bool,
    ) -> Self {
        let hit = hit_note(model, grid, notes, pos);
        let (gesture, note, original, group) = match hit {
            Some((i, on_edge)) => {
                let gesture = if on_edge {
                    Gesture::Resize
                } else {
                    Gesture::Move
                };
                let group = if model.is_selected(i) && model.selection.len() >= 2 {
                    model
                        .selection
                        .iter()
                        .filter_map(|&j| notes.get(j).map(|n| (j, *n)))
                        .collect()
                } else {
                    if !ctrl {
                        model.set_selection(vec![i]);
                    }
                    Vec::new()
                };
                (gesture, Some(i), notes[i], group)
            }
            None => {
                let gesture = if ctrl {
                    Gesture::Select
                } else {
                    model.set_selection(Vec::new());
                    Gesture::Draw
                };
                (
                    gesture,
                    None,
                    RollNote {
                        start: cell.0 as u8,
                        length: 1,
                        pitch: cell.1,
                        velocity: DEFAULT_VELOCITY,
                        selected: false,
                    },
                    Vec::new(),
                )
            }
        };
        Self {
            gesture,
            note,
            original,
            group,
            ctrl,
            press: cell,
            current: cell,
            press_pos: pos,
            current_pos: pos,
        }
    }

    /// The marquee rect of a `Select` gesture.
    fn marquee(&self) -> Rect {
        Rect::from_two_pos(self.press_pos, self.current_pos)
    }

    /// Steps and rows the pointer moved since the press.
    fn delta(&self, model: &PianoRollModel) -> (i64, i64) {
        let steps = self.current.0 as i64 - self.press.0 as i64;
        let rows =
            model.scale_index(self.current.1) as i64 - model.scale_index(self.press.1) as i64;
        (steps, rows)
    }

    /// The note this gesture would commit if released now (single-note gestures).
    fn preview(&self, model: &PianoRollModel) -> RollNote {
        let steps = model.steps.max(1) as i64;
        let (ds, drows) = self.delta(model);
        let cs = self.current.0 as i64;
        let ps = self.press.0 as i64;
        let mut n = self.original;
        match self.gesture {
            Gesture::Draw | Gesture::Select => {
                let start = ps.clamp(0, steps - 1);
                n.start = start as u8;
                n.length = (cs - start + 1).clamp(1, steps - start) as u8;
            }
            Gesture::Move => {
                let len = i64::from(n.length.max(1)).min(steps);
                n.start = (i64::from(n.start) + ds).clamp(0, steps - len) as u8;
                n.pitch = model.shift_pitch(n.pitch, drows);
            }
            Gesture::Resize => {
                let start = i64::from(n.start).min(steps - 1);
                n.length = (cs - start + 1).clamp(1, steps - start) as u8;
            }
        }
        n
    }

    /// The notes a group gesture would commit if released now: the same shift (clamped so
    /// the whole group stays on the grid) or the same length change for every member.
    fn preview_group(&self, model: &PianoRollModel) -> Vec<(usize, RollNote)> {
        let (ds, drows) = self.delta(model);
        match self.gesture {
            Gesture::Move => shift_notes(model, &self.group, ds, drows),
            Gesture::Resize => {
                let end =
                    i64::from(self.original.start) + i64::from(self.original.length.max(1)) - 1;
                let dl = self.current.0 as i64 - end;
                resize_notes(model, &self.group, dl)
            }
            Gesture::Draw | Gesture::Select => Vec::new(),
        }
    }
}

/// How far `notes` may move together and stay on the grid: `(min_steps, max_steps,
/// min_rows, max_rows)`, each range containing 0.
fn shift_bounds(model: &PianoRollModel, notes: &[(usize, RollNote)]) -> (i64, i64, i64, i64) {
    let grid_steps = model.steps.max(1) as i64;
    let max_index = model.max_index() as i64;
    let min_start = notes
        .iter()
        .map(|(_, n)| i64::from(n.start))
        .min()
        .unwrap_or(0);
    let max_end = notes
        .iter()
        .map(|(_, n)| i64::from(n.start) + i64::from(n.length.max(1)))
        .max()
        .unwrap_or(0);
    let min_index = notes
        .iter()
        .map(|(_, n)| model.scale_index(n.pitch) as i64)
        .min()
        .unwrap_or(0);
    let max_index_in = notes
        .iter()
        .map(|(_, n)| model.scale_index(n.pitch) as i64)
        .max()
        .unwrap_or(0);
    (
        (-min_start).min(0),
        (grid_steps - max_end).max(0),
        (-min_index).min(0),
        (max_index - max_index_in).max(0),
    )
}

/// Whether `notes` can move by exactly `steps` columns and `rows` rows and stay on the grid.
fn shift_fits(model: &PianoRollModel, notes: &[(usize, RollNote)], steps: i64, rows: i64) -> bool {
    let (min_s, max_s, min_r, max_r) = shift_bounds(model, notes);
    (min_s..=max_s).contains(&steps) && (min_r..=max_r).contains(&rows)
}

/// `notes` moved by `steps` columns and `rows` rows together, the shift clamped so every
/// note stays on the grid (the group keeps its shape).
fn shift_notes(
    model: &PianoRollModel,
    notes: &[(usize, RollNote)],
    steps: i64,
    rows: i64,
) -> Vec<(usize, RollNote)> {
    if notes.is_empty() {
        return Vec::new();
    }
    let grid_steps = model.steps.max(1) as i64;
    let (min_s, max_s, min_r, max_r) = shift_bounds(model, notes);
    let ds = steps.clamp(min_s, max_s);
    let drows = rows.clamp(min_r, max_r);
    notes
        .iter()
        .map(|&(i, mut n)| {
            n.start = (i64::from(n.start) + ds).clamp(0, grid_steps - 1) as u8;
            n.pitch = model.shift_pitch(n.pitch, drows);
            (i, n)
        })
        .collect()
}

/// `notes` each lengthened by `delta` steps (at least 1, at most to the grid end).
fn resize_notes(
    model: &PianoRollModel,
    notes: &[(usize, RollNote)],
    delta: i64,
) -> Vec<(usize, RollNote)> {
    let grid_steps = model.steps.max(1) as i64;
    notes
        .iter()
        .map(|&(i, mut n)| {
            let start = i64::from(n.start).min(grid_steps - 1);
            n.length = (i64::from(n.length.max(1)) + delta).clamp(1, grid_steps - start) as u8;
            (i, n)
        })
        .collect()
}

/// The topmost (last) note under `pos` and whether the press is in its resize zone.
fn hit_note(
    model: &PianoRollModel,
    grid: Rect,
    notes: &[RollNote],
    pos: Pos2,
) -> Option<(usize, bool)> {
    notes
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, n)| model.is_visible(n.pitch))
        .find(|(_, n)| note_rect(model, grid, n).contains(pos))
        .map(|(i, n)| {
            let r = note_rect(model, grid, n);
            let on_edge = r.width() > RESIZE_HANDLE && pos.x >= r.max.x - RESIZE_HANDLE;
            (i, on_edge)
        })
}

/// A note popping out after a right-click removal.
#[derive(Debug, Clone, Copy)]
struct Pop {
    rect: Rect,
    start: f64,
}

/// Draws the piano roll for `notes` and reports this frame's interaction.
///
/// Size: [`PianoRollModel::size`]. Painting: inset-m background, black-key rows tinted
/// ([`black_row_tint`]), faint row separators, beat lines (shadow-dark 1 pt every [`BEAT`]
/// steps) and bar lines (2 pt every [`BAR`]), the key column (one key per row), notes as
/// radius-6 bars (accent, `accent_hover` plus a 6 pt on-accent dot near the left end at
/// velocity >= 0.95, on-glow when selected) with a 1 pt on-accent handle line near the right
/// edge, a hovered-cell highlight, the 2 pt accent playhead and, while the roll has keyboard
/// focus, a 1.5 pt accent ring 1 pt outside the whole roll.
///
/// Gestures (state in temp memory under `response.id`):
/// * press on an empty cell, drag right, release: `add` with `start` = press step, `length` =
///   cells dragged + 1 (min 1, clipped to the grid end), `pitch` = press row, velocity 0.8. A
///   plain click on an empty cell adds a length-1 note. Either clears the selection.
/// * press on a note body and drag: `moved` on release with the new start / pitch (clamped
///   to the grid); when the note is one of a selection of two or more, `moved_many` with
///   every member shifted alike (the shift clamped so the group stays on the grid). Without
///   movement it is a click: the note becomes the selection (`selected` is reported), with
///   Ctrl held it is toggled in / out of the selection, with shift held `moved` with the
///   velocity toggled between 1.0 and 0.8.
/// * press within 6 pt of a note's right edge and drag: `moved` with the new length (min 1,
///   max to the grid end); for a selection, `moved_many` with every member lengthened alike.
/// * Ctrl-press on an empty cell and drag: a marquee; on release the notes it touches are
///   the selection.
/// * double-click on a note: `removed`.
/// * right-click on a note, or the right button held while the pointer crosses a note:
///   `removed` (one per frame) with a pop.
/// * Delete / Backspace while the roll has keyboard focus (it takes focus on a press):
///   `removed` for a single selected note, `removed_many` for more.
/// * Ctrl+A: every note becomes the selection.
/// * arrow keys with a selection: `moved` / `moved_many` nudged one row up / down or one
///   step left / right; Ctrl+Up / Ctrl+Down: an octave.
/// * wheel over the roll: `model.low_pitch` moves one row per `row_height` points, clamped to
///   [`PianoRollModel::max_low_pitch`]; `scrolled` is set. Shift+wheel is reserved (no-op).
///
/// At most one kind of edit is reported per frame.
pub fn piano_roll(
    ui: &mut Ui,
    theme: &Theme,
    model: &mut PianoRollModel,
    notes: &[RollNote],
) -> PianoRollOutput {
    model.normalise(notes.len());
    let (outer, response) = ui.allocate_exact_size(model.size(), Sense::click_and_drag());
    let grid = grid_rect(outer);
    let mut out = PianoRollOutput::default();
    let usable = model.steps > 0 && model.pitches > 0;
    // The arrow keys nudge notes while the roll has focus; keep egui's focus navigation off
    // them.
    if response.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
    }

    let drag_id: Id = response.id.with("drag");
    let scroll_id: Id = response.id.with("scroll");
    let pops_id: Id = response.id.with("pops");
    // Stored as `Option<Drag>` so the idle state is a plain `None` value in temp memory.
    let mut drag: Option<Drag> = ui.data(|d| d.get_temp::<Option<Drag>>(drag_id)).flatten();
    let mut pops: Vec<Pop> = ui
        .data(|d| d.get_temp::<Vec<Pop>>(pops_id))
        .unwrap_or_default();
    let now = ui.input(|i| i.time);

    // Pointer gestures (primary button).
    if usable {
        let pos = response.interact_pointer_pos();
        let (primary_down, ctrl, shift) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.modifiers.command,
                i.modifiers.shift,
            )
        });
        let began = (response.is_pointer_button_down_on() && primary_down)
            || response.clicked()
            || response.double_clicked();
        if drag.is_none()
            && began
            && let Some(p) = pos
            && let Some(cell) = cell_at(model, grid, p)
        {
            drag = Some(Drag::begin(model, grid, notes, cell, p, ctrl));
            response.request_focus();
        }
        if let Some(d) = drag.as_mut() {
            if let Some(p) = pos {
                d.current = clamped_cell_at(model, grid, p);
                d.current_pos = p.clamp(grid.min, grid.max);
            }
            let still_down = response.is_pointer_button_down_on() && primary_down;
            if !still_down {
                finish(model, grid, notes, d, &response, shift, &mut out);
                drag = None;
            }
        }
    } else {
        drag = None;
    }

    // Right button: a click on a note, or the button held while crossing one, removes it.
    if usable && !out.has_edit() && drag.is_none() {
        let (secondary_down, pointer) =
            ui.input(|i| (i.pointer.secondary_down(), i.pointer.interact_pos()));
        let over = response.secondary_clicked() || (secondary_down && response.contains_pointer());
        if over
            && let Some(p) = pointer
            && grid.contains(p)
            && let Some((i, _)) = hit_note(model, grid, notes, p)
        {
            out.removed = Some(i);
            model.deselect(i);
            pops.push(Pop {
                rect: note_rect(model, grid, &notes[i]).intersect(grid),
                start: now,
            });
            response.request_focus();
        }
    }

    // Keyboard (never on top of a pointer edit).
    if !out.has_edit() && response.has_focus() {
        keyboard(ui, model, notes, &mut out);
    }

    // Wheel: one row per `row_height` points; shift+wheel is reserved.
    if usable && response.contains_pointer() {
        let (delta, shift) = ui.input(|i| (i.smooth_scroll_delta(), i.modifiers.shift));
        if delta.y != 0.0 && !shift {
            ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
            let mut accum = ui.data(|d| d.get_temp::<f32>(scroll_id)).unwrap_or(0.0) + delta.y;
            let rows = (accum / model.row_height.max(1.0)).round();
            if rows != 0.0 {
                accum -= rows * model.row_height.max(1.0);
                let max = model.scale_index(model.max_low_pitch()) as i64;
                let index = (model.scale_index(model.low_pitch) as i64 + rows as i64).clamp(0, max);
                let low = model.pitch_of_index(index as usize);
                if low != model.low_pitch {
                    model.low_pitch = low;
                    out.scrolled = true;
                }
            }
            ui.data_mut(|d| d.insert_temp(scroll_id, accum));
        }
    }

    pops.retain(|p| now - p.start < POP_SECONDS);
    if !pops.is_empty() {
        ui.ctx().request_repaint();
    }
    ui.data_mut(|d| d.insert_temp(drag_id, drag.clone()));
    ui.data_mut(|d| d.insert_temp(pops_id, pops.clone()));

    if ui.is_rect_visible(outer) {
        paint(
            ui,
            theme,
            model,
            notes,
            outer,
            grid,
            &response,
            drag.as_ref(),
            &pops,
            now,
        );
    }

    response.widget_info(|| WidgetInfo::labeled(Role::Group, ui.is_enabled(), "Piano roll"));
    out
}

/// The keyboard part of [`piano_roll`]: Delete / Backspace, Ctrl+A and the arrow nudges.
fn keyboard(
    ui: &mut Ui,
    model: &mut PianoRollModel,
    notes: &[RollNote],
    out: &mut PianoRollOutput,
) {
    let delete = ui.input_mut(|i| {
        i.consume_key(Modifiers::NONE, Key::Delete) | i.consume_key(Modifiers::NONE, Key::Backspace)
    });
    if delete && !model.selection.is_empty() {
        if model.selection.len() == 1 {
            out.removed = model.selection.first().copied();
        } else {
            let mut all = model.selection.clone();
            all.sort_unstable();
            out.removed_many = all;
        }
        model.set_selection(Vec::new());
        return;
    }
    if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
        model.set_selection((0..notes.len()).collect());
        return;
    }
    if model.selection.is_empty() {
        return;
    }
    let octave = model.octave_rows();
    let nudges: [(Modifiers, Key, i64, i64); 6] = [
        (Modifiers::COMMAND, Key::ArrowUp, 0, octave),
        (Modifiers::COMMAND, Key::ArrowDown, 0, -octave),
        (Modifiers::NONE, Key::ArrowUp, 0, 1),
        (Modifiers::NONE, Key::ArrowDown, 0, -1),
        (Modifiers::NONE, Key::ArrowLeft, -1, 0),
        (Modifiers::NONE, Key::ArrowRight, 1, 0),
    ];
    for (modifiers, key, ds, drows) in nudges {
        if ui.input_mut(|i| i.consume_key(modifiers, key)) {
            let group: Vec<(usize, RollNote)> = model
                .selection
                .iter()
                .filter_map(|&i| notes.get(i).map(|n| (i, *n)))
                .collect();
            // A nudge that does not fit (a note at the edge) moves nobody.
            if shift_fits(model, &group, ds, drows) {
                let moved = shift_notes(model, &group, ds, drows);
                if moved.len() == 1 {
                    out.moved = moved.first().copied();
                } else {
                    out.moved_many = moved;
                }
            }
            return;
        }
    }
}

/// Commits a finished gesture into `out` (and the selection).
fn finish(
    model: &mut PianoRollModel,
    grid: Rect,
    notes: &[RollNote],
    d: &Drag,
    response: &Response,
    shift: bool,
    out: &mut PianoRollOutput,
) {
    match (d.gesture, d.note) {
        (Gesture::Draw, _) => out.add = Some(d.preview(model)),
        (Gesture::Select, _) => {
            let marquee = d.marquee();
            let touched: Vec<usize> = notes
                .iter()
                .enumerate()
                .filter(|(_, n)| model.is_visible(n.pitch))
                .filter(|(_, n)| note_rect(model, grid, n).intersects(marquee))
                .map(|(i, _)| i)
                .collect();
            model.set_selection(touched);
        }
        (gesture, Some(i)) if i < notes.len() => {
            let preview = d.preview(model);
            let unmoved = preview.start == d.original.start
                && preview.pitch == d.original.pitch
                && preview.length == d.original.length;
            if response.double_clicked() {
                out.removed = Some(i);
                model.deselect(i);
            } else if unmoved {
                // A click (no cell movement).
                if d.ctrl {
                    if model.is_selected(i) {
                        model.deselect(i);
                    } else {
                        model.select_also(i);
                    }
                } else {
                    if shift && gesture == Gesture::Move {
                        let mut n = d.original;
                        n.velocity = if n.velocity >= ACCENT_THRESHOLD {
                            DEFAULT_VELOCITY
                        } else {
                            ACCENT_VELOCITY
                        };
                        out.moved = Some((i, n));
                    }
                    out.selected = Some(i);
                    model.set_selection(vec![i]);
                }
            } else if d.group.is_empty() {
                out.moved = Some((i, preview));
            } else {
                out.moved_many = d.preview_group(model);
            }
        }
        _ => {}
    }
}

/// Paints the roll: background, rows, lines, key column, notes, drag ghosts, marquee, pops,
/// playhead and the focus ring.
#[allow(clippy::too_many_arguments)]
fn paint(
    ui: &Ui,
    theme: &Theme,
    model: &PianoRollModel,
    notes: &[RollNote],
    outer: Rect,
    grid: Rect,
    response: &Response,
    drag: Option<&Drag>,
    pops: &[Pop],
    now: f64,
) {
    // The ring sits outside `outer`, so it uses the unclipped painter.
    if response.has_focus() {
        paint_focus_rect(ui.painter(), outer, ROLL_RADIUS, theme);
    }
    let painter = ui.painter().with_clip_rect(outer.intersect(ui.clip_rect()));
    paint_soft_rect(&painter, outer, ROLL_RADIUS, Depth::InsetM, theme);
    if model.steps == 0 || model.pitches == 0 {
        return;
    }

    // Rows: black-key tint across the whole width, faint separators in the grid.
    let tint = black_row_tint(theme);
    let separator = Stroke::new(1.0, theme.shadow_dark.gamma_multiply(0.25));
    for row in 0..model.pitches {
        let Some(pitch) = model.pitch_at_row(row) else {
            break;
        };
        let r = row_rect(model, grid, pitch);
        if is_black_key(pitch) {
            let full = Rect::from_min_max(pos2(outer.min.x, r.min.y), r.max);
            painter.rect_filled(full, 0.0, tint);
        }
        if model.held[usize::from(pitch)] {
            painter.rect_filled(r, 0.0, theme.accent.gamma_multiply(HELD_ROW_WASH));
        }
        if row > 0 {
            painter.hline(grid.x_range(), r.max.y, separator);
        }
    }

    // Beat and bar lines.
    for s in 1..model.steps {
        if s % BEAT != 0 {
            continue;
        }
        let x = grid.min.x + s as f32 * model.step_width;
        let w = if s % BAR == 0 { 2.0 } else { 1.0 };
        painter.vline(x, grid.y_range(), Stroke::new(w, theme.shadow_dark));
    }
    painter.vline(
        grid.min.x,
        grid.y_range(),
        Stroke::new(1.0, theme.shadow_dark),
    );

    // Hovered cell (idle only) and the resize cursor over a note's right edge.
    let hover = response.hover_pos().filter(|p| grid.contains(*p));
    if drag.is_none()
        && let Some(p) = hover
    {
        if let Some((step, pitch)) = cell_at(model, grid, p) {
            let row = row_rect(model, grid, pitch);
            let x = grid.min.x + step as f32 * model.step_width;
            let cell =
                Rect::from_min_size(pos2(x, row.min.y), vec2(model.step_width, model.row_height));
            painter.rect_filled(cell, 0.0, theme.shadow_highlight.gamma_multiply(0.5));
        }
        if hit_note(model, grid, notes, p).is_some_and(|(_, edge)| edge) {
            ui.output_mut(|o| o.cursor_icon = CursorIcon::ResizeHorizontal);
        }
    }

    // Notes.
    for (i, n) in notes.iter().enumerate() {
        if !model.is_visible(n.pitch) || usize::from(n.start) >= model.steps {
            continue;
        }
        let r = note_rect(model, grid, n).intersect(grid);
        if r.width() <= 0.0 {
            continue;
        }
        let accented = n.velocity >= ACCENT_THRESHOLD;
        let fill = if accented {
            theme.accent_hover
        } else {
            theme.accent
        };
        if n.selected || model.is_selected(i) {
            paint_soft_rect_with_fill(&painter, r, NOTE_RADIUS, Depth::OnGlow, fill, theme);
        } else {
            painter.rect_filled(r, NOTE_RADIUS, fill);
        }
        // Accent marker: a 6 pt on-accent dot near the left end, kept inside the bar (a
        // length-1 note at the default 24 pt step is wide enough; clipped stubs skip it).
        if accented && r.width() >= ACCENT_DOT_INSET * 2.0 {
            let radius = (ACCENT_DOT * 0.5).min(r.height() * 0.5);
            painter.circle_filled(
                pos2(r.min.x + ACCENT_DOT_INSET, r.center().y),
                radius,
                theme.on_accent,
            );
        }
        if r.width() > HANDLE_INSET * 2.0 {
            let x = r.max.x - HANDLE_INSET;
            painter.vline(
                x,
                (r.min.y + 3.0)..=(r.max.y - 3.0),
                Stroke::new(1.0, theme.on_accent),
            );
        }
    }

    // Drag ghosts: the note(s) as they would be committed, or the marquee.
    if let Some(d) = drag {
        let ghost_fill = theme.accent.gamma_multiply(0.45);
        match d.gesture {
            Gesture::Select => {
                let m = d.marquee().intersect(grid);
                painter.rect_filled(m, 2.0, theme.accent.gamma_multiply(MARQUEE_WASH));
                painter.rect_stroke(
                    m,
                    2.0,
                    Stroke::new(1.0, theme.accent),
                    egui::StrokeKind::Inside,
                );
            }
            _ if !d.group.is_empty() => {
                for (_, ghost) in d.preview_group(model) {
                    if model.is_visible(ghost.pitch) {
                        let r = note_rect(model, grid, &ghost).intersect(grid);
                        if r.width() > 0.0 {
                            painter.rect_filled(r, NOTE_RADIUS, ghost_fill);
                        }
                    }
                }
            }
            _ => {
                let ghost = d.preview(model);
                if model.is_visible(ghost.pitch) {
                    let r = note_rect(model, grid, &ghost).intersect(grid);
                    if r.width() > 0.0 {
                        painter.rect_filled(r, NOTE_RADIUS, ghost_fill);
                    }
                }
            }
        }
    }

    // Pops: the removed note's outline grows and fades.
    for pop in pops {
        let t = ((now - pop.start) / POP_SECONDS).clamp(0.0, 1.0) as f32;
        let grow = 1.0 + POP_GROWTH * t;
        let r = Rect::from_center_size(pop.rect.center(), pop.rect.size() * grow);
        let alpha = 1.0 - t;
        painter.rect_filled(r, NOTE_RADIUS, theme.accent.gamma_multiply(0.35 * alpha));
        painter.rect_stroke(
            r,
            NOTE_RADIUS,
            Stroke::new(1.5, theme.accent_hover.gamma_multiply(alpha)),
            egui::StrokeKind::Outside,
        );
    }

    // Playhead.
    if let Some(step) = model.playhead_step {
        let x = grid.min.x + step * model.step_width;
        if x >= grid.min.x && x <= grid.max.x {
            painter.vline(x, grid.y_range(), Stroke::new(PLAYHEAD_WIDTH, theme.accent));
        }
    }

    // Key column: one key per row. White keys are surface tiles spanning the column with their
    // note name; black keys are shorter `key_black` stubs on the left. A held key (the
    // computer keyboard, the on-screen piano or MIDI) is accent with its name in on-accent.
    let white = theme
        .surface
        .lerp_to_gamma(theme.shadow_highlight, WHITE_KEY_LIFT);
    for row in 0..model.pitches {
        let Some(pitch) = model.pitch_at_row(row) else {
            break;
        };
        let key = key_rect(model, outer, pitch);
        let held = model.held[usize::from(pitch)];
        let black = is_black_key(pitch);
        let fill = if held {
            theme.accent
        } else if black {
            theme.key_black
        } else {
            white
        };
        let radius = if black { 3.0 } else { 4.0 };
        painter.rect_filled(key, radius, fill);
        if !black {
            let colour = if held {
                theme.on_accent
            } else if pitch % 12 == 0 {
                theme.text_primary
            } else {
                theme.text_secondary
            };
            text::paint_text(
                &painter,
                pos2(key.max.x - KEY_LABEL_INSET, key.center().y),
                Align2::RIGHT_CENTER,
                &note_name(pitch),
                TextKind::Label,
                colour,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> (PianoRollModel, Rect) {
        let model = PianoRollModel::default();
        let outer = Rect::from_min_size(pos2(10.0, 20.0), model.size());
        (model, grid_rect(outer))
    }

    fn note(start: u8, length: u8, pitch: u8) -> RollNote {
        RollNote {
            start,
            length,
            pitch,
            velocity: 0.8,
            selected: false,
        }
    }

    #[test]
    fn default_size_and_grid() {
        let model = PianoRollModel::default();
        assert_eq!(model.size(), vec2(48.0 + 16.0 * 24.0, 36.0 * 14.0));
        let outer = Rect::from_min_size(pos2(10.0, 20.0), model.size());
        let g = grid_rect(outer);
        assert_eq!(g.min, pos2(58.0, 20.0));
        assert_eq!(g.max, outer.max);
        assert_eq!(model.max_low_pitch(), 91);
    }

    #[test]
    fn key_rects_fill_the_key_column_one_per_row() {
        let model = PianoRollModel::default();
        let outer = Rect::from_min_size(pos2(10.0, 20.0), model.size());
        // The bottom row is C2 (36): a white key spanning the column minus the insets.
        let c2 = key_rect(&model, outer, 36);
        assert_eq!(c2.min.x, 10.0 + KEY_INSET_X);
        assert_eq!(c2.max.x, 10.0 + KEY_COLUMN_WIDTH - KEY_INSET_X);
        assert_eq!(c2.max.y, outer.max.y - KEY_INSET_Y);
        assert_eq!(c2.height(), model.row_height - 2.0 * KEY_INSET_Y);
        // C#2 sits one row up and is a short stub on the left.
        let cs2 = key_rect(&model, outer, 37);
        assert_eq!(cs2.min.x, c2.min.x);
        assert_eq!(cs2.width(), c2.width() * BLACK_KEY_FRACTION);
        assert_eq!(cs2.max.y, c2.min.y - 2.0 * KEY_INSET_Y);
        // Every visible row has a key inside the column.
        for pitch in 36..72u8 {
            let k = key_rect(&model, outer, pitch);
            assert!(outer.contains_rect(k), "{pitch}: {k:?}");
            assert!(k.max.x < grid_rect(outer).min.x);
        }
        assert!(!model.held.iter().any(|h| *h), "nothing held by default");
    }

    #[test]
    fn white_key_scale_round_trips() {
        assert_eq!(white_index(0), 0, "C-1");
        assert_eq!(white_index(1), 0, "C#-1 shares C-1's index");
        assert_eq!(white_index(2), 1, "D-1");
        assert_eq!(white_index(12), 7, "C0");
        assert_eq!(white_index(60), 35, "C4");
        assert_eq!(white_index(127), WHITE_KEYS - 1, "G9");
        for index in 0..WHITE_KEYS {
            let p = white_pitch(index);
            assert!(!is_black_key(p), "{index} -> {p}");
            assert_eq!(white_index(p), index);
        }
        assert_eq!(white_pitch(WHITE_KEYS + 5), 127, "clamped");
    }

    #[test]
    fn hiding_the_black_keys_packs_the_white_keys_into_the_rows() {
        let mut model = PianoRollModel {
            black_keys: false,
            pitches: 8,
            ..PianoRollModel::default()
        };
        model.normalise(0);
        assert_eq!(model.low_pitch, 36);
        // Rows: C2 D2 E2 F2 G2 A2 B2 C3.
        let rows: Vec<u8> = (0..8).filter_map(|r| model.pitch_at_row(r)).collect();
        assert_eq!(rows, [36, 38, 40, 41, 43, 45, 47, 48]);
        assert_eq!(model.pitch_at_row(8), Some(50), "D3 would be the next row");
        assert!(model.is_visible(48) && !model.is_visible(50));
        assert!(!model.is_visible(37), "C#2 has no row");
        assert_eq!(model.row_of(37), 0, "it sits on C2's row");
        assert_eq!(model.shift_pitch(36, 1), 38);
        assert_eq!(
            model.shift_pitch(36, model.octave_rows()),
            48,
            "an octave is 7 rows"
        );
        assert_eq!(model.shift_pitch(127, 3), 127);
        assert_eq!(model.max_low_pitch(), white_pitch(WHITE_KEYS - 1 - 8));

        // Geometry follows: C3 is the top row, C#2 a zero-height line between C2 and D2.
        let outer = Rect::from_min_size(pos2(0.0, 0.0), model.size());
        let g = grid_rect(outer);
        assert_eq!(row_rect(&model, g, 48).min.y, g.min.y);
        let cs2 = row_rect(&model, g, 37);
        assert_eq!(cs2.height(), 0.0);
        assert_eq!(cs2.min.y, row_rect(&model, g, 36).min.y);
        assert_eq!(
            cell_at(&model, g, g.left_top() + vec2(1.0, 1.0)),
            Some((0, 48))
        );
        assert_eq!(
            cell_at(&model, g, g.left_bottom() + vec2(1.0, -1.0)),
            Some((0, 36))
        );

        // A black bottom pitch snaps up to the next white key.
        model.low_pitch = 37;
        model.normalise(0);
        assert_eq!(model.low_pitch, 38);
        // Black keys back on: the usual semitone rows.
        model.black_keys = true;
        model.normalise(0);
        assert_eq!(model.pitch_at_row(1), Some(39));
        assert!(model.is_visible(39));
    }

    #[test]
    fn selection_helpers_keep_the_primary_a_member() {
        let mut model = PianoRollModel::default();
        model.set_selection(vec![2, 0, 2]);
        assert_eq!(model.selection, [2, 0]);
        assert_eq!(model.selected, Some(0));
        model.select_also(5);
        assert_eq!(model.selected, Some(5));
        model.select_also(2);
        assert_eq!(model.selection, [2, 0, 5]);
        assert_eq!(model.selected, Some(2));
        model.deselect(2);
        assert_eq!(model.selection, [0, 5]);
        assert_eq!(model.selected, Some(5));
        // Normalising to a shorter note list drops stale indices.
        model.normalise(3);
        assert_eq!(model.selection, [0]);
        assert_eq!(model.selected, Some(0));
        model.selected = Some(9);
        model.normalise(3);
        assert_eq!(
            model.selected,
            Some(0),
            "a primary past the notes is replaced"
        );
        model.selected = Some(2);
        model.normalise(3);
        assert_eq!(
            model.selection,
            [0, 2],
            "a stray primary joins the selection"
        );
        assert_eq!(model.selected, Some(2));
        model.set_selection(Vec::new());
        assert_eq!(model.selected, None);
        assert!(!model.is_selected(0));
    }

    #[test]
    fn cell_and_note_rect_round_trip() {
        let (model, g) = grid();
        for step in 0..16 {
            for pitch in 36..72u8 {
                let n = note(step as u8, 1, pitch);
                let r = note_rect(&model, g, &n);
                assert_eq!(cell_at(&model, g, r.center()), Some((step, pitch)), "{n:?}");
                assert_eq!(r.height(), model.row_height - 2.0 * NOTE_INSET_Y);
            }
        }
        // Longer notes: the cell under the first column's centre is the start.
        let long = RollNote {
            velocity: 1.0,
            ..note(3, 5, 50)
        };
        let r = note_rect(&model, g, &long);
        assert_eq!(r.width(), 5.0 * 24.0);
        let first = r.left_center() + vec2(model.step_width * 0.5, 0.0);
        assert_eq!(cell_at(&model, g, first), Some((3, 50)));
    }

    #[test]
    fn cells_outside_the_grid_are_none_and_rows_rise_upwards() {
        let (model, g) = grid();
        assert_eq!(cell_at(&model, g, g.min - vec2(1.0, 0.0)), None);
        assert_eq!(cell_at(&model, g, g.max + vec2(0.0, 1.0)), None);
        // Bottom-left cell is (0, low_pitch); top-left is (0, low_pitch + pitches - 1).
        assert_eq!(
            cell_at(&model, g, g.left_bottom() + vec2(1.0, -1.0)),
            Some((0, 36))
        );
        assert_eq!(
            cell_at(&model, g, g.left_top() + vec2(1.0, 1.0)),
            Some((0, 71))
        );
        assert_eq!(
            cell_at(&model, g, g.right_bottom() + vec2(-1.0, -1.0)),
            Some((15, 36))
        );
        // Clamping snaps far-away positions onto the edge cells.
        assert_eq!(
            clamped_cell_at(&model, g, g.max + vec2(500.0, 500.0)),
            (15, 36)
        );
        assert_eq!(
            clamped_cell_at(&model, g, g.min - vec2(500.0, 500.0)),
            (0, 71)
        );
    }

    #[test]
    fn black_keys() {
        let blacks: Vec<u8> = (60..72u8).filter(|&p| is_black_key(p)).collect();
        assert_eq!(blacks, [61, 63, 66, 68, 70]);
        assert!(!is_black_key(0));
        assert!(is_black_key(127 - 1)); // 126 = F#9
    }

    #[test]
    fn drag_previews_clamp_to_the_grid() {
        let (mut model, g) = grid();
        let notes = [note(14, 2, 40)];
        // Draw from step 5 to the right edge: length clipped at the grid end.
        let draw_pos = note_rect(&model, g, &note(5, 1, 45)).center();
        let mut d = Drag::begin(&mut model, g, &notes, (5, 45), draw_pos, false);
        assert_eq!(d.gesture, Gesture::Draw);
        d.current = (15, 45);
        let p = d.preview(&model);
        assert_eq!((p.start, p.length, p.pitch), (5, 11, 45));
        assert_eq!(p.velocity, DEFAULT_VELOCITY);
        // Dragging left of the press keeps length 1.
        d.current = (2, 45);
        assert_eq!(d.preview(&model).length, 1);

        // Move the note off the right edge and below pitch 0: both clamp. The press selects.
        let body = note_rect(&model, g, &notes[0]).left_center() + vec2(4.0, 0.0);
        let mut m = Drag::begin(&mut model, g, &notes, (14, 40), body, false);
        assert_eq!((m.gesture, m.note), (Gesture::Move, Some(0)));
        assert_eq!(model.selection, [0]);
        m.current = (15, 36);
        let p = m.preview(&model);
        assert_eq!((p.start, p.length, p.pitch), (14, 2, 36));

        // Resize from the right edge zone: to step 15 inclusive.
        let edge = note_rect(&model, g, &notes[0]).right_center() - vec2(2.0, 0.0);
        let mut r = Drag::begin(&mut model, g, &notes, (15, 40), edge, false);
        assert_eq!(r.gesture, Gesture::Resize);
        r.current = (0, 40);
        assert_eq!(r.preview(&model).length, 1);
        r.current = (15, 40);
        assert_eq!(r.preview(&model).length, 2);

        // A Ctrl-press on empty space is a marquee and leaves the selection alone.
        let s = Drag::begin(&mut model, g, &notes, (2, 50), draw_pos, true);
        assert_eq!(s.gesture, Gesture::Select);
        assert_eq!(model.selection, [0]);
        // A plain press on empty space clears it.
        Drag::begin(&mut model, g, &notes, (2, 50), draw_pos, false);
        assert!(model.selection.is_empty());
    }

    #[test]
    fn group_gestures_move_and_resize_the_whole_selection() {
        let (mut model, g) = grid();
        let notes = [note(2, 2, 40), note(4, 1, 44), note(2, 1, 47)];
        model.set_selection(vec![0, 1, 2]);
        let body = note_rect(&model, g, &notes[1]).center();
        let mut m = Drag::begin(&mut model, g, &notes, (4, 44), body, false);
        assert_eq!(m.gesture, Gesture::Move);
        assert_eq!(m.group.len(), 3, "the selection moves together");
        // Two steps right, three rows up.
        m.current = (6, 47);
        let moved = m.preview_group(&model);
        assert_eq!(moved[0].1, note(4, 2, 43));
        assert_eq!(moved[1].1, note(6, 1, 47));
        assert_eq!(moved[2].1, note(4, 1, 50));
        // Far right and far down: the shift clamps so the group keeps its shape.
        m.current = (15, 36);
        let moved = m.preview_group(&model);
        assert_eq!(moved[1].1.start, 15);
        assert_eq!(moved[0].1.start, 13, "the long note ends at the grid edge");
        assert_eq!(moved[0].1.pitch, 32);
        assert_eq!(moved[2].1.pitch, 39);

        // Resize from the pressed note's right edge: every member grows by the same steps.
        let edge = note_rect(&model, g, &notes[0]).right_center() - vec2(2.0, 0.0);
        let mut r = Drag::begin(&mut model, g, &notes, (3, 40), edge, false);
        assert_eq!(r.gesture, Gesture::Resize);
        r.current = (5, 40);
        let resized = r.preview_group(&model);
        assert_eq!(resized[0].1.length, 4);
        assert_eq!(resized[1].1.length, 3);
        assert_eq!(resized[2].1.length, 3);
        r.current = (0, 40);
        assert!(r.preview_group(&model).iter().all(|(_, n)| n.length == 1));

        // Pressing a note outside the selection drops the group.
        model.set_selection(vec![1, 2]);
        let body0 = note_rect(&model, g, &notes[0]).center();
        let single = Drag::begin(&mut model, g, &notes, (2, 40), body0, false);
        assert!(single.group.is_empty());
        assert_eq!(model.selection, [0]);
    }

    #[test]
    fn nudges_shift_by_rows_and_octaves() {
        let model = PianoRollModel::default();
        let group = [(0, note(0, 1, 60)), (1, note(3, 2, 64))];
        let up = shift_notes(&model, &group, 0, model.octave_rows());
        assert_eq!((up[0].1.pitch, up[1].1.pitch), (72, 76));
        let right = shift_notes(&model, &group, 1, 0);
        assert_eq!((right[0].1.start, right[1].1.start), (1, 4));
        // Nothing can leave the grid: the left nudge of a note at 0 moves nobody.
        let left = shift_notes(&model, &group, -1, 0);
        assert_eq!((left[0].1.start, left[1].1.start), (0, 3));
        let top = [(0, note(0, 1, 120))];
        assert_eq!(shift_notes(&model, &top, 0, 12)[0].1.pitch, 127, "clamped");
        assert!(!shift_fits(&model, &top, 0, 12), "so a nudge would not fit");
        assert!(shift_fits(&model, &top, 0, 7) && shift_fits(&model, &group, 0, -12));
        assert!(!shift_fits(&model, &group, -1, 0));
        let white = PianoRollModel {
            black_keys: false,
            ..PianoRollModel::default()
        };
        let octave = shift_notes(&white, &group, 0, white.octave_rows());
        assert_eq!((octave[0].1.pitch, octave[1].1.pitch), (72, 76));
        let row = shift_notes(&white, &group, 0, 1);
        assert_eq!(
            (row[0].1.pitch, row[1].1.pitch),
            (62, 65),
            "a white key each"
        );
        assert_eq!(resize_notes(&model, &group, 20)[1].1.length, 13);
        assert_eq!(resize_notes(&model, &group, -5)[1].1.length, 1);
    }

    #[test]
    fn tint_is_a_little_darker_than_the_surface() {
        for theme in [Theme::dark(), Theme::light()] {
            let lum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
            let t = black_row_tint(&theme);
            assert!(lum(t) < lum(theme.surface) && lum(t) > lum(theme.shadow_dark));
        }
    }
}
