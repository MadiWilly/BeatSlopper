//! Buttons (`design/round3/WINNER.md` section 4, "Buttons" and "Switch").
//!
//! Every button is a flat face with a 1 px border and a 12 / 600 caps label (or a 16 pt glyph
//! at stroke 1.5), 28 tall (small 22, xs 24), radius 4 (small 3):
//!
//! * secondary: `panel_3` / `border_2`; hover `panel_4`; pressed `panel_2` + `border`.
//! * primary: `accent` face, `accent_press` border, `on_accent` text; hover `accent_hover`;
//!   pressed `accent_press`.
//! * toggle ON: `accent_wash` face, `accent` border + text. Solo ON: `ok` @ 16 %, `ok` border
//!   + text. Record-armed ON: `rec` @ 16 %, `rec` border + glyph.
//! * ghost: transparent face and border; hover `panel_3` + `border`; same ON states.
//! * disabled: the whole control at 45 % alpha. Focus: a 2 px accent ring 1 px outside.
//!
//! The switch (40 x 22, radius 11) is the light / dark toggle and any boolean option.

use std::f32::consts::TAU;

use egui::epaint::Shape;
use egui::{
    Align2, Color32, Painter, Rect, Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use crate::flat::{self, clamp_radius, paint_control, paint_focus_ring};
use crate::text::TextKind;
use crate::theme::{DISABLED_ALPHA, with_alpha};
use crate::{Theme, ThemeMode};

/// Line icons drawn by the painter (stroke 1.5, round caps, in a 16-unit box; play / stop /
/// rec / dots are filled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Play,
    Stop,
    Record,
    Sun,
    Moon,
    Minus,
    Plus,
    /// A cross: clear / close.
    Clear,
    /// A clock face: the RECENT projects section.
    Clock,
    Undo,
    Redo,
    /// Three lines: the project menu.
    Menu,
    ChevronDown,
    ChevronUp,
    ChevronLeft,
    ChevronRight,
    Search,
    Link,
    Copy,
    Paste,
    /// Three dots: the overflow menu.
    Dots,
    Folder,
    File,
    /// Note bars: the piano roll view (F5).
    Roll,
    /// Three faders: the mixer (F6).
    Mixer,
    /// A waveform: synth & fx (F7).
    Synth,
    /// Two blocks: the song view (F8).
    Song,
    /// A list: the browser (F9).
    Browser,
    /// Four corners: full screen (F11).
    Fullscreen,
    /// A check mark.
    Check,
    /// A window with an arrow leaving it at the top right: pop a module out to its own window.
    PopOut,
    /// A window with an arrow coming back into it: dock a module again.
    Dock,
}

/// Which look a button has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// `panel_3` face, `border_2`.
    Secondary,
    /// `accent` face, `on_accent` text.
    Primary,
    /// Transparent until hovered.
    Ghost,
    /// Secondary that lights `accent_wash` / `accent` when ON.
    Toggle(bool),
    /// Toggle in the `ok` colour (solo).
    Solo(bool),
    /// Toggle in the `rec` colour (record arm).
    Rec(bool),
    /// Ghost that lights like a toggle when ON (the view switcher, the rail).
    GhostToggle(bool),
}

impl ButtonKind {
    /// Whether this kind is lit (a toggle that is ON).
    pub fn is_on(self) -> bool {
        matches!(
            self,
            ButtonKind::Toggle(true)
                | ButtonKind::Solo(true)
                | ButtonKind::Rec(true)
                | ButtonKind::GhostToggle(true)
        )
    }

    /// The ON state's hue (`accent`, `ok` or `rec`), if this kind is lit.
    fn on_hue(self, theme: &Theme) -> Option<Color32> {
        match self {
            ButtonKind::Toggle(true) | ButtonKind::GhostToggle(true) => Some(theme.accent),
            ButtonKind::Solo(true) => Some(theme.ok),
            ButtonKind::Rec(true) => Some(theme.rec),
            _ => None,
        }
    }

    fn is_ghost(self) -> bool {
        matches!(self, ButtonKind::Ghost | ButtonKind::GhostToggle(_))
    }
}

/// Button heights: regular 28 (padding 0 10, 12 pt), small 22 (0 5, 11 pt, radius 3), xs 24
/// (0 6, 11 pt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    Regular,
    Small,
    Xs,
}

impl ButtonSize {
    pub fn height(self) -> f32 {
        match self {
            ButtonSize::Regular => 28.0,
            ButtonSize::Small => 22.0,
            ButtonSize::Xs => 24.0,
        }
    }

    pub fn padding_x(self) -> f32 {
        match self {
            ButtonSize::Regular => 10.0,
            ButtonSize::Small => 5.0,
            ButtonSize::Xs => 6.0,
        }
    }

    pub fn radius(self) -> f32 {
        match self {
            ButtonSize::Regular | ButtonSize::Xs => 4.0,
            ButtonSize::Small => 3.0,
        }
    }

    /// Text size (12 regular, 11 small / xs).
    pub fn text_size(self) -> f32 {
        match self {
            ButtonSize::Regular => 12.0,
            ButtonSize::Small | ButtonSize::Xs => 11.0,
        }
    }

    /// Glyph box of an icon button of this size (16 regular, 12 small / xs).
    pub fn glyph(self) -> f32 {
        match self {
            ButtonSize::Regular => 16.0,
            ButtonSize::Small | ButtonSize::Xs => 12.0,
        }
    }
}

/// Height of a regular button (28); the old `.pill` name kept for the app.
pub const PILL_HEIGHT: f32 = 28.0;
/// Height of a regular button.
pub const BUTTON_HEIGHT: f32 = 28.0;
/// Size of the square mixer toggles ("M" / "S") drawn with [`toggle_button_sized`].
pub const TOGGLE_SQUARE: Vec2 = Vec2::new(28.0, 28.0);
/// Corner radius of [`toggle_button_sized`] (Deck: 4).
pub const TOGGLE_SIZED_RADIUS: f32 = 4.0;
/// Soft Console depth threshold; unused by Deck, kept for the app.
pub const SMALL_DEPTH_MAX: f32 = 32.0;
/// Line width of icon glyphs.
pub const ICON_STROKE: f32 = 1.5;
/// Glyph box of a 28 pt icon button.
pub const ICON_BOX: f32 = 16.0;
/// Size of the switch track (40 x 22) and its knob (16).
pub const TOGGLE_TRACK: Vec2 = Vec2::new(40.0, 22.0);
pub const TOGGLE_KNOB: f32 = 16.0;
/// Size of the small switch (34 x 20) and its knob (14).
pub const TOGGLE_TRACK_SMALL: Vec2 = Vec2::new(34.0, 20.0);
pub const TOGGLE_KNOB_SMALL: f32 = 14.0;
/// Size of a rail / window-header icon button (28, with a 14 glyph).
pub const RAIL_BUTTON: f32 = 28.0;
pub const RAIL_GLYPH: f32 = 14.0;

/// The interaction state a face is painted for.
#[derive(Debug, Clone, Copy, Default)]
pub struct FaceState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

impl FaceState {
    /// The state of `response` (focus counts only while the pointer is not pressing it).
    pub fn of(response: &Response) -> Self {
        let pressed = response.is_pointer_button_down_on();
        Self {
            hovered: response.hovered(),
            pressed,
            focused: response.has_focus() && !pressed && !response.hovered(),
        }
    }
}

/// Paints a button face of `kind` in `state` into `rect` and returns the text / glyph colour.
pub fn paint_button_face(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    kind: ButtonKind,
    state: FaceState,
    theme: &Theme,
) -> Color32 {
    let (fill, border, fg) = match kind {
        ButtonKind::Primary => {
            let fill = if state.pressed {
                theme.accent_press
            } else if state.hovered {
                theme.accent_hover
            } else {
                theme.accent
            };
            (fill, theme.accent_press, theme.on_accent)
        }
        _ => match kind.on_hue(theme) {
            Some(hue) => {
                let wash = if hue == theme.accent {
                    theme.accent_wash
                } else {
                    with_alpha(hue, 0.16)
                };
                let fill = if state.hovered && !state.pressed {
                    wash.blend(with_alpha(theme.panel_4, 0.5))
                } else {
                    wash
                };
                (fill, hue, hue)
            }
            None if kind.is_ghost() => {
                if state.pressed {
                    (theme.panel_2, theme.border, theme.text)
                } else if state.hovered {
                    (theme.panel_3, theme.border, theme.text)
                } else {
                    (Color32::TRANSPARENT, Color32::TRANSPARENT, theme.text_2)
                }
            }
            None => {
                if state.pressed {
                    (theme.panel_2, theme.border, theme.text)
                } else if state.hovered {
                    (theme.panel_4, theme.border_2, theme.text)
                } else {
                    (theme.panel_3, theme.border_2, theme.text)
                }
            }
        },
    };
    paint_control(painter, rect, radius, fill, border);
    if state.focused {
        paint_focus_ring(painter, rect, radius, theme);
    }
    fg
}

/// A painter for a control: the ui's painter at 45 % opacity when the ui is disabled.
pub fn control_painter(ui: &Ui) -> Painter {
    let mut painter = ui.painter().clone();
    if !ui.is_enabled() {
        painter.multiply_opacity(DISABLED_ALPHA);
    }
    painter
}

/// Secondary text button: 28 h, min 28 w, padding 0 10, 12 / 600 caps.
pub fn button(ui: &mut Ui, theme: &Theme, label: &str) -> Response {
    button_kind(ui, theme, label, ButtonKind::Secondary, ButtonSize::Regular)
}

/// Primary text button (accent face, on-accent text).
pub fn primary_button(ui: &mut Ui, theme: &Theme, label: &str) -> Response {
    button_kind(ui, theme, label, ButtonKind::Primary, ButtonSize::Regular)
}

/// Ghost text button (transparent until hovered).
pub fn ghost_button(ui: &mut Ui, theme: &Theme, label: &str) -> Response {
    button_kind(ui, theme, label, ButtonKind::Ghost, ButtonSize::Regular)
}

/// Small (22 h) secondary text button.
pub fn small_button(ui: &mut Ui, theme: &Theme, label: &str) -> Response {
    button_kind(ui, theme, label, ButtonKind::Secondary, ButtonSize::Small)
}

/// Text button that shows the toggle ON state (`accent_wash` face, accent border + text).
pub fn toggle_button(ui: &mut Ui, theme: &Theme, label: &str, on: bool) -> Response {
    button_kind(
        ui,
        theme,
        label,
        ButtonKind::Toggle(on),
        ButtonSize::Regular,
    )
}

/// Solo toggle (`ok` hue when ON).
pub fn solo_button(ui: &mut Ui, theme: &Theme, label: &str, on: bool) -> Response {
    button_kind(ui, theme, label, ButtonKind::Solo(on), ButtonSize::Regular)
}

/// Record-arm toggle (`rec` hue when ON): a 28 square with the record glyph.
pub fn rec_button(ui: &mut Ui, theme: &Theme, on: bool) -> Response {
    icon_button_kind(
        ui,
        theme,
        Icon::Record,
        vec2(28.0, 28.0),
        ButtonKind::Rec(on),
    )
}

/// Toggle button of an explicit `size` (e.g. [`TOGGLE_SQUARE`], 28 x 28, for the mixer's "M"
/// / "S"): a radius-4 face with the toggle ON treatment when `on` and the 12 / 600 caps label
/// centred. Sizes under 24 tall take the small (11 pt, radius 3) type.
pub fn toggle_button_sized(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    on: bool,
    size: Vec2,
) -> Response {
    let button_size = if size.y < 24.0 {
        ButtonSize::Small
    } else {
        ButtonSize::Regular
    };
    text_button(
        ui,
        theme,
        label,
        ButtonKind::Toggle(on),
        button_size,
        Some(size),
    )
}

/// The general text button: `label` in `kind` at `size`, as wide as the label plus padding
/// (never narrower than the height).
pub fn button_kind(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    kind: ButtonKind,
    size: ButtonSize,
) -> Response {
    text_button(ui, theme, label, kind, size, None)
}

/// [`button_kind`] that fills `width` (a full-width primary EXPORT WAV, a grid button).
pub fn button_wide(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    kind: ButtonKind,
    size: ButtonSize,
    width: f32,
) -> Response {
    text_button(
        ui,
        theme,
        label,
        kind,
        size,
        Some(vec2(width, size.height())),
    )
}

fn text_button(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    kind: ButtonKind,
    size: ButtonSize,
    exact: Option<Vec2>,
) -> Response {
    let galley = layout_button_text(ui.painter(), label, size, Color32::PLACEHOLDER);
    let desired = exact.unwrap_or_else(|| {
        vec2(
            (galley.size().x + 2.0 * size.padding_x()).max(size.height()),
            size.height(),
        )
    });
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let radius = size.radius();
        let colour = paint_button_face(
            &painter,
            rect,
            radius,
            kind,
            FaceState::of(&response),
            theme,
        );
        let text_rect = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size());
        painter.galley(text_rect.min + vec2(0.5, 0.0), galley.clone(), colour);
        painter.galley(text_rect.min, galley, colour);
    }
    let name = label.to_owned();
    response.widget_info(|| match kind {
        ButtonKind::Toggle(on)
        | ButtonKind::Solo(on)
        | ButtonKind::Rec(on)
        | ButtonKind::GhostToggle(on) => {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), on, &name)
        }
        _ => WidgetInfo::labeled(Role::Button, ui.is_enabled(), &name),
    });
    response
}

/// Lays out a button label: caps, tracked +0.04em, 12 (regular) or 11 (small / xs) pt.
pub fn layout_button_text(
    painter: &Painter,
    label: &str,
    size: ButtonSize,
    colour: Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut format = TextKind::Button.format(colour);
    format.font_id = egui::FontId::proportional(size.text_size());
    format.extra_letter_spacing = 0.04 * size.text_size();
    painter.layout_job(egui::epaint::text::LayoutJob::single_section(
        TextKind::Button.display_string(label),
        format,
    ))
}

/// Square icon button of side `diameter` (28: glyph 16; 24 / 22: glyph 12), secondary face.
/// The name "diameter" is the Soft Console signature; Deck draws a radius-4 square.
pub fn icon_button(ui: &mut Ui, theme: &Theme, icon: Icon, diameter: f32) -> Response {
    icon_button_kind(
        ui,
        theme,
        icon,
        vec2(diameter, diameter),
        ButtonKind::Secondary,
    )
}

/// Icon button of `size` in `kind` (primary PLAY, ghost undo / redo, toggle views, rec arm).
pub fn icon_button_kind(
    ui: &mut Ui,
    theme: &Theme,
    icon: Icon,
    size: Vec2,
    kind: ButtonKind,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let radius = if size.min_elem() < 24.0 { 3.0 } else { 4.0 };
        let colour = paint_button_face(
            &painter,
            rect,
            radius,
            kind,
            FaceState::of(&response),
            theme,
        );
        let glyph = glyph_box_for(size.min_elem());
        let icon_rect = Rect::from_center_size(rect.center(), vec2(glyph, glyph));
        paint_icon(&painter, icon, icon_rect, Stroke::new(ICON_STROKE, colour));
    }
    response.widget_info(|| match kind {
        ButtonKind::Toggle(on)
        | ButtonKind::Solo(on)
        | ButtonKind::Rec(on)
        | ButtonKind::GhostToggle(on) => {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), on, icon.name())
        }
        _ => WidgetInfo::labeled(Role::Button, ui.is_enabled(), icon.name()),
    });
    response
}

/// The glyph box of an icon button of side `side`: 16 at 28 and up, 14 for the rail, 12 for
/// the small / header buttons.
pub fn glyph_box_for(side: f32) -> f32 {
    if side >= 28.0 {
        ICON_BOX
    } else if side >= 26.0 {
        RAIL_GLYPH
    } else {
        12.0
    }
}

/// A rail / view-switcher button: 28 x 28 ghost icon button with a 14 glyph that lights as a
/// toggle when `on`, named `tooltip` for accessibility and shown as its hover tooltip.
pub fn rail_button(ui: &mut Ui, theme: &Theme, icon: Icon, on: bool, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(RAIL_BUTTON, RAIL_BUTTON), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let colour = paint_button_face(
            &painter,
            rect,
            4.0,
            ButtonKind::GhostToggle(on),
            FaceState::of(&response),
            theme,
        );
        let icon_rect = Rect::from_center_size(rect.center(), vec2(RAIL_GLYPH, RAIL_GLYPH));
        paint_icon(&painter, icon, icon_rect, Stroke::new(ICON_STROKE, colour));
    }
    let name = if tooltip.is_empty() {
        icon.name().to_owned()
    } else {
        tooltip.to_owned()
    };
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, &name));
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// A 24 x 24 ghost icon button with a 12 glyph: the window header's keycap-side controls
/// (collapse / close) and the browser's small ghost icons.
pub fn header_button(ui: &mut Ui, theme: &Theme, icon: Icon) -> Response {
    icon_button_kind(ui, theme, icon, vec2(24.0, 24.0), ButtonKind::Ghost)
}

/// Light / dark switch: a 40 x 22 [`switch`] that is ON in the dark mode. Flips `mode` when
/// clicked. Accessible as the "Dark theme" switch.
pub fn theme_toggle(ui: &mut Ui, theme: &Theme, mode: &mut ThemeMode) -> Response {
    let mut dark = *mode == ThemeMode::Dark;
    let response = switch(ui, theme, &mut dark, "Dark theme");
    *mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    response
}

/// The switch (40 x 22, radius 11, `panel_2` track, `border_2`, 16 pt `text_2` knob at
/// (2, 2); ON: `accent_wash` track, `accent` border, `accent` knob at x 20). Flips `on` when
/// clicked; `changed()` when it did. Accessible as a switch named `name`.
pub fn switch(ui: &mut Ui, theme: &Theme, on: &mut bool, name: &str) -> Response {
    switch_sized(ui, theme, on, name, TOGGLE_TRACK, TOGGLE_KNOB)
}

/// The small switch (34 x 20, knob 14) of the mixer side column.
pub fn switch_small(ui: &mut Ui, theme: &Theme, on: &mut bool, name: &str) -> Response {
    switch_sized(ui, theme, on, name, TOGGLE_TRACK_SMALL, TOGGLE_KNOB_SMALL)
}

fn switch_sized(
    ui: &mut Ui,
    theme: &Theme,
    on: &mut bool,
    name: &str,
    track_size: Vec2,
    knob: f32,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(track_size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let lit = *on;
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let radius = track_size.y * 0.5;
        let (fill, border, knob_fill) = if lit {
            (theme.accent_wash, theme.accent, theme.accent)
        } else {
            (theme.panel_2, theme.border_2, theme.text_2)
        };
        paint_control(&painter, rect, radius, fill, border);
        let t = ui
            .ctx()
            .animate_bool_with_time(response.id.with("knob"), lit, 0.12);
        let inset = (track_size.y - knob) * 0.5;
        let x0 = rect.min.x + inset + knob * 0.5;
        let x1 = rect.max.x - inset - knob * 0.5;
        let centre = pos2(egui::lerp(x0..=x1, t), rect.center().y);
        painter.circle_filled(centre, knob * 0.5, knob_fill);
        let state = FaceState::of(&response);
        if state.focused {
            paint_focus_ring(&painter, rect, radius, theme);
        }
    }
    let name = name.to_owned();
    response.widget_info(|| WidgetInfo::selected(Role::Switch, ui.is_enabled(), lit, &name));
    response
}

impl Icon {
    /// Accessible name of the icon (used as the button label for screen readers and tests).
    pub fn name(self) -> &'static str {
        match self {
            Icon::Play => "Play",
            Icon::Stop => "Stop",
            Icon::Record => "Record",
            Icon::Sun => "Light theme",
            Icon::Moon => "Dark theme",
            Icon::Minus => "Decrease",
            Icon::Plus => "Increase",
            Icon::Clear => "Clear",
            Icon::Clock => "Recent",
            Icon::Undo => "Undo",
            Icon::Redo => "Redo",
            Icon::Menu => "Menu",
            Icon::ChevronDown => "Expand",
            Icon::ChevronUp => "Collapse",
            Icon::ChevronLeft => "Previous",
            Icon::ChevronRight => "Next",
            Icon::Search => "Search",
            Icon::Link => "Link",
            Icon::Copy => "Copy",
            Icon::Paste => "Paste",
            Icon::Dots => "More",
            Icon::Folder => "Open",
            Icon::File => "File",
            Icon::Roll => "Piano roll",
            Icon::Mixer => "Mixer",
            Icon::Synth => "Synth & FX",
            Icon::Song => "Song",
            Icon::Browser => "Browser",
            Icon::Fullscreen => "Full screen",
            Icon::Check => "Check",
            Icon::PopOut => "Pop out",
            Icon::Dock => "Dock",
        }
    }
}

/// Draws `icon` as `stroke.width` lines (round caps) fitted to `rect`: the glyphs are defined
/// in a 16-unit box and scaled to `rect`. Play, stop, record and dots are filled.
pub fn paint_icon(painter: &Painter, icon: Icon, rect: Rect, stroke: Stroke) {
    let s = rect.width().min(rect.height()) / 16.0;
    let origin = rect.center() - vec2(8.0 * s, 8.0 * s);
    let p = move |x: f32, y: f32| origin + vec2(x * s, y * s);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], stroke);
        painter.circle_filled(p(a.0, a.1), stroke.width * 0.5, stroke.color);
        painter.circle_filled(p(b.0, b.1), stroke.width * 0.5, stroke.color);
    };
    let polyline = |pts: &[(f32, f32)]| {
        for w in pts.windows(2) {
            line(w[0], w[1]);
        }
    };
    let fill = stroke.color;
    match icon {
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                vec![p(4.0, 2.5), p(4.0, 13.5), p(13.0, 8.0)],
                fill,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_min_max(p(3.5, 3.5), p(12.5, 12.5)),
                clamp_radius(rect, 1.0 * s),
                fill,
            );
        }
        Icon::Record => {
            painter.circle_filled(p(8.0, 8.0), 4.5 * s, fill);
        }
        Icon::Dots => {
            for x in [4.0, 8.0, 12.0] {
                painter.circle_filled(p(x, 8.0), 1.3 * s, fill);
            }
        }
        Icon::Sun => {
            painter.circle_stroke(p(8.0, 8.0), 3.0 * s, stroke);
            let rays: [[f32; 4]; 8] = [
                [8.0, 1.5, 8.0, 3.0],
                [8.0, 13.0, 8.0, 14.5],
                [1.5, 8.0, 3.0, 8.0],
                [13.0, 8.0, 14.5, 8.0],
                [3.4, 3.4, 4.4, 4.4],
                [11.6, 11.6, 12.6, 12.6],
                [3.4, 12.6, 4.4, 11.6],
                [11.6, 4.4, 12.6, 3.4],
            ];
            for [x0, y0, x1, y1] in rays {
                line((x0, y0), (x1, y1));
            }
        }
        Icon::Moon => {
            // M13 10.2 A5.5 5.5 0 0 1 5.8 3 a5.5 5.5 0 1 0 7.2 7.2 z
            let p1 = (5.8, 3.0);
            let p2 = (13.0, 10.2);
            let c1 = (8.0, 8.0);
            let c2 = (10.879_f32, 5.121_f32);
            let r = 5.5;
            let mut pts = Vec::with_capacity(40);
            let a1 = angle(c1, p1);
            let mut a2 = angle(c1, p2);
            if a2 > a1 {
                a2 -= TAU;
            }
            for i in 0..=24 {
                let a = a1 + (a2 - a1) * i as f32 / 24.0;
                pts.push(p(c1.0 + r * a.cos(), c1.1 + r * a.sin()));
            }
            let b1 = angle(c2, p2);
            let mut b2 = angle(c2, p1);
            if b2 < b1 {
                b2 += TAU;
            }
            for i in 1..16 {
                let a = b1 + (b2 - b1) * i as f32 / 16.0;
                pts.push(p(c2.0 + r * a.cos(), c2.1 + r * a.sin()));
            }
            painter.add(Shape::closed_line(pts, stroke));
        }
        Icon::Minus => line((3.0, 8.0), (13.0, 8.0)),
        Icon::Plus => {
            line((3.0, 8.0), (13.0, 8.0));
            line((8.0, 3.0), (8.0, 13.0));
        }
        Icon::Clear => {
            line((4.0, 4.0), (12.0, 12.0));
            line((12.0, 4.0), (4.0, 12.0));
        }
        Icon::Clock => {
            painter.circle_stroke(p(8.0, 8.0), 6.0 * s, stroke);
            line((8.0, 8.0), (8.0, 4.5));
            line((8.0, 8.0), (10.8, 9.5));
        }
        Icon::Undo | Icon::Redo => {
            // M6 4 L3 7 l3 3; M3 7 h6 a4 4 0 0 1 0 8 (mirrored for redo).
            let m = |x: f32| if icon == Icon::Redo { 16.0 - x } else { x };
            polyline(&[(m(6.0), 4.0), (m(3.0), 7.0), (m(6.0), 10.0)]);
            line((m(3.0), 7.0), (m(9.0), 7.0));
            let mut pts = Vec::with_capacity(14);
            for i in 0..=12 {
                let a = -0.25 * TAU + 0.5 * TAU * i as f32 / 12.0;
                pts.push(p(m(9.0 + 4.0 * a.cos()), 11.0 + 4.0 * a.sin()));
            }
            painter.add(Shape::line(pts, stroke));
        }
        Icon::Menu => {
            for y in [4.5, 8.0, 11.5] {
                line((3.0, y), (13.0, y));
            }
        }
        Icon::ChevronDown => polyline(&[(4.0, 6.0), (8.0, 10.0), (12.0, 6.0)]),
        Icon::ChevronUp => polyline(&[(4.0, 10.0), (8.0, 6.0), (12.0, 10.0)]),
        Icon::ChevronLeft => polyline(&[(10.0, 4.0), (6.0, 8.0), (10.0, 12.0)]),
        Icon::ChevronRight => polyline(&[(6.0, 4.0), (10.0, 8.0), (6.0, 12.0)]),
        Icon::Search => {
            painter.circle_stroke(p(7.0, 7.0), 4.5 * s, stroke);
            line((10.5, 10.5), (14.0, 14.0));
        }
        Icon::Link => {
            painter.circle_stroke(p(5.5, 10.5), 3.0 * s, stroke);
            painter.circle_stroke(p(10.5, 5.5), 3.0 * s, stroke);
            line((6.5, 9.5), (9.5, 6.5));
        }
        Icon::Copy => {
            painter.rect_stroke(
                Rect::from_min_max(p(6.0, 6.0), p(13.0, 13.0)),
                1.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            polyline(&[
                (4.0, 10.0),
                (3.0, 10.0),
                (3.0, 3.0),
                (10.0, 3.0),
                (10.0, 4.0),
            ]);
        }
        Icon::Paste => {
            painter.rect_stroke(
                Rect::from_min_max(p(4.0, 3.5), p(12.0, 14.0)),
                1.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.rect_filled(Rect::from_min_max(p(6.0, 2.0), p(10.0, 4.5)), 0.0, fill);
        }
        Icon::Folder => {
            polyline(&[
                (2.0, 13.0),
                (2.0, 3.5),
                (6.0, 3.5),
                (7.5, 5.0),
                (14.0, 5.0),
                (14.0, 13.0),
                (2.0, 13.0),
            ]);
        }
        Icon::File => {
            polyline(&[
                (4.0, 14.0),
                (4.0, 2.0),
                (9.5, 2.0),
                (12.5, 5.0),
                (12.5, 14.0),
                (4.0, 14.0),
            ]);
            polyline(&[(9.5, 2.0), (9.5, 5.0), (12.5, 5.0)]);
        }
        Icon::Roll => {
            for (x, y, w) in [(3.0, 4.0, 5.0), (6.0, 7.5, 7.0), (4.0, 11.0, 4.0)] {
                painter.rect_filled(
                    Rect::from_min_size(p(x, y), vec2(w * s, 2.0 * s)),
                    1.0 * s,
                    fill,
                );
            }
        }
        Icon::Mixer => {
            for (x, y) in [(4.0, 6.0), (8.0, 10.0), (12.0, 8.0)] {
                line((x, 2.5), (x, 13.5));
                painter.rect_filled(
                    Rect::from_center_size(p(x, y), vec2(3.5 * s, 2.0 * s)),
                    0.5 * s,
                    fill,
                );
            }
        }
        Icon::Synth => {
            let pts: Vec<Pos2Tuple> = (0..=16)
                .map(|i| {
                    let x = 2.0 + 12.0 * i as f32 / 16.0;
                    (x, 8.0 - 4.5 * (i as f32 / 16.0 * TAU).sin())
                })
                .collect();
            polyline(&pts);
        }
        Icon::Song => {
            painter.rect_stroke(
                Rect::from_min_max(p(2.0, 5.0), p(7.0, 11.0)),
                1.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(8.5, 5.0), p(14.0, 11.0)),
                1.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Browser => {
            for y in [4.0, 8.0, 12.0] {
                painter.circle_filled(p(3.5, y), 1.1 * s, fill);
                line((6.5, y), (13.0, y));
            }
        }
        Icon::Fullscreen => {
            polyline(&[(3.0, 6.0), (3.0, 3.0), (6.0, 3.0)]);
            polyline(&[(10.0, 3.0), (13.0, 3.0), (13.0, 6.0)]);
            polyline(&[(13.0, 10.0), (13.0, 13.0), (10.0, 13.0)]);
            polyline(&[(6.0, 13.0), (3.0, 13.0), (3.0, 10.0)]);
        }
        Icon::Check => polyline(&[(3.0, 8.5), (6.5, 12.0), (13.0, 4.5)]),
        Icon::PopOut => {
            // A window open at its top-right corner, the arrow leaving through the gap.
            polyline(&[
                (7.0, 3.0),
                (3.0, 3.0),
                (3.0, 13.0),
                (13.0, 13.0),
                (13.0, 9.0),
            ]);
            line((8.0, 8.0), (13.5, 2.5));
            polyline(&[(9.5, 2.5), (13.5, 2.5), (13.5, 6.5)]);
        }
        Icon::Dock => {
            polyline(&[
                (7.0, 3.0),
                (3.0, 3.0),
                (3.0, 13.0),
                (13.0, 13.0),
                (13.0, 9.0),
            ]);
            line((13.5, 2.5), (7.5, 8.5));
            polyline(&[(7.5, 4.5), (7.5, 8.5), (11.5, 8.5)]);
        }
    }
}

type Pos2Tuple = (f32, f32);

fn angle(center: (f32, f32), point: (f32, f32)) -> f32 {
    let a = (point.1 - center.1).atan2(point.0 - center.0);
    if a < 0.0 { a + TAU } else { a }
}

/// A 28 pt square secondary button with a 12 pt minus / plus (or any) glyph: the "-" / "+"
/// of the stepper and the BPM row, the clear button of a sequencer row.
pub fn stepper_button(ui: &mut Ui, theme: &Theme, icon: Icon) -> Response {
    const SIDE: f32 = 28.0;
    let (rect, response) = ui.allocate_exact_size(vec2(SIDE, SIDE), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let colour = paint_button_face(
            &painter,
            rect,
            4.0,
            ButtonKind::Secondary,
            FaceState::of(&response),
            theme,
        );
        let icon_rect = Rect::from_center_size(rect.center(), vec2(12.0, 12.0));
        paint_icon(&painter, icon, icon_rect, Stroke::new(ICON_STROKE, colour));
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), icon.name()));
    response
}

/// A 1 x 20 `border_2` separator in a toolbar row (allocates 1 x 20 plus the item spacing).
pub fn toolbar_separator(ui: &mut Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 20.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        flat::paint_separator(ui.painter(), rect.center().x, rect.center().y, 20.0, theme);
    }
}
