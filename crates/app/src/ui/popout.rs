//! Pop-out windows: every module of the shell ([`Panel`]: the browser, the channel rack, the
//! views and the keyboard) can leave the main window for a window of its own and come back.
//!
//! * POP OUT: the [`header_control`] in the module's window header (a ghost toggle named
//!   [`Panel::pop_out_label`]) moves it out. The main window reflows without it (the browser
//!   column, the rack and the keyboard strip give their room to the rest), or, for a view,
//!   shows the [`placeholder`] when that view's tab is selected.
//! * DOCK: the same header control in the module's own window ([`Panel::dock_label`]), the
//!   lit POP OUT in the placeholder, or closing the window bring it back; DOCK ALL
//!   ([`DOCK_ALL_LABEL`], in the toolbar while anything is out) brings every one back.
//!
//! [`show_windows`] draws every popped-out panel each frame, whatever the main window shows,
//! as an immediate viewport: a real OS window under eframe, an embedded `egui::Window` where
//! the backend has no multi-window support (headless tests). A real window reads its own
//! keyboard ([`AppState::handle_window_input`]), so shortcuts and the piano keys work there
//! as in the main window; F-keys of a popped-out view bring its window to the front
//! ([`AppState::focus_request`]).

use egui::{Align, Layout, Response, Role, Ui, Vec2, ViewportClass, WidgetInfo, vec2};
use neu::{ButtonKind, Icon, TextKind, Theme};

use crate::app::{AppState, Panel, View};
use crate::ui::{browser, keyboard, roll_view, sequencer, shell};

/// Accessible name (and label) of the toolbar button that docks every popped-out panel.
pub const DOCK_ALL_LABEL: &str = "Dock all";
/// Padding between a popped-out window's edge and its module.
const WINDOW_PAD: f32 = 4.0;

/// Size `panel`'s window opens at (logical points).
#[must_use]
pub fn default_size(panel: Panel) -> Vec2 {
    match panel {
        Panel::Browser => vec2(280.0, 640.0),
        Panel::Rack => vec2(760.0, 560.0),
        Panel::Roll => roll_view::ROLL_WINDOW_SIZE,
        Panel::Mixer => vec2(1000.0, 520.0),
        Panel::Synth => vec2(1200.0, 560.0),
        Panel::Song => vec2(720.0, 480.0),
        Panel::Visual => vec2(720.0, 440.0),
        Panel::Keyboard => vec2(1000.0, keyboard::HEIGHT + 2.0 * WINDOW_PAD),
    }
}

/// Smallest size `panel`'s window can be resized to.
#[must_use]
pub fn min_size(panel: Panel) -> Vec2 {
    match panel {
        Panel::Browser => vec2(browser::WIDTH + 2.0 * WINDOW_PAD, 320.0),
        Panel::Rack => vec2(crate::app::MIN_RACK_WIDTH, 240.0),
        Panel::Roll => vec2(600.0, 300.0),
        Panel::Mixer => vec2(480.0, 300.0),
        Panel::Synth => vec2(720.0, 320.0),
        Panel::Song => vec2(400.0, 240.0),
        Panel::Visual => vec2(320.0, 200.0),
        Panel::Keyboard => vec2(480.0, keyboard::HEIGHT + 2.0 * WINDOW_PAD),
    }
}

/// The pop out / dock control of `panel`'s window header (call it in the header's
/// right-to-left slot): DOCK ([`Panel::dock_label`]) while the panel is in its own window,
/// POP OUT ([`Panel::pop_out_label`]) otherwise.
pub fn header_control(ui: &mut Ui, state: &mut AppState, theme: &Theme, panel: Panel) {
    if state.is_popped(panel) {
        let dock = neu::header_button(ui, theme, Icon::Dock);
        dock.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), panel.dock_label()));
        if dock
            .on_hover_text("Put it back in the main window")
            .clicked()
        {
            state.set_popped(panel, false);
        }
    } else {
        pop_out_button(ui, state, theme, panel);
    }
}

/// POP OUT as a ghost toggle, lit while `panel` is out (clicking it then docks it).
fn pop_out_button(ui: &mut Ui, state: &mut AppState, theme: &Theme, panel: Panel) -> Response {
    let popped = state.is_popped(panel);
    let response = neu::icon_button_kind(
        ui,
        theme,
        Icon::PopOut,
        vec2(24.0, 24.0),
        ButtonKind::GhostToggle(popped),
    );
    response.widget_info(|| {
        WidgetInfo::selected(Role::Button, ui.is_enabled(), popped, panel.pop_out_label())
    });
    let tip = if popped {
        "Put it back in the main window"
    } else {
        "Open in its own window (DOCK or closing it brings it back)"
    };
    let response = response.on_hover_text(tip);
    if response.clicked() {
        state.set_popped(panel, !popped);
    }
    response
}

/// What the main window shows for `view` while it is in its own window: the view's frame
/// with the lit POP OUT (it docks the view) and [`Panel::popped_label`].
pub fn placeholder(ui: &mut Ui, state: &mut AppState, theme: &Theme, view: View) {
    let panel = view.panel();
    neu::window_frame_sized(
        ui,
        theme,
        ui.available_size_before_wrap(),
        view.title(),
        None,
        |ui| {
            pop_out_button(ui, state, theme, panel);
        },
        |ui| {
            ui.allocate_ui_with_layout(
                ui.available_size_before_wrap(),
                Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    neu::text_widget(ui, panel.popped_label(), TextKind::Label, theme.text_2)
                        .on_hover_text("POP OUT here, DOCK there or DOCK ALL brings it back");
                },
            );
        },
    );
}

/// Draws the window of every popped-out panel (call once a frame, after the main window).
pub fn show_windows(ctx: &egui::Context, state: &mut AppState, theme: &Theme) {
    let panels: Vec<Panel> = state.popped_panels().collect();
    for panel in panels {
        window(ctx, state, theme, panel);
    }
}

/// `panel`'s own window ([`Panel::window_title`]). Closing it docks the panel.
fn window(ctx: &egui::Context, state: &mut AppState, theme: &Theme, panel: Panel) {
    let id = egui::ViewportId::from_hash_of(("beatslopper_popout", panel));
    let builder = egui::ViewportBuilder::default()
        .with_title(panel.window_title())
        .with_inner_size(default_size(panel))
        .with_min_inner_size(min_size(panel));
    ctx.show_viewport_immediate(id, builder, |ui, class| {
        if class == ViewportClass::Immediate {
            // A real window: its own keyboard and close button. (Embedded in the main window
            // headless, the main window already read this input.)
            state.handle_window_input(ui);
            if ui.input(|i| i.viewport().close_requested()) {
                state.set_popped(panel, false);
            }
            if state.focus_request == Some(panel) {
                state.focus_request = None;
                ui.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg).inner_margin(WINDOW_PAD))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    contents(ui, state, theme, panel);
                });
            });
    });
}

/// The module itself, filling the window.
fn contents(ui: &mut Ui, state: &mut AppState, theme: &Theme, panel: Panel) {
    match panel {
        Panel::Browser => browser::show(ui, state, theme),
        Panel::Rack => sequencer::show(ui, state, theme),
        Panel::Roll => shell::view_window(ui, state, theme, View::Roll),
        Panel::Mixer => shell::view_window(ui, state, theme, View::Mixer),
        Panel::Synth => shell::view_window(ui, state, theme, View::Synth),
        Panel::Song => shell::view_window(ui, state, theme, View::Song),
        Panel::Visual => shell::view_window(ui, state, theme, View::Visual),
        Panel::Keyboard => keyboard::show(ui, state, theme),
    }
}
