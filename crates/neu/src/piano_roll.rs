//! The piano roll (M7): time on X in steps, pitch on Y, notes as rounded accent bars.
//!
//! Layout: a [`KEY_COLUMN_WIDTH`] key column on the left (one key per row, see [`key_rect`]:
//! white keys as lit surface tiles labelled "D3" etc., black keys as shorter `key_black`
//! stubs; a key in `model.held` paints accent and its grid row gets a faint accent wash),
//! then `steps` columns of `step_width` by `pitches` rows of `row_height`.
//! The bottom row is `low_pitch`; pitch rises upwards. The whole widget is one inset-m surface;
//! black-key rows get a faint shadow-dark tint, beat lines fall every [`BEAT`] steps (1 pt) and
//! bar lines every [`BAR`] steps (2 pt), the playhead is a 2 pt accent line at a fractional step.
//! Accented notes (velocity >= [`ACCENT_THRESHOLD`]) carry a [`ACCENT_DOT`] (6 pt) on-accent
//! dot near their left end on top of the `accent_hover` fill, like the step cell's accent
//! marker, so the state does not rest on the 1.2:1 accent / accent-hover difference alone.
//! While the roll has keyboard focus a 1.5 pt accent ring (the shared
//! [`crate::shadow::paint_focus_rect`]) is drawn 1 pt outside the whole roll.
//!
//! As-built fill (not in `design/WINNER.md`): while idle, the grid cell under the pointer is
//! highlighted with a half-strength `shadow_highlight` wash so the draw target reads before
//! the press. It is a hover-only cue and conveys no state.
//!
//! Interaction is one `Sense::click_and_drag` response with a small gesture state machine kept
//! in egui's temporary memory under `response.id` (see [`piano_roll`] for the rules). The
//! widget never edits the notes itself: it reports at most one edit per frame in
//! [`PianoRollOutput`] and the host applies it. It does update `model.low_pitch` (wheel) and
//! `model.selected` (click) in place, because those are view state.
//!
//! All geometry is public and unit-tested: [`grid_rect`], [`cell_at`], [`note_rect`],
//! [`row_rect`], [`is_black_key`].

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
    /// honours `PianoRollModel::selected`).
    pub selected: bool,
}

/// View state of the roll: grid extent, scroll position, cell size, playhead and selection.
#[derive(Debug, Clone, PartialEq)]
pub struct PianoRollModel {
    /// Number of step columns (16 / 32 / 64).
    pub steps: usize,
    /// MIDI pitch of the bottom row.
    pub low_pitch: u8,
    /// Number of visible rows.
    pub pitches: usize,
    /// Column width in points.
    pub step_width: f32,
    /// Row height in points.
    pub row_height: f32,
    /// Playhead position in (fractional) steps; `None` hides it.
    pub playhead_step: Option<f32>,
    /// Index of the selected note, if any. Set by a click, cleared when that note is removed.
    pub selected: Option<usize>,
    /// Pitches currently sounding (the host's held keys): their key in the key column paints
    /// accent and their grid row gets a faint accent wash. View state only, never an edit.
    pub held: [bool; 128],
}

impl Default for PianoRollModel {
    /// 16 steps, three octaves (36 rows) from C2 (36), 24 x 14 cells, no playhead, no held
    /// keys.
    fn default() -> Self {
        Self {
            steps: 16,
            low_pitch: 36,
            pitches: 36,
            step_width: 24.0,
            row_height: 14.0,
            playhead_step: None,
            selected: None,
            held: [false; 128],
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

    /// The highest `low_pitch` the wheel scrolls to: `127 - pitches` (0 when more than the
    /// whole MIDI range is visible).
    pub fn max_low_pitch(&self) -> u8 {
        127_usize.saturating_sub(self.pitches).min(127) as u8
    }

    /// Whether `pitch` is on a visible row.
    pub fn is_visible(&self, pitch: u8) -> bool {
        let row = usize::from(pitch).checked_sub(usize::from(self.low_pitch));
        row.is_some_and(|r| r < self.pitches)
    }
}

/// What the roll reported this frame. At most one of `add`, `moved`, `removed` is set.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PianoRollOutput {
    /// A note was drawn (released a draw gesture or clicked an empty cell).
    pub add: Option<RollNote>,
    /// Note `index` should become this note (moved, resized, or velocity toggled).
    pub moved: Option<(usize, RollNote)>,
    /// Note `index` should be removed (double-click or Delete / Backspace).
    pub removed: Option<usize>,
    /// Note `index` was selected by a click this frame (`model.selected` is updated too).
    pub selected: Option<usize>,
    /// `model.low_pitch` changed from the wheel this frame.
    pub scrolled: bool,
}

impl PianoRollOutput {
    /// `true` when an edit (`add`, `moved` or `removed`) was reported.
    pub fn has_edit(&self) -> bool {
        self.add.is_some() || self.moved.is_some() || self.removed.is_some()
    }
}

/// The grid part of a roll allocated in `outer`: everything right of the key column.
pub fn grid_rect(outer: Rect) -> Rect {
    Rect::from_min_max(pos2(outer.min.x + KEY_COLUMN_WIDTH, outer.min.y), outer.max)
}

/// The `(step, pitch)` cell under `pos`, or `None` outside the grid (or above MIDI 127).
pub fn cell_at(model: &PianoRollModel, grid: Rect, pos: Pos2) -> Option<(usize, u8)> {
    if model.steps == 0 || model.pitches == 0 || !grid.contains(pos) {
        return None;
    }
    let (step, pitch) = clamped_cell_at(model, grid, pos);
    // Rows above MIDI 127 are empty space, not a cell.
    let row = ((grid.max.y - pos.y) / model.row_height).floor().max(0.0) as usize;
    (usize::from(model.low_pitch) + row <= 127).then_some((step, pitch))
}

/// The cell nearest `pos`, clamping positions outside the grid onto its edge rows / columns
/// and the pitch to 0..=127. `model.steps` and `model.pitches` must be non-zero.
fn clamped_cell_at(model: &PianoRollModel, grid: Rect, pos: Pos2) -> (usize, u8) {
    let col = ((pos.x - grid.min.x) / model.step_width).floor();
    let step = (col.max(0.0) as usize).min(model.steps - 1);
    let row_f = ((grid.max.y - pos.y) / model.row_height).floor();
    let row = (row_f.max(0.0) as usize).min(model.pitches - 1);
    let pitch = (usize::from(model.low_pitch) + row).min(127) as u8;
    (step, pitch)
}

/// The full-height rect of the row for `pitch` (may lie outside `grid` for pitches off-screen).
pub fn row_rect(model: &PianoRollModel, grid: Rect, pitch: u8) -> Rect {
    let row = i32::from(pitch) - i32::from(model.low_pitch);
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
    /// Press on a note body: the note follows the pointer by whole cells.
    Move,
    /// Press within [`RESIZE_HANDLE`] of a note's right edge: the length follows the pointer.
    Resize,
}

/// The in-progress gesture, kept in temp memory between the press and the release.
#[derive(Debug, Clone, Copy)]
struct Drag {
    gesture: Gesture,
    /// The note under the press (`Move` / `Resize`).
    note: Option<usize>,
    /// That note as it was at the press (a default note for `Draw`).
    original: RollNote,
    /// Cell of the press.
    press: (usize, u8),
    /// Cell under the pointer now (clamped to the grid).
    current: (usize, u8),
}

impl Drag {
    /// Starts the gesture for a press at `cell` / `pos` given the notes under the pointer.
    fn begin(
        model: &PianoRollModel,
        grid: Rect,
        notes: &[RollNote],
        cell: (usize, u8),
        pos: Pos2,
    ) -> Self {
        let hit = hit_note(model, grid, notes, pos);
        let (gesture, note, original) = match hit {
            Some((i, on_edge)) => (
                if on_edge {
                    Gesture::Resize
                } else {
                    Gesture::Move
                },
                Some(i),
                notes[i],
            ),
            None => (
                Gesture::Draw,
                None,
                RollNote {
                    start: cell.0 as u8,
                    length: 1,
                    pitch: cell.1,
                    velocity: DEFAULT_VELOCITY,
                    selected: false,
                },
            ),
        };
        Self {
            gesture,
            note,
            original,
            press: cell,
            current: cell,
        }
    }

    /// The note this gesture would commit if released now.
    fn preview(&self, model: &PianoRollModel) -> RollNote {
        let steps = model.steps.max(1) as i32;
        let (cs, cp) = (self.current.0 as i32, i32::from(self.current.1));
        let (ps, pp) = (self.press.0 as i32, i32::from(self.press.1));
        let mut n = self.original;
        match self.gesture {
            Gesture::Draw => {
                let start = ps.clamp(0, steps - 1);
                n.start = start as u8;
                n.length = (cs - start + 1).clamp(1, steps - start) as u8;
            }
            Gesture::Move => {
                let len = i32::from(n.length.max(1)).min(steps);
                n.start = (i32::from(n.start) + cs - ps).clamp(0, steps - len) as u8;
                n.pitch = (i32::from(n.pitch) + cp - pp).clamp(0, 127) as u8;
            }
            Gesture::Resize => {
                let start = i32::from(n.start).min(steps - 1);
                n.length = (cs - start + 1).clamp(1, steps - start) as u8;
            }
        }
        n
    }
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

/// Draws the piano roll for `notes` and reports this frame's interaction.
///
/// Size: [`PianoRollModel::size`]. Painting: inset-m background, black-key rows tinted
/// ([`black_row_tint`]), faint row separators, beat lines (shadow-dark 1 pt every [`BEAT`]
/// steps) and bar lines (2 pt every [`BAR`]), the key column with a raised-s "C3" tile per
/// octave, notes as radius-6 bars (accent, `accent_hover` plus a 6 pt on-accent dot near the
/// left end at velocity >= 0.95, on-glow when selected) with a 1 pt on-accent handle line near
/// the right edge, a hovered-cell highlight, the 2 pt accent playhead and, while the roll has
/// keyboard focus, a 1.5 pt accent ring 1 pt outside the whole roll.
///
/// Gestures (state in temp memory under `response.id`):
/// * press on an empty cell, drag right, release: `add` with `start` = press step, `length` =
///   cells dragged + 1 (min 1, clipped to the grid end), `pitch` = press row, velocity 0.8. A
///   plain click on an empty cell adds a length-1 note.
/// * press on a note body and drag: `moved` on release with the new start / pitch (clamped
///   to the grid). Without movement it is a click: `selected` (`model.selected` is set), or
///   with shift held `moved` with the velocity toggled between 1.0 and 0.8.
/// * press within 6 pt of a note's right edge and drag: `moved` with the new length (min 1,
///   max to the grid end).
/// * double-click on a note: `removed`.
/// * Delete / Backspace while the roll has keyboard focus (it takes focus on a press) and
///   `model.selected` is set: `removed`.
/// * wheel over the roll: `model.low_pitch` moves one row per `row_height` points, clamped to
///   `0..=127 - pitches`; `scrolled` is set. Shift+wheel is reserved (no-op).
///
/// At most one of `add` / `moved` / `removed` is reported per frame.
pub fn piano_roll(
    ui: &mut Ui,
    theme: &Theme,
    model: &mut PianoRollModel,
    notes: &[RollNote],
) -> PianoRollOutput {
    let (outer, response) = ui.allocate_exact_size(model.size(), Sense::click_and_drag());
    let grid = grid_rect(outer);
    let mut out = PianoRollOutput::default();
    let usable = model.steps > 0 && model.pitches > 0;

    let drag_id: Id = response.id.with("drag");
    let scroll_id: Id = response.id.with("scroll");
    // Stored as `Option<Drag>` so the idle state is a plain `None` value in temp memory.
    let mut drag: Option<Drag> = ui.data(|d| d.get_temp::<Option<Drag>>(drag_id)).flatten();

    // Pointer gestures.
    if usable {
        let pos = response.interact_pointer_pos();
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let began = (response.is_pointer_button_down_on() && primary_down)
            || response.clicked()
            || response.double_clicked();
        if drag.is_none()
            && began
            && let Some(p) = pos
            && let Some(cell) = cell_at(model, grid, p)
        {
            drag = Some(Drag::begin(model, grid, notes, cell, p));
            response.request_focus();
        }
        if let Some(d) = drag.as_mut() {
            if let Some(p) = pos {
                d.current = clamped_cell_at(model, grid, p);
            }
            let still_down = response.is_pointer_button_down_on() && primary_down;
            if !still_down {
                let shift = ui.input(|i| i.modifiers.shift);
                finish(model, notes, d, &response, shift, &mut out);
                drag = None;
            }
        }
    } else {
        drag = None;
    }

    // Keyboard: Delete / Backspace removes the selection (never on top of a pointer edit).
    if !out.has_edit()
        && response.has_focus()
        && let Some(sel) = model.selected.filter(|&s| s < notes.len())
    {
        let delete = ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Delete)
                | i.consume_key(Modifiers::NONE, Key::Backspace)
        });
        if delete {
            out.removed = Some(sel);
            model.selected = None;
        }
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
                let max = i32::from(model.max_low_pitch());
                let low = (i32::from(model.low_pitch) + rows as i32).clamp(0, max) as u8;
                if low != model.low_pitch {
                    model.low_pitch = low;
                    out.scrolled = true;
                }
            }
            ui.data_mut(|d| d.insert_temp(scroll_id, accum));
        }
    }

    ui.data_mut(|d| d.insert_temp(drag_id, drag));

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
        );
    }

    response.widget_info(|| WidgetInfo::labeled(Role::Group, ui.is_enabled(), "Piano roll"));
    out
}

/// Commits a finished gesture into `out` (and `model.selected`).
fn finish(
    model: &mut PianoRollModel,
    notes: &[RollNote],
    d: &Drag,
    response: &Response,
    shift: bool,
    out: &mut PianoRollOutput,
) {
    let preview = d.preview(model);
    match (d.gesture, d.note) {
        (Gesture::Draw, _) => out.add = Some(preview),
        (gesture, Some(i)) if i < notes.len() => {
            if response.double_clicked() {
                out.removed = Some(i);
                if model.selected == Some(i) {
                    model.selected = None;
                }
            } else if preview.start == d.original.start
                && preview.pitch == d.original.pitch
                && preview.length == d.original.length
            {
                // A click (no cell movement).
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
                model.selected = Some(i);
            } else {
                out.moved = Some((i, preview));
            }
        }
        _ => {}
    }
}

/// Paints the roll: background, rows, lines, key column, notes, drag ghost, playhead and the
/// focus ring.
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
        let Ok(pitch) = u8::try_from(usize::from(model.low_pitch) + row) else {
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
        if n.selected || model.selected == Some(i) {
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

    // Drag ghost: the note as it would be committed.
    if let Some(d) = drag {
        let ghost = d.preview(model);
        if model.is_visible(ghost.pitch) {
            let r = note_rect(model, grid, &ghost).intersect(grid);
            if r.width() > 0.0 {
                painter.rect_filled(r, NOTE_RADIUS, theme.accent.gamma_multiply(0.45));
            }
        }
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
        let Ok(pitch) = u8::try_from(usize::from(model.low_pitch) + row) else {
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
    fn cell_and_note_rect_round_trip() {
        let (model, g) = grid();
        for step in 0..16 {
            for pitch in 36..72u8 {
                let n = RollNote {
                    start: step as u8,
                    length: 1,
                    pitch,
                    velocity: 0.8,
                    selected: false,
                };
                let r = note_rect(&model, g, &n);
                assert_eq!(cell_at(&model, g, r.center()), Some((step, pitch)), "{n:?}");
                assert_eq!(r.height(), model.row_height - 2.0 * NOTE_INSET_Y);
            }
        }
        // Longer notes: the cell under the first column's centre is the start.
        let long = RollNote {
            start: 3,
            length: 5,
            pitch: 50,
            velocity: 1.0,
            selected: false,
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
        let (model, g) = grid();
        let notes = [RollNote {
            start: 14,
            length: 2,
            pitch: 40,
            velocity: 0.8,
            selected: false,
        }];
        // Draw from step 5 to the right edge: length clipped at the grid end.
        let draw_pos = note_rect(
            &model,
            g,
            &RollNote {
                start: 5,
                length: 1,
                pitch: 45,
                velocity: 0.8,
                selected: false,
            },
        )
        .center();
        let mut d = Drag::begin(&model, g, &notes, (5, 45), draw_pos);
        assert_eq!(d.gesture, Gesture::Draw);
        d.current = (15, 45);
        let p = d.preview(&model);
        assert_eq!((p.start, p.length, p.pitch), (5, 11, 45));
        assert_eq!(p.velocity, DEFAULT_VELOCITY);
        // Dragging left of the press keeps length 1.
        d.current = (2, 45);
        assert_eq!(d.preview(&model).length, 1);

        // Move the note off the right edge and below pitch 0: both clamp.
        let body = note_rect(&model, g, &notes[0]).left_center() + vec2(4.0, 0.0);
        let mut m = Drag::begin(&model, g, &notes, (14, 40), body);
        assert_eq!((m.gesture, m.note), (Gesture::Move, Some(0)));
        m.current = (15, 36);
        let p = m.preview(&model);
        assert_eq!((p.start, p.length, p.pitch), (14, 2, 36));

        // Resize from the right edge zone: to step 15 inclusive.
        let edge = note_rect(&model, g, &notes[0]).right_center() - vec2(2.0, 0.0);
        let mut r = Drag::begin(&model, g, &notes, (15, 40), edge);
        assert_eq!(r.gesture, Gesture::Resize);
        r.current = (0, 40);
        assert_eq!(r.preview(&model).length, 1);
        r.current = (15, 40);
        assert_eq!(r.preview(&model).length, 2);
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
