//! "(-) value (+)" stepper (`.stepper` in `design/preview-a.html`).
//!
//! A caps label (11/400, secondary) above a row of a 28 pt minus button, the value (13/400
//! tabular, centred in at least 36 pt) and a 28 pt plus button, with a 10 pt gap between them.
//! Clicks step the value by one and clamp it to the range.

use std::ops::RangeInclusive;

use egui::{Align2, Response, Sense, Ui, vec2};

use crate::Theme;
use crate::button::{Icon, stepper_button};
use crate::text::{self, TextKind};

/// Minimum width of the value column (`.stepper .val` `min-width`).
pub const STEPPER_VALUE_MIN_WIDTH: f32 = 36.0;
/// Gap between the stepper's buttons and its value (`.stepper` `gap`).
pub const STEPPER_GAP: f32 = 10.0;
/// Vertical gap between the label and the button row.
const LABEL_GAP: f32 = 6.0;

/// A labelled integer stepper: two 28 pt round buttons around a tabular value.
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

    let inner = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = LABEL_GAP;
        text::label(ui, theme, label);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = STEPPER_GAP;
            if stepper_button(ui, theme, Icon::Minus).clicked() {
                *value = (*value - 1).max(*range.start());
            }
            value_cell(ui, theme, &format(*value));
            if stepper_button(ui, theme, Icon::Plus).clicked() {
                *value = (*value + 1).min(*range.end());
            }
        });
    });

    let mut response = inner.response;
    if *value != before {
        response.mark_changed();
    }
    response
}

/// The centred value text, at least [`STEPPER_VALUE_MIN_WIDTH`] wide.
fn value_cell(ui: &mut Ui, theme: &Theme, text: &str) -> Response {
    let galley = text::layout(ui.painter(), text, TextKind::Value, theme.text_primary);
    let size = vec2(
        galley.size().x.max(STEPPER_VALUE_MIN_WIDTH),
        galley.size().y,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        text::paint_galley(
            ui.painter(),
            rect.center(),
            Align2::CENTER_CENTER,
            galley,
            TextKind::Value,
            theme.text_primary,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), text));
    response
}
