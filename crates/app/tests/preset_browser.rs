//! egui_kittest: the M9 preset browser as the BROWSER column's PRESETS section (M12). The
//! list shows every preset and filters by category chip, clicking an entry applies it to the
//! selected track, the search field narrows the list and Escape closes the section from
//! inside the field, prev / next wrap within the category and the MODIFIED LED clears on a
//! new preset, the shortcuts step presets and open the section with the search focused,
//! and the open section leaves the rack whole and its chips inside the column.

use app::ui::preset_browser::{
    BROWSE_LABEL, CATEGORY_LABELS, CLOSE_LABEL, NEXT_LABEL, PREV_LABEL, head_name,
};
use app::ui::synth_panel::MODIFIED_LABEL;
use app::{AppState, BeatSlopperApp, presets, ui};
use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use engine::ParamId;
use neu::{Theme, ThemeMode};

/// The BROWSER column alone (208 x 900) with the PRESETS section open.
fn browser_harness(mode: ThemeMode) -> Harness<'static, AppState> {
    let (mut state, _engine) = AppState::headless(mode);
    state.open_preset_browser(false);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(ui::browser::WIDTH + 16.0, 900.0))
        .build_ui_state(
            move |ui, state: &mut AppState| ui::browser::show(ui, state, &theme),
            state,
        );
    harness.run_steps(3);
    harness
}

/// The whole app (shortcuts and the piano through `tick`) at the design size.
fn app_harness() -> Harness<'static, BeatSlopperApp> {
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
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

/// The list entries shown: every button named after a preset (inside `within`, when
/// given).
fn shown_entries_within<S>(harness: &Harness<'_, S>, within: Option<egui::Rect>) -> Vec<String> {
    let names: Vec<&str> = presets::all().map(|p| p.name).collect();
    harness
        .get_all_by_role(Role::Button)
        .filter(|node| within.is_none_or(|area| area.contains_rect(node.rect())))
        .filter_map(|node| node.accesskit_node().label().map(|label| label.to_string()))
        .filter(|label| names.contains(&label.as_str()))
        .collect()
}

/// [`shown_entries_within`] over the whole harness (the column alone).
fn shown_entries<S>(harness: &Harness<'_, S>) -> Vec<String> {
    shown_entries_within(harness, None)
}

/// A category chip (a toggle button named `label`).
fn chip<'h, S>(harness: &'h Harness<'_, S>, label: &'h str) -> egui_kittest::Node<'h> {
    harness.get_by_role_and_label(Role::Button, label)
}

fn entry<'h, S>(harness: &'h Harness<'_, S>, name: &'h str) -> egui_kittest::Node<'h> {
    harness.get_by_role_and_label(Role::Button, name)
}

fn modified<S>(harness: &Harness<'_, S>) -> Option<Toggled> {
    harness
        .get_by_label(MODIFIED_LABEL)
        .accesskit_node()
        .toggled()
}

#[test]
fn browser_lists_every_preset_and_filters_by_category_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut harness = browser_harness(mode);
        harness.get_by_label(CLOSE_LABEL);
        harness.get_by_label(&head_name("Bass"));
        // `All`: the whole unified list, in order, the current preset toggled.
        let shown = shown_entries(&harness);
        assert_eq!(shown.len(), presets::count(), "{mode:?}");
        let expected: Vec<&str> = presets::all().map(|p| p.name).collect();
        assert_eq!(shown, expected, "{mode:?}: unified order");
        assert_eq!(
            entry(&harness, "Bass").accesskit_node().toggled(),
            Some(Toggled::True)
        );
        assert_eq!(
            entry(&harness, "Lofi Piano").accesskit_node().toggled(),
            Some(Toggled::False)
        );
        harness.get_by_label(&ui::preset_browser::count_text(
            presets::count(),
            presets::count(),
        ));
        // Every chip sits inside the column.
        for label in CATEGORY_LABELS {
            let rect = chip(&harness, label).rect();
            assert!(
                rect.max.x <= ui::browser::WIDTH + 16.0 && rect.min.x >= 0.0,
                "{label} at {rect:?}"
            );
        }

        // Each category: the entries are exactly the category's presets (the engine's
        // `by_category` plus the classics of that category).
        for (i, category) in presets::Category::ALL.iter().enumerate() {
            chip(&harness, CATEGORY_LABELS[i + 1]).click();
            harness.run_steps(2);
            assert_eq!(harness.state().preset_category, Some(*category));
            let shown = shown_entries(&harness);
            let expected: Vec<&str> = presets::by_category(Some(*category))
                .map(presets::name)
                .collect();
            assert_eq!(shown, expected, "{mode:?}: {}", category.label());
            let library = engine::presets::by_category(*category).count();
            let classics = presets::CLASSIC
                .iter()
                .filter(|p| p.category == *category)
                .count();
            assert_eq!(shown.len(), library + classics);
            assert!(shown.len() >= 10);
        }
        chip(&harness, ui::preset_browser::ALL_LABEL).click();
        harness.run_steps(2);
        assert_eq!(harness.state().preset_category, None);
        assert_eq!(shown_entries(&harness).len(), presets::count());
        assert_eq!(
            harness.state().preset_index(),
            0,
            "filtering applies nothing"
        );
        assert!(!harness.state().dirty);
    }
}

#[test]
fn clicking_an_entry_applies_it_to_the_selected_track_and_the_current_row_follows() {
    let mut harness = browser_harness(ThemeMode::Dark);
    harness.state_mut().add_track();
    harness.run_steps(2);
    assert_eq!(harness.state().edit_track, 1);
    entry(&harness, "Lofi Piano").click();
    harness.run_steps(2);
    assert_eq!(harness.state().preset_name(), "Lofi Piano");
    assert_eq!(harness.state().track(0).preset, 0, "track 1 keeps Bass");
    harness.get_by_label(&head_name("Lofi Piano"));
    assert!(harness.query_by_label(&head_name("Bass")).is_none());
    assert_eq!(
        entry(&harness, "Lofi Piano").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        entry(&harness, "Bass").accesskit_node().toggled(),
        Some(Toggled::False)
    );
    assert!(harness.state().dirty);
    assert!(
        harness.state().preset_browser_open,
        "a pick keeps the section open"
    );
    // The sound followed: a lo-fi keys preset with vinyl noise, lo-fi on.
    assert!(harness.state().param(ParamId::LofiMix) > 0.0);
    assert!(harness.state().param(ParamId::LofiNoise) > 0.0);

    // Selecting track 1 shows its own preset in the current row and the list.
    harness.state_mut().select_track(0);
    harness.run_steps(2);
    harness.get_by_label(&head_name("Bass"));
    assert_eq!(
        entry(&harness, "Bass").accesskit_node().toggled(),
        Some(Toggled::True)
    );

    // CLOSE collapses the section: the list and the chips are gone, the current row stays.
    harness.get_by_label(CLOSE_LABEL).click();
    harness.run_steps(2);
    assert!(!harness.state().preset_browser_open);
    assert!(shown_entries(&harness).is_empty());
    assert!(harness.query_by_label(CLOSE_LABEL).is_none());
    assert!(
        harness
            .query_by_role_and_label(Role::Button, CATEGORY_LABELS[1])
            .is_none()
    );
    harness.get_by_label(&head_name("Bass"));
    // The section head opens it again and closes it; the current preset's name opens it.
    harness.get_by_label(BROWSE_LABEL).click();
    harness.run_steps(2);
    assert!(harness.state().preset_browser_open);
    assert_eq!(shown_entries(&harness).len(), presets::count());
    harness.get_by_label(BROWSE_LABEL).click();
    harness.run_steps(2);
    assert!(!harness.state().preset_browser_open);
    harness.get_by_label(&head_name("Bass")).click();
    harness.run_steps(2);
    assert!(harness.state().preset_browser_open);
}

#[test]
fn search_narrows_the_list_and_escape_closes_from_the_field() {
    let mut harness = browser_harness(ThemeMode::Light);
    let field = harness.get_by_role(Role::TextInput);
    field.click();
    harness.run_steps(2);
    harness.event(Event::Text("piano".to_owned()));
    harness.run_steps(2);
    assert_eq!(harness.state().preset_query, "piano");
    let shown = shown_entries(&harness);
    assert!(shown.contains(&"Lofi Piano".to_owned()), "{shown:?}");
    assert!(shown.contains(&"Toy Piano".to_owned()), "{shown:?}");
    assert_eq!(shown.len(), engine::presets::search("piano").len());
    assert!(shown.len() < presets::count());
    harness.get_by_label(&ui::preset_browser::count_text(
        shown.len(),
        presets::count(),
    ));
    // A category on top of the search; a tag search; nothing matching.
    chip(&harness, CATEGORY_LABELS[1]).click();
    harness.run_steps(2);
    assert_eq!(shown_entries(&harness), vec!["Lofi Piano", "Toy Piano"]);
    chip(&harness, ui::preset_browser::ALL_LABEL).click();
    harness.run_steps(2);
    harness.get_by_role(Role::TextInput).click();
    harness.run_steps(1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(Event::Text("808".to_owned()));
    harness.run_steps(2);
    assert_eq!(harness.state().preset_query, "808");
    let shown = shown_entries(&harness);
    assert!(shown.contains(&"Trap 808".to_owned()), "{shown:?}");
    assert!(shown.contains(&"808 Sub".to_owned()), "{shown:?}");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(Event::Text("zzzz".to_owned()));
    harness.run_steps(2);
    assert!(shown_entries(&harness).is_empty());
    harness.get_by_label(ui::preset_browser::NO_MATCH);
    // Escape inside the field closes the section; the text is kept for next time.
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().preset_browser_open);
    assert_eq!(harness.state().preset_query, "zzzz");
    assert_eq!(
        harness.state().preset_index(),
        0,
        "searching applies nothing"
    );
    // Typing into the field opens the section again.
    harness.get_by_role(Role::TextInput).click();
    harness.run_steps(2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(Event::Text("pad".to_owned()));
    harness.run_steps(2);
    assert!(harness.state().preset_browser_open);
    assert_eq!(harness.state().preset_query, "pad");
}

#[test]
fn prev_next_wrap_within_the_category_and_the_led_clears_on_a_new_preset() {
    let mut harness = browser_harness(ThemeMode::Dark);
    let count = presets::count();
    harness.get_by_label(PREV_LABEL).click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().preset_index(),
        count - 1,
        "wraps to the last"
    );
    harness.get_by_label(&head_name(presets::name(count - 1)));
    harness.get_by_label(NEXT_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().preset_index(), 0, "wraps to the first");
    harness.get_by_label(NEXT_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().preset_name(), "Sub");

    // Inside a category: only its presets, wrapping at both ends.
    chip(&harness, CATEGORY_LABELS[2]).click(); // Lofi Guitars
    harness.run_steps(2);
    let guitars = harness.state().preset_filter();
    assert_eq!(guitars.len(), 12);
    harness.get_by_label(NEXT_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().preset_index(), guitars[0], "Nylon Guitar");
    harness.get_by_label(&head_name("Nylon Guitar"));
    harness.get_by_label(PREV_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().preset_index(), guitars[11]);
    harness.get_by_label(NEXT_LABEL).click();
    harness.run_steps(2);
    assert_eq!(harness.state().preset_index(), guitars[0]);
    assert_eq!(
        entry(&harness, "Nylon Guitar").accesskit_node().toggled(),
        Some(Toggled::True)
    );

    // The LED: a knob move lights it, the next preset clears it.
    assert_eq!(modified(&harness), Some(Toggled::False));
    harness.state_mut().set_param(ParamId::FilterCutoff, 900.0);
    harness.run_steps(2);
    assert_eq!(modified(&harness), Some(Toggled::True));
    harness.get_by_label(NEXT_LABEL).click();
    harness.run_steps(2);
    assert_eq!(modified(&harness), Some(Toggled::False));
    assert_eq!(harness.state().preset_index(), guitars[1]);
    assert_eq!(
        harness.state().param(ParamId::FilterCutoff),
        presets::get(guitars[1])
            .unwrap()
            .value(ParamId::FilterCutoff)
    );
}

#[test]
fn shortcuts_step_presets_and_open_the_section_with_the_search_focused() {
    let mut harness = app_harness();
    assert!(!harness.state().state().preset_browser_open);
    assert!(harness.query_by_label(CLOSE_LABEL).is_none());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::PageDown);
    harness.run_steps(2);
    assert_eq!(harness.state().state().preset_name(), "Sub");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::PageUp);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::PageUp);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state().preset_index(),
        presets::count() - 1,
        "wraps"
    );
    harness.get_by_label(&head_name(presets::name(presets::count() - 1)));
    // The tempo nudge is untouched.
    let bpm = harness.state().state().bpm;
    harness.key_press_modifiers(Modifiers::COMMAND, Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(harness.state().state().bpm, bpm + 1.0);

    // Ctrl+Shift+P: the section opens with the search field focused; typing goes into the
    // field, plays no piano key and Space does not start the loop.
    harness.key_press_modifiers(app::shortcuts::COMMAND_SHIFT, Key::P);
    harness.run_steps(3);
    assert!(harness.state().state().preset_browser_open);
    harness.get_by_label(CLOSE_LABEL);
    assert!(
        harness.ctx.memory(|m| m.focused()).is_some(),
        "the search field has the keyboard"
    );
    harness.key_press(Key::A);
    harness.event(Event::Text("a".to_owned()));
    harness.key_press(Key::Space);
    harness.event(Event::Text(" ".to_owned()));
    harness.event(Event::Text("pad".to_owned()));
    harness.run_steps(2);
    assert_eq!(harness.state().state().note_on_count, 0, "no piano");
    assert!(!harness.state().state().playing, "Space typed, not played");
    assert_eq!(harness.state().state().preset_query, "a pad");
    // The matching entries are listed inside the browser column.
    let column_right = harness
        .get_by_label(ui::browser::COLLAPSE_LABEL)
        .rect()
        .max
        .x;
    let column = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(column_right + 1.0, 800.0));
    let shown = shown_entries_within(&harness, Some(column));
    let expected: Vec<&str> = harness
        .state()
        .state()
        .preset_matches()
        .into_iter()
        .map(presets::name)
        .collect();
    assert_eq!(shown, expected);
    assert!(shown.iter().all(|name| {
        let i = presets::index_of(name).unwrap();
        presets::matches(presets::get(i).unwrap(), "a pad")
    }));
    assert!(shown.len() < presets::count());
    // Ctrl+PageDown is a shortcut, so it is quiet while the field has the keyboard.
    let before = harness.state().state().preset_index();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::PageDown);
    harness.run_steps(2);
    assert_eq!(harness.state().state().preset_index(), before);

    // Escape closes it (from the field); afterwards Space plays again.
    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!harness.state().state().preset_browser_open);
    assert!(harness.query_by_label(CLOSE_LABEL).is_none());
    harness.key_press(Key::Space);
    harness.run_steps(2);
    assert!(harness.state().state().playing);
    harness.key_press(Key::Space);
    harness.run_steps(2);
    // A second Ctrl+Shift+P focuses the field again with the old text; typing appends.
    harness.key_press_modifiers(app::shortcuts::COMMAND_SHIFT, Key::P);
    harness.run_steps(3);
    harness.event(Event::Text("s".to_owned()));
    harness.run_steps(2);
    assert_eq!(harness.state().state().preset_query, "a pads");
    // With the browser collapsed to the rail, Ctrl+Shift+P opens the column too.
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    harness.key_press(Key::F9);
    harness.run_steps(2);
    assert!(!harness.state().state().browser_open);
    harness.key_press_modifiers(app::shortcuts::COMMAND_SHIFT, Key::P);
    harness.run_steps(3);
    assert!(harness.state().state().browser_open && harness.state().state().preset_browser_open);
    harness.get_by_label(CLOSE_LABEL);
}

/// At the design size the open section leaves the rack whole (its sixteen steps stay where
/// they were) and its chips, field, CLOSE button and entries sit inside the browser column.
#[test]
fn the_open_section_keeps_the_rack_whole_and_stays_inside_the_column() {
    let mut harness = app_harness();
    let steps_before = harness.get_by_label("Synth step 16").rect();
    let handle = harness.get_by_label(ui::shell::RACK_HANDLE_NAME).rect();
    assert!(steps_before.max.x <= handle.min.x);
    let column_right = harness
        .get_by_label(ui::browser::COLLAPSE_LABEL)
        .rect()
        .max
        .x;

    harness.get_by_label(BROWSE_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().state().preset_browser_open);
    assert_eq!(
        harness.get_by_label("Synth step 16").rect(),
        steps_before,
        "the section does not move the rack"
    );
    for label in CATEGORY_LABELS {
        let rect = chip(&harness, label).rect();
        assert!(
            rect.max.x <= column_right + 0.5 && rect.min.x >= 0.0,
            "{label} at {rect:?} is outside the column (right edge {column_right})"
        );
    }
    let close = harness.get_by_label(CLOSE_LABEL).rect();
    assert!(close.max.x <= column_right + 0.5);
    let field = harness.get_by_role(Role::TextInput).rect();
    assert!(field.max.x <= column_right + 0.5 && field.min.x >= 0.0);
    // The list: one column of rows inside the column, scrolling.
    let first = entry(&harness, "Sub").rect();
    let second = entry(&harness, "Lead").rect();
    assert!(first.max.x <= column_right + 0.5 && second.max.x <= column_right + 0.5);
    assert!(second.min.y > first.max.y - 0.5, "one row under the other");
    assert_eq!(first.height(), ui::preset_browser::ROW_HEIGHT);

    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!harness.state().state().preset_browser_open);
    assert_eq!(harness.get_by_label("Synth step 16").rect(), steps_before);
}
