//! Knobs and the concentric-ring dial (M2 stub: the neu subagent implements the real painting;
//! keep these signatures, see docs/ARCHITECTURE.md "M2 additions / neu").

use egui::{Response, Sense, Ui, vec2};

use crate::Theme;

/// Knob size: `Md` (cap 40, arc 50, block 52) or `Sm` (cap 26, arc 34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobSize {
    Md,
    Sm,
}

/// Dial size: `Lg` (arc 96, rings 84/64/44) or `Sm` (arc 72, rings 62/46/32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialSize {
    Lg,
    Sm,
}

/// How a knob or dial maps and displays its value.
#[derive(Clone, Copy)]
pub struct KnobSpec<'a> {
    /// Caps label shown above the control.
    pub label: &'a str,
    pub min: f32,
    pub max: f32,
    /// Value restored by a double-click.
    pub default: f32,
    /// Map the drag linearly in log space (for frequencies).
    pub log: bool,
    /// Draw the value arc from 12 o'clock instead of from the minimum.
    pub bipolar: bool,
    pub size: KnobSize,
    /// Formats the value for display.
    pub fmt: fn(f32) -> String,
}

/// Default value formatter: two significant decimals.
pub fn fmt_plain(v: f32) -> String {
    format!("{v:.2}")
}

/// A rotary knob bound to `value`. `changed()` when the value moved; double-click resets.
pub fn knob(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &KnobSpec<'_>) -> Response {
    let size = match spec.size {
        KnobSize::Md => 52.0,
        KnobSize::Sm => 34.0,
    };
    let (rect, response) = ui.allocate_exact_size(vec2(size, size + 30.0), Sense::click_and_drag());
    if response.double_clicked() {
        *value = spec.default;
    }
    ui.painter().rect_filled(rect, 8.0, theme.surface);
    response
}

/// The concentric-ring dial (master volume, filter cutoff). Same interaction as [`knob`].
pub fn dial(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &KnobSpec<'_>,
    size: DialSize,
) -> Response {
    let px = match size {
        DialSize::Lg => 96.0,
        DialSize::Sm => 72.0,
    };
    let (rect, response) = ui.allocate_exact_size(vec2(px, px), Sense::click_and_drag());
    if response.double_clicked() {
        *value = spec.default;
    }
    ui.painter()
        .circle_filled(rect.center(), px * 0.5, theme.surface);
    response
}
