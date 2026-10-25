//! The Soft Console depth painter, kept under its old names and drawing flat (Deck, M12).
//!
//! `design/round3/WINNER.md` retires the paired soft shadows: depth is the four panel tones
//! plus 1 px borders. The functions here keep their signatures so `crates/app` compiles, and
//! every treatment now paints one flat shape with no blur:
//!
//! * **Raised** (`RaisedL`): `panel` fill + 1 px `border` (a group / dock). `RaisedM` /
//!   `RaisedS`: `panel_3` fill + 1 px `border_2` (a button face).
//! * **Inset** (`InsetM` / `InsetS`): `panel_2` fill + 1 px `border_2` (a well / field; Deck
//!   wells are panel-2).
//! * **OnGlow**: `accent` fill + 1 px `accent_press` border (an ON state). No glow.
//!
//! Circles use the same treatments with circle geometry. The focus ring helpers delegate to
//! [`crate::flat`] (a 2 px accent ring 1 px outside the control). All of this is retired in
//! M13 in favour of [`crate::flat`] directly.

use egui::epaint::Shape;
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

use crate::flat::{self, clamp_radius, flat_rect_shape};
use crate::{ShadowPair, Theme};

/// Which flat treatment to paint (the Soft Console names, Deck looks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// Groups / docks: `panel` + `border`.
    RaisedL,
    /// Button faces: `panel_3` + `border_2`.
    RaisedM,
    /// Small button faces: `panel_3` + `border_2`.
    RaisedS,
    /// Wells: `panel_2` + `border_2`.
    InsetM,
    /// Small wells: `panel_2` + `border_2`.
    InsetS,
    /// ON state: `accent` + `accent_press`.
    OnGlow,
}

impl Depth {
    /// `true` for the inset treatments (`InsetM`, `InsetS`).
    pub fn is_inset(self) -> bool {
        matches!(self, Depth::InsetM | Depth::InsetS)
    }

    /// `true` for the raised treatments (`RaisedL`, `RaisedM`, `RaisedS`).
    pub fn is_raised(self) -> bool {
        matches!(self, Depth::RaisedL | Depth::RaisedM | Depth::RaisedS)
    }

    /// The treatment a pressed control of this depth swaps to (raised -> inset, inset and
    /// on-glow stay as they are).
    pub fn pressed(self) -> Depth {
        match self {
            Depth::RaisedL | Depth::RaisedM => Depth::InsetM,
            Depth::RaisedS => Depth::InsetS,
            other => other,
        }
    }

    /// The Soft Console shadow pair of this depth: all zero in Deck (no shadows).
    pub fn pair(self, theme: &Theme) -> ShadowPair {
        match self {
            Depth::RaisedL => theme.raised_l,
            Depth::RaisedM => theme.raised_m,
            Depth::RaisedS => theme.raised_s,
            Depth::InsetM => theme.inset_m,
            Depth::InsetS => theme.inset_s,
            Depth::OnGlow => theme.on_glow_dark,
        }
    }

    /// The fill colour of a control with this depth.
    pub fn fill(self, theme: &Theme) -> Color32 {
        match self {
            Depth::RaisedL => theme.panel,
            Depth::RaisedM | Depth::RaisedS => theme.panel_3,
            Depth::InsetM | Depth::InsetS => theme.panel_2,
            Depth::OnGlow => theme.accent,
        }
    }

    /// The 1 px border colour of a control with this depth.
    pub fn border(self, theme: &Theme) -> Color32 {
        match self {
            Depth::RaisedL => theme.border,
            Depth::RaisedM | Depth::RaisedS | Depth::InsetM | Depth::InsetS => theme.border_2,
            Depth::OnGlow => theme.accent_press,
        }
    }
}

/// Soft Console blur multiplier; unused by Deck (no blurred shapes), kept for the app.
pub const BLUR_SCALE: f32 = 1.5;

/// Builds the one flat shape for a rounded rectangle of `depth`: fill + 1 px border, never a
/// shadow. Returned as a `Vec` for the old call sites.
pub fn soft_rect_shapes(rect: Rect, radius: f32, depth: Depth, theme: &Theme) -> Vec<Shape> {
    vec![flat_rect_shape(
        rect,
        radius,
        depth.fill(theme),
        depth.border(theme),
    )]
}

/// Like [`soft_rect_shapes`] with an explicit fill colour; the border is that of `depth`.
pub fn soft_rect_shapes_with_fill(
    rect: Rect,
    radius: f32,
    depth: Depth,
    fill: Color32,
    theme: &Theme,
) -> Vec<Shape> {
    vec![flat_rect_shape(rect, radius, fill, depth.border(theme))]
}

/// Paints a rounded rectangle with the flat treatment of `depth`.
///
/// `radius` is clamped to half the shorter side (Deck uses 4 / 3 / 2; a "pill" is 4).
pub fn paint_soft_rect(painter: &Painter, rect: Rect, radius: f32, depth: Depth, theme: &Theme) {
    painter.extend(soft_rect_shapes(rect, radius, depth, theme));
}

/// Like [`paint_soft_rect`] with an explicit fill colour.
pub fn paint_soft_rect_with_fill(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    depth: Depth,
    fill: Color32,
    theme: &Theme,
) {
    painter.extend(soft_rect_shapes_with_fill(rect, radius, depth, fill, theme));
}

/// Builds the flat shape for a circle of `depth`: fill + 1 px border.
pub fn soft_circle_shapes(center: Pos2, radius: f32, depth: Depth, theme: &Theme) -> Vec<Shape> {
    vec![Shape::Circle(egui::epaint::CircleShape {
        center,
        radius: radius.max(0.0),
        fill: depth.fill(theme),
        stroke: Stroke::new(1.0, depth.border(theme)),
    })]
}

/// Paints a circle with the flat treatment of `depth`.
pub fn paint_soft_circle(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    depth: Depth,
    theme: &Theme,
) {
    painter.extend(soft_circle_shapes(center, radius, depth, theme));
}

/// Like [`paint_soft_circle`] with an explicit fill colour.
pub fn paint_soft_circle_with_fill(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    depth: Depth,
    fill: Color32,
    theme: &Theme,
) {
    painter.add(Shape::Circle(egui::epaint::CircleShape {
        center,
        radius: radius.max(0.0),
        fill,
        stroke: Stroke::new(1.0, depth.border(theme)),
    }));
}

/// Stroke width of the keyboard-focus ring (= [`flat::FOCUS_RING`]).
pub const FOCUS_RING: f32 = flat::FOCUS_RING;
/// Gap between a control's edge and the inside of its focus ring (= [`flat::FOCUS_RING_GAP`]).
pub const FOCUS_RING_GAP: f32 = flat::FOCUS_RING_GAP;

/// Paints the keyboard-focus ring around a rounded rect (see [`flat::paint_focus_ring`]).
pub fn paint_focus_rect(painter: &Painter, rect: Rect, radius: f32, theme: &Theme) {
    flat::paint_focus_ring(painter, rect, radius, theme);
}

/// Paints the keyboard-focus ring around a circle (see [`flat::paint_focus_circle`]).
pub fn paint_focus_circle(painter: &Painter, center: Pos2, radius: f32, theme: &Theme) {
    flat::paint_focus_circle(painter, center, radius, theme);
}

/// The size of a shape's rect after the radius clamp (kept for callers that measured it).
pub fn clamped_size(rect: Rect, radius: f32) -> Vec2 {
    let _ = clamp_radius(rect, radius);
    rect.size()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat::is_shadow_shape;
    use egui::{pos2, vec2};

    #[test]
    fn every_depth_is_one_flat_rect_with_a_border_and_no_shadow() {
        let theme = Theme::dark();
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 40.0));
        for depth in [
            Depth::RaisedL,
            Depth::RaisedM,
            Depth::RaisedS,
            Depth::InsetM,
            Depth::InsetS,
            Depth::OnGlow,
        ] {
            let shapes = soft_rect_shapes(rect, 999.0, depth, &theme);
            assert_eq!(shapes.len(), 1, "{depth:?}");
            assert!(!is_shadow_shape(&shapes[0]), "{depth:?} paints a shadow");
            let Shape::Rect(r) = &shapes[0] else {
                panic!("{depth:?}: a rect")
            };
            assert_eq!(r.fill, depth.fill(&theme));
            assert_eq!(r.stroke.color, depth.border(&theme));
            assert_eq!(r.stroke.width, 1.0);
            assert_eq!(r.blur_width, 0.0);
            // A "pill" radius clamps to half the height (20), not 999.
            assert_eq!(r.corner_radius, egui::CornerRadius::same(20));
        }
        assert_eq!(Depth::RaisedL.fill(&theme), theme.panel);
        assert_eq!(Depth::RaisedS.fill(&theme), theme.panel_3);
        assert_eq!(Depth::InsetS.fill(&theme), theme.panel_2);
        assert_eq!(Depth::OnGlow.fill(&theme), theme.accent);
        assert_eq!(Depth::OnGlow.border(&theme), theme.accent_press);
        assert_eq!(Depth::RaisedL.border(&theme), theme.border);
    }

    #[test]
    fn explicit_fills_keep_the_depth_border() {
        let theme = Theme::light();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 28.0));
        let shapes = soft_rect_shapes_with_fill(rect, 4.0, Depth::InsetM, theme.accent, &theme);
        let Shape::Rect(r) = &shapes[0] else {
            panic!("a rect")
        };
        assert_eq!(r.fill, theme.accent);
        assert_eq!(r.stroke.color, theme.border_2);
        let circle = soft_circle_shapes(pos2(5.0, 5.0), 5.0, Depth::OnGlow, &theme);
        let Shape::Circle(c) = &circle[0] else {
            panic!("a circle")
        };
        assert_eq!(c.fill, theme.accent);
        assert_eq!(c.stroke.color, theme.accent_press);
    }

    #[test]
    fn pressed_maps_raised_to_inset() {
        assert_eq!(Depth::RaisedL.pressed(), Depth::InsetM);
        assert_eq!(Depth::RaisedS.pressed(), Depth::InsetS);
        assert_eq!(Depth::OnGlow.pressed(), Depth::OnGlow);
        assert!(Depth::InsetM.is_inset() && Depth::RaisedM.is_raised());
    }

    #[test]
    fn tiny_shapes_do_not_panic() {
        let theme = Theme::dark();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(0.4, 0.4));
        for depth in [Depth::RaisedS, Depth::InsetS, Depth::OnGlow] {
            let _ = soft_rect_shapes(rect, 5.0, depth, &theme);
            let _ = soft_circle_shapes(pos2(0.0, 0.0), 0.2, depth, &theme);
        }
    }
}
