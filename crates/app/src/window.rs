//! Window sizing and title (M4).
//!
//! # Sizing at display scaling
//!
//! `ViewportBuilder::with_inner_size` takes *logical points*: egui-winit turns it into a winit
//! `LogicalSize` (times egui's zoom factor, 1.0 here) and winit scales that by the monitor's
//! scale factor, so a fresh launch at 125 / 150 % gets the requested size in points (measured
//! on a 4K display at 150 %: inner rect 1280 x 1140 points = 1920 x 1710 pixels). Two things
//! still make the window open smaller than [`REQUESTED_INNER_SIZE`]:
//!
//! 1. `persist_window: true`: eframe restores the previous session's geometry (`"window"` in
//!    `%APPDATA%\beatslopper\data\app.ron`) and that *replaces* the requested size. A size
//!    persisted by an older build with a smaller window (or a window that was squeezed by
//!    the OS) comes back at every launch. eframe clamps it only to the largest monitor.
//! 2. A monitor whose work area is smaller than the request: a 1080p display at 125 % has
//!    1536 x 864 points, at 150 % only 1280 x 720. The window cannot fit; the OS may clamp or
//!    let it hang off the screen.
//!
//! The fix lives in `BeatSlopperApp::tick`: on the first frame that reports a monitor size,
//! the target is [`fit_inner_size`] (the request, clamped to the monitor minus
//! [`WORK_AREA_MARGIN`] for the taskbar and title bar, but never below [`MIN_INNER_SIZE`]);
//! if the actual inner rect is smaller than that by more than [`RESIZE_TOLERANCE`] in either
//! direction, one `ViewportCommand::InnerSize(target)` is sent. A remembered size within the
//! tolerance is kept (the user's choice); when the window cannot fit at all the main area
//! scrolls (`crate::ui::main_area`). The check is skipped when egui reports no monitor
//! (headless / `egui_kittest`).

use std::path::Path;

use egui::Vec2;

/// Inner size the window asks for, in logical points: wide enough for the SYNTH | FX row and
/// tall enough to show the whole sequencer above the KEYS panel without scrolling.
pub const REQUESTED_INNER_SIZE: Vec2 = Vec2::new(1280.0, 1140.0);

/// Smallest inner size the window can be resized to (the panels stack with a scroll area).
pub const MIN_INNER_SIZE: Vec2 = Vec2::new(1240.0, 760.0);

/// Points kept free of the monitor's size for the taskbar, title bar and borders.
pub const WORK_AREA_MARGIN: f32 = 60.0;

/// How much smaller than the target (fraction of each dimension) the window may be before
/// the first-frame check resizes it.
pub const RESIZE_TOLERANCE: f32 = 0.10;

/// Title shown when the project is clean and has no file.
pub const BASE_TITLE: &str = "BeatSlopper";

/// The size to open at: `requested`, clamped to what the monitor can show (`monitor` minus
/// [`WORK_AREA_MARGIN`] per axis) but never below `min`. `None` for the monitor keeps the
/// request as is.
#[must_use]
pub fn fit_inner_size(requested: Vec2, min: Vec2, monitor: Option<Vec2>) -> Vec2 {
    let Some(monitor) = monitor else {
        return requested;
    };
    if !(monitor.x > 0.0 && monitor.y > 0.0) {
        return requested;
    }
    let available = monitor - Vec2::splat(WORK_AREA_MARGIN);
    requested.min(available).max(min)
}

/// `true` when `actual` is smaller than `target` by more than [`RESIZE_TOLERANCE`] in width
/// or height (a bigger window is left alone).
#[must_use]
pub fn needs_resize(actual: Vec2, target: Vec2) -> bool {
    let threshold = target * (1.0 - RESIZE_TOLERANCE);
    actual.x < threshold.x || actual.y < threshold.y
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_large_monitor_keeps_the_requested_size() {
        // 4K at 150 % = 2560 x 1440 points.
        assert_eq!(
            fit_inner_size(
                REQUESTED_INNER_SIZE,
                MIN_INNER_SIZE,
                Some(Vec2::new(2560.0, 1440.0))
            ),
            REQUESTED_INNER_SIZE
        );
        assert_eq!(
            fit_inner_size(REQUESTED_INNER_SIZE, MIN_INNER_SIZE, None),
            REQUESTED_INNER_SIZE
        );
        assert_eq!(
            fit_inner_size(REQUESTED_INNER_SIZE, MIN_INNER_SIZE, Some(Vec2::ZERO)),
            REQUESTED_INNER_SIZE
        );
    }

    #[test]
    fn a_small_monitor_clamps_to_its_work_area_but_not_below_the_minimum() {
        // 1080p at 125 % = 1536 x 864 points: the height is clamped to 804.
        let fit = fit_inner_size(
            REQUESTED_INNER_SIZE,
            MIN_INNER_SIZE,
            Some(Vec2::new(1536.0, 864.0)),
        );
        assert_eq!(fit, Vec2::new(1280.0, 804.0));
        // 1080p at 150 % = 1280 x 720 points: below the minimum in both axes, so the minimum
        // wins and the main area scrolls.
        let fit = fit_inner_size(
            REQUESTED_INNER_SIZE,
            MIN_INNER_SIZE,
            Some(Vec2::new(1280.0, 720.0)),
        );
        assert_eq!(fit, MIN_INNER_SIZE);
        // 1080p at 100 %: width fits, height is clamped to 1020.
        let fit = fit_inner_size(
            REQUESTED_INNER_SIZE,
            MIN_INNER_SIZE,
            Some(Vec2::new(1920.0, 1080.0)),
        );
        assert_eq!(fit, Vec2::new(1280.0, 1020.0));
    }

    #[test]
    fn resize_only_when_clearly_smaller() {
        let target = REQUESTED_INNER_SIZE;
        assert!(!needs_resize(target, target));
        assert!(
            !needs_resize(Vec2::new(1400.0, 1300.0), target),
            "bigger is fine"
        );
        // 5 % short (a remembered size): kept.
        assert!(!needs_resize(Vec2::new(1280.0, 1096.0), target));
        assert!(!needs_resize(Vec2::new(1216.0, 1140.0), target));
        // Clearly short in one axis: resized.
        assert!(needs_resize(Vec2::new(1280.0, 800.0), target));
        assert!(needs_resize(Vec2::new(1000.0, 1140.0), target));
        // The neu gallery's 760 x 520 at 150 %.
        assert!(needs_resize(Vec2::new(760.0, 520.0), target));
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
