//! The KEYBOARD strip (M1; Deck since M12, WINNER section 5 "KEYBOARD strip"): a 96 pt
//! window (header 28 + body 68) at the bottom of the shell, collapsible to its header.
//!
//! Header: "KEYBOARD" · sub "A W S E D F T G Y H U J K O L P ; · Z / X octave" (collapsed:
//! "· oct 4 · vel 80 %" after it) · sep · "Oct" · `-` · the octave · `+` · sep · "Vel" ·
//! the 24 pt h-slider · the value · spacer · the audio line ("48 kHz · 256 · 5.3 ms", or
//! "no audio device") · chevron · close. Body: two octaves of keys (14 white keys, 784 pt,
//! centred). Keys light up while held via the computer keyboard or the mouse; a mouse press
//! plays the same `NoteEvent`s as a key press.
//!
//! Note numbering: the leftmost key is C of the current octave, MIDI `(octave + 1) * 12`, so
//! octave 4 starts on C4 = 60 (see `crate::input`).
//!
//! Accessible names: the octave readout [`OCTAVE_LABEL`] ("Keyboard octave", its value the
//! octave number; "Octave" is a chord of the roll's CHORDS column), the buttons
//! [`OCTAVE_DOWN`] / [`OCTAVE_UP`], the velocity slider "Velocity", the chevron
//! [`COLLAPSE_LABEL`] / [`EXPAND_LABEL`], the close button [`CLOSE_LABEL`], the audio line
//! [`AUDIO_INFO_NAME`].

use egui::{Role, Ui, WidgetInfo, vec2};
use neu::slider::SliderSpec;
use neu::{ButtonKind, Icon, PianoModel, TextKind, Theme};

use crate::app::AppState;
use crate::input::{KeyMap, MAX_OCTAVE, MAX_VELOCITY, MIN_OCTAVE, MIN_VELOCITY};

/// White keys shown (two octaves).
pub const WHITE_KEYS: usize = 14;
/// Height of the open strip (header 28 + body 68) and the collapsed header.
pub const HEIGHT: f32 = 96.0;
pub const COLLAPSED_HEIGHT: f32 = neu::WINDOW_HEADER;
/// Window title.
pub const TITLE: &str = "Keyboard";
/// The header's sub-title: the key map.
pub const KEY_MAP_SUB: &str = "A W S E D F T G Y H U J K O L P ; · Z / X octave";
/// Accessible names.
pub const OCTAVE_LABEL: &str = "Keyboard octave";
pub const OCTAVE_DOWN: &str = "Octave down";
pub const OCTAVE_UP: &str = "Octave up";
pub const VELOCITY_LABEL: &str = "Velocity";
pub const COLLAPSE_LABEL: &str = "Collapse keyboard";
pub const EXPAND_LABEL: &str = "Expand keyboard";
pub const HIDE_LABEL: &str = "Hide keyboard";
pub const CLOSE_LABEL: &str = "Close keyboard";
pub const AUDIO_INFO_NAME: &str = "Audio device";
/// Velocity slider default (the fixed velocity of played notes until the user changes it).
pub const DEFAULT_VELOCITY: f32 = 0.8;

/// `0.8` -> `"80 %"`.
pub fn fmt_velocity(v: f32) -> String {
    format!("{} %", (v * 100.0).round() as i32)
}

/// The audio line: `"48 kHz"` with the device's sample rate, or `"no audio device"`.
#[must_use]
pub fn audio_info(state: &AppState) -> String {
    match (&state.audio, &state.audio_error) {
        (Some(audio), _) => format!(
            "{} kHz · {} ch",
            (audio.sample_rate() / 1000.0 * 10.0).round() / 10.0,
            audio.channels()
        ),
        (None, Some(_)) => "no audio device".to_owned(),
        (None, None) => format!("{} kHz · offline", (state.sample_rate() / 1000.0).round()),
    }
}

/// The header's sub-title for the current state.
#[must_use]
pub fn header_sub(state: &AppState) -> String {
    if state.keyboard_open {
        KEY_MAP_SUB.to_owned()
    } else {
        format!(
            "{KEY_MAP_SUB} · oct {} · vel {}",
            state.input.octave,
            fmt_velocity(state.input.velocity)
        )
    }
}

/// Renders the KEYBOARD strip into the available rect (the caller gives it [`HEIGHT`] or
/// [`COLLAPSED_HEIGHT`]) and applies any control or key changes to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let size = ui.available_size_before_wrap();
    let open = state.keyboard_open && size.y > COLLAPSED_HEIGHT + 0.5;
    let sub = header_sub(state);
    // The header and the body both edit the state; the frame runs them one after the other.
    let cell = std::cell::RefCell::new(state);
    neu::window_frame_sized(
        ui,
        theme,
        vec2(size.x, if open { size.y } else { COLLAPSED_HEIGHT }),
        TITLE,
        Some(&sub),
        |ui| header_controls(ui, &mut cell.borrow_mut(), theme, open),
        |ui| {
            if open {
                piano(ui, &mut cell.borrow_mut(), theme);
            }
        },
    );
}

/// The header's controls, laid out right-to-left: close, the chevron, the audio line, the
/// velocity group, the octave group.
fn header_controls(ui: &mut Ui, state: &mut AppState, theme: &Theme, open: bool) {
    let close = neu::header_button(ui, theme, Icon::Clear);
    close.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), CLOSE_LABEL));
    if close
        .on_hover_text("Hide the keys (the header stays)")
        .clicked()
    {
        state.keyboard_open = false;
    }
    let (icon, name, tip) = if open {
        (Icon::ChevronDown, COLLAPSE_LABEL, "Collapse to the header")
    } else if state.keyboard_open {
        // The in-flow bar while the keys float over the workspace (narrow).
        (Icon::ChevronDown, HIDE_LABEL, "Hide the floating keys")
    } else {
        (Icon::ChevronUp, EXPAND_LABEL, "Show the keys")
    };
    let chevron = neu::header_button(ui, theme, icon);
    chevron.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), name));
    if chevron.on_hover_text(tip).clicked() {
        state.toggle_keyboard();
    }
    let info = audio_info(state);
    neu::text_widget(ui, &info, TextKind::Small, theme.text_3)
        .widget_info(|| WidgetInfo::labeled(Role::Label, true, AUDIO_INFO_NAME));
    ui.add_space(8.0);
    // Velocity (right to left: value, slider, label).
    neu::value(ui, theme, &fmt_velocity(state.input.velocity));
    let spec = SliderSpec {
        label: VELOCITY_LABEL,
        min: MIN_VELOCITY,
        max: MAX_VELOCITY,
        default: DEFAULT_VELOCITY,
        fmt: fmt_velocity,
    };
    let mut velocity = state.input.velocity;
    let response = neu::hslider_sized(ui, theme, &mut velocity, &spec, vec2(72.0, 24.0))
        .on_hover_text("Velocity of played notes (double-click resets)");
    if response.changed() || velocity != state.input.velocity {
        state.input.set_velocity(velocity);
    }
    neu::label(ui, theme, "Vel");
    neu::toolbar_separator(ui, theme);
    // Octave (right to left: +, value, -, label).
    let up = neu::icon_button_kind(
        ui,
        theme,
        Icon::Plus,
        vec2(22.0, 22.0),
        ButtonKind::Secondary,
    );
    up.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), OCTAVE_UP));
    if up.on_hover_text("Octave up (X)").clicked() {
        state
            .input
            .set_octave((state.input.octave + 1).min(MAX_OCTAVE));
    }
    let value = state.input.octave.to_string();
    let (rect, response) = ui.allocate_exact_size(vec2(16.0, 24.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        neu::text::paint_text(
            ui.painter(),
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &value,
            TextKind::Value,
            theme.text,
        );
    }
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Button, true, OCTAVE_LABEL);
        info.current_text_value = Some(value.clone());
        info
    });
    let down = neu::icon_button_kind(
        ui,
        theme,
        Icon::Minus,
        vec2(22.0, 22.0),
        ButtonKind::Secondary,
    );
    down.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), OCTAVE_DOWN));
    if down.on_hover_text("Octave down (Z)").clicked() {
        state
            .input
            .set_octave((state.input.octave - 1).max(MIN_OCTAVE));
    }
    neu::label(ui, theme, "Oct");
}

/// The keys, centred in the body.
fn piano(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let labels = KeyMap::labels(state.input.octave);
    let model = PianoModel {
        base_note: state.input.base_note(),
        white_keys: WHITE_KEYS,
        held: &state.held_notes,
        labels: &labels,
    };
    let out = ui
        .with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.add_space(2.0);
            neu::piano_keys(ui, theme, &model)
        })
        .inner;

    // Mouse presses play exactly like the computer keyboard (same velocity, same messages).
    if let Some(note) = out.released {
        state.note_off(note);
    }
    if let Some(note) = out.pressed {
        state.note_on(note, state.input.velocity);
    }
}
