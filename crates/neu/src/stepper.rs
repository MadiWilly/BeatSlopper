//! "(-) value (+)" stepper: a label (12 / 500 caps, `text_2`) above a row of a 28 pt square
//! minus button, the value (13 tabular, centred in at least [`STEPPER_VALUE_MIN_WIDTH`]) and
//! a 28 pt square plus button ([`crate::button::stepper_button`]: `panel_3`, `border_2`,
//! radius 4), with a [`STEPPER_GAP`] between them. Clicks step the value by one and clamp it
//! to the range.
//!
//! QOL pass: a click on the value opens the inline editor of [`crate::value_edit`] (Enter
//! commits the typed integer, clamped; Escape cancels) and the wheel over the row steps the
//! value by one per notch. The value is a button named `"<label> value"`.

use std::ops::RangeInclusive;

use egui::{Align2, Response, Sense, Ui, vec2};

use crate::Theme;
use crate::button::{Icon, stepper_button};
use crate::knob::readout_name;
use crate::text::{self, TextKind};
use crate::value_edit::{parse_int, readout, wheel_steps};

/// Minimum width of the value column (34, the BPM display's minimum).
pub const STEPPER_VALUE_MIN_WIDTH: f32 = 34.0;
/// Gap between the stepper's buttons and its value.
pub const STEPPER_GAP: f32 = 6.0;
/// Vertical gap between the label and the button row.
const LABEL_GAP: f32 = 4.0;

/// A labelled integer stepper: two 28 pt square buttons around a tabular value.
/// `changed()` on the returned response is true when a button fired (and the value moved).
pub fn stepper(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    value: &mut i32,
    range: RangeInclusive<i32>,
) -> Response {
    stepper_with(ui, theme, label, value, range, |v| v.to_string())
}

/// [`stepper`] with a custom value formatter (e.g. `|v| format!("{v}%")`).
pub fn stepper_with(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    value: &mut i32,
    range: RangeInclusive<i32>,
    format: impl Fn(i32) -> String,
) -> Response {
    // A value outside the range is pulled in silently; only clicks count as a change.
    *value = (*value).clamp(*range.start(), *range.end());
    let before = *value;

    let (lo, hi) = (*range.start(), *range.end());
    let inner = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = LABEL_GAP;
        text::label(ui, theme, label);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = STEPPER_GAP;
            if stepper_button(ui, theme, Icon::Minus).clicked() {
                *value = (*value - 1).max(lo);
            }
            if let Some(typed) = value_cell(ui, theme, &format(*value), label)
                && let Some(parsed) = parse_int(&typed, lo, hi)
            {
                *value = parsed;
            }
            if stepper_button(ui, theme, Icon::Plus).clicked() {
                *value = (*value + 1).min(hi);
            }
        });
    });

    let mut response = inner.response;
    let steps = wheel_steps(ui, &response);
    if steps != 0 {
        *value = (*value + steps).clamp(lo, hi);
    }
    if *value != before {
        response.mark_changed();
    }
    response
}

/// The centred value text, at least [`STEPPER_VALUE_MIN_WIDTH`] wide: a readout that opens
/// the inline editor on click. Returns the committed text, if any.
fn value_cell(ui: &mut Ui, theme: &Theme, text: &str, label: &str) -> Option<String> {
    let galley = text::layout(ui.painter(), text, TextKind::Value, theme.text);
    let size = vec2(
        galley.size().x.max(STEPPER_VALUE_MIN_WIDTH),
        galley.size().y.max(28.0),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    readout(
        ui,
        theme,
        response.id.with("readout"),
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        TextKind::Value.font_id(),
        theme.text,
        &readout_name(label),
    )
}
