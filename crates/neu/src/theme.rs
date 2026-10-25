//! Theme tokens transcribed from `design/WINNER.md` ("Soft Console").
//!
//! These values are the source of truth. Do not tune them here; change the design file first.

use egui::Color32;

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

/// A soft shadow pair: highlight to the top-left, dark to the bottom-right.
/// Offsets are in points, positive values mean "towards the bottom-right".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowPair {
    pub offset: f32,
    pub blur: f32,
}

/// All colour, shadow, radius and spacing tokens for one theme mode.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub mode: ThemeMode,

    // Colours
    pub surface: Color32,
    pub shadow_highlight: Color32,
    pub shadow_dark: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub key_black: Color32,
    /// Accent with alpha: the ON-state outer glow colour.
    pub glow: Color32,

    // Shadows (two per element, never more)
    pub raised_l: ShadowPair,
    pub raised_m: ShadowPair,
    pub raised_s: ShadowPair,
    pub inset_m: ShadowPair,
    pub inset_s: ShadowPair,
    /// ON state: glow (0, 0, blur 14) plus a dark shadow (4, 4, blur 10).
    pub on_glow_blur: f32,
    pub on_glow_dark: ShadowPair,

    // Radii
    pub radius_panel: f32,
    pub radius_key_white: f32,
    pub radius_key_black: f32,
    pub radius_pill: f32,

    // Spacing scale
    pub space_1: f32,
    pub space_2: f32,
    pub space_3: f32,
    pub space_4: f32,
    pub space_5: f32,
    pub space_6: f32,
    pub space_8: f32,
    pub panel_padding: f32,
}

/// Corner radius of egui tooltips, popups and windows styled by [`Theme::apply`].
pub const POPUP_RADIUS: u8 = 12;
/// Number of track swatches in [`Theme::track_colour`] (one per instrument track, M10).
pub const TRACK_COLOURS: usize = 8;

/// The eight instrument-track swatches of the dark theme (M10): the accent itself, then the
/// accent family walked around the hue wheel at the accent's weight (bright, 300/400-weight
/// tints that sit on the dark surface). Index order: accent, amber, green, teal, sky,
/// indigo, pink, coral.
const TRACK_COLOURS_DARK: [Color32; TRACK_COLOURS] = [
    rgb(0xFF7A1A),
    rgb(0xFFC83C),
    rgb(0x4ADE80),
    rgb(0x2DD4BF),
    rgb(0x38BDF8),
    rgb(0x818CF8),
    rgb(0xF472B6),
    rgb(0xF87171),
];

/// The eight instrument-track swatches of the light theme (M10): the accent itself, then the
/// same hue walk at the 600/700 weight so every swatch stays darker than the light surface.
/// Index order: accent, amber, green, teal, sky, violet, pink, red.
const TRACK_COLOURS_LIGHT: [Color32; TRACK_COLOURS] = [
    rgb(0x1E5BFF),
    rgb(0xB45309),
    rgb(0x15803D),
    rgb(0x0F766E),
    rgb(0x0284C7),
    rgb(0x6D28D9),
    rgb(0xBE185D),
    rgb(0xDC2626),
];
/// Seconds the pointer must rest on a control before its tooltip appears
/// (`Style::interaction.tooltip_delay`, set by [`Theme::apply`]).
pub const TOOLTIP_DELAY: f32 = 0.3;

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

impl Theme {
    pub fn for_mode(mode: ThemeMode) -> Theme {
        match mode {
            ThemeMode::Light => Theme::light(),
            ThemeMode::Dark => Theme::dark(),
        }
    }

    pub fn dark() -> Theme {
        Theme {
            mode: ThemeMode::Dark,
            surface: rgb(0x262A31),
            shadow_highlight: rgb(0x333941),
            shadow_dark: rgb(0x171A1F),
            accent: rgb(0xFF7A1A),
            accent_hover: rgb(0xFF9440),
            on_accent: rgb(0x141821),
            text_primary: rgb(0xF2F2F2),
            text_secondary: rgb(0xA8ADB5),
            key_black: rgb(0x1C1F25),
            glow: rgba(0xFF7A1A, 140), // 55 % alpha
            ..Theme::shared(ThemeMode::Dark)
        }
    }

    pub fn light() -> Theme {
        Theme {
            mode: ThemeMode::Light,
            surface: rgb(0xE8ECF3),
            shadow_highlight: rgb(0xFFFFFF),
            shadow_dark: rgb(0xC3CAD6),
            accent: rgb(0x1E5BFF),
            accent_hover: rgb(0x2E6AFF),
            on_accent: rgb(0xFFFFFF),
            text_primary: rgb(0x1C2130),
            text_secondary: rgb(0x5E6677),
            key_black: rgb(0xB4BCCA),
            glow: rgba(0x1E5BFF, 115), // 45 % alpha
            ..Theme::shared(ThemeMode::Light)
        }
    }

    /// Mode-independent tokens (shadows, radii, spacing). Colours are placeholders here.
    fn shared(mode: ThemeMode) -> Theme {
        Theme {
            mode,
            surface: Color32::TRANSPARENT,
            shadow_highlight: Color32::TRANSPARENT,
            shadow_dark: Color32::TRANSPARENT,
            accent: Color32::TRANSPARENT,
            accent_hover: Color32::TRANSPARENT,
            on_accent: Color32::TRANSPARENT,
            text_primary: Color32::TRANSPARENT,
            text_secondary: Color32::TRANSPARENT,
            key_black: Color32::TRANSPARENT,
            glow: Color32::TRANSPARENT,
            raised_l: ShadowPair {
                offset: 8.0,
                blur: 20.0,
            },
            raised_m: ShadowPair {
                offset: 5.0,
                blur: 12.0,
            },
            raised_s: ShadowPair {
                offset: 3.0,
                blur: 8.0,
            },
            inset_m: ShadowPair {
                offset: 4.0,
                blur: 10.0,
            },
            inset_s: ShadowPair {
                offset: 2.0,
                blur: 6.0,
            },
            on_glow_blur: 14.0,
            on_glow_dark: ShadowPair {
                offset: 4.0,
                blur: 10.0,
            },
            radius_panel: 24.0,
            radius_key_white: 12.0,
            radius_key_black: 10.0,
            radius_pill: 999.0,
            space_1: 4.0,
            space_2: 8.0,
            space_3: 12.0,
            space_4: 16.0,
            space_5: 20.0,
            space_6: 24.0,
            space_8: 32.0,
            panel_padding: 20.0,
        }
    }

    /// Applies the palette to egui's global visuals (window fill, text colours, selection).
    /// The widget kit paints its own shadows; this only sets the defaults for plain egui text
    /// and layout, plus the tooltip/popup look (surface fill, [`Self::popup_shadow`], no
    /// stroke, [`POPUP_RADIUS`] corners) and the [`TOOLTIP_DELAY`].
    pub fn apply(&self, ctx: &egui::Context) {
        let egui_theme = self.egui_theme();
        let mut visuals = match self.mode {
            ThemeMode::Light => egui::Visuals::light(),
            ThemeMode::Dark => egui::Visuals::dark(),
        };
        visuals.panel_fill = self.surface;
        visuals.window_fill = self.surface;
        visuals.extreme_bg_color = self.surface;
        visuals.faint_bg_color = self.surface;
        visuals.override_text_color = Some(self.text_primary);
        visuals.hyperlink_color = self.accent;
        visuals.selection.bg_fill = self.accent;
        visuals.selection.stroke.color = self.on_accent;
        visuals.widgets.noninteractive.bg_fill = self.surface;
        visuals.widgets.noninteractive.fg_stroke.color = self.text_secondary;
        visuals.widgets.inactive.bg_fill = self.surface;
        visuals.widgets.hovered.bg_fill = self.surface;
        visuals.widgets.active.bg_fill = self.surface;
        visuals.widgets.inactive.fg_stroke.color = self.text_secondary;
        visuals.widgets.hovered.fg_stroke.color = self.text_primary;
        visuals.widgets.active.fg_stroke.color = self.text_primary;
        // Depth comes from the kit's shadows only: no egui outlines anywhere.
        for w in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            w.bg_stroke = egui::Stroke::NONE;
        }
        visuals.window_stroke = egui::Stroke::NONE;
        visuals.window_shadow = egui::epaint::Shadow::NONE;
        // Tooltips and popups (`Frame::popup`): surface fill, a raised-s-like drop shadow, no
        // stroke, 12 pt corners. egui reads `menu_corner_radius` for popups and tooltips and
        // `window_corner_radius` for windows; both get the same radius.
        visuals.popup_shadow = self.popup_shadow();
        visuals.window_corner_radius = egui::CornerRadius::same(POPUP_RADIUS);
        visuals.menu_corner_radius = egui::CornerRadius::same(POPUP_RADIUS);

        // Make egui's own notion of light/dark follow ours, then install the palette and the
        // type scale (label 11, body/value 13, button 12, heading 15) for that theme.
        ctx.set_theme(egui_theme);
        ctx.style_mut_of(egui_theme, |style| {
            style.visuals = visuals;
            style.text_styles = Self::text_styles();
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(16.0, 6.0);
            style.interaction.tooltip_delay = TOOLTIP_DELAY;
        });
    }

    /// The single egui drop shadow used for tooltips and popups: the dark half of `raised_s`
    /// (offset 3, blur 8) in the theme's `shadow_dark` colour. egui's `Shadow` has no highlight
    /// half, so the kit's two-shadow look is approximated by the dark one alone.
    pub fn popup_shadow(&self) -> egui::epaint::Shadow {
        egui::epaint::Shadow {
            offset: [self.raised_s.offset as i8, self.raised_s.offset as i8],
            blur: self.raised_s.blur as u8,
            spread: 0,
            color: self.shadow_dark,
        }
    }

    /// The colour of instrument track `index` (M10): swatch 0 is the accent, the other seven
    /// walk the accent family around the hue wheel. The index wraps every [`TRACK_COLOURS`]
    /// tracks. Every swatch is at least 1.5:1 against the surface of its theme, and distinct
    /// from the others, so a track reads both as a full note and as a dimmed ghost
    /// ([`crate::piano_roll::ghost_fill`]).
    pub fn track_colour(&self, index: u8) -> Color32 {
        let swatches = match self.mode {
            ThemeMode::Light => &TRACK_COLOURS_LIGHT,
            ThemeMode::Dark => &TRACK_COLOURS_DARK,
        };
        swatches[usize::from(index) % TRACK_COLOURS]
    }

    /// The egui theme that corresponds to this mode.
    pub fn egui_theme(&self) -> egui::Theme {
        match self.mode {
            ThemeMode::Light => egui::Theme::Light,
            ThemeMode::Dark => egui::Theme::Dark,
        }
    }

    /// egui text styles sized like the mock: small/label 11, body/value 13, button 12,
    /// heading 15 (the brand line), monospace 13 (tabular values).
    pub fn text_styles() -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
        use egui::{FontFamily, FontId, TextStyle};
        [
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(15.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into_iter()
        .collect()
    }
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

    #[test]
    fn text_and_on_states_meet_aa_in_both_themes() {
        for theme in [Theme::dark(), Theme::light()] {
            let mode = theme.mode;
            let check = |what: &str, fg: Color32, bg: Color32, min: f32| {
                let ratio = contrast_ratio(fg, bg);
                assert!(
                    ratio >= min,
                    "{mode:?}: {what} is {ratio:.2}:1, needs {min}:1"
                );
            };
            check(
                "secondary text on surface",
                theme.text_secondary,
                theme.surface,
                4.5,
            );
            check(
                "primary text on surface",
                theme.text_primary,
                theme.surface,
                7.0,
            );
            check(
                "on-accent text on accent",
                theme.on_accent,
                theme.accent,
                4.5,
            );
            check(
                "on-accent text on hovered accent",
                theme.on_accent,
                theme.accent_hover,
                4.5,
            );
            // The accent itself (chips, lit cells, thumbs) against the surface: UI state.
            check("accent on surface", theme.accent, theme.surface, 3.0);
        }
        assert!((contrast_ratio(Color32::WHITE, Color32::BLACK) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn palettes_differ_and_match_winner() {
        let d = Theme::dark();
        let l = Theme::light();
        assert_ne!(d.surface, l.surface);
        assert_eq!(d.surface, Color32::from_rgb(0x26, 0x2A, 0x31));
        assert_eq!(l.surface, Color32::from_rgb(0xE8, 0xEC, 0xF3));
        assert_eq!(d.accent, Color32::from_rgb(0xFF, 0x7A, 0x1A));
        assert_eq!(l.accent, Color32::from_rgb(0x1E, 0x5B, 0xFF));
        assert_eq!(d.raised_l, l.raised_l);
    }

    #[test]
    fn track_colours_are_distinct_and_read_on_the_surface_in_both_themes() {
        for theme in [Theme::dark(), Theme::light()] {
            let mode = theme.mode;
            let swatches: Vec<Color32> = (0..TRACK_COLOURS as u8)
                .map(|i| theme.track_colour(i))
                .collect();
            assert_eq!(
                swatches[0], theme.accent,
                "{mode:?}: swatch 0 is the accent"
            );
            for (i, a) in swatches.iter().enumerate() {
                let ratio = contrast_ratio(*a, theme.surface);
                assert!(
                    ratio >= 1.5,
                    "{mode:?}: track {i} is {ratio:.2}:1 on the surface, needs 1.5:1"
                );
                for (j, b) in swatches.iter().enumerate().skip(i + 1) {
                    assert_ne!(a, b, "{mode:?}: tracks {i} and {j} share a colour");
                }
            }
            // The index wraps: track 8 is track 0 again, 255 is track 7.
            assert_eq!(theme.track_colour(8), swatches[0]);
            assert_eq!(theme.track_colour(255), swatches[7]);
        }
        // The two themes are different palettes (bright tints on dark, deep tones on light).
        assert_ne!(
            Theme::dark().track_colour(3),
            Theme::light().track_colour(3)
        );
    }
}
