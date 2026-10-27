//! The PIANO ROLL window (M7, M10; the main window's roll view since M12, WINNER section 5
//! "PIANO ROLL window"): `neu::piano_roll_coloured` over the edit track's notes of the edit
//! pattern, the other tracks' notes as ghosts, inside a `window_frame` with its toolbar.
//!
//! - **Header** "PIANO ROLL" · sub "<track> · <preset>" · F5 · POP OUT (lit while the roll
//!   is in its own window).
//! - **Toolbar**: the track dropdown (small, with the track's colour bar; picks the edit
//!   track) · zoom `-` / `+` (the step columns, [`AppState::roll_extra_width`]) · GHOSTS
//!   (small toggle, [`AppState::roll_ghosts`]) · BLACK KEYS (small toggle,
//!   [`AppState::roll_black_keys`]) · the hint "Ctrl+drag selects · wheel scrolls pitch".
//!   Draw | Select and the snap dropdown wait for M13 (the roll's own restyle).
//! - **Body**: the roll in an always-visible horizontal scroll area (fitted to the width at
//!   up to [`MAX_ROLL_STEP_WIDTH`] per step, zoomed wider by the extra width; rows fitted to
//!   the height, at least [`MIN_ROLL_PITCHES`], [`AppState::roll_low_pitch`] at the bottom;
//!   the wheel scrolls pitch) and the CHORDS column ([`CHORD_COLUMN_WIDTH`]) right of it:
//!   [`CHORDS`] as small toggle buttons; with one armed, a draw on the roll places the whole
//!   chord on the drawn root ([`AppState::add_chord`], one undo step).
//!
//! The roll's geometry comes from [`roll_model`] (feeding the model's own `size()` back in
//! reproduces it, so tests can rebuild it from the rendered rect) and every edit the widget
//! reports is applied by [`apply_roll_output`] through the undoable note methods on
//! `AppState`. POP OUT ([`POP_OUT_LABEL`]) moves the whole window, toolbar included, into a
//! window of its own ([`crate::ui::popout`]); the main window shows [`POPPED_LABEL`]
//! meanwhile and DOCK ([`DOCK_LABEL`]) or POP OUT again bring it back.

use egui::{Align, Layout, Role, Ui, Vec2, WidgetInfo, vec2};
use engine::Note;
use neu::{
    ButtonKind, ButtonSize, GhostNote, Icon, KEY_COLUMN_WIDTH, PianoRollModel, PianoRollOutput,
    RULER_HEIGHT, RollNote, TextKind, Theme, VELOCITY_LANE_HEIGHT,
};

use crate::app::{AppState, Panel};
use crate::chords::CHORDS;
use crate::ui::popout;

/// Window title.
pub const TITLE: &str = "Piano roll";
/// Row height of the piano roll (one pitch; WINNER: row 13).
pub const ROLL_ROW_HEIGHT: f32 = 13.0;
/// Fewest pitch rows the roll shows, whatever the height (an octave).
pub const MIN_ROLL_PITCHES: usize = 12;
/// Narrowest and widest fitted step column of the roll (64 steps in a narrow window / 16 in
/// a wide one).
pub const MIN_ROLL_STEP_WIDTH: f32 = 12.0;
pub const MAX_ROLL_STEP_WIDTH: f32 = 48.0;
/// Widest step column the roll zooms to.
pub const MAX_ROLL_STEP_WIDTH_ZOOMED: f32 = 192.0;
/// Points one zoom click adds to every step column.
pub const ZOOM_STEP: f32 = 8.0;
/// Width of the CHORDS column to the right of the roll.
pub const CHORD_COLUMN_WIDTH: f32 = 96.0;
/// Gap between the roll's scroll area and the chord column, and the body padding.
pub const COLUMN_GAP: f32 = 8.0;
pub const BODY_PAD: f32 = 8.0;
/// Caps title over the chord buttons.
pub const CHORDS_TITLE: &str = "Chords";
/// Accessible names of the toolbar controls.
pub const TRACK_DROPDOWN: &str = "Track";
pub const ZOOM_IN_LABEL: &str = "Zoom in";
pub const ZOOM_OUT_LABEL: &str = "Zoom out";
pub const GHOSTS_LABEL: &str = "Ghosts";
pub const BLACK_KEYS_LABEL: &str = "Black keys";
pub const ROLL_HINT: &str = "Ctrl+drag selects · wheel scrolls pitch";
/// Accessible name of the header button that moves the roll into its own window (lit while
/// it is there).
pub const POP_OUT_LABEL: &str = Panel::Roll.pop_out_label();
/// Accessible name of the button in the roll window that brings the roll back.
pub const DOCK_LABEL: &str = Panel::Roll.dock_label();
/// Title of the roll's own window.
pub const ROLL_WINDOW_TITLE: &str = Panel::Roll.window_title();
/// What the view shows in place of the roll while it is in its own window.
pub const POPPED_LABEL: &str = Panel::Roll.popped_label();
/// Size the roll window opens at.
pub const ROLL_WINDOW_SIZE: Vec2 = vec2(1100.0, 620.0);

/// The roll's fitted width: every step at [`MAX_ROLL_STEP_WIDTH`], no wider than
/// `available` (and never narrower than the key column).
#[must_use]
pub fn fitted_roll_width(state: &AppState, available: f32) -> f32 {
    let steps = state.pattern().len().max(1) as f32;
    (KEY_COLUMN_WIDTH + steps * MAX_ROLL_STEP_WIDTH)
        .min(available)
        .max(KEY_COLUMN_WIDTH)
}

/// The roll's width: the fitted width plus the zoom ([`AppState::roll_extra_width`]).
#[must_use]
pub fn roll_width(state: &AppState, available: f32) -> f32 {
    (fitted_roll_width(state, available) + state.roll_extra_width).max(KEY_COLUMN_WIDTH)
}

/// The roll's view model for a roll of `size`: one column per step of the edit pattern, the
/// column width fitted to the width (whole points,
/// `MIN_ROLL_STEP_WIDTH..=MAX_ROLL_STEP_WIDTH_ZOOMED`), as many [`ROLL_ROW_HEIGHT`] rows as
/// fit under the ruler and above the velocity lane (at least [`MIN_ROLL_PITCHES`]),
/// [`AppState::roll_low_pitch`] at the bottom (clamped so the rows stay within MIDI), the
/// playhead while the edit pattern plays, the roll selection and the held keys.
#[must_use]
pub fn roll_model(state: &AppState, size: Vec2) -> PianoRollModel {
    let steps = state.pattern().len().max(1);
    let step_width = ((size.x - KEY_COLUMN_WIDTH) / steps as f32)
        .floor()
        .clamp(MIN_ROLL_STEP_WIDTH, MAX_ROLL_STEP_WIDTH_ZOOMED);
    let rows = (size.y - RULER_HEIGHT - VELOCITY_LANE_HEIGHT) / ROLL_ROW_HEIGHT;
    let pitches = (rows.floor().max(0.0) as usize).max(MIN_ROLL_PITCHES);
    let mut model = PianoRollModel {
        steps,
        low_pitch: state.roll_low_pitch,
        pitches,
        step_width,
        row_height: ROLL_ROW_HEIGHT,
        playhead_step: state.roll_playhead(),
        selected: state.roll_selected,
        selection: roll_selection(state),
        held: state.held_notes,
        black_keys: state.roll_black_keys,
    };
    model.normalise(state.edit_notes().len());
    model
}

/// The other active tracks' notes of the edit pattern as ghosts (M10), each in its track's
/// colour; every track but the edit track, in slot order. Built per frame.
#[must_use]
pub fn roll_ghosts(state: &AppState, theme: &Theme) -> Vec<GhostNote> {
    state
        .active_tracks()
        .filter(|&slot| slot != state.edit_track)
        .flat_map(|slot| {
            let colour = theme.track_colour(state.track(slot).colour);
            state.track_notes(slot).iter().map(move |n| GhostNote {
                start: n.start,
                length: n.length,
                pitch: n.pitch,
                colour,
            })
        })
        .collect()
}

/// The roll's selection: [`AppState::roll_selection`] plus the primary
/// [`AppState::roll_selected`] when it is missing from it.
#[must_use]
pub fn roll_selection(state: &AppState) -> Vec<usize> {
    let mut selection = state.roll_selection.clone();
    if let Some(primary) = state.roll_selected
        && !selection.contains(&primary)
    {
        selection.push(primary);
    }
    selection
}

/// The roll's view of a pattern note (`selected` when it is in the roll selection).
#[must_use]
pub fn roll_notes(state: &AppState) -> Vec<RollNote> {
    state
        .edit_notes()
        .iter()
        .enumerate()
        .map(|(i, n)| RollNote {
            start: n.start,
            length: n.length,
            pitch: n.pitch,
            velocity: n.velocity,
            selected: state.roll_selected == Some(i) || state.roll_selection.contains(&i),
        })
        .collect()
}

/// A roll note as a pattern note.
#[must_use]
pub fn to_note(n: RollNote) -> Note {
    Note::new(n.start, n.length, n.pitch, n.velocity)
}

/// Applies what the roll reported this frame: the scrolled bottom pitch and the selection
/// (view state the widget edits in `model`), then the one edit, if any, through
/// [`AppState::add_note`] / [`AppState::add_chord`] / [`AppState::set_note`] /
/// [`AppState::move_notes`] / [`AppState::remove_note`] / [`AppState::remove_notes`]. The
/// selection follows its notes across the re-sort an add or move causes.
pub fn apply_roll_output(state: &mut AppState, model: &PianoRollModel, out: PianoRollOutput) {
    state.set_roll_low_pitch(model.low_pitch);
    let before = state.edit_notes().to_vec();
    let mut wanted: Vec<Note> = model
        .selection
        .iter()
        .filter_map(|&i| before.get(i).copied())
        .collect();
    let mut primary: Option<Note> = model.selected.and_then(|i| before.get(i).copied());
    let mut follow = |old: Option<&Note>, new: Note, wanted: &mut Vec<Note>| {
        if let Some(old) = old {
            for w in wanted.iter_mut().filter(|w| *w == old) {
                *w = new;
            }
            if primary.as_ref() == Some(old) {
                primary = Some(new);
            }
        }
    };

    if let Some(n) = out.add {
        match state.armed_chord.and_then(|i| CHORDS.get(i)) {
            Some(chord) => {
                state.add_chord(chord, n.pitch, n.start, n.length, n.velocity);
            }
            None => {
                state.add_note(to_note(n));
            }
        }
    } else if !out.moved_many.is_empty() {
        let edits: Vec<(usize, Note)> = out
            .moved_many
            .iter()
            .map(|&(i, n)| (i, to_note(n)))
            .collect();
        state.move_notes(&edits);
        for (i, n) in &edits {
            let mut n = *n;
            n.normalise();
            follow(before.get(*i), n, &mut wanted);
        }
    } else if let Some((i, n)) = out.moved {
        let mut note = to_note(n);
        note.normalise();
        state.set_note(i, note);
        follow(before.get(i), note, &mut wanted);
    } else if !out.removed_many.is_empty() {
        state.remove_notes(&out.removed_many);
        for i in &out.removed_many {
            if let Some(gone) = before.get(*i) {
                wanted.retain(|w| w != gone);
                if primary.as_ref() == Some(gone) {
                    primary = None;
                }
            }
        }
    } else if let Some(i) = out.removed {
        state.remove_note(i);
        if let Some(gone) = before.get(i) {
            wanted.retain(|w| w != gone);
            if primary.as_ref() == Some(gone) {
                primary = None;
            }
        }
    }

    let after = state.edit_notes();
    let mut selection: Vec<usize> = wanted
        .iter()
        .filter_map(|w| after.iter().position(|n| n == w))
        .collect();
    selection.dedup();
    let primary = primary
        .and_then(|p| after.iter().position(|n| *n == p))
        .or_else(|| selection.last().copied());
    state.roll_selection = selection;
    state.roll_selected = primary;
}

/// The header's sub-title: `"Synth · Bass"`.
#[must_use]
pub fn header_sub(state: &AppState) -> String {
    let track = state.edit_track_ui();
    format!("{} · {}", track.name, track.preset_name())
}

/// Renders the PIANO ROLL window into the available rect and applies any edit to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let sub = header_sub(state);
    // The header and the body both edit the state; the frame runs them one after the other.
    let cell = std::cell::RefCell::new(state);
    neu::window_frame_sized(
        ui,
        theme,
        ui.available_size_before_wrap(),
        TITLE,
        Some(&sub),
        |ui| {
            popout::header_control(ui, &mut cell.borrow_mut(), theme, Panel::Roll);
            neu::keycap(ui, theme, "F5");
        },
        |ui| {
            let state = &mut **cell.borrow_mut();
            toolbar(ui, state, theme);
            body(ui, state, theme);
        },
    );
}

/// The toolbar: track dropdown · zoom - / + · GHOSTS · BLACK KEYS · the hint.
fn toolbar(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::toolbar(ui, theme, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let slots: Vec<engine::TrackId> = state.active_tracks().collect();
        let names: Vec<String> = slots.iter().map(|s| state.track(*s).name.clone()).collect();
        let items: Vec<&str> = names.iter().map(String::as_str).collect();
        let track = state.edit_track_ui().clone();
        let colour = theme.track_colour(track.colour);
        if let Some(picked) = neu::dropdown(
            ui,
            theme,
            TRACK_DROPDOWN,
            &track.name,
            &items,
            120.0,
            true,
            Some(colour),
        ) && let Some(slot) = slots.get(picked)
        {
            state.select_track(*slot);
        }
        let small = vec2(22.0, 22.0);
        let out = neu::icon_button_kind(ui, theme, Icon::Minus, small, ButtonKind::Secondary);
        out.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), ZOOM_OUT_LABEL));
        if out.on_hover_text("Narrower step columns").clicked() {
            let steps = state.pattern().len() as f32;
            state.set_roll_extra_width(state.roll_extra_width - ZOOM_STEP * steps);
        }
        let zoom_in = neu::icon_button_kind(ui, theme, Icon::Plus, small, ButtonKind::Secondary);
        zoom_in.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), ZOOM_IN_LABEL));
        if zoom_in.on_hover_text("Wider step columns").clicked() {
            let steps = state.pattern().len() as f32;
            state.set_roll_extra_width(state.roll_extra_width + ZOOM_STEP * steps);
        }
        if neu::button_kind(
            ui,
            theme,
            GHOSTS_LABEL,
            ButtonKind::Toggle(state.roll_ghosts),
            ButtonSize::Small,
        )
        .on_hover_text("Show the other tracks' notes as ghosts (reference only)")
        .clicked()
        {
            state.roll_ghosts = !state.roll_ghosts;
        }
        if neu::button_kind(
            ui,
            theme,
            BLACK_KEYS_LABEL,
            ButtonKind::Toggle(state.roll_black_keys),
            ButtonSize::Small,
        )
        .on_hover_text("Show the black-key rows on the roll (off: white keys only)")
        .clicked()
        {
            state.toggle_black_keys();
        }
        // The hint only where it fits (a narrow main window drops it).
        let hint_width = neu::text::layout(ui.painter(), ROLL_HINT, TextKind::Small, theme.text_3)
            .size()
            .x;
        if ui.available_width() >= hint_width + 8.0 {
            neu::text_widget(ui, ROLL_HINT, TextKind::Small, theme.text_3);
        }
    });
}

/// The body: the roll in its horizontal scroll area, the CHORDS column right of it.
fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let avail = ui.available_size_before_wrap();
    let roll_room =
        (avail.x - 2.0 * BODY_PAD - COLUMN_GAP - CHORD_COLUMN_WIDTH).max(KEY_COLUMN_WIDTH);
    let height = (avail.y - BODY_PAD - neu::SCROLLBAR_WIDTH - 4.0)
        .max(RULER_HEIGHT + VELOCITY_LANE_HEIGHT + MIN_ROLL_PITCHES as f32 * ROLL_ROW_HEIGHT);
    let width = roll_width(state, roll_room);
    let model = roll_model(state, vec2(width, height));
    let size = model.size();
    ui.add_space(BODY_PAD);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(BODY_PAD);
        ui.allocate_ui_with_layout(
            vec2(roll_room, avail.y - BODY_PAD),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(roll_room);
                neu::scroll_horizontal("roll_scroll")
                    .max_height(size.y + neu::SCROLLBAR_WIDTH + 4.0)
                    .show(ui, |ui| {
                        ui.set_min_width(size.x);
                        roll(ui, state, theme, model);
                    });
            },
        );
        ui.add_space(COLUMN_GAP);
        chord_column(ui, state, theme, avail.y - BODY_PAD);
    });
}

/// The roll itself (with ghosts when GHOSTS is on); applies what it reports.
fn roll(ui: &mut Ui, state: &mut AppState, theme: &Theme, mut model: PianoRollModel) {
    let notes = roll_notes(state);
    let ghosts = if state.roll_ghosts {
        roll_ghosts(state, theme)
    } else {
        Vec::new()
    };
    let colour = theme.track_colour(state.edit_track_ui().colour);
    let out = neu::piano_roll_coloured(ui, theme, &mut model, &notes, &ghosts, colour);
    apply_roll_output(state, &model, out);
}

/// The CHORDS column: a caps title and one small toggle per [`CHORDS`] entry (most common at
/// the top) in a scroll area of `height`. The lit one is the armed chord; clicking it again
/// disarms.
fn chord_column(ui: &mut Ui, state: &mut AppState, theme: &Theme, height: f32) {
    ui.allocate_ui_with_layout(
        vec2(CHORD_COLUMN_WIDTH, height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(CHORD_COLUMN_WIDTH);
            ui.spacing_mut().item_spacing.y = 4.0;
            neu::label(ui, theme, CHORDS_TITLE)
                .on_hover_text("Pick a chord, then draw on the roll to place it");
            let list_height = (height - 24.0).max(0.0);
            neu::scroll_vertical("chords")
                .max_height(list_height)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let mut clicked = None;
                    for (i, chord) in CHORDS.iter().enumerate() {
                        let on = state.armed_chord == Some(i);
                        if neu::button_wide(
                            ui,
                            theme,
                            chord.name,
                            ButtonKind::Toggle(on),
                            ButtonSize::Small,
                            CHORD_COLUMN_WIDTH - neu::SCROLLBAR_WIDTH - 4.0,
                        )
                        .clicked()
                        {
                            clicked = Some(i);
                        }
                    }
                    if clicked.is_some() {
                        state.arm_chord(clicked);
                    }
                });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_reproduces_its_own_size() {
        let (state, _engine) = AppState::headless(neu::ThemeMode::Dark);
        let model = roll_model(&state, vec2(700.0, 400.0));
        assert_eq!(model.steps, 16);
        assert_eq!(model.step_width, 41.0, "floor((700 - 40) / 16)");
        assert_eq!(model.row_height, ROLL_ROW_HEIGHT);
        assert_eq!(model.pitches, 26, "floor((400 - 16 - 44) / 13)");
        let again = roll_model(&state, model.size());
        assert_eq!(again.size(), model.size());
        assert_eq!(again.pitches, model.pitches);
        // Too small: an octave at the narrowest columns.
        let tiny = roll_model(&state, vec2(100.0, 50.0));
        assert_eq!(tiny.step_width, MIN_ROLL_STEP_WIDTH);
        assert_eq!(tiny.pitches, MIN_ROLL_PITCHES);
        // The fitted width caps the columns at 48.
        assert_eq!(fitted_roll_width(&state, 2000.0), 40.0 + 16.0 * 48.0);
        assert_eq!(fitted_roll_width(&state, 300.0), 300.0);
        assert_eq!(header_sub(&state), "Synth · Bass");
    }
}
