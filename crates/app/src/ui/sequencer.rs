//! The CHANNEL RACK window (M3 / M5 / M10 / M11; Deck since M12, `design/round3/WINNER.md`
//! section 5 "CHANNEL RACK window"): FL Studio's channel rack over the edit pattern.
//!
//! - **Header** "CHANNEL RACK" · sub "Pattern A · 2 bars · <track> · <preset>" · F5 ·
//!   chevron (collapse to the header) · close (the same: the F5 key or the toolbar's roll
//!   button bring it back).
//! - **Toolbar** (36): the pattern tiles A..H (`neu::pattern_tile`, named "Pattern A"; a
//!   click sends `SelectPattern`, so while playing the engine switches at the next bar and
//!   the queued letter blinks beside the tiles until it does) · "Len" + the small segmented
//!   `1 bar | 2 bars | 4 bars` (doubling copies the loop into the new half, QOL) · "Bar" +
//!   the small segmented of bars (patterns longer than a bar) · FOLLOW · spacer · copy ·
//!   paste · clear pattern (small ghost icons). Compact (a rack under [`COMPACT_BELOW`]
//!   wide, the narrow layout): dropdowns "Pattern A" / "2 bars", the bar segmented, FOLLOW
//!   and a dots menu with Copy · Paste · Clear.
//! - **Body** (padding-top 4; a vertical scroll area): the ruler (20 h, beat numbers counted
//!   through the pattern, the playhead triangle) over the steps, the DRUMS section head
//!   (with the VOL TUNE / INS legends), the five drum rows, the TRACKS head ("Tracks n / 8",
//!   VOL PAN), one row per active track and the add row ("+ Add track Ctrl+T", or the
//!   "Delete <name>?" confirm). The left columns (LED · S · knob · knob · INS · NAME) stay put
//!   while the sixteen step cells scroll horizontally in their own always-visible scroll
//!   area, so a narrow rack shows a 210 pt viewport of the steps (WINNER narrow).
//!
//! Row: 28 h on a 32 pitch, padding 0 8, gap 6, columns LED 10 · S 22 · VOL 22 · TUNE / PAN
//! 22 · INS 22 · NAME 76 · STEPS 426 = 652 ([`CONTENT_WIDTH`]); compact rows drop the two
//! knobs (the values live in the LED tooltip) and use a 64 name. The selected row (the edit
//! track) sits on the accent wash with a 2 px accent bar; a muted row's LED is off and its
//! name reads `text_2`.
//!
//! The grid shows bar [`AppState::shown_bar`] of the edit pattern: cell `i` edits the
//! absolute step `shown_bar * 16 + i` and the playhead column lights only when the playing
//! pattern, bar and step match. Every cell reports an accessible name ([`cell_name`], e.g.
//! "Kick step 17" on bar 2, "Synth step 3" on a track row).
//!
//! Interactions: click toggles a drum step, Shift-click turns it on and flips the accent,
//! Ctrl-click flips the accent; pressing a drum cell and dragging across its row paints every
//! cell to the first cell's new state (one undo step per stroke, `crate::drag_paint`). A
//! track cell click selects the step for note entry (click again to deselect; it selects
//! the track first when it is another track's row), Shift-click flips its accent (or gives
//! an empty step a note). Right-click clears a cell. The row LED: click mutes, right-click
//! solos. A drum NAME button: double-click mutes (Shift: solo); a track NAME button: click
//! selects, double-click renames inline, Shift + double-click solos; right-click opens the
//! row menu (`crate::ui::tracks`). The state methods on `AppState` do the work and send the
//! engine messages.

use egui::{
    Align, Align2, CursorIcon, Frame, Layout, Modifiers, Rect, Response, Role, ScrollArea, Sense,
    Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};
use engine::pattern::pattern_letter;
use engine::track::COLOUR_COUNT;
use engine::{
    Channel, ChannelParam, DrumRow, PATTERN_COUNT, PatternLength, STEPS, SynthStep, TrackId,
};
use neu::{
    ButtonKind, ButtonSize, Icon, KnobSize, KnobSpec, LedKind, MenuItem, STEP_CELL, STEP_GAP,
    STEP_GROUP_GAP, StepCellSpec, TextKind, Theme, ValueEditOutput,
};

use crate::app::{AppState, EditKey, Panel, View, queued_blink_visible};
use crate::drag_paint::PaintRow;
use crate::shortcuts::Shortcut;
use crate::ui::controls::{fmt_level_db, fmt_pan};
use crate::ui::{popout, tracks};

/// Height of a row's controls and the row pitch.
pub const ROW_HEIGHT: f32 = 28.0;
pub const ROW_PITCH: f32 = 32.0;
/// Row padding (left and right) and the gap between columns.
pub const ROW_PAD: f32 = 8.0;
pub const COL_GAP: f32 = 6.0;
/// The column widths: LED, S, a mini knob, the insert box, the name (wide / compact).
pub const LED_WIDTH: f32 = 10.0;
pub const SOLO_WIDTH: f32 = 22.0;
pub const KNOB_WIDTH: f32 = 22.0;
pub const INS_WIDTH: f32 = 22.0;
pub const NAME_WIDTH: f32 = 76.0;
pub const NAME_WIDTH_COMPACT: f32 = 64.0;
/// Width of the 16-cell grid: 16 cells, 12 x 2 gaps and 3 x 6 beat gaps (426).
pub const GRID_WIDTH: f32 = 16.0 * STEP_CELL + 12.0 * STEP_GAP + 3.0 * STEP_GROUP_GAP;
/// The row's content width in the wide layout (652) and the compact one (368).
pub const CONTENT_WIDTH: f32 = left_width(false) + GRID_WIDTH + ROW_PAD;
pub const CONTENT_WIDTH_COMPACT: f32 = left_width(true) + STEPS_VIEWPORT_COMPACT + ROW_PAD;
/// The steps viewport of a compact rack (WINNER narrow: 210).
pub const STEPS_VIEWPORT_COMPACT: f32 = 210.0;
/// A rack narrower than this drops its knob columns and scrolls its steps.
pub const COMPACT_BELOW: f32 = 600.0;
/// Height of the ruler row, a section head and the add row.
pub const RULER_HEIGHT: f32 = 20.0;
pub const SECTION_HEAD: f32 = 24.0;
pub const ADD_ROW: f32 = 28.0;
/// Body padding-top.
pub const BODY_PAD_TOP: f32 = 4.0;
/// Window title.
pub const TITLE: &str = "Channel rack";
/// Section titles.
pub const DRUMS_TITLE: &str = "Drums";
pub const TRACKS_TITLE: &str = "Tracks";
/// Pattern tile letters, A..H.
pub const PATTERN_TABS: [&str; PATTERN_COUNT] = ["A", "B", "C", "D", "E", "F", "G", "H"];
/// Caps labels in the toolbar.
pub const LEN_LABEL: &str = "Len";
pub const BAR_LABEL: &str = "Bar";
pub const FOLLOW_LABEL: &str = "Follow";
/// Accessible names of the toolbar's icon buttons.
pub const COPY_LABEL: &str = "Copy";
pub const PASTE_LABEL: &str = "Paste";
pub const CLEAR_PATTERN_LABEL: &str = "Clear pattern";
/// Accessible names of the compact toolbar's dropdowns and menu.
pub const PATTERN_DROPDOWN: &str = "Pattern";
pub const LENGTH_DROPDOWN: &str = "Length";
pub const MORE_LABEL: &str = "More";
/// Accessible names of the header buttons.
pub const COLLAPSE_LABEL: &str = "Collapse rack";
pub const EXPAND_LABEL: &str = "Expand rack";
pub const CLOSE_LABEL: &str = "Close rack";
/// Alpha of a muted row's name.
pub const MUTED_ALPHA: f32 = 0.45;

/// Width of the fixed left columns (padding and gaps included, up to the steps).
#[must_use]
pub const fn left_width(compact: bool) -> f32 {
    if compact {
        ROW_PAD
            + LED_WIDTH
            + COL_GAP
            + SOLO_WIDTH
            + COL_GAP
            + INS_WIDTH
            + COL_GAP
            + NAME_WIDTH_COMPACT
            + COL_GAP
    } else {
        ROW_PAD
            + LED_WIDTH
            + COL_GAP
            + SOLO_WIDTH
            + COL_GAP
            + KNOB_WIDTH
            + COL_GAP
            + KNOB_WIDTH
            + COL_GAP
            + INS_WIDTH
            + COL_GAP
            + NAME_WIDTH
            + COL_GAP
    }
}

/// Whether a rack `width` points wide uses the compact rows.
#[must_use]
pub fn is_compact(width: f32) -> bool {
    width < COMPACT_BELOW
}

/// Left edge of cell `i` relative to the grid's left edge.
#[must_use]
pub fn cell_x(i: usize) -> f32 {
    neu::step_x_offset(i)
}

/// Accessible name of a step cell: `"Kick step 1"` (`step` is the absolute, 0-based index).
#[must_use]
pub fn cell_name(row: &str, step: usize) -> String {
    format!("{row} step {}", step + 1)
}

/// Accessible name of a pattern tile: `"Pattern B"`.
#[must_use]
pub fn tile_name(pattern: u8) -> String {
    format!("Pattern {}", pattern_letter(pattern))
}

/// Accessible name of the queued-pattern indicator: `"Queued B"`.
#[must_use]
pub fn queued_name(pattern: u8) -> String {
    format!("Queued {}", pattern_letter(pattern))
}

/// Caps label of a drum row, spelled like the mixer strips (`C.Hat`, `O.Hat`).
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

/// Accessible name of a drum row's insert number box: `"Row insert Kick"`.
#[must_use]
pub fn row_box_name(text: &str) -> String {
    format!("Row insert {text}")
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

/// The header's sub-title: `"Pattern A · 2 bars · Synth · Bass"`.
#[must_use]
pub fn header_sub(state: &AppState) -> String {
    let track = state.edit_track_ui();
    format!(
        "Pattern {} · {} · {} · {}",
        state.current_letter(),
        state.pattern().pattern_length().label(),
        track.name,
        track.preset_name()
    )
}

/// The lane cell of track `slot` at `step`: the edit track's derived lane, or the lowest
/// note starting at the step of another track's notes.
#[must_use]
pub fn lane_cell(state: &AppState, slot: TrackId, step: usize) -> SynthStep {
    if slot == state.edit_track {
        return state.pattern().synth[step % engine::MAX_STEPS];
    }
    let mut cell = SynthStep::default();
    for note in state.track_notes(slot) {
        if usize::from(note.start) == step && cell.note.is_none_or(|p| note.pitch < p) {
            cell = note.lane_step();
        }
    }
    cell
}

/// Height of the body's content for `state` (the rows, heads, ruler and add row), so a
/// caller can size the window.
#[must_use]
pub fn body_height(state: &AppState) -> f32 {
    let rows = (DrumRow::COUNT + state.active_track_count()) as f32;
    BODY_PAD_TOP + RULER_HEIGHT + 2.0 * SECTION_HEAD + rows * ROW_PITCH + ADD_ROW
}

/// Renders the CHANNEL RACK window into the available rect (collapsed to its 28 pt header
/// while `!state.rack_open`; the caller gives it 28 points then) and applies any edit to
/// `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let size = ui.available_size_before_wrap();
    // In its own window the rack is always expanded.
    let open = state.rack_open || state.is_popped(Panel::Rack);
    let height = if open { size.y } else { neu::WINDOW_HEADER };
    let compact = is_compact(size.x);
    let sub = header_sub(state);
    // The header and the body both edit the state; the frame runs them one after the other.
    let cell = std::cell::RefCell::new(state);
    neu::window_frame_sized(
        ui,
        theme,
        vec2(size.x, height),
        TITLE,
        Some(&sub),
        |ui| header_right(ui, &mut cell.borrow_mut(), theme, compact),
        |ui| {
            if open {
                let state = &mut **cell.borrow_mut();
                toolbar(ui, state, theme, compact);
                body(ui, state, theme, compact);
            }
        },
    );
}

/// The header's right slot (right-to-left): close, the chevron, POP OUT, the F5 keycap; in
/// the rack's own window DOCK and the keycap only.
fn header_right(ui: &mut Ui, state: &mut AppState, theme: &Theme, compact: bool) {
    if state.is_popped(Panel::Rack) {
        popout::header_control(ui, state, theme, Panel::Rack);
        if !compact {
            neu::keycap(ui, theme, "F5");
        }
        return;
    }
    let close = neu::header_button(ui, theme, Icon::Clear);
    close.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), CLOSE_LABEL));
    if close
        .on_hover_text("Hide the rack (F5 brings it back)")
        .clicked()
    {
        state.rack_open = false;
    }
    let (icon, name, tip) = if state.rack_open {
        (Icon::ChevronUp, COLLAPSE_LABEL, "Collapse to the header")
    } else {
        (Icon::ChevronDown, EXPAND_LABEL, "Expand the rack (F5)")
    };
    let chevron = neu::header_button(ui, theme, icon);
    chevron.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), name));
    if chevron.on_hover_text(tip).clicked() {
        state.toggle_rack();
    }
    popout::header_control(ui, state, theme, Panel::Rack);
    if !compact {
        neu::keycap(ui, theme, "F5");
    }
}

// ---------------------------------------------------------------------------------------------
// Toolbar.

fn toolbar(ui: &mut Ui, state: &mut AppState, theme: &Theme, compact: bool) {
    neu::toolbar(ui, theme, |ui| {
        if compact {
            pattern_dropdown(ui, state, theme);
            queued_indicator(ui, state, theme);
            length_dropdown(ui, state, theme);
        } else {
            pattern_tiles(ui, state, theme);
            queued_indicator(ui, state, theme);
            neu::label(ui, theme, LEN_LABEL);
            length_group(ui, state, theme);
        }
        bar_selector(ui, state, theme);
        if neu::button_kind(
            ui,
            theme,
            FOLLOW_LABEL,
            ButtonKind::Toggle(state.follow),
            ButtonSize::Small,
        )
        .on_hover_text("Follow the playing bar (the roll scrolls with the playhead too)")
        .clicked()
        {
            state.toggle_follow();
        }
        // The icons flush right.
        let width_id = ui.scope_id().with("rack_toolbar_right");
        let known: Option<f32> = ui.data(|d| d.get_temp(width_id));
        if let Some(width) = known {
            let pad = ui.available_width() - width;
            if pad > 0.0 {
                ui.add_space(pad);
            }
        }
        let group = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if compact {
                more_menu(ui, state, theme);
            } else {
                edit_icons(ui, state, theme);
            }
        });
        ui.data_mut(|d| d.insert_temp(width_id, group.response.rect.width()));
    });
}

/// The A..H tiles: the current one lit, the ones with content in `text`.
fn pattern_tiles(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let current = state.current();
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, letter) in PATTERN_TABS.iter().enumerate() {
            let has_notes = !state.bank.patterns[i].is_empty();
            if neu::pattern_tile(ui, theme, letter, has_notes, i as u8 == current)
                .on_hover_text(format!(
                    "Pattern {letter} (switches at the next bar while playing)"
                ))
                .clicked()
            {
                clicked = Some(i as u8);
            }
        }
    });
    if let Some(index) = clicked
        && index != current
    {
        state.select_pattern(index);
    }
}

/// The compact toolbar's pattern dropdown ("Pattern A").
fn pattern_dropdown(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let names: Vec<String> = (0..PATTERN_COUNT as u8).map(tile_name).collect();
    let items: Vec<&str> = names.iter().map(String::as_str).collect();
    let value = tile_name(state.current());
    if let Some(picked) = neu::dropdown(
        ui,
        theme,
        PATTERN_DROPDOWN,
        &value,
        &items,
        96.0,
        true,
        None,
    ) && picked as u8 != state.current()
    {
        state.select_pattern(picked as u8);
    }
}

/// The compact toolbar's length dropdown ("2 bars").
fn length_dropdown(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let items = length_items();
    let value = state.pattern().pattern_length().label();
    if let Some(picked) = neu::dropdown(ui, theme, LENGTH_DROPDOWN, value, &items, 72.0, true, None)
    {
        set_length_item(state, picked);
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
    let (rect, response) =
        ui.allocate_exact_size(vec2(galley.size().x.max(12.0), ROW_HEIGHT), Sense::hover());
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

/// LEN: the small segmented `1 bar | 2 bars | 4 bars`.
fn length_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let items = length_items();
    let before = PatternLength::ALL
        .iter()
        .position(|l| *l == state.pattern().pattern_length())
        .unwrap_or(0);
    let mut selected = before;
    neu::segmented_small(ui, theme, &mut selected, &items)
        .on_hover_text("Pattern length: doubling copies the loop into the new half");
    if selected != before {
        set_length_item(state, selected);
    }
}

/// Sets the edit pattern's length to item `index` of [`length_items`] by doubling (the loop
/// is copied into the new half) or halving (hidden steps are kept) step by step.
fn set_length_item(state: &mut AppState, index: usize) {
    let Some(target) = PatternLength::ALL.get(index) else {
        return;
    };
    let target = target.steps() as usize;
    let mut guard = 0;
    while state.pattern().len() < target && guard < 4 {
        if !state.double_pattern_length() {
            break;
        }
        guard += 1;
    }
    while state.pattern().len() > target && guard < 8 {
        if !state.halve_pattern_length() {
            break;
        }
        guard += 1;
    }
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
    neu::label(ui, theme, BAR_LABEL);
    neu::segmented_small(ui, theme, &mut selected, &items).on_hover_text("Bar shown in the grid");
    if selected != before {
        state.show_bar(selected);
    }
}

/// Copy · paste · clear as small ghost icons.
fn edit_icons(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let small = vec2(22.0, 22.0);
    let copy = neu::icon_button_kind(ui, theme, Icon::Copy, small, ButtonKind::Ghost);
    copy.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), COPY_LABEL));
    if copy.on_hover_text("Copy this pattern").clicked() {
        state.copy_pattern();
    }
    let paste = ui
        .add_enabled_ui(state.clipboard.is_some(), |ui| {
            let paste = neu::icon_button_kind(ui, theme, Icon::Paste, small, ButtonKind::Ghost);
            paste.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), PASTE_LABEL));
            paste
                .on_hover_text("Paste the copied pattern here")
                .clicked()
        })
        .inner;
    if paste {
        state.paste_pattern();
    }
    let clear = neu::icon_button_kind(ui, theme, Icon::Clear, small, ButtonKind::Ghost);
    clear.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), CLEAR_PATTERN_LABEL));
    if clear
        .on_hover_text("Clear every step of this pattern")
        .clicked()
    {
        state.clear_pattern();
    }
}

/// The compact toolbar's dots menu: Copy · Paste · Clear pattern.
fn more_menu(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let button = neu::icon_button_kind(ui, theme, Icon::Dots, vec2(22.0, 22.0), ButtonKind::Ghost);
    button.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), MORE_LABEL));
    let items = [
        MenuItem::item(COPY_LABEL),
        if state.clipboard.is_some() {
            MenuItem::item(PASTE_LABEL)
        } else {
            MenuItem::disabled(PASTE_LABEL, None)
        },
        MenuItem::item(CLEAR_PATTERN_LABEL),
    ];
    match neu::menu_popup(ui, theme, &button, &items) {
        Some(0) => state.copy_pattern(),
        Some(1) => {
            state.paste_pattern();
        }
        Some(2) => state.clear_pattern(),
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Body.

/// One line of the body, drawn by both column blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Ruler,
    DrumsHead,
    Drum(DrumRow),
    TracksHead,
    Track(usize, TrackId),
    AddRow,
}

impl Line {
    fn height(self) -> f32 {
        match self {
            Line::Ruler => RULER_HEIGHT,
            Line::DrumsHead | Line::TracksHead => SECTION_HEAD,
            Line::Drum(_) | Line::Track(..) => ROW_PITCH,
            Line::AddRow => ADD_ROW,
        }
    }
}

fn lines(state: &AppState) -> Vec<Line> {
    let mut lines = vec![Line::Ruler, Line::DrumsHead];
    lines.extend(DrumRow::ALL.into_iter().map(Line::Drum));
    lines.push(Line::TracksHead);
    lines.extend(
        state
            .active_tracks()
            .enumerate()
            .map(|(i, slot)| Line::Track(i, slot)),
    );
    lines.push(Line::AddRow);
    lines
}

/// The body: a vertical scroll area holding the fixed left columns beside the steps' own
/// horizontal scroll area.
fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme, compact: bool) {
    // A paint stroke lives as long as a button is down.
    if !ui.input(|i| i.pointer.any_down()) {
        state.end_paint();
    }
    // A confirm for a track that went away (undo, a load) is dropped.
    if tracks::pending_delete(ui).is_some_and(|slot| !state.track_active(slot)) {
        tracks::set_pending_delete(ui, None);
    }
    let lines = lines(state);
    let left = left_width(compact);
    neu::scroll_vertical("rack_body").show(ui, |ui| {
        ui.add_space(BODY_PAD_TOP);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.allocate_ui_with_layout(vec2(left, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(left);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for line in &lines {
                    left_line(ui, state, theme, *line, compact);
                }
            });
            let steps_width = GRID_WIDTH + ROW_PAD;
            let viewport = if compact {
                STEPS_VIEWPORT_COMPACT
            } else {
                ui.available_width().max(steps_width)
            };
            ui.allocate_ui_with_layout(vec2(viewport, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(viewport);
                let out = ScrollArea::horizontal()
                    .id_salt("rack_steps")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        ui.set_min_width(steps_width);
                        for line in &lines {
                            steps_line(ui, state, theme, *line, steps_width);
                        }
                    });
                ui.data_mut(|d| {
                    d.insert_temp(
                        steps_scroll_id(),
                        StepsScroll {
                            content: out.content_size.x,
                            viewport: out.inner_rect.width(),
                        },
                    )
                });
            });
        });
    });
}

/// What the steps' horizontal scroll area measured last frame (tests read it through
/// [`steps_scroll`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepsScroll {
    /// Width of the sixteen cells and their gaps.
    pub content: f32,
    /// Width of the viewport the cells scroll in.
    pub viewport: f32,
}

impl StepsScroll {
    /// Whether the cells overflow the viewport (the horizontal scrollbar has a thumb shorter
    /// than its track).
    #[must_use]
    pub fn scrolls(self) -> bool {
        self.content > self.viewport + 0.5
    }
}

fn steps_scroll_id() -> egui::Id {
    egui::Id::NULL.with("rack_steps_scroll")
}

/// The steps scroll area's measurement from the last frame, if the rack was drawn.
#[must_use]
pub fn steps_scroll(ctx: &egui::Context) -> Option<StepsScroll> {
    ctx.data(|d| d.get_temp(steps_scroll_id()))
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

/// Absolute step index of grid column `column` in the shown bar.
fn step_of(state: &AppState, column: usize) -> usize {
    state.shown_bar * STEPS + column
}

/// The gap before cell `step` (none before the first, a wider one at group boundaries).
fn cell_gap(ui: &mut Ui, step: usize) {
    if step > 0 {
        ui.add_space(cell_x(step) - cell_x(step - 1) - STEP_CELL);
    }
}

/// Paints the selected row's accent wash (and the bar when `bar`) over `rect`.
fn paint_row_wash(ui: &Ui, rect: Rect, theme: &Theme, bar: bool) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme.accent_wash);
    if bar {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            theme.accent,
        );
    }
}

// ---- The steps block.

fn steps_line(ui: &mut Ui, state: &mut AppState, theme: &Theme, line: Line, width: f32) {
    match line {
        Line::Ruler => ruler(ui, state, theme, width),
        Line::DrumsHead | Line::TracksHead | Line::AddRow => {
            ui.allocate_exact_size(vec2(width, line.height()), Sense::hover());
        }
        Line::Drum(row) => drum_cells(ui, state, theme, row, width),
        Line::Track(_, slot) => track_cells(ui, state, theme, slot, width),
    }
}

/// The ruler over the steps: a 24 x 16 cell per column with the beat number (counted through
/// the pattern) over every fourth column and a dot over the others; the playhead triangle
/// under the current column.
fn ruler(ui: &mut Ui, state: &AppState, theme: &Theme, width: f32) {
    let first_beat = state.shown_bar * 4;
    ui.allocate_ui_with_layout(
        vec2(width, RULER_HEIGHT),
        Layout::left_to_right(Align::Max),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for column in 0..STEPS {
                cell_gap(ui, column);
                let (rect, response) =
                    ui.allocate_exact_size(vec2(STEP_CELL, RULER_HEIGHT), Sense::hover());
                let current = is_current(state, column);
                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    let centre = pos2(rect.center().x, rect.min.y + 8.0);
                    if column % 4 == 0 {
                        let text = (first_beat + column / 4 + 1).to_string();
                        neu::text::paint_text(
                            painter,
                            centre,
                            Align2::CENTER_CENTER,
                            &text,
                            TextKind::Micro,
                            theme.text_2,
                        );
                    } else {
                        painter.circle_filled(centre, 1.0, theme.text_3);
                    }
                    if current {
                        neu::step_cell::paint_playhead_triangle(
                            painter,
                            pos2(rect.center().x, rect.max.y - 3.0),
                            theme,
                        );
                    }
                }
                if column % 4 == 0 {
                    let text = (first_beat + column / 4 + 1).to_string();
                    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &text));
                }
            }
        },
    );
}

/// A row of sixteen cells, `ROW_PITCH` tall, with the wash when `selected`.
fn cells_row(
    ui: &mut Ui,
    theme: &Theme,
    width: f32,
    selected: bool,
    mut cell: impl FnMut(&mut Ui, usize),
) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_PITCH), Sense::hover());
    if selected && ui.is_rect_visible(rect) {
        paint_row_wash(ui, rect, theme, false);
    }
    let inner = Rect::from_min_max(
        pos2(rect.min.x, rect.min.y + (ROW_PITCH - STEP_CELL) * 0.5),
        pos2(rect.max.x, rect.max.y - (ROW_PITCH - STEP_CELL) * 0.5),
    );
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center))
            .id_salt((rect.min.y as i32, "cells")),
    );
    row.spacing_mut().item_spacing.x = 0.0;
    for column in 0..STEPS {
        cell_gap(&mut row, column);
        cell(&mut row, column);
    }
}

fn drum_cells(ui: &mut Ui, state: &mut AppState, theme: &Theme, row: DrumRow, width: f32) {
    cells_row(ui, theme, width, false, |ui, column| {
        let step = step_of(state, column);
        let cell = state.pattern().drums[row.index()][step];
        let spec = StepCellSpec {
            on: cell.on,
            accent: cell.accent,
            current: is_current(state, column),
            selected: false,
            label: None,
        };
        let name = cell_name(row_name(row), step);
        let response = neu::step_cell_at(ui, theme, &spec, &name, column, None);
        // Right-click clears the cell; a primary press starts the drag-paint stroke (Ctrl:
        // accent, Shift: shift-toggle); a running stroke paints the cell it reaches.
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
        response.on_hover_text("Click toggles · Shift-click accents · drag paints");
    });
}

fn track_cells(ui: &mut Ui, state: &mut AppState, theme: &Theme, slot: TrackId, width: f32) {
    let track = state.track(slot).clone();
    let colour = theme.track_colour(track.colour);
    let is_edit = slot == state.edit_track;
    cells_row(ui, theme, width, is_edit, |ui, column| {
        let step = step_of(state, column);
        let cell = lane_cell(state, slot, step);
        let text = cell.note.map(neu::note_name);
        let spec = StepCellSpec {
            on: cell.note.is_some(),
            accent: cell.accent,
            current: is_current(state, column),
            selected: is_edit && state.selected_step == Some(step),
            label: text.as_deref(),
        };
        let name = cell_name(&track.name, step);
        let response = neu::step_cell_at(ui, theme, &spec, &name, column, Some(colour));
        if response.secondary_clicked() {
            if !is_edit {
                state.select_track(slot);
            }
            state.clear_synth_step(step);
        } else if press_on(ui, &response) {
            if !is_edit {
                state.select_track(slot);
            }
            let mods = press_modifiers(ui);
            if mods.command || mods.ctrl {
                state.toggle_synth_accent(step);
            } else {
                state.click_synth_step(step, mods.shift);
            }
        }
        response.on_hover_text("Click selects the step (keys set its note) · Shift-click accents");
    });
}

// ---- The left block.

fn left_line(ui: &mut Ui, state: &mut AppState, theme: &Theme, line: Line, compact: bool) {
    match line {
        Line::Ruler => {
            ui.allocate_exact_size(vec2(ui.available_width(), RULER_HEIGHT), Sense::hover());
        }
        Line::DrumsHead => section_head(ui, theme, DRUMS_TITLE, None, "Vol Tune", compact),
        Line::TracksHead => {
            let count = format!("{} / {}", state.active_track_count(), engine::MAX_TRACKS);
            section_head(ui, theme, TRACKS_TITLE, Some(&count), "Vol Pan", compact);
        }
        Line::Drum(row) => drum_controls(ui, state, theme, row, compact),
        Line::Track(index, slot) => track_controls(ui, state, theme, index, slot, compact),
        Line::AddRow => add_row(ui, state, theme),
    }
}

/// A section head: the title in section type (the count in `text_3` after it) and the
/// column legends over the knob columns and the INS column.
fn section_head(
    ui: &mut Ui,
    theme: &Theme,
    title: &str,
    count: Option<&str>,
    knob_legend: &str,
    compact: bool,
) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), SECTION_HEAD), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, title));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let title_rect = neu::text::paint_text(
        painter,
        pos2(rect.min.x + ROW_PAD, rect.center().y),
        Align2::LEFT_CENTER,
        &TextKind::Section.display_string(title),
        TextKind::Section,
        theme.text_3,
    );
    if let Some(count) = count {
        neu::text::paint_text(
            painter,
            pos2(title_rect.max.x + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            count,
            TextKind::Small,
            theme.text_3,
        );
    }
    // Legends: over the two knobs (wide only) and over INS, right-aligned to their columns.
    let knobs_right = rect.min.x
        + ROW_PAD
        + LED_WIDTH
        + COL_GAP
        + SOLO_WIDTH
        + COL_GAP
        + KNOB_WIDTH
        + COL_GAP
        + KNOB_WIDTH;
    let ins_right = if compact {
        rect.min.x + ROW_PAD + LED_WIDTH + COL_GAP + SOLO_WIDTH + COL_GAP + INS_WIDTH
    } else {
        knobs_right + COL_GAP + INS_WIDTH
    };
    if !compact {
        neu::text::paint_text(
            painter,
            pos2(knobs_right, rect.center().y),
            Align2::RIGHT_CENTER,
            &TextKind::Hint.display_string(knob_legend),
            TextKind::Hint,
            theme.text_3,
        );
    }
    neu::text::paint_text(
        painter,
        pos2(ins_right, rect.center().y),
        Align2::RIGHT_CENTER,
        "INS",
        TextKind::Hint,
        theme.text_3,
    );
}

/// Allocates a `ROW_PITCH` row across the left block and lays `add` out left to right on its
/// 28 pt centre line with the row padding and gaps. `selected` paints the wash and the bar.
fn left_row(ui: &mut Ui, theme: &Theme, selected: bool, add: impl FnOnce(&mut Ui)) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_PITCH), Sense::hover());
    if selected && ui.is_rect_visible(rect) {
        paint_row_wash(ui, rect, theme, true);
    }
    let inner = Rect::from_min_max(
        pos2(
            rect.min.x + ROW_PAD,
            rect.min.y + (ROW_PITCH - ROW_HEIGHT) * 0.5,
        ),
        pos2(rect.max.x, rect.max.y - (ROW_PITCH - ROW_HEIGHT) * 0.5),
    );
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center))
            .id_salt((rect.min.y as i32, "controls")),
    );
    row.spacing_mut().item_spacing = vec2(COL_GAP, 0.0);
    add(&mut row);
    rect
}

/// A mini knob (22) bound to `value`: returns the new value when it moved and whether this
/// frame started a drag. Named `name`; the tooltip reads `note: value`.
#[allow(clippy::too_many_arguments)]
fn row_knob(
    ui: &mut Ui,
    theme: &Theme,
    value: f32,
    min: f32,
    max: f32,
    default: f32,
    bipolar: bool,
    fmt: fn(f32) -> String,
    name: &str,
    note: &str,
) -> Option<(f32, bool)> {
    let spec = KnobSpec {
        label: "",
        min,
        max,
        default,
        log: false,
        bipolar,
        size: KnobSize::Sm,
        fmt,
    };
    let mut v = value;
    let tip = format!("{note}: {}", fmt(value));
    let response = neu::knob_noted(ui, theme, &mut v, &spec, Some(&tip));
    let shown = f64::from(v);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), shown, name));
    (response.changed() || v != value).then_some((v, response.drag_started()))
}

/// The 22 x 22 insert number box reading `box_text(route)`; a click opens the route picker.
fn insert_box(ui: &mut Ui, theme: &Theme, route: u8, name: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(INS_WIDTH, INS_WIDTH), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = neu::control_painter(ui);
        let fill = if response.hovered() {
            theme.panel_3
        } else {
            theme.panel_2
        };
        neu::paint_control(&painter, rect, 3.0, fill, theme.border_2);
        neu::text::paint_text(
            &painter,
            rect.center(),
            Align2::CENTER_CENTER,
            &crate::ui::mixer::box_text(route),
            TextKind::Small,
            theme.text,
        );
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), name));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// The LED: on while audible; a click mutes / unmutes, a right-click solos.
fn row_led(ui: &mut Ui, theme: &Theme, audible: bool, name: &str, tip: &str) -> Response {
    neu::led_named(ui, theme, audible, LedKind::Ok, name).on_hover_text(tip)
}

fn drum_controls(ui: &mut Ui, state: &mut AppState, theme: &Theme, row: DrumRow, compact: bool) {
    let label = row_label(row);
    let paint_row = PaintRow::Drum(row);
    let channel = crate::qol::row_channel(paint_row, state.edit_track);
    let muted = state.row_muted(paint_row);
    let soloed = state.row_soloed(paint_row);
    let volume = state.pattern().drum_volume[row.index()];
    let tune = state.pattern().drum_tune[row.index()];
    left_row(ui, theme, false, |ui| {
        let tip = format!(
            "{} · {} · {}{}",
            row_name(row),
            fmt_row_db(volume),
            fmt_semitones(tune),
            if muted { " · muted" } else { "" }
        );
        let led = row_led(ui, theme, !muted, &tracks::led_name(label), &tip);
        if led.clicked() {
            state.toggle_row_mute(paint_row);
        } else if led.secondary_clicked() {
            state.toggle_row_solo(paint_row);
        }
        let solo = neu::button_kind(ui, theme, "S", ButtonKind::Solo(soloed), ButtonSize::Small);
        let solo_name = tracks::drum_solo_name(label);
        solo.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), soloed, &solo_name)
        });
        if solo.on_hover_text("Solo").clicked() {
            state.toggle_row_solo(paint_row);
        }
        if !compact {
            if let Some((db, drag_started)) = row_knob(
                ui,
                theme,
                volume,
                -24.0,
                6.0,
                0.0,
                false,
                fmt_row_db,
                &tracks::drum_volume_name(label),
                "Vol",
            ) {
                if drag_started {
                    state.begin_continuous_edit(EditKey::DrumVolume(row), true);
                }
                state.set_drum_volume(row, db);
            }
            if let Some((st, drag_started)) = row_knob(
                ui,
                theme,
                tune,
                -12.0,
                12.0,
                0.0,
                true,
                fmt_semitones,
                &tracks::drum_tune_name(label),
                "Tune",
            ) {
                if drag_started {
                    state.begin_continuous_edit(EditKey::DrumTune(row), true);
                }
                state.set_drum_tune(row, st);
            }
        }
        let route = state.mixer.route(channel);
        let box_response = insert_box(ui, theme, route, &row_box_name(label)).on_hover_text(
            format!("Mixer insert: {} · click to pick", state.route_name(route)),
        );
        if let Some(picked) = crate::ui::mixer::route_picker(
            theme,
            &box_response,
            state,
            route,
            &crate::ui::mixer::source_route_choices(),
        ) {
            state.set_route(channel, picked);
            state.select_insert(picked, false);
        }
        let name_size = vec2(
            if compact {
                NAME_WIDTH_COMPACT
            } else {
                NAME_WIDTH
            },
            24.0,
        );
        let name = neu::name_button_sized(ui, theme, label, None, false, muted, name_size);
        let state_text = match (muted, soloed) {
            (true, true) => "Muted, solo · ",
            (true, false) => "Muted · ",
            (false, true) => "Solo · ",
            (false, false) => "",
        };
        let name = name.on_hover_text(format!(
            "{state_text}Double-click: mute · Shift + double-click: solo · right-click: menu"
        ));
        if name.double_clicked() {
            if press_modifiers(ui).shift {
                state.toggle_row_solo(paint_row);
            } else {
                state.toggle_row_mute(paint_row);
            }
        }
        let items = [
            MenuItem::item(if muted {
                tracks::UNMUTE_LABEL
            } else {
                tracks::MUTE_LABEL
            }),
            MenuItem::item(if soloed {
                tracks::UNSOLO_LABEL
            } else {
                tracks::SOLO_LABEL
            }),
            MenuItem::Separator,
            MenuItem::item(tracks::CLEAR_ROW_LABEL),
        ];
        match context_menu(ui, theme, &name, &items) {
            Some(0) => state.toggle_row_mute(paint_row),
            Some(1) => state.toggle_row_solo(paint_row),
            Some(3) => state.clear_row(row),
            _ => {}
        }
    });
}

fn track_controls(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    index: usize,
    slot: TrackId,
    compact: bool,
) {
    let track = state.track(slot).clone();
    let channel = Channel::from_track(slot);
    let selected = slot == state.edit_track;
    let muted = state.mixer.strip(channel).mute;
    let soloed = state.mixer.strip(channel).solo;
    let level = state.mixer.get(channel, ChannelParam::Level);
    let pan = state.mixer.get(channel, ChannelParam::Pan);
    let colour = theme.track_colour(track.colour);
    left_row(ui, theme, selected, |ui| {
        let tip = format!(
            "{} · {} · {} · {}{}",
            track.name,
            track.preset_name(),
            fmt_level_db(level),
            fmt_pan(pan),
            if muted { " · muted" } else { "" }
        );
        let led = row_led(ui, theme, !muted, &tracks::led_name(&track.name), &tip);
        if led.clicked() {
            state.toggle_mute(channel);
        } else if led.secondary_clicked() {
            state.toggle_solo(channel);
        }
        let solo = neu::button_kind(ui, theme, "S", ButtonKind::Solo(soloed), ButtonSize::Small);
        let solo_name = tracks::solo_name(&track.name);
        solo.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), soloed, &solo_name)
        });
        if solo.on_hover_text(format!("Solo {}", track.name)).clicked() {
            state.toggle_solo(channel);
        }
        if !compact {
            let level_range = ChannelParam::Level.range();
            if let Some((db, drag_started)) = row_knob(
                ui,
                theme,
                level,
                *level_range.start(),
                *level_range.end(),
                ChannelParam::Level.default_value(channel),
                false,
                fmt_level_db,
                &tracks::level_name(&track.name),
                "Vol",
            ) {
                state.edit_mixer(channel, ChannelParam::Level, db, drag_started);
            }
            let pan_range = ChannelParam::Pan.range();
            if let Some((p, drag_started)) = row_knob(
                ui,
                theme,
                pan,
                *pan_range.start(),
                *pan_range.end(),
                ChannelParam::Pan.default_value(channel),
                true,
                fmt_pan,
                &tracks::pan_name(&track.name),
                "Pan",
            ) {
                state.edit_mixer(channel, ChannelParam::Pan, p, drag_started);
            }
        }
        let route = state.mixer.route(channel);
        let box_response = insert_box(ui, theme, route, &tracks::insert_box_name(&track.name))
            .on_hover_text(format!(
                "Mixer insert: {} · click to pick ({} links a free one)",
                state.route_name(route),
                Shortcut::LinkInsert.label()
            ));
        if let Some(picked) = crate::ui::mixer::route_picker(
            theme,
            &box_response,
            state,
            route,
            &crate::ui::mixer::source_route_choices(),
        ) {
            state.set_route(channel, picked);
            state.select_insert(picked, false);
        }
        // The NAME button: the row's button (selected = the edit track).
        let name_size = vec2(
            if compact {
                NAME_WIDTH_COMPACT
            } else {
                NAME_WIDTH
            },
            24.0,
        );
        let name = neu::name_button_sized(
            ui,
            theme,
            &track.name,
            Some(colour),
            selected,
            muted,
            name_size,
        );
        let row_name = tracks::row_name(index, &track.name);
        name.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, &row_name)
        });
        let name = name
            .on_hover_text(format!(
                "{} · {}\nClick selects · double-click renames · right-click: menu",
                track.name,
                track.preset_name()
            ))
            .on_hover_cursor(CursorIcon::PointingHand);
        if name.double_clicked() {
            if press_modifiers(ui).shift {
                state.toggle_solo(channel);
            } else {
                neu::begin_edit(ui, tracks::rename_id(slot), &track.name);
            }
        } else if name.clicked() {
            state.select_track(slot);
        }
        // The inline rename editor over the name.
        let font = egui::FontId::proportional(12.0);
        match neu::show_edit(
            ui,
            theme,
            tracks::rename_id(slot),
            name.rect,
            font,
            Align::Min,
        ) {
            ValueEditOutput::Committed(typed) => {
                state.rename_track(slot, &typed);
            }
            ValueEditOutput::Editing | ValueEditOutput::Idle | ValueEditOutput::Cancelled => {}
        }
        // The row menu.
        let slots: Vec<TrackId> = state.active_tracks().collect();
        let position = slots.iter().position(|s| *s == slot).unwrap_or(0);
        let can_up = position > 0;
        let can_down = position + 1 < slots.len();
        let can_delete = slots.len() > 1;
        let can_dup = state.free_track_slot().is_some();
        let enabled = |label, on| {
            if on {
                MenuItem::item(label)
            } else {
                MenuItem::disabled(label, None)
            }
        };
        let items = [
            MenuItem::item(tracks::RENAME_LABEL),
            enabled(tracks::DUP_LABEL, can_dup),
            enabled(tracks::DEL_LABEL, can_delete),
            MenuItem::Separator,
            enabled(tracks::MOVE_UP_NAME, can_up),
            enabled(tracks::MOVE_DOWN_NAME, can_down),
            MenuItem::item(tracks::COLOUR_LABEL),
            MenuItem::Separator,
            MenuItem::keyed(tracks::LINK_NAME, Shortcut::LinkInsert.label()),
            MenuItem::item(tracks::CLEAR_NOTES_LABEL),
        ];
        match context_menu(ui, theme, &name, &items) {
            Some(0) => neu::begin_edit(ui, tracks::rename_id(slot), &track.name),
            Some(1) => {
                state.duplicate_track(slot);
            }
            Some(2) => tracks::set_pending_delete(ui, Some(slot)),
            Some(4) => {
                state.move_track(slot, true);
            }
            Some(5) => {
                state.move_track(slot, false);
            }
            Some(6) => {
                state.set_track_colour(slot, (track.colour + 1) % COLOUR_COUNT);
            }
            Some(8) => {
                if state.link_track_to_free_insert(slot).is_some() {
                    state.set_view(View::Mixer);
                }
            }
            Some(9) => {
                state.select_track(slot);
                state.clear_notes();
            }
            _ => {}
        }
    });
}

/// Opens `items` as a context menu under a right-click on `anchor`; returns the entry
/// picked this frame.
fn context_menu(
    ui: &Ui,
    theme: &Theme,
    anchor: &Response,
    items: &[MenuItem<'_>],
) -> Option<usize> {
    let _ = ui;
    egui::Popup::context_menu(anchor)
        .frame(Frame::NONE)
        .gap(4.0)
        .show(|ui| neu::menu(ui, theme, items))
        .and_then(|r| r.inner)
}

/// The add row: the small plus icon, "Add track" and the Ctrl+T keycap, or the pending
/// "Delete <name>?" confirm with YES / NO.
fn add_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ADD_ROW), Sense::hover());
    let inner = Rect::from_min_max(pos2(rect.min.x + ROW_PAD, rect.min.y), rect.max);
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center))
            .id_salt("rack_add_row"),
    );
    row.spacing_mut().item_spacing = vec2(COL_GAP, 0.0);
    let ui = &mut row;
    if let Some(slot) = tracks::pending_delete(ui) {
        let question = tracks::delete_question(&state.track(slot).name);
        neu::text_widget(ui, &question, TextKind::Label, theme.text);
        if neu::button_kind(
            ui,
            theme,
            tracks::YES_LABEL,
            ButtonKind::Primary,
            ButtonSize::Small,
        )
        .on_hover_text("Delete the track and its notes (Ctrl+Z brings it back)")
        .clicked()
        {
            state.remove_track(slot);
            tracks::set_pending_delete(ui, None);
        }
        if neu::button_kind(
            ui,
            theme,
            tracks::NO_LABEL,
            ButtonKind::Secondary,
            ButtonSize::Small,
        )
        .on_hover_text("Keep the track")
        .clicked()
        {
            tracks::set_pending_delete(ui, None);
        }
        return;
    }
    let add = neu::icon_button_kind(
        ui,
        theme,
        Icon::Plus,
        vec2(22.0, 22.0),
        ButtonKind::Secondary,
    );
    add.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), tracks::ADD_NAME));
    if add
        .on_hover_text(format!(
            "New track with the default preset ({}) · {}",
            Shortcut::AddTrack.label(),
            tracks::HINT
        ))
        .clicked()
    {
        state.add_track();
    }
    // Painted, not a widget: the button already carries the accessible name.
    let galley = neu::text::layout(
        ui.painter(),
        tracks::ADD_LABEL,
        TextKind::Label,
        theme.text_2,
    );
    let (text_rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    if ui.is_rect_visible(text_rect) {
        neu::text::paint_galley(
            ui.painter(),
            text_rect.center(),
            Align2::CENTER_CENTER,
            galley,
            TextKind::Label,
            theme.text_2,
        );
    }
    neu::keycap(ui, theme, Shortcut::AddTrack.label());
}

// ---------------------------------------------------------------------------------------------
// Input helpers.

/// The modifiers of a press this frame: the raw modifier state, or the modifiers carried by
/// this frame's pointer-button events (synthetic clicks under `egui_kittest` deliver them
/// only on the event).
pub fn press_modifiers(ui: &Ui) -> Modifiers {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_positions_follow_the_deck_maths() {
        assert_eq!(cell_x(0), 0.0);
        assert_eq!(cell_x(1), 26.0);
        assert_eq!(cell_x(3), 78.0);
        assert_eq!(cell_x(4), 108.0);
        assert_eq!(cell_x(15) + STEP_CELL, GRID_WIDTH);
        assert_eq!(GRID_WIDTH, 426.0);
    }

    #[test]
    fn the_row_budget_matches_winner() {
        // LED 10 · S 22 · knob 22 · knob 22 · INS 22 · NAME 76 · STEPS 426 with padding 8 and
        // gaps 6: 652 content; 10 scrollbar + 2 border = 664.
        assert_eq!(CONTENT_WIDTH, 652.0);
        assert_eq!(
            CONTENT_WIDTH + neu::SCROLLBAR_WIDTH + 2.0,
            crate::app::DEFAULT_RACK_WIDTH
        );
        // Compact: no knobs, name 64, a 210 steps viewport: inside the 400 narrow rack.
        let compact = CONTENT_WIDTH_COMPACT + neu::SCROLLBAR_WIDTH + 2.0;
        let narrow_rack = crate::app::MIN_RACK_WIDTH;
        assert!(compact <= narrow_rack, "{compact} inside {narrow_rack}");
        assert!(is_compact(400.0) && !is_compact(664.0));
    }

    #[test]
    fn names_and_formatters() {
        assert_eq!(cell_name("Kick", 0), "Kick step 1");
        assert_eq!(cell_name("Synth", 15), "Synth step 16");
        assert_eq!(cell_name("Kick", 16), "Kick step 17");
        assert_eq!(queued_name(1), "Queued B");
        assert_eq!(tile_name(2), "Pattern C");
        assert_eq!(row_name(DrumRow::ClosedHat), "Closed hat");
        assert_eq!(row_box_name("Kick"), "Row insert Kick");
        assert_eq!(fmt_row_db(0.0), "0 dB");
        assert_eq!(fmt_row_db(-6.0), "-6 dB");
        assert_eq!(fmt_row_db(3.0), "+3 dB");
        assert_eq!(fmt_semitones(0.0), "+0 st");
        assert_eq!(fmt_semitones(-3.0), "-3 st");
        assert_eq!(length_items(), ["1 bar", "2 bars", "4 bars"]);
        assert_eq!(PATTERN_TABS.len(), PATTERN_COUNT);
    }

    #[test]
    fn the_lane_cell_of_another_track_is_its_lowest_note() {
        let (mut state, _engine) = AppState::headless(neu::ThemeMode::Dark);
        state.add_track();
        // Track 2 is the edit track; track 1 keeps the demo bass line.
        assert_eq!(state.edit_track, 1);
        let demo = engine::Pattern::demo();
        for step in 0..STEPS {
            assert_eq!(lane_cell(&state, 0, step), demo.synth[step], "step {step}");
        }
        assert_eq!(lane_cell(&state, 1, 0), SynthStep::default());
        assert!(body_height(&state) > 7.0 * ROW_PITCH);
        assert!(header_sub(&state).starts_with("Pattern A · 1 bar · Track 2 · "));
    }
}
