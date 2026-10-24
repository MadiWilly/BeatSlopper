//! Transport bar (M0): wordmark, test-tone toggle, master gain, theme toggle.
//!
//! Layout follows `design/preview-a.html` `<header class="transport">`: brand on the left
//! (name 15/300 caps, sub-label 11 secondary), controls in groups, theme toggle flush right
//! (sun, switch, moon). Play/stop/BPM/swing and the master dial arrive in M2/M3.

use egui::{Align, Layout, RichText, Ui};
use neu::Theme;

use crate::app::AppState;

/// Minimum height of the transport bar's content row.
const ROW_HEIGHT: f32 = 48.0;

/// Renders the transport bar into `ui` and applies any control changes to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.horizontal(|ui| {
            ui.set_min_height(ROW_HEIGHT);
            ui.spacing_mut().item_spacing.x = theme.space_6;

            brand(ui, theme, state.audio_error.as_deref());

            ui.add_space(theme.space_2);
            test_tone_group(ui, state, theme);
            master_gain_group(ui, state, theme);

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
            None => label(ui, theme, "M0 · SOFT CONSOLE"),
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

fn master_gain_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.space_1;
        let master_label = label(ui, theme, "MASTER");
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = 140.0;
            let slider = egui::Slider::new(&mut state.master_gain, 0.0..=1.0)
                .show_value(false)
                .trailing_fill(true);
            if ui.add(slider).labelled_by(master_label.id).changed() {
                state.send_master_gain();
            }
            ui.label(
                RichText::new(format!(
                    "{:>3}%",
                    (state.master_gain * 100.0).round() as i32
                ))
                .size(13.0)
                .color(theme.text_primary),
            );
        });
    });
}

fn theme_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    // The switch paints its own sun/moon icons; flush right in the bar.
    neu::theme_toggle(ui, theme, &mut state.mode);
}
