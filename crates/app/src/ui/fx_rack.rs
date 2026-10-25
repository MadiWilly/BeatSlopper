//! FX rack (M2): drive, delay, reverb and the master stage.
//!
//! Layout follows `design/preview-a.html` `<section class="panel fx">`: a panel head ("FX"
//! left, the `FX | Mixer` tab right, M6: it swaps this slot for the MIXER panel) over
//! `.fx-body`, two columns pushed apart:
//!
//! - left: DRIVE (AMOUNT, MIX knobs), DELAY (head row with the time `segmented`
//!   1/16 | 1/8 | 1/4 | 1/2, then FEEDBACK and MIX knobs);
//! - right: REVERB (SIZE, DAMPING, MIX knobs), MASTER (GAIN knob in dB, beside it the LIMIT
//!   `toggle_button` and the ACTIVE `lamp`, which for now lights whenever the limiter is on;
//!   the real gain-reduction light arrives with the engine's limiter meter).
//!
//! The rack is [`WIDTH`] wide (`.row2{grid-template-columns:1fr 460px}` plus the shadow room
//! the egui panel needs) and fills the height it is given, like the SYNTH panel beside it.

use egui::Ui;
use engine::ParamId;
use neu::Theme;

use crate::app::AppState;
use crate::ui::controls::{self, GROUP_GAP, KNOB_GAP, Knob, PANEL_HEAD_GAP, fmt_db, fmt_percent};

/// Width of the rack including its panel padding.
pub const WIDTH: f32 = 476.0;
/// Delay division names in `DelayTime` index order.
pub const DELAY_DIVISIONS: [&str; 4] = ["1/16", "1/8", "1/4", "1/2"];

/// Gap between sections in a column (`.fx-col{gap:16px}`).
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

fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = COLUMN_GAP;
        column(ui, |ui| {
            drive_section(ui, state, theme);
            delay_section(ui, state, theme);
        });
        controls::right_aligned(ui, "fx_right_column", |ui| {
            column(ui, |ui| {
                reverb_section(ui, state, theme);
                master_section(ui, state, theme);
            });
        });
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
        |ui, _| {
            controls::group_label(ui, theme, "Drive");
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

fn reverb_section(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    section(
        ui,
        state,
        |ui, _| {
            controls::group_label(ui, theme, "Reverb");
        },
        |ui, state| {
            Knob::new(ParamId::ReverbSize, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::ReverbDamping, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::ReverbMix, fmt_percent).show(ui, state, theme);
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
