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
//! * M6 mixer: the tall [`fader`], the peak [`meter`] / [`stereo_meter`] (dB curve via
//!   [`meter_fraction`]) and the square [`toggle_button_sized`] for mute / solo.
//! * M7 editor: the [`piano_roll`] (notes as rounded bars on a step x pitch grid, with draw /
//!   move / resize / delete gestures and the geometry helpers [`cell_at`], [`note_rect`]).
//! * Visualizers ([`visualizer`]): six theme-matched drawings of the master output
//!   ([`scope`], [`bars`], [`radial`], [`vector`], [`waves`], [`particles`]) over a
//!   [`VisualInput`] with a [`VisualStyle`], and the [`PRESETS`] table ([`VisualPreset`]).
//! * QOL pass: [`value_edit`], the typed value entry (click a readout or right-click a
//!   control, type `128 bpm` / `-6 dB` / `2.4k`, Enter) and the wheel nudge shared by every
//!   knob, dial, slider and stepper ([`readout`], [`parse_value`], [`wheel_notches`]).

pub mod button;
pub mod knob;
pub mod meter;
pub mod panel;
pub mod piano_keys;
pub mod piano_roll;
pub mod segmented;
pub mod shadow;
pub mod slider;
pub mod step_cell;
pub mod stepper;
pub mod text;
pub mod theme;
pub mod value_edit;
pub mod visualizer;

pub use button::{
    Icon, SMALL_DEPTH_MAX, TOGGLE_SIZED_RADIUS, TOGGLE_SQUARE, button, icon_button, paint_icon,
    stepper_button, theme_toggle, toggle_button, toggle_button_sized,
};
pub use knob::{
    DIMMED_OPACITY, DialSize, KnobSize, KnobSpec, dial, fmt_plain, knob, knob_dimmed, knob_noted,
    knob_with, paint_arc, tooltip_text,
};
pub use meter::{
    METER_FLOOR_DB, METER_HOT_LEVEL, METER_HOT_SEPARATOR, METER_HOT_TOP, METER_TRACK,
    STEREO_METER_SIZE, STEREO_METER_TRACK_WIDTH, meter, meter_fraction, stereo_meter,
};
pub use panel::panel;
pub use piano_keys::{
    PianoKey, PianoModel, PianoOutput, key_at, key_layout, keyboard_size, keyboard_width,
    note_name, piano_keys,
};
pub use piano_roll::{
    ACCENT_DOT, ACCENT_DOT_INSET, ACCENT_THRESHOLD, ACCENT_VELOCITY, DEFAULT_VELOCITY,
    GHOST_OPACITY, GHOST_OUTLINE_OPACITY, GHOST_RADIUS, GhostNote, KEY_COLUMN_WIDTH, NOTE_RADIUS,
    PianoRollModel, PianoRollOutput, RESIZE_HANDLE, RollNote, black_row_tint, cell_at, ghost_fill,
    ghost_outline, ghost_rects, grid_rect, is_black_key, note_rect, piano_roll,
    piano_roll_with_ghosts, row_rect,
};
pub use segmented::{SEGMENTED_CHIP_HEIGHT, SEGMENTED_HEIGHT, chip_rect, segmented};
pub use shadow::{
    Depth, FOCUS_RING, FOCUS_RING_GAP, paint_focus_circle, paint_focus_rect, paint_soft_circle,
    paint_soft_circle_with_fill, paint_soft_rect, paint_soft_rect_with_fill, soft_circle_shapes,
    soft_rect_shapes, soft_rect_shapes_with_fill,
};
pub use slider::{
    FADER_TRACK, FADER_TRAVEL, SLIDER_THUMB, SLIDER_TRACK, SLIDER_TRAVEL, SLIDER_WIDTH, SliderSpec,
    fader, lamp, vslider, vslider_fraction_at, vslider_thumb_pos,
};
pub use stepper::{stepper, stepper_with};
pub use text::{TextKind, brand, display, label, title, value};
pub use theme::{
    POPUP_RADIUS, ShadowPair, TOOLTIP_DELAY, TRACK_COLOURS, Theme, ThemeMode, contrast_ratio,
};
pub use value_edit::{
    ValueEditOutput, begin_edit, cancel_edit, floating_edit, is_editing, parse_int, parse_number,
    parse_value, readout, show_edit, value_for_display, wheel_notches, wheel_steps,
};
pub use visualizer::{
    OFF_PRESET, PRESETS, Palette, VisualInput, VisualPreset, VisualStyle, Visualizer, bars,
    particles, radial, scope, vector, visualizer as show_visualizer, waves,
};

pub use step_cell::{
    ACCENT_MARKER, BEAT_NUMBER_HEIGHT, PLAYHEAD_DOT_CURRENT, PLAYHEAD_DOT_IDLE,
    PLAYHEAD_ROW_HEIGHT, SELECT_RING, STEP_CELL, STEP_GAP, STEP_GROUP, STEP_GROUP_GAP,
    StepCellSpec, StepRowOutput, beat_numbers, playhead_dot, playhead_row, playhead_tint,
    step_cell, step_cell_named, step_gap_after, step_index_at, step_row, step_row_width,
    step_x_offset, with_step_gaps,
};
