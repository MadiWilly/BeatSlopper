//! egui_kittest tests for the Deck kit (M12): every new widget renders in both themes with its
//! states and accessible names, the shell pieces lay out to the WINNER.md numbers, the menu /
//! dropdown / tooltip carry the one floating shadow and nothing else does.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, MouseWheelSource, MouseWheelUnit, Pos2, TouchPhase, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use neu::{
    ButtonKind, ButtonSize, Icon, LedKind, MenuItem, SliderSpec, TOOLTIP_DELAY, TagKind, Theme,
    ThemeMode,
};

/// Whether a node reports the ON / selected state (`WidgetInfo::selected`).
fn is_on(node: &egui_kittest::Node<'_>) -> bool {
    let n = node.accesskit_node();
    matches!(n.toggled(), Some(egui::accesskit::Toggled::True)) || n.is_selected() == Some(true)
}

fn pct(v: f32) -> String {
    format!("{v:.0} %")
}

/// Press at `from`, move to `to` (one event), release there.
fn drag<S>(harness: &mut Harness<'_, S>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.run();
    harness.hover_at(to);
    harness.run();
    harness.drop_at(to);
    harness.run();
}

fn wheel<S>(harness: &mut Harness<'_, S>, delta_y: f32) {
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, delta_y),
        modifiers: Modifiers::NONE,
        phase: TouchPhase::Move,
        source: MouseWheelSource::Wheel,
    });
    harness.run();
}

#[test]
fn buttons_render_every_kind_state_and_size_in_both_themes() {
    #[derive(Default)]
    struct S {
        follow: bool,
        solo: bool,
        armed: bool,
        clicks: u32,
    }
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h = Harness::builder()
            .with_size(vec2(900.0, 400.0))
            .build_ui_state(
                move |ui, s: &mut S| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    ui.horizontal(|ui| {
                        if neu::button(ui, &theme, "Clone").clicked() {
                            s.clicks += 1;
                        }
                        neu::primary_button(ui, &theme, "Export");
                        neu::ghost_button(ui, &theme, "Ghost");
                        neu::small_button(ui, &theme, "Small");
                        neu::button_kind(ui, &theme, "Xs", ButtonKind::Secondary, ButtonSize::Xs);
                        neu::button_wide(
                            ui,
                            &theme,
                            "Export WAV",
                            ButtonKind::Primary,
                            ButtonSize::Regular,
                            160.0,
                        );
                        if neu::toggle_button(ui, &theme, "Follow", s.follow).clicked() {
                            s.follow = !s.follow;
                        }
                        if neu::solo_button(ui, &theme, "S", s.solo).clicked() {
                            s.solo = !s.solo;
                        }
                        if neu::rec_button(ui, &theme, s.armed).clicked() {
                            s.armed = !s.armed;
                        }
                        ui.add_enabled_ui(false, |ui| {
                            neu::button(ui, &theme, "Disabled");
                            neu::primary_button(ui, &theme, "Off");
                        });
                    });
                    ui.horizontal(|ui| {
                        for icon in [
                            Icon::Play,
                            Icon::Stop,
                            Icon::Undo,
                            Icon::Redo,
                            Icon::Menu,
                            Icon::ChevronDown,
                            Icon::ChevronUp,
                            Icon::ChevronLeft,
                            Icon::ChevronRight,
                            Icon::Search,
                            Icon::Link,
                            Icon::Copy,
                            Icon::Paste,
                            Icon::Dots,
                            Icon::Folder,
                            Icon::File,
                            Icon::Roll,
                            Icon::Mixer,
                            Icon::Synth,
                            Icon::Song,
                            Icon::Browser,
                            Icon::Fullscreen,
                            Icon::Check,
                        ] {
                            neu::icon_button_kind(
                                ui,
                                &theme,
                                icon,
                                vec2(28.0, 28.0),
                                ButtonKind::Ghost,
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        neu::icon_button_kind(
                            ui,
                            &theme,
                            Icon::Play,
                            vec2(28.0, 28.0),
                            ButtonKind::Primary,
                        );
                        neu::header_button(ui, &theme, Icon::Clear);
                        neu::toolbar_separator(ui, &theme);
                        neu::stepper_button(ui, &theme, Icon::Minus);
                    });
                },
                S::default(),
            );
        h.run();
        // Sizes.
        assert_eq!(h.get_by_label("Clone").rect().height(), 28.0, "{mode:?}");
        assert!(h.get_by_label("Clone").rect().width() >= 28.0);
        assert_eq!(h.get_by_label("Small").rect().height(), 22.0);
        assert_eq!(h.get_by_label("Xs").rect().height(), 24.0);
        assert_eq!(h.get_by_label("Export WAV").rect().width(), 160.0);
        assert_eq!(h.get_by_label("Record").rect().size(), vec2(28.0, 28.0));
        assert_eq!(h.get_by_label("Clear").rect().size(), vec2(24.0, 24.0));
        assert_eq!(h.get_by_label("Decrease").rect().size(), vec2(28.0, 28.0));
        assert!(h.get_all_by_label("Play").count() >= 2);
        // States: hover, press, toggle ON (reported as selected).
        for label in ["Clone", "Export", "Ghost", "Follow", "S", "Record"] {
            h.get_by_label(label).hover();
            h.run();
            h.get_by_label(label).click();
            h.run();
        }
        assert_eq!(h.state().clicks, 1);
        assert!(h.state().follow && h.state().solo && h.state().armed);
        assert!(is_on(&h.get_by_label("Follow")));
        assert!(is_on(&h.get_by_label("S")));
        assert!(is_on(&h.get_by_label("Record")));
        // Disabled buttons do not click.
        h.get_by_label("Disabled").click();
        h.run();
        assert_eq!(h.state().clicks, 1);
    }
}

#[test]
fn switches_flip_and_the_theme_toggle_is_the_dark_switch() {
    let mut h = Harness::builder().build_ui_state(
        |ui, s: &mut (bool, ThemeMode, u32)| {
            let theme = Theme::for_mode(s.1);
            theme.apply(ui.ctx());
            ui.horizontal(|ui| {
                if neu::switch(ui, &theme, &mut s.0, "Follow").changed() {
                    s.2 += 1;
                }
                neu::switch_small(ui, &theme, &mut s.0, "Mix");
                neu::theme_toggle(ui, &theme, &mut s.1);
            });
        },
        (false, ThemeMode::Light, 0),
    );
    h.run();
    assert_eq!(h.get_by_label("Follow").rect().size(), vec2(40.0, 22.0));
    assert_eq!(h.get_by_label("Mix").rect().size(), vec2(34.0, 20.0));
    assert_eq!(h.get_by_label("Dark theme").rect().size(), vec2(40.0, 22.0));
    assert!(!is_on(&h.get_by_label("Follow")));
    h.get_by_label("Follow").click();
    h.run();
    assert!(h.state().0);
    assert_eq!(h.state().2, 1);
    assert!(is_on(&h.get_by_label("Follow")));
    h.get_by_label("Dark theme").click();
    h.run();
    assert_eq!(h.state().1, ThemeMode::Dark);
    assert!(is_on(&h.get_by_label("Dark theme")));
}

#[test]
fn window_frame_has_a_28_header_with_title_right_slot_and_a_clipped_body() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h = Harness::builder()
            .with_size(vec2(600.0, 400.0))
            .build_ui_state(
                move |ui, closed: &mut bool| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    neu::window_frame_sized(
                        ui,
                        &theme,
                        vec2(400.0, 200.0),
                        "Piano roll",
                        Some("Keys · Dusty Rhodes"),
                        |ui| {
                            if neu::header_button(ui, &theme, Icon::Clear).clicked() {
                                *closed = true;
                            }
                            neu::keycap(ui, &theme, "F5");
                        },
                        |ui| {
                            neu::toolbar(ui, &theme, |ui| {
                                neu::small_button(ui, &theme, "Draw");
                            });
                            neu::value(ui, &theme, "Body text");
                        },
                    );
                    // A collapsed window: header only.
                    neu::window_frame_sized(
                        ui,
                        &theme,
                        vec2(400.0, neu::WINDOW_HEADER),
                        "Keyboard",
                        None,
                        |_| {},
                        |ui| {
                            neu::value(ui, &theme, "Hidden body");
                        },
                    );
                },
                false,
            );
        h.run();
        let close = h.get_by_label("Clear").rect();
        let keycap = h.get_by_label("F5").rect();
        let body = h.get_by_label("Body text").rect();
        let draw = h.get_by_label("Draw").rect();
        assert!(
            close.max.y <= 8.0 + neu::WINDOW_HEADER + 2.0,
            "{mode:?}: {close:?}"
        );
        assert!(
            keycap.max.x < close.min.x,
            "keycap left of close: {keycap:?} {close:?}"
        );
        assert!(
            draw.min.y >= 8.0 + neu::WINDOW_HEADER,
            "toolbar under the header"
        );
        assert!(body.min.y >= draw.max.y, "body under the toolbar");
        assert!(
            h.query_by_label("Hidden body").is_none(),
            "collapsed hides the body"
        );
        h.get_by_label("Clear").click();
        h.run();
        assert!(*h.state());
    }
}

#[test]
fn toolbars_tabs_and_hint_bar_lay_out_to_the_rhythm() {
    let mut h = Harness::builder()
        .with_size(vec2(700.0, 300.0))
        .build_ui_state(
            |ui, s: &mut (usize, u32)| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                neu::app_toolbar(ui, &theme, |ui| {
                    neu::button(ui, &theme, "Tap");
                    neu::toolbar_separator(ui, &theme);
                    neu::display(ui, &theme, "120");
                });
                neu::toolbar(ui, &theme, |ui| {
                    neu::small_button(ui, &theme, "Follow");
                });
                if neu::tab_strip(
                    ui,
                    &theme,
                    &mut s.0,
                    &[
                        ("Piano roll", Some("F5")),
                        ("Mixer", Some("F6")),
                        ("Synth & FX", None),
                    ],
                )
                .changed()
                {
                    s.1 += 1;
                }
                neu::hint_bar(ui, &theme, &[("Space", "Play / Stop"), ("Ctrl+L", "Link")]);
            },
            (0, 0),
        );
    h.run();
    let tap = h.get_by_label("Tap").rect();
    let follow = h.get_by_label("Follow").rect();
    // The app toolbar is 40 tall with its control centred; the window toolbar 36.
    assert!((tap.center().y - (8.0 + 20.0)).abs() < 1.0, "{tap:?}");
    assert!(
        (follow.center().y - (8.0 + 40.0 + 8.0 + 18.0)).abs() < 1.0,
        "{follow:?}"
    );
    assert_eq!(tap.height(), 28.0);
    assert_eq!(follow.height(), 22.0);
    // Tabs: named, selected state, click and arrows.
    let roll = h.get_by_role_and_label(Role::Tab, "Piano roll");
    assert!(is_on(&roll));
    assert_eq!(roll.rect().height(), neu::TAB_STRIP);
    h.get_by_role_and_label(Role::Tab, "Mixer").click();
    h.run();
    assert_eq!(*h.state(), (1, 1));
    assert!(is_on(&h.get_by_role_and_label(Role::Tab, "Mixer")));
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().0, 2);
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().0, 2, "clamped at the last tab");
    h.key_press(Key::ArrowLeft);
    h.run();
    assert_eq!(h.state().0, 1);
    h.get_by_label("Space Play / Stop · Ctrl+L Link");
}

#[test]
fn rail_buttons_leds_tags_chips_keycaps_and_tiles_render_and_name_themselves() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h = Harness::builder()
            .with_size(vec2(700.0, 200.0))
            .with_step_dt(TOOLTIP_DELAY / 3.0)
            .build_ui_state(
                move |ui, on: &mut bool| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    ui.horizontal(|ui| {
                        if neu::rail_button(ui, &theme, Icon::Browser, *on, "Browser (F9)")
                            .clicked()
                        {
                            *on = !*on;
                        }
                        neu::rail_button(ui, &theme, Icon::Roll, false, "");
                        neu::led(ui, &theme, true, LedKind::Ok);
                        neu::led_named(ui, &theme, false, LedKind::Ok, "Mute Kick");
                        neu::led_named(ui, &theme, true, LedKind::Accent, "Limiter");
                        neu::led_named(ui, &theme, true, LedKind::Rec, "Record LED");
                        neu::lamp(ui, &theme, true, "MIDI");
                        neu::lamp_kind(ui, &theme, false, "Limit", LedKind::Accent);
                        neu::tag(ui, &theme, "vinyl", TagKind::Vinyl);
                        neu::tag(ui, &theme, "drone", TagKind::Drone);
                        neu::tag(ui, &theme, "slot 2", TagKind::Plain);
                        neu::tag(ui, &theme, "keys", TagKind::Colour(theme.track_colour(0)));
                        neu::chip(ui, &theme, theme.track_colour(1));
                        neu::keycap(ui, &theme, "Ctrl+S");
                        neu::pattern_tile(ui, &theme, "A", true, true);
                        neu::pattern_tile(ui, &theme, "B", false, false);
                        neu::pattern_tile(ui, &theme, "C", true, false);
                    });
                },
                false,
            );
        h.run();
        let rail = h.get_by_label("Browser (F9)");
        assert_eq!(rail.rect().size(), vec2(28.0, 28.0));
        assert!(!is_on(&rail));
        h.get_by_label("Browser (F9)").click();
        h.run();
        assert!(*h.state());
        assert!(is_on(&h.get_by_label("Browser (F9)")));
        h.get_by_label("Piano roll");
        assert_eq!(h.get_by_label("LED").rect().size(), vec2(10.0, 10.0));
        assert!(is_on(&h.get_by_label("LED")));
        assert!(!is_on(&h.get_by_label("Mute Kick")));
        h.get_by_label("Limiter");
        h.get_by_label("Record LED");
        assert!(is_on(&h.get_by_label("MIDI")));
        assert_eq!(h.get_by_label("vinyl").rect().height(), neu::TAG_HEIGHT);
        h.get_by_label("drone");
        h.get_by_label("slot 2");
        h.get_by_label("keys");
        assert_eq!(h.get_by_label("Ctrl+S").rect().height(), neu::KEYCAP_HEIGHT);
        assert_eq!(h.get_by_label("Pattern A").rect().size(), vec2(24.0, 24.0));
        assert!(is_on(&h.get_by_label("Pattern A")));
        assert!(!is_on(&h.get_by_label("Pattern B")));
        h.get_by_label("Pattern C").hover();
        h.run();
        // The rail button's tooltip after the delay.
        h.get_by_label("Browser (F9)").hover();
        h.run_steps(7);
        assert!(
            h.get_all_by_label("Browser (F9)").count() >= 2,
            "{mode:?}: the tooltip repeats the name"
        );
    }
}

#[test]
fn number_box_drags_scrolls_and_types_within_its_range() {
    let mut h = Harness::builder()
        .with_size(vec2(200.0, 200.0))
        .build_ui_state(
            |ui, s: &mut (u8, u32)| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                ui.add_space(40.0);
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    if neu::number_box_named(ui, &theme, &mut s.0, 0..=16, "Insert of Kick")
                        .changed()
                    {
                        s.1 += 1;
                    }
                });
            },
            (3, 0),
        );
    h.run_steps(2);
    let node = h.get_by_label("Insert of Kick");
    assert_eq!(node.rect().size(), vec2(22.0, 22.0));
    assert_eq!(node.value().as_deref(), Some("3"));
    let centre = node.rect().center();
    // Drag up 8 px: two steps.
    drag(&mut h, centre, centre - vec2(0.0, 8.0));
    assert_eq!(h.state().0, 5);
    assert!(h.state().1 >= 1);
    // Wheel down one notch: one step down.
    h.hover_at(centre);
    h.run();
    wheel(&mut h, -1.0);
    assert_eq!(h.state().0, 4);
    // Click opens the editor; a typed value commits, clamped to the range.
    h.get_by_label("Insert of Kick").click();
    h.run_steps(2);
    h.event(Event::Text("40".to_owned()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().0, 16);
    assert_eq!(
        h.get_by_label("Insert of Kick").value().as_deref(),
        Some("16")
    );
}

#[test]
fn name_buttons_report_selection_and_fit_the_rack() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h = Harness::builder()
            .with_size(vec2(400.0, 120.0))
            .build_ui_state(
                move |ui, picked: &mut Option<usize>| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    ui.horizontal(|ui| {
                        if neu::name_button(ui, &theme, "Kick", None, false).clicked() {
                            *picked = Some(0);
                        }
                        if neu::name_button(
                            ui,
                            &theme,
                            "Dusty Rhodes electric piano",
                            Some(theme.track_colour(2)),
                            true,
                        )
                        .clicked()
                        {
                            *picked = Some(1);
                        }
                        neu::name_button_sized(
                            ui,
                            &theme,
                            "Muted",
                            Some(theme.track_colour(3)),
                            false,
                            true,
                            vec2(64.0, 24.0),
                        );
                    });
                },
                None,
            );
        h.run();
        assert_eq!(h.get_by_label("Kick").rect().size(), neu::NAME_BUTTON_RACK);
        assert_eq!(neu::NAME_BUTTON_RACK, vec2(76.0, 24.0));
        assert!(!is_on(&h.get_by_label("Kick")));
        assert!(is_on(&h.get_by_label("Dusty Rhodes electric piano")));
        assert_eq!(h.get_by_label("Muted").rect().width(), 64.0);
        h.get_by_label("Kick").hover();
        h.run();
        h.get_by_label("Kick").click();
        h.run();
        assert_eq!(*h.state(), Some(0));
    }
}

#[test]
fn drawer_handle_and_list_rows_report_their_gestures() {
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut (f32, Option<usize>)| {
                let theme = Theme::light();
                theme.apply(ui.ctx());
                let handle = neu::drawer_handle(ui, &theme);
                if handle.dragged() {
                    s.0 += handle.drag_delta().y;
                }
                for (i, name) in ["Tape Wurli", "Velvet EP", "Dusty Rhodes"]
                    .iter()
                    .enumerate()
                {
                    let row = neu::list_row_named(ui, &theme, i == 2, name, |ui| {
                        neu::body(ui, &theme, name);
                        if i == 0 {
                            neu::tag(ui, &theme, "vinyl", TagKind::Vinyl);
                        }
                        if i == 2 {
                            neu::led_named(ui, &theme, true, LedKind::Accent, "Modified");
                        }
                    });
                    if row.clicked() {
                        s.1 = Some(i);
                    }
                }
            },
            (0.0, None),
        );
    h.run();
    let handle = h.get_by_label("Drawer handle").rect();
    assert_eq!(handle.height(), neu::DRAWER_HANDLE_HIT);
    drag(&mut h, handle.center(), handle.center() + vec2(0.0, 30.0));
    assert!(h.state().0 > 20.0, "drag delta {}", h.state().0);
    let row = h.get_by_role_and_label(Role::ListItem, "Velvet EP");
    assert_eq!(row.rect().height(), neu::LIST_ROW);
    assert!(!is_on(&row));
    assert!(is_on(
        &h.get_by_role_and_label(Role::ListItem, "Dusty Rhodes")
    ));
    h.get_by_role_and_label(Role::ListItem, "Velvet EP").hover();
    h.run();
    h.get_by_role_and_label(Role::ListItem, "Velvet EP").click();
    h.run();
    assert_eq!(h.state().1, Some(1));
}

#[test]
fn menu_reports_the_clicked_entry_and_ignores_disabled_ones() {
    const ITEMS: [MenuItem<'static>; 6] = [
        MenuItem::Entry {
            label: "New",
            key: Some("Ctrl+N"),
            enabled: true,
        },
        MenuItem::Entry {
            label: "Save",
            key: Some("Ctrl+S"),
            enabled: true,
        },
        MenuItem::Separator,
        MenuItem::Entry {
            label: "Export WAV…",
            key: Some("Ctrl+E"),
            enabled: true,
        },
        MenuItem::Separator,
        MenuItem::Entry {
            label: "Redo",
            key: Some("Ctrl+Y"),
            enabled: false,
        },
    ];
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, picked: &mut Vec<usize>| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                ui.add_space(20.0);
                if let Some(i) = neu::menu(ui, &theme, &ITEMS) {
                    picked.push(i);
                }
            },
            Vec::new(),
        );
    h.run();
    let menu = h.get_by_role_and_label(Role::Menu, "Menu").rect();
    assert!(menu.width() >= neu::MENU_MIN_WIDTH);
    assert_eq!(menu.height(), 8.0 + 4.0 * neu::MENU_ITEM + 2.0 * 9.0);
    let save = h.get_by_role_and_label(Role::MenuItem, "Save");
    assert_eq!(save.rect().height(), neu::MENU_ITEM);
    save.hover();
    h.run();
    h.get_by_role_and_label(Role::MenuItem, "Save").click();
    h.run();
    assert_eq!(*h.state(), vec![1]);
    h.get_by_role_and_label(Role::MenuItem, "Export WAV…")
        .click();
    h.run();
    assert_eq!(*h.state(), vec![1, 3]);
    h.get_by_role_and_label(Role::MenuItem, "Redo").click();
    h.run();
    assert_eq!(*h.state(), vec![1, 3], "disabled entries never fire");
}

#[test]
fn dropdown_opens_its_menu_and_picks_an_entry() {
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut (usize, Option<usize>)| {
                let theme = Theme::light();
                theme.apply(ui.ctx());
                const SNAPS: [&str; 3] = ["1/4", "1/8", "1/16"];
                if let Some(i) =
                    neu::dropdown(ui, &theme, "Snap", SNAPS[s.0], &SNAPS, 90.0, false, None)
                {
                    s.0 = i;
                    s.1 = Some(i);
                }
                neu::dropdown(
                    ui,
                    &theme,
                    "Track",
                    "Keys",
                    &["Keys", "Bass"],
                    120.0,
                    true,
                    Some(theme.track_colour(5)),
                );
            },
            (2, None),
        );
    h.run();
    let field = h.get_by_role_and_label(Role::ComboBox, "Snap");
    assert_eq!(field.rect().size(), vec2(90.0, 28.0));
    assert_eq!(field.value().as_deref(), Some("1/16"));
    assert_eq!(
        h.get_by_role_and_label(Role::ComboBox, "Track")
            .rect()
            .height(),
        22.0
    );
    h.get_by_role_and_label(Role::ComboBox, "Snap").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::MenuItem, "1/8").click();
    h.run_steps(2);
    assert_eq!(*h.state(), (1, Some(1)));
    assert_eq!(
        h.get_by_role_and_label(Role::ComboBox, "Snap")
            .value()
            .as_deref(),
        Some("1/8")
    );
    assert!(
        h.query_by_role_and_label(Role::MenuItem, "1/8").is_none(),
        "the popup closes on the pick"
    );
}

#[test]
fn tooltip_with_keycap_shows_text_and_key_after_the_delay() {
    let mut h = Harness::builder()
        .with_size(vec2(300.0, 200.0))
        .with_step_dt(TOOLTIP_DELAY / 3.0)
        .build_ui(|ui| {
            let theme = Theme::dark();
            theme.apply(ui.ctx());
            ui.add_space(40.0);
            let play = neu::icon_button(ui, &theme, Icon::Play, 28.0);
            neu::tooltip_with_keycap(play, &theme, "Play / Stop", "Space");
        });
    h.run();
    h.get_by_label("Play").hover();
    h.step();
    assert!(
        h.query_by_role(Role::Tooltip).is_none(),
        "waits for the delay"
    );
    h.run_steps(6);
    h.get_by_role(Role::Tooltip)
        .get_by_label("Play / Stop (Space)");
}

#[test]
fn text_field_edits_and_names_itself_by_its_placeholder() {
    let mut h = Harness::builder()
        .with_size(vec2(300.0, 100.0))
        .build_ui_state(
            |ui, text: &mut String| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                neu::text_field(
                    ui,
                    &theme,
                    text,
                    "Search presets",
                    Some(Icon::Search),
                    180.0,
                );
            },
            String::new(),
        );
    h.run();
    let field = h.get_by_role_and_label(Role::TextInput, "Search presets");
    assert!((field.rect().height() - (neu::FIELD_HEIGHT - 2.0)).abs() <= 2.0);
    field.click();
    h.run_steps(2);
    h.event(Event::Text("rho".to_owned()));
    h.run_steps(2);
    assert_eq!(h.state(), "rho");
}

#[test]
fn sliders_meters_and_the_group_render_and_react() {
    let mut h = Harness::builder()
        .with_size(vec2(500.0, 400.0))
        .with_step_dt(0.01)
        .with_max_steps(32)
        .build_ui_state(
            |ui, v: &mut f32| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                let spec = SliderSpec {
                    label: "Vel",
                    min: 0.0,
                    max: 100.0,
                    default: 80.0,
                    fmt: pct,
                };
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    neu::hslider(ui, &theme, v, &spec);
                    neu::hslider_sized(ui, &theme, v, &spec, vec2(72.0, 24.0));
                    neu::meter_horizontal(ui, &theme, (0.7, 0.5), neu::METER_HORIZONTAL);
                    neu::gr_meter(ui, &theme, 0.3, 120.0);
                    neu::stereo_meter(ui, &theme, (1.0, 0.9), neu::STEREO_METER_SIZE);
                    neu::meter(ui, &theme, 0.8, neu::METER_TRACK);
                    neu::segmented_small(ui, &theme, &mut 1, &["1", "2", "4"]);
                });
                neu::group(
                    ui,
                    &theme,
                    "Osc 1",
                    |ui| {
                        neu::segmented_small(ui, &theme, &mut 0, &["Sine", "Saw"]);
                    },
                    |ui| {
                        neu::value(ui, &theme, "Knobs go here");
                    },
                );
                let (inner, _) = neu::well(ui, &theme, vec2(200.0, 44.0));
                assert!(inner.width() < 200.0);
            },
            80.0,
        );
    h.run();
    let sliders: Vec<_> = h.get_all_by_role_and_label(Role::Slider, "Vel").collect();
    assert_eq!(sliders.len(), 2);
    let first = sliders[0].rect();
    assert_eq!(first.size(), neu::HSLIDER_SIZE);
    assert_eq!(neu::HSLIDER_SIZE, vec2(72.0, 28.0));
    // Press a quarter of the travel from the left: 25 %.
    let x = first.min.x + 6.0 + 0.25 * (72.0 - 12.0);
    h.hover_at(pos2(x, first.center().y));
    h.run();
    h.drag_at(pos2(x, first.center().y));
    h.run();
    h.drop_at(pos2(x, first.center().y));
    h.run();
    assert!((*h.state() - 25.0).abs() < 1.0, "{}", h.state());
    assert_eq!(
        h.get_by_label("Gain reduction").rect().width(),
        neu::GR_METER_WIDTH
    );
    assert_eq!(h.get_all_by_label("Stereo level").count(), 2);
    h.get_by_label("Level");
    h.get_by_label("Osc 1");
    h.get_by_label("Knobs go here");
}

#[test]
fn piano_roll_coloured_renders_with_a_velocity_lane_and_sets_velocity_from_it() {
    use neu::{PianoRollModel, RollNote};
    struct Roll {
        model: PianoRollModel,
        notes: Vec<RollNote>,
    }
    let mut h = Harness::builder()
        .with_size(vec2(560.0, 640.0))
        .build_ui_state(
            |ui, roll: &mut Roll| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                let out = neu::piano_roll_coloured(
                    ui,
                    &theme,
                    &mut roll.model,
                    &roll.notes,
                    &[],
                    theme.track_colour(4),
                );
                if let Some((i, n)) = out.moved {
                    roll.notes[i] = n;
                }
            },
            Roll {
                model: PianoRollModel {
                    playhead_step: Some(2.5),
                    ..PianoRollModel::default()
                },
                notes: vec![
                    RollNote {
                        start: 2,
                        length: 3,
                        pitch: 48,
                        velocity: 0.9,
                        selected: true,
                    },
                    RollNote {
                        start: 6,
                        length: 1,
                        pitch: 52,
                        velocity: 0.4,
                        selected: false,
                    },
                ],
            },
        );
    h.run();
    let outer = h.get_by_label("Piano roll").rect();
    assert_eq!(outer.size(), PianoRollModel::default().size());
    let grid = neu::grid_rect(outer);
    let lane = neu::velocity_rect(outer);
    assert_eq!(lane.height(), neu::VELOCITY_LANE_HEIGHT);
    // Press in the lane over the second note's column, near the top: velocity ~1.
    let x = grid.min.x + 6.5 * h.state().model.step_width;
    let y = lane.min.y + 4.0;
    h.hover_at(pos2(x, y));
    h.run();
    h.drag_at(pos2(x, y));
    h.run();
    h.drop_at(pos2(x, y));
    h.run();
    assert!(
        h.state().notes[1].velocity > 0.9,
        "velocity {}",
        h.state().notes[1].velocity
    );
    assert_eq!(
        h.state().notes[0].velocity,
        0.9,
        "the other note is untouched"
    );
}

/// Renders a sheet of every non-floating widget through `Context::run_ui` and returns how
/// many shadow (blurred rect) shapes were emitted.
fn shadow_count(theme: &Theme, add: impl FnMut(&mut egui::Ui)) -> usize {
    let ctx = egui::Context::default();
    theme.apply(&ctx);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, add);
    out.textures_delta.clear();
    out.shapes
        .iter()
        .filter(|c| neu::is_shadow_shape(&c.shape))
        .count()
}

#[test]
fn no_widget_paints_a_shadow_except_the_floating_menu() {
    use neu::{KnobSize, KnobSpec, StepCellSpec};
    for theme in [Theme::dark(), Theme::light()] {
        let mode = theme.mode;
        let count = shadow_count(&theme, |ui| {
            let t = &theme;
            let mut f = 0.5;
            let mut n = 3u8;
            let mut sel = 0;
            let mut on = true;
            neu::panel(ui, t, |ui| {
                neu::window_frame_sized(
                    ui,
                    t,
                    vec2(500.0, 120.0),
                    "Window",
                    Some("sub"),
                    |ui| {
                        neu::header_button(ui, t, Icon::Clear);
                        neu::keycap(ui, t, "F5");
                    },
                    |ui| {
                        neu::toolbar(ui, t, |ui| {
                            neu::small_button(ui, t, "Draw");
                            neu::dropdown(ui, t, "Snap", "1/16", &["1/16"], 80.0, true, None);
                        });
                        neu::tab_strip(ui, t, &mut sel, &[("A", Some("F5")), ("B", None)]);
                    },
                );
                ui.horizontal(|ui| {
                    neu::button(ui, t, "Copy");
                    neu::primary_button(ui, t, "Export");
                    neu::ghost_button(ui, t, "Ghost");
                    neu::toggle_button(ui, t, "Follow", true);
                    neu::solo_button(ui, t, "S", true);
                    neu::rec_button(ui, t, true);
                    neu::icon_button(ui, t, Icon::Play, 28.0);
                    neu::rail_button(ui, t, Icon::Mixer, true, "Mixer");
                    neu::switch(ui, t, &mut on, "Switch");
                    neu::stepper_button(ui, t, Icon::Plus);
                    neu::segmented(ui, t, &mut sel, &["Pattern", "Song"]);
                    neu::led(ui, t, true, LedKind::Ok);
                    neu::tag(ui, t, "vinyl", TagKind::Vinyl);
                    neu::chip(ui, t, t.track_colour(0));
                    neu::number_box(ui, t, &mut n, 0..=16);
                    neu::name_button(ui, t, "Keys", Some(t.track_colour(1)), true);
                    neu::pattern_tile(ui, t, "A", true, true);
                });
                ui.horizontal(|ui| {
                    let spec = KnobSpec {
                        label: "Cutoff",
                        min: 0.0,
                        max: 1.0,
                        default: 0.5,
                        log: false,
                        bipolar: true,
                        size: KnobSize::Md,
                        fmt: |v| format!("{v:.2}"),
                    };
                    neu::knob(ui, t, &mut f, &spec);
                    neu::knob(
                        ui,
                        t,
                        &mut f,
                        &KnobSpec {
                            label: "",
                            size: KnobSize::Sm,
                            ..spec
                        },
                    );
                    neu::dial(ui, t, &mut f, &spec, neu::DialSize::Lg);
                    let s = SliderSpec {
                        label: "A",
                        min: 0.0,
                        max: 1.0,
                        default: 0.5,
                        fmt: |v| format!("{v:.2}"),
                    };
                    neu::vslider(ui, t, &mut f, &s);
                    neu::fader(ui, t, &mut f, &s);
                    neu::hslider(ui, t, &mut f, &s);
                    neu::stereo_meter(ui, t, (0.9, 0.95), neu::STEREO_METER_SIZE);
                    neu::gr_meter(ui, t, 0.2, 120.0);
                    neu::lamp(ui, t, true, "MIDI");
                    let mut i = 4;
                    neu::stepper(ui, t, "Oct", &mut i, 1..=7);
                    neu::step_row(
                        ui,
                        t,
                        "Kick",
                        &[
                            StepCellSpec {
                                on: true,
                                accent: true,
                                current: true,
                                ..StepCellSpec::default()
                            },
                            StepCellSpec::default(),
                        ],
                    );
                    neu::list_row(ui, t, true, |ui| {
                        neu::body(ui, t, "Row");
                    });
                });
                neu::hint_bar(ui, t, &[("Space", "Play")]);
                neu::drawer_handle(ui, t);
                let held = [false; 128];
                let labels = [None; 128];
                neu::piano_keys(
                    ui,
                    t,
                    &neu::PianoModel {
                        base_note: 60,
                        white_keys: 7,
                        held: &held,
                        labels: &labels,
                    },
                );
                let mut model = neu::PianoRollModel {
                    pitches: 8,
                    ..neu::PianoRollModel::default()
                };
                neu::piano_roll(ui, t, &mut model, &[]);
                neu::group(
                    ui,
                    t,
                    "Group",
                    |_| {},
                    |ui| {
                        neu::value(ui, t, "x");
                    },
                );
            });
        });
        assert_eq!(count, 0, "{mode:?}: a non-floating widget painted a shadow");

        // The menu is the one floating surface: exactly one shadow.
        let count = shadow_count(&theme, |ui| {
            neu::menu(ui, &theme, &[MenuItem::item("New"), MenuItem::Separator]);
        });
        assert_eq!(count, 1, "{mode:?}: the menu carries the floating shadow");
    }
}

#[test]
fn deck_text_tokens_read_on_every_surface_they_sit_on() {
    use neu::contrast_ratio;
    for theme in [Theme::dark(), Theme::light()] {
        let t = &theme;
        let check = |what: &str, fg, bg, min: f32| {
            let r = contrast_ratio(fg, bg);
            assert!(
                r >= min - 0.05,
                "{:?}: {what} is {r:.2}:1, needs {min}",
                t.mode
            );
        };
        // Buttons: text on panel-3 / panel-4 / panel-2 (pressed).
        check("text on panel-3", t.text, t.panel_3, 4.5);
        check("text on panel-4", t.text, t.panel_4, 4.5);
        check("text on panel-2", t.text, t.panel_2, 4.5);
        // Tabs / labels: text-2 on panel-2 and panel.
        check("text-2 on panel-2", t.text_2, t.panel_2, 4.5);
        check("text-2 on panel", t.text_2, t.panel, 4.5);
        // Primary buttons and ON steps.
        check("on-accent on accent", t.on_accent, t.accent, 4.5);
        check(
            "on-accent on accent-press",
            t.on_accent,
            t.accent_press,
            4.5,
        );
        // Solo / rec ON text on their washes over panel-3.
        let wash = |c: egui::Color32| t.panel_3.lerp_to_gamma(c, 0.16);
        check("accent on accent-wash", t.accent, wash(t.accent), 3.0);
        check("ok on ok-wash", t.ok, wash(t.ok), 3.0);
        check("rec on rec-wash", t.rec, wash(t.rec), 3.0);
        // Track colours carry the on-track text.
        for i in 0..8 {
            check("on-track on a track", t.on_track, t.track_colour(i), 4.5);
        }
        // Keycaps and hints are UI-level (3:1), never body text.
        check("text-3 on panel-2", t.text_3, t.panel_2, 3.0);
        // The black piano key's name.
        check("black key text", t.key_black_text, t.key_black_face[0], 4.5);
    }
}
