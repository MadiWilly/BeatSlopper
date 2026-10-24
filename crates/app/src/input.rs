//! Computer-keyboard piano input (M1).
//!
//! `A W S E D F T G Y H U J K O L P ;` map to semitone offsets 0..=16 above the current
//! octave's C (about 1.5 octaves); `Z` / `X` shift the octave down / up within 1..=7.
//!
//! Note numbering: `note = (octave + 1) * 12 + offset`, so the default octave 4 puts `A` on
//! MIDI 60 = C4 (the mock labels the leftmost key "C4"; A4 = 69 = 440 Hz in the engine).
//!
//! The OS repeats key-down events while a key is held, so this module is driven by a set of
//! *held* keys: a key only triggers a note the first time it goes down (`repeat` events and a
//! second press without a release are ignored) and releases the note it was assigned at press
//! time, so an octave change while a key is held still releases the right note. Losing window
//! focus releases everything (the key-ups would otherwise never arrive).

use egui::{Event, Key};

/// Default octave (`A` = C4 = MIDI 60).
pub const DEFAULT_OCTAVE: i32 = 4;
/// Lowest selectable octave.
pub const MIN_OCTAVE: i32 = 1;
/// Highest selectable octave.
pub const MAX_OCTAVE: i32 = 7;
/// Default note-on velocity.
pub const DEFAULT_VELOCITY: f32 = 0.8;
/// Lowest selectable velocity.
pub const MIN_VELOCITY: f32 = 0.05;
/// Highest selectable velocity.
pub const MAX_VELOCITY: f32 = 1.0;
/// Number of mapped piano keys (semitone offsets 0..=16).
pub const KEY_COUNT: usize = 17;

/// A note event produced by the keyboard (or the on-screen piano).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteEvent {
    /// A note started (MIDI note, velocity 0..=1).
    On { note: u8, velocity: f32 },
    /// A note stopped.
    Off { note: u8 },
}

impl NoteEvent {
    /// The MIDI note this event is about.
    pub fn note(self) -> u8 {
        match self {
            NoteEvent::On { note, .. } | NoteEvent::Off { note } => note,
        }
    }
}

/// The computer keys in semitone order (offset 0 = C ... offset 16 = E an octave up) and
/// their display caps.
const KEYS: [(Key, char); KEY_COUNT] = [
    (Key::A, 'A'),
    (Key::W, 'W'),
    (Key::S, 'S'),
    (Key::E, 'E'),
    (Key::D, 'D'),
    (Key::F, 'F'),
    (Key::T, 'T'),
    (Key::G, 'G'),
    (Key::Y, 'Y'),
    (Key::H, 'H'),
    (Key::U, 'U'),
    (Key::J, 'J'),
    (Key::K, 'K'),
    (Key::O, 'O'),
    (Key::L, 'L'),
    (Key::P, 'P'),
    (Key::Semicolon, ';'),
];

/// `egui::Key` -> semitone offset for the piano keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyMap;

impl KeyMap {
    /// Semitone offset (0..=16) of a mapped key, or `None` for anything else.
    pub fn offset(key: Key) -> Option<u8> {
        KEYS.iter()
            .position(|(k, _)| *k == key)
            .map(|offset| offset as u8)
    }

    /// The key whose cap sits on the given semitone offset.
    pub fn key(offset: u8) -> Option<Key> {
        KEYS.get(offset as usize).map(|(k, _)| *k)
    }

    /// Uppercase cap of the key on the given semitone offset (`';'` stays `';'`).
    pub fn cap(offset: u8) -> Option<char> {
        KEYS.get(offset as usize).map(|(_, c)| *c)
    }

    /// Computer-key cap per MIDI note for the given octave (input to `neu::PianoModel`).
    pub fn labels(octave: i32) -> [Option<char>; 128] {
        let mut labels = [None; 128];
        for offset in 0..KEY_COUNT as u8 {
            let note = note_for(octave, offset) as usize;
            labels[note] = Self::cap(offset);
        }
        labels
    }
}

/// MIDI note of the leftmost C for `octave` (clamped to a valid note).
pub fn base_note(octave: i32) -> u8 {
    ((octave.clamp(MIN_OCTAVE, MAX_OCTAVE) + 1) * 12).clamp(0, 127) as u8
}

/// MIDI note for a semitone offset in `octave`, clamped to 0..=127.
pub fn note_for(octave: i32, offset: u8) -> u8 {
    ((octave + 1) * 12 + i32::from(offset)).clamp(0, 127) as u8
}

/// Which mapped keys are physically down, with the note each was assigned when pressed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HeldKeys {
    notes: [Option<u8>; KEY_COUNT],
}

impl HeldKeys {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The note assigned to the key at `offset`, if that key is down.
    pub fn note(&self, offset: u8) -> Option<u8> {
        self.notes.get(offset as usize).copied().flatten()
    }

    /// Whether the key at `offset` is down.
    pub fn is_held(&self, offset: u8) -> bool {
        self.note(offset).is_some()
    }

    /// Marks the key down with `note`. Returns `false` (and changes nothing) if already down.
    pub fn press(&mut self, offset: u8, note: u8) -> bool {
        match self.notes.get_mut(offset as usize) {
            Some(slot @ None) => {
                *slot = Some(note);
                true
            }
            _ => false,
        }
    }

    /// Marks the key up and returns the note it was holding, if it was down.
    pub fn release(&mut self, offset: u8) -> Option<u8> {
        self.notes.get_mut(offset as usize).and_then(Option::take)
    }

    /// Releases every key, returning the notes they were holding (in semitone order).
    pub fn release_all(&mut self) -> Vec<u8> {
        self.notes.iter_mut().filter_map(Option::take).collect()
    }

    /// Number of keys currently down.
    pub fn len(&self) -> usize {
        self.notes.iter().flatten().count()
    }

    /// Whether no key is down.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Keyboard-piano state: held keys, octave and the fixed velocity.
#[derive(Debug, Clone, PartialEq)]
pub struct InputState {
    /// Mapped keys currently down and the notes they hold.
    pub held: HeldKeys,
    /// Current octave (1..=7; `A` = C of this octave, MIDI `(octave + 1) * 12`).
    pub octave: i32,
    /// Velocity used for every note-on (0.05..=1.0).
    pub velocity: f32,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            held: HeldKeys::new(),
            octave: DEFAULT_OCTAVE,
            velocity: DEFAULT_VELOCITY,
        }
    }
}

impl InputState {
    /// Default state: octave 4, velocity 0.8, nothing held.
    pub fn new() -> Self {
        Self::default()
    }

    /// MIDI note of the leftmost on-screen C for the current octave.
    pub fn base_note(&self) -> u8 {
        base_note(self.octave)
    }

    /// MIDI note for a semitone offset in the current octave.
    pub fn note_for(&self, offset: u8) -> u8 {
        note_for(self.octave, offset)
    }

    /// Shifts the octave by `delta`, clamped to 1..=7. Held keys keep their notes.
    pub fn shift_octave(&mut self, delta: i32) {
        self.set_octave(self.octave + delta);
    }

    /// Sets the octave, clamped to 1..=7. Held keys keep their notes.
    pub fn set_octave(&mut self, octave: i32) {
        self.octave = octave.clamp(MIN_OCTAVE, MAX_OCTAVE);
    }

    /// Sets the velocity, clamped to 0.05..=1.0.
    pub fn set_velocity(&mut self, velocity: f32) {
        self.velocity = velocity.clamp(MIN_VELOCITY, MAX_VELOCITY);
    }

    /// Turns this frame's egui events into note events.
    ///
    /// * `Event::Key` with `repeat` is ignored (no retrigger while held).
    /// * Keys with ctrl / alt / command held are ignored (shortcuts, not notes).
    /// * A press of a mapped key that is not already down emits `On`; a release of a held key
    ///   emits `Off` for the note assigned at press time. Unmapped keys are ignored.
    /// * `Z` / `X` presses shift the octave down / up.
    /// * `Event::WindowFocused(false)` releases everything.
    pub fn handle_events(&mut self, events: &[Event]) -> Vec<NoteEvent> {
        let mut out = Vec::new();
        for event in events {
            match event {
                Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } => {
                    if *repeat || modifiers.ctrl || modifiers.alt || modifiers.command {
                        continue;
                    }
                    self.handle_key(*key, *pressed, &mut out);
                }
                Event::WindowFocused(false) => out.extend(self.release_all()),
                _ => {}
            }
        }
        out
    }

    fn handle_key(&mut self, key: Key, pressed: bool, out: &mut Vec<NoteEvent>) {
        match key {
            Key::Z if pressed => self.shift_octave(-1),
            Key::X if pressed => self.shift_octave(1),
            _ => {
                let Some(offset) = KeyMap::offset(key) else {
                    return;
                };
                if pressed {
                    let note = self.note_for(offset);
                    if self.held.press(offset, note) {
                        out.push(NoteEvent::On {
                            note,
                            velocity: self.velocity,
                        });
                    }
                } else if let Some(note) = self.held.release(offset) {
                    out.push(NoteEvent::Off { note });
                }
            }
        }
    }

    /// Releases every held key and returns the matching `Off` events.
    pub fn release_all(&mut self) -> Vec<NoteEvent> {
        self.held
            .release_all()
            .into_iter()
            .map(|note| NoteEvent::Off { note })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;

    fn key(key: Key, pressed: bool, repeat: bool) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers: Modifiers::default(),
        }
    }

    fn down(k: Key) -> Event {
        key(k, true, false)
    }

    fn up(k: Key) -> Event {
        key(k, false, false)
    }

    fn repeat(k: Key) -> Event {
        key(k, true, true)
    }

    #[test]
    fn key_map_covers_seventeen_semitones() {
        assert_eq!(KeyMap::offset(Key::A), Some(0));
        assert_eq!(KeyMap::offset(Key::W), Some(1));
        assert_eq!(KeyMap::offset(Key::K), Some(12));
        assert_eq!(KeyMap::offset(Key::Semicolon), Some(16));
        assert_eq!(KeyMap::offset(Key::Q), None);
        assert_eq!(KeyMap::cap(16), Some(';'));
        assert_eq!(KeyMap::cap(17), None);
        for offset in 0..KEY_COUNT as u8 {
            let k = KeyMap::key(offset).unwrap();
            assert_eq!(KeyMap::offset(k), Some(offset));
        }
    }

    #[test]
    fn default_octave_puts_a_on_middle_c() {
        let state = InputState::new();
        assert_eq!(state.octave, 4);
        assert_eq!(state.base_note(), 60);
        assert_eq!(state.note_for(0), 60);
        assert_eq!(state.note_for(9), 69); // H = A4 = 440 Hz
        assert_eq!(note_for(7, 16), 112);
        assert_eq!(note_for(1, 0), 24);
    }

    #[test]
    fn labels_follow_the_octave() {
        let labels = KeyMap::labels(4);
        assert_eq!(labels[60], Some('A'));
        assert_eq!(labels[61], Some('W'));
        assert_eq!(labels[76], Some(';'));
        assert_eq!(labels[77], None);
        assert_eq!(labels[59], None);
        assert_eq!(labels.iter().flatten().count(), KEY_COUNT);
        assert_eq!(KeyMap::labels(3)[48], Some('A'));
    }

    #[test]
    fn press_emits_one_on_and_release_emits_off() {
        let mut state = InputState::new();
        let on = state.handle_events(&[down(Key::A)]);
        assert_eq!(
            on,
            vec![NoteEvent::On {
                note: 60,
                velocity: DEFAULT_VELOCITY
            }]
        );
        assert!(state.held.is_held(0));
        let off = state.handle_events(&[up(Key::A)]);
        assert_eq!(off, vec![NoteEvent::Off { note: 60 }]);
        assert!(state.held.is_empty());
    }

    #[test]
    fn repeat_does_not_retrigger() {
        let mut state = InputState::new();
        let events = state.handle_events(&[down(Key::A), repeat(Key::A), repeat(Key::A)]);
        let ons = events
            .iter()
            .filter(|e| matches!(e, NoteEvent::On { .. }))
            .count();
        assert_eq!(ons, 1);
        assert_eq!(events.len(), 1);
        // A second non-repeat press without a release is also not a retrigger.
        assert!(state.handle_events(&[down(Key::A)]).is_empty());
        assert_eq!(state.held.len(), 1);
    }

    #[test]
    fn release_after_octave_change_releases_the_original_note() {
        let mut state = InputState::new();
        state.handle_events(&[down(Key::A)]);
        let shifted = state.handle_events(&[down(Key::X), up(Key::X)]);
        assert!(shifted.is_empty());
        assert_eq!(state.octave, 5);
        assert_eq!(state.base_note(), 72);
        let off = state.handle_events(&[up(Key::A)]);
        assert_eq!(off, vec![NoteEvent::Off { note: 60 }]);
        // A fresh press now plays in the new octave.
        let on = state.handle_events(&[down(Key::A)]);
        assert_eq!(on[0].note(), 72);
    }

    #[test]
    fn octave_is_clamped_and_repeat_does_not_shift() {
        let mut state = InputState::new();
        for _ in 0..10 {
            state.handle_events(&[down(Key::Z), up(Key::Z)]);
        }
        assert_eq!(state.octave, MIN_OCTAVE);
        for _ in 0..10 {
            state.handle_events(&[down(Key::X), up(Key::X)]);
        }
        assert_eq!(state.octave, MAX_OCTAVE);
        state.set_octave(4);
        state.handle_events(&[repeat(Key::X), repeat(Key::X)]);
        assert_eq!(state.octave, 4);
    }

    #[test]
    fn unknown_keys_and_modified_keys_are_ignored() {
        let mut state = InputState::new();
        assert!(
            state
                .handle_events(&[down(Key::Q), up(Key::Q), down(Key::Space), down(Key::Enter)])
                .is_empty()
        );
        assert!(state.handle_events(&[up(Key::A)]).is_empty()); // key-up without a key-down
        let ctrl_a = Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        };
        assert!(state.handle_events(&[ctrl_a]).is_empty());
        assert!(state.held.is_empty());
        assert!(
            state
                .handle_events(&[Event::Text("a".into()), Event::PointerGone])
                .is_empty()
        );
    }

    #[test]
    fn focus_loss_releases_everything() {
        let mut state = InputState::new();
        state.handle_events(&[down(Key::A), down(Key::D), down(Key::G)]);
        assert_eq!(state.held.len(), 3);
        let mut offs = state.handle_events(&[Event::WindowFocused(false)]);
        offs.sort_by_key(|e| e.note());
        assert_eq!(
            offs,
            vec![
                NoteEvent::Off { note: 60 },
                NoteEvent::Off { note: 64 },
                NoteEvent::Off { note: 67 },
            ]
        );
        assert!(state.held.is_empty());
        // The later key-ups are then ignored.
        assert!(state.handle_events(&[up(Key::A)]).is_empty());
        // Gaining focus does nothing.
        assert!(
            state
                .handle_events(&[Event::WindowFocused(true)])
                .is_empty()
        );
    }

    #[test]
    fn release_all_is_idempotent() {
        let mut state = InputState::new();
        state.handle_events(&[down(Key::A)]);
        assert_eq!(state.release_all(), vec![NoteEvent::Off { note: 60 }]);
        assert!(state.release_all().is_empty());
    }

    #[test]
    fn velocity_is_used_and_clamped() {
        let mut state = InputState::new();
        state.set_velocity(2.0);
        assert_eq!(state.velocity, MAX_VELOCITY);
        state.set_velocity(0.0);
        assert_eq!(state.velocity, MIN_VELOCITY);
        state.set_velocity(0.5);
        let on = state.handle_events(&[down(Key::A)]);
        assert_eq!(
            on,
            vec![NoteEvent::On {
                note: 60,
                velocity: 0.5
            }]
        );
    }
}
