//! Keyboard shortcuts (M4): a pure mapping from this frame's egui input to [`Shortcut`]s.
//!
//! | Keys                     | Action                                   |
//! |--------------------------|------------------------------------------|
//! | Space                    | play / stop                              |
//! | R                        | record arm                               |
//! | Ctrl+Z / Ctrl+Shift+Z    | undo / redo                              |
//! | Ctrl+Y                   | redo                                     |
//! | Ctrl+S / Ctrl+Shift+S    | save (Save As without a path) / save as  |
//! | Ctrl+O                   | open                                     |
//! | Ctrl+E                   | export WAV (current repeats)             |
//! | Ctrl+N                   | new                                      |
//! | Ctrl+Shift+O             | open the next RECENT project (QOL)       |
//! | + / = / Ctrl+Up          | tempo up 1 BPM (QOL)                     |
//! | - / Ctrl+Down            | tempo down 1 BPM (QOL)                   |
//! | Ctrl+T                   | add an instrument track (M10)            |
//! | Ctrl+Shift+D             | duplicate the edit track (M10)           |
//! | Alt+Up / Alt+Down        | select the previous / next track (M10)   |
//! | Ctrl+PageUp / PageDown   | previous / next preset (M9)              |
//! | Ctrl+Shift+P             | open the preset browser, focus search    |
//! | Ctrl+L                   | link the edit track to a free insert (M11)|
//! | F5 / F6 / F7 / F8        | piano roll / mixer / synth & fx / song view (M12) |
//! | F9                       | toggle the BROWSER column (M12)          |
//! | F11                      | the visualizer over the workspace (M12)  |
//!
//! Home (rewind while playing), Escape (deselect / test tone off; twice = panic; closes the
//! preset browser) and the Up / Down / PageUp / PageDown note nudges of a selected bass
//! step are handled in `AppState::handle_keyboard`, because they depend on focus and
//! selection state. The preset steps are Ctrl+PageUp / PageDown because Ctrl+Up / Down are
//! the tempo nudge (the plan's Ctrl+Up / Down were taken by the QOL pass first).
//!
//! `Ctrl` is egui's `COMMAND` (Cmd on macOS). The keys are *consumed* so nothing else in the
//! frame sees them: Space and R are not piano keys and the piano ignores Ctrl combinations
//! anyway (`crate::input`), but consuming keeps that true for any future widget too. The
//! caller decides when shortcuts apply at all (not while a text field is focused, never in
//! `--smoke-test`); see `AppState::take_shortcuts`.

use egui::{InputState, Key, Modifiers};

/// `Ctrl+Shift` (Cmd+Shift on macOS), spelled out because `Modifiers | Modifiers` is not `const`.
pub const COMMAND_SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: true,
};

/// One keyboard shortcut, in the order they are checked (most specific first, so that
/// `Ctrl+Shift+S` is Save As and never Save: egui's key matching ignores extra Shift).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shortcut {
    SaveAs,
    OpenRecent,
    Redo,
    Save,
    Undo,
    Open,
    Export,
    New,
    PlayStop,
    ToggleArm,
    BpmUp,
    BpmDown,
    /// Ctrl+Shift+D: clone the edit track (M10). Before `AddTrack` only by convention; no
    /// plain Ctrl+D exists.
    DuplicateTrack,
    /// Ctrl+T: a new track (M10).
    AddTrack,
    /// Alt+Up: the previous active track (M10).
    PrevTrack,
    /// Alt+Down: the next active track (M10).
    NextTrack,
    /// Ctrl+PageUp: the previous preset of the browser's category filter, wrapping (M9).
    PrevPreset,
    /// Ctrl+PageDown: the next preset of the browser's category filter, wrapping (M9).
    NextPreset,
    /// Ctrl+Shift+P: open the preset browser with the search field focused (M9).
    FindPreset,
    /// Ctrl+L: route the edit track to the first free mixer insert (M11, FL's Ctrl+L).
    LinkInsert,
    /// F5: the PIANO ROLL view (and the rack re-expands) (M12).
    ViewRoll,
    /// F6: the MIXER view (M12; the drawer in M13).
    ViewMixer,
    /// F7: the SYNTH & FX view (M12).
    ViewSynth,
    /// F8: the SONG view (M12).
    ViewSong,
    /// F9: show / hide the BROWSER column (M12).
    ToggleBrowser,
    /// F11: the visualizer over the workspace until Escape (M12).
    VisualFull,
}

impl Shortcut {
    /// Every shortcut, in check order.
    pub const ALL: [Shortcut; 26] = [
        Shortcut::SaveAs,
        Shortcut::OpenRecent,
        Shortcut::Redo,
        Shortcut::Save,
        Shortcut::Undo,
        Shortcut::Open,
        Shortcut::Export,
        Shortcut::New,
        Shortcut::PlayStop,
        Shortcut::ToggleArm,
        Shortcut::BpmUp,
        Shortcut::BpmDown,
        Shortcut::DuplicateTrack,
        Shortcut::AddTrack,
        Shortcut::PrevTrack,
        Shortcut::NextTrack,
        Shortcut::PrevPreset,
        Shortcut::NextPreset,
        Shortcut::FindPreset,
        Shortcut::LinkInsert,
        Shortcut::ViewRoll,
        Shortcut::ViewMixer,
        Shortcut::ViewSynth,
        Shortcut::ViewSong,
        Shortcut::ToggleBrowser,
        Shortcut::VisualFull,
    ];

    /// The key combinations that trigger this shortcut.
    #[must_use]
    pub fn bindings(self) -> &'static [(Modifiers, Key)] {
        match self {
            Shortcut::SaveAs => &[(COMMAND_SHIFT, Key::S)],
            Shortcut::OpenRecent => &[(COMMAND_SHIFT, Key::O)],
            Shortcut::Redo => &[(COMMAND_SHIFT, Key::Z), (Modifiers::COMMAND, Key::Y)],
            Shortcut::Save => &[(Modifiers::COMMAND, Key::S)],
            Shortcut::Undo => &[(Modifiers::COMMAND, Key::Z)],
            Shortcut::Open => &[(Modifiers::COMMAND, Key::O)],
            Shortcut::Export => &[(Modifiers::COMMAND, Key::E)],
            Shortcut::New => &[(Modifiers::COMMAND, Key::N)],
            Shortcut::PlayStop => &[(Modifiers::NONE, Key::Space)],
            Shortcut::ToggleArm => &[(Modifiers::NONE, Key::R)],
            // `+` arrives with shift on most layouts; egui's logical matching tolerates it.
            Shortcut::BpmUp => &[
                (Modifiers::NONE, Key::Plus),
                (Modifiers::NONE, Key::Equals),
                (Modifiers::COMMAND, Key::ArrowUp),
            ],
            Shortcut::BpmDown => &[
                (Modifiers::NONE, Key::Minus),
                (Modifiers::COMMAND, Key::ArrowDown),
            ],
            Shortcut::DuplicateTrack => &[(COMMAND_SHIFT, Key::D)],
            Shortcut::AddTrack => &[(Modifiers::COMMAND, Key::T)],
            Shortcut::PrevTrack => &[(Modifiers::ALT, Key::ArrowUp)],
            Shortcut::NextTrack => &[(Modifiers::ALT, Key::ArrowDown)],
            Shortcut::PrevPreset => &[(Modifiers::COMMAND, Key::PageUp)],
            Shortcut::NextPreset => &[(Modifiers::COMMAND, Key::PageDown)],
            Shortcut::FindPreset => &[(COMMAND_SHIFT, Key::P)],
            Shortcut::LinkInsert => &[(Modifiers::COMMAND, Key::L)],
            Shortcut::ViewRoll => &[(Modifiers::NONE, Key::F5)],
            Shortcut::ViewMixer => &[(Modifiers::NONE, Key::F6)],
            Shortcut::ViewSynth => &[(Modifiers::NONE, Key::F7)],
            Shortcut::ViewSong => &[(Modifiers::NONE, Key::F8)],
            Shortcut::ToggleBrowser => &[(Modifiers::NONE, Key::F9)],
            Shortcut::VisualFull => &[(Modifiers::NONE, Key::F11)],
        }
    }

    /// Human-readable binding for tooltips, e.g. `"Ctrl+Shift+S"`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Shortcut::SaveAs => "Ctrl+Shift+S",
            Shortcut::OpenRecent => "Ctrl+Shift+O",
            Shortcut::Redo => "Ctrl+Y",
            Shortcut::Save => "Ctrl+S",
            Shortcut::Undo => "Ctrl+Z",
            Shortcut::Open => "Ctrl+O",
            Shortcut::Export => "Ctrl+E",
            Shortcut::New => "Ctrl+N",
            Shortcut::PlayStop => "Space",
            Shortcut::ToggleArm => "R",
            Shortcut::BpmUp => "+ / Ctrl+Up",
            Shortcut::BpmDown => "- / Ctrl+Down",
            Shortcut::DuplicateTrack => "Ctrl+Shift+D",
            Shortcut::AddTrack => "Ctrl+T",
            Shortcut::PrevTrack => "Alt+Up",
            Shortcut::NextTrack => "Alt+Down",
            Shortcut::PrevPreset => "Ctrl+PageUp",
            Shortcut::NextPreset => "Ctrl+PageDown",
            Shortcut::FindPreset => "Ctrl+Shift+P",
            Shortcut::LinkInsert => "Ctrl+L",
            Shortcut::ViewRoll => "F5",
            Shortcut::ViewMixer => "F6",
            Shortcut::ViewSynth => "F7",
            Shortcut::ViewSong => "F8",
            Shortcut::ToggleBrowser => "F9",
            Shortcut::VisualFull => "F11",
        }
    }

    /// Whether this shortcut can open a file dialog.
    #[must_use]
    pub fn opens_dialog(self) -> bool {
        matches!(
            self,
            Shortcut::SaveAs | Shortcut::Save | Shortcut::Open | Shortcut::Export
        )
    }
}

/// Takes every shortcut pressed this frame out of `input` (the key presses are consumed), in
/// check order. A shortcut pressed several times in one frame is reported once.
pub fn take_shortcuts(input: &mut InputState) -> Vec<Shortcut> {
    let mut out = Vec::new();
    for shortcut in Shortcut::ALL {
        let mut hit = false;
        for (modifiers, key) in shortcut.bindings() {
            // Consume every press (also repeats) so the key does not leak to other widgets.
            while input.consume_key(*modifiers, *key) {
                hit = true;
            }
        }
        if hit {
            out.push(shortcut);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, RawInput};

    fn press(modifiers: Modifiers, key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn input_with(events: Vec<Event>, modifiers: Modifiers) -> InputState {
        let _ = modifiers;
        let raw = RawInput {
            events,
            ..RawInput::default()
        };
        InputState::default().begin_pass(raw, false, 1.0, egui::InputOptions::default())
    }

    #[test]
    fn plain_keys_map_to_transport_shortcuts() {
        let mut input = input_with(
            vec![
                press(Modifiers::NONE, Key::Space),
                press(Modifiers::NONE, Key::R),
            ],
            Modifiers::NONE,
        );
        assert_eq!(
            take_shortcuts(&mut input),
            vec![Shortcut::PlayStop, Shortcut::ToggleArm]
        );
        // Consumed: a second pass finds nothing.
        assert!(take_shortcuts(&mut input).is_empty());
        assert!(!input.key_pressed(Key::Space));
    }

    #[test]
    fn ctrl_combinations_prefer_the_specific_binding() {
        let shift_ctrl = COMMAND_SHIFT;
        let mut input = input_with(
            vec![press(shift_ctrl, Key::S), press(shift_ctrl, Key::Z)],
            shift_ctrl,
        );
        assert_eq!(
            take_shortcuts(&mut input),
            vec![Shortcut::SaveAs, Shortcut::Redo]
        );

        let mut input = input_with(
            vec![
                press(Modifiers::COMMAND, Key::S),
                press(Modifiers::COMMAND, Key::Z),
                press(Modifiers::COMMAND, Key::Y),
                press(Modifiers::COMMAND, Key::O),
                press(Modifiers::COMMAND, Key::E),
                press(Modifiers::COMMAND, Key::N),
            ],
            Modifiers::COMMAND,
        );
        assert_eq!(
            take_shortcuts(&mut input),
            vec![
                Shortcut::Redo,
                Shortcut::Save,
                Shortcut::Undo,
                Shortcut::Open,
                Shortcut::Export,
                Shortcut::New,
            ]
        );
    }

    #[test]
    fn piano_keys_and_ctrl_space_are_not_shortcuts() {
        let mut input = input_with(
            vec![
                press(Modifiers::NONE, Key::A),
                press(Modifiers::NONE, Key::S),
                press(Modifiers::NONE, Key::Z),
                press(Modifiers::COMMAND, Key::Space),
                press(Modifiers::COMMAND, Key::R),
            ],
            Modifiers::NONE,
        );
        assert!(take_shortcuts(&mut input).is_empty());
        // The piano keys are still there for `crate::input`.
        assert!(input.key_pressed(Key::A));
        assert!(input.key_pressed(Key::S));
    }

    #[test]
    fn tempo_nudges_and_recent_cycling_are_shortcuts() {
        let mut input = input_with(
            vec![
                press(Modifiers::SHIFT, Key::Plus),
                press(Modifiers::NONE, Key::Minus),
                press(Modifiers::COMMAND, Key::ArrowUp),
                press(COMMAND_SHIFT, Key::O),
            ],
            Modifiers::NONE,
        );
        assert_eq!(
            take_shortcuts(&mut input),
            vec![Shortcut::OpenRecent, Shortcut::BpmUp, Shortcut::BpmDown]
        );
        // Plain arrows are not shortcuts (they nudge a selected note or a focused knob).
        let mut input = input_with(
            vec![
                press(Modifiers::NONE, Key::ArrowUp),
                press(Modifiers::NONE, Key::Equals),
            ],
            Modifiers::NONE,
        );
        assert_eq!(take_shortcuts(&mut input), vec![Shortcut::BpmUp]);
        assert!(input.key_pressed(Key::ArrowUp));
        assert!(!Shortcut::OpenRecent.opens_dialog());
        assert!(!Shortcut::BpmUp.opens_dialog());
    }

    #[test]
    fn track_shortcuts_add_duplicate_and_select_tracks() {
        let mut input = input_with(
            vec![
                press(Modifiers::COMMAND, Key::T),
                press(COMMAND_SHIFT, Key::D),
                press(Modifiers::ALT, Key::ArrowUp),
                press(Modifiers::ALT, Key::ArrowDown),
            ],
            Modifiers::NONE,
        );
        assert_eq!(
            take_shortcuts(&mut input),
            vec![
                Shortcut::DuplicateTrack,
                Shortcut::AddTrack,
                Shortcut::PrevTrack,
                Shortcut::NextTrack,
            ]
        );
        assert!(take_shortcuts(&mut input).is_empty(), "consumed");
        // Plain T is a piano key, plain arrows nudge notes, Ctrl+D is nothing.
        let mut input = input_with(
            vec![
                press(Modifiers::NONE, Key::T),
                press(Modifiers::NONE, Key::ArrowDown),
                press(Modifiers::COMMAND, Key::D),
            ],
            Modifiers::NONE,
        );
        assert!(take_shortcuts(&mut input).is_empty());
        assert!(input.key_pressed(Key::T));
        assert!(input.key_pressed(Key::ArrowDown));
        for shortcut in [
            Shortcut::AddTrack,
            Shortcut::DuplicateTrack,
            Shortcut::PrevTrack,
            Shortcut::NextTrack,
        ] {
            assert!(!shortcut.opens_dialog());
        }
    }

    #[test]
    fn preset_shortcuts_step_and_find_without_taking_the_tempo_nudge() {
        let mut input = input_with(
            vec![
                press(Modifiers::COMMAND, Key::PageUp),
                press(Modifiers::COMMAND, Key::PageDown),
                press(COMMAND_SHIFT, Key::P),
                press(Modifiers::COMMAND, Key::ArrowUp),
            ],
            Modifiers::NONE,
        );
        assert_eq!(
            take_shortcuts(&mut input),
            vec![
                Shortcut::BpmUp,
                Shortcut::PrevPreset,
                Shortcut::NextPreset,
                Shortcut::FindPreset,
            ]
        );
        assert!(take_shortcuts(&mut input).is_empty(), "consumed");
        // Plain PageUp / PageDown nudge a selected note by an octave; plain P is a piano key;
        // Ctrl+P alone is nothing.
        let mut input = input_with(
            vec![
                press(Modifiers::NONE, Key::PageUp),
                press(Modifiers::NONE, Key::PageDown),
                press(Modifiers::NONE, Key::P),
                press(Modifiers::COMMAND, Key::P),
            ],
            Modifiers::NONE,
        );
        assert!(take_shortcuts(&mut input).is_empty());
        assert!(input.key_pressed(Key::PageUp));
        assert!(input.key_pressed(Key::PageDown));
        assert!(input.key_pressed(Key::P));
        for shortcut in [
            Shortcut::PrevPreset,
            Shortcut::NextPreset,
            Shortcut::FindPreset,
        ] {
            assert!(!shortcut.opens_dialog());
        }
        assert_eq!(Shortcut::PrevPreset.label(), "Ctrl+PageUp");
        assert_eq!(Shortcut::FindPreset.label(), "Ctrl+Shift+P");
    }

    #[test]
    fn ctrl_l_links_the_edit_track_to_an_insert() {
        let mut input = input_with(
            vec![
                press(Modifiers::COMMAND, Key::L),
                press(Modifiers::NONE, Key::L),
            ],
            Modifiers::NONE,
        );
        assert_eq!(take_shortcuts(&mut input), vec![Shortcut::LinkInsert]);
        assert!(take_shortcuts(&mut input).is_empty(), "consumed");
        // Plain L is a piano key.
        assert!(input.key_pressed(Key::L));
        assert!(!Shortcut::LinkInsert.opens_dialog());
        assert_eq!(Shortcut::LinkInsert.label(), "Ctrl+L");
    }

    #[test]
    fn every_shortcut_has_a_label_and_dialog_flag() {
        for shortcut in Shortcut::ALL {
            assert!(!shortcut.label().is_empty());
            assert!(!shortcut.bindings().is_empty());
        }
        assert!(Shortcut::Save.opens_dialog());
        assert!(Shortcut::Open.opens_dialog());
        assert!(!Shortcut::Undo.opens_dialog());
        assert!(!Shortcut::New.opens_dialog());
        assert!(!Shortcut::PlayStop.opens_dialog());
    }
}
