//! Text helpers for the Deck type scale (`design/round3/WINNER.md` section 3).
//!
//! egui ships one proportional face, so the 500 / 600 weights of the design are approximated:
//! 400 is the face as is, 500 is painted twice with a third-of-a-point horizontal offset (a faux
//! medium) and 600 twice with half a point (a faux semibold). Tracking (+0.06em on titles,
//! +0.04em on labels and buttons, +0.02em on keycaps) uses `TextFormat::extra_letter_spacing`.
//! Every level also exists as a named egui `TextStyle` (see [`crate::Theme::text_styles`]).

use std::sync::Arc;

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Galley, Painter, Pos2, Rect, vec2};

/// A level of the type scale. Sizes in points, see WINNER.md "Type".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKind {
    /// 12 / 600 / UPPER / +0.06em, `text`: window + dock headers, group heads.
    Title,
    /// 12 / 500 / UPPER / +0.04em, `text_2`: knob + group labels, column heads.
    Label,
    /// 12 / 600 / UPPER / +0.04em: every button, tab, chip.
    Button,
    /// 13 / 400, tabular, `text`: knob values, fields, list rows, position.
    Value,
    /// 13 / 400, `text`: menus, browser rows.
    Body,
    /// 18 / 600, tabular, `text`: the BPM number.
    Display,
    /// 11 / 500, `text_2`: tags, insert numbers, step note labels, strip names.
    Small,
    /// 10 / 600 / UPPER / +0.02em, `text_3`: keycaps.
    Hint,
    /// 11 / 600 / UPPER / +0.06em, `text_3`: rack section heads, synth section rules.
    Section,
    /// 10 / 600, `text_2` / `text_3`: ruler numbers, roll key names, slot pills.
    Micro,
    /// Deck: the project name (13 / 600, no caps, no tracking). The Soft Console wordmark
    /// level is retired; [`brand`] draws this.
    Brand,
}

impl TextKind {
    /// Every level, for the named egui text styles.
    pub const ALL: [TextKind; 11] = [
        TextKind::Title,
        TextKind::Label,
        TextKind::Button,
        TextKind::Value,
        TextKind::Body,
        TextKind::Display,
        TextKind::Small,
        TextKind::Hint,
        TextKind::Section,
        TextKind::Micro,
        TextKind::Brand,
    ];

    /// Font size in points.
    pub fn size(self) -> f32 {
        match self {
            TextKind::Title | TextKind::Label | TextKind::Button => 12.0,
            TextKind::Value | TextKind::Body | TextKind::Brand => 13.0,
            TextKind::Display => 18.0,
            TextKind::Small | TextKind::Section => 11.0,
            TextKind::Hint | TextKind::Micro => 10.0,
        }
    }

    /// Nominal weight (400 / 500 / 600).
    pub fn weight(self) -> u16 {
        match self {
            TextKind::Value | TextKind::Body => 400,
            TextKind::Label | TextKind::Small => 500,
            TextKind::Title
            | TextKind::Button
            | TextKind::Display
            | TextKind::Hint
            | TextKind::Section
            | TextKind::Micro
            | TextKind::Brand => 600,
        }
    }

    /// Whether the text is set in capitals with tracking (see [`Self::letter_spacing`]).
    pub fn is_caps(self) -> bool {
        matches!(
            self,
            TextKind::Title
                | TextKind::Label
                | TextKind::Button
                | TextKind::Hint
                | TextKind::Section
        )
    }

    /// Whether the text reads heavier than 400 (painted as a faux medium / semibold).
    pub fn is_medium(self) -> bool {
        self.weight() >= 500
    }

    /// Extra letter spacing in points: +0.06em on titles and section heads, +0.04em on labels
    /// and buttons, +0.02em on keycaps, none otherwise.
    pub fn letter_spacing(self) -> f32 {
        match self {
            TextKind::Title | TextKind::Section => 0.06 * self.size(),
            TextKind::Label | TextKind::Button => 0.04 * self.size(),
            TextKind::Hint => 0.02 * self.size(),
            _ => 0.0,
        }
    }

    /// The horizontal offset of the second paint for the faux weight (0 for 400).
    pub fn faux_offset(self) -> f32 {
        match self.weight() {
            600 => 0.5,
            500 => 0.35,
            _ => 0.0,
        }
    }

    /// The name of this level's `egui::TextStyle::Name` entry.
    pub fn style_name(self) -> &'static str {
        match self {
            TextKind::Title => "title",
            TextKind::Label => "label",
            TextKind::Button => "button",
            TextKind::Value => "value",
            TextKind::Body => "body",
            TextKind::Display => "display",
            TextKind::Small => "small",
            TextKind::Hint => "hint",
            TextKind::Section => "section",
            TextKind::Micro => "micro",
            TextKind::Brand => "brand",
        }
    }

    /// The egui text style of this level.
    pub fn text_style(self) -> egui::TextStyle {
        egui::TextStyle::Name(self.style_name().into())
    }

    /// The egui font for this level.
    pub fn font_id(self) -> FontId {
        FontId::proportional(self.size())
    }

    /// The default colour of this level in `theme`.
    pub fn colour(self, theme: &crate::Theme) -> Color32 {
        match self {
            TextKind::Title
            | TextKind::Button
            | TextKind::Value
            | TextKind::Body
            | TextKind::Display
            | TextKind::Brand => theme.text,
            TextKind::Label | TextKind::Small | TextKind::Micro => theme.text_2,
            TextKind::Hint | TextKind::Section => theme.text_3,
        }
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

/// Paints an already laid out galley anchored at `pos`. Levels above weight 400 are painted
/// twice, offset by [`TextKind::faux_offset`], to read heavier than egui's one face.
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
        painter.galley(
            rect.min + vec2(kind.faux_offset(), 0.0),
            galley.clone(),
            color,
        );
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

/// Adds a label-level (12 / 500 caps, `text_2`) text to the ui as a plain widget.
pub fn label(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Label, theme.text_2)
}

/// Adds a title-level (12 / 600 caps, `text`) text to the ui as a plain widget.
pub fn title(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Title, theme.text)
}

/// Adds a value-level (13, `text`) text to the ui as a plain widget.
pub fn value(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Value, theme.text)
}

/// Adds a body-level (13, `text`) text to the ui as a plain widget.
pub fn body(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Body, theme.text)
}

/// Adds a display-level (18 / 600, `text`) text to the ui as a plain widget.
pub fn display(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Display, theme.text)
}

/// Adds a small-level (11 / 500, `text_2`) text to the ui as a plain widget.
pub fn small(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Small, theme.text_2)
}

/// Adds a hint-level (10 / 600 caps, `text_3`) text to the ui as a plain widget.
pub fn hint(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Hint, theme.text_3)
}

/// Adds a section-level (11 / 600 caps tracked, `text_3`) text to the ui as a plain widget.
pub fn section(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Section, theme.text_3)
}

/// Adds a micro-level (10 / 600, `text_2`) text to the ui as a plain widget.
pub fn micro(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Micro, theme.text_2)
}

/// Adds the project name (13 / 600, `text`) to the ui as a plain widget. Deck has no
/// wordmark: this is the toolbar's project-name level. The accessible label is `text`.
pub fn brand(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) -> egui::Response {
    add_text(ui, text, TextKind::Brand, theme.text)
}

/// Adds `text` at `kind` in `color` as a plain widget.
pub fn text_widget(
    ui: &mut egui::Ui,
    text: &str,
    kind: TextKind,
    color: Color32,
) -> egui::Response {
    add_text(ui, text, kind, color)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_matches_winner() {
        assert_eq!(TextKind::Title.size(), 12.0);
        assert_eq!(TextKind::Title.weight(), 600);
        assert!((TextKind::Title.letter_spacing() - 0.72).abs() < 1e-5);
        assert_eq!(TextKind::Label.size(), 12.0);
        assert_eq!(TextKind::Label.weight(), 500);
        assert!((TextKind::Label.letter_spacing() - 0.48).abs() < 1e-5);
        assert_eq!(TextKind::Button.weight(), 600);
        assert_eq!(TextKind::Value.size(), 13.0);
        assert_eq!(TextKind::Value.weight(), 400);
        assert_eq!(TextKind::Display.size(), 18.0);
        assert_eq!(TextKind::Small.size(), 11.0);
        assert_eq!(TextKind::Hint.size(), 10.0);
        assert!((TextKind::Hint.letter_spacing() - 0.2).abs() < 1e-5);
        assert_eq!(TextKind::Section.size(), 11.0);
        assert_eq!(TextKind::Micro.size(), 10.0);
        assert!(TextKind::Title.is_caps() && TextKind::Hint.is_caps());
        assert!(!TextKind::Value.is_caps() && !TextKind::Small.is_caps());
        assert_eq!(TextKind::Hint.display_string("Ctrl+L"), "CTRL+L");
        assert_eq!(TextKind::Body.display_string("Tape Wurli"), "Tape Wurli");
        for kind in TextKind::ALL {
            assert!(kind.size() >= 10.0, "{kind:?}: nothing below 10");
        }
    }
}
