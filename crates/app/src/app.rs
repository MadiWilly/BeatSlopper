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
//!
//! Parameters (M2): [`AppState::params`] mirrors the engine's parameter array. Every control
//! edits it through [`AppState::set_param`], which clamps, stores and sends
//! `Message::SetParam`; [`AppState::apply_preset`] resets to defaults, overlays a preset and
//! sends every value. After the audio stream starts, every parameter and the tempo are sent
//! once so the engine matches the UI. The chosen preset index is persisted next to the theme
//! override and re-applied on the next launch.

use eframe::{App, CreationContext, Frame, Storage};
use engine::{EngineHandle, Message, ParamId};
use neu::{Theme, ThemeMode};
use serde::{Deserialize, Serialize};

use crate::audio_io::AudioOutput;
use crate::input::{InputState, NoteEvent};
use crate::presets;

/// eframe storage key for the manual theme override.
pub const THEME_OVERRIDE_KEY: &str = "theme_override";

/// eframe storage key for the selected preset index.
pub const PRESET_INDEX_KEY: &str = "preset_index";

/// Tempo sent to the engine at startup (the M3 transport will own it).
pub const DEFAULT_BPM: f32 = 120.0;

/// Lowest / highest tempo the engine accepts (`Engine::set_bpm` clamps to the same range).
pub const MIN_BPM: f32 = 60.0;
pub const MAX_BPM: f32 = 200.0;

/// Every parameter at its [`ParamId::default_value`], indexed by [`ParamId::index`].
#[must_use]
pub fn default_params() -> [f32; ParamId::COUNT] {
    let mut params = [0.0; ParamId::COUNT];
    for id in ParamId::ALL {
        params[id.index()] = id.default_value();
    }
    params
}

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
    /// UI-side mirror of every engine parameter, indexed by [`ParamId::index`]. Always within
    /// [`ParamId::range`]; edit through [`AppState::set_param`] so the engine follows.
    pub params: [f32; ParamId::COUNT],
    /// Index into [`presets::ALL`] of the preset shown as selected.
    pub preset_index: usize,
    /// Tempo in BPM (M3's transport will edit it; M2 only sends the default for the delay).
    pub bpm: f32,
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
        (Self::from_parts(mode, handle, None), engine)
    }

    /// Fresh state (defaults everywhere) around an engine handle and an optional stream.
    fn from_parts(mode: ThemeMode, engine: EngineHandle, audio: Option<AudioOutput>) -> Self {
        Self {
            theme_override: None,
            mode,
            engine,
            test_tone: false,
            params: default_params(),
            preset_index: 0,
            bpm: DEFAULT_BPM,
            audio,
            audio_error: None,
            input: InputState::new(),
            held_notes: [false; 128],
            note_on_count: 0,
            note_off_count: 0,
        }
    }

    /// Tries to open the default audio device. On failure the app keeps running silently.
    /// With a stream running, every parameter and the tempo are sent so the engine matches
    /// the UI from the first buffer.
    pub fn with_audio(mode: ThemeMode) -> Self {
        match AudioOutput::start(engine::new_engine) {
            Ok((audio, handle)) => {
                let mut state = Self::from_parts(mode, handle, Some(audio));
                state.resend_all_params();
                state.set_bpm(DEFAULT_BPM);
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

    /// Current UI value of parameter `id` (always within [`ParamId::range`]).
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        self.params[id.index()]
    }

    /// Master gain, linear (`1.0` = unity); the same value as `param(MasterGain)`.
    #[must_use]
    pub fn master_gain(&self) -> f32 {
        self.param(ParamId::MasterGain)
    }

    /// `true` when the soft limiter is switched on.
    #[must_use]
    pub fn limiter_on(&self) -> bool {
        self.param(ParamId::LimiterOn) >= 0.5
    }

    /// Clamps `value` into the parameter's range, stores it and tells the engine. Returns the
    /// stored value.
    pub fn set_param(&mut self, id: ParamId, value: f32) -> f32 {
        let value = id.clamp(value);
        self.params[id.index()] = value;
        self.engine.send(Message::SetParam(id, value));
        value
    }

    /// Sends every parameter to the engine (startup, or after the UI state was rebuilt).
    pub fn resend_all_params(&mut self) {
        for id in ParamId::ALL {
            self.engine
                .send(Message::SetParam(*id, self.params[id.index()]));
        }
    }

    /// Stores the tempo (clamped to [`MIN_BPM`]`..=`[`MAX_BPM`]) and tells the engine.
    pub fn set_bpm(&mut self, bpm: f32) {
        let bpm = if bpm.is_finite() {
            bpm.clamp(MIN_BPM, MAX_BPM)
        } else {
            DEFAULT_BPM
        };
        self.bpm = bpm;
        self.engine.send(Message::SetBpm(bpm));
    }

    /// Selects preset `index` (clamped to the list): every parameter goes back to its default,
    /// the preset's values are applied on top and everything is sent to the engine.
    pub fn apply_preset(&mut self, index: usize) {
        let index = index.min(presets::ALL.len().saturating_sub(1));
        self.preset_index = index;
        self.params = default_params();
        if let Some(preset) = presets::ALL.get(index) {
            for (id, value) in preset.values {
                self.params[id.index()] = id.clamp(*value);
            }
        }
        self.resend_all_params();
    }

    /// Name of the selected preset.
    #[must_use]
    pub fn preset_name(&self) -> &'static str {
        presets::ALL
            .get(self.preset_index)
            .map_or("", |preset| preset.name)
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
        let preset_index: Option<usize> = cc
            .storage
            .and_then(|s| eframe::get_value::<usize>(s, PRESET_INDEX_KEY));

        let mut state = AppState::with_audio(mode);
        state.theme_override = theme_override;
        if let Some(index) = preset_index {
            // Re-apply the remembered preset so the sound matches the highlighted name.
            state.apply_preset(index);
        }

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
        eframe::set_value(storage, PRESET_INDEX_KEY, &self.state.preset_index);
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
        // The tone fades out and the FX chain (delay feedback, reverb) adds a tail; wait for
        // silence rather than assuming a tail length.
        assert!(
            renders_silent_within(&mut engine, 6.0, 1e-4),
            "tone did not stop"
        );
    }

    /// Renders up to `seconds` of audio in 4096-frame blocks; `true` as soon as one whole
    /// block stays below `threshold`.
    fn renders_silent_within(engine: &mut engine::Engine, seconds: f32, threshold: f32) -> bool {
        let mut out = vec![0.0f32; 4096 * 2];
        let blocks = (seconds * 48_000.0 / 4096.0).ceil() as usize;
        for _ in 0..blocks {
            engine.render(&mut out, 4096);
            if out.iter().all(|s| s.abs() < threshold) {
                return true;
            }
        }
        false
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
        // Release tail plus the FX chain's delay/reverb tail: wait for silence.
        assert!(
            renders_silent_within(&mut engine, 6.0, 1e-3),
            "note did not release"
        );
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
    fn fresh_state_holds_the_defaults() {
        let (state, _engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(state.params, default_params());
        assert_eq!(state.preset_index, 0);
        assert_eq!(state.bpm, DEFAULT_BPM);
        assert_eq!(state.master_gain(), ParamId::MasterGain.default_value());
        assert!(state.limiter_on());
        for id in ParamId::ALL {
            assert!(id.range().contains(&state.param(*id)), "{id:?}");
        }
    }

    #[test]
    fn set_param_clamps_stores_and_reaches_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(state.set_param(ParamId::FilterCutoff, 1.0e9), 20_000.0);
        assert_eq!(state.set_param(ParamId::Osc1Detune, -999.0), -50.0);
        assert_eq!(state.set_param(ParamId::MasterGain, 0.5), 0.5);
        assert_eq!(
            state.set_param(ParamId::AmpSustain, f32::NAN),
            ParamId::AmpSustain.default_value()
        );
        assert_eq!(state.set_param(ParamId::LimiterOn, 0.0), 0.0);
        assert_eq!(state.param(ParamId::FilterCutoff), 20_000.0);
        assert_eq!(state.param(ParamId::Osc1Detune), -50.0);
        assert_eq!(state.master_gain(), 0.5);
        assert!(!state.limiter_on());

        engine.process_messages();
        assert_eq!(engine.param(ParamId::FilterCutoff), 20_000.0);
        assert_eq!(engine.param(ParamId::Osc1Detune), -50.0);
        assert_eq!(engine.param(ParamId::MasterGain), 0.5);
        assert_eq!(
            engine.param(ParamId::AmpSustain),
            ParamId::AmpSustain.default_value()
        );
        assert_eq!(engine.param(ParamId::LimiterOn), 0.0);
        // Untouched parameters keep their defaults on both sides.
        assert_eq!(
            engine.param(ParamId::ReverbMix),
            state.param(ParamId::ReverbMix)
        );
    }

    #[test]
    fn every_preset_sends_all_params_and_stays_in_range() {
        assert_eq!(presets::ALL.len(), 8);
        for (i, preset) in presets::ALL.iter().enumerate() {
            let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
            // Dirty a value first so we can see the preset reset it.
            state.set_param(ParamId::Glide, 0.9);
            let free_before = state.engine.free_slots();
            state.apply_preset(i);
            assert_eq!(state.preset_index, i);
            assert_eq!(state.preset_name(), preset.name);
            assert_eq!(
                free_before - state.engine.free_slots(),
                ParamId::COUNT,
                "preset {} must send one SetParam per id",
                preset.name
            );

            engine.process_messages();
            for id in ParamId::ALL {
                let value = state.param(*id);
                assert!(
                    id.range().contains(&value),
                    "{}: {id:?} = {value} out of range",
                    preset.name
                );
                assert_eq!(engine.param(*id), value, "{}: {id:?}", preset.name);
            }
            for (id, value) in preset.values {
                assert_eq!(
                    state.param(*id),
                    id.clamp(*value),
                    "{}: {id:?}",
                    preset.name
                );
            }
            if !preset.values.iter().any(|(id, _)| *id == ParamId::Glide) {
                assert_eq!(state.param(ParamId::Glide), ParamId::Glide.default_value());
            }
        }
    }

    #[test]
    fn preset_index_is_clamped_to_the_list() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.apply_preset(usize::MAX);
        assert_eq!(state.preset_index, presets::ALL.len() - 1);
        assert_eq!(
            state.preset_name(),
            presets::ALL[presets::ALL.len() - 1].name
        );
    }

    #[test]
    fn bpm_is_clamped_and_sent() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.set_bpm(1000.0);
        assert_eq!(state.bpm, MAX_BPM);
        state.set_bpm(f32::NAN);
        assert_eq!(state.bpm, DEFAULT_BPM);
        state.set_bpm(90.0);
        engine.process_messages();
        assert_eq!(engine.bpm(), 90.0);
    }

    #[test]
    fn resend_all_params_sends_one_message_per_id() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.params[ParamId::DriveMix.index()] = 0.9;
        let free_before = state.engine.free_slots();
        state.resend_all_params();
        assert_eq!(free_before - state.engine.free_slots(), ParamId::COUNT);
        engine.process_messages();
        assert_eq!(engine.param(ParamId::DriveMix), 0.9);
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
