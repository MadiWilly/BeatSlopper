//! egui_kittest tests for the M4 polish: value tooltips on knobs, dials and sliders appear
//! after the themed delay with `"label: value"` text (styled through `Theme::apply`), and the
//! brand wordmark renders in both themes with the mock's size and tracking.

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::{
    DialSize, KnobSize, KnobSpec, POPUP_RADIUS, SliderSpec, TOOLTIP_DELAY, TextKind, Theme,
    ThemeMode,
};

fn pct(v: f32) -> String {
    format!("{v:.0} %")
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

const ROW_VOLUME: KnobSpec<'static> = KnobSpec {
    label: "",
    min: 0.0,
    max: 100.0,
    default: 80.0,
    log: false,
    bipolar: false,
    size: KnobSize::Sm,
    fmt: pct,
};

const MASTER: KnobSpec<'static> = KnobSpec {
    label: "Master",
    min: 0.0,
    max: 100.0,
    default: 75.0,
    log: false,
    bipolar: false,
    size: KnobSize::Md,
    fmt: pct,
};

const ATTACK: SliderSpec<'static> = SliderSpec {
    label: "A",
    min: 0.0,
    max: 100.0,
    default: 12.0,
    fmt: pct,
};

/// Frame length used by the tooltip tests: a third of the delay, so "one step after hovering"
/// is clearly before the tooltip is due and six steps clearly after.
const STEP: f32 = TOOLTIP_DELAY / 3.0;

/// All four value controls side by side, spaced so no tooltip can cover a neighbour.
fn controls(ui: &mut egui::Ui, state: &mut [f32; 4], mode: ThemeMode) {
    let theme = Theme::for_mode(mode);
    theme.apply(ui.ctx());
    ui.add_space(40.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 60.0;
        ui.add_space(20.0);
        neu::knob(ui, &theme, &mut state[0], &MIX);
        neu::knob(ui, &theme, &mut state[1], &ROW_VOLUME);
        neu::dial(ui, &theme, &mut state[2], &MASTER, DialSize::Sm);
        neu::vslider(ui, &theme, &mut state[3], &ATTACK);
    });
}

fn controls_harness(mode: ThemeMode) -> Harness<'static, [f32; 4]> {
    let mut harness = Harness::builder()
        .with_size(vec2(600.0, 320.0))
        .with_step_dt(STEP)
        .build_ui_state(
            move |ui, state: &mut [f32; 4]| controls(ui, state, mode),
            [50.0, 80.0, 75.0, 12.0],
        );
    harness.run();
    harness
}

/// Hovers the slider-role control named `label`, then checks that it has no tooltip one step
/// later and that a tooltip reading `tooltip` hangs under it once the delay has passed.
///
/// The control is found by role because an unlabelled knob is *named* by its tooltip text,
/// so a plain label query would match the control itself.
fn hover_and_expect_tooltip(harness: &mut Harness<'_, [f32; 4]>, label: &str, tooltip: &str) {
    harness.get_by_role_and_label(Role::Slider, label).hover();
    harness.step();
    assert!(
        harness
            .get_by_role_and_label(Role::Slider, label)
            .query_by_role(Role::Tooltip)
            .is_none(),
        "{label}: tooltip {tooltip:?} must wait for the {TOOLTIP_DELAY} s delay"
    );
    harness.run_steps(6);
    harness
        .get_by_role_and_label(Role::Slider, label)
        .get_by_role(Role::Tooltip)
        .get_by_label(tooltip);
}

#[test]
fn knob_tooltip_shows_label_and_value_after_the_delay() {
    let mut harness = controls_harness(ThemeMode::Dark);
    hover_and_expect_tooltip(&mut harness, "Mix", "Mix: 50 %");

    // The tooltip tracks the value.
    harness.state_mut()[0] = 72.0;
    harness.run_steps(2);
    harness.get_by_label("Mix: 72 %");
    assert!(harness.query_by_label("Mix: 50 %").is_none());
}

#[test]
fn unlabelled_knob_tooltip_is_just_the_value() {
    let mut harness = controls_harness(ThemeMode::Light);
    // The empty label means the tooltip text is also the control's accessible name.
    hover_and_expect_tooltip(&mut harness, "80 %", "80 %");
    assert!(harness.query_by_label(": 80 %").is_none());
    // Exactly one tooltip label is shown, under exactly one slider.
    assert_eq!(harness.get_all_by_role(Role::Tooltip).count(), 1);
}

#[test]
fn dial_and_slider_show_tooltips() {
    let mut harness = controls_harness(ThemeMode::Dark);
    hover_and_expect_tooltip(&mut harness, "Master", "Master: 75 %");
    hover_and_expect_tooltip(&mut harness, "A", "A: 12 %");
}

#[test]
fn dimmed_knob_stays_live_and_names_the_reason_in_its_tooltip() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut harness = Harness::builder()
            .with_size(vec2(300.0, 300.0))
            .with_step_dt(STEP)
            .build_ui_state(
                move |ui, value: &mut f32| {
                    let theme = Theme::for_mode(mode);
                    theme.apply(ui.ctx());
                    ui.add_space(60.0);
                    ui.horizontal(|ui| {
                        ui.add_space(60.0);
                        neu::knob_dimmed(
                            ui,
                            &theme,
                            value,
                            &KnobSpec {
                                label: "Pulse",
                                ..MIX
                            },
                            Some("Square wave only"),
                        );
                    });
                },
                50.0,
            );
        harness.run();
        harness.get_by_role_and_label(Role::Slider, "Pulse").hover();
        harness.run_steps(7);
        harness
            .get_by_role_and_label(Role::Slider, "Pulse")
            .get_by_role(Role::Tooltip)
            .get_by_label("Pulse: 50 % · Square wave only");
        // Still interactive: a click focuses it and the arrow keys step it like any knob.
        harness.get_by_role_and_label(Role::Slider, "Pulse").click();
        harness.run();
        harness.key_press(egui::Key::ArrowUp);
        harness.run();
        assert!((*harness.state() - 52.0).abs() < 1e-3, "{mode:?}");
    }
}

#[test]
fn tooltip_text_formats_label_and_value() {
    assert_eq!(neu::tooltip_text("Mix", "50 %"), "Mix: 50 %");
    assert_eq!(neu::tooltip_text("", "50 %"), "50 %");
}

#[test]
fn theme_styles_tooltips_and_sets_the_delay() {
    for theme in [Theme::dark(), Theme::light()] {
        let ctx = egui::Context::default();
        theme.apply(&ctx);
        let style = ctx.style_of(theme.egui_theme());
        let v = &style.visuals;
        assert_eq!(v.window_fill, theme.surface);
        assert_eq!(v.window_stroke, egui::Stroke::NONE);
        assert_eq!(v.override_text_color, Some(theme.text_primary));
        assert_eq!(v.popup_shadow, theme.popup_shadow());
        assert_ne!(v.popup_shadow, egui::epaint::Shadow::NONE);
        assert_eq!(v.popup_shadow.offset, [3, 3]);
        assert_eq!(v.popup_shadow.blur, 8);
        assert_eq!(v.popup_shadow.spread, 0);
        assert_eq!(v.popup_shadow.color, theme.shadow_dark);
        assert_eq!(
            v.window_corner_radius,
            egui::CornerRadius::same(POPUP_RADIUS)
        );
        assert_eq!(v.menu_corner_radius, egui::CornerRadius::same(POPUP_RADIUS));
        assert_eq!(POPUP_RADIUS, 12);
        assert_eq!(style.interaction.tooltip_delay, TOOLTIP_DELAY);
        assert_eq!(TOOLTIP_DELAY, 0.3);
        // The popup frame egui builds for tooltips picks these up.
        let frame = egui::Frame::popup(&style);
        assert_eq!(frame.fill, theme.surface);
        assert_eq!(frame.stroke, egui::Stroke::NONE);
        assert_eq!(frame.shadow, theme.popup_shadow());
        assert_eq!(frame.corner_radius, egui::CornerRadius::same(POPUP_RADIUS));
    }
}

#[test]
fn brand_kind_has_the_mock_size_and_tracking() {
    let kind = TextKind::Brand;
    assert_eq!(kind.size(), 15.0);
    assert!(
        (kind.letter_spacing() - 2.4).abs() < 1e-5,
        "+0.16em of 15 pt"
    );
    assert!(kind.is_caps());
    assert!(
        !kind.is_medium(),
        "the brand is weight 300, never faux-bold"
    );
    assert_eq!(kind.display_string("BeatSlopper"), "BEATSLOPPER");
    let format = kind.format(egui::Color32::WHITE);
    assert_eq!(format.font_id, egui::FontId::proportional(15.0));
    assert!((format.extra_letter_spacing - 2.4).abs() < 1e-5);
    // The other caps levels keep +0.12em.
    assert!((TextKind::Label.letter_spacing() - 1.32).abs() < 1e-5);
}

#[test]
fn brand_renders_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut harness = Harness::builder()
            .with_size(vec2(400.0, 100.0))
            .build_ui(move |ui| {
                let theme = Theme::for_mode(mode);
                theme.apply(ui.ctx());
                ui.horizontal(|ui| {
                    neu::brand(ui, &theme, "BeatSlopper");
                    neu::title(ui, &theme, "Transport");
                });
            });
        harness.run();
        let brand = harness.get_by_label("BeatSlopper");
        let title = harness.get_by_label("Transport");
        let brand_rect = brand.rect();
        assert!(
            brand_rect.height() > title.rect().height(),
            "15 pt brand is taller than the 12 pt title ({mode:?})"
        );
        // Eleven tracked 15 pt capitals are far wider than the same text untracked at 12 pt.
        assert!(
            brand_rect.width() > 110.0,
            "{mode:?}: {}",
            brand_rect.width()
        );
    }
}
