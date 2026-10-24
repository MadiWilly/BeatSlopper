//! egui_kittest: the M0 screens render in both themes without a window or an audio device.

use app::ui;
use app::{AppState, BeatSlopperApp};
use egui_kittest::Harness;
use neu::{Theme, ThemeMode};

fn render_panels(mode: ThemeMode) -> AppState {
    let (state, _engine) = AppState::headless(mode);
    let theme = Theme::for_mode(mode);
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 720.0))
        .build_ui_state(
            move |ui, state: &mut AppState| {
                ui::transport_bar::show(ui, state, &theme);
                ui.add_space(10.0);
                ui::main_area(ui, state, &theme);
            },
            state,
        );
    harness.run_steps(5);
    harness.into_state()
}

#[test]
fn transport_and_main_area_render_in_dark_theme() {
    let state = render_panels(ThemeMode::Dark);
    assert_eq!(state.mode, ThemeMode::Dark);
    assert!(!state.test_tone);
}

#[test]
fn transport_and_main_area_render_in_light_theme() {
    let state = render_panels(ThemeMode::Light);
    assert_eq!(state.mode, ThemeMode::Light);
}

#[test]
fn full_layout_renders_in_both_themes_and_counts_frames() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (app, _engine) = BeatSlopperApp::headless(mode, true);
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, 1080.0))
            .build_ui_state(
                |ui, app: &mut BeatSlopperApp| {
                    app.tick(ui.ctx());
                    app.show(ui);
                },
                app,
            );
        let steps = 3;
        harness.run_steps(steps);
        let app = harness.into_state();
        assert_eq!(app.state().mode, mode);
        assert_eq!(app.theme().mode, mode);
        // The builder settles the first frame (several steps, since smoke mode requests a
        // repaint every frame), then `run_steps` adds exactly `steps` more.
        assert!(app.frames() > steps as u32, "frames = {}", app.frames());
    }
}
