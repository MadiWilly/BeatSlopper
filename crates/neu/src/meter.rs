//! Peak meters (`design/round3/WINNER.md` section 4, "Meters").
//!
//! A meter column is a `panel_2` track with a 1 px `border` (radius 2) filled from the bottom
//! with three colour bands: `ok` over the bottom 70 % of the column, `accent` from 70 to 88 %
//! and `clip` (= `rec`) over the top 12 %, masked by the track above the level, with 1 px
//! `panel` ticks every 4 pt (3 lit, 1 dark). The fill height follows a dB curve
//! ([`meter_fraction`]): 0 dBFS (`level = 1`) reaches the top, [`METER_FLOOR_DB`] (-60 dB)
//! sits at the bottom, and anything quieter (or `NaN`) is empty. Clip is simply a level in
//! the top band (no extra latch). Meters redraw every frame and carry no shadow.
//!
//! [`stereo_meter`] draws two [`STEREO_METER_TRACK_WIDTH`] (5) wide columns with a 2 gap for
//! a left / right pair; [`meter_horizontal`] draws the toolbar's two 72 x 5 rows; [`gr_meter`]
//! the 4 wide gain-reduction column that fills from the top.

use egui::{CornerRadius, Rect, Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2};

use crate::Theme;
use crate::flat::paint_control;

/// Default size of a channel meter column: 5 wide, 120 tall.
pub const METER_TRACK: Vec2 = Vec2::new(5.0, 120.0);
/// Default size of a stereo meter: two 5 pt columns with a 2 pt gap, 120 tall.
pub const STEREO_METER_SIZE: Vec2 = Vec2::new(12.0, 120.0);
/// Width of each column of a [`stereo_meter`].
pub const STEREO_METER_TRACK_WIDTH: f32 = 5.0;
/// Gap between the two stereo columns (and the two horizontal rows).
pub const METER_GAP: f32 = 2.0;
/// Level at the bottom of the meter, in dBFS.
pub const METER_FLOOR_DB: f32 = -60.0;
/// Fraction of the column height where the `ok` band ends and the `accent` band begins.
pub const METER_HOT_LEVEL: f32 = 0.70;
/// Fraction of the column height taken by the `clip` band at the top (88 .. 100 %).
pub const METER_HOT_TOP: f32 = 0.12;
/// Width of the tick lines across the fill (1 px, every 4 pt).
pub const METER_HOT_SEPARATOR: f32 = 1.0;
pub const METER_TICK_PITCH: f32 = 4.0;
/// Size of the horizontal meter (two 72 x 5 rows with a 2 gap: 72 x 12; 16 with its frame).
pub const METER_HORIZONTAL: Vec2 = Vec2::new(72.0, 12.0);
/// Width of the gain-reduction meter (4).
pub const GR_METER_WIDTH: f32 = 4.0;
/// Corner radius of meter columns.
pub const METER_RADIUS: f32 = 2.0;

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

/// Peak meter: a `size` column (e.g. [`METER_TRACK`], 5 x 120) filled from the bottom in
/// proportion to [`meter_fraction`]`(level)` with the `ok` / `accent` / `clip` bands. `NaN`
/// reads as silence. The response only senses hover; it reports the fraction as its value.
pub fn meter(ui: &mut Ui, theme: &Theme, level: f32, size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_meter_column(ui.painter(), theme, rect, meter_fraction(level));
    }
    let frac = meter_fraction(level);
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Meter, ui.is_enabled(), "Level");
        info.value = Some(f64::from(frac));
        info
    });
    response
}

/// Stereo peak meter: two [`STEREO_METER_TRACK_WIDTH`]-wide columns at the left and right
/// edges of `size` (e.g. [`STEREO_METER_SIZE`], 12 x 120, leaving a 2 pt gap) showing the
/// `(left, right)` levels like [`meter`]. When `size.x` is narrower than two columns each
/// takes half the width.
pub fn stereo_meter(ui: &mut Ui, theme: &Theme, levels: (f32, f32), size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let w = STEREO_METER_TRACK_WIDTH.min(size.x * 0.5).max(0.0);
        let left = Rect::from_min_size(rect.min, vec2(w, size.y));
        let right = Rect::from_min_size(pos2(rect.max.x - w, rect.min.y), vec2(w, size.y));
        paint_meter_column(ui.painter(), theme, left, meter_fraction(levels.0));
        paint_meter_column(ui.painter(), theme, right, meter_fraction(levels.1));
    }
    let frac = meter_fraction(levels.0).max(meter_fraction(levels.1));
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Meter, ui.is_enabled(), "Stereo level");
        info.value = Some(f64::from(frac));
        info
    });
    response
}

/// Horizontal stereo meter: two rows of `size` (default [`METER_HORIZONTAL`], 72 x 12 for
/// 72 x 5 rows with a 2 gap) filling from the left with the same bands.
pub fn meter_horizontal(ui: &mut Ui, theme: &Theme, levels: (f32, f32), size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let h = ((size.y - METER_GAP) * 0.5).max(1.0);
        let top = Rect::from_min_size(rect.min, vec2(size.x, h));
        let bottom = Rect::from_min_size(pos2(rect.min.x, rect.max.y - h), vec2(size.x, h));
        paint_meter_row(ui.painter(), theme, top, meter_fraction(levels.0));
        paint_meter_row(ui.painter(), theme, bottom, meter_fraction(levels.1));
    }
    let frac = meter_fraction(levels.0).max(meter_fraction(levels.1));
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Meter, ui.is_enabled(), "Stereo level");
        info.value = Some(f64::from(frac));
        info
    });
    response
}

/// Gain-reduction meter: a [`GR_METER_WIDTH`] x `height` `panel_2` column with a `border`,
/// `accent` filling from the top by `reduction` (0..=1 of the column). Named "Gain reduction".
pub fn gr_meter(ui: &mut Ui, theme: &Theme, reduction: f32, height: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(GR_METER_WIDTH, height), Sense::hover());
    let frac = if reduction.is_nan() {
        0.0
    } else {
        reduction.clamp(0.0, 1.0)
    };
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        paint_control(painter, rect, METER_RADIUS, theme.panel_2, theme.border);
        let h = frac * (rect.height() - 2.0);
        if h > 0.0 {
            let fill = Rect::from_min_size(
                pos2(rect.min.x + 1.0, rect.min.y + 1.0),
                vec2(rect.width() - 2.0, h),
            );
            painter.rect_filled(fill, 1.0, theme.accent);
        }
    }
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Meter, ui.is_enabled(), "Gain reduction");
        info.value = Some(f64::from(frac));
        info
    });
    response
}

/// The colour of the band at `frac` of the column height (0 bottom .. 1 top).
pub fn band_colour(theme: &Theme, frac: f32) -> egui::Color32 {
    if frac >= 1.0 - METER_HOT_TOP {
        theme.clip
    } else if frac >= METER_HOT_LEVEL {
        theme.accent
    } else {
        theme.ok
    }
}

/// Paints one vertical meter column into `track`: the `panel_2` track with its `border`, the
/// three bands up to `frac`, and the 1 px `panel` ticks every 4 pt.
pub fn paint_meter_column(painter: &egui::Painter, theme: &Theme, track: Rect, frac: f32) {
    paint_control(painter, track, METER_RADIUS, theme.panel_2, theme.border);
    let inner = track.shrink(1.0);
    if inner.height() <= 0.0 || inner.width() <= 0.0 {
        return;
    }
    let frac = frac.clamp(0.0, 1.0);
    if frac <= 0.0 {
        return;
    }
    let y_of = |f: f32| inner.max.y - f * inner.height();
    let bands = [
        (0.0, METER_HOT_LEVEL, theme.ok),
        (METER_HOT_LEVEL, 1.0 - METER_HOT_TOP, theme.accent),
        (1.0 - METER_HOT_TOP, 1.0, theme.clip),
    ];
    for (lo, hi, colour) in bands {
        let hi = hi.min(frac);
        if hi <= lo {
            continue;
        }
        let r = Rect::from_min_max(pos2(inner.min.x, y_of(hi)), pos2(inner.max.x, y_of(lo)));
        painter.rect_filled(r, CornerRadius::ZERO, colour);
    }
    // Ticks: 1 px panel lines every 4 pt from the bottom over the lit part.
    let top = y_of(frac);
    let mut y = inner.max.y - METER_TICK_PITCH;
    while y > top {
        painter.hline(
            inner.x_range(),
            y,
            Stroke::new(METER_HOT_SEPARATOR, theme.panel),
        );
        y -= METER_TICK_PITCH;
    }
}

/// Paints one horizontal meter row into `track`, filling from the left.
pub fn paint_meter_row(painter: &egui::Painter, theme: &Theme, track: Rect, frac: f32) {
    paint_control(painter, track, METER_RADIUS, theme.panel_2, theme.border);
    let inner = track.shrink(1.0);
    if inner.height() <= 0.0 || inner.width() <= 0.0 {
        return;
    }
    let frac = frac.clamp(0.0, 1.0);
    if frac <= 0.0 {
        return;
    }
    let x_of = |f: f32| inner.min.x + f * inner.width();
    let bands = [
        (0.0, METER_HOT_LEVEL, theme.ok),
        (METER_HOT_LEVEL, 1.0 - METER_HOT_TOP, theme.accent),
        (1.0 - METER_HOT_TOP, 1.0, theme.clip),
    ];
    for (lo, hi, colour) in bands {
        let hi = hi.min(frac);
        if hi <= lo {
            continue;
        }
        let r = Rect::from_min_max(pos2(x_of(lo), inner.min.y), pos2(x_of(hi), inner.max.y));
        painter.rect_filled(r, CornerRadius::ZERO, colour);
    }
    let right = x_of(frac);
    let mut x = inner.min.x + METER_TICK_PITCH;
    while x < right {
        painter.vline(
            x,
            inner.y_range(),
            Stroke::new(METER_HOT_SEPARATOR, theme.panel),
        );
        x += METER_TICK_PITCH;
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

    #[test]
    fn bands_split_at_70_and_88_percent() {
        let t = Theme::dark();
        assert_eq!(band_colour(&t, 0.0), t.ok);
        assert_eq!(band_colour(&t, 0.69), t.ok);
        assert_eq!(band_colour(&t, 0.70), t.accent);
        assert_eq!(band_colour(&t, 0.87), t.accent);
        assert_eq!(band_colour(&t, 0.88), t.clip);
        assert_eq!(band_colour(&t, 1.0), t.clip);
        assert_eq!(t.clip, t.rec);
    }
}
