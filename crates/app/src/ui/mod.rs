//! UI panels. Each panel is a plain function over `&mut Ui`, `&mut AppState` and `&Theme`,
//! so `egui_kittest` can render it inside any `Ui` without a window or an audio device.
//! [`show`] lays them out in the real window.
//!
//! Page layout (design/WINNER.md): page padding 12 20 16, page gap 10, panels radius 24.

use egui::{CentralPanel, Frame, Margin, Panel, Ui};
use neu::Theme;

use crate::app::AppState;

pub mod transport_bar;

/// Page padding: top 12, sides 20, bottom 16.
pub const PAGE_PAD_TOP: i8 = 12;
pub const PAGE_PAD_SIDE: i8 = 20;
pub const PAGE_PAD_BOTTOM: i8 = 16;
/// Gap between the transport bar and the main area.
pub const PAGE_GAP: i8 = 10;

/// Full-window layout: transport bar on top, main area below. `ui` is the root `Ui` eframe
/// hands to `App::ui` (or any `Ui` under `egui_kittest`).
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    Panel::top("transport")
        .frame(Frame::NONE.fill(theme.surface).inner_margin(Margin {
            left: PAGE_PAD_SIDE,
            right: PAGE_PAD_SIDE,
            top: PAGE_PAD_TOP,
            bottom: 0,
        }))
        .show_separator_line(false)
        .resizable(false)
        .show(ui, |ui| transport_bar::show(ui, state, theme));

    CentralPanel::default()
        .frame(Frame::NONE.fill(theme.surface).inner_margin(Margin {
            left: PAGE_PAD_SIDE,
            right: PAGE_PAD_SIDE,
            top: PAGE_GAP,
            bottom: PAGE_PAD_BOTTOM,
        }))
        .show(ui, |ui| main_area(ui, state, theme));
}

/// Placeholder panel for the M1-M3 screens (synth, FX, sequencer, piano).
pub fn main_area(ui: &mut Ui, _state: &mut AppState, theme: &Theme) {
    let avail = ui.available_size();
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(avail - egui::vec2(theme.panel_padding * 2.0, theme.panel_padding * 2.0));
        ui.vertical_centered(|ui| {
            ui.add_space(theme.space_8);
            ui.label(
                egui::RichText::new("INSTRUMENTS")
                    .size(12.0)
                    .color(theme.text_secondary),
            );
            ui.add_space(theme.space_2);
            ui.label(
                egui::RichText::new("Synth, FX, sequencer and piano arrive in M1-M3.")
                    .size(13.0)
                    .color(theme.text_secondary),
            );
        });
    });
}
