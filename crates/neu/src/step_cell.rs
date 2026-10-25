//! Sequencer step cells, the playhead dot row and the beat-number header.
//!
//! Geometry and treatments follow `design/preview-a.html` (`.step`, `.step.on`, `.step.acc`,
//! `.step.cur`, `.beat .ph`) and `design/WINNER.md` ("Control sizes": step cell 32 circle, gap
//! 8, group gap 20, accent marker 6 dot in on-accent; playhead dot 6 idle in shadow-dark, 8
//! accent + on-glow on the current column).
//!
//! A row of `n` cells is `n` circles of [`STEP_CELL`] separated by [`STEP_GAP`], except after
//! every fourth cell where the gap is [`STEP_GROUP_GAP`] instead (the group gap replaces the
//! normal gap, it is not added to it). [`step_row_width`] and [`step_x_offset`] give that
//! layout in points so headers and other rows can line up with the cells; [`step_row`],
//! [`playhead_row`] and [`beat_numbers`] lay the three kinds of row out with those gaps.

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, FontId, Painter, Pos2, Rect, Response, Role, Sense, Stroke, Ui, WidgetInfo,
    vec2,
};

use crate::Theme;
use crate::shadow::{Depth, paint_soft_circle, paint_soft_circle_with_fill};
use crate::text::{self, TextKind};

/// Diameter of a step cell.
pub const STEP_CELL: f32 = 32.0;
/// Gap between cells.
pub const STEP_GAP: f32 = 8.0;
/// Extra gap between groups of four.
pub const STEP_GROUP_GAP: f32 = 20.0;
/// Number of cells per group (the group gap follows every `STEP_GROUP`th cell).
pub const STEP_GROUP: usize = 4;
/// Height of the playhead dot row ([`playhead_dot`] is `STEP_CELL` x `PLAYHEAD_ROW_HEIGHT`).
pub const PLAYHEAD_ROW_HEIGHT: f32 = 10.0;
/// Diameter of the idle playhead dot (shadow-dark).
pub const PLAYHEAD_DOT_IDLE: f32 = 6.0;
/// Diameter of the playhead dot on the current column (accent + on-glow).
pub const PLAYHEAD_DOT_CURRENT: f32 = 8.0;
/// Height of the beat-number header row (`.beat .num` in the mock).
pub const BEAT_NUMBER_HEIGHT: f32 = 14.0;
/// Diameter of the accent marker dot inside an accented, unlabelled cell.
pub const ACCENT_MARKER: f32 = 6.0;
/// Stroke width of the selection ring around a cell.
pub const SELECT_RING: f32 = 2.0;
/// Gap between the cell edge and the inside of the selection ring.
pub const SELECT_RING_GAP: f32 = 1.0;
/// Font size of the cell label (11 / 500 in the mock, painted as a faux medium).
pub const STEP_LABEL_SIZE: f32 = 11.0;

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

impl Default for StepCellSpec<'_> {
    /// An off, unaccented, unlabelled cell with the playhead elsewhere.
    fn default() -> Self {
        Self {
            on: false,
            accent: false,
            current: false,
            selected: false,
            label: None,
        }
    }
}

/// The gap that follows cell `index` in a row: [`STEP_GROUP_GAP`] after every fourth cell,
/// [`STEP_GAP`] otherwise.
pub fn step_gap_after(index: usize) -> f32 {
    if index % STEP_GROUP == STEP_GROUP - 1 {
        STEP_GROUP_GAP
    } else {
        STEP_GAP
    }
}

/// Left edge of cell `index` relative to the row's left edge: `index` cells plus the gaps that
/// follow the cells before it (`step_x_offset(4)` = 4 x 32 + 3 x 8 + 20 = 172).
pub fn step_x_offset(index: usize) -> f32 {
    (0..index).map(|i| STEP_CELL + step_gap_after(i)).sum()
}

/// Total width of a row of `steps` cells with their gaps (`step_row_width(16)` = 16 x 32 +
/// 12 x 8 + 3 x 20 = 668). Zero cells have zero width.
pub fn step_row_width(steps: usize) -> f32 {
    match steps.checked_sub(1) {
        Some(last) => step_x_offset(last) + STEP_CELL,
        None => 0.0,
    }
}

/// The cell under `x` (relative to the row's left edge) in a row of `steps` cells, if `x`
/// falls on a cell rather than a gap.
pub fn step_index_at(x: f32, steps: usize) -> Option<usize> {
    (0..steps).find(|&i| {
        let left = step_x_offset(i);
        (left..=left + STEP_CELL).contains(&x)
    })
}

/// A 32 pt circular step cell. The caller toggles its state on `clicked()`.
///
/// Off: raised-s surface (inset-s while pressed or on the playhead column). On: accent fill
/// with the on-glow; `accent_hover` when accented, hovered or on the playhead column, plus a
/// 6 pt on-accent dot when accented and unlabelled. `selected` draws a 2 pt accent ring just
/// outside the circle. `label` is painted centred at 11 pt (on-accent on lit cells, secondary
/// otherwise, primary on hover).
///
/// Accessibility: a check box named after the label (or "Step"); see [`step_cell_named`] to
/// give it a proper name such as "Kick step 1".
pub fn step_cell(ui: &mut Ui, theme: &Theme, spec: &StepCellSpec<'_>) -> Response {
    let name = spec.label.unwrap_or("Step");
    step_cell_named(ui, theme, spec, name)
}

/// [`step_cell`] with an explicit accessible name (reported as
/// `WidgetInfo::selected(Role::CheckBox, on, name)`).
pub fn step_cell_named(
    ui: &mut Ui,
    theme: &Theme,
    spec: &StepCellSpec<'_>,
    name: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(STEP_CELL, STEP_CELL), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let pressed = response.is_pointer_button_down_on();
        let painter = ui.painter();
        let center = rect.center();
        let radius = STEP_CELL * 0.5;

        let text_color = if spec.on {
            let fill = if spec.accent || spec.current || hovered {
                theme.accent_hover
            } else {
                theme.accent
            };
            paint_soft_circle_with_fill(painter, center, radius, Depth::OnGlow, fill, theme);
            if spec.accent && spec.label.is_none() {
                painter.circle_filled(center, ACCENT_MARKER * 0.5, theme.on_accent);
            }
            theme.on_accent
        } else if spec.current || pressed {
            paint_soft_circle_with_fill(
                painter,
                center,
                radius,
                Depth::InsetS,
                playhead_tint(theme),
                theme,
            );
            if hovered {
                theme.text_primary
            } else {
                theme.text_secondary
            }
        } else {
            paint_soft_circle(painter, center, radius, Depth::RaisedS, theme);
            if hovered {
                theme.text_primary
            } else {
                theme.text_secondary
            }
        };

        if spec.selected {
            painter.circle_stroke(
                center,
                radius + SELECT_RING_GAP + SELECT_RING * 0.5,
                Stroke::new(SELECT_RING, theme.accent),
            );
        }

        if let Some(label) = spec.label {
            paint_step_label(painter, center, label, text_color);
        }
    }

    let on = spec.on;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, ui.is_enabled(), on, name));
    response
}

/// The surface tinted a little towards the dark shadow: the fill of a pressed-in cell on the
/// playhead column, so the column reads even where the inset shadow is faint.
pub fn playhead_tint(theme: &Theme) -> Color32 {
    theme.surface.lerp_to_gamma(theme.shadow_dark, 0.18)
}

/// Paints a cell label (11 pt, faux medium) centred on `center`.
fn paint_step_label(painter: &Painter, center: Pos2, label: &str, color: Color32) {
    let format = TextFormat {
        font_id: FontId::proportional(STEP_LABEL_SIZE),
        color,
        ..TextFormat::default()
    };
    let galley = painter.layout_job(LayoutJob::single_section(label.to_owned(), format));
    let rect = Align2::CENTER_CENTER.anchor_size(center, galley.size());
    // Weight 500: painted twice, a third of a point apart (see `text::paint_galley`).
    painter.galley(rect.min + vec2(0.35, 0.0), galley.clone(), color);
    painter.galley(rect.min, galley, color);
}

/// The playhead indicator under a column: a 6 pt idle dot in shadow-dark, or an 8 pt accent
/// dot with the on-glow when `current`, centred in a 32 x 10 cell so it sits under its step.
pub fn playhead_dot(ui: &mut Ui, theme: &Theme, current: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(STEP_CELL, PLAYHEAD_ROW_HEIGHT), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if current {
            paint_soft_circle(
                painter,
                rect.center(),
                PLAYHEAD_DOT_CURRENT * 0.5,
                Depth::OnGlow,
                theme,
            );
        } else {
            painter.circle_filled(rect.center(), PLAYHEAD_DOT_IDLE * 0.5, theme.shadow_dark);
        }
    }
    response
}

/// Lays `steps` items out left to right with the step gaps ([`STEP_GAP`], [`STEP_GROUP_GAP`]
/// after every fourth), calling `item` once per index. Each item must be [`STEP_CELL`] wide
/// for the columns to line up. Returns the rect of the whole row.
pub fn with_step_gaps(ui: &mut Ui, steps: usize, mut item: impl FnMut(&mut Ui, usize)) -> Rect {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for i in 0..steps {
            item(ui, i);
            if i + 1 < steps {
                ui.add_space(step_gap_after(i));
            }
        }
    })
    .response
    .rect
}

/// What a [`step_row`] reported this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StepRowOutput {
    /// The cell that was clicked, if any.
    pub clicked: Option<usize>,
    /// The cell under the pointer, if any.
    pub hovered: Option<usize>,
}

/// A row of step cells, one per spec, with the step gaps. Cells are named
/// "`{row_name}` step `{n}`" (1-based) for accessibility and tests. Returns which cell was
/// clicked / hovered; the caller updates its pattern (shift for accent is read from
/// `ui.input(|i| i.modifiers.shift)` by the caller).
pub fn step_row(
    ui: &mut Ui,
    theme: &Theme,
    row_name: &str,
    specs: &[StepCellSpec<'_>],
) -> StepRowOutput {
    let mut out = StepRowOutput::default();
    with_step_gaps(ui, specs.len(), |ui, i| {
        let name = format!("{row_name} step {}", i + 1);
        let response = step_cell_named(ui, theme, &specs[i], &name);
        if response.clicked() {
            out.clicked = Some(i);
        }
        if response.hovered() {
            out.hovered = Some(i);
        }
    });
    out
}

/// A row of `steps` playhead dots with the step gaps, the dot at `current` lit. 10 pt high.
pub fn playhead_row(ui: &mut Ui, theme: &Theme, steps: usize, current: Option<usize>) -> Rect {
    with_step_gaps(ui, steps, |ui, i| {
        playhead_dot(ui, theme, current == Some(i));
    })
}

/// The beat numbers over a row of `steps` cells: "1", "2", ... centred over every fourth
/// column, label-level text in the secondary colour, 14 pt high.
pub fn beat_numbers(ui: &mut Ui, theme: &Theme, steps: usize) -> Rect {
    let (rect, _) = ui.allocate_exact_size(
        vec2(step_row_width(steps), BEAT_NUMBER_HEIGHT),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        for beat in 0..steps.div_ceil(STEP_GROUP) {
            let x = rect.min.x + step_x_offset(beat * STEP_GROUP) + STEP_CELL * 0.5;
            text::paint_text(
                painter,
                egui::pos2(x, rect.center().y),
                Align2::CENTER_CENTER,
                &(beat + 1).to_string(),
                TextKind::Label,
                theme.text_secondary,
            );
        }
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_geometry_matches_the_mock() {
        assert_eq!(step_row_width(16), 668.0);
        assert_eq!(step_x_offset(4), 172.0);
        assert_eq!(step_x_offset(0), 0.0);
        assert_eq!(step_x_offset(1), 40.0);
        assert_eq!(step_row_width(0), 0.0);
        assert_eq!(step_row_width(1), STEP_CELL);
        assert_eq!(step_row_width(4), 4.0 * STEP_CELL + 3.0 * STEP_GAP);
    }

    #[test]
    fn index_at_finds_cells_and_skips_gaps() {
        assert_eq!(step_index_at(0.0, 16), Some(0));
        assert_eq!(step_index_at(31.0, 16), Some(0));
        assert_eq!(step_index_at(36.0, 16), None);
        assert_eq!(step_index_at(172.0, 16), Some(4));
        assert_eq!(step_index_at(160.0, 16), None);
        assert_eq!(step_index_at(667.0, 16), Some(15));
        assert_eq!(step_index_at(669.0, 16), None);
    }

    #[test]
    fn labels_on_the_playhead_tint_meet_aa() {
        // Bass-lane note names (secondary text) on a tinted playhead-column cell.
        for theme in [Theme::dark(), Theme::light()] {
            let ratio = crate::theme::contrast_ratio(theme.text_secondary, playhead_tint(&theme));
            assert!(ratio >= 4.5, "{:?}: {ratio:.2}:1", theme.mode);
        }
    }

    #[test]
    fn tint_sits_between_surface_and_shadow() {
        for theme in [Theme::dark(), Theme::light()] {
            let tint = playhead_tint(&theme);
            let lum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
            let (lo, hi) = (lum(theme.shadow_dark), lum(theme.surface));
            assert!(lo < lum(tint) && lum(tint) < hi, "{tint:?}");
        }
    }
}
