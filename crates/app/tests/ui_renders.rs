//! egui_kittest: the M0-M4 screens render in both themes without a window or an audio device,
//! the computer keyboard plays the on-screen piano, the SYNTH / FX panels leave every
//! parameter untouched until a control is used, clicking a sequencer cell toggles it, and the
//! MIDI lamp / selector render without opening a port.

use app::ui;
use app::{AppState, BeatSlopperApp, default_params};
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
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
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 1080.0))
        .build_ui_state(
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
            state.pattern,
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
        !harness.state().pattern.drums[kick][1].on,
        "demo: kick step 2 is off"
    );

    harness.get_by_label("Kick step 2").click();
    harness.step();
    let cell = harness.state().pattern.drums[kick][1];
    assert!(cell.on && !cell.accent, "{cell:?}");

    harness.get_by_label("Kick step 2").click();
    harness.step();
    assert!(!harness.state().pattern.drums[kick][1].on);

    // Shift-click: on with the accent; again: accent off.
    harness
        .get_by_label("Snare step 3")
        .click_modifiers(Modifiers::SHIFT);
    harness.step();
    let cell = harness.state().pattern.drums[DrumRow::Snare.index()][2];
    assert!(cell.on && cell.accent, "{cell:?}");
    harness
        .get_by_label("Snare step 3")
        .click_modifiers(Modifiers::SHIFT);
    harness.step();
    let cell = harness.state().pattern.drums[DrumRow::Snare.index()][2];
    assert!(cell.on && !cell.accent, "{cell:?}");

    // Only the clicked cells changed.
    let mut expected = Pattern::demo();
    expected.drums[DrumRow::Snare.index()][2].on = true;
    assert_eq!(harness.state().pattern, expected);
}

#[test]
fn clear_pills_clear_a_row_and_everything() {
    let mut harness = sequencer_harness();
    // Six CLEAR pills (five drum rows and the bass lane), in row order.
    let clears = harness.get_all_by_label("Clear").count();
    assert_eq!(clears, 6);
    harness.get_all_by_label("Clear").nth(3).unwrap().click();
    harness.step();
    let pattern = &harness.state().pattern;
    assert!(
        pattern.drums[DrumRow::ClosedHat.index()]
            .iter()
            .all(|s| !s.on)
    );
    assert!(pattern.drums[DrumRow::Kick.index()].iter().any(|s| s.on));

    harness.get_by_label("Clear all").click();
    harness.step();
    assert!(harness.state().pattern.is_empty());
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
        state.pattern.synth[1],
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
        harness.state().state().pattern.synth[1],
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
