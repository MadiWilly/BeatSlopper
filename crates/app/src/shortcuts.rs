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
    Redo,
    Save,
    Undo,
    Open,
    Export,
    New,
    PlayStop,
    ToggleArm,
}

impl Shortcut {
    /// Every shortcut, in check order.
    pub const ALL: [Shortcut; 9] = [
        Shortcut::SaveAs,
        Shortcut::Redo,
        Shortcut::Save,
        Shortcut::Undo,
        Shortcut::Open,
        Shortcut::Export,
        Shortcut::New,
        Shortcut::PlayStop,
        Shortcut::ToggleArm,
    ];

    /// The key combinations that trigger this shortcut.
    #[must_use]
    pub fn bindings(self) -> &'static [(Modifiers, Key)] {
        match self {
            Shortcut::SaveAs => &[(COMMAND_SHIFT, Key::S)],
            Shortcut::Redo => &[(COMMAND_SHIFT, Key::Z), (Modifiers::COMMAND, Key::Y)],
            Shortcut::Save => &[(Modifiers::COMMAND, Key::S)],
            Shortcut::Undo => &[(Modifiers::COMMAND, Key::Z)],
            Shortcut::Open => &[(Modifiers::COMMAND, Key::O)],
            Shortcut::Export => &[(Modifiers::COMMAND, Key::E)],
            Shortcut::New => &[(Modifiers::COMMAND, Key::N)],
            Shortcut::PlayStop => &[(Modifiers::NONE, Key::Space)],
            Shortcut::ToggleArm => &[(Modifiers::NONE, Key::R)],
        }
    }

    /// Human-readable binding for tooltips, e.g. `"Ctrl+Shift+S"`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Shortcut::SaveAs => "Ctrl+Shift+S",
            Shortcut::Redo => "Ctrl+Y",
            Shortcut::Save => "Ctrl+S",
            Shortcut::Undo => "Ctrl+Z",
            Shortcut::Open => "Ctrl+O",
            Shortcut::Export => "Ctrl+E",
            Shortcut::New => "Ctrl+N",
            Shortcut::PlayStop => "Space",
            Shortcut::ToggleArm => "R",
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
