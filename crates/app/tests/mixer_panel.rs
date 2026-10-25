//! egui_kittest: the M11 FL-style MIXER panel. A source's number box routes it (and the
//! engine mirror follows), a slot pill loads an effect whose knobs replace the EQ / send
//! grid, a slot knob edit reaches the engine, the ROUTE pill only offers the master and
//! higher inserts, the head selects (Shift extends) and selected faders move together, the
//! pencil renames inline and the name survives the project text, undo takes a slot change
//! back, meters and the gain-reduction bar render, Ctrl+L links the edit track, the TRACKS
//! and sequencer number boxes route too, and everything renders in both themes.

use app::ui;
use app::ui::mixer;
use app::{AppState, BeatSlopperApp};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use engine::{Channel, InsertParam, Meters, SlotKind};
use neu::{Theme, ThemeMode};

/// The panel alone, wide enough for every insert strip to be in view (the real panel shows
/// two at a time and scrolls), with the offline engine kept for mirror checks.
fn panel_harness(mode: ThemeMode) -> (Harness<'static, AppState>, engine::Engine) {
    let (state, engine) = AppState::headless(mode);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(2040.0, 420.0))
        .build_ui_state(
            move |ui, state: &mut AppState| mixer::show(ui, state, &theme),
            state,
        );
    harness.run_steps(3);
    (harness, engine)
}

/// Drains the state's outbox into `engine` and processes it.
fn sync(harness: &mut Harness<'_, AppState>, engine: &mut engine::Engine) {
    loop {
        engine.process_messages();
        let state = harness.state_mut();
        if state.flush_outbox() == 0 && state.outbox.is_empty() {
            break;
        }
    }
    engine.process_messages();
}

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

fn pointer_click<S>(harness: &mut Harness<'_, S>, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
}

#[test]
fn the_number_box_routes_a_source_and_the_engine_follows() {
    let (mut harness, mut engine) = panel_harness(ThemeMode::Dark);
    assert_eq!(harness.state().mixer.route(Channel::Kick), 1);
    harness
        .get_by_label(&mixer::source_box_name("Kick"))
        .click();
    harness.run_steps(2);
    // The picker lists the master and every insert by name.
    harness.get_by_label(&mixer::pick_name(0));
    harness.get_by_label(&mixer::pick_name(16));
    harness.get_by_label(&mixer::pick_name(14)).click();
    harness.run_steps(2);
    let state = harness.state();
    assert_eq!(state.mixer.route(Channel::Kick), 14);
    assert!(state.dirty);
    assert_eq!(state.undo.len(), 1);
    assert_eq!(
        state.insert_selection,
        vec![14],
        "the pick selects that insert"
    );
    assert!(
        harness.query_by_label(&mixer::pick_name(14)).is_none(),
        "the picker closed"
    );
    sync(&mut harness, &mut engine);
    assert_eq!(engine.mixer().route(Channel::Kick), 14);
    assert_eq!(engine.mixer(), &harness.state().mixer);
    // The other sources are untouched and the kick now sits on insert 14's source list.
    assert_eq!(harness.state().mixer.route(Channel::Snare), 2);
    assert_eq!(harness.state().insert_sources(14), vec![Channel::Kick]);
    assert!(harness.state().insert_sources(1).is_empty());
}

#[test]
fn a_slot_pill_loads_an_effect_and_shows_its_knobs() {
    let (mut harness, mut engine) = panel_harness(ThemeMode::Dark);
    harness.get_by_label(&mixer::slot_name(1, 0)).click();
    harness.run_steps(2);
    for kind in SlotKind::ALL {
        harness.get_by_label(&mixer::kind_pick_name(kind));
    }
    harness
        .get_by_label(&mixer::kind_pick_name(SlotKind::Compressor))
        .click();
    harness.run_steps(2);
    let state = harness.state();
    assert_eq!(state.mixer.insert(1).slot(0).kind, SlotKind::Compressor);
    assert_eq!(
        state.open_slot,
        Some((1, 0)),
        "a fresh load opens its knobs"
    );
    assert_eq!(state.undo.len(), 1);
    // The grid shows the compressor's six knobs instead of the EQ / sends.
    let params = SlotKind::Compressor.params();
    assert_eq!(params.len(), 6);
    for param in params {
        harness.get_by_label(&mixer::slot_control_name(1, 0, param));
    }
    assert!(
        harness
            .query_by_label(&mixer::insert_control_name(1, InsertParam::Eq3Low))
            .is_none()
    );
    harness.get_by_label(&mixer::slot_edit_name(1, 0));
    sync(&mut harness, &mut engine);
    assert_eq!(engine.mixer().insert(1).slot(0).kind, SlotKind::Compressor);

    // The edit square toggles back to the EQ / sends view; a second slot with a two-knob
    // kind shows two knobs and leaves the other cells empty.
    harness.get_by_label(&mixer::slot_edit_name(1, 0)).click();
    harness.run_steps(2);
    assert_eq!(harness.state().open_slot, None);
    harness.get_by_label(&mixer::insert_control_name(1, InsertParam::Eq3Low));
    harness.get_by_label(&mixer::slot_name(1, 1)).click();
    harness.run_steps(2);
    harness
        .get_by_label(&mixer::kind_pick_name(SlotKind::Drive))
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().open_slot, Some((1, 1)));
    let drive = SlotKind::Drive.params();
    assert_eq!(drive.len(), 2);
    for param in drive {
        harness.get_by_label(&mixer::slot_control_name(1, 1, param));
    }

    // Undo takes the slot change back and the knobs with it.
    harness.state_mut().undo();
    harness.run_steps(2);
    assert_eq!(harness.state().mixer.insert(1).slot(1).kind, SlotKind::Off);
    assert_eq!(harness.state().open_slot, None);
    assert!(
        harness
            .query_by_label(&mixer::slot_edit_name(1, 1))
            .is_none()
    );
    harness.get_by_label(&mixer::insert_control_name(1, InsertParam::Eq3Low));
    sync(&mut harness, &mut engine);
    assert_eq!(engine.mixer(), &harness.state().mixer);
}

#[test]
fn a_slot_knob_edit_reaches_the_engine() {
    let (mut harness, mut engine) = panel_harness(ThemeMode::Dark);
    harness
        .state_mut()
        .set_slot_kind(1, 0, SlotKind::Compressor);
    harness.state_mut().toggle_open_slot(1, 0);
    harness.run_steps(2);
    let ratio = &SlotKind::Compressor.params()[1];
    let before = harness.state().mixer.insert(1).slot(0).get(1);
    let ratio_name = mixer::slot_control_name(1, 0, ratio);
    harness.get_by_label(&ratio_name).click();
    harness.run_steps(1);
    harness.key_press(Key::ArrowUp);
    harness.run_steps(2);
    let after = harness.state().mixer.insert(1).slot(0).get(1);
    assert!(after > before, "{after} > {before}");
    assert!(after <= ratio.max);
    sync(&mut harness, &mut engine);
    assert_eq!(engine.mixer().insert(1).slot(0).get(1), after);
    // Typed entry through the knob's readout is the kit's; the undo step is there.
    assert!(harness.state().undo.len() >= 2);
}

#[test]
fn the_route_pill_offers_the_master_and_higher_inserts_only() {
    let (mut harness, mut engine) = panel_harness(ThemeMode::Dark);
    harness.get_by_label(&mixer::route_name(14)).click();
    harness.run_steps(2);
    harness.get_by_label(&mixer::pick_name(0));
    harness.get_by_label(&mixer::pick_name(15));
    harness.get_by_label(&mixer::pick_name(16));
    assert!(harness.query_by_label(&mixer::pick_name(14)).is_none());
    assert!(harness.query_by_label(&mixer::pick_name(13)).is_none());
    harness.get_by_label(&mixer::pick_name(16)).click();
    harness.run_steps(2);
    assert_eq!(harness.state().mixer.insert(14).route, 16);
    assert_eq!(harness.state().undo.len(), 1);
    sync(&mut harness, &mut engine);
    assert_eq!(engine.mixer().insert(14).route, 16);
    // The state refuses a lower number outright (what the picker never offers).
    assert_eq!(
        harness.state_mut().set_insert(14, InsertParam::Route, 3.0),
        0.0
    );
    assert_eq!(harness.state().mixer.insert(14).route, 0);
    sync(&mut harness, &mut engine);
    assert_eq!(engine.mixer().insert(14).route, 0);
}

#[test]
fn head_clicks_select_and_selected_faders_move_together() {
    let (mut harness, mut engine) = panel_harness(ThemeMode::Dark);
    harness
        .get_by_label(&mixer::insert_head_name(1, "Insert 1"))
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().insert_selection, vec![1]);
    harness
        .get_by_label(&mixer::insert_head_name(2, "Insert 2"))
        .click_modifiers(Modifiers::SHIFT);
    harness.run_steps(2);
    assert_eq!(harness.state().insert_selection, vec![1, 2]);
    assert!(!harness.state().dirty, "selecting is not an edit");
    // A press in the middle of insert 1's fader sets it from the pointer; insert 2 moves by
    // the same amount, insert 3 stays.
    let fader = harness
        .get_by_label(&mixer::insert_control_name(1, InsertParam::Level))
        .rect();
    pointer_click(&mut harness, fader.center());
    let state = harness.state();
    let one = state.mixer.insert(1).level_db;
    assert!(one < -10.0 && one > -50.0, "{one}");
    assert_eq!(state.mixer.insert(2).level_db, one);
    assert_eq!(state.mixer.insert(3).level_db, 0.0);
    assert_eq!(state.undo.len(), 1, "one group move, one step");
    sync(&mut harness, &mut engine);
    assert_eq!(engine.mixer().insert(2).level_db, one);
    // A plain click on insert 3's head replaces the selection.
    harness
        .get_by_label(&mixer::insert_head_name(3, "Insert 3"))
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().insert_selection, vec![3]);
}

#[test]
fn the_pencil_renames_inline_and_the_name_survives_the_project_text() {
    let (mut harness, _engine) = panel_harness(ThemeMode::Dark);
    harness.get_by_label(&mixer::insert_rename_name(1)).click();
    harness.run_steps(2);
    assert!(
        harness.ctx.text_edit_focused(),
        "the inline editor has the keyboard"
    );
    harness.event(Event::Text("Drums".to_owned()));
    harness.run_steps(2);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let state = harness.state();
    assert_eq!(state.insert_ui(1).name, "Drums");
    assert!(state.dirty);
    assert_eq!(state.undo.len(), 1);
    harness.get_by_label(&mixer::insert_head_name(1, "Drums"));
    // The picker shows the new name too.
    harness
        .get_by_label(&mixer::source_box_name("Kick"))
        .click();
    harness.run_steps(2);
    harness.get_by_label(&mixer::pick_name(1));
    harness.key_press(Key::Escape);
    harness.run_steps(2);

    // Through the project text (what Save writes) into a fresh state.
    let text = harness.state().project_text();
    assert!(text.contains("\"insert_ui\""));
    assert!(text.contains("Drums"));
    let (mut other, _engine) = AppState::headless(ThemeMode::Light);
    other.apply_project_text(&text).unwrap();
    assert_eq!(other.insert_ui(1).name, "Drums");
    assert_eq!(other.insert_ui(2).name, "Insert 2");
    // Escape cancels a rename.
    harness.get_by_label(&mixer::insert_rename_name(2)).click();
    harness.run_steps(2);
    harness.event(Event::Text("Nope".to_owned()));
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state().insert_ui(2).name, "Insert 2");
}

#[test]
fn meters_render_and_a_compressor_adds_the_gain_reduction_bar() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (mut harness, _engine) = panel_harness(mode);
        assert_eq!(harness.get_all_by_label("Level").count(), 16);
        assert!(harness.query_by_label(mixer::GR_METER_NAME).is_none());
        {
            let state = harness.state_mut();
            state.set_slot_kind(1, 0, SlotKind::Compressor);
            state.set_slot_kind(2, 2, SlotKind::Eq3);
            state.toggle_open_slot(2, 2);
            state.select_insert(1, false);
            state.set_route(Channel::Kick, 0);
            let mut loud = Meters::SILENT;
            loud.insert[0] = 0.9;
            loud.insert_gr[0] = 8.0;
            loud.master = [0.5, 0.4];
            state.update_meters_with(loud, 0.0);
        }
        harness.run_steps(3);
        assert_eq!(harness.get_all_by_label(mixer::GR_METER_NAME).count(), 1);
        assert_eq!(harness.get_all_by_label("Level").count(), 16);
        harness.get_by_label("Stereo level");
        for param in SlotKind::Eq3.params() {
            harness.get_by_label(&mixer::slot_control_name(2, 2, param));
        }
        harness.get_by_label(&mixer::insert_control_name(1, InsertParam::Eq3High));
        let state = harness.into_state();
        assert_eq!(state.mode, mode);
        assert!(state.meters_active());
    }
}

#[test]
fn ctrl_l_links_the_edit_track_to_a_free_insert_and_shows_the_mixer() {
    let mut harness = app_harness_sized(egui::Vec2::new(1280.0, 1140.0));
    harness.run_steps(2);
    assert_eq!(harness.state().state().mixer.route(Channel::Synth), 6);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::L);
    harness.run_steps(3);
    let state = harness.state().state();
    assert_eq!(
        state.mixer.route(Channel::Synth),
        7,
        "7 is the first free insert"
    );
    assert_eq!(state.right_panel, app::app::RightPanel::Mixer);
    assert_eq!(state.highlighted_insert(), 7);
    assert!(state.dirty);
    // The mixer is showing with the source strips in view.
    harness.get_by_label("Mute Kick");
    harness.get_by_label(&mixer::source_box_name("Synth"));
    // Ctrl+Z puts the route back.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().state().mixer.route(Channel::Synth), 6);
}

#[test]
fn the_tracks_row_and_sequencer_row_boxes_route_their_channels() {
    let mut harness = app_harness_sized(egui::Vec2::new(1280.0, 1140.0));
    harness.state_mut().state_mut().right_tab = app::app::RightTab::Tracks;
    harness.run_steps(3);
    // TRACKS row: the box opens the same picker.
    harness
        .get_by_label(&ui::tracks::insert_box_name("Synth"))
        .click();
    harness.run_steps(2);
    harness.get_by_label(&mixer::pick_name(9)).click();
    harness.run_steps(2);
    let state = harness.state().state();
    assert_eq!(state.mixer.route(Channel::Synth), 9);
    assert_eq!(
        state.edit_track, 0,
        "a box click does not change the selection"
    );
    assert!(!harness.ctx.text_edit_focused(), "no rename editor opened");
    // The sequencer rows: the kick straight to the master, the bass row is the edit track.
    harness
        .get_by_label(&ui::sequencer::row_box_name("Kick"))
        .click();
    harness.run_steps(2);
    harness.get_by_label(&mixer::pick_name(0)).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().mixer.route(Channel::Kick), 0);
    assert!(
        !harness.state().state().mixer.strip(Channel::Kick).mute,
        "no mute toggled"
    );
    harness
        .get_by_label(&ui::sequencer::row_box_name(ui::sequencer::BASS_NAME))
        .click();
    harness.run_steps(2);
    harness.get_by_label(&mixer::pick_name(12)).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().mixer.route(Channel::Synth), 12);
    // The LINK button in the TRACKS footer does what Ctrl+L does.
    harness.get_by_label(ui::tracks::LINK_NAME).click();
    harness.run_steps(2);
    let state = harness.state().state();
    assert_eq!(
        state.mixer.route(Channel::Synth),
        1,
        "insert 1 was freed by the kick"
    );
    assert_eq!(state.right_panel, app::app::RightPanel::Mixer);
}

/// The whole app at the design size with the MIXER showing: the sequencer stays whole, the
/// panel stays inside the FX rack's slot (both docks scroll inside it) and the inserts dock
/// starts right of the source dock.
#[test]
fn the_mixer_stays_inside_its_slot_at_the_design_size() {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    app.state_mut().right_panel = app::app::RightPanel::Mixer;
    for _ in 0..3 {
        app.state_mut().add_track();
    }
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
    let sequencer_bottom = bass.max.y + ui::sequencer::ROW_GAP + 10.0 + 20.0;
    assert!(sequencer_bottom <= octave.min.y - 30.0);
    let seq = harness.get_by_label("Sequencer").rect();
    let kick = harness.get_by_label("Kick level").rect();
    let insert_one = harness.get_by_label("Insert 1 level").rect();
    let stereo = harness.get_by_label("Stereo level").rect();
    let page_right = 1280.0 - 20.0;
    assert!(stereo.max.x <= page_right + 0.5, "{stereo:?}");
    assert!(insert_one.min.x >= kick.min.x + mixer::SOURCE_DOCK_WIDTH);
    assert!(
        insert_one.max.y + 10.0 <= seq.min.y,
        "{insert_one:?} vs {seq:?}"
    );
    assert!(stereo.max.y + 10.0 <= seq.min.y);
    // The fader got a usable track under the head, pills, grid and route pill.
    assert!(insert_one.height() >= 60.0, "{insert_one:?}");
    // The panel's sources scroll: the fourth track's strip sits past the dock.
    let last = harness.get_by_label("Track 4 level").rect();
    assert!(last.min.x >= kick.min.x + mixer::SOURCE_DOCK_WIDTH);
}
