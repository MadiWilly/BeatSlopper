//! `neu`: neumorphic egui widget kit and theme tokens for BeatSlopper.
//!
//! Tokens come from `design/WINNER.md` (round 2, "Soft Console"). Every control is the surface
//! colour plus at most two soft shadows; ON states are accent fill plus an accent glow.
//!
//! * [`theme`]: the token transcription ([`Theme`], [`ThemeMode`], [`ShadowPair`]).
//! * [`shadow`]: the depth painter ([`Depth`], [`paint_soft_rect`], [`paint_soft_circle`]).
//! * [`text`]: the type scale ([`TextKind`]) and tracked caps labels.
//! * [`panel`], [`button`]: the M0 widgets.
//! * [`piano_keys`], [`stepper`]: the M1 widgets (the on-screen keyboard and the "(-) n (+)"
//!   control).
//! * [`knob`], [`segmented`], [`slider`]: the M2 controls (rotary knobs and the concentric
//!   dial, the segmented selector, the thin vertical slider and the indicator lamp).
//! * [`step_cell`]: the M3 sequencer pieces (step cells, the playhead dot row, beat numbers
//!   and the row geometry helpers).
//! * M4 polish: the brand wordmark ([`brand`], [`TextKind::Brand`]) and a `"label: value"`
//!   tooltip ([`tooltip_text`]) on every knob, dial and slider, styled by [`Theme::apply`].

pub mod button;
pub mod knob;
pub mod panel;
pub mod piano_keys;
pub mod segmented;
pub mod shadow;
pub mod slider;
pub mod step_cell;
pub mod stepper;
pub mod text;
pub mod theme;

pub use button::{
    Icon, button, icon_button, paint_icon, stepper_button, theme_toggle, toggle_button,
};
pub use knob::{DialSize, KnobSize, KnobSpec, dial, fmt_plain, knob, paint_arc, tooltip_text};
pub use panel::panel;
pub use piano_keys::{
    PianoKey, PianoModel, PianoOutput, key_at, key_layout, keyboard_size, keyboard_width,
    note_name, piano_keys,
};
pub use segmented::segmented;
pub use shadow::{
    Depth, paint_soft_circle, paint_soft_circle_with_fill, paint_soft_rect,
    paint_soft_rect_with_fill, soft_circle_shapes, soft_rect_shapes, soft_rect_shapes_with_fill,
};
pub use slider::{
    SLIDER_THUMB, SLIDER_TRACK, SLIDER_TRAVEL, SLIDER_WIDTH, SliderSpec, lamp, vslider,
    vslider_fraction_at, vslider_thumb_pos,
};
pub use stepper::{stepper, stepper_with};
pub use text::{TextKind, brand, display, label, title, value};
pub use theme::{POPUP_RADIUS, ShadowPair, TOOLTIP_DELAY, Theme, ThemeMode};

pub use step_cell::{
    ACCENT_MARKER, BEAT_NUMBER_HEIGHT, PLAYHEAD_DOT_CURRENT, PLAYHEAD_DOT_IDLE,
    PLAYHEAD_ROW_HEIGHT, SELECT_RING, STEP_CELL, STEP_GAP, STEP_GROUP, STEP_GROUP_GAP,
    StepCellSpec, StepRowOutput, beat_numbers, playhead_dot, playhead_row, playhead_tint,
    step_cell, step_cell_named, step_gap_after, step_index_at, step_row, step_row_width,
    step_x_offset, with_step_gaps,
};
