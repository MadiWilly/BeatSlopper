//! BeatSlopper binary: argument parsing and the eframe run loop.
//!
//! `--smoke-test` opens the window, renders about 60 frames, prints `smoke-test: ok` and
//! closes with exit code 0 (even without an audio device). Any eframe error exits non-zero.

use std::process::ExitCode;

use app::BeatSlopperApp;

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

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BeatSlopper")
            .with_app_id("beatslopper")
            .with_inner_size([1280.0, 1140.0])
            .with_min_inner_size([1240.0, 760.0]),
        persist_window: true,
        ..Default::default()
    }
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
}
