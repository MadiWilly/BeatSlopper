//! Flat painting primitives for Deck: fills with 1 px borders, the keyboard-focus ring, the
//! one floating shadow and the two-stop vertical gradients the design allows on knob faces,
//! fader / slider tracks and thumbs and piano keys.
//!
//! Depth everywhere is the four panel tones plus 1 px borders (`design/round3/WINNER.md`
//! section 3). [`paint_shadow_floating`] is the only shadow painter in the kit; it is used by
//! [`crate::shell::menu`], the tooltip and the floating drawer, nowhere else.

use egui::epaint::{Mesh, Shape};
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

use crate::Theme;

/// One of the surface tones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// `bg`: the window background.
    Bg,
    /// `panel`: window / dock bodies, toolbars, groups, rows.
    Panel,
    /// `panel_2`: headers, fields, wells, menus.
    Panel2,
    /// `panel_3`: button faces, row hover.
    Panel3,
    /// `panel_4`: button hover, selected segment, tooltip face.
    Panel4,
}

impl Tone {
    /// The tone's colour in `theme`.
    pub fn colour(self, theme: &Theme) -> Color32 {
        match self {
            Tone::Bg => theme.bg,
            Tone::Panel => theme.panel,
            Tone::Panel2 => theme.panel_2,
            Tone::Panel3 => theme.panel_3,
            Tone::Panel4 => theme.panel_4,
        }
    }
}

/// Stroke width of the keyboard-focus ring (2 px).
pub const FOCUS_RING: f32 = 2.0;
/// Gap between a control's edge and the inside of its focus ring (1 px).
pub const FOCUS_RING_GAP: f32 = 1.0;

/// Clamps a radius to half the shorter side of `rect`.
pub fn clamp_radius(rect: Rect, radius: f32) -> f32 {
    radius.clamp(0.0, (rect.width().min(rect.height()) * 0.5).max(0.0))
}

/// The shape of a flat surface: `fill` with a 1 px `border` stroke drawn inside `rect`.
pub fn flat_rect_shape(rect: Rect, radius: f32, fill: Color32, border: Color32) -> Shape {
    let cr = CornerRadius::from(clamp_radius(rect, radius));
    Shape::Rect(egui::epaint::RectShape::new(
        rect,
        cr,
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    ))
}

/// Paints a surface of `tone` with a 1 px `border` hairline (windows, docks, groups, rows).
pub fn paint_panel(painter: &Painter, rect: Rect, tone: Tone, radius: f32, theme: &Theme) {
    painter.add(flat_rect_shape(
        rect,
        radius,
        tone.colour(theme),
        theme.border,
    ));
}

/// Paints a control face: `fill` with a 1 px `border` stroke (callers pass `border_2` for
/// interactive controls, `border` for pressed ones and separators).
pub fn paint_control(painter: &Painter, rect: Rect, radius: f32, fill: Color32, border: Color32) {
    painter.add(flat_rect_shape(rect, radius, fill, border));
}

/// Paints the keyboard-focus ring around a rounded rect: a [`FOCUS_RING`] accent stroke
/// [`FOCUS_RING_GAP`] outside `rect`, with the corner radius grown to match. Keyboard focus
/// only: callers check `response.has_focus()` and that the focus did not come from a click.
pub fn paint_focus_ring(painter: &Painter, rect: Rect, radius: f32, theme: &Theme) {
    let outer = rect.expand(FOCUS_RING_GAP);
    painter.rect_stroke(
        outer,
        clamp_radius(rect, radius) + FOCUS_RING_GAP,
        Stroke::new(FOCUS_RING, theme.accent),
        StrokeKind::Outside,
    );
}

/// Paints the focus ring around a circle of `radius` at `center`.
pub fn paint_focus_circle(painter: &Painter, center: Pos2, radius: f32, theme: &Theme) {
    painter.circle_stroke(
        center,
        radius + FOCUS_RING_GAP + FOCUS_RING * 0.5,
        Stroke::new(FOCUS_RING, theme.accent),
    );
}

/// The one floating shadow (`shadow` in WINNER.md) under `rect`: menus, tooltips and the
/// floating mixer drawer. Paint it before the surface.
pub fn paint_shadow_floating(painter: &Painter, rect: Rect, radius: f32, theme: &Theme) {
    let cr = CornerRadius::from(clamp_radius(rect, radius));
    painter.add(Shape::Rect(theme.shadow.as_shape(rect, cr)));
}

/// Whether a shape (recursing into `Shape::Vec`) is a blurred rect, i.e. a shadow.
pub fn is_shadow_shape(shape: &Shape) -> bool {
    match shape {
        Shape::Rect(r) => r.blur_width > 0.0,
        Shape::Vec(v) => v.iter().any(is_shadow_shape),
        _ => false,
    }
}

/// A mesh filling the rounded rect `rect` with a vertical two-stop gradient from `top` to
/// `bottom`: the rounded outline is tessellated and fanned from the centre, every vertex
/// coloured by its height. Draw a 1 px border over it to cover the unfeathered edge.
pub fn vertical_gradient_rect(rect: Rect, radius: f32, top: Color32, bottom: Color32) -> Shape {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Shape::Noop;
    }
    if top == bottom {
        return Shape::rect_filled(rect, clamp_radius(rect, radius), top);
    }
    let mut path = Vec::new();
    egui::epaint::tessellator::path::rounded_rectangle(
        &mut path,
        egui::epaint::RoundedRect::new(rect, clamp_radius(rect, radius)),
    );
    gradient_fan(rect, &path, top, bottom)
}

/// [`vertical_gradient_rect`] with per-corner radii (piano keys: 0 0 4 4).
pub fn vertical_gradient_rect_corners(
    rect: Rect,
    corners: egui::epaint::CornerRadiusF32,
    top: Color32,
    bottom: Color32,
) -> Shape {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Shape::Noop;
    }
    let mut path = Vec::new();
    egui::epaint::tessellator::path::rounded_rectangle(
        &mut path,
        egui::epaint::RoundedRect::new(rect, corners),
    );
    gradient_fan(rect, &path, top, bottom)
}

/// A mesh filling the circle at `center` with a vertical two-stop gradient.
pub fn vertical_gradient_circle(center: Pos2, radius: f32, top: Color32, bottom: Color32) -> Shape {
    if radius <= 0.0 {
        return Shape::Noop;
    }
    if top == bottom {
        return Shape::circle_filled(center, radius, top);
    }
    let n = ((radius * 2.0).ceil() as usize).clamp(16, 96);
    let path: Vec<Pos2> = (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    let rect = Rect::from_center_size(center, vec2(radius * 2.0, radius * 2.0));
    gradient_fan(rect, &path, top, bottom)
}

/// Fans `outline` (a convex polygon) from the centre of `rect` with vertex colours
/// interpolated from `top` (at `rect.min.y`) to `bottom` (at `rect.max.y`).
fn gradient_fan(rect: Rect, outline: &[Pos2], top: Color32, bottom: Color32) -> Shape {
    if outline.len() < 3 {
        return Shape::Noop;
    }
    let colour_at = |y: f32| {
        let t = ((y - rect.min.y) / rect.height().max(f32::EPSILON)).clamp(0.0, 1.0);
        top.lerp_to_gamma(bottom, t)
    };
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(outline.len() + 1);
    mesh.reserve_triangles(outline.len());
    let centre = rect.center();
    mesh.colored_vertex(centre, colour_at(centre.y));
    for p in outline {
        mesh.colored_vertex(*p, colour_at(p.y));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        let a = 1 + i;
        let b = 1 + (i + 1) % n;
        mesh.add_triangle(0, a, b);
    }
    Shape::mesh(mesh)
}

/// Paints a vertical gradient rounded rect with a 1 px `border` stroke over it.
pub fn paint_gradient_control(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    top: Color32,
    bottom: Color32,
    border: Color32,
) {
    painter.add(vertical_gradient_rect(rect, radius, top, bottom));
    painter.rect_stroke(
        rect,
        clamp_radius(rect, radius),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
}

/// A 1 x `height` `border_2` separator (toolbars: 1 x 20) at `x`, centred on `center_y`.
pub fn paint_separator(painter: &Painter, x: f32, center_y: f32, height: f32, theme: &Theme) {
    painter.vline(
        x,
        (center_y - height * 0.5)..=(center_y + height * 0.5),
        Stroke::new(1.0, theme.border_2),
    );
}

/// A 1 px `border` hairline along the bottom edge of `rect` (header / toolbar dividers).
pub fn paint_bottom_hairline(painter: &Painter, rect: Rect, theme: &Theme) {
    painter.hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(1.0, theme.border),
    );
}

/// The rect of a `w` x `h` box centred vertically on `rect`'s left edge plus `x`.
pub fn box_at(rect: Rect, x: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(rect.min.x + x, rect.center().y - h * 0.5), vec2(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_shapes_carry_no_blur() {
        let theme = Theme::dark();
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 40.0));
        let shape = flat_rect_shape(rect, 4.0, theme.panel_3, theme.border_2);
        assert!(!is_shadow_shape(&shape));
        let Shape::Rect(r) = shape else {
            panic!("a rect")
        };
        assert_eq!(r.fill, theme.panel_3);
        assert_eq!(r.stroke.color, theme.border_2);
        assert_eq!(r.stroke.width, 1.0);
        assert_eq!(r.stroke_kind, StrokeKind::Inside);
        assert_eq!(r.corner_radius, CornerRadius::same(4));
    }

    #[test]
    fn floating_shadow_is_the_only_blurred_shape() {
        let theme = Theme::light();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(180.0, 120.0));
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            paint_shadow_floating(ui.painter(), rect, 4.0, &theme);
            paint_panel(ui.painter(), rect, Tone::Panel2, 4.0, &theme);
        });
        out.textures_delta.clear();
        let shadows = out
            .shapes
            .iter()
            .filter(|c| is_shadow_shape(&c.shape))
            .count();
        assert_eq!(shadows, 1);
    }

    #[test]
    fn gradient_meshes_are_valid_and_fade_top_to_bottom() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(24.0, 12.0));
        let top = Color32::from_rgb(200, 200, 200);
        let bottom = Color32::from_rgb(20, 20, 20);
        let Shape::Mesh(mesh) = vertical_gradient_rect(rect, 3.0, top, bottom) else {
            panic!("a mesh")
        };
        assert!(mesh.is_valid());
        let brightest = mesh.vertices.iter().map(|v| v.color.r()).max().unwrap();
        let darkest = mesh.vertices.iter().map(|v| v.color.r()).min().unwrap();
        assert!(brightest > 150 && darkest < 60);
        let Shape::Mesh(circle) = vertical_gradient_circle(pos2(5.0, 5.0), 4.5, top, bottom) else {
            panic!("a mesh")
        };
        assert!(circle.is_valid());
        // Degenerate inputs are no-ops, equal stops a plain fill.
        assert!(matches!(
            vertical_gradient_rect(Rect::NOTHING, 3.0, top, bottom),
            Shape::Noop
        ));
        assert!(matches!(
            vertical_gradient_rect(rect, 3.0, top, top),
            Shape::Rect(_)
        ));
    }
}
