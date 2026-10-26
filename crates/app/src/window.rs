//! Window sizing and title (M4, re-done for the Deck shell in M12).
//!
//! # Sizing at display scaling
//!
//! `ViewportBuilder::with_inner_size` takes *logical points*: egui-winit turns it into a winit
//! `LogicalSize` and winit scales that by the monitor's scale factor, so a fresh launch at
//! 125 / 150 % gets the requested size in points. Two things still make the first window the
//! wrong size:
//!
//! 1. `persist_window: true`: eframe restores the previous session's geometry (`"window"` in
//!    `%APPDATA%\beatslopper\data\app.ron`) and that *replaces* the requested size. A size
//!    persisted by an older build (1280 x 1140 before M12) or on another monitor comes back
//!    at every launch. eframe clamps it only to the largest monitor.
//! 2. A monitor whose work area is smaller than the request (a 1080p display at 150 % has
//!    only 1280 x 720 points).
//!
//! The rule (`design/round3/WINNER.md` section 5, "Window sizing"), all in logical points:
//! the default inner size is 70 % of the work area clamped to [`DEFAULT_MIN`] ..
//! [`DEFAULT_MAX`], then reduced to the work area minus [`WORK_AREA_MARGIN`] when that is
//! smaller, floored at [`MIN_INNER_SIZE`] ([`default_inner_size`]). A persisted geometry is
//! kept only while it still fits the work area ([`fits_work_area`]); otherwise the rule runs
//! again ([`initial_inner_size`]). egui reports the monitor size, not the work area, so the
//! margin stands in for the taskbar and the title bar.
//!
//! The check lives in `BeatSlopperApp::tick`: on the first frame that reports a monitor size
//! the target is [`initial_inner_size`] over the actual inner size (what eframe restored or
//! what the OS gave) and the monitor; if the actual size differs from the target by more than
//! [`RESIZE_TOLERANCE`] in either direction one `ViewportCommand::InnerSize(target)` is sent.
//! The check is skipped when egui reports no monitor (headless / `egui_kittest`). The layout
//! itself works from [`MIN_INNER_SIZE`] up: every window body scrolls (`crate::ui::shell`).

use std::path::Path;

use egui::Vec2;

/// Smallest inner size the window can be resized to (and the floor of the sizing rule): the
/// narrow layout of WINNER section 5.
pub const MIN_INNER_SIZE: Vec2 = Vec2::new(840.0, 600.0);

/// Lower clamp of the 70 % rule: the design size of the wide layout.
pub const DEFAULT_MIN: Vec2 = Vec2::new(1280.0, 800.0);

/// Upper clamp of the 70 % rule.
pub const DEFAULT_MAX: Vec2 = Vec2::new(1920.0, 1200.0);

/// Share of the work area the default window takes.
pub const WORK_AREA_SHARE: f32 = 0.7;

/// Points kept free of the work area (width, height) for the title bar, borders and the
/// taskbar egui's monitor size does not subtract.
pub const WORK_AREA_MARGIN: Vec2 = Vec2::new(40.0, 60.0);

/// The size the viewport asks for before the monitor is known (the first frame corrects it).
pub const REQUESTED_INNER_SIZE: Vec2 = DEFAULT_MIN;

/// How much the actual size may differ from the target (fraction of each dimension) before
/// the first-frame check resizes it.
pub const RESIZE_TOLERANCE: f32 = 0.02;

/// Title shown when the project is clean and has no file.
pub const BASE_TITLE: &str = "BeatSlopper";

/// Whether `work_area` is a usable size (positive, finite).
fn usable(work_area: Vec2) -> bool {
    work_area.x.is_finite() && work_area.y.is_finite() && work_area.x > 0.0 && work_area.y > 0.0
}

/// The first-launch inner size for a monitor whose work area is `work_area` points:
/// `clamp(0.7 x work area, DEFAULT_MIN, DEFAULT_MAX)`, then no larger than the work area
/// minus [`WORK_AREA_MARGIN`], never below [`MIN_INNER_SIZE`]. `None` (unknown monitor) gives
/// [`DEFAULT_MIN`].
#[must_use]
pub fn default_inner_size(work_area: Option<Vec2>) -> Vec2 {
    let Some(work_area) = work_area.filter(|w| usable(*w)) else {
        return DEFAULT_MIN;
    };
    let share = work_area * WORK_AREA_SHARE;
    let clamped = share.max(DEFAULT_MIN).min(DEFAULT_MAX);
    let available = work_area - WORK_AREA_MARGIN;
    clamped.min(available).max(MIN_INNER_SIZE)
}

/// Whether a window of `size` fits inside `work_area` minus the margin (a persisted
/// geometry is only restored while this holds). An unknown work area fits everything.
#[must_use]
pub fn fits_work_area(size: Vec2, work_area: Option<Vec2>) -> bool {
    let Some(work_area) = work_area.filter(|w| usable(*w)) else {
        return true;
    };
    let available = work_area - WORK_AREA_MARGIN;
    size.x <= available.x + 0.5 && size.y <= available.y + 0.5
}

/// The size the window should have after its first frame: `persisted` (what eframe restored,
/// or the size the OS opened at) when it fits the work area and is at least
/// [`MIN_INNER_SIZE`]; otherwise [`default_inner_size`] for the work area.
#[must_use]
pub fn initial_inner_size(persisted: Option<Vec2>, work_area: Option<Vec2>) -> Vec2 {
    match persisted {
        Some(size)
            if size.x >= MIN_INNER_SIZE.x - 0.5
                && size.y >= MIN_INNER_SIZE.y - 0.5
                && fits_work_area(size, work_area) =>
        {
            size
        }
        _ => default_inner_size(work_area),
    }
}

/// `true` when `actual` differs from `target` by more than [`RESIZE_TOLERANCE`] in width or
/// height.
#[must_use]
pub fn needs_resize(actual: Vec2, target: Vec2) -> bool {
    let dx = (actual.x - target.x).abs();
    let dy = (actual.y - target.y).abs();
    dx > target.x * RESIZE_TOLERANCE || dy > target.y * RESIZE_TOLERANCE
}

/// Whether a window `width` points wide uses the narrow layout (WINNER: the state at or under
/// 1000 wide: the browser rail, the 400 pt rack without knob columns, the shorter toolbar).
#[must_use]
pub fn is_narrow(width: f32) -> bool {
    width <= NARROW_WIDTH
}

/// Widest window that still uses the narrow layout.
pub const NARROW_WIDTH: f32 = 1000.0;

/// The window title: `BeatSlopper`, with ` – <file>` when the project has a path and a
/// trailing ` *` while there are unsaved changes.
#[must_use]
pub fn window_title(project: Option<&Path>, dirty: bool) -> String {
    let mut title = BASE_TITLE.to_owned();
    if let Some(path) = project {
        title.push_str(" – ");
        title.push_str(&crate::project::file_label(path));
    }
    if dirty {
        title.push_str(" *");
    }
    title
}

/// [`window_title`] followed by the tempo and the edit pattern (QOL pass):
/// `BeatSlopper – beat.beatslopper.json * · 128 BPM · A`.
#[must_use]
pub fn window_title_full(project: Option<&Path>, dirty: bool, bpm: &str, pattern: char) -> String {
    format!("{} · {bpm} BPM · {pattern}", window_title(project, dirty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn full_title_adds_tempo_and_pattern() {
        assert_eq!(
            window_title_full(None, false, "120", 'A'),
            "BeatSlopper · 120 BPM · A"
        );
        let path = PathBuf::from("C:/songs/beat.beatslopper.json");
        assert_eq!(
            window_title_full(Some(&path), true, "128.5", 'C'),
            "BeatSlopper – beat.beatslopper.json * · 128.5 BPM · C"
        );
    }

    #[test]
    fn the_default_size_is_seventy_percent_of_the_work_area_clamped() {
        // 4K at 150 % = 2560 x 1440 points: 70 % is inside the clamp.
        assert_eq!(
            default_inner_size(Some(Vec2::new(2560.0, 1440.0))),
            Vec2::new(1792.0, 1008.0)
        );
        // 4K at 100 %: 70 % would be 2688 x 1512, clamped to the maximum.
        assert_eq!(
            default_inner_size(Some(Vec2::new(3840.0, 2160.0))),
            DEFAULT_MAX
        );
        // 1080p at 100 %: 70 % is 1344 x 756, the height clamps up to 800.
        assert_eq!(
            default_inner_size(Some(Vec2::new(1920.0, 1080.0))),
            Vec2::new(1344.0, 800.0)
        );
        // 1080p at 125 % = 1536 x 864: 1280 x 800 fits the work area minus the margin (804).
        assert_eq!(
            default_inner_size(Some(Vec2::new(1536.0, 864.0))),
            Vec2::new(1280.0, 800.0)
        );
        // 1080p at 150 % = 1280 x 720: the work area minus the margin wins (1240 x 660).
        assert_eq!(
            default_inner_size(Some(Vec2::new(1280.0, 720.0))),
            Vec2::new(1240.0, 660.0)
        );
        // A tiny monitor: never below the minimum.
        assert_eq!(
            default_inner_size(Some(Vec2::new(800.0, 600.0))),
            MIN_INNER_SIZE
        );
        // Unknown or nonsense monitor: the design size.
        assert_eq!(default_inner_size(None), DEFAULT_MIN);
        assert_eq!(default_inner_size(Some(Vec2::ZERO)), DEFAULT_MIN);
        assert_eq!(
            default_inner_size(Some(Vec2::new(f32::NAN, 1.0))),
            DEFAULT_MIN
        );
    }

    #[test]
    fn a_persisted_size_is_kept_only_while_it_fits() {
        let work_area = Some(Vec2::new(1536.0, 864.0));
        // Fits: kept as is.
        assert_eq!(
            initial_inner_size(Some(Vec2::new(1400.0, 780.0)), work_area),
            Vec2::new(1400.0, 780.0)
        );
        // The pre-M12 1280 x 1140 does not fit 864 - 60: the rule runs again.
        assert_eq!(
            initial_inner_size(Some(Vec2::new(1280.0, 1140.0)), work_area),
            Vec2::new(1280.0, 800.0)
        );
        // Below the minimum (the neu gallery's size, a squeezed window): the rule again.
        assert_eq!(
            initial_inner_size(Some(Vec2::new(760.0, 520.0)), work_area),
            Vec2::new(1280.0, 800.0)
        );
        // Nothing persisted: the rule.
        assert_eq!(
            initial_inner_size(None, Some(Vec2::new(2560.0, 1440.0))),
            Vec2::new(1792.0, 1008.0)
        );
        // Unknown monitor: anything at least the minimum is kept.
        assert!(fits_work_area(Vec2::new(5000.0, 5000.0), None));
        assert_eq!(
            initial_inner_size(Some(Vec2::new(1300.0, 900.0)), None),
            Vec2::new(1300.0, 900.0)
        );
        assert!(!fits_work_area(Vec2::new(1500.0, 800.0), work_area));
        assert!(fits_work_area(Vec2::new(1496.0, 804.0), work_area));
    }

    #[test]
    fn resize_only_when_clearly_different() {
        let target = Vec2::new(1280.0, 800.0);
        assert!(!needs_resize(target, target));
        // Within 2 %: the OS's rounding at a scale factor is left alone.
        assert!(!needs_resize(Vec2::new(1279.0, 801.0), target));
        assert!(needs_resize(Vec2::new(1280.0, 1140.0), target));
        assert!(needs_resize(Vec2::new(853.0, 800.0), target));
        assert!(needs_resize(Vec2::new(1400.0, 800.0), target), "bigger too");
    }

    #[test]
    fn narrow_is_at_or_under_a_thousand_points() {
        assert!(is_narrow(853.0), "the 150 % case");
        assert!(is_narrow(MIN_INNER_SIZE.x));
        assert!(is_narrow(1000.0));
        assert!(!is_narrow(1001.0));
        assert!(!is_narrow(1280.0));
    }

    #[test]
    fn title_marks_the_file_and_unsaved_changes() {
        assert_eq!(window_title(None, false), "BeatSlopper");
        assert_eq!(window_title(None, true), "BeatSlopper *");
        let path = PathBuf::from("C:/songs/beat.beatslopper.json");
        assert_eq!(
            window_title(Some(&path), false),
            "BeatSlopper – beat.beatslopper.json"
        );
        assert_eq!(
            window_title(Some(&path), true),
            "BeatSlopper – beat.beatslopper.json *"
        );
    }
}
