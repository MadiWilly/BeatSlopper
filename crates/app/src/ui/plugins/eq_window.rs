//! The EQ window: head row (title, the EQ it edits: `ON MASTER` or `ON INSERT 3 · SLOT 1`,
//! EQ ON), the graph ([`super::eq_graph`]), the selected
//! band's controls (TYPE segmented, FREQ / GAIN / Q knobs, ON) with the OUTPUT gain knob, and
//! the preset column with its MODIFIED lamp. Everything is the `neu` kit on a `neu::panel`.

use egui::{Align, Layout, Ui, vec2};
use engine::fx::eq::{
    self, BAND_COUNT, DEFAULT_Q, EqKind, EqParam, EqState, MAX_FREQ, MAX_GAIN_DB, MAX_Q, MIN_FREQ,
    MIN_GAIN_DB, MIN_Q,
};
use neu::{KnobSize, KnobSpec, Theme};

use super::{
    COLUMN_GAP, KNOB_BLOCK_HEIGHT, PRESET_COLUMN_WIDTH, SECTION_GAP, fmt_q, knob_edit, preset_list,
};
use crate::plugins::{EqTarget, PluginEdit, PluginShared};
use crate::ui::controls::{
    self, GROUP_GAP, KNOB_GAP, PANEL_HEAD_GAP, ROW_HEIGHT, fmt_hz, fmt_level_db,
};

/// Labels of the TYPE selector, in [`EqKind::ALL`] order.
pub const KIND_LABELS: [&str; 5] = ["L-Cut", "L-Shelf", "Peak", "H-Shelf", "H-Cut"];
/// Height of the band-controls block under the graph (head row + gap + knob block).
pub const CONTROLS_HEIGHT: f32 = ROW_HEIGHT + GROUP_GAP + KNOB_BLOCK_HEIGHT;
/// Smallest graph height.
pub const MIN_GRAPH_HEIGHT: f32 = 120.0;

/// Renders the whole EQ window body into `ui`.
pub fn show(ui: &mut Ui, shared: &mut PluginShared) {
    let theme = shared.theme.clone();
    neu::panel(ui, &theme, |ui| {
        ui.set_min_size(ui.available_size());
        head(ui, &theme, shared);
        ui.add_space(PANEL_HEAD_GAP);
        let total = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = COLUMN_GAP;
            let left_width = (ui.available_width() - PRESET_COLUMN_WIDTH - COLUMN_GAP).max(200.0);
            ui.allocate_ui_with_layout(
                vec2(left_width, total),
                Layout::top_down(Align::Min),
                |ui| {
                    let graph_height =
                        (total - CONTROLS_HEIGHT - SECTION_GAP).max(MIN_GRAPH_HEIGHT);
                    super::eq_graph::show(ui, shared, graph_height);
                    ui.add_space(SECTION_GAP);
                    band_controls(ui, &theme, shared);
                },
            );
            ui.allocate_ui_with_layout(
                vec2(PRESET_COLUMN_WIDTH, total),
                Layout::top_down(Align::Min),
                |ui| presets(ui, &theme, shared, total),
            );
        });
    });
}

fn head(ui: &mut Ui, theme: &Theme, shared: &mut PluginShared) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        neu::title(ui, theme, "Parametric EQ");
        // Which EQ this is: the master's, or the one loaded in an insert slot (that one
        // shapes only its insert).
        let on_master = shared.eq_target == EqTarget::Master;
        neu::label(ui, theme, &format!("ON {}", shared.eq_target_label)).on_hover_text(
            if on_master {
                "The master EQ: shapes the whole mix"
            } else {
                "This EQ sits in an insert's effect slot and shapes only that insert. \
                 The analyser still shows the master output."
            },
        );
        controls::right_aligned(ui, "eq_head_right", |ui| {
            ui.spacing_mut().item_spacing.x = KNOB_GAP;
            neu::slider::lamp(ui, theme, shared.eq.on, "Active");
            if neu::toggle_button(ui, theme, "EQ On", shared.eq.on).clicked() {
                let on = !shared.eq.on;
                shared.push_edit(PluginEdit::EqOn(on));
            }
        });
    });
}

/// The selected band's controls plus the output gain.
fn band_controls(ui: &mut Ui, theme: &Theme, shared: &mut PluginShared) {
    let i = shared.eq_selected.min(BAND_COUNT - 1);
    shared.eq_selected = i;
    let band = shared.eq.bands[i];
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = GROUP_GAP;
        let title = format!("Band {}", i + 1);
        controls::group_head(ui, theme, &title, |ui| {
            let before = band.kind.index();
            let mut selected = before;
            neu::segmented(ui, theme, &mut selected, &KIND_LABELS)
                .on_hover_text(EqKind::from_index(selected).label());
            if selected != before {
                shared.push_edit(PluginEdit::EqBand {
                    band: i,
                    param: EqParam::Kind,
                    value: selected as f32,
                    drag_started: true,
                });
            }
            ui.add_space(KNOB_GAP);
            if neu::toggle_button(ui, theme, "On", band.on).clicked() {
                shared.push_edit(PluginEdit::EqBand {
                    band: i,
                    param: EqParam::On,
                    value: f32::from(u8::from(!band.on)),
                    drag_started: true,
                });
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = KNOB_GAP;
            let freq_spec = KnobSpec {
                label: "Freq",
                min: MIN_FREQ,
                max: MAX_FREQ,
                default: EqState::LAYOUT_FREQS[i],
                log: true,
                bipolar: false,
                size: KnobSize::Md,
                fmt: fmt_hz,
            };
            knob_edit(ui, theme, band.freq, &freq_spec, |v, drag| {
                shared.push_edit(PluginEdit::EqBand {
                    band: i,
                    param: EqParam::Freq,
                    value: v,
                    drag_started: drag,
                });
            });
            let gain_spec = KnobSpec {
                label: "Gain",
                min: MIN_GAIN_DB,
                max: MAX_GAIN_DB,
                default: 0.0,
                log: false,
                bipolar: true,
                size: KnobSize::Md,
                fmt: fmt_level_db,
            };
            let gain_response = knob_edit(ui, theme, band.gain_db, &gain_spec, |v, drag| {
                shared.push_edit(PluginEdit::EqBand {
                    band: i,
                    param: EqParam::Gain,
                    value: v,
                    drag_started: drag,
                });
            });
            if !band.kind.has_gain() {
                gain_response.on_hover_text("Cuts have no gain; the knob is ignored.");
            }
            let q_spec = KnobSpec {
                label: "Q",
                min: MIN_Q,
                max: MAX_Q,
                default: DEFAULT_Q,
                log: true,
                bipolar: false,
                size: KnobSize::Md,
                fmt: fmt_q,
            };
            knob_edit(ui, theme, band.q, &q_spec, |v, drag| {
                shared.push_edit(PluginEdit::EqBand {
                    band: i,
                    param: EqParam::Q,
                    value: v,
                    drag_started: drag,
                });
            });
            ui.add_space(COLUMN_GAP);
            let out_spec = KnobSpec {
                label: "Output",
                min: MIN_GAIN_DB,
                max: MAX_GAIN_DB,
                default: 0.0,
                log: false,
                bipolar: true,
                size: KnobSize::Md,
                fmt: fmt_level_db,
            };
            knob_edit(ui, theme, shared.eq.gain_db, &out_spec, |v, drag| {
                shared.push_edit(PluginEdit::EqGain {
                    db: v,
                    drag_started: drag,
                });
            });
        });
    });
}

fn presets(ui: &mut Ui, theme: &Theme, shared: &mut PluginShared, total: f32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = GROUP_GAP;
        controls::group_head(ui, theme, "Presets", |ui| {
            neu::slider::lamp(ui, theme, shared.eq_modified(), "Modified");
        });
        let list_height = (total - ROW_HEIGHT - GROUP_GAP).max(60.0);
        let selected = eq::preset_index_of(&shared.eq).or(shared.eq_preset);
        if let Some(i) = preset_list(
            ui,
            theme,
            eq::PRESETS.iter().map(|p| p.name),
            selected,
            list_height,
        ) {
            shared.push_edit(PluginEdit::EqPreset(i));
        }
    });
}
