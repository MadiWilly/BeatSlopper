//! Visual gallery of the `neu` M0 to M4, M6 mixer and M7 piano-roll widgets in both themes.
//!
//! `cargo run -p neu --example gallery [-- --exit-after-secs N] [--light|--dark] [--scroll PX]`
//!
//! Shows a panel with every widget in its ON and OFF states, the raw depth swatches, the
//! theme toggle, the mock's SYNTH and FX panels built from the M2 controls (knobs, dials,
//! segmented selectors, vertical sliders, the limiter lamp), the SEQUENCER panel (the mock's
//! demo pattern in step cells with a running playhead, beat numbers and per-row clear pills),
//! the PIANO ROLL panel (a chord, a long note and the full draw / move / resize / delete
//! interaction on a scrollable three-octave grid with the running playhead), a VISUALIZERS
//! panel (the six presets over a synthetic beat, animated while Play is on) and a KEYS panel
//! (two octaves of piano keys with a few held notes and the octave/swing steppers), so the
//! look can be checked against `design/preview-a.html`. The transport line carries the brand
//! wordmark (M4), and every knob, dial and slider shows a styled value tooltip after a short
//! hover.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, Layout, Rect, Vec2, pos2, vec2};
use neu::{
    Depth, DialSize, Icon, KnobSize, KnobSpec, PianoRollModel, RollNote, SliderSpec, StepCellSpec,
    TextKind, Theme, ThemeMode, VisualInput,
};

/// The mock's drum rows (`rows` in `design/preview-a.html`): name, volume and the demo
/// pattern (0 off, 1 on, 2 accent).
const DRUM_ROWS: [(&str, f32, [u8; 16]); 5] = [
    (
        "Kick",
        90.0,
        [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0],
    ),
    (
        "Snare",
        80.0,
        [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0],
    ),
    (
        "Clap",
        60.0,
        [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1],
    ),
    (
        "Closed hat",
        70.0,
        [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0],
    ),
    (
        "Open hat",
        55.0,
        [0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1],
    ),
];

/// The mock's bassline: semitones above C2, -1 for a rest.
const BASS_ROW: [i8; 16] = [0, -1, -1, 0, -1, -1, 3, -1, 7, -1, -1, 3, -1, -1, 10, -1];

/// Width of the row-name column and of the row-control column (`.seq-row .name` / `.rowctl`).
const SEQ_NAME_WIDTH: f32 = 80.0;
const SEQ_CTL_WIDTH: f32 = 34.0;
/// Gap between the sequencer row's columns (`.seq-row` `gap: 16px`).
const SEQ_COL_GAP: f32 = 16.0;
/// Height of a sequencer row (`.seq-row`).
const SEQ_ROW_HEIGHT: f32 = 34.0;

/// Note name for a bass step (`noteName` in the mock: semitones above C2).
fn bass_note_name(semis: i8) -> String {
    const NOTES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let s = usize::try_from(semis).unwrap_or(0);
    format!("{}{}", NOTES[s % 12], 2 + s / 12)
}

/// The mock's synth and FX parameter values (`data-val` attributes in `design/preview-a.html`).
struct Params {
    preset: usize,
    osc1_wave: usize,
    osc1_detune: f32,
    osc1_mix: f32,
    osc1_pulse: f32,
    osc2_wave: usize,
    osc2_detune: f32,
    osc2_mix: f32,
    osc2_pulse: f32,
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
            osc1_pulse: 50.0,
            osc2_wave: 2,
            osc2_detune: 12.0,
            osc2_mix: 45.0,
            osc2_pulse: 35.0,
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

/// Pan display: `C` at the centre, otherwise `L n` / `R n`.
fn fmt_pan(v: f32) -> String {
    if v.abs() < 0.5 {
        "C".to_owned()
    } else if v < 0.0 {
        format!("L {:.0}", -v)
    } else {
        format!("R {v:.0}")
    }
}

/// Channel names of the gallery's MIXER strips (the first, middle and last M6 channels).
const MIXER_STRIPS: [&str; 3] = ["Kick", "Snare", "Synth"];

/// The mixer fader: level in dB, -60..=+6, unity by default.
const LEVEL_FADER: SliderSpec<'static> = SliderSpec {
    label: "Level",
    min: -60.0,
    max: 6.0,
    default: 0.0,
    fmt: fmt_db,
};

/// One MIXER strip's state (M6: level, pan, sends, mute, solo) plus the phase of its fake
/// meter pulse.
#[derive(Clone, Copy)]
struct Strip {
    level_db: f32,
    pan: f32,
    delay_send: f32,
    reverb_send: f32,
    mute: bool,
    solo: bool,
    /// Offset of this strip's pulse within the beat (fraction).
    phase: f32,
}

impl Strip {
    /// A fake peak level: a 120 BPM decaying pulse scaled by the fader, silent while muted or
    /// while another strip is soloed (`heard` is the mute/solo verdict).
    fn fake_level(&self, t: f32, heard: bool) -> f32 {
        if !heard {
            return 0.0;
        }
        let beat = (t * 2.0 + self.phase).fract();
        let pulse = (1.0 - beat).powi(2);
        10f32.powf(self.level_db / 20.0) * pulse
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
    /// The segmented demo row: a unison voice count and a two-item tab.
    unison: usize,
    lane_view: usize,
    params: Params,
    /// Notes held by the computer-keyboard mapping (shown lit) plus the mouse.
    held: [bool; 128],
    /// Computer-key caps for the two octaves of the mock (A W S E D F T G Y H U J K O L P ;).
    labels: [Option<char>; 128],
    last_piano: neu::PianoOutput,
    /// SEQUENCER panel state: drum steps (0 off, 1 on, 2 accent), bass steps (semitones above
    /// C2, -1 rest), the bass accent flags, the bass cell selected for note entry, the per-row
    /// volume knobs / bass tune knob, and the running playhead step.
    drums: [[u8; 16]; 5],
    bass: [i8; 16],
    bass_accent: [bool; 16],
    bass_selected: Option<usize>,
    drum_vol: [f32; 5],
    bass_tune: f32,
    playhead: usize,
    /// MIXER panel state: three strips and the master's stereo peak (held for display).
    mixer: [Strip; 3],
    /// PIANO ROLL panel state: the view model (scroll, selection, playhead) and the notes the
    /// roll edits through its output.
    roll: PianoRollModel,
    roll_notes: Vec<RollNote>,
    /// The last edit the roll reported, for the panel head.
    last_roll_edit: String,
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
            unison: 2,
            lane_view: 0,
            params: Params::default(),
            held,
            labels: Self::labels_for_octave(4),
            last_piano: neu::PianoOutput::default(),
            drums: [
                DRUM_ROWS[0].2,
                DRUM_ROWS[1].2,
                DRUM_ROWS[2].2,
                DRUM_ROWS[3].2,
                DRUM_ROWS[4].2,
            ],
            bass: BASS_ROW,
            bass_accent: [false; 16],
            bass_selected: Some(6),
            drum_vol: [
                DRUM_ROWS[0].1,
                DRUM_ROWS[1].1,
                DRUM_ROWS[2].1,
                DRUM_ROWS[3].1,
                DRUM_ROWS[4].1,
            ],
            bass_tune: 0.0,
            playhead: 4,
            mixer: [
                Strip {
                    level_db: 0.0,
                    pan: 0.0,
                    delay_send: 0.0,
                    reverb_send: 0.0,
                    mute: false,
                    solo: false,
                    phase: 0.0,
                },
                Strip {
                    level_db: -4.0,
                    pan: 20.0,
                    delay_send: 0.0,
                    reverb_send: 30.0,
                    mute: false,
                    solo: false,
                    phase: 0.5,
                },
                Strip {
                    level_db: -9.0,
                    pan: -35.0,
                    delay_send: 100.0,
                    reverb_send: 100.0,
                    mute: true,
                    solo: false,
                    phase: 0.25,
                },
            ],
            roll: PianoRollModel::default(),
            roll_notes: Self::demo_roll_notes(),
            last_roll_edit: String::new(),
            started: Instant::now(),
            exit_after,
            scroll_to,
        }
    }

    fn base_note(octave: i32) -> u8 {
        ((octave + 1) * 12).clamp(0, 127) as u8
    }

    /// The PIANO ROLL demo: a C minor chord (C3 Eb3 G3) over the first beat, a few bass
    /// pick-ups, and a long accented C4 across the second half of the bar.
    fn demo_roll_notes() -> Vec<RollNote> {
        let note = |start: u8, length: u8, pitch: u8, velocity: f32| RollNote {
            start,
            length,
            pitch,
            velocity,
            selected: false,
        };
        vec![
            note(0, 4, 48, 0.8),
            note(0, 4, 51, 0.8),
            note(0, 4, 55, 0.8),
            note(4, 1, 43, 0.8),
            note(6, 2, 46, 1.0),
            note(8, 8, 60, 1.0),
            note(12, 1, 58, 0.8),
            note(14, 2, 55, 0.8),
        ]
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
                            // The mock's `.brand`: the wordmark with +0.16em tracking.
                            neu::brand(ui, &theme, "BeatSlopper");
                            ui.add_space(24.0);
                            neu::label(ui, &theme, "M8 · Soft Console gallery");
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
                        ui.add_space(14.0);
                        // Segmented selectors: the sliding accent chip (the unison 1..7 row,
                        // the widest in the app), a two-item tab and a disabled one.
                        ui.horizontal(|ui| {
                            neu::label(ui, &theme, "Segmented");
                            neu::segmented(
                                ui,
                                &theme,
                                &mut self.unison,
                                &["1", "2", "3", "4", "5", "6", "7"],
                            );
                            neu::segmented(ui, &theme, &mut self.lane_view, &["Grid", "Roll"]);
                            ui.add_enabled_ui(false, |ui| {
                                let mut fixed = 1;
                                neu::segmented(ui, &theme, &mut fixed, &["Project", "Chain"]);
                            });
                            neu::label(ui, &theme, "disabled");
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
                    self.mixer_panel(ui, &theme);

                    // SEQUENCER panel: the mock's demo pattern with a playhead that runs at
                    // 120 BPM (a 16th every 125 ms) while "Play" is on.
                    if self.playing {
                        self.playhead = (self.started.elapsed().as_millis() / 125 % 16) as usize;
                        ctx.request_repaint_after(Duration::from_millis(30));
                    }
                    self.sequencer_panel(ui, &theme);
                    self.piano_roll_panel(ui, &theme);
                    self.visualizer_panel(ui, &theme);

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
                        // PULSE only shapes the square wave: dimmed (still live) otherwise.
                        neu::knob_dimmed(
                            ui,
                            theme,
                            &mut p.osc1_pulse,
                            &pct_knob("Pulse", 50.0),
                            (p.osc1_wave != 2).then_some("Square wave only"),
                        );
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
                        neu::knob_dimmed(
                            ui,
                            theme,
                            &mut p.osc2_pulse,
                            &pct_knob("Pulse", 50.0),
                            (p.osc2_wave != 2).then_some("Square wave only"),
                        );
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

    /// The M6 MIXER panel: three channel strips (name, bipolar pan knob, DELAY / REVERB send
    /// knobs, fader beside a peak meter fed a fake pulsing level, M / S toggles) and a MASTER
    /// strip with the stereo meter and the LIMIT toggle. Muted strips and, when any strip is
    /// soloed, the others fall silent in the meters.
    fn mixer_panel(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        const STRIP_GAP: f32 = 22.0;
        const KNOB_GAP: f32 = 6.0;
        let t = self.started.elapsed().as_secs_f32();
        let any_solo = self.mixer.iter().any(|s| s.solo);
        let heard = |s: &Strip| !s.mute && (!any_solo || s.solo);
        let levels: [f32; 3] = std::array::from_fn(|i| {
            let s = &self.mixer[i];
            if self.playing {
                s.fake_level(t, heard(s))
            } else {
                0.0
            }
        });
        // Master: the loudest strip, leaning left or right with its pan.
        let (ml, mr) = self
            .mixer
            .iter()
            .zip(levels)
            .fold((0.0f32, 0.0f32), |(l, r), (s, lv)| {
                let p = (s.pan / 100.0).clamp(-1.0, 1.0);
                (
                    l.max(lv * (1.0 - p.max(0.0))),
                    r.max(lv * (1.0 + p.min(0.0))),
                )
            });
        if self.playing {
            ui.ctx().request_repaint_after(Duration::from_millis(30));
        }

        let limiter = &mut self.limiter;
        let strips = &mut self.mixer;
        neu::panel(ui, theme, |ui| {
            ui.horizontal(|ui| {
                neu::title(ui, theme, "Mixer");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    neu::label(ui, theme, "Fader, meter, M / S");
                });
            });
            ui.add_space(14.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(STRIP_GAP, 10.0);
                for (i, strip) in strips.iter_mut().enumerate() {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = vec2(KNOB_GAP, 10.0);
                        col_label(ui, theme, MIXER_STRIPS[i]);
                        neu::knob(
                            ui,
                            theme,
                            &mut strip.pan,
                            &KnobSpec {
                                label: "Pan",
                                min: -100.0,
                                max: 100.0,
                                default: 0.0,
                                log: false,
                                bipolar: true,
                                size: KnobSize::Sm,
                                fmt: fmt_pan,
                            },
                        );
                        ui.horizontal(|ui| {
                            neu::knob(
                                ui,
                                theme,
                                &mut strip.delay_send,
                                &KnobSpec {
                                    size: KnobSize::Sm,
                                    ..pct_knob("Dly", 0.0)
                                },
                            );
                            neu::knob(
                                ui,
                                theme,
                                &mut strip.reverb_send,
                                &KnobSpec {
                                    size: KnobSize::Sm,
                                    ..pct_knob("Rev", 0.0)
                                },
                            );
                        });
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            neu::fader(ui, theme, &mut strip.level_db, &LEVEL_FADER);
                            // The meter sits level with the fader's track (below the value).
                            ui.vertical(|ui| {
                                ui.add_space(TextKind::Value.size() + 8.0 + 4.0);
                                neu::meter(ui, theme, levels[i], neu::METER_TRACK);
                            });
                        });
                        ui.horizontal(|ui| {
                            if neu::toggle_button_sized(
                                ui,
                                theme,
                                "M",
                                strip.mute,
                                neu::TOGGLE_SQUARE,
                            )
                            .clicked()
                            {
                                strip.mute = !strip.mute;
                            }
                            if neu::toggle_button_sized(
                                ui,
                                theme,
                                "S",
                                strip.solo,
                                neu::TOGGLE_SQUARE,
                            )
                            .clicked()
                            {
                                strip.solo = !strip.solo;
                            }
                        });
                    });
                }

                // MASTER strip: stereo meter and the limiter toggle.
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(KNOB_GAP, 10.0);
                    col_label(ui, theme, "Master");
                    neu::value(
                        ui,
                        theme,
                        &format!("{:+.1} dB", 20.0 * ml.max(mr).max(1e-6).log10()),
                    );
                    neu::stereo_meter(ui, theme, (ml, mr), neu::STEREO_METER_SIZE);
                    neu::label(ui, theme, "Peak");
                    if neu::toggle_button(ui, theme, "Limit", *limiter).clicked() {
                        *limiter = !*limiter;
                    }
                });
            });
        });
    }

    /// The mock's SEQUENCER panel: title, hint and "Clear all" in the head; a header row with
    /// the beat numbers and the playhead dots over the step columns; five drum rows (name,
    /// small volume knob, 16 step cells, a Clear pill) and the bass row whose cells carry note
    /// names. Click toggles a step, shift-click accents it (drums) or steps to the next note
    /// (bass); the last clicked bass cell shows the selection ring.
    fn sequencer_panel(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let shift = ui.input(|i| i.modifiers.shift);
        let steps_x = SEQ_NAME_WIDTH + SEQ_COL_GAP + SEQ_CTL_WIDTH + SEQ_COL_GAP;
        neu::panel(ui, theme, |ui| {
            ui.horizontal(|ui| {
                neu::title(ui, theme, "Sequencer");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    if neu::button(ui, theme, "Clear all").clicked() {
                        self.drums = [[0; 16]; 5];
                        self.bass = [-1; 16];
                        self.bass_accent = [false; 16];
                    }
                    neu::label(ui, theme, "shift-click = accent / next note");
                });
            });
            ui.add_space(14.0);

            // `.seq-head`: beat numbers 6 pt over the playhead dots, aligned with the steps.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(steps_x);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    neu::beat_numbers(ui, theme, 16);
                    neu::playhead_row(ui, theme, 16, Some(self.playhead));
                });
            });
            ui.add_space(6.0);

            // `.seq-grid`: rows 2 pt apart, each 34 pt high with 16 pt column gaps.
            ui.spacing_mut().item_spacing.y = 2.0;
            for (r, &(name, default_vol, _)) in DRUM_ROWS.iter().enumerate() {
                let specs: Vec<StepCellSpec<'_>> = self.drums[r]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| StepCellSpec {
                        on: v > 0,
                        accent: v == 2,
                        current: i == self.playhead,
                        selected: false,
                        label: None,
                    })
                    .collect();
                let mut clicked = None;
                let mut clear = false;
                Self::seq_row(ui, theme, name, &specs, &mut clicked, &mut clear, |ui| {
                    neu::knob(
                        ui,
                        theme,
                        &mut self.drum_vol[r],
                        &KnobSpec {
                            label: "",
                            size: KnobSize::Sm,
                            ..pct_knob("Vol", default_vol)
                        },
                    );
                });
                if let Some(i) = clicked {
                    let v = &mut self.drums[r][i];
                    *v = if shift {
                        if *v == 2 { 1 } else { 2 }
                    } else if *v > 0 {
                        0
                    } else {
                        1
                    };
                }
                if clear {
                    self.drums[r] = [0; 16];
                }
            }

            // Bass row: note labels, "–" on rests, the selected cell ringed.
            let labels: Vec<String> = self
                .bass
                .iter()
                .map(|&v| {
                    if v >= 0 {
                        bass_note_name(v)
                    } else {
                        "–".to_owned()
                    }
                })
                .collect();
            let specs: Vec<StepCellSpec<'_>> = self
                .bass
                .iter()
                .enumerate()
                .map(|(i, &v)| StepCellSpec {
                    on: v >= 0,
                    accent: self.bass_accent[i],
                    current: i == self.playhead,
                    selected: self.bass_selected == Some(i),
                    label: Some(labels[i].as_str()),
                })
                .collect();
            let mut clicked = None;
            let mut clear = false;
            Self::seq_row(ui, theme, "Bass", &specs, &mut clicked, &mut clear, |ui| {
                neu::knob(
                    ui,
                    theme,
                    &mut self.bass_tune,
                    &KnobSpec {
                        label: "",
                        min: -12.0,
                        max: 12.0,
                        default: 0.0,
                        log: false,
                        bipolar: true,
                        size: KnobSize::Sm,
                        fmt: |v| format!("{v:.0} st"),
                    },
                );
            });
            if let Some(i) = clicked {
                let v = &mut self.bass[i];
                *v = if shift {
                    if *v < 0 { 0 } else { (*v + 1) % 24 }
                } else if *v >= 0 {
                    -1
                } else {
                    0
                };
                self.bass_selected = Some(i);
            }
            if clear {
                self.bass = [-1; 16];
                self.bass_accent = [false; 16];
            }
        });
    }

    /// The M7 PIANO ROLL panel: title, a gesture hint, the selection read-out and a Clear pill
    /// in the head; the roll below with the sequencer's playhead (fractional while playing).
    /// The gallery owns the notes and applies every edit the roll reports.
    /// VISUALIZERS panel: the six drawings side by side over a synthetic "beat" (a decaying
    /// 55 Hz bass tone retriggered every beat, a hat-like noise burst every 16th, a kick hit
    /// bit on each beat, a spectrum shaped from the same envelopes), animated while "Play"
    /// is on, each with its preset's style.
    fn visualizer_panel(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let t = self.started.elapsed().as_secs_f64();
        let playing = self.playing;
        let beat_phase = ((t * 2.0) % 1.0) as f32; // 120 BPM: a beat every 0.5 s
        let step_phase = ((t * 8.0) % 1.0) as f32; // a 16th every 125 ms
        let env = if playing {
            (-beat_phase * 6.0).exp()
        } else {
            0.0
        };
        let hat = if playing {
            (-step_phase * 20.0).exp() * 0.35
        } else {
            0.0
        };
        let n = 2048;
        let mut seed = 0x2545_F491_u32;
        let mut noise = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        let left: Vec<f32> = (0..n)
            .map(|i| {
                let x = i as f32 / 48_000.0;
                let bass = (x * 55.0 * std::f32::consts::TAU).sin() * 0.8 * env;
                let click = (x * 880.0 * std::f32::consts::TAU).sin() * 0.3 * env * env;
                (bass + click + noise() * hat).clamp(-1.0, 1.0)
            })
            .collect();
        let right: Vec<f32> = left
            .iter()
            .enumerate()
            .map(|(i, s)| (s * 0.8 + noise() * hat * 0.5 * (i % 2) as f32).clamp(-1.0, 1.0))
            .collect();
        let spectrum: Vec<f32> = (0..64)
            .map(|k| {
                let low = (-(k as f32 - 6.0).powi(2) / 18.0).exp() * env;
                let mid = (-(k as f32 - 30.0).powi(2) / 60.0).exp() * env * 0.5;
                let high = (k as f32 / 64.0) * hat * 2.0;
                (low + mid + high).clamp(0.0, 1.0)
            })
            .collect();
        let hits = if playing && beat_phase < 0.05 {
            0b1_1111
        } else if playing && step_phase < 0.2 {
            0b1000
        } else {
            0
        };
        let peak = (env * 0.9 + hat).clamp(0.0, 1.0);
        let input = VisualInput {
            samples: &left,
            samples_r: Some(&right),
            spectrum: &spectrum,
            peak: (peak, peak * 0.85),
            hits,
            playhead: beat_phase,
            bpm: 120.0,
            time: t,
        };
        if playing {
            ui.ctx().request_repaint();
        }
        neu::panel(ui, theme, |ui| {
            ui.horizontal(|ui| {
                neu::title(ui, theme, "Visualizers");
                ui.add_space(24.0);
                neu::label(
                    ui,
                    theme,
                    "Press Play in the sequencer to animate; each tile is one preset",
                );
            });
            ui.add_space(14.0);
            let presets: Vec<&neu::VisualPreset> =
                neu::PRESETS.iter().filter(|p| !p.is_off()).collect();
            let columns = 3;
            let gap = 16.0;
            let tile_w = (ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32;
            let tile_h = 150.0;
            for row in presets.chunks(columns) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for preset in row {
                        ui.allocate_ui_with_layout(
                            vec2(tile_w, tile_h + 24.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_max_width(tile_w);
                                ui.spacing_mut().item_spacing.y = 6.0;
                                neu::label(ui, theme, preset.name);
                                ui.allocate_ui_with_layout(
                                    vec2(tile_w, tile_h),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.set_min_size(vec2(tile_w, tile_h));
                                        preset.show(ui, theme, &input);
                                    },
                                );
                            },
                        );
                    }
                });
                ui.add_space(gap);
            }
        });
    }

    fn piano_roll_panel(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        self.roll.playhead_step = self.playing.then(|| {
            let ms = self.started.elapsed().as_millis() as f32;
            (ms / 125.0) % self.roll.steps as f32
        });
        neu::panel(ui, theme, |ui| {
            ui.horizontal(|ui| {
                neu::title(ui, theme, "Piano roll");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    if neu::button(ui, theme, "Clear").clicked() {
                        self.roll_notes.clear();
                        self.roll.selected = None;
                        self.last_roll_edit = "cleared".to_owned();
                    }
                    let selected = match self.roll.selected {
                        Some(i) => self
                            .roll_notes
                            .get(i)
                            .map(|n| {
                                format!(
                                    "{} @ {} x{}",
                                    neu::note_name(n.pitch),
                                    n.start + 1,
                                    n.length
                                )
                            })
                            .unwrap_or_default(),
                        None => "none".to_owned(),
                    };
                    neu::value(ui, theme, &format!("sel {selected}  ·  {}", self.last_roll_edit));
                    neu::label(
                        ui,
                        theme,
                        "drag = draw / move / edge resize · dbl-click = delete · shift-click = accent · wheel = scroll",
                    );
                });
            });
            ui.add_space(14.0);
            let out = neu::piano_roll(ui, theme, &mut self.roll, &self.roll_notes);
            if let Some(n) = out.add {
                self.roll_notes.push(n);
                self.roll.selected = Some(self.roll_notes.len() - 1);
                self.last_roll_edit = format!("added {} x{}", neu::note_name(n.pitch), n.length);
            } else if let Some((i, n)) = out.moved {
                if let Some(slot) = self.roll_notes.get_mut(i) {
                    *slot = n;
                    self.last_roll_edit = format!(
                        "moved {} @ {} x{} v{:.1}",
                        neu::note_name(n.pitch),
                        n.start + 1,
                        n.length,
                        n.velocity
                    );
                }
            } else if let Some(i) = out.removed
                && i < self.roll_notes.len()
            {
                let n = self.roll_notes.remove(i);
                self.last_roll_edit = format!("removed {}", neu::note_name(n.pitch));
                // Indices above the removed note shift down by one.
                self.roll.selected = match self.roll.selected {
                    Some(s) if s == i => None,
                    Some(s) if s > i => Some(s - 1),
                    other => other,
                };
            }
            if out.scrolled {
                self.last_roll_edit = format!("view from {}", neu::note_name(self.roll.low_pitch));
            }
        });
    }

    /// One `.seq-row`: an 80 pt name column, a 34 pt control column (`ctl`), the step cells
    /// and a Clear pill, 16 pt apart, centred in a 34 pt row. Reports the clicked cell and the
    /// Clear press through the out parameters.
    #[allow(clippy::too_many_arguments)]
    fn seq_row(
        ui: &mut egui::Ui,
        theme: &Theme,
        name: &str,
        specs: &[StepCellSpec<'_>],
        clicked: &mut Option<usize>,
        clear: &mut bool,
        ctl: impl FnOnce(&mut egui::Ui),
    ) {
        ui.allocate_ui_with_layout(
            vec2(ui.available_width().max(1.0), SEQ_ROW_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = SEQ_COL_GAP;
                fixed_cell(
                    ui,
                    vec2(SEQ_NAME_WIDTH, SEQ_ROW_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        neu::label(ui, theme, name);
                    },
                );
                fixed_cell(
                    ui,
                    vec2(SEQ_CTL_WIDTH, SEQ_ROW_HEIGHT),
                    Layout::centered_and_justified(egui::Direction::LeftToRight),
                    ctl,
                );
                *clicked = neu::step_row(ui, theme, name, specs).clicked;
                if neu::button(ui, theme, "Clear").clicked() {
                    *clear = true;
                }
            },
        );
    }
}

/// A child ui of exactly `size` (the parent's cursor advances by `size` whatever the content
/// does; `Ui::allocate_ui_with_layout` would size the cell to its content instead), so the
/// sequencer's name and control columns stay fixed and the step columns line up.
fn fixed_cell(
    ui: &mut egui::Ui,
    size: Vec2,
    layout: Layout,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    add_contents(&mut child);
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
