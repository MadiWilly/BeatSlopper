//! SYNTH panel (M2, extended in M8): preset selector, oscillators, filter, amp envelope, LFO,
//! glide, and the M8 synthesis extensions (unison, FM / sub / noise, pluck mode, filter type
//! and envelope, pitch envelope, velocity routing).
//!
//! A panel head with the title (`Synth · <track>`) and the edit track's chip, then the body
//! (M12: the preset strip moved to the SYNTH & FX window's toolbar, [`preset_strip`], and the
//! preset browser to the BROWSER column; the panel is rehosted in that window until its M13
//! restyle). The body is a vertical `ScrollArea` of three rows of top-aligned columns; the
//! first row is the M2 mock, the M8 rows scroll into view:
//!
//! 1. OSC: two groups (`OSC 1`, `OSC 2`), each a head row (label + wave `segmented`) over a
//!    knob row (DETUNE bipolar in cents, MIX in %, PULSE width in %).
//!    FILTER: head row (label + TYPE `segmented` LP | BP | HP), CUTOFF `dial` (small, log),
//!    knob row (RES %, ENV bipolar %).
//!    AMP: label, four `vslider`s (A D S R; ms / %).
//!    LFO: label, knob row (RATE Hz, DEPTH %), target `segmented` (Pitch | Cutoff), then the
//!    GLIDE knob (ms), headed by its own caps label.
//! 2. UNISON: head row (label + voices `segmented` 1..7) over the SPREAD knob (cents).
//!    MOD: head row (label + noise COLOUR `segmented` White | Pink) over FM AMT (%), FM RATIO
//!    ("2.0x"), SUB (%) and NOISE (%) knobs.
//!    MODE: head row (label + `segmented` Synth | Pluck) over the pluck DAMP and BRIGHT knobs
//!    (always shown; they only matter in pluck mode).
//! 3. FILTER ENV: label, four `vslider`s (F.ATT F.DEC F.SUS F.REL) beside the KEY TRK knob (%).
//!    PITCH ENV: label, P.ENV (bipolar, semitones) and P.DEC (ms) knobs.
//!    VELOCITY: label, VEL>AMP (%) and VEL>CUT (%) knobs.
//!
//! Each row's column gap is the mock's 26 pt when the panel is wide enough and shrinks (down
//! to 4) using the columns' total width from the previous frame, so the panel never spills
//! over the FX rack at the minimum window width.

use egui::{Align, Layout, ScrollArea, Ui, vec2};
use engine::ParamId;
use neu::{DialSize, TextKind, Theme};

use crate::app::AppState;
use crate::presets;
use crate::ui::controls::{
    self, GROUP_GAP, KNOB_BLOCK, KNOB_GAP, Knob, PANEL_HEAD_GAP, fmt_cents, fmt_hz, fmt_ms,
    fmt_percent, fmt_percent_signed, fmt_rate_hz, fmt_ratio, fmt_semitones,
};
use crate::ui::preset_browser;

/// Waveform names in `Osc1Wave` / `Osc2Wave` index order.
pub const WAVE_NAMES: [&str; 4] = ["Sine", "Saw", "Square", "Tri"];
/// LFO target names in `LfoTarget` index order.
pub const LFO_TARGETS: [&str; 2] = ["Pitch", "Cutoff"];
/// Unison voice counts in `Osc1Unison` order (value `i + 1`; see [`unison_segmented`]).
pub const UNISON_VOICES: [&str; 7] = ["1", "2", "3", "4", "5", "6", "7"];
/// Noise colour names in `NoiseColor` index order.
pub const NOISE_COLOURS: [&str; 2] = ["White", "Pink"];
/// Oscillator mode names in `OscMode` index order.
pub const OSC_MODES: [&str; 2] = ["Synth", "Pluck"];
/// Filter type names in `FilterType` index order.
pub const FILTER_TYPES: [&str; 3] = ["LP", "BP", "HP"];

/// Largest gap between the synth columns (`.synth-body{gap:26px}`).
pub const COL_GAP_MAX: f32 = 26.0;
/// Smallest gap the columns compress to when the panel is narrow (4, the scale's smallest
/// step: at 1280 x 1140 the first row's four columns, 679 pt since the design cleanup's 12 pt
/// A D S R gap, need 6.2 pt gaps to sit inside the panel's 698 pt; an 8 pt minimum put the
/// LFO column 5 pt past the panel and pushed the FX rack with it).
pub const COL_GAP_MIN: f32 = 4.0;
/// Gap between the A D S R sliders: the mock's 8 widened to 12 so the 13 pt values
/// ("600 ms") of neighbouring 36 pt columns never touch.
pub const SLIDER_GAP: f32 = 12.0;
/// Gap between the rows of the body (the group gap, like `.osc .group + .group`).
const ROW_GAP: f32 = GROUP_GAP;
/// A body row that cannot show at least this much (a head row, the group gap and a knob
/// block) before the panel's fold starts at the fold instead, so no head row is ever
/// orphaned from its knobs by the scroll edge (see [`page_break`]).
pub const MIN_ROW_VISIBLE: f32 = controls::ROW_HEIGHT + GROUP_GAP + 92.0;
/// Index of "Square" in [`WAVE_NAMES`]: the only wave the PULSE knobs act on.
const SQUARE: usize = 2;
/// Tooltip note on a dimmed PULSE knob.
pub const PULSE_DIMMED_NOTE: &str = "Square wave only";
/// Width of a two-knob row: the filter column's width, used to centre the cutoff dial.
const TWO_KNOBS: f32 = KNOB_BLOCK * 2.0 + KNOB_GAP;

/// Renders the SYNTH panel into `ui`, filling the space it is given, and applies any control
/// change to `state`. The preset browser lives in the BROWSER column since M12; the SYNTH
/// & FX window's toolbar carries the preset strip ([`preset_strip`]).
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        head(ui, state, theme);
        ui.add_space(PANEL_HEAD_GAP);
        body(ui, state, theme);
    });
}

/// Accessible name of the SYNTH & FX toolbar's MODIFIED LED (the browser's PRESETS section
/// carries the one named [`MODIFIED_LABEL`]).
pub const TOOLBAR_MODIFIED_LABEL: &str = "Preset modified";
/// Accessible name of the toolbar's preset name label.
pub const TOOLBAR_PRESET_NAME: &str = "Toolbar preset";

/// The SYNTH & FX window's toolbar strip (M12): the edit track's chip and name, the preset
/// name (a label named [`TOOLBAR_PRESET_NAME`]; the browser's PRESETS section holds the
/// prev / next buttons and the list) and the MODIFIED LED ([`TOOLBAR_MODIFIED_LABEL`]),
/// then the plugin pills ([`controls::plugin_pills`]).
pub fn preset_strip(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let track = state.edit_track_ui().clone();
    controls::track_chip(ui, theme, track.colour, &track.name);
    neu::text_widget(ui, &track.name, TextKind::Label, theme.text_2);
    neu::toolbar_separator(ui, theme);
    let preset = state.preset_name();
    let name = neu::text_widget(ui, preset, TextKind::Value, theme.text);
    // A button-role node so the name stays the name and the preset is its value (an egui
    // label node keeps its text as the value instead).
    name.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::Role::Button, true, TOOLBAR_PRESET_NAME);
        info.current_text_value = Some(preset.to_owned());
        info
    });
    name.on_hover_text("The preset of the selected track (Ctrl+PageUp / PageDown step it)");
    neu::led_named(
        ui,
        theme,
        state.preset_modified(),
        neu::LedKind::Accent,
        TOOLBAR_MODIFIED_LABEL,
    )
    .on_hover_text("Lit once a knob moved after the preset was applied");
    neu::toolbar_separator(ui, theme);
    controls::plugin_pills(ui, state, theme);
}

/// Names of every preset, in browser order (the unified list of `crate::presets`).
pub fn preset_names() -> Vec<&'static str> {
    presets::all().map(|preset| preset.name).collect()
}

/// The panel title for the edit track: `"Synth · <track name>"` (M10).
#[must_use]
pub fn head_title(state: &AppState) -> String {
    format!("Synth · {}", state.edit_track_ui().name)
}

/// Label of the LED that lights once a per-track parameter moved after the preset was
/// applied (M10; the browser's PRESETS section and the SYNTH & FX toolbar show it).
pub use preset_browser::MODIFIED_LABEL;

/// The head: [`head_title`] with the edit track's colour chip on the left.
fn head(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.set_min_height(controls::ROW_HEIGHT);
        ui.spacing_mut().item_spacing.x = controls::TRACK_CHIP_GAP;
        let track = state.edit_track_ui().clone();
        controls::track_chip(ui, theme, track.colour, &track.name);
        neu::title(ui, theme, &head_title(state))
            .on_hover_text("The selected track; every control here edits it");
    });
}

/// The scrolling body: the M2 row first, then the two M8 rows.
fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ScrollArea::vertical()
        .id_salt("synth_body")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            row(ui, "synth_row_1", 4, |ui| {
                osc_column(ui, state, theme);
                filter_column(ui, state, theme);
                amp_column(ui, state, theme);
                lfo_column(ui, state, theme);
            });
            page_break(ui);
            row(ui, "synth_row_2", 3, |ui| {
                unison_group(ui, state, theme);
                mod_group(ui, state, theme);
                mode_group(ui, state, theme);
            });
            page_break(ui);
            row(ui, "synth_row_3", 3, |ui| {
                filter_env_group(ui, state, theme);
                pitch_env_group(ui, state, theme);
                velocity_group(ui, state, theme);
            });
        });
}

/// Room left under the fold for the glow of a pushed row's selection chips (the on-glow
/// blur reaches about 21 pt past a chip), so not even their halo peeks over the edge.
const PAGE_BREAK_GLOW: f32 = 24.0;

/// Keeps the next body row off the fold: if it would start inside the scroll viewport with
/// less than [`MIN_ROW_VISIBLE`] showing (its head row cut off from its knobs), or just
/// under the edge with its chips' glow reaching in, it is pushed down to start
/// [`PAGE_BREAK_GLOW`] under the viewport's bottom edge, so it scrolls into view whole. The
/// measure is taken from the content's top, so it does not change while scrolling.
fn page_break(ui: &mut Ui) {
    let viewport = ui.clip_rect().height();
    let next_top = ui.cursor().min.y - ui.max_rect().min.y + ui.spacing().item_spacing.y;
    let rest = viewport - next_top;
    if rest > -PAGE_BREAK_GLOW && rest < MIN_ROW_VISIBLE {
        ui.add_space(rest + PAGE_BREAK_GLOW);
    }
}

/// A row of `columns` top-aligned columns whose gap compresses from [`COL_GAP_MAX`] down to
/// [`COL_GAP_MIN`] using the columns' total width from the previous frame (stored under
/// `id_salt`).
fn row(ui: &mut Ui, id_salt: &str, columns: usize, add: impl FnOnce(&mut Ui)) {
    let width_id = ui.scope_id().with(id_salt);
    let columns_width: Option<f32> = ui.data(|d| d.get_temp(width_id));
    let gaps = columns.saturating_sub(1) as f32;
    let gap = columns_width.map_or(COL_GAP_MAX, |w| {
        ((ui.available_width() - w) / gaps.max(1.0)).clamp(COL_GAP_MIN, COL_GAP_MAX)
    });

    let response = ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        add(ui);
    });
    let total = response.response.rect.width();
    ui.data_mut(|d| d.insert_temp(width_id, total - gap * gaps));
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

// ---------------------------------------------------------------------------------------------
// Row 1: the M2 mock (OSC, FILTER, AMP, LFO).

fn osc_column(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        osc_group(
            ui,
            state,
            theme,
            "Osc 1",
            [
                ParamId::Osc1Wave,
                ParamId::Osc1Detune,
                ParamId::Osc1Mix,
                ParamId::Osc1Pulse,
            ],
        );
        ui.add_space(GROUP_GAP); // `.osc .group + .group{margin-top:10px}` on top of the gap
        osc_group(
            ui,
            state,
            theme,
            "Osc 2",
            [
                ParamId::Osc2Wave,
                ParamId::Osc2Detune,
                ParamId::Osc2Mix,
                ParamId::Osc2Pulse,
            ],
        );
    });
}

/// One oscillator group: `[wave, detune, mix, pulse]`. The PULSE knob is dimmed (still
/// live) unless the wave is Square, the only wave it shapes.
fn osc_group(ui: &mut Ui, state: &mut AppState, theme: &Theme, name: &str, ids: [ParamId; 4]) {
    let [wave, detune, mix, pulse] = ids;
    controls::group_head(ui, theme, name, |ui| {
        controls::segmented(ui, state, theme, wave, &WAVE_NAMES);
    });
    let square = state.param(wave).round() as usize == SQUARE;
    knob_row(ui, |ui| {
        Knob::new(detune, fmt_cents)
            .bipolar()
            .show(ui, state, theme);
        Knob::new(mix, fmt_percent).show(ui, state, theme);
        Knob::new(pulse, fmt_percent)
            .dimmed_when(!square, PULSE_DIMMED_NOTE)
            .show(ui, state, theme);
    });
}

fn filter_column(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_head(ui, theme, "Filter", |ui| {
            controls::segmented(ui, state, theme, ParamId::FilterType, &FILTER_TYPES);
        });
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

// ---------------------------------------------------------------------------------------------
// Row 2: UNISON, MOD, MODE (M8).

fn unison_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_head(ui, theme, "Unison", |ui| unison_segmented(ui, state, theme));
        Knob::new(ParamId::Osc1Spread, fmt_cents_unsigned).show(ui, state, theme);
    });
}

/// The unison voice count `segmented`: item `i` is `i + 1` voices (`Osc1Unison` is `1..=7`,
/// not zero-based like the other discrete parameters).
fn unison_segmented(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let before = (state.param(ParamId::Osc1Unison).round() as usize)
        .saturating_sub(1)
        .min(UNISON_VOICES.len() - 1);
    let mut selected = before;
    neu::segmented::segmented(ui, theme, &mut selected, &UNISON_VOICES);
    if selected != before {
        state.edit_param(ParamId::Osc1Unison, (selected + 1) as f32, true);
    }
}

/// Spread in cents: `10.0` -> `"10 c"` (never negative, so no sign).
fn fmt_cents_unsigned(v: f32) -> String {
    format!("{} c", v.round() as i32)
}

fn mod_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_head(ui, theme, "Mod", |ui| {
            controls::segmented(ui, state, theme, ParamId::NoiseColor, &NOISE_COLOURS);
        });
        knob_row(ui, |ui| {
            Knob::new(ParamId::FmAmount, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::FmRatio, fmt_ratio).show(ui, state, theme);
            Knob::new(ParamId::SubLevel, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::NoiseLevel, fmt_percent).show(ui, state, theme);
        });
    });
}

fn mode_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_head(ui, theme, "Mode", |ui| {
            controls::segmented(ui, state, theme, ParamId::OscMode, &OSC_MODES);
        });
        knob_row(ui, |ui| {
            Knob::new(ParamId::PluckDamping, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::PluckBright, fmt_percent).show(ui, state, theme);
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Row 3: FILTER ENV, PITCH ENV, VELOCITY (M8).

fn filter_env_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_label(ui, theme, "Filter Env");
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = SLIDER_GAP;
            controls::vslider(ui, state, theme, ParamId::FilterAttack, fmt_ms);
            controls::vslider(ui, state, theme, ParamId::FilterDecay, fmt_ms);
            controls::vslider(ui, state, theme, ParamId::FilterSustain, fmt_percent);
            controls::vslider(ui, state, theme, ParamId::FilterRelease, fmt_ms);
            ui.add_space(KNOB_GAP - SLIDER_GAP);
            Knob::new(ParamId::FilterKeyTrack, fmt_percent).show(ui, state, theme);
        });
    });
}

fn pitch_env_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_label(ui, theme, "Pitch Env");
        knob_row(ui, |ui| {
            Knob::new(ParamId::PitchEnvAmount, fmt_semitones)
                .label("Amount")
                .bipolar()
                .show(ui, state, theme);
            Knob::new(ParamId::PitchEnvDecay, fmt_ms)
                .label("Decay")
                .show(ui, state, theme);
        });
    });
}

fn velocity_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    column(ui, |ui| {
        controls::group_label(ui, theme, "Velocity");
        knob_row(ui, |ui| {
            Knob::new(ParamId::VelToAmp, fmt_percent).show(ui, state, theme);
            Knob::new(ParamId::VelToCutoff, fmt_percent).show(ui, state, theme);
        });
    });
}
