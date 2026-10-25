//! Typed value entry and wheel nudging, shared by every numeric control of the kit (QOL pass).
//!
//! # Typed entry
//!
//! A numeric readout ([`readout`]) is the formatted value of a knob, dial, slider or stepper
//! painted as plain text. A click on it (or a right-click anywhere on the control, see the
//! callers) swaps the text for an inline `egui::TextEdit` styled by the theme: an inset-s pill
//! in the surface colour, no outline, the value font, an accent caret, the old text selected
//! so typing replaces it. Enter and focus loss commit, Escape cancels. The editor lives in
//! `ui.data` under the control's id ([`begin_edit`], [`show_edit`]); a committed string is
//! handed back for the caller to parse.
//!
//! # Parsing
//!
//! Controls store values in engine units (a mix knob holds `0.7` and shows `"70 %"`, a time
//! knob holds seconds and shows `"180 ms"`, the master dial holds a linear gain and shows dB).
//! A `KnobSpec` only carries the forward formatter, so the typed text is mapped back through
//! it: [`parse_number`] turns both the typed text and the formatter's output into one
//! "display number" (unit suffixes tolerated: `128 bpm`, `-6 dB`, `70%`, `2.4k` and `2.4 kHz`
//! are 2400, `1.5 s` is 1500 ms, pan `L 30` is -30 and `C` is 0, a decimal comma is a point),
//! then [`value_for_display`] finds the control value whose display number is the typed one:
//! an affine map when three probes of the formatter fit one (every percent, ms, Hz, cents,
//! semitone, dB-level and pan formatter does, so `70 %` lands on exactly `0.7`), otherwise a
//! bisection over the control's fraction (the dB-of-gain formatter). The result is clamped to
//! the range; garbage (`abc`, an empty string, an unknown unit) parses to `None` and leaves
//! the value alone.
//!
//! # Wheel
//!
//! [`wheel_notches`] reports the mouse-wheel notches over a control this frame (positive =
//! up) and clears egui's smoothed scroll delta so the panel's scroll area does not move too;
//! shift makes a notch a quarter notch. [`wheel_steps`] accumulates notches into whole steps
//! for integer controls (the stepper).

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::text::CCursor;
use egui::text_edit::TextEditState;
use egui::text_selection::CCursorRange;
use egui::{
    Align, Align2, Color32, CursorIcon, Event, FontId, Frame, Id, Key, Layout, Margin,
    MouseWheelUnit, Pos2, Rect, Response, Role, Sense, Ui, UiBuilder, WidgetInfo, vec2,
};

use crate::Theme;
use crate::shadow::{Depth, paint_soft_rect};

/// Narrowest inline editor (the value column of a stepper is 36 wide; a knob block is 52).
pub const EDIT_MIN_WIDTH: f32 = 44.0;
/// Horizontal padding of the editor pill around the text.
pub const EDIT_PAD_X: f32 = 6.0;
/// Vertical padding of the editor pill around the text.
pub const EDIT_PAD_Y: f32 = 2.0;
/// Points of trackpad scrolling that count as one wheel notch (egui's `points_per_scroll_line`).
pub const WHEEL_LINE_POINTS: f32 = 50.0;
/// Notches a page-unit wheel event counts for.
pub const WHEEL_PAGE_NOTCHES: f32 = 10.0;
/// Wheel notch multiplier while shift is held (fine).
pub const WHEEL_FINE_FACTOR: f32 = 0.25;
/// Longest typed value accepted.
pub const EDIT_CHAR_LIMIT: usize = 16;

/// The inline editor of one control: its text, the pass it was opened in and the pass it was
/// last shown in (an editor whose control stopped being drawn is dropped, see [`show_edit`]).
#[derive(Debug, Clone, Default)]
struct EditState {
    text: String,
    opened_pass: u64,
    shown_pass: u64,
}

fn state_id(id: Id) -> Id {
    id.with("neu_value_edit")
}

fn edit_widget_id(id: Id) -> Id {
    id.with("neu_value_edit_text")
}

/// What [`show_edit`] reports for a control this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueEditOutput {
    /// No editor is open for this control.
    Idle,
    /// The editor is open and still being typed into.
    Editing,
    /// Enter was pressed or the editor lost focus: the typed text, for the caller to parse.
    Committed(String),
    /// Escape was pressed: nothing changes.
    Cancelled,
}

/// Whether the inline editor of control `id` is open.
pub fn is_editing(ui: &Ui, id: Id) -> bool {
    ui.data(|d| d.get_temp::<EditState>(state_id(id)).is_some())
}

/// Opens the inline editor of control `id`, pre-filled with `text` (fully selected, so typing
/// replaces it) and focused.
pub fn begin_edit(ui: &mut Ui, id: Id, text: &str) {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.data_mut(|d| {
        d.insert_temp(
            state_id(id),
            EditState {
                text: text.to_owned(),
                opened_pass: pass,
                shown_pass: pass,
            },
        );
    });
    let widget_id = edit_widget_id(id);
    let mut state = TextEditState::default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(0),
        CCursor::new(text.chars().count()),
    )));
    TextEditState::store(state, ui.ctx(), widget_id);
    ui.memory_mut(|m| m.request_focus(widget_id));
}

/// Closes the inline editor of control `id` without committing.
pub fn cancel_edit(ui: &mut Ui, id: Id) {
    ui.data_mut(|d| d.remove_temp::<EditState>(state_id(id)));
}

/// Shows the inline editor of control `id`, if open, over `rect` (the readout's text rect:
/// the editor pill is that rect padded and at least [`EDIT_MIN_WIDTH`] wide, aligned to
/// `align` within it). `font` is the readout's font. Reports what happened; the editor is
/// closed on commit or cancel, and dropped when it was not shown for a frame (its control
/// went away).
pub fn show_edit(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    rect: Rect,
    font: FontId,
    align: Align,
) -> ValueEditOutput {
    let sid = state_id(id);
    let Some(mut state) = ui.data(|d| d.get_temp::<EditState>(sid)) else {
        return ValueEditOutput::Idle;
    };
    let pass = ui.ctx().cumulative_pass_nr();
    if pass > state.shown_pass + 1 {
        // The control was not drawn last frame: the editor is stale.
        ui.data_mut(|d| d.remove_temp::<EditState>(sid));
        return ValueEditOutput::Idle;
    }
    state.shown_pass = pass;

    let pill = editor_rect(rect, align);
    paint_soft_rect(ui.painter(), pill, theme.radius_pill, Depth::InsetS, theme);
    if pass == state.opened_pass {
        // Opened by a click this very frame: egui takes focus away from a widget created
        // in the frame of a click elsewhere, so the text field starts next frame.
        ui.data_mut(|d| d.insert_temp(sid, state));
        ui.ctx().request_repaint();
        return ValueEditOutput::Editing;
    }
    let inner = pill.shrink2(vec2(EDIT_PAD_X, EDIT_PAD_Y));
    let widget_id = edit_widget_id(id);
    let response = ui
        .scope_builder(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.text_cursor.stroke.color = theme.accent;
                visuals.selection.bg_fill = theme.glow;
                visuals.selection.stroke.color = theme.text_primary;
                ui.add(
                    egui::TextEdit::singleline(&mut state.text)
                        .id(widget_id)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(font)
                        .text_color(theme.text_primary)
                        .align(Align2([align, Align::Center]))
                        .desired_width(inner.width())
                        .min_size(inner.size())
                        .char_limit(EDIT_CHAR_LIMIT),
                )
            },
        )
        .inner;

    if response.lost_focus() {
        ui.data_mut(|d| d.remove_temp::<EditState>(sid));
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            return ValueEditOutput::Cancelled;
        }
        return ValueEditOutput::Committed(state.text);
    }
    if !response.has_focus() {
        // Focus went elsewhere without a lost-focus event (another editor opened): commit.
        ui.data_mut(|d| d.remove_temp::<EditState>(sid));
        return ValueEditOutput::Committed(state.text);
    }
    ui.data_mut(|d| d.insert_temp(sid, state));
    ValueEditOutput::Editing
}

/// The editor pill for a readout at `rect`: padded by [`EDIT_PAD_X`] / [`EDIT_PAD_Y`], at
/// least [`EDIT_MIN_WIDTH`] wide, grown towards the side `align` leaves free.
pub fn editor_rect(rect: Rect, align: Align) -> Rect {
    let width = (rect.width() + 2.0 * EDIT_PAD_X).max(EDIT_MIN_WIDTH);
    let height = rect.height() + 2.0 * EDIT_PAD_Y;
    let x = match align {
        Align::Min => rect.min.x - EDIT_PAD_X,
        Align::Center => rect.center().x - width * 0.5,
        Align::Max => rect.max.x + EDIT_PAD_X - width,
    };
    Rect::from_min_size(
        Pos2::new(x, rect.center().y - height * 0.5),
        vec2(width, height),
    )
}

/// A clickable numeric readout: paints `text` in `font` and `color` anchored at `pos`, or
/// the inline editor while one is open for `id`. A click opens the editor pre-filled with
/// `text`; the pointer shows the text cursor over it. Accessible as a button named `name`
/// (e.g. `"Cutoff value"`), so tests can click it. Returns the committed text, if the editor
/// committed this frame; the caller parses it (see [`parse_value`]).
#[allow(clippy::too_many_arguments)]
pub fn readout(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    name: &str,
) -> Option<String> {
    let galley = ui.painter().layout_job(LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font.clone(),
            color,
            ..TextFormat::default()
        },
    ));
    let text_rect = anchor.anchor_size(pos, galley.size());
    let align = anchor.x();
    let hit = editor_rect(text_rect, align);
    let response = ui.interact(hit, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), name));
    // The shown text is the node's value, so tests can read what the readout says.
    ui.ctx()
        .accesskit_node_builder(id, |node| node.set_value(text));

    match show_edit(ui, theme, id, text_rect, font, align) {
        ValueEditOutput::Committed(typed) => return Some(typed),
        ValueEditOutput::Editing => return None,
        ValueEditOutput::Idle | ValueEditOutput::Cancelled => {}
    }
    if response.clicked() {
        begin_edit(ui, id, text);
        return None;
    }
    if ui.is_rect_visible(text_rect) {
        ui.painter().galley(text_rect.min, galley, color);
    }
    response.on_hover_cursor(CursorIcon::Text);
    None
}

/// The inline editor floating over a control that shows no readout (an unlabelled small
/// knob): [`show_edit`] on a box of `font`'s row height centred on `centre`. Returns the
/// committed text, if any, this frame. Open it with [`begin_edit`] (the callers do so on a
/// right-click).
pub fn floating_edit(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    centre: Pos2,
    font: FontId,
) -> Option<String> {
    if !is_editing(ui, id) {
        return None;
    }
    let height = ui.fonts_mut(|f| f.row_height(&font));
    let rect = Rect::from_center_size(centre, vec2(EDIT_MIN_WIDTH - 2.0 * EDIT_PAD_X, height));
    match show_edit(ui, theme, id, rect, font, Align::Center) {
        ValueEditOutput::Committed(typed) => Some(typed),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing.

/// The display number of a typed or formatted value string (see the module docs): the
/// leading number with unit suffixes tolerated and scaled (`k` / `kHz` and `s` / `sec` by
/// 1000), pan letters (`L 30` = -30, `R 30` = 30, `C` = 0), a decimal comma, and surrounding
/// white space. `None` for anything else (`""`, `"abc"`, `"-inf dB"`, `"3 bananas"`).
pub fn parse_number(text: &str) -> Option<f32> {
    let lower = text.trim().to_lowercase().replace(',', ".");
    if lower.is_empty() {
        return None;
    }
    // Pan: a leading L / R letter is the sign, a lone C is the centre.
    let (sign, body) = match lower.as_bytes()[0] {
        b'l' if lower.len() > 1 => (-1.0, lower[1..].trim_start()),
        b'r' if lower.len() > 1 => (1.0, lower[1..].trim_start()),
        b'c' if lower.len() == 1 => return Some(0.0),
        _ => (1.0, lower.as_str()),
    };
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut explicit_sign = 1.0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        if bytes[i] == b'-' {
            explicit_sign = -1.0;
        }
        i += 1;
    }
    let start = i;
    let mut seen_digit = false;
    let mut seen_dot = false;
    while i < bytes.len() {
        match bytes[i] {
            b'0'..=b'9' => seen_digit = true,
            b'.' if !seen_dot => seen_dot = true,
            _ => break,
        }
        i += 1;
    }
    if !seen_digit {
        return None;
    }
    let number: f32 = body[start..i].parse().ok()?;
    let unit = body[i..].trim();
    let scale = unit_scale(unit)?;
    let value = sign * explicit_sign * number * scale;
    value.is_finite().then_some(value)
}

/// The factor a unit suffix applies to the number before it, or `None` for an unknown unit.
fn unit_scale(unit: &str) -> Option<f32> {
    Some(match unit {
        "" | "%" | "bpm" | "hz" | "ms" | "db" | "st" | "semi" | "c" | "ct" | "cents" | "x"
        | "bar" | "bars" | "steps" | "step" => 1.0,
        "k" | "khz" => 1000.0,
        "s" | "sec" => 1000.0,
        _ => return None,
    })
}

/// The control value in `min..=max` whose formatted display reads `target` (a display number
/// from [`parse_number`]). `log` is the control's drag mapping (used for the mid probe and the
/// bisection). `None` when `fmt` produces no number anywhere in the range.
pub fn value_for_display(
    fmt: fn(f32) -> String,
    min: f32,
    max: f32,
    log: bool,
    target: f32,
) -> Option<f32> {
    let (lo, hi) = (min.min(max), min.max(max));
    let is_log = log && lo > 0.0 && hi > lo;
    let at = |f: f32| -> f32 {
        let f = f.clamp(0.0, 1.0);
        if is_log {
            lo * (hi / lo).powf(f)
        } else {
            lo + f * (hi - lo)
        }
    };
    let display = |v: f32| parse_number(&fmt(v));
    let clamp = |v: f32| if v.is_nan() { lo } else { v.clamp(lo, hi) };

    if hi <= lo {
        return display(lo).map(|_| lo);
    }

    // Affine fit through the ends, checked at the middle.
    if let (Some(n_lo), Some(n_hi), Some(n_mid)) = (display(lo), display(hi), display(at(0.5))) {
        let a = (n_hi - n_lo) / (hi - lo);
        let b = n_lo - a * lo;
        let tolerance = 0.02 * (n_hi - n_lo).abs() + 0.6;
        if a != 0.0 && (a * at(0.5) + b - n_mid).abs() <= tolerance {
            return Some(clamp((target - b) / a));
        }
    }

    // Monotone (non-decreasing or non-increasing) display: bisect over the fraction.
    let probe = |f: f32| display(at(f));
    let increasing = match (probe(0.0), probe(1.0)) {
        (Some(a), Some(b)) => b >= a,
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (None, None) => match (probe(0.01), probe(0.99)) {
            (Some(a), Some(b)) => b >= a,
            _ => return None,
        },
    };
    let n = |f: f32| -> f32 {
        probe(f).unwrap_or(if increasing {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        })
    };
    let (mut a, mut b) = (0.0_f32, 1.0_f32);
    for _ in 0..48 {
        let m = 0.5 * (a + b);
        let reached = if increasing {
            n(m) >= target
        } else {
            n(m) <= target
        };
        if reached {
            b = m;
        } else {
            a = m;
        }
    }
    Some(clamp(at(b)))
}

/// Parses `text` typed into a control that shows `fmt(value)` over `min..=max` (`log` for a
/// log-mapped control): the display number of the text mapped back through the formatter
/// ([`value_for_display`]), falling back to the number itself when the formatter cannot be
/// inverted, clamped to the range. `None` for garbage.
pub fn parse_value(
    text: &str,
    min: f32,
    max: f32,
    log: bool,
    fmt: fn(f32) -> String,
) -> Option<f32> {
    let target = parse_number(text)?;
    let (lo, hi) = (min.min(max), min.max(max));
    Some(value_for_display(fmt, min, max, log, target).unwrap_or_else(|| target.clamp(lo, hi)))
}

/// Parses `text` typed into an integer control: the display number rounded and clamped to
/// `min..=max`. `None` for garbage.
pub fn parse_int(text: &str, min: i32, max: i32) -> Option<i32> {
    let n = parse_number(text)?;
    let n = n.round();
    if n < i32::MIN as f32 || n > i32::MAX as f32 {
        return None;
    }
    Some((n as i32).clamp(min.min(max), min.max(max)))
}

// ---------------------------------------------------------------------------------------------
// Wheel.

/// Mouse-wheel notches over `response` this frame (positive = wheel up / away), from the raw
/// wheel events, with shift scaling them by [`WHEEL_FINE_FACTOR`]. When non-zero, egui's
/// smoothed scroll delta is cleared so the panel's scroll area stays put. `0.0` when the
/// pointer is not over the control or nothing was scrolled.
pub fn wheel_notches(ui: &mut Ui, response: &Response) -> f32 {
    if !(response.contains_pointer() || response.hovered()) {
        return 0.0;
    }
    let (notches, shift) = ui.input(|i| {
        let mut shift = i.modifiers.shift;
        let notches: f32 = i
            .events
            .iter()
            .map(|e| match e {
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } => {
                    shift |= modifiers.shift;
                    match unit {
                        MouseWheelUnit::Line => delta.y,
                        MouseWheelUnit::Point => delta.y / WHEEL_LINE_POINTS,
                        MouseWheelUnit::Page => delta.y * WHEEL_PAGE_NOTCHES,
                    }
                }
                _ => 0.0,
            })
            .sum();
        (notches, shift)
    });
    if notches == 0.0 {
        return 0.0;
    }
    ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
    if shift {
        notches * WHEEL_FINE_FACTOR
    } else {
        notches
    }
}

/// [`wheel_notches`] accumulated into whole steps for integer controls (the fraction is kept
/// in `ui.data` under `response.id`, so a trackpad adds up). Shift does not apply here.
pub fn wheel_steps(ui: &mut Ui, response: &Response) -> i32 {
    let notches = wheel_notches(ui, response);
    if notches == 0.0 {
        return 0;
    }
    // Undo the fine factor: an integer control steps per notch, shift or not.
    let shift = ui.input(|i| i.modifiers.shift);
    let notches = if shift {
        notches / WHEEL_FINE_FACTOR
    } else {
        notches
    };
    let acc_id = response.id.with("neu_wheel_accum");
    let mut accum = ui.data(|d| d.get_temp::<f32>(acc_id)).unwrap_or(0.0) + notches;
    let steps = accum.trunc();
    accum -= steps;
    ui.data_mut(|d| d.insert_temp(acc_id, accum));
    steps as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pct(v: f32) -> String {
        format!("{} %", (v * 100.0).round() as i32)
    }
    fn ms(seconds: f32) -> String {
        if seconds >= 1.0 {
            format!("{seconds:.2} s")
        } else {
            format!("{:.0} ms", seconds * 1000.0)
        }
    }
    fn hz(v: f32) -> String {
        if v >= 1000.0 {
            format!("{:.1} kHz", v / 1000.0)
        } else if v >= 100.0 {
            format!("{v:.0} Hz")
        } else {
            format!("{v:.1} Hz")
        }
    }
    fn gain_db(g: f32) -> String {
        if g <= 0.0 {
            "-inf dB".to_owned()
        } else {
            format!("{:+.1} dB", 20.0 * g.log10())
        }
    }
    fn pan(p: f32) -> String {
        let pct = (p * 100.0).round() as i32;
        match pct {
            0 => "C".to_owned(),
            p if p < 0 => format!("L {}", -p),
            p => format!("R {p}"),
        }
    }
    fn bpm(v: f32) -> String {
        if (v - v.round()).abs() < 0.05 {
            format!("{}", v.round() as i32)
        } else {
            format!("{v:.1}")
        }
    }

    #[test]
    fn numbers_and_units_parse() {
        assert_eq!(parse_number("128"), Some(128.0));
        assert_eq!(parse_number(" 128 bpm "), Some(128.0));
        assert_eq!(parse_number("128,5"), Some(128.5));
        assert_eq!(parse_number("-6 dB"), Some(-6.0));
        assert_eq!(parse_number("+6dB"), Some(6.0));
        assert_eq!(parse_number("70%"), Some(70.0));
        assert_eq!(parse_number("2.4k"), Some(2400.0));
        assert_eq!(parse_number("2.4 kHz"), Some(2400.0));
        assert_eq!(parse_number("240 Hz"), Some(240.0));
        assert_eq!(parse_number("1.5 s"), Some(1500.0));
        assert_eq!(parse_number("180 ms"), Some(180.0));
        assert_eq!(parse_number("+7 c"), Some(7.0));
        assert_eq!(parse_number("-3 st"), Some(-3.0));
        assert_eq!(parse_number(".5"), Some(0.5));
        assert_eq!(parse_number("L 30"), Some(-30.0));
        assert_eq!(parse_number("R 30"), Some(30.0));
        assert_eq!(parse_number("C"), Some(0.0));
        assert_eq!(parse_number("c"), Some(0.0));
    }

    #[test]
    fn garbage_does_not_parse() {
        for junk in [
            "",
            "   ",
            "abc",
            "-inf dB",
            "3 bananas",
            "+",
            "-",
            ".",
            "1e5",
            "k",
        ] {
            assert_eq!(parse_number(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn affine_formatters_invert_exactly() {
        assert_eq!(parse_value("70 %", 0.0, 1.0, false, pct), Some(0.7));
        assert_eq!(parse_value("70", 0.0, 1.0, false, pct), Some(0.7));
        assert_eq!(parse_value("180 ms", 0.001, 2.0, false, ms), Some(0.18));
        assert_eq!(parse_value("1.5 s", 0.001, 2.0, false, ms), Some(1.5));
        let cut = parse_value("2.4k", 20.0, 20_000.0, true, hz).unwrap();
        assert!((cut - 2400.0).abs() < 0.5, "{cut}");
        let cut = parse_value("240 Hz", 20.0, 20_000.0, true, hz).unwrap();
        assert!((cut - 240.0).abs() < 0.5, "{cut}");
        assert_eq!(parse_value("L 30", -1.0, 1.0, false, pan), Some(-0.3));
        assert_eq!(parse_value("C", -1.0, 1.0, false, pan), Some(0.0));
        assert_eq!(parse_value("140", 60.0, 200.0, false, bpm), Some(140.0));
        assert_eq!(
            parse_value("128.5 bpm", 60.0, 200.0, false, bpm),
            Some(128.5)
        );
        assert_eq!(
            parse_value("-6 dB", -60.0, 6.0, false, |v| format!("{v:+.1} dB")),
            Some(-6.0)
        );
    }

    #[test]
    fn non_affine_formatters_bisect() {
        // Linear gain shown in dB: -6 dB is a gain of about 0.501.
        let g = parse_value("-6 dB", 0.0, 2.0, false, gain_db).unwrap();
        assert!((g - 0.501).abs() < 0.01, "{g}");
        assert_eq!(gain_db(g), "-6.0 dB");
        let g = parse_value("0 dB", 0.0, 2.0, false, gain_db).unwrap();
        assert!((g - 1.0).abs() < 0.01, "{g}");
    }

    #[test]
    fn typed_values_clamp_to_the_range() {
        assert_eq!(parse_value("500", 60.0, 200.0, false, bpm), Some(200.0));
        assert_eq!(parse_value("-10", 60.0, 200.0, false, bpm), Some(60.0));
        assert_eq!(parse_value("150 %", 0.0, 1.0, false, pct), Some(1.0));
        assert_eq!(parse_value("abc", 0.0, 1.0, false, pct), None);
        assert_eq!(parse_value("", 0.0, 1.0, false, pct), None);
        // A formatter with no numbers anywhere falls back to the raw number.
        assert_eq!(
            parse_value("3", 0.0, 10.0, false, |_| "x".to_owned()),
            Some(3.0)
        );
    }

    #[test]
    fn integers_round_and_clamp() {
        assert_eq!(parse_int("4", 1, 7), Some(4));
        assert_eq!(parse_int("4.6", 1, 7), Some(5));
        assert_eq!(parse_int("12", 1, 7), Some(7));
        assert_eq!(parse_int("-2", 1, 7), Some(1));
        assert_eq!(parse_int("oct", 1, 7), None);
    }

    #[test]
    fn editor_rect_pads_and_grows_towards_the_free_side() {
        let text = Rect::from_min_size(Pos2::new(100.0, 10.0), vec2(20.0, 16.0));
        let centred = editor_rect(text, Align::Center);
        assert_eq!(centred.width(), EDIT_MIN_WIDTH);
        assert_eq!(centred.center(), text.center());
        assert_eq!(centred.height(), 16.0 + 2.0 * EDIT_PAD_Y);
        let right = editor_rect(text, Align::Max);
        assert_eq!(right.max.x, text.max.x + EDIT_PAD_X);
        let left = editor_rect(text, Align::Min);
        assert_eq!(left.min.x, text.min.x - EDIT_PAD_X);
        let wide = Rect::from_min_size(Pos2::ZERO, vec2(60.0, 16.0));
        assert_eq!(
            editor_rect(wide, Align::Center).width(),
            60.0 + 2.0 * EDIT_PAD_X
        );
    }
}
