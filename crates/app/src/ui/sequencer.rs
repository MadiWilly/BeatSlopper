//! SEQUENCER panel (M3/M5): the 16-step grid of one bar of the edit pattern, the pattern tabs
//! and the pattern controls.
//!
//! Layout follows `design/preview-a.html` `<section class="panel seq">` with the M5 pattern
//! controls built from the kit (the mock has no pattern UI):
//!
//! - **Head row:** title, the pattern tabs A..H (`neu::segmented`; a click sends
//!   `SelectPattern`, so while playing the engine switches at the next bar and the queued
//!   letter blinks beside the tabs until it does), then flush right the LENGTH `segmented`
//!   (1 bar | 2 bars | 4 bars), the FOLLOW toggle pill, COPY, PASTE and the REC lamp.
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
//! flips its accent (or gives an empty step a note). The state methods on `AppState` do the
//! work and send the engine messages.
//!
//! Piano roll (M7): the head's `GRID | ROLL` tab ([`LANE_VIEW_TABS`], [`AppState::lane_view`])
//! swaps the grid area for `neu::piano_roll` over the edit pattern's whole note list (every
//! bar, every bar line). The roll takes exactly the space of the six rows and the bottom row
//! ([`ROLL_AREA_HEIGHT`]) under a beat-number row aligned to its columns; the panel height is
//! the same in both views. Its geometry comes from [`roll_model`] (columns fitted to the
//! width, rows to the height, [`AppState::roll_low_pitch`] at the bottom) and every edit the
//! widget reports is applied by [`apply_roll_output`] through the undoable note methods on
//! `AppState`.

use egui::{Align, Align2, Layout, Response, Role, Sense, Ui, Vec2, WidgetInfo, vec2};
use engine::pattern::pattern_letter;
use engine::{DrumRow, Note, PATTERN_COUNT, PatternLength, STEPS};
use neu::{
    KEY_COLUMN_WIDTH, KnobSize, KnobSpec, PianoRollModel, PianoRollOutput, RollNote, STEP_CELL,
    STEP_GAP, STEP_GROUP_GAP, StepCellSpec, TextKind, Theme,
};

use crate::app::{AppState, EditKey, LaneView, queued_blink_visible};
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

/// The roll's view model for a roll of `size` (the space it may take: `ROLL_AREA_HEIGHT` tall
/// and the panel's width): one column per step of the edit pattern, the column width fitted
/// to the width (whole points, `MIN_ROLL_STEP_WIDTH..=MAX_ROLL_STEP_WIDTH`), as many
/// [`ROLL_ROW_HEIGHT`] rows as fit the height (at least [`MIN_ROLL_PITCHES`]),
/// [`AppState::roll_low_pitch`] at the bottom (clamped so the rows stay within MIDI), the
/// playhead while the edit pattern plays and the roll selection. Feeding the model's own
/// `size()` back in reproduces it, so tests can rebuild it from the rendered rect.
#[must_use]
pub fn roll_model(state: &AppState, size: Vec2) -> PianoRollModel {
    let steps = state.pattern().len().max(1);
    let step_width = ((size.x - KEY_COLUMN_WIDTH) / steps as f32)
        .floor()
        .clamp(MIN_ROLL_STEP_WIDTH, MAX_ROLL_STEP_WIDTH);
    let pitches = ((size.y / ROLL_ROW_HEIGHT).floor().max(0.0) as usize).max(MIN_ROLL_PITCHES);
    let mut model = PianoRollModel {
        steps,
        low_pitch: state.roll_low_pitch,
        pitches,
        step_width,
        row_height: ROLL_ROW_HEIGHT,
        playhead_step: state.roll_playhead(),
        selected: state.roll_selected,
    };
    model.low_pitch = model.low_pitch.min(model.max_low_pitch());
    model
}

/// The roll's view of a pattern note (`selected` when it is `state.roll_selected`).
#[must_use]
pub fn roll_notes(state: &AppState) -> Vec<RollNote> {
    state
        .pattern()
        .notes()
        .iter()
        .enumerate()
        .map(|(i, n)| RollNote {
            start: n.start,
            length: n.length,
            pitch: n.pitch,
            velocity: n.velocity,
            selected: state.roll_selected == Some(i),
        })
        .collect()
}

/// A roll note as a pattern note.
#[must_use]
pub fn to_note(n: RollNote) -> Note {
    Note::new(n.start, n.length, n.pitch, n.velocity)
}

/// Applies what the roll reported this frame: the scrolled bottom pitch and the click
/// selection (view state the widget edits in `model`), then the one edit, if any, through
/// [`AppState::add_note`] / [`AppState::set_note`] / [`AppState::remove_note`]. The selection
/// follows its note across the re-sort an add or move causes.
pub fn apply_roll_output(state: &mut AppState, model: &PianoRollModel, out: PianoRollOutput) {
    state.set_roll_low_pitch(model.low_pitch);
    state.roll_selected = model.selected;
    let selected_note = model
        .selected
        .and_then(|i| state.pattern().notes().get(i).copied());
    if let Some(n) = out.add {
        state.add_note(to_note(n));
        state.roll_selected =
            selected_note.and_then(|s| state.pattern().notes().iter().position(|n| *n == s));
    } else if let Some((i, n)) = out.moved {
        let new_index = state.set_note(i, to_note(n));
        state.roll_selected = if model.selected == Some(i) {
            new_index
        } else {
            selected_note.and_then(|s| state.pattern().notes().iter().position(|n| *n == s))
        };
    } else if let Some(i) = out.removed {
        // `remove_note` shifts the selection past the removed index itself.
        state.remove_note(i);
    }
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
        + BOTTOM_ROW
}

/// Renders the SEQUENCER panel into `ui` and applies any edit to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
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
            LaneView::Roll => {
                let model = roll_model(state, vec2(ui.available_width(), ROLL_AREA_HEIGHT));
                roll_beat_numbers(ui, theme, &model);
                ui.add_space(ROW_GAP);
                roll_area(ui, state, theme, model);
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

/// The piano roll over the edit pattern's notes, in the space of the grid rows and the bottom
/// row; whatever it reports is applied by [`apply_roll_output`].
fn roll_area(ui: &mut Ui, state: &mut AppState, theme: &Theme, mut model: PianoRollModel) {
    let notes = roll_notes(state);
    let out = neu::piano_roll(ui, theme, &mut model, &notes);
    apply_roll_output(state, &model, out);
    // Whole rows only: pad the rest so the panel is as tall as in GRID view.
    let rest = ROLL_AREA_HEIGHT - model.size().y;
    if rest > 0.0 {
        ui.add_space(rest);
    }
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
        controls::right_aligned(ui, "sequencer_head", |ui| {
            ui.spacing_mut().item_spacing.x = HEAD_ITEM_GAP;
            length_selector(ui, state, theme);
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

/// LENGTH: 1 bar | 2 bars | 4 bars for the edit pattern.
fn length_selector(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let items = length_items();
    let before = PatternLength::ALL
        .iter()
        .position(|l| *l == state.pattern().pattern_length())
        .unwrap_or(0);
    let mut selected = before;
    neu::segmented(ui, theme, &mut selected, &items).on_hover_text("Pattern length");
    if selected != before {
        state.set_pattern_length(PatternLength::ALL[selected]);
    }
}

/// The row of beat numbers over the first cell of each group, counted through the pattern
/// (bar 2 shows 5 6 7 8).
fn beat_numbers(ui: &mut Ui, state: &AppState, theme: &Theme) {
    let height = label_row_height(ui);
    let first_beat = state.shown_bar * 4;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(GRID_LEFT);
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
        name_cell(ui, theme, row.label());
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
    grid_row(ui, ROW_HEIGHT, |ui| {
        name_cell(ui, theme, BASS_NAME);
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
