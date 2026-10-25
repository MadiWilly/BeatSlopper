//! BeatSlopper desktop app library.
//!
//! The binary (`src/main.rs`) only parses arguments and hands [`BeatSlopperApp`] to eframe.
//! Everything else lives here so that the UI and the state machine can be exercised by
//! `egui_kittest` and plain unit tests without opening a window or an audio device.
//!
//! Modules:
//! - [`app`]: [`BeatSlopperApp`] (the `eframe::App`) and [`AppState`] (UI-facing state).
//! - [`audio_io`]: the cpal output stream that owns the `engine::Engine`.
//! - [`input`]: computer-keyboard piano input (key map, held keys, octave, velocity).
//! - [`presets`]: the built-in synth presets shown in the SYNTH panel.
//! - [`project`]: save / load / WAV export glue over `engine::Project` (worker thread).
//! - [`ui`]: the panels, as functions over `&mut AppState` and `&neu::Theme`.

pub mod app;
pub mod audio_io;
pub mod input;
pub mod presets;
pub mod project;
pub mod ui;

pub use app::{AppState, BeatSlopperApp, default_params, resolve_theme_mode};
pub use audio_io::AudioOutput;
