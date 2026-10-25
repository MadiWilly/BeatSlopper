//! SEQUENCER panel (M3/M5): the 16-step grid of one bar of the edit pattern, the pattern tabs
//! and the pattern controls.
//!
//! Layout follows `design/preview-a.html` `<section class="panel seq">` with the M5 pattern
//! controls built from the kit (the mock has no pattern UI):
//!
//! - **Head row:** title, the pattern tabs A..H (`neu::segmented`; a click sends
//!   `SelectPattern`, so while playing the engine switches at the next bar and the queued
//!   letter blinks beside the tabs until it does), then flush right the LENGTH `segmented`
//!   group (QOL: `/2`, the length read-out, `x2`), the FOLLOW toggle pill, COPY, PASTE and
//!   the REC lamp.
//! - **Beat numbers** over the first cell of each group of four (numbered through the
//!   pattern: bar 2 shows 5..8).
//! - One row per [`DrumRow::ALL`] (caps name, small VOL and TUNE knobs, 16 `neu::step_cell`s
//!   in groups of four, a CLEAR pill) and the BASS lane (note names in the cells, no knobs).
//! - **Bottom row:** the BAR selector (`neu::segmented` 1..bars, shown for patterns longer
//!   than one bar) under the name column, the playhead dots under the grid and the pattern's
//!   CLEAR pill (accessible name "Clear pattern") under the row CLEAR pills.
//!
//! The grid shows bar [`AppState::shown_bar`] of the edit pattern: cell `i` edits the
//! absolute step `shown_bar * 16 + i` and the playhead column lights only when the playing
//! pattern, bar and step match. Cell `i` sits at [`cell_x`]`(i)` from the grid's left edge, so
//! the beat numbers and the playhead dots use the same maths as the rows. Every cell reports
//! an accessible name ([`cell_name`], e.g. "Kick step 17" on bar 2) so tests can click it.
//!
//! Interactions: click toggles a drum step, shift-click turns it on and flips the accent;
//! a bass cell click selects the step for note entry (click again to deselect), shift-click
//! flips its accent (or gives an empty step a note). Right-click clears a cell: a drum step
//! goes off (accent too), a bass step becomes a rest and loses its selection. The state
//! methods on `AppState` do the work and send the engine messages.
//!
//! QOL pass (`crate::qol`, `crate::drag_paint`): cells act on the *press*, so pressing a drum
//! cell and dragging across its row paints every cell to the first cell's new state (one
//! undo step per stroke; pressing an "on" cell starts an erase stroke); the bass lane keeps
//! its click-to-select entry;
//! Ctrl-click flips the accent like shift-click. A double-click on a row label mutes the
//! row's mixer channel (shift + double-click: solo); muted labels dim, soloed ones carry an
//! accent dot. The head gains `x2` / `/2` pills beside LENGTH ([`DOUBLE_LABEL`],
//! [`HALVE_LABEL`]), the beat-number row shows VOL / TUNE over the row knobs, and the hat rows
//! read `C.Hat` / `O.Hat` like the mixer strips ([`row_label`]).
//!
//! Piano roll (M7): the head's `GRID | ROLL` tab ([`LANE_VIEW_TABS`], [`AppState::lane_view`])
//! swaps the grid area for `neu::piano_roll` over the edit pattern's whole note list (every
//! bar, every bar line). The roll takes the space of the six rows and the bottom row
//! ([`ROLL_AREA_HEIGHT`], less the [`ROLL_HANDLE_HEIGHT`] strip under it) under a beat-number
//! row aligned to its columns, so the panel height is the same in both views until the
//! handle is dragged: each [`ROLL_ROW_HEIGHT`] of drag adds a row
//! ([`AppState::roll_extra_rows`], persisted) and the panel grows with it. Its geometry comes
//! from [`roll_model`] (columns fitted to the width left of the CHORDS column, rows to the
//! height, [`AppState::roll_low_pitch`] at the bottom, the held keys lit) and every edit the
//! widget reports is applied by [`apply_roll_output`] through the undoable note methods on
//! `AppState`. The CHORDS column ([`CHORD_COLUMN_WIDTH`], right of the roll) lists
//! [`CHORDS`] as toggle pills: with one armed, a draw on the roll places the whole chord on
//! the drawn root ([`AppState::add_chord`], one undo step).
//!
//! Instrument tracks (M10): the bass lane and the roll edit the *edit track's* notes
//! ([`AppState::edit_notes`]); the head shows the edit track's colour chip and name after
//! the `GRID | ROLL` tab ([`controls::track_tag`]), and the roll draws every other active
//! track's notes of the pattern as ghosts in their track colours ([`roll_ghosts`],
//! `neu::piano_roll_with_ghosts`): reference only, never editable.

use egui::epaint::text::LayoutJob;
use egui::{
    Align, Align2, CursorIcon, Layout, Modifiers, Response, Role, ScrollArea, Sense, Ui, Vec2,
    WidgetInfo, vec2,
};
use engine::pattern::pattern_letter;
use engine::{DrumRow, Note, PATTERN_COUNT, PatternLength, STEPS};
use neu::{
    GhostNote, KEY_COLUMN_WIDTH, KnobSize, KnobSpec, PianoRollModel, PianoRollOutput, RollNote,
    STEP_CELL, STEP_GAP, STEP_GROUP_GAP, StepCellSpec, TextKind, Theme,
};

use crate::app::{AppState, EditKey, LaneView, queued_blink_visible};
use crate::chords::CHORDS;
use crate::drag_paint::PaintRow;
use crate::ui::controls;

/// Width of the row-name column (`.seq-row .name`).
pub const NAME_WIDTH: f32 = 64.0;
/// Gap between the name column, the knobs, the grid and the clear pill (`.seq-row{gap}`).
pub const COLUMN_GAP: f32 = 14.0;
/// Gap between the VOL and TUNE knobs.
pub const KNOB_GAP: f32 = 8.0;
/// Width of a small knob block (`KnobSize::Sm.block_width()`, checked by a test).
pub const SM_KNOB: f32 = 34.0;
/// Width of the per-row control column: two small knobs and their gap.
pub const CONTROLS_WIDTH: f32 = 2.0 * SM_KNOB + KNOB_GAP;
/// Height of a grid row (a small knob is 34 tall, a cell 32).
pub const ROW_HEIGHT: f32 = 34.0;
/// Vertical gap between rows (`.seq-grid{row-gap}`, tightened so the grid fits the window).
pub const ROW_GAP: f32 = 2.0;
/// Gap between the panel head and the beat numbers.
pub const HEAD_GAP: f32 = 6.0;
/// Width of the 16-cell grid: 16 cells, 15 gaps, 3 of them widened by the group gap.
pub const GRID_WIDTH: f32 = 16.0 * STEP_CELL + 15.0 * STEP_GAP + 3.0 * STEP_GROUP_GAP;
/// Left edge of the grid relative to the panel content.
pub const GRID_LEFT: f32 = NAME_WIDTH + COLUMN_GAP + CONTROLS_WIDTH + COLUMN_GAP;
/// Caps name of the bass lane.
pub const BASS_NAME: &str = "Bass";
/// Height of the bottom row: the BAR selector, the playhead dots and the pattern CLEAR pill
/// (a `segmented` / pill is 30 tall).
pub const BOTTOM_ROW: f32 = 30.0;
/// Gap between items of the head row.
const HEAD_ITEM_GAP: f32 = 12.0;
/// Accessible name of the pattern CLEAR pill (the row pills are "Clear").
pub const CLEAR_PATTERN_LABEL: &str = "Clear pattern";
/// Caps label beside the bar selector.
pub const BAR_LABEL: &str = "Bar";
/// Pattern tab names, A..H.
pub const PATTERN_TABS: [&str; PATTERN_COUNT] = ["A", "B", "C", "D", "E", "F", "G", "H"];
/// Items of the `GRID | ROLL` view tab, in [`LaneView`] order.
pub const LANE_VIEW_TABS: [&str; 2] = ["Grid", "Roll"];
/// Label of the pill that doubles the pattern length (QOL).
pub const DOUBLE_LABEL: &str = "x2";
/// Label of the pill that halves the pattern length (QOL).
pub const HALVE_LABEL: &str = "/2";
/// Legends over the row knobs (QOL).
pub const KNOB_LEGENDS: [&str; 2] = ["Vol", "Tune"];
/// Narrowest length read-out between the `/2` and `x2` pills (QOL).
pub const LENGTH_TEXT_MIN_WIDTH: f32 = 44.0;
/// Diameter of the solo dot beside a row label (QOL).
pub const SOLO_DOT: f32 = 6.0;
/// Alpha of a muted row's label (QOL): dim, but still readable.
pub const MUTED_ALPHA: f32 = 0.45;
/// Height of the area the piano roll replaces: the six grid rows (each with its gap) and the
/// bottom row. The row gap after the beat numbers stays in both views.
pub const ROLL_AREA_HEIGHT: f32 = (DrumRow::COUNT + 1) as f32 * (ROW_HEIGHT + ROW_GAP) + BOTTOM_ROW;
/// Row height of the piano roll (one pitch).
pub const ROLL_ROW_HEIGHT: f32 = 14.0;
/// Fewest pitch rows the roll shows, whatever the height (an octave).
pub const MIN_ROLL_PITCHES: usize = 12;
/// Narrowest and widest step column of the roll (64 steps in a narrow panel / 16 in a wide one).
pub const MIN_ROLL_STEP_WIDTH: f32 = 12.0;
pub const MAX_ROLL_STEP_WIDTH: f32 = 48.0;
/// Height of the resize handle strip under the roll (inside [`ROLL_AREA_HEIGHT`], leaving
/// the default roll exactly 17 rows).
pub const ROLL_HANDLE_HEIGHT: f32 = 8.0;
/// Size of the grip painted in the middle of the handle strip.
pub const ROLL_GRIP: Vec2 = vec2(36.0, 4.0);
/// Accessible name of the handle under the roll.
pub const ROLL_HANDLE_LABEL: &str = "Resize roll";
/// Width of the CHORDS column to the right of the roll.
pub const CHORD_COLUMN_WIDTH: f32 = 96.0;
/// Size of one chord pill (the column minus room for the scroll bar).
pub const CHORD_PILL: Vec2 = vec2(84.0, 26.0);
/// Vertical gap between chord pills.
pub const CHORD_PILL_GAP: f32 = 6.0;
/// Caps title over the chord pills.
pub const CHORDS_TITLE: &str = "Chords";
/// Label of the head pill that shows / hides the roll's black-key rows.
pub const BLACK_KEYS_LABEL: &str = "Black keys";
/// Width of the handle strip right of the roll (drag it to make the roll wider).
pub const ROLL_WIDTH_HANDLE: f32 = 8.0;
/// Accessible name of the handle right of the roll.
pub const ROLL_WIDTH_HANDLE_LABEL: &str = "Resize roll width";
/// Widest step column the roll zooms to when dragged wider than its fitted width.
pub const MAX_ROLL_STEP_WIDTH_ZOOMED: f32 = 192.0;
/// Label of the head pill that moves the roll into its own window (lit while it is there).
pub const POP_OUT_LABEL: &str = "Pop out";
/// Label of the pill in the roll window that brings the roll back into the panel.
pub const DOCK_LABEL: &str = "Dock";
/// Title of the roll's own window.
pub const ROLL_WINDOW_TITLE: &str = "BeatSlopper piano roll";
/// What the panel shows in place of the roll while it is in its own window.
pub const POPPED_LABEL: &str = "The piano roll is open in its own window";
/// Size the roll window opens at.
pub const ROLL_WINDOW_SIZE: Vec2 = vec2(1100.0, 620.0);

/// The roll's fitted width: every step at [`MAX_ROLL_STEP_WIDTH`], no wider than
/// `available` (and never narrower than the key column).
#[must_use]
pub fn fitted_roll_width(state: &AppState, available: f32) -> f32 {
    let steps = state.pattern().len().max(1) as f32;
    (KEY_COLUMN_WIDTH + steps * MAX_ROLL_STEP_WIDTH)
        .min(available)
        .max(KEY_COLUMN_WIDTH)
}

/// The roll's width in the panel: the fitted width plus what the handle was dragged to
/// ([`AppState::roll_extra_width`]), no wider than `available`.
#[must_use]
pub fn roll_width(state: &AppState, available: f32) -> f32 {
    (fitted_roll_width(state, available) + state.roll_extra_width)
        .min(available)
        .max(KEY_COLUMN_WIDTH)
}

/// Height of the roll itself for `state`: the roll area minus the handle strip, plus the rows
/// the handle was dragged to ([`AppState::roll_extra_rows`]).
#[must_use]
pub fn roll_height(state: &AppState) -> f32 {
    ROLL_AREA_HEIGHT - ROLL_HANDLE_HEIGHT + roll_extra_height(state)
}

/// How much taller than the grid view the roll view makes the panel.
#[must_use]
pub fn roll_extra_height(state: &AppState) -> f32 {
    state.roll_extra_rows as f32 * ROLL_ROW_HEIGHT
}

/// The roll's view model for a roll of `size` (the space it may take: `ROLL_AREA_HEIGHT` tall
/// and the panel's width): one column per step of the edit pattern, the column width fitted
/// to the width (whole points, `MIN_ROLL_STEP_WIDTH..=MAX_ROLL_STEP_WIDTH`), as many
/// [`ROLL_ROW_HEIGHT`] rows as fit the height (at least [`MIN_ROLL_PITCHES`]),
/// [`AppState::roll_low_pitch`] at the bottom (clamped so the rows stay within MIDI), the
/// playhead while the edit pattern plays, the roll selection and the held keys
/// ([`AppState::held_notes`], lit in the key column). Feeding the model's own `size()` back
/// in reproduces it, so tests can rebuild it from the rendered rect.
#[must_use]
pub fn roll_model(state: &AppState, size: Vec2) -> PianoRollModel {
    let steps = state.pattern().len().max(1);
    let step_width = ((size.x - KEY_COLUMN_WIDTH) / steps as f32)
        .floor()
        .clamp(MIN_ROLL_STEP_WIDTH, MAX_ROLL_STEP_WIDTH_ZOOMED);
    let pitches = ((size.y / ROLL_ROW_HEIGHT).floor().max(0.0) as usize).max(MIN_ROLL_PITCHES);
    let mut model = PianoRollModel {
        steps,
        low_pitch: state.roll_low_pitch,
        pitches,
        step_width,
        row_height: ROLL_ROW_HEIGHT,
        playhead_step: state.roll_playhead(),
        selected: state.roll_selected,
        selection: roll_selection(state),
        held: state.held_notes,
        black_keys: state.roll_black_keys,
    };
    model.normalise(state.edit_notes().len());
    model
}

/// The other active tracks' notes of the edit pattern as ghosts (M10), each in its track's
/// colour; every track but the edit track, in slot order. Built per frame.
#[must_use]
pub fn roll_ghosts(state: &AppState, theme: &Theme) -> Vec<GhostNote> {
    state
        .active_tracks()
        .filter(|&slot| slot != state.edit_track)
        .flat_map(|slot| {
            let colour = theme.track_colour(state.track(slot).colour);
            state.track_notes(slot).iter().map(move |n| GhostNote {
                start: n.start,
                length: n.length,
                pitch: n.pitch,
                colour,
            })
        })
        .collect()
}

/// The roll's selection: [`AppState::roll_selection`] plus the primary
/// [`AppState::roll_selected`] when it is missing from it (older paths set the primary
/// alone).
#[must_use]
pub fn roll_selection(state: &AppState) -> Vec<usize> {
    let mut selection = state.roll_selection.clone();
    if let Some(primary) = state.roll_selected
        && !selection.contains(&primary)
    {
        selection.push(primary);
    }
    selection
}

/// The roll's view of a pattern note (`selected` when it is in the roll selection).
#[must_use]
pub fn roll_notes(state: &AppState) -> Vec<RollNote> {
    state
        .edit_notes()
        .iter()
        .enumerate()
        .map(|(i, n)| RollNote {
            start: n.start,
            length: n.length,
            pitch: n.pitch,
            velocity: n.velocity,
            selected: state.roll_selected == Some(i) || state.roll_selection.contains(&i),
        })
        .collect()
}

/// A roll note as a pattern note.
#[must_use]
pub fn to_note(n: RollNote) -> Note {
    Note::new(n.start, n.length, n.pitch, n.velocity)
}

/// Applies what the roll reported this frame: the scrolled bottom pitch and the selection
/// (view state the widget edits in `model`), then the one edit, if any, through
/// [`AppState::add_note`] / [`AppState::add_chord`] / [`AppState::set_note`] /
/// [`AppState::move_notes`] / [`AppState::remove_note`] / [`AppState::remove_notes`]. The
/// selection follows its notes across the re-sort an add or move causes: every selected
/// note is remembered by value (as the edit leaves it), then found again in the new list.
pub fn apply_roll_output(state: &mut AppState, model: &PianoRollModel, out: PianoRollOutput) {
    state.set_roll_low_pitch(model.low_pitch);
    let before = state.edit_notes().to_vec();
    let mut wanted: Vec<Note> = model
        .selection
        .iter()
        .filter_map(|&i| before.get(i).copied())
        .collect();
    let mut primary: Option<Note> = model.selected.and_then(|i| before.get(i).copied());
    // What a moved note becomes, carried into the remembered selection.
    let mut follow = |old: Option<&Note>, new: Note, wanted: &mut Vec<Note>| {
        if let Some(old) = old {
            for w in wanted.iter_mut().filter(|w| *w == old) {
                *w = new;
            }
            if primary.as_ref() == Some(old) {
                primary = Some(new);
            }
        }
    };

    if let Some(n) = out.add {
        // With a chord armed the drawn note is the root: every chord tone lands at once.
        match state.armed_chord.and_then(|i| CHORDS.get(i)) {
            Some(chord) => {
                state.add_chord(chord, n.pitch, n.start, n.length, n.velocity);
            }
            None => {
                state.add_note(to_note(n));
            }
        }
    } else if !out.moved_many.is_empty() {
        let edits: Vec<(usize, Note)> = out
            .moved_many
            .iter()
            .map(|&(i, n)| (i, to_note(n)))
            .collect();
        state.move_notes(&edits);
        for (i, n) in &edits {
            let mut n = *n;
            n.normalise();
            follow(before.get(*i), n, &mut wanted);
        }
    } else if let Some((i, n)) = out.moved {
        let mut note = to_note(n);
        note.normalise();
        state.set_note(i, note);
        follow(before.get(i), note, &mut wanted);
    } else if !out.removed_many.is_empty() {
        state.remove_notes(&out.removed_many);
        for i in &out.removed_many {
            if let Some(gone) = before.get(*i) {
                wanted.retain(|w| w != gone);
                if primary.as_ref() == Some(gone) {
                    primary = None;
                }
            }
        }
    } else if let Some(i) = out.removed {
        state.remove_note(i);
        if let Some(gone) = before.get(i) {
            wanted.retain(|w| w != gone);
            if primary.as_ref() == Some(gone) {
                primary = None;
            }
        }
    }

    let after = state.edit_notes();
    let mut selection: Vec<usize> = wanted
        .iter()
        .filter_map(|w| after.iter().position(|n| n == w))
        .collect();
    selection.dedup();
    let primary = primary
        .and_then(|p| after.iter().position(|n| *n == p))
        .or_else(|| selection.last().copied());
    state.roll_selection = selection;
    state.roll_selected = primary;
}

/// Left edge of cell `i` relative to the grid's left edge.
#[must_use]
pub fn cell_x(i: usize) -> f32 {
    i as f32 * (STEP_CELL + STEP_GAP) + (i / 4) as f32 * STEP_GROUP_GAP
}

/// Accessible name of a step cell: `"Kick step 1"` (`step` is the absolute, 0-based index).
#[must_use]
pub fn cell_name(row: &str, step: usize) -> String {
    format!("{row} step {}", step + 1)
}

/// Accessible name of the queued-pattern indicator: `"Queued B"`.
#[must_use]
pub fn queued_name(pattern: u8) -> String {
    format!("Queued {}", pattern_letter(pattern))
}

/// Caps label of a drum row in the grid, spelled like the mixer strips (`C.Hat`, `O.Hat`).
#[must_use]
pub fn row_label(row: DrumRow) -> &'static str {
    match row {
        DrumRow::Kick => "Kick",
        DrumRow::Snare => "Snare",
        DrumRow::Clap => "Clap",
        DrumRow::ClosedHat => "C.Hat",
        DrumRow::OpenHat => "O.Hat",
    }
}

/// Full row name for accessibility and tooltips (`DrumRow::label` is the short grid label).
#[must_use]
pub fn row_name(row: DrumRow) -> &'static str {
    match row {
        DrumRow::Kick => "Kick",
        DrumRow::Snare => "Snare",
        DrumRow::Clap => "Clap",
        DrumRow::ClosedHat => "Closed hat",
        DrumRow::OpenHat => "Open hat",
    }
}

/// `-6.0` -> `"-6 dB"`, `0.0` -> `"0 dB"`, `3.0` -> `"+3 dB"`.
pub fn fmt_row_db(v: f32) -> String {
    let db = v.round() as i32;
    if db == 0 {
        "0 dB".to_owned()
    } else {
        format!("{db:+} dB")
    }
}

/// `0.0` -> `"+0 st"`, `-3.0` -> `"-3 st"`.
pub fn fmt_semitones(v: f32) -> String {
    format!("{:+} st", v.round() as i32)
}

/// The LENGTH selector's items, in [`PatternLength::ALL`] order.
#[must_use]
pub fn length_items() -> [&'static str; 3] {
    [
        PatternLength::Bar1.label(),
        PatternLength::Bar2.label(),
        PatternLength::Bar4.label(),
    ]
}

/// Height of the beat-number row (one caps label line).
fn label_row_height(ui: &Ui) -> f32 {
    ui.ctx()
        .fonts_mut(|f| f.row_height(&TextKind::Label.font_id()))
}

/// Height of the whole panel (padding included), from the layout constants, so the page can
/// reserve the exact space without measuring a frame first. In ROLL view the rows the roll
/// was dragged taller by ([`roll_extra_height`]) are added.
#[must_use]
pub fn panel_height(ui: &Ui, theme: &Theme, state: &AppState) -> f32 {
    let rows = (DrumRow::COUNT + 1) as f32;
    let extra = match state.lane_view {
        LaneView::Grid => 0.0,
        LaneView::Roll => roll_extra_height(state),
    };
    2.0 * theme.panel_padding
        + controls::ROW_HEIGHT
        + HEAD_GAP
        + label_row_height(ui)
        + ROW_GAP
        + rows * (ROW_HEIGHT + ROW_GAP)
        + BOTTOM_ROW
        + extra
}

/// Renders the SEQUENCER panel into `ui` and applies any edit to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    // A paint stroke lives as long as a button is down.
    if !ui.input(|i| i.pointer.any_down()) {
        state.end_paint();
    }
    neu::panel(ui, theme, |ui| {
        // Every row is one allocation; the gaps below are explicit, so no implicit spacing.
        ui.spacing_mut().item_spacing.y = 0.0;
        head(ui, state, theme);
        ui.add_space(HEAD_GAP);
        match state.lane_view {
            LaneView::Grid => {
                beat_numbers(ui, state, theme);
                ui.add_space(ROW_GAP);
                for row in DrumRow::ALL {
                    drum_row(ui, state, theme, row);
                    ui.add_space(ROW_GAP);
                }
                bass_row(ui, state, theme);
                ui.add_space(ROW_GAP);
                bottom_row(ui, state, theme);
            }
            LaneView::Roll if state.roll_popped => {
                popped_placeholder(ui, state, theme);
                roll_window(ui.ctx(), state, theme);
            }
            LaneView::Roll => {
                let available =
                    (ui.available_width() - ROLL_WIDTH_HANDLE - COLUMN_GAP - CHORD_COLUMN_WIDTH)
                        .max(KEY_COLUMN_WIDTH);
                let max_extra = (available - fitted_roll_width(state, available)).max(0.0);
                let width = roll_width(state, available);
                let model = roll_model(state, vec2(width, roll_height(state)));
                roll_beat_numbers(ui, theme, &model);
                ui.add_space(ROW_GAP);
                roll_area(ui, state, theme, model, max_extra);
            }
        }
    });
}

/// The beat numbers over the roll: one per beat of the whole pattern (1..=4 x bars), centred
/// on the first column of each group of four.
fn roll_beat_numbers(ui: &mut Ui, theme: &Theme, model: &PianoRollModel) {
    let height = label_row_height(ui);
    let beats = model.steps / 4;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(KEY_COLUMN_WIDTH);
        for beat in 0..beats {
            if beat > 0 {
                ui.add_space(3.0 * model.step_width);
            }
            let (rect, response) =
                ui.allocate_exact_size(vec2(model.step_width, height), Sense::hover());
            let text = (beat + 1).to_string();
            if ui.is_rect_visible(rect) {
                neu::text::paint_text(
                    ui.painter(),
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &text,
                    TextKind::Label,
                    theme.text_secondary,
                );
            }
            response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &text));
        }
    });
}

/// The piano roll over the edit pattern's notes with the resize handle under it, and the
/// CHORDS column to its right, in the space of the grid rows and the bottom row (plus the
/// extra rows); whatever the roll reports is applied by [`apply_roll_output`].
fn roll_area(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    mut model: PianoRollModel,
    max_extra_width: f32,
) {
    let height = roll_height(state);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let notes = roll_notes(state);
            let ghosts = roll_ghosts(state, theme);
            let out = neu::piano_roll_with_ghosts(ui, theme, &mut model, &notes, &ghosts);
            apply_roll_output(state, &model, out);
            // Whole rows only: pad the rest so the handle sits at the same place.
            let rest = height - model.size().y;
            if rest > 0.0 {
                ui.add_space(rest);
            }
            roll_handle(ui, state, theme, model.size().x);
        });
        roll_width_handle(ui, state, theme, height, max_extra_width);
        ui.add_space(COLUMN_GAP);
        chord_column(ui, state, theme, height + ROLL_HANDLE_HEIGHT, true);
    });
}

/// The strip right of the roll: dragging it right makes the roll wider (the step columns
/// zoom, [`AppState::set_roll_extra_width`], at most `max_extra` past the fitted width so
/// the CHORDS column stays in the panel), dragging it left narrower, down to the fitted
/// width. Painted as a vertical [`ROLL_GRIP`], accent while hovered or dragged.
fn roll_width_handle(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    height: f32,
    max_extra: f32,
) {
    let (rect, response) = ui.allocate_exact_size(vec2(ROLL_WIDTH_HANDLE, height), Sense::drag());
    let origin_id = response.id.with("origin");
    if response.dragged()
        && let Some(p) = response.interact_pointer_pos()
    {
        let origin = ui
            .data(|d| d.get_temp::<(f32, f32)>(origin_id))
            .filter(|_| !response.drag_started())
            .unwrap_or((p.x, state.roll_extra_width));
        ui.data_mut(|d| d.insert_temp(origin_id, origin));
        let width = (origin.1 + (p.x - origin.0)).clamp(0.0, max_extra);
        if width != state.roll_extra_width {
            state.set_roll_extra_width(width);
        }
    }
    if ui.is_rect_visible(rect) {
        let lit = response.hovered() || response.dragged();
        let grip = egui::Rect::from_center_size(rect.center(), vec2(ROLL_GRIP.y, ROLL_GRIP.x));
        let fill = if lit { theme.accent } else { theme.shadow_dark };
        ui.painter().rect_filled(grip, ROLL_GRIP.y * 0.5, fill);
    }
    response
        .on_hover_cursor(CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to make the roll wider or narrower")
        .widget_info(|| {
            WidgetInfo::labeled(Role::Slider, ui.is_enabled(), ROLL_WIDTH_HANDLE_LABEL)
        });
}

/// What the panel shows while the roll is in its own window: [`POPPED_LABEL`] centred in
/// the space the roll view takes (beat numbers, roll, handle), so the panel keeps its height.
fn popped_placeholder(ui: &mut Ui, state: &AppState, theme: &Theme) {
    let height = label_row_height(ui) + ROW_GAP + roll_height(state) + ROLL_HANDLE_HEIGHT;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::centered_and_justified(egui::Direction::TopDown),
        |ui| {
            neu::label(ui, theme, POPPED_LABEL)
                .on_hover_text("Click POP OUT again, or DOCK in the window, to bring it back");
        },
    );
}

/// The roll in its own window ([`ROLL_WINDOW_TITLE`]) while [`AppState::roll_popped`]: an
/// immediate viewport (a real OS window under eframe, which can be maximised or put full
/// screen; an embedded `egui::Window` where the backend has no multi-window support, e.g.
/// headless tests). Closing the window docks the roll again.
fn roll_window(ctx: &egui::Context, state: &mut AppState, theme: &Theme) {
    let id = egui::ViewportId::from_hash_of("beatslopper_piano_roll_window");
    let builder = egui::ViewportBuilder::default()
        .with_title(ROLL_WINDOW_TITLE)
        .with_inner_size(ROLL_WINDOW_SIZE)
        .with_min_inner_size(vec2(600.0, 300.0));
    ctx.show_viewport_immediate(id, builder, |ctx, _class| {
        // This window's keyboard: shortcuts (Space, R, Ctrl+Z, ...) and the piano keys work
        // here as in the main window.
        state.handle_window_input(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            state.set_roll_popped(false);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme.surface)
                    .inner_margin(theme.panel_padding),
            )
            .show(ctx, |ui| roll_window_contents(ui, state, theme));
    });
}

/// Inside the roll window: a title row with the DOCK pill, then the beat numbers, the roll
/// fitted to the window (columns to its width, rows to its height) and the CHORDS column.
fn roll_window_contents(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.horizontal(|ui| {
        ui.set_min_height(controls::ROW_HEIGHT);
        ui.spacing_mut().item_spacing.x = HEAD_ITEM_GAP;
        neu::title(ui, theme, "Roll");
        controls::right_aligned(ui, "roll_window_head", |ui| {
            if neu::button(ui, theme, DOCK_LABEL)
                .on_hover_text("Put the roll back in the SEQUENCER panel")
                .clicked()
            {
                state.set_roll_popped(false);
            }
        });
    });
    ui.add_space(HEAD_GAP);
    let width = (ui.available_width() - COLUMN_GAP - CHORD_COLUMN_WIDTH).max(KEY_COLUMN_WIDTH);
    let height = (ui.available_height() - label_row_height(ui) - ROW_GAP)
        .max(ROLL_ROW_HEIGHT * MIN_ROLL_PITCHES as f32);
    let mut model = roll_model(state, vec2(width, height));
    roll_beat_numbers(ui, theme, &model);
    ui.add_space(ROW_GAP);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let notes = roll_notes(state);
        let ghosts = roll_ghosts(state, theme);
        let out = neu::piano_roll_with_ghosts(ui, theme, &mut model, &notes, &ghosts);
        apply_roll_output(state, &model, out);
        ui.add_space(COLUMN_GAP);
        chord_column(ui, state, theme, model.size().y, false);
    });
}

/// The strip under the roll: dragging it down adds rows to the roll, dragging it up takes
/// them away ([`AppState::set_roll_extra_rows`], one row per [`ROLL_ROW_HEIGHT`] points from
/// where the drag started). Painted as a [`ROLL_GRIP`] pill, accent while hovered or dragged.
fn roll_handle(ui: &mut Ui, state: &mut AppState, theme: &Theme, width: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROLL_HANDLE_HEIGHT), Sense::drag());
    let origin_id = response.id.with("origin");
    if response.dragged()
        && let Some(p) = response.interact_pointer_pos()
    {
        let origin = ui
            .data(|d| d.get_temp::<(f32, usize)>(origin_id))
            .filter(|_| !response.drag_started())
            .unwrap_or((p.y, state.roll_extra_rows));
        ui.data_mut(|d| d.insert_temp(origin_id, origin));
        let delta = ((p.y - origin.0) / ROLL_ROW_HEIGHT).round() as i64;
        let rows = (origin.1 as i64 + delta).max(0) as usize;
        if rows != state.roll_extra_rows {
            state.set_roll_extra_rows(rows);
        }
    }
    if ui.is_rect_visible(rect) {
        let lit = response.hovered() || response.dragged();
        let grip = egui::Rect::from_center_size(rect.center(), ROLL_GRIP);
        let fill = if lit { theme.accent } else { theme.shadow_dark };
        ui.painter().rect_filled(grip, ROLL_GRIP.y * 0.5, fill);
    }
    response
        .on_hover_cursor(CursorIcon::ResizeVertical)
        .on_hover_text("Drag to make the roll taller or shorter")
        .widget_info(|| WidgetInfo::labeled(Role::Slider, ui.is_enabled(), ROLL_HANDLE_LABEL));
}

/// The CHORDS column: the roll's view pills (BLACK KEYS, and POP OUT when `docked`), a caps
/// title and one toggle pill per [`CHORDS`] entry (most common at the top) in a scroll area
/// of the rest of the roll's height. The lit chord pill is the armed chord
/// ([`AppState::armed_chord`]); clicking it again disarms. With a chord armed, drawing on
/// the roll places the whole chord with the drawn note as its root.
fn chord_column(ui: &mut Ui, state: &mut AppState, theme: &Theme, height: f32, docked: bool) {
    ui.allocate_ui_with_layout(
        vec2(CHORD_COLUMN_WIDTH, height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = CHORD_PILL_GAP;
            let top = ui.cursor().min.y;
            if neu::toggle_button_sized(
                ui,
                theme,
                BLACK_KEYS_LABEL,
                state.roll_black_keys,
                CHORD_PILL,
            )
            .on_hover_text("Show the black-key rows on the roll (off: white keys only)")
            .clicked()
            {
                state.toggle_black_keys();
            }
            if docked
                && neu::toggle_button_sized(ui, theme, POP_OUT_LABEL, state.roll_popped, CHORD_PILL)
                    .on_hover_text("Open the roll in its own window (it can go full screen)")
                    .clicked()
            {
                state.set_roll_popped(!state.roll_popped);
            }
            ui.add_space(CHORD_PILL_GAP);
            neu::label(ui, theme, CHORDS_TITLE)
                .on_hover_text("Pick a chord, then draw on the roll to place it");
            let used = ui.cursor().min.y - top;
            let list_height = (height - used).max(0.0);
            ScrollArea::vertical()
                .id_salt("chords")
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = CHORD_PILL_GAP;
                    let mut clicked = None;
                    for (i, chord) in CHORDS.iter().enumerate() {
                        let on = state.armed_chord == Some(i);
                        let response =
                            neu::toggle_button_sized(ui, theme, chord.name, on, CHORD_PILL);
                        if response.clicked() {
                            clicked = Some(i);
                        }
                    }
                    if clicked.is_some() {
                        state.arm_chord(clicked);
                    }
                });
        },
    );
}

/// Title, pattern tabs and the queued letter on the left; LENGTH, FOLLOW, COPY, PASTE and the
/// REC lamp on the right.
fn head(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.set_min_height(controls::ROW_HEIGHT);
        ui.spacing_mut().item_spacing.x = HEAD_ITEM_GAP;
        neu::title(ui, theme, "Sequencer");
        pattern_tabs(ui, state, theme);
        queued_indicator(ui, state, theme);
        lane_view_tabs(ui, state, theme);
        // M10: the lane and the roll edit this track.
        head_track_tag(ui, state, theme);
        if state.lane_view == LaneView::Roll && state.roll_popped {
            // The roll is in its own window: the pill that brings it back stays reachable.
            if neu::toggle_button(ui, theme, POP_OUT_LABEL, true)
                .on_hover_text("The roll is in its own window; click to bring it back")
                .clicked()
            {
                state.set_roll_popped(false);
            }
        }
        controls::right_aligned(ui, "sequencer_head", |ui| {
            ui.spacing_mut().item_spacing.x = HEAD_ITEM_GAP;
            length_group(ui, state, theme);
            if neu::toggle_button(ui, theme, "Follow", state.follow)
                .on_hover_text("Follow the playing bar")
                .clicked()
            {
                state.toggle_follow();
            }
            if neu::button(ui, theme, "Copy")
                .on_hover_text("Copy this pattern")
                .clicked()
            {
                state.copy_pattern();
            }
            let paste = ui
                .add_enabled_ui(state.clipboard.is_some(), |ui| {
                    neu::button(ui, theme, "Paste")
                        .on_hover_text("Paste the copied pattern here")
                        .clicked()
                })
                .inner;
            if paste {
                state.paste_pattern();
            }
            neu::lamp(ui, theme, state.is_recording(), "Rec");
        });
    });
}

/// Narrowest name worth showing beside the head's chip; below it the name is the tooltip.
const TAG_NAME_MIN_WIDTH: f32 = 24.0;

/// The edit track's tag in the head (M10): the colour chip (`controls::track_chip`,
/// accessible as `"<name> colour"`), then the name as a caps label only as far as the room
/// beside the right group allows, cut with an ellipsis; with the whole head at 1280 wide the
/// chip alone fits and the name is its tooltip (it is also in the SYNTH head and the TRACKS
/// panel). The right group's width is what [`controls::right_aligned`] measured last frame
/// under `"sequencer_head"`, so the head never grows past the panel and pushes the right-hand
/// column off the window.
fn head_track_tag(ui: &mut Ui, state: &AppState, theme: &Theme) {
    let right_width: f32 = ui
        .data(|d| d.get_temp(ui.scope_id().with("sequencer_head")))
        .unwrap_or(0.0);
    let gap = ui.spacing().item_spacing.x;
    let room = ui.available_width() - right_width - gap;
    let name_room = room - controls::TRACK_CHIP - controls::TRACK_CHIP_GAP;
    let track = state.edit_track_ui();
    let tooltip = format!(
        "Edits track {} (Alt+Up / Alt+Down select another track)",
        track.name
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = controls::TRACK_CHIP_GAP;
        controls::track_chip(ui, theme, track.colour, &track.name).on_hover_text(&tooltip);
        if name_room < TAG_NAME_MIN_WIDTH {
            return;
        }
        let mut job = LayoutJob::single_section(
            TextKind::Label.display_string(&track.name),
            TextKind::Label.format(theme.text_secondary),
        );
        job.wrap.max_width = name_room;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let galley = ui.painter().layout_job(job);
        let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().galley(rect.min, galley, theme.text_secondary);
        }
        response.on_hover_text(tooltip);
    });
}

/// The A..H tabs: the selected item is the edit pattern; a click selects (and, while playing
/// in pattern mode, queues) that pattern.
fn pattern_tabs(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let before = usize::from(state.current()) % PATTERN_COUNT;
    let mut selected = before;
    neu::segmented(ui, theme, &mut selected, &PATTERN_TABS)
        .on_hover_text("Pattern (switches at the next bar while playing)");
    if selected != before {
        state.select_pattern(selected as u8);
    }
}

/// `GRID | ROLL` (M7): what the panel shows under the head. Not an edit.
fn lane_view_tabs(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let before = match state.lane_view {
        LaneView::Grid => 0,
        LaneView::Roll => 1,
    };
    let mut selected = before;
    neu::segmented(ui, theme, &mut selected, &LANE_VIEW_TABS)
        .on_hover_text("Grid: one note per step. Roll: chords and held notes");
    if selected != before {
        state.set_lane_view(if selected == 0 {
            LaneView::Grid
        } else {
            LaneView::Roll
        });
    }
}

/// The letter of the pattern the engine queued for the next bar, blinking at 2 Hz. Always
/// present as an (invisible when blinked off) label so tests can find it; nothing while no
/// switch is pending.
fn queued_indicator(ui: &mut Ui, state: &AppState, theme: &Theme) {
    let Some(queued) = state.playhead.queued else {
        return;
    };
    let text = pattern_letter(queued).to_string();
    let galley = neu::text::layout(ui.painter(), &text, TextKind::Title, theme.accent);
    let (rect, response) = ui.allocate_exact_size(
        vec2(galley.size().x.max(12.0), controls::ROW_HEIGHT),
        Sense::hover(),
    );
    let visible = queued_blink_visible(ui.input(|i| i.time));
    if visible && ui.is_rect_visible(rect) {
        neu::text::paint_galley(
            ui.painter(),
            rect.center(),
            Align2::CENTER_CENTER,
            galley,
            TextKind::Title,
            theme.accent,
        );
    }
    let name = queued_name(queued);
    response
        .on_hover_text(format!("Pattern {text} starts at the next bar"))
        .widget_info(|| WidgetInfo::labeled(Role::Label, ui.is_enabled(), &name));
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));
}

/// LENGTH: the `/2` pill, the length read-out (`1 bar`, `2 bars`, `4 bars`: the labels of
/// [`length_items`]) and the `x2` pill. `x2` doubles the edit pattern (copying the loop into
/// the empty new half), `/2` halves it (hidden steps are kept); they grey out at 64 and 16
/// steps. The M5 three-item segmented did not fit the head beside the pills at 1280 wide, so
/// the pills *are* the length control (two clicks of `x2` reach 4 bars).
fn length_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let len = state.pattern().len();
    ui.spacing_mut().item_spacing.x = KNOB_GAP;
    let halve = ui
        .add_enabled_ui(len > STEPS, |ui| {
            neu::button(ui, theme, HALVE_LABEL)
                .on_hover_text("Halve the pattern length (the hidden steps are kept)")
                .clicked()
        })
        .inner;
    if halve {
        state.halve_pattern_length();
    }
    let text = state.pattern().pattern_length().label();
    let galley = neu::text::layout(ui.painter(), text, TextKind::Value, theme.text_primary);
    let (rect, response) = ui.allocate_exact_size(
        vec2(
            galley.size().x.max(LENGTH_TEXT_MIN_WIDTH),
            controls::ROW_HEIGHT,
        ),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        neu::text::paint_galley(
            ui.painter(),
            rect.center(),
            Align2::CENTER_CENTER,
            galley,
            TextKind::Value,
            theme.text_primary,
        );
    }
    response
        .on_hover_text("Pattern length: /2 and x2 change it")
        .widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    let double = ui
        .add_enabled_ui(len < engine::MAX_STEPS, |ui| {
            neu::button(ui, theme, DOUBLE_LABEL)
                .on_hover_text("Double the pattern length (the loop is copied into the new half)")
                .clicked()
        })
        .inner;
    if double {
        state.double_pattern_length();
    }
    ui.spacing_mut().item_spacing.x = HEAD_ITEM_GAP;
}

/// The row of beat numbers over the first cell of each group, counted through the pattern
/// (bar 2 shows 5 6 7 8), with the VOL / TUNE legends over the row knobs.
fn beat_numbers(ui: &mut Ui, state: &AppState, theme: &Theme) {
    let height = label_row_height(ui);
    let first_beat = state.shown_bar * 4;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(NAME_WIDTH + COLUMN_GAP);
        for (i, legend) in KNOB_LEGENDS.iter().enumerate() {
            if i > 0 {
                ui.add_space(KNOB_GAP);
            }
            let (rect, response) = ui.allocate_exact_size(vec2(SM_KNOB, height), Sense::hover());
            if ui.is_rect_visible(rect) {
                neu::text::paint_text(
                    ui.painter(),
                    rect.center(),
                    Align2::CENTER_CENTER,
                    legend,
                    TextKind::Label,
                    theme.text_secondary,
                );
            }
            response.widget_info(|| WidgetInfo::labeled(Role::Label, true, *legend));
        }
        ui.add_space(COLUMN_GAP);
        for beat in 0..4 {
            if beat > 0 {
                ui.add_space(cell_x(beat * 4) - cell_x((beat - 1) * 4) - STEP_CELL);
            }
            let (rect, response) = ui.allocate_exact_size(vec2(STEP_CELL, height), Sense::hover());
            let text = (first_beat + beat + 1).to_string();
            if ui.is_rect_visible(rect) {
                neu::text::paint_text(
                    ui.painter(),
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &text,
                    TextKind::Label,
                    theme.text_secondary,
                );
            }
            response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &text));
        }
    });
}

/// Lays `add` out as one `height`-tall row with its items vertically centred.
fn grid_row(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            add(ui);
        },
    );
}

/// The caps row name, vertically centred in a [`NAME_WIDTH`] column. A double-click mutes
/// the row's mixer channel (shift + double-click: solo); a muted row's name dims to
/// [`MUTED_ALPHA`], a soloed row gets a [`SOLO_DOT`] in the accent colour after its name.
fn name_cell(ui: &mut Ui, state: &mut AppState, theme: &Theme, text: &str, row: PaintRow) {
    let (rect, response) = ui.allocate_exact_size(vec2(NAME_WIDTH, ROW_HEIGHT), Sense::click());
    if response.double_clicked() {
        if press_modifiers(ui).shift {
            state.toggle_row_solo(row);
        } else {
            state.toggle_row_mute(row);
        }
    }
    let muted = state.row_muted(row);
    let soloed = state.row_soloed(row);
    if ui.is_rect_visible(rect) {
        let color = if muted {
            theme.text_secondary.gamma_multiply(MUTED_ALPHA)
        } else if response.hovered() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let text_rect = neu::text::paint_text(
            ui.painter(),
            rect.left_center(),
            Align2::LEFT_CENTER,
            text,
            TextKind::Label,
            color,
        );
        if soloed {
            let dot = egui::pos2(
                (text_rect.max.x + KNOB_GAP).min(rect.max.x - SOLO_DOT * 0.5),
                rect.center().y,
            );
            ui.painter()
                .circle_filled(dot, SOLO_DOT * 0.5, theme.accent);
        }
    }
    let state_text = match (muted, soloed) {
        (true, true) => "Muted, solo · ",
        (true, false) => "Muted · ",
        (false, true) => "Solo · ",
        (false, false) => "",
    };
    response
        .on_hover_text(format!(
            "{state_text}Double-click: mute · Shift + double-click: solo"
        ))
        .widget_info(|| WidgetInfo::labeled(Role::Button, true, text));
}

/// The modifiers of a press this frame: the raw modifier state, or the modifiers carried by
/// this frame's pointer-button events (synthetic clicks under `egui_kittest` deliver them
/// only on the event).
fn press_modifiers(ui: &Ui) -> Modifiers {
    ui.input(|i| {
        let mut mods = i.modifiers;
        for e in &i.events {
            if let egui::Event::PointerButton { modifiers, .. } = e {
                mods |= *modifiers;
            }
        }
        mods
    })
}

/// Whether a primary button press began on `response` this frame.
fn press_on(ui: &Ui, response: &Response) -> bool {
    response.contains_pointer() && ui.input(|i| i.pointer.primary_pressed())
}

/// Whether the running paint stroke should reach `response` this frame: a button is down and
/// the pointer is over the cell.
fn stroke_reaches(ui: &Ui, state: &AppState, response: &Response) -> bool {
    state.paint.is_some() && response.contains_pointer() && ui.input(|i| i.pointer.any_down())
}

/// A small, unlabelled knob (value shown as a tooltip) for one row setting. Returns the new
/// value when it moved, and whether this frame started a drag (a fresh undo step).
fn row_knob(
    ui: &mut Ui,
    theme: &Theme,
    value: f32,
    min: f32,
    max: f32,
    bipolar: bool,
    fmt: fn(f32) -> String,
) -> Option<(f32, bool)> {
    let spec = KnobSpec {
        label: "",
        min,
        max,
        default: 0.0,
        log: false,
        bipolar,
        size: KnobSize::Sm,
        fmt,
    };
    let mut v = value;
    let response = neu::knob(ui, theme, &mut v, &spec);
    (response.changed() || v != value).then_some((v, response.drag_started()))
}

/// Absolute step index of grid column `column` in the shown bar.
fn step_of(state: &AppState, column: usize) -> usize {
    state.shown_bar * STEPS + column
}

fn drum_row(ui: &mut Ui, state: &mut AppState, theme: &Theme, row: DrumRow) {
    grid_row(ui, ROW_HEIGHT, |ui| {
        name_cell(ui, state, theme, row_label(row), PaintRow::Drum(row));
        ui.add_space(COLUMN_GAP);

        let volume = state.pattern().drum_volume[row.index()];
        if let Some((db, drag_started)) = row_knob(ui, theme, volume, -24.0, 6.0, false, fmt_row_db)
        {
            if drag_started {
                state.begin_continuous_edit(EditKey::DrumVolume(row), true);
            }
            state.set_drum_volume(row, db);
        }
        ui.add_space(KNOB_GAP);
        let tune = state.pattern().drum_tune[row.index()];
        if let Some((st, drag_started)) =
            row_knob(ui, theme, tune, -12.0, 12.0, true, fmt_semitones)
        {
            if drag_started {
                state.begin_continuous_edit(EditKey::DrumTune(row), true);
            }
            state.set_drum_tune(row, st);
        }
        ui.add_space(COLUMN_GAP);

        for column in 0..STEPS {
            cell_gap(ui, column);
            let step = step_of(state, column);
            let cell = state.pattern().drums[row.index()][step];
            let spec = StepCellSpec {
                on: cell.on,
                accent: cell.accent,
                current: is_current(state, column),
                selected: false,
                label: None,
            };
            let response = neu::step_cell(ui, theme, &spec);
            let name = cell_name(row_name(row), step);
            response.widget_info(|| {
                WidgetInfo::selected(Role::Button, ui.is_enabled(), cell.on, &name)
            });
            // Right-click clears the cell (test-feedback branch); a primary press starts the
            // QOL drag-paint stroke (Ctrl: accent, Shift: shift-toggle); a running stroke
            // paints the cell it reaches.
            let paint_row = PaintRow::Drum(row);
            if response.secondary_clicked() {
                state.clear_drum_step(row, step);
            } else if press_on(ui, &response) {
                let mods = press_modifiers(ui);
                if mods.command || mods.ctrl {
                    state.toggle_drum_accent(row, step);
                } else if mods.shift {
                    state.toggle_drum_step(row, step, true);
                } else {
                    state.begin_paint(paint_row, !cell.on, step);
                }
            } else if stroke_reaches(ui, state, &response) {
                state.paint_step(paint_row, step);
            }
        }
        ui.add_space(COLUMN_GAP);

        if clear_pill(ui, theme).clicked() {
            state.clear_row(row);
        }
    });
}

fn bass_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    grid_row(ui, ROW_HEIGHT, |ui| {
        name_cell(ui, state, theme, BASS_NAME, PaintRow::Bass);
        ui.add_space(COLUMN_GAP + CONTROLS_WIDTH + COLUMN_GAP);

        for column in 0..STEPS {
            cell_gap(ui, column);
            let step = step_of(state, column);
            let cell = state.pattern().synth[step];
            let text = cell.note.map_or_else(|| "–".to_owned(), neu::note_name);
            let spec = StepCellSpec {
                on: cell.note.is_some(),
                accent: cell.accent,
                current: is_current(state, column),
                selected: state.selected_step == Some(step),
                label: Some(&text),
            };
            let response = neu::step_cell(ui, theme, &spec);
            let name = cell_name(BASS_NAME, step);
            response.widget_info(|| {
                WidgetInfo::selected(Role::Button, ui.is_enabled(), cell.note.is_some(), &name)
            });
            if response.secondary_clicked() {
                state.clear_synth_step(step);
            } else if press_on(ui, &response) {
                let mods = press_modifiers(ui);
                if mods.command || mods.ctrl {
                    state.toggle_synth_accent(step);
                } else {
                    state.click_synth_step(step, mods.shift);
                }
            }
        }
        ui.add_space(COLUMN_GAP);

        if clear_pill(ui, theme).clicked() {
            state.clear_synth();
        }
    });
}

/// The bottom row: BAR selector (patterns longer than a bar) under the name column, the
/// playhead dots under the grid (accent dot on the current column while this bar plays) and
/// the pattern CLEAR pill under the row CLEAR pills.
fn bottom_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    grid_row(ui, BOTTOM_ROW, |ui| {
        let left = ui.cursor().min.x;
        bar_selector(ui, state, theme);
        // Whatever the selector took, the dots start at the grid's left edge.
        let used = ui.cursor().min.x - left;
        ui.add_space((GRID_LEFT - used).max(0.0));
        for column in 0..STEPS {
            cell_gap(ui, column);
            neu::playhead_dot(ui, theme, is_current(state, column));
        }
        ui.add_space(COLUMN_GAP);
        let response = clear_pill(ui, theme).on_hover_text("Clear every step of this pattern");
        response.widget_info(|| {
            WidgetInfo::labeled(Role::Button, ui.is_enabled(), CLEAR_PATTERN_LABEL)
        });
        if response.clicked() {
            state.clear_pattern();
        }
    });
}

/// `BAR 1 | 2 | ..` for a pattern of more than one bar; nothing for a one-bar pattern.
fn bar_selector(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let bars = state.pattern().bars();
    if bars <= 1 {
        return;
    }
    let names: Vec<String> = (1..=bars).map(|b| b.to_string()).collect();
    let items: Vec<&str> = names.iter().map(String::as_str).collect();
    let before = state.shown_bar.min(bars - 1);
    let mut selected = before;
    ui.spacing_mut().item_spacing.x = KNOB_GAP;
    neu::label(ui, theme, BAR_LABEL);
    neu::segmented(ui, theme, &mut selected, &items).on_hover_text("Bar shown in the grid");
    ui.spacing_mut().item_spacing.x = 0.0;
    if selected != before {
        state.show_bar(selected);
    }
}

/// The gap before cell `step` (none before the first, a wider one at group boundaries).
fn cell_gap(ui: &mut Ui, step: usize) {
    if step > 0 {
        ui.add_space(cell_x(step) - cell_x(step - 1) - STEP_CELL);
    }
}

/// Whether the playhead sits on grid column `column`: the transport runs, the edit pattern is
/// the one playing, its playing bar is the shown bar and the step matches.
fn is_current(state: &AppState, column: usize) -> bool {
    let pos = state.playhead;
    pos.playing
        && pos.pattern == state.current()
        && usize::from(pos.bar) == state.shown_bar
        && usize::from(pos.step) == column
}

fn clear_pill(ui: &mut Ui, theme: &Theme) -> Response {
    neu::button(ui, theme, "Clear")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_positions_follow_the_mock_maths() {
        assert_eq!(cell_x(0), 0.0);
        assert_eq!(cell_x(1), 40.0);
        assert_eq!(cell_x(3), 120.0);
        assert_eq!(cell_x(4), 180.0);
        assert_eq!(cell_x(15), 15.0 * 40.0 + 3.0 * 20.0);
        assert_eq!(cell_x(15) + STEP_CELL, GRID_WIDTH);
        assert_eq!(SM_KNOB, KnobSize::Sm.block_width());
    }

    #[test]
    fn names_and_formatters() {
        assert_eq!(cell_name("Kick", 0), "Kick step 1");
        assert_eq!(cell_name(BASS_NAME, 15), "Bass step 16");
        assert_eq!(cell_name("Kick", 16), "Kick step 17");
        assert_eq!(queued_name(1), "Queued B");
        assert_eq!(row_name(DrumRow::ClosedHat), "Closed hat");
        assert_eq!(fmt_row_db(0.0), "0 dB");
        assert_eq!(fmt_row_db(-6.0), "-6 dB");
        assert_eq!(fmt_row_db(3.0), "+3 dB");
        assert_eq!(fmt_semitones(0.0), "+0 st");
        assert_eq!(fmt_semitones(-3.0), "-3 st");
        assert_eq!(length_items(), ["1 bar", "2 bars", "4 bars"]);
        assert_eq!(PATTERN_TABS.len(), PATTERN_COUNT);
    }
}
