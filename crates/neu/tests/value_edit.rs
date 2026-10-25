//! egui_kittest: the QOL typed value entry and wheel nudge on the kit's controls: a click on
//! a knob's readout opens the inline editor, typing + Enter commits the parsed value (units
//! tolerated, clamped), Escape cancels, the wheel nudges knobs / sliders / steppers.

use egui::{Event, Key, Modifiers, MouseWheelUnit, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::slider::SliderSpec;
use neu::{KnobSize, KnobSpec, Theme};

fn hz(v: f32) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

fn pct(v: f32) -> String {
    format!("{} %", (v * 100.0).round() as i32)
}

const CUTOFF: KnobSpec<'static> = KnobSpec {
    label: "Cutoff",
    min: 20.0,
    max: 20_000.0,
    default: 2400.0,
    log: true,
    bipolar: false,
    size: KnobSize::Md,
    fmt: hz,
};

fn knob_harness() -> Harness<'static, f32> {
    let mut harness = Harness::builder()
        .with_size(vec2(200.0, 200.0))
        .build_ui_state(
            |ui, v: &mut f32| {
                let theme = Theme::dark();
                neu::knob(ui, &theme, v, &CUTOFF);
            },
            350.0,
        );
    harness.run_steps(2);
    harness
}

fn type_and_enter(harness: &mut Harness<'_, f32>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run_steps(2);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
}

#[test]
fn clicking_the_readout_opens_the_editor_and_enter_commits_with_units() {
    let mut harness = knob_harness();
    assert_eq!(
        harness.get_by_label("Cutoff value").value().as_deref(),
        Some("350 Hz")
    );
    harness.get_by_label("Cutoff value").click();
    harness.run_steps(2);
    // The pre-filled text is selected: typing replaces it.
    type_and_enter(&mut harness, "2.4k");
    let v = *harness.state();
    assert!((v - 2400.0).abs() < 1.0, "{v}");
    assert_eq!(
        harness.get_by_label("Cutoff value").value().as_deref(),
        Some("2.4 kHz")
    );

    // Clamped to the range; a unit the control shows is fine.
    harness.get_by_label("Cutoff value").click();
    harness.run_steps(2);
    type_and_enter(&mut harness, "99999 Hz");
    assert_eq!(*harness.state(), 20_000.0);

    // Garbage leaves the value alone.
    harness.get_by_label("Cutoff value").click();
    harness.run_steps(2);
    type_and_enter(&mut harness, "loud");
    assert_eq!(*harness.state(), 20_000.0);
}

#[test]
fn escape_cancels_and_a_right_click_opens_the_editor_too() {
    let mut harness = knob_harness();
    harness.get_by_label("Cutoff value").click();
    harness.run_steps(2);
    harness.event(Event::Text("900".to_owned()));
    harness.run_steps(2);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(*harness.state(), 350.0, "Escape keeps the old value");

    harness.get_by_label("Cutoff").click_secondary();
    harness.run_steps(2);
    type_and_enter(&mut harness, "900");
    let v = *harness.state();
    assert!((v - 900.0).abs() < 1.0, "{v}");
}

#[test]
fn the_wheel_nudges_a_knob_and_shift_makes_it_fine() {
    let mut harness = knob_harness();
    harness.get_by_label("Cutoff").hover();
    harness.run_steps(1);
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, 1.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
    harness.run_steps(2);
    let up = *harness.state();
    let expected = CUTOFF.from_frac(CUTOFF.to_frac(350.0) + neu::knob::KEY_STEP);
    assert!((up - expected).abs() < 0.5, "{up} vs {expected}");

    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, -1.0),
        modifiers: Modifiers::SHIFT,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
    harness.run_steps(2);
    let fine = *harness.state();
    assert!(fine < up && fine > 350.0, "{fine}");
}

#[test]
fn sliders_and_steppers_take_typed_values_and_wheel_steps() {
    let spec = SliderSpec {
        label: "Mix",
        min: 0.0,
        max: 1.0,
        default: 0.5,
        fmt: pct,
    };
    let mut harness = Harness::builder()
        .with_size(vec2(200.0, 260.0))
        .build_ui_state(
            move |ui, v: &mut f32| {
                let theme = Theme::light();
                neu::vslider(ui, &theme, v, &spec);
            },
            0.5,
        );
    harness.run_steps(2);
    harness.get_by_label("Mix value").click();
    harness.run_steps(2);
    harness.event(Event::Text("70 %".to_owned()));
    harness.run_steps(2);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(*harness.state(), 0.7);

    let mut harness = Harness::builder()
        .with_size(vec2(200.0, 100.0))
        .build_ui_state(
            |ui, v: &mut i32| {
                let theme = Theme::dark();
                neu::stepper(ui, &theme, "Octave", v, 1..=7);
            },
            4,
        );
    harness.run_steps(2);
    harness.get_by_label("Octave value").click();
    harness.run_steps(2);
    harness.event(Event::Text("6".to_owned()));
    harness.run_steps(2);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(*harness.state(), 6);

    harness.get_by_label("Increase").hover();
    harness.run_steps(1);
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, -2.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
    harness.run_steps(2);
    assert_eq!(*harness.state(), 4);
}
