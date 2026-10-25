//! Small Deck pieces: keycaps, LEDs, tags, chips, the insert number box, the channel name
//! button, pattern tiles and the text field (`design/round3/WINNER.md` section 4).

use std::ops::RangeInclusive;

use egui::{
    Align, Align2, Color32, CursorIcon, Frame, Id, Layout, Margin, Painter, Pos2, Rect, Response,
    Role, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::button::{FaceState, ICON_STROKE, Icon, control_painter, paint_icon};
use crate::flat::{paint_control, paint_focus_ring};
use crate::text::{self, TextKind};
use crate::theme::with_alpha;
use crate::value_edit::{begin_edit, parse_int, readout, wheel_steps};

/// Keycap: 16 h, padding 0 4, `panel_2`, `border_2`, radius 3, hint type.
pub const KEYCAP_HEIGHT: f32 = 16.0;
pub const KEYCAP_PADDING: f32 = 4.0;
/// LED diameter.
pub const LED_SIZE: f32 = 10.0;
/// Tag: 16 h, padding 0 5, radius 3.
pub const TAG_HEIGHT: f32 = 16.0;
pub const TAG_PADDING: f32 = 5.0;
/// Chip: 10 x 10, radius 2.
pub const CHIP_SIZE: f32 = 10.0;
/// Insert number box: 22 x 22, radius 3.
pub const NUMBER_BOX: f32 = 22.0;
/// Name button: 80 x 24 (76 in the rack, 64 at narrow), radius 4, colour bar 4.
pub const NAME_BUTTON: Vec2 = Vec2::new(80.0, 24.0);
pub const NAME_BUTTON_RACK: Vec2 = Vec2::new(76.0, 24.0);
pub const NAME_BAR: f32 = 4.0;
/// Pattern tile: 24 x 24, radius 3.
pub const PATTERN_TILE: f32 = 24.0;
/// Text field: 28 h, padding 0 8, radius 4.
pub const FIELD_HEIGHT: f32 = 28.0;
pub const FIELD_PADDING: f32 = 8.0;
/// Pixels of vertical drag per step of a number box.
pub const NUMBER_BOX_DRAG_PX: f32 = 4.0;

// ---------------------------------------------------------------------------------------------
// Keycap.

/// The size of a keycap for `text`.
pub fn keycap_size(painter: &Painter, text: &str) -> Vec2 {
    let galley = text::layout(painter, text, TextKind::Hint, Color32::PLACEHOLDER);
    vec2(galley.size().x + 2.0 * KEYCAP_PADDING, KEYCAP_HEIGHT)
}

/// Paints a keycap with its left edge at `left` (vertically centred on `left.y`). Returns its
/// rect. `face` / `border` / `colour` default to `panel_2` / `border_2` / `text_3`.
pub fn paint_keycap(painter: &Painter, left: Pos2, text: &str, theme: &Theme) -> Rect {
    paint_keycap_coloured(
        painter,
        left,
        text,
        theme.panel_2,
        theme.border_2,
        theme.text_3,
    )
}

/// [`paint_keycap`] with explicit colours (the black piano key's keycap).
pub fn paint_keycap_coloured(
    painter: &Painter,
    left: Pos2,
    text: &str,
    face: Color32,
    border: Color32,
    colour: Color32,
) -> Rect {
    let galley = text::layout(painter, text, TextKind::Hint, colour);
    let size = vec2(galley.size().x + 2.0 * KEYCAP_PADDING, KEYCAP_HEIGHT);
    let rect = Rect::from_min_size(pos2(left.x, left.y - size.y * 0.5), size);
    paint_control(painter, rect, 3.0, face, border);
    text::paint_galley(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        galley,
        TextKind::Hint,
        colour,
    );
    rect
}

/// A keycap widget ("F5", "Ctrl+S"): accessible as a label reading `text`.
pub fn keycap(ui: &mut Ui, theme: &Theme, text: &str) -> Response {
    let size = keycap_size(ui.painter(), text);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_keycap(ui.painter(), rect.left_center(), text, theme);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    response
}

// ---------------------------------------------------------------------------------------------
// LED, tag, chip.

/// The colour of a lit LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedKind {
    /// `ok` + (ok 60 % black): mute LED audible, MIDI lamp.
    Ok,
    /// `accent` + `accent_press`: limiter, MODIFIED.
    Accent,
    /// `rec` + (rec 60 % black): record.
    Rec,
}

/// Paints a 10 pt LED centred on `centre`: on = the kind's colour and border, off = `panel_4`
/// + `border_2`.
pub fn paint_led(painter: &Painter, centre: Pos2, on: bool, kind: LedKind, theme: &Theme) {
    let (fill, border) = if on {
        match kind {
            LedKind::Ok => (theme.ok, theme.led_on_border()),
            LedKind::Accent => (theme.accent, theme.accent_press),
            LedKind::Rec => (theme.rec, theme.led_rec_border()),
        }
    } else {
        (theme.panel_4, theme.border_2)
    };
    painter.circle(centre, LED_SIZE * 0.5, fill, Stroke::new(1.0, border));
}

/// A 10 pt LED with a 1 px border. Senses clicks (the rack's mute LED) and reports its state
/// as a check box named `name`.
pub fn led_named(ui: &mut Ui, theme: &Theme, on: bool, kind: LedKind, name: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(LED_SIZE, LED_SIZE), Sense::click());
    if ui.is_rect_visible(rect) {
        paint_led(&control_painter(ui), rect.center(), on, kind, theme);
    }
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, ui.is_enabled(), on, name));
    response
}

/// A 10 pt LED named "LED".
pub fn led(ui: &mut Ui, theme: &Theme, on: bool, kind: LedKind) -> Response {
    led_named(ui, theme, on, kind, "LED")
}

/// Which colour a tag takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// `border_2`, `text_2`.
    Plain,
    /// Track 7's colour with the border at 50 %.
    Vinyl,
    /// Track 4's colour with the border at 50 %.
    Drone,
    /// An explicit colour with the border at 50 %.
    Colour(Color32),
}

impl TagKind {
    /// The text / border hue of this kind.
    pub fn colour(self, theme: &Theme) -> Color32 {
        match self {
            TagKind::Plain => theme.text_2,
            TagKind::Vinyl => theme.track_colour(6),
            TagKind::Drone => theme.track_colour(3),
            TagKind::Colour(c) => c,
        }
    }
}

/// A tag: 16 h, padding 0 5, radius 3, 11 / 500 text; `vinyl` / `drone` in their track
/// colours with the border at 50 % alpha.
pub fn tag(ui: &mut Ui, theme: &Theme, text: &str, kind: TagKind) -> Response {
    let galley = text::layout(ui.painter(), text, TextKind::Small, Color32::PLACEHOLDER);
    let size = vec2(galley.size().x + 2.0 * TAG_PADDING, TAG_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let colour = kind.colour(theme);
        let border = if kind == TagKind::Plain {
            theme.border_2
        } else {
            with_alpha(colour, 0.5)
        };
        paint_control(&painter, rect, 3.0, Color32::TRANSPARENT, border);
        text::paint_galley(
            &painter,
            rect.center(),
            Align2::CENTER_CENTER,
            galley,
            TextKind::Small,
            colour,
        );
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    response
}

/// A 10 x 10 radius-2 colour chip.
pub fn chip(ui: &mut Ui, theme: &Theme, colour: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(CHIP_SIZE, CHIP_SIZE), Sense::hover());
    if ui.is_rect_visible(rect) {
        control_painter(ui).rect_filled(rect, theme.radius_chip, colour);
    }
    response
}

/// Paints a `w` x `h` radius-2 colour bar (strip / name-button colour bars).
pub fn paint_colour_bar(painter: &Painter, rect: Rect, colour: Color32, theme: &Theme) {
    painter.rect_filled(rect, theme.radius_chip, colour);
}

// ---------------------------------------------------------------------------------------------
// Number box.

/// The insert number box: 22 x 22, radius 3, `panel_2`, `border_2`, 11 / 600 text centred.
/// A vertical drag steps the value ([`NUMBER_BOX_DRAG_PX`] per step), the wheel steps it,
/// a click opens the inline editor (Enter commits, clamped to `range`). `changed()` when the
/// value moved. Accessible as a button named "Insert" whose value is the number.
pub fn number_box(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut u8,
    range: RangeInclusive<u8>,
) -> Response {
    number_box_named(ui, theme, value, range, "Insert")
}

/// [`number_box`] with an explicit accessible name ("Insert of Kick").
pub fn number_box_named(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut u8,
    range: RangeInclusive<u8>,
    name: &str,
) -> Response {
    let (lo, hi) = (*range.start(), *range.end());
    *value = (*value).clamp(lo, hi);
    let before = *value;
    let (rect, mut response) =
        ui.allocate_exact_size(vec2(NUMBER_BOX, NUMBER_BOX), Sense::click_and_drag());
    let drag_id: Id = response.id.with("drag_acc");

    // Drag: accumulate vertical pointer travel, one step per NUMBER_BOX_DRAG_PX (up = more).
    if response.dragged() {
        let dy = response.drag_delta().y;
        let mut acc = ui.data(|d| d.get_temp::<f32>(drag_id)).unwrap_or(0.0) - dy;
        let steps = (acc / NUMBER_BOX_DRAG_PX).trunc();
        acc -= steps * NUMBER_BOX_DRAG_PX;
        ui.data_mut(|d| d.insert_temp(drag_id, acc));
        let v = i32::from(*value) + steps as i32;
        *value = v.clamp(i32::from(lo), i32::from(hi)) as u8;
    } else if response.drag_stopped() {
        ui.data_mut(|d| d.remove_temp::<f32>(drag_id));
    }
    let wheel = wheel_steps(ui, &response);
    if wheel != 0 {
        let v = i32::from(*value) + wheel;
        *value = v.clamp(i32::from(lo), i32::from(hi)) as u8;
    }

    let text_now = value.to_string();
    let readout_id = response.id.with("readout");
    if response.clicked() && !response.dragged() {
        begin_edit(ui, readout_id, &text_now);
    }
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let state = FaceState::of(&response);
        let (fill, border) = if state.hovered || state.pressed {
            (theme.panel_3, theme.border_2)
        } else {
            (theme.panel_2, theme.border_2)
        };
        paint_control(&painter, rect, 3.0, fill, border);
        if state.focused {
            paint_focus_ring(&painter, rect, 3.0, theme);
        }
    }
    // The box is the named button (its value is the number); the readout under it paints the
    // text or the editor and is named "<name> value".
    let shown = text_now.clone();
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Button, ui.is_enabled(), name);
        info.current_text_value = Some(shown.clone());
        info
    });
    let typed = readout(
        ui,
        theme,
        readout_id,
        rect.center(),
        Align2::CENTER_CENTER,
        &text_now,
        egui::FontId::proportional(11.0),
        theme.text,
        &format!("{name} value"),
    );
    if let Some(text) = typed
        && let Some(parsed) = parse_int(&text, i32::from(lo), i32::from(hi))
    {
        *value = parsed as u8;
    }
    if *value != before {
        response.mark_changed();
    }
    response.on_hover_cursor(CursorIcon::ResizeVertical)
}

// ---------------------------------------------------------------------------------------------
// Name button, pattern tile.

/// The channel name button: 76 x 24 ([`NAME_BUTTON_RACK`]), radius 4, `panel_3` face,
/// `border_2`, 12 / 600 left-aligned text (padding 0 6 0 10, ellipsised), a 4 px colour bar
/// at the left edge (`text_3` when `colour` is `None`: drums); hover `panel_4`; `selected`
/// lights the toggle ON treatment (`accent_wash`, `accent` border). `muted` greys the text.
pub fn name_button(
    ui: &mut Ui,
    theme: &Theme,
    text: &str,
    colour: Option<Color32>,
    selected: bool,
) -> Response {
    name_button_sized(ui, theme, text, colour, selected, false, NAME_BUTTON_RACK)
}

/// [`name_button`] of an explicit size with the muted (text-2) look.
pub fn name_button_sized(
    ui: &mut Ui,
    theme: &Theme,
    text: &str,
    colour: Option<Color32>,
    selected: bool,
    muted: bool,
    size: Vec2,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let state = FaceState::of(&response);
        let (fill, border) = if selected {
            (theme.accent_wash, theme.accent)
        } else if state.pressed {
            (theme.panel_2, theme.border)
        } else if state.hovered {
            (theme.panel_4, theme.border_2)
        } else {
            (theme.panel_3, theme.border_2)
        };
        paint_control(&painter, rect, 4.0, fill, border);
        if state.focused {
            paint_focus_ring(&painter, rect, 4.0, theme);
        }
        let bar = Rect::from_min_max(
            pos2(rect.min.x + 1.0, rect.min.y + 1.0),
            pos2(rect.min.x + 1.0 + NAME_BAR, rect.max.y - 1.0),
        );
        painter.rect_filled(
            bar,
            egui::CornerRadius {
                nw: 3,
                sw: 3,
                ne: 0,
                se: 0,
            },
            colour.unwrap_or(theme.text_3),
        );
        let text_colour = if selected {
            theme.accent
        } else if muted {
            theme.text_2
        } else {
            theme.text
        };
        let max_w = (rect.width() - 10.0 - 6.0 - NAME_BAR).max(0.0);
        let shown = ellipsise(&painter, text, TextKind::Button, max_w);
        let galley = text::layout(&painter, &shown, TextKind::Button, text_colour);
        text::paint_galley(
            &painter,
            pos2(
                rect.min.x + 1.0 + NAME_BAR + 10.0 - NAME_BAR,
                rect.center().y,
            ),
            Align2::LEFT_CENTER,
            galley,
            TextKind::Button,
            text_colour,
        );
    }
    let name = text.to_owned();
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, &name));
    response
}

/// `text` shortened with "…" so it lays out within `max_width` at `kind`.
pub fn ellipsise(painter: &Painter, text: &str, kind: TextKind, max_width: f32) -> String {
    let fits = |s: &str| {
        text::layout(painter, s, kind, Color32::PLACEHOLDER)
            .size()
            .x
            <= max_width
    };
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    for n in (1..chars.len()).rev() {
        let candidate: String = chars[..n].iter().collect::<String>() + "…";
        if fits(&candidate) {
            return candidate;
        }
    }
    "…".to_owned()
}

/// A pattern tile (A..H): 24 x 24, radius 3, `panel_2`, `border_2`, 12 / 600 `text_2`;
/// `has_notes` = `text`; `current` = `accent` / `accent_press` / `on_accent`.
pub fn pattern_tile(
    ui: &mut Ui,
    theme: &Theme,
    letter: &str,
    has_notes: bool,
    current: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(PATTERN_TILE, PATTERN_TILE), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let state = FaceState::of(&response);
        let (fill, border, colour) = if current {
            (theme.accent, theme.accent_press, theme.on_accent)
        } else {
            let fill = if state.hovered {
                theme.panel_3
            } else {
                theme.panel_2
            };
            let colour = if has_notes { theme.text } else { theme.text_2 };
            (fill, theme.border_2, colour)
        };
        paint_control(&painter, rect, 3.0, fill, border);
        if state.focused {
            paint_focus_ring(&painter, rect, 3.0, theme);
        }
        text::paint_text(
            &painter,
            rect.center(),
            Align2::CENTER_CENTER,
            letter,
            TextKind::Button,
            colour,
        );
    }
    let name = format!("Pattern {letter}");
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), current, &name));
    response
}

// ---------------------------------------------------------------------------------------------
// Text field.

/// The Deck text field: 28 h, padding 0 8, `panel_2`, `border_2`, radius 4, 13 pt text, an
/// optional leading 14 pt `text_2` icon, `placeholder` in `text_3`; focus = `accent` border
/// plus a 1 px accent ring outside. `width` is the whole control's width. The response is the
/// inner `TextEdit`'s (`changed()`, `lost_focus()`, `has_focus()` as usual).
pub fn text_field(
    ui: &mut Ui,
    theme: &Theme,
    text: &mut String,
    placeholder: &str,
    icon: Option<Icon>,
    width: f32,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, FIELD_HEIGHT), Sense::hover());
    let widget_id = ui.make_persistent_id(("neu_text_field_edit", rect.min.x as i32));
    let focused = ui.memory(|m| m.has_focus(widget_id));
    let where_face = ui.painter().add(egui::Shape::Noop);
    let mut x = rect.min.x + FIELD_PADDING;
    if let Some(icon) = icon {
        let icon_rect = Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0));
        paint_icon(
            ui.painter(),
            icon,
            icon_rect,
            Stroke::new(ICON_STROKE, theme.text_2),
        );
        x += 14.0 + 6.0;
    }
    let inner = Rect::from_min_max(
        pos2(x, rect.min.y + 1.0),
        pos2(rect.max.x - FIELD_PADDING, rect.max.y - 1.0),
    );
    let response = ui
        .scope_builder(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.text_cursor.stroke = Stroke::new(1.0, theme.text);
                visuals.selection.bg_fill = theme.accent_wash;
                ui.add(
                    egui::TextEdit::singleline(text)
                        .id(widget_id)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(egui::FontId::proportional(13.0))
                        .text_color(theme.text)
                        .hint_text(egui::RichText::new(placeholder).color(theme.text_3))
                        .align(Align2::LEFT_CENTER)
                        .desired_width(inner.width())
                        .min_size(inner.size()),
                )
            },
        )
        .inner;
    let name = if placeholder.is_empty() {
        "Text field".to_owned()
    } else {
        placeholder.to_owned()
    };
    let shown = text.clone();
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::TextInput, true, &name);
        info.current_text_value = Some(shown.clone());
        info
    });
    let focused = focused || response.has_focus();
    let border = if focused {
        theme.accent
    } else {
        theme.border_2
    };
    ui.painter().set(
        where_face,
        crate::flat::flat_rect_shape(rect, 4.0, theme.panel_2, border),
    );
    if focused {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            5.0,
            Stroke::new(1.0, theme.accent),
            StrokeKind::Outside,
        );
    }
    response
}
