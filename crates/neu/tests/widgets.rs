//! egui_kittest tests for the M0 widgets: the theme toggle flips the mode, button clicks are
//! reported, and every widget renders in both themes without panicking.

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::{Icon, Theme, ThemeMode};

fn all_widgets(ui: &mut egui::Ui, mode: ThemeMode) {
    let theme = Theme::for_mode(mode);
    let mut mode = mode;
    neu::panel(ui, &theme, |ui| {
        neu::title(ui, &theme, "Transport");
        neu::label(ui, &theme, "Bpm");
        neu::value(ui, &theme, "120");
        neu::display(ui, &theme, "120");
        ui.horizontal(|ui| {
            for icon in [
                Icon::Play,
                Icon::Stop,
                Icon::Record,
                Icon::Sun,
                Icon::Moon,
                Icon::Minus,
                Icon::Plus,
                Icon::Clear,
            ] {
                neu::icon_button(ui, &theme, icon, 48.0);
                neu::stepper_button(ui, &theme, icon);
            }
        });
        ui.horizontal(|ui| {
            neu::button(ui, &theme, "Clear all");
            neu::toggle_button(ui, &theme, "Limit", true);
            neu::toggle_button(ui, &theme, "Armed", false);
            neu::theme_toggle(ui, &theme, &mut mode);
        });
        let (rect, _) = ui.allocate_exact_size(vec2(300.0, 60.0), egui::Sense::hover());
        for (i, depth) in [
            neu::Depth::RaisedL,
            neu::Depth::RaisedM,
            neu::Depth::RaisedS,
            neu::Depth::InsetM,
            neu::Depth::InsetS,
            neu::Depth::OnGlow,
        ]
        .into_iter()
        .enumerate()
        {
            let x = rect.min.x + 25.0 + i as f32 * 50.0;
            neu::paint_soft_rect(
                ui.painter(),
                egui::Rect::from_center_size(egui::pos2(x, rect.min.y + 15.0), vec2(40.0, 24.0)),
                theme.radius_pill,
                depth,
                &theme,
            );
            neu::paint_soft_circle(
                ui.painter(),
                egui::pos2(x, rect.min.y + 45.0),
                12.0,
                depth,
                &theme,
            );
        }
    });
}

#[test]
fn theme_toggle_flips_mode_and_palette() {
    let mut harness = Harness::new_ui_state(
        |ui, mode: &mut ThemeMode| {
            let theme = Theme::for_mode(*mode);
            theme.apply(ui.ctx());
            neu::theme_toggle(ui, &theme, mode);
        },
        ThemeMode::Light,
    );
    harness.run();
    assert_eq!(*harness.state(), ThemeMode::Light);
    assert_eq!(harness.ctx.theme(), egui::Theme::Light);

    harness.get_by_label("Dark theme").click();
    harness.run();
    assert_eq!(*harness.state(), ThemeMode::Dark);
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);

    let light = Theme::for_mode(ThemeMode::Light);
    let dark = Theme::for_mode(ThemeMode::Dark);
    assert_ne!(light.surface, dark.surface);
    assert_ne!(light.accent, dark.accent);
    assert_eq!(dark.surface, egui::Color32::from_rgb(0x26, 0x2A, 0x31));
    assert_eq!(light.surface, egui::Color32::from_rgb(0xE8, 0xEC, 0xF3));

    harness.get_by_label("Dark theme").click();
    harness.run();
    assert_eq!(*harness.state(), ThemeMode::Light);
}

#[test]
fn button_click_is_reported() {
    let mut harness = Harness::new_ui_state(
        |ui, clicks: &mut u32| {
            let theme = Theme::dark();
            if neu::button(ui, &theme, "Clear all").clicked() {
                *clicks += 1;
            }
        },
        0u32,
    );
    harness.run();
    assert_eq!(*harness.state(), 0);
    harness.get_by_label("Clear all").click();
    harness.run();
    assert_eq!(*harness.state(), 1);
    harness.get_by_label("Clear all").click();
    harness.run();
    assert_eq!(*harness.state(), 2);
}

#[test]
fn toggle_and_icon_buttons_report_clicks() {
    #[derive(Default)]
    struct S {
        limiter: bool,
        plays: u32,
    }
    let mut harness = Harness::new_ui_state(
        |ui, s: &mut S| {
            let theme = Theme::light();
            if neu::toggle_button(ui, &theme, "Limit", s.limiter).clicked() {
                s.limiter = !s.limiter;
            }
            if neu::icon_button(ui, &theme, Icon::Play, 48.0).clicked() {
                s.plays += 1;
            }
        },
        S::default(),
    );
    harness.run();
    harness.get_by_label("Limit").click();
    harness.get_by_label("Play").click();
    harness.run();
    assert!(harness.state().limiter);
    assert_eq!(harness.state().plays, 1);
}

#[test]
fn renders_in_both_themes() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let mut harness = Harness::builder()
            .with_size(vec2(1000.0, 600.0))
            .build_ui(move |ui| {
                Theme::for_mode(mode).apply(ui.ctx());
                all_widgets(ui, mode);
            });
        harness.run();
        // Second pass with the pointer over (and pressed on) a button to exercise hover/pressed
        // painting paths.
        let node = harness.get_by_label("Clear all");
        node.hover();
        harness.run();
        harness.get_by_label("Clear all").click();
        harness.run();
    }
}
