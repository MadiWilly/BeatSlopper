//! Transport bar: wordmark, Play / Stop / Record, BPM stepper, SWING knob, the MASTER dial,
//! the test-tone toggle and the theme toggle.
//!
//! Layout follows `design/preview-a.html` `<header class="transport">`: brand on the left
//! (name 15/300 caps, sub-label 11 secondary), the three 48 pt icon buttons, the BPM
//! `stepper` (60..=200, bound to `SetBpm`), the SWING small knob (0..=75 %, bound to
//! `SetSwing`), the large concentric dial (`neu::dial` Lg bound to `MasterGain`, shown in %),
//! then flush right: the theme toggle (sun, switch, moon) and a small Test tone toggle.
//!
//! Play and Record show their ON state as an accent fill with the on-glow (the mock's
//! `.icon-btn.on`); `neu::icon_button` has no on-state, so [`transport_button`] paints the
//! same circle with `neu::paint_soft_circle_with_fill` when on.
//!
//! The row is as tall as the dial. egui's horizontal layout pushes items taller than the row
//! *down* rather than centring everything on them, so the dial's height from the previous
//! frame sets the row height and the shorter groups centre on it.

use egui::{Align, Layout, Rect, Response, RichText, Role, Sense, Stroke, Ui, WidgetInfo, vec2};
use engine::ParamId;
use neu::button::ICON_STROKE;
use neu::shadow::Depth;
use neu::{DialSize, Icon, KnobSize, KnobSpec, Theme};

use crate::app::{AppState, MAX_BPM, MAX_SWING, MIN_BPM, MIN_SWING};
use crate::ui::controls::{Knob, fmt_percent};

/// Minimum height of the transport bar's content row (the dial normally sets it).
const ROW_HEIGHT: f32 = 48.0;
/// Diameter of the transport icon buttons.
pub const TRANSPORT_BUTTON: f32 = 48.0;
/// Gap between the transport buttons (`.tp-group{gap:10px}`).
const BUTTON_GAP: f32 = 10.0;

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

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme.space_6;
                theme_group(ui, state, theme);
                test_tone_group(ui, state, theme);
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
            None => label(ui, theme, "M3 · SOFT CONSOLE"),
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
        if transport_button(ui, theme, Icon::Play, state.playing).clicked() {
            state.toggle_play();
        }
        if transport_button(ui, theme, Icon::Stop, false).clicked() {
            state.stop();
        }
        if transport_button(ui, theme, Icon::Record, state.armed).clicked() {
            state.toggle_arm();
        }
    });
}

/// The BPM stepper (whole BPM, 60..=200).
fn bpm_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let mut bpm = state.bpm.round() as i32;
    let range = MIN_BPM as i32..=MAX_BPM as i32;
    if neu::stepper(ui, theme, "BPM", &mut bpm, range).changed() {
        state.set_bpm(bpm as f32);
    }
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

fn theme_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    // The switch paints its own sun/moon icons; flush right in the bar.
    neu::theme_toggle(ui, theme, &mut state.mode);
}
