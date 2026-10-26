//! SONG CHAIN panel (M5): the song chain as a list of entries, with the controls for the
//! selected entry.
//!
//! The SONG view's panel since M12 (rehosted in the SONG window until its M14 timeline; the
//! transport's Pattern | Song switch shows the view). Built from the kit, top to bottom:
//!
//! - head row: the title;
//! - the chain: one pill per entry ("A ×2", wrapping, scrolling when long). The entry the
//!   engine is playing (song mode, transport running) is the ON pill (OnGlow); the selected
//!   entry has an accent ring. A click selects;
//! - the selected entry's pattern (`neu::segmented` A..H);
//! - its REPEATS `stepper` (1..=8) beside the `<` / `>` move pills;
//! - ADD (appends the edit pattern once) and REMOVE pills.
//!
//! Everything goes through `AppState` (`add_chain_entry`, `remove_chain_entry`,
//! `set_chain_pattern`, `set_chain_repeats`, `move_chain_entry`), which sends `SetChainEntry`
//! / `SetChainLen` and records one undo step per change.

use egui::{Align, Layout, RichText, Role, ScrollArea, Stroke, Ui, WidgetInfo, vec2};
use engine::pattern::MAX_REPEATS;
use engine::{ChainEntry, PlayMode};
use neu::Theme;

use crate::app::AppState;
use crate::ui::controls::{self, GROUP_GAP, PANEL_HEAD_GAP};
use crate::ui::sequencer::PATTERN_TABS;

/// Gap between pills in a row.
const PILL_GAP: f32 = 8.0;
/// Vertical gap between rows of chain pills.
const CHAIN_ROW_GAP: f32 = 6.0;
/// Width of the selection ring around the selected entry's pill.
const SELECT_RING: f32 = 2.0;
/// Height of the REPEATS row (`neu::stepper`: label + 28 pt buttons).
const STEPPER_ROW: f32 = 48.0;
/// The pattern-selector row, the stepper row, the ADD / REMOVE row and the gaps between them.
const CONTROLS_HEIGHT: f32 =
    GROUP_GAP + controls::ROW_HEIGHT + GROUP_GAP + STEPPER_ROW + GROUP_GAP + controls::ROW_HEIGHT;
/// Shown when the chain is empty.
pub const EMPTY_CHAIN_LABEL: &str = "Empty chain: song mode plays the current pattern";

/// Visible text of a chain entry pill: `"A ×2"`.
#[must_use]
pub fn entry_text(entry: ChainEntry) -> String {
    format!("{} ×{}", entry.letter(), entry.repeats)
}

/// Accessible name of chain entry `index` (0-based): `"Entry 1: A x2"`.
#[must_use]
pub fn entry_name(index: usize, entry: ChainEntry) -> String {
    format!("Entry {}: {} x{}", index + 1, entry.letter(), entry.repeats)
}

/// Renders the SONG CHAIN panel into `ui`, filling the space it is given.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        // The gaps below are explicit (the row pitch must fit the sequencer row beside it).
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            ui.set_min_height(controls::ROW_HEIGHT);
            neu::title(ui, theme, "Song chain");
        });
        ui.add_space(PANEL_HEAD_GAP);
        let chain_height = (ui.available_height() - CONTROLS_HEIGHT).max(controls::ROW_HEIGHT);
        chain_list(ui, state, theme, chain_height);
        ui.add_space(GROUP_GAP);
        pattern_selector(ui, state, theme);
        ui.add_space(GROUP_GAP);
        repeats_row(ui, state, theme);
        ui.add_space(GROUP_GAP);
        add_remove_row(ui, state, theme);
    });
}

/// The chain entries as wrapping pills inside a scroll area of `height`.
fn chain_list(ui: &mut Ui, state: &mut AppState, theme: &Theme, height: f32) {
    let playing = (state.bank.mode == PlayMode::Song && state.playhead.playing)
        .then_some(usize::from(state.playhead.song_index));
    let entries: Vec<ChainEntry> = state.chain().to_vec();
    let selected = state.song_selected;
    let mut clicked = None;
    ScrollArea::vertical()
        .id_salt("song_chain")
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_min_height(height);
            if entries.is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(EMPTY_CHAIN_LABEL)
                            .size(12.0)
                            .color(theme.text_secondary),
                    )
                    .wrap(),
                );
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(PILL_GAP, CHAIN_ROW_GAP);
                for (index, entry) in entries.iter().enumerate() {
                    let text = entry_text(*entry);
                    let name = entry_name(index, *entry);
                    let response = neu::toggle_button(ui, theme, &text, playing == Some(index));
                    if selected == Some(index) && ui.is_rect_visible(response.rect) {
                        ui.painter().rect_stroke(
                            response.rect.expand(SELECT_RING + 1.0),
                            theme.radius_pill,
                            Stroke::new(SELECT_RING, theme.accent),
                            egui::StrokeKind::Outside,
                        );
                    }
                    response.widget_info(|| {
                        WidgetInfo::selected(
                            Role::Button,
                            ui.is_enabled(),
                            selected == Some(index),
                            &name,
                        )
                    });
                    if response.clicked() {
                        clicked = Some(index);
                    }
                }
            });
        });
    if let Some(index) = clicked {
        state.select_chain_entry(index);
    }
}

/// A..H for the selected entry (disabled while nothing is selected).
fn pattern_selector(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let selected_entry = state.selected_entry();
    let before = selected_entry.map_or(0, |(_, e)| usize::from(e.pattern) % PATTERN_TABS.len());
    let mut selected = before;
    ui.add_enabled_ui(selected_entry.is_some(), |ui| {
        neu::segmented(ui, theme, &mut selected, &PATTERN_TABS)
            .on_hover_text("Pattern of the selected entry");
    });
    if selected != before {
        state.set_chain_pattern(selected as u8);
    }
}

/// REPEATS stepper (1..=8) and the `<` / `>` move pills for the selected entry.
fn repeats_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), STEPPER_ROW),
        Layout::left_to_right(Align::Max),
        |ui| {
            ui.spacing_mut().item_spacing.x = PILL_GAP;
            let entry = state.selected_entry();
            let mut repeats = entry.map_or(1, |(_, e)| i32::from(e.repeats));
            let changed = ui
                .add_enabled_ui(entry.is_some(), |ui| {
                    neu::stepper(
                        ui,
                        theme,
                        "Repeats",
                        &mut repeats,
                        1..=i32::from(MAX_REPEATS),
                    )
                    .changed()
                })
                .inner;
            if changed {
                state.set_chain_repeats(repeats as u8);
            }
            let len = state.chain().len();
            let index = entry.map(|(i, _)| i);
            let can_left = index.is_some_and(|i| i > 0);
            let can_right = index.is_some_and(|i| i + 1 < len);
            // The pills sit on the stepper's button line.
            ui.add_space(PILL_GAP);
            let left = ui
                .add_enabled_ui(can_left, |ui| {
                    neu::button(ui, theme, "<")
                        .on_hover_text("Move the entry earlier")
                        .clicked()
                })
                .inner;
            if left {
                state.move_chain_entry(-1);
            }
            let right = ui
                .add_enabled_ui(can_right, |ui| {
                    neu::button(ui, theme, ">")
                        .on_hover_text("Move the entry later")
                        .clicked()
                })
                .inner;
            if right {
                state.move_chain_entry(1);
            }
        },
    );
}

/// ADD (append the edit pattern once) | REMOVE (the selected entry).
fn add_remove_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        if neu::button(ui, theme, "Add")
            .on_hover_text(format!(
                "Append pattern {} to the chain",
                state.current_letter()
            ))
            .clicked()
        {
            state.add_chain_entry();
        }
        let remove = ui
            .add_enabled_ui(state.selected_entry().is_some(), |ui| {
                neu::button(ui, theme, "Remove")
                    .on_hover_text("Remove the selected entry")
                    .clicked()
            })
            .inner;
        if remove {
            state.remove_chain_entry();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_texts() {
        assert_eq!(entry_text(ChainEntry::new(1, 2)), "B ×2");
        assert_eq!(entry_name(0, ChainEntry::new(0, 1)), "Entry 1: A x1");
        assert_eq!(entry_name(4, ChainEntry::new(7, 8)), "Entry 5: H x8");
    }
}
