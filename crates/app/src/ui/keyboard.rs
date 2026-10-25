//! KEYS panel (M1): the on-screen piano, the octave stepper and the velocity slider.
//!
//! Layout follows `design/preview-a.html` `<section class="panel piano">`: a panel head with
//! the title on the left and the controls on the right (the mock's `Z` / `X` buttons are the
//! keyboard shortcuts, shown here as hints beside the `neu::stepper` buttons), then two
//! octaves of keys (14 white keys, 1170 pt wide). Keys light up while held via the computer
//! keyboard or the mouse; a mouse press plays the same `NoteEvent`s as a key press.
//!
//! Note numbering: the leftmost key is C of the current octave, MIDI `(octave + 1) * 12`, so
//! octave 4 starts on C4 = 60 (see `crate::input`).
//!
//! The controls group is built left-to-right and pushed to the right edge using the width it
//! had last frame (egui's right-to-left layouts put nested vertical groups at the far left).

use egui::{Align2, Sense, Ui, vec2};
use neu::{PianoModel, TextKind, Theme};

use crate::app::AppState;
use crate::input::{KeyMap, MAX_OCTAVE, MAX_VELOCITY, MIN_OCTAVE, MIN_VELOCITY};

/// White keys shown (two octaves).
pub const WHITE_KEYS: usize = 14;

/// Height of a stepper button row (`neu::stepper`: 28 pt round buttons).
const STEPPER_ROW: f32 = 28.0;
/// Gap between a control's caps label and its row (matches `neu::stepper`).
const LABEL_GAP: f32 = 6.0;

/// Renders the KEYS panel into `ui` and applies any control or key changes to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        head(ui, state, theme);
        ui.add_space(theme.space_3);
        piano(ui, state, theme);
    });
}

/// Height of the caps label row used above the stepper and the slider.
fn label_height(ui: &Ui) -> f32 {
    ui.ctx()
        .fonts_mut(|f| f.row_height(&TextKind::Label.font_id()))
}

fn head(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let row_height = label_height(ui) + LABEL_GAP + STEPPER_ROW;
    ui.horizontal(|ui| {
        ui.set_min_height(row_height);
        neu::title(ui, theme, "Keys");

        // Right-align the controls using the width they measured last frame.
        let width_id = ui.scope_id().with("keys_head_controls_width");
        let known_width: Option<f32> = ui.data(|d| d.get_temp(width_id));
        if let Some(width) = known_width {
            let pad = ui.available_width() - width;
            if pad > 0.0 {
                ui.add_space(pad);
            }
        }
        let group = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.space_3;
            octave_group(ui, state, theme);
            ui.add_space(theme.space_3);
            velocity_group(ui, state, theme);
        });
        ui.data_mut(|d| d.insert_temp(width_id, group.response.rect.width()));
    });
}

/// A small secondary-colour shortcut hint, vertically centred on the stepper's button row
/// (so it sits beside the (-) / (+) buttons, under the label line).
fn hint(ui: &mut Ui, theme: &Theme, text: &str) {
    let label_h = label_height(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = LABEL_GAP;
        ui.add_space(label_h);
        let galley = ui.painter().layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(11.0),
            theme.text_secondary,
        );
        let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x, STEPPER_ROW), Sense::hover());
        if ui.is_rect_visible(rect) {
            let pos = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size());
            ui.painter().galley(pos.min, galley, theme.text_secondary);
        }
    });
}

/// `Z (-) 4 (+) X`: the octave stepper plus the two shortcut hints.
fn octave_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    hint(ui, theme, "Z");
    let mut octave = state.input.octave;
    if neu::stepper(ui, theme, "Octave", &mut octave, MIN_OCTAVE..=MAX_OCTAVE).changed() {
        state.input.set_octave(octave);
    }
    hint(ui, theme, "X");
}

fn velocity_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let mut velocity = state.input.velocity;
    // Label beside a label-less small knob keeps the header row at the stepper height; the
    // knob shows its value as a tooltip.
    let spec = neu::KnobSpec {
        label: "",
        min: MIN_VELOCITY,
        max: MAX_VELOCITY,
        default: DEFAULT_VELOCITY,
        log: false,
        bipolar: false,
        size: neu::KnobSize::Sm,
        fmt: fmt_velocity,
    };
    let row_height = label_height(ui) + LABEL_GAP + STEPPER_ROW;
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(row_height);
        ui.spacing_mut().item_spacing.x = theme.space_2;
        neu::label(ui, theme, "Velocity");
        if neu::knob(ui, theme, &mut velocity, &spec).changed() {
            state.input.set_velocity(velocity);
        }
    });
}

/// Velocity knob default (the fixed velocity of played notes until the user changes it).
const DEFAULT_VELOCITY: f32 = 0.8;

fn fmt_velocity(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}

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
