//! `neu`: the Deck egui widget kit and theme tokens for BeatSlopper.
//!
//! Tokens come from `design/round3/WINNER.md` (round 3, direction B "Deck"): flat panel tones,
//! 1 px borders, 4 px radii, one amber accent, eight track colours, 28 pt controls on an 8 pt
//! rhythm, always-visible scrollbars and FL-Studio-like windows with headers. The only shadow
//! is the floating one under menus, tooltips and the floating drawer.
//!
//! * [`theme`]: the token transcription ([`Theme`], [`ThemeMode`]) and [`Theme::apply`].
//! * [`flat`]: the painting primitives ([`paint_panel`], [`paint_focus_ring`],
//!   [`paint_shadow_floating`], the gradient meshes). [`shadow`] keeps the Soft Console names
//!   ([`Depth`], [`paint_soft_rect`], ...) drawing flat for the app; retired in M13.
//! * [`text`]: the type scale ([`TextKind`]) and the text helpers.
//! * [`panel`], [`button`]: frames and every button kind ([`ButtonKind`], [`ButtonSize`],
//!   the [`switch`], the [`rail_button`]).
//! * [`chrome`]: keycaps, LEDs, tags, chips, the insert [`number_box`], the channel
//!   [`name_button`], [`pattern_tile`]s and the [`text_field`].
//! * [`shell`]: [`window_frame`], [`toolbar`], [`tab_strip`], [`hint_bar`], [`drawer_handle`],
//!   [`list_row`], [`menu`] / [`dropdown`], [`tooltip_with_keycap`], the always-visible
//!   scroll areas and [`group`].
//! * [`knob`], [`segmented`], [`slider`], [`stepper`]: the value controls (knobs 32 / 22 / 40
//!   and the [`dial`] as the 40 knob, the box [`segmented`], the vertical / horizontal
//!   sliders, the [`fader`], the [`lamp`] LED, the [`stepper`]).
//! * [`meter`]: the banded peak meters (column, stereo, horizontal, gain reduction).
//! * [`step_cell`]: the 24 pt sequencer cells with the beat alternation, the playhead row and
//!   the beat ruler.
//! * [`piano_keys`]: the keyboard strip keys; [`piano_roll`]: the roll with ruler, key column,
//!   note bars by velocity, ghosts and the velocity lane.
//! * [`visualizer`]: the master-output drawings in a panel-2 well.
//! * [`value_edit`]: typed value entry (click a readout, type `128 bpm` / `-6 dB` / `2.4k`,
//!   Enter) and the wheel nudge shared by every numeric control.

pub mod button;
pub mod chrome;
pub mod flat;
pub mod knob;
pub mod meter;
pub mod panel;
pub mod piano_keys;
pub mod piano_roll;
pub mod segmented;
pub mod shadow;
pub mod shell;
pub mod slider;
pub mod step_cell;
pub mod stepper;
pub mod text;
pub mod theme;
pub mod value_edit;
pub mod visualizer;

pub use button::{
    BUTTON_HEIGHT, ButtonKind, ButtonSize, FaceState, ICON_BOX, ICON_STROKE, Icon, PILL_HEIGHT,
    RAIL_BUTTON, SMALL_DEPTH_MAX, TOGGLE_KNOB, TOGGLE_SIZED_RADIUS, TOGGLE_SQUARE, TOGGLE_TRACK,
    button, button_kind, button_wide, control_painter, ghost_button, header_button, icon_button,
    icon_button_kind, paint_button_face, paint_icon, primary_button, rail_button, rec_button,
    small_button, solo_button, stepper_button, switch, switch_small, theme_toggle, toggle_button,
    toggle_button_sized, toolbar_separator,
};
pub use chrome::{
    CHIP_SIZE, FIELD_HEIGHT, KEYCAP_HEIGHT, LED_SIZE, LedKind, NAME_BUTTON, NAME_BUTTON_RACK,
    NUMBER_BOX, PATTERN_TILE, TAG_HEIGHT, TagKind, chip, ellipsise, keycap, keycap_size, led,
    led_named, name_button, name_button_sized, number_box, number_box_named, paint_colour_bar,
    paint_keycap, paint_led, pattern_tile, tag, text_field,
};
pub use flat::{
    Tone, flat_rect_shape, is_shadow_shape, paint_control, paint_focus_ring,
    paint_gradient_control, paint_panel, paint_separator, paint_shadow_floating,
    vertical_gradient_circle, vertical_gradient_rect,
};
pub use knob::{
    DIMMED_OPACITY, DialSize, KNOB_BLOCK_MIN_WIDTH, KnobSize, KnobSpec, dial, fmt_plain, knob,
    knob_dimmed, knob_noted, knob_with, paint_arc, paint_knob, tooltip_text,
};
pub use meter::{
    GR_METER_WIDTH, METER_FLOOR_DB, METER_HORIZONTAL, METER_HOT_LEVEL, METER_HOT_SEPARATOR,
    METER_HOT_TOP, METER_TRACK, STEREO_METER_SIZE, STEREO_METER_TRACK_WIDTH, band_colour, gr_meter,
    meter, meter_fraction, meter_horizontal, stereo_meter,
};
pub use panel::{panel, panel_toned};
pub use piano_keys::{
    PianoKey, PianoModel, PianoOutput, key_at, key_layout, keyboard_size, keyboard_width,
    note_name, piano_keys,
};
pub use piano_roll::{
    ACCENT_DOT, ACCENT_DOT_INSET, ACCENT_THRESHOLD, ACCENT_VELOCITY, DEFAULT_VELOCITY,
    GHOST_OPACITY, GHOST_OUTLINE_OPACITY, GHOST_RADIUS, GhostNote, KEY_COLUMN_WIDTH, NOTE_RADIUS,
    PianoRollModel, PianoRollOutput, RESIZE_HANDLE, RULER_HEIGHT, RollNote, VELOCITY_LANE_HEIGHT,
    black_row_tint, cell_at, ghost_fill, ghost_outline, ghost_rects, grid_rect, is_black_key,
    key_column_rect, note_fill, note_rect, piano_roll, piano_roll_coloured, piano_roll_with_ghosts,
    row_rect, ruler_rect, velocity_at, velocity_bar_rect, velocity_rect,
};
pub use segmented::{
    SEGMENTED_CHIP_HEIGHT, SEGMENTED_HEIGHT, SEGMENTED_ITEM_PADDING, SEGMENTED_SMALL_HEIGHT,
    chip_colour, chip_rect, segmented, segmented_padded, segmented_small,
};
pub use shadow::{
    Depth, FOCUS_RING, FOCUS_RING_GAP, paint_focus_circle, paint_focus_rect, paint_soft_circle,
    paint_soft_circle_with_fill, paint_soft_rect, paint_soft_rect_with_fill, soft_circle_shapes,
    soft_rect_shapes, soft_rect_shapes_with_fill,
};
pub use shell::{
    APP_TOOLBAR, DRAWER_HANDLE, DRAWER_HANDLE_HIT, LIST_ROW, MENU_ITEM, MENU_MIN_WIDTH, MenuItem,
    TAB_STRIP, TOOLTIP_HEIGHT, WINDOW_HEADER, WINDOW_TOOLBAR, app_toolbar, drawer_handle, dropdown,
    group, hint_bar, list_row, list_row_named, menu, menu_popup, menu_size, scroll_both,
    scroll_horizontal, scroll_vertical, tab_strip, tab_strip_padded, toolbar, tooltip_with_keycap,
    well, window_frame, window_frame_sized,
};
pub use slider::{
    FADER_MIN_TRACK, FADER_TRACK, FADER_TRAVEL, FADER_WIDTH, HSLIDER_SIZE, SLIDER_THUMB,
    SLIDER_THUMB_WIDTH, SLIDER_TRACK, SLIDER_TRAVEL, SLIDER_WIDTH, SliderSpec, fader, fader_sized,
    hslider, hslider_fraction_at, hslider_sized, lamp, lamp_kind, vslider, vslider_fraction_at,
    vslider_thumb_pos,
};
pub use stepper::{stepper, stepper_with};
pub use text::{
    TextKind, body, brand, display, hint, label, micro, section, small, text_widget, title, value,
};
pub use theme::{
    DISABLED_ALPHA, POPUP_RADIUS, SCROLLBAR_THUMB, SCROLLBAR_WIDTH, ShadowPair, TOOLTIP_DELAY,
    TRACK_COLOURS, Theme, ThemeMode, contrast_ratio, towards_black, with_alpha,
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
    PLAYHEAD_ROW_HEIGHT, SELECT_RING, STEP_CELL, STEP_GAP, STEP_GROUP, STEP_GROUP_GAP, STEP_PITCH,
    StepCellSpec, StepRowOutput, beat_numbers, is_alt_beat, paint_step_cell, playhead_dot,
    playhead_row, playhead_tint, step_cell, step_cell_at, step_cell_named, step_gap_after,
    step_index_at, step_row, step_row_coloured, step_row_width, step_x_offset, with_step_gaps,
};
