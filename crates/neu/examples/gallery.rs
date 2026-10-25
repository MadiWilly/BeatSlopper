//! Visual gallery of the `neu` Deck kit (M12): the component sheet of
//! `design/round3/WINNER.md` / `preview-b.html` section 8, every widget in every state.
//!
//! `cargo run -p neu --example gallery [-- --exit-after-secs N] [--light|--dark|--both]
//! [--scroll PX]`
//!
//! The page is one sheet of cards: buttons (secondary / primary / toggle / icon / small),
//! segmented / dropdown / switch, text field / LEDs / tags, knobs (32 / 22 / 40 / bipolar /
//! disabled), fader / meters / sliders, step cells / note bars / ghosts (a step row and a
//! piano roll with a velocity lane), tabs / list rows / tooltip, the menu, scrollbars inside a
//! window header, the token swatches, the type scale, and keyboard keys / a mixer strip /
//! browser chips. `--both` (the default) shows the sheet twice, dark beside light; the
//! segmented at the top switches between dark, light and both.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Layout, Vec2, vec2};
use neu::{
    ButtonKind, ButtonSize, DialSize, GhostNote, Icon, KnobSize, KnobSpec, LedKind, MenuItem,
    PianoRollModel, RollNote, SliderSpec, StepCellSpec, TagKind, TextKind, Theme, ThemeMode,
    VisualInput, VisualStyle,
};

/// Which columns the page shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Dark,
    Light,
    Both,
}

fn fmt_pct(v: f32) -> String {
    format!("{v:.0} %")
}

fn fmt_cents(v: f32) -> String {
    format!("{v:+.0} c")
}

fn fmt_hz(v: f32) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

fn fmt_db(v: f32) -> String {
    format!("{v:+.1} dB")
}

/// The sheet's live values, shared by both columns so the two themes stay in step.
struct Sheet {
    follow: bool,
    solo: bool,
    armed: bool,
    playing: bool,
    switch_on: bool,
    switch_off: bool,
    pattern_song: usize,
    wave: usize,
    snap: usize,
    search: String,
    cutoff: f32,
    env: f32,
    detune: f32,
    pulse: f32,
    mini: f32,
    master: f32,
    fader: f32,
    vslider: f32,
    hslider: f32,
    octave: i32,
    insert: u8,
    steps: [u8; 16],
    roll: PianoRollModel,
    notes: Vec<RollNote>,
    tab: usize,
    picked_row: usize,
    menu_pick: Option<usize>,
    held: [bool; 128],
    labels: [Option<char>; 128],
    strip_on: [bool; 3],
    chip_on: usize,
}

impl Sheet {
    fn new() -> Self {
        let mut labels = [None; 128];
        for (i, c) in "AWSEDFTGYHUJKOLP;".chars().enumerate() {
            labels[60 + i] = Some(c);
        }
        let mut held = [false; 128];
        held[64] = true;
        held[66] = true;
        Self {
            follow: true,
            solo: true,
            armed: true,
            playing: false,
            switch_on: true,
            switch_off: false,
            pattern_song: 0,
            wave: 1,
            snap: 2,
            search: String::new(),
            cutoff: 2400.0,
            env: 35.0,
            detune: 0.0,
            pulse: 50.0,
            mini: 70.0,
            master: 80.0,
            fader: 0.0,
            vslider: 80.0,
            hslider: 80.0,
            octave: 4,
            insert: 3,
            steps: [1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0],
            roll: PianoRollModel {
                pitches: 12,
                low_pitch: 48,
                playhead_step: Some(6.0),
                ..PianoRollModel::default()
            },
            notes: vec![
                RollNote {
                    start: 0,
                    length: 3,
                    pitch: 52,
                    velocity: 1.0,
                    selected: false,
                },
                RollNote {
                    start: 3,
                    length: 2,
                    pitch: 55,
                    velocity: 0.5,
                    selected: false,
                },
                RollNote {
                    start: 5,
                    length: 2,
                    pitch: 57,
                    velocity: 0.8,
                    selected: true,
                },
                RollNote {
                    start: 8,
                    length: 4,
                    pitch: 52,
                    velocity: 0.7,
                    selected: false,
                },
            ],
            tab: 0,
            picked_row: 2,
            menu_pick: None,
            held,
            labels,
            strip_on: [true, false, false],
            chip_on: 0,
        }
    }
}

struct Gallery {
    view: View,
    applied: Option<ThemeMode>,
    sheet: Sheet,
    started: Instant,
    exit_after: Option<Duration>,
    scroll_to: Option<f32>,
}

impl Gallery {
    fn new(view: View, exit_after: Option<Duration>, scroll_to: Option<f32>) -> Self {
        Self {
            view,
            applied: None,
            sheet: Sheet::new(),
            started: Instant::now(),
            exit_after,
            scroll_to,
        }
    }

    /// The theme egui's own style follows (scrollbars, popups): the first column's.
    fn primary_mode(&self) -> ThemeMode {
        match self.view {
            View::Light => ThemeMode::Light,
            View::Dark | View::Both => ThemeMode::Dark,
        }
    }
}

/// One card of the sheet: a `panel` frame with a section head.
fn card(ui: &mut egui::Ui, theme: &Theme, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    neu::group(
        ui,
        theme,
        title,
        |_| {},
        |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            body(ui);
        },
    );
    ui.add_space(4.0);
}

/// A small caption under a control.
fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    neu::text::text_widget(ui, text, TextKind::Micro, theme.text_3);
}

/// A control with a caption under it.
fn captioned(ui: &mut egui::Ui, theme: &Theme, text: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        add(ui);
        caption(ui, theme, text);
    });
}

fn sheet(ui: &mut egui::Ui, theme: &Theme, s: &mut Sheet, playing_time: f64) {
    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
    let mode = theme.mode;

    card(ui, theme, "Buttons · secondary / primary / toggle", |ui| {
        ui.horizontal_wrapped(|ui| {
            captioned(ui, theme, "default", |ui| {
                neu::button(ui, theme, "Copy");
            });
            captioned(ui, theme, "disabled", |ui| {
                ui.add_enabled_ui(false, |ui| {
                    neu::button(ui, theme, "Copy");
                });
            });
            captioned(ui, theme, "primary", |ui| {
                neu::primary_button(ui, theme, "Export");
            });
            captioned(ui, theme, "primary off", |ui| {
                ui.add_enabled_ui(false, |ui| {
                    neu::primary_button(ui, theme, "Export");
                });
            });
            captioned(ui, theme, "ghost", |ui| {
                neu::ghost_button(ui, theme, "Ghost");
            });
            captioned(ui, theme, "toggle", |ui| {
                if neu::toggle_button(ui, theme, "Follow", s.follow).clicked() {
                    s.follow = !s.follow;
                }
            });
            captioned(ui, theme, "solo", |ui| {
                if neu::solo_button(ui, theme, "S", s.solo).clicked() {
                    s.solo = !s.solo;
                }
            });
            captioned(ui, theme, "rec armed", |ui| {
                if neu::rec_button(ui, theme, s.armed).clicked() {
                    s.armed = !s.armed;
                }
            });
            captioned(ui, theme, "M / S", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    neu::toggle_button_sized(ui, theme, "M", false, neu::TOGGLE_SQUARE);
                    neu::toggle_button_sized(ui, theme, "S", true, neu::TOGGLE_SQUARE);
                });
            });
            captioned(ui, theme, "small / xs", |ui| {
                ui.horizontal(|ui| {
                    neu::small_button(ui, theme, "Len");
                    neu::button_kind(ui, theme, "Open", ButtonKind::Secondary, ButtonSize::Xs);
                });
            });
        });
        ui.horizontal_wrapped(|ui| {
            captioned(ui, theme, "play (primary)", |ui| {
                let icon = if s.playing { Icon::Stop } else { Icon::Play };
                if neu::icon_button_kind(ui, theme, icon, vec2(28.0, 28.0), ButtonKind::Primary)
                    .clicked()
                {
                    s.playing = !s.playing;
                }
            });
            captioned(ui, theme, "icon", |ui| {
                neu::icon_button(ui, theme, Icon::Stop, 28.0);
            });
            captioned(ui, theme, "ghost icons", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for icon in [Icon::Menu, Icon::Undo, Icon::Redo, Icon::Copy, Icon::Paste] {
                        neu::icon_button_kind(ui, theme, icon, vec2(28.0, 28.0), ButtonKind::Ghost);
                    }
                });
            });
            captioned(ui, theme, "views (rail)", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, (icon, name)) in [
                        (Icon::Roll, "Piano roll (F5)"),
                        (Icon::Mixer, "Mixer (F6)"),
                        (Icon::Synth, "Synth & FX (F7)"),
                        (Icon::Song, "Song (F8)"),
                        (Icon::Browser, "Browser (F9)"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if neu::rail_button(ui, theme, icon, s.tab == i, name).clicked() {
                            s.tab = i;
                        }
                    }
                });
            });
            captioned(ui, theme, "header 24", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    neu::header_button(ui, theme, Icon::Minus);
                    neu::header_button(ui, theme, Icon::ChevronDown);
                    neu::header_button(ui, theme, Icon::Clear);
                });
            });
            captioned(ui, theme, "stepper", |ui| {
                neu::stepper(ui, theme, "Oct", &mut s.octave, 1..=7);
            });
            captioned(ui, theme, "separator", |ui| {
                ui.horizontal(|ui| {
                    neu::button(ui, theme, "Tap");
                    neu::toolbar_separator(ui, theme);
                    neu::display(ui, theme, "120");
                });
            });
        });
    });

    card(ui, theme, "Segmented · dropdown · switch", |ui| {
        ui.horizontal_wrapped(|ui| {
            captioned(ui, theme, "segmented", |ui| {
                neu::segmented(ui, theme, &mut s.pattern_song, &["Pattern", "Song"]);
            });
            captioned(ui, theme, "small", |ui| {
                neu::segmented_small(ui, theme, &mut s.wave, &["Sine", "Saw", "Square", "Tri"]);
            });
            captioned(ui, theme, "one item", |ui| {
                neu::segmented(ui, theme, &mut 0, &["None"]);
            });
            captioned(ui, theme, "dropdown", |ui| {
                const SNAPS: [&str; 4] = ["1/4", "1/8", "1/16", "1/32"];
                if let Some(i) =
                    neu::dropdown(ui, theme, "Snap", SNAPS[s.snap], &SNAPS, 90.0, false, None)
                {
                    s.snap = i;
                }
            });
            captioned(ui, theme, "track (small)", |ui| {
                neu::dropdown(
                    ui,
                    theme,
                    "Track",
                    "1 · Keys",
                    &["1 · Keys", "2 · Bass"],
                    110.0,
                    true,
                    Some(theme.track_colour(4)),
                );
            });
            captioned(ui, theme, "switch off / on", |ui| {
                ui.horizontal(|ui| {
                    neu::switch(ui, theme, &mut s.switch_off, "Ghosts");
                    neu::switch(ui, theme, &mut s.switch_on, "Follow");
                    neu::switch_small(ui, theme, &mut s.switch_on, "Mix");
                });
            });
            captioned(ui, theme, "theme", |ui| {
                let mut m = mode;
                neu::theme_toggle(ui, theme, &mut m);
            });
        });
    });

    card(ui, theme, "Text field · LEDs · tags · keycaps", |ui| {
        ui.horizontal_wrapped(|ui| {
            captioned(ui, theme, "field", |ui| {
                neu::text_field(
                    ui,
                    theme,
                    &mut s.search,
                    "Search presets",
                    Some(Icon::Search),
                    150.0,
                );
            });
            captioned(ui, theme, "number box", |ui| {
                neu::number_box(ui, theme, &mut s.insert, 0..=16);
            });
            captioned(ui, theme, "LED on / off / accent / rec", |ui| {
                ui.horizontal(|ui| {
                    neu::led(ui, theme, true, LedKind::Ok);
                    neu::led(ui, theme, false, LedKind::Ok);
                    neu::led(ui, theme, true, LedKind::Accent);
                    neu::led(ui, theme, true, LedKind::Rec);
                });
            });
            captioned(ui, theme, "lamps", |ui| {
                ui.horizontal(|ui| {
                    neu::lamp(ui, theme, true, "MIDI");
                    neu::lamp_kind(ui, theme, true, "Limit", LedKind::Accent);
                });
            });
            captioned(ui, theme, "tags", |ui| {
                ui.horizontal(|ui| {
                    neu::tag(ui, theme, "vinyl", TagKind::Vinyl);
                    neu::tag(ui, theme, "drone", TagKind::Drone);
                    neu::tag(ui, theme, "slot 2", TagKind::Plain);
                });
            });
            captioned(ui, theme, "chips", |ui| {
                ui.horizontal(|ui| {
                    for i in 0..8 {
                        neu::chip(ui, theme, theme.track_colour(i));
                    }
                });
            });
            captioned(ui, theme, "keycaps", |ui| {
                ui.horizontal(|ui| {
                    neu::keycap(ui, theme, "F5");
                    neu::keycap(ui, theme, "Ctrl+Shift+S");
                });
            });
            captioned(ui, theme, "name buttons", |ui| {
                ui.horizontal(|ui| {
                    neu::name_button(ui, theme, "Kick", None, false);
                    neu::name_button(ui, theme, "Dusty Rhodes", Some(theme.track_colour(4)), true);
                });
            });
            captioned(ui, theme, "pattern tiles", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, l) in ["A", "B", "C", "D"].iter().enumerate() {
                        neu::pattern_tile(ui, theme, l, i < 2, i == 0);
                    }
                });
            });
        });
    });

    card(
        ui,
        theme,
        "Knobs · 32 / mini 22 / 40 / bipolar / disabled",
        |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                neu::knob(
                    ui,
                    theme,
                    &mut s.cutoff,
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
                );
                neu::knob(
                    ui,
                    theme,
                    &mut s.env,
                    &KnobSpec {
                        label: "Env",
                        min: -100.0,
                        max: 100.0,
                        default: 0.0,
                        log: false,
                        bipolar: true,
                        size: KnobSize::Md,
                        fmt: |v| format!("{v:+.0} %"),
                    },
                );
                neu::knob(
                    ui,
                    theme,
                    &mut s.detune,
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
                neu::knob_dimmed(
                    ui,
                    theme,
                    &mut s.pulse,
                    &KnobSpec {
                        label: "Pulse",
                        min: 0.0,
                        max: 100.0,
                        default: 50.0,
                        log: false,
                        bipolar: false,
                        size: KnobSize::Md,
                        fmt: fmt_pct,
                    },
                    Some("Square wave only"),
                );
                captioned(ui, theme, "mini", |ui| {
                    neu::knob(
                        ui,
                        theme,
                        &mut s.mini,
                        &KnobSpec {
                            label: "",
                            min: 0.0,
                            max: 100.0,
                            default: 80.0,
                            log: false,
                            bipolar: false,
                            size: KnobSize::Sm,
                            fmt: fmt_pct,
                        },
                    );
                });
                captioned(ui, theme, "inline", |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        neu::label(ui, theme, "Swing");
                        neu::knob(
                            ui,
                            theme,
                            &mut s.mini,
                            &KnobSpec {
                                label: "",
                                min: 0.0,
                                max: 100.0,
                                default: 50.0,
                                log: false,
                                bipolar: false,
                                size: KnobSize::Sm,
                                fmt: fmt_pct,
                            },
                        );
                        neu::value(ui, theme, &fmt_pct(s.mini));
                    });
                });
                neu::dial(
                    ui,
                    theme,
                    &mut s.master,
                    &KnobSpec {
                        label: "Master",
                        min: 0.0,
                        max: 100.0,
                        default: 80.0,
                        log: false,
                        bipolar: false,
                        size: KnobSize::Lg,
                        fmt: fmt_pct,
                    },
                    DialSize::Lg,
                );
                neu::dial(
                    ui,
                    theme,
                    &mut s.cutoff,
                    &KnobSpec {
                        label: "Cutoff",
                        min: 20.0,
                        max: 20000.0,
                        default: 2400.0,
                        log: true,
                        bipolar: false,
                        size: KnobSize::Lg,
                        fmt: fmt_hz,
                    },
                    DialSize::Sm,
                );
            });
        },
    );

    card(ui, theme, "Fader · meters · sliders", |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let t = playing_time as f32;
            let level = 0.6 + 0.35 * (t * 3.0).sin().abs();
            captioned(ui, theme, "fader 0 dB", |ui| {
                neu::fader(
                    ui,
                    theme,
                    &mut s.fader,
                    &SliderSpec {
                        label: "Level",
                        min: -60.0,
                        max: 6.0,
                        default: 0.0,
                        fmt: fmt_db,
                    },
                );
            });
            captioned(ui, theme, "stereo", |ui| {
                neu::stereo_meter(
                    ui,
                    theme,
                    (level * 0.8, level * 0.7),
                    neu::STEREO_METER_SIZE,
                );
            });
            captioned(ui, theme, "clip", |ui| {
                neu::stereo_meter(ui, theme, (0.97, 0.95), neu::STEREO_METER_SIZE);
            });
            captioned(ui, theme, "GR", |ui| {
                neu::gr_meter(ui, theme, 0.22, 120.0);
            });
            captioned(ui, theme, "v-slider", |ui| {
                neu::vslider(
                    ui,
                    theme,
                    &mut s.vslider,
                    &SliderSpec {
                        label: "A",
                        min: 0.0,
                        max: 100.0,
                        default: 12.0,
                        fmt: fmt_pct,
                    },
                );
            });
            ui.vertical(|ui| {
                captioned(ui, theme, "h-slider", |ui| {
                    neu::hslider(
                        ui,
                        theme,
                        &mut s.hslider,
                        &SliderSpec {
                            label: "Vel",
                            min: 0.0,
                            max: 100.0,
                            default: 80.0,
                            fmt: fmt_pct,
                        },
                    );
                });
                captioned(ui, theme, "h-meter", |ui| {
                    neu::meter_horizontal(ui, theme, (level, level * 0.9), neu::METER_HORIZONTAL);
                });
                captioned(ui, theme, "mono", |ui| {
                    neu::meter(ui, theme, level, vec2(5.0, 40.0));
                });
            });
        });
    });

    card(ui, theme, "Step cells · note bars · ghost", |ui| {
        ui.horizontal(|ui| {
            let specs: Vec<StepCellSpec<'static>> = s
                .steps
                .iter()
                .enumerate()
                .map(|(i, &v)| StepCellSpec {
                    on: v > 0,
                    accent: v == 2,
                    current: i == 6,
                    selected: false,
                    label: None,
                })
                .collect();
            let out = neu::step_row(ui, theme, "Kick", &specs);
            if let Some(i) = out.clicked {
                s.steps[i] = (s.steps[i] + 1) % 3;
            }
        });
        ui.horizontal(|ui| {
            let specs: Vec<StepCellSpec<'static>> = (0..16)
                .map(|i| StepCellSpec {
                    on: i % 4 == 0,
                    accent: false,
                    current: i == 6,
                    selected: i == 8,
                    label: Some(if i % 4 == 0 { "C3" } else { "–" }),
                })
                .collect();
            neu::step_row_coloured(ui, theme, "Keys", &specs, Some(theme.track_colour(4)));
        });
        neu::beat_numbers(ui, theme, 16);
        neu::playhead_row(ui, theme, 16, Some(6));
        ui.horizontal(|ui| {
            caption(
                ui,
                theme,
                "off · hover · on · accent · playhead · note cells · ruler",
            );
        });
        let ghosts = vec![
            GhostNote {
                start: 0,
                length: 5,
                pitch: 50,
                colour: theme.track_colour(6),
            },
            GhostNote {
                start: 6,
                length: 2,
                pitch: 48,
                colour: theme.track_colour(3),
            },
        ];
        let out = neu::piano_roll_coloured(
            ui,
            theme,
            &mut s.roll,
            &s.notes,
            &ghosts,
            theme.track_colour(4),
        );
        if let Some(n) = out.add {
            s.notes.push(n);
        }
        if let Some((i, n)) = out.moved {
            s.notes[i] = n;
        }
        for (i, n) in out.moved_many {
            s.notes[i] = n;
        }
        if let Some(i) = out.removed {
            s.notes.remove(i);
        }
        let mut gone = out.removed_many;
        gone.sort_unstable_by(|a, b| b.cmp(a));
        for i in gone {
            s.notes.remove(i);
        }
        caption(
            ui,
            theme,
            "vel 100 · 50 · selected / ghosts 35 % / playhead / velocity lane",
        );
    });

    card(ui, theme, "Tabs · list rows · tooltip · hint bar", |ui| {
        neu::tab_strip(
            ui,
            theme,
            &mut s.tab,
            &[
                ("Piano roll", Some("F5")),
                ("Mixer", Some("F6")),
                ("Synth & FX", Some("F7")),
                ("Song", Some("F8")),
                ("Browser", Some("F9")),
            ],
        );
        for (i, (name, tag, modified)) in [
            ("Tape Wurli", Some(TagKind::Vinyl), false),
            ("Velvet EP", None, false),
            ("Dusty Rhodes", None, true),
        ]
        .into_iter()
        .enumerate()
        {
            let row = neu::list_row_named(ui, theme, s.picked_row == i, name, |ui| {
                neu::body(ui, theme, name);
                if let Some(kind) = tag {
                    neu::tag(ui, theme, "vinyl", kind);
                }
                if modified {
                    neu::led_named(ui, theme, true, LedKind::Accent, "Modified");
                    neu::text::text_widget(ui, "MODIFIED", TextKind::Small, theme.text_3);
                }
            });
            if row.clicked() {
                s.picked_row = i;
            }
        }
        ui.horizontal(|ui| {
            let play = neu::icon_button(ui, theme, Icon::Play, 28.0);
            neu::tooltip_with_keycap(play, theme, "Play / Stop", "Space");
            let link = neu::small_button(ui, theme, "Link");
            neu::tooltip_with_keycap(link, theme, "Link to new insert", "Ctrl+L");
            caption(ui, theme, "hover for the tooltips");
        });
        neu::hint_bar(
            ui,
            theme,
            &[
                ("Space", "Play / Stop"),
                ("Ctrl+L", "Link"),
                ("F5", "Piano roll"),
            ],
        );
    });

    card(ui, theme, "Menu", |ui| {
        ui.horizontal_top(|ui| {
            let items = [
                MenuItem::keyed("New", "Ctrl+N"),
                MenuItem::keyed("Open…", "Ctrl+O"),
                MenuItem::keyed("Save", "Ctrl+S"),
                MenuItem::keyed("Save as…", "Ctrl+Shift+S"),
                MenuItem::Separator,
                MenuItem::keyed("Export WAV…", "Ctrl+E"),
                MenuItem::Separator,
                MenuItem::disabled("Redo", Some("Ctrl+Y")),
            ];
            if let Some(i) = neu::menu(ui, theme, &items) {
                s.menu_pick = Some(i);
            }
            ui.vertical(|ui| {
                let anchor = neu::icon_button_kind(
                    ui,
                    theme,
                    Icon::Menu,
                    vec2(28.0, 28.0),
                    ButtonKind::Ghost,
                );
                if let Some(i) = neu::menu_popup(ui, theme, &anchor, &items) {
                    s.menu_pick = Some(i);
                }
                caption(ui, theme, "click for the floating menu");
                if let Some(i) = s.menu_pick {
                    neu::value(ui, theme, &format!("picked {i}"));
                }
            });
        });
    });

    card(
        ui,
        theme,
        "Scrollbars · window header · drawer handle",
        |ui| {
            neu::window_frame_sized(
                ui,
                theme,
                vec2(ui.available_width(), 160.0),
                "Piano roll",
                Some("Keys · Dusty Rhodes"),
                |ui| {
                    neu::header_button(ui, theme, Icon::Clear);
                    neu::header_button(ui, theme, Icon::Minus);
                    neu::keycap(ui, theme, "F5");
                },
                |ui| {
                    neu::toolbar(ui, theme, |ui| {
                        neu::segmented_small(ui, theme, &mut s.wave, &["Draw", "Select"]);
                        neu::small_button(ui, theme, "Ghosts");
                    });
                    neu::scroll_both("gallery_scroll").show(ui, |ui| {
                    for i in 0..12 {
                        neu::value(
                            ui,
                            theme,
                            &format!("Row {i} · 10 px tracks, 6 px thumbs, always drawn, a long line to scroll sideways"),
                        );
                    }
                });
                },
            );
            neu::drawer_handle(ui, theme);
            neu::window_frame_sized(
                ui,
                theme,
                vec2(ui.available_width(), neu::WINDOW_HEADER),
                "Keyboard",
                Some("collapsed · oct 4 · vel 80 %"),
                |ui| {
                    neu::header_button(ui, theme, Icon::ChevronDown);
                },
                |_| {},
            );
        },
    );

    card(ui, theme, "Swatches · tokens", |ui| {
        ui.horizontal_wrapped(|ui| {
            for (name, c, fg) in [
                ("bg", theme.bg, theme.text),
                ("panel", theme.panel, theme.text),
                ("panel-2", theme.panel_2, theme.text),
                ("panel-3", theme.panel_3, theme.text),
                ("panel-4", theme.panel_4, theme.text),
                ("border", theme.border, theme.text),
                ("border-2", theme.border_2, theme.text),
                ("accent", theme.accent, theme.on_accent),
                ("hover", theme.accent_hover, theme.on_accent),
                ("press", theme.accent_press, theme.on_accent),
                ("rec", theme.rec, theme.on_track),
                ("ok", theme.ok, theme.on_track),
                ("grid", theme.grid_beat, theme.text),
                ("bar", theme.grid_bar, theme.text),
            ] {
                swatch(ui, theme, name, c, fg);
            }
        });
        ui.horizontal_wrapped(|ui| {
            for i in 0..8u8 {
                swatch(
                    ui,
                    theme,
                    &format!("t{}", i + 1),
                    theme.track_colour(i),
                    theme.on_track,
                );
            }
            for (name, c) in [
                ("text", theme.text),
                ("text-2", theme.text_2),
                ("text-3", theme.text_3),
            ] {
                swatch(ui, theme, name, theme.panel_2, c);
            }
        });
    });

    card(ui, theme, "Type scale", |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        neu::title(ui, theme, "Title 12 / 600 caps +0.06em");
        neu::label(ui, theme, "Label 12 / 500 caps · text-2");
        neu::button(ui, theme, "Button 12 / 600");
        neu::value(ui, theme, "Value 13 / 400 tabular · 2.4 kHz · 120 BPM");
        neu::body(ui, theme, "Body 13 / 400 · menus, browser rows");
        neu::display(ui, theme, "128");
        neu::small(ui, theme, "Small 11 / 500 · tags, insert numbers");
        neu::hint(ui, theme, "Hint 10 / 600 caps · keycaps");
        neu::section(ui, theme, "Section 11 / 600 caps +0.06em");
        neu::micro(ui, theme, "Micro 10 / 600 · ruler numbers, key names");
        neu::brand(ui, theme, "Project name 13 / 600");
    });

    card(
        ui,
        theme,
        "Keyboard keys · mixer strip · browser chips",
        |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let model = neu::PianoModel {
                    base_note: 60,
                    white_keys: 7,
                    held: &s.held,
                    labels: &s.labels,
                };
                let out = neu::piano_keys(ui, theme, &model);
                if let Some(n) = out.pressed {
                    s.held[usize::from(n)] = true;
                }
                if let Some(n) = out.released {
                    s.held[usize::from(n)] = false;
                }
                // A mixer insert strip: 54 wide, head, colour bar, slot pills, fader + meter + GR,
                // M / S, route.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    ui.set_width(54.0);
                    neu::small(ui, theme, "1 · Keys");
                    neu::chrome::paint_colour_bar(
                        ui.painter(),
                        egui::Rect::from_min_size(ui.cursor().min, vec2(44.0, 3.0)),
                        theme.track_colour(4),
                        theme,
                    );
                    ui.add_space(4.0);
                    for (i, name) in ["EQ", "REV", "OFF"].iter().enumerate() {
                        if neu::button_wide(
                            ui,
                            theme,
                            name,
                            ButtonKind::Toggle(s.strip_on[i]),
                            ButtonSize::Small,
                            46.0,
                        )
                        .clicked()
                        {
                            s.strip_on[i] = !s.strip_on[i];
                        }
                    }
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        neu::fader_sized(
                            ui,
                            theme,
                            &mut s.fader,
                            &SliderSpec {
                                label: "Keys level",
                                min: -60.0,
                                max: 6.0,
                                default: 0.0,
                                fmt: fmt_db,
                            },
                            120.0,
                            false,
                        );
                        ui.add_space(20.0);
                        neu::stereo_meter(ui, theme, (0.7, 0.6), neu::STEREO_METER_SIZE);
                        neu::gr_meter(ui, theme, 0.1, 120.0);
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        neu::toggle_button_sized(ui, theme, "M", false, vec2(22.0, 22.0));
                        neu::toggle_button_sized(ui, theme, "S", false, vec2(22.0, 22.0));
                    });
                    neu::small(ui, theme, "-> M");
                });
                // Browser chips: 22 h toggle buttons wrapping.
                ui.vertical(|ui| {
                    ui.set_max_width(300.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for (i, name) in [
                            "All",
                            "Lofi Keys",
                            "Lofi Guitars",
                            "Lofi Pads",
                            "EDM Leads",
                            "EDM Bass",
                            "EDM Plucks",
                            "Risers & FX",
                        ]
                        .iter()
                        .enumerate()
                        {
                            if neu::button_kind(
                                ui,
                                theme,
                                name,
                                ButtonKind::Toggle(s.chip_on == i),
                                ButtonSize::Small,
                            )
                            .clicked()
                            {
                                s.chip_on = i;
                            }
                        }
                    });
                    let (inner, _) = neu::well(ui, theme, vec2(300.0, 60.0));
                    let samples: Vec<f32> = (0..256)
                        .map(|i| {
                            let x = i as f32 / 256.0;
                            0.6 * (x * 12.0 + playing_time as f32 * 4.0).sin()
                        })
                        .collect();
                    let input = VisualInput {
                        samples: &samples,
                        samples_r: None,
                        spectrum: &[0.0; 64],
                        peak: (0.6, 0.6),
                        hits: 0,
                        playhead: 0.0,
                        bpm: 120.0,
                        time: playing_time,
                    };
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                    neu::scope(&mut child, theme, &input, &VisualStyle::DEFAULT);
                });
            });
        },
    );
}

/// A 56 x 40 token swatch with its name.
fn swatch(ui: &mut egui::Ui, theme: &Theme, name: &str, fill: egui::Color32, fg: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(56.0, 40.0), egui::Sense::hover());
    neu::paint_control(ui.painter(), rect, theme.radius_small, fill, theme.border_2);
    neu::text::paint_text(
        ui.painter(),
        rect.center(),
        egui::Align2::CENTER_CENTER,
        name,
        TextKind::Micro,
        fg,
    );
}

impl eframe::App for Gallery {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        let mode = self.primary_mode();
        if self.applied != Some(mode) {
            Theme::for_mode(mode).apply(ctx);
            self.applied = Some(mode);
        }
        let theme = Theme::for_mode(mode);
        if let Some(limit) = self.exit_after {
            if self.started.elapsed() > limit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        if self.sheet.playing {
            ctx.request_repaint();
        }
        let time = ctx.input(|i| i.time);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme.bg).inner_margin(4.0))
            .show(root, |ui| {
                neu::app_toolbar(ui, &theme, |ui| {
                    neu::brand(ui, &theme, "neu gallery · Deck");
                    neu::toolbar_separator(ui, &theme);
                    let mut v = match self.view {
                        View::Dark => 0,
                        View::Light => 1,
                        View::Both => 2,
                    };
                    if neu::segmented(ui, &theme, &mut v, &["Dark", "Light", "Both"]).changed() {
                        self.view = [View::Dark, View::Light, View::Both][v];
                    }
                    neu::toolbar_separator(ui, &theme);
                    neu::hint_bar(ui, &theme, &[("Space", "Play"), ("F5", "Roll")]);
                });
                ui.add_space(4.0);
                let mut scroll = neu::scroll_vertical("gallery_page");
                if let Some(offset) = self.scroll_to {
                    scroll = scroll.vertical_scroll_offset(offset);
                    if self.started.elapsed() > Duration::from_millis(300) {
                        self.scroll_to = None;
                    }
                    ctx.request_repaint();
                }
                scroll.show(ui, |ui| {
                    let playing_time = if self.sheet.playing { time } else { 0.0 };
                    match self.view {
                        View::Dark | View::Light => {
                            ui.set_max_width(ui.available_width() - 4.0);
                            sheet(ui, &theme, &mut self.sheet, playing_time);
                        }
                        View::Both => {
                            let width = (ui.available_width() - 12.0) * 0.5;
                            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.allocate_ui_with_layout(
                                    vec2(width, ui.available_height()),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.set_max_width(width);
                                        sheet(ui, &Theme::dark(), &mut self.sheet, playing_time);
                                    },
                                );
                                let light = Theme::light();
                                let rect = egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    vec2(width, ui.available_height().max(100.0)),
                                );
                                ui.painter().rect_filled(rect, 0.0, light.bg);
                                ui.allocate_ui_with_layout(
                                    vec2(width, ui.available_height()),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.set_max_width(width);
                                        sheet(ui, &light, &mut self.sheet, playing_time);
                                    },
                                );
                            });
                        }
                    }
                });
            });
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut exit_after = None;
    let mut view = View::Both;
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
            "--light" => view = View::Light,
            "--dark" => view = View::Dark,
            "--both" => view = View::Both,
            _ => {}
        }
        i += 1;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("neu gallery")
            .with_inner_size(Vec2::new(1280.0, 860.0))
            .with_min_inner_size(Vec2::new(840.0, 600.0)),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "neu gallery",
        options,
        Box::new(move |_cc| Ok(Box::new(Gallery::new(view, exit_after, scroll_to)))),
    )
}
