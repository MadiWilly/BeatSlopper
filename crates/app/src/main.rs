//! BeatSlopper binary: argument parsing and the eframe run loop.
//!
//! `--smoke-test` opens the window, renders about 60 frames, prints `smoke-test: ok` and
//! closes with exit code 0 (even without an audio device). Any eframe error exits non-zero.
//!
//! Release builds use the Windows GUI subsystem, so launching from the desktop opens no
//! console. Their output and exit code still come through `cargo run` and pipes (the gate's
//! smoke test), but not when the exe is started by hand from a console; debug builds keep it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;
use std::sync::Arc;

use app::BeatSlopperApp;
use app::window::{MIN_INNER_SIZE, REQUESTED_INNER_SIZE};

/// Command-line options understood by the binary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Args {
    smoke_test: bool,
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Args {
    let mut parsed = Args::default();
    for arg in args {
        if arg == "--smoke-test" {
            parsed.smoke_test = true;
        } else {
            eprintln!("beatslopper: ignoring unknown argument {arg:?}");
        }
    }
    parsed
}

/// Sizes are logical points; `persist_window` restores the previous session's geometry, and
/// the app applies the work-area rule on its first frame (see `app::window`: a persisted
/// size is kept only while it fits the monitor, otherwise 70 % of the work area).
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(app::window::BASE_TITLE)
            .with_app_id("beatslopper")
            .with_inner_size(REQUESTED_INNER_SIZE)
            .with_min_inner_size(MIN_INNER_SIZE)
            .with_icon(app_icon()),
        persist_window: true,
        ..Default::default()
    }
}

/// Title-bar and taskbar icon; without it eframe shows the egui logo.
fn app_icon() -> Arc<egui::IconData> {
    let png = include_bytes!("../assets/icon.png");
    Arc::new(eframe::icon_data::from_png_bytes(png).expect("assets/icon.png is a valid PNG"))
}

fn main() -> ExitCode {
    let args = parse_args(std::env::args().skip(1));

    let result = eframe::run_native(
        "BeatSlopper",
        native_options(),
        Box::new(move |cc| Ok(Box::new(BeatSlopperApp::new(cc, args.smoke_test)))),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("beatslopper: eframe error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smoke_test_flag() {
        assert!(parse_args(["--smoke-test".to_string()]).smoke_test);
        assert!(!parse_args(Vec::<String>::new()).smoke_test);
        assert!(parse_args(["--bogus".to_string(), "--smoke-test".to_string()]).smoke_test);
    }

    #[test]
    fn app_icon_decodes() {
        let icon = app_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
    }
}
