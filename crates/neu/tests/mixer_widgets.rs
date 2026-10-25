//! egui_kittest tests for the M6 mixer widgets: the fader follows the pointer over its 140 pt
//! track and resets on a double-click, the meters render for silence, -6 dBFS, full scale and
//! `NaN` without panicking, the square toggle reports clicks and renders ON / OFF in both
//! themes, and the dB mapping helper matches the design's curve.

use egui::{Key, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::{SliderSpec, Theme, ThemeMode};

fn db(v: f32) -> String {
    format!("{v:+.1} dB")
}

const LEVEL: SliderSpec<'static> = SliderSpec {
    label: "Level",
    min: -60.0,
    max: 6.0,
    default: 0.0,
    fmt: db,
};

fn fader_harness(mode: ThemeMode) -> Harness<'static, f32> {
    let mut harness = Harness::builder()
        .with_size(vec2(200.0, 320.0))
        .with_step_dt(0.01)
        // A hover starts the tooltip countdown, which repaints until the pointer is still;
        // let `run()` settle through it.
        .with_max_steps(32)
        .build_ui_state(
            move |ui, value: &mut f32| {
                let theme = Theme::for_mode(mode);
                theme.apply(ui.ctx());
                ui.add_space(40.0);
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    neu::fader(ui, &theme, value, &LEVEL);
                });
            },
            -12.0,
        );
    harness.run();
    harness
}

#[test]
fn fader_press_and_drag_set_the_value_proportional_to_position() {
    let mut harness = fader_harness(ThemeMode::Dark);
    let hit = harness.get_by_label("Level").rect();
    assert_eq!(hit.size(), vec2(neu::SLIDER_WIDTH, neu::FADER_TRACK.y));
    let track = Rect::from_center_size(hit.center(), neu::FADER_TRACK);
    let x = hit.center().x;
    let y_at = |frac_from_top: f32| {
        track.min.y + neu::SLIDER_THUMB * 0.5 + frac_from_top * neu::FADER_TRAVEL
    };

    // Press a quarter of the travel down from the top: 75 % of -60..=6 is -10.5 dB.
    harness.hover_at(pos2(x, y_at(0.25)));
    harness.run();
    harness.drag_at(pos2(x, y_at(0.25)));
    harness.run();
    let v = *harness.state();
    assert!((v + 10.5).abs() < 0.3, "expected ~-10.5, got {v}");

    // Drag to 90 % down (10 % of the range): -53.4 dB, then past the bottom (clamps).
    harness.hover_at(pos2(x, y_at(0.9)));
    harness.run();
    let v = *harness.state();
    assert!((v + 53.4).abs() < 0.3, "expected ~-53.4, got {v}");
    harness.hover_at(pos2(x, y_at(0.9) + 300.0));
    harness.run();
    assert_eq!(*harness.state(), -60.0);
    harness.drop_at(pos2(x, y_at(0.9) + 300.0));
    harness.run();

    // The helper and the widget agree, over the fader's own travel.
    assert!((neu::vslider_fraction_at(track, y_at(0.25)) - 0.75).abs() < 1e-5);
    assert!((neu::vslider_thumb_pos(track, 0.75).y - y_at(0.25)).abs() < 1e-4);
}

#[test]
fn fader_double_click_resets_to_default() {
    let mut harness = fader_harness(ThemeMode::Light);
    *harness.state_mut() = -37.0;
    harness.run();
    assert_eq!(*harness.state(), -37.0);
    harness.get_by_label("Level").click();
    harness.get_by_label("Level").click();
    harness.run();
    assert_eq!(*harness.state(), 0.0);
}

#[test]
fn fader_arrow_keys_step_only_while_focused() {
    const PAN: SliderSpec<'static> = SliderSpec {
        label: "Pan",
        min: -60.0,
        max: 6.0,
        default: 0.0,
        fmt: db,
    };
    let mut harness = Harness::builder()
        .with_size(vec2(240.0, 320.0))
        .with_step_dt(0.01)
        .with_max_steps(32)
        .build_ui_state(
            |ui, values: &mut (f32, f32)| {
                let theme = Theme::for_mode(ThemeMode::Dark);
                theme.apply(ui.ctx());
                ui.add_space(40.0);
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    neu::fader(ui, &theme, &mut values.0, &LEVEL);
                    ui.add_space(20.0);
                    neu::fader(ui, &theme, &mut values.1, &PAN);
                });
            },
            (-12.0, -12.0),
        );
    harness.run();
    assert!(!harness.get_by_label("Level").is_focused());

    // Unfocused: ArrowUp changes nothing.
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(*harness.state(), (-12.0, -12.0));

    // Click the Level fader: it takes focus (and wears the ring); ArrowUp steps 1/50 of the
    // 66 dB range (1.32 dB) on it alone.
    harness.get_by_label("Level").click();
    harness.run();
    assert!(harness.get_by_label("Level").is_focused());
    assert!(!harness.get_by_label("Pan").is_focused());
    let (level, pan) = *harness.state();
    harness.key_press(Key::ArrowUp);
    harness.run();
    let (after_level, after_pan) = *harness.state();
    assert!(
        (after_level - (level + 1.32)).abs() < 1e-3,
        "expected {level} + 1.32, got {after_level}"
    );
    assert_eq!(after_pan, pan, "the unfocused fader is untouched");

    // Focus surrendered: ArrowUp is ignored again.
    harness.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    harness.run();
    assert!(!harness.get_by_label("Level").is_focused());
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(*harness.state(), (after_level, after_pan));
}

#[test]
fn meters_render_for_every_level_including_nan() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let mut harness = Harness::builder()
            .with_size(vec2(200.0, 240.0))
            .build_ui_state(
                move |ui, level: &mut f32| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        neu::meter(ui, &theme, *level, neu::METER_TRACK);
                        neu::stereo_meter(
                            ui,
                            &theme,
                            (*level, *level * 0.5),
                            neu::STEREO_METER_SIZE,
                        );
                        // An odd size: narrower than two stereo tracks, and short.
                        neu::stereo_meter(ui, &theme, (*level, *level), vec2(5.0, 20.0));
                    });
                },
                0.0,
            );
        for level in [0.0, 0.5, 1.0, f32::NAN, -0.3, 4.0] {
            *harness.state_mut() = level;
            harness.run();
            harness.get_by_label("Level");
            assert_eq!(
                harness.get_all_by_label("Stereo level").count(),
                2,
                "{mode:?} level {level}"
            );
        }
    }
}

#[test]
fn sized_toggle_click_reports_and_renders_on_off_in_both_themes() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let mut harness = Harness::builder()
            .with_size(vec2(200.0, 120.0))
            .build_ui_state(
                move |ui, s: &mut (bool, bool, u32)| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    ui.horizontal(|ui| {
                        if neu::toggle_button_sized(ui, &theme, "M", s.0, neu::TOGGLE_SQUARE)
                            .clicked()
                        {
                            s.0 = !s.0;
                            s.2 += 1;
                        }
                        if neu::toggle_button_sized(ui, &theme, "S", s.1, vec2(40.0, 28.0))
                            .clicked()
                        {
                            s.1 = !s.1;
                            s.2 += 1;
                        }
                    });
                },
                (false, true, 0),
            );
        harness.run();
        assert_eq!(harness.get_by_label("M").rect().size(), neu::TOGGLE_SQUARE);
        assert_eq!(harness.get_by_label("S").rect().size(), vec2(40.0, 28.0));

        // Hover (ON tint path), then click: ON. Click again: OFF.
        harness.get_by_label("M").hover();
        harness.run();
        harness.get_by_label("M").click();
        harness.run();
        assert_eq!(*harness.state(), (true, true, 1), "{mode:?}");
        harness.run();
        assert_eq!(*harness.state(), (true, true, 1), "{mode:?}: no repeat");
        harness.get_by_label("M").click();
        harness.run();
        assert_eq!(*harness.state(), (false, true, 2), "{mode:?}");
        harness.get_by_label("S").click();
        harness.run();
        assert_eq!(*harness.state(), (false, false, 3), "{mode:?}");
    }
}

#[test]
fn meter_fraction_follows_the_db_curve() {
    assert_eq!(neu::meter_fraction(0.0), 0.0);
    assert_eq!(neu::meter_fraction(1.0), 1.0);
    assert_eq!(neu::meter_fraction(0.001), 0.0);
    let half = neu::meter_fraction(0.5);
    assert!((half - 0.9).abs() < 0.002, "0.5 -> {half}");
    assert_eq!(neu::meter_fraction(f32::NAN), 0.0);
    assert_eq!(neu::meter_fraction(2.0), 1.0);
    // Monotonic between the floor and full scale.
    let mut last = 0.0;
    for i in 1..=100 {
        let f = neu::meter_fraction(i as f32 / 100.0);
        assert!(f >= last, "not monotonic at {i}");
        last = f;
    }
}

/// M11: `fader_sized` fits its track to the given height (never under `FADER_MIN_TRACK`),
/// drops the caps label when asked (the block is shorter by the label row) and still reports
/// the value, follows the pointer and names itself for accessibility.
#[test]
fn fader_sized_fits_the_track_and_can_drop_its_label() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut harness = Harness::builder()
            .with_size(vec2(260.0, 300.0))
            .with_step_dt(0.01)
            .with_max_steps(32)
            .build_ui_state(
                move |ui, values: &mut [f32; 3]| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    ui.add_space(20.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(20.0);
                        let a = neu::fader_sized(ui, &theme, &mut values[0], &LEVEL, 90.0, true);
                        ui.add_space(20.0);
                        let short = SliderSpec {
                            label: "Short",
                            ..LEVEL
                        };
                        let b = neu::fader_sized(ui, &theme, &mut values[1], &short, 90.0, false);
                        ui.add_space(20.0);
                        let tiny = SliderSpec {
                            label: "Tiny",
                            ..LEVEL
                        };
                        let c = neu::fader_sized(ui, &theme, &mut values[2], &tiny, 1.0, false);
                        ui.data_mut(|d| {
                            d.insert_temp(egui::Id::NULL.with("rects"), [a.rect, b.rect, c.rect]);
                        });
                    });
                },
                [-12.0, -12.0, -12.0],
            );
        harness.run();
        let [a, b, c]: [Rect; 3] = harness
            .ctx
            .data(|d| d.get_temp(egui::Id::NULL.with("rects")))
            .unwrap();
        assert_eq!(
            a.height(),
            90.0,
            "the interactive column is the track's height"
        );
        assert_eq!(b.height(), 90.0);
        assert_eq!(c.height(), neu::FADER_MIN_TRACK, "a 1 pt track is clamped");
        // Both 90 pt faders start their track at the same y (the value row above is the
        // same); the unlabelled one has nothing under it.
        assert_eq!(a.min.y, b.min.y);
        let labelled = harness.get_by_label("Level");
        let bare = harness.get_by_label("Short");
        assert_eq!(labelled.rect().height(), 90.0);
        assert_eq!(bare.rect().height(), 90.0);
        harness.get_by_label("Tiny");

        // The bare fader still follows the pointer: a click at its top is the maximum.
        let top = pos2(b.center().x, b.min.y + neu::SLIDER_THUMB * 0.5);
        harness.event(egui::Event::PointerMoved(top));
        harness.event(egui::Event::PointerButton {
            pos: top,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.event(egui::Event::PointerButton {
            pos: top,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        assert_eq!(harness.state()[1], 6.0);
        assert_eq!(harness.state()[0], -12.0, "the neighbour is untouched");
    }
}
