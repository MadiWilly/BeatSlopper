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

/// Right-click clears a cell: a drum step goes off (accent too), a bass step becomes a rest
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

    // A selected bass step with a note: the note goes, so does the selection.
    harness.get_by_label("Bass step 5").click();
    harness.step();
    assert_eq!(harness.state().selected_step, Some(4));
    harness.get_by_label("Bass step 5").click_secondary();
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
    assert_eq!(
        harness.state().title(),
        Some("BeatSlopper · 120 BPM · A"),
        "the title carries the tempo and the edit pattern"
    );

    harness.get_by_label("Clap step 1").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    assert!(harness.state().state().dirty);
    harness.get_by_label(ui::project_bar::UNSAVED_LABEL);
    assert_eq!(harness.state().title(), Some("BeatSlopper * · 120 BPM · A"));

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
        Some("BeatSlopper – marker.beatslopper.json · 120 BPM · A")
    );
    assert!(path.is_file());
    harness.get_by_label("marker.beatslopper.json");

    // Another edit dirties again (and the title gets its star back).
    harness.get_by_label("Clap step 1").click();
    harness.run_steps(2); // the frame that handles the input, then the one that redraws
    harness.get_by_label(ui::project_bar::UNSAVED_LABEL);
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
    harness.get_by_label("1 bar");
    harness.get_by_label(ui::sequencer::DOUBLE_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().pattern().len(), 32);
    harness.get_by_label("2 bars");
    harness.get_by_label(ui::sequencer::BAR_LABEL);
    radio(&harness, "1");
    radio(&harness, "2").click();
    harness.run_steps(2);
    assert_eq!(harness.state().shown_bar, 1);
    assert!(!harness.state().follow, "picking a bar switches FOLLOW off");
    // Bar 2: beat numbers 5..8 and absolute cell names (x2 copied the loop into bar 2, so
    // step 17 is already on like step 1; step 18 is off like step 2).
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

    harness.get_by_label("Follow").click();
    harness.run_steps(2);
    assert!(harness.state().follow);

    harness.get_by_label(ui::sequencer::HALVE_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().pattern().len(), 16);
    harness.get_by_label("1 bar");
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
    radio(&harness, ui::controls::RIGHT_TABS[0]);
    // The SONG CHAIN panel (its REPEATS stepper) is not showing in pattern mode.
    assert!(harness.query_by_label("Repeats").is_none());
    harness.get_by_label("A · bar 1/1");
    harness.get_by_label(ui::project_bar::EXPORT_LOOP_LABEL);

    radio(&harness, "Song").click();
    harness.run_steps(2);
    let state = harness.state().state();
    assert_eq!(state.bank.mode, engine::PlayMode::Song);
    assert_eq!(state.right_tab, app::app::RightTab::Chain);
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
    radio(&harness, ui::controls::RIGHT_TABS[0]).click();
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
            harness.get_by_label(&format!("{name} delay send"));
            harness.get_by_label(&format!("{name} reverb send"));
            harness.get_by_label(&format!("{name} level"));
            harness.get_by_label(&format!("Mute {name}"));
            harness.get_by_label(&format!("Solo {name}"));
        }
        harness.get_by_label("Master");
        harness.get_by_label("Limit");
        for legend in ui::mixer::KNOB_LEGENDS {
            harness.get_by_label(legend);
        }
        // Six channel meters and the master's stereo meter.
        assert_eq!(harness.get_all_by_label("Level").count(), 6);
        harness.get_by_label("Stereo level");
        let state = harness.into_state();
        assert_eq!(state.mode, mode);
        assert_eq!(state.mixer, engine::MixerState::default());
        assert_eq!(state.params, default_params());
        assert!(!state.dirty);
    }
}

/// The `FX | Mixer` tab in the FX rack's head swaps the slot for the MIXER panel; M / S
/// clicks set the strip's mute / solo (discrete, undoable) and the LIMIT toggle is the
/// limiter parameter.
#[test]
fn mixer_tab_shows_the_panel_and_mute_solo_clicks_set_the_strip() {
    let mut harness = app_harness_sized(egui::Vec2::new(1280.0, 1140.0));
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().right_panel,
        app::app::RightPanel::Fx
    );
    harness.get_by_label("Drive");
    assert!(harness.query_by_label("Mute Kick").is_none());

    radio(&harness, "Mixer").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().right_panel,
        app::app::RightPanel::Mixer
    );
    assert!(
        harness.query_by_label("Drive").is_none(),
        "the rack is gone"
    );
    assert!(
        !harness.state().state().dirty,
        "switching tabs is not an edit"
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

    // Back to the FX rack from the mixer's head.
    radio(&harness, "FX").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().right_panel,
        app::app::RightPanel::Fx
    );
    harness.get_by_label("Drive");
    assert!(harness.query_by_label("Mute Kick").is_none());
}

/// With the MIXER tab active the top row keeps its height: at the default window size the
/// whole sequencer is still visible without scrolling (the FX-rack variant is
/// `whole_sequencer_is_visible_at_the_default_window_size`).
#[test]
fn whole_sequencer_is_visible_with_the_mixer_shown() {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    app.state_mut().right_panel = app::app::RightPanel::Mixer;
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
    harness.get_by_label("Mute Kick");
    let bass = harness.get_by_label("Bass step 16").rect();
    let octave = harness.get_by_label("Octave").rect();
    let sequencer_bottom = bass.max.y + ui::sequencer::ROW_GAP + 10.0 + 20.0;
    let keys_panel_top = octave.min.y - 30.0;
    assert!(
        sequencer_bottom <= keys_panel_top,
        "sequencer ends at {sequencer_bottom} but the KEYS panel starts at {keys_panel_top}"
    );
    // The mixer's strips end inside the top row: the master meter and the synth fader sit
    // above the sequencer panel.
    let seq = harness.get_by_label("Sequencer").rect();
    let synth_level = harness.get_by_label("Synth level").rect();
    let stereo = harness.get_by_label("Stereo level").rect();
    assert!(
        synth_level.max.y + 30.0 <= seq.min.y,
        "{synth_level:?} vs {seq:?}"
    );
    assert!(stereo.max.y + 20.0 <= seq.min.y, "{stereo:?} vs {seq:?}");
    // Six strips side by side: the kick fader is left of the synth fader, both in the slot.
    let kick_level = harness.get_by_label("Kick level").rect();
    assert!(kick_level.max.x < synth_level.min.x);
    assert!(synth_level.max.x < stereo.min.x);
}

// -------------------------------------------------------------------------------------------
// M7: the piano roll.

use egui::{PointerButton, Pos2};
use engine::Note;
use neu::{PianoRollModel, RollNote};

/// The roll's model and grid rect as rendered, rebuilt from the "Piano roll" node's rect the
/// way `ui::sequencer::roll_model` documents.
fn roll_geometry<S>(harness: &Harness<'_, S>, state: &AppState) -> (PianoRollModel, egui::Rect) {
    let outer = harness.get_by_label("Piano roll").rect();
    let model = ui::sequencer::roll_model(state, outer.size());
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
}

/// The SEQUENCER panel alone, in ROLL view over an empty bass part, with a 60 Hz step so two
/// clicks in a row are a double-click.
fn roll_harness(mode: ThemeMode) -> Harness<'static, AppState> {
    roll_harness_sized(mode, egui::Vec2::new(1000.0, 440.0))
}

/// [`roll_harness`] at another window size.
fn roll_harness_sized(mode: ThemeMode, size: egui::Vec2) -> Harness<'static, AppState> {
    let (mut state, _engine) = AppState::headless(mode);
    state.clear_synth();
    state.undo.clear();
    state.dirty = false;
    state.set_lane_view(app::app::LaneView::Roll);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            move |ui, state: &mut AppState| ui::sequencer::show(ui, state, &theme),
            state,
        );
    harness.run_steps(3);
    harness
}

/// The ROLL tab replaces the grid with the piano roll in both themes without editing anything;
/// the GRID tab brings the rows back.
#[test]
fn roll_tab_shows_the_piano_roll_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (state, _engine) = AppState::headless(mode);
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1000.0, 440.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::sequencer::show(ui, state, &theme),
                state,
            );
        harness.run_steps(3);
        harness.get_by_label("Bass step 16");
        assert!(harness.query_by_label("Piano roll").is_none());

        radio(&harness, "Roll").click();
        harness.run_steps(3);
        assert_eq!(harness.state().lane_view, app::app::LaneView::Roll);
        let roll = harness.get_by_label("Piano roll").rect();
        assert!(
            harness.query_by_label("Bass step 16").is_none(),
            "the grid rows are gone"
        );
        assert!(harness.query_by_label("Kick step 1").is_none());
        // Sixteen columns fitted to the panel, rows fitted to the six-row area.
        let (model, _grid) = roll_geometry(&harness, harness.state());
        assert_eq!(model.steps, 16);
        assert_eq!(model.step_width, ui::sequencer::MAX_ROLL_STEP_WIDTH);
        assert_eq!(model.row_height, ui::sequencer::ROLL_ROW_HEIGHT);
        assert!(
            model.pitches >= ui::sequencer::MIN_ROLL_PITCHES,
            "{}",
            model.pitches
        );
        assert!(roll.height() <= ui::sequencer::ROLL_AREA_HEIGHT);
        assert_eq!(model.low_pitch, app::app::DEFAULT_ROLL_LOW_PITCH);
        // The demo's bass line is on the roll: nothing was edited by rendering.
        assert_eq!(harness.state().pattern().clone(), Pattern::demo());
        assert!(!harness.state().dirty);
        assert_eq!(harness.state().undo.len(), 0);

        radio(&harness, "Grid").click();
        harness.run_steps(3);
        assert_eq!(harness.state().lane_view, app::app::LaneView::Grid);
        harness.get_by_label("Bass step 16");
        assert!(harness.query_by_label("Piano roll").is_none());
        assert_eq!(harness.state().pattern().clone(), Pattern::demo());
    }
}

/// The CHORDS column arms a chord: the next click on the roll places every tone of it on the
/// clicked root as one undo step; clicking the lit pill disarms it and a click is a plain
/// note again.
#[test]
fn chord_pills_arm_a_chord_that_a_roll_click_places() {
    let mut harness = roll_harness(ThemeMode::Dark);
    harness.get_by_label(ui::sequencer::CHORDS_TITLE);
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

/// Dragging the handle under the roll down makes it taller row by row (view state, not an
/// edit); dragging back up shrinks it, never below the default height.
#[test]
fn roll_handle_drags_the_roll_taller_and_shorter() {
    let mut harness = roll_harness(ThemeMode::Dark);
    let before = harness.get_by_label("Piano roll").rect();
    let handle = harness
        .get_by_label(ui::sequencer::ROLL_HANDLE_LABEL)
        .rect();
    assert!(
        handle.min.y >= before.max.y,
        "the handle sits under the roll"
    );
    assert_eq!(handle.height(), ui::sequencer::ROLL_HANDLE_HEIGHT);

    let from = handle.center();
    let to = from + egui::vec2(0.0, 3.0 * ui::sequencer::ROLL_ROW_HEIGHT);
    pointer_drag(&mut harness, from, to);
    harness.step();
    assert_eq!(harness.state().roll_extra_rows, 3);
    let after = harness.get_by_label("Piano roll").rect();
    assert_eq!(
        after.height(),
        before.height() + 3.0 * ui::sequencer::ROLL_ROW_HEIGHT
    );
    assert!(!harness.state().dirty);
    assert_eq!(harness.state().undo.len(), 0);

    let handle = harness
        .get_by_label(ui::sequencer::ROLL_HANDLE_LABEL)
        .rect();
    let from = handle.center();
    let to = from - egui::vec2(0.0, 10.0 * ui::sequencer::ROLL_ROW_HEIGHT);
    pointer_drag(&mut harness, from, to);
    harness.step();
    assert_eq!(
        harness.state().roll_extra_rows,
        0,
        "never below the default"
    );
    assert_eq!(
        harness.get_by_label("Piano roll").rect().height(),
        before.height()
    );
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
    harness.event(Event::ModifiersChanged(Modifiers::default()));
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

/// The roll over three one-step notes (E2 at 3, G#2 at 5, B2 at 7), nothing to undo.
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

/// The BLACK KEYS pill hides the black-key rows: every row is a white key and notes on black
/// keys have no row until it is turned back on. Persisted view state, not an edit.
#[test]
fn black_keys_pill_hides_the_black_rows() {
    let mut harness = roll_harness(ThemeMode::Dark);
    harness.state_mut().add_note(Note::new(0, 1, 37, 0.8));
    harness.state_mut().undo.clear();
    harness.run_steps(2);
    let (model, _grid) = roll_geometry(&harness, harness.state());
    assert!(model.black_keys && model.is_visible(37));

    harness
        .get_by_label(ui::sequencer::BLACK_KEYS_LABEL)
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
        .get_by_label(ui::sequencer::BLACK_KEYS_LABEL)
        .click();
    harness.run_steps(2);
    assert!(harness.state().roll_black_keys);
    assert!(roll_geometry(&harness, harness.state()).0.is_visible(37));
    // The pill is only there in ROLL view.
    radio(&harness, "Grid").click();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label(ui::sequencer::BLACK_KEYS_LABEL)
            .is_none()
    );
}

/// Dragging the handle right of the roll makes it wider (the step columns zoom), no further
/// than the panel has room for; dragging it back narrows it down to the fitted width.
#[test]
fn width_handle_drags_the_roll_wider_up_to_the_panel() {
    // A wide window: a 1000 pt one leaves the fitted roll only a few points of slack.
    let mut harness = roll_harness_sized(ThemeMode::Dark, egui::Vec2::new(1300.0, 440.0));
    let before = harness.get_by_label("Piano roll").rect();
    let (model, _grid) = roll_geometry(&harness, harness.state());
    assert_eq!(model.step_width, ui::sequencer::MAX_ROLL_STEP_WIDTH);
    let handle = harness
        .get_by_label(ui::sequencer::ROLL_WIDTH_HANDLE_LABEL)
        .rect();
    assert!(
        handle.min.x >= before.max.x,
        "the handle sits right of the roll"
    );
    assert_eq!(handle.width(), ui::sequencer::ROLL_WIDTH_HANDLE);

    pointer_drag(
        &mut harness,
        handle.center(),
        handle.center() + egui::vec2(64.0, 0.0),
    );
    harness.step();
    assert_eq!(harness.state().roll_extra_width, 64.0);
    let (model, _grid) = roll_geometry(&harness, harness.state());
    assert_eq!(model.step_width, ui::sequencer::MAX_ROLL_STEP_WIDTH + 4.0);
    let after = harness.get_by_label("Piano roll").rect();
    assert_eq!(after.width(), before.width() + 64.0);
    assert!(!harness.state().dirty);

    // Far right: as wide as the panel allows, the CHORDS column still in view.
    let handle = harness
        .get_by_label(ui::sequencer::ROLL_WIDTH_HANDLE_LABEL)
        .rect();
    pointer_drag(
        &mut harness,
        handle.center(),
        handle.center() + egui::vec2(2000.0, 0.0),
    );
    harness.step();
    let widest = harness.get_by_label("Piano roll").rect();
    assert!(widest.width() > after.width());
    let chords = harness.get_by_label("Major").rect();
    assert!(
        chords.max.x <= 1300.0 && chords.min.x > widest.max.x,
        "chords {chords:?} roll {widest:?}"
    );

    // Far left: back to the fitted width, never narrower.
    let handle = harness
        .get_by_label(ui::sequencer::ROLL_WIDTH_HANDLE_LABEL)
        .rect();
    pointer_drag(
        &mut harness,
        handle.center(),
        handle.center() - egui::vec2(3000.0, 0.0),
    );
    harness.step();
    assert_eq!(harness.state().roll_extra_width, 0.0);
    assert_eq!(
        harness.get_by_label("Piano roll").rect().width(),
        before.width()
    );
    assert_eq!(harness.state().undo.len(), 0);
}

/// POP OUT moves the roll into its own window (an embedded window headless): the panel
/// shows a placeholder, the roll in the window still edits the pattern, and DOCK (or POP
/// OUT again) brings it back.
#[test]
fn pop_out_pill_moves_the_roll_to_its_own_window_and_dock_returns_it() {
    let mut harness = roll_harness(ThemeMode::Dark);
    harness.get_by_label(ui::sequencer::POP_OUT_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().roll_popped);
    harness.get_by_label(ui::sequencer::POPPED_LABEL);
    harness.get_by_label(ui::sequencer::ROLL_WINDOW_TITLE);
    assert!(
        harness
            .query_by_label(ui::sequencer::ROLL_HANDLE_LABEL)
            .is_none(),
        "no resize handles in the window"
    );

    // The roll in the window still draws notes.
    let (model, grid) = roll_geometry(&harness, harness.state());
    assert!(model.is_visible(40), "{}", model.pitches);
    pointer_click(&mut harness, roll_cell(&model, grid, 1, 40));
    assert_eq!(
        harness.state().pattern().notes(0),
        &[Note::new(1, 1, 40, 0.8)]
    );

    harness.get_by_label(ui::sequencer::DOCK_LABEL).click();
    harness.run_steps(3);
    assert!(!harness.state().roll_popped);
    assert!(
        harness
            .query_by_label(ui::sequencer::POPPED_LABEL)
            .is_none()
    );
    assert!(harness.query_by_label(ui::sequencer::DOCK_LABEL).is_none());
    harness.get_by_label("Piano roll");
    harness.get_by_label(ui::sequencer::ROLL_HANDLE_LABEL);

    // POP OUT again, then the lit pill docks it too (the embedded window covers the head in
    // the headless harness, so that click goes through accessibility).
    harness.get_by_label(ui::sequencer::POP_OUT_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().roll_popped);
    harness
        .get_by_label(ui::sequencer::POP_OUT_LABEL)
        .click_accesskit();
    harness.run_steps(3);
    assert!(!harness.state().roll_popped);
}

/// Keys held on the computer keyboard reach the roll's model (its key column lights them).
#[test]
fn held_keys_reach_the_roll_model() {
    let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
    state.set_lane_view(app::app::LaneView::Roll);
    let size = egui::vec2(800.0, ui::sequencer::roll_height(&state));
    state.note_on(60, 0.8);
    let model = ui::sequencer::roll_model(&state, size);
    assert!(model.held[60]);
    assert_eq!(model.held.iter().filter(|h| **h).count(), 1);
    state.note_off(60);
    assert!(!ui::sequencer::roll_model(&state, size).held[60]);
}

/// A drag on the roll draws a note into the edit pattern (undoable, dirty), a click adds a
/// one-step note, a double-click removes a note, and back in GRID view the lane shows the
/// remaining note in its bass cell.
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

    // Back on the grid the bass cell of step 3 carries the note.
    radio(&harness, "Grid").click();
    harness.run_steps(3);
    assert!(harness.query_by_label("Piano roll").is_none());
    // `WidgetInfo::selected` reaches accesskit as the node's toggled state.
    let bass = harness.get_by_label("Bass step 3");
    assert_eq!(
        bass.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "the cell is on"
    );
    assert_eq!(
        harness
            .get_by_label("Bass step 4")
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    assert_eq!(harness.state().pattern().synth[2].note, Some(48));
    // Ctrl+Z is the app's job (tested in the whole-app flow below); here the stack is intact.
    assert_eq!(harness.state().undo.len(), 3);
}

/// The whole app, end to end: ROLL, draw a chord and a long note, play (the playhead on the
/// roll moves with the offline engine), undo and redo, save, reopen from the file and find
/// the notes back on the roll.
#[test]
fn roll_draw_play_undo_save_and_reopen_through_the_whole_app() {
    let (mut app, mut engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    app.state_mut().clear_synth();
    app.state_mut().undo.clear();
    app.state_mut().dirty = false;
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 1140.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    radio(&harness, "Roll").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state().lane_view, app::app::LaneView::Roll);

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
        harness.state().state().lane_view,
        app::app::LaneView::Roll,
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

/// The lowest `RadioButton` named `label`: the MODE group's "Pluck" item (before M9 the
/// panel head's preset row had a "Pluck" item too; the browser's entries are buttons).
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

/// With the M8 groups the SYNTH panel and the FX rack scroll inside the top row instead of
/// growing it: at the design size the sequencer row stays where it was.
#[test]
fn m8_groups_do_not_push_the_sequencer_down() {
    let mut harness = app_harness_sized(egui::Vec2::new(1280.0, 1140.0));
    harness.run_steps(3);
    let bass = harness.get_by_label("Bass step 16").rect();
    let octave = harness.get_by_label("Octave").rect();
    let sequencer_bottom = bass.max.y + ui::sequencer::ROW_GAP + 10.0 + 20.0;
    let keys_panel_top = octave.min.y - 30.0;
    assert!(sequencer_bottom <= keys_panel_top);
    // The first body row of each panel is visible unscrolled; the M8 rows start below it.
    let glide = harness.get_by_label("Glide").rect();
    let fm = harness.get_by_label("FM Amt").rect();
    let drive = harness.get_by_label("Drive").rect();
    let bits = harness.get_by_label("Bits").rect();
    assert!(fm.min.y > glide.min.y, "the M8 row is below the M2 row");
    assert!(bits.min.y > drive.min.y, "LO-FI is below DRIVE");
}

// ---------------------------------------------------------------------------------------------
// Visualizers.

use app::app::{RightTab, load_visual_preset};
use app::ui::visualizer::{CLOSE_LABEL, FULL_LABEL, STRIP_SIZE};

/// Renders the transport bar at 1240 wide with the given visualizer preset and returns the
/// rects of the mini strip (if drawn), the MASTER dial and the Test tone pill.
fn transport_with_preset(
    mode: ThemeMode,
    preset: usize,
) -> (Option<egui::Rect>, egui::Rect, egui::Rect) {
    let (mut state, _engine) = AppState::headless(mode);
    state.set_visual_preset(preset);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1240.0, 200.0))
        .build_ui_state(
            move |ui, state: &mut AppState| ui::transport_bar::show(ui, state, &theme),
            state,
        );
    harness.run_steps(4);
    let strip = harness
        .query_by_label("Scope visualizer")
        .map(|node| node.rect());
    let master = harness.get_by_label("Master").rect();
    let test_tone = harness.get_by_label("Test tone").rect();
    (strip, master, test_tone)
}

#[test]
fn transport_strip_renders_in_both_themes_without_growing_the_bar() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (none, master_off, tone_off) = transport_with_preset(mode, neu::OFF_PRESET);
        assert!(none.is_none(), "Off draws no strip");
        let (strip, master_on, tone_on) = transport_with_preset(mode, 0);
        let strip = strip.expect("Soft Scope draws the strip at 1240 wide");
        assert!(strip.width() <= STRIP_SIZE.x + 0.5 && strip.height() <= STRIP_SIZE.y + 0.5);
        assert!(strip.width() >= ui::visualizer::STRIP_MIN_WIDTH);
        // The dial sets the row height; the strip sits inside its vertical span.
        assert!(strip.min.y >= master_on.min.y - 0.5 && strip.max.y <= master_on.max.y + 0.5);
        assert_eq!(
            master_on.height(),
            master_off.height(),
            "the bar keeps its height"
        );
        // The right-hand group stays inside the bar, to the right of the strip.
        assert!(tone_on.min.x >= strip.max.x, "{tone_on:?} vs {strip:?}");
        assert!(tone_on.max.x <= 1240.0 - 20.0 && tone_off.max.x <= 1240.0 - 20.0);
    }
}

/// Asserts that the VISUALIZER panel's PRESET stepper shows `text`. The QOL pass turned every
/// stepper value into a typed readout: a button named `"<label> value"` whose accessible value
/// is the shown text, so the text is no longer a node label of its own.
fn assert_preset_shows(harness: &Harness<'_, AppState>, text: &str) {
    assert_eq!(
        harness
            .get_by_label(&neu::knob::readout_name(ui::visualizer::PRESET_LABEL))
            .value()
            .as_deref(),
        Some(text),
        "the PRESET readout"
    );
}

#[test]
fn visualizer_panel_fits_the_right_column_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (mut state, _engine) = AppState::headless(mode);
        state.right_tab = RightTab::Visual;
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, 1140.0))
            .build_ui_state(
                move |ui, state: &mut AppState| {
                    state.handle_keyboard(ui.ctx());
                    ui::show(ui, state, &theme);
                },
                state,
            );
        harness.run_steps(4);
        let octave = harness.get_by_label("Octave").rect();
        let keys_panel_top = octave.min.y - 30.0;
        // Panel drawing + transport strip both exist; the panel's is the lower one.
        let mut scopes: Vec<egui::Rect> = harness
            .query_all_by_label("Scope visualizer")
            .map(|n| n.rect())
            .collect();
        assert_eq!(scopes.len(), 2, "strip + panel");
        scopes.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
        let panel_scope = scopes[1];
        assert!(
            panel_scope.width() > 100.0 && panel_scope.height() > 40.0,
            "{panel_scope:?}"
        );
        assert!(
            panel_scope.max.y + 20.0 <= keys_panel_top,
            "the panel ends at {} but KEYS starts at {keys_panel_top}",
            panel_scope.max.y
        );
        // Inside the column: no wider than the panel's 220 pt inner width, right of the grid.
        let bass = harness.get_by_label("Bass step 16").rect();
        assert!(
            panel_scope.width() <= 220.5 && panel_scope.min.x >= bass.max.x,
            "{panel_scope:?}"
        );
        harness.get_by_label(FULL_LABEL);
        harness.get_by_label(ui::visualizer::PRESET_LABEL);
        assert_preset_shows(&harness, "Soft Scope");
        assert_eq!(harness.state().right_tab, RightTab::Visual);
        assert!(!harness.state().visual_full);
    }
}

#[test]
fn full_toggle_opens_the_overlay_and_escape_or_close_shuts_it() {
    let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
    state.right_tab = RightTab::Visual;
    let theme = Theme::dark();
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 1140.0))
        .build_ui_state(
            move |ui, state: &mut AppState| {
                state.handle_keyboard(ui.ctx());
                ui::show(ui, state, &theme);
            },
            state,
        );
    harness.run_steps(3);
    assert!(harness.query_by_label(CLOSE_LABEL).is_none());

    harness.get_by_label(FULL_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().visual_full);
    let close = harness.get_by_label(CLOSE_LABEL).rect();
    let top_row = harness
        .state()
        .top_row_rect
        .expect("the top row was measured");
    assert!(
        top_row.contains_rect(close),
        "CLOSE sits inside the overlay over the top row"
    );
    assert_eq!(
        harness.query_all_by_label("Scope visualizer").count(),
        3,
        "strip + panel + overlay"
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
    let (mut state, _engine) = AppState::headless(ThemeMode::Light);
    state.right_tab = RightTab::Visual;
    let theme = Theme::light();
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 1140.0))
        .build_ui_state(
            move |ui, state: &mut AppState| ui::show(ui, state, &theme),
            state,
        );
    harness.run_steps(2);
    // The overlay carries one pill per preset; the panel's stepper shows the chosen name.
    harness.get_by_label(FULL_LABEL).click();
    harness.run_steps(2);
    harness.get_by_label("Orbit").click();
    harness.run_steps(2);
    assert_eq!(harness.state().visual_preset().name, "Orbit");
    assert_eq!(
        harness.query_all_by_label("Radial visualizer").count(),
        3,
        "strip + panel + overlay"
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

// QOL pass: typed values, tap tempo, drag-paint, Ctrl-click, row mute, x2 / /2,
// note nudges, Escape panic, the RECENT tab.

use app::ui::sequencer::{DOUBLE_LABEL, HALVE_LABEL};
use app::ui::tempo::{BPM_VALUE_NAME, TAP_LABEL};

/// The whole app plus the engine behind it, so a test can render a block and check what
/// reached the audio side.
fn app_harness_with_engine(step_dt: f32) -> (Harness<'static, BeatSlopperApp>, engine::Engine) {
    let (app, engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 1080.0))
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
}

#[test]
fn clicking_a_knob_value_and_typing_sets_the_parameter() {
    let mut harness = app_harness();
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

    // The bass lane: Ctrl-click flips the accent (an empty cell gets a note).
    harness
        .get_by_label("Bass step 3")
        .click_modifiers(Modifiers::COMMAND);
    harness.run_steps(2);
    assert!(harness.state().state().pattern().synth[2].accent);
}

#[test]
fn the_tap_pill_sets_the_tempo_from_the_taps() {
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
fn x2_doubles_the_pattern_and_half_brings_it_back() {
    let mut harness = app_harness();
    harness.run_steps(2);
    assert!(
        harness
            .get_by_label(HALVE_LABEL)
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label(DOUBLE_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().len(), 32);
    harness.get_by_label(ui::sequencer::BAR_LABEL);
    let kick = DrumRow::Kick.index();
    assert!(
        harness.state().state().pattern().drums[kick][16].on,
        "the loop was copied into bar 2"
    );
    harness.get_by_label(HALVE_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().pattern().len(), 16);
    assert!(harness.query_by_label(ui::sequencer::BAR_LABEL).is_none());
}

#[test]
fn up_down_and_page_keys_nudge_the_selected_bass_note() {
    let mut harness = app_harness();
    harness.run_steps(2);
    let note = harness.state().state().pattern().synth[0].note.unwrap();
    harness.get_by_label("Bass step 1").click();
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
fn a_double_clicked_row_label_mutes_the_channel() {
    // Two clicks a 60 Hz frame apart are a double-click to egui.
    let (mut harness, _engine) = app_harness_with_engine(1.0 / 60.0);
    harness.run_steps(2);
    let double_click = |harness: &mut Harness<'_, BeatSlopperApp>, label: &str, mods| {
        for _ in 0..2 {
            // The preset list has a "Bass" item too: take the row label (a button).
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
    double_click(&mut harness, "Bass", Modifiers::SHIFT);
    assert!(
        harness
            .state()
            .state()
            .mixer
            .strip(engine::Channel::Synth)
            .solo
    );
}

#[test]
fn escape_once_deselects_and_twice_silences_everything() {
    let mut harness = app_harness();
    harness.run_steps(2);
    harness.get_by_label("Test tone").click();
    harness.get_by_label("Bass step 2").click();
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
fn the_recent_tab_lists_saved_projects_and_ctrl_shift_o_cycles_them() {
    let mut harness = app_harness();
    harness.run_steps(2);
    harness.get_by_label(ui::project_bar::RECENT_LABEL).click();
    harness.run_steps(2);
    harness.get_by_label(ui::recent_panel::EMPTY_LABEL);
    assert_eq!(
        harness.state().state().right_tab,
        app::app::RightTab::Recent
    );

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
    // Opening a project shows the PROJECT panel again; a click on an entry opens it too,
    // and dirty asks first.
    assert_eq!(
        harness.state().state().right_tab,
        app::app::RightTab::Project
    );
    harness.get_by_label("Kick step 2").click();
    harness.run_steps(2);
    harness.get_by_label(ui::project_bar::RECENT_LABEL).click();
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
    harness.get_by_label(ui::project_bar::RECENT_LABEL).click();
    harness.run_steps(2);
    harness
        .get_by_label(ui::recent_panel::CLEAR_LIST_LABEL)
        .click();
    harness.run_steps(2);
    harness.get_by_label(ui::recent_panel::EMPTY_LABEL);
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

// -------------------------------------------------------------------------------------------
// M10: instrument tracks in the panels.

/// The SYNTH head names the edit track ("Synth · <name>") with its colour chip; adding a
/// track changes it, and the preset row edits the edit track only.
#[test]
fn synth_head_names_the_edit_track_and_follows_add_and_select() {
    let mut harness = synth_panel_harness(ThemeMode::Dark);
    harness.get_by_label(&ui::synth_panel::head_title(harness.state()));
    harness.get_by_label("Synth · Synth");
    harness.get_by_label(&ui::controls::track_chip_name("Synth"));
    harness.get_by_label(ui::synth_panel::MODIFIED_LABEL);
    assert!(harness.query_by_label("Synth · Track 2").is_none());

    harness.state_mut().add_track();
    harness.run_steps(2);
    harness.get_by_label("Synth · Track 2");
    harness.get_by_label("Track 2 colour");
    assert!(harness.query_by_label("Synth · Synth").is_none());
    assert_eq!(
        harness
            .get_by_label(ui::synth_panel::MODIFIED_LABEL)
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False),
        "a fresh track is not modified"
    );

    // The preset strip applies to the edit track (track 2); track 1 keeps its preset: the
    // browser's "Keys" entry (M9; the head shows the name), then `<` steps back to "Pad".
    harness.get_by_label(&ui::preset_browser::head_name("Bass"));
    harness
        .get_by_label(ui::preset_browser::BROWSE_LABEL)
        .click();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Keys")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().track(1).preset, 5);
    assert_eq!(harness.state().track(0).preset, 0);
    assert_eq!(harness.state().preset_index(), 5);
    harness.get_by_label(&ui::preset_browser::head_name("Keys"));
    harness
        .get_by_label(ui::preset_browser::CLOSE_LABEL)
        .click();
    harness.run_steps(2);
    harness.get_by_label(ui::preset_browser::PREV_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().preset_index(), 4, "Pad");
    harness.get_by_label(&ui::preset_browser::head_name("Pad"));
    assert_eq!(harness.state().track(0).preset, 0);

    // A knob move lights the MOD lamp; selecting track 1 shows its own state again.
    harness.state_mut().set_param(engine::ParamId::Glide, 0.4);
    harness.run_steps(2);
    assert_eq!(
        harness
            .get_by_label(ui::synth_panel::MODIFIED_LABEL)
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    harness.state_mut().select_track(0);
    harness.run_steps(2);
    harness.get_by_label("Synth · Synth");
    assert_eq!(harness.state().preset_index(), 0);
    assert_eq!(
        harness
            .get_by_label(ui::synth_panel::MODIFIED_LABEL)
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    assert_eq!(
        harness.state().param(engine::ParamId::Glide),
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
    // Five drum meters and three track meters.
    assert_eq!(harness.get_all_by_label("Level").count(), 8);
    // Eight strips side by side, each in its own column.
    let kick = harness.get_by_label("Kick level").rect();
    let lead = harness.get_by_label("Lead level").rect();
    let pad = harness.get_by_label("Pad level").rect();
    let third = harness.get_by_label("Track 3 level").rect();
    assert!(kick.max.x <= lead.min.x && lead.max.x <= pad.min.x && pad.max.x <= third.min.x);
    assert_eq!(lead.min.y, kick.min.y, "the same row");
    assert!(
        third.max.x > ui::mixer::WIDTH + 40.0,
        "eight strips overflow the panel: the body scrolls ({third:?})"
    );

    // Removing a track hides its strip.
    harness.state_mut().remove_track(1);
    harness.run_steps(2);
    assert!(harness.query_by_label("Pad level").is_none());
    harness.get_by_label("Track 3 level");
    assert_eq!(harness.get_all_by_label("Level").count(), 7);
    // And with the default two-track mixer nothing overflows.
    harness.state_mut().remove_track(2);
    harness.run_steps(2);
    assert_eq!(harness.get_all_by_label("Level").count(), 6);
    let stereo = harness.get_by_label("Stereo level").rect();
    assert!(stereo.max.x <= ui::mixer::WIDTH + 40.0);
}

/// The roll draws the other active tracks' notes as ghosts in their colours (both themes):
/// the ghost helper gets the demo bass line while the empty second track is selected, a
/// click on a ghost cell draws a note on the edit track, and selecting the first track
/// turns that note into the ghost.
#[test]
fn roll_shows_the_other_tracks_notes_as_ghosts_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (mut state, _engine) = AppState::headless(mode);
        state.add_track();
        state.set_lane_view(app::app::LaneView::Roll);
        state.undo.clear();
        state.dirty = false;
        let theme = Theme::for_mode(mode);
        let ghosts = ui::sequencer::roll_ghosts(&state, &theme);
        let demo = Pattern::demo();
        assert_eq!(ghosts.len(), demo.notes(0).len());
        assert!(ghosts.iter().all(|g| g.colour == theme.track_colour(0)));
        assert!(ghosts.iter().any(|g| g.start == 0 && g.pitch == 36));

        let ui_theme = theme.clone();
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1000.0, 440.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                move |ui, state: &mut AppState| ui::sequencer::show(ui, state, &ui_theme),
                state,
            );
        harness.run_steps(3);
        harness.get_by_label("Piano roll");
        harness.get_by_label("Track 2 colour");
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
            ui::sequencer::roll_ghosts(harness.state(), &theme).len(),
            demo.notes(0).len()
        );

        harness.state_mut().select_track(0);
        harness.run_steps(2);
        harness.get_by_label("Synth colour");
        let ghosts = ui::sequencer::roll_ghosts(harness.state(), &theme);
        assert_eq!(ghosts.len(), 1);
        assert_eq!(ghosts[0].colour, theme.track_colour(1));
        assert_eq!(harness.state().edit_notes(), demo.notes(0));
    }
}

/// Ctrl+T adds a track, Ctrl+Shift+D duplicates the edit track, Alt+Up / Alt+Down select
/// the previous / next track; the synth head and the sequencer head follow, and Ctrl+Z
/// undoes the structural changes.
#[test]
fn track_shortcuts_add_duplicate_and_select_through_the_app() {
    let mut harness = app_harness();
    harness.run_steps(2);
    harness.get_by_label("Synth · Synth");

    harness.key_press_modifiers(Modifiers::COMMAND, Key::T);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    assert_eq!(harness.state().state().edit_track, 1);
    harness.get_by_label("Synth · Track 2");
    // Ctrl+T is not the piano's T key.
    assert_eq!(harness.state().state().note_on_count, 0);
    assert!(harness.state().state().dirty);

    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 3);
    assert_eq!(harness.state().state().edit_track, 2);
    assert_eq!(harness.state().state().track(2).name, "Track 2 copy");
    harness.get_by_label("Synth · Track 2 copy");
    // The sequencer head carries the edit track's chip too (the mixer is not shown).
    assert!(harness.get_all_by_label("Track 2 copy colour").count() >= 2);

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
    harness.get_by_label("Synth · Synth");
    // Selecting is not an undo step: Ctrl+Z undoes the duplicate, then the add.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 1);
    assert_eq!(harness.state().state().edit_track, 0);
    harness.get_by_label("Synth · Synth");
}

/// With three tracks in the mixer the top row keeps its height and the whole sequencer,
/// its head's track tag included, is still visible at the default window size.
#[test]
fn whole_sequencer_is_visible_with_three_tracks_in_the_mixer() {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    app.state_mut().right_panel = app::app::RightPanel::Mixer;
    app.state_mut().add_track();
    app.state_mut().add_track();
    app.state_mut().rename_track(2, "Sixteen chars ok");
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
    harness.get_by_label("Mute Kick");
    harness.get_by_label("Sixteen chars ok level");
    let bass = harness.get_by_label("Bass step 16").rect();
    let octave = harness.get_by_label("Octave").rect();
    let sequencer_bottom = bass.max.y + ui::sequencer::ROW_GAP + 10.0 + 20.0;
    let keys_panel_top = octave.min.y - 30.0;
    assert!(
        sequencer_bottom <= keys_panel_top,
        "sequencer ends at {sequencer_bottom} but the KEYS panel starts at {keys_panel_top}"
    );
    // The head: the track tag sits after the GRID | ROLL tab and before the right group.
    let seq = harness.get_by_label("Sequencer").rect();
    let tag = harness
        .get_all_by_label("Sixteen chars ok colour")
        .map(|n| n.rect())
        .find(|r| r.min.y >= seq.min.y - 1.0 && r.max.y <= seq.max.y + 1.0)
        .expect("the sequencer head's chip");
    let rec = harness.get_by_label("Rec").rect();
    assert!(tag.max.x < rec.min.x, "{tag:?} vs {rec:?}");
    let roll_tab = radio(&harness, "Roll").rect();
    assert!(roll_tab.max.x <= tag.min.x);
    // The mixer's strips stay inside the top row.
    let synth_level = harness.get_by_label("Synth level").rect();
    assert!(
        synth_level.max.y + 30.0 <= seq.min.y,
        "{synth_level:?} vs {seq:?}"
    );
}
