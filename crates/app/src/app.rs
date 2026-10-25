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
//!
//! Sequencer (M3/M5): [`AppState::bank`] is the UI's source of truth for the eight patterns,
//! the song chain, the play mode and the selected (edit) pattern; [`AppState::pattern`] is
//! the edit pattern. Every edit goes through a `set_*` / `toggle_*` / `clear_*` method that
//! updates the mirror and sends the matching `Message`. Step indices are absolute within the
//! pattern (`bar * 16 + column`, see [`AppState::shown_bar`]). The playhead is read from
//! `EngineHandle::position()` every frame while the transport runs. Note entry: a selected
//! bass step takes the next note-on; with the transport playing and record armed, note-ons are
//! quantised to the nearest step ([`quantise_step`]) and written to the bass lane. Save / load
//! / export live in [`crate::project`].
//!
//! Patterns and song mode (M5): the whole bank is resent with [`AppState::send_bank`]
//! (startup, project load, undo). It is built by [`bank_messages`]: per pattern a
//! `SetEditPattern`, a `ClearPattern`, only the *non-empty* steps (within the pattern's
//! [`Pattern::used_steps`], temporarily lengthening the pattern so hidden steps survive), the
//! length and the row volumes / tunes; then the chain, the play mode (only when it differs
//! from what the engine already has, so a resend never restarts a running song) and
//! `SelectPattern(current)`. A dense bank is more than the 1024-slot queue holds, so every
//! message goes through [`AppState::send`]: straight onto the queue while it has room, else
//! into [`AppState::outbox`], which [`AppState::flush_outbox`] drains on every frame, in order.
//! Selecting a pattern tab sends `SelectPattern` (the engine queues the switch to the next bar
//! while playing; [`Position::queued`] drives the blinking letter); copy / paste / clear and
//! the chain edits are undo steps like any other edit.
//!
//! MIDI (M4): the `// ---- MIDI ----` section. The MIDI thread ([`crate::midi`]) sends notes to
//! the engine itself over the engine's second queue ([`MidiHandle`]) and mirrors them to the
//! UI through `midi_rx`; [`AppState::poll_midi`] drains that every tick and applies them with
//! [`NoteSource::Midi`], which lights keys and feeds note entry / overdub exactly like the
//! computer keyboard but does *not* send `NoteOn` / `NoteOff` again.
//!
//! Polish (M4): every user edit goes through [`AppState::begin_edit`] (discrete edits) or
//! [`AppState::begin_continuous_edit`] (knob drags, coalesced per drag) before it mutates the
//! mirrors, which pushes a [`Snapshot`] onto [`AppState::undo`] and sets [`AppState::dirty`].
//! [`AppState::undo`] / [`AppState::redo`] restore a snapshot and `resend_all`. Shortcuts
//! ([`crate::shortcuts`]) are read in [`AppState::take_shortcuts`] and dispatched by
//! [`BeatSlopperApp::tick`]; the ones that open file dialogs are refused unless
//! [`AppState::dialogs_enabled`] (never under `--smoke-test` or in tests). New / Open while
//! dirty park a [`PendingAction`] that the project panel turns into a DISCARD / KEEP confirm.
//! The window title follows [`crate::window::window_title`] and the first frame fixes an
//! undersized window ([`crate::window`]).
//!
//! Mixer (M6): [`AppState::mixer`] mirrors the engine's six strips; [`AppState::set_mixer`]
//! clamps with `ChannelParam::clamp`, stores, sends `Message::SetMixer` and records the undo
//! step (discrete for mute / solo, coalesced per drag for level, pan and the sends). The
//! mixer is part of the [`Snapshot`], the project file (v4) and `resend_all`
//! ([`mixer_messages`]: 36 values). The engine's block peaks are read every frame
//! ([`AppState::update_meters`]) and shown with a [`METER_DECAY_DB_PER_S`] fall
//! ([`decay_meter`]); the UI keeps repainting while a meter is still falling. The top row's
//! right slot shows the FX rack or the MIXER ([`AppState::right_panel`], persisted).
//!
//! Piano roll (M7): the synth part of a pattern is its note list (`Pattern::notes`, chords and
//! held notes); the grid's bass lane is the derived one-note-per-step view of it. The
//! SEQUENCER panel shows the step GRID or the piano ROLL ([`AppState::lane_view`], persisted
//! with the roll's bottom pitch [`AppState::roll_low_pitch`]). Roll edits go through
//! [`AppState::add_note`] / [`AppState::set_note`] / [`AppState::remove_note`] /
//! [`AppState::clear_notes`]: each is one undo step, marks the project dirty, mutates the
//! mirror through the `Pattern` note API (which re-sorts and rebuilds the lane) and replays
//! the edit pattern's whole note list to the engine ([`note_messages`]: `ClearNotes` then one
//! `SetNote` per note; a re-sort moves indices, so replaying is the simplest way to keep both
//! sides identical, and the outbox absorbs the burst). Grid edits keep using `SetSynthStep`
//! (a length-1 note upsert on both sides), as do click-then-key note entry and record-arm
//! overdub. [`pattern_messages`] replays notes the same way, so a resend (startup, load,
//! undo) carries chords and long notes.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use eframe::{App, CreationContext, Frame, Storage};
use engine::pattern::{DRUM_TUNE_RANGE, DRUM_VOLUME_RANGE, MAX_REPEATS, pattern_letter};
use engine::{
    Bank, ChainEntry, Channel, ChannelParam, DrumRow, DrumStep, EngineHandle, MAX_CHAIN, MAX_NOTES,
    Message, Meters, MidiHandle, MixerState, Note, PATTERN_COUNT, ParamId, Pattern, PatternLength,
    PlayMode, Position, STEPS,
};
use neu::{Theme, ThemeMode};
use serde::{Deserialize, Serialize};

use crate::audio_io::AudioOutput;
use crate::input::{InputState, NoteEvent};
use crate::midi::{MidiConnection, MidiInput, UI_QUEUE_CAPACITY};
use crate::presets;
use crate::project::ExportJob;
use crate::shortcuts::{self, Shortcut};
use crate::undo::{Snapshot, UndoStack};
use crate::window;

/// eframe storage key for the manual theme override.
pub const THEME_OVERRIDE_KEY: &str = "theme_override";

/// eframe storage key for the selected preset index.
pub const PRESET_INDEX_KEY: &str = "preset_index";

/// eframe storage key for the last project path.
pub const PROJECT_PATH_KEY: &str = "project_path";

/// eframe storage key for the export repeats choice.
pub const EXPORT_REPEATS_KEY: &str = "export_repeats";

/// eframe storage key for the chosen MIDI input port name.
pub const MIDI_PORT_KEY: &str = "midi_port";

/// eframe storage key for the right-hand top panel choice (FX rack or MIXER).
pub const RIGHT_PANEL_KEY: &str = "right_panel";

/// eframe storage key for the sequencer's lane view (step GRID or piano ROLL, M7).
pub const LANE_VIEW_KEY: &str = "lane_view";

/// eframe storage key for the piano roll's bottom visible pitch (M7).
pub const ROLL_LOW_PITCH_KEY: &str = "roll_low_pitch";

/// Bottom pitch of a fresh piano roll (C2).
pub const DEFAULT_ROLL_LOW_PITCH: u8 = 36;

/// eframe storage key for how many rows taller than default the piano roll is.
pub const ROLL_EXTRA_ROWS_KEY: &str = "roll_extra_rows";

/// Most extra rows the roll can be dragged to (the default roll shows 17 rows, so the whole
/// MIDI range fits at the maximum).
pub const MAX_ROLL_EXTRA_ROWS: usize = 112;

/// eframe storage key for whether the piano roll shows the black-key rows.
pub const ROLL_BLACK_KEYS_KEY: &str = "roll_black_keys";

/// How fast a shown meter falls after its peak, in dB per second (M6).
pub const METER_DECAY_DB_PER_S: f32 = 20.0;

/// Shown meter levels at or below this are treated as silent (no repaint needed).
pub const METER_SILENT: f32 = 0.001;

/// How often the MIDI port list is re-read while the selector is showing.
pub const MIDI_PORTS_REFRESH: Duration = Duration::from_secs(2);

/// Tempo of a fresh project.
pub const DEFAULT_BPM: f32 = 120.0;

/// Lowest / highest tempo the engine accepts (`Engine::set_bpm` clamps to the same range).
pub const MIN_BPM: f32 = 60.0;
pub const MAX_BPM: f32 = 200.0;

/// Swing range offered by the SWING knob (fraction of a step odd steps are delayed by).
pub const MIN_SWING: f32 = 0.0;
pub const MAX_SWING: f32 = 0.75;

/// Repeat counts offered by the EXPORT WAV selector.
pub const EXPORT_REPEATS: [u32; 4] = [1, 2, 4, 8];

/// How long a status line stays visible.
pub const STATUS_DURATION: Duration = Duration::from_secs(5);

/// A recorded note-on at or above this velocity becomes an accented step.
pub const ACCENT_VELOCITY: f32 = 0.95;

/// Changes to the same continuous control (knob, dial, slider) closer together than this join
/// one undo step; the first change after a longer pause starts a new one.
pub const CONTINUOUS_EDIT_GAP: Duration = Duration::from_millis(300);

/// A New / Open that waits for the user to confirm discarding unsaved changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAction {
    New,
    Open,
}

/// Which panel the right-hand column shows (M5): the PROJECT panel or the SONG CHAIN panel.
/// The column has room for one at a time; switching the play mode switches the tab too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RightTab {
    #[default]
    Project,
    Chain,
}

/// Which panel the top row's right-hand slot shows (M6): the FX rack or the MIXER. Both heads
/// carry the `FX | Mixer` tab; the choice is persisted under [`RIGHT_PANEL_KEY`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RightPanel {
    #[default]
    Fx,
    Mixer,
}

/// What the SEQUENCER panel shows under its head (M7): the step GRID (drum rows and the bass
/// lane) or the piano ROLL of the edit pattern's notes. Persisted under [`LANE_VIEW_KEY`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaneView {
    #[default]
    Grid,
    Roll,
}

/// The decayed level a meter shows one frame on: a new peak replaces the shown level at once,
/// otherwise the shown level falls by [`METER_DECAY_DB_PER_S`] (`shown * 10^(-20 * dt / 20)`
/// over `dt` seconds). Non-finite or negative input reads as silence.
#[must_use]
pub fn decay_meter(shown: f32, new: f32, dt_seconds: f32) -> f32 {
    let shown = if shown.is_finite() {
        shown.max(0.0)
    } else {
        0.0
    };
    let new = if new.is_finite() { new.max(0.0) } else { 0.0 };
    let dt = if dt_seconds.is_finite() {
        dt_seconds.max(0.0)
    } else {
        0.0
    };
    let fallen = shown * 10f32.powf(-METER_DECAY_DB_PER_S * dt / 20.0);
    let level = new.max(fallen);
    if level <= METER_SILENT * 0.1 {
        0.0
    } else {
        level
    }
}

/// Applies [`decay_meter`] to every channel and master value: `shown` becomes the levels to
/// draw given the engine's latest block peaks `new` and the `dt_seconds` since the last call.
pub fn decay_meters(shown: &mut Meters, new: &Meters, dt_seconds: f32) {
    for (s, n) in shown.channel.iter_mut().zip(new.channel.iter()) {
        *s = decay_meter(*s, *n, dt_seconds);
    }
    for (s, n) in shown.master.iter_mut().zip(new.master.iter()) {
        *s = decay_meter(*s, *n, dt_seconds);
    }
}

/// `true` when any shown meter is still above [`METER_SILENT`] (so the UI keeps repainting).
#[must_use]
pub fn meters_active(shown: &Meters) -> bool {
    shown
        .channel
        .iter()
        .chain(shown.master.iter())
        .any(|v| *v > METER_SILENT)
}

/// Blink period of the queued-pattern letter: visible for half of it (2 Hz toggling).
pub const QUEUED_BLINK_SECONDS: f64 = 0.5;

/// Whether the queued-pattern letter is visible at `time` (seconds, `egui::InputState::time`).
#[must_use]
pub fn queued_blink_visible(time: f64) -> bool {
    (time / (QUEUED_BLINK_SECONDS * 0.5)).floor() as i64 % 2 == 0
}

/// Identifies a continuous control for undo coalescing (see [`AppState::begin_continuous_edit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey {
    Param(ParamId),
    Swing,
    DrumVolume(DrumRow),
    DrumTune(DrumRow),
    /// A mixer strip's continuous value (level, pan, a send); mute / solo are discrete.
    Mixer(Channel, ChannelParam),
}

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

/// The step a note played at `position` is recorded to, as an absolute index into a pattern of
/// `length` steps: the current step (`bar * 16 + step`) while less than half of it has
/// elapsed, otherwise the next one, wrapping at `length` (so 15 -> 0 in a one-bar pattern).
#[must_use]
pub fn quantise_step(position: Position, length: usize) -> usize {
    let length = length.max(1);
    let step = position.pattern_step();
    if position.frac >= 0.5 {
        (step + 1) % length
    } else {
        step % length
    }
}

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
    /// Tempo in BPM (`MIN_BPM..=MAX_BPM`).
    pub bpm: f32,
    /// Swing amount (`MIN_SWING..=MAX_SWING`).
    pub swing: f32,
    /// UI-side mirror of the engine's bank: patterns A..H, the chain, the play mode and the
    /// selected (edit) pattern. Source of truth for the grid and the SONG CHAIN panel.
    pub bank: Bank,
    /// The pattern COPY took; PASTE writes it into the edit pattern.
    pub clipboard: Option<Pattern>,
    /// The 16-step bar of the edit pattern the grid shows (`0..pattern().bars()`).
    pub shown_bar: usize,
    /// While playing the edit pattern, the grid follows the playing bar.
    pub follow: bool,
    /// Which panel the right-hand column shows.
    pub right_tab: RightTab,
    /// Which panel the top row's right slot shows: the FX rack or the MIXER (persisted).
    pub right_panel: RightPanel,
    /// What the SEQUENCER panel shows: the step GRID or the piano ROLL (M7, persisted).
    pub lane_view: LaneView,
    /// MIDI pitch of the piano roll's bottom row (M7, persisted; the wheel scrolls it).
    pub roll_low_pitch: u8,
    /// Index into [`Pattern::notes`] of the note selected on the piano roll, if any.
    pub roll_selected: Option<usize>,
    /// How many rows taller than its default the piano roll is (persisted; the handle under
    /// the roll drags it, `0..=MAX_ROLL_EXTRA_ROWS`).
    pub roll_extra_rows: usize,
    /// Index into [`crate::chords::CHORDS`] of the chord the next roll draw places, if any
    /// (the CHORDS column beside the roll arms it; not an edit, not persisted).
    pub armed_chord: Option<usize>,
    /// Every note selected on the piano roll (indices into [`Pattern::notes`], the primary
    /// [`Self::roll_selected`] among them). Cleared wherever the primary is.
    pub roll_selection: Vec<usize>,
    /// Whether the piano roll shows the black-key rows (persisted; the BLACK KEYS pill).
    pub roll_black_keys: bool,
    /// Chain entry selected in the SONG CHAIN panel (`None` when the chain is empty).
    pub song_selected: Option<usize>,
    /// UI-side mirror of the engine's mixer (M6): six strips of level, pan, mute, solo and
    /// sends. Edit through [`AppState::set_mixer`] so the engine follows.
    pub mixer: MixerState,
    /// Block peaks as last read from the engine (`EngineHandle::meters`, every frame).
    pub meters: Meters,
    /// The decayed levels the mixer draws (see [`decay_meters`]).
    pub shown_meters: Meters,
    /// When the shown meters were last updated (for the decay's `dt`).
    pub meters_updated: Option<Instant>,
    /// Messages that did not fit the engine queue yet, sent in order by
    /// [`AppState::flush_outbox`] every frame.
    pub outbox: VecDeque<Message>,
    /// The play mode last sent to the engine (so a bank resend only sends `SetPlayMode` when
    /// it changed: setting song mode while playing restarts the chain).
    pub engine_mode: PlayMode,
    /// Whether the UI asked the transport to run (the button state; `playhead.playing` is what
    /// the audio thread reports).
    pub playing: bool,
    /// Record arm: while playing, note-ons are written to the bass lane.
    pub armed: bool,
    /// Bass-lane step selected for note entry (absolute index into the edit pattern).
    pub selected_step: Option<usize>,
    /// Playhead as last read from the engine (updated every frame by `tick`).
    pub playhead: Position,
    /// Path of the current project file (set by Open / Save As; persisted).
    pub project_path: Option<PathBuf>,
    /// Bars written by EXPORT WAV (one of [`EXPORT_REPEATS`]; persisted).
    pub export_repeats: u32,
    /// A short status line and when it was set (hidden after [`STATUS_DURATION`]).
    pub status: Option<(String, Instant)>,
    /// A running WAV export, if any (polled by `tick`).
    pub export_job: Option<ExportJob>,
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

    // ---- Polish (M4) ----
    /// Undo / redo history of [`Snapshot`]s (the state *before* each edit).
    pub undo: UndoStack,
    /// `true` while the project differs from what was last saved / opened / created.
    pub dirty: bool,
    /// A New / Open waiting for the DISCARD / KEEP confirm in the project panel.
    pub pending_action: Option<PendingAction>,
    /// Whether file dialogs may open (false under `--smoke-test` and in headless tests).
    pub dialogs_enabled: bool,
    /// The last continuous control edited and when (undo coalescing of knob drags).
    pub last_continuous_edit: Option<(EditKey, Instant)>,

    // ---- MIDI ----
    /// `false` disables MIDI entirely (`--smoke-test`): no port listing, no connecting.
    pub midi_enabled: bool,
    /// Input port names as last listed (see [`AppState::refresh_midi_ports`]).
    pub midi_ports: Vec<String>,
    /// When `midi_ports` was last refreshed.
    pub midi_ports_refreshed: Option<Instant>,
    /// The port the user chose (persisted under [`MIDI_PORT_KEY`]); reconnected when present.
    pub midi_port: Option<String>,
    /// The open connection, if any.
    pub midi_conn: Option<MidiConnection>,
    /// Note events mirrored from the MIDI thread, drained by [`AppState::poll_midi`].
    pub midi_rx: Option<Receiver<NoteEvent>>,
    /// The engine's MIDI queue producer while no connection owns it.
    pub midi_handle: Option<MidiHandle>,
    /// Called from the MIDI thread after each message so the UI repaints (the egui context).
    pub midi_wake: Option<egui::Context>,
    /// A port name that failed to open; not retried until the user picks a port again.
    pub midi_failed_port: Option<String>,
    /// MIDI note-ons applied through [`AppState::poll_midi`] (diagnostics).
    pub midi_note_on_count: u64,
    /// MIDI note-offs applied through [`AppState::poll_midi`] (diagnostics).
    pub midi_note_off_count: u64,
}

/// Where a note event came from, which decides whether the UI path tells the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteSource {
    /// Computer keyboard or on-screen piano: the UI sends `NoteOn` / `NoteOff` to the engine.
    Local,
    /// MIDI input: the MIDI thread already sent the message over its own queue, so the UI only
    /// updates the key lights, note entry and overdub.
    Midi,
}

impl AppState {
    /// State without an audio device, for tests and for the no-device fallback. Returns the
    /// `Engine` too so tests can render offline and check the messages arrived.
    pub fn headless(mode: ThemeMode) -> (Self, engine::Engine) {
        let (handle, engine) = engine::new_engine(48_000.0);
        let mut state = Self::from_parts(mode, handle, None);
        // No test may ever block on an OS file dialog.
        state.dialogs_enabled = false;
        (state, engine)
    }

    /// Fresh state (defaults everywhere, the demo pattern) around an engine handle and an
    /// optional stream.
    fn from_parts(mode: ThemeMode, mut engine: EngineHandle, audio: Option<AudioOutput>) -> Self {
        // The MIDI queue producer is taken once, right here, so a later connection can own it.
        let midi_handle = engine.take_midi_handle();
        Self {
            theme_override: None,
            mode,
            engine,
            test_tone: false,
            params: default_params(),
            preset_index: 0,
            bpm: DEFAULT_BPM,
            swing: 0.0,
            bank: Bank::default(),
            clipboard: None,
            shown_bar: 0,
            follow: true,
            right_tab: RightTab::Project,
            right_panel: RightPanel::Fx,
            lane_view: LaneView::Grid,
            roll_low_pitch: DEFAULT_ROLL_LOW_PITCH,
            roll_selected: None,
            roll_extra_rows: 0,
            armed_chord: None,
            roll_selection: Vec::new(),
            roll_black_keys: true,
            song_selected: Some(0),
            mixer: MixerState::default(),
            meters: Meters::SILENT,
            shown_meters: Meters::SILENT,
            meters_updated: None,
            outbox: VecDeque::new(),
            engine_mode: PlayMode::Pattern,
            playing: false,
            armed: false,
            selected_step: None,
            playhead: Position::stopped(0),
            project_path: None,
            export_repeats: EXPORT_REPEATS[0],
            status: None,
            export_job: None,
            audio,
            audio_error: None,
            input: InputState::new(),
            held_notes: [false; 128],
            note_on_count: 0,
            note_off_count: 0,
            undo: UndoStack::new(),
            dirty: false,
            pending_action: None,
            dialogs_enabled: true,
            last_continuous_edit: None,
            midi_enabled: true,
            midi_ports: Vec::new(),
            midi_ports_refreshed: None,
            midi_port: None,
            midi_conn: None,
            midi_rx: None,
            midi_handle,
            midi_wake: None,
            midi_failed_port: None,
            midi_note_on_count: 0,
            midi_note_off_count: 0,
        }
    }

    /// Tries to open the default audio device. On failure the app keeps running silently.
    /// With a stream running, every parameter, the tempo, the swing and the whole bank are
    /// sent so the engine matches the UI from the first buffer.
    pub fn with_audio(mode: ThemeMode) -> Self {
        match AudioOutput::start(engine::new_engine) {
            Ok((audio, handle)) => {
                let mut state = Self::from_parts(mode, handle, Some(audio));
                state.resend_all();
                state
            }
            Err(err) => {
                eprintln!("beatslopper: audio disabled: {err}");
                let (mut state, _engine) = Self::headless(mode);
                state.audio_error = Some(err);
                state.dialogs_enabled = true;
                state
            }
        }
    }

    /// Sample rate of the running stream, or 48 kHz without one (offline export).
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.audio
            .as_ref()
            .map_or(48_000.0, AudioOutput::sample_rate)
    }

    /// Sends `msg` to the engine, keeping order: straight onto the queue while the outbox is
    /// empty and the queue has room, otherwise onto the back of [`AppState::outbox`] for
    /// [`AppState::flush_outbox`]. Nothing is ever dropped.
    pub fn send(&mut self, msg: Message) {
        if self.outbox.is_empty() && self.engine.send(msg) {
            return;
        }
        self.outbox.push_back(msg);
    }

    /// Moves as many queued-up messages as fit from the outbox onto the engine queue (called
    /// every frame). Returns how many were sent.
    pub fn flush_outbox(&mut self) -> usize {
        let mut sent = 0;
        while let Some(&msg) = self.outbox.front() {
            if !self.engine.send(msg) {
                break;
            }
            self.outbox.pop_front();
            sent += 1;
        }
        sent
    }

    /// Flips the test tone and tells the engine.
    pub fn toggle_test_tone(&mut self) {
        self.set_test_tone(!self.test_tone);
    }

    /// Sets the test tone and tells the engine.
    pub fn set_test_tone(&mut self, on: bool) {
        self.test_tone = on;
        self.send(Message::TestTone(on));
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
    /// stored value. A real change is an undo step, coalesced per drag
    /// ([`AppState::begin_continuous_edit`]); [`AppState::edit_param`] adds the drag-start hint.
    pub fn set_param(&mut self, id: ParamId, value: f32) -> f32 {
        let value = id.clamp(value);
        if value != self.params[id.index()] {
            self.begin_continuous_edit(EditKey::Param(id), false);
        }
        self.params[id.index()] = value;
        self.send(Message::SetParam(id, value));
        value
    }

    /// [`AppState::set_param`] from a control: `drag_started` (egui's `Response::drag_started`,
    /// or `true` for a click on a discrete control) always starts a new undo step.
    pub fn edit_param(&mut self, id: ParamId, value: f32, drag_started: bool) -> f32 {
        if drag_started && id.clamp(value) != self.params[id.index()] {
            self.begin_continuous_edit(EditKey::Param(id), true);
        }
        self.set_param(id, value)
    }

    /// Sends every parameter to the engine (startup, or after the UI state was rebuilt).
    pub fn resend_all_params(&mut self) {
        for id in ParamId::ALL {
            self.send(Message::SetParam(*id, self.params[id.index()]));
        }
    }

    /// Sends everything the engine mirrors: parameters, tempo, swing, the mixer and the whole
    /// bank (startup, project load, undo).
    pub fn resend_all(&mut self) {
        self.resend_all_params();
        self.send(Message::SetBpm(self.bpm));
        self.send(Message::SetSwing(self.swing));
        self.resend_mixer();
        self.send_bank();
    }

    // -----------------------------------------------------------------------------------------
    // Mixer (M6): the strip mirror, `SetMixer`, and the decaying meters.

    /// The strip of `channel` (level, pan, mute, solo, sends).
    #[must_use]
    pub fn strip(&self, channel: Channel) -> &engine::Strip {
        self.mixer.strip(channel)
    }

    /// Clamps `value` with [`ChannelParam::clamp`] (mute / solo snap to 0 / 1), stores it in
    /// the mixer mirror and sends `Message::SetMixer`. Returns the stored value. A real change
    /// is an undo step: discrete for mute / solo, coalesced per drag for level, pan and the
    /// sends ([`AppState::edit_mixer`] adds the drag-start hint).
    pub fn set_mixer(&mut self, channel: Channel, param: ChannelParam, value: f32) -> f32 {
        let value = param.clamp(channel, value);
        if value != self.mixer.get(channel, param) {
            if param.is_toggle() {
                self.begin_edit();
            } else {
                self.begin_continuous_edit(EditKey::Mixer(channel, param), false);
            }
        }
        let stored = self.mixer.set(channel, param, value);
        self.send(Message::SetMixer {
            channel,
            param,
            value: stored,
        });
        stored
    }

    /// [`AppState::set_mixer`] from a control: `drag_started` always starts a new undo step
    /// for a continuous value (toggles are discrete anyway).
    pub fn edit_mixer(
        &mut self,
        channel: Channel,
        param: ChannelParam,
        value: f32,
        drag_started: bool,
    ) -> f32 {
        if drag_started
            && !param.is_toggle()
            && param.clamp(channel, value) != self.mixer.get(channel, param)
        {
            self.begin_continuous_edit(EditKey::Mixer(channel, param), true);
        }
        self.set_mixer(channel, param, value)
    }

    /// Flips `channel`'s mute (one undo step).
    pub fn toggle_mute(&mut self, channel: Channel) {
        let on = self.mixer.strip(channel).mute;
        self.set_mixer(channel, ChannelParam::Mute, if on { 0.0 } else { 1.0 });
    }

    /// Flips `channel`'s solo (one undo step).
    pub fn toggle_solo(&mut self, channel: Channel) {
        let on = self.mixer.strip(channel).solo;
        self.set_mixer(channel, ChannelParam::Solo, if on { 0.0 } else { 1.0 });
    }

    /// Sends every mixer value ([`mixer_messages`]: 36 `SetMixer`s) so the engine's mixer
    /// matches the mirror.
    pub fn resend_mixer(&mut self) {
        for msg in mixer_messages(&self.mixer) {
            self.send(msg);
        }
    }

    /// Reads the engine's latest block peaks and decays the shown meters towards them
    /// ([`decay_meters`] with the wall-clock time since the last call). Called every frame.
    pub fn update_meters(&mut self) {
        let now = Instant::now();
        let dt = self
            .meters_updated
            .map_or(0.0, |at| now.duration_since(at).as_secs_f32());
        self.meters_updated = Some(now);
        let new = self.engine.meters();
        self.update_meters_with(new, dt);
    }

    /// The decay step with explicit inputs (tests): `new` replaces the raw meters and the shown
    /// meters fall by [`METER_DECAY_DB_PER_S`] over `dt_seconds` unless `new` is louder.
    pub fn update_meters_with(&mut self, new: Meters, dt_seconds: f32) {
        self.meters = new;
        decay_meters(&mut self.shown_meters, &new, dt_seconds);
    }

    /// `true` while a shown meter is still falling (above [`METER_SILENT`]).
    #[must_use]
    pub fn meters_active(&self) -> bool {
        meters_active(&self.shown_meters)
    }

    /// Stores the tempo (clamped to [`MIN_BPM`]`..=`[`MAX_BPM`]) and tells the engine. A change
    /// is one undo step.
    pub fn set_bpm(&mut self, bpm: f32) {
        let bpm = if bpm.is_finite() {
            bpm.clamp(MIN_BPM, MAX_BPM)
        } else {
            DEFAULT_BPM
        };
        if bpm != self.bpm {
            self.begin_edit();
        }
        self.bpm = bpm;
        self.send(Message::SetBpm(bpm));
    }

    /// Stores the swing (clamped to [`MIN_SWING`]`..=`[`MAX_SWING`]) and tells the engine. A
    /// change is an undo step coalesced per knob drag.
    pub fn set_swing(&mut self, swing: f32) {
        let swing = if swing.is_finite() {
            swing.clamp(MIN_SWING, MAX_SWING)
        } else {
            0.0
        };
        if swing != self.swing {
            self.begin_continuous_edit(EditKey::Swing, false);
        }
        self.swing = swing;
        self.send(Message::SetSwing(swing));
    }

    /// Selects preset `index` (clamped to the list): every parameter goes back to its default,
    /// the preset's values are applied on top and everything is sent to the engine. One undo
    /// step.
    pub fn apply_preset(&mut self, index: usize) {
        let index = index.min(presets::ALL.len().saturating_sub(1));
        self.begin_edit();
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

    // -----------------------------------------------------------------------------------------
    // Transport.

    /// Starts the loop from step 0.
    pub fn play(&mut self) {
        self.playing = true;
        self.send(Message::Play);
    }

    /// Stops the loop (the engine releases the sequencer's notes and rewinds).
    pub fn stop(&mut self) {
        self.playing = false;
        self.send(Message::Stop);
    }

    /// Play / Stop from one button.
    pub fn toggle_play(&mut self) {
        if self.playing {
            self.stop();
        } else {
            self.play();
        }
    }

    /// Flips the record arm (overdub into the bass lane while playing).
    pub fn toggle_arm(&mut self) {
        self.armed = !self.armed;
    }

    /// `true` while note-ons are being recorded into the bass lane.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.playing && self.armed
    }

    /// Reads the playhead the audio thread last published and, while following, moves the
    /// shown bar along with it when the edit pattern is the one playing.
    pub fn update_playhead(&mut self) {
        self.playhead = self.engine.position();
        if self.follow && self.playhead.playing && self.playhead.pattern == self.bank.current {
            self.shown_bar = usize::from(self.playhead.bar).min(self.pattern().bars() - 1);
        }
    }

    /// Switches between pattern and song mode: the bank mirror, the engine (`SetPlayMode`) and
    /// the right-hand column's tab (SONG CHAIN in song mode, PROJECT in pattern mode). Not an
    /// undo step, like the transport.
    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.bank.mode = mode;
        self.engine_mode = mode;
        self.send(Message::SetPlayMode(mode));
        self.right_tab = match mode {
            PlayMode::Song => RightTab::Chain,
            PlayMode::Pattern => RightTab::Project,
        };
    }

    /// The transport's position read-out: `"A · bar 2/4"` in pattern mode, `"SONG 3/5 · B"`
    /// in song mode (chain entry, chain length, playing pattern).
    #[must_use]
    pub fn position_text(&self) -> String {
        let pos = self.playhead;
        let letter = pattern_letter(pos.pattern);
        match self.bank.mode {
            PlayMode::Song => format!(
                "SONG {}/{} · {}",
                usize::from(pos.song_index) + 1,
                usize::from(self.bank.chain_len).max(1),
                letter
            ),
            PlayMode::Pattern => format!(
                "{letter} · bar {}/{}",
                usize::from(pos.bar) + 1,
                self.bank.pattern(pos.pattern).bars()
            ),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Patterns (M5): the edit pattern, tabs, length, copy / paste / clear.

    /// The edit pattern (the bank's `current`): what the grid shows and the step edits change.
    #[must_use]
    pub fn pattern(&self) -> &Pattern {
        self.bank.current_pattern()
    }

    /// The edit pattern, mutably. Prefer the `set_*` methods, which also tell the engine.
    pub fn pattern_mut(&mut self) -> &mut Pattern {
        self.bank.current_pattern_mut()
    }

    /// Index of the edit pattern (`0..PATTERN_COUNT`).
    #[must_use]
    pub fn current(&self) -> u8 {
        self.bank.current
    }

    /// Letter of the edit pattern (`A`..`H`).
    #[must_use]
    pub fn current_letter(&self) -> char {
        pattern_letter(self.bank.current)
    }

    /// Makes pattern `index` the edit pattern and tells the engine (`SelectPattern`: while
    /// playing in pattern mode the *playing* pattern switches at the next bar boundary and the
    /// letter blinks until then; when stopped the switch is immediate). The bass selection is
    /// dropped and the shown bar clamped. Not an undo step and not an edit.
    pub fn select_pattern(&mut self, index: u8) {
        let index = index % PATTERN_COUNT as u8;
        self.bank.current = index;
        self.selected_step = None;
        self.clear_roll_selection();
        self.clamp_shown_bar();
        self.send(Message::SelectPattern(index));
    }

    /// Sets the edit pattern's length (1, 2 or 4 bars). Steps beyond the new length are kept
    /// (hidden). One undo step when it changes.
    pub fn set_pattern_length(&mut self, length: PatternLength) {
        if self.pattern().pattern_length() == length {
            return;
        }
        self.begin_edit();
        self.pattern_mut().set_length(length);
        self.clamp_shown_bar();
        self.send(Message::SetPatternLength {
            pattern: self.bank.current,
            steps: length.steps(),
        });
    }

    /// Shows bar `bar` (clamped) of the edit pattern and stops following the playhead, so the
    /// choice sticks.
    pub fn show_bar(&mut self, bar: usize) {
        self.shown_bar = bar.min(self.pattern().bars() - 1);
        self.follow = false;
    }

    /// Flips FOLLOW (the grid follows the playing bar of the edit pattern).
    pub fn toggle_follow(&mut self) {
        self.follow = !self.follow;
    }

    fn clamp_shown_bar(&mut self) {
        self.shown_bar = self.shown_bar.min(self.pattern().bars() - 1);
        if let Some(step) = self.selected_step
            && step >= self.pattern().len()
        {
            self.selected_step = None;
        }
        let notes = self.pattern().notes().len();
        if self.roll_selected.is_some_and(|index| index >= notes)
            || self.roll_selection.iter().any(|&index| index >= notes)
        {
            self.clear_roll_selection();
        }
    }

    /// COPY: takes a copy of the edit pattern into the clipboard.
    pub fn copy_pattern(&mut self) {
        self.clipboard = Some(self.pattern().clone());
        self.set_status(format!("Copied pattern {}", self.current_letter()));
    }

    /// PASTE: writes the clipboard into the edit pattern (length, steps, volumes and tunes) and
    /// sends it. One undo step; `false` when there is nothing to paste.
    pub fn paste_pattern(&mut self) -> bool {
        let Some(pattern) = self.clipboard.clone() else {
            self.set_status("Nothing to paste: copy a pattern first");
            return false;
        };
        if pattern != *self.pattern() {
            self.begin_edit();
            *self.pattern_mut() = pattern;
            self.selected_step = None;
            self.clamp_shown_bar();
            self.send_pattern();
        }
        self.set_status(format!("Pasted into pattern {}", self.current_letter()));
        true
    }

    /// CLEAR: clears every step of the edit pattern (length, volume and tune stay). An undo
    /// step only if there was something to clear.
    pub fn clear_pattern(&mut self) {
        if !self.pattern().is_empty() {
            self.begin_edit();
        }
        self.pattern_mut().clear_all();
        self.send(Message::ClearPattern(self.bank.current));
    }

    /// Clears every row and the bass lane of the edit pattern (same as
    /// [`clear_pattern`](Self::clear_pattern)).
    pub fn clear_all(&mut self) {
        self.clear_pattern();
    }

    // -----------------------------------------------------------------------------------------
    // Pattern edits (mirror first, then the matching message). Step indices are absolute
    // within the edit pattern (`bar * 16 + column`) and wrap into its length.

    /// Sets one drum step (step index wraps into `0..length`).
    pub fn set_drum_step(&mut self, row: DrumRow, step: usize, on: bool, accent: bool) {
        let step = step % self.pattern().len();
        // An off step carries no accent.
        let accent = accent && on;
        self.begin_edit();
        self.pattern_mut()
            .set_drum_step(row, step as u8, on, accent);
        self.send(Message::SetDrumStep {
            row,
            step: step as u8,
            on,
            accent,
        });
    }

    /// A click on a drum cell: toggles the step; with shift, turns it on and flips the accent.
    pub fn toggle_drum_step(&mut self, row: DrumRow, step: usize, shift: bool) {
        let step = step % self.pattern().len();
        let current = self.pattern().drums[row.index()][step];
        if shift {
            self.set_drum_step(row, step, true, !(current.on && current.accent));
        } else {
            self.set_drum_step(row, step, !current.on, false);
        }
    }

    /// A right-click on a drum cell: clears the step (off, no accent). Nothing at all (no undo
    /// step, no message) when the step is already clear.
    pub fn clear_drum_step(&mut self, row: DrumRow, step: usize) {
        let step = step % self.pattern().len();
        let current = self.pattern().drums[row.index()][step];
        if current.on || current.accent {
            self.set_drum_step(row, step, false, false);
        }
    }

    /// Sets one bass-lane step (step index wraps into `0..length`).
    pub fn set_synth_step(&mut self, step: usize, note: Option<u8>, accent: bool) {
        let step = step % self.pattern().len();
        let note = note.map(|n| n.min(127));
        let accent = accent && note.is_some();
        self.begin_edit();
        self.pattern_mut().set_synth_step(step as u8, note, accent);
        self.send(Message::SetSynthStep {
            step: step as u8,
            note,
            accent,
        });
    }

    /// A click on a bass cell: selects the step for note entry (a second click deselects).
    /// With shift: flips the accent of a step that has a note, or gives an empty step the C
    /// of the current keyboard octave.
    pub fn click_synth_step(&mut self, step: usize, shift: bool) {
        let step = step % self.pattern().len();
        if shift {
            let current = self.pattern().synth[step];
            match current.note {
                Some(note) => self.set_synth_step(step, Some(note), !current.accent),
                None => self.set_synth_step(step, Some(self.input.base_note()), false),
            }
        } else if self.selected_step == Some(step) {
            self.selected_step = None;
        } else {
            self.selected_step = Some(step);
        }
    }

    /// A right-click on a bass cell: the step becomes a rest and, if selected, loses its
    /// selection. An already-empty step is left alone (no undo step, no message).
    pub fn clear_synth_step(&mut self, step: usize) {
        let step = step % self.pattern().len();
        if self.selected_step == Some(step) {
            self.selected_step = None;
        }
        let current = self.pattern().synth[step];
        if current.note.is_some() || current.accent {
            self.set_synth_step(step, None, false);
        }
    }

    /// Clears the selected bass step (Delete / Backspace), keeping it selected.
    pub fn clear_selected_step(&mut self) {
        if let Some(step) = self.selected_step {
            self.set_synth_step(step, None, false);
        }
    }

    /// Clears one drum row (volume and tune stay). An undo step only if the row had steps.
    pub fn clear_row(&mut self, row: DrumRow) {
        if self.pattern().drums[row.index()].iter().any(|s| s.on) {
            self.begin_edit();
        }
        self.pattern_mut().clear_row(row);
        self.send(Message::ClearRow(row));
    }

    /// Clears the bass lane. An undo step only if the lane had notes.
    pub fn clear_synth(&mut self) {
        if self.pattern().synth.iter().any(|s| s.note.is_some()) {
            self.begin_edit();
        }
        self.pattern_mut().clear_synth();
        self.send(Message::ClearSynth);
    }

    // -----------------------------------------------------------------------------------------
    // Piano-roll note edits (M7). Every edit mutates the mirror through the `Pattern` note API
    // (which clamps, re-sorts and rebuilds the grid lane), is one undo step, marks the project
    // dirty and replays the edit pattern's whole note list to the engine ([`send_notes`]):
    // a re-sort moves indices, so `ClearNotes` + one `SetNote` per note (at most
    // `MAX_NOTES + 1` messages) is the simplest way to keep both sides identical.

    /// Adds `note` to the edit pattern and returns its index in the sorted list, or `None`
    /// (no edit, no undo step) when the pattern is full, `note.length == 0` or it starts at or
    /// beyond the pattern length.
    pub fn add_note(&mut self, note: Note) -> Option<usize> {
        if note.length == 0
            || usize::from(note.start) >= self.pattern().len()
            || self.pattern().notes().len() >= MAX_NOTES
        {
            return None;
        }
        self.begin_edit();
        let index = self.pattern_mut().add_note(note)?;
        self.send_notes();
        Some(index)
    }

    /// Replaces note `index` of the edit pattern with `note` and returns its new index after
    /// re-sorting; `None` (no edit) when `index` is out of range or `note` is invalid. An
    /// unchanged note is not an edit either (returns `Some(index)`).
    pub fn set_note(&mut self, index: usize, note: Note) -> Option<usize> {
        let current = *self.pattern().notes().get(index)?;
        let mut target = note;
        target.normalise();
        if target == current {
            return Some(index);
        }
        if note.length == 0 || usize::from(note.start) >= self.pattern().len() {
            return None;
        }
        self.begin_edit();
        let new_index = self.pattern_mut().set_note(index, note)?;
        self.send_notes();
        Some(new_index)
    }

    /// Removes note `index` of the edit pattern; `false` (no edit) when out of range. A
    /// selection on the roll follows the shift of the later notes.
    pub fn remove_note(&mut self, index: usize) -> bool {
        if index >= self.pattern().notes().len() {
            return false;
        }
        self.begin_edit();
        self.pattern_mut().remove_note(index);
        let shift = |s: usize| match s {
            s if s == index => None,
            s if s > index => Some(s - 1),
            s => Some(s),
        };
        self.roll_selected = self.roll_selected.and_then(shift);
        self.roll_selection = self
            .roll_selection
            .iter()
            .filter_map(|&s| shift(s))
            .collect();
        self.send_notes();
        true
    }

    /// Removes every note of the edit pattern (the bass lane becomes rests). An undo step only
    /// if there was a note.
    pub fn clear_notes(&mut self) {
        if self.pattern().notes().is_empty() {
            return;
        }
        self.begin_edit();
        self.pattern_mut().clear_notes();
        self.clear_roll_selection();
        self.send_notes();
    }

    /// Replays the edit pattern's note list to the engine ([`note_messages`]).
    pub fn send_notes(&mut self) {
        let index = self.bank.current;
        for msg in note_messages(self.pattern(), index) {
            self.send(msg);
        }
    }

    /// Switches the SEQUENCER panel between the step GRID and the piano ROLL. Not an edit.
    /// The grid's bass selection is dropped on the way to the roll (in ROLL mode the computer
    /// keyboard only plays, unless record armed).
    pub fn set_lane_view(&mut self, view: LaneView) {
        self.lane_view = view;
        if view == LaneView::Roll {
            self.selected_step = None;
        } else {
            self.clear_roll_selection();
        }
    }

    /// Sets the piano roll's bottom pitch (clamped so `pitches` rows stay within MIDI).
    pub fn set_roll_low_pitch(&mut self, low_pitch: u8) {
        self.roll_low_pitch = low_pitch.min(127);
    }

    /// Makes the piano roll `rows` rows taller than its default (clamped to
    /// `0..=MAX_ROLL_EXTRA_ROWS`). View state, not an edit.
    pub fn set_roll_extra_rows(&mut self, rows: usize) {
        self.roll_extra_rows = rows.min(MAX_ROLL_EXTRA_ROWS);
    }

    /// Arms chord `index` of [`crate::chords::CHORDS`] for the next roll draw, or disarms it
    /// when it is the armed one already (or `None` / out of range). Not an edit.
    pub fn arm_chord(&mut self, index: Option<usize>) {
        self.armed_chord = match index {
            Some(i) if i < crate::chords::CHORDS.len() && self.armed_chord != Some(i) => Some(i),
            _ => None,
        };
    }

    /// Adds every note of `chord` on `root` at `start` for `length` steps (velocity
    /// `velocity`), skipping pitches above 127 and pitches the pattern already has a note on at
    /// that step, as one undo step. Returns how many notes were added (`0` means no edit: the
    /// pattern is full, `length == 0` or `start` is past the pattern end).
    pub fn add_chord(
        &mut self,
        chord: &crate::chords::Chord,
        root: u8,
        start: u8,
        length: u8,
        velocity: f32,
    ) -> usize {
        if length == 0 || usize::from(start) >= self.pattern().len() {
            return 0;
        }
        let wanted: Vec<u8> = crate::chords::chord_pitches(chord, root)
            .filter(|&p| self.pattern().note_at(start, p).is_none())
            .collect();
        let room = MAX_NOTES.saturating_sub(self.pattern().notes().len());
        let wanted = &wanted[..wanted.len().min(room)];
        if wanted.is_empty() {
            return 0;
        }
        self.begin_edit();
        let mut added = 0;
        for &pitch in wanted {
            let note = Note::new(start, length, pitch, velocity);
            if self.pattern_mut().add_note(note).is_some() {
                added += 1;
            }
        }
        self.send_notes();
        added
    }

    /// Replaces several notes of the edit pattern at once (`edits` pairs an index into the
    /// current note list with the note it should become) as one undo step. Returns how many
    /// notes changed (`0` means no edit: nothing differed, or every edit was out of range or
    /// invalid). The list is rebuilt, so indices move; the roll selection is remapped by the
    /// caller ([`crate::ui::sequencer::apply_roll_output`]).
    pub fn move_notes(&mut self, edits: &[(usize, Note)]) -> usize {
        let len = self.pattern().len();
        let mut target = self.pattern().notes().to_vec();
        let mut changed = 0;
        for &(index, note) in edits {
            let Some(slot) = target.get_mut(index) else {
                continue;
            };
            let mut note = note;
            note.normalise();
            if note.length == 0 || usize::from(note.start) >= len || *slot == note {
                continue;
            }
            *slot = note;
            changed += 1;
        }
        if changed == 0 {
            return 0;
        }
        self.begin_edit();
        self.pattern_mut().clear_notes();
        for note in target {
            self.pattern_mut().add_note(note);
        }
        self.send_notes();
        changed
    }

    /// Removes several notes of the edit pattern at once as one undo step; indices out of
    /// range are ignored. Returns how many were removed (`0` means no edit). The roll
    /// selection is dropped.
    pub fn remove_notes(&mut self, indices: &[usize]) -> usize {
        let count = self.pattern().notes().len();
        let mut order: Vec<usize> = indices.iter().copied().filter(|&i| i < count).collect();
        order.sort_unstable();
        order.dedup();
        if order.is_empty() {
            return 0;
        }
        self.begin_edit();
        for &index in order.iter().rev() {
            self.pattern_mut().remove_note(index);
        }
        self.clear_roll_selection();
        self.send_notes();
        order.len()
    }

    /// Drops the piano roll's selection (primary and the rest).
    pub fn clear_roll_selection(&mut self) {
        self.roll_selected = None;
        self.roll_selection.clear();
    }

    /// Shows or hides the piano roll's black-key rows. View state, not an edit.
    pub fn toggle_black_keys(&mut self) {
        self.roll_black_keys = !self.roll_black_keys;
    }

    /// The piano roll's playhead in fractional steps of the edit pattern (`bar * 16 + step +
    /// frac`) while the transport plays that pattern; `None` otherwise.
    #[must_use]
    pub fn roll_playhead(&self) -> Option<f32> {
        let pos = self.playhead;
        (pos.playing && pos.pattern == self.current()).then(|| {
            f32::from(pos.bar) * STEPS as f32 + f32::from(pos.step) + pos.frac.clamp(0.0, 1.0)
        })
    }

    /// Per-row drum volume in dB (clamped by the pattern); undo coalesces per knob drag.
    pub fn set_drum_volume(&mut self, row: DrumRow, db: f32) {
        let before = self.pattern().drum_volume[row.index()];
        let db = if db.is_finite() {
            db.clamp(*DRUM_VOLUME_RANGE.start(), *DRUM_VOLUME_RANGE.end())
        } else {
            before
        };
        if db != before {
            self.begin_continuous_edit(EditKey::DrumVolume(row), false);
        }
        self.pattern_mut().set_drum_volume(row, db);
        self.send(Message::SetDrumVolume { row, db });
    }

    /// Per-row drum tune in semitones (clamped by the pattern); undo coalesces per knob drag.
    pub fn set_drum_tune(&mut self, row: DrumRow, semitones: f32) {
        let before = self.pattern().drum_tune[row.index()];
        let semitones = if semitones.is_finite() {
            semitones.clamp(*DRUM_TUNE_RANGE.start(), *DRUM_TUNE_RANGE.end())
        } else {
            before
        };
        if semitones != before {
            self.begin_continuous_edit(EditKey::DrumTune(row), false);
        }
        self.pattern_mut().set_drum_tune(row, semitones);
        self.send(Message::SetDrumTune { row, semitones });
    }

    /// Sends the edit pattern to the engine ([`pattern_messages`]: clear, then only its
    /// non-empty steps, the length and the row volumes / tunes) and reselects it so the
    /// engine's edit pattern is the mirror's.
    pub fn send_pattern(&mut self) {
        let index = self.bank.current;
        for msg in pattern_messages(self.pattern(), index) {
            self.send(msg);
        }
        self.send(Message::SelectPattern(index));
    }

    /// Sends the whole bank ([`bank_messages`]): every pattern, the chain, the play mode when
    /// it differs from the engine's, and `SelectPattern(current)`. Messages beyond the queue's
    /// free slots wait in the outbox.
    pub fn send_bank(&mut self) {
        let mode = (self.bank.mode != self.engine_mode).then_some(self.bank.mode);
        self.engine_mode = self.bank.mode;
        for msg in bank_messages(&self.bank, mode) {
            self.send(msg);
        }
    }

    /// Sends the chain (length, then every entry).
    pub fn send_chain(&mut self) {
        for msg in chain_messages(&self.bank) {
            self.send(msg);
        }
    }

    /// Replaces the edit pattern and sends it to the engine (one undo step).
    pub fn set_pattern(&mut self, pattern: Pattern) {
        self.begin_edit();
        *self.pattern_mut() = pattern;
        self.pattern_mut().normalise();
        self.selected_step = None;
        self.clamp_shown_bar();
        self.send_pattern();
    }

    // -----------------------------------------------------------------------------------------
    // Song chain (M5). Every change is one undo step; the engine gets `SetChainEntry` /
    // `SetChainLen`.

    /// The valid chain entries.
    #[must_use]
    pub fn chain(&self) -> &[ChainEntry] {
        self.bank.chain_entries()
    }

    /// The selected chain entry (index and value), if the chain has one.
    #[must_use]
    pub fn selected_entry(&self) -> Option<(usize, ChainEntry)> {
        let index = self.song_selected?;
        self.chain().get(index).map(|entry| (index, *entry))
    }

    /// Selects chain entry `index` (ignored when out of range).
    pub fn select_chain_entry(&mut self, index: usize) {
        if index < self.chain().len() {
            self.song_selected = Some(index);
        }
    }

    fn clamp_song_selected(&mut self) {
        let len = self.chain().len();
        self.song_selected = if len == 0 {
            None
        } else {
            Some(self.song_selected.unwrap_or(0).min(len - 1))
        };
    }

    /// ADD: appends the edit pattern once to the chain and selects the new entry. `false`
    /// when the chain is full.
    pub fn add_chain_entry(&mut self) -> bool {
        if self.chain().len() >= MAX_CHAIN {
            self.set_status(format!("Chain is full ({MAX_CHAIN} entries)"));
            return false;
        }
        self.begin_edit();
        let entry = ChainEntry::new(self.bank.current, 1);
        self.bank.push(entry);
        let index = self.chain().len() - 1;
        self.song_selected = Some(index);
        self.send(Message::SetChainEntry {
            index: index as u8,
            pattern: entry.pattern,
            repeats: entry.repeats,
        });
        true
    }

    /// REMOVE: deletes the selected entry; the selection moves to the next one (or the last).
    /// `false` when nothing is selected.
    pub fn remove_chain_entry(&mut self) -> bool {
        let Some((index, _)) = self.selected_entry() else {
            return false;
        };
        self.begin_edit();
        self.bank.remove_entry(index as u8);
        self.clamp_song_selected();
        self.send_chain();
        true
    }

    /// Sets the selected entry's pattern.
    pub fn set_chain_pattern(&mut self, pattern: u8) {
        if let Some((index, entry)) = self.selected_entry() {
            self.set_chain_entry(index, ChainEntry::new(pattern, entry.repeats));
        }
    }

    /// Sets the selected entry's repeat count (clamped to `1..=8`).
    pub fn set_chain_repeats(&mut self, repeats: u8) {
        if let Some((index, entry)) = self.selected_entry() {
            self.set_chain_entry(index, ChainEntry::new(entry.pattern, repeats));
        }
    }

    /// Replaces chain entry `index` (must be within the chain). One undo step when it changes.
    pub fn set_chain_entry(&mut self, index: usize, entry: ChainEntry) {
        if self.chain().get(index).is_none_or(|e| *e == entry) {
            return;
        }
        self.begin_edit();
        self.bank.set_entry(index as u8, entry);
        let entry = self.bank.chain[index];
        self.send(Message::SetChainEntry {
            index: index as u8,
            pattern: entry.pattern,
            repeats: entry.repeats,
        });
    }

    /// Moves the selected entry one place earlier (`-1`) or later (`+1`) in the chain; the
    /// selection follows it. `false` when it cannot move.
    pub fn move_chain_entry(&mut self, delta: i32) -> bool {
        let Some((index, _)) = self.selected_entry() else {
            return false;
        };
        let len = self.chain().len();
        let Some(target) = index.checked_add_signed(delta as isize) else {
            return false;
        };
        if target >= len || target == index {
            return false;
        }
        self.begin_edit();
        self.bank.chain.swap(index, target);
        self.song_selected = Some(target);
        // Resend the whole chain: at most MAX_CHAIN + 1 messages.
        self.send_chain();
        true
    }

    // -----------------------------------------------------------------------------------------
    // Undo / redo and the unsaved marker.

    /// The current state as an undo [`Snapshot`].
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            bank: self.bank.clone(),
            params: self.params,
            bpm: self.bpm,
            swing: self.swing,
            preset_index: self.preset_index,
            mixer: self.mixer,
            project_path: self.project_path.clone(),
        }
    }

    /// Call before a discrete user edit: records the current state as an undo step (unless it
    /// equals the step on top, see [`UndoStack::push`]) and marks the project dirty. Ends any
    /// running continuous edit, so the next knob change starts a new step.
    pub fn begin_edit(&mut self) {
        let snapshot = self.snapshot();
        self.undo.push(snapshot);
        self.last_continuous_edit = None;
        self.dirty = true;
    }

    /// Call before a change to a continuous control (`key`): records an undo step when this is
    /// the first change of a drag (`drag_started`), the first change to this control at all,
    /// the first after a change to a *different* control, or the first after a pause of
    /// [`CONTINUOUS_EDIT_GAP`]; changes closer together join the running step. Always marks
    /// the project dirty.
    pub fn begin_continuous_edit(&mut self, key: EditKey, drag_started: bool) {
        let now = Instant::now();
        let continuing = !drag_started
            && self.last_continuous_edit.is_some_and(|(last, at)| {
                last == key && now.duration_since(at) < CONTINUOUS_EDIT_GAP
            });
        if !continuing {
            let snapshot = self.snapshot();
            self.undo.push(snapshot);
        }
        self.last_continuous_edit = Some((key, now));
        self.dirty = true;
    }

    /// Marks the project as having unsaved changes without an undo step.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Marks the project as matching its file (after save / open / new).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Steps back one edit: restores the previous snapshot and resends everything to the
    /// engine. Returns `false` when there was nothing to undo.
    pub fn undo(&mut self) -> bool {
        let current = self.snapshot();
        match self.undo.undo(current) {
            Some(snapshot) => {
                self.restore(snapshot);
                true
            }
            None => false,
        }
    }

    /// Re-applies the last undone edit. Returns `false` when there was nothing to redo.
    pub fn redo(&mut self) -> bool {
        let current = self.snapshot();
        match self.undo.redo(current) {
            Some(snapshot) => {
                self.restore(snapshot);
                true
            }
            None => false,
        }
    }

    /// Puts `snapshot` into the mirrors and the engine (`resend_all`); the project is dirty
    /// afterwards (the file no longer matches), and a running knob drag is cut.
    fn restore(&mut self, snapshot: Snapshot) {
        let Snapshot {
            bank,
            params,
            bpm,
            swing,
            preset_index,
            mixer,
            project_path,
        } = snapshot;
        self.bank = bank;
        self.params = params;
        self.bpm = bpm;
        self.swing = swing;
        self.preset_index = preset_index;
        self.mixer = mixer;
        self.project_path = project_path;
        self.last_continuous_edit = None;
        self.dirty = true;
        self.clamp_shown_bar();
        self.clamp_song_selected();
        self.resend_all();
    }

    /// Reads this frame's shortcuts (consuming their keys) unless a text field has the
    /// keyboard. The caller dispatches them (see [`BeatSlopperApp::tick`]).
    pub fn take_shortcuts(&mut self, ctx: &egui::Context) -> Vec<Shortcut> {
        if ctx.text_edit_focused() {
            return Vec::new();
        }
        ctx.input_mut(shortcuts::take_shortcuts)
    }

    // -----------------------------------------------------------------------------------------
    // Status line.

    /// Shows `message` for [`STATUS_DURATION`].
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    /// The status line, if one is still showing.
    #[must_use]
    pub fn status_text(&self) -> Option<&str> {
        self.status
            .as_ref()
            .filter(|(_, since)| since.elapsed() < STATUS_DURATION)
            .map(|(text, _)| text.as_str())
    }

    /// Drops an expired status line.
    pub fn expire_status(&mut self) {
        if self.status_text().is_none() {
            self.status = None;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Notes.

    /// Starts `note` (MIDI) at `velocity` (0..=1): tells the engine and lights the key. A
    /// selected bass step takes the note (and stays selected); otherwise, while playing and
    /// armed, the note is quantised to the nearest step and recorded (accent at
    /// [`ACCENT_VELOCITY`] or above). The note always sounds live.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        self.note_on_from(note, velocity, NoteSource::Local);
    }

    /// Stops `note` (MIDI): tells the engine and unlights the key.
    pub fn note_off(&mut self, note: u8) {
        self.note_off_from(note, NoteSource::Local);
    }

    /// [`AppState::note_on`] with an explicit source: a [`NoteSource::Midi`] note does
    /// everything except the engine message (the MIDI thread already sent it).
    pub fn note_on_from(&mut self, note: u8, velocity: f32, source: NoteSource) {
        let note = note.min(127);
        let velocity = velocity.clamp(0.0, 1.0);
        // The grid's bass selection takes the note only while the grid is showing; on the
        // roll the keyboard just plays (overdub below still records length-1 notes).
        if let Some(step) = self
            .selected_step
            .filter(|_| self.lane_view == LaneView::Grid)
        {
            let accent = self.pattern().synth[step % self.pattern().len()].accent;
            self.set_synth_step(step, Some(note), accent);
        } else if self.is_recording() {
            let step = quantise_step(self.playhead, self.pattern().len());
            self.set_synth_step(step, Some(note), velocity >= ACCENT_VELOCITY);
        }
        self.held_notes[usize::from(note)] = true;
        match source {
            NoteSource::Local => {
                self.note_on_count += 1;
                self.send(Message::NoteOn { note, velocity });
            }
            NoteSource::Midi => self.midi_note_on_count += 1,
        }
    }

    /// [`AppState::note_off`] with an explicit source (see [`AppState::note_on_from`]).
    pub fn note_off_from(&mut self, note: u8, source: NoteSource) {
        let note = note.min(127);
        self.held_notes[usize::from(note)] = false;
        match source {
            NoteSource::Local => {
                self.note_off_count += 1;
                self.send(Message::NoteOff { note });
            }
            NoteSource::Midi => self.midi_note_off_count += 1,
        }
    }

    /// Releases every key (computer keyboard and on-screen) and silences the engine.
    pub fn all_notes_off(&mut self) {
        self.input.release_all();
        self.held_notes = [false; 128];
        self.send(Message::AllNotesOff);
    }

    /// Applies note events from the keyboard (or the on-screen piano).
    pub fn apply_note_events(&mut self, events: &[NoteEvent]) {
        for event in events {
            self.apply_note_event(*event, NoteSource::Local);
        }
    }

    /// Applies one note event from `source` (the shared path for keys, mouse and MIDI).
    pub fn apply_note_event(&mut self, event: NoteEvent, source: NoteSource) {
        match event {
            NoteEvent::On { note, velocity } => self.note_on_from(note, velocity, source),
            NoteEvent::Off { note } => self.note_off_from(note, source),
        }
    }

    /// Reads this frame's egui events and plays the computer keyboard. While a text edit has
    /// focus the keys type instead of play; anything still held is released so no note sticks.
    /// With a bass step selected, Delete / Backspace clear it and Escape deselects it; Escape
    /// also cancels a pending DISCARD / KEEP confirm.
    pub fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let events = if ctx.text_edit_focused() {
            self.input.release_all()
        } else {
            let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            if escape && self.pending_action.is_some() {
                self.cancel_pending();
            }
            if self.selected_step.is_some() {
                let clear = ctx.input(|i| {
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                });
                if clear {
                    self.clear_selected_step();
                }
                if escape {
                    self.selected_step = None;
                }
            }
            if escape && (self.roll_selected.is_some() || !self.roll_selection.is_empty()) {
                self.clear_roll_selection();
            }
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

    // ---- MIDI ----

    /// `true` while a MIDI input port is open.
    #[must_use]
    pub fn midi_connected(&self) -> bool {
        self.midi_conn.is_some()
    }

    /// Name of the open MIDI port, if any.
    #[must_use]
    pub fn midi_port_name(&self) -> Option<&str> {
        self.midi_conn.as_ref().map(MidiConnection::port_name)
    }

    /// Re-reads the port list if it is older than [`MIDI_PORTS_REFRESH`] (or never read), then
    /// reconciles: a connection whose port vanished is closed (the choice is kept so it
    /// reconnects when the device comes back), and the chosen port is opened when it is
    /// present and not connected. Cheap to call every frame; does nothing when MIDI is off.
    pub fn refresh_midi_ports(&mut self) {
        if !self.midi_enabled {
            return;
        }
        let stale = self
            .midi_ports_refreshed
            .is_none_or(|at| at.elapsed() >= MIDI_PORTS_REFRESH);
        if !stale {
            return;
        }
        self.midi_ports = MidiInput::list_ports();
        self.midi_ports_refreshed = Some(Instant::now());

        if let Some(name) = self.midi_port_name()
            && !self.midi_ports.iter().any(|p| p == name)
        {
            let name = name.to_owned();
            self.close_midi();
            self.set_status(format!("MIDI unplugged: {}", truncate_name(&name, 18)));
        }
        if self.midi_conn.is_none()
            && let Some(wanted) = self.midi_port.clone()
            && self.midi_failed_port.as_deref() != Some(wanted.as_str())
            && self.midi_ports.contains(&wanted)
        {
            self.connect_midi(&wanted);
        }
    }

    /// Opens `name` as the MIDI input (closing any current connection first) and remembers it
    /// as the chosen port. Failures show in the status line; the choice is kept so the port
    /// is retried on the next launch (but not every refresh).
    pub fn connect_midi(&mut self, name: &str) {
        self.midi_port = Some(name.to_owned());
        if !self.midi_enabled {
            return;
        }
        if self.midi_port_name() == Some(name) {
            return;
        }
        self.close_midi();
        let Some(handle) = self.midi_handle.take() else {
            self.set_status("MIDI unavailable (engine queue taken)");
            return;
        };
        let (tx, rx): (SyncSender<NoteEvent>, Receiver<NoteEvent>) =
            std::sync::mpsc::sync_channel(UI_QUEUE_CAPACITY);
        let wake: Option<Box<dyn Fn() + Send>> = self
            .midi_wake
            .clone()
            .map(|ctx| Box::new(move || ctx.request_repaint()) as Box<dyn Fn() + Send>);
        match MidiInput::connect(name, handle, tx, wake) {
            Ok(conn) => {
                self.midi_conn = Some(conn);
                self.midi_rx = Some(rx);
                self.midi_failed_port = None;
                self.set_status(format!("MIDI: {}", truncate_name(name, 18)));
            }
            Err(err) => {
                self.midi_handle = err.midi;
                self.midi_failed_port = Some(name.to_owned());
                self.set_status(err.message);
            }
        }
    }

    /// Closes the connection (if any), silences everything (`AllNotesOff` through the UI
    /// queue, every key light off) and forgets the chosen port ("None" in the selector).
    pub fn disconnect_midi(&mut self) {
        self.midi_port = None;
        self.midi_failed_port = None;
        self.close_midi();
    }

    /// Closes the connection without touching the remembered choice. Any notes it held are
    /// silenced and unlit.
    fn close_midi(&mut self) {
        self.midi_rx = None;
        if let Some(conn) = self.midi_conn.take() {
            self.midi_handle = Some(conn.close());
            self.all_notes_off();
        }
    }

    /// Applies every note event the MIDI thread mirrored since the last tick, through the same
    /// path as the computer keyboard but as [`NoteSource::Midi`] (no second engine message).
    /// Returns how many events were applied.
    pub fn poll_midi(&mut self) -> usize {
        let mut count = 0;
        while let Some(rx) = self.midi_rx.as_ref() {
            match rx.try_recv() {
                Ok(event) => {
                    self.apply_note_event(event, NoteSource::Midi);
                    count += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The connection's callback data is gone; stop polling a dead channel.
                    self.midi_rx = None;
                    break;
                }
            }
        }
        count
    }
}

/// Shortens `name` to at most `max` characters, ending in an ellipsis when it was cut.
#[must_use]
pub fn truncate_name(name: &str, max: usize) -> String {
    let count = name.chars().count();
    if count <= max {
        return name.to_owned();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = name.chars().take(keep).collect();
    out.push('…');
    out
}

/// The messages that put `pattern` into the engine's slot `index`, in order: `SetEditPattern`,
/// `ClearPattern` (so steps the engine had and the mirror has not are gone), the length
/// (widened to [`Pattern::used_steps`] when data lives beyond the length, because step
/// messages wrap into the engine pattern's length), every non-empty drum step, the note list
/// ([`note_messages`]: `ClearNotes` then one `SetNote` per note, M7), the real length when it
/// was widened, and each row's volume and tune. Only non-empty steps and the actual notes are
/// sent, so a sparse pattern costs a few dozen messages and a dense 64-step one at most
/// `64 * 5 + 65 + 14`.
#[must_use]
pub fn pattern_messages(pattern: &Pattern, index: u8) -> Vec<Message> {
    let mut out = Vec::new();
    out.push(Message::SetEditPattern(index));
    out.push(Message::ClearPattern(index));
    let len = pattern.len() as u8;
    let used = pattern.used_steps();
    // Step messages wrap into the engine pattern's *current* length, so the length (widened
    // to the used steps) always goes first.
    out.push(Message::SetPatternLength {
        pattern: index,
        steps: used as u8,
    });
    for row in DrumRow::ALL {
        for (step, cell) in pattern.drums[row.index()].iter().take(used).enumerate() {
            if *cell != DrumStep::default() {
                out.push(Message::SetDrumStep {
                    row,
                    step: step as u8,
                    on: cell.on,
                    accent: cell.accent,
                });
            }
        }
    }
    // M7: the synth part is the note list (chords, held notes), not the derived lane. Notes
    // starting on hidden steps are accepted while the length is widened.
    out.extend(note_messages(pattern, index));
    if used != usize::from(len) {
        out.push(Message::SetPatternLength {
            pattern: index,
            steps: len,
        });
    }
    for row in DrumRow::ALL {
        out.push(Message::SetDrumVolume {
            row,
            db: pattern.drum_volume[row.index()],
        });
        out.push(Message::SetDrumTune {
            row,
            semitones: pattern.drum_tune[row.index()],
        });
    }
    out
}

/// The messages that replace pattern `index`'s note list in the engine with `pattern`'s (M7):
/// `ClearNotes(index)` then one `SetNote` per note in sorted order, each with
/// `index == note_count` (an append), so the engine's list ends up identical to the mirror's
/// whatever it held before. At most `MAX_NOTES + 1` messages. A note that starts at or beyond
/// the engine pattern's *current* length is ignored by the engine, so callers that replay a
/// pattern with hidden notes widen the length first ([`pattern_messages`] does).
#[must_use]
pub fn note_messages(pattern: &Pattern, index: u8) -> Vec<Message> {
    let notes = pattern.notes();
    let mut out = Vec::with_capacity(notes.len() + 1);
    out.push(Message::ClearNotes(index));
    for (i, note) in notes.iter().enumerate() {
        out.push(Message::SetNote {
            pattern: index,
            index: i as u8,
            note: *note,
        });
    }
    out
}

/// The messages that put `mixer` into the engine: one `SetMixer` per channel and parameter
/// (`Channel::COUNT * ChannelParam::ALL.len()` = 36), in strip then parameter order.
#[must_use]
pub fn mixer_messages(mixer: &MixerState) -> Vec<Message> {
    let mut out = Vec::with_capacity(Channel::COUNT * ChannelParam::ALL.len());
    for channel in Channel::ALL {
        for param in ChannelParam::ALL {
            out.push(Message::SetMixer {
                channel,
                param,
                value: mixer.get(channel, param),
            });
        }
    }
    out
}

/// The messages that put `bank`'s chain into the engine: `SetChainLen` then one
/// `SetChainEntry` per entry (at most `MAX_CHAIN + 1`).
#[must_use]
pub fn chain_messages(bank: &Bank) -> Vec<Message> {
    let entries = bank.chain_entries();
    let mut out = Vec::with_capacity(entries.len() + 1);
    out.push(Message::SetChainLen(entries.len() as u8));
    for (index, entry) in entries.iter().enumerate() {
        out.push(Message::SetChainEntry {
            index: index as u8,
            pattern: entry.pattern,
            repeats: entry.repeats.clamp(1, MAX_REPEATS),
        });
    }
    out
}

/// The messages that put the whole `bank` into the engine: [`pattern_messages`] for every
/// pattern, [`chain_messages`], `SetPlayMode(mode)` when `mode` is `Some` (the caller passes
/// it only when the engine's mode differs) and finally `SelectPattern(current)`, which makes
/// the engine's edit pattern the mirror's and, when stopped, rewinds to it.
#[must_use]
pub fn bank_messages(bank: &Bank, mode: Option<PlayMode>) -> Vec<Message> {
    let mut out = Vec::new();
    for (index, pattern) in bank.patterns.iter().enumerate() {
        out.extend(pattern_messages(pattern, index as u8));
    }
    out.extend(chain_messages(bank));
    if let Some(mode) = mode {
        out.push(Message::SetPlayMode(mode));
    }
    out.push(Message::SelectPattern(bank.current));
    out
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
    /// The window title last sent (`None` until the first frame).
    title: Option<String>,
    /// Whether the first-frame window size check has run (see [`crate::window`]).
    size_checked: bool,
}

impl BeatSlopperApp {
    /// Builds the app: loads the persisted theme override, preset, project path and export
    /// repeats, opens audio (failure is logged and tolerated) and applies the initial theme.
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
        let project_path: Option<PathBuf> = cc
            .storage
            .and_then(|s| eframe::get_value::<Option<PathBuf>>(s, PROJECT_PATH_KEY))
            .flatten();
        let export_repeats: Option<u32> = cc
            .storage
            .and_then(|s| eframe::get_value::<u32>(s, EXPORT_REPEATS_KEY));
        let midi_port: Option<String> = cc
            .storage
            .and_then(|s| eframe::get_value::<Option<String>>(s, MIDI_PORT_KEY))
            .flatten();
        let right_panel: Option<RightPanel> = cc
            .storage
            .and_then(|s| eframe::get_value::<RightPanel>(s, RIGHT_PANEL_KEY));
        let lane_view: Option<LaneView> = cc
            .storage
            .and_then(|s| eframe::get_value::<LaneView>(s, LANE_VIEW_KEY));
        let roll_low_pitch: Option<u8> = cc
            .storage
            .and_then(|s| eframe::get_value::<u8>(s, ROLL_LOW_PITCH_KEY));

        let mut state = AppState::with_audio(mode);
        state.theme_override = theme_override;
        state.right_panel = right_panel.unwrap_or_default();
        state.set_lane_view(lane_view.unwrap_or_default());
        state.set_roll_low_pitch(roll_low_pitch.unwrap_or(DEFAULT_ROLL_LOW_PITCH));
        let roll_extra_rows: Option<usize> = cc
            .storage
            .and_then(|s| eframe::get_value::<usize>(s, ROLL_EXTRA_ROWS_KEY));
        state.set_roll_extra_rows(roll_extra_rows.unwrap_or(0));
        let roll_black_keys: Option<bool> = cc
            .storage
            .and_then(|s| eframe::get_value::<bool>(s, ROLL_BLACK_KEYS_KEY));
        state.roll_black_keys = roll_black_keys.unwrap_or(true);
        if let Some(index) = preset_index {
            // Re-apply the remembered preset so the sound matches the highlighted name.
            state.apply_preset(index);
        }
        state.project_path = project_path;
        if let Some(repeats) = export_repeats.filter(|r| EXPORT_REPEATS.contains(r)) {
            state.export_repeats = repeats;
        }

        // ---- MIDI ----
        // The smoke test never touches a MIDI device. Otherwise list the ports now and reopen
        // the remembered one when it is present (silently skipped when absent).
        state.midi_enabled = !smoke_test;
        state.midi_wake = Some(cc.egui_ctx.clone());
        state.midi_port = midi_port;
        state.refresh_midi_ports();

        // Nothing above was a user edit: start with a clean project and no history. The smoke
        // test never opens a dialog (no key reaches it, but be explicit).
        state.undo.clear();
        state.dirty = false;
        state.dialogs_enabled = !smoke_test;

        let mut app = Self {
            theme: Theme::for_mode(mode),
            state,
            smoke_test,
            frames: 0,
            smoke_done: false,
            applied: false,
            title: None,
            size_checked: false,
        };
        app.apply_theme(&cc.egui_ctx);
        app
    }

    /// Test/headless constructor: no storage, no audio, explicit mode.
    pub fn headless(mode: ThemeMode, smoke_test: bool) -> (Self, engine::Engine) {
        let (mut state, engine) = AppState::headless(mode);
        state.theme_override = Some(mode);
        state.midi_enabled = !smoke_test;
        (
            Self {
                theme: Theme::for_mode(mode),
                state,
                smoke_test,
                frames: 0,
                smoke_done: false,
                applied: false,
                title: None,
                size_checked: false,
            },
            engine,
        )
    }

    /// The window title last sent to the viewport (`None` before the first frame).
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
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

    /// Per-frame logic (runs before the UI): theme sync, playhead, export polling, shortcuts,
    /// the computer-keyboard piano, the window title / size and the smoke-test counter. Public
    /// so tests can drive a frame without an `eframe::Frame`.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.sync_theme(ctx);
        if self.state.audio_error.is_none()
            && self.state.audio.as_ref().is_some_and(|a| a.had_error())
        {
            self.state.audio_error = Some("audio stream error".to_owned());
        }
        self.state.flush_outbox();
        self.state.update_playhead();
        self.state.update_meters();
        self.state.poll_export();
        self.state.expire_status();
        // Shortcuts first, so their keys are consumed before the piano sees the frame.
        if !self.smoke_test {
            for shortcut in self.state.take_shortcuts(ctx) {
                self.run_shortcut(shortcut);
            }
        }
        self.state.handle_keyboard(ctx);
        self.sync_title(ctx);
        self.check_window_size(ctx);
        // ---- MIDI ----
        let midi_events = self.state.poll_midi();

        if self.state.playing
            || self.state.playhead.playing
            || self.state.meters_active()
            || midi_events > 0
            || !self.state.outbox.is_empty()
        {
            // The playhead moves between frames; keep drawing while the loop runs, while the
            // meters are still falling, and while the outbox has messages waiting.
            ctx.request_repaint();
        } else if self.state.is_exporting() || self.state.status.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.state.midi_enabled {
            // Pick up hot-plugged MIDI devices without waiting for mouse movement.
            ctx.request_repaint_after(MIDI_PORTS_REFRESH);
        }

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

    /// Performs one keyboard shortcut. Dialog shortcuts go through the project panel's
    /// actions, which refuse to open a dialog unless [`AppState::dialogs_enabled`].
    pub fn run_shortcut(&mut self, shortcut: Shortcut) {
        let state = &mut self.state;
        match shortcut {
            Shortcut::PlayStop => state.toggle_play(),
            Shortcut::ToggleArm => state.toggle_arm(),
            Shortcut::Undo => {
                state.undo();
            }
            Shortcut::Redo => {
                state.redo();
            }
            Shortcut::Save => crate::ui::project_bar::save(state),
            Shortcut::SaveAs => crate::ui::project_bar::save_as(state),
            Shortcut::Open => crate::ui::project_bar::request_open(state),
            Shortcut::Export => crate::ui::project_bar::export(state),
            Shortcut::New => {
                state.request_new();
            }
        }
    }

    /// Sends the window title when it changed: the project file name and a trailing ` *`
    /// while there are unsaved changes.
    fn sync_title(&mut self, ctx: &egui::Context) {
        let title = window::window_title(self.state.project_path.as_deref(), self.state.dirty);
        if self.title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = Some(title);
        }
    }

    /// Once, on the first frame that reports a monitor: if the window opened clearly smaller
    /// than [`window::REQUESTED_INNER_SIZE`] (a stale persisted size, a scaling quirk), ask
    /// for the size that fits the monitor. See [`crate::window`] for the investigation.
    fn check_window_size(&mut self, ctx: &egui::Context) {
        if self.size_checked {
            return;
        }
        let (inner, monitor) = ctx.input(|i| {
            let viewport = i.viewport();
            (viewport.inner_rect, viewport.monitor_size)
        });
        let (Some(inner), Some(monitor)) = (inner, monitor) else {
            // Headless (egui_kittest) or not yet known: try again next frame.
            return;
        };
        self.size_checked = true;
        let target = window::fit_inner_size(
            window::REQUESTED_INNER_SIZE,
            window::MIN_INNER_SIZE,
            Some(monitor),
        );
        if window::needs_resize(inner.size(), target) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target));
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
        eframe::set_value(storage, PROJECT_PATH_KEY, &self.state.project_path);
        eframe::set_value(storage, EXPORT_REPEATS_KEY, &self.state.export_repeats);
        eframe::set_value(storage, MIDI_PORT_KEY, &self.state.midi_port);
        eframe::set_value(storage, RIGHT_PANEL_KEY, &self.state.right_panel);
        eframe::set_value(storage, LANE_VIEW_KEY, &self.state.lane_view);
        eframe::set_value(storage, ROLL_LOW_PITCH_KEY, &self.state.roll_low_pitch);
        eframe::set_value(storage, ROLL_EXTRA_ROWS_KEY, &self.state.roll_extra_rows);
        eframe::set_value(storage, ROLL_BLACK_KEYS_KEY, &self.state.roll_black_keys);
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
    use engine::{DrumStep, SynthStep};

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
        assert_eq!(state.swing, 0.0);
        assert_eq!(state.pattern().clone(), Pattern::demo());
        assert!(!state.playing && !state.armed);
        assert_eq!(state.selected_step, None);
        assert_eq!(state.export_repeats, 1);
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
    fn swing_is_clamped_and_sent() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.set_swing(2.0);
        assert_eq!(state.swing, MAX_SWING);
        state.set_swing(-1.0);
        assert_eq!(state.swing, 0.0);
        state.set_swing(f32::INFINITY);
        assert_eq!(state.swing, 0.0);
        state.set_swing(0.5);
        engine.process_messages();
        assert_eq!(engine.transport().swing(), 0.5);
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

    // -----------------------------------------------------------------------------------------
    // M3: transport and sequencer.

    fn at(step: u8, frac: f32) -> Position {
        Position {
            step,
            frac,
            playing: true,
            ..Position::stopped(0)
        }
    }

    #[test]
    fn quantisation_rounds_to_the_nearest_step_and_wraps() {
        assert_eq!(quantise_step(at(3, 0.0), 16), 3);
        assert_eq!(quantise_step(at(3, 0.49), 16), 3);
        assert_eq!(quantise_step(at(3, 0.5), 16), 4);
        assert_eq!(quantise_step(at(3, 0.99), 16), 4);
        assert_eq!(quantise_step(at(15, 0.2), 16), 15);
        assert_eq!(quantise_step(at(15, 0.5), 16), 0);
        assert_eq!(quantise_step(at(15, 0.99), 16), 0);
    }

    #[test]
    fn play_and_stop_reach_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        assert!(!engine.transport().is_playing());
        state.play();
        assert!(state.playing);
        engine.process_messages();
        assert!(engine.transport().is_playing());
        assert!(state.engine.position().playing);

        state.toggle_play();
        assert!(!state.playing);
        engine.process_messages();
        assert!(!engine.transport().is_playing());
        assert!(!state.engine.position().playing);

        state.toggle_play();
        assert!(state.playing);
        state.toggle_arm();
        assert!(state.armed && state.is_recording());
        state.toggle_arm();
        assert!(!state.is_recording());
    }

    /// Drains the outbox into the engine until both are empty (a resend that exceeds the
    /// queue takes several frames; this plays the frames).
    fn sync(state: &mut AppState, engine: &mut engine::Engine) -> usize {
        let mut frames = 0;
        loop {
            engine.process_messages();
            frames += 1;
            if state.flush_outbox() == 0 && state.outbox.is_empty() {
                break;
            }
        }
        frames
    }

    #[test]
    fn send_pattern_sends_only_the_used_steps_plus_volume_and_tune() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.pattern_mut().drum_volume[DrumRow::Snare.index()] = -6.0;
        state.pattern_mut().drum_tune[DrumRow::Kick.index()] = 3.0;
        assert_ne!(engine.pattern(), state.pattern());

        let messages = pattern_messages(state.pattern(), 0);
        let used_steps = state
            .pattern()
            .drums
            .iter()
            .flatten()
            .filter(|s| **s != DrumStep::default())
            .count();
        let notes = state.pattern().notes().len();
        assert!(notes > 0, "the demo has a bass line");
        // SetEditPattern, ClearPattern, the length, the non-empty drum steps, ClearNotes plus
        // one SetNote per note (M7), 5 volumes, 5 tunes.
        assert_eq!(
            messages.len(),
            3 + used_steps + 1 + notes + 2 * DrumRow::COUNT
        );
        assert!(messages.contains(&Message::ClearNotes(0)));
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::SetSynthStep { .. })),
            "the lane is derived: notes are sent, not lane cells"
        );
        assert_eq!(messages[0], Message::SetEditPattern(0));
        assert_eq!(messages[1], Message::ClearPattern(0));
        assert_eq!(
            messages[2],
            Message::SetPatternLength {
                pattern: 0,
                steps: 16
            }
        );

        let free_before = state.engine.free_slots();
        state.send_pattern();
        assert_eq!(
            free_before - state.engine.free_slots(),
            messages.len() + 1,
            "plus SelectPattern"
        );
        engine.process_messages();
        assert_eq!(engine.pattern(), state.pattern());
        assert_eq!(engine.pattern().drum_volume[DrumRow::Snare.index()], -6.0);
        assert_eq!(engine.pattern().drum_tune[DrumRow::Kick.index()], 3.0);
    }

    #[test]
    fn resend_all_fits_the_queue_and_mirrors_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.set_bpm(140.0);
        state.set_swing(0.25);
        let free_before = state.engine.free_slots();
        state.resend_all();
        let expected = ParamId::COUNT
            + 2
            + mixer_messages(&state.mixer).len()
            + bank_messages(&state.bank, None).len();
        assert!(expected < engine::QUEUE_CAPACITY, "{expected}");
        assert_eq!(free_before - state.engine.free_slots(), expected);
        assert!(state.outbox.is_empty(), "the default bank fits in one go");
        engine.process_messages();
        assert_eq!(engine.bpm(), 140.0);
        assert_eq!(engine.transport().swing(), 0.25);
        assert_eq!(engine.bank(), &state.bank);
        assert_eq!(engine.mixer(), &state.mixer);
    }

    // -----------------------------------------------------------------------------------------
    // M6: mixer mirror, undo granularity, meters.

    #[test]
    fn set_mixer_clamps_stores_and_reaches_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(state.mixer, MixerState::default());
        assert_eq!(
            state.set_mixer(Channel::Kick, ChannelParam::Level, 100.0),
            engine::mixer::MAX_LEVEL_DB
        );
        assert_eq!(
            state.set_mixer(Channel::Snare, ChannelParam::Level, -1000.0),
            engine::mixer::MIN_LEVEL_DB
        );
        assert_eq!(state.set_mixer(Channel::Clap, ChannelParam::Pan, 7.0), 1.0);
        assert_eq!(
            state.set_mixer(Channel::ClosedHat, ChannelParam::DelaySend, -0.5),
            0.0
        );
        assert_eq!(
            state.set_mixer(Channel::OpenHat, ChannelParam::Mute, 0.7),
            1.0
        );
        assert!(state.strip(Channel::OpenHat).mute);
        assert_eq!(
            state.set_mixer(Channel::Synth, ChannelParam::ReverbSend, f32::NAN),
            1.0,
            "non-finite resets to the channel default"
        );
        assert_eq!(
            state.set_mixer(Channel::Synth, ChannelParam::Solo, 1.0),
            1.0
        );
        assert!(state.strip(Channel::Synth).solo);
        assert!(state.dirty);

        engine.process_messages();
        assert_eq!(engine.mixer(), &state.mixer);
        assert_eq!(
            engine.mixer().get(Channel::Kick, ChannelParam::Level),
            engine::mixer::MAX_LEVEL_DB
        );
        assert!(engine.mixer().strip(Channel::OpenHat).mute);
        assert_eq!(
            engine.mixer().effective_gain(Channel::Kick),
            0.0,
            "synth solo"
        );

        // Every strip value is resent in one go: 36 messages.
        let messages = mixer_messages(&state.mixer);
        assert_eq!(messages.len(), Channel::COUNT * ChannelParam::ALL.len());
        assert_eq!(
            messages[0],
            Message::SetMixer {
                channel: Channel::Kick,
                param: ChannelParam::Level,
                value: engine::mixer::MAX_LEVEL_DB
            }
        );
        let (mut other, mut fresh) = AppState::headless(ThemeMode::Dark);
        other.mixer = state.mixer;
        let free_before = other.engine.free_slots();
        other.resend_mixer();
        assert_eq!(free_before - other.engine.free_slots(), 36);
        fresh.process_messages();
        assert_eq!(fresh.mixer(), &state.mixer);
    }

    #[test]
    fn mute_and_solo_are_discrete_undo_steps_and_a_fader_drag_coalesces() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.toggle_mute(Channel::Kick);
        state.toggle_mute(Channel::Kick);
        state.toggle_solo(Channel::Snare);
        assert!(!state.strip(Channel::Kick).mute);
        assert!(state.strip(Channel::Snare).solo);
        assert_eq!(state.undo.len(), 3, "three clicks, three steps");

        // A fader drag: many changes within the gap are one step.
        for v in 1..=20 {
            state.set_mixer(Channel::Clap, ChannelParam::Level, -(v as f32));
        }
        assert_eq!(state.undo.len(), 4);
        assert_eq!(state.strip(Channel::Clap).level_db, -20.0);
        // Another strip's fader is another control.
        state.set_mixer(Channel::OpenHat, ChannelParam::Level, -3.0);
        assert_eq!(state.undo.len(), 5);
        // A pan drag coalesces too, and `drag_started` cuts it.
        for v in 1..=5 {
            state.set_mixer(Channel::Kick, ChannelParam::Pan, v as f32 / 10.0);
        }
        assert_eq!(state.undo.len(), 6);
        state.edit_mixer(Channel::Kick, ChannelParam::Pan, -0.5, true);
        assert_eq!(state.undo.len(), 7);
        state.edit_mixer(Channel::Kick, ChannelParam::Pan, -0.6, false);
        assert_eq!(state.undo.len(), 7, "the drag continues");
        // A mute in the middle of a drag ends it.
        state.set_mixer(Channel::Synth, ChannelParam::ReverbSend, 0.5);
        state.toggle_mute(Channel::Synth);
        state.set_mixer(Channel::Synth, ChannelParam::ReverbSend, 0.4);
        assert_eq!(state.undo.len(), 10);
        // Setting the value a strip already has is not an edit.
        state.set_mixer(Channel::Synth, ChannelParam::ReverbSend, 0.4);
        state.toggle_mute(Channel::Synth);
        state.toggle_mute(Channel::Synth);
        assert_eq!(state.undo.len(), 12);

        engine.process_messages();
        assert_eq!(engine.mixer(), &state.mixer);

        // Undo walks back through the mixer and resends it: the synth ends muted (12), was
        // unmuted (11), had its send at 0.4 (10), was muted (9), had its send at 0.5 (8).
        assert!(state.strip(Channel::Synth).mute);
        assert!(state.undo());
        assert!(!state.strip(Channel::Synth).mute);
        assert!(state.undo());
        assert!(state.strip(Channel::Synth).mute);
        assert_eq!(state.strip(Channel::Synth).reverb_send, 0.4);
        assert!(state.undo());
        assert_eq!(state.strip(Channel::Synth).reverb_send, 0.5);
        assert!(state.undo());
        assert!(!state.strip(Channel::Synth).mute);
        assert!(state.undo());
        assert_eq!(state.strip(Channel::Synth).reverb_send, 1.0, "the default");
        assert_eq!(state.strip(Channel::Kick).pan, -0.6);
        assert!(state.undo()); // the -0.5 / -0.6 drag at once
        assert_eq!(state.strip(Channel::Kick).pan, 0.5);
        assert!(state.undo()); // the whole first pan drag
        assert_eq!(state.strip(Channel::Kick).pan, 0.0);
        assert_eq!(state.strip(Channel::OpenHat).level_db, -3.0);
        assert!(state.undo()); // open hat level
        assert_eq!(state.strip(Channel::OpenHat).level_db, 0.0);
        assert_eq!(state.strip(Channel::Clap).level_db, -20.0);
        assert!(state.undo()); // the whole clap drag at once
        assert_eq!(state.strip(Channel::Clap).level_db, 0.0);
        assert!(state.undo()); // snare solo
        assert!(!state.strip(Channel::Snare).solo);
        assert!(state.undo()); // kick unmute
        assert!(state.strip(Channel::Kick).mute);
        assert!(state.undo()); // kick mute
        assert!(!state.undo());
        assert_eq!(state.mixer, MixerState::default());
        sync(&mut state, &mut engine);
        assert_eq!(engine.mixer(), &MixerState::default());

        assert!(state.redo());
        assert!(state.strip(Channel::Kick).mute);
        sync(&mut state, &mut engine);
        assert!(engine.mixer().strip(Channel::Kick).mute);
    }

    #[test]
    fn meter_decay_replaces_peaks_and_falls_twenty_db_per_second() {
        // A new, louder peak replaces the shown level at once.
        assert_eq!(decay_meter(0.0, 0.5, 0.016), 0.5);
        assert_eq!(decay_meter(0.2, 0.8, 1.0), 0.8);
        // Without new signal the level falls 20 dB per second: -20 dB after 1 s (x 0.1),
        // -10 dB after 0.5 s, -1 dB after 50 ms.
        assert!((decay_meter(1.0, 0.0, 1.0) - 0.1).abs() < 1e-6);
        assert!((decay_meter(1.0, 0.0, 0.5) - 10f32.powf(-0.5)).abs() < 1e-6);
        assert!((decay_meter(1.0, 0.0, 0.05) - 10f32.powf(-0.05)).abs() < 1e-6);
        // A quieter new peak loses to the decaying one.
        let fallen = decay_meter(1.0, 0.0, 0.1);
        assert_eq!(decay_meter(1.0, 0.5, 0.1), fallen);
        // No time passed: nothing falls.
        assert_eq!(decay_meter(0.7, 0.0, 0.0), 0.7);
        // Bad input reads as silence; the tail snaps to exactly zero.
        assert_eq!(decay_meter(f32::NAN, f32::NAN, f32::NAN), 0.0);
        assert_eq!(decay_meter(-1.0, -1.0, 1.0), 0.0);
        assert_eq!(decay_meter(0.00005, 0.0, 0.0), 0.0);
        assert_eq!(decay_meter(1.0, 0.0, 10.0), 0.0, "-200 dB is silence");

        let mut shown = Meters::SILENT;
        let mut new = Meters::SILENT;
        new.channel[Channel::Snare.index()] = 0.9;
        new.master = [0.6, 0.3];
        decay_meters(&mut shown, &new, 0.0);
        assert_eq!(shown, new);
        assert!(meters_active(&shown));
        decay_meters(&mut shown, &Meters::SILENT, 1.0);
        assert!((shown.channel[Channel::Snare.index()] - 0.09).abs() < 1e-6);
        assert!((shown.master[0] - 0.06).abs() < 1e-6);
        assert_eq!(shown.channel[Channel::Kick.index()], 0.0);
        decay_meters(&mut shown, &Meters::SILENT, 5.0);
        assert!(!meters_active(&shown));

        // Through the state: the raw meters are replaced, the shown ones decay.
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(!state.meters_active());
        state.update_meters_with(new, 0.0);
        assert_eq!(state.meters, new);
        assert_eq!(state.shown_meters, new);
        assert!(state.meters_active());
        state.update_meters_with(Meters::SILENT, 1.0);
        assert_eq!(state.meters, Meters::SILENT);
        assert!((state.shown_meters.master[1] - 0.03).abs() < 1e-6);
        // The real path reads the engine (silent here) without panicking.
        state.update_meters();
        assert!(state.meters_updated.is_some());
    }

    #[test]
    fn rendered_audio_shows_up_in_the_meters() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_bank();
        state.play();
        engine.process_messages();
        // The first block holds the demo kick's attack on step 1.
        let mut out = vec![0.0f32; 1024 * 2];
        engine.render(&mut out, 1024);
        state.update_meters();
        assert!(
            state.meters.channel[Channel::Kick.index()] > 0.01,
            "the demo kick on step 1 peaks: {:?}",
            state.meters
        );
        assert!(state.meters.master_peak() > 0.01);
        assert!(state.meters_active());
    }

    // -----------------------------------------------------------------------------------------
    // M5: bank mirror, patterns, chain.

    /// Three non-empty patterns of 16 / 32 / 64 steps (with hidden data beyond a length), a
    /// four-entry chain and pattern C selected.
    fn m5_bank() -> Bank {
        let mut bank = Bank::default();
        bank.patterns[1] = Pattern::demo_variation();
        bank.set_length(1, PatternLength::Bar2);
        bank.patterns[1].set_drum_step(DrumRow::Kick, 20, true, true);
        bank.patterns[1].set_synth_step(31, Some(48), false);
        bank.patterns[1].drum_volume[2] = -4.0;
        bank.patterns[2] = Pattern::demo();
        bank.set_length(2, PatternLength::Bar4);
        bank.patterns[2].set_drum_step(DrumRow::Snare, 63, true, false);
        bank.patterns[2].set_synth_step(40, Some(50), true);
        bank.patterns[2].drum_tune[4] = 5.0;
        // Hidden data beyond the length survives a resend.
        bank.patterns[0].drums[DrumRow::Clap.index()][30].on = true;
        bank.truncate(0);
        for (p, r) in [(0, 2), (1, 1), (2, 3), (1, 8)] {
            bank.push(ChainEntry::new(p, r));
        }
        bank.current = 2;
        bank
    }

    #[test]
    fn send_bank_puts_the_whole_mirror_into_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.bank = m5_bank();
        assert_ne!(engine.bank(), &state.bank);
        let messages = bank_messages(&state.bank, None);
        assert!(
            messages.len() < engine::QUEUE_CAPACITY,
            "{}",
            messages.len()
        );
        assert_eq!(*messages.last().unwrap(), Message::SelectPattern(2));
        assert!(messages.contains(&Message::SetChainLen(4)));
        assert!(messages.contains(&Message::SetChainEntry {
            index: 3,
            pattern: 1,
            repeats: 8
        }));
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::SetPlayMode(_))),
            "the mode is unchanged: not resent"
        );
        // Pattern A has hidden data at step 30: it is sent at a widened length, then the real
        // length is restored.
        assert!(messages.contains(&Message::SetPatternLength {
            pattern: 0,
            steps: 32
        }));
        assert!(messages.contains(&Message::SetPatternLength {
            pattern: 0,
            steps: 16
        }));

        state.send_bank();
        assert!(state.outbox.is_empty());
        engine.process_messages();
        assert_eq!(engine.bank(), &state.bank);
        assert_eq!(engine.edit_pattern(), 2);
        assert_eq!(engine.pattern(), &state.bank.patterns[2]);
        assert_eq!(engine.bank().pattern(1).len(), 32);
        assert_eq!(engine.bank().pattern(2).len(), 64);
        assert!(engine.bank().pattern(0).drums[DrumRow::Clap.index()][30].on);

        // Resending over an engine that already holds data converges too (ClearPattern first).
        state.pattern_mut().clear_all();
        state.bank.truncate(1);
        state.send_bank();
        engine.process_messages();
        assert_eq!(engine.bank(), &state.bank);
    }

    #[test]
    fn a_dense_bank_overflows_the_queue_and_drains_through_the_outbox() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        for (i, pattern) in state.bank.patterns.iter_mut().enumerate() {
            pattern.set_length(PatternLength::Bar4);
            for row in DrumRow::ALL {
                for step in 0..64 {
                    pattern.set_drum_step(row, step as u8, true, step % 3 == 0);
                }
            }
            for step in 0..64u8 {
                pattern.set_synth_step(step, Some(36 + i as u8 + step % 12), step % 2 == 0);
            }
        }
        let messages = bank_messages(&state.bank, None);
        assert!(
            messages.len() > engine::QUEUE_CAPACITY,
            "{} messages",
            messages.len()
        );
        state.resend_all();
        assert!(!state.outbox.is_empty(), "the rest waits in the outbox");
        assert_eq!(
            state.engine.free_slots(),
            0,
            "the queue is full, nothing dropped"
        );
        let frames = sync(&mut state, &mut engine);
        assert!(frames > 1, "took {frames} frames");
        assert_eq!(engine.bank(), &state.bank);
        // Messages sent while the outbox drains stay in order behind it.
        state.resend_all();
        state.set_bpm(99.0);
        assert!(state.outbox.contains(&Message::SetBpm(99.0)));
        sync(&mut state, &mut engine);
        assert_eq!(engine.bpm(), 99.0);
        assert_eq!(engine.bank(), &state.bank);
    }

    #[test]
    fn selecting_a_tab_while_stopped_switches_the_edit_pattern_immediately() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_bank();
        engine.process_messages();
        state.click_synth_step(3, false);
        state.shown_bar = 0;
        state.select_pattern(1);
        assert_eq!(state.current(), 1);
        assert_eq!(state.current_letter(), 'B');
        assert!(state.pattern().is_empty(), "B is empty");
        assert_eq!(state.selected_step, None, "selection is dropped");
        assert!(
            !state.dirty && state.undo.is_empty(),
            "selecting is not an edit"
        );
        engine.process_messages();
        assert_eq!(engine.edit_pattern(), 1);
        assert_eq!(engine.bank().current, 1);
        assert_eq!(
            engine.playing_pattern(),
            1,
            "stopped: the switch is immediate"
        );
        assert_eq!(engine.queued_pattern(), None);

        // Edits now land in B; A is untouched.
        state.toggle_drum_step(DrumRow::Kick, 0, false);
        assert!(state.bank.patterns[1].drums[0][0].on);
        assert_eq!(state.bank.patterns[0], Pattern::demo());
        engine.process_messages();
        assert_eq!(engine.bank(), &state.bank);

        // Playing in pattern mode: the switch is queued to the bar boundary.
        state.play();
        engine.process_messages();
        state.select_pattern(0);
        engine.process_messages();
        assert_eq!(engine.playing_pattern(), 1);
        assert_eq!(engine.queued_pattern(), Some(0));
        // The position (queued letter included) is published per rendered block.
        let mut out = vec![0.0f32; 64 * 2];
        engine.render(&mut out, 64);
        state.update_playhead();
        assert_eq!(state.playhead.queued, Some(0));
        assert_eq!(state.playhead.pattern, 1);
        assert_eq!(state.current(), 0, "editing A already");
        // Selecting the playing pattern again cancels the queue.
        state.select_pattern(1);
        engine.process_messages();
        assert_eq!(engine.queued_pattern(), None);
        assert!(queued_blink_visible(0.0));
        assert!(!queued_blink_visible(0.3));
        assert!(queued_blink_visible(0.55));
    }

    #[test]
    fn copy_paste_and_clear_are_undoable() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_bank();
        engine.process_messages();
        assert!(!state.paste_pattern(), "nothing copied yet");
        assert!(state.undo.is_empty());

        state.copy_pattern();
        assert_eq!(state.clipboard.as_ref(), Some(&Pattern::demo()));
        assert!(!state.dirty, "copying is not an edit");
        state.select_pattern(3);
        assert!(state.paste_pattern());
        assert_eq!(state.bank.patterns[3], Pattern::demo());
        assert_eq!(state.undo.len(), 1);
        assert!(state.dirty);
        engine.process_messages();
        assert_eq!(engine.bank(), &state.bank);
        assert!(
            state.paste_pattern(),
            "pasting the same again is a no-op edit"
        );
        assert_eq!(state.undo.len(), 1);

        state.clear_pattern();
        assert!(state.bank.patterns[3].is_empty());
        assert_eq!(state.undo.len(), 2);
        state.clear_pattern();
        assert_eq!(
            state.undo.len(),
            2,
            "clearing an empty pattern is not an edit"
        );
        engine.process_messages();
        assert_eq!(engine.bank(), &state.bank);

        assert!(state.undo());
        assert_eq!(state.bank.patterns[3], Pattern::demo());
        assert!(state.undo());
        assert!(state.bank.patterns[3].is_empty());
        assert_eq!(state.current(), 3, "the snapshot keeps the selection");
        assert!(state.redo());
        assert_eq!(state.bank.patterns[3], Pattern::demo());
        sync(&mut state, &mut engine);
        assert_eq!(engine.bank(), &state.bank);
    }

    #[test]
    fn pattern_length_and_absolute_steps_for_bar_two() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_bank();
        engine.process_messages();
        // A one-bar pattern wraps step 17 to step 1.
        state.toggle_drum_step(DrumRow::Clap, 17, false);
        assert!(state.pattern().drums[DrumRow::Clap.index()][1].on);
        state.toggle_drum_step(DrumRow::Clap, 17, false);

        state.set_pattern_length(PatternLength::Bar2);
        assert_eq!(state.pattern().len(), 32);
        assert_eq!(state.pattern().bars(), 2);
        assert_eq!(state.undo.len(), 3, "two toggles and the length");
        state.set_pattern_length(PatternLength::Bar2);
        assert_eq!(state.undo.len(), 3, "same length: no edit");
        engine.process_messages();
        assert_eq!(engine.pattern().len(), 32);

        // Bar 2, column 1 = absolute step 17.
        state.show_bar(1);
        assert_eq!(state.shown_bar, 1);
        assert!(!state.follow, "picking a bar stops following");
        state.toggle_drum_step(DrumRow::Clap, 17, false);
        assert!(state.pattern().drums[DrumRow::Clap.index()][17].on);
        assert!(!state.pattern().drums[DrumRow::Clap.index()][1].on);
        state.click_synth_step(20, false);
        assert_eq!(state.selected_step, Some(20));
        state.note_on(60, 0.8);
        state.note_off(60);
        assert_eq!(state.pattern().synth[20].note, Some(60));
        engine.process_messages();
        assert_eq!(engine.pattern(), state.pattern());
        assert!(engine.pattern().drums[DrumRow::Clap.index()][17].on);

        // Recording quantises into bar 2 as well.
        state.selected_step = None;
        state.play();
        state.toggle_arm();
        state.playhead = Position {
            bar: 1,
            ..at(5, 0.6)
        };
        state.note_on(62, 1.0);
        assert_eq!(state.pattern().synth[22].note, Some(62));
        state.playhead = Position {
            bar: 1,
            ..at(15, 0.7)
        };
        state.note_on(64, 0.5);
        assert_eq!(
            state.pattern().synth[0].note,
            Some(64),
            "wraps at the length"
        );

        // Shortening hides bar 2 (data kept) and clamps the shown bar and the selection.
        state.selected_step = Some(20);
        state.set_pattern_length(PatternLength::Bar1);
        assert_eq!(state.shown_bar, 0);
        assert_eq!(state.selected_step, None);
        assert!(state.pattern().drums[DrumRow::Clap.index()][17].on);
        assert_eq!(state.pattern().used_steps(), 32);
        sync(&mut state, &mut engine);
        assert_eq!(engine.bank(), &state.bank);
        assert_eq!(quantise_step(at(15, 0.5), 16), 0);
        assert_eq!(
            quantise_step(
                Position {
                    bar: 1,
                    ..at(15, 0.5)
                },
                32
            ),
            0
        );
        assert_eq!(
            quantise_step(
                Position {
                    bar: 1,
                    ..at(2, 0.1)
                },
                32
            ),
            18
        );
    }

    #[test]
    fn follow_moves_the_shown_bar_with_the_playing_pattern_only() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.set_pattern_length(PatternLength::Bar4);
        state.send_bank();
        engine.process_messages();
        assert!(state.follow);
        // Play one full bar: the engine is in bar 1.
        state.play();
        engine.process_messages();
        let mut out = vec![0.0f32; 1024 * 2];
        let bar = engine::export::bar_frames(DEFAULT_BPM, 48_000.0);
        let mut rendered = 0;
        while rendered < bar + 512 {
            engine.render(&mut out, 1024);
            rendered += 1024;
        }
        state.update_playhead();
        assert_eq!(state.playhead.bar, 1);
        assert_eq!(state.shown_bar, 1, "following");
        // Another pattern is being edited: the grid stays put.
        state.select_pattern(1);
        state.show_bar(0);
        state.follow = true;
        state.update_playhead();
        assert_eq!(state.shown_bar, 0);
        assert_eq!(state.position_text(), "A · bar 2/4");
        state.stop();
    }

    #[test]
    fn chain_edits_reach_the_engine_and_undo() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_bank();
        engine.process_messages();
        assert_eq!(state.chain(), &[ChainEntry::new(0, 1)]);
        assert_eq!(state.selected_entry(), Some((0, ChainEntry::new(0, 1))));

        state.select_pattern(1);
        assert!(state.add_chain_entry());
        assert_eq!(state.chain().len(), 2);
        assert_eq!(state.song_selected, Some(1));
        assert_eq!(state.chain()[1], ChainEntry::new(1, 1));
        state.set_chain_repeats(3);
        state.set_chain_repeats(3);
        state.set_chain_pattern(2);
        assert_eq!(state.chain()[1], ChainEntry::new(2, 3));
        assert_eq!(state.undo.len(), 3, "add, repeats, pattern");
        engine.process_messages();
        assert_eq!(engine.bank().chain_entries(), state.chain());

        assert!(state.move_chain_entry(-1));
        assert_eq!(state.chain()[0], ChainEntry::new(2, 3));
        assert_eq!(state.song_selected, Some(0));
        assert!(!state.move_chain_entry(-1), "already first");
        assert!(state.move_chain_entry(1));
        assert!(!state.move_chain_entry(1), "already last");
        engine.process_messages();
        assert_eq!(engine.bank().chain_entries(), state.chain());

        assert!(state.remove_chain_entry());
        assert_eq!(state.chain(), &[ChainEntry::new(0, 1)]);
        assert_eq!(state.song_selected, Some(0));
        assert!(state.remove_chain_entry());
        assert!(state.chain().is_empty());
        assert_eq!(state.song_selected, None);
        assert!(!state.remove_chain_entry());
        engine.process_messages();
        assert_eq!(engine.bank().chain_len, 0);

        assert!(state.undo());
        assert_eq!(state.chain(), &[ChainEntry::new(0, 1)]);
        assert_eq!(state.song_selected, Some(0), "selection comes back");
        sync(&mut state, &mut engine);
        assert_eq!(engine.bank(), &state.bank);

        // A full chain refuses another entry.
        for _ in 0..70 {
            state.add_chain_entry();
        }
        assert_eq!(state.chain().len(), MAX_CHAIN);
        assert!(!state.add_chain_entry());
        sync(&mut state, &mut engine);
        assert_eq!(engine.bank(), &state.bank);
    }

    #[test]
    fn play_mode_switches_the_engine_the_tab_and_the_position_text() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.bank = Bank::demo();
        state.send_bank();
        engine.process_messages();
        assert_eq!(state.right_tab, RightTab::Project);
        assert_eq!(state.position_text(), "A · bar 1/1");

        state.set_play_mode(PlayMode::Song);
        assert_eq!(state.bank.mode, PlayMode::Song);
        assert_eq!(state.right_tab, RightTab::Chain);
        assert!(!state.dirty, "the mode is transport state");
        engine.process_messages();
        assert_eq!(engine.play_mode(), PlayMode::Song);
        state.update_playhead();
        assert_eq!(state.position_text(), "SONG 1/4 · A");

        // A resend does not send the mode again (it would restart a running song).
        let messages = bank_messages(&state.bank, None);
        let free_before = state.engine.free_slots();
        state.send_bank();
        assert_eq!(free_before - state.engine.free_slots(), messages.len());
        // ... unless the mirror's mode differs from the engine's (undo of a project load).
        state.bank.mode = PlayMode::Pattern;
        let free_before = state.engine.free_slots();
        state.send_bank();
        assert_eq!(free_before - state.engine.free_slots(), messages.len() + 1);
        engine.process_messages();
        assert_eq!(engine.play_mode(), PlayMode::Pattern);
        assert_eq!(state.engine_mode, PlayMode::Pattern);

        state.set_play_mode(PlayMode::Pattern);
        assert_eq!(state.right_tab, RightTab::Project);
    }

    #[test]
    fn drum_clicks_toggle_and_shift_clicks_accent() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_pattern();
        let kick = DrumRow::Kick.index();
        // Demo: kick step 1 is off.
        assert!(!state.pattern().drums[kick][1].on);
        state.toggle_drum_step(DrumRow::Kick, 1, false);
        assert_eq!(
            state.pattern().drums[kick][1],
            DrumStep {
                on: true,
                accent: false
            }
        );
        // Shift on an unaccented step: stays on, gains the accent.
        state.toggle_drum_step(DrumRow::Kick, 1, true);
        assert_eq!(
            state.pattern().drums[kick][1],
            DrumStep {
                on: true,
                accent: true
            }
        );
        // Shift again: still on, accent off.
        state.toggle_drum_step(DrumRow::Kick, 1, true);
        assert_eq!(
            state.pattern().drums[kick][1],
            DrumStep {
                on: true,
                accent: false
            }
        );
        // Shift on an off step turns it on, accented.
        state.toggle_drum_step(DrumRow::Snare, 2, true);
        assert_eq!(
            state.pattern().drums[DrumRow::Snare.index()][2],
            DrumStep {
                on: true,
                accent: true
            }
        );
        // A plain click on an accented step turns it off and drops the accent.
        state.toggle_drum_step(DrumRow::Snare, 2, false);
        assert_eq!(
            state.pattern().drums[DrumRow::Snare.index()][2],
            DrumStep::default()
        );
        // Step indices wrap like the engine's.
        state.toggle_drum_step(DrumRow::Clap, 17, false);
        assert!(state.pattern().drums[DrumRow::Clap.index()][1].on);

        engine.process_messages();
        assert_eq!(engine.pattern(), state.pattern());
    }

    #[test]
    fn bass_clicks_select_and_shift_clicks_accent_or_add_a_note() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_pattern();
        state.click_synth_step(0, false);
        assert_eq!(state.selected_step, Some(0));
        state.click_synth_step(5, false);
        assert_eq!(state.selected_step, Some(5));
        state.click_synth_step(5, false);
        assert_eq!(state.selected_step, None);

        // Demo step 0 has C2; shift flips its accent and leaves the selection alone.
        assert_eq!(state.pattern().synth[0].note, Some(36));
        state.click_synth_step(0, true);
        assert_eq!(
            state.pattern().synth[0],
            SynthStep {
                note: Some(36),
                accent: true
            }
        );
        state.click_synth_step(0, true);
        assert!(!state.pattern().synth[0].accent);
        assert_eq!(state.selected_step, None);

        // Demo step 1 is a rest; shift gives it the keyboard's C (octave 4 = 60).
        assert_eq!(state.pattern().synth[1].note, None);
        state.click_synth_step(1, true);
        assert_eq!(
            state.pattern().synth[1],
            SynthStep {
                note: Some(60),
                accent: false
            }
        );

        engine.process_messages();
        assert_eq!(engine.pattern(), state.pattern());
    }

    #[test]
    fn a_selected_step_takes_the_next_note_and_delete_clears_it() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_pattern();
        state.click_synth_step(3, false);
        state.note_on(67, 0.8);
        // The note sounds live and lands in the step; the selection stays put.
        assert!(state.held_notes[67]);
        assert_eq!(state.note_on_count, 1);
        assert_eq!(state.pattern().synth[3].note, Some(67));
        assert!(!state.pattern().synth[3].accent);
        assert_eq!(state.selected_step, Some(3));
        state.note_off(67);

        // A second note replaces it (accent untouched).
        state.set_synth_step(3, Some(67), true);
        state.note_on(69, 1.0);
        assert_eq!(
            state.pattern().synth[3],
            SynthStep {
                note: Some(69),
                accent: true
            }
        );

        state.clear_selected_step();
        assert_eq!(state.pattern().synth[3], SynthStep::default());
        assert_eq!(state.selected_step, Some(3));

        engine.process_messages();
        assert_eq!(engine.pattern(), state.pattern());
    }

    #[test]
    fn record_arm_quantises_note_ons_into_the_bass_lane() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.clear_synth();
        // Not playing: nothing is recorded even when armed.
        state.toggle_arm();
        state.playhead = at(2, 0.1);
        state.note_on(60, 0.8);
        assert!(state.pattern().synth.iter().all(|s| s.note.is_none()));
        state.note_off(60);

        state.play();
        state.playhead = at(2, 0.1);
        state.note_on(60, 0.8);
        assert_eq!(
            state.pattern().synth[2],
            SynthStep {
                note: Some(60),
                accent: false
            }
        );
        state.note_off(60);

        // Late in the step: the next one; velocity >= 0.95 accents.
        state.playhead = at(2, 0.6);
        state.note_on(62, 1.0);
        assert_eq!(
            state.pattern().synth[3],
            SynthStep {
                note: Some(62),
                accent: true
            }
        );
        state.note_off(62);

        // Wraps at the end of the bar.
        state.playhead = at(15, 0.75);
        state.note_on(64, 0.95);
        assert_eq!(
            state.pattern().synth[0],
            SynthStep {
                note: Some(64),
                accent: true
            }
        );
        // The notes still sounded live.
        assert_eq!(state.note_on_count, 4);

        // Playing but not armed: no recording.
        state.toggle_arm();
        state.playhead = at(8, 0.0);
        state.note_on(65, 0.8);
        assert_eq!(state.pattern().synth[8].note, None);
    }

    #[test]
    fn clear_row_synth_and_all_reach_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.set_drum_volume(DrumRow::Kick, -3.0);
        state.set_drum_tune(DrumRow::Kick, 99.0);
        assert_eq!(state.pattern().drum_tune[DrumRow::Kick.index()], 12.0);

        state.clear_row(DrumRow::Kick);
        assert!(
            state.pattern().drums[DrumRow::Kick.index()]
                .iter()
                .all(|s| !s.on)
        );
        assert!(
            state.pattern().drums[DrumRow::Snare.index()]
                .iter()
                .any(|s| s.on)
        );
        // Volume and tune survive a row clear.
        assert_eq!(state.pattern().drum_volume[DrumRow::Kick.index()], -3.0);

        state.clear_synth();
        assert!(state.pattern().synth.iter().all(|s| s.note.is_none()));
        assert!(!state.pattern().is_empty());

        state.clear_all();
        assert!(state.pattern().is_empty());

        engine.process_messages();
        assert_eq!(engine.pattern(), state.pattern());
        assert_eq!(engine.pattern().drum_volume[DrumRow::Kick.index()], -3.0);
        assert_eq!(engine.pattern().drum_tune[DrumRow::Kick.index()], 12.0);
    }

    // -----------------------------------------------------------------------------------------
    // Piano roll (M7): notes in the mirror and the engine.

    /// A chord (C E G at step 0, four steps long), a six-step note at step 8 and a hidden
    /// note at step 20 of a one-bar pattern.
    fn chord_pattern() -> Pattern {
        let mut p = Pattern::empty();
        p.add_note(Note::new(0, 4, 48, 0.8));
        p.add_note(Note::new(0, 4, 52, 0.8));
        p.add_note(Note::new(0, 4, 55, 1.0));
        p.add_note(Note::new(8, 6, 43, 0.8));
        // Hidden: widen, add, shrink (the note stays in the list beyond the length).
        p.set_length(PatternLength::Bar2);
        p.add_note(Note::new(20, 2, 60, 0.8));
        p.set_length(PatternLength::Bar1);
        assert_eq!(p.notes().len(), 5);
        p
    }

    #[test]
    fn pattern_messages_replay_chords_long_and_hidden_notes_into_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.bank.patterns[2] = chord_pattern();
        let messages = pattern_messages(&state.bank.patterns[2], 2);
        assert_eq!(
            messages
                .iter()
                .filter(|m| matches!(m, Message::SetNote { pattern: 2, .. }))
                .count(),
            5
        );
        assert!(messages.contains(&Message::ClearNotes(2)));
        // The hidden note is sent while the length is widened to two bars.
        let widened = messages
            .iter()
            .position(|m| {
                *m == Message::SetPatternLength {
                    pattern: 2,
                    steps: 32,
                }
            })
            .unwrap();
        let restored = messages
            .iter()
            .position(|m| {
                *m == Message::SetPatternLength {
                    pattern: 2,
                    steps: 16,
                }
            })
            .unwrap();
        let last_note = messages
            .iter()
            .rposition(|m| matches!(m, Message::SetNote { .. }))
            .unwrap();
        assert!(widened < last_note && last_note < restored);

        state.send_bank();
        sync(&mut state, &mut engine);
        assert_eq!(
            engine.bank().patterns[2].notes(),
            state.bank.patterns[2].notes()
        );
        assert_eq!(engine.bank().patterns[2], state.bank.patterns[2]);
        assert_eq!(engine.bank(), &state.bank);
        // The derived lane agrees on both sides: the lowest chord note at step 0.
        assert_eq!(engine.bank().patterns[2].synth[0].note, Some(48));
        assert_eq!(engine.bank().patterns[2].synth[8].note, Some(43));
    }

    #[test]
    fn note_edits_reach_the_engine_and_are_undoable_and_dirty() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.clear_synth();
        state.send_pattern();
        sync(&mut state, &mut engine);
        state.undo.clear();
        state.dirty = false;

        // Add a chord: each add is one undo step, the list stays sorted by start then pitch.
        assert_eq!(state.add_note(Note::new(4, 2, 55, 0.8)), Some(0));
        assert_eq!(
            state.add_note(Note::new(4, 2, 48, 0.8)),
            Some(0),
            "sorted below"
        );
        assert_eq!(state.add_note(Note::new(0, 8, 36, 1.0)), Some(0));
        assert_eq!(state.undo.len(), 3);
        assert!(state.dirty);
        assert_eq!(
            state.pattern().notes(),
            &[
                Note::new(0, 8, 36, 1.0),
                Note::new(4, 2, 48, 0.8),
                Note::new(4, 2, 55, 0.8)
            ]
        );
        // The lane is the derived view: lowest note per start step, accent at 1.0.
        assert_eq!(state.pattern().synth[0].note, Some(36));
        assert!(state.pattern().synth[0].accent);
        assert_eq!(state.pattern().synth[4].note, Some(48));
        sync(&mut state, &mut engine);
        assert_eq!(engine.pattern().notes(), state.pattern().notes());

        // Move the long note up a fifth and shorten it: it re-sorts behind the chord.
        assert_eq!(state.set_note(0, Note::new(6, 3, 43, 1.0)), Some(2));
        assert_eq!(state.undo.len(), 4);
        assert_eq!(state.pattern().notes()[2], Note::new(6, 3, 43, 1.0));
        // Setting a note to itself is not an edit; out-of-range indices are refused.
        assert_eq!(state.set_note(2, Note::new(6, 3, 43, 1.0)), Some(2));
        assert_eq!(state.set_note(9, Note::new(1, 1, 60, 0.8)), None);
        assert_eq!(
            state.set_note(2, Note::new(40, 1, 60, 0.8)),
            None,
            "beyond the bar"
        );
        assert_eq!(state.undo.len(), 4);
        sync(&mut state, &mut engine);
        assert_eq!(engine.pattern().notes(), state.pattern().notes());

        // Remove the middle chord note; a selection past it shifts down.
        state.roll_selected = Some(2);
        assert!(state.remove_note(1));
        assert_eq!(state.roll_selected, Some(1));
        assert!(!state.remove_note(5));
        assert_eq!(state.undo.len(), 5);
        assert_eq!(state.pattern().notes().len(), 2);
        sync(&mut state, &mut engine);
        assert_eq!(engine.pattern().notes(), state.pattern().notes());

        // Undo walks back through every step, the engine following.
        assert!(state.undo());
        assert_eq!(state.pattern().notes().len(), 3);
        assert!(state.undo());
        assert_eq!(state.pattern().notes()[0], Note::new(0, 8, 36, 1.0));
        sync(&mut state, &mut engine);
        assert_eq!(engine.pattern().notes(), state.pattern().notes());
        assert!(state.redo());
        assert_eq!(state.pattern().notes()[2], Note::new(6, 3, 43, 1.0));

        // Clear: one step while there is something to clear, none afterwards.
        let before = state.undo.len();
        state.clear_notes();
        assert!(state.pattern().notes().is_empty());
        assert_eq!(state.undo.len(), before + 1);
        state.clear_notes();
        assert_eq!(state.undo.len(), before + 1);
        sync(&mut state, &mut engine);
        assert!(engine.pattern().notes().is_empty());

        // A full pattern refuses a 65th note without an undo step.
        for i in 0..MAX_NOTES {
            assert!(
                state
                    .add_note(Note::new((i % 16) as u8, 1, 30 + i as u8, 0.8))
                    .is_some()
            );
        }
        let full = state.undo.len();
        assert_eq!(state.add_note(Note::new(0, 1, 100, 0.8)), None);
        assert_eq!(state.undo.len(), full);
    }

    #[test]
    fn set_synth_step_still_upserts_a_length_one_note_under_the_lane() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_pattern();
        state.clear_synth();
        state.add_note(Note::new(3, 4, 48, 0.8));
        state.add_note(Note::new(3, 4, 52, 0.8));
        // The grid writes one note per step: both chord notes at step 3 give way to a
        // length-1 accented note, the lane shows it, and the engine agrees.
        state.set_synth_step(3, Some(60), true);
        assert_eq!(state.pattern().notes(), &[Note::step(3, 60, true)]);
        assert_eq!(
            state.pattern().synth[3],
            SynthStep {
                note: Some(60),
                accent: true
            }
        );
        state.set_synth_step(5, Some(43), false);
        assert_eq!(state.pattern().notes().len(), 2);
        assert_eq!(state.pattern().synth[5].note, Some(43));
        state.set_synth_step(3, None, false);
        assert_eq!(state.pattern().notes(), &[Note::step(5, 43, false)]);
        assert_eq!(state.pattern().synth[3], SynthStep::default());
        sync(&mut state, &mut engine);
        assert_eq!(engine.pattern(), state.pattern());
        // Every lane cell is what its notes say.
        let mut rebuilt = state.pattern().clone();
        rebuilt.rebuild_lane();
        assert_eq!(rebuilt.synth, state.pattern().synth);
    }

    #[test]
    fn lane_view_and_roll_playhead() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(state.lane_view, LaneView::Grid);
        assert_eq!(state.roll_low_pitch, DEFAULT_ROLL_LOW_PITCH);
        assert_eq!(state.roll_playhead(), None, "stopped");

        // Switching to the roll drops the grid selection and is not an edit; the keyboard
        // then only plays (no note lands in the pattern).
        state.click_synth_step(2, false);
        state.set_lane_view(LaneView::Roll);
        assert_eq!(state.selected_step, None);
        assert!(!state.dirty);
        let demo = Pattern::demo();
        state.note_on(67, 0.8);
        state.note_off(67);
        assert_eq!(state.pattern().clone(), demo);
        assert_eq!(state.note_on_count, 1, "it still sounds");
        // ... unless record armed and playing: overdub writes a length-1 note as before.
        state.play();
        state.toggle_arm();
        state.playhead = at(5, 0.1);
        state.note_on(60, 1.0);
        assert_eq!(
            state.pattern().synth[5],
            Note::step(5, 60, true).lane_step()
        );
        assert!(state.pattern().note_at(5, 60).is_some());

        // The playhead in roll steps: bar * 16 + step + frac while the edit pattern plays.
        state.playhead = Position {
            bar: 1,
            ..at(3, 0.5)
        };
        assert_eq!(state.roll_playhead(), Some(19.5));
        state.playhead.pattern = 4;
        assert_eq!(state.roll_playhead(), None, "another pattern is playing");
        state.playhead = Position::stopped(0);
        assert_eq!(state.roll_playhead(), None);

        // Back to the grid drops the roll selection; the bottom pitch is clamped to MIDI.
        state.roll_selected = Some(0);
        state.set_lane_view(LaneView::Grid);
        assert_eq!(state.roll_selected, None);
        state.set_roll_low_pitch(200);
        assert_eq!(state.roll_low_pitch, 127);
        // Selecting another pattern or undoing past the notes clears a stale selection.
        state.roll_selected = Some(0);
        state.select_pattern(3);
        assert_eq!(state.roll_selected, None);
        state.select_pattern(0);
        state.roll_selected = Some(state.pattern().notes().len() - 1);
        state.clear_notes();
        assert_eq!(state.roll_selected, None);
    }

    #[test]
    fn roll_extra_rows_and_armed_chord_are_view_state() {
        use crate::chords::CHORDS;
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(state.roll_extra_rows, 0);
        state.set_roll_extra_rows(5);
        assert_eq!(state.roll_extra_rows, 5);
        state.set_roll_extra_rows(usize::MAX);
        assert_eq!(state.roll_extra_rows, MAX_ROLL_EXTRA_ROWS);

        assert_eq!(state.armed_chord, None);
        state.arm_chord(Some(1));
        assert_eq!(state.armed_chord, Some(1));
        state.arm_chord(Some(1));
        assert_eq!(state.armed_chord, None, "arming the armed chord disarms it");
        state.arm_chord(Some(0));
        state.arm_chord(Some(2));
        assert_eq!(state.armed_chord, Some(2), "another chord takes over");
        state.arm_chord(Some(CHORDS.len()));
        assert_eq!(state.armed_chord, None, "out of range disarms");
        state.arm_chord(Some(0));
        state.arm_chord(None);
        assert_eq!(state.armed_chord, None);
        assert!(!state.dirty, "none of this is an edit");
        assert_eq!(state.undo.len(), 0);
    }

    #[test]
    fn add_chord_places_every_tone_as_one_undo_step() {
        use crate::chords::CHORDS;
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.clear_notes();
        state.undo.clear();
        state.dirty = false;
        let major = &CHORDS[0];
        assert_eq!(state.add_chord(major, 48, 2, 3, 0.8), 3);
        let notes = state.pattern().notes().to_vec();
        assert_eq!(notes.len(), 3);
        for (n, pitch) in notes.iter().zip([48, 52, 55]) {
            assert_eq!((n.start, n.length, n.pitch), (2, 3, pitch), "{n:?}");
        }
        assert!(state.dirty);
        assert_eq!(state.undo.len(), 1, "one undo step for the whole chord");

        // The root and the fifth are there already: minor adds only its third.
        assert_eq!(state.add_chord(&CHORDS[1], 48, 2, 1, 0.8), 1);
        assert_eq!(state.pattern().notes().len(), 4);
        assert_eq!(state.undo.len(), 2);
        // Nothing to add, zero length or past the end: no edit.
        assert_eq!(state.add_chord(major, 48, 2, 1, 0.8), 0);
        assert_eq!(state.add_chord(major, 60, 0, 0, 0.8), 0);
        assert_eq!(state.add_chord(major, 60, 16, 1, 0.8), 0);
        assert_eq!(state.undo.len(), 2);
        // Tones above MIDI 127 are dropped.
        assert_eq!(state.add_chord(major, 125, 5, 1, 0.8), 1);
        assert_eq!(state.pattern().notes().len(), 5);

        assert!(state.undo());
        assert!(state.undo());
        assert!(state.undo());
        assert!(state.pattern().notes().is_empty());
        assert!(!state.undo());
    }

    #[test]
    fn move_and_remove_notes_are_one_undo_step_each() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.clear_notes();
        state.undo.clear();
        state.dirty = false;
        state.add_note(Note::new(0, 1, 48, 0.8));
        state.add_note(Note::new(2, 2, 52, 0.8));
        state.add_note(Note::new(4, 1, 55, 0.8));
        state.undo.clear();

        // Two notes move up an octave; the third stays. The list is re-sorted.
        let moved =
            state.move_notes(&[(0, Note::new(0, 1, 60, 0.8)), (2, Note::new(4, 1, 67, 0.8))]);
        assert_eq!(moved, 2);
        assert_eq!(
            state.pattern().notes(),
            &[
                Note::new(0, 1, 60, 0.8),
                Note::new(2, 2, 52, 0.8),
                Note::new(4, 1, 67, 0.8)
            ]
        );
        assert_eq!(state.undo.len(), 1);
        assert!(state.dirty);
        // Unchanged, out of range or invalid edits are no edit at all.
        assert_eq!(state.move_notes(&[(1, Note::new(2, 2, 52, 0.8))]), 0);
        assert_eq!(state.move_notes(&[(9, Note::new(2, 2, 52, 0.8))]), 0);
        assert_eq!(state.move_notes(&[(1, Note::new(16, 2, 52, 0.8))]), 0);
        assert_eq!(state.undo.len(), 1);

        // Removing two at once (an out-of-range index ignored) drops the selection.
        state.roll_selected = Some(0);
        state.roll_selection = vec![0, 2];
        assert_eq!(state.remove_notes(&[2, 0, 0, 9]), 2);
        assert_eq!(state.pattern().notes(), &[Note::new(2, 2, 52, 0.8)]);
        assert_eq!(state.roll_selected, None);
        assert!(state.roll_selection.is_empty());
        assert_eq!(state.undo.len(), 2);
        assert_eq!(state.remove_notes(&[5]), 0, "nothing in range");
        assert!(state.undo());
        assert_eq!(state.pattern().notes().len(), 3);
        assert!(state.undo());
        assert_eq!(state.pattern().notes()[0].pitch, 48);

        // A single removal shifts the rest of the selection down like the primary.
        state.roll_selected = Some(2);
        state.roll_selection = vec![0, 1, 2];
        state.remove_note(1);
        assert_eq!(state.roll_selected, Some(1));
        assert_eq!(state.roll_selection, [0, 1]);
        // Switching pattern or view, Escape and clear_notes drop the whole selection.
        state.select_pattern(1);
        assert!(state.roll_selection.is_empty() && state.roll_selected.is_none());
        state.select_pattern(0);
        state.roll_selection = vec![0];
        state.set_lane_view(LaneView::Grid);
        assert!(state.roll_selection.is_empty());
        state.set_lane_view(LaneView::Roll);
        state.roll_selection = vec![0, 1];
        state.clear_notes();
        assert!(state.roll_selection.is_empty());

        assert!(state.roll_black_keys, "black keys shown by default");
        state.toggle_black_keys();
        assert!(!state.roll_black_keys);
    }

    // -----------------------------------------------------------------------------------------
    // ---- MIDI ----

    use std::sync::mpsc::sync_channel;

    /// A headless state with a fake MIDI thread: `tx` plays the midir callback's UI half.
    fn midi_state() -> (AppState, engine::Engine, SyncSender<NoteEvent>) {
        let (mut state, engine) = AppState::headless(ThemeMode::Dark);
        let (tx, rx) = sync_channel(UI_QUEUE_CAPACITY);
        state.midi_rx = Some(rx);
        (state, engine, tx)
    }

    #[test]
    fn headless_state_owns_the_midi_queue_handle() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(state.midi_handle.is_some());
        assert!(
            state.engine.take_midi_handle().is_none(),
            "taken exactly once"
        );
        assert!(state.midi_enabled);
        assert!(!state.midi_connected());
        assert_eq!(state.midi_port_name(), None);
        assert_eq!(state.poll_midi(), 0, "no receiver: nothing to drain");
    }

    #[test]
    fn midi_events_light_keys_without_a_second_engine_message() {
        let (mut state, mut engine, tx) = midi_state();
        let free_before = state.engine.free_slots();
        tx.try_send(NoteEvent::On {
            note: 60,
            velocity: 0.8,
        })
        .unwrap();
        assert_eq!(state.poll_midi(), 1);
        assert!(state.held_notes[60], "the on-screen key lights");
        assert_eq!(state.midi_note_on_count, 1);
        assert_eq!(state.note_on_count, 0, "no local note-on was counted");
        assert_eq!(
            state.engine.free_slots(),
            free_before,
            "the UI queue must not carry a duplicate NoteOn"
        );
        engine.process_messages();
        assert_eq!(engine.voices().held_voices(), 0);

        // The MIDI thread's half: the same note through the MIDI queue sounds exactly once.
        let midi = state.midi_handle.as_mut().unwrap();
        assert!(midi.send(Message::NoteOn {
            note: 60,
            velocity: 0.8
        }));
        engine.process_messages();
        assert_eq!(engine.voices().held_voices(), 1);

        tx.try_send(NoteEvent::Off { note: 60 }).unwrap();
        assert_eq!(state.poll_midi(), 1);
        assert!(!state.held_notes[60]);
        assert_eq!(state.midi_note_off_count, 1);
        assert_eq!(state.note_off_count, 0);
        assert_eq!(state.engine.free_slots(), free_before);
        engine.process_messages();
        assert_eq!(
            engine.voices().held_voices(),
            1,
            "the UI path never released it"
        );
        state
            .midi_handle
            .as_mut()
            .unwrap()
            .send(Message::NoteOff { note: 60 });
        engine.process_messages();
        assert_eq!(engine.voices().held_voices(), 0);
    }

    #[test]
    fn midi_note_fills_the_selected_bass_step() {
        let (mut state, mut engine, tx) = midi_state();
        state.send_pattern();
        engine.process_messages();
        state.click_synth_step(3, false);
        let free_before = state.engine.free_slots();
        tx.try_send(NoteEvent::On {
            note: 67,
            velocity: 0.8,
        })
        .unwrap();
        assert_eq!(state.poll_midi(), 1);
        assert_eq!(state.pattern().synth[3].note, Some(67));
        assert_eq!(state.selected_step, Some(3));
        assert!(state.held_notes[67]);
        // Exactly one message went to the engine: the step edit, not a NoteOn.
        assert_eq!(free_before - state.engine.free_slots(), 1);
        engine.process_messages();
        assert_eq!(engine.pattern().synth[3].note, Some(67));
        assert_eq!(engine.voices().held_voices(), 0);
    }

    #[test]
    fn midi_note_overdubs_while_armed_and_playing() {
        let (mut state, _engine, tx) = midi_state();
        state.clear_synth();
        state.play();
        state.toggle_arm();
        state.playhead = at(2, 0.6);
        tx.try_send(NoteEvent::On {
            note: 62,
            velocity: 1.0,
        })
        .unwrap();
        tx.try_send(NoteEvent::Off { note: 62 }).unwrap();
        assert_eq!(state.poll_midi(), 2);
        assert_eq!(
            state.pattern().synth[3],
            SynthStep {
                note: Some(62),
                accent: true
            }
        );
        assert!(!state.held_notes[62]);
        assert_eq!(state.note_on_count, 0);
        assert_eq!(state.midi_note_on_count, 1);
    }

    #[test]
    fn poll_midi_drains_everything_and_drops_a_dead_channel() {
        let (mut state, _engine, tx) = midi_state();
        for note in 60..70u8 {
            tx.try_send(NoteEvent::On {
                note,
                velocity: 0.5,
            })
            .unwrap();
        }
        assert_eq!(state.poll_midi(), 10);
        assert_eq!(state.poll_midi(), 0);
        assert!((60..70).all(|n| state.held_notes[n]));
        drop(tx);
        assert_eq!(state.poll_midi(), 0);
        assert!(
            state.midi_rx.is_none(),
            "a disconnected sender ends polling"
        );
    }

    #[test]
    fn disconnect_without_a_connection_forgets_the_choice_quietly() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.midi_port = Some("Some Keys".to_owned());
        state.note_on(60, 0.8);
        let free_before = state.engine.free_slots();
        state.disconnect_midi();
        assert_eq!(state.midi_port, None);
        assert!(state.midi_rx.is_none());
        assert!(state.midi_handle.is_some());
        // Nothing was open, so nothing to silence: no AllNotesOff, keys untouched.
        assert_eq!(state.engine.free_slots(), free_before);
        assert!(state.held_notes[60]);
        engine.process_messages();
        assert_eq!(engine.voices().held_voices(), 1);
    }

    #[test]
    fn connecting_to_a_missing_port_keeps_the_handle_and_the_choice() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.connect_midi("no such port 0xDEADBEEF");
        assert!(!state.midi_connected());
        assert!(state.midi_handle.is_some(), "the queue handle came back");
        assert_eq!(state.midi_port.as_deref(), Some("no such port 0xDEADBEEF"));
        assert_eq!(
            state.midi_failed_port.as_deref(),
            Some("no such port 0xDEADBEEF")
        );
        assert!(state.status_text().is_some_and(|s| s.contains("MIDI")));

        // The refresh does not retry a failed port, and lists without panicking.
        state.refresh_midi_ports();
        assert!(state.midi_ports_refreshed.is_some());
        assert!(!state.midi_connected());
        // A second refresh within the window is skipped (same timestamp).
        let at = state.midi_ports_refreshed;
        state.refresh_midi_ports();
        assert_eq!(state.midi_ports_refreshed, at);
    }

    #[test]
    fn disabled_midi_never_lists_or_connects() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.midi_enabled = false;
        state.refresh_midi_ports();
        assert!(state.midi_ports.is_empty());
        assert!(state.midi_ports_refreshed.is_none());
        state.connect_midi("anything");
        assert!(!state.midi_connected());
        assert_eq!(state.midi_port.as_deref(), Some("anything"));
        assert!(state.midi_handle.is_some());
        assert_eq!(state.status_text(), None);
    }

    #[test]
    fn truncate_name_keeps_short_names_and_ellipsises_long_ones() {
        assert_eq!(truncate_name("MPK mini 3", 18), "MPK mini 3");
        assert_eq!(
            truncate_name("Arturia KeyStep 32 MIDI 1", 18),
            "Arturia KeyStep 3…"
        );
        assert_eq!(
            truncate_name("Arturia KeyStep 32 MIDI 1", 18)
                .chars()
                .count(),
            18
        );
        assert_eq!(truncate_name("", 5), "");
        assert_eq!(truncate_name("abc", 1), "…");
        assert_eq!(truncate_name("日本語のポート名です", 5), "日本語の…");
    }

    // -----------------------------------------------------------------------------------------
    // M4 polish: undo / redo, dirty flag, shortcuts dispatch.

    /// Backdates the last continuous edit so the next change counts as "after a pause".
    fn quiet_period(state: &mut AppState) {
        if let Some((key, _)) = state.last_continuous_edit {
            state.last_continuous_edit = Some((key, Instant::now() - CONTINUOUS_EDIT_GAP * 2));
        }
    }

    #[test]
    fn fresh_state_has_no_history_and_is_clean() {
        let (state, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(state.undo.is_empty());
        assert!(!state.dirty);
        assert_eq!(state.pending_action, None);
        assert!(!state.dialogs_enabled, "tests never open dialogs");
        let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
        assert!(app.state().undo.is_empty());
        assert!(!app.state().dirty);
        assert_eq!(app.title(), None, "no frame yet");
    }

    #[test]
    fn a_step_toggle_is_undone_and_redone_in_the_mirror_and_the_engine() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.send_pattern();
        engine.process_messages();
        let demo = Pattern::demo();
        let kick = DrumRow::Kick.index();

        state.toggle_drum_step(DrumRow::Kick, 1, false);
        assert!(state.pattern().drums[kick][1].on);
        assert!(state.dirty);
        assert_eq!(state.undo.len(), 1);
        assert!(state.undo.can_undo() && !state.undo.can_redo());

        assert!(state.undo());
        assert_eq!(state.pattern().clone(), demo);
        assert!(!state.undo.can_undo() && state.undo.can_redo());
        engine.process_messages();
        assert_eq!(
            engine.pattern(),
            state.pattern(),
            "undo resends the pattern"
        );

        assert!(state.redo());
        assert!(state.pattern().drums[kick][1].on);
        assert!(state.undo.can_undo() && !state.undo.can_redo());
        engine.process_messages();
        assert_eq!(engine.pattern(), state.pattern());

        // Nothing more to redo / a second undo then exhausts the history.
        assert!(!state.redo());
        assert!(state.undo());
        assert!(!state.undo());
        assert_eq!(state.pattern().clone(), demo);
    }

    #[test]
    fn undo_restores_params_bpm_swing_preset_and_resends() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.set_bpm(150.0);
        state.set_swing(0.5);
        quiet_period(&mut state);
        state.set_param(ParamId::FilterCutoff, 500.0);
        state.apply_preset(3);
        assert_eq!(state.undo.len(), 4, "four distinct edits");

        assert!(state.undo()); // preset
        assert_eq!(state.preset_index, 0);
        assert_eq!(state.param(ParamId::FilterCutoff), 500.0);
        assert!(state.undo()); // cutoff
        assert_eq!(
            state.param(ParamId::FilterCutoff),
            ParamId::FilterCutoff.default_value()
        );
        assert!(state.undo()); // swing
        assert_eq!(state.swing, 0.0);
        assert_eq!(state.bpm, 150.0);
        assert!(state.undo()); // bpm
        assert_eq!(state.bpm, DEFAULT_BPM);
        // Four resends of every parameter exceed one queue's worth: play the frames.
        sync(&mut state, &mut engine);
        assert_eq!(engine.bpm(), DEFAULT_BPM);
        assert_eq!(engine.transport().swing(), 0.0);
        assert_eq!(
            engine.param(ParamId::FilterCutoff),
            ParamId::FilterCutoff.default_value()
        );
        assert_eq!(
            state.snapshot(),
            AppState::headless(ThemeMode::Dark).0.snapshot()
        );

        for _ in 0..4 {
            assert!(state.redo());
        }
        assert_eq!(state.preset_index, 3);
        assert_eq!(state.bpm, 150.0);
        assert_eq!(state.swing, 0.5);
        sync(&mut state, &mut engine);
        assert_eq!(engine.bpm(), 150.0);
        for id in ParamId::ALL {
            assert_eq!(engine.param(*id), state.param(*id), "{id:?}");
        }
    }

    #[test]
    fn a_knob_drag_coalesces_into_one_undo_step() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        // A drag: many changes within the gap.
        for v in 1..=20 {
            state.set_param(ParamId::DriveAmount, v as f32 / 40.0);
        }
        assert_eq!(state.undo.len(), 1, "one step for the whole drag");
        assert_eq!(state.param(ParamId::DriveAmount), 0.5);

        // Still within the gap, a *different* control starts a new step.
        state.set_param(ParamId::DriveMix, 0.9);
        assert_eq!(state.undo.len(), 2);
        // Back on the first knob within the gap: new step again (the control changed).
        state.set_param(ParamId::DriveAmount, 0.6);
        assert_eq!(state.undo.len(), 3);

        // After a pause the same knob starts a new step.
        quiet_period(&mut state);
        state.set_param(ParamId::DriveAmount, 0.7);
        assert_eq!(state.undo.len(), 4);
        // `drag_started` forces a new step even without a pause.
        state.edit_param(ParamId::DriveAmount, 0.8, true);
        assert_eq!(state.undo.len(), 5);
        state.edit_param(ParamId::DriveAmount, 0.85, false);
        assert_eq!(state.undo.len(), 5, "the drag continues");
        // Setting the value it already has is not an edit.
        state.edit_param(ParamId::DriveAmount, 0.85, true);
        assert_eq!(state.undo.len(), 5);

        // Undoing the last drag returns to the value before it (0.7), not to 0.8.
        assert!(state.undo());
        assert_eq!(state.param(ParamId::DriveAmount), 0.7);
        assert!(state.undo());
        assert_eq!(state.param(ParamId::DriveAmount), 0.6);
        assert!(state.undo());
        assert_eq!(state.param(ParamId::DriveAmount), 0.5);
        assert!(state.undo());
        assert_eq!(
            state.param(ParamId::DriveMix),
            ParamId::DriveMix.default_value()
        );
        assert!(state.undo());
        assert_eq!(
            state.param(ParamId::DriveAmount),
            ParamId::DriveAmount.default_value()
        );
        assert!(!state.undo.can_undo());
    }

    #[test]
    fn swing_and_row_knobs_coalesce_too_and_discrete_edits_cut_a_drag() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        for v in 1..=10 {
            state.set_swing(v as f32 / 20.0);
        }
        assert_eq!(state.undo.len(), 1);
        for v in 1..=10 {
            state.set_drum_volume(DrumRow::Kick, -(v as f32));
        }
        assert_eq!(state.undo.len(), 2);
        for v in 1..=10 {
            state.set_drum_tune(DrumRow::Kick, v as f32 / 2.0);
        }
        assert_eq!(state.undo.len(), 3);
        state.set_drum_tune(DrumRow::Snare, 1.0);
        assert_eq!(state.undo.len(), 4, "another row is another control");
        state.begin_continuous_edit(EditKey::DrumTune(DrumRow::Snare), true);
        state.set_drum_tune(DrumRow::Snare, 2.0);
        assert_eq!(state.undo.len(), 5, "a new drag on the same knob");

        // A step toggle in the middle of a drag ends the drag: the next knob change is new.
        state.set_param(ParamId::Glide, 0.2);
        state.toggle_drum_step(DrumRow::Clap, 0, false);
        state.set_param(ParamId::Glide, 0.3);
        assert_eq!(state.undo.len(), 8);

        // Seven undos: Glide, the clap step, Glide, both snare drags, the tune drag and the whole
        // volume drag at once; the swing drag remains.
        for _ in 0..7 {
            assert!(state.undo());
        }
        assert_eq!(state.pattern().drum_volume[DrumRow::Kick.index()], 0.0);
        assert_eq!(state.swing, 0.5);
    }

    #[test]
    fn no_op_edits_add_no_history() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.clear_all();
        assert_eq!(state.undo.len(), 1);
        state.clear_all();
        state.clear_row(DrumRow::Kick);
        state.clear_synth();
        assert_eq!(
            state.undo.len(),
            1,
            "clearing an empty pattern is not an edit"
        );
        state.set_bpm(DEFAULT_BPM);
        state.set_swing(0.0);
        state.set_param(ParamId::Glide, ParamId::Glide.default_value());
        state.set_drum_volume(DrumRow::Kick, 0.0);
        state.set_drum_tune(DrumRow::Kick, f32::NAN);
        assert_eq!(state.undo.len(), 1);
        // begin_edit twice without a change in between records once.
        let before = state.undo.len();
        state.begin_edit();
        state.begin_edit();
        assert_eq!(state.undo.len(), before + 1);
    }

    #[test]
    fn history_is_capped_at_one_hundred_steps() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        for i in 0..130 {
            state.toggle_drum_step(DrumRow::Kick, i % engine::STEPS, false);
        }
        assert_eq!(state.undo.len(), crate::undo::UNDO_CAP);
        let mut undone = 0;
        while state.undo() {
            undone += 1;
        }
        assert_eq!(undone, crate::undo::UNDO_CAP);
        assert_eq!(state.undo.redo_len(), crate::undo::UNDO_CAP);
    }

    #[test]
    fn note_entry_and_overdub_are_undoable() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        let demo = Pattern::demo();
        state.click_synth_step(2, false);
        assert_eq!(state.undo.len(), 0, "selecting is not an edit");
        assert!(!state.dirty);
        state.note_on(67, 0.8);
        state.note_off(67);
        assert_eq!(state.pattern().synth[2].note, Some(67));
        assert_eq!(state.undo.len(), 1);
        assert!(state.undo());
        assert_eq!(state.pattern().clone(), demo);
        assert_eq!(state.selected_step, Some(2), "undo keeps the selection");

        state.selected_step = None;
        state.play();
        state.toggle_arm();
        state.playhead = at(5, 0.1);
        state.note_on(60, 1.0);
        assert_eq!(state.pattern().synth[5].note, Some(60));
        assert!(state.undo());
        assert_eq!(state.pattern().clone(), demo);
        assert!(state.playing, "undo does not touch the transport");
    }

    #[test]
    fn shortcuts_drive_transport_and_history_but_never_dialogs_headless() {
        let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
        app.run_shortcut(Shortcut::PlayStop);
        assert!(app.state().playing);
        app.run_shortcut(Shortcut::ToggleArm);
        assert!(app.state().armed);
        app.run_shortcut(Shortcut::PlayStop);
        assert!(!app.state().playing);

        app.state_mut().toggle_drum_step(DrumRow::Kick, 1, false);
        app.run_shortcut(Shortcut::Undo);
        assert_eq!(app.state().pattern().clone(), Pattern::demo());
        app.run_shortcut(Shortcut::Redo);
        assert!(app.state().pattern().drums[DrumRow::Kick.index()][1].on);

        // Dialogs are disabled headless: these return at once and change nothing.
        let before = app.state().snapshot();
        app.run_shortcut(Shortcut::Save);
        app.run_shortcut(Shortcut::SaveAs);
        app.run_shortcut(Shortcut::Export);
        assert_eq!(app.state().snapshot(), before);
        assert!(!app.state().is_exporting());
        // New / Open while dirty only ask.
        app.run_shortcut(Shortcut::New);
        assert_eq!(app.state().pending_action, Some(PendingAction::New));
        app.run_shortcut(Shortcut::Open);
        assert_eq!(app.state().pending_action, Some(PendingAction::Open));
        assert_eq!(app.state().snapshot(), before);
        app.state_mut().cancel_pending();
        // Clean: New happens right away.
        app.state_mut().mark_clean();
        app.run_shortcut(Shortcut::New);
        assert_eq!(app.state().pattern().clone(), Pattern::empty());
        assert_eq!(app.state().pending_action, None);
    }

    #[test]
    fn status_line_shows_then_expires() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(state.status_text(), None);
        state.set_status("Saved");
        assert_eq!(state.status_text(), Some("Saved"));
        state.expire_status();
        assert!(state.status.is_some());
        // Backdate the status past its lifetime.
        state.status = Some(("Saved".into(), Instant::now() - STATUS_DURATION * 2));
        assert_eq!(state.status_text(), None);
        state.expire_status();
        assert!(state.status.is_none());
    }
}
