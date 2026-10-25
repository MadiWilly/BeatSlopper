//! Save / load / export glue (M3): builds an `engine::Project` from the UI mirrors, applies a
//! loaded one back (resending everything to the engine) and runs WAV exports on a worker
//! thread.
//!
//! The UI thread never holds an `Engine`, so a project is built from [`AppState`]'s own
//! mirrors (bpm, swing, bank, params, preset, octave, velocity) rather than with
//! `Project::from_engine`; both produce the same file because the mirrors are what the engine
//! was sent. Exports render a fresh offline engine on a `std::thread` and report back over a
//! channel that [`AppState::poll_export`] drains every frame, so neither the UI nor the audio
//! thread ever blocks on disk I/O.
//!
//! M4: applying a project is an undo step (so Open and New can be undone, path included);
//! save / open / new clear [`AppState::dirty`]. New and Open while dirty go through
//! [`AppState::request_new`] / [`AppState::request_open`], which park a
//! [`PendingAction`] for the project panel's DISCARD / KEEP confirm instead of acting.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;

use engine::{ExportStats, ParamId, Project};

use crate::app::{AppState, EXPORT_REPEATS, PendingAction};

/// Default file name offered by the Save As dialog.
pub const DEFAULT_PROJECT_FILE: &str = "beat.beatslopper.json";
/// Default file name offered by the Export dialog.
pub const DEFAULT_WAV_FILE: &str = "beat.wav";

/// What an export renders: the current pattern a number of times, or the whole song chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// The edit pattern, `repeats` times (one repeat = its full 1/2/4-bar length).
    Loop {
        /// One of [`EXPORT_REPEATS`].
        repeats: u32,
    },
    /// The whole chain once (`render_song_wav`).
    Song,
}

impl ExportKind {
    /// Short description for the status line: `"loop x2"` or `"song"`.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            ExportKind::Loop { repeats } => format!("loop x{repeats}"),
            ExportKind::Song => "song".to_owned(),
        }
    }
}

/// A WAV export running on a worker thread.
pub struct ExportJob {
    rx: Receiver<Result<ExportStats, String>>,
    handle: Option<JoinHandle<()>>,
    path: PathBuf,
    kind: ExportKind,
}

impl ExportJob {
    /// Spawns the worker: renders `kind` of `project` at `sample_rate` to `path`.
    pub fn spawn(project: Project, kind: ExportKind, sample_rate: f32, path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let target = path.clone();
        let handle = std::thread::Builder::new()
            .name("beatslopper-export".into())
            .spawn(move || {
                let result = match kind {
                    ExportKind::Loop { repeats } => {
                        engine::export::render_loop_wav(&project, repeats, sample_rate, &target)
                    }
                    ExportKind::Song => {
                        engine::export::render_song_wav(&project, sample_rate, &target)
                    }
                }
                .map_err(|e| e.to_string());
                // The receiver is gone only if the app already dropped the job.
                let _ = tx.send(result);
            })
            .ok();
        Self {
            rx,
            handle,
            path,
            kind,
        }
    }

    /// Where the WAV is being written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What is being rendered.
    #[must_use]
    pub fn kind(&self) -> ExportKind {
        self.kind
    }

    /// `Some` once the worker reported; `None` while it is still rendering.
    fn try_finish(&mut self) -> Option<Result<ExportStats, String>> {
        let result = match self.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("export thread stopped early".to_owned()),
        };
        if let Some(handle) = self.handle.take() {
            // The worker has sent its result, so this returns at once.
            let _ = handle.join();
        }
        Some(result)
    }
}

/// Short display name of a file path (the file name, or the whole path if it has none).
#[must_use]
pub fn file_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl AppState {
    /// `true` while a WAV export is running.
    #[must_use]
    pub fn is_exporting(&self) -> bool {
        self.export_job.is_some()
    }

    /// Snapshot of the UI state as a project (what Save writes and Export renders).
    ///
    /// The `..Project::default()` is deliberate although every field is listed today: a field
    /// the engine adds to `Project` (M7 and later) then defaults instead of breaking the build.
    #[must_use]
    #[allow(clippy::needless_update)]
    pub fn build_project(&self) -> Project {
        let params = ParamId::ALL
            .iter()
            .map(|&id| (id.key().to_owned(), self.param(id)))
            .collect();
        Project {
            version: engine::project::FORMAT_VERSION,
            bpm: self.bpm,
            swing: self.swing,
            bank: self.bank.clone(),
            params,
            mixer: self.mixer,
            preset_index: self.preset_index,
            octave: self.input.octave,
            velocity: self.input.velocity,
            ..Project::default()
        }
    }

    /// Replaces the UI state with `project` and resends everything to the engine. Playback
    /// state, the audio stream and the theme are untouched. One undo step (the caller decides
    /// whether the result counts as dirty).
    pub fn apply_project(&mut self, project: &Project) {
        self.begin_edit();
        self.bpm = if project.bpm.is_finite() {
            project.bpm.clamp(crate::app::MIN_BPM, crate::app::MAX_BPM)
        } else {
            crate::app::DEFAULT_BPM
        };
        self.swing = if project.swing.is_finite() {
            project
                .swing
                .clamp(crate::app::MIN_SWING, crate::app::MAX_SWING)
        } else {
            0.0
        };
        self.preset_index = project
            .preset_index
            .min(crate::presets::ALL.len().saturating_sub(1));
        for &id in ParamId::ALL {
            self.params[id.index()] = id.clamp(project.param(id));
        }
        self.input.set_octave(project.octave);
        self.input.set_velocity(project.velocity);
        // M6: the mixer (a v3 file has none and loads with the defaults).
        self.mixer = project.mixer;
        self.mixer.normalise();
        self.bank = project.bank.clone();
        self.bank.normalise();
        self.selected_step = None;
        self.shown_bar = 0;
        self.song_selected = (self.bank.chain_len > 0).then_some(0);
        self.right_tab = match self.bank.mode {
            engine::PlayMode::Song => crate::app::RightTab::Chain,
            engine::PlayMode::Pattern => crate::app::RightTab::Project,
        };
        self.resend_all();
    }

    /// NEW: an empty bank (every pattern blank, chain `[A x1]`, pattern mode) with every
    /// setting at its default; the file path is forgotten and the project is clean. Undoable
    /// (the previous state and path come back).
    pub fn new_project(&mut self) {
        let project = Project::from_pattern(engine::Pattern::empty());
        self.apply_project(&project);
        self.project_path = None;
        self.pending_action = None;
        self.mark_clean();
        self.set_status("New project");
    }

    /// Saves the current state to `path`, remembers the path and clears the unsaved marker.
    /// Errors become the status line (and the project stays dirty).
    pub fn save_project_to(&mut self, path: PathBuf) -> bool {
        match self.build_project().save(&path) {
            Ok(()) => {
                self.set_status(format!("Saved {}", file_label(&path)));
                self.remember_recent(&path);
                self.project_path = Some(path);
                self.mark_clean();
                true
            }
            Err(err) => {
                self.set_status(format!("Save failed: {err}"));
                false
            }
        }
    }

    /// Loads `path`, applies it (undoably) and remembers the path; the project is clean.
    /// Errors become the status line and leave the state alone.
    pub fn load_project_from(&mut self, path: PathBuf) -> bool {
        match Project::load(&path) {
            Ok(project) => {
                self.apply_project(&project);
                self.set_status(format!("Opened {}", file_label(&path)));
                self.remember_recent(&path);
                self.project_path = Some(path);
                self.pending_action = None;
                self.mark_clean();
                true
            }
            Err(err) => {
                self.set_status(format!("Open failed: {err}"));
                false
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Discard confirm for New / Open.

    /// NEW from a button or Ctrl+N: with unsaved changes this only parks
    /// [`PendingAction::New`] for the DISCARD / KEEP confirm and returns `false`; otherwise the
    /// new project is created now and `true` is returned.
    pub fn request_new(&mut self) -> bool {
        if self.dirty {
            self.pending_action = Some(PendingAction::New);
            false
        } else {
            self.new_project();
            true
        }
    }

    /// OPEN from a button or Ctrl+O: with unsaved changes this parks [`PendingAction::Open`]
    /// and returns `false`; otherwise returns `true` and the caller shows the file dialog.
    pub fn request_open(&mut self) -> bool {
        if self.dirty {
            self.pending_action = Some(PendingAction::Open);
            false
        } else {
            true
        }
    }

    /// DISCARD: takes the pending action and performs the state side of it (a New happens
    /// here; an Open is returned for the caller to show the dialog). `None` when nothing was
    /// pending.
    pub fn confirm_pending(&mut self) -> Option<PendingAction> {
        let action = self.pending_action.take()?;
        match action {
            PendingAction::New => self.new_project(),
            PendingAction::OpenRecent => {
                self.open_pending_recent();
            }
            PendingAction::Open => {}
        }
        Some(action)
    }

    /// KEEP (or Escape): forgets the pending action; nothing else changes.
    pub fn cancel_pending(&mut self) {
        self.pending_action = None;
        self.pending_recent = None;
    }

    /// What EXPORT renders right now: the whole song in song mode, else the edit pattern
    /// `export_repeats` times.
    #[must_use]
    pub fn export_kind(&self) -> ExportKind {
        match self.bank.mode {
            engine::PlayMode::Song => ExportKind::Song,
            engine::PlayMode::Pattern => ExportKind::Loop {
                repeats: if EXPORT_REPEATS.contains(&self.export_repeats) {
                    self.export_repeats
                } else {
                    EXPORT_REPEATS[0]
                },
            },
        }
    }

    /// Starts exporting ([`export_kind`](Self::export_kind)) to `path` on a worker thread.
    /// Refused (with a status line) while another export runs.
    pub fn start_export(&mut self, path: PathBuf) -> bool {
        if self.is_exporting() {
            self.set_status("Already exporting");
            return false;
        }
        let kind = self.export_kind();
        if let ExportKind::Loop { repeats } = kind {
            self.export_repeats = repeats;
        }
        let job = ExportJob::spawn(self.build_project(), kind, self.sample_rate(), path);
        self.export_job = Some(job);
        true
    }

    /// Picks up a finished export: the result becomes the status line, which says what was
    /// exported (`"Exported song (12.0 s) to beat.wav"`, `"Exported loop x2 (6.0 s) to ..."`).
    pub fn poll_export(&mut self) {
        let Some(job) = self.export_job.as_mut() else {
            return;
        };
        let Some(result) = job.try_finish() else {
            return;
        };
        let name = file_label(job.path());
        let what = job.kind().describe();
        self.export_job = None;
        match result {
            Ok(stats) => self.set_status(format!(
                "Exported {what} ({:.1} s) to {name}",
                stats.duration_seconds
            )),
            Err(err) => self.set_status(format!("Export failed: {err}")),
        }
    }

    /// Blocks until a running export has finished (tests and shutdown).
    pub fn finish_export(&mut self) {
        while self.is_exporting() {
            self.poll_export();
            if self.is_exporting() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{DrumRow, Pattern};
    use neu::ThemeMode;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("beatslopper-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn project_round_trips_through_the_app_state() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.set_bpm(133.0);
        state.set_swing(0.3);
        state.apply_preset(3);
        state.set_param(ParamId::FilterCutoff, 1234.0);
        state.set_drum_step(DrumRow::Clap, 7, true, true);
        state.set_synth_step(9, Some(41), true);
        state.set_drum_volume(DrumRow::OpenHat, -9.0);
        state.set_drum_tune(DrumRow::Kick, -2.0);
        state.input.set_octave(3);
        state.input.set_velocity(0.6);
        // M5: a second pattern of two bars with a step in bar 2, a four-bar pattern C, a
        // chain and song mode.
        state.select_pattern(1);
        state.set_pattern_length(engine::PatternLength::Bar2);
        state.set_drum_step(DrumRow::Kick, 20, true, false);
        state.set_synth_step(31, Some(48), true);
        state.select_pattern(2);
        state.set_pattern_length(engine::PatternLength::Bar4);
        state.set_drum_step(DrumRow::Snare, 63, true, true);
        state.add_chain_entry();
        state.set_chain_repeats(3);
        state.select_pattern(1);
        state.add_chain_entry();
        state.set_play_mode(engine::PlayMode::Song);

        let project = state.build_project();
        assert_eq!(project.version, 3.max(engine::project::FORMAT_VERSION));
        assert_eq!(project.bpm, 133.0);
        assert_eq!(project.param(ParamId::FilterCutoff), 1234.0);
        assert_eq!(project.bank, state.bank);
        assert_eq!(project.bank.chain_entries().len(), 3);
        assert_eq!(project.bank.mode, engine::PlayMode::Song);
        assert_eq!(project.bank.current, 1);
        let json = project.to_json_pretty();
        assert!(json.contains("\"mode\": \"song\""));
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back, project);

        let (mut loaded, mut engine) = AppState::headless(ThemeMode::Light);
        loaded.apply_project(&back);
        assert_eq!(loaded.bpm, 133.0);
        assert_eq!(loaded.swing, 0.3);
        assert_eq!(loaded.preset_index, 3);
        assert_eq!(loaded.params, state.params);
        assert_eq!(loaded.bank, state.bank);
        assert_eq!(loaded.current(), 1);
        assert_eq!(loaded.pattern().len(), 32);
        assert_eq!(loaded.bank.patterns[2].len(), 64);
        assert!(loaded.bank.patterns[2].drums[DrumRow::Snare.index()][63].on);
        assert_eq!(loaded.right_tab, crate::app::RightTab::Chain, "song mode");
        assert_eq!(loaded.song_selected, Some(0));
        assert_eq!(loaded.input.octave, 3);
        assert_eq!(loaded.input.velocity, 0.6);
        assert_eq!(loaded.build_project(), project);
        // The load resent everything: the offline engine mirrors the loaded state, the mode
        // included (the engine started in pattern mode).
        while !loaded.outbox.is_empty() {
            engine.process_messages();
            loaded.flush_outbox();
        }
        engine.process_messages();
        assert_eq!(engine.bpm(), 133.0);
        assert_eq!(engine.transport().swing(), 0.3);
        assert_eq!(engine.param(ParamId::FilterCutoff), 1234.0);
        assert_eq!(engine.bank(), &state.bank);
        assert_eq!(engine.play_mode(), engine::PlayMode::Song);
        assert_eq!(engine.edit_pattern(), 1);
    }

    #[test]
    fn save_and_open_through_files_and_new_resets() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.set_bpm(99.0);
        state.toggle_drum_step(DrumRow::Kick, 1, true);
        let path = temp_path("roundtrip.beatslopper.json");
        assert!(state.save_project_to(path.clone()));
        assert_eq!(state.project_path.as_deref(), Some(path.as_path()));
        assert!(state.status_text().unwrap().starts_with("Saved"));

        let (mut other, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(other.load_project_from(path.clone()));
        assert_eq!(other.bpm, 99.0);
        assert_eq!(other.bank, state.bank);
        assert_eq!(other.project_path.as_deref(), Some(path.as_path()));

        other.new_project();
        assert_eq!(other.bpm, crate::app::DEFAULT_BPM);
        assert_eq!(other.pattern().clone(), Pattern::empty());
        assert_eq!(other.project_path, None);

        assert!(!other.load_project_from(temp_path("missing.beatslopper.json")));
        assert!(other.status_text().unwrap().starts_with("Open failed"));
        // A failed open leaves the state alone.
        assert_eq!(other.pattern().clone(), Pattern::empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dirty_flag_follows_edits_saves_opens_and_new() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(!state.dirty, "fresh state is clean");
        state.toggle_drum_step(DrumRow::Kick, 1, false);
        assert!(state.dirty, "an edit dirties");

        let path = temp_path("dirty.beatslopper.json");
        assert!(state.save_project_to(path.clone()));
        assert!(!state.dirty, "save cleans");
        state.set_bpm(100.0);
        assert!(state.dirty);
        state.set_bpm(100.0);
        assert!(state.dirty, "no-op edits do not clean");

        // A failed save keeps the marker.
        assert!(!state.save_project_to(PathBuf::from("Z:/no/such/dir/x.json")));
        assert!(state.dirty);

        assert!(state.load_project_from(path.clone()));
        assert!(!state.dirty, "open cleans");
        assert_eq!(state.bpm, crate::app::DEFAULT_BPM);
        state.set_param(ParamId::Glide, 0.5);
        assert!(state.dirty);
        assert!(!state.load_project_from(temp_path("missing.json")));
        assert!(state.dirty, "a failed open changes nothing");

        state.new_project();
        assert!(!state.dirty, "new cleans");
        state.mark_dirty();
        assert!(state.dirty);
        state.mark_clean();
        assert!(!state.dirty);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn open_and_new_are_undoable_with_their_paths() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.set_bpm(150.0);
        let saved = temp_path("undoable.beatslopper.json");
        assert!(state.save_project_to(saved.clone()));
        let before = state.snapshot();
        assert_eq!(before.project_path.as_deref(), Some(saved.as_path()));

        // NEW: empty, no path, clean; undo brings the beat and its path back (dirty).
        state.new_project();
        assert_eq!(state.pattern().clone(), Pattern::empty());
        assert_eq!(state.project_path, None);
        assert!(!state.dirty);
        assert!(state.undo());
        assert_eq!(state.snapshot(), before);
        assert_eq!(state.project_path.as_deref(), Some(saved.as_path()));
        assert!(state.dirty);
        assert!(state.redo());
        assert_eq!(state.pattern().clone(), Pattern::empty());
        assert_eq!(state.project_path, None);

        // OPEN: the loaded file replaces everything; undo returns to the pre-open state.
        engine.process_messages(); // keep the queue short: every resend is ~140 messages
        state.clear_all();
        state.set_bpm(70.0);
        let pre_open = state.snapshot();
        assert!(state.load_project_from(saved.clone()));
        assert_eq!(state.bpm, 150.0);
        assert!(!state.dirty);
        assert!(state.undo());
        assert_eq!(state.snapshot(), pre_open);
        assert_eq!(state.bpm, 70.0);
        assert_eq!(state.project_path, None);
        engine.process_messages();
        assert_eq!(engine.bpm(), 70.0);
        assert_eq!(engine.pattern(), state.pattern());
        let _ = std::fs::remove_file(saved);
    }

    #[test]
    fn new_and_open_ask_before_discarding_unsaved_changes() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        // Clean: New happens at once, Open may proceed.
        assert!(state.request_new());
        assert_eq!(state.pattern().clone(), Pattern::empty());
        assert_eq!(state.pending_action, None);
        assert!(state.request_open());
        assert_eq!(state.pending_action, None);

        // Dirty: both park a pending action and change nothing.
        state.toggle_drum_step(DrumRow::Snare, 4, false);
        let edited = state.snapshot();
        assert!(!state.request_new());
        assert_eq!(state.pending_action, Some(PendingAction::New));
        assert_eq!(state.snapshot(), edited);
        assert!(!state.request_open());
        assert_eq!(state.pending_action, Some(PendingAction::Open));
        assert_eq!(state.snapshot(), edited);

        // KEEP cancels; nothing is lost.
        state.cancel_pending();
        assert_eq!(state.pending_action, None);
        assert_eq!(state.snapshot(), edited);
        assert!(state.dirty);
        assert_eq!(state.confirm_pending(), None, "nothing pending");

        // DISCARD on Open hands the action back for the dialog; the state is untouched.
        assert!(!state.request_open());
        assert_eq!(state.confirm_pending(), Some(PendingAction::Open));
        assert_eq!(state.pending_action, None);
        assert_eq!(state.snapshot(), edited);

        // DISCARD on New performs the New right here.
        assert!(!state.request_new());
        assert_eq!(state.confirm_pending(), Some(PendingAction::New));
        assert_eq!(state.pattern().clone(), Pattern::empty());
        assert!(!state.dirty);
        assert_eq!(state.pending_action, None);
        // ... and it is undoable.
        assert!(state.undo());
        assert_eq!(state.snapshot(), edited);
    }

    #[test]
    fn export_runs_on_a_worker_and_reports_a_status() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.export_repeats = 2;
        let path = temp_path("export.wav");
        assert!(state.start_export(path.clone()));
        assert!(state.is_exporting());
        assert!(!state.start_export(path.clone()), "one export at a time");
        state.finish_export();
        assert!(!state.is_exporting());
        let status = state.status_text().unwrap().to_owned();
        // 2 bars at 120 BPM = 4 s, plus the 2 s tail.
        assert_eq!(status, "Exported loop x2 (6.0 s) to export.wav", "{status}");
        let bytes = std::fs::metadata(&path).unwrap().len();
        assert!(bytes > 44, "WAV has data: {bytes} bytes");
        let _ = std::fs::remove_file(path);

        // An unwritable path becomes an error status, not a panic.
        let bad = PathBuf::from("Z:/no/such/dir/export.wav");
        assert!(state.start_export(bad));
        state.finish_export();
        assert!(state.status_text().unwrap().starts_with("Export failed"));

        // Song mode renders the whole chain (A A B = 3 bars = 6 s, plus the tail), whatever
        // the BARS choice, and the status says so.
        state.bank = engine::Bank::demo();
        state.bank.truncate(0);
        for p in [0, 0, 1] {
            state.bank.push(engine::ChainEntry::new(p, 1));
        }
        state.set_play_mode(engine::PlayMode::Song);
        assert_eq!(state.export_kind(), ExportKind::Song);
        let song = temp_path("song.wav");
        assert!(state.start_export(song.clone()));
        state.finish_export();
        let status = state.status_text().unwrap().to_owned();
        assert_eq!(status, "Exported song (8.0 s) to song.wav", "{status}");
        let bytes = std::fs::metadata(&song).unwrap().len();
        assert_eq!(
            bytes,
            44 + 8 * 48_000 * 4,
            "3 bars + 2 s tail, 16-bit stereo"
        );
        let _ = std::fs::remove_file(song);
        assert_eq!(ExportKind::Loop { repeats: 4 }.describe(), "loop x4");
    }

    #[test]
    fn mixer_round_trips_through_the_project_and_v3_files_load_defaults() {
        use engine::{Channel, ChannelParam, MixerState};

        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.set_mixer(Channel::Kick, ChannelParam::Level, -6.0);
        state.set_mixer(Channel::Clap, ChannelParam::Pan, 0.4);
        state.set_mixer(Channel::ClosedHat, ChannelParam::Mute, 1.0);
        state.set_mixer(Channel::Snare, ChannelParam::Solo, 1.0);
        state.set_mixer(Channel::Snare, ChannelParam::ReverbSend, 0.8);
        state.set_mixer(Channel::Synth, ChannelParam::DelaySend, 0.25);
        assert_ne!(state.mixer, MixerState::default());

        let project = state.build_project();
        assert_eq!(project.version, engine::project::FORMAT_VERSION);
        assert_eq!(project.mixer, state.mixer);
        let json = project.to_json_pretty();
        assert!(json.contains("\"mixer\""));
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back.mixer, state.mixer);

        let (mut loaded, mut engine) = AppState::headless(ThemeMode::Light);
        loaded.apply_project(&back);
        assert_eq!(loaded.mixer, state.mixer);
        assert_eq!(loaded.strip(Channel::Kick).level_db, -6.0);
        assert!(loaded.strip(Channel::ClosedHat).mute);
        assert!(loaded.strip(Channel::Snare).solo);
        while !loaded.outbox.is_empty() {
            engine.process_messages();
            loaded.flush_outbox();
        }
        engine.process_messages();
        assert_eq!(engine.mixer(), &state.mixer, "the load resent the mixer");

        // Through a file as well.
        let path = temp_path("mixer.beatslopper.json");
        assert!(state.save_project_to(path.clone()));
        let (mut from_file, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(from_file.load_project_from(path.clone()));
        assert_eq!(from_file.mixer, state.mixer);
        let _ = std::fs::remove_file(path);

        // A version-3 file (no `mixer`) loads with the default strips, whatever the mirror
        // held before.
        let v3_text = r#"{ "version": 3, "bpm": 99.0, "swing": 0.1, "preset_index": 2 }"#;
        let old = Project::from_json(v3_text).unwrap();
        assert_eq!(old.version, engine::project::FORMAT_VERSION);
        assert_eq!(old.bpm, 99.0);
        assert_eq!(old.mixer, MixerState::default());
        loaded.apply_project(&old);
        assert_eq!(loaded.bpm, 99.0);
        assert_eq!(loaded.mixer, MixerState::default());
        assert_eq!(loaded.strip(Channel::Kick).level_db, 0.0);
        assert!(!loaded.strip(Channel::ClosedHat).mute);
        // Undo brings the non-default mixer back.
        assert!(loaded.undo());
        assert_eq!(loaded.mixer, state.mixer);
    }

    /// M7: a chord and a held note survive save / open (format version 5), and a v4 file's
    /// one-note-per-step lane loads as length-1 notes.
    #[test]
    fn notes_round_trip_through_a_v5_file_and_v4_lanes_become_notes() {
        use engine::Note;

        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.clear_synth();
        state.add_note(Note::new(0, 4, 48, 0.8));
        state.add_note(Note::new(0, 4, 52, 0.8));
        state.add_note(Note::new(0, 4, 55, 1.0));
        state.add_note(Note::new(8, 6, 43, 0.8));
        let notes = state.pattern().notes().to_vec();
        assert_eq!(notes.len(), 4);

        let project = state.build_project();
        assert_eq!(project.version, 5);
        assert_eq!(engine::project::FORMAT_VERSION, 5);
        let json = project.to_json_pretty();
        assert!(json.contains("\"notes\""));
        assert_eq!(Project::from_json(&json).unwrap(), project);

        let path = temp_path("notes.beatslopper.json");
        assert!(state.save_project_to(path.clone()));
        let (mut loaded, mut engine) = AppState::headless(ThemeMode::Light);
        assert!(loaded.load_project_from(path.clone()));
        assert_eq!(loaded.pattern().notes(), notes.as_slice());
        assert_eq!(loaded.bank, state.bank);
        assert_eq!(loaded.roll_selected, None);
        while !loaded.outbox.is_empty() {
            engine.process_messages();
            loaded.flush_outbox();
        }
        engine.process_messages();
        assert_eq!(
            engine.pattern().notes(),
            notes.as_slice(),
            "the load resent the notes"
        );
        let _ = std::fs::remove_file(path);

        // A v4 file: the lane's steps become length-1 notes.
        let v4_text = r#"{
            "version": 4, "bpm": 110.0,
            "bank": { "patterns": [ { "length": 16, "synth": [
                { "note": 36, "accent": false }, { "note": null, "accent": false },
                { "note": 43, "accent": true } ] } ] }
        }"#;
        let old = Project::from_json(v4_text).unwrap();
        assert_eq!(old.version, 5);
        loaded.apply_project(&old);
        assert_eq!(loaded.bpm, 110.0);
        assert_eq!(
            loaded.pattern().notes(),
            &[Note::step(0, 36, false), Note::step(2, 43, true)]
        );
        assert_eq!(loaded.pattern().synth[2].note, Some(43));
        assert!(loaded.pattern().synth[2].accent);
    }

    #[test]
    fn file_labels_use_the_file_name() {
        assert_eq!(
            file_label(Path::new("C:/songs/beat.beatslopper.json")),
            "beat.beatslopper.json"
        );
    }
}
