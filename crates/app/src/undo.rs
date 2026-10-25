//! Undo / redo (M4): a snapshot stack over everything a project edit can change.
//!
//! A [`Snapshot`] is the UI's own mirror of the engine's musical state (the whole pattern
//! bank, parameters, tempo, swing, preset) plus the project path, so undoing a project *open*
//! also brings the previous file path back and a following Save writes to the right file.
//! Snapshots are cheap (a few kilobytes) so the stack simply stores whole copies:
//! [`UNDO_CAP`] of them, the oldest dropped first.
//!
//! The stack stores the state *before* each edit: `AppState::begin_edit` pushes the current
//! snapshot, then the edit mutates the state. [`UndoStack::undo`] hands that snapshot back and
//! parks the current one on the redo stack; [`UndoStack::redo`] does the reverse. A push
//! clears the redo stack (a new edit after an undo forks the history, as everywhere).
//!
//! Coalescing happens in two places:
//! - [`UndoStack::push`] ignores a snapshot equal to the one on top, so calling `begin_edit`
//!   twice without a change between (a click that edits nothing) adds no entry.
//! - `AppState::begin_continuous_edit` only pushes for the first change of a knob drag: a
//!   change arriving less than `AppState::CONTINUOUS_EDIT_GAP` after the previous change to
//!   the *same* control joins the running entry, unless egui reports `drag_started` on that
//!   frame (a fresh drag always starts a fresh entry). So a drag is one undo step.

use std::path::PathBuf;

use engine::inserts::INSERT_COUNT;
use engine::{Bank, EqState, MAX_TRACKS, MixerState, ParamId, ReverbState, TrackId};

use crate::app::{InsertUi, TrackUi};

/// Most undo steps remembered; older ones are dropped.
pub const UNDO_CAP: usize = 100;

/// Everything an undo step restores.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// All eight patterns, the chain, the play mode and the selected pattern (M5).
    /// All eight patterns, the chain, the play mode and the selected pattern (M5). Boxed:
    /// a bank is 40 KiB (M10) and a snapshot is built on the stack at every edit.
    pub bank: Box<Bank>,
    /// The global parameters (M10: per-track ids live in `track_params`).
    pub params: [f32; ParamId::COUNT],
    /// The eight track slots (M10): name, colour, active flag, preset, modified marker.
    pub tracks: [TrackUi; MAX_TRACKS],
    /// Every slot's per-track parameters (M10).
    pub track_params: [[f32; ParamId::COUNT]; MAX_TRACKS],
    /// The selected track (M10), so undoing a track operation puts the selection back.
    pub edit_track: TrackId,
    pub bpm: f32,
    pub swing: f32,
    /// The 13 mixer strips (M6; 5 drums + 8 tracks since M10), plus the 16 inserts and the
    /// source routes (M11).
    pub mixer: MixerState,
    /// The inserts' names and colours (M11).
    pub insert_ui: [InsertUi; INSERT_COUNT],
    /// The master EQ (plugins milestone).
    pub eq: EqState,
    /// Every reverb value (plugins milestone).
    pub reverb: ReverbState,
    /// The file the project was saved to / opened from, so undoing Open / New restores it.
    pub project_path: Option<PathBuf>,
}

/// Undo and redo stacks of [`Snapshot`]s, capped at [`UNDO_CAP`] undo steps.
#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl UndoStack {
    /// An empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `snapshot` as the state before an edit and forgets any redo history. A snapshot
    /// equal to the one on top is not recorded again. Returns `true` when an entry was added.
    pub fn push(&mut self, snapshot: Snapshot) -> bool {
        self.redo.clear();
        if self.undo.last() == Some(&snapshot) {
            return false;
        }
        if self.undo.len() >= UNDO_CAP {
            self.undo.remove(0);
        }
        self.undo.push(snapshot);
        true
    }

    /// Steps back: returns the snapshot to restore and remembers `current` for [`Self::redo`].
    /// `None` (and no change) when there is nothing to undo.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// Steps forward again: returns the snapshot to restore and remembers `current` for
    /// [`Self::undo`]. `None` (and no change) when there is nothing to redo.
    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    /// Whether [`Self::undo`] would do anything.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether [`Self::redo`] would do anything.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undo steps remembered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// `true` when there is neither undo nor redo history.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty() && self.redo.is_empty()
    }

    /// Redo steps remembered.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// The snapshot an undo would restore, if any.
    #[must_use]
    pub fn peek(&self) -> Option<&Snapshot> {
        self.undo.last()
    }

    /// Forgets all history (startup, after the persisted preset was applied).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        DEFAULT_BPM, default_insert_ui, default_params, default_track_params, default_tracks,
    };

    fn snap(bpm: f32) -> Snapshot {
        Snapshot {
            bank: Box::new(Bank::default()),
            params: default_params(),
            tracks: default_tracks(),
            track_params: default_track_params(),
            edit_track: 0,
            bpm,
            swing: 0.0,
            mixer: MixerState::default(),
            insert_ui: default_insert_ui(),
            eq: EqState::default(),
            reverb: ReverbState::default(),
            project_path: None,
        }
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut stack = UndoStack::new();
        assert!(!stack.can_undo() && !stack.can_redo() && stack.is_empty());
        assert_eq!(stack.undo(snap(1.0)), None);
        assert_eq!(stack.redo(snap(1.0)), None);

        // Edit 1: 120 -> 130; edit 2: 130 -> 140.
        assert!(stack.push(snap(DEFAULT_BPM)));
        assert!(stack.push(snap(130.0)));
        assert_eq!(stack.len(), 2);
        assert!(stack.can_undo() && !stack.can_redo());

        assert_eq!(stack.undo(snap(140.0)), Some(snap(130.0)));
        assert_eq!(stack.undo(snap(130.0)), Some(snap(DEFAULT_BPM)));
        assert_eq!(stack.undo(snap(DEFAULT_BPM)), None, "history exhausted");
        assert!(!stack.can_undo() && stack.can_redo());
        assert_eq!(stack.redo_len(), 2);

        assert_eq!(stack.redo(snap(DEFAULT_BPM)), Some(snap(130.0)));
        assert_eq!(stack.redo(snap(130.0)), Some(snap(140.0)));
        assert_eq!(stack.redo(snap(140.0)), None);
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn a_push_after_undo_forgets_the_redo_branch() {
        let mut stack = UndoStack::new();
        stack.push(snap(120.0));
        stack.push(snap(130.0));
        stack.undo(snap(140.0));
        assert!(stack.can_redo());
        stack.push(snap(130.0));
        assert!(!stack.can_redo());
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn equal_snapshots_coalesce_on_top() {
        let mut stack = UndoStack::new();
        assert!(stack.push(snap(120.0)));
        assert!(!stack.push(snap(120.0)), "same as the top: not recorded");
        assert_eq!(stack.len(), 1);
        assert!(stack.push(snap(121.0)));
        // Returning to an earlier value is a different top, so it is recorded.
        assert!(stack.push(snap(120.0)));
        assert_eq!(stack.len(), 3);
        assert_eq!(stack.peek(), Some(&snap(120.0)));
    }

    #[test]
    fn the_stack_is_capped_and_drops_the_oldest() {
        let mut stack = UndoStack::new();
        for i in 0..(UNDO_CAP + 25) {
            stack.push(snap(i as f32));
        }
        assert_eq!(stack.len(), UNDO_CAP);
        // The oldest 25 are gone: the deepest undo restores entry 25.
        let mut last = None;
        let mut current = snap(9999.0);
        while let Some(s) = stack.undo(current.clone()) {
            current = s.clone();
            last = Some(s);
        }
        assert_eq!(last, Some(snap(25.0)));
        assert_eq!(stack.redo_len(), UNDO_CAP);

        stack.clear();
        assert!(stack.is_empty());
    }
}
