//! Soft-shadow painting primitives: the one place where neumorphic depth is drawn.
//!
//! Every control in the kit is a single fill plus at most two soft shadows
//! (`design/WINNER.md`, "Shadows"). Three treatments exist:
//!
//! * **Raised** (`RaisedL/M/S`): two outer shadows under a surface-colour fill, the highlight
//!   colour towards the top-left and the dark colour towards the bottom-right. These are plain
//!   [`epaint::Shadow`] shapes (a blurred rounded rect per shadow).
//! * **Inset** (`InsetM/S`): surface fill plus a simulated inner shadow. egui has no inner
//!   shadow primitive, so the inner shadow is a single colour-graded mesh: concentric rings of
//!   the shape, the outer ring on the edge and the rings moving inwards, where each vertex
//!   carries the alpha a CSS `inset x y blur` shadow would have at that depth for the edge's
//!   outward normal (dark along the top-left edges, highlight along the bottom-right).
//! * **OnGlow**: accent fill plus a centred accent glow shadow and a dark drop shadow.
//!
//! Circles use the same treatments with circle geometry (a fully rounded square).

use egui::epaint::{self, Mesh, RectShape, Shape};
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Vec2, pos2, vec2};

use crate::{ShadowPair, Theme};

/// Which depth treatment to paint. See `design/WINNER.md` "Shadows".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// Panels: -8 -8 20 highlight / 8 8 20 dark.
    RaisedL,
    /// 48 pt icon buttons, dial rings, piano keys: -5 -5 12 / 5 5 12.
    RaisedM,
    /// Knob caps, stepper buttons, step cells, toggle knob: -3 -3 8 / 3 3 8.
    RaisedS,
    /// Segmented tracks, toggle tracks, pressed buttons: inset 4 4 10 dark / -4 -4 10 highlight.
    InsetM,
    /// Slider tracks, playhead column, lamp off: inset 2 2 6 dark / -2 -2 6 highlight.
    InsetS,
    /// Accent fill + accent glow + dark shadow (every ON state).
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

    /// The inset treatment a pressed control of this depth swaps to (raised-l/m -> inset-m,
    /// raised-s -> inset-s, inset and on-glow stay as they are).
    pub fn pressed(self) -> Depth {
        match self {
            Depth::RaisedL | Depth::RaisedM => Depth::InsetM,
            Depth::RaisedS => Depth::InsetS,
            other => other,
        }
    }

    /// The shadow pair (offset, blur) for this depth. For `OnGlow` this is the dark shadow.
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
        if self == Depth::OnGlow {
            theme.accent
        } else {
            theme.surface
        }
    }
}

/// Multiplier from a CSS `box-shadow` blur radius to epaint's `blur_width`.
///
/// CSS blurs with a Gaussian of sigma = blur / 2, whose visible penumbra spans roughly
/// +-0.75 blur around the edge. epaint feathers linearly over `blur_width` centred on the
/// edge, so a width of 1.5 x blur gives the same 50 % value at the edge and the same visible
/// extent.
pub const BLUR_SCALE: f32 = 1.5;

/// Builds the shapes for a rounded rectangle with the surface (or accent) fill and its two
/// soft shadows, back to front, without painting them. Use this when the shapes must be
/// inserted behind content that is laid out first (see [`crate::panel`]).
pub fn soft_rect_shapes(rect: Rect, radius: f32, depth: Depth, theme: &Theme) -> Vec<Shape> {
    let radius = clamp_radius(rect, radius);
    let cr = CornerRadius::from(radius);
    let mut shapes = Vec::with_capacity(3);
    match depth {
        Depth::RaisedL | Depth::RaisedM | Depth::RaisedS => {
            let pair = depth.pair(theme);
            shapes.push(outer_shadow(
                rect,
                cr,
                -pair.offset,
                pair.blur,
                theme.shadow_highlight,
            ));
            shapes.push(outer_shadow(
                rect,
                cr,
                pair.offset,
                pair.blur,
                theme.shadow_dark,
            ));
            shapes.push(Shape::Rect(RectShape::filled(rect, cr, theme.surface)));
        }
        Depth::InsetM | Depth::InsetS => {
            shapes.push(Shape::Rect(RectShape::filled(rect, cr, theme.surface)));
            shapes.push(inner_shadow_mesh(rect, radius, depth.pair(theme), theme));
        }
        Depth::OnGlow => {
            let dark = theme.on_glow_dark;
            shapes.push(outer_shadow(rect, cr, 0.0, theme.on_glow_blur, theme.glow));
            shapes.push(outer_shadow(
                rect,
                cr,
                dark.offset,
                dark.blur,
                theme.shadow_dark,
            ));
            shapes.push(Shape::Rect(RectShape::filled(rect, cr, theme.accent)));
        }
    }
    shapes
}

/// Like [`soft_rect_shapes`] but with an explicit fill colour (for hover tints, the piano
/// black-key fill, ...). The shadows are those of `depth`.
pub fn soft_rect_shapes_with_fill(
    rect: Rect,
    radius: f32,
    depth: Depth,
    fill: Color32,
    theme: &Theme,
) -> Vec<Shape> {
    let mut shapes = soft_rect_shapes(rect, radius, depth, theme);
    let cr = CornerRadius::from(clamp_radius(rect, radius));
    let fill_shape = Shape::Rect(RectShape::filled(rect, cr, fill));
    // The fill is the last shape for raised/on-glow and the first for inset.
    let idx = if depth.is_inset() {
        0
    } else {
        shapes.len() - 1
    };
    shapes[idx] = fill_shape;
    shapes
}

/// Paints a rounded rectangle with the surface fill and its two soft shadows.
///
/// `radius` is clamped to half the shorter side, so `theme.radius_pill` (999) yields a pill.
pub fn paint_soft_rect(painter: &Painter, rect: Rect, radius: f32, depth: Depth, theme: &Theme) {
    painter.extend(soft_rect_shapes(rect, radius, depth, theme));
}

/// Like [`paint_soft_rect`] with an explicit fill colour (hover tint, key-black, ...).
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

/// Builds the shapes for a circle with the surface (or accent) fill and its two soft shadows.
pub fn soft_circle_shapes(center: Pos2, radius: f32, depth: Depth, theme: &Theme) -> Vec<Shape> {
    let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    let mut shapes = soft_rect_shapes(rect, radius, depth, theme);
    // Replace the rounded-rect fill with an exact circle (CornerRadius is integer-valued).
    let idx = if depth.is_inset() {
        0
    } else {
        shapes.len() - 1
    };
    shapes[idx] = Shape::circle_filled(center, radius, depth.fill(theme));
    shapes
}

/// Paints a circle with the surface fill and its two soft shadows.
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
    let mut shapes = soft_circle_shapes(center, radius, depth, theme);
    let idx = if depth.is_inset() {
        0
    } else {
        shapes.len() - 1
    };
    shapes[idx] = Shape::circle_filled(center, radius, fill);
    painter.extend(shapes);
}

fn clamp_radius(rect: Rect, radius: f32) -> f32 {
    radius.clamp(0.0, (rect.width().min(rect.height()) * 0.5).max(0.0))
}

/// One outer shadow: a blurred rounded rect offset by `(offset, offset)`.
fn outer_shadow(rect: Rect, cr: CornerRadius, offset: f32, blur: f32, color: Color32) -> Shape {
    let o = offset.round().clamp(-127.0, 127.0) as i8;
    let shadow = epaint::Shadow {
        offset: [o, o],
        blur: (blur * BLUR_SCALE).round().clamp(0.0, 255.0) as u8,
        spread: 0,
        color,
    };
    Shape::Rect(shadow.as_shape(rect, cr))
}

/// Standard normal CDF, logistic approximation (max error about 0.01).
fn phi(x: f32) -> f32 {
    1.0 / (1.0 + (-1.702 * x).exp())
}

/// The simulated inner shadow of `pair` for a rounded rect, as one colour-graded mesh.
///
/// For a point at depth `t` inside an edge whose outward normal is `n`, a CSS
/// `inset ox oy blur` shadow has coverage `1 - Phi((t + dot(o, n)) / sigma)` with
/// `sigma = blur / 2`. The dark shadow uses `o = (+off, +off)`, the highlight `o = (-off, -off)`;
/// the two are composited per vertex and the rings are triangulated into one mesh. The outer
/// ring sits half a point outside the edge with zero alpha so the edge stays anti-aliased.
fn inner_shadow_mesh(rect: Rect, radius: f32, pair: ShadowPair, theme: &Theme) -> Shape {
    let half_min = rect.width().min(rect.height()) * 0.5;
    if half_min <= 0.5 {
        return Shape::Noop;
    }
    let sigma = (pair.blur * 0.5).max(0.5);
    let reach = (pair.offset + 2.5 * sigma).min(half_min - 0.25);
    // Ring depths: a 1 pt anti-aliasing band on the edge, then denser near the edge.
    let depths: [f32; 8] = [
        -0.5,
        0.5,
        reach * 0.12,
        reach * 0.24,
        reach * 0.38,
        reach * 0.55,
        reach * 0.76,
        reach,
    ];

    let per_corner = ((radius / 2.0).ceil() as usize).clamp(4, 12) + 1;
    let ring_len = per_corner * 4;
    let o = pair.offset;

    let mut mesh = Mesh::default();
    mesh.reserve_vertices(ring_len * depths.len());
    mesh.reserve_triangles(ring_len * 2 * (depths.len() - 1));

    for (k, &t) in depths.iter().enumerate() {
        let alpha_scale = if k == 0 { 0.0 } else { 1.0 };
        let t_eval = t.max(0.0);
        for (p, n) in rounded_rect_ring(rect, radius, t, per_corner) {
            let d = o * (n.x + n.y); // dot(n, (o, o))
            let a_dark = (1.0 - phi((t_eval + d) / sigma)) * alpha_scale;
            let a_hi = (1.0 - phi((t_eval - d) / sigma)) * alpha_scale;
            let dark = theme.shadow_dark.gamma_multiply(a_dark.clamp(0.0, 1.0));
            let hi = theme.shadow_highlight.gamma_multiply(a_hi.clamp(0.0, 1.0));
            mesh.colored_vertex(p, dark.blend(hi));
        }
    }
    for k in 0..depths.len() - 1 {
        let a0 = (k * ring_len) as u32;
        let b0 = ((k + 1) * ring_len) as u32;
        for i in 0..ring_len as u32 {
            let j = (i + 1) % ring_len as u32;
            mesh.add_triangle(a0 + i, a0 + j, b0 + i);
            mesh.add_triangle(a0 + j, b0 + j, b0 + i);
        }
    }
    Shape::mesh(mesh)
}

/// Points (with outward unit normals) along the outline of `rect` with corner `radius`,
/// moved inwards by `depth`, clockwise from the top-left corner, `per_corner` points per
/// quarter arc (the straight edges are implied by consecutive corner end points).
fn rounded_rect_ring(
    rect: Rect,
    radius: f32,
    depth: f32,
    per_corner: usize,
) -> impl Iterator<Item = (Pos2, Vec2)> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let half_min = rect.width().min(rect.height()) * 0.5;
    let depth = depth.min(half_min - 0.01);
    let r = (radius - depth).max(0.0);
    let inner = rect.shrink(depth);
    // Arc centres, clockwise from the top-left; each arc starts at `start` and sweeps +90 deg.
    let corners = [
        (pos2(inner.min.x + r, inner.min.y + r), PI),
        (pos2(inner.max.x - r, inner.min.y + r), 1.5 * PI),
        (pos2(inner.max.x - r, inner.max.y - r), 0.0),
        (pos2(inner.min.x + r, inner.max.y - r), FRAC_PI_2),
    ];
    corners.into_iter().flat_map(move |(c, start)| {
        (0..per_corner).map(move |i| {
            let a = start + FRAC_PI_2 * i as f32 / (per_corner - 1) as f32;
            let n = vec2(a.cos(), a.sin());
            (c + n * r, n)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raised_has_two_shadows_and_a_fill() {
        let theme = Theme::dark();
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 40.0));
        for depth in [
            Depth::RaisedL,
            Depth::RaisedM,
            Depth::RaisedS,
            Depth::OnGlow,
        ] {
            let shapes = soft_rect_shapes(rect, 999.0, depth, &theme);
            assert_eq!(shapes.len(), 3, "{depth:?}");
            assert!(matches!(shapes[2], Shape::Rect(_)));
        }
    }

    #[test]
    fn inset_is_fill_plus_mesh() {
        let theme = Theme::light();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 28.0));
        for depth in [Depth::InsetM, Depth::InsetS] {
            let shapes = soft_rect_shapes(rect, 999.0, depth, &theme);
            assert_eq!(shapes.len(), 2);
            assert!(matches!(shapes[0], Shape::Rect(_)));
            let Shape::Mesh(mesh) = &shapes[1] else {
                panic!("expected a mesh")
            };
            assert!(mesh.is_valid());
            assert!(!mesh.indices.is_empty());
        }
    }

    #[test]
    fn inset_is_dark_at_top_left_and_light_at_bottom_right() {
        let theme = Theme::dark();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0));
        let Shape::Mesh(mesh) = inner_shadow_mesh(rect, 10.0, theme.inset_m, &theme) else {
            panic!("expected a mesh");
        };
        // Vertices on the second ring (0.5 pt inside the edge); radius 10 -> 5 + 1 points per corner.
        let per_corner = 6;
        let ring_len = per_corner * 4;
        let ring: Vec<_> = mesh.vertices[ring_len..2 * ring_len].to_vec();
        let top = ring
            .iter()
            .find(|v| (v.pos.y - 0.5).abs() < 0.01 && v.pos.x > 50.0)
            .unwrap();
        let bottom = ring
            .iter()
            .find(|v| (v.pos.y - 99.5).abs() < 0.01 && v.pos.x > 50.0)
            .unwrap();
        // Dark colour is darker than the surface, highlight is lighter: compare luminance.
        let lum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        assert!(
            top.color.a() > 100,
            "top edge should be strongly shaded: {:?}",
            top.color
        );
        assert!(
            lum(top.color) < lum(bottom.color),
            "top={top:?} bottom={bottom:?}"
        );
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
