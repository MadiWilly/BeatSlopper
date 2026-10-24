//! Visual gallery of the `neu` M0 and M1 widgets in both themes.
//!
//! `cargo run -p neu --example gallery [-- --exit-after-secs N] [--light|--dark]`
//!
//! Shows a panel with every widget in its ON and OFF states, the raw depth swatches, the
//! theme toggle and a KEYS panel (two octaves of piano keys with a few held notes and the
//! octave/swing steppers), so the look can be checked against `design/preview-a.html`.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Rect, Stroke, Vec2, pos2, vec2};
use neu::{Depth, Icon, TextKind, Theme, ThemeMode};

struct Gallery {
    mode: ThemeMode,
    applied: Option<ThemeMode>,
    playing: bool,
    armed: bool,
    limiter: bool,
    clicks: u32,
    octave: i32,
    swing: i32,
    /// Notes held by the computer-keyboard mapping (shown lit) plus the mouse.
    held: [bool; 128],
    /// Computer-key caps for the two octaves of the mock (A W S E D F T G Y H U J K O L P ;).
    labels: [Option<char>; 128],
    last_piano: neu::PianoOutput,
    started: Instant,
    exit_after: Option<Duration>,
}

/// The mock's key map: semitone offset from the base C -> computer key cap.
const KEYMAP: &str = "AWSEDFTGYHUJKOLP;";

impl Gallery {
    fn new(
        cc: &eframe::CreationContext<'_>,
        mode: Option<ThemeMode>,
        exit_after: Option<Duration>,
    ) -> Self {
        let mode = mode
            .or_else(|| cc.egui_ctx.system_theme().map(ThemeMode::from_egui))
            .unwrap_or(ThemeMode::Dark);
        let mut held = [false; 128];
        // A chord like the mock's screenshot: E4 (white) and G#4 (black), plus a high C.
        held[64] = true;
        held[68] = true;
        held[72] = true;
        Self {
            mode,
            applied: None,
            playing: true,
            armed: false,
            limiter: true,
            clicks: 0,
            octave: 4,
            swing: 54,
            held,
            labels: Self::labels_for_octave(4),
            last_piano: neu::PianoOutput::default(),
            started: Instant::now(),
            exit_after,
        }
    }

    fn base_note(octave: i32) -> u8 {
        ((octave + 1) * 12).clamp(0, 127) as u8
    }

    fn labels_for_octave(octave: i32) -> [Option<char>; 128] {
        let mut labels = [None; 128];
        let base = usize::from(Self::base_note(octave));
        for (i, c) in KEYMAP.chars().enumerate() {
            if let Some(slot) = labels.get_mut(base + i) {
                *slot = Some(c);
            }
        }
        labels
    }
}

impl eframe::App for Gallery {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        if self.applied != Some(self.mode) {
            Theme::for_mode(self.mode).apply(ctx);
            self.applied = Some(self.mode);
        }
        let theme = Theme::for_mode(self.mode);

        if let Some(limit) = self.exit_after {
            if self.started.elapsed() > limit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme.surface).inner_margin(28.0))
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    // Panel shadows reach 18 pt past the panel; keep the right margin for them.
                    ui.set_max_width(ui.available_width() - 20.0);

                    neu::panel(ui, &theme, |ui| {
                        ui.horizontal(|ui| {
                            neu::title(ui, &theme, "Transport");
                            ui.add_space(24.0);
                            neu::label(ui, &theme, "Soft Console gallery");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    neu::theme_toggle(ui, &theme, &mut self.mode);
                                },
                            );
                        });
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            if neu::icon_button(ui, &theme, Icon::Play, 48.0).clicked() {
                                self.playing = !self.playing;
                            }
                            neu::icon_button(ui, &theme, Icon::Stop, 48.0);
                            if neu::icon_button(ui, &theme, Icon::Record, 48.0).clicked() {
                                self.armed = !self.armed;
                            }
                            ui.add_space(20.0);
                            neu::stepper_button(ui, &theme, Icon::Minus);
                            neu::display(ui, &theme, "120");
                            neu::stepper_button(ui, &theme, Icon::Plus);
                            ui.add_space(20.0);
                            for icon in [Icon::Sun, Icon::Moon, Icon::Clear] {
                                neu::icon_button(ui, &theme, icon, 28.0);
                            }
                        });
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if neu::button(ui, &theme, "Clear all").clicked() {
                                self.clicks += 1;
                            }
                            neu::button(ui, &theme, "Save");
                            if neu::toggle_button(ui, &theme, "Play", self.playing).clicked() {
                                self.playing = !self.playing;
                            }
                            if neu::toggle_button(ui, &theme, "Limit", self.limiter).clicked() {
                                self.limiter = !self.limiter;
                            }
                            neu::toggle_button(ui, &theme, "Armed", self.armed);
                            neu::value(ui, &theme, &format!("clicks {}", self.clicks));
                        });
                    });

                    neu::panel(ui, &theme, |ui| {
                        neu::title(ui, &theme, "Depth swatches");
                        ui.add_space(14.0);
                        let depths = [
                            (Depth::RaisedL, "raised-l"),
                            (Depth::RaisedM, "raised-m"),
                            (Depth::RaisedS, "raised-s"),
                            (Depth::InsetM, "inset-m"),
                            (Depth::InsetS, "inset-s"),
                            (Depth::OnGlow, "on-glow"),
                        ];
                        let cell = vec2(96.0, 120.0);
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(
                                cell.x * depths.len() as f32 + 16.0 * (depths.len() - 1) as f32,
                                cell.y,
                            ),
                            egui::Sense::hover(),
                        );
                        let painter = ui.painter();
                        for (i, (depth, name)) in depths.iter().enumerate() {
                            let x = rect.min.x + i as f32 * (cell.x + 16.0);
                            let sq = Rect::from_min_size(
                                pos2(x + 8.0, rect.min.y + 8.0),
                                vec2(80.0, 40.0),
                            );
                            neu::paint_soft_rect(painter, sq, 12.0, *depth, &theme);
                            neu::paint_soft_circle(
                                painter,
                                pos2(x + 48.0, rect.min.y + 80.0),
                                16.0,
                                *depth,
                                &theme,
                            );
                            neu::text::paint_text(
                                painter,
                                pos2(x + 48.0, rect.max.y),
                                Align2::CENTER_BOTTOM,
                                name,
                                TextKind::Label,
                                theme.text_secondary,
                            );
                        }
                        ui.add_space(8.0);
                        // A slider-like thin inset track and a pressed pill, like the mock.
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(400.0, 40.0), egui::Sense::hover());
                        let painter = ui.painter();
                        let track = Rect::from_center_size(
                            pos2(rect.min.x + 60.0, rect.center().y),
                            vec2(100.0, 6.0),
                        );
                        neu::paint_soft_rect(painter, track, 3.0, Depth::InsetS, &theme);
                        neu::paint_soft_circle(
                            painter,
                            pos2(track.min.x + 70.0, track.center().y),
                            7.0,
                            Depth::OnGlow,
                            &theme,
                        );
                        let seg = Rect::from_center_size(
                            pos2(rect.min.x + 250.0, rect.center().y),
                            vec2(180.0, 30.0),
                        );
                        neu::paint_soft_rect(
                            painter,
                            seg,
                            theme.radius_pill,
                            Depth::InsetM,
                            &theme,
                        );
                        for (i, s) in ["Sine", "Saw", "Square", "Tri"].iter().enumerate() {
                            let x = seg.min.x + 24.0 + i as f32 * 44.0;
                            let col = if i == 1 {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            };
                            neu::text::paint_text(
                                painter,
                                pos2(x, seg.center().y),
                                Align2::CENTER_CENTER,
                                s,
                                TextKind::Value,
                                col,
                            );
                        }
                        let _ = Stroke::NONE;
                        let _ = Color32::TRANSPARENT;
                    });

                    // KEYS panel, like the mock: title + octave stepper in the head, two octaves
                    // of keys below. Padding is the mock's 16 20 18 (approximated by the panel's
                    // 20 and a slightly tighter head gap).
                    neu::panel(ui, &theme, |ui| {
                        ui.horizontal(|ui| {
                            neu::title(ui, &theme, "Keys");
                            ui.add_space(24.0);
                            let before = self.octave;
                            if neu::stepper(ui, &theme, "Octave", &mut self.octave, 1..=7).changed()
                                && self.octave != before
                            {
                                self.labels = Self::labels_for_octave(self.octave);
                            }
                            ui.add_space(24.0);
                            neu::stepper_with(ui, &theme, "Swing", &mut self.swing, 50..=75, |v| {
                                format!("{v}%")
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    neu::value(
                                        ui,
                                        &theme,
                                        &format!(
                                            "pressed {:?}  released {:?}",
                                            self.last_piano.pressed, self.last_piano.released
                                        ),
                                    );
                                },
                            );
                        });
                        ui.add_space(10.0);
                        let model = neu::PianoModel {
                            base_note: Self::base_note(self.octave),
                            white_keys: 14,
                            held: &self.held,
                            labels: &self.labels,
                        };
                        let out = neu::piano_keys(ui, &theme, &model);
                        if out.pressed.is_some() || out.released.is_some() {
                            self.last_piano = out;
                        }
                        if let Some(n) = out.released {
                            self.held[usize::from(n)] = false;
                        }
                        if let Some(n) = out.pressed {
                            self.held[usize::from(n)] = true;
                        }
                    });
                    ui.add_space(20.0);
                });
            });
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut exit_after = None;
    let mut mode = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--exit-after-secs" => {
                exit_after = args
                    .get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .map(Duration::from_secs_f32);
                i += 1;
            }
            "--light" => mode = Some(ThemeMode::Light),
            "--dark" => mode = Some(ThemeMode::Dark),
            _ => {}
        }
        i += 1;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("neu gallery")
            .with_inner_size(Vec2::new(1280.0, 860.0))
            .with_min_inner_size(Vec2::new(1280.0, 860.0)),
        // Always open at the designed size so the 1170 pt keyboard is visible.
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "neu gallery",
        options,
        Box::new(move |cc| Ok(Box::new(Gallery::new(cc, mode, exit_after)))),
    )
}
