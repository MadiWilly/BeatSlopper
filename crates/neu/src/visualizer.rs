//! Audio visualizers: theme-matched drawings of the master output (M-visual).
//!
//! Every visualizer is a plain function `fn(ui, &Theme, &VisualInput, &VisualStyle) ->
//! Response` that fills the space it is given (`ui.available_size_before_wrap()`, at least
//! [`MIN_SIZE`]) with an inset-m panel and the drawing. Everything is painted with egui's
//! painter from the kit's tokens: accent / accent-hover strokes and fills, the shadow-dark
//! colour for the faint grid, the surface colour for occlusion. "Glow" is always a second,
//! wider, low-alpha stroke under the main one, never a blur; no textures, no shaders, no
//! per-frame allocation beyond a few small `Vec`s of points.
//!
//! * [`scope`]: oscilloscope line, trigger-stabilised on a rising zero crossing
//!   ([`trigger_index`]).
//! * [`bars`]: [`BAR_COUNT`] log-spaced spectrum bars with rounded tops, raised-s stubs when
//!   quiet, accent fill rising and accent-hover peak-hold caps that fall.
//! * [`radial`]: a circular spectrum around a centre circle whose radius pulses with the
//!   master peak; kick hits emit an expanding ring that fades over [`RING_SECONDS`].
//! * [`vector`]: a Lissajous / vectorscope of left vs right as a fading point cloud (one
//!   small quad per sample, per-vertex alpha) with a stereo [`correlation`] meter.
//! * [`waves`]: the last [`WAVE_LAYERS`] windows stacked back to front with decreasing alpha
//!   and surface-colour occlusion (the "joy division" strip), good as a background.
//! * [`particles`]: a burst of up to [`MAX_PARTICLES`] particles per drum hit (a different
//!   size and tint per row) under gravity, over faint spectrum bars.
//!
//! Animation state (peak holds, ring start times, the wave history, the particle field) lives
//! in egui's temporary memory under the widget's id, so every call site animates on its own.
//!
//! [`Visualizer`] names the six drawings; [`VisualPreset`] bundles one with a [`VisualStyle`]
//! (glow, grid, mirror, thickness, fade and a [`Palette`] tweak); [`PRESETS`] is the table the
//! app offers ("Soft Scope", "Club Bars", "Orbit", "Vector", "Tide", "Fireworks" and "Off").

use egui::epaint::Mesh;
use egui::{
    Color32, CornerRadius, Id, Painter, Pos2, Rect, Response, Role, Sense, Shape, Stroke, Ui,
    Vec2, WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::shadow::{Depth, paint_soft_circle, paint_soft_rect};

/// Number of bins the spectrum input is expected to carry (log spaced, `0..=1` each).
pub const SPECTRUM_BINS: usize = 64;
/// Bars drawn by [`bars`] (pairs of spectrum bins).
pub const BAR_COUNT: usize = 32;
/// Largest number of live particles in [`particles`] (a fixed array).
pub const MAX_PARTICLES: usize = 256;
/// How long a kick ring takes to reach the edge and fade out, in seconds.
pub const RING_SECONDS: f32 = 0.3;
/// Windows kept by [`waves`].
pub const WAVE_LAYERS: usize = 16;
/// Points per wave layer.
pub const WAVE_POINTS: usize = 96;
/// Sample pairs drawn by [`vector`] (the newest of the window).
pub const VECTOR_POINTS: usize = 512;
/// Corner radius of the inset backdrop.
pub const PANEL_RADIUS: f32 = 12.0;
/// Smallest size a visualizer draws at.
pub const MIN_SIZE: Vec2 = vec2(40.0, 20.0);
/// Peak-hold caps fall by this fraction of the bar height per second.
pub const PEAK_HOLD_FALL: f32 = 0.6;
/// Gravity applied to particles, in points per second squared.
pub const PARTICLE_GRAVITY: f32 = 700.0;
/// Spectrum values below this draw as the quiet stub.
pub const QUIET: f32 = 0.03;

/// One frame of audio data for a visualizer. Slices may be empty (silence); the drawings
/// never index past what they are given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualInput<'a> {
    /// Mono window, oldest first (e.g. 1024 or 2048 samples, nominally -1..=1).
    pub samples: &'a [f32],
    /// The right channel when a stereo pair is available ([`vector`] uses `samples` as left).
    pub samples_r: Option<&'a [f32]>,
    /// Spectrum magnitudes `0..=1`, [`SPECTRUM_BINS`] log-spaced bins (any length works).
    pub spectrum: &'a [f32],
    /// Master peak (left, right), linear `0..=1`.
    pub peak: (f32, f32),
    /// Drum rows that hit this frame (bit `DrumRow::index`, bit 0 = kick).
    pub hits: u8,
    /// Playhead position within the bar, `0..1`.
    pub playhead: f32,
    /// Tempo, for anything that wants to sync.
    pub bpm: f32,
    /// Wall time in seconds (`egui::InputState::time`), for the animations.
    pub time: f64,
}

impl VisualInput<'static> {
    /// Silence: empty slices, no hits.
    pub const SILENT: VisualInput<'static> = VisualInput {
        samples: &[],
        samples_r: None,
        spectrum: &[],
        peak: (0.0, 0.0),
        hits: 0,
        playhead: 0.0,
        bpm: 120.0,
        time: 0.0,
    };
}

/// Which accent the drawing leans on: the plain accent, the hover accent, or a blend that
/// moves from accent to accent-hover with level / age.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Palette {
    #[default]
    Accent,
    Hover,
    Blend,
}

impl Palette {
    /// The colour at `t` (`0..=1`, e.g. level or age fraction).
    #[must_use]
    pub fn color(self, theme: &Theme, t: f32) -> Color32 {
        match self {
            Palette::Accent => theme.accent,
            Palette::Hover => theme.accent_hover,
            Palette::Blend => theme
                .accent
                .lerp_to_gamma(theme.accent_hover, safe_unit(t)),
        }
    }
}

/// Styling knobs shared by every visualizer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualStyle {
    /// Draw the wider low-alpha glow stroke under lines and hot bars.
    pub glow: bool,
    /// Draw the faint grid on the backdrop.
    pub grid: bool,
    /// Mirror the drawing (scope: flipped copy; bars: from the centre line both ways;
    /// radial: symmetric halves; waves: hills both ways).
    pub mirror: bool,
    /// Line / point thickness in points.
    pub thickness: f32,
    /// How fast history fades, `0..=1` (0 = long trails, 1 = short).
    pub fade: f32,
    /// Palette tweak.
    pub palette: Palette,
}

impl VisualStyle {
    /// Glow on, no grid, no mirror, 2 pt, fade 0.5, plain accent.
    pub const DEFAULT: VisualStyle = VisualStyle {
        glow: true,
        grid: false,
        mirror: false,
        thickness: 2.0,
        fade: 0.5,
        palette: Palette::Accent,
    };

    fn thickness(&self) -> f32 {
        if self.thickness.is_finite() {
            self.thickness.clamp(0.5, 12.0)
        } else {
            2.0
        }
    }

    fn fade(&self) -> f32 {
        safe_unit(self.fade)
    }
}

impl Default for VisualStyle {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The six drawings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visualizer {
    Scope,
    Bars,
    Radial,
    Vector,
    Waves,
    Particles,
}

impl Visualizer {
    /// Every visualizer, in menu order.
    pub const ALL: [Visualizer; 6] = [
        Visualizer::Scope,
        Visualizer::Bars,
        Visualizer::Radial,
        Visualizer::Vector,
        Visualizer::Waves,
        Visualizer::Particles,
    ];

    /// Display name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Visualizer::Scope => "Scope",
            Visualizer::Bars => "Bars",
            Visualizer::Radial => "Radial",
            Visualizer::Vector => "Vector",
            Visualizer::Waves => "Waves",
            Visualizer::Particles => "Particles",
        }
    }

    /// Accessible name of the widget (`"Scope visualizer"`).
    #[must_use]
    pub fn widget_label(self) -> &'static str {
        match self {
            Visualizer::Scope => "Scope visualizer",
            Visualizer::Bars => "Bars visualizer",
            Visualizer::Radial => "Radial visualizer",
            Visualizer::Vector => "Vector visualizer",
            Visualizer::Waves => "Waves visualizer",
            Visualizer::Particles => "Particles visualizer",
        }
    }

    /// Draws this visualizer (see [`visualizer`]).
    pub fn show(
        self,
        ui: &mut Ui,
        theme: &Theme,
        input: &VisualInput<'_>,
        style: &VisualStyle,
    ) -> Response {
        visualizer(ui, theme, self, input, style)
    }
}

/// A named visualizer + style combination. `visualizer == None` is "Off".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualPreset {
    pub name: &'static str,
    pub visualizer: Option<Visualizer>,
    pub style: VisualStyle,
}

impl VisualPreset {
    /// `true` for the "Off" preset.
    #[must_use]
    pub fn is_off(&self) -> bool {
        self.visualizer.is_none()
    }

    /// Draws the preset, or nothing when it is off.
    pub fn show(&self, ui: &mut Ui, theme: &Theme, input: &VisualInput<'_>) -> Option<Response> {
        self.visualizer
            .map(|kind| visualizer(ui, theme, kind, input, &self.style))
    }
}

/// Index of the "Off" entry in [`PRESETS`].
pub const OFF_PRESET: usize = 6;

/// The preset table offered by the app, "Off" last.
pub const PRESETS: [VisualPreset; 7] = [
    VisualPreset {
        name: "Soft Scope",
        visualizer: Some(Visualizer::Scope),
        style: VisualStyle {
            glow: true,
            grid: false,
            mirror: false,
            thickness: 2.0,
            fade: 0.5,
            palette: Palette::Accent,
        },
    },
    VisualPreset {
        name: "Club Bars",
        visualizer: Some(Visualizer::Bars),
        style: VisualStyle {
            glow: true,
            grid: false,
            mirror: true,
            thickness: 2.0,
            fade: 0.5,
            palette: Palette::Blend,
        },
    },
    VisualPreset {
        name: "Orbit",
        visualizer: Some(Visualizer::Radial),
        style: VisualStyle {
            glow: true,
            grid: false,
            mirror: true,
            thickness: 2.0,
            fade: 0.5,
            palette: Palette::Accent,
        },
    },
    VisualPreset {
        name: "Vector",
        visualizer: Some(Visualizer::Vector),
        style: VisualStyle {
            glow: true,
            grid: true,
            mirror: false,
            thickness: 2.5,
            fade: 0.15,
            palette: Palette::Blend,
        },
    },
    VisualPreset {
        name: "Tide",
        visualizer: Some(Visualizer::Waves),
        style: VisualStyle {
            glow: false,
            grid: false,
            mirror: false,
            thickness: 1.5,
            fade: 0.2,
            palette: Palette::Accent,
        },
    },
    VisualPreset {
        name: "Fireworks",
        visualizer: Some(Visualizer::Particles),
        style: VisualStyle {
            glow: true,
            grid: false,
            mirror: false,
            thickness: 2.0,
            fade: 0.5,
            palette: Palette::Blend,
        },
    },
    VisualPreset {
        name: "Off",
        visualizer: None,
        style: VisualStyle::DEFAULT,
    },
];

/// The preset names, in table order.
#[must_use]
pub fn preset_names() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.name).collect()
}

/// Index of the preset called `name`.
#[must_use]
pub fn preset_index(name: &str) -> Option<usize> {
    PRESETS.iter().position(|p| p.name == name)
}

/// The preset at `index`, clamped into the table.
#[must_use]
pub fn preset(index: usize) -> &'static VisualPreset {
    &PRESETS[index.min(PRESETS.len() - 1)]
}

/// Draws `kind` into the available space.
pub fn visualizer(
    ui: &mut Ui,
    theme: &Theme,
    kind: Visualizer,
    input: &VisualInput<'_>,
    style: &VisualStyle,
) -> Response {
    match kind {
        Visualizer::Scope => scope(ui, theme, input, style),
        Visualizer::Bars => bars(ui, theme, input, style),
        Visualizer::Radial => radial(ui, theme, input, style),
        Visualizer::Vector => vector(ui, theme, input, style),
        Visualizer::Waves => waves(ui, theme, input, style),
        Visualizer::Particles => particles(ui, theme, input, style),
    }
}

// ---------------------------------------------------------------------------------------------
// Shared helpers.

fn safe_unit(v: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
}

fn safe_sample(v: f32) -> f32 {
    if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }
}

/// Allocates the widget's rect (the available space, at least [`MIN_SIZE`]) and tags it.
fn allocate(ui: &mut Ui, kind: Visualizer) -> (Rect, Response) {
    let size = ui.available_size_before_wrap().max(MIN_SIZE);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Image, ui.is_enabled(), kind.widget_label()));
    (rect, response)
}

/// The inset-m backdrop with the optional faint grid; returns the painter clipped to it.
fn backdrop(ui: &Ui, theme: &Theme, rect: Rect, style: &VisualStyle) -> Painter {
    let painter = ui.painter().with_clip_rect(rect);
    paint_soft_rect(&painter, rect, PANEL_RADIUS, Depth::InsetM, theme);
    if style.grid {
        let color = theme.shadow_dark.gamma_multiply(0.45);
        let stroke = Stroke::new(1.0, color);
        let inner = rect.shrink(4.0);
        for i in 1..4 {
            let x = inner.min.x + inner.width() * i as f32 / 4.0;
            painter.vline(x, inner.y_range(), stroke);
        }
        painter.hline(inner.x_range(), inner.center().y, stroke);
    }
    painter
}

fn glow_color(theme: &Theme) -> Color32 {
    theme.glow.gamma_multiply(0.55)
}

/// Clamped-to-u8 corner radius.
fn corner(r: f32) -> u8 {
    r.round().clamp(0.0, 255.0) as u8
}

/// Adds an axis-aligned quad of half size `h` around `p` with one colour.
fn add_point_quad(mesh: &mut Mesh, p: Pos2, h: f32, color: Color32) {
    let i = mesh.vertices.len() as u32;
    mesh.colored_vertex(pos2(p.x - h, p.y - h), color);
    mesh.colored_vertex(pos2(p.x + h, p.y - h), color);
    mesh.colored_vertex(pos2(p.x + h, p.y + h), color);
    mesh.colored_vertex(pos2(p.x - h, p.y + h), color);
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
}

/// Elapsed seconds between two wall times, clamped to a sane frame length.
fn frame_dt(last: f64, now: f64) -> f32 {
    if last <= 0.0 || !now.is_finite() || !last.is_finite() {
        return 0.0;
    }
    ((now - last) as f32).clamp(0.0, 0.1)
}

// ---------------------------------------------------------------------------------------------
// Scope.

/// Index of the first rising zero crossing that still leaves `span` samples after it, else 0.
#[must_use]
pub fn trigger_index(samples: &[f32], span: usize) -> usize {
    let limit = samples.len().saturating_sub(span);
    (1..=limit)
        .find(|&i| samples[i - 1] < 0.0 && samples[i] >= 0.0)
        .unwrap_or(0)
}

/// Oscilloscope: the newest half of the window from a rising zero crossing, one point per
/// pixel, accent line of `thickness` with a 3x wide glow underneath; mirrored copy at 45 %
/// when `mirror`.
pub fn scope(ui: &mut Ui, theme: &Theme, input: &VisualInput<'_>, style: &VisualStyle) -> Response {
    let (rect, response) = allocate(ui, Visualizer::Scope);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = backdrop(ui, theme, rect, style);
    let inner = rect.shrink2(vec2(6.0, 4.0));
    let color = style.palette.color(theme, 0.5);
    let thickness = style.thickness();
    let samples = input.samples;
    if samples.len() < 4 || inner.width() < 2.0 {
        painter.hline(
            inner.x_range(),
            inner.center().y,
            Stroke::new(thickness, color.gamma_multiply(0.6)),
        );
        return response;
    }
    let span = (samples.len() / 2).max(2);
    let start = trigger_index(samples, span);
    let n = (inner.width().floor() as usize).clamp(2, span);
    let amp = inner.height() * 0.5 * 0.92;
    let cy = inner.center().y;
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let idx = start + i * (span - 1) / (n - 1);
        let s = safe_sample(samples[idx.min(samples.len() - 1)]);
        let x = inner.min.x + inner.width() * i as f32 / (n - 1) as f32;
        points.push(pos2(x, cy - s * amp));
    }
    if style.glow {
        painter.add(Shape::line(
            points.clone(),
            Stroke::new(thickness * 3.0, glow_color(theme)),
        ));
    }
    if style.mirror {
        let flipped: Vec<Pos2> = points.iter().map(|p| pos2(p.x, 2.0 * cy - p.y)).collect();
        painter.add(Shape::line(
            flipped,
            Stroke::new(thickness, color.gamma_multiply(0.45)),
        ));
    }
    painter.add(Shape::line(points, Stroke::new(thickness, color)));
    response
}

// ---------------------------------------------------------------------------------------------
// Bars.

/// Groups a spectrum into [`BAR_COUNT`] bar heights `0..=1` (mean of each bin group; an empty
/// spectrum gives zeros, NaN reads as 0).
#[must_use]
pub fn bar_values(spectrum: &[f32]) -> [f32; BAR_COUNT] {
    let mut out = [0.0_f32; BAR_COUNT];
    let len = spectrum.len();
    if len == 0 {
        return out;
    }
    for (i, v) in out.iter_mut().enumerate() {
        let lo = (i * len / BAR_COUNT).min(len - 1);
        let hi = ((i + 1) * len / BAR_COUNT).clamp(lo + 1, len);
        let sum: f32 = spectrum[lo..hi].iter().map(|s| safe_unit(*s)).sum();
        *v = sum / (hi - lo) as f32;
    }
    out
}

/// Peak-hold caps of [`bars`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct PeakHold {
    hold: [f32; BAR_COUNT],
    last_time: f64,
}

impl Default for PeakHold {
    fn default() -> Self {
        Self {
            hold: [0.0; BAR_COUNT],
            last_time: 0.0,
        }
    }
}

impl PeakHold {
    /// A new peak jumps the cap up; otherwise it falls by [`PEAK_HOLD_FALL`] per second.
    fn update(&mut self, values: &[f32; BAR_COUNT], now: f64) {
        let dt = frame_dt(self.last_time, now);
        self.last_time = now;
        for (h, v) in self.hold.iter_mut().zip(values) {
            *h = if *v >= *h {
                *v
            } else {
                (*h - PEAK_HOLD_FALL * dt).max(*v)
            };
        }
    }
}

/// Paints the bar field into `inner`. `alpha` scales every colour (the particles use a faint
/// copy); `caps` draws the peak-hold caps.
fn paint_bars(
    painter: &Painter,
    theme: &Theme,
    inner: Rect,
    values: &[f32; BAR_COUNT],
    hold: Option<&[f32; BAR_COUNT]>,
    style: &VisualStyle,
    alpha: f32,
) {
    let gap = (inner.width() / BAR_COUNT as f32 * 0.25).clamp(1.0, 4.0);
    let bar_w = ((inner.width() - gap * (BAR_COUNT as f32 - 1.0)) / BAR_COUNT as f32).max(1.0);
    let (base_y, max_h) = if style.mirror {
        (inner.center().y, inner.height() * 0.5)
    } else {
        (inner.max.y, inner.height())
    };
    let radius = corner(bar_w * 0.5);
    let top_round = CornerRadius {
        nw: radius,
        ne: radius,
        sw: 0,
        se: 0,
    };
    let bottom_round = CornerRadius {
        nw: 0,
        ne: 0,
        sw: radius,
        se: radius,
    };
    let stub_h = (max_h * 0.12).clamp(2.0, 4.0);
    for (i, &v) in values.iter().enumerate() {
        let x0 = inner.min.x + i as f32 * (bar_w + gap);
        if v < QUIET {
            let stub = Rect::from_min_max(pos2(x0, base_y - stub_h), pos2(x0 + bar_w, base_y));
            if alpha >= 1.0 {
                paint_soft_rect(painter, stub, bar_w * 0.5, Depth::RaisedS, theme);
            } else {
                painter.rect_filled(
                    stub,
                    CornerRadius::same(radius),
                    theme.shadow_dark.gamma_multiply(alpha),
                );
            }
            continue;
        }
        let h = (v * max_h).max(stub_h);
        let bar = Rect::from_min_max(pos2(x0, base_y - h), pos2(x0 + bar_w, base_y));
        let color = style.palette.color(theme, v).gamma_multiply(alpha);
        if style.glow && v > 0.5 {
            let glow = glow_color(theme).gamma_multiply(alpha);
            painter.rect_filled(bar.expand(2.0), CornerRadius::same(radius + 2), glow);
            if style.mirror {
                let reflect = Rect::from_min_max(pos2(x0, base_y), pos2(x0 + bar_w, base_y + h));
                painter.rect_filled(reflect.expand(2.0), CornerRadius::same(radius + 2), glow);
            }
        }
        painter.rect_filled(bar, top_round, color);
        if style.mirror {
            let reflect = Rect::from_min_max(pos2(x0, base_y), pos2(x0 + bar_w, base_y + h));
            painter.rect_filled(reflect, bottom_round, color.gamma_multiply(0.55));
        }
        if let Some(hold) = hold
            && hold[i] > v + 0.02
        {
            let y = base_y - hold[i] * max_h;
            let cap = Stroke::new(2.0, theme.accent_hover.gamma_multiply(alpha));
            painter.hline(x0..=x0 + bar_w, y, cap);
            if style.mirror {
                painter.hline(x0..=x0 + bar_w, base_y + hold[i] * max_h, cap);
            }
        }
    }
}

/// Spectrum bars: [`BAR_COUNT`] bars with rounded tops, raised-s stubs when quiet, accent
/// fill rising with level, accent-hover peak-hold caps falling at [`PEAK_HOLD_FALL`] per
/// second. `mirror` draws the bars from the centre line both ways.
pub fn bars(ui: &mut Ui, theme: &Theme, input: &VisualInput<'_>, style: &VisualStyle) -> Response {
    let (rect, response) = allocate(ui, Visualizer::Bars);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = backdrop(ui, theme, rect, style);
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let values = bar_values(input.spectrum);
    let id: Id = response.id.with("peak_hold");
    let time = input.time;
    let hold = ui.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<PeakHold>(id);
        state.update(&values, time);
        state.hold
    });
    paint_bars(&painter, theme, inner, &values, Some(&hold), style, 1.0);
    response
}

// ---------------------------------------------------------------------------------------------
// Radial.

/// Start times of the kick rings of [`radial`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rings {
    start: [f64; 4],
    next: usize,
}

impl Default for Rings {
    fn default() -> Self {
        Self {
            start: [f64::NEG_INFINITY; 4],
            next: 0,
        }
    }
}

/// Circular spectrum: bars radiate from a centre circle whose radius follows the master
/// peak; every kick hit starts a ring that expands to the edge and fades over
/// [`RING_SECONDS`]. `mirror` draws the spectrum symmetrically on both halves.
pub fn radial(ui: &mut Ui, theme: &Theme, input: &VisualInput<'_>, style: &VisualStyle) -> Response {
    let (rect, response) = allocate(ui, Visualizer::Radial);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = backdrop(ui, theme, rect, style);
    let c = rect.center();
    let outer = rect.width().min(rect.height()) * 0.5 - 6.0;
    if outer < 6.0 {
        return response;
    }
    let peak = safe_unit(input.peak.0.max(input.peak.1));
    let r0 = outer * (0.22 + 0.12 * peak);
    let thickness = style.thickness();

    // Bars.
    let spectrum = input.spectrum;
    let n = spectrum.len();
    if n > 0 {
        let total = if style.mirror { 2 * n } else { n };
        let width = (std::f32::consts::TAU * (r0 + 2.0) / total as f32 * 0.65).max(1.0);
        let reach = (outer - r0 - 3.0).max(0.0);
        for (i, &raw) in spectrum.iter().enumerate() {
            let v = safe_unit(raw);
            if v < QUIET * 0.5 {
                continue;
            }
            let len = v * reach;
            let color = style.palette.color(theme, v);
            let angles: [Option<f32>; 2] = if style.mirror {
                let a = -std::f32::consts::FRAC_PI_2 + (i as f32 + 0.5) / n as f32 * std::f32::consts::PI;
                [Some(a), Some(-std::f32::consts::PI - a)]
            } else {
                [
                    Some(-std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU),
                    None,
                ]
            };
            for angle in angles.into_iter().flatten() {
                let dir = vec2(angle.cos(), angle.sin());
                let a = c + dir * (r0 + 2.0);
                let b = c + dir * (r0 + 2.0 + len);
                if style.glow && v > 0.5 {
                    painter.line_segment([a, b], Stroke::new(width * 2.5, glow_color(theme)));
                }
                painter.line_segment([a, b], Stroke::new(width, color));
            }
        }
    }

    // Kick rings.
    let id: Id = response.id.with("rings");
    let (time, hit) = (input.time, input.hits & 1 != 0);
    let rings = ui.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<Rings>(id);
        if hit {
            state.start[state.next] = time;
            state.next = (state.next + 1) % state.start.len();
        }
        *state
    });
    for start in rings.start {
        let age = (time - start) as f32;
        if !(0.0..RING_SECONDS).contains(&age) {
            continue;
        }
        let t = age / RING_SECONDS;
        let radius = r0 + t * (outer - r0);
        let color = theme.accent.gamma_multiply((1.0 - t) * 0.9);
        if style.glow {
            painter.circle_stroke(c, radius, Stroke::new(thickness * 3.0, glow_color(theme).gamma_multiply(1.0 - t)));
        }
        painter.circle_stroke(c, radius, Stroke::new(thickness, color));
    }

    // Centre circle (over the bars' inner ends), with an accent core on loud peaks.
    paint_soft_circle(&painter, c, r0, Depth::RaisedM, theme);
    if peak > 0.05 {
        let core = r0 * 0.5 * peak;
        painter.circle_filled(c, core, style.palette.color(theme, peak));
    }
    response
}

// ---------------------------------------------------------------------------------------------
// Vector.

/// Pearson correlation of two channels over their common length: 1 = mono, -1 = anti-phase,
/// 0 for silence or unrelated signals.
#[must_use]
pub fn correlation(left: &[f32], right: &[f32]) -> f32 {
    let n = left.len().min(right.len());
    if n == 0 {
        return 0.0;
    }
    let (mut ll, mut rr, mut lr) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (l, r) in left[..n].iter().zip(&right[..n]) {
        let (l, r) = (f64::from(safe_sample(*l)), f64::from(safe_sample(*r)));
        ll += l * l;
        rr += r * r;
        lr += l * r;
    }
    let denom = (ll * rr).sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        (lr / denom).clamp(-1.0, 1.0) as f32
    }
}

/// Vectorscope: the newest [`VECTOR_POINTS`] stereo samples as a point cloud (`x = L - R`,
/// `y = L + R`), each a small quad whose alpha falls with age (`fade` sets how fast), with a
/// glow pass of larger quads and a small stereo [`correlation`] meter along the bottom.
pub fn vector(ui: &mut Ui, theme: &Theme, input: &VisualInput<'_>, style: &VisualStyle) -> Response {
    let (rect, response) = allocate(ui, Visualizer::Vector);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = backdrop(ui, theme, rect, style);
    let c = rect.center();
    let left = input.samples;
    let right = input.samples_r.unwrap_or(left);
    let n = left.len().min(right.len());
    let thickness = style.thickness();
    let meter_h = 6.0_f32;
    let sx = (rect.width() * 0.5 - 8.0).max(1.0);
    let sy = (rect.height() * 0.5 - 8.0 - meter_h).max(1.0);
    if n == 0 {
        painter.circle_filled(c, thickness, style.palette.color(theme, 0.0));
    } else {
        let count = n.min(VECTOR_POINTS);
        let start = n - count;
        let exponent = 0.5 + 2.5 * style.fade();
        let q = thickness * 0.75;
        let mut mesh = Mesh::default();
        mesh.reserve_vertices(count * 4);
        mesh.reserve_triangles(count * 2);
        let mut glow = Mesh::default();
        if style.glow {
            glow.reserve_vertices(count * 4);
            glow.reserve_triangles(count * 2);
        }
        for i in 0..count {
            let age = (i + 1) as f32 / count as f32;
            let alpha = age.powf(exponent);
            let (l, r) = (safe_sample(left[start + i]), safe_sample(right[start + i]));
            let x = ((l - r) * std::f32::consts::FRAC_1_SQRT_2).clamp(-1.0, 1.0);
            let y = -((l + r) * std::f32::consts::FRAC_1_SQRT_2).clamp(-1.0, 1.0);
            let p = pos2(c.x + x * sx, c.y - meter_h * 0.5 + y * sy);
            let color = style.palette.color(theme, age);
            add_point_quad(&mut mesh, p, q, color.gamma_multiply(alpha));
            if style.glow {
                add_point_quad(&mut glow, p, q * 3.0, glow_color(theme).gamma_multiply(alpha * 0.35));
            }
        }
        if style.glow {
            painter.add(Shape::mesh(glow));
        }
        painter.add(Shape::mesh(mesh));
    }

    // Correlation meter: an inset-s track with an accent dot from -1 (left) to +1 (right).
    let track_w = (rect.width() * 0.4).clamp(20.0, 80.0);
    let track = Rect::from_center_size(
        pos2(c.x, rect.max.y - 6.0 - meter_h * 0.5),
        vec2(track_w, meter_h),
    );
    paint_soft_rect(&painter, track, meter_h * 0.5, Depth::InsetS, theme);
    painter.vline(
        track.center().x,
        track.y_range(),
        Stroke::new(1.0, theme.shadow_dark.gamma_multiply(0.7)),
    );
    let corr = correlation(left, right);
    let x = track.center().x + corr * (track.width() * 0.5 - meter_h * 0.5);
    painter.circle_filled(pos2(x, track.center().y), meter_h * 0.5, theme.accent);
    response
}

// ---------------------------------------------------------------------------------------------
// Waves.

/// Envelope history of [`waves`]: the last [`WAVE_LAYERS`] windows, [`WAVE_POINTS`] each.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WaveHistory {
    layers: [[f32; WAVE_POINTS]; WAVE_LAYERS],
    /// Slot the next window goes into.
    head: usize,
    /// Windows stored so far (`0..=WAVE_LAYERS`).
    len: usize,
}

impl Default for WaveHistory {
    fn default() -> Self {
        Self {
            layers: [[0.0; WAVE_POINTS]; WAVE_LAYERS],
            head: 0,
            len: 0,
        }
    }
}

impl WaveHistory {
    fn push(&mut self, layer: [f32; WAVE_POINTS]) {
        self.layers[self.head] = layer;
        self.head = (self.head + 1) % WAVE_LAYERS;
        self.len = (self.len + 1).min(WAVE_LAYERS);
    }

    /// Layer `j` with 0 the oldest and `len - 1` the newest.
    fn layer(&self, j: usize) -> &[f32; WAVE_POINTS] {
        let oldest = (self.head + WAVE_LAYERS - self.len) % WAVE_LAYERS;
        &self.layers[(oldest + j) % WAVE_LAYERS]
    }
}

/// Reduces a window to [`WAVE_POINTS`] envelope values `0..=1` (mean magnitude per bucket,
/// scaled so a -6 dBFS tone fills the strip).
#[must_use]
pub fn wave_envelope(samples: &[f32]) -> [f32; WAVE_POINTS] {
    let mut out = [0.0_f32; WAVE_POINTS];
    let len = samples.len();
    if len == 0 {
        return out;
    }
    for (i, v) in out.iter_mut().enumerate() {
        let lo = (i * len / WAVE_POINTS).min(len - 1);
        let hi = ((i + 1) * len / WAVE_POINTS).clamp(lo + 1, len);
        let sum: f32 = samples[lo..hi].iter().map(|s| safe_sample(*s).abs()).sum();
        *v = (sum / (hi - lo) as f32 * 2.5).min(1.0);
    }
    out
}

/// Wave strip: each frame's envelope becomes a new front layer; the last [`WAVE_LAYERS`]
/// layers stack from back (top, faint) to front (bottom, full), each filled with the surface
/// colour so it occludes the ones behind. `fade` sets how faint the oldest layer is;
/// `mirror` draws the hills both ways.
pub fn waves(ui: &mut Ui, theme: &Theme, input: &VisualInput<'_>, style: &VisualStyle) -> Response {
    let (rect, response) = allocate(ui, Visualizer::Waves);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = backdrop(ui, theme, rect, style);
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let id: Id = response.id.with("wave_history");
    let envelope = wave_envelope(input.samples);
    let history = ui.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<WaveHistory>(id);
        state.push(envelope);
        *state
    });
    let len = history.len;
    if len == 0 || inner.width() < 2.0 {
        return response;
    }
    let thickness = style.thickness() * 0.75;
    let amp_h = inner.height() * if style.mirror { 0.25 } else { 0.45 };
    let dy = inner.height() * 0.75 / WAVE_LAYERS as f32;
    let front_y = if style.mirror {
        inner.max.y - amp_h
    } else {
        inner.max.y
    };
    let min_alpha = 1.0 - style.fade() * 0.9;
    let mut points = Vec::with_capacity(WAVE_POINTS);
    for j in 0..len {
        let layer = history.layer(j);
        let base_y = front_y - (len - 1 - j) as f32 * dy;
        let frac = if len > 1 {
            j as f32 / (len - 1) as f32
        } else {
            1.0
        };
        let alpha = min_alpha + (1.0 - min_alpha) * frac;
        points.clear();
        for (i, v) in layer.iter().enumerate() {
            let x = inner.min.x + inner.width() * i as f32 / (WAVE_POINTS - 1) as f32;
            points.push(pos2(x, base_y - v * amp_h));
        }
        // Occlusion: a quad strip from the line down to the baseline (up to the mirrored
        // line when mirrored), in the surface colour.
        let mut fill = Mesh::default();
        fill.reserve_vertices((WAVE_POINTS - 1) * 4);
        fill.reserve_triangles((WAVE_POINTS - 1) * 2);
        for w in points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (a2, b2) = if style.mirror {
                (pos2(a.x, 2.0 * base_y - a.y), pos2(b.x, 2.0 * base_y - b.y))
            } else {
                (pos2(a.x, base_y), pos2(b.x, base_y))
            };
            let i = fill.vertices.len() as u32;
            fill.colored_vertex(a, theme.surface);
            fill.colored_vertex(b, theme.surface);
            fill.colored_vertex(b2, theme.surface);
            fill.colored_vertex(a2, theme.surface);
            fill.add_triangle(i, i + 1, i + 2);
            fill.add_triangle(i, i + 2, i + 3);
        }
        painter.add(Shape::mesh(fill));
        let color = style.palette.color(theme, frac).gamma_multiply(alpha);
        if style.glow && j + 1 == len {
            painter.add(Shape::line(
                points.clone(),
                Stroke::new(thickness * 3.0, glow_color(theme)),
            ));
        }
        if style.mirror {
            let flipped: Vec<Pos2> = points.iter().map(|p| pos2(p.x, 2.0 * base_y - p.y)).collect();
            painter.add(Shape::line(flipped, Stroke::new(thickness, color)));
        }
        painter.add(Shape::line(points.clone(), Stroke::new(thickness, color)));
    }
    response
}

// ---------------------------------------------------------------------------------------------
// Particles.

/// One particle of [`particles`]; `life <= 0` means the slot is free.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Particle {
    /// Position relative to the widget's top-left, in points.
    pos: Vec2,
    vel: Vec2,
    life: f32,
    life0: f32,
    row: u8,
    size: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            life: 0.0,
            life0: 1.0,
            row: 0,
            size: 2.0,
        }
    }
}

/// The fixed particle pool plus its PRNG and clock.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ParticleField {
    particles: [Particle; MAX_PARTICLES],
    rng: u32,
    last_time: f64,
}

impl Default for ParticleField {
    fn default() -> Self {
        Self {
            particles: [Particle::default(); MAX_PARTICLES],
            rng: 0x9E37_79B9,
            last_time: 0.0,
        }
    }
}

/// Per drum row (kick, snare, clap, closed hat, open hat): burst size, origin x (fraction of
/// the width), launch speed (pt/s), lifetime (s) and radius (pt).
const ROW_BURST: [(usize, f32, f32, f32, f32); 5] = [
    (28, 0.5, 300.0, 1.0, 4.0),
    (20, 0.3, 260.0, 0.8, 3.0),
    (16, 0.7, 240.0, 0.7, 2.5),
    (10, 0.15, 200.0, 0.5, 1.8),
    (12, 0.85, 220.0, 0.6, 2.0),
];

impl ParticleField {
    /// xorshift32, `0..1`.
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Spawns a burst for every row bit in `hits` inside a widget of `size`.
    fn spawn(&mut self, hits: u8, size: Vec2, thickness: f32) {
        for (row, &(count, ox, speed, life, radius)) in ROW_BURST.iter().enumerate() {
            if hits & (1 << row) == 0 {
                continue;
            }
            let origin = vec2(size.x * ox, size.y * 0.78);
            for _ in 0..count {
                let angle = -std::f32::consts::FRAC_PI_2 + (self.rand() - 0.5) * 1.6;
                let v = speed * (0.4 + 0.6 * self.rand());
                let slot = self.free_slot();
                self.particles[slot] = Particle {
                    pos: origin + vec2((self.rand() - 0.5) * 6.0, 0.0),
                    vel: vec2(angle.cos(), angle.sin()) * v,
                    life,
                    life0: life,
                    row: row as u8,
                    size: radius * thickness * 0.5,
                };
            }
        }
    }

    /// A dead slot, else the one closest to dying.
    fn free_slot(&self) -> usize {
        let mut best = 0;
        let mut best_life = f32::INFINITY;
        for (i, p) in self.particles.iter().enumerate() {
            if p.life <= 0.0 {
                return i;
            }
            if p.life < best_life {
                best_life = p.life;
                best = i;
            }
        }
        best
    }

    /// Integrates gravity and drains life; `fade` speeds the draining up.
    fn step(&mut self, now: f64, fade: f32) {
        let dt = frame_dt(self.last_time, now);
        self.last_time = now;
        if dt <= 0.0 {
            return;
        }
        let drain = 0.5 + fade * 1.5;
        for p in self.particles.iter_mut().filter(|p| p.life > 0.0) {
            p.vel.y += PARTICLE_GRAVITY * dt;
            p.pos += p.vel * dt;
            p.life -= dt * drain;
        }
    }

    fn alive(&self) -> usize {
        self.particles.iter().filter(|p| p.life > 0.0).count()
    }
}

/// Particle bursts: every drum hit launches a burst (kick biggest, hats smallest; tint from
/// accent to accent-hover by row) that falls under gravity and fades out, over a faint copy
/// of the spectrum bars. At most [`MAX_PARTICLES`] live at once (the oldest are recycled).
pub fn particles(
    ui: &mut Ui,
    theme: &Theme,
    input: &VisualInput<'_>,
    style: &VisualStyle,
) -> Response {
    let (rect, response) = allocate(ui, Visualizer::Particles);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = backdrop(ui, theme, rect, style);
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let values = bar_values(input.spectrum);
    let faint = VisualStyle {
        glow: false,
        mirror: false,
        ..*style
    };
    paint_bars(&painter, theme, inner, &values, None, &faint, 0.3);

    let id: Id = response.id.with("particles");
    let (time, hits, size, thickness, fade) =
        (input.time, input.hits, rect.size(), style.thickness(), style.fade());
    let field = ui.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<ParticleField>(id);
        state.step(time, fade);
        if hits != 0 {
            state.spawn(hits, size, thickness);
        }
        *state
    });
    for p in field.particles.iter().filter(|p| p.life > 0.0) {
        let t = safe_unit(p.life / p.life0.max(1e-3));
        let center = rect.min + p.pos;
        if !rect.expand(8.0).contains(center) {
            continue;
        }
        let radius = p.size * (0.6 + 0.4 * t);
        let color = style.palette.color(theme, f32::from(p.row) / 4.0);
        if style.glow {
            painter.circle_filled(center, radius * 2.2, glow_color(theme).gamma_multiply(t * 0.5));
        }
        painter.circle_filled(center, radius, color.gamma_multiply(t));
    }
    response
}

/// Live particles of the [`particles`] widget that produced `response` (tests).
#[must_use]
pub fn live_particles(ui: &Ui, response: &Response) -> usize {
    let id: Id = response.id.with("particles");
    ui.data(|d| d.get_temp::<ParticleField>(id).map_or(0, |f| f.alive()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_finds_the_first_rising_crossing_that_leaves_room() {
        let s = [0.5, 0.2, -0.1, -0.3, 0.1, 0.4, -0.2, 0.3];
        assert_eq!(trigger_index(&s, 4), 4);
        // Only crossings that leave `span` samples count; otherwise the start is used.
        assert_eq!(trigger_index(&s, 6), 0);
        assert_eq!(trigger_index(&[], 4), 0);
        assert_eq!(trigger_index(&[0.1; 8], 4), 0);
    }

    #[test]
    fn bar_values_group_bins_and_tolerate_odd_lengths() {
        assert_eq!(bar_values(&[]), [0.0; BAR_COUNT]);
        let mut spectrum = [0.0_f32; SPECTRUM_BINS];
        spectrum[0] = 1.0;
        spectrum[1] = 0.5;
        spectrum[63] = f32::NAN;
        let bars = bar_values(&spectrum);
        assert!((bars[0] - 0.75).abs() < 1e-6);
        assert_eq!(bars[1], 0.0);
        assert_eq!(bars[31], 0.0, "NaN reads as 0");
        let short = bar_values(&[1.0, 1.0, 1.0]);
        assert!(short.iter().all(|v| (*v - 1.0).abs() < 1e-6));
        let one = bar_values(&[2.0]);
        assert!(one.iter().all(|v| *v == 1.0), "clamped to 1");
    }

    #[test]
    fn correlation_reads_mono_anti_phase_and_silence() {
        let l: Vec<f32> = (0..64).map(|i| (i as f32 * 0.3).sin()).collect();
        let r: Vec<f32> = l.iter().map(|s| -s).collect();
        assert!((correlation(&l, &l) - 1.0).abs() < 1e-5);
        assert!((correlation(&l, &r) + 1.0).abs() < 1e-5);
        assert_eq!(correlation(&[0.0; 16], &[0.0; 16]), 0.0);
        assert_eq!(correlation(&[], &l), 0.0);
        let nan = [f32::NAN; 8];
        assert_eq!(correlation(&nan, &nan), 0.0);
    }

    #[test]
    fn wave_envelope_is_unit_bounded() {
        assert_eq!(wave_envelope(&[]), [0.0; WAVE_POINTS]);
        let loud = [1.0_f32; 2048];
        assert!(wave_envelope(&loud).iter().all(|v| *v == 1.0));
        let quiet = [0.1_f32; 100];
        assert!(wave_envelope(&quiet).iter().all(|v| (*v - 0.25).abs() < 1e-5));
    }

    #[test]
    fn presets_cover_every_visualizer_once_plus_off() {
        assert_eq!(PRESETS.len(), 7);
        assert!(PRESETS[OFF_PRESET].is_off());
        assert_eq!(PRESETS[OFF_PRESET].name, "Off");
        let names = preset_names();
        for (i, name) in names.iter().enumerate() {
            assert_eq!(preset_index(name), Some(i));
            assert!(names[..i].iter().all(|n| n != name), "duplicate {name}");
        }
        for kind in Visualizer::ALL {
            assert_eq!(
                PRESETS.iter().filter(|p| p.visualizer == Some(kind)).count(),
                1,
                "{} should have exactly one preset",
                kind.label()
            );
        }
        assert_eq!(preset(99).name, "Off");
        assert_eq!(preset(0).name, "Soft Scope");
        assert_eq!(preset_index("nope"), None);
    }

    #[test]
    fn palette_blend_moves_from_accent_to_hover() {
        let theme = Theme::dark();
        assert_eq!(Palette::Accent.color(&theme, 0.9), theme.accent);
        assert_eq!(Palette::Hover.color(&theme, 0.1), theme.accent_hover);
        assert_eq!(Palette::Blend.color(&theme, 0.0), theme.accent);
        assert_eq!(Palette::Blend.color(&theme, 1.0), theme.accent_hover);
        assert_eq!(Palette::Blend.color(&theme, f32::NAN), theme.accent);
    }

    #[test]
    fn peak_hold_jumps_up_and_falls_slowly() {
        let mut hold = PeakHold::default();
        let mut values = [0.0_f32; BAR_COUNT];
        values[3] = 0.8;
        hold.update(&values, 1.0);
        assert_eq!(hold.hold[3], 0.8);
        values[3] = 0.1;
        hold.update(&values, 1.1);
        assert!((hold.hold[3] - (0.8 - PEAK_HOLD_FALL * 0.1)).abs() < 1e-4);
        // A frame longer than 100 ms counts as 100 ms (no jump after a stall).
        hold.update(&values, 10.0);
        assert!((hold.hold[3] - (0.8 - PEAK_HOLD_FALL * 0.2)).abs() < 1e-4);
        for i in 0..30 {
            hold.update(&values, 10.1 + f64::from(i) * 0.1);
        }
        assert_eq!(hold.hold[3], 0.1, "never below the live value");
    }

    #[test]
    fn particle_field_is_bounded_and_recycles() {
        let mut field = ParticleField::default();
        field.spawn(0b1_1111, vec2(200.0, 100.0), 2.0);
        let total: usize = ROW_BURST.iter().map(|b| b.0).sum();
        assert_eq!(field.alive(), total);
        for _ in 0..10 {
            field.spawn(0b1_1111, vec2(200.0, 100.0), 2.0);
        }
        assert!(field.alive() <= MAX_PARTICLES);
        field.step(1.0, 1.0);
        field.step(1.05, 1.0);
        assert!(field.particles.iter().any(|p| p.life > 0.0 && p.vel.y > -300.0));
        for i in 0..100 {
            field.step(1.1 + i as f64 * 0.05, 1.0);
        }
        assert_eq!(field.alive(), 0, "everything fades out");
    }

    #[test]
    fn wave_history_orders_layers_oldest_first() {
        let mut h = WaveHistory::default();
        for k in 0..(WAVE_LAYERS + 3) {
            h.push([k as f32; WAVE_POINTS]);
        }
        assert_eq!(h.len, WAVE_LAYERS);
        assert_eq!(h.layer(0)[0], 3.0);
        assert_eq!(h.layer(WAVE_LAYERS - 1)[0], (WAVE_LAYERS + 2) as f32);
    }
}
