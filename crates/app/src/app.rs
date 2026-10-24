//! `BeatSlopperApp`: the `eframe::App`, plus the UI-facing [`AppState`].
//!
//! Theme policy (docs/ARCHITECTURE.md): the mode follows the OS (`ctx.system_theme()`,
//! falling back to dark) unless the user flipped the manual toggle, which stores an override.
//! Only the override is persisted (eframe storage, key [`THEME_OVERRIDE_KEY`]); egui's own
//! memory is not persisted so the OS theme never leaks into storage.
//!
//! Keyboard piano (M1): every frame [`BeatSlopperApp::tick`] feeds the frame's egui events to
//! [`crate::input::InputState`] and forwards the resulting note events to the engine as
//! `Message::NoteOn` / `Message::NoteOff`, mirroring them into [`AppState::held_notes`] for
//! the on-screen keys. Mouse presses on the on-screen piano go through the same path.

use eframe::{App, CreationContext, Frame, Storage};
use engine::{EngineHandle, Message};
use neu::{Theme, ThemeMode};
use serde::{Deserialize, Serialize};

use crate::audio_io::AudioOutput;
use crate::input::{InputState, NoteEvent};

/// eframe storage key for the manual theme override.
pub const THEME_OVERRIDE_KEY: &str = "theme_override";

/// Frames to render before `--smoke-test` closes the window.
pub const SMOKE_TEST_FRAMES: u32 = 60;

/// Serialisable mirror of [`ThemeMode`] for storage (keeps `neu` free of serde).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredThemeMode {
    Light,
    Dark,
}

impl From<ThemeMode> for StoredThemeMode {
    fn from(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => StoredThemeMode::Light,
            ThemeMode::Dark => StoredThemeMode::Dark,
        }
    }
}

impl From<StoredThemeMode> for ThemeMode {
    fn from(mode: StoredThemeMode) -> Self {
        match mode {
            StoredThemeMode::Light => ThemeMode::Light,
            StoredThemeMode::Dark => ThemeMode::Dark,
        }
    }
}

/// Resolves the effective theme mode: a manual override beats the OS theme; no OS theme means
/// dark.
pub fn resolve_theme_mode(
    theme_override: Option<ThemeMode>,
    system_theme: Option<egui::Theme>,
) -> ThemeMode {
    theme_override.unwrap_or_else(|| {
        system_theme
            .map(ThemeMode::from_egui)
            .unwrap_or(ThemeMode::Dark)
    })
}

fn to_egui_theme(mode: ThemeMode) -> egui::Theme {
    match mode {
        ThemeMode::Light => egui::Theme::Light,
        ThemeMode::Dark => egui::Theme::Dark,
    }
}

/// Everything the UI functions read and mutate. Constructible without a window or an audio
/// device ([`AppState::headless`]) so the panels can be rendered under `egui_kittest`.
pub struct AppState {
    /// Manual theme choice; `None` means "follow the OS".
    pub theme_override: Option<ThemeMode>,
    /// The mode currently shown. The theme toggle widget edits this; `BeatSlopperApp` turns a
    /// change into an override.
    pub mode: ThemeMode,
    /// UI-side handle to the engine (lock-free queue).
    pub engine: EngineHandle,
    /// Whether the M0 test tone is on.
    pub test_tone: bool,
    /// Master gain, linear 0..=1 in the UI.
    pub master_gain: f32,
    /// `Some` while an output stream is running; `None` when no device could be opened.
    pub audio: Option<AudioOutput>,
    /// Set when the audio device could not be opened (shown in the transport bar).
    pub audio_error: Option<String>,
    /// Computer-keyboard piano state (held keys, octave, velocity).
    pub input: InputState,
    /// Which MIDI notes are sounding (keyboard or mouse), for the on-screen piano.
    pub held_notes: [bool; 128],
    /// Note-on messages sent so far (diagnostics; the tests use it to prove key repeat does
    /// not retrigger).
    pub note_on_count: u64,
    /// Note-off messages sent so far (diagnostics).
    pub note_off_count: u64,
}

impl AppState {
    /// State without an audio device, for tests and for the no-device fallback. Returns the
    /// `Engine` too so tests can render offline and check the messages arrived.
    pub fn headless(mode: ThemeMode) -> (Self, engine::Engine) {
        let (handle, engine) = engine::new_engine(48_000.0);
        (
            Self {
                theme_override: None,
                mode,
                engine: handle,
                test_tone: false,
                master_gain: 0.8,
                audio: None,
                audio_error: None,
                input: InputState::new(),
                held_notes: [false; 128],
                note_on_count: 0,
                note_off_count: 0,
            },
            engine,
        )
    }

    /// Tries to open the default audio device. On failure the app keeps running silently.
    pub fn with_audio(mode: ThemeMode) -> Self {
        match AudioOutput::start(engine::new_engine) {
            Ok((audio, handle)) => {
                let mut state = Self {
                    theme_override: None,
                    mode,
                    engine: handle,
                    test_tone: false,
                    master_gain: 0.8,
                    audio: Some(audio),
                    audio_error: None,
                    input: InputState::new(),
                    held_notes: [false; 128],
                    note_on_count: 0,
                    note_off_count: 0,
                };
                state.send_master_gain();
                state
            }
            Err(err) => {
                eprintln!("beatslopper: audio disabled: {err}");
                let (mut state, _engine) = Self::headless(mode);
                state.audio_error = Some(err);
                state
            }
        }
    }

    /// Flips the test tone and tells the engine.
    pub fn toggle_test_tone(&mut self) {
        self.set_test_tone(!self.test_tone);
    }

    /// Sets the test tone and tells the engine.
    pub fn set_test_tone(&mut self, on: bool) {
        self.test_tone = on;
        self.engine.send(Message::TestTone(on));
    }

    /// Sends the current master gain to the engine.
    pub fn send_master_gain(&mut self) {
        let gain = self.master_gain.clamp(0.0, 1.0);
        self.engine.send(Message::MasterGain(gain));
    }

    /// Starts `note` (MIDI) at `velocity` (0..=1): tells the engine and lights the key.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let note = note.min(127);
        self.held_notes[usize::from(note)] = true;
        self.note_on_count += 1;
        self.engine.send(Message::NoteOn {
            note,
            velocity: velocity.clamp(0.0, 1.0),
        });
    }

    /// Stops `note` (MIDI): tells the engine and unlights the key.
    pub fn note_off(&mut self, note: u8) {
        let note = note.min(127);
        self.held_notes[usize::from(note)] = false;
        self.note_off_count += 1;
        self.engine.send(Message::NoteOff { note });
    }

    /// Releases every key (computer keyboard and on-screen) and silences the engine.
    pub fn all_notes_off(&mut self) {
        self.input.release_all();
        self.held_notes = [false; 128];
        self.engine.send(Message::AllNotesOff);
    }

    /// Applies note events from the keyboard (or the on-screen piano).
    pub fn apply_note_events(&mut self, events: &[NoteEvent]) {
        for event in events {
            match *event {
                NoteEvent::On { note, velocity } => self.note_on(note, velocity),
                NoteEvent::Off { note } => self.note_off(note),
            }
        }
    }

    /// Reads this frame's egui events and plays the computer keyboard. While a text edit has
    /// focus the keys type instead of play; anything still held is released so no note sticks.
    pub fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let events = if ctx.text_edit_focused() {
            self.input.release_all()
        } else {
            ctx.input(|i| self.input.handle_events(&i.events))
        };
        self.apply_note_events(&events);
    }

    /// Flips the theme mode as a manual choice (stored as the override).
    pub fn toggle_theme(&mut self) {
        self.set_theme_override(self.mode.toggled());
    }

    /// Records a manual theme choice.
    pub fn set_theme_override(&mut self, mode: ThemeMode) {
        self.theme_override = Some(mode);
        self.mode = mode;
    }

    /// If the toggle widget changed `mode` away from the resolved mode, make it an override.
    /// Returns `true` when an override was recorded.
    pub fn commit_mode_change(&mut self, previous: ThemeMode) -> bool {
        if self.mode != previous {
            self.theme_override = Some(self.mode);
            true
        } else {
            false
        }
    }

    /// Current theme tokens for `mode`.
    pub fn theme(&self) -> Theme {
        Theme::for_mode(self.mode)
    }
}

/// The eframe application.
pub struct BeatSlopperApp {
    /// Tokens for the mode currently applied to the egui context.
    theme: Theme,
    state: AppState,
    smoke_test: bool,
    frames: u32,
    smoke_done: bool,
    /// Whether `theme` has been applied to the context at least once.
    applied: bool,
}

impl BeatSlopperApp {
    /// Builds the app: loads the persisted theme override, opens audio (failure is logged and
    /// tolerated) and applies the initial theme.
    pub fn new(cc: &CreationContext<'_>, smoke_test: bool) -> Self {
        let theme_override: Option<ThemeMode> = cc
            .storage
            .and_then(|s| eframe::get_value::<Option<StoredThemeMode>>(s, THEME_OVERRIDE_KEY))
            .flatten()
            .map(ThemeMode::from);
        let mode = resolve_theme_mode(theme_override, cc.egui_ctx.system_theme());

        let mut state = AppState::with_audio(mode);
        state.theme_override = theme_override;

        let mut app = Self {
            theme: Theme::for_mode(mode),
            state,
            smoke_test,
            frames: 0,
            smoke_done: false,
            applied: false,
        };
        app.apply_theme(&cc.egui_ctx);
        app
    }

    /// Test/headless constructor: no storage, no audio, explicit mode.
    pub fn headless(mode: ThemeMode, smoke_test: bool) -> (Self, engine::Engine) {
        let (mut state, engine) = AppState::headless(mode);
        state.theme_override = Some(mode);
        (
            Self {
                theme: Theme::for_mode(mode),
                state,
                smoke_test,
                frames: 0,
                smoke_done: false,
                applied: false,
            },
            engine,
        )
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Frames rendered so far (smoke-test counter).
    pub fn frames(&self) -> u32 {
        self.frames
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        ctx.set_theme(to_egui_theme(self.theme.mode));
        self.theme.apply(ctx);
        self.applied = true;
    }

    /// Re-resolves the mode (override, else OS, else dark) and re-applies the theme if it
    /// changed since the last frame.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let mode = resolve_theme_mode(self.state.theme_override, ctx.system_theme());
        self.state.mode = mode;
        if !self.applied || mode != self.theme.mode {
            self.theme = Theme::for_mode(mode);
            self.apply_theme(ctx);
        }
    }

    /// Per-frame logic (runs before the UI): theme sync, the computer-keyboard piano and the
    /// smoke-test counter. Public so tests can drive a frame without an `eframe::Frame`.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.sync_theme(ctx);
        self.state.handle_keyboard(ctx);

        if self.smoke_test {
            self.frames += 1;
            if self.frames >= SMOKE_TEST_FRAMES && !self.smoke_done {
                self.smoke_done = true;
                println!("smoke-test: ok");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }

    /// Draws the whole window into `ui` and applies a manual theme toggle, if any.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let before = self.state.mode;
        crate::ui::show(ui, &mut self.state, &self.theme);
        if self.state.commit_mode_change(before) {
            // Re-apply immediately so the very next frame is already in the new palette.
            self.theme = Theme::for_mode(self.state.mode);
            self.apply_theme(ui.ctx());
        }
    }
}

impl App for BeatSlopperApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        self.show(ui);
    }

    fn save(&mut self, storage: &mut dyn Storage) {
        let stored: Option<StoredThemeMode> = self.state.theme_override.map(Into::into);
        eframe::set_value(storage, THEME_OVERRIDE_KEY, &stored);
    }

    fn persist_egui_memory(&self) -> bool {
        // Only the explicit theme override is remembered (see module docs).
        false
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.theme.surface.to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_beats_os_theme() {
        assert_eq!(
            resolve_theme_mode(Some(ThemeMode::Light), Some(egui::Theme::Dark)),
            ThemeMode::Light
        );
        assert_eq!(
            resolve_theme_mode(Some(ThemeMode::Dark), Some(egui::Theme::Light)),
            ThemeMode::Dark
        );
    }

    #[test]
    fn os_theme_is_used_without_override() {
        assert_eq!(
            resolve_theme_mode(None, Some(egui::Theme::Light)),
            ThemeMode::Light
        );
        assert_eq!(
            resolve_theme_mode(None, Some(egui::Theme::Dark)),
            ThemeMode::Dark
        );
    }

    #[test]
    fn falls_back_to_dark_without_os_theme() {
        assert_eq!(resolve_theme_mode(None, None), ThemeMode::Dark);
        assert_eq!(
            resolve_theme_mode(Some(ThemeMode::Light), None),
            ThemeMode::Light
        );
    }

    #[test]
    fn stored_mode_round_trips() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let stored: StoredThemeMode = mode.into();
            assert_eq!(ThemeMode::from(stored), mode);
        }
    }

    #[test]
    fn toggle_records_override_and_reaches_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(state.theme_override, None);
        state.toggle_theme();
        assert_eq!(state.theme_override, Some(ThemeMode::Light));
        assert_eq!(state.mode, ThemeMode::Light);

        state.toggle_test_tone();
        assert!(state.test_tone);
        let mut out = vec![0.0f32; 512 * 2];
        engine.render(&mut out, 512);
        assert!(out.iter().any(|s| s.abs() > 0.01));

        state.toggle_test_tone();
        assert!(!state.test_tone);
        // The engine may fade the tone out; give it half a second, then expect silence.
        let mut out = vec![0.0f32; 4096 * 2];
        for _ in 0..6 {
            engine.render(&mut out, 4096);
        }
        assert!(out.iter().all(|s| s.abs() < 1e-4), "tone did not stop");
    }

    #[test]
    fn note_events_reach_the_engine_and_light_the_keys() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.apply_note_events(&[NoteEvent::On {
            note: 60,
            velocity: 0.8,
        }]);
        assert!(state.held_notes[60]);
        assert_eq!(state.note_on_count, 1);
        let mut out = vec![0.0f32; 2048 * 2];
        engine.render(&mut out, 2048);
        assert!(out.iter().any(|s| s.abs() > 0.001), "note-on made no sound");

        state.apply_note_events(&[NoteEvent::Off { note: 60 }]);
        assert!(!state.held_notes[60]);
        assert_eq!(state.note_off_count, 1);
        // Two seconds covers any sensible default release tail.
        let mut out = vec![0.0f32; 4096 * 2];
        for _ in 0..24 {
            engine.render(&mut out, 4096);
        }
        assert!(out.iter().all(|s| s.abs() < 1e-3), "note did not release");
    }

    #[test]
    fn all_notes_off_clears_everything() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.note_on(64, 1.0);
        state.note_on(200, 1.0); // clamped to 127, never panics
        assert!(state.held_notes[127]);
        state.all_notes_off();
        assert!(state.held_notes.iter().all(|h| !h));
        assert!(state.input.held.is_empty());
    }

    #[test]
    fn widget_mode_change_becomes_override() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(!state.commit_mode_change(ThemeMode::Dark));
        state.mode = ThemeMode::Light; // what the toggle widget does
        assert!(state.commit_mode_change(ThemeMode::Dark));
        assert_eq!(state.theme_override, Some(ThemeMode::Light));
    }
}
