//! UI panels and the shell. Each panel is a plain function over `&mut Ui`, `&mut AppState`
//! and `&Theme`, so `egui_kittest` can render it inside any `Ui` without a window or an audio
//! device. [`show`] lays the Deck shell out in the real window ([`shell`]: the toolbar, the
//! BROWSER column, the CHANNEL RACK, the main window's view behind its tab strip, the mixer
//! bar, the KEYBOARD strip and the hint bar; `design/round3/WINNER.md` section 5).
//!
//! The pieces: [`transport_bar`] (the toolbar), [`browser`] (the column with the
//! [`preset_browser`], [`project_bar`] and [`recent_panel`] sections), [`sequencer`] (the
//! rack, with [`tracks`] for the track rows' names and menu), [`roll_view`] (the piano roll
//! window), [`mixer`], [`synth_panel`] + [`fx_rack`], [`song`] and [`visualizer`] (the other
//! views, rehosted in windows until M13 / M14), [`keyboard`] (the strip), [`plugins`] (the
//! pop-out windows) and [`controls`] (the bound controls and formatters).

use egui::Ui;
use neu::Theme;

use crate::app::AppState;

pub mod browser;
pub mod controls;
pub mod fx_rack;
pub mod keyboard;
pub mod mixer;
pub mod plugins;
pub mod preset_browser;
pub mod project_bar;
pub mod recent_panel;
pub mod roll_view;
pub mod sequencer;
pub mod shell;
pub mod song;
pub mod synth_panel;
pub mod tempo;
pub mod tracks;
pub mod transport_bar;
pub mod visualizer;

/// Full-window layout: the Deck shell. `ui` is the root `Ui` eframe hands to `App::ui` (or
/// any `Ui` under `egui_kittest`).
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    shell::show(ui, state, theme);
}
