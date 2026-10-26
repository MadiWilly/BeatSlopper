//! egui_kittest: the M0-M11 behaviour through the M12 Deck shell. The panels render in both
//! themes without a window or an audio device, the computer keyboard plays the on-screen
//! piano, the SYNTH / FX panels leave every parameter untouched until a control is used,
//! clicking a rack cell toggles it, the MIDI lamp renders without opening a port, the M4
//! polish works end to end (Space / R / Ctrl+Z shortcuts, the unsaved marker, the DISCARD /
//! KEEP confirm) and the M5 pattern UI does too: the A..H tiles switch the grid, LEN shows the
//! BAR selector, the queued letter appears while a switch is pending, MODE shows the SONG
//! view, whose ADD / REMOVE edit the chain. The roll, the visualizer, the QOL pass and the
//! M10 tracks follow. The shell's own layout is `tests/shell_layout.rs`.

use app::app::View;
use app::ui;
use app::ui::tracks;
use app::{AppState, BeatSlopperApp, default_params};
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use engine::{DrumRow, Pattern, SynthStep};
use neu::{Theme, ThemeMode};

/// Renders one panel for a few frames at `size` and returns the state afterwards.
fn render_one(
    mode: ThemeMode,
    size: egui::Vec2,
    panel: fn(&mut egui::Ui, &mut AppState, &Theme),
) -> AppState {
    let (state, _engine) = AppState::headless(mode);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder().with_size(size).build_ui_state(
        move |ui, state: &mut AppState| panel(ui, state, &theme),
        state,
    );
    harness.run_steps(4);
    harness.into_state()
}

#[test]
fn synth_panel_renders_in_both_themes_without_touching_params() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(mode, egui::Vec2::new(780.0, 520.0), ui::synth_panel::show);
        assert_eq!(state.mode, mode);
        assert_eq!(state.preset_index(), 0);
        assert_eq!(
            state.params,
            default_params(),
            "rendering must not change params"
        );
    }
}

#[test]
fn fx_rack_renders_in_both_themes_without_touching_params() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(
            mode,
            egui::Vec2::new(ui::fx_rack::WIDTH + 40.0, 520.0),
            ui::fx_rack::show,
        );
        assert_eq!(state.mode, mode);
        assert!(state.limiter_on());
        assert_eq!(
            state.params,
            default_params(),
            "rendering must not change params"
        );
    }
}

#[test]
fn the_shell_renders_in_both_themes_without_touching_params() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(mode, egui::Vec2::new(1280.0, 800.0), ui::show);
        assert_eq!(state.params, default_params());
        assert_eq!(state.mode, mode);
        assert!(!state.test_tone);
        assert_eq!(state.view, View::Roll);
        assert!(!state.dirty);
    }
}

#[test]
fn transport_master_slider_shows_master_gain_without_changing_it() {
    let state = render_one(
        ThemeMode::Dark,
        egui::Vec2::new(1240.0, 200.0),
        ui::transport_bar::show,
    );
    assert_eq!(
        state.master_gain(),
        engine::ParamId::MasterGain.default_value()
    );
}

#[test]
fn keyboard_strip_renders_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (state, _engine) = AppState::headless(mode);
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, ui::keyboard::HEIGHT + 16.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::keyboard::show(ui, state, &theme),
                state,
            );
        harness.run_steps(3);
        harness.get_by_label("Piano keys");
        assert_eq!(
            harness
                .get_by_label(ui::keyboard::OCTAVE_LABEL)
                .value()
                .as_deref(),
            Some("4")
        );
        let state = harness.into_state();
        assert_eq!(state.input.octave, 4);
        assert!(state.held_notes.iter().all(|h| !h));
        assert_eq!(state.note_on_count, 0);
    }
}

#[test]
fn full_layout_renders_in_both_themes_and_counts_frames() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (app, _engine) = BeatSlopperApp::headless(mode, true);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, 800.0))
            .build_ui_state(
                |ui, app: &mut BeatSlopperApp| {
                    app.tick(ui.ctx());
                    app.show(ui);
                },
                app,
            );
        let steps = 3;
        harness.run_steps(steps);
        let app = harness.into_state();
        assert_eq!(app.state().mode, mode);
        assert_eq!(app.theme().mode, mode);
        // The builder settles the first frame (several steps, since smoke mode requests a
        // repaint every frame), then `run_steps` adds exactly `steps` more.
        assert!(app.frames() > steps as u32, "frames = {}", app.frames());
    }
}

/// The whole app at the 1280 x 800 design size.
fn app_harness() -> Harness<'static, BeatSlopperApp> {
    app_harness_sized(egui::Vec2::new(1280.0, 800.0))
}

/// The whole app at 1920 x 1080 (the rehosted MIXER and SYNTH & FX panels fit their windows
/// without scrolling there).
fn wide_harness() -> Harness<'static, BeatSlopperApp> {
    app_harness_sized(egui::Vec2::new(1920.0, 1080.0))
}

/// The whole app at `size`.
fn app_harness_sized(size: egui::Vec2) -> Harness<'static, BeatSlopperApp> {
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut BeatSlopperApp| {
            app.tick(ui.ctx());
            app.show(ui);
        },
        app,
    )
}

fn repeat_event(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers: Modifiers::default(),
    }
}

/// Octave 4 means `A` = C4 = MIDI 60 (see `app::input`): pressing it lights key 60, a repeat
/// event does not retrigger, and releasing it clears the key.
#[test]
fn pressing_a_holds_middle_c_and_repeat_does_not_retrigger() {
    let mut harness = app_harness();
    assert!(!harness.state().state().held_notes[60]);

    harness.key_down(Key::A);
    harness.step();
    let state = harness.state().state();
    assert!(state.held_notes[60], "A should hold C4 (MIDI 60)");
    assert_eq!(state.note_on_count, 1);
    assert_eq!(state.input.held.len(), 1);

    // OS key repeat while held: no second note-on.
    harness.event(repeat_event(Key::A));
    harness.event(repeat_event(Key::A));
    harness.step();
    assert_eq!(harness.state().state().note_on_count, 1);
    assert!(harness.state().state().held_notes[60]);

    harness.key_up(Key::A);
    harness.step();
    let state = harness.state().state();
    assert!(!state.held_notes[60]);
    assert_eq!(state.note_on_count, 1);
    assert_eq!(state.note_off_count, 1);
    assert!(state.input.held.is_empty());
}

#[test]
fn octave_keys_shift_the_keyboard_and_release_uses_the_pressed_note() {
    let mut harness = app_harness();
    harness.key_down(Key::H); // A4 = 69 at octave 4
    harness.step();
    assert!(harness.state().state().held_notes[69]);

    harness.key_down(Key::X);
    harness.key_up(Key::X);
    harness.step();
    assert_eq!(harness.state().state().input.octave, 5);
    assert_eq!(harness.state().state().input.base_note(), 72);
    assert!(
        harness.state().state().held_notes[69],
        "octave shift must not drop held notes"
    );

    harness.key_up(Key::H);
    harness.step();
    let state = harness.state().state();
    assert!(!state.held_notes[69]);
    assert!(state.held_notes.iter().all(|h| !h));

    harness.key_down(Key::Z);
    harness.key_up(Key::Z);
    harness.step();
    assert_eq!(harness.state().state().input.octave, 4);
}

#[test]
fn losing_window_focus_releases_held_notes() {
    let mut harness = app_harness();
    harness.key_down(Key::A);
    harness.key_down(Key::D);
    harness.step();
    assert!(harness.state().state().held_notes[60]);
    assert!(harness.state().state().held_notes[64]);

    harness.event(Event::WindowFocused(false));
    harness.step();
    let state = harness.state().state();
    assert!(state.held_notes.iter().all(|h| !h));
    assert_eq!(state.note_off_count, 2);
}

// -------------------------------------------------------------------------------------------
// M3: the rack, the project section, the transport.

#[test]
fn rack_renders_in_both_themes_without_touching_the_pattern() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(mode, egui::Vec2::new(1000.0, 440.0), ui::sequencer::show);
        assert_eq!(state.mode, mode);
        assert_eq!(
            state.pattern().clone(),
            Pattern::demo(),
            "rendering must not edit the pattern"
        );
        assert_eq!(state.selected_step, None);
        assert_eq!(state.params, default_params());
    }
}

#[test]
fn project_section_renders_in_both_themes_without_dialogs() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(
            mode,
            egui::Vec2::new(ui::browser::WIDTH, 400.0),
            ui::project_bar::show,
        );
        assert_eq!(state.project_path, None);
        assert!(!state.is_exporting());
        assert_eq!(state.status_text(), None);
        assert_eq!(state.export_repeats, 1);
    }
}

/// The CHANNEL RACK window alone at 1000 x 440 (its wide rows: the knob columns show).
fn sequencer_harness() -> Harness<'static, AppState> {
    let (state, _engine) = AppState::headless(ThemeMode::Dark);
    let theme = Theme::for_mode(ThemeMode::Dark);
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 440.0))
        .build_ui_state(
            move |ui, state: &mut AppState| ui::sequencer::show(ui, state, &theme),
            state,
        )
}

/// The M3 UI gate: clicking a step cell toggles it in the pattern mirror.
#[test]
fn clicking_a_step_cell_toggles_it() {
    let mut harness = sequencer_harness();
    let kick = DrumRow::Kick.index();
    assert!(
        !harness.state().pattern().drums[kick][1].on,
        "demo: kick step 2 is off"
    );

    harness.get_by_label("Kick step 2").click();
    harness.step();
    let cell = harness.state().pattern().drums[kick][1];
    assert!(cell.on && !cell.accent, "{cell:?}");

    harness.get_by_label("Kick step 2").click();
    harness.step();
    assert!(!harness.state().pattern().drums[kick][1].on);

    // Shift-click: on with the accent; again: accent off.
    harness
        .get_by_label("Snare step 3")
        .click_modifiers(Modifiers::SHIFT);
    harness.step();
    let cell = harness.state().pattern().drums[DrumRow::Snare.index()][2];
    assert!(cell.on && cell.accent, "{cell:?}");
    harness
        .get_by_label("Snare step 3")
        .click_modifiers(Modifiers::SHIFT);
    harness.step();
    let cell = harness.state().pattern().drums[DrumRow::Snare.index()][2];
    assert!(cell.on && !cell.accent, "{cell:?}");

    // Only the clicked cells changed.
    let mut expected = Pattern::demo();
    expected.drums[DrumRow::Snare.index()][2].on = true;
    assert_eq!(harness.state().pattern().clone(), expected);
}

/// Right-click clears a cell: a drum step goes off (accent too), a track step becomes a rest
/// and loses its selection. One undo step each; an already-clear cell is left alone.
#[test]
fn right_clicking_a_step_cell_clears_it() {
    let mut harness = sequencer_harness();
    let kick = DrumRow::Kick.index();
    let demo = Pattern::demo();
    assert!(demo.drums[kick][0].on && demo.drums[kick][0].accent);
    assert!(!harness.state().dirty);

    harness.get_by_label("Kick step 1").click_secondary();
    harness.step();
    let cell = harness.state().pattern().drums[kick][0];
    assert!(!cell.on && !cell.accent, "{cell:?}");
    assert!(harness.state().dirty);

    // Right-clicking an already-clear cell changes nothing (and is not an undo step).
    harness.get_by_label("Kick step 2").click_secondary();
    harness.step();
    let mut expected = demo.clone();
    expected.drums[kick][0] = Default::default();
    assert_eq!(harness.state().pattern().clone(), expected);

    // A selected track step with a note: the note goes, so does the selection.
    harness.get_by_label("Synth step 5").click();
    harness.step();
    assert_eq!(harness.state().selected_step, Some(4));
    harness.get_by_label("Synth step 5").click_secondary();
    harness.step();
    assert_eq!(harness.state().pattern().synth[4], SynthStep::default());
    assert_eq!(harness.state().selected_step, None);
    expected.set_synth_step(0, 4, None, false);
    assert_eq!(harness.state().pattern().clone(), expected);

    // Two edits, two undo steps: back to the demo pattern.
    assert!(harness.state_mut().undo());
    assert!(harness.state_mut().undo());
    assert_eq!(harness.state().pattern().clone(), demo);
    assert!(!harness.state_mut().undo(), "nothing more to undo");
}

/// The row menu's CLEAR ROW clears one drum row; the toolbar's CLEAR PATTERN clears all.
#[test]
fn clear_row_and_clear_pattern_clear_a_row_and_everything() {
    let mut harness = sequencer_harness();
    harness.get_by_label("C.Hat").click_secondary();
    harness.run_steps(2);
    harness.get_by_label(tracks::CLEAR_ROW_LABEL).click();
    harness.run_steps(2);
    let pattern = harness.state().pattern();
    assert!(
        pattern.drums[DrumRow::ClosedHat.index()]
            .iter()
            .all(|s| !s.on)
    );
    assert!(pattern.drums[DrumRow::Kick.index()].iter().any(|s| s.on));
    assert!(
        harness.query_by_label(tracks::CLEAR_ROW_LABEL).is_none(),
        "the menu closed"
    );

    harness
        .get_by_label(ui::sequencer::CLEAR_PATTERN_LABEL)
        .click();
    harness.step();
    assert!(harness.state().pattern().is_empty());
}

/// The app's per-frame logic (`tick`: keyboard, playhead) over just the rack, so note entry
/// can be tested without the whole window's layout.
fn sequencer_app_harness() -> Harness<'static, BeatSlopperApp> {
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 440.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                let theme = app.theme().clone();
                ui::sequencer::show(ui, app.state_mut(), &theme);
            },
            app,
        )
}

#[test]
fn a_track_cell_selects_the_step_and_the_keyboard_sets_its_note() {
    let mut harness = sequencer_app_harness();
    harness.get_by_label("Synth step 2").click();
    harness.step();
    assert_eq!(harness.state().state().selected_step, Some(1));

    // `A` = C4 = 60 at octave 4: the note lands in the step, still sounds, selection stays.
    harness.key_down(Key::A);
    harness.step();
    let state = harness.state().state();
    assert_eq!(
        state.pattern().synth[1],
        SynthStep {
            note: Some(60),
            accent: false
        }
    );
    assert!(state.held_notes[60]);
    assert_eq!(state.selected_step, Some(1));
    harness.key_up(Key::A);
    harness.step();

    harness.key_down(Key::Delete);
    harness.key_up(Key::Delete);
    harness.step();
    assert_eq!(
        harness.state().state().pattern().synth[1],
        SynthStep::default()
    );
    assert_eq!(harness.state().state().selected_step, Some(1));

    harness.key_down(Key::Escape);
    harness.key_up(Key::Escape);
    harness.step();
    assert_eq!(harness.state().state().selected_step, None);

    // A second click on a selected cell deselects it.
    harness.get_by_label("Synth step 5").click();
    harness.step();
    assert_eq!(harness.state().state().selected_step, Some(4));
    harness.get_by_label("Synth step 5").click();
    harness.step();
    assert_eq!(harness.state().state().selected_step, None);
}

#[test]
fn transport_buttons_drive_play_stop_and_arm() {
    let mut harness = app_harness();
    assert!(!harness.state().state().playing);
    harness.get_by_label("Play").click();
    harness.step();
    assert!(harness.state().state().playing);
    harness.get_by_label("Record").click();
    harness.step();
    assert!(harness.state().state().armed);
    assert!(harness.state().state().is_recording());
    harness.get_by_label("Stop").click();
    harness.step();
    assert!(!harness.state().state().playing);
    assert!(harness.state().state().armed, "stop keeps the arm");
    harness.get_by_label("Record").click();
    harness.step();
    assert!(!harness.state().state().armed);
}

// -------------------------------------------------------------------------------------------
// M4: MIDI.

/// The toolbar shows the "MIDI" lamp (off) whose menu lists "None" first in both themes;
/// rendering connects nothing and touches no parameter.
#[test]
fn midi_lamp_and_selector_render_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (state, _engine) = AppState::headless(mode);
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, 200.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::transport_bar::show(ui, state, &theme),
                state,
            );
        harness.run_steps(4);
        let lamp = harness.get_by_label(ui::transport_bar::MIDI_NAME);
        assert!(lamp.rect().width() > 0.0);
        assert_eq!(
            lamp.accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::False),
            "no port is open"
        );
        // The menu lists "None" first, then every port this machine has.
        let items = ui::transport_bar::midi_items(harness.state());
        assert_eq!(items[0], ui::transport_bar::MIDI_NONE);
        assert_eq!(items.len(), harness.state().midi_ports.len() + 1);
        lamp.click();
        harness.run_steps(2);
        harness.get_by_role_and_label(
            egui::accesskit::Role::MenuItem,
            ui::transport_bar::MIDI_NONE,
        );
        let state = harness.into_state();
        assert_eq!(state.mode, mode);
        assert!(!state.midi_connected(), "rendering must not open a port");
        assert_eq!(state.midi_port, None);
        assert!(
            state.midi_ports_refreshed.is_some(),
            "the selector lists ports"
        );
        assert_eq!(state.params, default_params());
    }
}

/// A MIDI note mirrored to the UI lights the key through the full per-frame path (`tick`)
/// without a local note-on.
#[test]
fn midi_notes_light_keys_through_tick() {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    app.state_mut().midi_rx = Some(rx);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 800.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    tx.try_send(app::input::NoteEvent::On {
        note: 64,
        velocity: 0.9,
    })
    .unwrap();
    harness.step();
    let state = harness.state().state();
    assert!(state.held_notes[64]);
    assert_eq!(state.midi_note_on_count, 1);
    assert_eq!(state.note_on_count, 0);
    tx.try_send(app::input::NoteEvent::Off { note: 64 })
        .unwrap();
    harness.step();
    assert!(!harness.state().state().held_notes[64]);
}

// -------------------------------------------------------------------------------------------
// M4: polish (shortcuts, undo, unsaved marker, discard confirm).

fn temp_project_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("beatslopper-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn space_toggles_play_and_r_arms_without_playing_notes() {
    let mut harness = app_harness();
    harness.key_press(Key::Space);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(harness.state().state().playing, "Space starts the loop");
    harness.key_press(Key::R);
    harness.run_steps(2);
    assert!(harness.state().state().armed, "R arms recording");
    assert!(harness.state().state().is_recording());
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(!harness.state().state().playing, "Space stops it");
    assert!(harness.state().state().armed, "stopping keeps the arm");
    harness.key_press(Key::R);
    harness.run_steps(2);
    assert!(!harness.state().state().armed);
    let state = harness.state().state();
    assert_eq!(state.note_on_count, 0, "neither key is a piano key");
    assert!(state.held_notes.iter().all(|h| !h));
    assert!(!state.dirty, "transport state is not a project edit");
}

#[test]
fn ctrl_z_after_a_step_click_restores_it_and_ctrl_y_redoes() {
    let mut harness = app_harness();
    let kick = DrumRow::Kick.index();
    harness.get_by_label("Kick step 2").click();
    harness.run_steps(2);
    assert!(harness.state().state().pattern().drums[kick][1].on);
    assert!(harness.state().state().dirty);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert!(
        !harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Z undid"
    );
    assert_eq!(harness.state().state().pattern().clone(), Pattern::demo());

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    harness.run_steps(2);
    assert!(
        harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Y redid"
    );

    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    harness.run_steps(2);
    assert!(
        harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Shift+Z: nothing to redo"
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert!(!harness.state().state().pattern().drums[kick][1].on);
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    harness.run_steps(2);
    assert!(
        harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Shift+Z redoes"
    );
    // Ctrl+Z is not a piano key: `Z` alone would shift the octave.
    assert_eq!(harness.state().state().input.octave, 4);
    assert_eq!(harness.state().state().note_on_count, 0);
}

#[test]
fn undo_and_redo_buttons_work_and_grey_out() {
    let mut harness = app_harness();
    // Nothing to undo yet: the toolbar buttons are disabled.
    assert!(harness.get_by_label("Undo").accesskit_node().is_disabled());
    assert!(harness.get_by_label("Redo").accesskit_node().is_disabled());

    harness.get_by_label("Snare step 1").click();
    harness.run_steps(2);
    assert!(harness.state().state().pattern().drums[DrumRow::Snare.index()][0].on);
    assert!(!harness.get_by_label("Undo").accesskit_node().is_disabled());
    harness.get_by_label("Undo").click();
    harness.run_steps(2);
    assert!(!harness.state().state().pattern().drums[DrumRow::Snare.index()][0].on);
    assert!(!harness.get_by_label("Redo").accesskit_node().is_disabled());
    harness.get_by_label("Redo").click();
    harness.run_steps(2);
    assert!(harness.state().state().pattern().drums[DrumRow::Snare.index()][0].on);
}

#[test]
fn unsaved_marker_appears_after_an_edit_and_clears_on_save() {
    let mut harness = app_harness();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label(ui::project_bar::UNSAVED_LABEL)
            .is_none()
    );
    assert!(
        harness
            .query_by_label(ui::transport_bar::UNSAVED_DOT_NAME)
            .is_none()
    );
    assert_eq!(
        harness.state().title(),
        Some("BeatSlopper · 120 BPM · A"),
        "the title carries the tempo and the edit pattern"
    );

    harness.get_by_label("Clap step 1").click();
    harness.run_steps(2);
    assert!(harness.state().state().dirty);
    // The dot after the project name in the toolbar and on the browser's file row.
    harness.get_by_label(ui::project_bar::UNSAVED_LABEL);
    harness.get_by_label(ui::transport_bar::UNSAVED_DOT_NAME);
    assert_eq!(harness.state().title(), Some("BeatSlopper * · 120 BPM · A"));

    // Save to a known temp file: Ctrl+S needs no dialog once there is a path.
    let path = temp_project_path("marker.beatslopper.json");
    harness.state_mut().state_mut().project_path = Some(path.clone());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::S);
    harness.run_steps(2);
    assert!(!harness.state().state().dirty);
    assert!(
        harness
            .query_by_label(ui::project_bar::UNSAVED_LABEL)
            .is_none()
    );
    assert!(
        harness
            .query_by_label(ui::transport_bar::UNSAVED_DOT_NAME)
            .is_none()
    );
    assert_eq!(
        harness.state().title(),
        Some("BeatSlopper – marker.beatslopper.json · 120 BPM · A")
    );
    assert!(path.is_file());
    // The file name in the toolbar and on the browser's file row.
    assert!(
        harness
            .query_all_by_label("marker.beatslopper.json")
            .count()
            >= 1
    );

    // Another edit dirties again (and the title gets its star back).
    harness.get_by_label("Clap step 1").click();
    harness.run_steps(2);
    harness.get_by_label(ui::project_bar::UNSAVED_LABEL);
    harness.get_by_label(ui::transport_bar::UNSAVED_DOT_NAME);
    assert_eq!(
        harness.state().title(),
        Some("BeatSlopper – marker.beatslopper.json * · 120 BPM · A")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn new_while_dirty_asks_and_keep_or_escape_cancels_discard_proceeds() {
    let mut harness = app_harness();
    harness.get_by_label("Kick step 2").click();
    harness.run_steps(2);
    let edited = harness.state().state().pattern().clone();

    harness.get_by_label("New").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().pending_action,
        Some(app::app::PendingAction::New)
    );
    harness.get_by_label(ui::project_bar::DISCARD_QUESTION);
    assert_eq!(
        harness.state().state().pattern().clone(),
        edited,
        "nothing discarded yet"
    );

    harness.get_by_label("Keep").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().pending_action, None);
    assert!(
        harness
            .query_by_label(ui::project_bar::DISCARD_QUESTION)
            .is_none()
    );
    assert_eq!(harness.state().state().pattern().clone(), edited);
    assert!(harness.state().state().dirty);

    // Ctrl+N asks too; Escape cancels.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::N);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().pending_action,
        Some(app::app::PendingAction::New)
    );
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state().state().pending_action, None);
    assert_eq!(harness.state().state().pattern().clone(), edited);

    // The project menu's NEW asks as well; DISCARD creates the new project; undo brings the
    // edit back.
    harness.get_by_label(ui::transport_bar::MENU_NAME).click();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::accesskit::Role::MenuItem, "New")
        .click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().pending_action,
        Some(app::app::PendingAction::New)
    );
    harness.get_by_label("Discard").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().clone(), Pattern::empty());
    assert!(!harness.state().state().dirty);
    assert!(
        harness
            .query_by_label(ui::project_bar::DISCARD_QUESTION)
            .is_none()
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().clone(), edited);
}

/// Open while dirty asks as well; headless there is no dialog, so DISCARD just clears the
/// pending action.
#[test]
fn open_while_dirty_asks_and_never_opens_a_dialog_headless() {
    let mut harness = app_harness();
    harness.get_by_label("Kick step 2").click();
    harness.run_steps(2);
    harness.get_by_label("Open").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().pending_action,
        Some(app::app::PendingAction::Open)
    );
    harness.get_by_label("Discard").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().pending_action, None);
    assert!(harness.state().state().dirty, "no file was opened");
}

// -------------------------------------------------------------------------------------------
// M5: pattern tiles, length / bar selector, queued letter, MODE, the SONG view.

/// A `RadioButton` (segmented item) named `label`, skipping plain labels with the same text
/// (the beat numbers are "1".."4" too).
fn radio<'h, S>(harness: &'h Harness<'_, S>, label: &'h str) -> egui_kittest::Node<'h> {
    harness
        .get_all_by_label(label)
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::RadioButton)
        .unwrap_or_else(|| panic!("no segmented item {label:?}"))
}

/// The pattern tiles' accessible names, A..H.
const TILE_NAMES: [&str; 8] = [
    "Pattern A",
    "Pattern B",
    "Pattern C",
    "Pattern D",
    "Pattern E",
    "Pattern F",
    "Pattern G",
    "Pattern H",
];

/// The pattern tile `letter` ("Pattern B").
fn tile<'h, S>(harness: &'h Harness<'_, S>, letter: &str) -> egui_kittest::Node<'h> {
    let index = ui::sequencer::PATTERN_TABS
        .iter()
        .position(|l| *l == letter)
        .unwrap_or_else(|| panic!("no tile {letter}"));
    harness.get_by_label(TILE_NAMES[index])
}

#[test]
fn pattern_tiles_switch_the_grid_and_edits_land_in_the_selected_pattern() {
    let mut harness = sequencer_harness();
    harness.run_steps(2);
    for letter in ui::sequencer::PATTERN_TABS {
        tile(&harness, letter);
    }
    assert_eq!(harness.state().current(), 0);
    assert_eq!(
        tile(&harness, "A").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    // The demo kick plays on step 1: the grid shows A.
    assert!(harness.state().pattern().drums[DrumRow::Kick.index()][0].on);

    tile(&harness, "B").click();
    harness.step();
    assert_eq!(harness.state().current(), 1);
    assert!(harness.state().pattern().is_empty(), "B is empty");
    assert!(!harness.state().dirty, "selecting a tile is not an edit");

    harness.get_by_label("Kick step 1").click();
    harness.step();
    let state = harness.state();
    assert!(state.bank.patterns[1].drums[DrumRow::Kick.index()][0].on);
    assert_eq!(state.bank.patterns[0], Pattern::demo(), "A is unchanged");
    assert!(state.dirty);

    tile(&harness, "A").click();
    harness.step();
    assert_eq!(harness.state().current(), 0);
    assert_eq!(harness.state().pattern(), &Pattern::demo());
}

#[test]
fn length_two_bars_shows_the_bar_selector_and_bar_two_edits_absolute_steps() {
    let mut harness = sequencer_harness();
    harness.run_steps(2);
    assert!(
        harness.query_by_label(ui::sequencer::BAR_LABEL).is_none(),
        "one bar: no selector"
    );
    assert_eq!(
        radio(&harness, "1 bar").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    radio(&harness, "2 bars").click();
    harness.run_steps(2);
    assert_eq!(harness.state().pattern().len(), 32);
    assert_eq!(
        radio(&harness, "2 bars").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    harness.get_by_label(ui::sequencer::BAR_LABEL);
    radio(&harness, "1");
    radio(&harness, "2").click();
    harness.run_steps(2);
    assert_eq!(harness.state().shown_bar, 1);
    assert!(!harness.state().follow, "picking a bar switches FOLLOW off");
    // Bar 2: beat numbers 5..8 and absolute cell names (doubling copied the loop into bar 2,
    // so step 17 is already on like step 1; step 18 is off like step 2).
    harness.get_by_label("5");
    harness.get_by_label("8");
    assert!(harness.state().pattern().drums[DrumRow::Kick.index()][16].on);
    harness.get_by_label("Kick step 18").click();
    harness.run_steps(2);
    let pattern = harness.state().pattern();
    assert!(pattern.drums[DrumRow::Kick.index()][17].on);
    assert_eq!(
        pattern.drums[DrumRow::Kick.index()][1],
        Pattern::demo().drums[0][1]
    );
    assert!(harness.query_by_label("Kick step 1").is_none());

    harness.get_by_label(ui::sequencer::FOLLOW_LABEL).click();
    harness.run_steps(2);
    assert!(harness.state().follow);

    radio(&harness, "1 bar").click();
    harness.run_steps(2);
    assert_eq!(harness.state().pattern().len(), 16);
    assert_eq!(harness.state().shown_bar, 0, "clamped to the new length");
    assert!(harness.query_by_label(ui::sequencer::BAR_LABEL).is_none());
    harness.get_by_label("Kick step 1");
    // Four bars from one: two doublings, both copying the loop along.
    radio(&harness, "4 bars").click();
    harness.run_steps(2);
    assert_eq!(harness.state().pattern().len(), 64);
    assert!(harness.state().pattern().drums[DrumRow::Kick.index()][48].on);
}

#[test]
fn copy_paste_and_clear_work_from_the_toolbar() {
    let mut harness = sequencer_harness();
    harness.run_steps(2);
    assert!(
        harness
            .get_by_label(ui::sequencer::PASTE_LABEL)
            .accesskit_node()
            .is_disabled(),
        "nothing to paste yet"
    );
    harness.get_by_label(ui::sequencer::COPY_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().clipboard.as_ref(), Some(&Pattern::demo()));
    tile(&harness, "C").click();
    harness.run_steps(2);
    harness.get_by_label(ui::sequencer::PASTE_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().bank.patterns[2], Pattern::demo());
    harness
        .get_by_label(ui::sequencer::CLEAR_PATTERN_LABEL)
        .click();
    harness.run_steps(2);
    assert!(harness.state().bank.patterns[2].is_empty());
    assert_eq!(harness.state().bank.patterns[0], Pattern::demo());
    assert!(harness.state_mut().undo());
    assert_eq!(harness.state().bank.patterns[2], Pattern::demo());
}

/// While playing in pattern mode, clicking another tile queues it: the engine reports the
/// queue once it renders, and the blinking letter ("Queued B") shows beside the tiles until
/// the switch happens at the bar boundary.
#[test]
fn queued_pattern_letter_shows_until_the_bar_boundary() {
    let (app, mut engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 440.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                let theme = app.theme().clone();
                ui::sequencer::show(ui, app.state_mut(), &theme);
            },
            app,
        );
    harness.run_steps(2);
    assert!(harness.query_by_label("Queued B").is_none());
    harness.state_mut().state_mut().play();
    let mut out = vec![0.0f32; 512 * 2];
    engine.render(&mut out, 512);
    harness.step();
    tile(&harness, "B").click();
    harness.step();
    assert_eq!(harness.state().state().current(), 1, "editing B at once");
    engine.render(&mut out, 512);
    harness.step();
    assert_eq!(harness.state().state().playhead.queued, Some(1));
    assert_eq!(harness.state().state().playhead.pattern, 0, "A still plays");
    harness.get_by_label("Queued B");

    // Render to the bar boundary: B plays, the letter is gone.
    let bar = engine::export::bar_frames(app::app::DEFAULT_BPM, 48_000.0);
    let mut rendered = 1024;
    while rendered < bar + 512 {
        engine.render(&mut out, 512);
        rendered += 512;
    }
    harness.step();
    assert_eq!(harness.state().state().playhead.queued, None);
    assert_eq!(harness.state().state().playhead.pattern, 1);
    assert!(harness.query_by_label("Queued B").is_none());
}

#[test]
fn mode_switch_shows_the_song_view_and_add_remove_edit_the_chain() {
    let mut harness = app_harness();
    harness.run_steps(2);
    // The SONG view (its REPEATS stepper) is not showing in pattern mode.
    assert!(harness.query_by_label("Repeats").is_none());
    harness.get_by_label("A · bar 1/1");
    harness.get_by_label(ui::project_bar::EXPORT_LOOP_LABEL);

    radio(&harness, "Song").click();
    harness.run_steps(2);
    let state = harness.state().state();
    assert_eq!(state.bank.mode, engine::PlayMode::Song);
    assert_eq!(state.view, View::Song);
    assert!(!state.dirty, "the mode is not a project edit");
    harness.get_by_label("Repeats");
    harness.get_by_label("SONG 1/1 · A");
    harness.get_by_label("Entry 1: A x1");

    // ADD appends the edit pattern; the new entry is selected and REMOVE takes it away.
    harness.get_by_label("Add").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().chain().len(), 2);
    assert_eq!(harness.state().state().song_selected, Some(1));
    harness.get_by_label("Entry 2: A x1");
    harness.get_by_label("SONG 1/2 · A");
    // The pattern selector changes the selected entry (the rack's tiles are "Pattern B").
    radio(&harness, "B").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().chain()[1],
        engine::ChainEntry::new(1, 1)
    );
    harness.get_by_label("Entry 2: B x1");
    harness.get_by_label("Remove").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().chain().len(), 1);
    assert!(harness.query_by_label("Entry 2: B x1").is_none());
    assert!(harness.state().state().dirty, "chain edits are edits");
    // Undo brings the entry back.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().chain().len(), 2);

    // The PROJECT section reads EXPORT SONG in song mode and drops the repeats selector; the
    // status line (what an export reports) sits inside the browser column under it.
    harness.get_by_label(ui::project_bar::EXPORT_SONG_LABEL);
    assert!(
        harness
            .query_all_by_label("x1")
            .all(|n| n.accesskit_node().role() != egui::accesskit::Role::RadioButton)
    );
    harness.get_by_label(ui::project_bar::WHOLE_SONG_LABEL);
    harness
        .state_mut()
        .state_mut()
        .set_status("Exported song (8.0 s) to song.wav");
    harness.run_steps(2);
    let status = harness
        .get_by_label("Exported song (8.0 s) to song.wav")
        .rect();
    let export = harness
        .get_by_label(ui::project_bar::EXPORT_SONG_LABEL)
        .rect();
    assert!(status.min.y >= export.max.y, "{status:?} under {export:?}");
    let column_right = harness
        .get_by_label(ui::browser::COLLAPSE_LABEL)
        .rect()
        .max
        .x;
    assert!(
        status.max.x <= column_right + 0.5,
        "status {status:?} is inside the column ({column_right})"
    );

    radio(&harness, "Pattern").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().bank.mode, engine::PlayMode::Pattern);
    assert_eq!(harness.state().state().view, View::Roll);
    harness.get_by_label(ui::project_bar::EXPORT_LOOP_LABEL);
    radio(&harness, "x1");
    harness.get_by_label("A · bar 1/1");
}

/// The SONG panel renders on its own in both themes without touching the chain.
#[test]
fn song_panel_renders_in_both_themes_without_editing() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(mode, egui::Vec2::new(300.0, 400.0), ui::song::show);
        assert_eq!(state.chain(), &[engine::ChainEntry::new(0, 1)]);
        assert_eq!(state.song_selected, Some(0));
        assert!(!state.dirty);
    }
}

// -------------------------------------------------------------------------------------------
// M6: the MIXER panel.

/// The MIXER panel renders on its own in both themes without touching the mixer or the
/// parameters, with every strip control addressable by name.
#[test]
fn mixer_panel_renders_in_both_themes_without_editing() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (state, _engine) = AppState::headless(mode);
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(ui::mixer::WIDTH + 40.0, 420.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::mixer::show(ui, state, &theme),
                state,
            );
        harness.run_steps(4);
        let names: Vec<String> = ui::mixer::visible_channels(harness.state())
            .into_iter()
            .map(|c| ui::mixer::channel_name(harness.state(), c))
            .collect();
        assert_eq!(
            names.last().map(String::as_str),
            Some("Synth"),
            "one track strip"
        );
        for name in names {
            harness.get_by_label(&format!("{name} pan"));
            harness.get_by_label(&format!("{name} level"));
            harness.get_by_label(&format!("Mute {name}"));
            harness.get_by_label(&format!("Solo {name}"));
            harness.get_by_label(&ui::mixer::source_box_name(&name));
            // M11: the sends moved onto the inserts.
            assert!(
                harness
                    .query_by_label(&format!("{name} delay send"))
                    .is_none()
            );
        }
        harness.get_by_label("Master");
        harness.get_by_label("Limit");
        // M11: sixteen insert strips, laid out whether or not they are scrolled into view.
        for n in 1..=16u8 {
            harness.get_by_label(&ui::mixer::insert_head_name(n, &format!("Insert {n}")));
            harness.get_by_label(&ui::mixer::insert_control_name(
                n,
                engine::InsertParam::Level,
            ));
            harness.get_by_label(&format!("Mute insert {n}"));
            harness.get_by_label(&format!("Solo insert {n}"));
            harness.get_by_label(&ui::mixer::route_name(n));
            for slot in 0..3 {
                harness.get_by_label(&ui::mixer::slot_name(n, slot));
                assert!(
                    harness
                        .query_by_label(&ui::mixer::slot_edit_name(n, slot))
                        .is_none(),
                    "an empty slot has no edit square"
                );
            }
            for label in ["low", "mid", "mid hz", "high", "delay send", "reverb send"] {
                harness.get_by_label(&format!("Insert {n} {label}"));
            }
        }
        // Sixteen insert meters and the master's stereo meter; no compressor, no GR bar.
        assert_eq!(harness.get_all_by_label("Level").count(), 16);
        harness.get_by_label("Stereo level");
        assert!(harness.query_by_label(ui::mixer::GR_METER_NAME).is_none());
        let state = harness.into_state();
        assert_eq!(state.mode, mode);
        assert_eq!(state.mixer, engine::MixerState::default());
        assert_eq!(state.params, default_params());
        assert!(!state.dirty);
    }
}

/// The MIXER tab shows the mixer view; M / S clicks set the strip's mute / solo (discrete,
/// undoable) and the LIMIT toggle is the limiter parameter; the SYNTH & FX tab shows the
/// rack again.
#[test]
fn mixer_tab_shows_the_panel_and_mute_solo_clicks_set_the_strip() {
    let mut harness = wide_harness();
    harness.run_steps(2);
    assert_eq!(harness.state().state().view, View::Roll);
    assert!(harness.query_by_label("Mute Kick").is_none());

    harness
        .get_by_role_and_label(egui::accesskit::Role::Tab, View::Mixer.tab_label())
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().view, View::Mixer);
    assert!(
        harness.query_by_label("Drive").is_none(),
        "the FX rack is not showing"
    );
    assert!(
        !harness.state().state().dirty,
        "switching views is not an edit"
    );

    harness.get_by_label("Mute Kick").click();
    harness.run_steps(2);
    let state = harness.state().state();
    assert!(state.mixer.strips[0].mute);
    assert!(!state.mixer.strips[0].solo);
    assert!(state.dirty);
    assert_eq!(state.undo.len(), 1);

    harness.get_by_label("Solo Kick").click();
    harness.run_steps(2);
    let state = harness.state().state();
    assert!(state.mixer.strips[0].solo);
    assert!(state.mixer.strips[0].mute);
    assert_eq!(state.undo.len(), 2);
    // The other strips are untouched.
    assert!(state.mixer.strips[1..].iter().all(|s| !s.mute && !s.solo));
    // The rack's LED and S mirror the strip.
    assert_eq!(
        harness
            .get_by_label(&tracks::led_name("Kick"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False),
        "the LED is off while muted"
    );

    // Clicking again clears; Ctrl+Z steps back one click at a time.
    harness.get_by_label("Mute Kick").click();
    harness.run_steps(2);
    assert!(!harness.state().state().mixer.strips[0].mute);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert!(harness.state().state().mixer.strips[0].mute);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert!(!harness.state().state().mixer.strips[0].solo);
    assert!(harness.state().state().mixer.strips[0].mute);

    // LIMIT in the master column is the limiter parameter.
    assert!(harness.state().state().limiter_on());
    harness.get_by_label("Limit").click();
    harness.run_steps(2);
    assert!(!harness.state().state().limiter_on());

    // The SYNTH & FX view shows the rack (and no mixer).
    harness.key_press(Key::F7);
    harness.run_steps(2);
    assert_eq!(harness.state().state().view, View::Synth);
    harness.get_by_label("Drive");
    assert!(harness.query_by_label("Mute Kick").is_none());
}

/// With the MIXER view showing the rack keeps its sixteen steps and the mixer's strips sit
/// inside the main window.
#[test]
fn the_rack_stays_whole_with_the_mixer_shown() {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    app.state_mut().set_view(View::Mixer);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1920.0, 1080.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    harness.get_by_label("Mute Kick");
    let last = harness.get_by_label("Synth step 16").rect();
    let handle = harness.get_by_label(ui::shell::RACK_HANDLE_NAME).rect();
    assert!(
        last.max.x <= handle.min.x,
        "the steps end before the splitter"
    );
    let mixer_bar = harness.get_by_label(ui::shell::MIXER_BAR_NAME).rect();
    assert!(last.max.y < mixer_bar.min.y);
    // The strips sit right of the splitter, above the mixer bar.
    let kick_level = harness.get_by_label("Kick level").rect();
    let synth_level = harness.get_by_label("Synth level").rect();
    let stereo = harness.get_by_label("Stereo level").rect();
    assert!(
        kick_level.min.x >= handle.max.x,
        "{kick_level:?} vs {handle:?}"
    );
    assert!(kick_level.max.x < synth_level.min.x);
    assert!(synth_level.max.x < stereo.min.x);
    assert!(stereo.max.x <= 1920.0 && stereo.max.y <= mixer_bar.min.y);
}

// -------------------------------------------------------------------------------------------
// M7: the piano roll.

use egui::{PointerButton, Pos2};
use engine::Note;
use neu::{PianoRollModel, RollNote};

/// The roll's model and grid rect as rendered, rebuilt from the "Piano roll" node's rect the
/// way `ui::roll_view::roll_model` documents.
fn roll_geometry<S>(harness: &Harness<'_, S>, state: &AppState) -> (PianoRollModel, egui::Rect) {
    let outer = harness.get_by_label("Piano roll").rect();
    let model = ui::roll_view::roll_model(state, outer.size());
    assert_eq!(
        model.size(),
        outer.size(),
        "the model reproduces its own rect"
    );
    (model, neu::grid_rect(outer))
}

/// Centre of the roll cell `(step, pitch)`.
fn roll_cell(model: &PianoRollModel, grid: egui::Rect, step: u8, pitch: u8) -> Pos2 {
    let probe = RollNote {
        start: step,
        length: 1,
        pitch,
        velocity: 0.8,
        selected: false,
    };
    neu::note_rect(model, grid, &probe).center()
}

fn pointer_press<S>(harness: &mut Harness<'_, S>, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn pointer_release<S>(harness: &mut Harness<'_, S>, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn pointer_drag<S>(harness: &mut Harness<'_, S>, from: Pos2, to: Pos2) {
    pointer_press(harness, from);
    harness.event(Event::PointerMoved(to));
    harness.step();
    pointer_release(harness, to);
}

fn pointer_click<S>(harness: &mut Harness<'_, S>, pos: Pos2) {
    pointer_press(harness, pos);
    pointer_release(harness, pos);
    harness.step();
}

/// The PIANO ROLL window alone at 1000 x 440 with an empty edit track.
fn roll_harness(mode: ThemeMode) -> Harness<'static, AppState> {
    roll_harness_sized(mode, egui::Vec2::new(1000.0, 440.0))
}

/// [`roll_harness`] at another window size.
fn roll_harness_sized(mode: ThemeMode, size: egui::Vec2) -> Harness<'static, AppState> {
    let (mut state, _engine) = AppState::headless(mode);
    state.clear_synth();
    state.undo.clear();
    state.dirty = false;
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            move |ui, state: &mut AppState| ui::roll_view::show(ui, state, &theme),
            state,
        );
    harness.run_steps(3);
    harness
}

/// The roll view shows the piano roll in both themes without editing anything: sixteen
/// columns fitted to the window, rows fitted to its height, the default bottom pitch.
#[test]
fn the_roll_view_shows_the_piano_roll_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (state, _engine) = AppState::headless(mode);
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1000.0, 440.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::roll_view::show(ui, state, &theme),
                state,
            );
        harness.run_steps(3);
        let roll = harness.get_by_label("Piano roll").rect();
        let (model, _grid) = roll_geometry(&harness, harness.state());
        assert_eq!(model.steps, 16);
        assert_eq!(model.step_width, ui::roll_view::MAX_ROLL_STEP_WIDTH);
        assert_eq!(model.row_height, ui::roll_view::ROLL_ROW_HEIGHT);
        assert!(
            model.pitches >= ui::roll_view::MIN_ROLL_PITCHES,
            "{}",
            model.pitches
        );
        assert!(roll.max.y <= 440.0 && roll.max.x <= 1000.0, "{roll:?}");
        assert_eq!(model.low_pitch, app::app::DEFAULT_ROLL_LOW_PITCH);
        // The toolbar: the track dropdown names the edit track; GHOSTS and BLACK KEYS are on.
        assert_eq!(
            harness
                .get_by_label(ui::roll_view::TRACK_DROPDOWN)
                .value()
                .as_deref(),
            Some("Synth")
        );
        for label in [ui::roll_view::GHOSTS_LABEL, ui::roll_view::BLACK_KEYS_LABEL] {
            assert_eq!(
                harness.get_by_label(label).accesskit_node().toggled(),
                Some(egui::accesskit::Toggled::True)
            );
        }
        harness.get_by_label(ui::roll_view::CHORDS_TITLE);
        // The demo's bass line is on the roll: nothing was edited by rendering.
        assert_eq!(harness.state().pattern().clone(), Pattern::demo());
        assert!(!harness.state().dirty);
        assert_eq!(harness.state().undo.len(), 0);
    }
}

/// The CHORDS column arms a chord: the next click on the roll places every tone of it on the
/// clicked root as one undo step; clicking the lit button disarms it and a click is a plain
/// note again.
#[test]
fn chord_buttons_arm_a_chord_that_a_roll_click_places() {
    let mut harness = roll_harness(ThemeMode::Dark);
    harness.get_by_label(ui::roll_view::CHORDS_TITLE);
    harness.get_by_label("Major").click();
    harness.step();
    assert_eq!(harness.state().armed_chord, Some(0));
    assert!(!harness.state().dirty, "arming is not an edit");

    let (model, grid) = roll_geometry(&harness, harness.state());
    assert!(
        model.is_visible(40) && model.is_visible(47),
        "{}",
        model.pitches
    );
    // E2 as the root: E2 G#2 B2.
    pointer_click(&mut harness, roll_cell(&model, grid, 2, 40));
    let notes = harness.state().pattern().notes(0).to_vec();
    let pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    assert_eq!(pitches, vec![40, 44, 47]);
    assert!(notes.iter().all(|n| n.start == 2 && n.length == 1));
    assert_eq!(harness.state().undo.len(), 1, "one undo step");
    assert!(harness.state().dirty);
    assert_eq!(harness.state().armed_chord, Some(0), "stays armed");

    harness.get_by_label("Major").click();
    harness.step();
    assert_eq!(harness.state().armed_chord, None);
    pointer_click(&mut harness, roll_cell(&model, grid, 5, 50));
    assert_eq!(
        harness.state().pattern().notes(0).len(),
        4,
        "a plain note again"
    );
}

/// The zoom buttons widen the step columns (view state, not an edit) and the roll scrolls
/// sideways in its window; zooming out never goes below the fitted width.
#[test]
fn zoom_buttons_widen_the_columns_and_the_chords_stay_in_view() {
    let mut harness = roll_harness(ThemeMode::Dark);
    let before = harness.get_by_label("Piano roll").rect();
    let (model, _grid) = roll_geometry(&harness, harness.state());
    assert_eq!(model.step_width, ui::roll_view::MAX_ROLL_STEP_WIDTH);

    harness.get_by_label(ui::roll_view::ZOOM_IN_LABEL).click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().roll_extra_width,
        ui::roll_view::ZOOM_STEP * 16.0
    );
    let (model, _grid) = roll_geometry(&harness, harness.state());
    assert_eq!(
        model.step_width,
        ui::roll_view::MAX_ROLL_STEP_WIDTH + ui::roll_view::ZOOM_STEP
    );
    let after = harness.get_by_label("Piano roll").rect();
    assert_eq!(
        after.width(),
        before.width() + ui::roll_view::ZOOM_STEP * 16.0
    );
    assert!(!harness.state().dirty);
    assert_eq!(harness.state().undo.len(), 0);

    // Far in: the CHORDS column keeps its place inside the window right of the scroller.
    for _ in 0..12 {
        harness.get_by_label(ui::roll_view::ZOOM_IN_LABEL).click();
        harness.run_steps(1);
    }
    harness.run_steps(2);
    let widest = harness.get_by_label("Piano roll").rect();
    assert!(widest.width() > after.width());
    let chords = harness.get_by_label("Major").rect();
    assert!(
        chords.max.x <= 1000.0 && chords.min.x > before.max.x,
        "chords {chords:?} roll {widest:?}"
    );

    // Out again, past the start: back to the fitted width, never narrower.
    for _ in 0..20 {
        harness.get_by_label(ui::roll_view::ZOOM_OUT_LABEL).click();
        harness.run_steps(1);
    }
    harness.run_steps(2);
    assert_eq!(harness.state().roll_extra_width, 0.0);
    assert_eq!(
        harness.get_by_label("Piano roll").rect().width(),
        before.width()
    );
    assert_eq!(harness.state().undo.len(), 0);
}

fn pointer_press_with<S>(harness: &mut Harness<'_, S>, pos: Pos2, modifiers: Modifiers) {
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    harness.step();
}

fn pointer_release_with<S>(harness: &mut Harness<'_, S>, pos: Pos2, modifiers: Modifiers) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.step();
}

fn secondary_press<S>(harness: &mut Harness<'_, S>, pos: Pos2, pressed: bool) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

/// A roll with three notes (E2 at step 3, G#2 at step 5, B2 at step 7) and a clean history.
fn three_note_roll() -> Harness<'static, AppState> {
    let mut harness = roll_harness(ThemeMode::Dark);
    let state = harness.state_mut();
    state.add_note(Note::new(2, 1, 40, 0.8));
    state.add_note(Note::new(4, 1, 44, 0.8));
    state.add_note(Note::new(6, 1, 47, 0.8));
    state.undo.clear();
    state.dirty = false;
    harness.run_steps(2);
    harness
}

/// A click selects one note, Ctrl-click adds another, dragging one of them moves both (one
/// undo step), a Ctrl-drag marquee selects every note it touches and Delete removes them all.
#[test]
fn ctrl_click_marquee_and_group_drag_edit_a_selection_together() {
    let mut harness = three_note_roll();
    let (model, grid) = roll_geometry(&harness, harness.state());
    pointer_click(&mut harness, roll_cell(&model, grid, 2, 40));
    assert_eq!(harness.state().roll_selected, Some(0));
    assert_eq!(harness.state().roll_selection, [0]);

    let second = roll_cell(&model, grid, 4, 44);
    pointer_press_with(&mut harness, second, Modifiers::COMMAND);
    pointer_release_with(&mut harness, second, Modifiers::COMMAND);
    assert_eq!(harness.state().roll_selection, [0, 1]);
    assert_eq!(
        harness.state().roll_selected,
        Some(1),
        "the last clicked is primary"
    );
    assert_eq!(harness.state().undo.len(), 0, "selecting is not an edit");

    // Drag the second note two steps right: both selected notes move, the third stays.
    pointer_drag(&mut harness, second, roll_cell(&model, grid, 6, 44));
    harness.step();
    assert_eq!(
        harness.state().pattern().notes(0),
        &[
            Note::new(4, 1, 40, 0.8),
            Note::new(6, 1, 44, 0.8),
            Note::new(6, 1, 47, 0.8)
        ]
    );
    assert_eq!(harness.state().undo.len(), 1, "one undo step for the group");
    assert_eq!(
        harness.state().roll_selection,
        [0, 1],
        "the selection follows"
    );
    assert_eq!(harness.state().roll_selected, Some(1));

    // Ctrl-click the primary again: it leaves the selection.
    let moved_second = roll_cell(&model, grid, 6, 44);
    pointer_press_with(&mut harness, moved_second, Modifiers::COMMAND);
    pointer_release_with(&mut harness, moved_second, Modifiers::COMMAND);
    assert_eq!(harness.state().roll_selection, [0]);
    assert_eq!(harness.state().roll_selected, Some(0));

    // A Ctrl-drag marquee from an empty corner over everything selects all three.
    let from = roll_cell(&model, grid, 0, 38);
    let to = roll_cell(&model, grid, 9, 50);
    pointer_press_with(&mut harness, from, Modifiers::COMMAND);
    harness.event(Event::PointerMoved(to));
    harness.step();
    pointer_release_with(&mut harness, to, Modifiers::COMMAND);
    assert_eq!(harness.state().roll_selection.len(), 3);
    assert_eq!(
        harness.state().pattern().notes(0).len(),
        3,
        "a marquee adds nothing"
    );

    harness.key_press_modifiers(Modifiers::NONE, Key::Delete);
    harness.step();
    assert!(harness.state().pattern().notes(0).is_empty());
    assert_eq!(harness.state().undo.len(), 2);
    assert!(harness.state().roll_selection.is_empty());
    assert!(harness.state_mut().undo());
    assert_eq!(harness.state().pattern().notes(0).len(), 3);
}

/// The arrow keys nudge the selection by a row or a step; Ctrl+Up / Ctrl+Down move it an
/// octave; a nudge that would push a note off the grid does nothing.
#[test]
fn arrow_keys_nudge_the_selection_and_ctrl_arrows_shift_an_octave() {
    let mut harness = three_note_roll();
    let (model, grid) = roll_geometry(&harness, harness.state());
    pointer_click(&mut harness, roll_cell(&model, grid, 2, 40));
    harness.key_press_modifiers(Modifiers::NONE, Key::ArrowUp);
    harness.step();
    assert_eq!(
        harness.state().pattern().notes(0)[0],
        Note::new(2, 1, 41, 0.8)
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::ArrowUp);
    harness.step();
    assert_eq!(
        harness.state().pattern().notes(0)[0],
        Note::new(2, 1, 53, 0.8)
    );
    harness.key_press_modifiers(Modifiers::NONE, Key::ArrowRight);
    harness.step();
    assert_eq!(
        harness.state().pattern().notes(0)[0],
        Note::new(3, 1, 53, 0.8)
    );
    assert_eq!(harness.state().undo.len(), 3);
    assert_eq!(harness.state().roll_selected, Some(0), "still selected");

    // Add the third note (B2) to the selection: Ctrl+Down drops both an octave.
    let third = roll_cell(&model, grid, 6, 47);
    pointer_press_with(&mut harness, third, Modifiers::COMMAND);
    pointer_release_with(&mut harness, third, Modifiers::COMMAND);
    assert_eq!(harness.state().roll_selection, [0, 2]);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::ArrowDown);
    harness.step();
    let notes = harness.state().pattern().notes(0).to_vec();
    assert_eq!(notes[0], Note::new(3, 1, 41, 0.8));
    assert_eq!(
        notes[1],
        Note::new(4, 1, 44, 0.8),
        "not selected, unchanged"
    );
    assert_eq!(notes[2], Note::new(6, 1, 35, 0.8));
    assert_eq!(harness.state().undo.len(), 4);
    assert_eq!(harness.state().roll_selection, [0, 2]);

    // Ten steps left would push the first note off the grid: nobody moves.
    for _ in 0..10 {
        harness.key_press_modifiers(Modifiers::NONE, Key::ArrowLeft);
        harness.step();
    }
    // (The list re-sorts by start, so compare by value.)
    assert_eq!(
        harness.state().pattern().notes(0),
        &[
            Note::new(0, 1, 41, 0.8),
            Note::new(3, 1, 35, 0.8),
            Note::new(4, 1, 44, 0.8)
        ]
    );
    assert_eq!(harness.state().undo.len(), 7, "three nudges fitted");
    assert_eq!(
        harness.state().roll_selection,
        [0, 1],
        "the selection followed"
    );
}

/// A right-click on a note removes it; the right button held while crossing a note removes
/// that one too. Each is one undo step.
#[test]
fn right_click_and_right_drag_remove_notes() {
    let mut harness = three_note_roll();
    let (model, grid) = roll_geometry(&harness, harness.state());
    let first = roll_cell(&model, grid, 2, 40);
    secondary_press(&mut harness, first, true);
    secondary_press(&mut harness, first, false);
    assert_eq!(harness.state().pattern().notes(0).len(), 2);
    assert_eq!(
        harness.state().pattern().notes(0)[0],
        Note::new(4, 1, 44, 0.8)
    );
    assert_eq!(harness.state().undo.len(), 1);

    // Right button down on empty space, then across the B2: gone.
    secondary_press(&mut harness, roll_cell(&model, grid, 9, 50), true);
    assert_eq!(
        harness.state().pattern().notes(0).len(),
        2,
        "empty space: nothing"
    );
    harness.event(Event::PointerMoved(roll_cell(&model, grid, 6, 47)));
    harness.step();
    secondary_press(&mut harness, roll_cell(&model, grid, 6, 47), false);
    assert_eq!(
        harness.state().pattern().notes(0),
        &[Note::new(4, 1, 44, 0.8)]
    );
    assert_eq!(harness.state().undo.len(), 2);
}

/// The BLACK KEYS toggle hides the black-key rows: every row is a white key and notes on
/// black keys have no row until it is turned back on. Persisted view state, not an edit; the
/// toggle lives on the roll view only.
#[test]
fn black_keys_toggle_hides_the_black_rows() {
    let mut harness = roll_harness(ThemeMode::Dark);
    harness.state_mut().add_note(Note::new(0, 1, 37, 0.8));
    harness.state_mut().undo.clear();
    harness.run_steps(2);
    let (model, _grid) = roll_geometry(&harness, harness.state());
    assert!(model.black_keys && model.is_visible(37));

    harness
        .get_by_label(ui::roll_view::BLACK_KEYS_LABEL)
        .click();
    harness.run_steps(2);
    assert!(!harness.state().roll_black_keys);
    let (model, _grid) = roll_geometry(&harness, harness.state());
    assert!(!model.black_keys);
    assert_eq!(model.pitch_at_row(0), Some(36));
    assert_eq!(model.pitch_at_row(1), Some(38), "D2 is the next row");
    assert!(!model.is_visible(37), "C#2 has no row");
    assert_eq!(harness.state().undo.len(), 0);

    harness
        .get_by_label(ui::roll_view::BLACK_KEYS_LABEL)
        .click();
    harness.run_steps(2);
    assert!(harness.state().roll_black_keys);
    assert!(roll_geometry(&harness, harness.state()).0.is_visible(37));

    // In the whole app the toggle is only there in the roll view.
    let mut app = app_harness();
    app.run_steps(2);
    app.get_by_label(ui::roll_view::BLACK_KEYS_LABEL);
    app.key_press(Key::F7);
    app.run_steps(2);
    assert!(
        app.query_by_label(ui::roll_view::BLACK_KEYS_LABEL)
            .is_none()
    );
}

/// The GHOSTS toggle hides the other tracks' notes; a toggle is view state.
#[test]
fn ghosts_toggle_hides_the_other_tracks_notes() {
    let mut harness = roll_harness(ThemeMode::Dark);
    harness.state_mut().add_note(Note::new(0, 2, 40, 0.8));
    harness.state_mut().add_track();
    harness.state_mut().undo.clear();
    harness.state_mut().dirty = false;
    harness.run_steps(2);
    assert!(harness.state().roll_ghosts);
    let theme = Theme::dark();
    assert!(!ui::roll_view::roll_ghosts(harness.state(), &theme).is_empty());
    harness.get_by_label(ui::roll_view::GHOSTS_LABEL).click();
    harness.run_steps(2);
    assert!(!harness.state().roll_ghosts);
    assert_eq!(
        harness
            .get_by_label(ui::roll_view::GHOSTS_LABEL)
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    assert!(!harness.state().dirty && harness.state().undo.is_empty());
    assert!(!harness.state().layout().roll_ghosts, "persisted");
}

/// POP OUT moves the roll into its own window (an embedded window headless): the view shows
/// a placeholder, the roll in the window still edits the pattern, and DOCK (or POP OUT
/// again) brings it back.
#[test]
fn pop_out_moves_the_roll_to_its_own_window_and_dock_returns_it() {
    let mut harness = roll_harness(ThemeMode::Dark);
    harness.get_by_label(ui::roll_view::POP_OUT_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().roll_popped);
    harness.get_by_label(ui::roll_view::POPPED_LABEL);
    harness.get_by_label(ui::roll_view::ROLL_WINDOW_TITLE);
    assert_eq!(
        harness
            .get_all_by_label(ui::roll_view::CHORDS_TITLE)
            .count(),
        1,
        "one CHORDS column: the window's"
    );

    // The roll in the window still draws notes.
    let (model, grid) = roll_geometry(&harness, harness.state());
    assert!(model.is_visible(40), "{}", model.pitches);
    pointer_click(&mut harness, roll_cell(&model, grid, 1, 40));
    assert_eq!(
        harness.state().pattern().notes(0),
        &[Note::new(1, 1, 40, 0.8)]
    );

    harness.get_by_label(ui::roll_view::DOCK_LABEL).click();
    harness.run_steps(3);
    assert!(!harness.state().roll_popped);
    assert!(
        harness
            .query_by_label(ui::roll_view::POPPED_LABEL)
            .is_none()
    );
    assert!(harness.query_by_label(ui::roll_view::DOCK_LABEL).is_none());
    harness.get_by_label("Piano roll");

    // POP OUT again, then the lit button docks it too (the embedded window covers the header
    // in the headless harness, so that click goes through accessibility).
    harness.get_by_label(ui::roll_view::POP_OUT_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().roll_popped);
    harness
        .get_by_label(ui::roll_view::POP_OUT_LABEL)
        .click_accesskit();
    harness.run_steps(3);
    assert!(!harness.state().roll_popped);
}

/// Keys held on the computer keyboard reach the roll's model (its key column lights them).
#[test]
fn held_keys_reach_the_roll_model() {
    let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
    let size = egui::vec2(800.0, 400.0);
    state.note_on(60, 0.8);
    let model = ui::roll_view::roll_model(&state, size);
    assert!(model.held[60]);
    assert_eq!(model.held.iter().filter(|h| **h).count(), 1);
    state.note_off(60);
    let model = ui::roll_view::roll_model(&state, size);
    assert!(!model.held[60]);
}

/// Drawing on the roll adds notes (a drag for a long one, a click for a one-step one); a
/// double-click removes a note; the rack's lane follows every edit.
#[test]
fn drawing_on_the_roll_adds_notes_and_double_click_removes_one() {
    let mut harness = roll_harness(ThemeMode::Dark);
    assert!(harness.state().pattern().notes(0).is_empty());

    // Draw C3 from step 3 to step 6 (four steps).
    let (model, grid) = roll_geometry(&harness, harness.state());
    let from = roll_cell(&model, grid, 2, 48);
    let to = roll_cell(&model, grid, 5, 48);
    pointer_drag(&mut harness, from, to);
    harness.step();
    assert_eq!(
        harness.state().pattern().notes(0),
        &[Note::new(2, 4, 48, 0.8)],
        "the drag drew a four-step C3"
    );
    assert!(harness.state().dirty);
    assert_eq!(harness.state().undo.len(), 1);
    // The lane follows: step 3 shows C3.
    assert_eq!(harness.state().pattern().synth[2].note, Some(48));

    // A click on an empty cell above it adds a one-step E3: a chord at step 3.
    let e3 = roll_cell(&model, grid, 2, 52);
    pointer_click(&mut harness, e3);
    harness.step();
    assert_eq!(
        harness.state().pattern().notes(0),
        &[Note::new(2, 4, 48, 0.8), Note::new(2, 1, 52, 0.8)]
    );
    assert_eq!(harness.state().undo.len(), 2);
    assert_eq!(
        harness.state().pattern().synth[2].note,
        Some(48),
        "the lane shows the lowest note of the chord"
    );

    // Half a second later (so the next click is not a double-click with the one that added
    // the E3) double-click the E3: removed; the C3 stays.
    harness.run_steps(30);
    let e3_rect = neu::note_rect(
        &model,
        grid,
        &RollNote {
            start: 2,
            length: 1,
            pitch: 52,
            velocity: 0.8,
            selected: false,
        },
    );
    pointer_click(&mut harness, e3_rect.center());
    assert_eq!(
        harness.state().roll_selected,
        Some(1),
        "the first click selects"
    );
    pointer_click(&mut harness, e3_rect.center());
    harness.step();
    assert_eq!(
        harness.state().pattern().notes(0),
        &[Note::new(2, 4, 48, 0.8)],
        "the double-click removed the E3"
    );
    assert_eq!(harness.state().roll_selected, None);
    assert_eq!(harness.state().undo.len(), 3);

    // The rack's lane cell of step 3 carries the note; step 4 is a rest.
    let state = harness.state();
    assert_eq!(ui::sequencer::lane_cell(state, 0, 2).note, Some(48));
    assert_eq!(ui::sequencer::lane_cell(state, 0, 3), SynthStep::default());
    assert_eq!(state.undo.len(), 3);
}

/// The whole app, end to end: draw a chord and a long note on the roll, play (the playhead
/// on the roll moves with the offline engine), undo and redo, save, reopen from the file and
/// find the notes back on the roll.
#[test]
fn roll_draw_play_undo_save_and_reopen_through_the_whole_app() {
    let (mut app, mut engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    app.state_mut().clear_synth();
    app.state_mut().undo.clear();
    app.state_mut().dirty = false;
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1920.0, 1080.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    assert_eq!(harness.state().state().view, View::Roll);

    // A C2 E2 G2 chord at step 1 (clicks) and a six-step C3 from step 9 (drag).
    let (model, grid) = roll_geometry(&harness, harness.state().state());
    for pitch in [36u8, 40, 43] {
        let pos = roll_cell(&model, grid, 0, pitch);
        pointer_click(&mut harness, pos);
    }
    let from = roll_cell(&model, grid, 8, 48);
    let to = roll_cell(&model, grid, 13, 48);
    pointer_drag(&mut harness, from, to);
    harness.run_steps(2);
    let expected = [
        Note::new(0, 1, 36, 0.8),
        Note::new(0, 1, 40, 0.8),
        Note::new(0, 1, 43, 0.8),
        Note::new(8, 6, 48, 0.8),
    ];
    assert_eq!(harness.state().state().pattern().notes(0), &expected);
    assert_eq!(harness.state().state().undo.len(), 4);
    assert!(harness.state().state().dirty);
    engine.process_messages();
    assert_eq!(
        engine.pattern().notes(0),
        &expected,
        "every edit reached the engine"
    );
    // The rack's lane shows the chord's lowest note on step 1.
    assert_eq!(
        harness
            .get_by_label("Synth step 1")
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );

    // Space plays; the offline engine renders a few blocks and the roll's playhead follows.
    assert_eq!(harness.state().state().roll_playhead(), None);
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(harness.state().state().playing);
    let mut out = vec![0.0f32; 4096 * 2];
    engine.process_messages();
    for _ in 0..4 {
        engine.render(&mut out, 4096);
    }
    harness.run_steps(2);
    let playhead = harness.state().state().roll_playhead();
    // 4 x 4096 samples at 48 kHz and 120 BPM is 2.7 steps in.
    assert!(
        playhead.is_some_and(|p| p > 1.0 && p < 16.0),
        "playhead {playhead:?}"
    );
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(!harness.state().state().playing);

    // Ctrl+Z takes the long note away, Ctrl+Y brings it back.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().notes(0), &expected[..3]);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().notes(0), &expected);

    // Save, start a new project (roll empty), reopen: the notes are back on the roll.
    let path = temp_project_path("roll.beatslopper.json");
    assert!(
        harness
            .state_mut()
            .state_mut()
            .save_project_to(path.clone())
    );
    assert!(!harness.state().state().dirty);
    harness.state_mut().state_mut().new_project();
    harness.run_steps(2);
    assert!(harness.state().state().pattern().notes(0).is_empty());
    assert!(
        harness
            .state_mut()
            .state_mut()
            .load_project_from(path.clone())
    );
    harness.run_steps(3);
    assert_eq!(harness.state().state().pattern().notes(0), &expected);
    assert_eq!(
        harness.state().state().view,
        View::Roll,
        "the view is a setting, not project data"
    );
    harness.get_by_label("Piano roll");
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------------
// M8: synthesis extensions in the SYNTH panel and the FX rack.

/// The SYNTH panel rendered tall enough that all three body rows are inside the scroll area's
/// viewport, so every new control is clickable.
fn synth_panel_harness(mode: ThemeMode) -> Harness<'static, AppState> {
    let (state, _engine) = AppState::headless(mode);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(780.0, 900.0))
        .build_ui_state(
            move |ui, state: &mut AppState| ui::synth_panel::show(ui, state, &theme),
            state,
        );
    harness.run_steps(3);
    harness
}

#[test]
fn synth_panel_shows_the_m8_groups_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let harness = synth_panel_harness(mode);
        for name in [
            "Pulse", "Spread", "FM Amt", "FM Ratio", "Sub", "Noise", "Damp", "Bright", "F.Att",
            "F.Dec", "F.Sus", "F.Rel", "Key Trk", "Amount", "Decay", "Vel>Amp", "Vel>Cut",
        ] {
            assert!(
                harness.get_all_by_label(name).count() >= 1,
                "{mode:?}: no control named {name:?}"
            );
        }
        // Both oscillators have a PULSE knob; the unison, mode, colour and type selectors are
        // segmented items.
        assert_eq!(harness.get_all_by_label("Pulse").count(), 2);
        for item in ["7", "Pluck", "Pink", "BP"] {
            radio(&harness, item);
        }
        assert_eq!(
            harness.state().params,
            default_params(),
            "rendering must not change params"
        );
    }
}

/// The lowest `RadioButton` named `label`: the MODE group's "Pluck" item.
fn lowest_radio<'h, S>(harness: &'h Harness<'_, S>, label: &'h str) -> egui_kittest::Node<'h> {
    harness
        .get_all_by_label(label)
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::RadioButton)
        .max_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
        .unwrap_or_else(|| panic!("no segmented item {label:?}"))
}

#[test]
fn clicking_pluck_sets_the_osc_mode_and_unison_items_map_to_voice_counts() {
    let mut harness = synth_panel_harness(ThemeMode::Dark);
    assert_eq!(harness.state().param(engine::ParamId::OscMode), 0.0);
    let preset_before = harness.state().preset_index();
    lowest_radio(&harness, "Pluck").click();
    harness.run_steps(2);
    assert_eq!(harness.state().param(engine::ParamId::OscMode), 1.0);
    assert_eq!(
        harness.state().preset_index(),
        preset_before,
        "not the preset"
    );
    assert!(harness.state().dirty, "a mode change is a project edit");

    radio(&harness, "HP").click();
    harness.run_steps(2);
    assert_eq!(harness.state().param(engine::ParamId::FilterType), 2.0);

    // Unison item "5" is five voices (the parameter is 1..=7, not zero-based).
    assert_eq!(harness.state().param(engine::ParamId::Osc1Unison), 1.0);
    radio(&harness, "5").click();
    harness.run_steps(2);
    assert_eq!(harness.state().param(engine::ParamId::Osc1Unison), 5.0);

    radio(&harness, "Pink").click();
    harness.run_steps(2);
    assert_eq!(harness.state().param(engine::ParamId::NoiseColor), 1.0);
}

#[test]
fn fx_rack_shows_the_lofi_chorus_and_sidechain_groups_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (state, _engine) = AppState::headless(mode);
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(ui::fx_rack::WIDTH + 40.0, 700.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::fx_rack::show(ui, state, &theme),
                state,
            );
        harness.run_steps(3);
        for name in [
            "Bits", "Wobble", "Vinyl", "Tone", "Depth", "Pump", "Pump Rel",
        ] {
            assert!(
                harness.get_all_by_label(name).count() >= 1,
                "{mode:?}: no control named {name:?}"
            );
        }
        // LO-FI's RATE and CHORUS's RATE; the DRIVE, DELAY, REVERB, LO-FI and CHORUS MIX knobs.
        assert_eq!(harness.get_all_by_label("Rate").count(), 2);
        assert_eq!(harness.get_all_by_label("Mix").count(), 5);
        assert_eq!(
            harness.state().params,
            default_params(),
            "rendering must not change params"
        );
    }
}

/// In the SYNTH & FX view the SYNTH panel and the FX rack sit side by side inside the window
/// (the M8 rows scroll inside each panel) and the rack is collapsed to its header.
#[test]
fn the_synth_view_hosts_both_panels_beside_each_other() {
    let mut harness = wide_harness();
    harness.key_press(Key::F7);
    harness.run_steps(3);
    assert_eq!(harness.state().state().view, View::Synth);
    assert!(!harness.state().state().rack_open);
    harness.get_by_label(ui::sequencer::EXPAND_LABEL);
    let glide = harness.get_by_label("Glide").rect();
    let fm = harness.get_by_label("FM Amt").rect();
    let drive = harness.get_by_label("Drive").rect();
    let bits = harness.get_by_label("Bits").rect();
    assert!(fm.min.y > glide.min.y, "the M8 row is below the M2 row");
    assert!(bits.min.y > drive.min.y, "LO-FI is below DRIVE");
    assert!(
        drive.min.x > glide.max.x,
        "the FX rack is right of the synth"
    );
    let mixer_bar = harness.get_by_label(ui::shell::MIXER_BAR_NAME).rect();
    for rect in [glide, fm, drive, bits] {
        assert!(
            rect.max.y <= mixer_bar.min.y && rect.max.x <= 1920.0,
            "{rect:?}"
        );
    }
    // The toolbar strip names the track and its preset, with the plugin pills.
    assert_eq!(
        harness
            .get_by_label(ui::synth_panel::TOOLBAR_PRESET_NAME)
            .value()
            .as_deref(),
        Some("Bass")
    );
    harness.get_by_label(ui::synth_panel::TOOLBAR_MODIFIED_LABEL);
    harness.get_by_label("EQ");
    harness.get_by_label("REVERB");
    // F5 brings the roll and the expanded rack back.
    harness.key_press(Key::F5);
    harness.run_steps(2);
    assert!(harness.state().state().rack_open);
    harness.get_by_label("Kick step 1");
}

// ---------------------------------------------------------------------------------------------
// Visualizers.

use app::app::load_visual_preset;
use app::ui::visualizer::{CLOSE_LABEL, FULL_LABEL};

/// Asserts that the VISUALIZER panel's PRESET stepper shows `text`: a button named
/// `"<label> value"` whose accessible value is the shown text.
fn assert_preset_shows<S>(harness: &Harness<'_, S>, text: &str) {
    assert_eq!(
        harness
            .get_by_label(&neu::knob::readout_name(ui::visualizer::PRESET_LABEL))
            .value()
            .as_deref(),
        Some(text),
        "the PRESET readout"
    );
}

/// The whole shell with the VISUALIZER view showing.
fn visual_harness(mode: ThemeMode) -> Harness<'static, AppState> {
    let (mut state, _engine) = AppState::headless(mode);
    state.set_view(View::Visual);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 800.0))
        .build_ui_state(
            move |ui, state: &mut AppState| {
                state.handle_keyboard(ui.ctx());
                ui::show(ui, state, &theme);
            },
            state,
        );
    harness.run_steps(4);
    harness
}

#[test]
fn visualizer_view_fits_the_main_window_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let harness = visual_harness(mode);
        let mixer_bar = harness.get_by_label(ui::shell::MIXER_BAR_NAME).rect();
        let scopes: Vec<egui::Rect> = harness
            .query_all_by_label("Scope visualizer")
            .map(|n| n.rect())
            .collect();
        assert_eq!(scopes.len(), 1, "the view's drawing (no transport strip)");
        let scope = scopes[0];
        assert!(scope.width() > 100.0 && scope.height() > 40.0, "{scope:?}");
        assert!(
            scope.max.y <= mixer_bar.min.y && scope.max.x <= 1280.0,
            "the drawing ends inside the window: {scope:?}"
        );
        // Right of the collapsed rack's header, in the main window.
        let expand = harness.get_by_label(ui::sequencer::EXPAND_LABEL).rect();
        assert!(scope.min.y >= expand.max.y, "{scope:?} under {expand:?}");
        harness.get_by_label(FULL_LABEL);
        harness.get_by_label(ui::visualizer::PRESET_LABEL);
        assert_preset_shows(&harness, "Soft Scope");
        assert_eq!(harness.state().view, View::Visual);
        assert!(!harness.state().visual_full);
    }
}

#[test]
fn full_toggle_opens_the_overlay_and_escape_or_close_shuts_it() {
    let mut harness = visual_harness(ThemeMode::Dark);
    assert!(harness.query_by_label(CLOSE_LABEL).is_none());

    harness.get_by_label(FULL_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().visual_full);
    let close = harness.get_by_label(CLOSE_LABEL).rect();
    let workspace = harness
        .state()
        .workspace_rect
        .expect("the workspace was measured");
    assert!(
        workspace.contains_rect(close),
        "CLOSE sits inside the overlay over the workspace"
    );
    assert_eq!(
        harness.query_all_by_label("Scope visualizer").count(),
        2,
        "panel + overlay"
    );

    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!harness.state().visual_full, "Escape closes the overlay");
    assert!(harness.query_by_label(CLOSE_LABEL).is_none());

    harness.get_by_label(FULL_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().visual_full);
    harness.get_by_label(CLOSE_LABEL).click();
    harness.run_steps(3);
    assert!(!harness.state().visual_full, "CLOSE closes the overlay");
}

#[test]
fn preset_pills_choose_a_preset_and_off_hides_the_drawing() {
    let mut harness = visual_harness(ThemeMode::Light);
    // The overlay carries one pill per preset; the panel's stepper shows the chosen name.
    harness.get_by_label(FULL_LABEL).click();
    harness.run_steps(2);
    harness.get_by_label("Orbit").click();
    harness.run_steps(2);
    assert_eq!(harness.state().visual_preset().name, "Orbit");
    assert_eq!(
        harness.query_all_by_label("Radial visualizer").count(),
        2,
        "panel + overlay"
    );
    harness.get_by_label("Off").click();
    harness.run_steps(2);
    assert!(harness.state().visual_preset().is_off());
    assert!(harness.query_by_label("Radial visualizer").is_none());
    assert!(harness.query_by_label("Scope visualizer").is_none());
    // Stepping past "Off" wraps to the first preset; the stepper label shows it.
    harness.get_by_label(CLOSE_LABEL).click();
    harness.run_steps(2);
    harness.state_mut().set_visual_preset(neu::OFF_PRESET);
    harness.run_steps(1);
    assert_preset_shows(&harness, "Off");
    harness.state_mut().set_visual_preset(0);
    harness.run_steps(1);
    assert_preset_shows(&harness, "Soft Scope");
    assert!(
        !harness.state().dirty,
        "the visualizer is a setting, not a project edit"
    );
}

#[derive(Default)]
struct MemStorage(std::collections::HashMap<String, String>);

impl eframe::Storage for MemStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn visual_preset_choice_is_persisted() {
    use eframe::App;
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut storage = MemStorage::default();
    assert_eq!(load_visual_preset(&storage), None, "nothing stored yet");
    app.state_mut().set_visual_preset(4);
    app.save(&mut storage);
    assert_eq!(load_visual_preset(&storage), Some(4));
    app.state_mut().set_visual_preset(neu::OFF_PRESET);
    app.save(&mut storage);
    assert_eq!(load_visual_preset(&storage), Some(neu::OFF_PRESET));
    // A stale index past the table is ignored rather than crashing.
    eframe::Storage::set_string(&mut storage, app::app::VISUAL_PRESET_KEY, "99".to_owned());
    assert_eq!(load_visual_preset(&storage), None);
}

/// The shell layout (the view, the docks, the rack width) is saved as one value and comes
/// back on the next launch.
#[test]
fn the_shell_layout_is_persisted() {
    use eframe::App;
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut storage = MemStorage::default();
    app.state_mut().set_view(View::Synth);
    app.state_mut().toggle_browser();
    app.state_mut().set_rack_width(720.0);
    app.state_mut().roll_ghosts = false;
    app.save(&mut storage);
    let stored: app::app::ShellLayout =
        eframe::get_value(&storage, app::app::SHELL_LAYOUT_KEY).expect("saved");
    assert_eq!(stored.view, View::Synth);
    assert!(!stored.browser_open && !stored.rack_open && !stored.roll_ghosts);
    assert_eq!(stored.rack_width, 720.0);
    let (mut fresh, _engine) = AppState::headless(ThemeMode::Dark);
    fresh.apply_layout(stored);
    assert_eq!(fresh.view, View::Synth);
    assert!(!fresh.browser_open);
    assert_eq!(fresh.rack_width, 720.0);
}

#[test]
fn rendered_audio_reaches_the_visualizer_feed() {
    let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
    assert!(state.scope_window.is_silent(1e-6));
    assert!(
        !state.visuals_active(),
        "silence, stopped: nothing to animate"
    );
    engine.set_test_tone(true);
    let mut out = vec![0.0_f32; 4096 * 2];
    engine.render(&mut out, 4096);
    state.update_visuals();
    assert!(
        !state.scope_window.is_silent(1e-3),
        "the tap filled the window"
    );
    assert!(state.spectrum.is_active(), "the analyser saw the tone");
    let band = app::spectrum::Spectrum::band_of(440.0).unwrap();
    assert_eq!(
        state.spectrum.peak_bin(),
        Some(band),
        "A4 peaks in its band"
    );
    assert!(state.visuals_active());
    let input = state.visual_input(1.0);
    assert_eq!(input.samples.len(), engine::SCOPE_WINDOW_FRAMES);
    assert_eq!(
        input.samples_r.map(<[f32]>::len),
        Some(engine::SCOPE_WINDOW_FRAMES)
    );
    assert_eq!(input.spectrum.len(), app::spectrum::BINS);
    assert_eq!(input.hits, 0);
    state.set_visual_preset(neu::OFF_PRESET);
    assert!(!state.visuals_active(), "Off never asks for repaints");
}

// QOL pass: typed values, tap tempo, drag-paint, Ctrl-click, row mute, the length selector,
// note nudges, Escape panic, the RECENT section.

use app::ui::tempo::{BPM_VALUE_NAME, TAP_LABEL};

/// The whole app plus the engine behind it, so a test can render a block and check what
/// reached the audio side.
fn app_harness_with_engine(step_dt: f32) -> (Harness<'static, BeatSlopperApp>, engine::Engine) {
    let (app, engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 800.0))
        .with_step_dt(step_dt)
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    (harness, engine)
}

/// Pushes the UI's queued messages into the engine (one rendered block).
fn drain(harness: &mut Harness<'_, BeatSlopperApp>, engine: &mut engine::Engine) {
    harness.state_mut().state_mut().flush_outbox();
    let mut buf = vec![0.0; 2 * 64];
    engine.render(&mut buf, 64);
}

fn type_text(harness: &mut Harness<'_, BeatSlopperApp>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run_steps(2);
}

#[test]
fn clicking_the_bpm_display_and_typing_140_sets_the_tempo_and_sends_set_bpm() {
    let (mut harness, mut engine) = app_harness_with_engine(1.0 / 60.0);
    harness.run_steps(2);
    assert_eq!(
        harness.get_by_label(BPM_VALUE_NAME).value().as_deref(),
        Some("120")
    );
    harness.get_by_label(BPM_VALUE_NAME).click();
    harness.run_steps(3);
    type_text(&mut harness, "140");
    // Shortcuts stay quiet while the editor has the keyboard.
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(!harness.state().state().playing, "Space typed, not played");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(harness.state().state().bpm, 140.0);
    assert_eq!(
        harness.get_by_label(BPM_VALUE_NAME).value().as_deref(),
        Some("140")
    );
    assert!(harness.state().state().dirty);
    drain(&mut harness, &mut engine);
    assert_eq!(engine.transport().bpm(), 140.0, "SetBpm reached the engine");
    assert_eq!(harness.state().title(), Some("BeatSlopper * · 140 BPM · A"));

    // Units and decimals: "128.5 bpm"; Escape cancels an edit.
    harness.get_by_label(BPM_VALUE_NAME).click();
    harness.run_steps(3);
    type_text(&mut harness, "128.5 bpm");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(harness.state().state().bpm, 128.5);
    assert_eq!(
        harness.get_by_label(BPM_VALUE_NAME).value().as_deref(),
        Some("128.5")
    );
    harness.get_by_label(BPM_VALUE_NAME).click();
    harness.run_steps(3);
    type_text(&mut harness, "99");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state().state().bpm, 128.5, "Escape cancelled");

    // Plus / minus nudges from a decimal tempo land on whole numbers.
    harness.key_press(Key::Plus);
    harness.run_steps(2);
    assert_eq!(harness.state().state().bpm, 129.0);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::ArrowDown);
    harness.run_steps(2);
    assert_eq!(harness.state().state().bpm, 128.0);
    // The toolbar's own buttons nudge too.
    harness.get_by_label("Increase").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().bpm, 129.0);
}

#[test]
fn clicking_a_knob_value_and_typing_sets_the_parameter() {
    let mut harness = wide_harness();
    harness.key_press(Key::F7);
    harness.run_steps(2);
    harness.get_by_label("Cutoff value").click();
    harness.run_steps(3);
    type_text(&mut harness, "1.2k");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let cutoff = harness.state().state().param(engine::ParamId::FilterCutoff);
    assert!((cutoff - 1200.0).abs() < 1.0, "{cutoff}");
    assert!(harness.state().state().dirty);
    assert_eq!(
        harness.get_by_label("Cutoff value").value().as_deref(),
        Some("1.2 kHz")
    );
    // A percent knob: "70 %" is exactly 0.7; the undo stack has both edits.
    harness
        .get_all_by_label("Mix value")
        .next()
        .unwrap()
        .click();
    harness.run_steps(3);
    type_text(&mut harness, "60 %");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(harness.state().state().param(engine::ParamId::Osc1Mix), 0.6);
    assert_eq!(harness.state().state().undo.len(), 2);
}

#[test]
fn dragging_across_three_cells_paints_them_as_one_undo_step() {
    let mut harness = app_harness();
    harness.run_steps(2);
    let snare = DrumRow::Snare.index();
    let c2 = harness.get_by_label("Snare step 2").rect().center();
    let c3 = harness.get_by_label("Snare step 3").rect().center();
    let c4 = harness.get_by_label("Snare step 4").rect().center();
    for step in 1..4 {
        assert!(!harness.state().state().pattern().drums[snare][step].on);
    }
    pointer_press(&mut harness, c2);
    assert!(
        harness.state().state().pattern().drums[snare][1].on,
        "the press paints"
    );
    harness.event(Event::PointerMoved(c3));
    harness.step();
    harness.event(Event::PointerMoved(c4));
    harness.step();
    pointer_release(&mut harness, c4);
    harness.run_steps(2);
    let drums = &harness.state().state().pattern().drums[snare];
    assert!(
        drums[1].on && drums[2].on && drums[3].on,
        "{:?}",
        &drums[..4]
    );
    assert!(drums[4].on, "the demo hit on step 5 is untouched");
    assert_eq!(
        harness.state().state().undo.len(),
        1,
        "one stroke, one undo step"
    );
    assert!(
        harness.state().state().paint.is_none(),
        "the stroke ended on release"
    );

    // Pressing an "on" cell starts an erase stroke: 2, 3 and 4 go off again.
    pointer_press(&mut harness, c2);
    harness.event(Event::PointerMoved(c3));
    harness.step();
    harness.event(Event::PointerMoved(c4));
    harness.step();
    pointer_release(&mut harness, c4);
    harness.run_steps(2);
    let drums = &harness.state().state().pattern().drums[snare];
    assert!(!drums[1].on && !drums[2].on && !drums[3].on);
    assert_eq!(harness.state().state().undo.len(), 2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    let drums = &harness.state().state().pattern().drums[snare];
    assert!(
        drums[1].on && drums[2].on && drums[3].on,
        "undo restores the stroke"
    );
}

#[test]
fn ctrl_click_toggles_the_accent() {
    let mut harness = app_harness();
    harness.run_steps(2);
    let kick = DrumRow::Kick.index();
    // Demo: kick step 1 is on; a plain click turns it off first.
    harness.get_by_label("Kick step 1").click();
    harness.run_steps(2);
    assert!(!harness.state().state().pattern().drums[kick][0].on);

    harness
        .get_by_label("Kick step 1")
        .click_modifiers(Modifiers::COMMAND);
    harness.run_steps(2);
    let cell = harness.state().state().pattern().drums[kick][0];
    assert!(
        cell.on && cell.accent,
        "Ctrl-click: on with the accent {cell:?}"
    );
    harness
        .get_by_label("Kick step 1")
        .click_modifiers(Modifiers::COMMAND);
    harness.run_steps(2);
    let cell = harness.state().state().pattern().drums[kick][0];
    assert!(
        cell.on && !cell.accent,
        "Ctrl-click again: accent off {cell:?}"
    );

    // The track lane: Ctrl-click flips the accent (an empty cell gets a note).
    harness
        .get_by_label("Synth step 3")
        .click_modifiers(Modifiers::COMMAND);
    harness.run_steps(2);
    assert!(harness.state().state().pattern().synth[2].accent);
}

#[test]
fn the_tap_button_sets_the_tempo_from_the_taps() {
    // The taps use the UI clock, so the expected tempo comes from the frame times of the
    // frames that processed the clicks (a harness step may run more than one pass).
    let (mut harness, mut engine) = app_harness_with_engine(0.25);
    harness.run_steps(2);
    harness.state_mut().state_mut().set_bpm(90.0);
    harness.run_steps(1);
    let mut times = Vec::new();
    harness.get_by_label(TAP_LABEL).click();
    harness.step();
    times.push(harness.ctx.input(|i| i.time));
    harness.step();
    assert_eq!(harness.state().state().bpm, 90.0, "one tap sets nothing");
    assert_eq!(harness.state().state().tap.len(), 1);
    for _ in 0..3 {
        harness.get_by_label(TAP_LABEL).click();
        harness.step();
        times.push(harness.ctx.input(|i| i.time));
        harness.step();
    }
    let mean = (times[3] - times[0]) / 3.0;
    let expected = (60.0 / mean as f32).clamp(60.0, 200.0);
    let bpm = harness.state().state().bpm;
    assert!(
        (bpm - expected).abs() < 0.6,
        "bpm {bpm} vs expected {expected} from taps {times:?}"
    );
    assert_ne!(bpm, 90.0);
    assert_eq!(harness.state().state().tap.len(), 4);
    drain(&mut harness, &mut engine);
    assert!((engine.transport().bpm() - bpm).abs() < 0.01);
}

#[test]
fn the_length_selector_doubles_the_pattern_and_brings_it_back() {
    let mut harness = app_harness();
    harness.run_steps(2);
    assert_eq!(
        radio(&harness, "1 bar").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    radio(&harness, "2 bars").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().len(), 32);
    harness.get_by_label(ui::sequencer::BAR_LABEL);
    let kick = DrumRow::Kick.index();
    assert!(
        harness.state().state().pattern().drums[kick][16].on,
        "the loop was copied into bar 2"
    );
    radio(&harness, "1 bar").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().len(), 16);
    assert!(harness.query_by_label(ui::sequencer::BAR_LABEL).is_none());
}

#[test]
fn up_down_and_page_keys_nudge_the_selected_track_note() {
    let mut harness = app_harness();
    harness.run_steps(2);
    let note = harness.state().state().pattern().synth[0].note.unwrap();
    harness.get_by_label("Synth step 1").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().selected_step, Some(0));
    harness.key_press(Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().pattern().synth[0].note,
        Some(note + 1)
    );
    harness.key_press(Key::PageUp);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().pattern().synth[0].note,
        Some(note + 13)
    );
    harness.key_press(Key::ArrowDown);
    harness.key_press(Key::PageDown);
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().synth[0].note, Some(note));
    assert_eq!(
        harness.state().state().note_on_count,
        0,
        "arrows are not piano keys"
    );
}

#[test]
fn a_double_clicked_row_name_mutes_the_channel() {
    // Two clicks a 60 Hz frame apart are a double-click to egui.
    let (mut harness, _engine) = app_harness_with_engine(1.0 / 60.0);
    harness.run_steps(2);
    let double_click = |harness: &mut Harness<'_, BeatSlopperApp>, label: &str, mods| {
        for _ in 0..2 {
            harness
                .get_all_by_label(label)
                .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
                .unwrap()
                .click_modifiers(mods);
            harness.step();
        }
        harness.run_steps(2);
    };
    double_click(&mut harness, "C.Hat", Modifiers::NONE);
    assert!(
        harness
            .state()
            .state()
            .mixer
            .strip(engine::Channel::ClosedHat)
            .mute
    );
    assert!(
        !harness.state().state().dirty || harness.state().state().undo.len() == 1,
        "a mute is one mixer edit"
    );
    assert_eq!(
        harness
            .get_by_label(&tracks::led_name("C.Hat"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False),
        "the row LED is off"
    );
    // Wait out the double-click window before the next pair.
    harness.run_steps(30);
    double_click(&mut harness, "C.Hat", Modifiers::NONE);
    assert!(
        !harness
            .state()
            .state()
            .mixer
            .strip(engine::Channel::ClosedHat)
            .mute
    );
    harness.run_steps(30);
    // A track row: Shift + double-click solos it.
    double_click(
        &mut harness,
        &tracks::row_name(0, "Synth"),
        Modifiers::SHIFT,
    );
    assert!(
        harness
            .state()
            .state()
            .mixer
            .strip(engine::Channel::Synth)
            .solo
    );
    // The LED: a click mutes, a right-click solos.
    harness.get_by_label(&tracks::led_name("Kick")).click();
    harness.run_steps(2);
    assert!(
        harness
            .state()
            .state()
            .mixer
            .strip(engine::Channel::Kick)
            .mute
    );
    harness
        .get_by_label(&tracks::led_name("Snare"))
        .click_secondary();
    harness.run_steps(2);
    assert!(
        harness
            .state()
            .state()
            .mixer
            .strip(engine::Channel::Snare)
            .solo
    );
}

#[test]
fn escape_once_deselects_and_twice_silences_everything() {
    let mut harness = app_harness();
    harness.run_steps(2);
    // The test tone lives in the project menu.
    harness.get_by_label(ui::transport_bar::MENU_NAME).click();
    harness.run_steps(2);
    harness
        .get_by_label(ui::transport_bar::MENU_TEST_TONE)
        .click();
    harness.run_steps(2);
    harness.get_by_label("Synth step 2").click();
    harness.run_steps(2);
    assert!(harness.state().state().test_tone);
    harness.key_down(Key::A);
    harness.run_steps(2);
    assert!(harness.state().state().held_notes[60]);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(
        !harness.state().state().test_tone,
        "Escape stops the test tone"
    );
    assert_eq!(harness.state().state().selected_step, None);
    assert!(
        harness.state().state().held_notes[60],
        "one Escape keeps the note"
    );
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(
        !harness.state().state().held_notes[60],
        "double-tap Escape: all notes off"
    );
    assert_eq!(
        harness.state().state().status_text(),
        Some("Panic: all notes off")
    );
}

#[test]
fn the_recent_section_lists_saved_projects_and_ctrl_shift_o_cycles_them() {
    let mut harness = app_harness();
    harness.run_steps(2);
    assert!(!harness.state().state().recent_open);
    harness.get_by_label(ui::project_bar::RECENT_LABEL).click();
    harness.run_steps(2);
    assert!(harness.state().state().recent_open);
    harness.get_by_label(ui::recent_panel::EMPTY_LABEL);

    let a = temp_project_path("recent-a.beatslopper.json");
    let b = temp_project_path("recent-b.beatslopper.json");
    harness.state_mut().state_mut().save_project_to(a.clone());
    harness.state_mut().state_mut().save_project_to(b.clone());
    harness.run_steps(2);
    harness.get_by_label(ui::recent_panel::entry_name(0, &b).as_str());
    harness.get_by_label(ui::recent_panel::entry_name(1, &a).as_str());

    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::O);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().project_path.as_deref(),
        Some(a.as_path()),
        "Ctrl+Shift+O opened the other entry"
    );
    // Opening a project shows the roll view; the section stays open and the open project
    // is its selected row. A click on an entry opens it too, and dirty asks first.
    assert_eq!(harness.state().state().view, View::Roll);
    assert_eq!(
        harness
            .get_by_label(ui::recent_panel::entry_name(0, &a).as_str())
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    harness.get_by_label("Kick step 2").click();
    harness.run_steps(2);
    harness
        .get_by_label(ui::recent_panel::entry_name(1, &b).as_str())
        .click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().pending_action,
        Some(app::app::PendingAction::OpenRecent)
    );
    harness.get_by_label(ui::project_bar::DISCARD_QUESTION);
    harness.get_by_label("Discard").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().project_path.as_deref(),
        Some(b.as_path())
    );
    assert!(!harness.state().state().dirty);
    harness
        .get_by_label(ui::recent_panel::CLEAR_LIST_LABEL)
        .click();
    harness.run_steps(2);
    harness.get_by_label(ui::recent_panel::EMPTY_LABEL);
    // The section head collapses the list again; the project menu reopens it.
    harness.get_by_label(ui::project_bar::RECENT_LABEL).click();
    harness.run_steps(2);
    assert!(!harness.state().state().recent_open);
    assert!(
        harness
            .query_by_label(ui::recent_panel::EMPTY_LABEL)
            .is_none()
    );
    harness.get_by_label(ui::transport_bar::MENU_NAME).click();
    harness.run_steps(2);
    harness.get_by_label(ui::transport_bar::MENU_RECENT).click();
    harness.run_steps(2);
    assert!(harness.state().state().recent_open);
    harness.get_by_label(ui::recent_panel::EMPTY_LABEL);
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

// -------------------------------------------------------------------------------------------
// M10: instrument tracks in the panels.

/// The SYNTH panel's head names the edit track ("Synth · <name>") with its colour chip; the
/// browser's PRESETS section applies to the edit track only and its MODIFIED LED follows.
#[test]
fn synth_head_names_the_edit_track_and_the_browser_follows_add_and_select() {
    let mut harness = wide_harness();
    harness.key_press(Key::F7);
    harness.run_steps(3);
    harness.get_by_label(&ui::synth_panel::head_title(harness.state().state()));
    harness.get_by_label("Synth · Synth");
    // The chip: the synth head, the toolbar strip and the FX rack's four per-track groups.
    assert_eq!(
        harness
            .get_all_by_label(&ui::controls::track_chip_name("Synth"))
            .count(),
        6
    );
    harness.get_by_label(ui::synth_panel::MODIFIED_LABEL);
    assert!(harness.query_by_label("Synth · Track 2").is_none());

    harness.state_mut().state_mut().add_track();
    harness.run_steps(2);
    harness.get_by_label("Synth · Track 2");
    assert_eq!(harness.get_all_by_label("Track 2 colour").count(), 6);
    assert!(harness.query_by_label("Synth colour").is_none());
    assert!(harness.query_by_label("Synth · Synth").is_none());
    assert_eq!(
        harness
            .get_by_label(ui::synth_panel::MODIFIED_LABEL)
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False),
        "a fresh track is not modified"
    );

    // The PRESETS section applies to the edit track (track 2); track 1 keeps its preset: the
    // "Keys" entry (M9), then `<` steps back to "Pad".
    harness.get_by_label(&ui::preset_browser::head_name("Bass"));
    harness
        .get_by_label(ui::preset_browser::BROWSE_LABEL)
        .click();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Keys")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().track(1).preset, 5);
    assert_eq!(harness.state().state().track(0).preset, 0);
    assert_eq!(harness.state().state().preset_index(), 5);
    harness.get_by_label(&ui::preset_browser::head_name("Keys"));
    harness
        .get_by_label(ui::preset_browser::CLOSE_LABEL)
        .click();
    harness.run_steps(2);
    harness.get_by_label(ui::preset_browser::PREV_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().preset_index(), 4, "Pad");
    harness.get_by_label(&ui::preset_browser::head_name("Pad"));
    assert_eq!(harness.state().state().track(0).preset, 0);

    // A knob move lights the LEDs; selecting track 1 shows its own state again.
    harness
        .state_mut()
        .state_mut()
        .set_param(engine::ParamId::Glide, 0.4);
    harness.run_steps(2);
    for label in [
        ui::synth_panel::MODIFIED_LABEL,
        ui::synth_panel::TOOLBAR_MODIFIED_LABEL,
    ] {
        assert_eq!(
            harness.get_by_label(label).accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
    }
    harness.state_mut().state_mut().select_track(0);
    harness.run_steps(2);
    harness.get_by_label("Synth · Synth");
    assert_eq!(harness.state().state().preset_index(), 0);
    assert_eq!(
        harness
            .get_by_label(ui::synth_panel::MODIFIED_LABEL)
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    assert_eq!(
        harness.state().state().param(engine::ParamId::Glide),
        engine::ParamId::Glide.default_value()
    );
}

/// The FX rack's per-track groups carry the edit track's chip and name; the shared buses
/// do not. Rendering still edits nothing.
#[test]
fn fx_rack_labels_the_insert_groups_with_the_edit_track() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (mut state, _engine) = AppState::headless(mode);
        state.rename_track(0, "Lead");
        state.undo.clear();
        state.dirty = false;
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(ui::fx_rack::WIDTH + 40.0, 900.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::fx_rack::show(ui, state, &theme),
                state,
            );
        harness.run_steps(4);
        // DRIVE, LO-FI, CHORUS and SIDECHAIN: one chip each.
        assert_eq!(harness.get_all_by_label("Lead colour").count(), 4);
        for group in [
            "Drive",
            "Lo-Fi",
            "Chorus",
            "Sidechain",
            "Delay",
            "Reverb",
            "Master",
        ] {
            harness.get_by_label(group);
        }
        let state = harness.into_state();
        assert_eq!(state.params, default_params());
        assert!(!state.dirty);
    }
}

/// The MIXER shows the five drum strips, one strip per active track (named after the track,
/// with its colour chip) and the master; an unused slot has no strip, and more strips than
/// fit the panel scroll instead of overlapping.
#[test]
fn mixer_shows_one_strip_per_active_track() {
    let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
    state.rename_track(0, "Lead");
    state.add_track();
    state.rename_track(1, "Pad");
    state.add_track();
    let theme = Theme::for_mode(ThemeMode::Dark);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(ui::mixer::WIDTH + 40.0, 420.0))
        .build_ui_state(
            move |ui, state: &mut AppState| ui::mixer::show(ui, state, &theme),
            state,
        );
    harness.run_steps(4);
    for name in ["Lead", "Pad", "Track 3"] {
        harness.get_by_label(&format!("{name} level"));
        harness.get_by_label(&format!("{name} pan"));
        harness.get_by_label(&format!("Mute {name}"));
        harness.get_by_label(&format!("Solo {name}"));
        harness.get_by_label(&ui::controls::track_chip_name(name));
    }
    assert!(harness.query_by_label("Track 4 level").is_none());
    assert!(harness.query_by_label("Synth level").is_none(), "renamed");
    // M11: the meters are the sixteen inserts'; the source strips have none.
    assert_eq!(harness.get_all_by_label("Level").count(), 16);
    // Eight source strips side by side, each in its own column of the source dock.
    let kick = harness.get_by_label("Kick level").rect();
    let lead = harness.get_by_label("Lead level").rect();
    let pad = harness.get_by_label("Pad level").rect();
    let third = harness.get_by_label("Track 3 level").rect();
    assert!(kick.max.x <= lead.min.x && lead.max.x <= pad.min.x && pad.max.x <= third.min.x);
    assert_eq!(lead.min.y, kick.min.y, "the same row");
    assert!(
        third.min.x >= kick.min.x + ui::mixer::SOURCE_DOCK_WIDTH,
        "eight sources overflow the four-strip dock: it scrolls ({third:?})"
    );
    // The inserts dock starts right of the source dock, the master right of the inserts.
    let first_insert = harness.get_by_label("Insert 1 level").rect();
    let stereo = harness.get_by_label("Stereo level").rect();
    assert!(first_insert.min.x >= kick.min.x + ui::mixer::SOURCE_DOCK_WIDTH);
    assert!(stereo.min.x > first_insert.max.x);
    assert!(stereo.max.x <= ui::mixer::WIDTH + 40.0);

    // Removing a track hides its strip.
    harness.state_mut().remove_track(1);
    harness.run_steps(2);
    assert!(harness.query_by_label("Pad level").is_none());
    harness.get_by_label("Track 3 level");
    harness.state_mut().remove_track(2);
    harness.run_steps(2);
    assert!(harness.query_by_label("Track 3 level").is_none());
    assert_eq!(harness.get_all_by_label("Level").count(), 16);
}

/// The roll draws the other active tracks' notes as ghosts in their colours (both themes):
/// the ghost helper gets the demo bass line while the empty second track is selected, a
/// click on a ghost cell draws a note on the edit track, and selecting the first track
/// turns that note into the ghost; the track dropdown names the edit track.
#[test]
fn roll_shows_the_other_tracks_notes_as_ghosts_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (mut state, _engine) = AppState::headless(mode);
        state.add_track();
        state.undo.clear();
        state.dirty = false;
        let theme = Theme::for_mode(mode);
        let ghosts = ui::roll_view::roll_ghosts(&state, &theme);
        let demo = Pattern::demo();
        assert_eq!(ghosts.len(), demo.notes(0).len());
        assert!(ghosts.iter().all(|g| g.colour == theme.track_colour(0)));
        assert!(ghosts.iter().any(|g| g.start == 0 && g.pitch == 36));

        let ui_theme = theme.clone();
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1000.0, 440.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                move |ui, state: &mut AppState| ui::roll_view::show(ui, state, &ui_theme),
                state,
            );
        harness.run_steps(3);
        harness.get_by_label("Piano roll");
        assert_eq!(
            harness
                .get_by_label(ui::roll_view::TRACK_DROPDOWN)
                .value()
                .as_deref(),
            Some("Track 2")
        );
        assert!(harness.state().edit_notes().is_empty());
        assert!(!harness.state().dirty, "ghosts are not an edit");

        // A press on a ghost's cell draws a note there, on the edit track.
        let (model, grid) = roll_geometry(&harness, harness.state());
        assert!(model.is_visible(36));
        pointer_click(&mut harness, roll_cell(&model, grid, 0, 36));
        assert_eq!(harness.state().edit_notes().len(), 1);
        assert_eq!(harness.state().edit_notes()[0].pitch, 36);
        assert_eq!(
            harness.state().track_notes(0),
            demo.notes(0),
            "the ghosts' track is untouched"
        );
        assert_eq!(
            ui::roll_view::roll_ghosts(harness.state(), &theme).len(),
            demo.notes(0).len()
        );

        harness.state_mut().select_track(0);
        harness.run_steps(2);
        assert_eq!(
            harness
                .get_by_label(ui::roll_view::TRACK_DROPDOWN)
                .value()
                .as_deref(),
            Some("Synth")
        );
        let ghosts = ui::roll_view::roll_ghosts(harness.state(), &theme);
        assert_eq!(ghosts.len(), 1);
        assert_eq!(ghosts[0].colour, theme.track_colour(1));
        assert_eq!(harness.state().edit_notes(), demo.notes(0));
    }
}

/// Ctrl+T adds a track, Ctrl+Shift+D duplicates the edit track, Alt+Up / Alt+Down select
/// the previous / next track; the rack's rows and the roll's track dropdown follow, and
/// Ctrl+Z undoes the structural changes.
#[test]
fn track_shortcuts_add_duplicate_and_select_through_the_app() {
    let mut harness = app_harness();
    harness.run_steps(2);
    harness.get_by_label(&tracks::row_name(0, "Synth"));
    let dropdown = |harness: &Harness<'_, BeatSlopperApp>| {
        harness
            .get_by_label(ui::roll_view::TRACK_DROPDOWN)
            .value()
            .unwrap_or_default()
    };
    assert_eq!(dropdown(&harness), "Synth");

    harness.key_press_modifiers(Modifiers::COMMAND, Key::T);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    assert_eq!(harness.state().state().edit_track, 1);
    harness.get_by_label(&tracks::row_name(1, "Track 2"));
    assert_eq!(dropdown(&harness), "Track 2");
    // Ctrl+T is not the piano's T key.
    assert_eq!(harness.state().state().note_on_count, 0);
    assert!(harness.state().state().dirty);

    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 3);
    assert_eq!(harness.state().state().edit_track, 2);
    assert_eq!(harness.state().state().track(2).name, "Track 2 copy");
    harness.get_by_label(&tracks::row_name(2, "Track 2 copy"));
    assert_eq!(dropdown(&harness), "Track 2 copy");
    // The selected row is the edit track's.
    assert_eq!(
        harness
            .get_by_label(&tracks::row_name(2, "Track 2 copy"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );

    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 1);
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 0);
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 2, "wraps");
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 0);
    assert_eq!(dropdown(&harness), "Synth");
    // Selecting is not an undo step: Ctrl+Z undoes the duplicate, then the add.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 1);
    assert_eq!(harness.state().state().edit_track, 0);
    harness.get_by_label(&tracks::row_name(0, "Synth"));
    assert!(
        harness
            .query_by_label(&tracks::row_name(1, "Track 2"))
            .is_none()
    );
}

/// With three tracks and the MIXER view the rack keeps every row and its sixteen steps,
/// and the mixer's strips (the long name's strip included) sit inside the main window.
#[test]
fn the_rack_keeps_three_tracks_whole_with_the_mixer_shown() {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    app.state_mut().set_view(View::Mixer);
    app.state_mut().add_track();
    app.state_mut().add_track();
    app.state_mut().rename_track(2, "Sixteen chars ok");
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1920.0, 1080.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    harness.get_by_label("Mute Kick");
    harness.get_by_label("Sixteen chars ok level");
    let handle = harness.get_by_label(ui::shell::RACK_HANDLE_NAME).rect();
    let mixer_bar = harness.get_by_label(ui::shell::MIXER_BAR_NAME).rect();
    for name in ["Synth", "Track 2", "Sixteen chars ok"] {
        let last = harness.get_by_label(&format!("{name} step 16")).rect();
        assert!(
            last.max.x <= handle.min.x && last.max.y <= mixer_bar.min.y,
            "{name}: {last:?}"
        );
    }
    let row = harness
        .get_by_label(&tracks::row_name(2, "Sixteen chars ok"))
        .rect();
    assert_eq!(
        row.width(),
        ui::sequencer::NAME_WIDTH,
        "the name button keeps its width"
    );
    let synth_level = harness.get_by_label("Synth level").rect();
    assert!(
        synth_level.min.x >= handle.max.x,
        "{synth_level:?} vs {handle:?}"
    );
    assert!(synth_level.max.y <= mixer_bar.min.y);
}

/// M10 review: the MASTER column is pinned at the mixer's right edge, outside the strips'
/// scroll area: with eight tracks the strips scroll while MASTER keeps its place inside the
/// panel.
#[test]
fn mixer_master_stays_pinned_with_eight_tracks() {
    let master_at = |extra: usize| {
        let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
        for _ in 0..extra {
            state.add_track();
        }
        let theme = Theme::for_mode(ThemeMode::Dark);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(ui::mixer::WIDTH + 40.0, 420.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::mixer::show(ui, state, &theme),
                state,
            );
        harness.run_steps(4);
        let master = harness.get_by_label("Master").rect();
        let limit = harness.get_by_label("Limit").rect();
        let meter = harness.get_by_label("Stereo level").rect();
        // The harness's 8 pt margin and the Deck panel's 8 pt padding.
        let panel_right = ui::mixer::WIDTH + 40.0 - 8.0 - Theme::dark().panel_padding;
        for rect in [master, limit, meter] {
            assert!(
                rect.max.x <= panel_right + 0.5,
                "{extra} extra tracks: {rect:?} inside the panel ({panel_right})"
            );
        }
        let last = format!("{} level", if extra == 0 { "Synth" } else { "Track 8" });
        let last = harness.get_by_label(&last).rect();
        (master, last)
    };
    let (master_one, last_one) = master_at(0);
    let (master_eight, last_eight) = master_at(7);
    assert_eq!(master_one.min.x, master_eight.min.x, "MASTER does not move");
    assert!(
        last_one.max.x <= master_one.min.x,
        "one track: every strip before MASTER"
    );
    assert!(
        last_eight.min.x > master_eight.min.x,
        "eight tracks: the last source strip is scrolled out past MASTER ({last_eight:?} vs \
         {master_eight:?})"
    );
    assert!(
        ui::mixer::sources_width(DrumRow::COUNT + 8) > ui::mixer::CONTENT_WIDTH,
        "thirteen source strips are wider than the whole panel"
    );
}
