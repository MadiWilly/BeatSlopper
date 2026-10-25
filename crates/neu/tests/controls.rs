//! egui_kittest tests for the M2 controls: knob drags move and clamp the value, double-click
//! resets, a log knob maps the midpoint drag to the geometric mean, the segmented selector
//! reports one change per click, the vertical slider follows the pointer, and everything
//! renders in both themes.

use egui::{Key, Modifiers, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::{DialSize, KnobSize, KnobSpec, SliderSpec, Theme, ThemeMode};

fn pct(v: f32) -> String {
    format!("{v:.0} %")
}

fn hz(v: f32) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

const MIX: KnobSpec<'static> = KnobSpec {
    label: "Mix",
    min: 0.0,
    max: 100.0,
    default: 50.0,
    log: false,
    bipolar: false,
    size: KnobSize::Md,
    fmt: pct,
};

const CUTOFF: KnobSpec<'static> = KnobSpec {
    label: "Cutoff",
    min: 20.0,
    max: 20000.0,
    default: 20.0,
    log: true,
    bipolar: false,
    size: KnobSize::Md,
    fmt: hz,
};

const ATTACK: SliderSpec<'static> = SliderSpec {
    label: "A",
    min: 0.0,
    max: 100.0,
    default: 12.0,
    fmt: pct,
};

/// Press at `from`, move to `to` (one event), release there.
fn drag(harness: &mut Harness<'_, f32>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.run();
    harness.hover_at(to);
    harness.run();
    harness.drop_at(to);
    harness.run();
}

fn knob_harness(spec: KnobSpec<'static>, start: f32) -> Harness<'static, f32> {
    let mut harness = Harness::builder()
        .with_size(vec2(300.0, 300.0))
        .with_step_dt(0.01)
        .build_ui_state(
            move |ui, value: &mut f32| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                ui.add_space(100.0);
                ui.horizontal(|ui| {
                    ui.add_space(100.0);
                    neu::knob(ui, &theme, value, &spec);
                });
            },
            start,
        );
    harness.run();
    harness
}

#[test]
fn knob_drag_moves_in_the_right_direction_and_clamps() {
    let mut harness = knob_harness(MIX, 50.0);
    let centre = harness.get_by_label("Mix").rect().center();

    // Up 30 px of the 150 px range: +20 of 100.
    drag(&mut harness, centre, centre - vec2(0.0, 30.0));
    let v = *harness.state();
    assert!((v - 70.0).abs() < 0.5, "expected ~70, got {v}");

    // Down 15 px: -10.
    drag(&mut harness, centre, centre + vec2(0.0, 15.0));
    let v = *harness.state();
    assert!((v - 60.0).abs() < 0.5, "expected ~60, got {v}");

    // Far past the range in either direction clamps.
    drag(&mut harness, centre, centre - vec2(0.0, 400.0));
    assert_eq!(*harness.state(), 100.0);
    drag(&mut harness, centre, centre + vec2(0.0, 900.0));
    assert_eq!(*harness.state(), 0.0);

    // Shift makes the same travel four times finer.
    harness.hover_at(centre);
    harness.run();
    harness.event_modifiers(
        egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::SHIFT,
        },
        Modifiers::SHIFT,
    );
    harness.run();
    harness.event_modifiers(
        egui::Event::PointerMoved(centre - vec2(0.0, 60.0)),
        Modifiers::SHIFT,
    );
    harness.run();
    harness.event_modifiers(
        egui::Event::PointerButton {
            pos: centre - vec2(0.0, 60.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::SHIFT,
        },
        Modifiers::SHIFT,
    );
    harness.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    harness.run();
    let v = *harness.state();
    assert!((v - 10.0).abs() < 0.5, "expected ~10 (60 px / 4), got {v}");
}

#[test]
fn knob_double_click_resets_to_default() {
    let mut harness = knob_harness(MIX, 50.0);
    *harness.state_mut() = 83.0;
    harness.run();
    assert_eq!(*harness.state(), 83.0);
    harness.get_by_label("Mix").click();
    harness.get_by_label("Mix").click();
    harness.run();
    assert_eq!(*harness.state(), 50.0);
}

#[test]
fn knob_arrow_keys_step_a_fiftieth_and_home_resets() {
    let mut harness = knob_harness(MIX, 50.0);
    harness.get_by_label("Mix").click();
    harness.run();
    assert!(harness.get_by_label("Mix").is_focused());
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert!((*harness.state() - 52.0).abs() < 1e-3);
    harness.key_press(Key::ArrowDown);
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert!((*harness.state() - 48.0).abs() < 1e-3);
    harness.key_press(Key::Home);
    harness.run();
    assert_eq!(*harness.state(), 50.0);
}

#[test]
fn knob_out_of_range_value_is_clamped_without_a_change() {
    let mut harness = Harness::builder().build_ui_state(
        |ui, s: &mut (f32, u32)| {
            let theme = Theme::light();
            if neu::knob(ui, &theme, &mut s.0, &MIX).changed() {
                s.1 += 1;
            }
        },
        (250.0, 0),
    );
    harness.run();
    assert_eq!(harness.state().0, 100.0);
    assert_eq!(
        harness.state().1,
        1,
        "pulling into range counts as a change once"
    );
    harness.run();
    assert_eq!(harness.state().1, 1);
}

#[test]
fn log_knob_midpoint_drag_lands_on_the_geometric_mean() {
    let mut harness = knob_harness(CUTOFF, 20.0);
    let centre = harness.get_by_label("Cutoff").rect().center();
    // Half the 150 px range: the fraction goes 0 -> 0.5, i.e. sqrt(20 * 20000) = 632.5 Hz.
    drag(&mut harness, centre, centre - vec2(0.0, 75.0));
    let v = *harness.state();
    let mean = (20.0_f32 * 20000.0).sqrt();
    assert!((v - mean).abs() / mean < 0.01, "expected ~{mean}, got {v}");
    // The same travel again reaches the maximum (and clamps there).
    drag(&mut harness, centre, centre - vec2(0.0, 75.0));
    let v = *harness.state();
    assert!(
        (v - 20000.0).abs() / 20000.0 < 0.01,
        "expected ~20000, got {v}"
    );
    drag(&mut harness, centre, centre - vec2(0.0, 75.0));
    assert_eq!(*harness.state(), 20000.0);
}

#[test]
fn dial_drags_like_a_knob_in_both_sizes() {
    for size in [DialSize::Lg, DialSize::Sm] {
        let mut harness = Harness::builder()
            .with_size(vec2(400.0, 300.0))
            .with_step_dt(0.01)
            .build_ui_state(
                move |ui, value: &mut f32| {
                    let theme = Theme::light();
                    theme.apply(ui.ctx());
                    ui.add_space(60.0);
                    ui.horizontal(|ui| {
                        ui.add_space(60.0);
                        neu::dial(ui, &theme, value, &MIX, size);
                    });
                },
                50.0,
            );
        harness.run();
        let centre = harness.get_by_label("Mix").rect().center();
        drag(&mut harness, centre, centre - vec2(0.0, 30.0));
        let v = *harness.state();
        assert!((v - 70.0).abs() < 0.5, "{size:?}: expected ~70, got {v}");
        harness.get_by_label("Mix").click();
        harness.get_by_label("Mix").click();
        harness.run();
        assert_eq!(*harness.state(), 50.0, "{size:?}");
    }
}

#[test]
fn segmented_click_selects_and_reports_one_change() {
    let mut harness = Harness::builder().build_ui_state(
        |ui, s: &mut (usize, u32)| {
            let theme = Theme::dark();
            theme.apply(ui.ctx());
            if neu::segmented(ui, &theme, &mut s.0, &["Sine", "Saw", "Square", "Tri"]).changed() {
                s.1 += 1;
            }
        },
        (0, 0),
    );
    harness.run();
    assert_eq!(*harness.state(), (0, 0));

    harness.get_by_label("Saw").click();
    harness.run();
    assert_eq!(*harness.state(), (1, 1));
    harness.run();
    harness.run();
    assert_eq!(
        *harness.state(),
        (1, 1),
        "changed() only on the frame it moved"
    );

    // Clicking the selected item again is not a change.
    harness.get_by_label("Saw").click();
    harness.run();
    assert_eq!(*harness.state(), (1, 1));

    harness.get_by_label("Tri").click();
    harness.run();
    assert_eq!(*harness.state(), (3, 2));

    // Keyboard: the focused item moves the selection left/right and clamps.
    harness.key_press(Key::ArrowLeft);
    harness.run();
    assert_eq!(*harness.state(), (2, 3));
    harness.key_press(Key::ArrowRight);
    harness.key_press(Key::ArrowRight);
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(*harness.state(), (3, 4));
}

#[test]
fn segmented_clamps_a_stale_selection() {
    let mut harness = Harness::builder().build_ui_state(
        |ui, s: &mut usize| {
            let theme = Theme::light();
            neu::segmented(ui, &theme, s, &["Pitch", "Cutoff"]);
        },
        7usize,
    );
    harness.run();
    assert_eq!(*harness.state(), 1);
}

#[test]
fn vslider_press_and_drag_follow_the_pointer() {
    let mut harness = Harness::builder()
        .with_size(vec2(200.0, 300.0))
        .with_step_dt(0.01)
        .build_ui_state(
            |ui, value: &mut f32| {
                let theme = Theme::dark();
                theme.apply(ui.ctx());
                ui.add_space(40.0);
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    neu::vslider(ui, &theme, value, &ATTACK);
                });
            },
            12.0,
        );
    harness.run();
    let hit = harness.get_by_label("A").rect();
    assert_eq!(hit.size(), vec2(neu::SLIDER_WIDTH, neu::SLIDER_TRACK.y));
    let track = egui::Rect::from_center_size(hit.center(), neu::SLIDER_TRACK);
    let x = hit.center().x;

    // Press a quarter of the travel down from the top: 75 %.
    let y75 = track.min.y + neu::SLIDER_THUMB * 0.5 + 0.25 * neu::SLIDER_TRAVEL;
    harness.hover_at(pos2(x, y75));
    harness.run();
    harness.drag_at(pos2(x, y75));
    harness.run();
    let v = *harness.state();
    assert!((v - 75.0).abs() < 0.5, "expected ~75, got {v}");

    // Drag to 10 % and then far below the track (clamps to 0).
    let y10 = track.min.y + neu::SLIDER_THUMB * 0.5 + 0.9 * neu::SLIDER_TRAVEL;
    harness.hover_at(pos2(x, y10));
    harness.run();
    let v = *harness.state();
    assert!((v - 10.0).abs() < 0.5, "expected ~10, got {v}");
    harness.hover_at(pos2(x, y10 + 300.0));
    harness.run();
    assert_eq!(*harness.state(), 0.0);
    harness.drop_at(pos2(x, y10 + 300.0));
    harness.run();

    // The helper agrees with the widget.
    assert!((neu::vslider_fraction_at(track, y75) - 0.75).abs() < 1e-5);

    // Double-click resets.
    harness.get_by_label("A").click();
    harness.get_by_label("A").click();
    harness.run();
    assert_eq!(*harness.state(), 12.0);
}

#[test]
fn renders_in_both_themes() {
    #[derive(Clone)]
    struct S {
        detune: f32,
        mix: f32,
        cutoff: f32,
        rate: f32,
        master: f32,
        wave: usize,
        adsr: [f32; 4],
        on: bool,
    }
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let mut harness = Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .build_ui_state(
                move |ui, s: &mut S| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    neu::panel(ui, &theme, |ui| {
                        ui.horizontal(|ui| {
                            neu::knob(
                                ui,
                                &theme,
                                &mut s.detune,
                                &KnobSpec {
                                    label: "Detune",
                                    min: -50.0,
                                    max: 50.0,
                                    default: 0.0,
                                    log: false,
                                    bipolar: true,
                                    size: KnobSize::Md,
                                    fmt: |v| format!("{v:.0} ct"),
                                },
                            );
                            neu::knob(ui, &theme, &mut s.mix, &MIX);
                            neu::knob(
                                ui,
                                &theme,
                                &mut s.rate,
                                &KnobSpec {
                                    label: "",
                                    size: KnobSize::Sm,
                                    ..MIX
                                },
                            );
                            neu::knob(
                                ui,
                                &theme,
                                &mut s.rate,
                                &KnobSpec {
                                    label: "Rate",
                                    size: KnobSize::Sm,
                                    ..CUTOFF
                                },
                            );
                            neu::dial(ui, &theme, &mut s.cutoff, &CUTOFF, DialSize::Sm);
                            neu::dial(
                                ui,
                                &theme,
                                &mut s.master,
                                &KnobSpec {
                                    label: "Master",
                                    ..MIX
                                },
                                DialSize::Lg,
                            );
                        });
                        ui.horizontal(|ui| {
                            neu::segmented(
                                ui,
                                &theme,
                                &mut s.wave,
                                &["Sine", "Saw", "Square", "Tri"],
                            );
                            neu::segmented(ui, &theme, &mut s.wave, &[]);
                            for (i, l) in ["A", "D", "S", "R"].iter().enumerate() {
                                neu::vslider(
                                    ui,
                                    &theme,
                                    &mut s.adsr[i],
                                    &SliderSpec { label: l, ..ATTACK },
                                );
                            }
                            neu::lamp(ui, &theme, s.on, "Active");
                            neu::lamp(ui, &theme, !s.on, "Idle");
                        });
                    });
                },
                S {
                    detune: 12.0,
                    mix: 70.0,
                    cutoff: 2400.0,
                    rate: 4.5,
                    master: 80.0,
                    wave: 1,
                    adsr: [12.0, 40.0, 70.0, 30.0],
                    on: true,
                },
            );
        harness.run();
        // Hover and press paths: a knob, a dial, an item and a slider thumb.
        for label in ["Detune", "Master", "Square", "S"] {
            harness.get_by_label(label).hover();
            harness.run();
            harness.get_by_label(label).click();
            harness.run();
        }
        // Extremes of the bipolar and log knobs, both lamp states.
        harness.state_mut().detune = -50.0;
        harness.state_mut().cutoff = 20000.0;
        harness.state_mut().on = false;
        harness.run();
        harness.state_mut().detune = 50.0;
        harness.state_mut().cutoff = 20.0;
        harness.run();
        assert_eq!(harness.state().wave, 2);
    }
}
