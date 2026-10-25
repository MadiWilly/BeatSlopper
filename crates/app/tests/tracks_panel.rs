//! egui_kittest: the TRACKS panel (M10, part 2). The panel lists one row per active track,
//! a row click selects, ADD / DUP / DEL (with the inline YES / NO confirm) edit the track
//! list, the pencil opens the inline rename editor (Enter commits, Escape cancels, shortcuts
//! stay quiet while it has the keyboard), M / S bind to the track's mixer strip, ROLL switches
//! the lane view, the arrows reorder, Ctrl+T shows the tab, and the whole sequencer stays
//! visible at 1280 x 1140 with the tab showing and eight tracks listed.

use app::app::{LaneView, RightTab};
use app::ui;
use app::ui::tracks;
use app::{AppState, BeatSlopperApp};
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use neu::{Theme, ThemeMode};

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

/// The whole app at the design size with the TRACKS tab showing and `extra` tracks added.
fn tracks_app_harness(extra: usize) -> Harness<'static, BeatSlopperApp> {
    let (mut app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    for _ in 0..extra {
        app.state_mut().add_track();
    }
    // The set-up tracks are not edits the tests care about.
    app.state_mut().undo.clear();
    app.state_mut().dirty = false;
    app.state_mut().right_tab = RightTab::Tracks;
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
    harness
}

fn type_text(harness: &mut Harness<'_, BeatSlopperApp>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run_steps(2);
}

/// The panel alone lists one row per active track with the right names, in both themes,
/// without editing anything.
#[test]
fn tracks_panel_lists_one_row_per_active_track_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (mut state, _engine) = AppState::headless(mode);
        state.add_track();
        state.add_track();
        state.rename_track(2, "Sixteen chars ok");
        state.right_tab = RightTab::Tracks;
        state.undo.clear();
        state.dirty = false;
        let theme = Theme::for_mode(mode);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(ui::RIGHT_COLUMN_WIDTH, 400.0))
            .build_ui_state(
                move |ui, state: &mut AppState| ui::tracks::show(ui, state, &theme),
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
        harness.get_by_label(&tracks::swatch_name("Synth"));
        harness.get_by_label(&tracks::rename_name("Track 2"));
        harness.get_by_label(&tracks::mute_name("Sixteen chars ok"));
        harness.get_by_label(&tracks::solo_name("Synth"));
        harness.get_by_label(&tracks::roll_name("Synth"));
        harness.get_by_label(tracks::ADD_NAME);
        harness.get_by_label(tracks::DUP_LABEL);
        harness.get_by_label(tracks::DEL_LABEL);
        harness.get_by_label(tracks::MOVE_UP_NAME);
        harness.get_by_label(tracks::MOVE_DOWN_NAME);
        harness.get_by_label(tracks::HINT);
        // The selected row is track 3 (the last added); the others are not.
        assert_eq!(
            harness
                .get_by_label(&tracks::row_name(2, "Sixteen chars ok"))
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        assert_eq!(
            harness
                .get_by_label(&tracks::row_name(0, "Synth"))
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::False)
        );
        // The four-item tab fits the column's inner width.
        let tab = harness
            .get_by_label(&ui::controls::RIGHT_TABS.join(" / "))
            .rect();
        assert!(tab.width() <= ui::RIGHT_COLUMN_WIDTH - 40.0, "{tab:?}");
        let state = harness.state();
        assert!(!state.dirty, "rendering edits nothing");
        assert_eq!(state.active_track_count(), 3);
        assert_eq!(state.edit_track, 2);
    }
}

/// A click on a row selects its track: `edit_track` changes and the SYNTH head follows.
#[test]
fn tracks_panel_row_click_selects_and_the_synth_head_follows() {
    let mut harness = tracks_app_harness(1);
    assert_eq!(harness.state().state().edit_track, 1);
    harness.get_by_label("Synth · Track 2");

    harness.get_by_label(&tracks::row_name(0, "Synth")).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 0);
    harness.get_by_label("Synth · Synth");
    assert_eq!(
        harness
            .get_by_label(&tracks::row_name(0, "Synth"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    assert!(!harness.state().state().dirty, "selecting is not an edit");
}

/// ADD adds and selects, DUP duplicates as "<name> copy", DEL asks: NO keeps, YES removes
/// and selects the nearest track; with one track DEL is disabled.
#[test]
fn tracks_panel_add_dup_and_del_with_the_inline_confirm() {
    let mut harness = tracks_app_harness(0);
    assert!(
        harness
            .get_by_label(tracks::DEL_LABEL)
            .accesskit_node()
            .is_disabled()
    );

    harness.get_by_label(tracks::ADD_NAME).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    assert_eq!(harness.state().state().edit_track, 1);
    harness.get_by_label(&tracks::row_name(1, "Track 2"));
    harness.get_by_label("Synth · Track 2");
    assert!(harness.state().state().dirty);

    harness.get_by_label(tracks::DUP_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 3);
    assert_eq!(harness.state().state().edit_track, 2);
    assert_eq!(harness.state().state().track(2).name, "Track 2 copy");
    harness.get_by_label(&tracks::row_name(2, "Track 2 copy"));

    // DEL asks; NO keeps the track and brings the pills back.
    assert!(
        !harness
            .get_by_label(tracks::DEL_LABEL)
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label(tracks::DEL_LABEL).click();
    harness.run_steps(2);
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
    harness.get_by_label(tracks::DEL_LABEL).click();
    harness.run_steps(2);
    harness.get_by_label(tracks::YES_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 2);
    assert_eq!(harness.state().state().edit_track, 1);
    assert!(
        harness
            .query_by_label(&tracks::row_name(2, "Track 2 copy"))
            .is_none()
    );
    harness.get_by_label("Synth · Track 2");

    // Down to one track: DEL greys out and a click does nothing.
    harness.get_by_label(tracks::DEL_LABEL).click();
    harness.run_steps(2);
    harness.get_by_label(tracks::YES_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 1);
    assert_eq!(harness.state().state().edit_track, 0);
    assert!(
        harness
            .get_by_label(tracks::DEL_LABEL)
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label(tracks::DEL_LABEL).click();
    harness.run_steps(2);
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

/// The pencil opens the inline rename editor: shortcuts and the piano stay quiet while it
/// has the keyboard, Enter commits the typed name, Escape cancels.
#[test]
fn tracks_panel_rename_commits_on_enter_and_cancels_on_escape() {
    let mut harness = tracks_app_harness(0);
    harness.get_by_label(&tracks::rename_name("Synth")).click();
    harness.run_steps(2);
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
    harness.get_by_label("Synth · Lead");
    assert!(harness.state().state().dirty);

    // Escape cancels.
    harness.get_by_label(&tracks::rename_name("Lead")).click();
    harness.run_steps(2);
    type_text(&mut harness, "Nope");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state().state().track(0).name, "Lead");
    harness.get_by_label(&tracks::row_name(0, "Lead"));
    // After the editor closed the shortcuts work again.
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(harness.state().state().playing);
}

/// M / S toggle the track's mixer strip; ROLL selects the track and shows the piano roll.
#[test]
fn tracks_panel_m_s_and_roll_bind_to_the_track() {
    let mut harness = tracks_app_harness(1);
    let synth = engine::Channel::from_track(0);
    let two = engine::Channel::from_track(1);
    harness.get_by_label(&tracks::mute_name("Synth")).click();
    harness.run_steps(2);
    assert!(harness.state().state().mixer.strip(synth).mute);
    assert!(!harness.state().state().mixer.strip(two).mute);
    assert_eq!(
        harness
            .get_by_label(&tracks::mute_name("Synth"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    harness.get_by_label(&tracks::solo_name("Track 2")).click();
    harness.run_steps(2);
    assert!(harness.state().state().mixer.strip(two).solo);
    assert!(!harness.state().state().mixer.strip(synth).solo);
    // Clicking M did not select the track.
    assert_eq!(harness.state().state().edit_track, 1);
    harness.get_by_label(&tracks::mute_name("Synth")).click();
    harness.run_steps(2);
    assert!(!harness.state().state().mixer.strip(synth).mute);

    assert_eq!(harness.state().state().lane_view, LaneView::Grid);
    harness.get_by_label(&tracks::roll_name("Synth")).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().edit_track, 0);
    assert_eq!(harness.state().state().lane_view, LaneView::Roll);
    assert_eq!(
        harness
            .get_by_label(&tracks::roll_name("Synth"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    assert_eq!(
        harness
            .get_by_label(&tracks::roll_name("Track 2"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False)
    );
}

/// The arrows move the selected track (the selection follows it) and the chip cycles
/// the colour.
#[test]
fn tracks_panel_arrows_reorder_and_the_chip_cycles_the_colour() {
    let mut harness = tracks_app_harness(1);
    assert_eq!(harness.state().state().edit_track, 1);
    assert!(
        harness
            .get_by_label(tracks::MOVE_DOWN_NAME)
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label(tracks::MOVE_UP_NAME).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().track(0).name, "Track 2");
    assert_eq!(harness.state().state().track(1).name, "Synth");
    assert_eq!(harness.state().state().edit_track, 0);
    harness.get_by_label(&tracks::row_name(0, "Track 2"));
    harness.get_by_label(&tracks::row_name(1, "Synth"));
    assert!(
        harness
            .get_by_label(tracks::MOVE_UP_NAME)
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label(tracks::MOVE_DOWN_NAME).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().track(0).name, "Synth");
    assert_eq!(harness.state().state().edit_track, 1);

    let before = harness.state().state().track(0).colour;
    harness.get_by_label(&tracks::swatch_name("Synth")).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().track(0).colour, (before + 1) % 8);
    assert_eq!(
        harness.state().state().edit_track,
        1,
        "the chip does not select"
    );
}

/// Ctrl+T adds a track and shows the TRACKS tab so the new row is visible.
#[test]
fn ctrl_t_shows_the_tracks_tab() {
    let mut harness = app_harness_sized(egui::Vec2::new(1280.0, 1140.0));
    harness.run_steps(2);
    assert_eq!(harness.state().state().right_tab, RightTab::Project);
    assert!(harness.query_by_label(tracks::ADD_NAME).is_none());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::T);
    harness.run_steps(2);
    assert_eq!(harness.state().state().right_tab, RightTab::Tracks);
    harness.get_by_label(&tracks::row_name(1, "Track 2"));
    harness.get_by_label(tracks::ADD_NAME);
}

/// With the TRACKS tab showing and all eight tracks active, the whole sequencer is still
/// visible at the design size; the list scrolls inside the panel and the pills stay put.
#[test]
fn whole_sequencer_is_visible_with_the_tracks_tab_shown() {
    let mut harness = tracks_app_harness(7);
    assert_eq!(harness.state().state().active_track_count(), 8);
    let bass = harness.get_by_label("Bass step 16").rect();
    let octave = harness.get_by_label("Octave").rect();
    let sequencer_bottom = bass.max.y + ui::sequencer::ROW_GAP + 10.0 + 20.0;
    let keys_panel_top = octave.min.y - 30.0;
    assert!(
        sequencer_bottom <= keys_panel_top,
        "sequencer ends at {sequencer_bottom} but the KEYS panel starts at {keys_panel_top}"
    );
    // The action pills and the hint sit under the list, above the sequencer's bottom.
    let add = harness.get_by_label(tracks::ADD_NAME).rect();
    let hint = harness.get_by_label(tracks::HINT).rect();
    assert!(
        hint.max.y <= sequencer_bottom,
        "{hint:?} vs {sequencer_bottom}"
    );
    assert!(add.max.y < hint.min.y);
    let first = harness.get_by_label(&tracks::row_name(0, "Synth")).rect();
    assert!(first.max.y <= add.min.y);
    // The list holds every track (scrolled-away rows still exist).
    harness.get_by_label(&tracks::row_name(7, "Track 8"));
    // The sequencer head's track tag no longer pushes the column off the window: every
    // row control, the ROLL pill at the far right included, is inside the page padding.
    let roll = harness.get_by_label(&tracks::roll_name("Synth")).rect();
    assert!(roll.max.x <= 1280.0 - 20.0, "{roll:?}");
    let rec = harness.get_by_label("Rec").rect();
    assert!(rec.max.x < roll.min.x - 40.0, "{rec:?} vs {roll:?}");
    // ADD refuses a ninth track and keeps the eight.
    harness.get_by_label(tracks::ADD_NAME).click();
    harness.run_steps(2);
    assert_eq!(harness.state().state().active_track_count(), 8);
}
