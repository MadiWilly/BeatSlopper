//! FX rack (M2, extended in M8): drive, delay, reverb, the master stage, lo-fi, chorus and
//! the sidechain pump.
//!
//! Layout follows `design/preview-a.html` `<section class="panel fx">`: a panel head ("FX"
//! left, the `FX | Mixer` tab right, M6: it swaps this slot for the MIXER panel) over
//! `.fx-body`. The body is a vertical `ScrollArea`; the M2 rows always fit the top row of the
//! page unscrolled, the M8 rows scroll into view:
//!
//! M10: DRIVE, LO-FI, CHORUS and SIDECHAIN are per track (the `ParamId::is_synth_insert`
//! ids): their heads carry the edit track's colour chip and name and their knobs edit that
//! track's insert; DELAY, REVERB and MASTER are the shared buses.
//!
//! 1. two columns pushed apart: left DRIVE (AMOUNT, MIX knobs), DELAY (head row with the time
//!    `segmented` 1/16 | 1/8 | 1/4 | 1/2, then FEEDBACK and MIX knobs); right REVERB (SIZE,
//!    DAMPING, MIX knobs), MASTER (GAIN knob in dB, beside it the LIMIT `toggle_button` and
//!    the ACTIVE `lamp`, which for now lights whenever the limiter is on; the real
//!    gain-reduction light arrives with the engine's limiter meter);
//! 2. LO-FI (BITS "12 bit", RATE "x8", WOBBLE %, VINYL %, TONE Hz log, MIX %), full width;
//! 3. two columns pushed apart: CHORUS (RATE Hz, DEPTH %, MIX %) and SIDECHAIN (PUMP %, PUMP
//!    REL ms: the synth channel ducks on every kick hit).
//!
//! The rack is [`WIDTH`] wide (`.row2{grid-template-columns:1fr 460px}` plus the shadow room
//! the egui panel needs) and fills the height it is given, like the SYNTH panel beside it.

use egui::{ScrollArea, Ui};
use engine::ParamId;
use neu::Theme;

use crate::app::AppState;
use crate::plugins::PluginKind;
use crate::ui::controls::{
    self, GROUP_GAP, HEAD_GAP, KNOB_GAP, Knob, PANEL_HEAD_GAP, ROW_HEIGHT, fmt_bits, fmt_db,
    fmt_hz, fmt_ms, fmt_percent, fmt_rate_hz, fmt_rate_x,
};

/// Width of the rack including its panel padding.
pub const WIDTH: f32 = 476.0;
/// Label of the REVERB group's pill that opens the REVERB window (not "Open": the PROJECT
/// panel's file button owns that accessible name).
pub const REVERB_MORE_LABEL: &str = "More";
/// Tooltip of [`REVERB_MORE_LABEL`].
pub const REVERB_MORE_TOOLTIP: &str = "Open the REVERB window: pre-delay, width, low / high \
                                       cut and presets. Size, damping and mix are these knobs.";
/// Tooltip of the REVERB group label: sends go in, MIX is the return level.
pub const REVERB_TOOLTIP: &str = "Reverb bus. The mixer's REV sends set how much of each \
                                  channel goes in; MIX is the return level.";
/// Second tooltip line of the rack's reverb MIX knob (the window's MIX says the same).
pub const REVERB_MIX_NOTE: &str = "Return level of the reverb bus (the mixer's REV sends feed it)";
/// Delay division names in `DelayTime` index order.
pub const DELAY_DIVISIONS: [&str; 4] = ["1/16", "1/8", "1/4", "1/2"];

/// Gap between sections in a column and between the body's rows (`.fx-col{gap:16px}`).
const SECTION_GAP: f32 = 16.0;
/// Smallest gap between the two columns (`.fx-body{column-gap:18px}`); they spread further
/// apart when the rack is wider than its contents.
const COLUMN_GAP: f32 = 18.0;
/// Gap between the limiter toggle and the lamp (`.master-side{gap:8px}`).
const SIDE_GAP: f32 = 8.0;
/// Pushes the limiter toggle and lamp down beside the knob (`.master-side{padding-top:18px}`).
const SIDE_PAD_TOP: f32 = 18.0;

/// Renders the FX rack into `ui`, filling the space it is given, and applies any control
/// change to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        controls::right_panel_head(ui, state, theme, "FX");
        ui.add_space(PANEL_HEAD_GAP);
        body(ui, state, theme);
    });
}

/// The scrolling body: the M2 columns first, then LO-FI, then CHORUS | SIDECHAIN.
fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ScrollArea::vertical()
        .id_salt("fx_body")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = SECTION_GAP;
            two_columns(
                ui,
                state,
                "fx_right_column",
                |ui, state| {
                    column(ui, |ui| {
                        drive_section(ui, state, theme);
                        delay_section(ui, state, theme);
                    });
                },
                |ui, state| {
                    column(ui, |ui| {
                        reverb_section(ui, state, theme);
                        master_section(ui, state, theme);
                    });
                },
            );
            lofi_section(ui, state, theme);
            two_columns(
                ui,
                state,
                "fx_right_column_2",
                |ui, state| chorus_section(ui, state, theme),
                |ui, state| sidechain_section(ui, state, theme),
            );
        });
}

/// Two top-aligned columns, `right` pushed to the rack's right edge (`id_salt` keys its
/// measured width, see [`controls::right_aligned`]).
fn two_columns(
    ui: &mut Ui,
    state: &mut AppState,
    id_salt: &str,
    left: impl FnOnce(&mut Ui, &mut AppState),
    right: impl FnOnce(&mut Ui, &mut AppState),
) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = COLUMN_GAP;
        left(ui, state);
        controls::right_aligned(ui, id_salt, |ui| right(ui, state));
    });
}

/// A top-down column of sections (`.fx-col`).
fn column<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SECTION_GAP;
        add(ui)
    })
    .inner
}

/// The head of a per-track group (M10: DRIVE, LO-FI, CHORUS, SIDECHAIN, the
/// [`ParamId::is_synth_insert`] ids): the caps label, then the edit track's chip and name
/// ([`controls::track_tag`]), because these knobs edit that track's insert, not a shared bus.
fn track_group_head(ui: &mut Ui, state: &AppState, theme: &Theme, text: &str) {
    controls::group_head(ui, theme, text, |ui| {
        controls::track_tag(ui, state, theme);
    });
}

/// A section: head, then its controls row (`.fx-sec{gap:10px}`, `.fx-ctls{gap:10px}`).
fn section<R>(
    ui: &mut Ui,
    state: &mut AppState,
    head: impl FnOnce(&mut Ui, &mut AppState),
    controls: impl FnOnce(&mut Ui, &mut AppState) -> R,
) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = GROUP_GAP;
        head(ui, state);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = KNOB_GAP;
            controls(ui, state)
        })
        .inner
    })
    .inner
}

fn drive_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, state| {
            track_group_head(ui, state, theme, "Drive");
        },
        |ui, state| {
            Knob::new(ParamId::DriveAmount, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::DriveMix, fmt_percent).show(ui, state, theme);
        },
    );
}

fn delay_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, state| {
            controls::group_head(ui, theme, "Delay", |ui| {
                controls::segmented(ui, state, theme, ParamId::DelayTime, &DELAY_DIVISIONS);
            });
        },
        |ui, state| {
            Knob::new(ParamId::DelayFeedback, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::DelayMix, fmt_percent).show(ui, state, theme);
        },
    );
}

/// The REVERB group's head: the label (its tooltip tells the return level from the mixer's
/// sends) and the [`REVERB_MORE_LABEL`] pill that opens the REVERB window, lit while it is
/// open. The three knobs here and the window edit one `ReverbState`.
fn reverb_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, state| {
            ui.horizontal(|ui| {
                ui.set_min_height(ROW_HEIGHT);
                ui.spacing_mut().item_spacing.x = HEAD_GAP;
                neu::label(ui, theme, "Reverb").on_hover_text(REVERB_TOOLTIP);
                let open = state.is_plugin_open(PluginKind::Reverb);
                if neu::toggle_button(ui, theme, REVERB_MORE_LABEL, open)
                    .on_hover_text(REVERB_MORE_TOOLTIP)
                    .clicked()
                {
                    state.toggle_plugin(PluginKind::Reverb);
                }
            });
        },
        |ui, state| {
            Knob::new(ParamId::ReverbSize, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::ReverbDamping, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::ReverbMix, fmt_percent)
                .note(REVERB_MIX_NOTE)
                .show(ui, state, theme);
        },
    );
}

fn master_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, _| {
            controls::group_label(ui, theme, "Master");
        },
        |ui, state| {
            Knob::new(ParamId::MasterGain, fmt_db)
                .label("Gain")
                .show(ui, state, theme);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = SIDE_GAP;
                ui.add_space(SIDE_PAD_TOP);
                controls::toggle(ui, state, theme, ParamId::LimiterOn, "Limit");
                neu::slider::lamp(ui, theme, state.limiter_on(), "Active");
            });
        },
    );
}

fn lofi_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, state| {
            track_group_head(ui, state, theme, "Lo-Fi");
        },
        |ui, state| {
            Knob::new(ParamId::LofiBits, fmt_bits).show(ui, state, theme);
            Knob::new(ParamId::LofiRate, fmt_rate_x).show(ui, state, theme);
            Knob::new(ParamId::LofiWobble, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::LofiNoise, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::LofiTone, fmt_hz)
                .log()
                .show(ui, state, theme);
            Knob::new(ParamId::LofiMix, fmt_percent).show(ui, state, theme);
        },
    );
}

fn chorus_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, state| {
            track_group_head(ui, state, theme, "Chorus");
        },
        |ui, state| {
            Knob::new(ParamId::ChorusRate, fmt_rate_hz).show(ui, state, theme);
            Knob::new(ParamId::ChorusDepth, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::ChorusMix, fmt_percent).show(ui, state, theme);
        },
    );
}

fn sidechain_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, state| {
            track_group_head(ui, state, theme, "Sidechain");
        },
        |ui, state| {
            Knob::new(ParamId::SidechainAmount, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::SidechainRelease, fmt_ms).show(ui, state, theme);
        },
    );
}
