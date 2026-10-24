//! `neu`: neumorphic egui widget kit and theme tokens for BeatSlopper.
//!
//! Tokens come from `design/WINNER.md` (round 2, "Soft Console"). Every control is the surface
//! colour plus at most two soft shadows; ON states are accent fill plus an accent glow.
//!
//! * [`theme`]: the token transcription ([`Theme`], [`ThemeMode`], [`ShadowPair`]).
//! * [`shadow`]: the depth painter ([`Depth`], [`paint_soft_rect`], [`paint_soft_circle`]).
//! * [`text`]: the type scale ([`TextKind`]) and tracked caps labels.
//! * [`panel`], [`button`]: the M0 widgets.

pub mod button;
pub mod panel;
pub mod shadow;
pub mod text;
pub mod theme;

pub use button::{
    Icon, button, icon_button, paint_icon, stepper_button, theme_toggle, toggle_button,
};
pub use panel::panel;
pub use shadow::{
    Depth, paint_soft_circle, paint_soft_circle_with_fill, paint_soft_rect,
    paint_soft_rect_with_fill, soft_circle_shapes, soft_rect_shapes, soft_rect_shapes_with_fill,
};
pub use text::{TextKind, display, label, title, value};
pub use theme::{ShadowPair, Theme, ThemeMode};
