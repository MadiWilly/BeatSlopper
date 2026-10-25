//! egui_kittest: the M9 preset browser in the SYNTH panel. The head strip shows the selected
//! track's preset between `<` / `>`; BROWSE opens the browser, whose category row filters
//! the list (counts match the unified list's categories), whose search narrows it, and
//! whose entries apply to the selected track; `<` / `>` and Ctrl+PageUp / PageDown wrap
//! within the category; Ctrl+Shift+P opens the browser with the search focused, where
//! typing neither plays the piano nor fires Space; Escape closes; the MODIFIED lamp lights
//! on a knob move and clears on a new preset; both themes render; the browser leaves the
//! 1280 x 1140 whole-sequencer layout alone.

use app::ui::preset_browser::{
    self, BROWSE_LABEL, CATEGORY_LABELS, CLOSE_LABEL, NEXT_LABEL, PREV_LABEL, head_name,
};
use app::ui::synth_panel::MODIFIED_LABEL;
use app::{AppState, BeatSlopperApp, presets, ui};
use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use engine::ParamId;
use neu::{Theme, ThemeMode};

/// The SYNTH panel alone, 780 x 520 (its share of the design page), with the browser open.
fn synth_harness(mode: ThemeMode) -> Harness<'static, AppState> {
    let (mut state, _engine) = AppState::headless(mode);
    state.open_preset_browser(false);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(780.0, 520.0))
        .build_ui_state(
            move |ui, state: &mut AppState| ui::synth_panel::show(ui, state, &theme),
            state,
        );
    harness.run_steps(3);
    harness
}

/// The whole app (shortcuts and the piano through `tick`) at the design size.
fn app_harness() -> Harness<'static, BeatSlopperApp> {
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
    harness
}

/// The browser entries shown: every button named after a preset (inside `within`, when
/// given: the whole app has a "Bass" button in the sequencer's lane label too).
fn shown_entries_within<S>(harness: &Harness<'_, S>, within: Option<egui::Rect>) -> Vec<String> {
    let names: Vec<&str> = presets::all().map(|p| p.name).collect();
    harness
        .get_all_by_role(Role::Button)
        .filter(|node| within.is_none_or(|area| area.contains_rect(node.rect())))
        .filter_map(|node| node.accesskit_node().label().map(|label| label.to_string()))
        .filter(|label| names.contains(&label.as_str()))
        .collect()
}

/// [`shown_entries_within`] over the whole harness (the SYNTH panel alone).
fn shown_entries<S>(harness: &Harness<'_, S>) -> Vec<String> {
    shown_entries_within(harness, None)
}

fn radio<'h, S>(harness: &'h Harness<'_, S>, label: &'h str) -> egui_kittest::Node<'h> {
    harness.get_by_role_and_label(Role::RadioButton, label)
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
        let mut harness = synth_harness(mode);
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
        harness.get_by_label(&preset_browser::count_text(
            presets::count(),
            presets::count(),
        ));

        // Each category: the entries are exactly the category's presets (the engine's
        // `by_category` plus the classics of that category).
        for (i, category) in presets::Category::ALL.iter().enumerate() {
            radio(&harness, CATEGORY_LABELS[i + 1]).click();
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
        radio(&harness, preset_browser::ALL_LABEL).click();
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
fn clicking_an_entry_applies_it_to_the_selected_track_and_the_head_follows() {
    let mut harness = synth_harness(ThemeMode::Dark);
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
        "a pick keeps the browser open"
    );
    // The sound followed: a lo-fi keys preset with vinyl noise, lo-fi on.
    assert!(harness.state().param(ParamId::LofiMix) > 0.0);
    assert!(harness.state().param(ParamId::LofiNoise) > 0.0);

    // Selecting track 1 shows its own preset in the head and the list.
    harness.state_mut().select_track(0);
    harness.run_steps(2);
    harness.get_by_label(&head_name("Bass"));
    assert_eq!(
        entry(&harness, "Bass").accesskit_node().toggled(),
        Some(Toggled::True)
    );

    // CLOSE puts the oscillator groups back.
    assert!(harness.query_by_label("Osc 1").is_none());
    harness.get_by_label(CLOSE_LABEL).click();
    harness.run_steps(2);
    assert!(!harness.state().preset_browser_open);
    harness.get_by_label("Osc 1");
    assert!(harness.query_by_label(CLOSE_LABEL).is_none());
    // BROWSE opens it again; the name well does too.
    harness.get_by_label(BROWSE_LABEL).click();
    harness.run_steps(2);
    assert!(harness.state().preset_browser_open);
    harness.get_by_label(BROWSE_LABEL).click();
    harness.run_steps(2);
    assert!(!harness.state().preset_browser_open);
    harness.get_by_label(&head_name("Bass")).click();
    harness.run_steps(2);
    assert!(harness.state().preset_browser_open);
}

#[test]
fn search_narrows_the_list_and_escape_closes_from_the_field() {
    let mut harness = synth_harness(ThemeMode::Light);
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
    harness.get_by_label(&preset_browser::count_text(shown.len(), presets::count()));
    // A category on top of the search; a tag search; nothing matching.
    radio(&harness, CATEGORY_LABELS[1]).click();
    harness.run_steps(2);
    assert_eq!(shown_entries(&harness), vec!["Lofi Piano", "Toy Piano"]);
    radio(&harness, preset_browser::ALL_LABEL).click();
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
    harness.get_by_label(preset_browser::NO_MATCH);
    // Escape inside the field closes the browser; the text is kept for next time.
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().preset_browser_open);
    assert_eq!(harness.state().preset_query, "zzzz");
    assert_eq!(
        harness.state().preset_index(),
        0,
        "searching applies nothing"
    );
}

#[test]
fn prev_next_wrap_within_the_category_and_the_lamp_clears_on_a_new_preset() {
    let mut harness = synth_harness(ThemeMode::Dark);
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
    radio(&harness, CATEGORY_LABELS[2]).click(); // Lofi Guitars
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

    // The lamp: a knob move lights it, the next preset clears it.
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
fn shortcuts_step_presets_and_open_the_browser_with_the_search_focused() {
    let mut harness = app_harness();
    assert!(!harness.state().state().preset_browser_open);
    harness.get_by_label("Osc 1");
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

    // Ctrl+Shift+P: the browser opens with the search field focused; typing goes into the
    // field, plays no piano key and Space does not start the loop.
    harness.key_press_modifiers(app::shortcuts::COMMAND_SHIFT, Key::P);
    harness.run_steps(3);
    assert!(harness.state().state().preset_browser_open);
    harness.get_by_label(CLOSE_LABEL);
    assert!(harness.query_by_label("Osc 1").is_none());
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
    let top_row = harness.state().state().top_row_rect.expect("measured");
    let shown = shown_entries_within(&harness, Some(top_row));
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
    harness.get_by_label("Osc 1");
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
}

/// At the design size the browser, open or closed, leaves the whole sequencer visible (see
/// `ui_renders::whole_sequencer_is_visible_at_the_default_window_size`) and its category
/// row fits inside the SYNTH panel.
#[test]
fn browser_keeps_the_whole_sequencer_visible_at_the_design_size() {
    let mut harness = app_harness();
    let check = |harness: &Harness<'_, BeatSlopperApp>| {
        let bass = harness.get_by_label("Bass step 16").rect();
        let octave = harness.get_by_label("Octave").rect();
        let sequencer_bottom = bass.max.y + ui::sequencer::ROW_GAP + 10.0 + 20.0;
        let keys_panel_top = octave.min.y - 30.0;
        assert!(
            sequencer_bottom <= keys_panel_top,
            "sequencer ends at {sequencer_bottom} but the KEYS panel starts at {keys_panel_top}"
        );
    };
    check(&harness);
    let top_row = harness.state().state().top_row_rect.expect("measured");
    let head_before = harness.get_by_label(MODIFIED_LABEL).rect();

    harness.get_by_label(BROWSE_LABEL).click();
    harness.run_steps(3);
    assert!(harness.state().state().preset_browser_open);
    check(&harness);
    // The row keeps its height and gets no wider (on `main` the closed panel's LFO DEPTH
    // readout already overhangs the FX rack by a few points, an M8 layout artefact the
    // browser does not add to).
    let open_row = harness.state().state().top_row_rect.expect("measured");
    assert_eq!(open_row.min, top_row.min, "the top row did not move");
    assert_eq!(open_row.max.y, top_row.max.y, "the top row kept its height");
    assert!(
        open_row.max.x <= top_row.max.x,
        "the browser must not widen the top row: {open_row:?} vs {top_row:?}"
    );
    assert_eq!(harness.get_by_label(MODIFIED_LABEL).rect(), head_before);
    let synth_right = open_row.max.x - ui::fx_rack::WIDTH - 10.0;
    for label in CATEGORY_LABELS {
        let rect = radio(&harness, label).rect();
        assert!(
            rect.max.x <= synth_right && rect.min.x >= top_row.min.x,
            "{label} at {rect:?} is outside the SYNTH panel (right edge {synth_right})"
        );
    }
    let close = harness.get_by_label(CLOSE_LABEL).rect();
    assert!(close.max.x <= synth_right && top_row.contains_rect(close));
    let field = harness.get_by_role(Role::TextInput).rect();
    assert!(top_row.contains_rect(field));
    // The list fills the rest of the panel: several columns of entries, inside the panel.
    let first = entry(&harness, "Sub").rect(); // "Bass" is the sequencer lane too
    let second = entry(&harness, "Lead").rect();
    assert!(
        second.min.x > first.max.x,
        "more than one column at 1280 wide"
    );
    assert!(top_row.contains_rect(first) && top_row.contains_rect(second));

    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!harness.state().state().preset_browser_open);
    check(&harness);
}
