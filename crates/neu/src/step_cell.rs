//! Sequencer step cells and playhead dots (M3 stub: the neu subagent implements the real
//! painting; keep these signatures, see docs/ARCHITECTURE.md "M3 additions / neu").

use egui::{Response, Sense, Ui, vec2};

use crate::Theme;

/// Diameter of a step cell.
pub const STEP_CELL: f32 = 32.0;
/// Gap between cells.
pub const STEP_GAP: f32 = 8.0;
/// Extra gap between groups of four.
pub const STEP_GROUP_GAP: f32 = 20.0;

/// What a step cell shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepCellSpec<'a> {
    /// The step is active.
    pub on: bool,
    /// The step is accented (brighter fill with a dot).
    pub accent: bool,
    /// The playhead is on this column.
    pub current: bool,
    /// The step is selected for note entry (bass lane).
    pub selected: bool,
    /// Text inside the cell (bass note name), if any.
    pub label: Option<&'a str>,
}

/// A 32 pt circular step cell. The caller toggles its state on `clicked()`.
pub fn step_cell(ui: &mut Ui, theme: &Theme, spec: &StepCellSpec<'_>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(STEP_CELL, STEP_CELL), Sense::click());
    let fill = if spec.on { theme.accent } else { theme.surface };
    ui.painter()
        .circle_filled(rect.center(), STEP_CELL * 0.5, fill);
    response
}

/// The playhead indicator under a column: a 6 pt idle dot, or an 8 pt accent dot when current.
pub fn playhead_dot(ui: &mut Ui, theme: &Theme, current: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(STEP_CELL, 10.0), Sense::hover());
    let (r, fill) = if current {
        (4.0, theme.accent)
    } else {
        (3.0, theme.shadow_dark)
    };
    ui.painter().circle_filled(rect.center(), r, fill);
    response
}
