//! egui_kittest: the M0-M5 screens render in both themes without a window or an audio device,
//! the computer keyboard plays the on-screen piano, the SYNTH / FX panels leave every
//! parameter untouched until a control is used, clicking a sequencer cell toggles it, the
//! MIDI lamp / selector render without opening a port, the M4 polish works end to end (Space /
//! R / Ctrl+Z shortcuts, the "Unsaved changes" marker, the DISCARD / KEEP confirm) and the M5
//! pattern UI does too: the A..H tabs switch the grid, LENGTH shows the BAR selector, the
//! queued letter appears while a switch is pending, MODE flips the right-hand column to the
//! SONG CHAIN panel, whose ADD / REMOVE edit the chain.

use app::ui;
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
        assert_eq!(state.preset_index, 0);
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
fn main_area_renders_both_panels_side_by_side() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(mode, egui::Vec2::new(1240.0, 600.0), ui::main_area);
        assert_eq!(state.params, default_params());
    }
}

#[test]
fn transport_dial_shows_master_gain_without_changing_it() {
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

fn render_panels(mode: ThemeMode) -> AppState {
    let (state, _engine) = AppState::headless(mode);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 900.0))
        .build_ui_state(
            move |ui, state: &mut AppState| {
                ui::transport_bar::show(ui, state, &theme);
                ui.add_space(10.0);
                ui::main_area(ui, state, &theme);
                ui.add_space(10.0);
                ui::keyboard::show(ui, state, &theme);
            },
            state,
        );
    harness.run_steps(5);
    harness.into_state()
}

#[test]
fn transport_and_main_area_render_in_dark_theme() {
    let state = render_panels(ThemeMode::Dark);
    assert_eq!(state.mode, ThemeMode::Dark);
    assert!(!state.test_tone);
}

#[test]
fn transport_and_main_area_render_in_light_theme() {
    let state = render_panels(ThemeMode::Light);
    assert_eq!(state.mode, ThemeMode::Light);
}

#[test]
fn keyboard_panel_renders_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (state, _engine) = AppState::headless(mode);
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, 300.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::keyboard::show(ui, state, &theme),
                state,
            );
        harness.run_steps(3);
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
            .with_size(egui::Vec2::new(1280.0, 1080.0))
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

fn app_harness() -> Harness<'static, BeatSlopperApp> {
    app_harness_sized(egui::Vec2::new(1280.0, 1080.0))
}

/// The whole app at `size` (the design size is 1280 x 1140; see `app::window`).
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
// M3: sequencer, project panel, transport.

#[test]
fn sequencer_panel_renders_in_both_themes_without_touching_the_pattern() {
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
fn project_panel_renders_in_both_themes_without_dialogs() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(
            mode,
            egui::Vec2::new(ui::project_bar::WIDTH + 40.0, 400.0),
            ui::project_bar::show,
        );
        assert_eq!(state.project_path, None);
        assert!(!state.is_exporting());
        assert_eq!(state.status_text(), None);
        assert_eq!(state.export_repeats, 1);
    }
}

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

#[test]
fn clear_pills_clear_a_row_and_everything() {
    let mut harness = sequencer_harness();
    // Six CLEAR pills (five drum rows and the bass lane), in row order.
    let clears = harness.get_all_by_label("Clear").count();
    assert_eq!(clears, 6);
    harness.get_all_by_label("Clear").nth(3).unwrap().click();
    harness.step();
    let pattern = harness.state().pattern();
    assert!(
        pattern.drums[DrumRow::ClosedHat.index()]
            .iter()
            .all(|s| !s.on)
    );
    assert!(pattern.drums[DrumRow::Kick.index()].iter().any(|s| s.on));

    harness
        .get_by_label(ui::sequencer::CLEAR_PATTERN_LABEL)
        .click();
    harness.step();
    assert!(harness.state().pattern().is_empty());
}

#[test]
fn a_bass_cell_selects_the_step_and_the_keyboard_sets_its_note() {
    let mut harness = sequencer_app_harness();
    harness.get_by_label("Bass step 2").click();
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
    harness.get_by_label("Bass step 5").click();
    harness.step();
    assert_eq!(harness.state().state().selected_step, Some(4));
    harness.get_by_label("Bass step 5").click();
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

/// The transport bar shows the "MIDI" lamp (off) and the port selector with its "None" entry
/// in both themes; rendering connects nothing and touches no parameter.
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
        let lamp = harness.get_by_label("MIDI");
        assert!(lamp.rect().width() > 0.0);
        // The selector lists "None" first (a segmented group or a combo box, depending on
        // how many ports this machine has).
        let items = ui::transport_bar::midi_items(harness.state());
        assert_eq!(items[0], ui::transport_bar::MIDI_NONE);
        assert_eq!(items.len(), harness.state().midi_ports.len() + 1);
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
        .with_size(egui::Vec2::new(1280.0, 1080.0))
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

/// The app's per-frame logic (`tick`: keyboard, playhead) over just the SEQUENCER panel, so
/// note entry can be tested without the whole window's layout.
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

/// At the default window size (`main.rs`: 1280 x 1140) the whole sequencer, down to its
/// playhead dots, is visible without scrolling the main area.
#[test]
fn whole_sequencer_is_visible_at_the_default_window_size() {
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 1140.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    let bass = harness.get_by_label("Bass step 16").rect();
    let octave = harness.get_by_label("Octave").rect();
    // Below the bass row: the row gap, the 10 pt dot row and the panel padding (20); the KEYS
    // panel's label sits 10 (page gap) + 20 (padding) under the main area's end.
    let sequencer_bottom = bass.max.y + ui::sequencer::ROW_GAP + 10.0 + 20.0;
    let keys_panel_top = octave.min.y - 30.0;
    assert!(
        sequencer_bottom <= keys_panel_top,
        "sequencer ends at {sequencer_bottom} but the KEYS panel starts at {keys_panel_top}"
    );
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
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(harness.state().state().armed, "R arms recording");
    assert!(harness.state().state().is_recording());
    harness.key_press(Key::Space);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(!harness.state().state().playing, "Space stops it");
    assert!(harness.state().state().armed, "stopping keeps the arm");
    harness.key_press(Key::R);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
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
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(harness.state().state().pattern().drums[kick][1].on);
    assert!(harness.state().state().dirty);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(
        !harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Z undid"
    );
    assert_eq!(harness.state().state().pattern().clone(), Pattern::demo());

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(
        harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Y redid"
    );

    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(
        harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Shift+Z: nothing to redo"
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(!harness.state().state().pattern().drums[kick][1].on);
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(
        harness.state().state().pattern().drums[kick][1].on,
        "Ctrl+Shift+Z redoes"
    );
    // Ctrl+Z is not a piano key: `Z` alone would shift the octave.
    assert_eq!(harness.state().state().input.octave, 4);
    assert_eq!(harness.state().state().note_on_count, 0);
}

#[test]
fn undo_and_redo_pills_work_and_grey_out() {
    let mut harness = app_harness();
    // Nothing to undo yet: the pills are disabled.
    assert!(harness.get_by_label("Undo").accesskit_node().is_disabled());
    assert!(harness.get_by_label("Redo").accesskit_node().is_disabled());

    harness.get_by_label("Snare step 1").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(harness.state().state().pattern().drums[DrumRow::Snare.index()][0].on);
    assert!(!harness.get_by_label("Undo").accesskit_node().is_disabled());
    harness.get_by_label("Undo").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(!harness.state().state().pattern().drums[DrumRow::Snare.index()][0].on);
    assert!(!harness.get_by_label("Redo").accesskit_node().is_disabled());
    harness.get_by_label("Redo").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
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
    assert_eq!(harness.state().title(), Some("BeatSlopper"));

    harness.get_by_label("Clap step 1").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(harness.state().state().dirty);
    harness.get_by_label(ui::project_bar::UNSAVED_LABEL);
    assert_eq!(harness.state().title(), Some("BeatSlopper *"));

    // Save to a known temp file: Ctrl+S needs no dialog once there is a path.
    let path = temp_project_path("marker.beatslopper.json");
    harness.state_mut().state_mut().project_path = Some(path.clone());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::S);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(!harness.state().state().dirty);
    assert!(
        harness
            .query_by_label(ui::project_bar::UNSAVED_LABEL)
            .is_none()
    );
    assert_eq!(
        harness.state().title(),
        Some("BeatSlopper – marker.beatslopper.json")
    );
    assert!(path.is_file());
    harness.get_by_label("marker.beatslopper.json");

    // Another edit dirties again (and the title gets its star back).
    harness.get_by_label("Clap step 1").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    harness.get_by_label(ui::project_bar::UNSAVED_LABEL);
    assert_eq!(
        harness.state().title(),
        Some("BeatSlopper – marker.beatslopper.json *")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn new_while_dirty_asks_and_keep_or_escape_cancels_discard_proceeds() {
    let mut harness = app_harness();
    harness.get_by_label("Kick step 2").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    let edited = harness.state().state().pattern().clone();

    harness.get_by_label("New").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
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
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
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
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert_eq!(
        harness.state().state().pending_action,
        Some(app::app::PendingAction::New)
    );
    harness.key_press(Key::Escape);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert_eq!(harness.state().state().pending_action, None);
    assert_eq!(harness.state().state().pattern().clone(), edited);

    // DISCARD creates the new project; undo brings the edit back.
    harness.get_by_label("New").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    harness.get_by_label("Discard").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert_eq!(harness.state().state().pattern().clone(), Pattern::empty());
    assert!(!harness.state().state().dirty);
    assert!(
        harness
            .query_by_label(ui::project_bar::DISCARD_QUESTION)
            .is_none()
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert_eq!(harness.state().state().pattern().clone(), edited);
}

/// Open while dirty asks as well; headless there is no dialog, so DISCARD just clears the
/// pending action.
#[test]
fn open_while_dirty_asks_and_never_opens_a_dialog_headless() {
    let mut harness = app_harness();
    harness.get_by_label("Kick step 2").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    harness.get_by_label("Open").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert_eq!(
        harness.state().state().pending_action,
        Some(app::app::PendingAction::Open)
    );
    harness.get_by_label("Discard").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert_eq!(harness.state().state().pending_action, None);
    assert!(harness.state().state().dirty, "no file was opened");
}

// -------------------------------------------------------------------------------------------
// M5: pattern tabs, length / bar selector, queued letter, MODE, SONG CHAIN panel.

/// A `RadioButton` (segmented item) named `label`, skipping plain labels with the same text
/// (the beat numbers are "1".."4" too).
fn radio<'h, S>(harness: &'h Harness<'_, S>, label: &'h str) -> egui_kittest::Node<'h> {
    harness
        .get_all_by_label(label)
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::RadioButton)
        .unwrap_or_else(|| panic!("no segmented item {label:?}"))
}

#[test]
fn pattern_tabs_switch_the_grid_and_edits_land_in_the_selected_pattern() {
    let mut harness = sequencer_harness();
    harness.run_steps(2);
    for tab in ui::sequencer::PATTERN_TABS {
        radio(&harness, tab);
    }
    assert_eq!(harness.state().current(), 0);
    // The demo kick plays on step 1: the grid shows A.
    assert!(harness.state().pattern().drums[DrumRow::Kick.index()][0].on);

    radio(&harness, "B").click();
    harness.step();
    assert_eq!(harness.state().current(), 1);
    assert!(harness.state().pattern().is_empty(), "B is empty");
    assert!(!harness.state().dirty, "selecting a tab is not an edit");

    harness.get_by_label("Kick step 1").click();
    harness.step();
    let state = harness.state();
    assert!(state.bank.patterns[1].drums[DrumRow::Kick.index()][0].on);
    assert_eq!(state.bank.patterns[0], Pattern::demo(), "A is unchanged");
    assert!(state.dirty);

    radio(&harness, "A").click();
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
    radio(&harness, "2 bars").click();
    harness.run_steps(2);
    assert_eq!(harness.state().pattern().len(), 32);
    harness.get_by_label(ui::sequencer::BAR_LABEL);
    radio(&harness, "1");
    radio(&harness, "2").click();
    harness.run_steps(2);
    assert_eq!(harness.state().shown_bar, 1);
    assert!(!harness.state().follow, "picking a bar switches FOLLOW off");
    // Bar 2: beat numbers 5..8 and absolute cell names.
    harness.get_by_label("5");
    harness.get_by_label("8");
    harness.get_by_label("Kick step 17").click();
    harness.run_steps(2);
    let pattern = harness.state().pattern();
    assert!(pattern.drums[DrumRow::Kick.index()][16].on);
    assert_eq!(
        pattern.drums[DrumRow::Kick.index()][0],
        Pattern::demo().drums[0][0]
    );
    assert!(harness.query_by_label("Kick step 1").is_none());

    harness.get_by_label("Follow").click();
    harness.run_steps(2);
    assert!(harness.state().follow);

    radio(&harness, "1 bar").click();
    harness.run_steps(2);
    assert_eq!(harness.state().pattern().len(), 16);
    assert_eq!(harness.state().shown_bar, 0, "clamped to the new length");
    assert!(harness.query_by_label(ui::sequencer::BAR_LABEL).is_none());
    harness.get_by_label("Kick step 1");
}

#[test]
fn copy_paste_and_clear_pills_work_from_the_head() {
    let mut harness = sequencer_harness();
    harness.run_steps(2);
    assert!(
        harness.get_by_label("Paste").accesskit_node().is_disabled(),
        "nothing to paste yet"
    );
    harness.get_by_label("Copy").click();
    harness.run_steps(2);
    assert_eq!(harness.state().clipboard.as_ref(), Some(&Pattern::demo()));
    radio(&harness, "C").click();
    harness.run_steps(2);
    harness.get_by_label("Paste").click();
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

/// While playing in pattern mode, clicking another tab queues it: the engine reports the
/// queue once it renders, and the blinking letter ("Queued B") shows beside the tabs until the
/// switch happens at the bar boundary.
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
    radio(&harness, "B").click();
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
fn mode_switch_shows_the_song_chain_panel_and_add_remove_edit_the_chain() {
    let mut harness = app_harness_sized(egui::Vec2::new(1280.0, 1140.0));
    harness.run_steps(2);
    radio(&harness, "Project");
    assert!(harness.query_by_label("Song chain").is_none());
    harness.get_by_label("A · bar 1/1");
    harness.get_by_label(ui::project_bar::EXPORT_LOOP_LABEL);

    radio(&harness, "Song").click();
    harness.run_steps(2);
    let state = harness.state().state();
    assert_eq!(state.bank.mode, engine::PlayMode::Song);
    assert_eq!(state.right_tab, app::app::RightTab::Chain);
    assert!(!state.dirty, "the mode is not a project edit");
    harness.get_by_label("Song chain");
    harness.get_by_label("SONG 1/1 · A");
    harness.get_by_label("Entry 1: A x1");

    // ADD appends the edit pattern; the new entry is selected and REMOVE takes it away.
    harness.get_by_label("Add").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().chain().len(), 2);
    assert_eq!(harness.state().state().song_selected, Some(1));
    harness.get_by_label("Entry 2: A x1");
    harness.get_by_label("SONG 1/2 · A");
    // The pattern selector changes the selected entry (the sequencer tabs are "A".."H" too,
    // so pick the right column's item by position).
    harness
        .get_all_by_label("B")
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::RadioButton)
        .max_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x))
        .unwrap()
        .click();
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

    // The tab goes back to PROJECT without leaving song mode; EXPORT reads EXPORT SONG.
    radio(&harness, "Project").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().right_tab,
        app::app::RightTab::Project
    );
    assert_eq!(harness.state().state().bank.mode, engine::PlayMode::Song);
    harness.get_by_label(ui::project_bar::EXPORT_SONG_LABEL);
    assert!(harness.query_by_label("Bars").is_none());
    // The status line (what an export reports) fits under the export row, inside the panel.
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
    let octave = harness.get_by_label("Octave").rect();
    assert!(status.min.y >= export.max.y, "{status:?} under {export:?}");
    assert!(
        status.max.y + 20.0 <= octave.min.y - 30.0,
        "status {status:?} is inside the panel above the KEYS panel at {octave:?}"
    );

    radio(&harness, "Pattern").click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().bank.mode, engine::PlayMode::Pattern);
    harness.get_by_label(ui::project_bar::EXPORT_LOOP_LABEL);
    harness.get_by_label("A · bar 1/1");
}

/// The SONG CHAIN panel renders on its own in both themes without touching the chain.
#[test]
fn song_panel_renders_in_both_themes_without_editing() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let state = render_one(
            mode,
            egui::Vec2::new(ui::RIGHT_COLUMN_WIDTH + 40.0, 400.0),
            ui::song::show,
        );
        assert_eq!(state.chain(), &[engine::ChainEntry::new(0, 1)]);
        assert_eq!(state.song_selected, Some(0));
        assert!(!state.dirty);
    }
}
