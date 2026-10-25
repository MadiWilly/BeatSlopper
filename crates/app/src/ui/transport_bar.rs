//! Transport bar: wordmark, test-tone toggle, the MASTER dial, theme toggle.
//!
//! Layout follows `design/preview-a.html` `<header class="transport">`: brand on the left
//! (name 15/300 caps, sub-label 11 secondary), controls in groups, the large concentric dial
//! (`neu::dial` Lg bound to `MasterGain`, shown in %), theme toggle flush right (sun, switch,
//! moon). Play/stop/BPM/swing arrive in M3.
//!
//! The row is as tall as the dial. egui's horizontal layout pushes items taller than the row
//! *down* rather than centring everything on them, so the dial's height from the previous
//! frame sets the row height and the shorter groups centre on it.

use egui::{Align, Layout, RichText, Ui};
use engine::ParamId;
use neu::{DialSize, Theme};

use crate::app::AppState;
use crate::ui::controls::{Knob, fmt_percent};

/// Minimum height of the transport bar's content row (the dial normally sets it).
const ROW_HEIGHT: f32 = 48.0;

/// Renders the transport bar into `ui` and applies any control changes to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        let height_id = ui.scope_id().with("transport_row_height");
        let row_height: f32 = ui
            .data(|d| d.get_temp(height_id))
            .unwrap_or(ROW_HEIGHT)
            .max(ROW_HEIGHT);
        ui.set_min_height(row_height);
        ui.horizontal(|ui| {
            ui.set_min_height(row_height);
            ui.spacing_mut().item_spacing.x = theme.space_6;

            brand(ui, theme, state.audio_error.as_deref());

            ui.add_space(theme.space_2);
            test_tone_group(ui, state, theme);
            let dial = master_dial(ui, state, theme);
            ui.data_mut(|d| d.insert_temp(height_id, dial.rect.height()));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                theme_group(ui, state, theme);
            });
        });
    });
}

fn label(ui: &mut Ui, theme: &Theme, text: &str) -> egui::Response {
    ui.label(RichText::new(text).size(11.0).color(theme.text_secondary))
}

fn brand(ui: &mut Ui, theme: &Theme, audio_error: Option<&str>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.space_1;
        ui.label(
            RichText::new("BEATSLOPPER")
                .size(15.0)
                .color(theme.text_primary),
        );
        match audio_error {
            None => label(ui, theme, "M2 · SOFT CONSOLE"),
            Some(_) => label(ui, theme, "NO AUDIO DEVICE"),
        };
    });
}

fn test_tone_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.space_1;
        label(ui, theme, "ENGINE");
        if neu::toggle_button(ui, theme, "Test tone", state.test_tone).clicked() {
            state.toggle_test_tone();
        }
    });
}

/// The hero control: the large dial bound to `MasterGain` (label MASTER, value in %).
fn master_dial(ui: &mut Ui, state: &mut AppState, theme: &Theme) -> egui::Response {
    Knob::new(ParamId::MasterGain, fmt_percent)
        .label("Master")
        .show_dial(ui, state, theme, DialSize::Lg)
}

fn theme_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    // The switch paints its own sun/moon icons; flush right in the bar.
    neu::theme_toggle(ui, theme, &mut state.mode);
}
