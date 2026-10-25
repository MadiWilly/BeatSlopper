//! SEQUENCER panel (M3): the 16-step grid.
//!
//! Layout follows `design/preview-a.html` `<section class="panel seq">`: a panel head (title
//! left, the shift-click hint, the REC lamp and the CLEAR ALL pill right), a beat-number row
//! (1-4 over the first cell of each group of four), one row per [`DrumRow::ALL`] (caps name,
//! small VOL and TUNE knobs, 16 `neu::step_cell`s in groups of four, a CLEAR pill), the BASS
//! lane (note names in the cells, no knobs) and the playhead dots under the grid.
//!
//! Cell `i` sits at [`cell_x`]`(i)` from the grid's left edge, so the beat numbers and the
//! playhead dots use the same maths as the rows. Every cell reports an accessible name
//! ([`cell_name`], e.g. "Kick step 1") so tests can click it by label.
//!
//! Interactions: click toggles a drum step, shift-click turns it on and flips the accent;
//! a bass cell click selects the step for note entry (click again to deselect), shift-click
//! flips its accent (or gives an empty step a note). The state methods on `AppState` do the
//! work and send the engine messages.

use egui::{Align, Align2, Layout, Response, Role, Sense, Ui, WidgetInfo, vec2};
use engine::{DrumRow, STEPS};
use neu::{KnobSize, KnobSpec, STEP_CELL, STEP_GAP, STEP_GROUP_GAP, StepCellSpec, TextKind, Theme};

use crate::app::AppState;
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
pub const ROW_GAP: f32 = 4.0;
/// Gap between the panel head and the beat numbers.
pub const HEAD_GAP: f32 = 10.0;
/// Width of the 16-cell grid: 16 cells, 15 gaps, 3 of them widened by the group gap.
pub const GRID_WIDTH: f32 = 16.0 * STEP_CELL + 15.0 * STEP_GAP + 3.0 * STEP_GROUP_GAP;
/// Left edge of the grid relative to the panel content.
pub const GRID_LEFT: f32 = NAME_WIDTH + COLUMN_GAP + CONTROLS_WIDTH + COLUMN_GAP;
/// Caps name of the bass lane.
pub const BASS_NAME: &str = "Bass";
/// Height of the playhead dot row (`neu::playhead_dot`).
const DOT_ROW: f32 = 10.0;

/// Left edge of cell `i` relative to the grid's left edge.
#[must_use]
pub fn cell_x(i: usize) -> f32 {
    i as f32 * (STEP_CELL + STEP_GAP) + (i / 4) as f32 * STEP_GROUP_GAP
}

/// Accessible name of a step cell: `"Kick step 1"`.
#[must_use]
pub fn cell_name(row: &str, step: usize) -> String {
    format!("{row} step {}", step + 1)
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

/// Height of the beat-number row (one caps label line).
fn label_row_height(ui: &Ui) -> f32 {
    ui.ctx()
        .fonts_mut(|f| f.row_height(&TextKind::Label.font_id()))
}

/// Height of the whole panel (padding included), from the layout constants, so the page can
/// reserve the exact space without measuring a frame first.
#[must_use]
pub fn panel_height(ui: &Ui, theme: &Theme) -> f32 {
    let rows = (DrumRow::COUNT + 1) as f32;
    2.0 * theme.panel_padding
        + controls::ROW_HEIGHT
        + HEAD_GAP
        + label_row_height(ui)
        + ROW_GAP
        + rows * (ROW_HEIGHT + ROW_GAP)
        + DOT_ROW
}

/// Renders the SEQUENCER panel into `ui` and applies any edit to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        // Every row is one allocation; the gaps below are explicit, so no implicit spacing.
        ui.spacing_mut().item_spacing.y = 0.0;
        head(ui, state, theme);
        ui.add_space(HEAD_GAP);
        beat_numbers(ui, theme);
        ui.add_space(ROW_GAP);
        for row in DrumRow::ALL {
            drum_row(ui, state, theme, row);
            ui.add_space(ROW_GAP);
        }
        bass_row(ui, state, theme);
        ui.add_space(ROW_GAP);
        playhead_row(ui, state, theme);
    });
}

fn head(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.set_min_height(controls::ROW_HEIGHT);
        neu::title(ui, theme, "Sequencer");
        controls::right_aligned(ui, "sequencer_head", |ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            neu::label(ui, theme, "shift-click = accent");
            neu::lamp(ui, theme, state.is_recording(), "Rec");
            if neu::button(ui, theme, "Clear all").clicked() {
                state.clear_all();
            }
        });
    });
}

/// The row of `1 2 3 4` over the first cell of each group.
fn beat_numbers(ui: &mut Ui, theme: &Theme) {
    let height = label_row_height(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(GRID_LEFT);
        for beat in 0..4 {
            if beat > 0 {
                ui.add_space(cell_x(beat * 4) - cell_x((beat - 1) * 4) - STEP_CELL);
            }
            let (rect, response) = ui.allocate_exact_size(vec2(STEP_CELL, height), Sense::hover());
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

/// Lays `add` out as one [`ROW_HEIGHT`]-tall row with its items vertically centred.
fn grid_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), ROW_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            add(ui);
        },
    );
}

/// The caps row name, vertically centred in a [`NAME_WIDTH`] column.
fn name_cell(ui: &mut Ui, theme: &Theme, text: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(NAME_WIDTH, ROW_HEIGHT), Sense::hover());
    if ui.is_rect_visible(rect) {
        neu::text::paint_text(
            ui.painter(),
            rect.left_center(),
            Align2::LEFT_CENTER,
            text,
            TextKind::Label,
            theme.text_secondary,
        );
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
}

/// A small, unlabelled knob (value shown as a tooltip) for one row setting.
fn row_knob(
    ui: &mut Ui,
    theme: &Theme,
    value: f32,
    min: f32,
    max: f32,
    bipolar: bool,
    fmt: fn(f32) -> String,
) -> Option<f32> {
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
    (response.changed() || v != value).then_some(v)
}

fn drum_row(ui: &mut Ui, state: &mut AppState, theme: &Theme, row: DrumRow) {
    grid_row(ui, |ui| {
        name_cell(ui, theme, row.label());
        ui.add_space(COLUMN_GAP);

        let volume = state.pattern.drum_volume[row.index()];
        if let Some(db) = row_knob(ui, theme, volume, -24.0, 6.0, false, fmt_row_db) {
            state.set_drum_volume(row, db);
        }
        ui.add_space(KNOB_GAP);
        let tune = state.pattern.drum_tune[row.index()];
        if let Some(st) = row_knob(ui, theme, tune, -12.0, 12.0, true, fmt_semitones) {
            state.set_drum_tune(row, st);
        }
        ui.add_space(COLUMN_GAP);

        for step in 0..STEPS {
            cell_gap(ui, step);
            let cell = state.pattern.drums[row.index()][step];
            let spec = StepCellSpec {
                on: cell.on,
                accent: cell.accent,
                current: is_current(state, step),
                selected: false,
                label: None,
            };
            let response = neu::step_cell(ui, theme, &spec);
            let name = cell_name(row_name(row), step);
            response.widget_info(|| {
                WidgetInfo::selected(Role::Button, ui.is_enabled(), cell.on, &name)
            });
            if response.clicked() {
                let shift = controls::click_shift(ui);
                state.toggle_drum_step(row, step, shift);
            }
        }
        ui.add_space(COLUMN_GAP);

        if clear_pill(ui, theme).clicked() {
            state.clear_row(row);
        }
    });
}

fn bass_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    grid_row(ui, |ui| {
        name_cell(ui, theme, BASS_NAME);
        ui.add_space(COLUMN_GAP + CONTROLS_WIDTH + COLUMN_GAP);

        for step in 0..STEPS {
            cell_gap(ui, step);
            let cell = state.pattern.synth[step];
            let text = cell.note.map_or_else(|| "–".to_owned(), neu::note_name);
            let spec = StepCellSpec {
                on: cell.note.is_some(),
                accent: cell.accent,
                current: is_current(state, step),
                selected: state.selected_step == Some(step),
                label: Some(&text),
            };
            let response = neu::step_cell(ui, theme, &spec);
            let name = cell_name(BASS_NAME, step);
            response.widget_info(|| {
                WidgetInfo::selected(Role::Button, ui.is_enabled(), cell.note.is_some(), &name)
            });
            if response.clicked() {
                let shift = controls::click_shift(ui);
                state.click_synth_step(step, shift);
            }
        }
        ui.add_space(COLUMN_GAP);

        if clear_pill(ui, theme).clicked() {
            state.clear_synth();
        }
    });
}

/// The playhead dots under the grid (accent dot on the current column while playing).
fn playhead_row(ui: &mut Ui, state: &AppState, theme: &Theme) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), DOT_ROW),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(GRID_LEFT);
            for step in 0..STEPS {
                cell_gap(ui, step);
                neu::playhead_dot(ui, theme, is_current(state, step));
            }
        },
    );
}

/// The gap before cell `step` (none before the first, a wider one at group boundaries).
fn cell_gap(ui: &mut Ui, step: usize) {
    if step > 0 {
        ui.add_space(cell_x(step) - cell_x(step - 1) - STEP_CELL);
    }
}

/// Whether the playhead sits on `step`.
fn is_current(state: &AppState, step: usize) -> bool {
    state.playhead.playing && usize::from(state.playhead.step) == step
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
        assert_eq!(row_name(DrumRow::ClosedHat), "Closed hat");
        assert_eq!(fmt_row_db(0.0), "0 dB");
        assert_eq!(fmt_row_db(-6.0), "-6 dB");
        assert_eq!(fmt_row_db(3.0), "+3 dB");
        assert_eq!(fmt_semitones(0.0), "+0 st");
        assert_eq!(fmt_semitones(-3.0), "-3 st");
    }
}
