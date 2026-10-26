//! egui_kittest: the CHANNEL RACK's track rows (M10, part 2; the Deck rack since M12). The
//! rack lists one row per active track, a row click selects, the row menu's DUPLICATE /
//! DELETE (with the inline YES / NO confirm) edit the track list, RENAME opens the inline
//! editor (Enter commits, Escape cancels, shortcuts stay quiet while it has the keyboard),
//! the LED and `S` bind to the track's mixer strip, MOVE UP / DOWN reorder, NEXT COLOUR
//! cycles the swatch, Ctrl+T expands the rack, eight tracks scroll inside the rack, and the
//! VOL knob edits the strip.

use app::app::View;
use app::ui;
use app::ui::tracks;
use app::{AppState, BeatSlopperApp};
use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use neu::{Theme, ThemeMode};

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

/// The whole app at 1280 x 800 with `extra` tracks added (the rack shows its wide rows).
fn tracks_app_harness(extra: usize) -> Harness<'static, BeatSlopperApp> {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    for _ in 0..extra {
        app.state_mut().add_track();
    }
    // The set-up tracks are not edits the tests care about.
    app.state_mut().undo.clear();
    app.state_mut().dirty = false;
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 800.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    harness
}

fn type_text(harness: &mut Harness<'_, BeatSlopperApp>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run_steps(2);
}

/// Opens the row menu of the track row named `row` (a right-click) and clicks `entry`.
fn menu_pick(harness: &mut Harness<'_, BeatSlopperApp>, row: &str, entry: &str) {
    harness.get_by_label(row).click_secondary();
    harness.run_steps(2);
    harness.get_by_role_and_label(Role::MenuItem, entry).click();
    harness.run_steps(2);
}

/// Whether the row menu's `entry` is disabled (the menu is opened and closed again).
fn menu_entry_disabled(harness: &mut Harness<'_, BeatSlopperApp>, row: &str, entry: &str) -> bool {
    harness.get_by_label(row).click_secondary();
    harness.run_steps(2);
    let disabled = harness
        .get_by_role_and_label(Role::MenuItem, entry)
        .accesskit_node()
        .is_disabled();
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    // A menu that is still open (Escape only closes it on the next frame) closes on a click
    // elsewhere; the toolbar's hint panel is a safe target.
    if harness
        .query_by_role_and_label(Role::MenuItem, entry)
        .is_some()
    {
        harness.get_by_label(ui::transport_bar::HINT_NAME).click();
        harness.run_steps(2);
    }
    disabled
}

/// The rack alone lists one row per active track with the right names, in both themes,
/// without editing anything.
#[test]
fn the_rack_lists_one_row_per_active_track_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (mut state, _engine) = AppState::headless(mode);
        state.add_track();
        state.add_track();
        state.rename_track(2, "Sixteen chars ok");
        state.undo.clear();
        state.dirty = false;
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(700.0, 500.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::sequencer::show(ui, state, &theme),
                state,
            );
        harness.run_steps(3);
        harness.get_by_label(&tracks::row_name(0, "Synth"));
        harness.get_by_label(&tracks::row_name(1, "Track 2"));
        harness.get_by_label(&tracks::row_name(2, "Sixteen chars ok"));
        assert!(
            harness
                .query_by_label(&tracks::row_name(3, "Track 4"))
                .is_none()
        );
        harness.get_by_label(&tracks::led_name("Synth"));
        harness.get_by_label(&tracks::solo_name("Sixteen chars ok"));
        harness.get_by_label(&tracks::level_name("Synth"));
        harness.get_by_label(&tracks::pan_name("Track 2"));
        harness.get_by_label(&tracks::insert_box_name("Synth"));
        harness.get_by_label(tracks::ADD_NAME);
        harness.get_by_label("Sixteen chars ok step 16");
        // The selected row is track 3 (the last added); the others are not.
        assert_eq!(
            harness
                .get_by_label(&tracks::row_name(2, "Sixteen chars ok"))
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );
        assert_eq!(
            harness
                .get_by_label(&tracks::row_name(0, "Synth"))
                .accesskit_node()
                .toggled(),
            Some(Toggled::False)
        );
        // The TRACKS section head counts them.
        harness.get_by_label(ui::sequencer::TRACKS_TITLE);
        let state = harness.state();
        assert!(!state.dirty, "rendering edits nothing");
        assert_eq!(state.active_track_count(), 3);
        assert_eq!(state.edit_track, 2);
    }
}

/// A click on a row's name selects its track: `edit_track` changes, the roll's track
/// dropdown and the rack header follow.
#[test]
fn a_row_click_selects_and_the_roll_follows() {
    let mut harness = tracks_app_harness(1);
    assert_eq!(harness.state().state().edit_track, 1);
    assert_eq!(
        harness
            .get_by_label(ui::roll_view::TRACK_DROPDOWN)
            .value()
            .as_deref(),
        Some("Track 2")
    );

    harness.get_by_label(&tracks::row_name(0, "Synth")).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 0);
    assert_eq!(
        harness
            .get_by_label(ui::roll_view::TRACK_DROPDOWN)
            .value()
            .as_deref(),
        Some("Synth")
    );
    assert_eq!(
        harness
            .get_by_label(&tracks::row_name(0, "Synth"))
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    assert!(!harness.state().state().dirty, "selecting is not an edit");
    // The roll's dropdown selects too.
    harness.get_by_label(ui::roll_view::TRACK_DROPDOWN).click();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(Role::MenuItem, "Track 2")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 1);
}

/// ADD adds and selects, DUPLICATE duplicates as "<name> copy", DELETE asks: NO keeps, YES
/// removes and selects the nearest track; with one track DELETE is disabled.
#[test]
fn add_duplicate_and_delete_with_the_inline_confirm() {
    let mut harness = tracks_app_harness(0);
    assert!(menu_entry_disabled(
        &mut harness,
        &tracks::row_name(0, "Synth"),
        tracks::DEL_LABEL
    ));

    harness.get_by_label(tracks::ADD_NAME).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    assert_eq!(harness.state().state().edit_track, 1);
    harness.get_by_label(&tracks::row_name(1, "Track 2"));
    assert!(harness.state().state().dirty);

    menu_pick(
        &mut harness,
        &tracks::row_name(1, "Track 2"),
        tracks::DUP_LABEL,
    );
    assert_eq!(harness.state().state().active_track_count(), 3);
    assert_eq!(harness.state().state().edit_track, 2);
    assert_eq!(harness.state().state().track(2).name, "Track 2 copy");
    harness.get_by_label(&tracks::row_name(2, "Track 2 copy"));

    // DELETE asks in the add row; NO keeps the track and brings the add row back.
    assert!(!menu_entry_disabled(
        &mut harness,
        &tracks::row_name(2, "Track 2 copy"),
        tracks::DEL_LABEL
    ));
    menu_pick(
        &mut harness,
        &tracks::row_name(2, "Track 2 copy"),
        tracks::DEL_LABEL,
    );
    harness.get_by_label(&tracks::delete_question("Track 2 copy"));
    assert!(harness.query_by_label(tracks::ADD_NAME).is_none());
    harness.get_by_label(tracks::NO_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 3);
    assert!(
        harness
            .query_by_label(&tracks::delete_question("Track 2 copy"))
            .is_none()
    );
    harness.get_by_label(tracks::ADD_NAME);

    // YES removes it and selects the nearest track (track 2).
    menu_pick(
        &mut harness,
        &tracks::row_name(2, "Track 2 copy"),
        tracks::DEL_LABEL,
    );
    harness.get_by_label(tracks::YES_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    assert_eq!(harness.state().state().edit_track, 1);
    assert!(
        harness
            .query_by_label(&tracks::row_name(2, "Track 2 copy"))
            .is_none()
    );

    // Down to one track: DELETE greys out.
    menu_pick(
        &mut harness,
        &tracks::row_name(1, "Track 2"),
        tracks::DEL_LABEL,
    );
    harness.get_by_label(tracks::YES_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 1);
    assert_eq!(harness.state().state().edit_track, 0);
    assert!(menu_entry_disabled(
        &mut harness,
        &tracks::row_name(0, "Synth"),
        tracks::DEL_LABEL
    ));
    assert!(
        harness
            .query_by_label(&tracks::delete_question("Synth"))
            .is_none()
    );
    assert_eq!(harness.state().state().active_track_count(), 1);
    // Undo brings the deleted track back, row included.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    harness.get_by_label(&tracks::row_name(1, "Track 2"));
}

/// RENAME opens the inline editor: shortcuts and the piano stay quiet while it has the
/// keyboard, Enter commits the typed name, Escape cancels; a double-click on the name opens
/// it too.
#[test]
fn rename_commits_on_enter_and_cancels_on_escape() {
    let mut harness = tracks_app_harness(0);
    menu_pick(
        &mut harness,
        &tracks::row_name(0, "Synth"),
        tracks::RENAME_LABEL,
    );
    assert!(
        harness.ctx.text_edit_focused(),
        "the inline editor has the keyboard"
    );
    type_text(&mut harness, "Lead");
    // Space is not PLAY and the letters are not piano keys while the field has focus.
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(!harness.state().state().playing);
    assert_eq!(harness.state().state().note_on_count, 0);
    assert_eq!(
        harness.state().state().track(0).name,
        "Synth",
        "not yet committed"
    );
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    // `rename_track` trims, so a Space typed into the field leaves no trailing blank.
    assert_eq!(harness.state().state().track(0).name, "Lead");
    harness.get_by_label(&tracks::row_name(0, "Lead"));
    assert_eq!(
        harness
            .get_by_label(ui::roll_view::TRACK_DROPDOWN)
            .value()
            .as_deref(),
        Some("Lead")
    );
    assert!(harness.state().state().dirty);

    // Escape cancels.
    menu_pick(
        &mut harness,
        &tracks::row_name(0, "Lead"),
        tracks::RENAME_LABEL,
    );
    type_text(&mut harness, "Nope");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state().state().track(0).name, "Lead");
    harness.get_by_label(&tracks::row_name(0, "Lead"));
    // After the editor closed the shortcuts work again.
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(harness.state().state().playing);
    harness.key_press(Key::Space);
    harness.run_steps(2);

    // A double-click on the name opens the editor as well.
    let (mut fast, _engine) = {
        let (app, engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
        let harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, 800.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BeatSlopperApp| {
                    app.tick(ui.ctx());
                    app.show(ui);
                },
                app,
            );
        (harness, engine)
    };
    fast.run_steps(3);
    for _ in 0..2 {
        fast.get_by_label(&tracks::row_name(0, "Synth")).click();
        fast.step();
    }
    fast.run_steps(2);
    assert!(fast.ctx.text_edit_focused(), "a double-click renames");
    fast.event(Event::Text("Keys".to_owned()));
    fast.run_steps(2);
    fast.key_press(Key::Enter);
    fast.run_steps(2);
    assert_eq!(fast.state().state().track(0).name, "Keys");
}

/// The LED and `S` toggle the track's mixer strip; neither selects the track.
#[test]
fn led_and_s_bind_to_the_track() {
    let mut harness = tracks_app_harness(1);
    let synth = engine::Channel::from_track(0);
    let two = engine::Channel::from_track(1);
    harness.get_by_label(&tracks::led_name("Synth")).click();
    harness.run_steps(2);
    assert!(harness.state().state().mixer.strip(synth).mute);
    assert!(!harness.state().state().mixer.strip(two).mute);
    assert_eq!(
        harness
            .get_by_label(&tracks::led_name("Synth"))
            .accesskit_node()
            .toggled(),
        Some(Toggled::False),
        "the LED is off while muted"
    );
    harness.get_by_label(&tracks::solo_name("Track 2")).click();
    harness.run_steps(2);
    assert!(harness.state().state().mixer.strip(two).solo);
    assert!(!harness.state().state().mixer.strip(synth).solo);
    assert_eq!(
        harness
            .get_by_label(&tracks::solo_name("Track 2"))
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    // Clicking the LED did not select the track.
    assert_eq!(harness.state().state().edit_track, 1);
    harness.get_by_label(&tracks::led_name("Synth")).click();
    harness.run_steps(2);
    assert!(!harness.state().state().mixer.strip(synth).mute);
    // A right-click on the LED solos.
    harness
        .get_by_label(&tracks::led_name("Synth"))
        .click_secondary();
    harness.run_steps(2);
    assert!(harness.state().state().mixer.strip(synth).solo);
    assert_eq!(harness.state().state().edit_track, 1);
}

/// MOVE UP / DOWN move the selected track (the selection follows it) and NEXT COLOUR cycles
/// the swatch.
#[test]
fn move_entries_reorder_and_next_colour_cycles_the_swatch() {
    let mut harness = tracks_app_harness(1);
    assert_eq!(harness.state().state().edit_track, 1);
    assert!(menu_entry_disabled(
        &mut harness,
        &tracks::row_name(1, "Track 2"),
        tracks::MOVE_DOWN_NAME
    ));
    menu_pick(
        &mut harness,
        &tracks::row_name(1, "Track 2"),
        tracks::MOVE_UP_NAME,
    );
    assert_eq!(harness.state().state().track(0).name, "Track 2");
    assert_eq!(harness.state().state().track(1).name, "Synth");
    assert_eq!(harness.state().state().edit_track, 0);
    harness.get_by_label(&tracks::row_name(0, "Track 2"));
    harness.get_by_label(&tracks::row_name(1, "Synth"));
    assert!(menu_entry_disabled(
        &mut harness,
        &tracks::row_name(0, "Track 2"),
        tracks::MOVE_UP_NAME
    ));
    menu_pick(
        &mut harness,
        &tracks::row_name(0, "Track 2"),
        tracks::MOVE_DOWN_NAME,
    );
    assert_eq!(harness.state().state().track(0).name, "Synth");
    assert_eq!(harness.state().state().edit_track, 1);

    let before = harness.state().state().track(0).colour;
    menu_pick(
        &mut harness,
        &tracks::row_name(0, "Synth"),
        tracks::COLOUR_LABEL,
    );
    assert_eq!(harness.state().state().track(0).colour, (before + 1) % 8);
    assert_eq!(
        harness.state().state().edit_track,
        1,
        "the colour does not select"
    );
}

/// Ctrl+T adds a track and expands the rack so the new row is visible.
#[test]
fn ctrl_t_adds_a_track_and_expands_the_rack() {
    let mut harness = app_harness_sized(egui::Vec2::new(1280.0, 800.0));
    harness.run_steps(2);
    harness.key_press(Key::F7);
    harness.run_steps(2);
    assert!(!harness.state().state().rack_open);
    assert!(harness.query_by_label(tracks::ADD_NAME).is_none());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::T);
    harness.run_steps(2);
    assert!(harness.state().state().rack_open);
    assert_eq!(harness.state().state().view, View::Synth, "the view stays");
    harness.get_by_label(&tracks::row_name(1, "Track 2"));
    harness.get_by_label(tracks::ADD_NAME);
}

/// With all eight tracks active the rack scrolls inside its window: every row exists, the
/// visible ones sit inside the rack, and ADD refuses a ninth track.
#[test]
fn eight_tracks_scroll_inside_the_rack() {
    let mut harness = tracks_app_harness(7);
    assert_eq!(harness.state().state().active_track_count(), 8);
    let handle = harness.get_by_label(ui::shell::RACK_HANDLE_NAME).rect();
    let first = harness.get_by_label(&tracks::row_name(0, "Synth")).rect();
    let kick = harness.get_by_label("Kick").rect();
    assert!(first.min.y > kick.max.y, "the tracks follow the drums");
    assert!(first.max.x <= handle.min.x, "inside the rack");
    // The list holds every track (scrolled-away rows still exist).
    harness.get_by_label(&tracks::row_name(7, "Track 8"));
    let last = harness.get_by_label(&tracks::row_name(7, "Track 8")).rect();
    assert_eq!(
        last.min.y - first.min.y,
        7.0 * ui::sequencer::ROW_PITCH,
        "rows on the 32 pt pitch"
    );
    let add = harness.get_by_label(tracks::ADD_NAME).rect();
    assert!(add.min.y > last.max.y, "the add row follows the last track");
    // The sixteen steps of every row line up with the drums'.
    let kick_16 = harness.get_by_label("Kick step 16").rect();
    let eight_16 = harness.get_by_label("Track 8 step 16").rect();
    assert_eq!(kick_16.min.x, eight_16.min.x);
    // ADD refuses a ninth track and keeps the eight.
    harness.state_mut().state_mut().add_track();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 8);
    assert_eq!(
        harness.state().state().status_text(),
        Some("All 8 tracks are in use")
    );
}

/// Every track row carries the VOL mini knob bound to the track's strip (the mixer's fader
/// shows the same value) and the PAN knob, both inside the row's left columns.
#[test]
fn the_vol_knob_edits_the_strip_and_the_row_fits() {
    let mut harness = tracks_app_harness(1);
    harness
        .state_mut()
        .state_mut()
        .rename_track(1, "Sixteen chars ok");
    harness.run_steps(2);
    for (index, name) in ["Synth", "Sixteen chars ok"].into_iter().enumerate() {
        let level = tracks::level_name(name);
        let knob = harness.get_by_label(&level);
        assert_eq!(knob.accesskit_node().role(), Role::Slider);
        assert_eq!(knob.accesskit_node().numeric_value(), Some(0.0));
        let knob = knob.rect();
        let solo = harness.get_by_label(&tracks::solo_name(name)).rect();
        let pan = harness.get_by_label(&tracks::pan_name(name)).rect();
        let ins = harness.get_by_label(&tracks::insert_box_name(name)).rect();
        let row = harness.get_by_label(&tracks::row_name(index, name)).rect();
        let first_cell = harness.get_by_label(&format!("{name} step 1")).rect();
        assert!(
            solo.max.x <= knob.min.x && knob.max.x <= pan.min.x && pan.max.x <= ins.min.x,
            "S | VOL | PAN | INS"
        );
        assert!(ins.max.x <= row.min.x, "the name follows the number box");
        assert!(row.max.x <= first_cell.min.x, "the cells follow the name");
        assert_eq!(row.width(), ui::sequencer::NAME_WIDTH, "a fixed name width");
        assert_eq!(knob.width(), ui::sequencer::KNOB_WIDTH);
        assert!(knob.height() <= ui::sequencer::ROW_HEIGHT);
    }

    // The knob reads the strip and edits it: a mixer change shows, the keyboard moves it.
    let channel = engine::Channel::from_track(1);
    harness
        .state_mut()
        .state_mut()
        .set_mixer(channel, engine::ChannelParam::Level, -6.0);
    harness.run_steps(2);
    let level = tracks::level_name("Sixteen chars ok");
    let knob = harness.get_by_label(&level);
    assert_eq!(knob.accesskit_node().numeric_value(), Some(-6.0));
    knob.focus();
    harness.run_steps(2);
    harness.key_press(Key::ArrowRight);
    harness.run_steps(2);
    let level = harness
        .state()
        .state()
        .mixer
        .get(channel, engine::ChannelParam::Level);
    assert!(level > -6.0, "the arrow key raised the level: {level}");
    assert!(
        harness.state().state().undo.len() >= 2,
        "set_mixer and the key press are undo steps"
    );
    assert_eq!(
        harness.state().state().edit_track,
        1,
        "editing the level does not change the selection"
    );
}
