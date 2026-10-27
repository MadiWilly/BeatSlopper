//! The Deck shell (M12, `design/round3/WINNER.md` section 5 "Layout"): the window frame on
//! `bg` with 4 pt padding and 4 pt gaps, a column of
//!
//! ```text
//! TOOLBAR 40                                   (crate::ui::transport_bar)
//! BODY (flex): BROWSER 208 / rail 28 | WORKSPACE
//!     WORKSPACE: the tab strip 28 (PIANO ROLL F5 | MIXER F6 | SYNTH & FX F7 | SONG F8 |
//!                VISUALIZER) across the top, then
//!     rack open:  CHANNEL RACK (rack_width) | handle | MAIN (flex)
//!     rack collapsed:  CHANNEL RACK header 28 over MAIN (flex)
//! MIXER BAR 28                                  (the drawer's collapsed bar; M13 opens it)
//! KEYBOARD 96 / 28                              (crate::ui::keyboard)
//! HINT BAR 28                                   (neu::hint_bar + the audio line)
//! ```
//!
//! The views (`crate::app::View`): PIANO ROLL ([`crate::ui::roll_view`]), MIXER (the M11
//! panel rehosted in a window with a two-way scroll area until the M13 drawer), SYNTH & FX
//! (the SYNTH panel beside the FX rack, stacked under 980 points of main width, with the
//! preset strip in the window toolbar; their Deck groups are M13), SONG (the SONG CHAIN
//! panel; the timeline is M14) and VISUALIZER (the panel; M14 puts it beside SONG). The
//! F5..F9 keys, the toolbar's view buttons and the tab strip all go through
//! `AppState::set_view`; F11 / the FULL toggle cover the workspace with the visualizer
//! overlay.
//!
//! Narrow (`crate::window::is_narrow`, at or under 1000 wide): the first narrow frame
//! collapses the browser to the rail and the keyboard to its header (the user can open them
//! again; an open keyboard then floats over the workspace), the rack is 400 wide, the tab
//! items use the 8 pt padding and the toolbar drops its optional items. Every window body
//! scrolls instead of clipping, so the layout works from 840 x 600 up.
//!
//! Accessible names: the tabs are [`crate::app::View::tab_label`] ("PIANO ROLL" ...), the
//! rack | main splitter [`RACK_HANDLE_NAME`], the mixer bar [`MIXER_BAR_NAME`] (a button;
//! its chevron [`MIXER_OPEN_NAME`] / [`MIXER_CLOSE_NAME`], its meter [`MIXER_METER_NAME`],
//! its insert chips [`mixer_chip_name`]: inserts 1..=5 plus the highlighted insert when it
//! is further along, the highlighted one lit; a click opens the MIXER view on that insert),
//! the hint bar's label (the keycap · label pairs of [`HINTS`] joined) and the audio line
//! [`AUDIO_INFO_NAME`]. The persisted pieces are the [`crate::app::ShellLayout`].

use egui::{
    Align, Align2, CursorIcon, Layout, Rect, Response, Role, Sense, Ui, UiBuilder, WidgetInfo,
    pos2, vec2,
};
use neu::{Icon, TextKind, Theme};

use crate::app::{AppState, MIN_RACK_WIDTH, View};
use crate::ui::{
    browser, fx_rack, keyboard, mixer, roll_view, sequencer, song, synth_panel, transport_bar,
    visualizer,
};
use crate::window;

/// Frame padding and the gap between docks (WINNER: 4).
pub const FRAME_PAD: f32 = 4.0;
pub const GAP: f32 = 4.0;
/// Height of the mixer bar and the hint bar.
pub const MIXER_BAR: f32 = 28.0;
pub const HINT_BAR: f32 = 28.0;
/// The splitter's drawn width and hit width.
pub const SPLIT_WIDTH: f32 = 4.0;
pub const SPLIT_HIT: f32 = 8.0;
/// Narrowest the main window may be dragged (the rack handle stops there).
pub const MAIN_MIN_WIDTH: f32 = 240.0;
/// Width of the rack in the narrow layout.
pub const NARROW_RACK_WIDTH: f32 = MIN_RACK_WIDTH;
/// Under this main-window width the SYNTH & FX view stacks the synth panel over the rack.
pub const SYNTH_STACK_BELOW: f32 = 980.0;
/// Height of each stacked panel in the SYNTH & FX view.
pub const SYNTH_STACK_HEIGHT: f32 = 440.0;
/// Narrowest the SYNTH panel is laid out at (its first knob row needs 698 points; the view
/// scrolls sideways below that).
pub const SYNTH_PANEL_MIN_WIDTH: f32 = 700.0;
/// Height the mixer panel is given inside its window (its body is laid out by hand).
pub const MIXER_PANEL_HEIGHT: f32 = 380.0;
/// Accessible names.
pub const RACK_HANDLE_NAME: &str = "Rack handle";
pub const MIXER_BAR_NAME: &str = "Mixer bar";
pub const MIXER_OPEN_NAME: &str = "Open mixer";
pub const MIXER_CLOSE_NAME: &str = "Close mixer";
pub const MIXER_METER_NAME: &str = "Mixer bar meter";
pub const AUDIO_INFO_NAME: &str = "Audio info";
/// Inserts the mixer bar always shows a chip for (the highlighted insert gets one too).
pub const BAR_CHIPS: u8 = 5;

/// Accessible name of the mixer bar's chip for insert `insert`: `"Mixer bar insert 3"`.
#[must_use]
pub fn mixer_chip_name(insert: u8) -> String {
    format!("Mixer bar insert {insert}")
}
/// The hint bar's keycap · label pairs.
pub const HINTS: [(&str, &str); 7] = [
    ("Space", "Play / Stop"),
    ("R", "Rec"),
    ("Ctrl+Z", "Undo"),
    ("Ctrl+T", "Add track"),
    ("Ctrl+L", "Link insert"),
    ("F5..F9", "Views"),
    ("Esc", "Deselect · twice: panic"),
];
/// Width kept for the audio line at the hint bar's right.
const AUDIO_INFO_WIDTH: f32 = 150.0;

/// The tab strip's items, in [`View::ALL`] order.
#[must_use]
pub fn tab_items() -> Vec<(&'static str, Option<&'static str>)> {
    View::ALL.iter().map(|v| (v.tab_label(), v.key())).collect()
}

/// Draws the whole window into `ui` (the root `Ui` eframe hands to `App::ui`, or any `Ui`
/// under `egui_kittest`) and applies every control change to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, theme.bg);
    let narrow = window::is_narrow(full.width());
    state.apply_narrow_defaults(narrow);
    let inner = full.shrink(FRAME_PAD);

    let keyboard_height = if state.keyboard_open && !narrow {
        keyboard::HEIGHT
    } else {
        keyboard::COLLAPSED_HEIGHT
    };
    let fixed = neu::APP_TOOLBAR + MIXER_BAR + keyboard_height + HINT_BAR + 4.0 * GAP;
    let body_height = (inner.height() - fixed).max(120.0);

    let mut y = inner.min.y;
    let toolbar_rect =
        Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), neu::APP_TOOLBAR));
    y += neu::APP_TOOLBAR + GAP;
    let body_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), body_height));
    y += body_height + GAP;
    let mixer_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), MIXER_BAR));
    y += MIXER_BAR + GAP;
    let keyboard_rect =
        Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), keyboard_height));
    y += keyboard_height + GAP;
    let hint_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), HINT_BAR));

    region(ui, toolbar_rect, "toolbar", |ui| {
        transport_bar::show(ui, state, theme)
    });
    region(ui, body_rect, "body", |ui| body(ui, state, theme, narrow));
    region(ui, mixer_rect, "mixer_bar", |ui| {
        mixer_bar(ui, state, theme)
    });
    region(ui, keyboard_rect, "keyboard", |ui| {
        keyboard::show(ui, state, theme)
    });
    region(ui, hint_rect, "hint_bar", |ui| hint_bar(ui, state, theme));

    if narrow && state.keyboard_open {
        // The open keyboard floats over the bottom of the workspace at narrow.
        let float = Rect::from_min_size(
            pos2(body_rect.min.x, body_rect.max.y - keyboard::HEIGHT),
            vec2(body_rect.width(), keyboard::HEIGHT),
        );
        egui::Area::new(ui.scope_id().with("keyboard_float"))
            .order(egui::Order::Foreground)
            .fixed_pos(float.min)
            .interactable(true)
            .show(ui.ctx(), |ui| {
                ui.allocate_ui_with_layout(float.size(), Layout::top_down(Align::Min), |ui| {
                    ui.set_min_size(float.size());
                    ui.set_max_size(float.size());
                    neu::paint_shadow_floating(ui.painter(), float, theme.radius_control, theme);
                    keyboard::show(ui, state, theme);
                });
            });
    }
    if state.visual_full {
        visualizer::overlay(ui, state, theme);
    }
}

/// A child `Ui` confined to `rect` (top-down, clipped).
fn region<R>(ui: &mut Ui, rect: Rect, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min))
            .id_salt(id),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    add(&mut child)
}

/// The body: the browser column (or rail) beside the workspace.
fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme, narrow: bool) {
    let rect = ui.max_rect();
    let browser_width = if state.browser_open {
        browser::WIDTH
    } else {
        browser::RAIL_WIDTH
    };
    let browser_rect = Rect::from_min_size(rect.min, vec2(browser_width, rect.height()));
    let workspace = Rect::from_min_max(pos2(browser_rect.max.x + GAP, rect.min.y), rect.max);
    state.workspace_rect = Some(workspace);
    region(ui, browser_rect, "browser", |ui| {
        if state.browser_open {
            browser::show(ui, state, theme);
        } else {
            browser::rail(ui, state, theme);
        }
    });
    region(ui, workspace, "workspace", |ui| {
        workspace_contents(ui, state, theme, narrow)
    });
}

/// The workspace: the tab strip across the top, then the rack beside (open) or over
/// (collapsed) the main window.
fn workspace_contents(ui: &mut Ui, state: &mut AppState, theme: &Theme, narrow: bool) {
    let whole = ui.max_rect();
    let strip_rect = Rect::from_min_size(whole.min, vec2(whole.width(), neu::TAB_STRIP));
    region(ui, strip_rect, "tabs", |ui| {
        view_tabs(ui, state, theme, narrow)
    });
    let rect = Rect::from_min_max(pos2(whole.min.x, strip_rect.max.y + GAP), whole.max);
    if state.rack_open {
        let max_rack = (rect.width() - SPLIT_HIT - MAIN_MIN_WIDTH).max(MIN_RACK_WIDTH);
        let rack_width = if narrow {
            NARROW_RACK_WIDTH.min(max_rack)
        } else {
            state.rack_width.clamp(MIN_RACK_WIDTH, max_rack)
        };
        let rack_rect = Rect::from_min_size(rect.min, vec2(rack_width, rect.height()));
        let handle_rect = Rect::from_min_size(
            pos2(rack_rect.max.x, rect.min.y),
            vec2(SPLIT_HIT, rect.height()),
        );
        let main_rect = Rect::from_min_max(pos2(handle_rect.max.x, rect.min.y), rect.max);
        region(ui, rack_rect, "rack", |ui| {
            sequencer::show(ui, state, theme)
        });
        let handle = split_handle(ui, handle_rect, theme);
        if handle.dragged() && !narrow {
            let dx = handle.drag_delta().x;
            if dx != 0.0 {
                state.set_rack_width(rack_width + dx);
            }
        }
        region(ui, main_rect, "main", |ui| {
            main_window(ui, state, theme, narrow)
        });
    } else {
        let rack_rect = Rect::from_min_size(rect.min, vec2(rect.width(), neu::WINDOW_HEADER));
        let main_rect = Rect::from_min_max(pos2(rect.min.x, rack_rect.max.y + GAP), rect.max);
        region(ui, rack_rect, "rack", |ui| {
            sequencer::show(ui, state, theme)
        });
        region(ui, main_rect, "main", |ui| {
            main_window(ui, state, theme, narrow)
        });
    }
}

/// The vertical splitter between the rack and the main window: a 4 pt `border` bar in an
/// 8 pt hit (`border_2` hovered, `accent` dragged). Named [`RACK_HANDLE_NAME`].
fn split_handle(ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
    let response = ui.interact(rect, ui.scope_id().with("rack_handle"), Sense::drag());
    if ui.is_rect_visible(rect) {
        let colour = if response.dragged() {
            theme.accent
        } else if response.hovered() {
            theme.border_2
        } else {
            theme.border
        };
        let bar = Rect::from_center_size(rect.center(), vec2(SPLIT_WIDTH, rect.height()));
        ui.painter().rect_filled(bar, 2.0, colour);
        let grip = Rect::from_center_size(rect.center(), vec2(2.0, 32.0));
        ui.painter().rect_filled(grip, 1.0, theme.text_3);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Splitter, ui.is_enabled(), RACK_HANDLE_NAME));
    response
        .on_hover_cursor(CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to trade width between the rack and the main window")
}

/// The tab strip across the workspace: one tab per view, mirroring the toolbar's buttons.
fn view_tabs(ui: &mut Ui, state: &mut AppState, theme: &Theme, narrow: bool) {
    let items = tab_items();
    let before = View::ALL.iter().position(|v| *v == state.view).unwrap_or(0);
    let mut selected = before;
    let padding = if narrow { 8.0 } else { neu::shell::TAB_PADDING };
    neu::tab_strip_padded(ui, theme, &mut selected, &items, padding);
    if selected != before
        && let Some(view) = View::ALL.get(selected)
    {
        if *view == View::Mixer {
            state.toggle_mixer_drawer();
        } else {
            state.set_view(*view);
        }
    }
}

/// The main window: the view's window.
fn main_window(ui: &mut Ui, state: &mut AppState, theme: &Theme, _narrow: bool) {
    match state.view {
        View::Roll => roll_view::show(ui, state, theme),
        View::Mixer => mixer_window(ui, state, theme),
        View::Synth => synth_window(ui, state, theme),
        View::Song => song_window(ui, state, theme),
        View::Visual => visual_window(ui, state, theme),
    }
}

/// The MIXER view: the M11 panel in a two-way scroll area (M13 brings the drawer).
fn mixer_window(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let sub = format!(
        "{} inserts · master · insert {} selected",
        engine::inserts::INSERT_COUNT,
        state.highlighted_insert()
    );
    neu::window_frame_sized(
        ui,
        theme,
        ui.available_size_before_wrap(),
        View::Mixer.title(),
        Some(&sub),
        |ui| {
            neu::keycap(ui, theme, "F6");
        },
        |ui| {
            neu::toolbar(ui, theme, |ui| {
                crate::ui::controls::plugin_pills(ui, state, theme)
            });
            let avail = ui.available_size_before_wrap();
            neu::scroll_both("mixer_host").show(ui, |ui| {
                let size = vec2(
                    mixer::WIDTH.max(avail.x - neu::SCROLLBAR_WIDTH),
                    MIXER_PANEL_HEIGHT.max(avail.y - neu::SCROLLBAR_WIDTH),
                );
                ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    mixer::show(ui, state, theme);
                });
            });
        },
    );
}

/// The SYNTH & FX view: the preset strip in the toolbar, the SYNTH panel beside (or over)
/// the FX rack.
fn synth_window(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let sub = roll_view::header_sub(state);
    neu::window_frame_sized(
        ui,
        theme,
        ui.available_size_before_wrap(),
        View::Synth.title(),
        Some(&sub),
        |ui| {
            neu::keycap(ui, theme, "F7");
        },
        |ui| {
            neu::toolbar(ui, theme, |ui| synth_panel::preset_strip(ui, state, theme));
            let avail = ui.available_size_before_wrap();
            let stacked = avail.x < SYNTH_STACK_BELOW;
            neu::scroll_both("synth_host").show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                if stacked {
                    // The panels keep their design width and scroll sideways when the window
                    // is narrower (their Deck groups reflow in M13).
                    let width = (avail.x - neu::SCROLLBAR_WIDTH).max(SYNTH_PANEL_MIN_WIDTH);
                    ui.allocate_ui_with_layout(
                        vec2(width, SYNTH_STACK_HEIGHT),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_size(vec2(width, SYNTH_STACK_HEIGHT));
                            ui.set_max_size(vec2(width, SYNTH_STACK_HEIGHT));
                            synth_panel::show(ui, state, theme);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        vec2(width, SYNTH_STACK_HEIGHT),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_size(vec2(width, SYNTH_STACK_HEIGHT));
                            ui.set_max_size(vec2(width, SYNTH_STACK_HEIGHT));
                            fx_rack::show(ui, state, theme);
                        },
                    );
                } else {
                    let height = (avail.y - neu::SCROLLBAR_WIDTH).max(320.0);
                    let synth_width = (avail.x - neu::SCROLLBAR_WIDTH - fx_rack::WIDTH - GAP)
                        .max(SYNTH_PANEL_MIN_WIDTH);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            vec2(synth_width, height),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_min_size(vec2(synth_width, height));
                                ui.set_max_size(vec2(synth_width, height));
                                synth_panel::show(ui, state, theme);
                            },
                        );
                        ui.allocate_ui_with_layout(
                            vec2(fx_rack::WIDTH, height),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_min_size(vec2(fx_rack::WIDTH, height));
                                ui.set_max_size(vec2(fx_rack::WIDTH, height));
                                fx_rack::show(ui, state, theme);
                            },
                        );
                    });
                }
            });
        },
    );
}

/// The SONG view: the SONG CHAIN panel (the timeline is M14).
fn song_window(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let entries = state.chain().len();
    let bars: usize = state
        .chain()
        .iter()
        .map(|e| state.bank.pattern(e.pattern).bars() * usize::from(e.repeats))
        .sum();
    let sub = format!(
        "{entries} {} · {bars} {}",
        if entries == 1 { "entry" } else { "entries" },
        if bars == 1 { "bar" } else { "bars" }
    );
    neu::window_frame_sized(
        ui,
        theme,
        ui.available_size_before_wrap(),
        View::Song.title(),
        Some(&sub),
        |ui| {
            neu::keycap(ui, theme, "F8");
        },
        |ui| {
            let size = ui.available_size_before_wrap();
            ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(size);
                ui.set_max_size(size);
                song::show(ui, state, theme);
            });
        },
    );
}

/// The VISUALIZER view: the panel (M14 puts it beside SONG).
fn visual_window(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let sub = state.visual_preset().name.to_owned();
    neu::window_frame_sized(
        ui,
        theme,
        ui.available_size_before_wrap(),
        View::Visual.title(),
        Some(&sub),
        |ui| {
            neu::keycap(ui, theme, "F11");
        },
        |ui| {
            let size = ui.available_size_before_wrap();
            ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(size);
                ui.set_max_size(size);
                visualizer::show(ui, state, theme);
            });
        },
    );
}

/// The mixer drawer's collapsed bar: "MIXER" · "16 inserts · master" · the first five
/// insert chips and "… 16" · spacer · the master h-meter · F6 · the chevron. A click
/// anywhere on it opens the MIXER view (the chevron closes it again).
fn mixer_bar(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let rect = ui.max_rect();
    let response = ui.interact(rect, ui.scope_id().with("mixer_bar"), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            theme.panel_3
        } else {
            theme.panel_2
        };
        ui.painter().add(neu::flat_rect_shape(
            rect,
            theme.radius_control,
            fill,
            theme.border,
        ));
    }
    let open = state.view == View::Mixer;
    response
        .widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), open, MIXER_BAR_NAME));
    let response = response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Mixer (F6)");
    let inner = Rect::from_min_max(
        pos2(rect.min.x + 8.0, rect.min.y),
        pos2(rect.max.x - 4.0, rect.max.y),
    );
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center))
            .id_salt("mixer_bar_row"),
    );
    row.spacing_mut().item_spacing = vec2(8.0, 0.0);
    row.set_clip_rect(inner.intersect(ui.clip_rect()));
    let ui = &mut row;
    neu::title(ui, theme, "Mixer");
    neu::text_widget(ui, "16 inserts · master", TextKind::Label, theme.text_2);
    ui.spacing_mut().item_spacing.x = 4.0;
    // Inserts 1..=5, then the highlighted insert when it is further along.
    let highlighted = state.highlighted_insert();
    let mut chips: Vec<u8> = (1..=BAR_CHIPS).collect();
    if highlighted > BAR_CHIPS && highlighted <= engine::inserts::MAX_INSERT {
        chips.push(highlighted);
    }
    let mut picked = None;
    for n in chips {
        let name = state.insert_display_name(n);
        let chip = insert_chip(ui, theme, &n.to_string(), &name, n == highlighted, true);
        chip.widget_info(|| {
            WidgetInfo::selected(
                Role::Button,
                ui.is_enabled(),
                n == highlighted,
                mixer_chip_name(n),
            )
        });
        if chip
            .on_hover_text(format!(
                "Insert {n}: {name} · click to open it in the mixer"
            ))
            .on_hover_cursor(CursorIcon::PointingHand)
            .clicked()
        {
            picked = Some(n);
        }
    }
    insert_chip(ui, theme, "…", "16", false, false)
        .on_hover_text("16 inserts · F6 opens the mixer");
    ui.spacing_mut().item_spacing.x = 8.0;
    // Right-aligned: the meter, the keycap and the chevron (measured last frame).
    let width_id = ui.scope_id().with("mixer_bar_right");
    let known: Option<f32> = ui.data(|d| d.get_temp(width_id));
    if let Some(width) = known {
        let pad = ui.available_width() - width;
        if pad > 0.0 {
            ui.add_space(pad);
        }
    }
    let group = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let levels = (state.shown_meters.master[0], state.shown_meters.master[1]);
        neu::meter_horizontal(ui, theme, levels, neu::METER_HORIZONTAL)
            .widget_info(|| WidgetInfo::labeled(Role::Meter, true, MIXER_METER_NAME));
        neu::keycap(ui, theme, "F6");
        let (icon, name) = if open {
            (Icon::ChevronDown, MIXER_CLOSE_NAME)
        } else {
            (Icon::ChevronUp, MIXER_OPEN_NAME)
        };
        let chevron = neu::header_button(ui, theme, icon);
        chevron.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), name));
        chevron.on_hover_text("Mixer (F6)").clicked()
    });
    ui.data_mut(|d| d.insert_temp(width_id, group.response.rect.width()));
    if let Some(n) = picked {
        state.select_insert(n, false);
        state.set_view(View::Mixer);
    } else if group.inner {
        state.toggle_mixer_drawer();
    } else if response.clicked() && !open {
        state.set_view(View::Mixer);
    }
}

/// A mini insert chip: 20 h, padding 0 6, the number in `text_3` 600 and the name in
/// `text_2`, `panel` fill with a `border` (`lit`: the accent wash and an accent border),
/// radius 3. `clickable` senses clicks (the insert chips), else hover only.
fn insert_chip(
    ui: &mut Ui,
    theme: &Theme,
    number: &str,
    name: &str,
    lit: bool,
    clickable: bool,
) -> Response {
    let painter = ui.painter().clone();
    let shown = neu::ellipsise(&painter, name, TextKind::Small, 60.0);
    let num = neu::text::layout(&painter, number, TextKind::Small, theme.text_3);
    let text = neu::text::layout(&painter, &shown, TextKind::Small, theme.text_2);
    let width = 6.0 + num.size().x + 4.0 + text.size().x + 6.0;
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 20.0), sense);
    if ui.is_rect_visible(rect) {
        let (fill, border) = if lit {
            (theme.accent_wash, theme.accent)
        } else if clickable && response.hovered() {
            (theme.panel_3, theme.border_2)
        } else {
            (theme.panel, theme.border)
        };
        neu::paint_control(&painter, rect, 3.0, fill, border);
        let num_rect = neu::text::paint_galley(
            &painter,
            pos2(rect.min.x + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            num,
            TextKind::Small,
            theme.text_3,
        );
        neu::text::paint_galley(
            &painter,
            pos2(num_rect.max.x + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            TextKind::Small,
            theme.text_2,
        );
    }
    response
}

/// The hint bar: the keycap · label pairs, the audio line at the right.
fn hint_bar(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let rect = ui.max_rect();
    ui.painter().add(neu::flat_rect_shape(
        rect,
        theme.radius_control,
        theme.panel,
        theme.border,
    ));
    let inner = Rect::from_min_max(
        pos2(rect.min.x + 8.0, rect.min.y),
        pos2(rect.max.x - 8.0, rect.max.y),
    );
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center))
            .id_salt("hint_bar_row"),
    );
    row.set_clip_rect(inner.intersect(ui.clip_rect()));
    let ui = &mut row;
    let hint_width = (ui.available_width() - AUDIO_INFO_WIDTH).max(40.0);
    ui.allocate_ui_with_layout(
        vec2(hint_width, HINT_BAR),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(hint_width);
            neu::hint_bar(ui, theme, &HINTS);
        },
    );
    let info = keyboard::audio_info(state);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        neu::text_widget(ui, &info, TextKind::Small, theme.text_3)
            .widget_info(|| WidgetInfo::labeled(Role::Label, true, AUDIO_INFO_NAME))
    });
}
