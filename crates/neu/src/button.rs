//! Buttons: pill text buttons, circular icon buttons and the light/dark theme toggle.
//!
//! Geometry and treatments follow `design/preview-a.html`: `.pill` / `.btn` (height 30,
//! padding 0 16, caps tracked label, secondary colour, primary on hover, inset while pressed),
//! `.icon-btn` (circle, 1.5 pt line icons in a box 20/48 of the diameter) and `.toggle`
//! (50x28 inset track, 22 pt knob that is accent + on-glow when dark).

use std::f32::consts::TAU;

use egui::epaint::Shape;
use egui::{
    Align2, Color32, Painter, Rect, Response, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2,
};

use crate::shadow::{
    Depth, paint_soft_circle, paint_soft_circle_with_fill, paint_soft_rect,
    paint_soft_rect_with_fill,
};
use crate::text::{self, TextKind};
use crate::{Theme, ThemeMode};

/// Line icons drawn by the painter (stroke 1.5, 20 pt box).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Stop,
    Record,
    Sun,
    Moon,
    Minus,
    Plus,
    Clear,
    /// A clock face: the RECENT projects button (QOL).
    Clock,
}

/// Height of pill buttons (`.pill` in the mock).
pub const PILL_HEIGHT: f32 = 30.0;
/// Size of the square mixer toggles ("M" / "S") drawn with [`toggle_button_sized`].
pub const TOGGLE_SQUARE: egui::Vec2 = egui::Vec2::new(28.0, 28.0);
/// Corner radius of [`toggle_button_sized`] (clamped to a pill for small sizes).
pub const TOGGLE_SIZED_RADIUS: f32 = 8.0;
/// Largest shorter side of a [`toggle_button_sized`] that takes the small (raised-s /
/// inset-s) depth; anything bigger is raised-m / inset-m.
pub const SMALL_DEPTH_MAX: f32 = 32.0;
/// Line width of icon glyphs.
pub const ICON_STROKE: f32 = 1.5;
/// Size of the theme-toggle track.
pub const TOGGLE_TRACK: egui::Vec2 = egui::Vec2::new(50.0, 28.0);
/// Diameter of the theme-toggle knob.
pub const TOGGLE_KNOB: f32 = 22.0;
/// Size of the sun/moon glyphs flanking the theme toggle and the gap to the track.
const TOGGLE_GLYPH: f32 = 16.0;
const TOGGLE_GAP: f32 = 10.0;

/// Pill text button: raised-m, inset-m while pressed. Label is drawn in caps, tracked, 12 pt,
/// secondary colour (primary on hover).
pub fn button(ui: &mut Ui, theme: &Theme, label: &str) -> Response {
    pill(ui, theme, label, None)
}

/// Pill text button that shows the ON state (accent fill + glow, on-accent text) when `on`.
pub fn toggle_button(ui: &mut Ui, theme: &Theme, label: &str, on: bool) -> Response {
    pill(ui, theme, label, Some(on))
}

/// Toggle button of an explicit `size` (e.g. [`TOGGLE_SQUARE`], 28x28, for the mixer's "M" /
/// "S"): a [`TOGGLE_SIZED_RADIUS`] rounded rect with the raised / inset (pressed) / OnGlow
/// (`on`) treatment of [`toggle_button`] and the 12 pt medium caps label centred. Per the
/// WINNER size-to-depth mapping, sizes whose shorter side is at most [`SMALL_DEPTH_MAX`]
/// (32 pt) use raised-s / inset-s; larger ones raised-m / inset-m.
pub fn toggle_button_sized(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    on: bool,
    size: egui::Vec2,
) -> Response {
    let raised = if size.min_elem() <= SMALL_DEPTH_MAX {
        Depth::RaisedS
    } else {
        Depth::RaisedM
    };
    toggle_rect(
        ui,
        theme,
        label,
        Some(on),
        size,
        TOGGLE_SIZED_RADIUS,
        raised,
    )
}

fn pill(ui: &mut Ui, theme: &Theme, label: &str, on: Option<bool>) -> Response {
    let galley = text::layout(ui.painter(), label, TextKind::Button, Color32::PLACEHOLDER);
    let size = vec2(galley.size().x + 2.0 * theme.space_4, PILL_HEIGHT);
    toggle_rect(
        ui,
        theme,
        label,
        on,
        size,
        theme.radius_pill,
        Depth::RaisedM,
    )
}

/// The shared body of [`button`], [`toggle_button`] and [`toggle_button_sized`]: a `size`
/// rounded rect of `radius` with the `raised` / `raised.pressed()` / on-glow treatment and a
/// centred label.
fn toggle_rect(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    on: Option<bool>,
    size: egui::Vec2,
    radius: f32,
    raised: Depth,
) -> Response {
    let galley = text::layout(ui.painter(), label, TextKind::Button, Color32::PLACEHOLDER);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let pressed = response.is_pointer_button_down_on();
        let hovered = response.hovered();
        let painter = ui.painter();
        let text_color = if on == Some(true) {
            let fill = if hovered {
                theme.accent_hover
            } else {
                theme.accent
            };
            paint_soft_rect_with_fill(painter, rect, radius, Depth::OnGlow, fill, theme);
            theme.on_accent
        } else {
            let depth = if pressed { raised.pressed() } else { raised };
            paint_soft_rect(painter, rect, radius, depth, theme);
            if hovered {
                theme.text_primary
            } else {
                theme.text_secondary
            }
        };
        text::paint_galley(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            galley,
            TextKind::Button,
            text_color,
        );
    }

    response.widget_info(|| match on {
        Some(on) => WidgetInfo::selected(Role::Button, ui.is_enabled(), on, label),
        None => WidgetInfo::labeled(Role::Button, ui.is_enabled(), label),
    });
    response
}

/// Circular icon button of the given diameter (48 for transport), raised-m, inset-m while
/// pressed. The glyph is a 1.5 pt line drawing in a box 20/48 of the diameter.
pub fn icon_button(ui: &mut Ui, theme: &Theme, icon: Icon, diameter: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(diameter, diameter), Sense::click());
    if ui.is_rect_visible(rect) {
        let pressed = response.is_pointer_button_down_on();
        let depth = if pressed {
            Depth::InsetM
        } else {
            Depth::RaisedM
        };
        let painter = ui.painter();
        paint_soft_circle(painter, rect.center(), diameter * 0.5, depth, theme);
        let color = if response.hovered() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let box_size = diameter * 20.0 / 48.0;
        let icon_rect = Rect::from_center_size(rect.center(), vec2(box_size, box_size));
        paint_icon(painter, icon, icon_rect, Stroke::new(ICON_STROKE, color));
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), icon.name()));
    response
}

/// Light/dark switch: a 16 pt sun glyph, a 50x28 inset pill track with a 22 pt knob (accent +
/// on-glow when dark, raised-s surface when light) and a 16 pt moon glyph. Flips `mode` when
/// clicked anywhere on it. Total size 102x28.
pub fn theme_toggle(ui: &mut Ui, theme: &Theme, mode: &mut ThemeMode) -> Response {
    let width = TOGGLE_GLYPH + TOGGLE_GAP + TOGGLE_TRACK.x + TOGGLE_GAP + TOGGLE_GLYPH;
    let (rect, response) = ui.allocate_exact_size(vec2(width, TOGGLE_TRACK.y), Sense::click());
    if response.clicked() {
        *mode = mode.toggled();
    }
    let dark = *mode == ThemeMode::Dark;

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let glyph_color = if response.hovered() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let stroke = Stroke::new(ICON_STROKE, glyph_color);

        let sun = Rect::from_center_size(
            pos2(rect.min.x + TOGGLE_GLYPH * 0.5, rect.center().y),
            vec2(TOGGLE_GLYPH, TOGGLE_GLYPH),
        );
        let moon = Rect::from_center_size(
            pos2(rect.max.x - TOGGLE_GLYPH * 0.5, rect.center().y),
            vec2(TOGGLE_GLYPH, TOGGLE_GLYPH),
        );
        paint_icon(painter, Icon::Sun, sun, stroke);
        paint_icon(painter, Icon::Moon, moon, stroke);

        let track = Rect::from_center_size(rect.center(), TOGGLE_TRACK);
        paint_soft_rect(painter, track, theme.radius_pill, Depth::InsetM, theme);

        // Knob: 3 pt inside the track at either end, animated like the mock's .15 s transition.
        let t = ui
            .ctx()
            .animate_bool_with_time(response.id.with("knob"), dark, 0.15);
        let inset = (TOGGLE_TRACK.y - TOGGLE_KNOB) * 0.5;
        let x0 = track.min.x + inset + TOGGLE_KNOB * 0.5;
        let x1 = track.max.x - inset - TOGGLE_KNOB * 0.5;
        let center = pos2(egui::lerp(x0..=x1, t), track.center().y);
        let knob_depth = if dark { Depth::OnGlow } else { Depth::RaisedS };
        let knob_fill = if dark && response.hovered() {
            theme.accent_hover
        } else {
            knob_depth.fill(theme)
        };
        paint_soft_circle_with_fill(
            painter,
            center,
            TOGGLE_KNOB * 0.5,
            knob_depth,
            knob_fill,
            theme,
        );
    }

    response
        .widget_info(|| WidgetInfo::selected(Role::Switch, ui.is_enabled(), dark, "Dark theme"));
    response
}

impl Icon {
    /// Accessible name of the icon (used as the button label for screen readers and tests).
    pub fn name(self) -> &'static str {
        match self {
            Icon::Play => "Play",
            Icon::Stop => "Stop",
            Icon::Record => "Record",
            Icon::Sun => "Light theme",
            Icon::Moon => "Dark theme",
            Icon::Minus => "Decrease",
            Icon::Plus => "Increase",
            Icon::Clear => "Clear",
            Icon::Clock => "Recent",
        }
    }
}

/// Draws `icon` as 1.5 pt (or `stroke.width`) lines fitted to `rect` (the 20 pt icon box of the
/// mock; the glyphs are defined in that 20-unit space and scaled to `rect`).
pub fn paint_icon(painter: &Painter, icon: Icon, rect: Rect, stroke: Stroke) {
    // Map from the mock's 20-unit icon box to `rect`.
    let s = rect.width().min(rect.height()) / 20.0;
    let origin = rect.center() - vec2(10.0 * s, 10.0 * s);
    let p = move |x: f32, y: f32| origin + vec2(x * s, y * s);
    match icon {
        Icon::Play => {
            // M6 3.5 v13 L16 10 z
            painter.add(Shape::closed_line(
                vec![p(6.0, 3.5), p(6.0, 16.5), p(16.0, 10.0)],
                stroke,
            ));
        }
        Icon::Stop => {
            // rect x=4.5 y=4.5 w=11 h=11 rx=1.5
            let r = Rect::from_min_max(p(4.5, 4.5), p(15.5, 15.5));
            painter.rect_stroke(r, 1.5 * s, stroke, egui::StrokeKind::Middle);
        }
        Icon::Record => {
            painter.circle_stroke(p(10.0, 10.0), 5.5 * s, stroke);
        }
        Icon::Sun => {
            // 16-unit viewBox glyph scaled by 1.25: circle r3 at (8,8) + 8 rays.
            let k = 1.25;
            painter.circle_stroke(p(8.0 * k, 8.0 * k), 3.0 * k * s, stroke);
            let rays: [[f32; 4]; 8] = [
                [8.0, 1.5, 8.0, 3.0],
                [8.0, 13.0, 8.0, 14.5],
                [1.5, 8.0, 3.0, 8.0],
                [13.0, 8.0, 14.5, 8.0],
                [3.4, 3.4, 4.4, 4.4],
                [11.6, 11.6, 12.6, 12.6],
                [3.4, 12.6, 4.4, 11.6],
                [11.6, 4.4, 12.6, 3.4],
            ];
            for [x0, y0, x1, y1] in rays {
                painter.line_segment([p(x0 * k, y0 * k), p(x1 * k, y1 * k)], stroke);
            }
        }
        Icon::Moon => {
            // 16-unit glyph: M13 10.2 A5.5 5.5 0 0 1 5.8 3 a5.5 5.5 0 1 0 7.2 7.2 z
            // Outer arc: circle (8,8) r 5.5 from P1 to P2 around the bottom-left (major arc).
            // Inner arc: circle (10.88,5.12) r 5.5 from P2 back to P1 (minor arc).
            let k = 1.25;
            let p1 = (5.8, 3.0);
            let p2 = (13.0, 10.2);
            let c1 = (8.0, 8.0);
            let c2 = (10.879_f32, 5.121_f32);
            let r = 5.5;
            let mut pts = Vec::with_capacity(40);
            let a1 = angle(c1, p1);
            let mut a2 = angle(c1, p2);
            if a2 > a1 {
                a2 -= TAU; // sweep decreasing (via left and bottom)
            }
            for i in 0..=24 {
                let a = a1 + (a2 - a1) * i as f32 / 24.0;
                pts.push(p((c1.0 + r * a.cos()) * k, (c1.1 + r * a.sin()) * k));
            }
            let b1 = angle(c2, p2);
            let mut b2 = angle(c2, p1);
            if b2 < b1 {
                b2 += TAU; // sweep increasing (via bottom and left)
            }
            for i in 1..16 {
                let a = b1 + (b2 - b1) * i as f32 / 16.0;
                pts.push(p((c2.0 + r * a.cos()) * k, (c2.1 + r * a.sin()) * k));
            }
            painter.add(Shape::closed_line(pts, stroke));
        }
        Icon::Minus => {
            painter.line_segment([p(4.0, 10.0), p(16.0, 10.0)], stroke);
        }
        Icon::Plus => {
            painter.line_segment([p(4.0, 10.0), p(16.0, 10.0)], stroke);
            painter.line_segment([p(10.0, 4.0), p(10.0, 16.0)], stroke);
        }
        Icon::Clear => {
            painter.line_segment([p(5.5, 5.5), p(14.5, 14.5)], stroke);
            painter.line_segment([p(14.5, 5.5), p(5.5, 14.5)], stroke);
        }
        Icon::Clock => {
            // Face r 7 at (10,10); hands to 12 and to 3 o'clock-ish.
            painter.circle_stroke(p(10.0, 10.0), 7.0 * s, stroke);
            painter.line_segment([p(10.0, 10.0), p(10.0, 5.5)], stroke);
            painter.line_segment([p(10.0, 10.0), p(13.5, 11.5)], stroke);
        }
    }
}

fn angle(center: (f32, f32), point: (f32, f32)) -> f32 {
    let a = (point.1 - center.1).atan2(point.0 - center.0);
    if a < 0.0 { a + TAU } else { a }
}

/// Convenience: a small (28 pt) stepper button with a minus or plus glyph (`.stepper button`
/// in the mock: raised-s, inset-s while pressed).
pub fn stepper_button(ui: &mut Ui, theme: &Theme, icon: Icon) -> Response {
    const DIAMETER: f32 = 28.0;
    let (rect, response) = ui.allocate_exact_size(vec2(DIAMETER, DIAMETER), Sense::click());
    if ui.is_rect_visible(rect) {
        let depth = if response.is_pointer_button_down_on() {
            Depth::InsetS
        } else {
            Depth::RaisedS
        };
        let painter = ui.painter();
        paint_soft_circle(painter, rect.center(), DIAMETER * 0.5, depth, theme);
        let color = if response.hovered() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(12.0, 12.0));
        paint_icon(painter, icon, icon_rect, Stroke::new(ICON_STROKE, color));
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), icon.name()));
    response
}
