//! egui_kittest: the M12 Deck shell's layout (`design/round3/WINNER.md` section 5). At
//! 840 x 600, 1280 x 800 and 1920 x 1080 logical points the toolbar fits its 40 pt bar, no
//! element lies outside the window, the rack's sixteen steps are visible at 1280 and up and
//! scroll horizontally at 840 (the steps scroll area measures wider than its viewport), the
//! F-keys switch the views, the browser collapses to the rail and back, the keyboard strip
//! collapses, the mixer bar opens the mixer view and the hint bar shows the shortcuts.

use app::app::View;
use app::ui;
use app::ui::shell::{AUDIO_INFO_NAME, HINTS, MIXER_BAR_NAME, MIXER_OPEN_NAME, RACK_HANDLE_NAME};
use app::ui::transport_bar::{BROWSER_NAME, HINT_NAME, MENU_NAME, MIDI_NAME};
use app::{AppState, BeatSlopperApp};
use egui::accesskit::Role;
use egui::{Key, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use neu::ThemeMode;

/// The three sizes the layout must work at.
const SIZES: [Vec2; 3] = [
    Vec2::new(840.0, 600.0),
    Vec2::new(1280.0, 800.0),
    Vec2::new(1920.0, 1080.0),
];

/// The whole app at `size`.
fn app_harness(size: Vec2) -> Harness<'static, BeatSlopperApp> {
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut harness = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut BeatSlopperApp| {
            app.tick(ui.ctx());
            app.show(ui);
        },
        app,
    );
    harness.run_steps(3);
    harness
}

fn state<'h>(harness: &'h Harness<'_, BeatSlopperApp>) -> &'h AppState {
    harness.state().state()
}

/// Every node with a rect, as `(label, role, rect)`. Entries of the scrolling lists (the
/// roll's CHORDS column, the preset list, the mixer's insert strips, the recent list) are
/// laid out past their viewport and clipped, so they are left out: egui registers them with
/// their unclipped rects.
fn all_rects(harness: &Harness<'_, BeatSlopperApp>) -> Vec<(String, Role, egui::Rect)> {
    let chords: Vec<&str> = app::chords::CHORDS.iter().map(|c| c.name).collect();
    let presets: Vec<&str> = app::presets::all().map(|p| p.name).collect();
    harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let n = node.accesskit_node();
            let bounds = n.bounding_box()?;
            let rect = egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                return None;
            }
            let label = n.label().map(|l| l.to_string()).unwrap_or_default();
            let scrolled_list = chords.contains(&label.as_str())
                || presets.contains(&label.as_str())
                || label.starts_with("Insert ")
                || label.starts_with("Mute insert")
                || label.starts_with("Solo insert")
                || label.starts_with("Route of insert")
                || label.starts_with("Rename insert")
                || label.starts_with("Recent ")
                || label == "Level"
                || label == "Gain reduction";
            (!scrolled_list).then_some((label, n.role(), rect))
        })
        .collect()
}

/// The toolbar's controls, from the project menu to the theme switch.
fn toolbar_labels() -> Vec<&'static str> {
    vec![
        MENU_NAME,
        "Play",
        "Stop",
        "Record",
        "Decrease",
        ui::tempo::BPM_VALUE_NAME,
        "Increase",
        ui::transport_bar::MASTER_NAME,
        HINT_NAME,
        MIDI_NAME,
        View::Roll.button_name(),
        View::Mixer.button_name(),
        View::Synth.button_name(),
        View::Song.button_name(),
        View::Visual.button_name(),
        BROWSER_NAME,
        "Dark theme",
    ]
}

#[test]
fn the_toolbar_fits_its_bar_at_every_size() {
    for size in SIZES {
        let harness = app_harness(size);
        // The bar is 40 tall around its 28 pt controls (the harness adds its own 8 pt
        // margin around the shell, so the band is read off the PLAY button).
        let play = harness.get_by_label("Play").rect();
        let bar_top = play.min.y - 6.0;
        let bar_bottom = play.max.y + 6.0;
        let mut last_right = 0.0f32;
        for label in toolbar_labels() {
            let rect = harness
                .get_all_by_label(label)
                .map(|n| n.rect())
                .find(|r| r.min.y >= bar_top - 0.5 && r.max.y <= bar_bottom + 0.5)
                .unwrap_or_else(|| panic!("{size:?}: {label} in the toolbar"));
            assert!(
                rect.min.x >= 0.0 && rect.max.x <= size.x + 0.5,
                "{size:?}: {label} at {rect:?} inside the bar"
            );
            assert!(
                rect.min.x >= last_right - 0.5,
                "{size:?}: {label} at {rect:?} follows the previous control ({last_right})"
            );
            last_right = rect.max.x;
        }
        // Wide: undo / redo, TAP, swing, the meter and the port name; narrow: dropped.
        let narrow = app::window::is_narrow(size.x);
        for label in [
            "Undo",
            "Redo",
            ui::tempo::TAP_LABEL,
            "Swing",
            ui::transport_bar::MASTER_METER_NAME,
        ] {
            assert_eq!(
                harness.query_all_by_label(label).count() > 0,
                !narrow,
                "{size:?}: {label}"
            );
        }
        let mode = if narrow { "Pat" } else { "Pattern" };
        harness.get_by_role_and_label(Role::RadioButton, mode);
        harness.get_by_role_and_label(Role::RadioButton, "Song");
    }
}

#[test]
fn nothing_lies_outside_the_window_at_every_size() {
    for size in SIZES {
        let harness = app_harness(size);
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let outside: Vec<_> = all_rects(&harness)
            .into_iter()
            .filter(|(_, _, r)| {
                r.min.x < -0.5 || r.min.y < -0.5 || r.max.x > size.x + 0.5 || r.max.y > size.y + 0.5
            })
            .collect();
        assert!(
            outside.is_empty(),
            "{size:?}: outside {window:?}: {outside:#?}"
        );
        // The frame's docks sit where WINNER puts them: the toolbar, then the body, the mixer
        // bar, the keyboard and the hint bar, 4 apart.
        let mixer_bar = harness.get_by_label(MIXER_BAR_NAME).rect();
        let hint = harness.get_by_label(&hint_bar_name()).rect();
        let audio = harness.get_by_label(AUDIO_INFO_NAME).rect();
        assert_eq!(mixer_bar.height(), ui::shell::MIXER_BAR);
        assert!(
            hint.max.y <= size.y - ui::shell::FRAME_PAD + 0.5,
            "{hint:?}"
        );
        assert!(
            audio.max.x <= size.x - ui::shell::FRAME_PAD + 0.5,
            "{audio:?}"
        );
        assert!(
            audio.min.x >= hint.max.x - 0.5,
            "the audio line is right of the hints"
        );
        assert!(mixer_bar.max.y < hint.min.y);
    }
}

/// The hint bar's accessible name: the keycap · label pairs joined.
fn hint_bar_name() -> String {
    HINTS
        .iter()
        .map(|(k, l)| format!("{k} {l}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[test]
fn the_hint_bar_shows_the_shortcuts() {
    let harness = app_harness(Vec2::new(1280.0, 800.0));
    let name = hint_bar_name();
    assert!(name.contains("Space Play / Stop") && name.contains("F5..F9 Views"));
    let hint = harness.get_by_label(&name).rect();
    assert_eq!(hint.height(), ui::shell::HINT_BAR);
    // The toolbar's hint panel shows a status line while one is showing.
    let mut harness = harness;
    harness
        .state_mut()
        .state_mut()
        .set_status("Exported song (8.0 s) to song.wav");
    harness.run_steps(2);
    harness.get_by_label(HINT_NAME);
    assert_eq!(
        state(&harness).status_text(),
        Some("Exported song (8.0 s) to song.wav")
    );
}

#[test]
fn the_rack_shows_sixteen_steps_wide_and_scrolls_them_narrow() {
    for size in SIZES {
        let harness = app_harness(size);
        let first = harness.get_by_label("Kick step 1").rect();
        let last = harness.get_by_label("Kick step 16").rect();
        let scroll = ui::sequencer::steps_scroll(&harness.ctx).expect("the rack was drawn");
        assert_eq!(
            last.min.x - first.min.x,
            ui::sequencer::cell_x(15),
            "{size:?}: the Deck cell maths"
        );
        if app::window::is_narrow(size.x) {
            // 400 wide: no knob columns, a 210 viewport, the steps scroll.
            assert!(scroll.scrolls(), "{size:?}: {scroll:?}");
            assert_eq!(scroll.viewport, ui::sequencer::STEPS_VIEWPORT_COMPACT);
            assert!(
                harness
                    .query_by_label(&ui::tracks::level_name("Synth"))
                    .is_none()
            );
            assert!(
                last.max.x > first.min.x + scroll.viewport,
                "{size:?}: step 16 lies past the viewport ({last:?})"
            );
        } else {
            assert!(!scroll.scrolls(), "{size:?}: {scroll:?}");
            assert!(scroll.viewport >= ui::sequencer::GRID_WIDTH);
            harness.get_by_label(&ui::tracks::level_name("Synth"));
            harness.get_by_label(&ui::tracks::pan_name("Synth"));
            harness.get_by_label("Kick volume");
            harness.get_by_label("Kick tune");
            let handle = harness.get_by_label(RACK_HANDLE_NAME).rect();
            assert!(
                handle.min.x >= last.max.x,
                "the splitter is right of the steps"
            );
            // The rack is its default width: the handle sits 664 points after the rack's
            // left edge (the row's LED sits 8 + 1 inside it).
            let led = harness.get_by_label(&ui::tracks::led_name("Kick")).rect();
            let rack_left = led.min.x - ui::sequencer::ROW_PAD - 1.0;
            assert!(
                (handle.min.x - (rack_left + app::app::DEFAULT_RACK_WIDTH)).abs() <= 0.5,
                "{size:?}: handle at {handle:?}, rack left {rack_left}"
            );
        }
        // Every row control is there in both layouts.
        for label in [
            "Kick",
            "Snare",
            "Clap",
            "C.Hat",
            "O.Hat",
            &ui::tracks::row_name(0, "Synth"),
            &ui::tracks::led_name("Kick"),
            &ui::tracks::drum_solo_name("Kick"),
            &ui::sequencer::row_box_name("Kick"),
            &ui::tracks::insert_box_name("Synth"),
            ui::tracks::ADD_NAME,
            "Synth step 16",
            ui::sequencer::FOLLOW_LABEL,
        ] {
            harness.get_by_label(label);
        }
        // Wide: the pattern tiles; compact: the pattern and length dropdowns.
        if app::window::is_narrow(size.x) {
            harness.get_by_label(ui::sequencer::PATTERN_DROPDOWN);
            harness.get_by_label(ui::sequencer::LENGTH_DROPDOWN);
            assert!(harness.query_by_label("Pattern A").is_none());
        } else {
            harness.get_by_label("Pattern A");
            harness.get_by_label("Pattern H");
            harness.get_by_role_and_label(Role::RadioButton, "1 bar");
        }
    }
}

#[test]
fn the_f_keys_switch_the_views_and_the_tab_strip_follows() {
    let mut harness = app_harness(Vec2::new(1280.0, 800.0));
    assert_eq!(state(&harness).view, View::Roll);
    harness.get_by_label("Piano roll");
    let tab = |harness: &Harness<'_, BeatSlopperApp>, view: View| {
        harness
            .get_by_role_and_label(Role::Tab, view.tab_label())
            .accesskit_node()
            .toggled()
    };
    assert_eq!(
        tab(&harness, View::Roll),
        Some(egui::accesskit::Toggled::True)
    );

    harness.key_press(Key::F7);
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Synth);
    assert!(!state(&harness).rack_open, "SYNTH & FX collapses the rack");
    harness.get_by_label("Osc 1");
    harness.get_by_label("Drive");
    assert!(harness.query_by_label("Piano roll").is_none());
    assert_eq!(
        tab(&harness, View::Synth),
        Some(egui::accesskit::Toggled::True)
    );
    // The collapsed rack keeps its header (expand button) and hides its rows.
    harness.get_by_label(ui::sequencer::EXPAND_LABEL);
    assert!(harness.query_by_label("Kick step 1").is_none());

    harness.key_press(Key::F8);
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Song);
    harness.get_by_label("Repeats");

    harness.key_press(Key::F6);
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Mixer);
    assert!(state(&harness).mixer_drawer_open);
    harness.get_by_label("Mute Kick");
    harness.key_press(Key::F6);
    harness.run_steps(2);
    assert_eq!(
        state(&harness).view,
        View::Song,
        "F6 again returns to the view it replaced"
    );

    harness.key_press(Key::F5);
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Roll);
    assert!(state(&harness).rack_open, "F5 re-expands the rack");
    harness.get_by_label("Piano roll");
    harness.get_by_label("Kick step 1");

    // The tab strip and the toolbar buttons switch too.
    harness
        .get_by_role_and_label(Role::Tab, View::Visual.tab_label())
        .click();
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Visual);
    harness.get_by_label(ui::visualizer::FULL_LABEL);
    harness.get_by_label(View::Synth.button_name()).click();
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Synth);
    assert!(!state(&harness).dirty, "views are not edits");
    assert!(state(&harness).undo.is_empty());
}

#[test]
fn the_browser_collapses_to_the_rail_and_back() {
    let mut harness = app_harness(Vec2::new(1280.0, 800.0));
    assert!(state(&harness).browser_open);
    harness.get_by_label(ui::preset_browser::BROWSE_LABEL);
    harness.get_by_label(ui::browser::PROJECT_LABEL);
    harness.get_by_label("New");
    let rack_before = harness.get_by_label("Kick step 1").rect();
    harness.get_by_label(ui::browser::COLLAPSE_LABEL).click();
    harness.run_steps(2);
    assert!(!state(&harness).browser_open);
    harness.get_by_label(ui::browser::RAIL_EXPAND);
    harness.get_by_label(ui::browser::RAIL_PRESETS);
    assert!(harness.query_by_label("New").is_none());
    let rack_after = harness.get_by_label("Kick step 1").rect();
    assert_eq!(
        rack_before.min.x - rack_after.min.x,
        ui::browser::WIDTH - ui::browser::RAIL_WIDTH,
        "the rack moves left by the column's width"
    );
    // F9 and the toolbar button bring it back.
    harness.key_press(Key::F9);
    harness.run_steps(2);
    assert!(state(&harness).browser_open);
    harness.get_by_label("New");
    harness.get_by_label(BROWSER_NAME).click();
    harness.run_steps(2);
    assert!(!state(&harness).browser_open);
    // A rail button opens the column on its section.
    harness.get_by_label(ui::browser::RAIL_RECENT).click();
    harness.run_steps(2);
    assert!(state(&harness).browser_open && state(&harness).recent_open);
    harness.get_by_label(ui::recent_panel::EMPTY_LABEL);
}

#[test]
fn the_keyboard_strip_collapses_and_expands() {
    let mut harness = app_harness(Vec2::new(1280.0, 800.0));
    assert!(state(&harness).keyboard_open);
    let octave = harness.get_by_label(ui::keyboard::OCTAVE_LABEL).rect();
    assert_eq!(
        harness
            .get_by_label(ui::keyboard::OCTAVE_LABEL)
            .value()
            .as_deref(),
        Some("4")
    );
    let keys_open = harness.get_by_label("Piano keys").rect();
    assert!(keys_open.min.y > octave.max.y);
    harness.get_by_label(ui::keyboard::COLLAPSE_LABEL).click();
    harness.run_steps(2);
    assert!(!state(&harness).keyboard_open);
    assert!(harness.query_by_label("Piano keys").is_none());
    harness.get_by_label(ui::keyboard::EXPAND_LABEL).click();
    harness.run_steps(2);
    assert!(state(&harness).keyboard_open);
    harness.get_by_label("Piano keys");
    // The octave buttons still work from the header.
    harness.get_by_label(ui::keyboard::OCTAVE_UP).click();
    harness.run_steps(2);
    assert_eq!(state(&harness).input.octave, 5);
    harness.get_by_label(ui::keyboard::OCTAVE_DOWN).click();
    harness.run_steps(2);
    assert_eq!(state(&harness).input.octave, 4);
}

#[test]
fn the_mixer_bar_opens_the_mixer_view() {
    let mut harness = app_harness(Vec2::new(1280.0, 800.0));
    assert!(harness.query_by_label("Mute Kick").is_none());
    harness.get_by_label(MIXER_OPEN_NAME).click();
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Mixer);
    harness.get_by_label("Mute Kick");
    harness.get_by_label(ui::shell::MIXER_CLOSE_NAME).click();
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Roll);
    // A click on the bar itself opens it too.
    harness.get_by_label(MIXER_BAR_NAME).click();
    harness.run_steps(2);
    assert_eq!(state(&harness).view, View::Mixer);
    harness.get_by_label(ui::shell::MIXER_METER_NAME);
}

#[test]
fn the_narrow_layout_keeps_everything_reachable() {
    // The 150 % case: 1280 physical pixels = 853 logical points.
    let mut harness = app_harness(Vec2::new(853.0, 600.0));
    assert!(!state(&harness).browser_open, "narrow: the rail");
    assert!(!state(&harness).keyboard_open, "narrow: the keyboard bar");
    // The rack toolbar's compact controls.
    harness.get_by_label(ui::sequencer::PATTERN_DROPDOWN);
    harness.get_by_label(ui::sequencer::LENGTH_DROPDOWN);
    harness.get_by_label(ui::sequencer::MORE_LABEL);
    // The presets open from the rail and the list is inside the window.
    harness.get_by_label(ui::browser::RAIL_PRESETS).click();
    harness.run_steps(3);
    assert!(state(&harness).browser_open && state(&harness).preset_browser_open);
    let entry = harness
        .get_by_role_and_label(Role::Button, "Lofi Piano")
        .rect();
    assert!(entry.min.x >= 0.0 && entry.max.x <= 853.0, "{entry:?}");
    // The keyboard opens over the body and its keys are reachable.
    harness.get_by_label(ui::keyboard::EXPAND_LABEL).click();
    harness.run_steps(3);
    assert!(state(&harness).keyboard_open);
    let keys = harness.get_by_label("Piano keys").rect();
    assert!(
        keys.max.y <= 600.0 && keys.min.x >= 0.0 && keys.max.x <= 853.0,
        "{keys:?}"
    );
    // Back to the bars, then every view: the shell's own controls stay inside the window
    // (the rehosted panels scroll inside their windows; their contents are M13 / M14).
    harness.key_press(Key::Escape);
    harness.get_by_label(ui::keyboard::COLLAPSE_LABEL).click();
    harness.get_by_label(ui::browser::COLLAPSE_LABEL).click();
    harness.run_steps(2);
    assert!(!state(&harness).keyboard_open && !state(&harness).browser_open);
    let shell_controls = [
        MENU_NAME,
        "Play",
        "Dark theme",
        ui::browser::RAIL_EXPAND,
        MIXER_BAR_NAME,
        ui::keyboard::EXPAND_LABEL,
        AUDIO_INFO_NAME,
    ];
    for key in [Key::F6, Key::F7, Key::F8, Key::F5] {
        harness.key_press(key);
        harness.run_steps(2);
        for label in shell_controls {
            let rect = harness.get_by_label(label).rect();
            assert!(
                rect.min.x >= 0.0
                    && rect.max.x <= 853.5
                    && rect.min.y >= 0.0
                    && rect.max.y <= 600.5,
                "{key:?}: {label} at {rect:?}"
            );
        }
        for view in View::ALL {
            let tab = harness
                .get_by_role_and_label(Role::Tab, view.tab_label())
                .rect();
            assert!(
                tab.max.x <= 853.5 && tab.min.x >= 0.0,
                "{key:?}: {view:?} tab at {tab:?}"
            );
        }
    }
    // In each view the first control of the rehosted panel is in reach.
    harness.key_press(Key::F6);
    harness.run_steps(2);
    let mute = harness.get_by_label("Mute Kick").rect();
    assert!(mute.max.x <= 853.5 && mute.max.y <= 600.5, "{mute:?}");
    harness.key_press(Key::F7);
    harness.run_steps(2);
    let osc = harness.get_by_label("Osc 1").rect();
    assert!(osc.max.x <= 853.5 && osc.max.y <= 600.5, "{osc:?}");
    harness.key_press(Key::F8);
    harness.run_steps(2);
    let add = harness.get_by_label("Add").rect();
    assert!(add.max.x <= 853.5 && add.max.y <= 600.5, "{add:?}");
    harness.key_press(Key::F5);
    harness.run_steps(2);
    let roll = harness.get_by_label("Piano roll").rect();
    assert!(roll.min.x >= 0.0 && roll.max.y <= 600.5, "{roll:?}");
}

#[test]
fn the_rack_handle_trades_width_with_the_main_window() {
    let mut harness = app_harness(Vec2::new(1920.0, 1080.0));
    let before = harness.get_by_label(RACK_HANDLE_NAME).rect();
    let roll_before = harness.get_by_label("Piano roll").rect();
    let from = before.center();
    let to = from + egui::vec2(120.0, 0.0);
    harness.event(egui::Event::PointerMoved(from));
    harness.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    harness.event(egui::Event::PointerMoved(to));
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    assert_eq!(
        state(&harness).rack_width,
        app::app::DEFAULT_RACK_WIDTH + 120.0
    );
    let after = harness.get_by_label(RACK_HANDLE_NAME).rect();
    assert_eq!(after.min.x - before.min.x, 120.0);
    let roll_after = harness.get_by_label("Piano roll").rect();
    assert!(roll_after.min.x > roll_before.min.x);
    assert!(!state(&harness).dirty, "a split is view state");
    // The layout persists through the storage round trip.
    let layout = state(&harness).layout();
    assert_eq!(layout.rack_width, app::app::DEFAULT_RACK_WIDTH + 120.0);
}

#[test]
fn the_overlay_covers_the_workspace_until_escape() {
    let mut harness = app_harness(Vec2::new(1280.0, 800.0));
    harness.key_press(Key::F11);
    harness.run_steps(3);
    assert!(state(&harness).visual_full);
    let close = harness.get_by_label(ui::visualizer::CLOSE_LABEL).rect();
    let workspace = state(&harness).workspace_rect.expect("measured");
    assert!(workspace.contains_rect(close), "{close:?} in {workspace:?}");
    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!state(&harness).visual_full);
    assert!(
        harness
            .query_by_label(ui::visualizer::CLOSE_LABEL)
            .is_none()
    );
}
