//! egui_kittest tests for the M1 widgets: the piano keyboard reports mouse presses and releases
//! with the right MIDI notes (black keys win where they overlap white keys) and renders in both
//! themes; the stepper steps and clamps.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::{PianoModel, PianoOutput, Theme, ThemeMode};

const BASE: u8 = 60; // C4
const WHITE_KEYS: usize = 14;

/// Collected piano events across frames.
struct PianoLog {
    pressed: Vec<u8>,
    released: Vec<u8>,
    held: [bool; 128],
}

impl Default for PianoLog {
    fn default() -> Self {
        Self {
            pressed: Vec::new(),
            released: Vec::new(),
            held: [false; 128],
        }
    }
}

impl PianoLog {
    fn record(&mut self, out: PianoOutput) {
        if let Some(n) = out.released {
            self.released.push(n);
            self.held[usize::from(n)] = false;
        }
        if let Some(n) = out.pressed {
            self.pressed.push(n);
            self.held[usize::from(n)] = true;
        }
    }
}

fn labels() -> [Option<char>; 128] {
    let mut labels = [None; 128];
    for (i, c) in "AWSEDFTGYHUJKOLP;".chars().enumerate() {
        labels[usize::from(BASE) + i] = Some(c);
    }
    labels
}

fn piano_harness(mode: ThemeMode) -> Harness<'static, PianoLog> {
    let labels = labels();
    let mut harness = Harness::builder()
        .with_size(vec2(1240.0, 200.0))
        .build_ui_state(
            move |ui, log: &mut PianoLog| {
                let theme = Theme::for_mode(mode);
                theme.apply(ui.ctx());
                let model = PianoModel {
                    base_note: BASE,
                    white_keys: WHITE_KEYS,
                    held: &log.held,
                    labels: &labels,
                };
                let out = neu::piano_keys(ui, &theme, &model);
                log.record(out);
            },
            PianoLog::default(),
        );
    harness.run();
    harness
}

fn keyboard_origin(harness: &Harness<'_, PianoLog>) -> Pos2 {
    harness.get_by_label("Piano keys").rect().min
}

fn press_at(harness: &mut Harness<'_, PianoLog>, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    harness.run();
}

fn move_to(harness: &mut Harness<'_, PianoLog>, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.run();
}

fn release_at(harness: &mut Harness<'_, PianoLog>, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    harness.run();
}

/// Centre of the lower (unobstructed) part of white key `index`.
fn white_key_pos(origin: Pos2, index: usize) -> Pos2 {
    pos2(
        origin.x
            + index as f32 * neu::piano_keys::WHITE_KEY_PITCH
            + neu::piano_keys::WHITE_KEY_WIDTH * 0.5,
        origin.y + 85.0,
    )
}

#[test]
fn renders_in_both_themes_with_and_without_held_notes() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let mut harness = piano_harness(mode);
        // Hover a white and a black key to exercise the hover colours.
        let origin = keyboard_origin(&harness);
        move_to(&mut harness, white_key_pos(origin, 3));
        move_to(&mut harness, pos2(origin.x + 81.0, origin.y + 20.0));
        assert!(harness.state().pressed.is_empty());

        // Hold a white and a black key through the model.
        harness.state_mut().held[64] = true;
        harness.state_mut().held[68] = true;
        harness.run();
        harness.state_mut().held = [false; 128];
        harness.run();
    }
}

#[test]
fn white_key_press_and_release_report_the_note() {
    let mut harness = piano_harness(ThemeMode::Dark);
    let origin = keyboard_origin(&harness);
    assert_eq!(
        harness.get_by_label("Piano keys").rect().size(),
        neu::keyboard_size(WHITE_KEYS)
    );

    // White key 0 is C4, key 3 is F4 (C D E F), key 7 is C5.
    let f4 = white_key_pos(origin, 3);
    press_at(&mut harness, f4);
    assert_eq!(harness.state().pressed, vec![65]);
    assert!(harness.state().released.is_empty());
    assert!(harness.state().held[65]);

    // Releasing away from the key still releases F4.
    release_at(&mut harness, pos2(origin.x - 50.0, origin.y - 50.0));
    assert_eq!(harness.state().released, vec![65]);
    assert!(!harness.state().held[65]);

    press_at(&mut harness, white_key_pos(origin, 7));
    release_at(&mut harness, white_key_pos(origin, 7));
    assert_eq!(harness.state().pressed, vec![65, 72]);
    assert_eq!(harness.state().released, vec![65, 72]);
}

#[test]
fn black_key_wins_where_it_overlaps_a_white_key() {
    let mut harness = piano_harness(ThemeMode::Light);
    let origin = keyboard_origin(&harness);

    // C#4 is centred on the gap between white 0 and 1 (x = 84 - 3 = 81), 44 wide, 62 tall.
    // x = 70 is inside white key 0 (0..78) and inside the black key (59..103).
    let on_black = pos2(origin.x + 70.0, origin.y + 30.0);
    press_at(&mut harness, on_black);
    release_at(&mut harness, on_black);
    assert_eq!(harness.state().pressed, vec![61]);
    assert_eq!(harness.state().released, vec![61]);

    // The same x below the black key is the white key C4.
    let below_black = pos2(origin.x + 70.0, origin.y + 85.0);
    press_at(&mut harness, below_black);
    release_at(&mut harness, below_black);
    assert_eq!(harness.state().pressed, vec![61, 60]);

    // No black key between E and F: the gap after white key 2 is empty.
    let e_f_gap = pos2(origin.x + 3.0 * 84.0 - 3.0, origin.y + 30.0);
    press_at(&mut harness, e_f_gap);
    release_at(&mut harness, e_f_gap);
    assert_eq!(harness.state().pressed, vec![61, 60]);
}

#[test]
fn dragging_across_keys_is_a_glissando() {
    let mut harness = piano_harness(ThemeMode::Dark);
    let origin = keyboard_origin(&harness);

    press_at(&mut harness, white_key_pos(origin, 0));
    move_to(&mut harness, white_key_pos(origin, 1));
    move_to(&mut harness, white_key_pos(origin, 2));
    release_at(&mut harness, white_key_pos(origin, 2));
    assert_eq!(harness.state().pressed, vec![60, 62, 64]);
    assert_eq!(harness.state().released, vec![60, 62, 64]);
    assert!(harness.state().held.iter().all(|h| !h));
}

#[test]
fn stepper_steps_and_clamps() {
    #[derive(Default)]
    struct S {
        value: i32,
        changes: u32,
    }
    let mut harness = Harness::new_ui_state(
        |ui, s: &mut S| {
            let theme = Theme::dark();
            if neu::stepper(ui, &theme, "Octave", &mut s.value, 1..=3).changed() {
                s.changes += 1;
            }
        },
        S {
            value: 2,
            changes: 0,
        },
    );
    harness.run();
    assert_eq!(harness.state().changes, 0);
    assert_eq!(
        harness.get_by_label("Octave value").value().as_deref(),
        Some("2")
    );

    harness.get_by_label("Increase").click();
    harness.run();
    assert_eq!(harness.state().value, 3);
    assert_eq!(harness.state().changes, 1);
    assert_eq!(
        harness.get_by_label("Octave value").value().as_deref(),
        Some("3")
    );

    // Clamped at the top: no change reported.
    harness.get_by_label("Increase").click();
    harness.run();
    assert_eq!(harness.state().value, 3);
    assert_eq!(harness.state().changes, 1);

    for _ in 0..2 {
        harness.get_by_label("Decrease").click();
        harness.run();
    }
    assert_eq!(harness.state().value, 1);
    assert_eq!(harness.state().changes, 3);

    harness.get_by_label("Decrease").click();
    harness.run();
    assert_eq!(harness.state().value, 1);
    assert_eq!(harness.state().changes, 3);
}

#[test]
fn stepper_with_formats_the_value() {
    let mut harness = Harness::new_ui_state(
        |ui, v: &mut i32| {
            let theme = Theme::light();
            neu::stepper_with(ui, &theme, "Swing", v, 50..=75, |v| format!("{v}%"));
        },
        54,
    );
    harness.run();
    assert_eq!(
        harness.get_by_label("Swing value").value().as_deref(),
        Some("54%")
    );
    harness.get_by_label("Swing");
    harness.get_by_label("Decrease").click();
    harness.run();
    assert_eq!(*harness.state(), 53);
    assert_eq!(
        harness.get_by_label("Swing value").value().as_deref(),
        Some("53%")
    );
}
