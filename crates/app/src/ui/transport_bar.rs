//! The app toolbar (M12, `design/round3/WINNER.md` section 5 "Toolbar"): a 40 pt
//! `neu::app_toolbar` in FL order, left to right:
//!
//! project menu (ghost icon "Menu": New / Open / Recent projects / Save / Save as / Export /
//! Test tone) · project name (13 / 600) + the unsaved dot · sep · undo · redo (ghost icons)
//! · sep · PLAY (primary icon, a stop glyph while playing; the name stays "Play") · STOP ·
//! REC (toggle) · `Pattern | Song` segmented · sep · BPM `-` value `+` TAP
//! ([`crate::ui::tempo`]) · sep · "Swing" + inline mini knob + value · sep · position
//! ("A · bar 1/2") · sep · master h-slider 72 + h-meter 72 · HINT panel (flex: the status
//! line, else the default hint) · MIDI lamp + port name (a click opens the port menu) · sep
//! · VIEWS: five rail buttons (roll F5, mixer F6, synth F7, song F8, browser F9; the active
//! one is ON) · sep · the theme switch.
//!
//! Narrow (at or under 1000 wide, [`crate::window::is_narrow`]): the project name, undo /
//! redo, TAP, swing, the h-meter, the hint text and the MIDI port name are dropped and
//! "Pattern" reads "Pat", so the bar fits 840 points.
//!
//! The right-hand group (MIDI, views, theme) is pushed to the right edge with the width it
//! had last frame ([`controls::right_aligned`]); the hint panel takes what is left.

use egui::{Align2, Response, Role, Sense, Ui, WidgetInfo, pos2, vec2};
use engine::{ParamId, PlayMode};
use neu::slider::SliderSpec;
use neu::{ButtonKind, Icon, KnobSize, KnobSpec, LedKind, MenuItem, TextKind, Theme};

use crate::app::{AppState, EditKey, MAX_SWING, MIN_SWING, View, truncate_name};
use crate::project::file_label;
use crate::shortcuts::Shortcut;
use crate::ui::controls::{self, fmt_percent};
use crate::ui::project_bar::{self, EXPORT_LOOP_LABEL, EXPORT_SONG_LABEL, UNSAVED_LABEL};

/// Side of the toolbar's icon buttons.
pub const BUTTON: f32 = 28.0;
/// The selector's "disconnected" entry.
pub const MIDI_NONE: &str = "None";
/// Longest port name shown after the MIDI lamp.
const MIDI_LABEL_CHARS: usize = 18;
/// Longest project name shown in the bar.
const NAME_CHARS: usize = 24;
/// The MODE selector's items, in `PlayMode` order (Pattern, Song).
pub const MODE_ITEMS: [&str; 2] = ["Pattern", "Song"];
/// The MODE selector's items at narrow.
pub const MODE_ITEMS_NARROW: [&str; 2] = ["Pat", "Song"];
/// Accessible name of the project menu button.
pub const MENU_NAME: &str = "Menu";
/// The project menu's entries (in order; the test tone entry is last).
pub const MENU_RECENT: &str = "Recent projects";
pub const MENU_TEST_TONE: &str = "Test tone";
/// Accessible name of the hint panel.
pub const HINT_NAME: &str = "Hint";
/// What the hint panel reads while no status line is showing.
pub const DEFAULT_HINT: &str = "Space play · R rec · Ctrl+T add track · F5..F9 views";
/// Accessible name of the MIDI lamp button (its menu lists the ports).
pub const MIDI_NAME: &str = "MIDI";
/// Accessible name of the master meter (the mixer's master meter is "Stereo level").
pub const MASTER_METER_NAME: &str = "Master meter";
/// Accessible name of the master slider.
pub const MASTER_NAME: &str = "Master";
/// Accessible name of the F9 browser toggle.
pub const BROWSER_NAME: &str = "Browser";
/// Width assumed for the right-hand group before it has been measured (first frame).
const RIGHT_GROUP_FALLBACK_WIDTH: f32 = 320.0;
/// Smallest width the hint panel keeps.
const HINT_MIN_WIDTH: f32 = 40.0;

/// Renders the toolbar into `ui` (a 40 pt `app_toolbar` filling the width) and applies any
/// control change to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let narrow = crate::window::is_narrow(ui.available_width() + 8.0);
    neu::app_toolbar(ui, theme, |ui| {
        project_group(ui, state, theme, narrow);
        neu::toolbar_separator(ui, theme);
        if !narrow {
            history_group(ui, state, theme);
            neu::toolbar_separator(ui, theme);
        }
        transport_group(ui, state, theme);
        mode_group(ui, state, theme, narrow);
        neu::toolbar_separator(ui, theme);
        crate::ui::tempo::bpm_group(ui, state, theme, !narrow);
        neu::toolbar_separator(ui, theme);
        if !narrow {
            swing_group(ui, state, theme);
            neu::toolbar_separator(ui, theme);
        }
        position(ui, state, theme);
        neu::toolbar_separator(ui, theme);
        master_group(ui, state, theme, narrow);

        // The hint panel takes the room left before the right-hand group.
        let width_id = ui.scope_id().with("toolbar_right_width");
        let known_width: Option<f32> = ui.data(|d| d.get_temp(width_id));
        let reserved =
            known_width.unwrap_or(RIGHT_GROUP_FALLBACK_WIDTH) + ui.spacing().item_spacing.x;
        let hint_width = (ui.available_width() - reserved).max(HINT_MIN_WIDTH);
        hint_panel(ui, state, theme, hint_width, narrow);
        let group = ui.horizontal(|ui| {
            midi_group(ui, state, theme, narrow);
            neu::toolbar_separator(ui, theme);
            if state.any_popped() {
                dock_all_button(ui, state, theme);
            }
            view_group(ui, state, theme);
            neu::toolbar_separator(ui, theme);
            neu::theme_toggle(ui, theme, &mut state.mode);
        });
        ui.data_mut(|d| d.insert_temp(width_id, group.response.rect.width()));
    });
}

/// The project menu button and the project name with its unsaved dot.
fn project_group(ui: &mut Ui, state: &mut AppState, theme: &Theme, narrow: bool) {
    let button = neu::icon_button_kind(
        ui,
        theme,
        Icon::Menu,
        vec2(BUTTON, BUTTON),
        ButtonKind::Ghost,
    )
    .on_hover_text("Project: new, open, save, export");
    let song = state.bank.mode == PlayMode::Song;
    let export = if song {
        EXPORT_SONG_LABEL
    } else {
        EXPORT_LOOP_LABEL
    };
    let tone = if state.test_tone { "ON" } else { "OFF" };
    let items = [
        MenuItem::keyed("New", Shortcut::New.label()),
        MenuItem::keyed("Open", Shortcut::Open.label()),
        MenuItem::keyed(MENU_RECENT, Shortcut::OpenRecent.label()),
        MenuItem::Separator,
        MenuItem::keyed("Save", Shortcut::Save.label()),
        MenuItem::keyed("Save as", Shortcut::SaveAs.label()),
        MenuItem::Separator,
        MenuItem::keyed(export, Shortcut::Export.label()),
        MenuItem::Separator,
        MenuItem::keyed(MENU_TEST_TONE, tone),
    ];
    if let Some(picked) = neu::menu_popup(ui, theme, &button, &items) {
        match picked {
            0 => {
                state.request_new();
            }
            1 => project_bar::request_open(state),
            2 => {
                state.browser_open = true;
                state.recent_open = true;
            }
            4 => project_bar::save(state),
            5 => project_bar::save_as(state),
            7 => project_bar::export(state),
            9 => state.toggle_test_tone(),
            _ => {}
        }
    }
    if narrow {
        return;
    }
    let name = state
        .project_path
        .as_deref()
        .map_or_else(|| "Untitled".to_owned(), file_label);
    let shown = truncate_name(&name, NAME_CHARS);
    let full = state.project_path.as_deref().map_or_else(
        || "No file yet (Ctrl+S saves)".to_owned(),
        |p| p.display().to_string(),
    );
    neu::brand(ui, theme, &shown).on_hover_text(full);
    if state.dirty {
        unsaved_dot(ui, theme);
    }
}

/// Accessible name of the toolbar's unsaved dot (the browser's file row carries the one
/// named [`UNSAVED_LABEL`]).
pub const UNSAVED_DOT_NAME: &str = "Unsaved dot";

/// The 7 pt accent dot after the project name while there are unsaved changes; accessible
/// as [`UNSAVED_DOT_NAME`], its tooltip [`UNSAVED_LABEL`].
pub fn unsaved_dot(ui: &mut Ui, theme: &Theme) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(7.0, BUTTON), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().circle_filled(rect.center(), 3.5, theme.accent);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, UNSAVED_DOT_NAME));
    response.on_hover_text(UNSAVED_LABEL)
}

/// Undo | redo ghost icons, disabled while there is nothing to step to.
fn history_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let undo = ui
        .add_enabled_ui(state.undo.can_undo(), |ui| {
            neu::icon_button_kind(
                ui,
                theme,
                Icon::Undo,
                vec2(BUTTON, BUTTON),
                ButtonKind::Ghost,
            )
            .on_hover_text(Shortcut::Undo.label())
            .clicked()
        })
        .inner;
    if undo {
        state.undo();
    }
    let redo = ui
        .add_enabled_ui(state.undo.can_redo(), |ui| {
            neu::icon_button_kind(
                ui,
                theme,
                Icon::Redo,
                vec2(BUTTON, BUTTON),
                ButtonKind::Ghost,
            )
            .on_hover_text(Shortcut::Redo.label())
            .clicked()
        })
        .inner;
    if redo {
        state.redo();
    }
}

/// PLAY (primary; the stop glyph while playing, named "Play" throughout) | STOP | REC.
fn transport_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let playing = state.playing;
    let icon = if playing { Icon::Stop } else { Icon::Play };
    let play = neu::icon_button_kind(ui, theme, icon, vec2(BUTTON, BUTTON), ButtonKind::Primary);
    play.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), playing, "Play"));
    if play.on_hover_text(Shortcut::PlayStop.label()).clicked() {
        state.toggle_play();
    }
    if neu::icon_button(ui, theme, Icon::Stop, BUTTON)
        .on_hover_text("Stop and rewind")
        .clicked()
    {
        state.stop();
    }
    if neu::rec_button(ui, theme, state.armed)
        .on_hover_text(format!("Record arm ({})", Shortcut::ToggleArm.label()))
        .clicked()
    {
        state.toggle_arm();
    }
}

/// `Pattern | Song` (`SetPlayMode`).
fn mode_group(ui: &mut Ui, state: &mut AppState, theme: &Theme, narrow: bool) {
    let before = match state.bank.mode {
        PlayMode::Pattern => 0,
        PlayMode::Song => 1,
    };
    let mut selected = before;
    let items = if narrow {
        &MODE_ITEMS_NARROW
    } else {
        &MODE_ITEMS
    };
    neu::segmented(ui, theme, &mut selected, items)
        .on_hover_text("Pattern: loop the selected pattern. Song: play the chain.");
    if selected != before {
        state.set_play_mode(if selected == 0 {
            PlayMode::Pattern
        } else {
            PlayMode::Song
        });
    }
}

/// "Swing" + the inline mini knob + its value (0..=75 % of a step).
fn swing_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::label(ui, theme, "Swing");
    let spec = KnobSpec {
        label: "",
        min: MIN_SWING,
        max: MAX_SWING,
        default: 0.0,
        log: false,
        bipolar: false,
        size: KnobSize::Sm,
        fmt: fmt_percent,
    };
    let before = state.swing;
    let mut swing = before;
    let response = neu::knob(ui, theme, &mut swing, &spec);
    let shown = f64::from(swing);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), shown, "Swing"));
    if response.changed() || swing != before {
        if response.drag_started() {
            state.begin_continuous_edit(EditKey::Swing, true);
        }
        state.set_swing(swing);
    }
    neu::value(ui, theme, &fmt_percent(state.swing));
}

/// The position read-out (`"A · bar 2/4"` or `"SONG 3/5 · B"`).
fn position(ui: &mut Ui, state: &AppState, theme: &Theme) {
    let text = state.position_text();
    neu::text_widget(ui, &text, TextKind::Value, theme.text_2)
        .on_hover_text("Playing pattern · bar (song: chain entry / length · pattern)");
}

/// The master h-slider (72) bound to `MasterGain` and the stereo h-meter (72; wide only).
fn master_group(ui: &mut Ui, state: &mut AppState, theme: &Theme, narrow: bool) {
    let range = ParamId::MasterGain.range();
    let spec = SliderSpec {
        label: MASTER_NAME,
        min: *range.start(),
        max: *range.end(),
        default: ParamId::MasterGain.default_value(),
        fmt: controls::fmt_db,
    };
    let before = state.param(ParamId::MasterGain);
    let mut value = before;
    let response = neu::hslider(ui, theme, &mut value, &spec).on_hover_text(format!(
        "Master {} (double-click resets)",
        controls::fmt_db(before)
    ));
    if response.changed() || value != before {
        state.edit_param(ParamId::MasterGain, value, response.drag_started());
    }
    if !narrow {
        let levels = (state.shown_meters.master[0], state.shown_meters.master[1]);
        neu::meter_horizontal(ui, theme, levels, neu::METER_HORIZONTAL)
            .widget_info(|| WidgetInfo::labeled(Role::Meter, true, MASTER_METER_NAME));
    }
}

/// The hint panel: the status line while one shows, else [`DEFAULT_HINT`]; clipped to
/// `width` (narrow: an empty flex spacer).
fn hint_panel(ui: &mut Ui, state: &AppState, theme: &Theme, width: f32, narrow: bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, BUTTON), Sense::hover());
    let text = state.status_text().unwrap_or(DEFAULT_HINT);
    if !narrow && ui.is_rect_visible(rect) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let colour = if state.status_text().is_some() {
            theme.text
        } else {
            theme.text_2
        };
        let mut format = TextKind::Label.format(colour);
        format.extra_letter_spacing = 0.0;
        let galley = painter.layout_job(egui::epaint::text::LayoutJob::single_section(
            text.to_owned(),
            format,
        ));
        painter.galley(
            pos2(rect.min.x + 4.0, rect.center().y - galley.size().y * 0.5),
            galley,
            colour,
        );
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, HINT_NAME));
    if !narrow {
        response.on_hover_text(text);
    }
}

// ---------------------------------------------------------------------------------------------
// MIDI.

/// The selector's entries: "None", then every listed port (plus the connected port if the
/// list no longer has it, so the selection always has something to point at).
pub fn midi_items(state: &AppState) -> Vec<String> {
    let mut items = vec![MIDI_NONE.to_owned()];
    items.extend(state.midi_ports.iter().cloned());
    if let Some(name) = state.midi_port_name()
        && !state.midi_ports.iter().any(|p| p == name)
    {
        items.push(name.to_owned());
    }
    items
}

/// The MIDI lamp (lit while a port is open) with the "MIDI" label as one button named
/// [`MIDI_NAME`]: a click opens the port menu ("None" | every port). Wide: the connected
/// port's name follows.
fn midi_group(ui: &mut Ui, state: &mut AppState, theme: &Theme, narrow: bool) {
    state.refresh_midi_ports();
    let connected = state.midi_connected();
    let galley = neu::text::layout(ui.painter(), MIDI_NAME, TextKind::Label, theme.text_2);
    let size = vec2(neu::LED_SIZE + 6.0 + galley.size().x, BUTTON);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        neu::paint_led(
            painter,
            pos2(rect.min.x + neu::LED_SIZE * 0.5, rect.center().y),
            connected,
            LedKind::Ok,
            theme,
        );
        let colour = if response.hovered() {
            theme.text
        } else {
            theme.text_2
        };
        neu::text::paint_galley(
            painter,
            pos2(rect.min.x + neu::LED_SIZE + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            galley,
            TextKind::Label,
            colour,
        );
    }
    response
        .widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), connected, MIDI_NAME));
    let items = midi_items(state);
    let entries: Vec<MenuItem<'_>> = items.iter().map(|i| MenuItem::item(i)).collect();
    let response = response.on_hover_text(match state.midi_port_name() {
        Some(name) => format!("MIDI in: {name} · click to change"),
        None => "MIDI in: no device · click to pick a port".to_owned(),
    });
    if let Some(picked) = neu::menu_popup(ui, theme, &response, &entries) {
        if picked == 0 {
            state.disconnect_midi();
        } else if let Some(name) = items.get(picked) {
            let name = name.clone();
            state.connect_midi(&name);
        }
    }
    if !narrow {
        let text = state.midi_port_name().map_or_else(
            || "no device".to_owned(),
            |n| truncate_name(n, MIDI_LABEL_CHARS),
        );
        neu::text_widget(ui, &text, TextKind::Label, theme.text_2);
    }
}

/// DOCK ALL (only while a module is in its own window): every popped-out module back in the
/// main window.
fn dock_all_button(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let count = state.popped_panels().count();
    let tip = if count == 1 {
        "Bring the popped-out window back into the main window".to_owned()
    } else {
        format!("Bring all {count} popped-out windows back into the main window")
    };
    if neu::button_kind(
        ui,
        theme,
        crate::ui::popout::DOCK_ALL_LABEL,
        ButtonKind::Secondary,
        neu::ButtonSize::Small,
    )
    .on_hover_text(tip)
    .clicked()
    {
        state.dock_all();
    }
}

/// The view switcher: one rail button per [`View`] (ON for the showing view) and the F9
/// browser toggle.
fn view_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.spacing_mut().item_spacing.x = 2.0;
    for view in View::ALL {
        let on = state.view == view;
        let tooltip = match view.key() {
            Some(key) => format!("{} ({key})", view.title()),
            None => view.title().to_owned(),
        };
        let response = neu::rail_button(ui, theme, view.icon(), on, &tooltip);
        let name = view.button_name();
        response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, name));
        if response.clicked() {
            if view == View::Mixer {
                state.toggle_mixer_drawer();
            } else {
                state.set_view(view);
            }
        }
    }
    let response = neu::rail_button(
        ui,
        theme,
        Icon::Browser,
        state.browser_open,
        &format!("Browser ({})", Shortcut::ToggleBrowser.label()),
    );
    let on = state.browser_open;
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, BROWSER_NAME));
    if response.clicked() {
        state.toggle_browser();
    }
    ui.spacing_mut().item_spacing.x = 5.0;
}
