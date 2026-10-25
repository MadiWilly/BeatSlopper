//! Visual gallery of the `neu` M0, M1 and M2 widgets in both themes.
//!
//! `cargo run -p neu --example gallery [-- --exit-after-secs N] [--light|--dark] [--scroll PX]`
//!
//! Shows a panel with every widget in its ON and OFF states, the raw depth swatches, the
//! theme toggle, the mock's SYNTH and FX panels built from the M2 controls (knobs, dials,
//! segmented selectors, vertical sliders, the limiter lamp) and a KEYS panel (two octaves of
//! piano keys with a few held notes and the octave/swing steppers), so the look can be checked
//! against `design/preview-a.html`.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, Layout, Rect, Vec2, pos2, vec2};
use neu::{Depth, DialSize, Icon, KnobSize, KnobSpec, SliderSpec, TextKind, Theme, ThemeMode};

/// The mock's synth and FX parameter values (`data-val` attributes in `design/preview-a.html`).
struct Params {
    preset: usize,
    osc1_wave: usize,
    osc1_detune: f32,
    osc1_mix: f32,
    osc2_wave: usize,
    osc2_detune: f32,
    osc2_mix: f32,
    cutoff: f32,
    res: f32,
    env: f32,
    adsr: [f32; 4],
    lfo_rate: f32,
    lfo_depth: f32,
    lfo_target: usize,
    drive_amount: f32,
    drive_mix: f32,
    delay_div: usize,
    delay_fb: f32,
    delay_mix: f32,
    rev_size: f32,
    rev_damp: f32,
    rev_mix: f32,
    gain_db: f32,
    master: f32,
    /// The sequencer row's small volume knob.
    row_vol: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            preset: 0,
            osc1_wave: 0,
            osc1_detune: 0.0,
            osc1_mix: 70.0,
            osc2_wave: 1,
            osc2_detune: 12.0,
            osc2_mix: 45.0,
            cutoff: 2400.0,
            res: 22.0,
            env: 40.0,
            adsr: [12.0, 180.0, 70.0, 320.0],
            lfo_rate: 4.5,
            lfo_depth: 25.0,
            lfo_target: 1,
            drive_amount: 35.0,
            drive_mix: 50.0,
            delay_div: 1,
            delay_fb: 35.0,
            delay_mix: 25.0,
            rev_size: 60.0,
            rev_damp: 40.0,
            rev_mix: 20.0,
            gain_db: 0.0,
            master: 80.0,
            row_vol: 90.0,
        }
    }
}

fn fmt_pct(v: f32) -> String {
    format!("{v:.0} %")
}

fn fmt_cents(v: f32) -> String {
    format!("{v:.0} ct")
}

fn fmt_hz(v: f32) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

fn fmt_hz_1dp(v: f32) -> String {
    format!("{v:.1} Hz")
}

fn fmt_ms(v: f32) -> String {
    format!("{v:.0} ms")
}

fn fmt_db(v: f32) -> String {
    if v > 0.0 {
        format!("+{v:.0} dB")
    } else {
        format!("{v:.0} dB")
    }
}

/// A unipolar 0..=100 % knob (`Md`) like the mock's `data-unit="%"` knobs.
fn pct_knob(label: &str, default: f32) -> KnobSpec<'_> {
    KnobSpec {
        label,
        min: 0.0,
        max: 100.0,
        default,
        log: false,
        bipolar: false,
        size: KnobSize::Md,
        fmt: fmt_pct,
    }
}

/// The mock's `.col > .lbl` / `.fx-sec > .lbl`: a caps label centred in a 30 pt row.
fn col_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width().max(1.0), 30.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            neu::label(ui, theme, text);
        },
    );
}

/// The mock's `.ghead`: a caps label and a segmented selector, 14 pt apart, in a 30 pt row.
fn group_head(ui: &mut egui::Ui, theme: &Theme, text: &str, selected: &mut usize, items: &[&str]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        neu::label(ui, theme, text);
        neu::segmented(ui, theme, selected, items);
    });
}

const WAVES: [&str; 4] = ["Sine", "Saw", "Square", "Tri"];

/// Gap between the FX panel's two columns (`.fx-body` `gap: 16px 18px`).
const FX_COLUMN_GAP: f32 = 18.0;

struct Gallery {
    mode: ThemeMode,
    applied: Option<ThemeMode>,
    playing: bool,
    armed: bool,
    limiter: bool,
    clicks: u32,
    octave: i32,
    swing: i32,
    params: Params,
    /// Notes held by the computer-keyboard mapping (shown lit) plus the mouse.
    held: [bool; 128],
    /// Computer-key caps for the two octaves of the mock (A W S E D F T G Y H U J K O L P ;).
    labels: [Option<char>; 128],
    last_piano: neu::PianoOutput,
    started: Instant,
    exit_after: Option<Duration>,
    /// Initial vertical scroll offset (`--scroll`), applied for the first few frames (the
    /// first frame is a sizing pass whose content height would clamp it).
    scroll_to: Option<f32>,
}

/// The mock's key map: semitone offset from the base C -> computer key cap.
const KEYMAP: &str = "AWSEDFTGYHUJKOLP;";

impl Gallery {
    fn new(
        cc: &eframe::CreationContext<'_>,
        mode: Option<ThemeMode>,
        exit_after: Option<Duration>,
        scroll_to: Option<f32>,
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
            params: Params::default(),
            held,
            labels: Self::labels_for_octave(4),
            last_piano: neu::PianoOutput::default(),
            started: Instant::now(),
            exit_after,
            scroll_to,
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
                let mut scroll = egui::ScrollArea::vertical();
                if let Some(offset) = self.scroll_to {
                    scroll = scroll.vertical_scroll_offset(offset);
                    if self.started.elapsed() > Duration::from_millis(300) {
                        self.scroll_to = None;
                    }
                    ctx.request_repaint();
                }
                scroll.show(ui, |ui| {
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
                                    ui.spacing_mut().item_spacing.x = 24.0;
                                    neu::theme_toggle(ui, &theme, &mut self.mode);
                                    // The mock's master dial (`.dial.lg`, 0..=100 %, 80).
                                    neu::dial(
                                        ui,
                                        &theme,
                                        &mut self.params.master,
                                        &pct_knob("Master", 80.0),
                                        DialSize::Lg,
                                    );
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
                    });

                    // SYNTH and FX panels. The mock puts them side by side (`.row2`, FX 460
                    // wide); here they are stacked and sized to their content so every control
                    // stays visible whatever the window width (egui's face is wider than the
                    // mock's system font, so the synth row does not fit beside a 460 pt FX
                    // panel at 1280).
                    self.synth_panel(ui, &theme);
                    self.fx_panel(ui, &theme);

                    // KEYS panel, like the mock: title + octave stepper in the head, two octaves
                    // of keys below. Padding is the mock's 16 20 18 (approximated by the panel's
                    // 20 and a slightly tighter head gap).
                    neu::panel(ui, &theme, |ui| {
                        ui.horizontal(|ui| {
                            neu::title(ui, &theme, "Keys");
                            ui.add_space(24.0);
                            // The sequencer row's small knobs: unlabelled (value as a tooltip)
                            // and labelled.
                            neu::knob(
                                ui,
                                &theme,
                                &mut self.params.row_vol,
                                &KnobSpec {
                                    label: "",
                                    size: KnobSize::Sm,
                                    ..pct_knob("Vol", 90.0)
                                },
                            );
                            neu::knob(
                                ui,
                                &theme,
                                &mut self.params.row_vol,
                                &KnobSpec {
                                    size: KnobSize::Sm,
                                    ..pct_knob("Vol", 90.0)
                                },
                            );
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

impl Gallery {
    /// The mock's SYNTH panel: preset selector in the head; Osc 1 / Osc 2 groups, Filter
    /// (cutoff dial, res + env knobs), Amp (A D S R sliders) and LFO (rate + depth knobs,
    /// target selector) columns, 26 pt apart.
    fn synth_panel(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let p = &mut self.params;
        neu::panel(ui, theme, |ui| {
            ui.horizontal(|ui| {
                neu::title(ui, theme, "Synth");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    neu::segmented(ui, theme, &mut p.preset, &["Bass", "Lead", "Pad", "Pluck"]);
                });
            });
            ui.add_space(14.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(26.0, 10.0);

                // Osc 1 / Osc 2 (`.col.osc`): two groups 20 pt apart.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                    group_head(ui, theme, "Osc 1", &mut p.osc1_wave, &WAVES);
                    ui.horizontal(|ui| {
                        neu::knob(
                            ui,
                            theme,
                            &mut p.osc1_detune,
                            &KnobSpec {
                                label: "Detune",
                                min: -50.0,
                                max: 50.0,
                                default: 0.0,
                                log: false,
                                bipolar: true,
                                size: KnobSize::Md,
                                fmt: fmt_cents,
                            },
                        );
                        neu::knob(ui, theme, &mut p.osc1_mix, &pct_knob("Mix", 70.0));
                    });
                    ui.add_space(10.0);
                    group_head(ui, theme, "Osc 2", &mut p.osc2_wave, &WAVES);
                    ui.horizontal(|ui| {
                        neu::knob(
                            ui,
                            theme,
                            &mut p.osc2_detune,
                            &KnobSpec {
                                label: "Detune",
                                min: -50.0,
                                max: 50.0,
                                default: 0.0,
                                log: false,
                                bipolar: true,
                                size: KnobSize::Md,
                                fmt: fmt_cents,
                            },
                        );
                        neu::knob(ui, theme, &mut p.osc2_mix, &pct_knob("Mix", 45.0));
                    });
                });

                // Filter: the small cutoff dial (log 20 Hz..=20 kHz) over res + env knobs.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                    col_label(ui, theme, "Filter");
                    neu::dial(
                        ui,
                        theme,
                        &mut p.cutoff,
                        &KnobSpec {
                            label: "Cutoff",
                            min: 20.0,
                            max: 20000.0,
                            default: 2400.0,
                            log: true,
                            bipolar: false,
                            size: KnobSize::Md,
                            fmt: fmt_hz,
                        },
                        DialSize::Sm,
                    );
                    ui.horizontal(|ui| {
                        neu::knob(ui, theme, &mut p.res, &pct_knob("Res", 22.0));
                        neu::knob(
                            ui,
                            theme,
                            &mut p.env,
                            &KnobSpec {
                                label: "Env",
                                min: -100.0,
                                max: 100.0,
                                default: 40.0,
                                log: false,
                                bipolar: true,
                                size: KnobSize::Md,
                                fmt: fmt_pct,
                            },
                        );
                    });
                });

                // Amp: A D S R sliders 8 pt apart.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 10.0);
                    col_label(ui, theme, "Amp");
                    ui.horizontal(|ui| {
                        let specs = [
                            ("A", 2000.0, 12.0, fmt_ms as fn(f32) -> String),
                            ("D", 2000.0, 180.0, fmt_ms),
                            ("S", 100.0, 70.0, fmt_pct),
                            ("R", 4000.0, 320.0, fmt_ms),
                        ];
                        for (i, (label, max, default, fmt)) in specs.into_iter().enumerate() {
                            neu::vslider(
                                ui,
                                theme,
                                &mut p.adsr[i],
                                &SliderSpec {
                                    label,
                                    min: 0.0,
                                    max,
                                    default,
                                    fmt,
                                },
                            );
                        }
                    });
                });

                // LFO: log rate + depth knobs, then the target selector.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                    col_label(ui, theme, "LFO");
                    ui.horizontal(|ui| {
                        neu::knob(
                            ui,
                            theme,
                            &mut p.lfo_rate,
                            &KnobSpec {
                                label: "Rate",
                                min: 0.1,
                                max: 20.0,
                                default: 4.5,
                                log: true,
                                bipolar: false,
                                size: KnobSize::Md,
                                fmt: fmt_hz_1dp,
                            },
                        );
                        neu::knob(ui, theme, &mut p.lfo_depth, &pct_knob("Depth", 25.0));
                    });
                    neu::segmented(ui, theme, &mut p.lfo_target, &["Pitch", "Cutoff"]);
                });
            });
        });
    }

    /// The mock's FX panel: Drive and Delay on the left, Reverb and Master (gain knob, limiter
    /// toggle and the "Active" lamp) on the right, sections 16 pt apart.
    fn fx_panel(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let p = &mut self.params;
        let limiter = &mut self.limiter;
        neu::panel(ui, theme, |ui| {
            ui.horizontal(|ui| {
                neu::title(ui, theme, "FX");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    neu::label(ui, theme, "Post chain");
                });
            });
            ui.add_space(14.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(FX_COLUMN_GAP, 16.0);

                // Two content-sized columns (`.fx-body` `grid-template-columns: auto auto`).
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 16.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                        col_label(ui, theme, "Drive");
                        ui.horizontal(|ui| {
                            neu::knob(ui, theme, &mut p.drive_amount, &pct_knob("Amount", 35.0));
                            neu::knob(ui, theme, &mut p.drive_mix, &pct_knob("Mix", 50.0));
                        });
                    });
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                        group_head(
                            ui,
                            theme,
                            "Delay",
                            &mut p.delay_div,
                            &["1/16", "1/8", "1/4", "1/2"],
                        );
                        ui.horizontal(|ui| {
                            neu::knob(ui, theme, &mut p.delay_fb, &pct_knob("Feedback", 35.0));
                            neu::knob(ui, theme, &mut p.delay_mix, &pct_knob("Mix", 25.0));
                        });
                    });
                });

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 16.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                        col_label(ui, theme, "Reverb");
                        ui.horizontal(|ui| {
                            neu::knob(ui, theme, &mut p.rev_size, &pct_knob("Size", 60.0));
                            neu::knob(ui, theme, &mut p.rev_damp, &pct_knob("Damping", 40.0));
                            neu::knob(ui, theme, &mut p.rev_mix, &pct_knob("Mix", 20.0));
                        });
                    });
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                        col_label(ui, theme, "Master");
                        ui.horizontal(|ui| {
                            neu::knob(
                                ui,
                                theme,
                                &mut p.gain_db,
                                &KnobSpec {
                                    label: "Gain",
                                    min: -24.0,
                                    max: 12.0,
                                    default: 0.0,
                                    log: false,
                                    bipolar: true,
                                    size: KnobSize::Md,
                                    fmt: fmt_db,
                                },
                            );
                            // `.master-side`: the limiter toggle and the lamp, 8 pt apart,
                            // pushed down 18 pt to sit beside the knob's cap.
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 8.0;
                                ui.add_space(18.0);
                                if neu::toggle_button(ui, theme, "Limit", *limiter).clicked() {
                                    *limiter = !*limiter;
                                }
                                // Lit while the limiter is on (the mock also requires the gain
                                // to be above unity; here the toggle alone shows both states).
                                neu::lamp(ui, theme, *limiter, "Active");
                            });
                        });
                    });
                });
            });
        });
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut exit_after = None;
    let mut mode = None;
    let mut scroll_to = None;
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
            "--scroll" => {
                scroll_to = args.get(i + 1).and_then(|s| s.parse().ok());
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
        Box::new(move |cc| Ok(Box::new(Gallery::new(cc, mode, exit_after, scroll_to)))),
    )
}
