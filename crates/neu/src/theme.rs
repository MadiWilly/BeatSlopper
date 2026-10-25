//! Theme tokens transcribed from `design/round3/WINNER.md` (direction B, "Deck").
//!
//! These values are the source of truth. Do not tune them here; change the design file first.
//!
//! The Deck tokens are the fields named after the table in WINNER.md section 2 (`bg`, `panel`,
//! `panel_2` .. `panel_4`, `border`, `border_2`, `text`, `text_2`, `text_3`, `accent`,
//! `accent_hover`, `accent_press`, `on_accent`, `accent_wash`, `rec`, `ok`, `clip`,
//! `grid_beat`, `grid_bar`, `step_off`, `step_off_alt`, `on_track`, `tracks`, `shadow`) plus
//! the knob-face and black-key gradient pairs, the radii (4 / 3 / 2), the spacing scale and the
//! layout rhythm (control 28, row pitch 32, header 28, toolbar 40).
//!
//! The Soft Console field names (`surface`, `text_primary`, `text_secondary`,
//! `shadow_highlight`, `shadow_dark`, `glow`, `key_black`, the `ShadowPair`s, `radius_*`,
//! `panel_padding`) are kept as aliases mapped onto the Deck tones so the app keeps compiling
//! through M12; they are retired in M13.

use egui::Color32;
use egui::epaint::Shadow;

/// Light or dark palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn from_egui(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Light => ThemeMode::Light,
            egui::Theme::Dark => ThemeMode::Dark,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Light,
        }
    }
}

/// A soft shadow pair (offset, blur). Soft Console only: Deck draws no paired shadows, the
/// fields that hold these are kept so `crates/app` compiles and are retired in M13.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowPair {
    pub offset: f32,
    pub blur: f32,
}

/// All colour, shadow, radius and spacing tokens for one theme mode.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub mode: ThemeMode,

    // ----- Deck colour tokens (WINNER.md section 2) -----
    /// Window background, the 4 pt gutters between docks.
    pub bg: Color32,
    /// Window / dock body, toolbar, groups, strips, rows.
    pub panel: Color32,
    /// Window headers, fields, dropdowns, wells, scrollbar tracks, menus, ruler + key column.
    pub panel_2: Color32,
    /// Button face, row hover, ghost-button hover.
    pub panel_3: Color32,
    /// Button hover, selected segment, tooltip face, menu item hover, LED off, slot ON.
    pub panel_4: Color32,
    /// Hairlines: window frames, dividers, rows, step cells off, pressed buttons.
    pub border: Color32,
    /// Control borders: buttons, fields, knob track arc + face ring, faders, keycaps, tags.
    pub border_2: Color32,
    /// Primary text: values, titles, buttons, knob pointer.
    pub text: Color32,
    /// Labels, column heads, unselected segments / tabs, sub-titles.
    pub text_2: Color32,
    /// Disabled, keycaps, placeholders, hints, section labels, ruler numbers; never body text.
    pub text_3: Color32,
    /// ON step, primary button, selection bar, underline, playhead, knob value arc.
    pub accent: Color32,
    /// Primary hover; accent-step fill.
    pub accent_hover: Color32,
    /// Primary border, ON-step border, accent-step inset ring, LED accent border.
    pub accent_press: Color32,
    /// Text / icons on accent.
    pub on_accent: Color32,
    /// Accent at 16 % (dark) / 14 % (light): toggle ON face, selected row, switch ON track.
    pub accent_wash: Color32,
    /// Record-armed button and LED.
    pub rec: Color32,
    /// Solo ON, mute LED audible, MIDI lamp, meter body.
    pub ok: Color32,
    /// Meter top 12 % (= `rec`).
    pub clip: Color32,
    /// Roll step lines, row dividers, scope / EQ-curve grid.
    pub grid_beat: Color32,
    /// Roll beat and bar lines, C-row dividers.
    pub grid_bar: Color32,
    /// Step cell off, beats 2 and 4.
    pub step_off: Color32,
    /// Step cell off, beats 1 and 3.
    pub step_off_alt: Color32,
    /// Text on a track colour.
    pub on_track: Color32,
    /// The eight track colours (chips, name-button bars, note bars, ghosts).
    pub tracks: [Color32; TRACK_COLOURS],
    /// The one shadow: menus, tooltips, the floating drawer. **No other shadow anywhere.**
    pub shadow: Shadow,
    /// Knob face vertical gradient, top then bottom.
    pub knob_face: [Color32; 2],
    /// Black piano key vertical gradient, top then bottom.
    pub key_black_face: [Color32; 2],
    /// Black piano key border.
    pub key_black_border: Color32,
    /// Black piano key note name.
    pub key_black_text: Color32,
    /// Black piano key keycap: face, border, text.
    pub key_black_keycap: [Color32; 3],

    // ----- Soft Console aliases (retired in M13) -----
    /// = `panel`.
    pub surface: Color32,
    /// = `border`.
    pub shadow_highlight: Color32,
    /// = `border_2`.
    pub shadow_dark: Color32,
    /// = `text`.
    pub text_primary: Color32,
    /// = `text_2`.
    pub text_secondary: Color32,
    /// = the black piano key's top colour.
    pub key_black: Color32,
    /// = `accent_wash`.
    pub glow: Color32,
    pub raised_l: ShadowPair,
    pub raised_m: ShadowPair,
    pub raised_s: ShadowPair,
    pub inset_m: ShadowPair,
    pub inset_s: ShadowPair,
    pub on_glow_blur: f32,
    pub on_glow_dark: ShadowPair,

    // ----- Radii -----
    /// 4: buttons, windows, fields, dropdowns, groups, note bars, menus, tooltips.
    pub radius_control: f32,
    /// 3: step cells, keycaps, small buttons, segmented items, slot pills, thumbs.
    pub radius_small: f32,
    /// 2: meter columns, colour bars, chips.
    pub radius_chip: f32,
    /// = `radius_control` (Deck panels are 4, not 24).
    pub radius_panel: f32,
    /// = `radius_control` (keys are radius 0 0 4 4).
    pub radius_key_white: f32,
    pub radius_key_black: f32,
    /// = `radius_control`: Deck has no pills, a "pill" draws as a 4 pt rounded rect.
    pub radius_pill: f32,

    // ----- Spacing scale and rhythm -----
    pub space_1: f32,
    pub space_2: f32,
    pub space_3: f32,
    pub space_4: f32,
    pub space_5: f32,
    pub space_6: f32,
    pub space_8: f32,
    /// Dock padding (8).
    pub panel_padding: f32,
    /// Control height (28).
    pub control_height: f32,
    /// Row pitch (32 = 28 + 4).
    pub row_pitch: f32,
    /// Window header height (28).
    pub header_height: f32,
    /// Top toolbar height (40).
    pub toolbar_height: f32,
    /// Window toolbar height (36).
    pub window_toolbar_height: f32,
    /// Tab strip height (28).
    pub tab_strip_height: f32,
    /// Dock gap / splitter width (4); the splitter hit is `gutter_hit`.
    pub gutter: f32,
    pub gutter_hit: f32,
}

/// Corner radius of egui tooltips, popups and windows styled by [`Theme::apply`] (Deck: 4).
pub const POPUP_RADIUS: u8 = 4;
/// Number of track swatches in [`Theme::track_colour`].
pub const TRACK_COLOURS: usize = 8;
/// Alpha of a disabled control (the whole control at 45 %).
pub const DISABLED_ALPHA: f32 = 0.45;
/// Seconds the pointer must rest on a control before its tooltip appears
/// (`Style::interaction.tooltip_delay`, set by [`Theme::apply`]).
pub const TOOLTIP_DELAY: f32 = 0.3;
/// Scrollbar track width (always drawn) and thumb width, per [`Theme::apply`].
pub const SCROLLBAR_WIDTH: f32 = 10.0;
pub const SCROLLBAR_THUMB: f32 = 6.0;

/// The eight track colours of the dark theme: >= 5.6:1 against `panel`, `on_track` >= 5.9:1
/// on every swatch.
const TRACK_COLOURS_DARK: [Color32; TRACK_COLOURS] = [
    rgb(0xF87171),
    rgb(0xA3E635),
    rgb(0x4ADE80),
    rgb(0x2DD4BF),
    rgb(0x38BDF8),
    rgb(0x818CF8),
    rgb(0xC084FC),
    rgb(0xF472B6),
];

/// The eight track colours of the light theme: >= 4.5:1 against `panel`, white >= 5.0:1 on
/// every swatch.
const TRACK_COLOURS_LIGHT: [Color32; TRACK_COLOURS] = [
    rgb(0xB91C1C),
    rgb(0x4D7C0F),
    rgb(0x15803D),
    rgb(0x0F766E),
    rgb(0x0369A1),
    rgb(0x4F46E5),
    rgb(0x7E22CE),
    rgb(0xBE185D),
];

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

const fn rgba(hex: u32, alpha: u8) -> Color32 {
    let a = alpha as u32;
    Color32::from_rgba_premultiplied(
        (((hex >> 16) & 0xFF) * a / 255) as u8,
        (((hex >> 8) & 0xFF) * a / 255) as u8,
        ((hex & 0xFF) * a / 255) as u8,
        alpha,
    )
}

/// `colour` mixed `t` of the way towards black (the WINNER.md "x % towards black" rule).
pub fn towards_black(colour: Color32, t: f32) -> Color32 {
    colour.lerp_to_gamma(Color32::BLACK, t.clamp(0.0, 1.0))
}

/// `colour` at `alpha` (0..=1), premultiplied.
pub fn with_alpha(colour: Color32, alpha: f32) -> Color32 {
    colour.gamma_multiply(alpha.clamp(0.0, 1.0))
}

impl Theme {
    pub fn for_mode(mode: ThemeMode) -> Theme {
        match mode {
            ThemeMode::Light => Theme::light(),
            ThemeMode::Dark => Theme::dark(),
        }
    }

    pub fn dark() -> Theme {
        Theme::build(
            ThemeMode::Dark,
            Palette {
                bg: rgb(0x111316),
                panel: rgb(0x1B1E23),
                panel_2: rgb(0x23272D),
                panel_3: rgb(0x2B3038),
                panel_4: rgb(0x343A43),
                border: rgb(0x30353D),
                border_2: rgb(0x434A54),
                text: rgb(0xE9EBEE),
                text_2: rgb(0x9AA3AE),
                text_3: rgb(0x667080),
                accent: rgb(0xF0B429),
                accent_hover: rgb(0xF8C64E),
                accent_press: rgb(0xD39B14),
                on_accent: rgb(0x17120A),
                accent_wash: rgba(0xF0B429, 41), // 16 %
                rec: rgb(0xF0433A),
                ok: rgb(0x3DD68C),
                grid_beat: rgb(0x2A2F36),
                grid_bar: rgb(0x3C434D),
                step_off: rgb(0x24282E),
                step_off_alt: rgb(0x292E35),
                on_track: rgb(0x15181D),
                tracks: TRACK_COLOURS_DARK,
                shadow_colour: rgba(0x000000, 140), // rgba(0,0,0,.55)
                knob_face: [rgb(0x3A4049), rgb(0x23272D)],
                key_black_face: [rgb(0x0C0E11), rgb(0x1A1D22)],
                key_black_border: rgb(0x000000),
                key_black_text: rgb(0x9AA3AE),
            },
        )
    }

    pub fn light() -> Theme {
        Theme::build(
            ThemeMode::Light,
            Palette {
                bg: rgb(0xDFE2E7),
                panel: rgb(0xF3F4F6),
                panel_2: rgb(0xFFFFFF),
                panel_3: rgb(0xE8EAEE),
                panel_4: rgb(0xDCDFE5),
                border: rgb(0xC6CBD3),
                border_2: rgb(0xA9B0BA),
                text: rgb(0x15181D),
                text_2: rgb(0x525A66),
                text_3: rgb(0x7E8794),
                accent: rgb(0xB45309),
                accent_hover: rgb(0xC9640F),
                accent_press: rgb(0x9A4607),
                on_accent: rgb(0xFFFFFF),
                accent_wash: rgba(0xB45309, 36), // 14 %
                rec: rgb(0xC81E1E),
                ok: rgb(0x15803D),
                grid_beat: rgb(0xE1E4E9),
                grid_bar: rgb(0xC6CBD3),
                step_off: rgb(0xE6E8EC),
                step_off_alt: rgb(0xDEE1E6),
                on_track: rgb(0xFFFFFF),
                tracks: TRACK_COLOURS_LIGHT,
                shadow_colour: rgba(0x141E32, 56), // rgba(20,30,50,.22)
                knob_face: [rgb(0xFFFFFF), rgb(0xD5D9DF)],
                key_black_face: [rgb(0x2A2F38), rgb(0x3D4450)],
                key_black_border: rgb(0x000000),
                key_black_text: rgb(0xC9CED6),
            },
        )
    }

    fn build(mode: ThemeMode, p: Palette) -> Theme {
        let unused = ShadowPair {
            offset: 0.0,
            blur: 0.0,
        };
        Theme {
            mode,
            bg: p.bg,
            panel: p.panel,
            panel_2: p.panel_2,
            panel_3: p.panel_3,
            panel_4: p.panel_4,
            border: p.border,
            border_2: p.border_2,
            text: p.text,
            text_2: p.text_2,
            text_3: p.text_3,
            accent: p.accent,
            accent_hover: p.accent_hover,
            accent_press: p.accent_press,
            on_accent: p.on_accent,
            accent_wash: p.accent_wash,
            rec: p.rec,
            ok: p.ok,
            clip: p.rec,
            grid_beat: p.grid_beat,
            grid_bar: p.grid_bar,
            step_off: p.step_off,
            step_off_alt: p.step_off_alt,
            on_track: p.on_track,
            tracks: p.tracks,
            shadow: Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: p.shadow_colour,
            },
            knob_face: p.knob_face,
            key_black_face: p.key_black_face,
            key_black_border: p.key_black_border,
            key_black_text: p.key_black_text,
            key_black_keycap: [rgb(0x15181D), rgb(0x2B3038), rgb(0x9AA3AE)],

            surface: p.panel,
            shadow_highlight: p.border,
            shadow_dark: p.border_2,
            text_primary: p.text,
            text_secondary: p.text_2,
            key_black: p.key_black_face[0],
            glow: p.accent_wash,
            raised_l: unused,
            raised_m: unused,
            raised_s: unused,
            inset_m: unused,
            inset_s: unused,
            on_glow_blur: 0.0,
            on_glow_dark: unused,

            radius_control: 4.0,
            radius_small: 3.0,
            radius_chip: 2.0,
            radius_panel: 4.0,
            radius_key_white: 4.0,
            radius_key_black: 4.0,
            radius_pill: 4.0,

            space_1: 4.0,
            space_2: 8.0,
            space_3: 12.0,
            space_4: 16.0,
            space_5: 20.0,
            space_6: 24.0,
            space_8: 32.0,
            panel_padding: 8.0,
            control_height: 28.0,
            row_pitch: 32.0,
            header_height: 28.0,
            toolbar_height: 40.0,
            window_toolbar_height: 36.0,
            tab_strip_height: 28.0,
            gutter: 4.0,
            gutter_hit: 8.0,
        }
    }

    /// Applies the palette to egui's `Style` for this mode (WINNER.md section 7): panel /
    /// window fills, widget fills and strokes per state, selection, hyperlink, the Deck type
    /// scale (standard styles plus the named Deck levels, see [`Self::text_styles`]), spacing
    /// (item spacing 8 / 8, button padding 10 / 0, interact size 28), always-visible solid
    /// scrollbars (10 pt bar, 6 pt thumb), popup / menu corner radius 4 with the one
    /// [`Self::shadow`], and the [`TOOLTIP_DELAY`].
    pub fn apply(&self, ctx: &egui::Context) {
        let egui_theme = self.egui_theme();
        let mut visuals = match self.mode {
            ThemeMode::Light => egui::Visuals::light(),
            ThemeMode::Dark => egui::Visuals::dark(),
        };
        let cr = egui::CornerRadius::same(POPUP_RADIUS);
        visuals.panel_fill = self.panel;
        visuals.window_fill = self.panel;
        visuals.extreme_bg_color = self.panel_2;
        visuals.faint_bg_color = self.panel_3;
        visuals.text_edit_bg_color = Some(self.panel_2);
        visuals.code_bg_color = self.panel_2;
        visuals.override_text_color = Some(self.text);
        visuals.weak_text_color = Some(self.text_2);
        visuals.hyperlink_color = self.accent;
        visuals.warn_fg_color = self.accent;
        visuals.error_fg_color = self.rec;
        visuals.selection.bg_fill = self.accent_wash;
        visuals.selection.stroke = egui::Stroke::new(1.0, self.accent);
        visuals.text_cursor.stroke = egui::Stroke::new(1.0, self.text);

        let w = &mut visuals.widgets;
        w.noninteractive.bg_fill = self.panel;
        w.noninteractive.weak_bg_fill = self.panel;
        w.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.border);
        w.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text_2);
        w.inactive.bg_fill = self.panel_3;
        w.inactive.weak_bg_fill = self.panel_3;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, self.border_2);
        w.inactive.fg_stroke = egui::Stroke::new(1.0, self.text_3);
        w.hovered.bg_fill = self.panel_4;
        w.hovered.weak_bg_fill = self.panel_4;
        w.hovered.bg_stroke = egui::Stroke::new(1.0, self.border_2);
        w.hovered.fg_stroke = egui::Stroke::new(1.0, self.text_2);
        w.active.bg_fill = self.panel_2;
        w.active.weak_bg_fill = self.panel_2;
        w.active.bg_stroke = egui::Stroke::new(1.0, self.border);
        w.active.fg_stroke = egui::Stroke::new(1.0, self.text);
        w.open.bg_fill = self.panel_2;
        w.open.weak_bg_fill = self.panel_2;
        w.open.bg_stroke = egui::Stroke::new(1.0, self.border_2);
        w.open.fg_stroke = egui::Stroke::new(1.0, self.text);
        for v in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            v.corner_radius = cr;
            v.expansion = 0.0;
        }

        // Windows: panel fill, 1 px border, radius 4, no shadow. Popups (menus, tooltips):
        // the one floating shadow.
        visuals.window_stroke = egui::Stroke::new(1.0, self.border);
        visuals.window_shadow = Shadow::NONE;
        visuals.window_corner_radius = cr;
        visuals.menu_corner_radius = cr;
        visuals.popup_shadow = self.shadow;
        visuals.striped = false;

        ctx.set_theme(egui_theme);
        ctx.style_mut_of(egui_theme, |style| {
            style.visuals = visuals;
            style.text_styles = Self::text_styles();
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(10.0, 0.0);
            style.spacing.interact_size = egui::vec2(28.0, 28.0);
            style.spacing.menu_margin = egui::Margin::symmetric(0, 4);
            style.spacing.window_margin = egui::Margin::same(8);
            style.spacing.scroll = Self::scroll_style();
            style.interaction.tooltip_delay = TOOLTIP_DELAY;
        });
    }

    /// The scrollbar style of [`Self::apply`]: solid, never auto-hidden, [`SCROLLBAR_WIDTH`]
    /// wide with a [`SCROLLBAR_THUMB`] wide thumb (the bar inset 2 on each side), handle at
    /// least 24 long, fully opaque in every state. The track is `extreme_bg_color` (panel-2),
    /// the thumb the widget `fg_stroke` colour of its state (text-3, hover text-2, drag text).
    pub fn scroll_style() -> egui::style::ScrollStyle {
        egui::style::ScrollStyle {
            floating: false,
            bar_width: SCROLLBAR_WIDTH,
            handle_min_length: 24.0,
            bar_inner_margin: (SCROLLBAR_WIDTH - SCROLLBAR_THUMB) * 0.5,
            bar_outer_margin: 0.0,
            foreground_color: true,
            dormant_background_opacity: 1.0,
            active_background_opacity: 1.0,
            interact_background_opacity: 1.0,
            dormant_handle_opacity: 1.0,
            active_handle_opacity: 1.0,
            interact_handle_opacity: 1.0,
            ..egui::style::ScrollStyle::solid()
        }
    }

    /// The one egui drop shadow (`shadow` in WINNER.md): used for popups and tooltips by
    /// [`Self::apply`] and for the floating drawer / menus by the kit.
    pub fn popup_shadow(&self) -> Shadow {
        self.shadow
    }

    /// The colour of instrument track `index`, wrapping every [`TRACK_COLOURS`] tracks.
    pub fn track_colour(&self, index: u8) -> Color32 {
        self.tracks[usize::from(index) % TRACK_COLOURS]
    }

    /// Note-bar outline: the track colour 60 % towards black.
    pub fn note_outline(colour: Color32) -> Color32 {
        towards_black(colour, 0.6)
    }

    /// Note-bar resize handle: the track colour 50 % towards black.
    pub fn note_handle(colour: Color32) -> Color32 {
        towards_black(colour, 0.5)
    }

    /// Note-step (rack track lane) border: the track colour 70 % towards black.
    pub fn note_step_border(colour: Color32) -> Color32 {
        towards_black(colour, 0.7)
    }

    /// LED-on border: `ok` 60 % towards black.
    pub fn led_on_border(&self) -> Color32 {
        towards_black(self.ok, 0.6)
    }

    /// Record LED border: `rec` 60 % towards black.
    pub fn led_rec_border(&self) -> Color32 {
        towards_black(self.rec, 0.6)
    }

    /// The egui theme that corresponds to this mode.
    pub fn egui_theme(&self) -> egui::Theme {
        match self.mode {
            ThemeMode::Light => egui::Theme::Light,
            ThemeMode::Dark => egui::Theme::Dark,
        }
    }

    /// egui text styles for the Deck type scale: the standard styles (Small 11, Body 13,
    /// Button 12, Heading 12, Monospace 13) plus one `TextStyle::Name` per level of
    /// [`crate::text::TextKind`] (`"title"` 12, `"label"` 12, `"button"` 12, `"value"` 13,
    /// `"body"` 13, `"display"` 18, `"small"` 11, `"hint"` 10, `"section"` 11, `"micro"` 10).
    pub fn text_styles() -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
        use egui::{FontFamily, FontId, TextStyle};
        let p = |size: f32| FontId::new(size, FontFamily::Proportional);
        let mut styles: std::collections::BTreeMap<TextStyle, FontId> = [
            (TextStyle::Small, p(11.0)),
            (TextStyle::Body, p(13.0)),
            (TextStyle::Button, p(12.0)),
            (TextStyle::Heading, p(12.0)),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into_iter()
        .collect();
        for kind in crate::text::TextKind::ALL {
            styles.insert(TextStyle::Name(kind.style_name().into()), kind.font_id());
        }
        styles
    }
}

/// The per-mode colour values [`Theme::build`] expands into a [`Theme`].
struct Palette {
    bg: Color32,
    panel: Color32,
    panel_2: Color32,
    panel_3: Color32,
    panel_4: Color32,
    border: Color32,
    border_2: Color32,
    text: Color32,
    text_2: Color32,
    text_3: Color32,
    accent: Color32,
    accent_hover: Color32,
    accent_press: Color32,
    on_accent: Color32,
    accent_wash: Color32,
    rec: Color32,
    ok: Color32,
    grid_beat: Color32,
    grid_bar: Color32,
    step_off: Color32,
    step_off_alt: Color32,
    on_track: Color32,
    tracks: [Color32; TRACK_COLOURS],
    shadow_colour: Color32,
    knob_face: [Color32; 2],
    key_black_face: [Color32; 2],
    key_black_border: Color32,
    key_black_text: Color32,
}

/// WCAG 2.x contrast ratio between two opaque colours (1.0 to 21.0). AA asks for at least
/// 4.5 for text and 3.0 for large text and UI state.
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    fn channel(c: u8) -> f32 {
        let s = f32::from(c) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(c: Color32) -> f32 {
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = (la.max(lb), la.min(lb));
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    // WINNER.md quotes ratios rounded to one decimal, so each check allows 0.05.
    fn check(theme: &Theme, what: &str, fg: Color32, bg: Color32, min: f32) {
        let ratio = contrast_ratio(fg, bg);
        assert!(
            ratio >= min - 0.05,
            "{:?}: {what} is {ratio:.2}:1, needs {min}:1",
            theme.mode
        );
    }

    #[test]
    fn text_tokens_meet_the_ratios_winner_lists() {
        for theme in [Theme::dark(), Theme::light()] {
            let t = &theme;
            check(t, "text on panel", t.text, t.panel, 11.0);
            check(t, "text on panel-3", t.text, t.panel_3, 11.0);
            check(t, "text-2 on panel", t.text_2, t.panel, 6.3);
            check(t, "text-2 on panel-2", t.text_2, t.panel_2, 5.9);
            check(t, "text-3 on panel (hints only)", t.text_3, t.panel, 3.3);
            check(t, "accent on panel", t.accent, t.panel, 4.6);
            check(t, "on-accent on accent", t.on_accent, t.accent, 5.0);
            // Not in the WINNER table: the light accent-hover is 3.95:1 under white, AA UI
            // (3:1) but not AA text. Reported to design; the primary hover is a button face.
            check(
                t,
                "on-accent on accent-hover",
                t.on_accent,
                t.accent_hover,
                3.0,
            );
            check(t, "ok on panel", t.ok, t.panel, 4.6);
            check(t, "rec on panel", t.rec, t.panel, 4.4);
            check(t, "text on panel-4 (tooltip face)", t.text, t.panel_4, 9.0);
            check(
                t,
                "text-2 on panel-4 (menu hover)",
                t.text_2,
                t.panel_4,
                4.5,
            );
        }
        assert!((contrast_ratio(Color32::WHITE, Color32::BLACK) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn palettes_match_winner() {
        let d = Theme::dark();
        let l = Theme::light();
        assert_eq!(d.bg, rgb(0x111316));
        assert_eq!(d.panel, rgb(0x1B1E23));
        assert_eq!(d.panel_2, rgb(0x23272D));
        assert_eq!(d.panel_3, rgb(0x2B3038));
        assert_eq!(d.panel_4, rgb(0x343A43));
        assert_eq!(d.border, rgb(0x30353D));
        assert_eq!(d.border_2, rgb(0x434A54));
        assert_eq!(d.text, rgb(0xE9EBEE));
        assert_eq!(d.text_2, rgb(0x9AA3AE));
        assert_eq!(d.text_3, rgb(0x667080));
        assert_eq!(d.accent, rgb(0xF0B429));
        assert_eq!(d.accent_hover, rgb(0xF8C64E));
        assert_eq!(d.accent_press, rgb(0xD39B14));
        assert_eq!(d.on_accent, rgb(0x17120A));
        assert_eq!(d.rec, rgb(0xF0433A));
        assert_eq!(d.ok, rgb(0x3DD68C));
        assert_eq!(d.clip, d.rec);
        assert_eq!(d.grid_beat, rgb(0x2A2F36));
        assert_eq!(d.grid_bar, rgb(0x3C434D));
        assert_eq!(d.step_off, rgb(0x24282E));
        assert_eq!(d.step_off_alt, rgb(0x292E35));
        assert_eq!(d.on_track, rgb(0x15181D));
        assert_eq!(d.accent_wash.a(), 41);
        assert_eq!(d.shadow.offset, [0, 8]);
        assert_eq!(d.shadow.blur, 24);
        assert_eq!(d.shadow.color.a(), 140);

        assert_eq!(l.bg, rgb(0xDFE2E7));
        assert_eq!(l.panel, rgb(0xF3F4F6));
        assert_eq!(l.panel_2, rgb(0xFFFFFF));
        assert_eq!(l.panel_3, rgb(0xE8EAEE));
        assert_eq!(l.panel_4, rgb(0xDCDFE5));
        assert_eq!(l.border, rgb(0xC6CBD3));
        assert_eq!(l.border_2, rgb(0xA9B0BA));
        assert_eq!(l.text, rgb(0x15181D));
        assert_eq!(l.text_2, rgb(0x525A66));
        assert_eq!(l.text_3, rgb(0x7E8794));
        assert_eq!(l.accent, rgb(0xB45309));
        assert_eq!(l.accent_hover, rgb(0xC9640F));
        assert_eq!(l.accent_press, rgb(0x9A4607));
        assert_eq!(l.on_accent, rgb(0xFFFFFF));
        assert_eq!(l.rec, rgb(0xC81E1E));
        assert_eq!(l.ok, rgb(0x15803D));
        assert_eq!(l.grid_beat, rgb(0xE1E4E9));
        assert_eq!(l.grid_bar, rgb(0xC6CBD3));
        assert_eq!(l.step_off, rgb(0xE6E8EC));
        assert_eq!(l.step_off_alt, rgb(0xDEE1E6));
        assert_eq!(l.on_track, rgb(0xFFFFFF));
        assert_eq!(l.accent_wash.a(), 36);
        assert_eq!(l.shadow.color.a(), 56);

        // Aliases point at the Deck tones.
        assert_eq!(d.surface, d.panel);
        assert_eq!(d.text_primary, d.text);
        assert_eq!(d.text_secondary, d.text_2);
        assert_eq!(d.shadow_highlight, d.border);
        assert_eq!(d.shadow_dark, d.border_2);
        assert_eq!(d.glow, d.accent_wash);
        assert_eq!(d.radius_pill, 4.0);
        assert_eq!(d.radius_panel, 4.0);
        assert_eq!(d.panel_padding, 8.0);
        assert_eq!(d.control_height, 28.0);
        assert_eq!(d.row_pitch, 32.0);
    }

    #[test]
    fn track_colours_match_winner_and_read_on_the_panel() {
        let dark = [
            0xF87171, 0xA3E635, 0x4ADE80, 0x2DD4BF, 0x38BDF8, 0x818CF8, 0xC084FC, 0xF472B6,
        ];
        let light = [
            0xB91C1C, 0x4D7C0F, 0x15803D, 0x0F766E, 0x0369A1, 0x4F46E5, 0x7E22CE, 0xBE185D,
        ];
        for (theme, hexes, min_panel, min_on_track) in [
            (Theme::dark(), dark, 5.6, 5.9),
            (Theme::light(), light, 4.5, 5.0),
        ] {
            for (i, hex) in hexes.into_iter().enumerate() {
                let c = theme.track_colour(i as u8);
                assert_eq!(c, rgb(hex), "{:?} track {i}", theme.mode);
                check(
                    &theme,
                    &format!("track {i} on panel"),
                    c,
                    theme.panel,
                    min_panel,
                );
                check(
                    &theme,
                    &format!("on-track on track {i}"),
                    theme.on_track,
                    c,
                    min_on_track,
                );
                for (j, other) in hexes.iter().enumerate().skip(i + 1) {
                    assert_ne!(c, rgb(*other), "tracks {i} and {j} share a colour");
                }
            }
            assert_eq!(theme.track_colour(8), theme.track_colour(0));
            assert_eq!(theme.track_colour(255), theme.track_colour(7));
        }
        assert_ne!(
            Theme::dark().track_colour(3),
            Theme::light().track_colour(3)
        );
    }

    #[test]
    fn derived_colours_follow_the_fixed_formulas() {
        let c = Color32::from_rgb(200, 100, 50);
        assert_eq!(Theme::note_outline(c), c.lerp_to_gamma(Color32::BLACK, 0.6));
        assert_eq!(Theme::note_handle(c), c.lerp_to_gamma(Color32::BLACK, 0.5));
        assert_eq!(
            Theme::note_step_border(c),
            c.lerp_to_gamma(Color32::BLACK, 0.7)
        );
        let t = Theme::dark();
        assert_eq!(t.led_on_border(), towards_black(t.ok, 0.6));
        assert_eq!(with_alpha(c, 0.5).a(), 128);
        assert_eq!(towards_black(c, 2.0), Color32::BLACK);
    }

    #[test]
    fn apply_sets_the_deck_style() {
        for theme in [Theme::dark(), Theme::light()] {
            let ctx = egui::Context::default();
            theme.apply(&ctx);
            let style = ctx.style_of(theme.egui_theme());
            let v = &style.visuals;
            assert_eq!(v.panel_fill, theme.panel);
            assert_eq!(v.window_fill, theme.panel);
            assert_eq!(v.extreme_bg_color, theme.panel_2);
            assert_eq!(v.window_stroke, egui::Stroke::new(1.0, theme.border));
            assert_eq!(v.window_shadow, Shadow::NONE);
            assert_eq!(v.popup_shadow, theme.shadow);
            assert_eq!(v.widgets.inactive.bg_fill, theme.panel_3);
            assert_eq!(v.widgets.hovered.bg_fill, theme.panel_4);
            assert_eq!(v.widgets.active.bg_fill, theme.panel_2);
            assert_eq!(v.widgets.inactive.bg_stroke.color, theme.border_2);
            assert_eq!(v.selection.bg_fill, theme.accent_wash);
            assert_eq!(v.override_text_color, Some(theme.text));
            assert_eq!(style.spacing.item_spacing, egui::vec2(8.0, 8.0));
            assert_eq!(style.spacing.interact_size, egui::vec2(28.0, 28.0));
            let s = &style.spacing.scroll;
            assert!(!s.floating);
            assert_eq!(s.bar_width, 10.0);
            assert_eq!(s.bar_width - 2.0 * s.bar_inner_margin, 6.0);
            assert_eq!(s.dormant_handle_opacity, 1.0);
            assert_eq!(s.dormant_background_opacity, 1.0);
            assert_eq!(style.interaction.tooltip_delay, TOOLTIP_DELAY);
            assert_eq!(
                style.text_styles[&egui::TextStyle::Body],
                egui::FontId::proportional(13.0)
            );
            assert_eq!(
                style.text_styles[&egui::TextStyle::Name("display".into())],
                egui::FontId::proportional(18.0)
            );
            assert_eq!(
                style.text_styles[&egui::TextStyle::Name("hint".into())],
                egui::FontId::proportional(10.0)
            );
        }
    }
}
