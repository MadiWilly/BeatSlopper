//! SYNTH panel (M2): preset selector, oscillators, filter, amp envelope, LFO and glide.
//!
//! Layout follows `design/preview-a.html` `<section class="panel synth">`: a panel head with
//! the title on the left and the preset `segmented` on the right, then `.synth-body`, a row of
//! top-aligned columns:
//!
//! - OSC: two groups (`OSC 1`, `OSC 2`), each a head row (label + wave `segmented`) over a
//!   knob row (DETUNE bipolar in cents, MIX in %).
//! - FILTER: label, CUTOFF `dial` (small, log), knob row (RES %, ENV bipolar %).
//! - AMP: label, four `vslider`s (A D S R; ms / %).
//! - LFO: label, knob row (RATE Hz, DEPTH %), target `segmented` (Pitch | Cutoff), then the
//!   GLIDE knob (ms), headed by its own caps label: the one M2 control the mock has no slot
//!   for, and the LFO column is the shortest.
//!
//! The column gap is the mock's 26 pt when the panel is wide enough and shrinks (down to 8)
//! using the columns' total width from the previous frame, so the panel never spills over the
//! FX rack at the minimum window width.

use egui::{Align, Layout, Ui, vec2};
use engine::ParamId;
use neu::{DialSize, Theme};

use crate::app::AppState;
use crate::presets;
use crate::ui::controls::{
    self, GROUP_GAP, KNOB_BLOCK, KNOB_GAP, Knob, PANEL_HEAD_GAP, fmt_cents, fmt_hz, fmt_ms,
    fmt_percent, fmt_percent_signed, fmt_rate_hz,
};

/// Waveform names in `Osc1Wave` / `Osc2Wave` index order.
pub const WAVE_NAMES: [&str; 4] = ["Sine", "Saw", "Square", "Tri"];
/// LFO target names in `LfoTarget` index order.
pub const LFO_TARGETS: [&str; 2] = ["Pitch", "Cutoff"];

/// Largest gap between the synth columns (`.synth-body{gap:26px}`).
pub const COL_GAP_MAX: f32 = 26.0;
/// Smallest gap the columns compress to when the panel is narrow.
pub const COL_GAP_MIN: f32 = 8.0;
/// Gap between the A D S R sliders (`.sliders{gap:8px}`).
const SLIDER_GAP: f32 = 8.0;
/// Number of columns in the body (OSC, FILTER, AMP, LFO).
const COLUMNS: usize = 4;
/// Width of a two-knob row: the filter column's width, used to centre the cutoff dial.
const TWO_KNOBS: f32 = KNOB_BLOCK * 2.0 + KNOB_GAP;

/// Renders the SYNTH panel into `ui`, filling the space it is given, and applies any control
/// change to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        head(ui, state, theme);
        ui.add_space(PANEL_HEAD_GAP);
        body(ui, state, theme);
    });
}

/// Names of every preset, in selector order.
pub fn preset_names() -> Vec<&'static str> {
    presets::ALL.iter().map(|preset| preset.name).collect()
}

fn head(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.set_min_height(controls::ROW_HEIGHT);
        neu::title(ui, theme, "Synth");
        controls::right_aligned(ui, "synth_preset", |ui| preset_selector(ui, state, theme));
    });
}

/// The preset `segmented`: a change applies the preset (defaults + overrides, all sent).
fn preset_selector(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let names = preset_names();
    let before = state.preset_index.min(names.len().saturating_sub(1));
    let mut selected = before;
    neu::segmented::segmented(ui, theme, &mut selected, &names);
    if selected != before {
        state.apply_preset(selected);
    }
}

fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let width_id = ui.scope_id().with("synth_columns_width");
    let columns_width: Option<f32> = ui.data(|d| d.get_temp(width_id));
    let gap = columns_width.map_or(COL_GAP_MAX, |w| {
        ((ui.available_width() - w) / (COLUMNS - 1) as f32).clamp(COL_GAP_MIN, COL_GAP_MAX)
    });

    let row = ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        osc_column(ui, state, theme);
        filter_column(ui, state, theme);
        amp_column(ui, state, theme);
        lfo_column(ui, state, theme);
    });
    let total = row.response.rect.width();
    ui.data_mut(|d| d.insert_temp(width_id, total - gap * (COLUMNS - 1) as f32));
}

/// A top-down column with the mock's 10 pt group gap.
fn column<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = GROUP_GAP;
        add(ui)
    })
    .inner
}

/// A row of knobs with the mock's 10 pt knob gap.
fn knob_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = KNOB_GAP;
        add(ui)
    })
    .inner
}

fn osc_column(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        osc_group(
            ui,
            state,
            theme,
            "Osc 1",
            ParamId::Osc1Wave,
            ParamId::Osc1Detune,
            ParamId::Osc1Mix,
        );
        ui.add_space(GROUP_GAP); // `.osc .group + .group{margin-top:10px}` on top of the gap
        osc_group(
            ui,
            state,
            theme,
            "Osc 2",
            ParamId::Osc2Wave,
            ParamId::Osc2Detune,
            ParamId::Osc2Mix,
        );
    });
}

fn osc_group(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    name: &str,
    wave: ParamId,
    detune: ParamId,
    mix: ParamId,
) {
    controls::group_head(ui, theme, name, |ui| {
        controls::segmented(ui, state, theme, wave, &WAVE_NAMES);
    });
    knob_row(ui, |ui| {
        Knob::new(detune, fmt_cents)
            .bipolar()
            .show(ui, state, theme);
        Knob::new(mix, fmt_percent).show(ui, state, theme);
    });
}

fn filter_column(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_label(ui, theme, "Filter");
        // Centre the dial over the two knobs below it.
        ui.allocate_ui_with_layout(
            vec2(TWO_KNOBS, 0.0),
            Layout::top_down(Align::Center),
            |ui| {
                Knob::new(ParamId::FilterCutoff, fmt_hz).log().show_dial(
                    ui,
                    state,
                    theme,
                    DialSize::Sm,
                );
            },
        );
        knob_row(ui, |ui| {
            Knob::new(ParamId::FilterResonance, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::FilterEnvAmount, fmt_percent_signed)
                .bipolar()
                .show(ui, state, theme);
        });
    });
}

fn amp_column(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_label(ui, theme, "Amp");
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = SLIDER_GAP;
            controls::vslider(ui, state, theme, ParamId::AmpAttack, fmt_ms);
            controls::vslider(ui, state, theme, ParamId::AmpDecay, fmt_ms);
            controls::vslider(ui, state, theme, ParamId::AmpSustain, fmt_percent);
            controls::vslider(ui, state, theme, ParamId::AmpRelease, fmt_ms);
        });
    });
}

fn lfo_column(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_label(ui, theme, "LFO");
        knob_row(ui, |ui| {
            Knob::new(ParamId::LfoRate, fmt_rate_hz).show(ui, state, theme);
            Knob::new(ParamId::LfoDepth, fmt_percent).show(ui, state, theme);
        });
        controls::segmented(ui, state, theme, ParamId::LfoTarget, &LFO_TARGETS);
        // The knob's own caps label ("GLIDE") heads this mini group; a second group gap
        // separates it from the LFO target like `.osc .group + .group`.
        ui.add_space(GROUP_GAP);
        Knob::new(ParamId::Glide, fmt_ms).show(ui, state, theme);
    });
}
