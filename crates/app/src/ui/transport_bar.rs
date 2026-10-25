//! Transport bar: wordmark, Play / Stop / Record, BPM group, SWING knob, the MASTER dial,
//! the MODE selector with the position read-out, the test-tone toggle and the theme toggle.
//!
//! Layout follows `design/preview-a.html` `<header class="transport">`: brand on the left
//! (name 15/300 caps, sub-label 11 secondary), the three 48 pt icon buttons, the BPM
//! `stepper` (60..=200, bound to `SetBpm`), the SWING small knob (0..=75 %, bound to
//! `SetSwing`), the large concentric dial (`neu::dial` Lg bound to `MasterGain`, shown in %),
//! the MODE group (M5: `Pattern | Song` `segmented` bound to `SetPlayMode` over the position
//! text `"A · bar 2/4"` / `"SONG 3/5 · B"` from `Position`),
//! then flush right: a small Test tone toggle, the MIDI group (M4: a "MIDI" lamp lit while a
//! port is open, the connected port's name, and a port selector with a "None" entry) and the
//! theme toggle (sun, switch, moon).
//!
//! Play and Record show their ON state as an accent fill with the on-glow (the mock's
//! `.icon-btn.on`); `neu::icon_button` has no on-state, so [`transport_button`] paints the
//! same circle with `neu::paint_soft_circle_with_fill` when on.
//!
//! The row is as tall as the dial. egui's horizontal layout pushes items taller than the row
//! *down* rather than centring everything on them, so the dial's height from the previous
//! frame sets the row height and the shorter groups centre on it. The right-hand group is
//! built left-to-right and pushed to the right edge using the width it had last frame (egui's
//! right-to-left layouts misplace nested vertical groups such as the MIDI block).

use egui::{Align, Layout, Rect, Response, RichText, Role, Sense, Stroke, Ui, WidgetInfo, vec2};
use engine::{ParamId, PlayMode};
use neu::button::ICON_STROKE;
use neu::shadow::Depth;
use neu::{DialSize, Icon, KnobSize, KnobSpec, Theme};

use crate::app::{AppState, EditKey, MAX_SWING, MIN_SWING, truncate_name};
use crate::shortcuts::Shortcut;
use crate::ui::controls::{Knob, fmt_percent};

/// Minimum height of the transport bar's content row (the dial normally sets it).
const ROW_HEIGHT: f32 = 48.0;
/// Diameter of the transport icon buttons.
pub const TRANSPORT_BUTTON: f32 = 48.0;
/// Gap between the transport buttons (`.tp-group{gap:10px}`).
const BUTTON_GAP: f32 = 10.0;
/// Up to this many ports the selector is a `neu::segmented`; above it an egui `ComboBox`.
pub const MIDI_SEGMENTED_MAX_PORTS: usize = 3;
/// The selector's "disconnected" entry.
pub const MIDI_NONE: &str = "None";
/// Longest port name shown in the segmented selector's items.
const MIDI_ITEM_CHARS: usize = 12;
/// Longest port name shown in the connected-port label.
const MIDI_LABEL_CHARS: usize = 18;
/// The MODE selector's items, in `PlayMode` order (Pattern, Song).
pub const MODE_ITEMS: [&str; 2] = ["Pattern", "Song"];

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
            transport_group(ui, state, theme);
            bpm_group(ui, state, theme);
            swing_group(ui, state, theme);
            let dial = master_dial(ui, state, theme);
            ui.data_mut(|d| d.insert_temp(height_id, dial.rect.height()));
            mode_group(ui, state, theme);

            // Right-align the remaining groups using the width they measured last frame.
            let width_id = ui.scope_id().with("transport_right_width");
            let known_width: Option<f32> = ui.data(|d| d.get_temp(width_id));
            if let Some(width) = known_width {
                let pad = ui.available_width() - width;
                if pad > 0.0 {
                    ui.add_space(pad);
                }
            }
            // `with_layout` takes the whole remaining rect, so the group spans the full row
            // height and its items centre on the same line as the dial and the buttons.
            let group = ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme.space_6;
                test_tone_group(ui, state, theme);
                midi_group(ui, state, theme);
                theme_group(ui, state, theme);
            });
            ui.data_mut(|d| d.insert_temp(width_id, group.response.rect.width()));
        });
    });
}

fn label(ui: &mut Ui, theme: &Theme, text: &str) -> egui::Response {
    ui.label(RichText::new(text).size(11.0).color(theme.text_secondary))
}

/// The wordmark (`neu::brand`: 15 pt caps with the mock's +0.16em tracking) over the status
/// sub-label.
fn brand(ui: &mut Ui, theme: &Theme, audio_error: Option<&str>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.space_1;
        neu::brand(ui, theme, "BeatSlopper");
        match audio_error {
            None => label(ui, theme, "SOFT CONSOLE"),
            Some(err) if err.contains("stream error") => label(ui, theme, "AUDIO STREAM ERROR"),
            Some(_) => label(ui, theme, "NO AUDIO DEVICE"),
        };
    });
}

/// A 48 pt circular transport button with an ON state: raised-m (inset-m while pressed) when
/// off, accent fill with the on-glow and the glyph in the on-accent colour when `on`.
pub fn transport_button(ui: &mut Ui, theme: &Theme, icon: Icon, on: bool) -> Response {
    let diameter = TRANSPORT_BUTTON;
    let (rect, response) = ui.allocate_exact_size(vec2(diameter, diameter), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = response.hovered();
        let color = if on {
            let fill = if hovered {
                theme.accent_hover
            } else {
                theme.accent
            };
            neu::paint_soft_circle_with_fill(
                painter,
                rect.center(),
                diameter * 0.5,
                Depth::OnGlow,
                fill,
                theme,
            );
            theme.on_accent
        } else {
            let depth = if response.is_pointer_button_down_on() {
                Depth::InsetM
            } else {
                Depth::RaisedM
            };
            neu::paint_soft_circle(painter, rect.center(), diameter * 0.5, depth, theme);
            if hovered {
                theme.text_primary
            } else {
                theme.text_secondary
            }
        };
        let box_size = diameter * 20.0 / 48.0;
        let icon_rect = Rect::from_center_size(rect.center(), vec2(box_size, box_size));
        neu::paint_icon(painter, icon, icon_rect, Stroke::new(ICON_STROKE, color));
    }
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, icon.name()));
    response
}

/// Play (on while the loop runs) | Stop | Record arm (on while armed).
fn transport_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        if transport_button(ui, theme, Icon::Play, state.playing)
            .on_hover_text(Shortcut::PlayStop.label())
            .clicked()
        {
            state.toggle_play();
        }
        if transport_button(ui, theme, Icon::Stop, false).clicked() {
            state.stop();
        }
        if transport_button(ui, theme, Icon::Record, state.armed)
            .on_hover_text(Shortcut::ToggleArm.label())
            .clicked()
        {
            state.toggle_arm();
        }
    });
}

/// The BPM group (QOL: typed entry, one decimal, TAP; see `crate::ui::tempo`).
fn bpm_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    crate::ui::tempo::bpm_group(ui, state, theme);
}

/// The SWING small knob, 0..=75 % of a step.
fn swing_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let spec = KnobSpec {
        label: "Swing",
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
    if response.changed() || swing != before {
        if response.drag_started() {
            state.begin_continuous_edit(EditKey::Swing, true);
        }
        state.set_swing(swing);
    }
}

fn test_tone_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    if neu::toggle_button(ui, theme, "Test tone", state.test_tone).clicked() {
        state.toggle_test_tone();
    }
}

/// The hero control: the large dial bound to `MasterGain` (label MASTER, value in %).
fn master_dial(ui: &mut Ui, state: &mut AppState, theme: &Theme) -> egui::Response {
    Knob::new(ParamId::MasterGain, fmt_percent)
        .label("Master")
        .show_dial(ui, state, theme, DialSize::Lg)
}

/// MODE: `Pattern | Song` (`SetPlayMode`) over the position read-out (`"A · bar 2/4"` or
/// `"SONG 3/5 · B"`), as a column centred on the row like the MIDI group.
fn mode_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let height_id = ui.scope_id().with("transport_mode_height");
    let known_height: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(0.0);
    let column = ui.allocate_ui_with_layout(
        vec2(0.0, known_height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = theme.space_2;
            neu::label(ui, theme, "Mode");
            let before = match state.bank.mode {
                PlayMode::Pattern => 0,
                PlayMode::Song => 1,
            };
            let mut selected = before;
            neu::segmented(ui, theme, &mut selected, &MODE_ITEMS)
                .on_hover_text("Pattern: loop the selected pattern. Song: play the chain.");
            if selected != before {
                state.set_play_mode(if selected == 0 {
                    PlayMode::Pattern
                } else {
                    PlayMode::Song
                });
            }
            let text = state.position_text();
            ui.label(RichText::new(&text).size(12.0).color(theme.text_primary))
                .widget_info(|| WidgetInfo::labeled(Role::Label, ui.is_enabled(), &text));
        },
    );
    ui.data_mut(|d| d.insert_temp(height_id, column.response.rect.height()));
}

fn theme_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    // The switch paints its own sun/moon icons; flush right in the bar.
    neu::theme_toggle(ui, theme, &mut state.mode);
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

/// Lamp + port name over the selector, vertically centred on the row. A zero-height column
/// would start at the row's centre line, so the column is allocated with the height it
/// measured last frame and egui centres it.
fn midi_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    state.refresh_midi_ports();
    let height_id = ui.scope_id().with("transport_midi_height");
    let known_height: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(0.0);
    let column = ui.allocate_ui_with_layout(
        vec2(0.0, known_height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = theme.space_2;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.space_3;
                neu::lamp(ui, theme, state.midi_connected(), "MIDI");
                let text = state.midi_port_name().map_or_else(
                    || "NO DEVICE".to_owned(),
                    |n| truncate_name(n, MIDI_LABEL_CHARS),
                );
                label(ui, theme, &text);
            });
            midi_selector(ui, state, theme);
        },
    );
    ui.data_mut(|d| d.insert_temp(height_id, column.response.rect.height()));
}

/// "None" | port names: a `neu::segmented` for a few ports, an egui `ComboBox` for many.
/// Picking a port connects to it; picking "None" disconnects.
fn midi_selector(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let items = midi_items(state);
    let before = state
        .midi_port_name()
        .and_then(|name| items.iter().position(|item| item == name))
        .unwrap_or(0);
    let mut selected = before;
    if items.len() <= MIDI_SEGMENTED_MAX_PORTS + 1 {
        let short: Vec<String> = items
            .iter()
            .map(|item| truncate_name(item, MIDI_ITEM_CHARS))
            .collect();
        let refs: Vec<&str> = short.iter().map(String::as_str).collect();
        neu::segmented(ui, theme, &mut selected, &refs);
    } else {
        egui::ComboBox::from_id_salt("midi_port")
            .width(150.0)
            .selected_text(
                RichText::new(truncate_name(&items[before], MIDI_LABEL_CHARS))
                    .size(12.0)
                    .color(theme.text_primary),
            )
            .show_ui(ui, |ui| {
                for (i, item) in items.iter().enumerate() {
                    ui.selectable_value(&mut selected, i, item.as_str());
                }
            });
    }
    if selected != before {
        if selected == 0 {
            state.disconnect_midi();
        } else if let Some(name) = items.get(selected) {
            let name = name.clone();
            state.connect_midi(&name);
        }
    }
}
