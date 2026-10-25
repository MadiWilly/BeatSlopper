//! egui_kittest tests for the visualizers: every drawing renders in both themes with silence,
//! a sine and noise (with drum hits) without panicking, in a strip-sized and a panel-sized
//! rect; each preset renders; 60 frames of each visualizer take well under a second in debug.

use std::time::Instant;

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::visualizer::{SPECTRUM_BINS, live_particles};
use neu::{PRESETS, Theme, ThemeMode, VisualInput, VisualStyle, Visualizer};

/// Which audio the harness feeds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Signal {
    Silence,
    Sine,
    Noise,
}

struct Scene {
    mode: ThemeMode,
    kind: Visualizer,
    style: VisualStyle,
    left: Vec<f32>,
    right: Vec<f32>,
    spectrum: Vec<f32>,
    hits: u8,
    peak: f32,
    frame: u32,
}

fn audio(signal: Signal) -> (Vec<f32>, Vec<f32>, Vec<f32>, f32) {
    let n = 2048;
    match signal {
        Signal::Silence => (vec![0.0; n], vec![0.0; n], vec![0.0; SPECTRUM_BINS], 0.0),
        Signal::Sine => {
            let left: Vec<f32> = (0..n)
                .map(|i| 0.7 * (i as f32 / 48_000.0 * 1000.0 * std::f32::consts::TAU).sin())
                .collect();
            let right: Vec<f32> = left.iter().map(|s| s * 0.5).collect();
            let mut spectrum = vec![0.0; SPECTRUM_BINS];
            spectrum[30] = 0.9;
            spectrum[29] = 0.5;
            spectrum[31] = 0.5;
            (left, right, spectrum, 0.7)
        }
        Signal::Noise => {
            let mut x = 0x1234_5678_u32;
            let mut rand = move || {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
            };
            let left: Vec<f32> = (0..n).map(|_| rand() * 0.9).collect();
            let right: Vec<f32> = (0..n).map(|_| rand() * 0.9).collect();
            let spectrum: Vec<f32> = (0..SPECTRUM_BINS)
                .map(|i| 1.0 - i as f32 / SPECTRUM_BINS as f32)
                .collect();
            (left, right, spectrum, 0.95)
        }
    }
}

fn scene(mode: ThemeMode, kind: Visualizer, signal: Signal) -> Scene {
    let (left, right, spectrum, peak) = audio(signal);
    Scene {
        mode,
        kind,
        style: VisualStyle::DEFAULT,
        left,
        right,
        spectrum,
        hits: if signal == Signal::Noise { 0b1_1111 } else { 0 },
        peak,
        frame: 0,
    }
}

fn harness(size: egui::Vec2, scene: Scene) -> Harness<'static, Scene> {
    Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, scene: &mut Scene| {
                let theme = Theme::for_mode(scene.mode);
                theme.apply(ui.ctx());
                scene.frame += 1;
                let input = VisualInput {
                    samples: &scene.left,
                    samples_r: Some(&scene.right),
                    spectrum: &scene.spectrum,
                    peak: (scene.peak, scene.peak * 0.8),
                    // Hits on the first two frames only, then the animations run on.
                    hits: if scene.frame <= 2 { scene.hits } else { 0 },
                    playhead: 0.25,
                    bpm: 120.0,
                    time: ui.input(|i| i.time),
                };
                scene.kind.show(ui, &theme, &input, &scene.style);
            },
            scene,
        )
}

#[test]
fn every_visualizer_renders_in_both_themes_for_silence_sine_and_noise() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for kind in Visualizer::ALL {
            for signal in [Signal::Silence, Signal::Sine, Signal::Noise] {
                for size in [vec2(220.0, 40.0), vec2(480.0, 300.0)] {
                    let mut h = harness(size, scene(mode, kind, signal));
                    h.run_steps(4);
                    // The harness root has an 8 pt margin; the widget fills what is left.
                    let rect = h.get_by_label(kind.widget_label()).rect();
                    assert!(
                        (rect.width() - (size.x - 16.0)).abs() < 1.0
                            && (rect.height() - (size.y - 16.0)).abs() < 1.0,
                        "{} fills the {size:?} rect, got {rect:?}",
                        kind.label()
                    );
                    assert!(h.state().frame >= 4, "{} frames", h.state().frame);
                }
            }
        }
    }
}

#[test]
fn every_preset_renders_and_off_draws_nothing() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for preset in PRESETS {
            let (left, right, spectrum, peak) = audio(Signal::Sine);
            let mut h = Harness::builder()
                .with_size(vec2(400.0, 240.0))
                .build_ui_state(
                    move |ui, drawn: &mut bool| {
                        let theme = Theme::for_mode(mode);
                        theme.apply(ui.ctx());
                        let input = VisualInput {
                            samples: &left,
                            samples_r: Some(&right),
                            spectrum: &spectrum,
                            peak: (peak, peak),
                            hits: 1,
                            playhead: 0.0,
                            bpm: 120.0,
                            time: ui.input(|i| i.time),
                        };
                        *drawn = preset.show(ui, &theme, &input).is_some();
                    },
                    false,
                );
            h.run_steps(3);
            assert_eq!(*h.state(), !preset.is_off(), "{}", preset.name);
            if let Some(kind) = preset.visualizer {
                h.get_by_label(kind.widget_label());
            } else {
                assert!(h.query_by_label("Scope visualizer").is_none());
            }
        }
    }
}

#[test]
fn particles_spawn_on_hits_and_fade_out() {
    let mut h = Harness::builder()
        .with_size(vec2(300.0, 200.0))
        .with_step_dt(0.05)
        .build_ui_state(
            |ui, (frame, alive): &mut (u32, usize)| {
                let theme = Theme::dark();
                *frame += 1;
                let input = VisualInput {
                    hits: if *frame == 2 { 0b1_1111 } else { 0 },
                    time: ui.input(|i| i.time),
                    ..VisualInput::SILENT
                };
                let response = neu::particles(ui, &theme, &input, &VisualStyle::DEFAULT);
                *alive = live_particles(ui, &response);
            },
            (0, 0),
        );
    h.run_steps(2);
    assert!(h.state().1 > 50, "a full hit set spawns a burst");
    h.run_steps(80);
    assert_eq!(h.state().1, 0, "everything fades within 4 s");
}

#[test]
fn sixty_frames_of_each_visualizer_stay_under_a_second() {
    for kind in Visualizer::ALL {
        let mut h = harness(
            vec2(600.0, 360.0),
            scene(ThemeMode::Dark, kind, Signal::Noise),
        );
        h.run_steps(2);
        let start = Instant::now();
        h.run_steps(60);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs_f32() < 1.0,
            "{} took {elapsed:?} for 60 frames",
            kind.label()
        );
    }
}

#[test]
fn degenerate_inputs_do_not_panic() {
    let weird: Vec<f32> = vec![f32::NAN, f32::INFINITY, -f32::INFINITY, 5.0, -5.0, 0.0, 1.0];
    for kind in Visualizer::ALL {
        let left = weird.clone();
        let mut h = Harness::builder()
            .with_size(vec2(64.0, 24.0))
            .build_ui_state(
                move |ui, _: &mut ()| {
                    let theme = Theme::light();
                    let input = VisualInput {
                        samples: &left,
                        samples_r: Some(&left[..3]),
                        spectrum: &[f32::NAN, 2.0, -1.0],
                        peak: (f32::NAN, f32::INFINITY),
                        hits: 0xFF,
                        playhead: f32::NAN,
                        bpm: 0.0,
                        time: f64::NAN,
                    };
                    let style = VisualStyle {
                        thickness: f32::NAN,
                        fade: -3.0,
                        mirror: true,
                        grid: true,
                        ..VisualStyle::DEFAULT
                    };
                    kind.show(ui, &theme, &input, &style);
                },
                (),
            );
        h.run_steps(3);
    }
}
