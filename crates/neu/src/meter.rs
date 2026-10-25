//! Peak meters for the mixer strips (M6).
//!
//! A meter is a vertical inset-s track filled from the bottom with the accent colour. The
//! fill height follows a dB curve ([`meter_fraction`]): 0 dBFS (`level = 1`) reaches the top,
//! [`METER_FLOOR_DB`] (-60 dB) sits at the bottom, and anything quieter (or `NaN`) is empty.
//! Above [`METER_HOT_LEVEL`] (-6 dBFS, `level = 0.5`) the top [`METER_HOT_TOP`] (10 %) of the
//! fill turns `accent_hover` as a "hot" band. Meters redraw every frame, so they carry no glow
//! shadow: just the inset track and a flat fill.
//!
//! [`stereo_meter`] draws two [`STEREO_METER_TRACK_WIDTH`]-wide tracks at either edge of its
//! size for a left/right pair.

use egui::{CornerRadius, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo, pos2, vec2};

use crate::Theme;
use crate::shadow::{Depth, paint_soft_rect};

/// Default size of a channel meter track: 6 wide, 140 tall (the fader's track).
pub const METER_TRACK: Vec2 = Vec2::new(6.0, 140.0);
/// Default size of the master's stereo meter: two 4 pt tracks with a 2 pt gap, 140 tall.
pub const STEREO_METER_SIZE: Vec2 = Vec2::new(10.0, 140.0);
/// Width of each track of a [`stereo_meter`].
pub const STEREO_METER_TRACK_WIDTH: f32 = 4.0;
/// Level at the bottom of the meter, in dBFS.
pub const METER_FLOOR_DB: f32 = -60.0;
/// Linear level (-6 dBFS) above which the top of the fill turns `accent_hover`.
pub const METER_HOT_LEVEL: f32 = 0.5;
/// Fraction of the fill height painted in `accent_hover` when the level is hot.
pub const METER_HOT_TOP: f32 = 0.1;

/// The filled fraction `0..=1` of a meter for a linear peak `level`: a dB curve with 0 dBFS
/// (`level = 1`) at the top and [`METER_FLOOR_DB`] at the bottom, so
/// `frac = (20 log10(level) + 60) / 60` clamped. `NaN`, zero and negative levels give 0;
/// levels above 1 clip to 1.
pub fn meter_fraction(level: f32) -> f32 {
    if level.is_nan() || level <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * level.log10();
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
}

/// Peak meter: a `size` inset-s track (e.g. [`METER_TRACK`], 6x140) filled from the bottom
/// with the accent in proportion to [`meter_fraction`]`(level)`; the top 10 % of the fill is
/// `accent_hover` once `level` exceeds [`METER_HOT_LEVEL`] (-6 dBFS). `NaN` reads as silence.
/// No glow (the meter changes every frame). The response only senses hover.
pub fn meter(ui: &mut Ui, theme: &Theme, level: f32, size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_meter_track(ui, theme, rect, level);
    }
    let frac = meter_fraction(level);
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Meter, ui.is_enabled(), "Level");
        info.value = Some(f64::from(frac));
        info
    });
    response
}

/// Stereo peak meter: two [`STEREO_METER_TRACK_WIDTH`]-wide tracks at the left and right
/// edges of `size` (e.g. [`STEREO_METER_SIZE`], 10x140, leaving a 2 pt gap) showing the
/// `(left, right)` levels like [`meter`]. When `size.x` is narrower than two tracks each
/// takes half the width.
pub fn stereo_meter(ui: &mut Ui, theme: &Theme, levels: (f32, f32), size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let w = STEREO_METER_TRACK_WIDTH.min(size.x * 0.5).max(0.0);
        let left = Rect::from_min_size(rect.min, vec2(w, size.y));
        let right = Rect::from_min_size(pos2(rect.max.x - w, rect.min.y), vec2(w, size.y));
        paint_meter_track(ui, theme, left, levels.0);
        paint_meter_track(ui, theme, right, levels.1);
    }
    let frac = meter_fraction(levels.0).max(meter_fraction(levels.1));
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Meter, ui.is_enabled(), "Stereo level");
        info.value = Some(f64::from(frac));
        info
    });
    response
}

/// Paints one meter track into `track`: the inset-s background, then the fill from the
/// bottom, then the hot band over its top when the level is above [`METER_HOT_LEVEL`].
fn paint_meter_track(ui: &Ui, theme: &Theme, track: Rect, level: f32) {
    let painter = ui.painter();
    let radius = track.width().min(track.height()) * 0.5;
    paint_soft_rect(painter, track, radius, Depth::InsetS, theme);

    let frac = meter_fraction(level);
    let fill_height = frac * track.height();
    if fill_height <= 0.0 {
        return;
    }
    let fill = Rect::from_min_max(pos2(track.min.x, track.max.y - fill_height), track.max);
    // CornerRadius is integer valued; the fill clamps to a pill like the track.
    let r = radius.round().clamp(0.0, 255.0) as u8;
    painter.rect_filled(fill, CornerRadius::same(r), theme.accent);

    if level > METER_HOT_LEVEL {
        let hot_height = (fill_height * METER_HOT_TOP).max(1.0).min(fill_height);
        let hot = Rect::from_min_size(fill.min, vec2(track.width(), hot_height));
        let corners = CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        };
        painter.rect_filled(hot, corners, theme.accent_hover);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_follows_the_db_curve() {
        assert_eq!(meter_fraction(0.0), 0.0);
        assert_eq!(meter_fraction(1.0), 1.0);
        assert_eq!(meter_fraction(0.001), 0.0);
        assert!((meter_fraction(0.5) - 0.8997).abs() < 1e-3);
        // -30 dB is half way.
        assert!((meter_fraction(10f32.powf(-1.5)) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn fraction_is_safe_for_bad_input() {
        assert_eq!(meter_fraction(f32::NAN), 0.0);
        assert_eq!(meter_fraction(-1.0), 0.0);
        assert_eq!(meter_fraction(f32::NEG_INFINITY), 0.0);
        assert_eq!(meter_fraction(f32::INFINITY), 1.0);
        assert_eq!(meter_fraction(7.0), 1.0);
    }
}
