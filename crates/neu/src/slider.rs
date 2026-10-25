//! Thin vertical slider, the mixer fader and the indicator lamp (`.vslider` / `.lamp` in
//! `design/preview-a.html`).
//!
//! The slider is a [`SLIDER_WIDTH`]-wide column: the formatted value (13 pt) on top, the
//! 6x100 inset-s track (radius 3) with a 14 pt accent on-glow thumb and two 4 pt accent end
//! markers 7 pt beyond either end, and the caps label at the bottom. The thumb centre travels
//! [`SLIDER_TRAVEL`] points (the track minus the thumb), so pressing or dragging anywhere on
//! the track sets the value from the pointer's y ([`vslider_fraction_at`]), clamped to the
//! range. A double-click resets to `spec.default`; arrows step 1/50 of the range while
//! focused, and the focused thumb wears the 1.5 pt accent focus ring
//! ([`crate::shadow::paint_focus_circle`]) 1 pt outside it. The response covers the track
//! column only (not the texts) so clicks on the value or label do nothing.
//!
//! The fader ([`fader`]) is the same control on the taller [`FADER_TRACK`] (6x140) for the
//! mixer strips; both are drawn by one private `vertical_slider`.
//!
//! The lamp is a 10 pt circle, inset-s when off and accent + on-glow when on, with its caps
//! label 10 pt to the right.

use egui::{
    Align2, Key, Modifiers, Pos2, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::shadow::{Depth, paint_focus_circle, paint_soft_circle, paint_soft_rect};
use crate::text::{self, TextKind};

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

/// Width of the slider column (`.vslider` `width`).
pub const SLIDER_WIDTH: f32 = 36.0;
/// Size of the track (`.vslider .track`): 6 wide, 100 tall.
pub const SLIDER_TRACK: Vec2 = Vec2::new(6.0, 100.0);
/// Corner radius of the track.
pub const SLIDER_TRACK_RADIUS: f32 = 3.0;
/// Diameter of the thumb (`.vslider .thumb`).
pub const SLIDER_THUMB: f32 = 14.0;
/// Diameter of the end markers (`.vslider .mk`).
pub const SLIDER_MARKER: f32 = 4.0;
/// Distance of a marker's centre beyond the track end (`top: -9px` for a 4 pt dot).
pub const SLIDER_MARKER_OFFSET: f32 = 7.0;
/// Vertical travel of the thumb centre (track height minus the thumb diameter).
pub const SLIDER_TRAVEL: f32 = SLIDER_TRACK.y - SLIDER_THUMB;
/// Size of the mixer fader's track: 6 wide, 140 tall (M6, `docs/ARCHITECTURE.md`).
pub const FADER_TRACK: Vec2 = Vec2::new(6.0, 140.0);
/// Vertical travel of the fader thumb centre (track height minus the thumb diameter).
pub const FADER_TRAVEL: f32 = FADER_TRACK.y - SLIDER_THUMB;
/// Gap between the value, the track and the label (`.vslider` `gap`).
pub const SLIDER_GAP: f32 = 8.0;
/// Fraction of the range one arrow-key press moves.
pub const SLIDER_KEY_STEP: f32 = 1.0 / 50.0;
/// Diameter of the lamp (`.lamp`).
pub const LAMP_DIAMETER: f32 = 10.0;
/// Gap between the lamp and its label (`.toggle-row` `gap`).
pub const LAMP_GAP: f32 = 10.0;

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
/// [`SLIDER_TRAVEL`] for the 6x100 track, [`FADER_TRAVEL`] for the fader's 6x140.
fn travel_of(track: Rect) -> f32 {
    (track.height() - SLIDER_THUMB).max(f32::EPSILON)
}

/// The fraction (1 at the top, 0 at the bottom) a pointer at `y` selects on a track whose
/// rect is `track`: the thumb centre maps linearly over the track's travel (its height minus
/// the thumb, [`SLIDER_TRAVEL`] for a slider, [`FADER_TRAVEL`] for a fader) starting half a
/// thumb below the top edge, like the mock's `fromY`.
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

/// Thin vertical slider (track 6x100, 14 pt accent thumb). `changed()` when the value moved.
///
/// The returned response's rect is the interactive column around the track
/// ([`SLIDER_WIDTH`] x the track height); the track itself is centred in it.
pub fn vslider(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &SliderSpec<'_>) -> Response {
    vertical_slider(ui, theme, value, spec, SLIDER_TRACK)
}

/// Mixer fader: the [`vslider`] control on the taller [`FADER_TRACK`] (6x140) with the same
/// 14 pt on-glow thumb, end markers, value above and caps label below. Press or drag sets the
/// value from the pointer, a double-click resets to `spec.default`, arrows step while
/// focused, and the `"label: value"` tooltip appears on hover. `changed()` when the value
/// moved.
///
/// The response's rect is the interactive column ([`SLIDER_WIDTH`] x [`FADER_TRACK`]`.y`).
pub fn fader(ui: &mut Ui, theme: &Theme, value: &mut f32, spec: &SliderSpec<'_>) -> Response {
    vertical_slider(ui, theme, value, spec, FADER_TRACK)
}

/// The shared slider body: a `track_size` inset-s track in a [`SLIDER_WIDTH`] column with the
/// value above and the label below.
fn vertical_slider(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut f32,
    spec: &SliderSpec<'_>,
    track_size: Vec2,
) -> Response {
    let painter = ui.painter().clone();
    let label_galley = text::layout(&painter, spec.label, TextKind::Label, theme.text_secondary);
    let value_height = text::layout(
        &painter,
        &spec.format(spec.clamp(*value)),
        TextKind::Value,
        theme.text_primary,
    )
    .size()
    .y;
    let height = value_height + SLIDER_GAP + track_size.y + SLIDER_GAP + label_galley.size().y;

    // The column is laid out as one block; only the track column senses input.
    let (rect, block) = ui.allocate_exact_size(vec2(SLIDER_WIDTH, height), Sense::hover());
    let track = Rect::from_center_size(
        pos2(
            rect.center().x,
            rect.min.y + value_height + SLIDER_GAP + track_size.y * 0.5,
        ),
        track_size,
    );
    let hit = Rect::from_center_size(track.center(), vec2(SLIDER_WIDTH, track_size.y));
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
    *value = v;
    if v != before {
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        text::paint_text(
            &painter,
            pos2(rect.center().x, rect.min.y),
            Align2::CENTER_TOP,
            &spec.format(v),
            TextKind::Value,
            theme.text_primary,
        );
        paint_soft_rect(&painter, track, SLIDER_TRACK_RADIUS, Depth::InsetS, theme);
        for y in [
            track.min.y - SLIDER_MARKER_OFFSET,
            track.max.y + SLIDER_MARKER_OFFSET,
        ] {
            painter.circle_filled(pos2(track.center().x, y), SLIDER_MARKER * 0.5, theme.accent);
        }
        let thumb = vslider_thumb_pos(track, spec.to_frac(v));
        let fill = if response.hovered() || response.dragged() {
            theme.accent_hover
        } else {
            theme.accent
        };
        crate::shadow::paint_soft_circle_with_fill(
            &painter,
            thumb,
            SLIDER_THUMB * 0.5,
            Depth::OnGlow,
            fill,
            theme,
        );
        if response.has_focus() {
            paint_focus_circle(&painter, thumb, SLIDER_THUMB * 0.5, theme);
        }
        text::paint_galley(
            &painter,
            pos2(rect.center().x, rect.max.y),
            Align2::CENTER_BOTTOM,
            label_galley,
            TextKind::Label,
            theme.text_secondary,
        );
    }

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
    response.on_hover_text(crate::knob::tooltip_text(spec.label, &spec.format(v)))
}

/// A 10 pt indicator lamp with a caps label (inset when off, accent glow when on).
pub fn lamp(ui: &mut Ui, theme: &Theme, on: bool, label: &str) -> Response {
    let painter = ui.painter().clone();
    let galley = text::layout(&painter, label, TextKind::Label, theme.text_secondary);
    let size = vec2(
        LAMP_DIAMETER + LAMP_GAP + galley.size().x,
        LAMP_DIAMETER.max(galley.size().y),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let centre = pos2(rect.min.x + LAMP_DIAMETER * 0.5, rect.center().y);
        let depth = if on { Depth::OnGlow } else { Depth::InsetS };
        paint_soft_circle(&painter, centre, LAMP_DIAMETER * 0.5, depth, theme);
        text::paint_galley(
            &painter,
            pos2(rect.min.x + LAMP_DIAMETER + LAMP_GAP, rect.center().y),
            Align2::LEFT_CENTER,
            galley,
            TextKind::Label,
            theme.text_secondary,
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
        assert_eq!(vslider_fraction_at(track, 10.0 + 7.0), 1.0);
        assert_eq!(vslider_fraction_at(track, 10.0 + 7.0 + SLIDER_TRAVEL), 0.0);
        assert!(
            (vslider_fraction_at(track, 10.0 + 7.0 + SLIDER_TRAVEL * 0.25) - 0.75).abs() < 1e-5
        );
        assert_eq!(vslider_fraction_at(track, -100.0), 1.0);
        assert_eq!(vslider_fraction_at(track, 1000.0), 0.0);
        let p = vslider_thumb_pos(track, 0.75);
        assert!((vslider_fraction_at(track, p.y) - 0.75).abs() < 1e-5);
    }

    #[test]
    fn fader_track_maps_over_its_own_travel() {
        let track = Rect::from_min_size(pos2(0.0, 10.0), FADER_TRACK);
        assert_eq!(FADER_TRAVEL, 126.0);
        assert_eq!(vslider_fraction_at(track, 10.0 + 7.0), 1.0);
        assert_eq!(vslider_fraction_at(track, 10.0 + 7.0 + FADER_TRAVEL), 0.0);
        let p = vslider_thumb_pos(track, 0.25);
        assert!((p.y - (10.0 + 7.0 + 0.75 * FADER_TRAVEL)).abs() < 1e-4);
        assert!((vslider_fraction_at(track, p.y) - 0.25).abs() < 1e-5);
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
