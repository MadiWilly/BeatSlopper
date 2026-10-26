//! VISUALIZER: the VISUALIZER view's panel and the full overlay (F11). Both draw the same
//! chosen preset ([`AppState::visual_preset`], `neu::PRESETS`) from the same per-frame feed
//! ([`AppState::visual_input`]).
//!
//! - [`show`]: the panel (rehosted in the VISUALIZER window until its M14 restyle beside
//!   the SONG window): the title row with the FULL toggle flush right, a PRESET stepper
//!   (`(-) Soft Scope (+)`, wrapping) and the drawing filling the rest.
//! - [`overlay`]: with [`AppState::visual_full`] set, a foreground `Area` the size of the
//!   workspace ([`AppState::workspace_rect`]: the rack and the main window) holding a panel
//!   with the title, the preset pills, a CLOSE button and the drawing. Escape (handled in
//!   `AppState::handle_keyboard`), F11 or CLOSE put it away.
//!
//! When the preset is "Off" the panel and the overlay show a panel-2 well with "VISUALIZER
//! OFF" centred in it. The transport's mini strip of the Soft Console page is gone (the Deck
//! toolbar has no room for it).

use egui::{Align, Align2, Area, Layout, Order, Ui, vec2};
use neu::{PRESETS, TextKind, Theme};

use crate::app::AppState;
use crate::ui::controls::{self, GROUP_GAP, PANEL_HEAD_GAP, ROW_HEIGHT};

/// The FULL toggle's label.
pub const FULL_LABEL: &str = "Full";
/// The panel's preset stepper label.
pub const PRESET_LABEL: &str = "Preset";
/// The overlay's CLOSE button label.
pub const CLOSE_LABEL: &str = "Close";
/// Placeholder text while the preset is "Off".
pub const OFF_TEXT: &str = "Visualizer off";
/// Gap between preset pills.
pub const PILL_GAP: f32 = 8.0;

/// The VISUALIZER panel, filling the space it is given: title row with the FULL toggle flush
/// right, the PRESET stepper (`(-) Soft Scope (+)`, wrapping at both ends), then the drawing
/// in the rest of the panel.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            ui.set_min_height(ROW_HEIGHT);
            neu::title(ui, theme, "Visualizer");
        });
        ui.add_space(PANEL_HEAD_GAP);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = PILL_GAP;
            preset_stepper(ui, state, theme);
            let full = state.visual_full;
            if controls::right_aligned(ui, "visualizer_full", |ui| {
                neu::toggle_button(ui, theme, FULL_LABEL, full)
                    .on_hover_text("Cover the workspace with the visualizer (F11; Escape closes)")
                    .clicked()
            }) {
                state.toggle_visual_full();
            }
        });
        ui.add_space(GROUP_GAP);
        draw(ui, state, theme);
    });
}

/// The panel's one-row preset selector: a stepper whose value is the preset name. Stepping
/// past either end wraps around the table.
pub fn preset_stepper(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let count = PRESETS.len() as i32;
    let before = state.visual_preset as i32;
    // One extra slot on each side so the stepper's clamp never blocks a wrap.
    let mut index = before + 1;
    let response = neu::stepper_with(ui, theme, PRESET_LABEL, &mut index, 0..=count + 1, |v| {
        PRESETS[(v - 1).rem_euclid(count) as usize].name.to_owned()
    });
    if response.changed() || index != before + 1 {
        state.set_visual_preset((index - 1).rem_euclid(count) as usize);
    }
}

/// One toggle pill per preset; the chosen one is ON. A click chooses.
pub fn preset_pills(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(PILL_GAP, 6.0);
        for (i, preset) in PRESETS.iter().enumerate() {
            if neu::toggle_button(ui, theme, preset.name, i == state.visual_preset).clicked() {
                state.set_visual_preset(i);
            }
        }
    });
}

/// Draws the chosen preset into the available space (or the "Off" placeholder).
pub fn draw(ui: &mut Ui, state: &AppState, theme: &Theme) {
    let preset = state.visual_preset();
    if preset.is_off() {
        off_placeholder(ui, theme);
        return;
    }
    let time = ui.input(|i| i.time);
    let input = state.visual_input(time);
    preset.show(ui, theme, &input);
}

/// A panel-2 well with "VISUALIZER OFF" centred in it.
fn off_placeholder(ui: &mut Ui, theme: &Theme) {
    let size = ui.available_size_before_wrap().max(vec2(40.0, 20.0));
    let (rect, _) = neu::well(ui, theme, size);
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            TextKind::Label.display_string(OFF_TEXT),
            TextKind::Label.font_id(),
            theme.text_2,
        );
    }
}

/// The full overlay over the workspace (see the module docs). Call once per frame after the
/// shell while [`AppState::visual_full`] is set.
pub fn overlay(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let rect = state
        .workspace_rect
        .unwrap_or_else(|| ui.max_rect().shrink(40.0));
    Area::new(ui.scope_id().with("visualizer_overlay"))
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            ui.allocate_ui_with_layout(rect.size(), Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(rect.size());
                ui.set_max_size(rect.size());
                neu::panel(ui, theme, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.horizontal(|ui| {
                        ui.set_min_height(ROW_HEIGHT);
                        neu::title(ui, theme, "Visualizer");
                        controls::right_aligned(ui, "visualizer_overlay_close", |ui| {
                            if neu::button(ui, theme, CLOSE_LABEL)
                                .on_hover_text("Close the full visualizer (Escape, F11)")
                                .clicked()
                            {
                                state.set_visual_full(false);
                            }
                        });
                    });
                    ui.add_space(PANEL_HEAD_GAP);
                    preset_pills(ui, state, theme);
                    ui.add_space(GROUP_GAP);
                    draw(ui, state, theme);
                });
            });
        });
}
