//! Thin vertical slider and the indicator lamp (M2 stub: the neu subagent implements the real
//! painting; keep these signatures).

use egui::{Response, Sense, Ui, vec2};

use crate::Theme;

/// How a slider maps and displays its value.
#[derive(Clone, Copy)]
pub struct SliderSpec<'a> {
    /// Caps label shown below the slider.
    pub label: &'a str,
    pub min: f32,
    pub max: f32,
    /// Value restored by a double-click.
    pub default: f32,
    /// Formats the value for display.
    pub fmt: fn(f32) -> String,
}

/// Thin vertical slider (track 6x100, 14 pt accent thumb). `changed()` when the value moved.
pub fn vslider(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &SliderSpec<'_>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 140.0), Sense::click_and_drag());
    if response.double_clicked() {
        *value = spec.default;
    }
    ui.painter().rect_filled(rect, 3.0, theme.surface);
    response
}

/// A 10 pt indicator lamp with a caps label (inset when off, accent glow when on).
pub fn lamp(ui: &mut Ui, theme: &Theme, on: bool, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(48.0, 28.0), Sense::hover());
    let fill = if on { theme.accent } else { theme.shadow_dark };
    ui.painter().circle_filled(rect.left_center(), 5.0, fill);
    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        label,
        egui::FontId::proportional(11.0),
        theme.text_secondary,
    );
    response
}
