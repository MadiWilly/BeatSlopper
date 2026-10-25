//! The EQ window's graph: log-frequency axis 20 Hz..20 kHz, dB axis -18..+18, the live
//! spectrum (filled, accent at 20 %), the response curve (accent, 2 pt) and one draggable
//! node per band.
//!
//! Interaction (FL Studio style): drag a node to move its frequency (x) and gain (y; the cuts
//! have no gain and only move horizontally); the scroll wheel over a node changes its Q;
//! double-click toggles the band on / off (an off band shows as a faint hollow dot on the
//! 0 dB line so it can be brought back); right-click selects the band and cycles its type;
//! a plain click selects. Every change is queued as a [`PluginEdit`] through
//! [`PluginShared::push_edit`].
//!
//! The geometry helpers ([`freq_to_x`], [`db_to_y`], [`node_pos`], [`hit_node`] and their
//! inverses) are public and unit-tested; the tests drive drags through them.

use egui::{Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use engine::fx::eq::{
    BAND_COUNT, EqBand, EqKind, EqParam, EqState, MAX_FREQ, MAX_GAIN_DB, MAX_Q, MIN_FREQ,
    MIN_GAIN_DB, MIN_Q,
};
use engine::spectrum::SPECTRUM_FLOOR_DB;
use neu::text::{TextKind, paint_text};
use neu::{Depth, Theme, paint_soft_circle, paint_soft_rect};

use crate::plugins::{PluginEdit, PluginShared};

/// Radius of a band node (a 12 pt circle).
pub const NODE_RADIUS: f32 = 6.0;
/// Distance from a node's centre within which the pointer grabs it.
pub const HIT_RADIUS: f32 = 11.0;
/// Corner radius of the graph's inset background.
pub const GRAPH_RADIUS: f32 = 16.0;
/// Inset from the graph's edge to the plot area (room for the axis labels).
pub const PLOT_INSET: f32 = 10.0;
/// Alpha of the spectrum fill (20 %).
pub const SPECTRUM_ALPHA: u8 = 51;
/// Width of the response curve.
pub const CURVE_WIDTH: f32 = 2.0;
/// Horizontal step between curve / spectrum samples, points.
pub const SAMPLE_STEP: f32 = 2.0;
/// Octaves of Q per scroll-wheel point (120 points = one notch = Q x 2).
pub const Q_WHEEL_OCTAVES_PER_POINT: f32 = 1.0 / 120.0;
/// Frequencies that get a grid line and a label.
pub const FREQ_GRID: [f32; 10] = [
    20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0, 20_000.0,
];
/// Gains that get a grid line and a label.
pub const DB_GRID: [f32; 7] = [-18.0, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0];

/// x of `hz` on the log axis of `plot` (clamped into the axis range).
#[must_use]
pub fn freq_to_x(plot: Rect, hz: f32) -> f32 {
    let hz = if hz.is_finite() {
        hz.clamp(MIN_FREQ, MAX_FREQ)
    } else {
        1000.0
    };
    plot.left() + (hz / MIN_FREQ).ln() / (MAX_FREQ / MIN_FREQ).ln() * plot.width()
}

/// Frequency at `x` (inverse of [`freq_to_x`], clamped).
#[must_use]
pub fn x_to_freq(plot: Rect, x: f32) -> f32 {
    let t = if plot.width() > 0.0 {
        ((x - plot.left()) / plot.width()).clamp(0.0, 1.0)
    } else {
        0.5
    };
    MIN_FREQ * (MAX_FREQ / MIN_FREQ).powf(t)
}

/// y of `db` on the gain axis of `plot` (0 dB at the vertical centre; not clamped, so the
/// curve may leave the plot and get clipped).
#[must_use]
pub fn db_to_y(plot: Rect, db: f32) -> f32 {
    plot.center().y - db / MAX_GAIN_DB * plot.height() * 0.5
}

/// Gain at `y` (inverse of [`db_to_y`], clamped to the band gain range).
#[must_use]
pub fn y_to_db(plot: Rect, y: f32) -> f32 {
    if plot.height() <= 0.0 {
        return 0.0;
    }
    ((plot.center().y - y) / (plot.height() * 0.5) * MAX_GAIN_DB).clamp(MIN_GAIN_DB, MAX_GAIN_DB)
}

/// y of a spectrum level: [`SPECTRUM_FLOOR_DB`] at the bottom, 0 dBFS at the top.
#[must_use]
pub fn spectrum_to_y(plot: Rect, db: f32) -> f32 {
    let t = ((db - SPECTRUM_FLOOR_DB) / -SPECTRUM_FLOOR_DB).clamp(0.0, 1.0);
    plot.bottom() - t * plot.height()
}

/// Where band `band`'s node sits: its frequency, and its gain (0 dB for the cuts).
#[must_use]
pub fn node_pos(plot: Rect, band: &EqBand) -> Pos2 {
    let db = if band.kind.has_gain() {
        band.gain_db
    } else {
        0.0
    };
    pos2(freq_to_x(plot, band.freq), db_to_y(plot, db))
}

/// The band whose node is nearest to `pos` within [`HIT_RADIUS`], if any (on bands win
/// ties).
#[must_use]
pub fn hit_node(plot: Rect, state: &EqState, pos: Pos2) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, band) in state.bands.iter().enumerate() {
        let d = node_pos(plot, band).distance(pos);
        if d <= HIT_RADIUS {
            let score = if band.on { d } else { d + 0.5 };
            if best.is_none_or(|(_, b)| score < b) {
                best = Some((i, score));
            }
        }
    }
    best.map(|(i, _)| i)
}

/// The plot area inside a graph rect.
#[must_use]
pub fn plot_rect(graph: Rect) -> Rect {
    graph.shrink(PLOT_INSET)
}

/// The next kind after `kind` (right-click cycling).
#[must_use]
pub fn next_kind(kind: EqKind) -> EqKind {
    EqKind::from_index((kind.index() + 1) % EqKind::ALL.len())
}

/// Short frequency label: `20`, `100`, `1k`, `20k`.
#[must_use]
pub fn freq_label(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{}k", (hz / 1000.0).round() as i32)
    } else {
        format!("{}", hz.round() as i32)
    }
}

/// Draws the graph `height` tall across the available width, handles its interaction and
/// returns the graph rect (also stored in [`PluginShared::eq_graph_rect`]).
pub fn show(ui: &mut Ui, shared: &mut PluginShared, height: f32) -> Rect {
    let theme = shared.theme.clone();
    let width = ui.available_width().max(120.0);
    let (graph, response) =
        ui.allocate_exact_size(vec2(width, height.max(80.0)), Sense::click_and_drag());
    shared.eq_graph_rect = Some(graph);
    let plot = plot_rect(graph);

    // Keep the analyser fresh (about 30 fps) and keep repainting while a window shows it.
    shared.update_spectrum(false);
    ui.ctx()
        .request_repaint_after(crate::plugins::SPECTRUM_INTERVAL);

    if ui.is_rect_visible(graph) {
        paint(ui, &theme, shared, graph, plot);
    }
    interact(ui, shared, plot, &response);
    graph
}

fn paint(ui: &Ui, theme: &Theme, shared: &PluginShared, graph: Rect, plot: Rect) {
    let painter = ui.painter().with_clip_rect(graph);
    paint_soft_rect(&painter, graph, GRAPH_RADIUS, Depth::InsetM, theme);

    // Grid.
    let faint = theme.shadow_dark.gamma_multiply(0.55);
    for hz in FREQ_GRID {
        let x = freq_to_x(plot, hz);
        painter.vline(x, plot.y_range(), Stroke::new(1.0, faint));
        paint_text(
            &painter,
            pos2(x, graph.bottom() - 1.0),
            Align2::CENTER_BOTTOM,
            &freq_label(hz),
            TextKind::Label,
            theme.text_secondary,
        );
    }
    for db in DB_GRID {
        let y = db_to_y(plot, db);
        let stroke = if db == 0.0 {
            Stroke::new(1.0, theme.shadow_dark)
        } else {
            Stroke::new(1.0, faint)
        };
        painter.hline(plot.x_range(), y, stroke);
        if db == MIN_GAIN_DB {
            // The bottom edge belongs to the frequency labels.
            continue;
        }
        let label = if db == 0.0 {
            "0".to_owned()
        } else {
            format!("{db:+.0}")
        };
        paint_text(
            &painter,
            pos2(graph.left() + 3.0, y),
            Align2::LEFT_CENTER,
            &label,
            TextKind::Label,
            theme.text_secondary,
        );
    }

    // Spectrum: a filled strip under the analyser curve.
    let fill = Color32::from_rgba_unmultiplied(
        theme.accent.r(),
        theme.accent.g(),
        theme.accent.b(),
        SPECTRUM_ALPHA,
    );
    let steps = ((plot.width() / SAMPLE_STEP).ceil() as usize).max(2);
    let mut mesh = Mesh::default();
    mesh.reserve_vertices((steps + 1) * 2);
    mesh.reserve_triangles(steps * 2);
    let mut curve = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let x = plot.left() + plot.width() * i as f32 / steps as f32;
        let hz = x_to_freq(plot, x);
        let level = shared.analyzer.level_at(hz, shared.sample_rate);
        let y = spectrum_to_y(plot, level);
        mesh.colored_vertex(pos2(x, y), fill);
        mesh.colored_vertex(pos2(x, plot.bottom()), fill);
        if i > 0 {
            let b = (i * 2) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
        let db = shared.eq.response_db(hz, shared.sample_rate);
        curve.push(pos2(x, db_to_y(plot, db)));
    }
    painter.add(Shape::mesh(mesh));

    // Response curve.
    painter.add(Shape::line(curve, Stroke::new(CURVE_WIDTH, theme.accent)));

    // Nodes.
    for (i, band) in shared.eq.bands.iter().enumerate() {
        let pos = node_pos(plot, band);
        if !band.on {
            painter.circle_stroke(
                pos,
                NODE_RADIUS * 0.7,
                Stroke::new(1.0, theme.text_secondary.gamma_multiply(0.6)),
            );
            continue;
        }
        if i == shared.eq_selected {
            paint_soft_circle(&painter, pos, NODE_RADIUS, Depth::OnGlow, theme);
        } else {
            paint_soft_circle(&painter, pos, NODE_RADIUS, Depth::RaisedS, theme);
            painter.circle_stroke(pos, NODE_RADIUS - 0.75, Stroke::new(1.5, theme.accent));
        }
        paint_text(
            &painter,
            pos - vec2(0.0, NODE_RADIUS + 3.0),
            Align2::CENTER_BOTTOM,
            &format!("{}", i + 1),
            TextKind::Label,
            theme.text_secondary,
        );
    }
    // Block any plain background painting outside the inset: nothing to do, the panel is
    // the surface colour. (Kept explicit so the graph never paints a hard border.)
    let _ = CornerRadius::ZERO;
}

fn interact(ui: &Ui, shared: &mut PluginShared, plot: Rect, response: &egui::Response) {
    let hover = response.hover_pos();
    let hovered = hover.and_then(|p| hit_node(plot, &shared.eq, p));

    if response.drag_started() {
        // egui reports the drag once the pointer has moved past its threshold: the node
        // to grab is the one under the *press* position, not the current one.
        let origin = ui
            .input(|input| input.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos());
        let grabbed = origin
            .and_then(|p| hit_node(plot, &shared.eq, p))
            .filter(|&i| shared.eq.bands[i].on);
        shared.eq_drag = grabbed;
        if let Some(i) = grabbed {
            shared.eq_selected = i;
        }
    }
    if response.dragged()
        && let Some(i) = shared.eq_drag
        && let Some(p) = response.interact_pointer_pos()
    {
        // Only the first edit of the drag opens a new undo step; the frequency and gain
        // edits of one node drag share the band's undo key and coalesce.
        let mut first = response.drag_started();
        let band = shared.eq.bands[i];
        let freq = x_to_freq(plot, p.x);
        if (freq - band.freq).abs() > 1e-3 {
            shared.push_edit(PluginEdit::EqBand {
                band: i,
                param: EqParam::Freq,
                value: freq,
                drag_started: first,
            });
            first = false;
        }
        if band.kind.has_gain() {
            let db = y_to_db(plot, p.y);
            if (db - band.gain_db).abs() > 1e-3 {
                shared.push_edit(PluginEdit::EqBand {
                    band: i,
                    param: EqParam::Gain,
                    value: db,
                    drag_started: first,
                });
            }
        }
    }
    if response.drag_stopped() {
        shared.eq_drag = None;
    }

    if response.double_clicked()
        && let Some(i) = hovered
    {
        let on = !shared.eq.bands[i].on;
        shared.eq_selected = i;
        shared.push_edit(PluginEdit::EqBand {
            band: i,
            param: EqParam::On,
            value: f32::from(u8::from(on)),
            drag_started: true,
        });
    } else if response.clicked()
        && let Some(i) = hovered
    {
        shared.eq_selected = i;
    }
    if response.secondary_clicked()
        && let Some(i) = hovered
    {
        shared.eq_selected = i;
        let kind = next_kind(shared.eq.bands[i].kind);
        shared.push_edit(PluginEdit::EqBand {
            band: i,
            param: EqParam::Kind,
            value: kind.index() as f32,
            drag_started: true,
        });
    }

    // Scroll wheel over a node: Q.
    if let Some(i) = hovered {
        let dy = ui.input(|input| input.smooth_scroll_delta.y);
        if dy != 0.0 {
            let q = shared.eq.bands[i].q * 2f32.powf(dy * Q_WHEEL_OCTAVES_PER_POINT);
            shared.push_edit(PluginEdit::EqBand {
                band: i,
                param: EqParam::Q,
                value: q.clamp(MIN_Q, MAX_Q),
                drag_started: false,
            });
        }
        let band = shared.eq.bands[i];
        response.clone().on_hover_text(format!(
            "Band {}: {} at {}, {}, Q {:.2}. Drag: freq / gain. Wheel: Q. Double-click: on / off. Right-click: type.",
            i + 1,
            band.kind.label(),
            crate::ui::controls::fmt_hz(band.freq),
            crate::ui::controls::fmt_level_db(band.gain_db),
            band.q
        ));
    }
    debug_assert!(shared.eq_selected < BAND_COUNT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Rect {
        Rect::from_min_size(pos2(10.0, 20.0), vec2(600.0, 200.0))
    }

    #[test]
    fn axes_round_trip() {
        let p = plot();
        assert_eq!(freq_to_x(p, MIN_FREQ), p.left());
        assert!((freq_to_x(p, MAX_FREQ) - p.right()).abs() < 1e-3);
        for hz in [20.0, 55.0, 440.0, 1000.0, 6300.0, 20_000.0] {
            let back = x_to_freq(p, freq_to_x(p, hz));
            assert!((back - hz).abs() / hz < 1e-4, "{hz} -> {back}");
        }
        assert_eq!(db_to_y(p, 0.0), p.center().y);
        assert_eq!(db_to_y(p, MAX_GAIN_DB), p.top());
        assert_eq!(db_to_y(p, MIN_GAIN_DB), p.bottom());
        for db in [-18.0, -7.5, 0.0, 3.0, 18.0] {
            assert!((y_to_db(p, db_to_y(p, db)) - db).abs() < 1e-4);
        }
        assert_eq!(y_to_db(p, -1000.0), MAX_GAIN_DB);
        assert_eq!(spectrum_to_y(p, SPECTRUM_FLOOR_DB), p.bottom());
        assert_eq!(spectrum_to_y(p, 0.0), p.top());
        assert_eq!(freq_to_x(p, f32::NAN), freq_to_x(p, 1000.0));
        assert_eq!(freq_label(1000.0), "1k");
        assert_eq!(freq_label(50.0), "50");
        assert_eq!(next_kind(EqKind::HighCut), EqKind::LowCut);
        assert_eq!(next_kind(EqKind::Peak), EqKind::HighShelf);
    }

    #[test]
    fn nodes_sit_on_their_values_and_hit_testing_prefers_on_bands() {
        let p = plot();
        let mut state = EqState::FLAT;
        state.set(3, EqParam::Gain, 6.0);
        let node = node_pos(p, &state.bands[3]);
        assert_eq!(node.x, freq_to_x(p, 1000.0));
        assert_eq!(node.y, db_to_y(p, 6.0));
        // Cuts sit on the 0 dB line whatever their gain.
        assert_eq!(node_pos(p, &state.bands[0]).y, p.center().y);
        assert_eq!(hit_node(p, &state, node), Some(3));
        assert_eq!(
            hit_node(p, &state, node + vec2(HIT_RADIUS * 0.9, 0.0)),
            Some(3)
        );
        assert_eq!(
            hit_node(p, &state, node + vec2(HIT_RADIUS * 3.0, 40.0)),
            None
        );
        // Two nodes on top of each other: the one that is on wins.
        state.set(2, EqParam::Freq, 1000.0);
        state.set(2, EqParam::Gain, 6.0);
        state.set(2, EqParam::On, 0.0);
        state.set(3, EqParam::On, 1.0);
        let at = node_pos(p, &state.bands[3]);
        assert_eq!(hit_node(p, &state, at), Some(3));
        state.set(3, EqParam::On, 0.0);
        state.set(2, EqParam::On, 1.0);
        assert_eq!(hit_node(p, &state, at), Some(2), "the on band wins");
        assert_eq!(hit_node(p, &state, pos2(at.x, p.center().y)), None);
    }
}
