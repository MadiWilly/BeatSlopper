//! Text helpers for the type scale in `design/WINNER.md` ("Radii, spacing, type").
//!
//! egui ships one proportional face (a light weight), so the 300-500 weight range of the mock
//! is approximated: light stays as is and the "500" levels are painted twice with a sub-point
//! horizontal offset (a faux medium). Tracking (+0.12em) uses `TextFormat::extra_letter_spacing`.

use std::sync::Arc;

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Galley, Painter, Pos2, Rect, vec2};

/// A level of the type scale. Sizes in points, see `design/WINNER.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    /// 11 / 400 / uppercase / +0.12em. Control names (secondary colour).
    Label,
    /// 12 / 500 / uppercase / +0.12em. Panel names (primary colour).
    Title,
    /// 13 / 400. Values (primary colour).
    Value,
    /// 22 / 300. The BPM number.
    Display,
    /// 12 / 500 / uppercase / +0.12em. Pill button labels.
    Button,
}

impl TextKind {
    /// Font size in points.
    pub fn size(self) -> f32 {
        match self {
            TextKind::Label => 11.0,
            TextKind::Title | TextKind::Button => 12.0,
            TextKind::Value => 13.0,
            TextKind::Display => 22.0,
        }
    }

    /// Whether the text is set in capitals with +0.12em tracking.
    pub fn is_caps(self) -> bool {
        matches!(self, TextKind::Label | TextKind::Title | TextKind::Button)
    }

    /// Whether the text reads as weight 500 (painted as a faux medium).
    pub fn is_medium(self) -> bool {
        matches!(self, TextKind::Title | TextKind::Button)
    }

    /// Extra letter spacing in points (0.12em for caps levels).
    pub fn letter_spacing(self) -> f32 {
        if self.is_caps() {
            0.12 * self.size()
        } else {
            0.0
        }
    }

    /// The egui font for this level.
    pub fn font_id(self) -> FontId {
        FontId::proportional(self.size())
    }

    /// The text format (font, tracking, colour) for this level.
    pub fn format(self, color: Color32) -> TextFormat {
        TextFormat {
            font_id: self.font_id(),
            extra_letter_spacing: self.letter_spacing(),
            color,
            ..TextFormat::default()
        }
    }

    /// The string as it is displayed (upper-cased for caps levels).
    pub fn display_string(self, text: &str) -> String {
        if self.is_caps() {
            text.to_uppercase()
        } else {
            text.to_owned()
        }
    }
}

/// Lays out `text` at the given level, single line, in `color`.
pub fn layout(painter: &Painter, text: &str, kind: TextKind, color: Color32) -> Arc<Galley> {
    let job = LayoutJob::single_section(kind.display_string(text), kind.format(color));
    painter.layout_job(job)
}

/// Paints an already laid out galley anchored at `pos`. Medium levels are painted twice,
/// offset by a third of a point, to read heavier than egui's light face.
pub fn paint_galley(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    galley: Arc<Galley>,
    kind: TextKind,
    color: Color32,
) -> Rect {
    let rect = anchor.anchor_size(pos, galley.size());
    if kind.is_medium() {
        painter.galley(rect.min + vec2(0.35, 0.0), galley.clone(), color);
    }
    painter.galley(rect.min, galley, color);
    rect
}

/// Lays out and paints `text` at the given level, anchored at `pos`. Returns the text rect.
pub fn paint_text(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    kind: TextKind,
    color: Color32,
) -> Rect {
    let galley = layout(painter, text, kind, color);
    paint_galley(painter, pos, anchor, galley, kind, color)
}

/// Adds a label-level (11 pt caps, secondary colour) text to the ui as a plain widget.
pub fn label(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Label, theme.text_secondary)
}

/// Adds a title-level (12 pt caps medium, primary colour) text to the ui as a plain widget.
pub fn title(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Title, theme.text_primary)
}

/// Adds a value-level (13 pt, primary colour) text to the ui as a plain widget.
pub fn value(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Value, theme.text_primary)
}

/// Adds a display-level (22 pt light, primary colour) text to the ui as a plain widget.
pub fn display(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Display, theme.text_primary)
}

fn add_text(ui: &mut egui::Ui, text: &str, kind: TextKind, color: Color32) -> egui::Response {
    let galley = layout(ui.painter(), text, kind, color);
    let (rect, response) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_galley(
            ui.painter(),
            rect.min,
            Align2::LEFT_TOP,
            galley,
            kind,
            color,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    response
}
