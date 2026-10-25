//! Quality-of-life state methods (QOL pass), kept out of `app.rs` so the shared file only
//! gains fields and hooks. See `docs/ARCHITECTURE.md` "QOL".
//!
//! - Tempo: [`AppState::nudge_bpm`] (`+` / `-`, Ctrl+Up / Ctrl+Down, the wheel over the BPM
//!   group) and [`AppState::tap_tempo_at`] (the TAP pill: the mean of the last four taps).
//! - Transport: [`AppState::rewind`] (Home while playing restarts the loop at step 0),
//!   [`AppState::escape`] (one Escape stops the test tone and deselects, a second within
//!   [`ESCAPE_DOUBLE_TAP`] is the panic: every note off) and [`AppState::panic`].
//! - Sequencer: drag-painting strokes ([`AppState::begin_paint`], [`AppState::paint_step`];
//!   one undo step per stroke), the accent toggles for Ctrl-click, right-click clearing,
//!   row mute / solo on a double-clicked row label (through the mixer strip of the row's
//!   channel, [`row_channel`]), the `x2` / `/2` pattern-length pills
//!   ([`AppState::double_pattern_length`] copies the content into the new half when that
//!   half is empty, [`AppState::halve_pattern_length`] keeps the hidden steps) and the
//!   Up / Down / PageUp / PageDown nudge of the selected bass step's note.
//! - Projects: the RECENT list ([`AppState::remember_recent`] on every save / open,
//!   [`AppState::request_open_recent`] with the usual DISCARD / KEEP confirm when dirty, and
//!   [`AppState::cycle_recent`] for Ctrl+Shift+O).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use engine::{Channel, DrumRow, Message, Note, PatternLength, STEPS};

use crate::app::{AppState, LaneView, MAX_BPM, MIN_BPM, PendingAction};
use crate::drag_paint::{PaintRow, PaintStroke};
use crate::tempo::nudge_bpm;

/// Two Escapes closer together than this are the panic (all notes off).
pub const ESCAPE_DOUBLE_TAP: Duration = Duration::from_millis(400);
/// Semitones one PageUp / PageDown moves the selected bass note.
pub const OCTAVE_SEMITONES: i32 = 12;

/// The mixer channel a sequencer row's mute / solo acts on.
#[must_use]
pub fn row_channel(row: PaintRow) -> Channel {
    match row {
        PaintRow::Drum(DrumRow::Kick) => Channel::Kick,
        PaintRow::Drum(DrumRow::Snare) => Channel::Snare,
        PaintRow::Drum(DrumRow::Clap) => Channel::Clap,
        PaintRow::Drum(DrumRow::ClosedHat) => Channel::ClosedHat,
        PaintRow::Drum(DrumRow::OpenHat) => Channel::OpenHat,
        PaintRow::Bass => Channel::Synth,
    }
}

impl AppState {
    // -----------------------------------------------------------------------------------------
    // Tempo.

    /// Moves the tempo by `steps` whole BPM (see [`crate::tempo::nudge_bpm`]); one undo step
    /// per change, like the stepper buttons.
    pub fn nudge_bpm(&mut self, steps: i32) {
        let bpm = nudge_bpm(self.bpm, steps);
        if bpm != self.bpm {
            self.set_bpm(bpm);
        }
    }

    /// Records a TAP at `now` (seconds on the UI clock, `egui::InputState::time`); from the
    /// second tap on the tempo follows the taps. Returns the tempo set, if any.
    pub fn tap_tempo_at(&mut self, now: f64) -> Option<f32> {
        let bpm = self.tap.tap(now)?;
        let bpm = (bpm * 10.0).round() / 10.0;
        let bpm = bpm.clamp(MIN_BPM, MAX_BPM);
        if bpm != self.bpm {
            self.set_bpm(bpm);
        }
        Some(bpm)
    }

    // -----------------------------------------------------------------------------------------
    // Transport and global keys.

    /// Home: while playing, restarts the loop at step 0 of the current pattern (song mode:
    /// chain entry 0) without stopping. Nothing when stopped (the engine is already rewound).
    pub fn rewind(&mut self) {
        if self.playing {
            self.send(Message::Play);
        }
    }

    /// Escape at `now`: cancels a pending DISCARD / KEEP, drops the bass and roll selections
    /// and stops the test tone. A second Escape within [`ESCAPE_DOUBLE_TAP`] is the panic
    /// ([`Self::panic`]). Returns `true` when it panicked.
    pub fn escape_at(&mut self, now: Instant) -> bool {
        let double = self
            .last_escape
            .is_some_and(|last| now.saturating_duration_since(last) <= ESCAPE_DOUBLE_TAP);
        self.last_escape = Some(now);
        if self.pending_action.is_some() {
            self.cancel_pending();
        }
        self.selected_step = None;
        self.roll_selected = None;
        if self.test_tone {
            self.set_test_tone(false);
        }
        if double {
            self.panic();
        }
        double
    }

    /// [`Self::escape_at`] now.
    pub fn escape(&mut self) -> bool {
        self.escape_at(Instant::now())
    }

    /// Panic: every sounding note off (keyboard, MIDI, sequencer voices), the test tone off,
    /// the record arm cleared, and a status line. The transport keeps running.
    pub fn panic(&mut self) {
        self.all_notes_off();
        if self.test_tone {
            self.set_test_tone(false);
        }
        self.armed = false;
        self.set_status("Panic: all notes off");
    }

    // -----------------------------------------------------------------------------------------
    // Sequencer: drag painting, accents, clearing, row mute / solo, length pills, note nudge.

    /// Starts a paint stroke on `row` that sets cells to `on`, painting `step` at once. One
    /// undo step for the whole stroke.
    pub fn begin_paint(&mut self, row: PaintRow, on: bool, step: usize) {
        self.begin_edit();
        let mut stroke = PaintStroke::new(row, on);
        stroke.mark(step);
        self.paint_raw(row, step, on);
        self.paint = Some(stroke);
    }

    /// Paints `step` of `row` with the running stroke, if there is one for that row and the
    /// cell is not in the stroke's state yet. Returns `true` when the cell changed.
    pub fn paint_step(&mut self, row: PaintRow, step: usize) -> bool {
        let current_on = self.cell_on(row, step);
        let Some(stroke) = self.paint.as_mut() else {
            return false;
        };
        if !stroke.wants(row, step, current_on) {
            return false;
        }
        stroke.mark(step);
        let on = stroke.on;
        self.paint_raw(row, step, on);
        true
    }

    /// Ends the running stroke (the button came up).
    pub fn end_paint(&mut self) {
        self.paint = None;
    }

    /// Whether cell `step` of `row` is on (a drum hit, or a bass note).
    #[must_use]
    pub fn cell_on(&self, row: PaintRow, step: usize) -> bool {
        let step = step % self.pattern().len();
        match row {
            PaintRow::Drum(drum) => self.pattern().drums[drum.index()][step].on,
            PaintRow::Bass => self.pattern().synth[step].note.is_some(),
        }
    }

    /// Sets a cell without an undo step (the stroke took it): a drum step on / off (an
    /// unaccented hit), or a bass rest (painting a bass cell "on" gives it the keyboard's C,
    /// like a shift-click on an empty cell).
    fn paint_raw(&mut self, row: PaintRow, step: usize, on: bool) {
        let step = step % self.pattern().len();
        match row {
            PaintRow::Drum(drum) => {
                self.pattern_mut()
                    .set_drum_step(drum, step as u8, on, false);
                self.send(Message::SetDrumStep {
                    row: drum,
                    step: step as u8,
                    on,
                    accent: false,
                });
            }
            PaintRow::Bass => {
                let note = on.then(|| self.input.base_note());
                self.pattern_mut().set_synth_step(step as u8, note, false);
                self.send(Message::SetSynthStep {
                    step: step as u8,
                    note,
                    accent: false,
                });
            }
        }
        self.dirty = true;
    }

    /// Ctrl-click on a drum cell: turns it on and flips its accent (same as shift-click).
    pub fn toggle_drum_accent(&mut self, row: DrumRow, step: usize) {
        self.toggle_drum_step(row, step, true);
    }

    /// Ctrl-click on a bass cell: flips the accent of its note (an empty cell gets the
    /// keyboard's C, like shift-click).
    pub fn toggle_synth_accent(&mut self, step: usize) {
        self.click_synth_step(step, true);
    }

    /// Right-click on a bass cell: clears its note (one undo step when there was one).
    pub fn clear_synth_step(&mut self, step: usize) {
        let step = step % self.pattern().len();
        if self.pattern().synth[step].note.is_some() {
            self.set_synth_step(step, None, false);
        }
        if self.selected_step == Some(step) {
            self.selected_step = None;
        }
    }

    /// Double-click on a row label: flips the mute of the row's mixer channel.
    pub fn toggle_row_mute(&mut self, row: PaintRow) {
        self.toggle_mute(row_channel(row));
    }

    /// Shift + double-click on a row label: flips the solo of the row's mixer channel.
    pub fn toggle_row_solo(&mut self, row: PaintRow) {
        self.toggle_solo(row_channel(row));
    }

    /// Whether the row's channel is muted (its label dims).
    #[must_use]
    pub fn row_muted(&self, row: PaintRow) -> bool {
        self.mixer.strip(row_channel(row)).mute
    }

    /// Whether the row's channel is soloed (its label gets the accent dot).
    #[must_use]
    pub fn row_soloed(&self, row: PaintRow) -> bool {
        self.mixer.strip(row_channel(row)).solo
    }

    /// `x2`: doubles the edit pattern's length (16 -> 32 -> 64). When the new half is empty
    /// (no hits, no notes starting there) the first half is copied into it, so the loop
    /// sounds the same and can then be varied. One undo step; `false` at 64 steps.
    pub fn double_pattern_length(&mut self) -> bool {
        let len = self.pattern().len();
        let Some(length) = PatternLength::from_steps((len * 2).min(u8::MAX as usize) as u8) else {
            return false;
        };
        if length.steps() as usize <= len {
            return false;
        }
        self.begin_edit();
        let new_len = length.steps() as usize;
        let pattern = self.pattern_mut();
        let half_empty = DrumRow::ALL.iter().all(|row| {
            pattern.drums[row.index()][len..new_len]
                .iter()
                .all(|s| !s.on)
        }) && pattern.notes().iter().all(|n| usize::from(n.start) < len);
        pattern.set_length(length);
        if half_empty {
            for row in DrumRow::ALL {
                let (first, second) = pattern.drums[row.index()].split_at_mut(len);
                second[..len].copy_from_slice(&first[..len]);
            }
            let shifted: Vec<Note> = pattern
                .notes()
                .iter()
                .filter(|n| usize::from(n.start) < len)
                .map(|n| Note::new(n.start + len as u8, n.length, n.pitch, n.velocity))
                .collect();
            for note in shifted {
                // Stops quietly when the pattern is full (64 notes).
                let _ = pattern.add_note(note);
            }
        }
        self.clamp_shown_bar_qol();
        self.send_pattern();
        true
    }

    /// `/2`: halves the edit pattern's length (64 -> 32 -> 16); the hidden steps are kept
    /// and come back with `x2`. One undo step; `false` at 16 steps.
    pub fn halve_pattern_length(&mut self) -> bool {
        let len = self.pattern().len();
        if len <= STEPS {
            return false;
        }
        let Some(length) = PatternLength::from_steps((len / 2) as u8) else {
            return false;
        };
        self.set_pattern_length(length);
        true
    }

    /// Keeps the shown bar and the selections inside the (new) pattern length.
    fn clamp_shown_bar_qol(&mut self) {
        let bars = self.pattern().bars();
        self.shown_bar = self.shown_bar.min(bars.saturating_sub(1));
        if let Some(step) = self.selected_step
            && step >= self.pattern().len()
        {
            self.selected_step = None;
        }
    }

    /// Up / Down (`semitones` = +-1) or PageUp / PageDown (+-12) with a bass step selected in
    /// the grid: moves its note, clamped to MIDI, keeping the accent. `false` when no note is
    /// selected or the note cannot move.
    pub fn nudge_selected_note(&mut self, semitones: i32) -> bool {
        if self.lane_view != LaneView::Grid {
            return false;
        }
        let Some(step) = self.selected_step else {
            return false;
        };
        let step = step % self.pattern().len();
        let cell = self.pattern().synth[step];
        let Some(note) = cell.note else {
            return false;
        };
        let moved = (i32::from(note) + semitones).clamp(0, 127) as u8;
        if moved == note {
            return false;
        }
        self.set_synth_step(step, Some(moved), cell.accent);
        true
    }

    // -----------------------------------------------------------------------------------------
    // Recent projects.

    /// Puts `path` at the front of the RECENT list (every save and open).
    pub fn remember_recent(&mut self, path: &Path) {
        self.recent.push(path.to_path_buf());
    }

    /// Opens a RECENT entry: with unsaved changes this parks [`PendingAction::OpenRecent`]
    /// for the DISCARD / KEEP confirm and returns `false`; otherwise loads it now. A file
    /// that is gone is dropped from the list with a status line.
    pub fn request_open_recent(&mut self, path: PathBuf) -> bool {
        if !path.is_file() {
            self.recent.remove(&path);
            self.set_status(format!("Missing: {}", crate::project::file_label(&path)));
            return false;
        }
        if self.dirty {
            self.pending_recent = Some(path);
            self.pending_action = Some(PendingAction::OpenRecent);
            return false;
        }
        self.load_project_from(path)
    }

    /// DISCARD after a RECENT open: loads the parked path.
    pub fn open_pending_recent(&mut self) -> bool {
        match self.pending_recent.take() {
            Some(path) => self.load_project_from(path),
            None => false,
        }
    }

    /// Ctrl+Shift+O: opens the next RECENT entry after the current project (wrapping; the
    /// most recent one when the current project is not listed), through
    /// [`Self::request_open_recent`]. `false` when there is nothing to cycle to.
    pub fn cycle_recent(&mut self) -> bool {
        self.recent.retain_existing();
        let Some(next) = self
            .recent
            .next_after(self.project_path.as_deref())
            .map(Path::to_path_buf)
        else {
            self.set_status("No other recent project");
            return false;
        };
        self.request_open_recent(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::EXPORT_REPEATS;
    use engine::{ChannelParam, Pattern, PlayMode};
    use neu::ThemeMode;

    fn drain(state: &mut AppState, engine: &mut engine::Engine) {
        state.flush_outbox();
        let mut buf = vec![0.0; 2 * 64];
        engine.render(&mut buf, 64);
    }

    #[test]
    fn bpm_nudges_and_taps_reach_the_engine_as_undo_steps() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.nudge_bpm(5);
        assert_eq!(state.bpm, 125.0);
        state.nudge_bpm(-1);
        assert_eq!(state.bpm, 124.0);
        assert_eq!(state.undo.len(), 2);
        drain(&mut state, &mut engine);
        assert_eq!(engine.transport().bpm(), 124.0);

        let t0 = 10.0;
        assert_eq!(state.tap_tempo_at(t0), None);
        let bpm = state.tap_tempo_at(t0 + 0.4).unwrap();
        assert!((bpm - 150.0).abs() < 0.2, "{bpm}");
        assert_eq!(state.bpm, bpm);
        drain(&mut state, &mut engine);
        assert!((engine.transport().bpm() - 150.0).abs() < 0.2);
        assert!(state.tap.is_flashing(t0 + 0.5));
        let _ = EXPORT_REPEATS;
    }

    #[test]
    fn rewind_restarts_only_while_playing() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        state.rewind();
        drain(&mut state, &mut engine);
        assert!(!engine.transport().is_playing());
        state.play();
        drain(&mut state, &mut engine);
        let mut buf = vec![0.0; 2 * 4096];
        engine.render(&mut buf, 4096);
        assert!(engine.position().pattern_step() > 0 || engine.position().frac > 0.0);
        state.rewind();
        drain(&mut state, &mut engine);
        let pos = engine.position();
        assert!(pos.playing);
        assert_eq!(pos.pattern_step(), 0);
    }

    #[test]
    fn escape_deselects_stops_the_tone_and_double_tap_panics() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.set_test_tone(true);
        state.selected_step = Some(3);
        state.armed = true;
        state.note_on(60, 0.8);
        let t0 = Instant::now();
        assert!(!state.escape_at(t0));
        assert!(!state.test_tone);
        assert_eq!(state.selected_step, None);
        assert!(state.held_notes[60], "one Escape keeps notes sounding");
        assert!(state.armed);
        assert!(
            state.escape_at(t0 + Duration::from_millis(200)),
            "double tap"
        );
        assert!(!state.held_notes[60]);
        assert!(!state.armed);
        assert_eq!(state.status_text(), Some("Panic: all notes off"));
        // A third, late Escape is a plain one again.
        assert!(!state.escape_at(t0 + Duration::from_secs(2)));
    }

    #[test]
    fn a_paint_stroke_sets_cells_once_as_one_undo_step() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        let kick = PaintRow::Drum(DrumRow::Kick);
        // Demo: kick step 2 is off; the press turns it on and the stroke paints 3 and 4.
        state.begin_paint(kick, true, 1);
        assert!(state.paint_step(kick, 2));
        assert!(state.paint_step(kick, 3));
        assert!(!state.paint_step(kick, 2), "once per stroke");
        assert!(!state.paint_step(kick, 1));
        assert!(
            !state.paint_step(PaintRow::Drum(DrumRow::Snare), 2),
            "other rows untouched"
        );
        let drums = &state.pattern().drums[DrumRow::Kick.index()];
        assert!(drums[1].on && drums[2].on && drums[3].on);
        assert!(!drums[1].accent);
        assert!(!state.pattern().drums[DrumRow::Snare.index()][2].on);
        assert_eq!(state.undo.len(), 1, "one stroke, one undo step");
        assert!(state.dirty);
        state.end_paint();
        assert!(state.paint.is_none());
        drain(&mut state, &mut engine);
        assert!(engine.pattern().drums[DrumRow::Kick.index()][3].on);

        // An erase stroke, then undo brings everything back in one step.
        state.begin_paint(kick, false, 0);
        assert!(state.paint_step(kick, 1));
        assert!(!state.pattern().drums[DrumRow::Kick.index()][0].on);
        state.end_paint();
        assert_eq!(state.undo.len(), 2);
        state.undo();
        state.undo();
        assert_eq!(state.pattern().clone(), Pattern::demo());
    }

    #[test]
    fn bass_strokes_only_erase_and_right_click_clears() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(
            state.pattern().synth[0].note.is_some(),
            "demo bass on step 1"
        );
        state.begin_paint(PaintRow::Bass, false, 0);
        assert!(state.paint_step(PaintRow::Bass, 2));
        assert_eq!(state.pattern().synth[0].note, None);
        assert_eq!(state.pattern().synth[2].note, None);
        state.end_paint();
        state.selected_step = Some(4);
        state.clear_synth_step(4);
        assert_eq!(state.pattern().synth[4].note, None);
        assert_eq!(state.selected_step, None);
    }

    #[test]
    fn row_labels_mute_and_solo_the_mixer_channel() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        assert_eq!(
            row_channel(PaintRow::Drum(DrumRow::ClosedHat)),
            Channel::ClosedHat
        );
        assert_eq!(row_channel(PaintRow::Bass), Channel::Synth);
        state.toggle_row_mute(PaintRow::Drum(DrumRow::ClosedHat));
        assert!(state.row_muted(PaintRow::Drum(DrumRow::ClosedHat)));
        state.toggle_row_solo(PaintRow::Bass);
        assert!(state.row_soloed(PaintRow::Bass));
        drain(&mut state, &mut engine);
        assert_eq!(
            engine.mixer().get(Channel::ClosedHat, ChannelParam::Mute),
            1.0
        );
        assert_eq!(engine.mixer().get(Channel::Synth, ChannelParam::Solo), 1.0);
        state.toggle_row_mute(PaintRow::Drum(DrumRow::ClosedHat));
        assert!(!state.row_muted(PaintRow::Drum(DrumRow::ClosedHat)));
    }

    #[test]
    fn x2_copies_the_loop_into_the_new_half_and_half_keeps_hidden_steps() {
        let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
        let demo = Pattern::demo();
        assert!(state.double_pattern_length());
        assert_eq!(state.pattern().len(), 32);
        for row in DrumRow::ALL {
            let drums = &state.pattern().drums[row.index()];
            assert_eq!(drums[16..32], demo.drums[row.index()][0..16], "{row:?}");
        }
        let notes = state.pattern().notes();
        assert_eq!(notes.len(), demo.notes().len() * 2);
        assert!(notes.iter().any(|n| n.start == 16));
        drain(&mut state, &mut engine);
        assert_eq!(engine.bank().current_pattern().len(), 32);
        assert!(engine.bank().current_pattern().drums[DrumRow::Kick.index()][16].on);

        // Vary the second half, halve, double again: the edited half is still there.
        state.toggle_drum_step(DrumRow::Kick, 17, false);
        assert!(state.halve_pattern_length());
        assert_eq!(state.pattern().len(), 16);
        assert!(state.double_pattern_length());
        assert!(
            state.pattern().drums[DrumRow::Kick.index()][17].on,
            "hidden steps kept"
        );
        assert!(state.double_pattern_length());
        assert_eq!(state.pattern().len(), 64);
        assert!(!state.double_pattern_length(), "64 is the maximum");
        for _ in 0..2 {
            assert!(state.halve_pattern_length());
        }
        assert!(!state.halve_pattern_length(), "16 is the minimum");
        assert!(state.undo.len() >= 6);
    }

    #[test]
    fn selected_note_nudges_by_semitones_and_octaves() {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        state.selected_step = Some(0);
        let note = state.pattern().synth[0].note.unwrap();
        assert!(state.nudge_selected_note(1));
        assert_eq!(state.pattern().synth[0].note, Some(note + 1));
        assert!(state.nudge_selected_note(OCTAVE_SEMITONES));
        assert_eq!(state.pattern().synth[0].note, Some(note + 13));
        assert!(state.nudge_selected_note(-200));
        assert_eq!(state.pattern().synth[0].note, Some(0));
        assert!(!state.nudge_selected_note(-1), "already at the bottom");
        state.selected_step = Some(1);
        assert!(
            !state.nudge_selected_note(1),
            "an empty step has no note to move"
        );
        state.set_lane_view(LaneView::Roll);
        state.selected_step = Some(0);
        assert!(!state.nudge_selected_note(1), "grid only");
        assert_eq!(state.bank.mode, PlayMode::Pattern);
    }

    #[test]
    fn recent_list_follows_saves_and_cycles_with_the_discard_confirm() {
        let dir = std::env::temp_dir().join(format!("beatslopper-qol-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.beatslopper.json");
        let b = dir.join("b.beatslopper.json");
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        assert!(state.save_project_to(a.clone()));
        state.toggle_drum_step(DrumRow::Kick, 1, false);
        assert!(state.save_project_to(b.clone()));
        assert_eq!(state.recent.paths(), &[b.clone(), a.clone()]);
        assert_eq!(state.project_path.as_deref(), Some(b.as_path()));

        // Clean: cycling opens the next entry (a) right away.
        assert!(state.cycle_recent());
        assert_eq!(state.project_path.as_deref(), Some(a.as_path()));
        assert_eq!(
            state.recent.paths(),
            &[a.clone(), b.clone()],
            "opening re-orders"
        );

        // Dirty: it asks first; DISCARD opens.
        state.toggle_drum_step(DrumRow::Snare, 1, false);
        assert!(!state.cycle_recent());
        assert_eq!(state.pending_action, Some(PendingAction::OpenRecent));
        assert_eq!(state.pending_recent.as_deref(), Some(b.as_path()));
        assert_eq!(state.confirm_pending(), Some(PendingAction::OpenRecent));
        assert_eq!(state.project_path.as_deref(), Some(b.as_path()));
        assert!(!state.dirty);
        assert_eq!(state.pending_recent, None);

        // A missing file is dropped from the list.
        let gone = dir.join("gone.beatslopper.json");
        state.remember_recent(&gone);
        assert!(!state.request_open_recent(gone.clone()));
        assert!(!state.recent.paths().contains(&gone));
        assert!(state.status_text().unwrap().starts_with("Missing"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
