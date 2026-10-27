//! Pop-out "plugin" windows (plugins milestone): the parametric EQ and the reverb, each a
//! native child window opened from the FX rack head, FL Studio style.
//!
//! # How the windows talk to the app
//!
//! Child windows are egui *deferred viewports* (`Context::show_viewport_deferred`): egui calls
//! their UI closure whenever *they* need repainting, independently of the main window, so the
//! closure must be `Send + Sync + 'static` and cannot borrow `AppState`. Everything the
//! windows read or write therefore lives in one [`PluginShared`] behind an `Arc<Mutex>`
//! ([`PluginHost::shared`]); the lock is only ever taken on the UI thread(s), never by audio.
//!
//! - **Down** (main -> windows), once per main frame in [`PluginHost::sync_from_app`]: the
//!   theme, the EQ / reverb mirrors, the selected presets, the sample rate, the reverb return
//!   level and the spectrum reader.
//! - **Up** (windows -> main): every control change is queued as a [`PluginEdit`] and applied
//!   optimistically to the shared mirror so the window reacts in the same frame; the main
//!   frame drains the queue ([`PluginHost::take_edits`]) and routes each edit through the
//!   `AppState` setters (`AppState::apply_plugin_edit`), which send the engine messages, mark
//!   the project dirty and record undo. Keyboard shortcuts pressed while a plugin window has
//!   focus are forwarded the same way, so Space still plays / stops.
//!
//! # Which EQ the window edits
//!
//! The one EQ window edits either the master EQ or the parametric EQ loaded into an insert's
//! effect slot ([`EqTarget`], `AppState::eq_target`): the FX-rack pill points it at the
//! master, a mixer slot's edit square at that slot. The target travels down with the
//! mirror, the window names it in its header, and every queued edit is stamped with the
//! target the window showed when it was made ([`PluginHost::take_edits`]), so an edit can
//! never land on an EQ the user was not looking at, even if the target changed meanwhile.
//!
//! The windows' open flags are owned by `AppState::plugin_open` (the FX-rack pills toggle
//! them; a window's close button queues [`PluginEdit::Close`]); their last position and size
//! are tracked here and persisted ([`PluginWindows`], [`PLUGIN_WINDOWS_KEY`]) so they reopen
//! where they were, and open on launch if they were open (never under `--smoke-test`).
//!
//! The reverb's impulse-response envelope is rendered on a worker thread, debounced by
//! [`ENVELOPE_DEBOUNCE`] after the last parameter change, so the UI never stalls.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egui::{Context, Rect, Vec2, ViewportBuilder, ViewportId, pos2, vec2};
use engine::fx::{eq, reverb};
use engine::{Analyzer, EqParam, EqState, ReverbParam, ReverbState, SPECTRUM_SIZE, SpectrumReader};
use neu::Theme;
use serde::{Deserialize, Serialize};

use crate::shortcuts::Shortcut;

/// eframe storage key for the plugin windows' geometry and open flags.
pub const PLUGIN_WINDOWS_KEY: &str = "plugin_windows";
/// Inner size of the EQ window, logical points.
pub const EQ_WINDOW_SIZE: Vec2 = Vec2::new(760.0, 480.0);
/// Inner size of the reverb window, logical points.
pub const REVERB_WINDOW_SIZE: Vec2 = Vec2::new(660.0, 420.0);
/// Smallest inner size either window can be resized to.
pub const MIN_WINDOW_SIZE: Vec2 = Vec2::new(520.0, 360.0);
/// How often the EQ window re-reads the spectrum (about 30 fps).
pub const SPECTRUM_INTERVAL: Duration = Duration::from_millis(33);
/// Quiet time after the last reverb change before the envelope is re-rendered.
pub const ENVELOPE_DEBOUNCE: Duration = Duration::from_millis(100);
/// Sample rate the envelope is rendered at (the tunings scale, so the decay is the same).
pub const ENVELOPE_SAMPLE_RATE: f32 = 24_000.0;
/// Longest time span the envelope plot covers.
pub const ENVELOPE_MAX_SECONDS: f32 = 10.0;
/// How fast the wet-level meter falls after its peak, dB per second.
pub const WET_METER_DECAY_DB_PER_S: f32 = 30.0;
/// Shown wet levels at or below this count as silent.
pub const WET_METER_SILENT: f32 = 0.001;
/// The EQ window's header label while it edits the master EQ.
pub const MASTER_LABEL: &str = "MASTER";

/// Which plugin window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginKind {
    /// The seven-band parametric EQ on the master bus.
    Eq,
    /// The reverb return.
    Reverb,
}

impl PluginKind {
    /// Both kinds, in pill order.
    pub const ALL: [PluginKind; 2] = [PluginKind::Eq, PluginKind::Reverb];

    /// Index into per-kind arrays.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Native window title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            PluginKind::Eq => "BeatSlopper EQ",
            PluginKind::Reverb => "BeatSlopper Reverb",
        }
    }

    /// Pill label in the FX rack head.
    #[must_use]
    pub const fn pill_label(self) -> &'static str {
        match self {
            PluginKind::Eq => "EQ",
            PluginKind::Reverb => "REVERB",
        }
    }

    /// Default inner size.
    #[must_use]
    pub const fn default_size(self) -> Vec2 {
        match self {
            PluginKind::Eq => EQ_WINDOW_SIZE,
            PluginKind::Reverb => REVERB_WINDOW_SIZE,
        }
    }

    /// Stable viewport id.
    #[must_use]
    pub fn viewport_id(self) -> ViewportId {
        ViewportId::from_hash_of(("beatslopper-plugin", self))
    }
}

/// Position / size / open state of one plugin window (persisted).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowGeom {
    /// Whether the window is (or was, when persisted) open.
    pub open: bool,
    /// Outer top-left position in logical points, when known.
    pub pos: Option<[f32; 2]>,
    /// Inner size in logical points, when known.
    pub size: Option<[f32; 2]>,
}

/// Geometry of both windows.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginWindows {
    pub eq: WindowGeom,
    pub reverb: WindowGeom,
}

impl PluginWindows {
    /// The geometry of `kind`.
    #[must_use]
    pub fn get(&self, kind: PluginKind) -> &WindowGeom {
        match kind {
            PluginKind::Eq => &self.eq,
            PluginKind::Reverb => &self.reverb,
        }
    }

    /// The geometry of `kind`, mutably.
    pub fn get_mut(&mut self, kind: PluginKind) -> &mut WindowGeom {
        match kind {
            PluginKind::Eq => &mut self.eq,
            PluginKind::Reverb => &mut self.reverb,
        }
    }

    /// The open flags as the `AppState` array (`[eq, reverb]`).
    #[must_use]
    pub fn open_flags(&self) -> [bool; 2] {
        [self.eq.open, self.reverb.open]
    }
}

/// The EQ the EQ window edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EqTarget {
    /// The seven-band EQ on the master bus.
    #[default]
    Master,
    /// The parametric EQ in effect slot `slot` (`0..3`) of insert number `insert`
    /// (`1..=16`).
    Slot { insert: u8, slot: u8 },
}

/// A change made in a plugin window, applied by the main frame through the `AppState`
/// setters (engine message, dirty flag, undo). EQ edits apply to the [`EqTarget`] they were
/// stamped with when queued.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PluginEdit {
    /// One EQ band value.
    EqBand {
        band: usize,
        param: EqParam,
        value: f32,
        /// `Response::drag_started` of the control: begins a new undo step.
        drag_started: bool,
    },
    /// The EQ master switch.
    EqOn(bool),
    /// The EQ output gain, dB.
    EqGain { db: f32, drag_started: bool },
    /// Apply EQ preset `index` (all bands).
    EqPreset(usize),
    /// One reverb value.
    Reverb {
        param: ReverbParam,
        value: f32,
        drag_started: bool,
    },
    /// Apply reverb preset `index`.
    ReverbPreset(usize),
    /// The window's close button was pressed.
    Close(PluginKind),
}

/// A rendered impulse-response envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeCache {
    /// The reverb settings it was rendered for.
    pub state: ReverbState,
    /// RMS per 10 ms bin, oldest first.
    pub bins: Vec<f32>,
    /// Time span covered (`bins.len() * 0.01`, before rounding).
    pub seconds: f32,
}

impl EnvelopeCache {
    /// Renders the envelope for `state` synchronously (the worker calls this too).
    #[must_use]
    pub fn render(state: ReverbState) -> Self {
        let seconds = envelope_seconds(&state);
        let bins = state.impulse_envelope(ENVELOPE_SAMPLE_RATE, seconds);
        Self {
            state,
            bins,
            seconds,
        }
    }

    /// The largest bin (the peak the plot normalises to), at least a tiny positive value.
    #[must_use]
    pub fn peak(&self) -> f32 {
        self.bins.iter().copied().fold(0.0_f32, f32::max).max(1e-9)
    }
}

/// Time span the envelope plot covers for `state`: the RT60 estimate, at least one second,
/// at most [`ENVELOPE_MAX_SECONDS`].
#[must_use]
pub fn envelope_seconds(state: &ReverbState) -> f32 {
    state.decay_time_60db().clamp(1.0, ENVELOPE_MAX_SECONDS)
}

/// A running envelope render.
struct EnvelopeJob {
    rx: Receiver<EnvelopeCache>,
    handle: Option<JoinHandle<()>>,
}

impl EnvelopeJob {
    fn spawn(state: ReverbState) -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("beatslopper-reverb-envelope".into())
            .spawn(move || {
                // The receiver is gone only if the window state was dropped meanwhile.
                let _ = tx.send(EnvelopeCache::render(state));
            })
            .ok();
        Self { rx, handle }
    }

    /// `Some` once the worker reported (or died); `None` while it is still rendering.
    fn try_finish(&mut self) -> Option<Option<EnvelopeCache>> {
        let result = match self.rx.try_recv() {
            Ok(cache) => Some(cache),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => None,
        };
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        Some(result)
    }
}

/// Everything the plugin windows read and write. Lives behind [`PluginHost::shared`];
/// constructible on its own ([`PluginShared::new`]) so the window UIs can be rendered as plain
/// panels under `egui_kittest`.
pub struct PluginShared {
    /// Theme tokens of the mode the main window shows.
    pub theme: Theme,
    /// Sample rate of the running stream (the EQ curve and analyser depend on it).
    pub sample_rate: f32,
    /// Whether the transport runs (the analyser repaints while it does).
    pub playing: bool,

    // ---- EQ ----
    /// The EQ the window shows and edits (mirror of `AppState::eq_target`).
    pub eq_target: EqTarget,
    /// What the window header calls the target (`"MASTER"`, `"INSERT 3 · KICK · SLOT 1"`).
    pub eq_target_label: String,
    /// Mirror of the targeted EQ (optimistically updated by the window's own edits).
    pub eq: EqState,
    /// The EQ preset last applied, for the list highlight and the MODIFIED marker.
    pub eq_preset: Option<usize>,
    /// Band the controls below the graph edit.
    pub eq_selected: usize,
    /// Band being dragged on the graph, if any.
    pub eq_drag: Option<usize>,
    /// Where the graph was last drawn (tests use it with the geometry helpers).
    pub eq_graph_rect: Option<Rect>,
    /// Reader of the master-output ring (`None` headless: the analyser stays at the floor).
    pub spectrum: Option<SpectrumReader>,
    /// FFT and display smoothing.
    pub analyzer: Analyzer,
    /// Snapshot buffer (boxed: 8 kB).
    pub spectrum_buf: Box<[f32; SPECTRUM_SIZE]>,
    /// When the analyser last ran.
    pub spectrum_updated: Option<Instant>,

    // ---- reverb ----
    /// Mirror of `AppState::reverb`.
    pub reverb: ReverbState,
    /// The reverb preset last applied.
    pub reverb_preset: Option<usize>,
    /// Block peak of the reverb return as last read from the engine.
    pub reverb_level: f32,
    /// The decayed level the wet meter draws.
    pub wet_shown: f32,
    /// When `wet_shown` was last updated.
    pub wet_updated: Option<Instant>,
    /// The envelope currently drawn (may lag `reverb` while a render is pending).
    pub envelope: Option<EnvelopeCache>,
    envelope_job: Option<EnvelopeJob>,
    /// When `reverb` last diverged from the drawn envelope (debounce start).
    envelope_pending: Option<Instant>,

    // ---- up ----
    /// Edits queued for the main frame.
    pub edits: Vec<PluginEdit>,
    /// The [`EqTarget`] each queued edit was made for (in step with `edits`).
    edit_targets: Vec<EqTarget>,
    /// Shortcuts pressed while a plugin window had focus.
    pub shortcuts: Vec<Shortcut>,
    /// Open flags (from the app) and the last known geometry (from the windows).
    pub windows: PluginWindows,
}

impl PluginShared {
    /// Fresh shared state in `theme`, flat EQ, default reverb, nothing open, no stream.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            sample_rate: 48_000.0,
            playing: false,
            eq_target: EqTarget::Master,
            eq_target_label: MASTER_LABEL.to_owned(),
            eq: EqState::default(),
            eq_preset: Some(0),
            eq_selected: 3,
            eq_drag: None,
            eq_graph_rect: None,
            spectrum: None,
            analyzer: Analyzer::new(),
            spectrum_buf: Box::new([0.0; SPECTRUM_SIZE]),
            spectrum_updated: None,
            reverb: ReverbState::default(),
            reverb_preset: None,
            reverb_level: 0.0,
            wet_shown: 0.0,
            wet_updated: None,
            envelope: None,
            envelope_job: None,
            envelope_pending: None,
            edits: Vec::new(),
            edit_targets: Vec::new(),
            shortcuts: Vec::new(),
            windows: PluginWindows::default(),
        }
    }

    /// Queues `edit` for the main frame and applies it to the local mirror at once, so the
    /// window shows the change in the same frame.
    pub fn push_edit(&mut self, edit: PluginEdit) {
        match edit {
            PluginEdit::EqBand {
                band, param, value, ..
            } => {
                self.eq.set(band, param, value);
            }
            PluginEdit::EqOn(on) => self.eq.on = on,
            PluginEdit::EqGain { db, .. } => {
                self.eq.set_gain_db(db);
            }
            PluginEdit::EqPreset(i) => {
                if let Some(p) = eq::PRESETS.get(i) {
                    self.eq = p.state;
                    self.eq_preset = Some(i);
                }
            }
            PluginEdit::Reverb { param, value, .. } => {
                self.reverb.set(param, value);
            }
            PluginEdit::ReverbPreset(i) => {
                if let Some(p) = reverb::PRESETS.get(i) {
                    self.reverb = p.state;
                    self.reverb_preset = Some(i);
                }
            }
            PluginEdit::Close(kind) => self.windows.get_mut(kind).open = false,
        }
        self.edits.push(edit);
        self.edit_targets.push(self.eq_target);
    }

    /// `true` when the EQ differs from its selected preset (or no preset matches).
    #[must_use]
    pub fn eq_modified(&self) -> bool {
        !self
            .eq_preset
            .and_then(|i| eq::PRESETS.get(i))
            .is_some_and(|p| p.state == self.eq)
    }

    /// `true` when the reverb differs from its selected preset (or no preset matches).
    #[must_use]
    pub fn reverb_modified(&self) -> bool {
        !self
            .reverb_preset
            .and_then(|i| reverb::PRESETS.get(i))
            .is_some_and(|p| p.state == self.reverb)
    }

    /// Re-reads the spectrum ring and runs the analyser when [`SPECTRUM_INTERVAL`] has passed
    /// (or at once when `force`). Returns `true` when it ran.
    pub fn update_spectrum(&mut self, force: bool) -> bool {
        let now = Instant::now();
        let dt = self
            .spectrum_updated
            .map_or(SPECTRUM_INTERVAL, |at| now.duration_since(at));
        if !force && dt < SPECTRUM_INTERVAL {
            return false;
        }
        self.spectrum_updated = Some(now);
        if let Some(reader) = &self.spectrum {
            reader.snapshot(&mut self.spectrum_buf);
        } else {
            self.spectrum_buf.fill(0.0);
        }
        self.analyzer.analyse(&self.spectrum_buf, dt.as_secs_f32());
        true
    }

    /// Applies the wet meter's decay: the shown level falls by [`WET_METER_DECAY_DB_PER_S`]
    /// since the last call unless the engine's level is louder.
    pub fn decay_wet(&mut self) {
        let now = Instant::now();
        let dt = self
            .wet_updated
            .map_or(0.0, |at| now.duration_since(at).as_secs_f32());
        self.wet_updated = Some(now);
        self.wet_shown = decay_level(self.wet_shown, self.reverb_level, dt);
    }

    /// Debounced envelope rendering: starts a worker [`ENVELOPE_DEBOUNCE`] after the reverb
    /// last changed, collects a finished one. Returns `true` while a render is pending or
    /// running (the window keeps repainting then).
    pub fn poll_envelope(&mut self) -> bool {
        if let Some(job) = &mut self.envelope_job
            && let Some(result) = job.try_finish()
        {
            self.envelope_job = None;
            if let Some(cache) = result {
                self.envelope = Some(cache);
            }
        }
        let current = self
            .envelope
            .as_ref()
            .is_some_and(|e| e.state == self.reverb);
        let rendering_current = self.envelope_job.is_some() && self.envelope_pending.is_none();
        if current {
            self.envelope_pending = None;
            return self.envelope_job.is_some();
        }
        if rendering_current {
            return true;
        }
        let now = Instant::now();
        let since = *self.envelope_pending.get_or_insert(now);
        if self.envelope_job.is_none() && now.duration_since(since) >= ENVELOPE_DEBOUNCE {
            self.envelope_job = Some(EnvelopeJob::spawn(self.reverb));
            self.envelope_pending = None;
        }
        true
    }

    /// Renders the envelope for the current reverb synchronously (tests, first frame).
    pub fn compute_envelope_now(&mut self) {
        self.envelope = Some(EnvelopeCache::render(self.reverb));
        self.envelope_pending = None;
    }

    /// The window geometry reported by the viewport this frame.
    pub fn record_geometry(
        &mut self,
        kind: PluginKind,
        outer_pos: Option<egui::Pos2>,
        inner: Option<Vec2>,
    ) {
        let geom = self.windows.get_mut(kind);
        if let Some(p) = outer_pos {
            geom.pos = Some([p.x, p.y]);
        }
        if let Some(s) = inner
            && s.x > 0.0
            && s.y > 0.0
        {
            geom.size = Some([s.x, s.y]);
        }
    }
}

/// One meter decay step: `shown` falls by [`WET_METER_DECAY_DB_PER_S`] over `dt_seconds`
/// unless `new` is louder (then it jumps up). Silence below [`WET_METER_SILENT`].
#[must_use]
pub fn decay_level(shown: f32, new: f32, dt_seconds: f32) -> f32 {
    let new = if new.is_finite() { new.max(0.0) } else { 0.0 };
    if new >= shown {
        return new;
    }
    let fallen = shown * 10f32.powf(-WET_METER_DECAY_DB_PER_S * dt_seconds.max(0.0) / 20.0);
    let v = fallen.max(new);
    if v <= WET_METER_SILENT { 0.0 } else { v }
}

/// Owns the shared state and drives the viewports from the main frame.
pub struct PluginHost {
    /// The state both windows render from.
    pub shared: Arc<Mutex<PluginShared>>,
    /// Last known geometry (what `save` persists).
    geometry: PluginWindows,
}

impl PluginHost {
    /// A host with fresh shared state in `theme`.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            shared: Arc::new(Mutex::new(PluginShared::new(theme))),
            geometry: PluginWindows::default(),
        }
    }

    /// Locks the shared state (a poisoned lock is recovered: the data is plain values).
    pub fn lock(&self) -> std::sync::MutexGuard<'_, PluginShared> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Restores persisted geometry (positions / sizes; the open flags are the caller's).
    pub fn restore(&mut self, persisted: PluginWindows) {
        self.geometry = persisted;
        self.lock().windows = persisted;
    }

    /// Geometry to persist, with the given open flags.
    #[must_use]
    pub fn geometry(&self, open: [bool; 2]) -> PluginWindows {
        let mut g = self.geometry;
        g.eq.open = open[PluginKind::Eq.index()];
        g.reverb.open = open[PluginKind::Reverb.index()];
        g
    }

    /// Takes the queued edits, each with the [`EqTarget`] the window showed when it was made,
    /// and the shortcuts (main frame).
    pub fn take_edits(&mut self) -> (Vec<(PluginEdit, EqTarget)>, Vec<Shortcut>) {
        let mut s = self.lock();
        let edits = std::mem::take(&mut s.edits);
        let mut targets = std::mem::take(&mut s.edit_targets);
        // Edits pushed straight into the pub queue (tests) count for the current target.
        targets.resize(edits.len(), s.eq_target);
        (
            edits.into_iter().zip(targets).collect(),
            std::mem::take(&mut s.shortcuts),
        )
    }

    /// Pushes the app's current values down (main frame, after the edits were applied).
    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        theme: &Theme,
        sample_rate: f32,
        playing: bool,
        eq_target: EqTarget,
        eq_target_label: &str,
        eq: &EqState,
        eq_preset: Option<usize>,
        reverb: &ReverbState,
        reverb_preset: Option<usize>,
        reverb_level: f32,
        spectrum: Option<SpectrumReader>,
        open: [bool; 2],
    ) {
        let mut s = self.lock();
        if s.theme != *theme {
            s.theme = theme.clone();
        }
        s.sample_rate = sample_rate;
        s.playing = playing;
        if s.eq_target != eq_target {
            // Another EQ: drop the drag of the one that was shown.
            s.eq_target = eq_target;
            s.eq_drag = None;
        }
        if s.eq_target_label != eq_target_label {
            eq_target_label.clone_into(&mut s.eq_target_label);
        }
        s.eq = *eq;
        s.eq_preset = eq_preset;
        s.reverb = *reverb;
        s.reverb_preset = reverb_preset;
        s.reverb_level = reverb_level;
        if s.spectrum.is_none() {
            s.spectrum = spectrum;
        }
        s.windows.eq.open = open[PluginKind::Eq.index()];
        s.windows.reverb.open = open[PluginKind::Reverb.index()];
        // Remember where the windows are for persistence.
        let windows = s.windows;
        drop(s);
        self.geometry = windows;
    }

    /// Shows a deferred viewport for every open window (call every main frame; a window
    /// that is not shown in a frame is closed by egui).
    pub fn show_viewports(&self, ctx: &Context, open: [bool; 2]) {
        for kind in PluginKind::ALL {
            if !open[kind.index()] {
                continue;
            }
            let geom = *self.geometry.get(kind);
            let size = geom
                .size
                .map_or(kind.default_size(), |s| vec2(s[0], s[1]))
                .max(MIN_WINDOW_SIZE);
            let mut builder = ViewportBuilder::default()
                .with_title(kind.title())
                .with_app_id("beatslopper")
                .with_inner_size(size)
                .with_min_inner_size(MIN_WINDOW_SIZE);
            if let Some(p) = geom.pos {
                builder = builder.with_position(pos2(p[0], p[1]));
            }
            let shared = Arc::clone(&self.shared);
            ctx.show_viewport_deferred(kind.viewport_id(), builder, move |ui, class| {
                let mut s = shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                crate::ui::plugins::viewport_ui(ui, &mut s, kind, class);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_ids_and_geometry() {
        assert_ne!(
            PluginKind::Eq.viewport_id(),
            PluginKind::Reverb.viewport_id()
        );
        assert_ne!(PluginKind::Eq.viewport_id(), ViewportId::ROOT);
        let mut w = PluginWindows::default();
        w.get_mut(PluginKind::Reverb).open = true;
        assert_eq!(w.open_flags(), [false, true]);
        assert!(w.get(PluginKind::Reverb).open);
        let json = serde_json::to_string(&w).unwrap();
        let back: PluginWindows = serde_json::from_str(&json).unwrap();
        assert_eq!(back, w);
        let legacy: PluginWindows = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy, PluginWindows::default());
    }

    #[test]
    fn edits_apply_optimistically_and_queue() {
        let mut s = PluginShared::new(Theme::dark());
        assert!(!s.eq_modified(), "flat equals the Flat preset");
        s.push_edit(PluginEdit::EqBand {
            band: 3,
            param: EqParam::Gain,
            value: 6.0,
            drag_started: true,
        });
        assert_eq!(s.eq.bands[3].gain_db, 6.0);
        assert!(s.eq_modified());
        s.push_edit(PluginEdit::EqPreset(2));
        assert_eq!(s.eq, eq::PRESETS[2].state);
        assert!(!s.eq_modified());
        assert!(s.reverb_modified(), "no reverb preset selected yet");
        s.push_edit(PluginEdit::ReverbPreset(1));
        assert!(!s.reverb_modified());
        s.push_edit(PluginEdit::Reverb {
            param: ReverbParam::Width,
            value: 0.2,
            drag_started: false,
        });
        assert_eq!(s.reverb.width, 0.2);
        assert!(s.reverb_modified());
        s.windows.eq.open = true;
        s.push_edit(PluginEdit::Close(PluginKind::Eq));
        assert!(!s.windows.eq.open);
        assert_eq!(s.edits.len(), 5);
        assert!(matches!(s.edits[0], PluginEdit::EqBand { band: 3, .. }));
    }

    #[test]
    fn envelope_renders_and_wet_meter_decays() {
        let mut s = PluginShared::new(Theme::light());
        s.compute_envelope_now();
        let env = s.envelope.as_ref().unwrap();
        assert_eq!(env.state, s.reverb);
        assert!(env.bins.len() >= 100);
        assert!(env.peak() > 0.0);
        assert!(
            !s.poll_envelope(),
            "nothing pending while the envelope is current"
        );
        s.reverb.size = 0.9;
        assert!(s.poll_envelope(), "a change starts the debounce");
        assert!(envelope_seconds(&s.reverb) >= 1.0);
        assert!(
            envelope_seconds(&ReverbState::new(1.0, 0.0, 0.0, 1.0, 20.0, 20_000.0, 1.0))
                <= ENVELOPE_MAX_SECONDS
        );
        assert_eq!(decay_level(0.5, 0.8, 0.1), 0.8);
        let fallen = decay_level(0.5, 0.0, 1.0);
        assert!((fallen - 0.5 * 10f32.powf(-1.5)).abs() < 1e-6);
        assert_eq!(decay_level(0.001, 0.0, 1.0), 0.0);
        assert_eq!(decay_level(0.5, f32::NAN, 0.0), 0.5);
        s.reverb_level = 0.3;
        s.decay_wet();
        assert_eq!(s.wet_shown, 0.3);
    }

    #[test]
    fn host_round_trips_edits_and_geometry() {
        let mut host = PluginHost::new(Theme::dark());
        host.restore(PluginWindows {
            eq: WindowGeom {
                open: true,
                pos: Some([10.0, 20.0]),
                size: Some([800.0, 500.0]),
            },
            reverb: WindowGeom::default(),
        });
        host.lock().push_edit(PluginEdit::EqOn(false));
        host.lock().shortcuts.push(Shortcut::PlayStop);
        // An edit made while the window showed an insert slot's EQ keeps that target even
        // though the window was pointed back at the master before the main frame drained it.
        let slot = EqTarget::Slot { insert: 3, slot: 1 };
        host.lock().eq_target = slot;
        host.lock().push_edit(PluginEdit::EqGain {
            db: 2.0,
            drag_started: true,
        });
        host.lock().eq_target = EqTarget::Master;
        let (edits, shortcuts) = host.take_edits();
        assert_eq!(
            edits,
            vec![
                (PluginEdit::EqOn(false), EqTarget::Master),
                (
                    PluginEdit::EqGain {
                        db: 2.0,
                        drag_started: true
                    },
                    slot
                )
            ]
        );
        assert_eq!(shortcuts, vec![Shortcut::PlayStop]);
        assert!(host.take_edits().0.is_empty());
        let theme = Theme::light();
        host.sync(
            &theme,
            44_100.0,
            true,
            slot,
            "INSERT 3 · SLOT 2",
            &eq::PRESETS[1].state,
            Some(1),
            &reverb::PRESETS[2].state,
            Some(2),
            0.4,
            None,
            [false, true],
        );
        let s = host.lock();
        assert_eq!(s.theme.mode, neu::ThemeMode::Light);
        assert_eq!(s.sample_rate, 44_100.0);
        assert_eq!(s.eq, eq::PRESETS[1].state);
        assert_eq!(s.eq_target, slot);
        assert_eq!(s.eq_target_label, "INSERT 3 · SLOT 2");
        assert_eq!(s.reverb_preset, Some(2));
        assert_eq!(s.windows.open_flags(), [false, true]);
        drop(s);
        let g = host.geometry([true, false]);
        assert!(g.eq.open && !g.reverb.open);
        assert_eq!(g.eq.pos, Some([10.0, 20.0]));
    }
}
