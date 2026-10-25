//! Sequencer step cells, the playhead row and the beat ruler
//! (`design/round3/WINNER.md` section 4, "Step cells").
//!
//! A cell is 24 x 24, radius 3, 1 px border; cells sit 2 apart inside a beat and 6 apart
//! between beats, so a row of 16 is 426 wide and the hit pitch is 28 (the gap belongs to the
//! cell). States: off = `step_off` on beats 2 and 4, `step_off_alt` on beats 1 and 3, with a
//! `border` hairline; hover = a `text_2` border; on = `accent` with an `accent_press` border;
//! accent (Shift-click) = `accent_hover`, `accent_press` border, a 2 px `accent_press` inset
//! ring and a 2 pt `on_accent` notch at y 4 inset 8 each side; playhead column = a 1 px
//! `text_2` ring outside every cell in the column (kept on accent cells) plus a 6 x 6 accent
//! triangle under the ruler cell; note cell (track lanes) = the track colour with a border
//! 70 % towards black and a 10 / 700 `on_track` label.
//!
//! [`step_row_width`] and [`step_x_offset`] give that layout in points so headers and other
//! rows can line up with the cells; [`step_row`], [`playhead_row`] and [`beat_numbers`] lay
//! the three kinds of row out with those gaps.

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, FontId, Painter, Pos2, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui,
    WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::button::control_painter;
use crate::flat::paint_control;
use crate::text::{self, TextKind};

/// Side of a step cell (24).
pub const STEP_CELL: f32 = 24.0;
/// Gap between cells inside a beat (2).
pub const STEP_GAP: f32 = 2.0;
/// Gap between beats (6; it replaces the normal gap after every fourth cell).
pub const STEP_GROUP_GAP: f32 = 6.0;
/// Number of cells per group (the group gap follows every `STEP_GROUP`th cell).
pub const STEP_GROUP: usize = 4;
/// Hit pitch of a cell in a row (28: the cell plus its share of the gaps).
pub const STEP_PITCH: f32 = 28.0;
/// Height of the playhead row ([`playhead_dot`] is `STEP_CELL` x `PLAYHEAD_ROW_HEIGHT`).
pub const PLAYHEAD_ROW_HEIGHT: f32 = 10.0;
/// Side of the playhead triangle (6).
pub const PLAYHEAD_DOT_CURRENT: f32 = 6.0;
/// Diameter of the idle playhead mark (a 2 pt `text_3` dot).
pub const PLAYHEAD_DOT_IDLE: f32 = 2.0;
/// Height of the beat-number ruler row (16, micro type).
pub const BEAT_NUMBER_HEIGHT: f32 = 16.0;
/// Height of the accent notch (2 pt, at y 4, inset 8 each side).
pub const ACCENT_MARKER: f32 = 2.0;
pub const ACCENT_NOTCH_Y: f32 = 4.0;
pub const ACCENT_NOTCH_INSET: f32 = 8.0;
/// Width of the accent cell's inset ring (2 px `accent_press`).
pub const ACCENT_RING: f32 = 2.0;
/// Stroke width of the playhead-column / selection ring (1 px, outside the cell).
pub const SELECT_RING: f32 = 1.0;
/// Gap between the cell edge and the inside of the ring (0: it hugs the cell).
pub const SELECT_RING_GAP: f32 = 0.0;
/// Font size of the cell label (10 / 700).
pub const STEP_LABEL_SIZE: f32 = 10.0;
/// Corner radius of a cell (3).
pub const STEP_RADIUS: f32 = 3.0;

/// What a step cell shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepCellSpec<'a> {
    /// The step is active.
    pub on: bool,
    /// The step is accented (brighter fill with the notch and inset ring).
    pub accent: bool,
    /// The playhead is on this column.
    pub current: bool,
    /// The step is selected for note entry (bass lane): a 1 px `text` ring.
    pub selected: bool,
    /// Text inside the cell (note name), if any.
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
/// follow the cells before it (`step_x_offset(4)` = 4 x 24 + 3 x 2 + 6 = 108).
pub fn step_x_offset(index: usize) -> f32 {
    (0..index).map(|i| STEP_CELL + step_gap_after(i)).sum()
}

/// Total width of a row of `steps` cells with their gaps (`step_row_width(16)` = 16 x 24 +
/// 12 x 2 + 3 x 6 = 426). Zero cells have zero width.
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

/// Whether cell `index` sits on beat 1 or 3 (the `step_off_alt` beats).
pub fn is_alt_beat(index: usize) -> bool {
    (index / STEP_GROUP).is_multiple_of(2)
}

/// A 24 pt square step cell on beat 1 (`step_off_alt` when off). The caller toggles its
/// state on `clicked()`. See [`step_cell_at`] for the beat alternation and note colours.
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
    step_cell_at(ui, theme, spec, name, 0, None)
}

/// The full step cell: `index` picks the off tone (beats 1 and 3 `step_off_alt`, 2 and 4
/// `step_off`) and `colour` makes a lit, labelled cell a note cell in that track colour.
pub fn step_cell_at(
    ui: &mut Ui,
    theme: &Theme,
    spec: &StepCellSpec<'_>,
    name: &str,
    index: usize,
    colour: Option<Color32>,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(STEP_CELL, STEP_CELL), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let pressed = response.is_pointer_button_down_on();
        let painter = control_painter(ui);
        paint_step_cell(&painter, rect, spec, index, colour, hovered, pressed, theme);
    }

    let on = spec.on;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, ui.is_enabled(), on, name));
    response
}

/// Paints one step cell into `rect` (see the module docs for the states).
#[allow(clippy::too_many_arguments)]
pub fn paint_step_cell(
    painter: &Painter,
    rect: Rect,
    spec: &StepCellSpec<'_>,
    index: usize,
    colour: Option<Color32>,
    hovered: bool,
    pressed: bool,
    theme: &Theme,
) {
    let note = spec.on && spec.label.is_some() && colour.is_some();
    let text_colour = if note {
        let c = colour.unwrap_or(theme.accent);
        paint_control(painter, rect, STEP_RADIUS, c, Theme::note_step_border(c));
        theme.on_track
    } else if spec.on {
        let fill = if spec.accent || hovered {
            theme.accent_hover
        } else {
            theme.accent
        };
        paint_control(painter, rect, STEP_RADIUS, fill, theme.accent_press);
        if spec.accent {
            painter.rect_stroke(
                rect.shrink(1.0),
                STEP_RADIUS - 1.0,
                Stroke::new(ACCENT_RING, theme.accent_press),
                StrokeKind::Inside,
            );
            if spec.label.is_none() {
                let notch = Rect::from_min_max(
                    pos2(rect.min.x + ACCENT_NOTCH_INSET, rect.min.y + ACCENT_NOTCH_Y),
                    pos2(
                        rect.max.x - ACCENT_NOTCH_INSET,
                        rect.min.y + ACCENT_NOTCH_Y + ACCENT_MARKER,
                    ),
                );
                painter.rect_filled(notch, 1.0, theme.on_accent);
            }
        }
        theme.on_accent
    } else {
        let fill = if pressed {
            theme.panel_2
        } else if is_alt_beat(index) {
            theme.step_off_alt
        } else {
            theme.step_off
        };
        let border = if hovered || pressed {
            theme.text_2
        } else {
            theme.border
        };
        paint_control(painter, rect, STEP_RADIUS, fill, border);
        if hovered { theme.text } else { theme.text_2 }
    };

    if spec.current {
        painter.rect_stroke(
            rect.expand(SELECT_RING_GAP),
            STEP_RADIUS + SELECT_RING_GAP,
            Stroke::new(SELECT_RING, theme.text_2),
            StrokeKind::Outside,
        );
    }
    if spec.selected {
        painter.rect_stroke(
            rect.expand(SELECT_RING_GAP + if spec.current { SELECT_RING } else { 0.0 }),
            STEP_RADIUS + 1.0,
            Stroke::new(SELECT_RING, theme.text),
            StrokeKind::Outside,
        );
    }

    if let Some(label) = spec.label {
        paint_step_label(painter, rect.center(), label, text_colour);
    }
}

/// The playhead column's cell tint: `panel_4` (Deck draws a ring, not a tint; kept for the
/// app's playhead row).
pub fn playhead_tint(theme: &Theme) -> Color32 {
    theme.panel_4
}

/// Paints a cell label (10 / 700) centred on `center`.
fn paint_step_label(painter: &Painter, center: Pos2, label: &str, color: Color32) {
    let format = TextFormat {
        font_id: FontId::proportional(STEP_LABEL_SIZE),
        color,
        ..TextFormat::default()
    };
    let galley = painter.layout_job(LayoutJob::single_section(label.to_owned(), format));
    let rect = Align2::CENTER_CENTER.anchor_size(center, galley.size());
    // Weight 700: painted twice, half a point apart (see `text::paint_galley`).
    painter.galley(rect.min + vec2(0.5, 0.0), galley.clone(), color);
    painter.galley(rect.min, galley, color);
}

/// The playhead indicator under a ruler cell: a 6 x 6 accent triangle (pointing down) when
/// `current`, a 2 pt `text_3` dot otherwise, centred in a 24 x 10 cell under its step.
pub fn playhead_dot(ui: &mut Ui, theme: &Theme, current: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(STEP_CELL, PLAYHEAD_ROW_HEIGHT), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if current {
            paint_playhead_triangle(painter, rect.center(), theme);
        } else {
            painter.circle_filled(rect.center(), PLAYHEAD_DOT_IDLE * 0.5, theme.text_3);
        }
    }
    response
}

/// Paints the 6 x 6 accent playhead triangle (pointing down) centred on `centre`.
pub fn paint_playhead_triangle(painter: &Painter, centre: Pos2, theme: &Theme) {
    let h = PLAYHEAD_DOT_CURRENT * 0.5;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(centre.x - h, centre.y - h),
            pos2(centre.x + h, centre.y - h),
            pos2(centre.x, centre.y + h),
        ],
        theme.accent,
        Stroke::NONE,
    ));
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

/// A row of step cells, one per spec, with the step gaps and the beat alternation. Cells are
/// named "`{row_name}` step `{n}`" (1-based) for accessibility and tests. Returns which cell
/// was clicked / hovered; the caller updates its pattern (shift for accent is read from
/// `ui.input(|i| i.modifiers.shift)` by the caller).
pub fn step_row(
    ui: &mut Ui,
    theme: &Theme,
    row_name: &str,
    specs: &[StepCellSpec<'_>],
) -> StepRowOutput {
    step_row_coloured(ui, theme, row_name, specs, None)
}

/// [`step_row`] whose lit, labelled cells are note cells in `colour` (a track lane).
pub fn step_row_coloured(
    ui: &mut Ui,
    theme: &Theme,
    row_name: &str,
    specs: &[StepCellSpec<'_>],
    colour: Option<Color32>,
) -> StepRowOutput {
    let mut out = StepRowOutput::default();
    with_step_gaps(ui, specs.len(), |ui, i| {
        let name = format!("{row_name} step {}", i + 1);
        let response = step_cell_at(ui, theme, &specs[i], &name, i, colour);
        if response.clicked() {
            out.clicked = Some(i);
        }
        if response.hovered() {
            out.hovered = Some(i);
        }
    });
    out
}

/// A row of `steps` playhead marks with the step gaps, the triangle at `current`. 10 pt high.
pub fn playhead_row(ui: &mut Ui, theme: &Theme, steps: usize, current: Option<usize>) -> Rect {
    with_step_gaps(ui, steps, |ui, i| {
        playhead_dot(ui, theme, current == Some(i));
    })
}

/// The beat ruler over a row of `steps` cells: "1", "2", ... in micro type (`text_2`) centred
/// over every fourth column and a `text_3` dot over the others, 16 pt high.
pub fn beat_numbers(ui: &mut Ui, theme: &Theme, steps: usize) -> Rect {
    let (rect, _) = ui.allocate_exact_size(
        vec2(step_row_width(steps), BEAT_NUMBER_HEIGHT),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        for i in 0..steps {
            let x = rect.min.x + step_x_offset(i) + STEP_CELL * 0.5;
            if i % STEP_GROUP == 0 {
                text::paint_text(
                    painter,
                    egui::pos2(x, rect.center().y),
                    Align2::CENTER_CENTER,
                    &(i / STEP_GROUP + 1).to_string(),
                    TextKind::Micro,
                    theme.text_2,
                );
            } else {
                painter.circle_filled(pos2(x, rect.center().y), 1.0, theme.text_3);
            }
        }
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_geometry_matches_winner() {
        assert_eq!(step_row_width(16), 426.0);
        assert_eq!(step_x_offset(4), 108.0);
        assert_eq!(step_x_offset(0), 0.0);
        assert_eq!(step_x_offset(1), 26.0);
        assert_eq!(step_row_width(0), 0.0);
        assert_eq!(step_row_width(1), STEP_CELL);
        assert_eq!(step_row_width(4), 4.0 * STEP_CELL + 3.0 * STEP_GAP);
        assert!(is_alt_beat(0) && is_alt_beat(3) && !is_alt_beat(4) && is_alt_beat(8));
    }

    #[test]
    fn index_at_finds_cells_and_skips_gaps() {
        assert_eq!(step_index_at(0.0, 16), Some(0));
        assert_eq!(step_index_at(23.0, 16), Some(0));
        assert_eq!(step_index_at(25.0, 16), None);
        assert_eq!(step_index_at(108.0, 16), Some(4));
        assert_eq!(step_index_at(104.0, 16), None);
        assert_eq!(step_index_at(425.0, 16), Some(15));
        assert_eq!(step_index_at(427.0, 16), None);
    }

    #[test]
    fn off_tones_and_labels_read() {
        // The off tones sit between bg and panel-3 and the step label colours meet AA.
        for theme in [Theme::dark(), Theme::light()] {
            let ratio = crate::theme::contrast_ratio(theme.on_accent, theme.accent);
            assert!(ratio >= 4.5, "{:?}: {ratio:.2}:1", theme.mode);
            let ratio = crate::theme::contrast_ratio(theme.text_2, theme.step_off);
            assert!(ratio >= 4.5, "{:?}: {ratio:.2}:1", theme.mode);
            assert_ne!(theme.step_off, theme.step_off_alt);
            assert_eq!(playhead_tint(&theme), theme.panel_4);
        }
    }
}
