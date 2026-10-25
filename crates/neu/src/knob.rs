//! Rotary knobs and the concentric-ring dial (`.knob` / `.dial` in `design/preview-a.html`).
//!
//! Both controls share one value model ([`KnobSpec`]): a linear or logarithmic map between the
//! value and a fraction `f` in `0..=1`, drawn as an arc that sweeps [`SWEEP_DEG`] (270 degrees)
//! clockwise from 7:30 ([`START_DEG`]) to 4:30. The pointer sits at `START_DEG + f * SWEEP_DEG`
//! measured clockwise from 12 o'clock. Bipolar knobs draw the value arc from 12 o'clock
//! (`f = 0.5`) towards the value instead of from the minimum.
//!
//! Interaction (the mock's `attachDrag`): a vertical drag of [`DRAG_RANGE_PX`] points covers
//! the full range (up = more), shift makes it [`FINE_DRAG_FACTOR`] as fast, a double-click
//! resets to `spec.default`, and arrow keys step [`KEY_STEP`] of the range while the control
//! has keyboard focus (Home restores the default). The response reports `changed()` only
//! when the value actually moved; the value is clamped to `min..=max` on every frame.
//!
//! QOL pass: the mouse wheel over the control nudges it by [`KEY_STEP`] per notch (shift:
//! a quarter), and the value can be typed: a click on the value readout under a labelled
//! knob (or beside a dial), or a right-click anywhere on the control (the only way for the
//! unlabelled small knobs, where the editor floats over the cap), opens the inline editor of
//! [`crate::value_edit`]; Enter commits the text parsed back through `spec.fmt`
//! ([`crate::value_edit::parse_value`]: `70 %`, `2.4k`, `-6 dB`, ...), Escape cancels. The
//! readout is accessible as a button named `"<label> value"`.
//!
//! Geometry per `design/WINNER.md` "Control sizes":
//!
//! * knob `Md`: block width 52, arc ring 50 (r 23, stroke 2.5), cap 40 raised-s, pointer 2x9;
//!   `Sm`: block 34, ring 34, cap 26, pointer 2x6 (the SVG scales with the ring, so the arc
//!   radius and stroke scale by 34/50).
//! * dial `Lg`: arc 96 (r 23 and stroke 2 in a 50-unit viewBox, scaled by 96/50), three
//!   raised-m rings 84 / 64 / 44 and a 6 pt accent dot on the innermost ring; `Sm`: arc 72,
//!   rings 62 / 46 / 32, dot 5.

use std::f32::consts::TAU;

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align2, FontId, Id, Key, Modifiers, Pos2, Response, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2,
    vec2,
};

use crate::Theme;
use crate::shadow::{Depth, paint_focus_circle, paint_soft_circle};
use crate::text::{self, TextKind};
use crate::value_edit::{begin_edit, floating_edit, parse_value, readout, wheel_notches};

/// Knob size: `Md` (cap 40, arc 50, block 52) or `Sm` (cap 26, arc 34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobSize {
    Md,
    Sm,
}

/// Dial size: `Lg` (arc 96, rings 84/64/44) or `Sm` (arc 72, rings 62/46/32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialSize {
    Lg,
    Sm,
}

/// How a knob or dial maps and displays its value.
#[derive(Clone, Copy)]
pub struct KnobSpec<'a> {
    /// Caps label shown above the control.
    pub label: &'a str,
    pub min: f32,
    pub max: f32,
    /// Value restored by a double-click.
    pub default: f32,
    /// Map the drag linearly in log space (for frequencies).
    pub log: bool,
    /// Draw the value arc from 12 o'clock instead of from the minimum.
    pub bipolar: bool,
    pub size: KnobSize,
    /// Formats the value for display.
    pub fmt: fn(f32) -> String,
}

/// Default value formatter: two significant decimals.
pub fn fmt_plain(v: f32) -> String {
    format!("{v:.2}")
}

/// Vertical drag distance, in points, that covers the whole range (the mock's `/ 150`).
pub const DRAG_RANGE_PX: f32 = 150.0;
/// Drag speed multiplier while shift is held (4x finer).
pub const FINE_DRAG_FACTOR: f32 = 0.25;
/// Fraction of the range one arrow-key press moves (1/50).
pub const KEY_STEP: f32 = 1.0 / 50.0;
/// Angle of the arc's start (the minimum), degrees clockwise from 12 o'clock: 7:30.
pub const START_DEG: f32 = -135.0;
/// Angular extent of the arc in degrees (7:30 clockwise to 4:30).
pub const SWEEP_DEG: f32 = 270.0;
/// Arc radius in the mock's 50-unit viewBox (`r="23"`); scales with the ring diameter.
const ARC_R_50: f32 = 23.0;
/// Number of polyline segments for a full 270 degree arc.
const ARC_SEGMENTS: usize = 48;
/// Gap between the label, the ring and the value in a `Md` knob (`.knob` `gap`).
const KNOB_GAP_MD: f32 = 5.0;
/// Width of the knob pointer (`.ptr` `width`).
const POINTER_WIDTH: f32 = 2.0;
/// Gap between the dial's text block and its ring (`.dial` `gap` / `.dial.sm` `gap`).
const DIAL_GAP_LG: f32 = 14.0;
const DIAL_GAP_SM: f32 = 6.0;
/// Gap between the dial's label and value (`.dial .text` `gap`).
const DIAL_TEXT_GAP: f32 = 4.0;
/// Minimum width of the large dial's text column (`.dial .text` `min-width`).
const DIAL_TEXT_MIN_WIDTH: f32 = 58.0;
/// Value size of the large dial (`.dial .text .val` `font-size`).
const DIAL_VALUE_SIZE_LG: f32 = 15.0;

impl KnobSize {
    /// Block width (`.knob` / `.knob.sm` `width`).
    pub fn block_width(self) -> f32 {
        match self {
            KnobSize::Md => 52.0,
            KnobSize::Sm => 34.0,
        }
    }

    /// Diameter of the arc ring box (`.ring`).
    pub fn ring(self) -> f32 {
        match self {
            KnobSize::Md => 50.0,
            KnobSize::Sm => 34.0,
        }
    }

    /// Diameter of the raised cap (`.cap`).
    pub fn cap(self) -> f32 {
        match self {
            KnobSize::Md => 40.0,
            KnobSize::Sm => 26.0,
        }
    }

    /// Radial extent of the pointer, `(inner, outer)` distance from the cap centre. `Md`: the
    /// 2x9 pointer starts 4 pt inside the cap edge; `Sm`: 2x6 starting 3 pt inside.
    fn pointer_span(self) -> (f32, f32) {
        match self {
            KnobSize::Md => (7.0, 16.0),
            KnobSize::Sm => (4.0, 10.0),
        }
    }

    /// Vertical gap between label, ring and value.
    fn gap(self) -> f32 {
        match self {
            KnobSize::Md => KNOB_GAP_MD,
            KnobSize::Sm => 0.0,
        }
    }
}

impl DialSize {
    /// Diameter of the arc ring box (`.dial .ring`).
    pub fn ring(self) -> f32 {
        match self {
            DialSize::Lg => 96.0,
            DialSize::Sm => 72.0,
        }
    }

    /// Diameters of the three raised-m rings, outermost first.
    pub fn rings(self) -> [f32; 3] {
        match self {
            DialSize::Lg => [84.0, 64.0, 44.0],
            DialSize::Sm => [62.0, 46.0, 32.0],
        }
    }

    /// Diameter of the accent pointer dot.
    pub fn dot(self) -> f32 {
        match self {
            DialSize::Lg => 6.0,
            DialSize::Sm => 5.0,
        }
    }

    /// Distance of the dot centre from the dial centre (`.dot` `top` + half its size, measured
    /// from the innermost ring's top edge).
    fn dot_radius(self) -> f32 {
        match self {
            DialSize::Lg => 22.0 - 8.0,
            DialSize::Sm => 16.0 - 6.5,
        }
    }
}

impl KnobSpec<'_> {
    /// Whether the drag maps in log space: `log` with a positive, non-degenerate range.
    pub fn is_log(&self) -> bool {
        self.log && self.min > 0.0 && self.max > self.min
    }

    /// `v` clamped to `min..=max` (tolerates a reversed range).
    pub fn clamp(&self, v: f32) -> f32 {
        let (lo, hi) = if self.min <= self.max {
            (self.min, self.max)
        } else {
            (self.max, self.min)
        };
        if v.is_nan() { lo } else { v.clamp(lo, hi) }
    }

    /// The fraction `0..=1` of the range that `v` sits at (log-spaced when [`Self::is_log`]).
    pub fn to_frac(&self, v: f32) -> f32 {
        let v = self.clamp(v);
        let f = if self.is_log() {
            (v / self.min).ln() / (self.max / self.min).ln()
        } else if self.max == self.min {
            0.0
        } else {
            (v - self.min) / (self.max - self.min)
        };
        f.clamp(0.0, 1.0)
    }

    /// The value at fraction `f` of the range (inverse of [`Self::to_frac`]).
    pub fn from_frac(&self, f: f32) -> f32 {
        let f = f.clamp(0.0, 1.0);
        let v = if self.is_log() {
            self.min * (self.max / self.min).powf(f)
        } else {
            self.min + f * (self.max - self.min)
        };
        self.clamp(v)
    }

    /// The value formatted with `fmt`.
    pub fn format(&self, v: f32) -> String {
        (self.fmt)(v)
    }

    /// The angle of the pointer for `v`, degrees clockwise from 12 o'clock.
    pub fn angle_deg(&self, v: f32) -> f32 {
        START_DEG + self.to_frac(v) * SWEEP_DEG
    }

    /// The value arc as `(from, to)` angles in degrees clockwise from 12 o'clock: from the
    /// minimum to the value, or between 12 o'clock and the value when `bipolar`.
    pub fn value_arc_deg(&self, v: f32) -> (f32, f32) {
        let a = self.angle_deg(v);
        if self.bipolar {
            let centre = START_DEG + 0.5 * SWEEP_DEG;
            (a.min(centre), a.max(centre))
        } else {
            (START_DEG, a)
        }
    }
}

/// The hover tooltip for a value control: `"label: value"`, or just `value` when `label` is
/// empty (the unlabelled sequencer row knobs).
pub fn tooltip_text(label: &str, value: &str) -> String {
    if label.is_empty() {
        value.to_owned()
    } else {
        format!("{label}: {value}")
    }
}

/// Opacity of a dimmed knob (see [`knob_dimmed`]).
pub const DIMMED_OPACITY: f32 = 0.5;
/// Accessible name of the value readout of a control labelled `label`: `"Cutoff value"`.
pub fn readout_name(label: &str) -> String {
    format!("{label} value")
}

/// Applies a committed editor string to `value` (parsed back through `spec.fmt`, clamped);
/// `true` when the value moved. Garbage leaves the value alone.
fn apply_typed(value: &mut f32, spec: &KnobSpec<'_>, typed: Option<String>) -> bool {
    let Some(text) = typed else {
        return false;
    };
    let Some(parsed) = parse_value(&text, spec.min, spec.max, spec.is_log(), spec.fmt) else {
        return false;
    };
    let parsed = spec.clamp(parsed);
    if parsed != *value {
        *value = parsed;
        true
    } else {
        false
    }
}

/// A rotary knob bound to `value`. `changed()` when the value moved; double-click resets.
///
/// Block: the caps label above, the arc ring with its raised cap and accent pointer, the
/// formatted value below. Hovering shows a [`tooltip_text`] tooltip (styled by
/// [`Theme::apply`]). A `Sm` knob with an empty label shows neither label nor value in the
/// block and relies on the tooltip alone (the sequencer row knobs). While the knob has
/// keyboard focus its cap wears the kit's 1.5 pt accent focus ring.
pub fn knob(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &KnobSpec<'_>) -> Response {
    knob_dimmed(ui, theme, value, spec, None)
}

/// [`knob`] that can be shown *dimmed*: with `dimmed` set to a reason (e.g. "Square wave
/// only"), the whole block is painted at [`DIMMED_OPACITY`] with the pointer in the secondary
/// text colour instead of the accent, and the tooltip reads `"label: value · reason"`. The
/// knob stays fully interactive; dimming only says the parameter has no effect right now.
pub fn knob_dimmed(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &KnobSpec<'_>,
    dimmed: Option<&str>,
) -> Response {
    knob_with(ui, theme, value, spec, dimmed, None)
}

/// [`knob`] whose tooltip carries a second line, `note` (e.g. "Return level of the reverb
/// bus"): one tooltip, `"label: value"` over the note, instead of two stacked hover texts.
pub fn knob_noted(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &KnobSpec<'_>,
    note: Option<&str>,
) -> Response {
    knob_with(ui, theme, value, spec, None, note)
}

/// The general knob: [`knob_dimmed`]'s `dimmed` reason and [`knob_noted`]'s `note` together.
/// The tooltip reads `"label: value · reason"` on its first line and `note` on a second.
pub fn knob_with(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &KnobSpec<'_>,
    dimmed: Option<&str>,
    note: Option<&str>,
) -> Response {
    let size = spec.size;
    let labelled = !spec.label.is_empty();
    let mut painter = ui.painter().clone();
    if dimmed.is_some() {
        painter.multiply_opacity(DIMMED_OPACITY);
    }
    let label_galley =
        labelled.then(|| text::layout(&painter, spec.label, TextKind::Label, theme.text_secondary));
    let value_height = if labelled {
        text::layout(
            &painter,
            &spec.format(spec.clamp(*value)),
            TextKind::Value,
            theme.text_primary,
        )
        .size()
        .y
    } else {
        0.0
    };
    let label_height = label_galley.as_ref().map_or(0.0, |g| g.size().y);
    let gap = if labelled { size.gap() } else { 0.0 };
    let height = label_height + gap + size.ring() + gap + value_height;

    let (rect, mut response) =
        ui.allocate_exact_size(vec2(size.block_width(), height), Sense::click_and_drag());
    let changed = rotary_interact(ui, &response, value, spec);
    if changed {
        response.mark_changed();
    }
    let v = *value;
    let centre = pos2(
        rect.center().x,
        rect.min.y + label_height + gap + size.ring() * 0.5,
    );
    let readout_id = response.id.with("readout");
    if response.secondary_clicked() {
        begin_edit(ui, readout_id, &spec.format(v));
    }

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || response.dragged();
        let scale = size.ring() / 50.0;
        paint_arcs(
            &painter,
            theme,
            centre,
            ARC_R_50 * scale,
            2.5 * scale,
            spec,
            v,
        );
        paint_soft_circle(&painter, centre, size.cap() * 0.5, Depth::RaisedS, theme);

        let (inner, outer) = size.pointer_span();
        let dir = angle_dir(spec.angle_deg(v));
        let color = if dimmed.is_some() {
            theme.text_secondary
        } else if hovered {
            theme.accent_hover
        } else {
            theme.accent
        };
        let a = centre + dir * inner;
        let b = centre + dir * outer;
        painter.line_segment([a, b], Stroke::new(POINTER_WIDTH, color));
        painter.circle_filled(a, POINTER_WIDTH * 0.5, color);
        painter.circle_filled(b, POINTER_WIDTH * 0.5, color);
        if response.has_focus() {
            paint_focus_circle(&painter, centre, size.cap() * 0.5, theme);
        }

        if let Some(galley) = label_galley {
            text::paint_galley(
                &painter,
                pos2(rect.center().x, rect.min.y),
                Align2::CENTER_TOP,
                galley,
                TextKind::Label,
                theme.text_secondary,
            );
        }
    }

    // The value readout (labelled knobs) or the floating editor over the cap (unlabelled).
    // The readout paints through `ui`, not the dimmed `painter`, so it dims its colour the
    // same way `multiply_opacity` would.
    let value_color = if dimmed.is_some() {
        theme.text_primary.gamma_multiply(DIMMED_OPACITY)
    } else {
        theme.text_primary
    };
    let typed = if labelled {
        readout(
            ui,
            theme,
            readout_id,
            pos2(rect.center().x, rect.max.y),
            Align2::CENTER_BOTTOM,
            &spec.format(v),
            TextKind::Value.font_id(),
            value_color,
            &readout_name(spec.label),
        )
    } else {
        floating_edit(ui, theme, readout_id, centre, TextKind::Value.font_id())
    };
    if apply_typed(value, spec, typed) {
        response.mark_changed();
    }
    let v = *value;

    declare_slider(ui, &response, spec, v);
    let mut tooltip = tooltip_text(spec.label, &spec.format(v));
    if let Some(reason) = dimmed {
        tooltip.push_str(" · ");
        tooltip.push_str(reason);
    }
    if let Some(note) = note {
        tooltip.push('\n');
        tooltip.push_str(note);
    }
    response.on_hover_text(tooltip)
}

/// The concentric-ring dial (master volume, filter cutoff). Same interaction and
/// [`tooltip_text`] hover tooltip as [`knob`]; the focus ring goes around the outermost
/// raised ring.
///
/// `Lg` lays the text block (label over a 15 pt value, right-aligned, at least 58 wide) to the
/// left of the 96 pt ring with a 14 pt gap; `Sm` stacks the centred text above the 72 pt ring
/// with a 6 pt gap, like `.dial` / `.dial.sm` in the mock.
pub fn dial(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &KnobSpec<'_>,
    size: DialSize,
) -> Response {
    let painter = ui.painter().clone();
    let label_galley = text::layout(&painter, spec.label, TextKind::Label, theme.text_secondary);
    let value_font = match size {
        DialSize::Lg => FontId::proportional(DIAL_VALUE_SIZE_LG),
        DialSize::Sm => TextKind::Value.font_id(),
    };
    let value_galley = |v: f32| {
        painter.layout_job(LayoutJob::single_section(
            spec.format(v),
            TextFormat {
                font_id: value_font.clone(),
                color: theme.text_primary,
                ..TextFormat::default()
            },
        ))
    };
    let probe = value_galley(spec.clamp(*value));
    let text_size = vec2(
        label_galley.size().x.max(probe.size().x),
        label_galley.size().y + DIAL_TEXT_GAP + probe.size().y,
    );
    let ring = size.ring();
    let block = match size {
        DialSize::Lg => vec2(
            text_size.x.max(DIAL_TEXT_MIN_WIDTH) + DIAL_GAP_LG + ring,
            ring,
        ),
        DialSize::Sm => vec2(ring, text_size.y + DIAL_GAP_SM + ring),
    };

    let (rect, mut response) = ui.allocate_exact_size(block, Sense::click_and_drag());
    let changed = rotary_interact(ui, &response, value, spec);
    if changed {
        response.mark_changed();
    }
    let v = *value;
    let readout_id = response.id.with("readout");
    if response.secondary_clicked() {
        begin_edit(ui, readout_id, &spec.format(v));
    }

    let mut typed = None;
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || response.dragged();
        let (centre, label_pos, label_anchor, value_pos, value_anchor) = match size {
            DialSize::Lg => {
                let centre = pos2(rect.max.x - ring * 0.5, rect.center().y);
                let text_right = rect.max.x - ring - DIAL_GAP_LG;
                let top = rect.center().y - text_size.y * 0.5;
                (
                    centre,
                    pos2(text_right, top),
                    Align2::RIGHT_TOP,
                    pos2(text_right, top + label_galley.size().y + DIAL_TEXT_GAP),
                    Align2::RIGHT_TOP,
                )
            }
            DialSize::Sm => {
                let centre = pos2(rect.center().x, rect.max.y - ring * 0.5);
                (
                    centre,
                    pos2(rect.center().x, rect.min.y),
                    Align2::CENTER_TOP,
                    pos2(
                        rect.center().x,
                        rect.min.y + label_galley.size().y + DIAL_TEXT_GAP,
                    ),
                    Align2::CENTER_TOP,
                )
            }
        };

        let scale = ring / 50.0;
        paint_arcs(
            &painter,
            theme,
            centre,
            ARC_R_50 * scale,
            2.0 * scale,
            spec,
            v,
        );
        for d in size.rings() {
            paint_soft_circle(&painter, centre, d * 0.5, Depth::RaisedM, theme);
        }
        let dot = centre + angle_dir(spec.angle_deg(v)) * size.dot_radius();
        let color = if hovered {
            theme.accent_hover
        } else {
            theme.accent
        };
        painter.circle_filled(dot, size.dot() * 0.5, color);
        if response.has_focus() {
            paint_focus_circle(&painter, centre, size.rings()[0] * 0.5, theme);
        }

        text::paint_galley(
            &painter,
            label_pos,
            label_anchor,
            label_galley,
            TextKind::Label,
            theme.text_secondary,
        );
        typed = readout(
            ui,
            theme,
            readout_id,
            value_pos,
            value_anchor,
            &spec.format(v),
            value_font,
            theme.text_primary,
            &readout_name(spec.label),
        );
    }
    if apply_typed(value, spec, typed) {
        response.mark_changed();
    }
    let v = *value;

    declare_slider(ui, &response, spec, v);
    response.on_hover_text(tooltip_text(spec.label, &spec.format(v)))
}

/// Paints a circular arc of `stroke` from `from_deg` to `to_deg` (degrees clockwise from
/// 12 o'clock) around `centre`, as a polyline with round caps. A zero-length arc paints a dot.
pub fn paint_arc(
    painter: &egui::Painter,
    centre: Pos2,
    radius: f32,
    from_deg: f32,
    to_deg: f32,
    stroke: Stroke,
) {
    if stroke.width <= 0.0 || radius <= 0.0 {
        return;
    }
    let (from, to) = (from_deg.min(to_deg), from_deg.max(to_deg));
    let span = to - from;
    let segments = ((span / SWEEP_DEG * ARC_SEGMENTS as f32).ceil() as usize).max(1);
    let cap = stroke.width * 0.5;
    if span < 0.01 {
        painter.circle_filled(centre + angle_dir(from) * radius, cap, stroke.color);
        return;
    }
    let points: Vec<Pos2> = (0..=segments)
        .map(|i| {
            let a = from + span * i as f32 / segments as f32;
            centre + angle_dir(a) * radius
        })
        .collect();
    let first = points[0];
    let last = points[segments];
    painter.add(egui::Shape::line(points, stroke));
    painter.circle_filled(first, cap, stroke.color);
    painter.circle_filled(last, cap, stroke.color);
}

/// Unit vector for an angle in degrees clockwise from 12 o'clock (egui's y axis points down).
fn angle_dir(deg: f32) -> Vec2 {
    let rad = deg / 360.0 * TAU;
    vec2(rad.sin(), -rad.cos())
}

/// The track arc (shadow-dark, full sweep) and the accent value arc.
fn paint_arcs(
    painter: &egui::Painter,
    theme: &Theme,
    centre: Pos2,
    radius: f32,
    width: f32,
    spec: &KnobSpec<'_>,
    v: f32,
) {
    paint_arc(
        painter,
        centre,
        radius,
        START_DEG,
        START_DEG + SWEEP_DEG,
        Stroke::new(width, theme.shadow_dark),
    );
    let (from, to) = spec.value_arc_deg(v);
    paint_arc(
        painter,
        centre,
        radius,
        from,
        to,
        Stroke::new(width, theme.accent),
    );
}

/// Shared knob/dial interaction. Returns `true` when `value` moved this frame.
///
/// Drag: incremental, each frame moves the fraction by `-dy / DRAG_RANGE_PX` (times
/// [`FINE_DRAG_FACTOR`] with shift), using the pointer's own y rather than egui's drag delta so
/// the first frame of a press counts too; the last pointer y is kept in `ui.data` under the
/// response id. Keyboard (while focused): arrows step [`KEY_STEP`] (shift: a quarter step),
/// Home restores the default. AccessKit Increment / Decrement / SetValue are honoured.
fn rotary_interact(ui: &mut Ui, response: &Response, value: &mut f32, spec: &KnobSpec<'_>) -> bool {
    let before = *value;
    let mut v = spec.clamp(before);
    let drag_id: Id = response.id.with("drag_y");

    if response.clicked() || response.drag_started() {
        response.request_focus();
    }

    if response.double_clicked() {
        v = spec.clamp(spec.default);
        ui.data_mut(|d| d.remove_temp::<f32>(drag_id));
    } else if response.is_pointer_button_down_on() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let last = ui.data(|d| d.get_temp::<f32>(drag_id));
            if let Some(last_y) = last {
                let dy = pos.y - last_y;
                if dy != 0.0 {
                    let fine = if ui.input(|i| i.modifiers.shift) {
                        FINE_DRAG_FACTOR
                    } else {
                        1.0
                    };
                    let f = spec.to_frac(v) - dy / DRAG_RANGE_PX * fine;
                    v = spec.from_frac(f);
                }
            }
            ui.data_mut(|d| d.insert_temp(drag_id, pos.y));
        }
    } else {
        ui.data_mut(|d| d.remove_temp::<f32>(drag_id));
    }

    // Wheel: one notch is a key step (shift: a quarter); the panel does not scroll.
    let notches = wheel_notches(ui, response);
    if notches != 0.0 {
        v = spec.from_frac(spec.to_frac(v) + notches * KEY_STEP);
    }

    if response.has_focus() {
        let mut steps = 0.0_f32;
        ui.input_mut(|i| {
            for (key, dir) in [
                (Key::ArrowUp, 1.0),
                (Key::ArrowRight, 1.0),
                (Key::ArrowDown, -1.0),
                (Key::ArrowLeft, -1.0),
            ] {
                steps += dir * i.count_and_consume_key(Modifiers::NONE, key) as f32;
                steps +=
                    dir * FINE_DRAG_FACTOR * i.count_and_consume_key(Modifiers::SHIFT, key) as f32;
            }
            if i.consume_key(Modifiers::NONE, Key::Home) {
                v = spec.clamp(spec.default);
            }
        });
        if steps != 0.0 {
            v = spec.from_frac(spec.to_frac(v) + steps * KEY_STEP);
        }
    }

    // Assistive technology: Increment / Decrement step like the arrow keys, SetValue is absolute.
    let (inc, dec, set) = ui.input(|i| {
        use egui::accesskit::{Action, ActionData};
        (
            i.num_accesskit_action_requests(response.id, Action::Increment),
            i.num_accesskit_action_requests(response.id, Action::Decrement),
            i.accesskit_action_requests(response.id, Action::SetValue)
                .filter_map(|r| match r.data {
                    Some(ActionData::NumericValue(x)) => Some(x as f32),
                    _ => None,
                })
                .last(),
        )
    });
    if let Some(x) = set {
        v = spec.clamp(x);
    }
    if inc != dec {
        v = spec.from_frac(spec.to_frac(v) + (inc as f32 - dec as f32) * KEY_STEP);
    }

    *value = v;
    v != before
}

/// Reports the control as a slider to AccessKit (role, label, value, bounds, step).
fn declare_slider(ui: &Ui, response: &Response, spec: &KnobSpec<'_>, v: f32) {
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(v), spec.label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        use egui::accesskit::Action;
        let (lo, hi) = (spec.min.min(spec.max), spec.min.max(spec.max));
        node.set_min_numeric_value(f64::from(lo));
        node.set_max_numeric_value(f64::from(hi));
        node.set_numeric_value_step(f64::from((hi - lo) * KEY_STEP));
        node.add_action(Action::SetValue);
        if v < hi {
            node.add_action(Action::Increment);
        }
        if lo < v {
            node.add_action(Action::Decrement);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(min: f32, max: f32, log: bool, bipolar: bool) -> KnobSpec<'static> {
        KnobSpec {
            label: "T",
            min,
            max,
            default: min,
            log,
            bipolar,
            size: KnobSize::Md,
            fmt: fmt_plain,
        }
    }

    #[test]
    fn linear_fraction_round_trips_and_clamps() {
        let s = spec(-50.0, 50.0, false, true);
        assert_eq!(s.to_frac(0.0), 0.5);
        assert_eq!(s.from_frac(0.25), -25.0);
        assert_eq!(s.clamp(999.0), 50.0);
        assert_eq!(s.clamp(-999.0), -50.0);
        assert_eq!(s.to_frac(999.0), 1.0);
        assert_eq!(s.value_arc_deg(50.0), (0.0, 135.0));
        assert_eq!(s.value_arc_deg(-50.0), (-135.0, 0.0));
    }

    #[test]
    fn log_fraction_midpoint_is_the_geometric_mean() {
        let s = spec(20.0, 20000.0, true, false);
        let mid = s.from_frac(0.5);
        assert!((mid - (20.0_f32 * 20000.0).sqrt()).abs() < 0.01, "{mid}");
        assert!((s.to_frac(mid) - 0.5).abs() < 1e-5);
        assert_eq!(s.from_frac(0.0), 20.0);
        assert_eq!(s.from_frac(1.0), 20000.0);
        // Log with a non-positive minimum falls back to linear.
        let lin = spec(0.0, 100.0, true, false);
        assert!(!lin.is_log());
        assert_eq!(lin.from_frac(0.5), 50.0);
    }

    #[test]
    fn angles_sweep_270_degrees_from_7_30() {
        let s = spec(0.0, 1.0, false, false);
        assert_eq!(s.angle_deg(0.0), -135.0);
        assert_eq!(s.angle_deg(1.0), 135.0);
        let up = angle_dir(0.0);
        assert!(up.x.abs() < 1e-6 && (up.y + 1.0).abs() < 1e-6);
        let right = angle_dir(90.0);
        assert!((right.x - 1.0).abs() < 1e-6 && right.y.abs() < 1e-6);
    }
}
