//! UI panels. Each panel is a plain function over `&mut Ui`, `&mut AppState` and `&Theme`,
//! so `egui_kittest` can render it inside any `Ui` without a window or an audio device.
//! [`show`] lays them out in the real window.
//!
//! Page layout (design/WINNER.md): page padding 12 20 16, page gap 10, panels radius 24.
//! Top to bottom: transport bar, main area (SYNTH panel | FX rack above SEQUENCER | right
//! column), KEYS panel. The right column ([`RIGHT_COLUMN_WIDTH`]) shows the PROJECT panel or
//! the SONG CHAIN panel ([`AppState::right_tab`]: the page is 1140 tall and has no room for
//! both). The top row's right slot ([`fx_rack::WIDTH`]) shows the FX rack or the MIXER
//! ([`AppState::right_panel`], M6; both heads carry the `FX | Mixer` tab). The main area
//! scrolls vertically when the window is at its minimum height. The right column's third
//! tab is the VISUALIZER panel ([`visualizer`]); with [`AppState::visual_full`] set the
//! visualizer overlay ([`visualizer::overlay`]) covers the SYNTH | FX row until Escape / CLOSE.

use egui::{Align, CentralPanel, Frame, Layout, Margin, Panel, ScrollArea, Ui, vec2};
use neu::Theme;

use crate::app::{AppState, RightPanel, RightTab};

pub mod controls;
pub mod fx_rack;
pub mod keyboard;
pub mod mixer;
pub mod project_bar;
pub mod sequencer;
pub mod song;
pub mod synth_panel;
pub mod transport_bar;
pub mod visualizer;

/// Width of the right-hand column (PROJECT / SONG CHAIN panel) including its padding: wide
/// enough for the A..H `segmented` (214 pt) inside the panel padding.
pub const RIGHT_COLUMN_WIDTH: f32 = 260.0;

/// Page padding: top 12, sides 20, bottom 16.
pub const PAGE_PAD_TOP: i8 = 12;
pub const PAGE_PAD_SIDE: i8 = 20;
pub const PAGE_PAD_BOTTOM: i8 = 16;
/// Gap between the transport bar and the main area, and between the panels of the main area.
pub const PAGE_GAP: i8 = 10;
/// Smallest height of the SYNTH | FX row (the panels' natural content height).
pub const TOP_ROW_MIN_HEIGHT: f32 = 330.0;

/// Full-window layout: transport bar on top, KEYS panel at the bottom, main area between.
/// `ui` is the root `Ui` eframe hands to `App::ui` (or any `Ui` under `egui_kittest`).
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

    Panel::bottom("keys")
        .frame(Frame::NONE.fill(theme.surface).inner_margin(Margin {
            left: PAGE_PAD_SIDE,
            right: PAGE_PAD_SIDE,
            top: PAGE_GAP,
            bottom: PAGE_PAD_BOTTOM,
        }))
        .show_separator_line(false)
        .resizable(false)
        .show(ui, |ui| keyboard::show(ui, state, theme));

    CentralPanel::default()
        .frame(Frame::NONE.fill(theme.surface).inner_margin(Margin {
            left: PAGE_PAD_SIDE,
            right: PAGE_PAD_SIDE,
            top: PAGE_GAP,
            bottom: 0,
        }))
        .show(ui, |ui| main_area(ui, state, theme));

    if state.visual_full {
        visualizer::overlay(ui, state, theme);
    }
}

/// The main area: SYNTH panel (flexible width) | FX rack or MIXER ([`fx_rack::WIDTH`]) on top, then
/// SEQUENCER (flexible width) | PROJECT or SONG CHAIN ([`RIGHT_COLUMN_WIDTH`]). The top row takes whatever
/// height the sequencer row ([`sequencer::panel_height`], known up front) leaves, but never
/// less than [`TOP_ROW_MIN_HEIGHT`]; when the panels need more height than there is (minimum
/// window size) the area scrolls vertically.
pub fn main_area(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let available = ui.available_height();
    let gap = f32::from(PAGE_GAP);
    let bottom_height = sequencer::panel_height(ui, theme);
    let top_height = (available - bottom_height - gap).max(TOP_ROW_MIN_HEIGHT);
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = gap;
            let synth_width = (ui.available_width() - fx_rack::WIDTH - gap).max(0.0);
            let top_row = ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.allocate_ui_with_layout(
                    vec2(synth_width, top_height),
                    Layout::top_down(Align::Min),
                    |ui| synth_panel::show(ui, state, theme),
                );
                ui.allocate_ui_with_layout(
                    vec2(fx_rack::WIDTH, top_height),
                    Layout::top_down(Align::Min),
                    |ui| match state.right_panel {
                        RightPanel::Fx => fx_rack::show(ui, state, theme),
                        RightPanel::Mixer => mixer::show(ui, state, theme),
                    },
                );
            });
            state.top_row_rect = Some(top_row.response.rect);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let seq_width = (ui.available_width() - RIGHT_COLUMN_WIDTH - gap).max(0.0);
                ui.allocate_ui_with_layout(
                    vec2(seq_width, bottom_height),
                    Layout::top_down(Align::Min),
                    |ui| sequencer::show(ui, state, theme),
                );
                ui.allocate_ui_with_layout(
                    vec2(RIGHT_COLUMN_WIDTH, bottom_height),
                    Layout::top_down(Align::Min),
                    |ui| match state.right_tab {
                        RightTab::Project => project_bar::show(ui, state, theme),
                        RightTab::Chain => song::show(ui, state, theme),
                        RightTab::Visual => visualizer::show(ui, state, theme),
                    },
                );
            });
        });
}
