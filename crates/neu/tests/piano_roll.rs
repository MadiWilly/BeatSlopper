//! egui_kittest tests for the M7 piano roll: it renders in both themes with and without notes,
//! every gesture reports the documented edit (draw, select, move, resize, double-click delete,
//! Delete key, shift-click accent), the wheel scrolls `low_pitch` with clamping, and the
//! public geometry helpers round-trip.

use egui::{
    Event, Key, Modifiers, MouseWheelSource, MouseWheelUnit, PointerButton, Pos2, Rect, TouchPhase,
    pos2, vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::{PianoRollModel, PianoRollOutput, RollNote, Theme, ThemeMode};

/// The host side of the roll: the notes, the view model and a log of every output that
/// carried anything.
struct Roll {
    model: PianoRollModel,
    notes: Vec<RollNote>,
    log: Vec<PianoRollOutput>,
}

impl Roll {
    /// Applies an output like the app would: push / replace / remove, fix the selection.
    fn apply(&mut self, out: PianoRollOutput) {
        if out.add.is_some()
            || out.moved.is_some()
            || out.removed.is_some()
            || out.selected.is_some()
            || out.scrolled
        {
            self.log.push(out.clone());
        }
        if let Some(n) = out.add {
            self.notes.push(n);
        } else if let Some((i, n)) = out.moved {
            self.notes[i] = n;
        } else if let Some(i) = out.removed {
            self.notes.remove(i);
            self.model.selected = match self.model.selected {
                Some(s) if s == i => None,
                Some(s) if s > i => Some(s - 1),
                other => other,
            };
        }
    }

    fn last(&self) -> PianoRollOutput {
        self.log.last().cloned().unwrap_or_default()
    }
}

fn note(start: u8, length: u8, pitch: u8) -> RollNote {
    RollNote {
        start,
        length,
        pitch,
        velocity: 0.8,
        selected: false,
    }
}

/// Two notes: a 2-step C3 at step 0 and a 1-step E3 at step 4.
fn two_notes() -> Vec<RollNote> {
    vec![note(0, 2, 48), note(4, 1, 52)]
}

fn harness(mode: ThemeMode, notes: Vec<RollNote>) -> Harness<'static, Roll> {
    let mut harness = Harness::builder()
        .with_size(vec2(560.0, 600.0))
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120)
        .build_ui_state(
            move |ui, roll: &mut Roll| {
                let theme = Theme::for_mode(mode);
                theme.apply(ui.ctx());
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    let out = neu::piano_roll(ui, &theme, &mut roll.model, &roll.notes);
                    roll.apply(out);
                });
            },
            Roll {
                model: PianoRollModel::default(),
                notes,
                log: Vec::new(),
            },
        );
    harness.run();
    harness
}

fn outer(harness: &Harness<'_, Roll>) -> Rect {
    harness.get_by_label("Piano roll").rect()
}

/// Centre of the cell `(step, pitch)`.
fn cell_center(harness: &Harness<'_, Roll>, step: u8, pitch: u8) -> Pos2 {
    let grid = neu::grid_rect(outer(harness));
    neu::note_rect(&harness.state().model, grid, &note(step, 1, pitch)).center()
}

fn note_rect(harness: &Harness<'_, Roll>, index: usize) -> Rect {
    let grid = neu::grid_rect(outer(harness));
    let n = harness.state().notes[index];
    neu::note_rect(&harness.state().model, grid, &n)
}

fn press(harness: &mut Harness<'_, Roll>, pos: Pos2, modifiers: Modifiers) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    harness.run();
}

fn move_to(harness: &mut Harness<'_, Roll>, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.run();
}

fn release(harness: &mut Harness<'_, Roll>, pos: Pos2, modifiers: Modifiers) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.run();
}

fn click(harness: &mut Harness<'_, Roll>, pos: Pos2) {
    press(harness, pos, Modifiers::NONE);
    release(harness, pos, Modifiers::NONE);
}

fn wheel(harness: &mut Harness<'_, Roll>, delta_y: f32, modifiers: Modifiers) {
    let ev = |phase, delta| Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta,
        phase,
        source: MouseWheelSource::Trackpad,
        modifiers,
    };
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(ev(TouchPhase::Start, vec2(0.0, 0.0)));
    harness.event(ev(TouchPhase::Move, vec2(0.0, delta_y)));
    harness.event(ev(TouchPhase::End, vec2(0.0, 0.0)));
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.run();
}

#[test]
fn renders_in_both_themes_with_0_and_10_notes() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let mut empty = harness(mode, Vec::new());
        assert_eq!(outer(&empty).size(), PianoRollModel::default().size());
        // Hover a cell for the highlight and a key-column spot.
        let c = cell_center(&empty, 3, 40);
        move_to(&mut empty, c);
        let key_column = outer(&empty).min + vec2(10.0, 10.0);
        move_to(&mut empty, key_column);
        assert!(empty.state().log.is_empty());

        let notes = vec![
            note(0, 4, 48),
            note(0, 4, 51),
            note(0, 4, 55),
            note(4, 1, 43),
            RollNote {
                velocity: 1.0,
                ..note(6, 2, 46)
            },
            RollNote {
                selected: true,
                ..note(8, 8, 60)
            },
            note(12, 1, 58),
            note(14, 4, 55), // runs past the grid end
            note(2, 1, 100), // above the visible rows
            note(15, 1, 36), // bottom-right cell
        ];
        assert_eq!(notes.len(), 10);
        let mut full = harness(mode, notes);
        full.state_mut().model.selected = Some(0);
        full.state_mut().model.playhead_step = Some(5.5);
        full.run();
        // Hover a note's right edge (resize cursor) and its body.
        let r = note_rect(&full, 0);
        move_to(&mut full, r.right_center() - vec2(2.0, 0.0));
        move_to(&mut full, r.center());
        assert!(full.state().log.is_empty());
    }
}

#[test]
fn draw_by_drag_reports_add() {
    let mut h = harness(ThemeMode::Dark, Vec::new());
    let from = cell_center(&h, 2, 48);
    let to = cell_center(&h, 5, 48);
    press(&mut h, from, Modifiers::NONE);
    assert!(h.state().log.is_empty(), "nothing before the release");
    let mid = cell_center(&h, 3, 48);
    move_to(&mut h, mid);
    move_to(&mut h, to);
    release(&mut h, to, Modifiers::NONE);
    let out = h.state().last();
    assert_eq!(out.add, Some(note(2, 4, 48)));
    assert_eq!(h.state().notes, vec![note(2, 4, 48)]);
    assert!(out.moved.is_none() && out.removed.is_none());

    // Dragging left of the press keeps length 1; the pitch is the press row.
    let from = cell_center(&h, 10, 40);
    press(&mut h, from, Modifiers::NONE);
    let left_up = cell_center(&h, 6, 44);
    release(&mut h, left_up, Modifiers::NONE);
    assert_eq!(h.state().last().add, Some(note(10, 1, 40)));

    // Dragging off the right edge clips at the grid end.
    let from = cell_center(&h, 12, 60);
    press(&mut h, from, Modifiers::NONE);
    let off_right = outer(&h).right_center() + vec2(100.0, 0.0);
    release(&mut h, off_right, Modifiers::NONE);
    assert_eq!(h.state().last().add, Some(note(12, 4, 60)));

    // A plain click on an empty cell adds a one-step note.
    let corner = cell_center(&h, 0, 36);
    click(&mut h, corner);
    assert_eq!(h.state().last().add, Some(note(0, 1, 36)));
    assert_eq!(h.state().notes.len(), 4);
}

#[test]
fn click_selects_a_note() {
    let mut h = harness(ThemeMode::Light, two_notes());
    let second = note_rect(&h, 1).center();
    click(&mut h, second);
    let out = h.state().last();
    assert_eq!(out.selected, Some(1));
    assert_eq!(h.state().model.selected, Some(1));
    assert!(out.add.is_none() && out.moved.is_none() && out.removed.is_none());

    let first = note_rect(&h, 0).center();
    click(&mut h, first);
    assert_eq!(h.state().model.selected, Some(0));
    assert_eq!(h.state().notes, two_notes(), "a click edits nothing");
}

#[test]
fn drag_moves_a_note() {
    let mut h = harness(ThemeMode::Dark, two_notes());
    let body = note_rect(&h, 0).center();
    press(&mut h, body, Modifiers::NONE);
    // Three steps right, two rows up.
    let target = cell_center(&h, 4, 50);
    move_to(&mut h, target);
    release(&mut h, target, Modifiers::NONE);
    let out = h.state().last();
    assert_eq!(out.moved, Some((0, note(3, 2, 50))));
    assert!(out.selected.is_none() && out.add.is_none());
    assert_eq!(h.state().notes[0], note(3, 2, 50));

    // Moving past the right edge clamps so the note stays in the grid.
    let body = note_rect(&h, 0).center();
    press(&mut h, body, Modifiers::NONE);
    let far = outer(&h).right_center() + vec2(200.0, 0.0);
    move_to(&mut h, far);
    release(&mut h, far, Modifiers::NONE);
    let moved = h.state().last().moved.unwrap();
    assert_eq!(moved.0, 0);
    assert_eq!(moved.1.start, 14);
    assert_eq!(moved.1.length, 2);
}

#[test]
fn drag_on_the_right_edge_resizes() {
    let mut h = harness(ThemeMode::Light, two_notes());
    let r = note_rect(&h, 0);
    let edge = r.right_center() - vec2(2.0, 0.0);
    press(&mut h, edge, Modifiers::NONE);
    let to = cell_center(&h, 6, 48);
    move_to(&mut h, to);
    release(&mut h, to, Modifiers::NONE);
    let out = h.state().last();
    assert_eq!(out.moved, Some((0, note(0, 7, 48))));
    assert_eq!(h.state().notes[0], note(0, 7, 48));

    // Shrinking below one step clamps to 1 (and the pitch never changes while resizing).
    let r = note_rect(&h, 0);
    press(&mut h, r.right_center() - vec2(2.0, 0.0), Modifiers::NONE);
    let to = cell_center(&h, 0, 60);
    move_to(&mut h, to);
    release(&mut h, to, Modifiers::NONE);
    assert_eq!(h.state().notes[0], note(0, 1, 48));
}

#[test]
fn double_click_removes_a_note() {
    let mut h = harness(ThemeMode::Dark, two_notes());
    let body = note_rect(&h, 1).center();
    click(&mut h, body);
    assert_eq!(h.state().model.selected, Some(1));
    click(&mut h, body);
    let out = h.state().last();
    assert_eq!(out.removed, Some(1));
    assert!(out.add.is_none() && out.moved.is_none());
    assert_eq!(h.state().notes, vec![note(0, 2, 48)]);
    assert_eq!(h.state().model.selected, None);
}

#[test]
fn delete_key_removes_the_selected_note() {
    let mut h = harness(ThemeMode::Light, two_notes());
    // Nothing selected: Delete does nothing.
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(h.state().notes.len(), 2);

    let first = note_rect(&h, 0).center();
    click(&mut h, first);
    assert_eq!(h.state().model.selected, Some(0));
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(h.state().last().removed, Some(0));
    assert_eq!(h.state().notes, vec![note(4, 1, 52)]);
    assert_eq!(h.state().model.selected, None);

    // Backspace works too.
    let first = note_rect(&h, 0).center();
    click(&mut h, first);
    h.key_press(Key::Backspace);
    h.run();
    assert_eq!(h.state().last().removed, Some(0));
    assert!(h.state().notes.is_empty());
}

#[test]
fn delete_key_only_acts_while_the_roll_has_focus() {
    let mut h = harness(ThemeMode::Dark, two_notes());
    assert!(!h.get_by_label("Piano roll").is_focused());

    // A click takes focus (the focus ring frame renders) and Delete removes the selection.
    let first = note_rect(&h, 0).center();
    click(&mut h, first);
    assert!(h.get_by_label("Piano roll").is_focused());
    assert_eq!(h.state().model.selected, Some(0));
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(h.state().last().removed, Some(0));
    assert_eq!(h.state().notes, vec![note(4, 1, 52)]);

    // Select the remaining note, then move focus elsewhere: Delete is ignored.
    let only = note_rect(&h, 0).center();
    click(&mut h, only);
    assert_eq!(h.state().model.selected, Some(0));
    assert!(h.get_by_label("Piano roll").is_focused());
    h.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    h.run();
    assert!(!h.get_by_label("Piano roll").is_focused());
    let edits = h.state().log.len();
    h.key_press(Key::Delete);
    h.key_press(Key::Backspace);
    h.run();
    assert_eq!(h.state().log.len(), edits, "no edit without focus");
    assert_eq!(h.state().notes, vec![note(4, 1, 52)]);
    assert_eq!(h.state().model.selected, Some(0), "the selection survives");
}

#[test]
fn shift_click_toggles_the_accent_velocity() {
    let mut h = harness(ThemeMode::Dark, two_notes());
    let body = note_rect(&h, 0).center();
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    press(&mut h, body, Modifiers::SHIFT);
    release(&mut h, body, Modifiers::SHIFT);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
    let out = h.state().last();
    let (i, n) = out.moved.expect("shift-click reports moved");
    assert_eq!(i, 0);
    assert_eq!(n.velocity, 1.0);
    assert_eq!((n.start, n.length, n.pitch), (0, 2, 48));
    assert_eq!(out.selected, Some(0));

    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    press(&mut h, body, Modifiers::SHIFT);
    release(&mut h, body, Modifiers::SHIFT);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
    assert_eq!(h.state().notes[0].velocity, 0.8);
}

#[test]
fn wheel_scrolls_low_pitch_and_clamps() {
    let mut h = harness(ThemeMode::Light, two_notes());
    let over = cell_center(&h, 8, 50);
    move_to(&mut h, over);
    assert_eq!(h.state().model.low_pitch, 36);

    // Content moving down (positive y) reveals higher pitches: 4 rows = 56 pt.
    wheel(&mut h, 4.0 * 14.0, Modifiers::NONE);
    assert_eq!(h.state().model.low_pitch, 40);
    assert!(h.state().last().scrolled);
    assert!(!h.state().last().add.is_some());

    wheel(&mut h, -2.0 * 14.0, Modifiers::NONE);
    assert_eq!(h.state().model.low_pitch, 38);

    // Shift+wheel is reserved: no change.
    wheel(&mut h, 4.0 * 14.0, Modifiers::SHIFT);
    assert_eq!(h.state().model.low_pitch, 38);

    // Clamps at both ends: 0 and 127 - pitches.
    wheel(&mut h, -5000.0, Modifiers::NONE);
    assert_eq!(h.state().model.low_pitch, 0);
    wheel(&mut h, 5000.0, Modifiers::NONE);
    assert_eq!(h.state().model.low_pitch, 127 - 36);

    // The notes are untouched by scrolling.
    assert_eq!(h.state().notes, two_notes());
}

#[test]
fn geometry_helpers_round_trip() {
    let model = PianoRollModel::default();
    let outer = Rect::from_min_size(pos2(100.0, 50.0), model.size());
    let grid = neu::grid_rect(outer);
    assert_eq!(grid.min.x, outer.min.x + neu::KEY_COLUMN_WIDTH);
    assert_eq!(grid.max, outer.max);

    for step in [0u8, 1, 7, 15] {
        for pitch in [36u8, 37, 48, 71] {
            let n = note(step, 1, pitch);
            let center = neu::note_rect(&model, grid, &n).center();
            assert_eq!(
                neu::cell_at(&model, grid, center),
                Some((usize::from(n.start), n.pitch)),
                "{n:?}"
            );
        }
    }
    // A long note's rect spans its length; the row rect is the full-height row.
    let long = note(4, 3, 60);
    let r = neu::note_rect(&model, grid, &long);
    assert_eq!(r.width(), 3.0 * model.step_width);
    assert_eq!(r.height(), model.row_height - 2.0);
    assert_eq!(neu::row_rect(&model, grid, 60).height(), model.row_height);
    assert!(neu::row_rect(&model, grid, 60).contains_rect(r));

    // Outside the grid: None. In the key column: None.
    assert_eq!(
        neu::cell_at(&model, grid, outer.min + vec2(10.0, 10.0)),
        None
    );
    assert_eq!(neu::cell_at(&model, grid, grid.max + vec2(1.0, 1.0)), None);

    // Black keys.
    assert!(neu::is_black_key(61));
    assert!(!neu::is_black_key(60));
    assert_eq!((36..48u8).filter(|&p| neu::is_black_key(p)).count(), 5);
}
