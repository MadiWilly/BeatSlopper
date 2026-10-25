//! Vertical sliders, the horizontal slider, the mixer fader and the indicator lamp
//! (`design/round3/WINNER.md` section 4, "Faders", "Sliders", "LED").
//!
//! **Vertical slider** ([`vslider`]): a [`SLIDER_WIDTH`] (28) column holding the formatted
//! value (13 pt) on top, the 6 x 72 track (radius 3, `panel_2` to `panel` gradient, 1 px
//! `border_2`) with a 4 wide accent fill from the bottom and a 20 x 12 thumb (radius 3,
//! `panel_4` to `panel_3` gradient, `border_2`, a 1 px `text_2` line), and the caps label
//! at the bottom. The thumb centre travels [`SLIDER_TRAVEL`] (60) points, so pressing or
//! dragging anywhere on the track sets the value from the pointer's y
//! ([`vslider_fraction_at`]), clamped to the range. A double-click resets to `spec.default`;
//! arrows step 1/50 of the range while focused, and the focused thumb wears the 2 px accent
//! focus ring. The response covers the track column only (not the texts).
//!
//! **Fader** ([`fader`], [`fader_sized`]): the same control at 24 x 120 for the mixer strips:
//! track 8 wide (radius 4), cap 24 x 12 (radius 3, gradient, a 1 px `text` line 5 from its
//! top inset 4), travel [`FADER_TRAVEL`] (108), a 1 px `text_3` unity mark at 75 % of the
//! travel. The response is the [`FADER_WIDTH`] (24) column. The stereo meter and GR meter sit
//! beside it in the strip (see [`crate::meter`]); the fader draws only itself.
//!
//! **Horizontal slider** ([`hslider`]): 72 x 28, track 72 x 6 at y 11, 4 h accent fill, thumb
//! 12 x 20 with a vertical `text_2` line, travel 60 (a 24 h variant for the keyboard header
//! through [`hslider_sized`]).
//!
//! **Lamp** ([`lamp`]): a 10 pt LED (`ok` when on, `panel_4` off, 1 px border) with its caps
//! label 6 pt to the right.
//!
//! QOL pass: the wheel over the track nudges the value by [`SLIDER_KEY_STEP`] per notch
//! (shift: a quarter); a click on the value readout above the track, or a right-click on the
//! track, opens the inline editor of [`crate::value_edit`] (Enter commits the text parsed
//! back through `spec.fmt`, Escape cancels). The readout is a button named `"<label> value"`.

use egui::{
    Align2, Key, Modifiers, Pos2, Rect, Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2,
    vec2,
};

use crate::Theme;
use crate::button::control_painter;
use crate::chrome::{LedKind, paint_led};
use crate::flat::{paint_focus_ring, paint_gradient_control};
use crate::knob::readout_name;
use crate::text::{self, TextKind};
use crate::value_edit::{begin_edit, parse_value, readout, wheel_notches};

/// How a slider maps and displays its value.
#[derive(Clone, Copy)]
pub struct SliderSpec<'a> {
    /// Caps label shown below the slider.
    pub label: &'a str,
    pub min: f32,
    pub max: f32,
    /// Value restored by a double-click.
    pub default: f32,
    /// Formats the value for display.
    pub fmt: fn(f32) -> String,
}

/// Width of the vertical slider column (28).
pub const SLIDER_WIDTH: f32 = 28.0;
/// Size of the vertical slider's track: 6 wide, 72 tall.
pub const SLIDER_TRACK: Vec2 = Vec2::new(6.0, 72.0);
/// Corner radius of the slider track.
pub const SLIDER_TRACK_RADIUS: f32 = 3.0;
/// Height of the slider thumb (12; the thumb is [`SLIDER_THUMB_WIDTH`] x 12) and of the
/// fader cap. The thumb centre starts half a thumb inside the track ends.
pub const SLIDER_THUMB: f32 = 12.0;
/// Width of the vertical slider's thumb (20).
pub const SLIDER_THUMB_WIDTH: f32 = 20.0;
/// Width of the accent fill inside the track (4).
pub const SLIDER_FILL: f32 = 4.0;
/// Soft Console end markers: retired (0) in Deck, kept for the app.
pub const SLIDER_MARKER: f32 = 0.0;
pub const SLIDER_MARKER_OFFSET: f32 = 0.0;
/// Vertical travel of the thumb centre (track height minus the thumb): 60.
pub const SLIDER_TRAVEL: f32 = SLIDER_TRACK.y - SLIDER_THUMB;
/// Size of the mixer fader's track: 8 wide, 120 tall.
pub const FADER_TRACK: Vec2 = Vec2::new(8.0, 120.0);
/// Width of the fader column and its cap (24).
pub const FADER_WIDTH: f32 = 24.0;
/// Vertical travel of the fader cap centre (track height minus the cap): 108.
pub const FADER_TRAVEL: f32 = FADER_TRACK.y - SLIDER_THUMB;
/// Gap between the value, the track and the label.
pub const SLIDER_GAP: f32 = 4.0;
/// Fraction of the range one arrow-key press moves.
pub const SLIDER_KEY_STEP: f32 = 1.0 / 50.0;
/// Diameter of the lamp (the LED).
pub const LAMP_DIAMETER: f32 = 10.0;
/// Gap between the lamp and its label.
pub const LAMP_GAP: f32 = 6.0;
/// Size of the horizontal slider (72 x 28), its track (72 x 6) and thumb (12 x 20).
pub const HSLIDER_SIZE: Vec2 = Vec2::new(72.0, 28.0);
pub const HSLIDER_TRACK_HEIGHT: f32 = 6.0;
pub const HSLIDER_THUMB: Vec2 = Vec2::new(12.0, 20.0);
/// Fraction of the fader travel (from the bottom) where the unity (0 dB) mark sits.
pub const FADER_UNITY_MARK: f32 = 0.75;

impl SliderSpec<'_> {
    /// `v` clamped to `min..=max` (tolerates a reversed range).
    pub fn clamp(&self, v: f32) -> f32 {
        let (lo, hi) = (self.min.min(self.max), self.min.max(self.max));
        if v.is_nan() { lo } else { v.clamp(lo, hi) }
    }

    /// The fraction `0..=1` of the range that `v` sits at (0 at `min`).
    pub fn to_frac(&self, v: f32) -> f32 {
        if self.max == self.min {
            0.0
        } else {
            ((self.clamp(v) - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }

    /// The value at fraction `f` of the range.
    pub fn from_frac(&self, f: f32) -> f32 {
        self.clamp(self.min + f.clamp(0.0, 1.0) * (self.max - self.min))
    }

    /// The value formatted with `fmt`.
    pub fn format(&self, v: f32) -> String {
        (self.fmt)(v)
    }
}

/// The thumb centre's travel on a track of the given rect (its height minus the thumb):
/// [`SLIDER_TRAVEL`] for the 6 x 72 track, [`FADER_TRAVEL`] for the fader's 8 x 120.
fn travel_of(track: Rect) -> f32 {
    (track.height() - SLIDER_THUMB).max(f32::EPSILON)
}

/// The fraction (1 at the top, 0 at the bottom) a pointer at `y` selects on a track whose
/// rect is `track`: the thumb centre maps linearly over the track's travel (its height minus
/// the thumb) starting half a thumb below the top edge.
pub fn vslider_fraction_at(track: Rect, y: f32) -> f32 {
    (1.0 - (y - track.min.y - SLIDER_THUMB * 0.5) / travel_of(track)).clamp(0.0, 1.0)
}

/// The thumb centre for fraction `f` on `track` (inverse of [`vslider_fraction_at`]).
pub fn vslider_thumb_pos(track: Rect, f: f32) -> Pos2 {
    pos2(
        track.center().x,
        track.min.y + SLIDER_THUMB * 0.5 + (1.0 - f.clamp(0.0, 1.0)) * travel_of(track),
    )
}

/// The fraction (0 at the left, 1 at the right) a pointer at `x` selects on a horizontal
/// `track` whose thumb is `thumb_width` wide.
pub fn hslider_fraction_at(track: Rect, thumb_width: f32, x: f32) -> f32 {
    let travel = (track.width() - thumb_width).max(f32::EPSILON);
    ((x - track.min.x - thumb_width * 0.5) / travel).clamp(0.0, 1.0)
}

/// Vertical slider (track 6 x 72, 20 x 12 thumb). `changed()` when the value moved.
///
/// The returned response's rect is the interactive column around the track
/// ([`SLIDER_WIDTH`] x the track height); the track itself is centred in it.
pub fn vslider(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &SliderSpec<'_>) -> Response {
    vertical_slider(
        ui,
        theme,
        value,
        spec,
        SLIDER_TRACK,
        SLIDER_WIDTH,
        true,
        false,
    )
}

/// Mixer fader: 24 x 120 (track 8 wide, cap 24 x 12, travel 108, unity mark). Press or drag
/// sets the value from the pointer, a double-click resets to `spec.default`, arrows step
/// while focused, and the `"label: value"` tooltip appears on hover. `changed()` when the
/// value moved. The response's rect is the [`FADER_WIDTH`] x [`FADER_TRACK`]`.y` column.
pub fn fader(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &SliderSpec<'_>) -> Response {
    vertical_slider(ui, theme, value, spec, FADER_TRACK, FADER_WIDTH, true, true)
}

/// Shortest track [`fader_sized`] accepts: the cap plus 12.
pub const FADER_MIN_TRACK: f32 = SLIDER_THUMB + 12.0;

/// A [`fader`] whose track is `track_height` tall (clamped to at least [`FADER_MIN_TRACK`])
/// and, with `label` false, no caps label under the track (the value still sits above it and
/// `spec.label` still names the control for the tooltip and accessibility).
pub fn fader_sized(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &SliderSpec<'_>,
    track_height: f32,
    label: bool,
) -> Response {
    let track = Vec2::new(FADER_TRACK.x, track_height.max(FADER_MIN_TRACK));
    vertical_slider(ui, theme, value, spec, track, FADER_WIDTH, label, true)
}

/// The shared vertical body: a `track_size` track in a `column` wide column with the value
/// above and the label below; `fader` draws the fader cap and unity mark instead of the
/// slider thumb.
#[allow(clippy::too_many_arguments)]
fn vertical_slider(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &SliderSpec<'_>,
    track_size: Vec2,
    column: f32,
    show_label: bool,
    fader: bool,
) -> Response {
    let painter = control_painter(ui);
    let label_galley = text::layout(&painter, spec.label, TextKind::Label, theme.text_2);
    let label_height = if show_label {
        SLIDER_GAP + label_galley.size().y
    } else {
        0.0
    };
    let value_height = text::layout(
        &painter,
        &spec.format(spec.clamp(*value)),
        TextKind::Value,
        theme.text,
    )
    .size()
    .y;
    let height = value_height + SLIDER_GAP + track_size.y + label_height;
    let width = column.max(label_galley.size().x.min(column + 16.0));

    // The column is laid out as one block; only the track column senses input.
    let (rect, block) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let track = Rect::from_center_size(
        pos2(
            rect.center().x,
            rect.min.y + value_height + SLIDER_GAP + track_size.y * 0.5,
        ),
        track_size,
    );
    let hit = Rect::from_center_size(track.center(), vec2(column, track_size.y));
    let mut response = ui.interact(hit, block.id.with("track"), Sense::click_and_drag());

    let before = *value;
    let mut v = spec.clamp(before);
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.double_clicked() {
        v = spec.clamp(spec.default);
    } else if (response.is_pointer_button_down_on() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        v = spec.from_frac(vslider_fraction_at(track, pos.y));
    }
    let notches = wheel_notches(ui, &response);
    if notches != 0.0 {
        v = spec.from_frac(spec.to_frac(v) + notches * SLIDER_KEY_STEP);
    }
    if response.has_focus() {
        let steps = ui.input_mut(|i| {
            i.count_and_consume_key(Modifiers::NONE, Key::ArrowUp) as i64
                + i.count_and_consume_key(Modifiers::NONE, Key::ArrowRight) as i64
                - i.count_and_consume_key(Modifiers::NONE, Key::ArrowDown) as i64
                - i.count_and_consume_key(Modifiers::NONE, Key::ArrowLeft) as i64
        });
        if steps != 0 {
            v = spec.from_frac(spec.to_frac(v) + steps as f32 * SLIDER_KEY_STEP);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Home)) {
            v = spec.clamp(spec.default);
        }
    }
    v = accesskit_adjust(ui, &response, spec, v);
    // Typed entry: the readout above the track, or a right-click on the track.
    let readout_id = block.id.with("readout");
    if response.secondary_clicked() {
        begin_edit(ui, readout_id, &spec.format(v));
    }
    let typed = readout(
        ui,
        theme,
        readout_id,
        pos2(rect.center().x, rect.min.y),
        Align2::CENTER_TOP,
        &spec.format(v),
        TextKind::Value.font_id(),
        theme.text,
        &readout_name(spec.label),
    );
    if let Some(text) = typed
        && let Some(parsed) = parse_value(&text, spec.min, spec.max, false, spec.fmt)
    {
        v = spec.clamp(parsed);
    }

    *value = v;
    if v != before {
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let radius = if fader { 4.0 } else { SLIDER_TRACK_RADIUS };
        paint_gradient_control(
            &painter,
            track,
            radius,
            theme.panel_2,
            theme.panel,
            theme.border_2,
        );
        let thumb = vslider_thumb_pos(track, spec.to_frac(v));
        if fader {
            // Unity mark: a 1 px text-3 line at 75 % of the travel, 6 past the track sides.
            let unity_y = vslider_thumb_pos(track, FADER_UNITY_MARK).y;
            painter.hline(
                (track.min.x - 6.0)..=(track.max.x + 6.0),
                unity_y,
                Stroke::new(1.0, theme.text_3),
            );
            let cap = Rect::from_center_size(thumb, vec2(column, SLIDER_THUMB));
            paint_gradient_control(
                &painter,
                cap,
                3.0,
                theme.panel_4,
                theme.panel_3,
                theme.border_2,
            );
            painter.hline(
                (cap.min.x + 4.0)..=(cap.max.x - 4.0),
                cap.min.y + 5.0,
                Stroke::new(1.0, theme.text),
            );
            if response.has_focus() && !response.is_pointer_button_down_on() {
                paint_focus_ring(&painter, cap, 3.0, theme);
            }
        } else {
            // Accent fill from the bottom up to the thumb.
            let fill = Rect::from_min_max(
                pos2(track.center().x - SLIDER_FILL * 0.5, thumb.y),
                pos2(track.center().x + SLIDER_FILL * 0.5, track.max.y - 1.0),
            );
            if fill.height() > 0.0 {
                painter.rect_filled(fill, 2.0, theme.accent);
            }
            let thumb_rect = Rect::from_center_size(thumb, vec2(SLIDER_THUMB_WIDTH, SLIDER_THUMB));
            paint_gradient_control(
                &painter,
                thumb_rect,
                3.0,
                theme.panel_4,
                theme.panel_3,
                theme.border_2,
            );
            painter.hline(
                (thumb_rect.min.x + 4.0)..=(thumb_rect.max.x - 4.0),
                thumb_rect.center().y,
                Stroke::new(1.0, theme.text_2),
            );
            if response.has_focus() && !response.is_pointer_button_down_on() {
                paint_focus_ring(&painter, thumb_rect, 3.0, theme);
            }
        }
        if show_label {
            text::paint_galley(
                &painter,
                pos2(rect.center().x, rect.max.y),
                Align2::CENTER_BOTTOM,
                label_galley,
                TextKind::Label,
                theme.text_2,
            );
        }
    }

    declare_slider(ui, &response, spec, v);
    response.on_hover_text(crate::knob::tooltip_text(spec.label, &spec.format(v)))
}

/// Horizontal slider: 72 x 28 (track 72 x 6 at y 11, 4 h accent fill, 12 x 20 thumb with a
/// vertical `text_2` line, travel 60). No texts: the caller labels it. `changed()` when the
/// value moved; double-click resets; arrows step while focused.
pub fn hslider(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &SliderSpec<'_>) -> Response {
    hslider_sized(ui, theme, value, spec, HSLIDER_SIZE)
}

/// [`hslider`] of an explicit size (the 24 h variant of the keyboard header): the track
/// keeps its 6 height and the thumb is `size.y - 8` tall.
pub fn hslider_sized(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &SliderSpec<'_>,
    size: Vec2,
) -> Response {
    let painter = control_painter(ui);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(size.x, HSLIDER_TRACK_HEIGHT));
    let thumb_size = vec2(HSLIDER_THUMB.x, (size.y - 8.0).max(8.0));

    let before = *value;
    let mut v = spec.clamp(before);
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.double_clicked() {
        v = spec.clamp(spec.default);
    } else if (response.is_pointer_button_down_on() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        v = spec.from_frac(hslider_fraction_at(track, thumb_size.x, pos.x));
    }
    let notches = wheel_notches(ui, &response);
    if notches != 0.0 {
        v = spec.from_frac(spec.to_frac(v) + notches * SLIDER_KEY_STEP);
    }
    if response.has_focus() {
        let steps = ui.input_mut(|i| {
            i.count_and_consume_key(Modifiers::NONE, Key::ArrowUp) as i64
                + i.count_and_consume_key(Modifiers::NONE, Key::ArrowRight) as i64
                - i.count_and_consume_key(Modifiers::NONE, Key::ArrowDown) as i64
                - i.count_and_consume_key(Modifiers::NONE, Key::ArrowLeft) as i64
        });
        if steps != 0 {
            v = spec.from_frac(spec.to_frac(v) + steps as f32 * SLIDER_KEY_STEP);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Home)) {
            v = spec.clamp(spec.default);
        }
    }
    v = accesskit_adjust(ui, &response, spec, v);
    *value = v;
    if v != before {
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        paint_gradient_control(
            &painter,
            track,
            SLIDER_TRACK_RADIUS,
            theme.panel_2,
            theme.panel,
            theme.border_2,
        );
        let travel = (track.width() - thumb_size.x).max(0.0);
        let cx = track.min.x + thumb_size.x * 0.5 + spec.to_frac(v) * travel;
        let fill = Rect::from_min_max(
            pos2(track.min.x + 1.0, track.center().y - SLIDER_FILL * 0.5),
            pos2(cx, track.center().y + SLIDER_FILL * 0.5),
        );
        if fill.width() > 0.0 {
            painter.rect_filled(fill, 2.0, theme.accent);
        }
        let thumb = Rect::from_center_size(pos2(cx, rect.center().y), thumb_size);
        paint_gradient_control(
            &painter,
            thumb,
            3.0,
            theme.panel_4,
            theme.panel_3,
            theme.border_2,
        );
        painter.vline(
            thumb.center().x,
            (thumb.min.y + 4.0)..=(thumb.max.y - 4.0),
            Stroke::new(1.0, theme.text_2),
        );
        if response.has_focus() && !response.is_pointer_button_down_on() {
            paint_focus_ring(&painter, thumb, 3.0, theme);
        }
    }

    declare_slider(ui, &response, spec, v);
    response.on_hover_text(crate::knob::tooltip_text(spec.label, &spec.format(v)))
}

/// Applies AccessKit Increment / Decrement / SetValue requests to `v`.
fn accesskit_adjust(ui: &Ui, response: &Response, spec: &SliderSpec<'_>, mut v: f32) -> f32 {
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
        v = spec.from_frac(spec.to_frac(v) + (inc as f32 - dec as f32) * SLIDER_KEY_STEP);
    }
    v
}

/// Reports the control as a slider to AccessKit.
fn declare_slider(ui: &Ui, response: &Response, spec: &SliderSpec<'_>, v: f32) {
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(v), spec.label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        use egui::accesskit::Action;
        let (lo, hi) = (spec.min.min(spec.max), spec.min.max(spec.max));
        node.set_min_numeric_value(f64::from(lo));
        node.set_max_numeric_value(f64::from(hi));
        node.set_numeric_value_step(f64::from((hi - lo) * SLIDER_KEY_STEP));
        node.add_action(Action::SetValue);
        if v < hi {
            node.add_action(Action::Increment);
        }
        if lo < v {
            node.add_action(Action::Decrement);
        }
    });
}

/// A 10 pt indicator lamp (the `ok` LED when on, `panel_4` off) with a caps label 6 to the
/// right. Accessible as a label with the ON state.
pub fn lamp(ui: &mut Ui, theme: &Theme, on: bool, label: &str) -> Response {
    lamp_kind(ui, theme, on, label, LedKind::Ok)
}

/// [`lamp`] in another LED colour (`LedKind::Accent` for the limiter, `Rec` for record).
pub fn lamp_kind(ui: &mut Ui, theme: &Theme, on: bool, label: &str, kind: LedKind) -> Response {
    let painter = control_painter(ui);
    let galley = text::layout(&painter, label, TextKind::Label, theme.text_2);
    let size = vec2(
        LAMP_DIAMETER + LAMP_GAP + galley.size().x,
        LAMP_DIAMETER.max(galley.size().y),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let centre = pos2(rect.min.x + LAMP_DIAMETER * 0.5, rect.center().y);
        paint_led(&painter, centre, on, kind, theme);
        text::paint_galley(
            &painter,
            pos2(rect.min.x + LAMP_DIAMETER + LAMP_GAP, rect.center().y),
            Align2::LEFT_CENTER,
            galley,
            TextKind::Label,
            theme.text_2,
        );
    }
    response.widget_info(|| WidgetInfo::selected(Role::Label, ui.is_enabled(), on, label));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_maps_the_thumb_travel() {
        let track = Rect::from_min_size(pos2(0.0, 10.0), SLIDER_TRACK);
        assert_eq!(SLIDER_TRAVEL, 60.0);
        assert_eq!(vslider_fraction_at(track, 10.0 + 6.0), 1.0);
        assert_eq!(vslider_fraction_at(track, 10.0 + 6.0 + SLIDER_TRAVEL), 0.0);
        assert!(
            (vslider_fraction_at(track, 10.0 + 6.0 + SLIDER_TRAVEL * 0.25) - 0.75).abs() < 1e-5
        );
        assert_eq!(vslider_fraction_at(track, -100.0), 1.0);
        assert_eq!(vslider_fraction_at(track, 1000.0), 0.0);
        let p = vslider_thumb_pos(track, 0.75);
        assert!((vslider_fraction_at(track, p.y) - 0.75).abs() < 1e-5);
    }

    #[test]
    fn fader_track_maps_over_its_own_travel() {
        let track = Rect::from_min_size(pos2(0.0, 10.0), FADER_TRACK);
        assert_eq!(FADER_TRAVEL, 108.0);
        assert_eq!(FADER_TRACK, vec2(8.0, 120.0));
        assert_eq!(FADER_WIDTH, 24.0);
        assert_eq!(vslider_fraction_at(track, 10.0 + 6.0), 1.0);
        assert_eq!(vslider_fraction_at(track, 10.0 + 6.0 + FADER_TRAVEL), 0.0);
        let p = vslider_thumb_pos(track, 0.25);
        assert!((p.y - (10.0 + 6.0 + 0.75 * FADER_TRAVEL)).abs() < 1e-4);
        assert!((vslider_fraction_at(track, p.y) - 0.25).abs() < 1e-5);
    }

    #[test]
    fn horizontal_fraction_maps_the_travel() {
        let track = Rect::from_min_size(pos2(10.0, 0.0), vec2(72.0, 6.0));
        assert_eq!(hslider_fraction_at(track, 12.0, 16.0), 0.0);
        assert_eq!(hslider_fraction_at(track, 12.0, 76.0), 1.0);
        assert!((hslider_fraction_at(track, 12.0, 46.0) - 0.5).abs() < 1e-5);
        assert_eq!(hslider_fraction_at(track, 12.0, -50.0), 0.0);
    }

    #[test]
    fn spec_round_trips_and_clamps() {
        let s = SliderSpec {
            label: "A",
            min: 0.0,
            max: 2000.0,
            default: 12.0,
            fmt: |v| format!("{v:.0} ms"),
        };
        assert_eq!(s.from_frac(0.5), 1000.0);
        assert_eq!(s.to_frac(500.0), 0.25);
        assert_eq!(s.clamp(-1.0), 0.0);
        assert_eq!(s.clamp(1e9), 2000.0);
        assert_eq!(s.format(12.0), "12 ms");
    }
}
