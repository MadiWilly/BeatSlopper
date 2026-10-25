//! The reverb window: the impulse-response decay envelope (filled area on a log dB scale
//! over `0..=max(RT60, 1 s)`, re-rendered on a worker thread 100 ms after the last change)
//! with the live wet-level meter beside it, the seven knobs (SIZE, DAMP, PRE-DELAY, WIDTH,
//! LOW CUT, HIGH CUT, MIX) and the preset column with its MODIFIED lamp.

use egui::{Align, Align2, Color32, Layout, Mesh, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use engine::fx::reverb::{self, ReverbParam};
use neu::text::{TextKind, paint_text};
use neu::{Depth, KnobSize, KnobSpec, Theme, paint_soft_rect};

use super::eq_graph::{GRAPH_RADIUS, PLOT_INSET, SPECTRUM_ALPHA};
use super::{
    COLUMN_GAP, KNOB_BLOCK_HEIGHT, PRESET_COLUMN_WIDTH, SECTION_GAP, fmt_ms_plain, fmt_seconds,
    knob_edit, preset_list,
};
use crate::plugins::{EnvelopeCache, PluginEdit, PluginShared, envelope_seconds};
use crate::ui::controls::{
    self, GROUP_GAP, KNOB_GAP, PANEL_HEAD_GAP, ROW_HEIGHT, fmt_hz, fmt_percent,
};

/// Bottom of the envelope plot's dB scale (relative to the peak).
pub const ENVELOPE_FLOOR_DB: f32 = -60.0;
/// Width of the wet meter beside the plot.
pub const WET_METER_WIDTH: f32 = 6.0;
/// Gap between the plot and the wet meter.
pub const WET_METER_GAP: f32 = 10.0;
/// Smallest plot height.
pub const MIN_PLOT_HEIGHT: f32 = 100.0;
/// Width of a 2 pt envelope line.
pub const ENVELOPE_LINE: f32 = 2.0;

/// Renders the whole reverb window body into `ui`.
pub fn show(ui: &mut Ui, shared: &mut PluginShared) {
    let theme = shared.theme.clone();
    let busy = shared.poll_envelope();
    if busy {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
    shared.decay_wet();
    if shared.wet_shown > 0.0 || shared.playing {
        // The level comes from the main frame: keep both sides moving.
        ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
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
                    let plot_height =
                        (total - KNOB_BLOCK_HEIGHT - SECTION_GAP).max(MIN_PLOT_HEIGHT);
                    plot_row(ui, &theme, shared, plot_height);
                    ui.add_space(SECTION_GAP);
                    knobs(ui, &theme, shared);
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

fn head(ui: &mut Ui, theme: &Theme, shared: &PluginShared) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        neu::title(ui, theme, "Reverb");
        controls::right_aligned(ui, "reverb_head_right", |ui| {
            ui.spacing_mut().item_spacing.x = KNOB_GAP;
            neu::label(ui, theme, "RT60");
            neu::value(ui, theme, &fmt_seconds(shared.reverb.decay_time_60db()));
        });
    });
}

/// The envelope plot with the wet meter at its right.
fn plot_row(ui: &mut Ui, theme: &Theme, shared: &mut PluginShared, height: f32) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = WET_METER_GAP;
        let plot_width = (ui.available_width() - WET_METER_WIDTH - WET_METER_GAP).max(120.0);
        let (rect, _response) = ui.allocate_exact_size(vec2(plot_width, height), Sense::hover());
        if ui.is_rect_visible(rect) {
            paint_envelope(ui, theme, shared, rect);
        }
        neu::meter(ui, theme, shared.wet_shown, vec2(WET_METER_WIDTH, height))
            .on_hover_text("Reverb return level");
    });
}

/// Time-axis tick step for a plot `seconds` long.
#[must_use]
pub fn tick_step(seconds: f32) -> f32 {
    if seconds <= 1.5 {
        0.25
    } else if seconds <= 4.0 {
        0.5
    } else if seconds <= 8.0 {
        1.0
    } else {
        2.0
    }
}

/// y of a level `db` below the peak (0 at the top, [`ENVELOPE_FLOOR_DB`] at the bottom).
#[must_use]
pub fn envelope_y(plot: Rect, db: f32) -> f32 {
    let t = (db / ENVELOPE_FLOOR_DB).clamp(0.0, 1.0);
    plot.top() + t * plot.height()
}

/// x of `seconds` on a plot spanning `span` seconds.
#[must_use]
pub fn time_x(plot: Rect, seconds: f32, span: f32) -> f32 {
    plot.left() + (seconds / span.max(1e-3)).clamp(0.0, 1.0) * plot.width()
}

fn paint_envelope(ui: &Ui, theme: &Theme, shared: &PluginShared, rect: Rect) {
    let painter = ui.painter().with_clip_rect(rect);
    paint_soft_rect(&painter, rect, GRAPH_RADIUS, Depth::InsetM, theme);
    let plot = rect.shrink(PLOT_INSET);
    let span = shared
        .envelope
        .as_ref()
        .map_or_else(|| envelope_seconds(&shared.reverb), |e| e.seconds);

    // Grid: time ticks along the bottom, dB lines every 20 dB.
    let faint = theme.shadow_dark.gamma_multiply(0.55);
    let step = tick_step(span);
    let mut t = 0.0;
    while t <= span + 1e-3 {
        let x = time_x(plot, t, span);
        painter.vline(x, plot.y_range(), Stroke::new(1.0, faint));
        let label = if step < 1.0 {
            format!("{t:.2}")
        } else {
            format!("{t:.0}")
        };
        paint_text(
            &painter,
            pos2(x, rect.bottom() - 1.0),
            Align2::CENTER_BOTTOM,
            &label,
            TextKind::Label,
            theme.text_secondary,
        );
        t += step;
    }
    for db in [0.0, -20.0, -40.0, -60.0] {
        let y = envelope_y(plot, db);
        painter.hline(plot.x_range(), y, Stroke::new(1.0, faint));
        paint_text(
            &painter,
            pos2(rect.left() + 3.0, y),
            if db == 0.0 {
                Align2::LEFT_TOP
            } else {
                Align2::LEFT_BOTTOM
            },
            &format!("{db:.0}"),
            TextKind::Label,
            theme.text_secondary,
        );
    }

    match &shared.envelope {
        Some(env) if !env.bins.is_empty() => paint_curve(&painter, theme, env, plot),
        _ => {
            paint_text(
                &painter,
                plot.center(),
                Align2::CENTER_CENTER,
                "Rendering",
                TextKind::Label,
                theme.text_secondary,
            );
        }
    }

    // RT60 marker.
    let rt60 = shared.reverb.decay_time_60db();
    if rt60 <= span {
        let x = time_x(plot, rt60, span);
        painter.vline(x, plot.y_range(), Stroke::new(1.0, theme.text_secondary));
        // The label sits to the right of the marker unless that would leave the plot.
        let (pos, anchor) = if x + 36.0 > plot.right() {
            (pos2(x - 4.0, plot.top() + 2.0), Align2::RIGHT_TOP)
        } else {
            (pos2(x + 4.0, plot.top() + 2.0), Align2::LEFT_TOP)
        };
        paint_text(
            &painter,
            pos,
            anchor,
            "RT60",
            TextKind::Label,
            theme.text_secondary,
        );
    }
}

fn paint_curve(painter: &egui::Painter, theme: &Theme, env: &EnvelopeCache, plot: Rect) {
    let peak = env.peak();
    let fill = Color32::from_rgba_unmultiplied(
        theme.accent.r(),
        theme.accent.g(),
        theme.accent.b(),
        SPECTRUM_ALPHA,
    );
    let n = env.bins.len();
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(n * 2);
    mesh.reserve_triangles(n.saturating_sub(1) * 2);
    let mut line = Vec::with_capacity(n);
    for (i, &v) in env.bins.iter().enumerate() {
        let x = plot.left() + plot.width() * i as f32 / (n.max(2) - 1) as f32;
        let db = if v > 0.0 {
            (20.0 * (v / peak).log10()).max(ENVELOPE_FLOOR_DB)
        } else {
            ENVELOPE_FLOOR_DB
        };
        let y = envelope_y(plot, db);
        mesh.colored_vertex(pos2(x, y), fill);
        mesh.colored_vertex(pos2(x, plot.bottom()), fill);
        if i > 0 {
            let b = (i * 2) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
        line.push(pos2(x, y));
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::line(line, Stroke::new(ENVELOPE_LINE, theme.accent)));
}

/// The seven knobs in [`ReverbParam::ALL`] order.
fn knobs(ui: &mut Ui, theme: &Theme, shared: &mut PluginShared) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = KNOB_GAP;
        for param in ReverbParam::ALL {
            let range = param.range();
            let fmt: fn(f32) -> String = match param {
                ReverbParam::PreDelay => fmt_ms_plain,
                ReverbParam::LowCut | ReverbParam::HighCut => fmt_hz,
                _ => fmt_percent,
            };
            let spec = KnobSpec {
                label: param.label(),
                min: *range.start(),
                max: *range.end(),
                default: param.default_value(),
                log: param.is_log(),
                bipolar: false,
                size: KnobSize::Md,
                fmt,
            };
            let value = shared.reverb.get(param);
            knob_edit(ui, theme, value, &spec, |v, drag| {
                shared.push_edit(PluginEdit::Reverb {
                    param,
                    value: v,
                    drag_started: drag,
                });
            });
        }
    });
}

fn presets(ui: &mut Ui, theme: &Theme, shared: &mut PluginShared, total: f32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = GROUP_GAP;
        controls::group_head(ui, theme, "Presets", |ui| {
            neu::slider::lamp(ui, theme, shared.reverb_modified(), "Modified");
        });
        let list_height = (total - ROW_HEIGHT - GROUP_GAP).max(60.0);
        let selected = reverb::preset_index_of(&shared.reverb).or(shared.reverb_preset);
        if let Some(i) = preset_list(
            ui,
            theme,
            reverb::PRESETS.iter().map(|p| p.name),
            selected,
            list_height,
        ) {
            shared.push_edit(PluginEdit::ReverbPreset(i));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plot_geometry() {
        let p = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 100.0));
        assert_eq!(envelope_y(p, 0.0), 0.0);
        assert_eq!(envelope_y(p, -60.0), 100.0);
        assert_eq!(envelope_y(p, -30.0), 50.0);
        assert_eq!(envelope_y(p, -90.0), 100.0);
        assert_eq!(time_x(p, 0.0, 2.0), 0.0);
        assert_eq!(time_x(p, 1.0, 2.0), 200.0);
        assert_eq!(time_x(p, 5.0, 2.0), 400.0);
        assert_eq!(tick_step(1.0), 0.25);
        assert_eq!(tick_step(3.0), 0.5);
        assert_eq!(tick_step(6.0), 1.0);
        assert_eq!(tick_step(10.0), 2.0);
    }
}
