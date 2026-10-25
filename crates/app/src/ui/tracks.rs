//! TRACKS panel (M10, part 2): the instrument track list, modelled on FL Studio's Channel
//! Rack and Ableton's track list. Sits in the right-hand column under the `Tracks` tab of the
//! `Proj | Chain | Vis | Tracks` head ([`controls::panel_head`]).
//!
//! One row ([`ROW_HEIGHT`]) per active track in slot order: the colour chip (a click cycles
//! the eight swatches), the name over the preset name (truncated with an ellipsis; the full
//! text is the tooltip) with the mixer insert **number box** (M11: a [`BOX_SIZE`] pill
//! reading the insert number, `M` for straight to the master, at the start of the preset
//! line; a click opens the picker, tooltip "Mixer insert"), a pencil that opens the inline rename editor (a double-click on the
//! row does too; Enter / focus loss commit, Escape cancels, and shortcuts and the piano keys
//! stay quiet while it has the keyboard, like every text field), the [`SQUARE`] `M` / `S`
//! squares and the small LEVEL knob bound to the track's mixer strip, and the `Roll` pill
//! that selects the track and shows the piano roll. A click on the name block selects the
//! track; the selected row sits on an inset-m well with a 3 pt accent bar (on-glow) at its
//! left edge, the design cleanup's rule that a selection reads as an accent cue, not as
//! weight alone. The list scrolls when the rows do not fit.
//!
//! Below the list: `+ Add` (the primary pill: a new track with the default preset, selected),
//! `Dup` (clone the selected track's settings, not its notes) and `Del` (an inline
//! "Delete <name>?" confirm with YES / NO, like the PROJECT panel's discard confirm; disabled
//! with one track), then the `▲` / `▼` move buttons and the `Link` button (M11, FL's Ctrl+L:
//! routes the selected track to the first free insert and shows the mixer) for the selected
//! track beside the hint. Everything goes through `AppState` (`add_track`, `duplicate_track`,
//! `remove_track`, `rename_track`, `set_track_colour`, `select_track`, `move_track`,
//! `toggle_mute`, `toggle_solo`, `set_lane_view`, `set_route`, `link_track_to_free_insert`),
//! so undo and the engine messages are the state's business.
//!
//! Accessible names (tests): rows [`row_name`] (`"Track row 1: Synth"`, a selected button),
//! chips [`swatch_name`] (`"Synth swatch"`), pencils [`rename_name`] (`"Rename Synth"`), the
//! squares [`mute_name`] / [`solo_name`] (`"Mute track Synth"`, `"Solo track Synth"`), the
//! level knobs [`level_name`] (`"Level track Synth"`, a slider), the roll pills
//! [`roll_name`] (`"Roll: Synth"`), the number boxes [`insert_box_name`] (`"Insert box
//! Synth"`; the picker's entries are `ui::mixer::pick_name`), the action pills [`ADD_NAME`],
//! [`DUP_LABEL`], [`DEL_LABEL`], [`YES_LABEL`], [`NO_LABEL`], the move buttons
//! [`MOVE_UP_NAME`] / [`MOVE_DOWN_NAME`], the link button [`LINK_NAME`], the question
//! [`delete_question`] and the hint [`HINT`].

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align, Color32, CursorIcon, FontId, Id, InnerResponse, Layout, Pos2, Rect, Response, RichText,
    Role, ScrollArea, Sense, Stroke, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};
use engine::mixer::{MAX_LEVEL_DB, MIN_LEVEL_DB};
use engine::{Channel, ChannelParam, TrackId};
use neu::{Depth, KnobSize, KnobSpec, Theme, ValueEditOutput, paint_soft_rect};

use crate::app::{AppState, LaneView, RightPanel};
use crate::shortcuts::Shortcut;
use crate::ui::controls::{
    self, GROUP_GAP, PANEL_HEAD_GAP, TRACK_CHIP, TRACK_CHIP_GAP, fmt_level_db,
};
use crate::ui::mixer;

/// Height of one track row.
pub const ROW_HEIGHT: f32 = 34.0;
/// Corner radius of the selected row's inset well (8: the accent bar sits on the well's
/// straight edge, clear of the corners).
const ROW_RADIUS: f32 = 8.0;
/// Padding at the row's left edge (the accent bar sits inside it) and right edge, on the
/// 4 / 8 / 12 scale.
const ROW_PAD_LEFT: f32 = 8.0;
const ROW_PAD_RIGHT: f32 = 4.0;
/// The selected row's accent bar: its width, height and distance from the row's left edge.
const ACCENT_BAR: Vec2 = vec2(3.0, 16.0);
const ACCENT_BAR_INSET: f32 = 2.0;
// The bar sits inside the left padding, on the well's straight edge (clear of its corners).
const _: () = assert!(ACCENT_BAR_INSET + ACCENT_BAR.x <= ROW_PAD_LEFT);
const _: () = assert!(ACCENT_BAR.y <= ROW_HEIGHT - 2.0 * ROW_RADIUS);
/// Side of the `M` / `S` squares and the height of the `Roll` pill and the move buttons.
pub const SQUARE: f32 = 20.0;
/// Width of the `Roll` pill (the 12 pt "Roll" is about 21 pt wide).
const ROLL_WIDTH: f32 = 32.0;
/// Side of the pencil's hit box.
const PENCIL: f32 = 14.0;
/// Width (and height) of the small LEVEL knob's block (`KnobSize::Sm`, the sequencer row
/// knob; checked by a test).
const KNOB: f32 = 34.0;
/// Gap between the name block, the pencil, the `M` / `S` squares and the knob.
const CELL_GAP: f32 = 4.0;
/// Gap between the knob and the `Roll` pill.
const ROLL_GAP: f32 = 4.0;
/// Gap between pills in a row.
const PILL_GAP: f32 = 8.0;
/// Height of the `▲` / `▼` + hint row (the hint may wrap to two 11 pt lines).
const MOVE_ROW: f32 = 28.0;
/// Width of the hover ring around a chip.
const CHIP_RING: f32 = 1.5;
/// Size of the insert number box at the start of the preset line (M11).
pub const BOX_SIZE: Vec2 = vec2(22.0, 13.0);
/// Gap between the number box and the preset name.
const BOX_GAP: f32 = 4.0;
/// Width of the `Link` button in the move row.
const LINK_WIDTH: f32 = 36.0;
// The move row holds two 11 pt hint lines; the number box sits inside the preset line.
const _: () = assert!(MOVE_ROW >= 2.0 * PRESET_SIZE + 4.0);
const _: () = assert!(BOX_SIZE.y <= PRESET_SIZE + 3.0);
/// The name's font (the kit's value level: 13 / 400).
const NAME_SIZE: f32 = 13.0;
/// The preset line's font (11 pt, secondary colour).
const PRESET_SIZE: f32 = 11.0;
/// Rows that fit the list at the 1280 x 1140 design size in GRID view (the panel is as tall
/// as the SEQUENCER beside it); more tracks scroll.
pub const VISIBLE_ROWS_AT_DESIGN_SIZE: usize = 5;

/// Visible text of the ADD pill.
pub const ADD_LABEL: &str = "+ Add";
/// Accessible name of the ADD pill.
pub const ADD_NAME: &str = "Add track";
/// The DUP pill (label and accessible name).
pub const DUP_LABEL: &str = "Dup";
/// The DEL pill (label and accessible name).
pub const DEL_LABEL: &str = "Del";
/// The confirm's YES pill.
pub const YES_LABEL: &str = "Yes";
/// The confirm's NO pill.
pub const NO_LABEL: &str = "No";
/// Accessible name of the `▲` button.
pub const MOVE_UP_NAME: &str = "Move track up";
/// Accessible name of the `▼` button.
pub const MOVE_DOWN_NAME: &str = "Move track down";
/// The hint under the list (it may wrap to two lines beside the move and link buttons).
pub const HINT: &str = "Keys play the selected track";
/// The `Link` button's label.
pub const LINK_LABEL: &str = "Link";
/// Accessible name of the `Link` button.
pub const LINK_NAME: &str = "Link to free insert";

/// Accessible name of the row of the `index`-th (0-based) active track: `"Track row 1: Synth"`.
#[must_use]
pub fn row_name(index: usize, name: &str) -> String {
    format!("Track row {}: {name}", index + 1)
}

/// Accessible name of a track's colour chip here: `"Synth swatch"` (the SYNTH / SEQUENCER
/// heads' chips are `"Synth colour"`, see [`controls::track_chip_name`]).
#[must_use]
pub fn swatch_name(name: &str) -> String {
    format!("{name} swatch")
}

/// Accessible name of a track's pencil: `"Rename Synth"`.
#[must_use]
pub fn rename_name(name: &str) -> String {
    format!("Rename {name}")
}

/// Accessible name of a track's `M` square: `"Mute track Synth"` (the mixer's strips are
/// `"Mute Synth"`).
#[must_use]
pub fn mute_name(name: &str) -> String {
    format!("Mute track {name}")
}

/// Accessible name of a track's `S` square: `"Solo track Synth"`.
#[must_use]
pub fn solo_name(name: &str) -> String {
    format!("Solo track {name}")
}

/// Accessible name of a track's LEVEL knob: `"Level track Synth"` (a slider; the mixer's
/// fader is `"Synth level"`).
#[must_use]
pub fn level_name(name: &str) -> String {
    format!("Level track {name}")
}

/// Accessible name of a track's `Roll` pill: `"Roll: Synth"`.
#[must_use]
pub fn roll_name(name: &str) -> String {
    format!("Roll: {name}")
}

/// The inline confirm's question: `"Delete Synth?"`.
#[must_use]
pub fn delete_question(name: &str) -> String {
    format!("Delete {name}?")
}

/// Accessible name of a track's insert number box: `"Insert box Synth"` (the mixer's source
/// strip box is `"Insert of Synth"`, the sequencer row's `"Row insert Bass"`).
#[must_use]
pub fn insert_box_name(name: &str) -> String {
    format!("Insert box {name}")
}

/// Where the pending DEL confirm lives (`ui.data`, the slot waiting for YES / NO).
fn pending_delete_id() -> Id {
    Id::NULL.with("tracks_panel_pending_delete")
}

/// The inline rename editor's id for `slot` (`neu::value_edit`): global, so it is the same
/// from the panel's `Ui` and from inside the list's scroll area.
fn rename_id(slot: TrackId) -> Id {
    Id::NULL.with(("tracks_panel_rename", slot))
}

/// The slot whose DEL confirm is showing, if any.
#[must_use]
pub fn pending_delete(ui: &Ui) -> Option<TrackId> {
    ui.data(|d| d.get_temp::<TrackId>(pending_delete_id()))
}

fn set_pending_delete(ui: &mut Ui, slot: Option<TrackId>) {
    ui.data_mut(|d| {
        if let Some(slot) = slot {
            d.insert_temp(pending_delete_id(), slot);
        } else {
            d.remove_temp::<TrackId>(pending_delete_id());
        }
    });
}

/// Whether the inline rename editor of `slot` is open.
#[must_use]
pub fn is_renaming(ui: &Ui, slot: TrackId) -> bool {
    neu::is_editing(ui, rename_id(slot))
}

/// Renders the TRACKS panel into `ui`, filling the space it is given.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        // The gaps below are explicit (the row pitch must fit the sequencer row beside it).
        ui.spacing_mut().item_spacing.y = 0.0;
        controls::panel_head(ui, state, theme, "Tracks");
        ui.add_space(PANEL_HEAD_GAP);
        // A confirm for a track that went away (undo, a load) is dropped.
        if pending_delete(ui).is_some_and(|slot| !state.track_active(slot)) {
            set_pending_delete(ui, None);
        }
        let footer = GROUP_GAP + neu::button::PILL_HEIGHT + GROUP_GAP + MOVE_ROW;
        let list_height = (ui.available_height() - footer).max(ROW_HEIGHT);
        list(ui, state, theme, list_height);
        ui.add_space(GROUP_GAP);
        match pending_delete(ui) {
            Some(slot) => delete_confirm(ui, state, theme, slot),
            None => {
                action_row(ui, state, theme);
                ui.add_space(GROUP_GAP);
                move_row(ui, state, theme);
            }
        }
    });
}

/// The rows, one per active track in slot order, inside a scroll area of `height`.
fn list(ui: &mut Ui, state: &mut AppState, theme: &Theme, height: f32) {
    let slots: Vec<TrackId> = state.active_tracks().collect();
    ScrollArea::vertical()
        .id_salt("tracks_list")
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, slot) in slots.iter().copied().enumerate() {
                row(ui, state, theme, index, slot);
            }
        });
}

/// A `Ui` confined to `rect` (so a child widget can sit at an exact spot of a row).
fn place<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        add,
    )
}

/// One track row: chip, name over preset, pencil, `M`, `S`, LEVEL, `Roll`.
fn row(ui: &mut Ui, state: &mut AppState, theme: &Theme, index: usize, slot: TrackId) {
    let track = state.track(slot).clone();
    let selected = state.edit_track == slot;
    let channel = Channel::from_track(slot);
    // The row itself only paints (the well, the accent bar); its controls sit on top.
    let (rect, row) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let visible = ui.is_rect_visible(rect);
    if visible && selected {
        paint_soft_rect(ui.painter(), rect, ROW_RADIUS, Depth::InsetM, theme);
        let bar = Rect::from_min_size(
            pos2(
                rect.min.x + ACCENT_BAR_INSET,
                rect.center().y - ACCENT_BAR.y * 0.5,
            ),
            ACCENT_BAR,
        );
        paint_soft_rect(ui.painter(), bar, ACCENT_BAR.x * 0.5, Depth::OnGlow, theme);
    }
    let row_hovered = row.contains_pointer();

    // Geometry, left to right: chip | name block | pencil | M | S | LEVEL | Roll.
    let cy = rect.center().y;
    let chip_rect = Rect::from_center_size(
        pos2(rect.min.x + ROW_PAD_LEFT + TRACK_CHIP * 0.5, cy),
        vec2(TRACK_CHIP, TRACK_CHIP),
    );
    let roll_rect = Rect::from_center_size(
        pos2(rect.max.x - ROW_PAD_RIGHT - ROLL_WIDTH * 0.5, cy),
        vec2(ROLL_WIDTH, SQUARE),
    );
    let knob_rect = Rect::from_center_size(
        pos2(roll_rect.min.x - ROLL_GAP - KNOB * 0.5, cy),
        vec2(KNOB, KNOB),
    );
    let solo_rect = Rect::from_center_size(
        pos2(knob_rect.min.x - CELL_GAP - SQUARE * 0.5, cy),
        vec2(SQUARE, SQUARE),
    );
    let mute_rect = Rect::from_center_size(
        pos2(solo_rect.min.x - CELL_GAP - SQUARE * 0.5, cy),
        vec2(SQUARE, SQUARE),
    );
    let pencil_rect = Rect::from_center_size(
        pos2(mute_rect.min.x - CELL_GAP - PENCIL * 0.5, cy),
        vec2(PENCIL, PENCIL),
    );
    let name_left = chip_rect.max.x + TRACK_CHIP_GAP;
    let name_width = (pencil_rect.min.x - CELL_GAP - name_left).max(0.0);

    // The name block is the row's button: a click selects, a double-click renames. (It is
    // the block, not the whole row, so its centre is never one of the controls.)
    let block_rect = Rect::from_min_max(
        pos2(name_left, rect.min.y),
        pos2(pencil_rect.min.x - CELL_GAP, rect.max.y),
    );
    let block = ui.interact(block_rect, rect_id(ui, slot, "row"), Sense::click());
    let name = row_name(index, &track.name);
    block.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, &name));
    let block = block
        .on_hover_text(format!(
            "{} · {}\nClick selects, double-click renames",
            track.name,
            track.preset_name()
        ))
        .on_hover_cursor(CursorIcon::PointingHand);
    if block.double_clicked() {
        neu::begin_edit(ui, rename_id(slot), &track.name);
    } else if block.clicked() {
        state.select_track(slot);
    }

    // Chip: a click cycles the colour.
    let chip = ui.interact(chip_rect, rect_id(ui, slot, "chip"), Sense::click());
    if visible {
        let colour = theme.track_colour(track.colour);
        ui.painter()
            .circle_filled(chip_rect.center(), TRACK_CHIP * 0.5, colour);
        if chip.hovered() {
            ui.painter().circle_stroke(
                chip_rect.center(),
                TRACK_CHIP * 0.5 + CHIP_RING + 1.0,
                Stroke::new(CHIP_RING, colour),
            );
        }
    }
    let swatch = swatch_name(&track.name);
    chip.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &swatch));
    if chip
        .on_hover_text("Colour: a click cycles the eight swatches")
        .on_hover_cursor(CursorIcon::PointingHand)
        .clicked()
    {
        state.set_track_colour(slot, track.colour.wrapping_add(1));
    }

    // Name over preset, or the rename editor over the name.
    let name_font = FontId::proportional(NAME_SIZE);
    let preset_font = FontId::proportional(PRESET_SIZE);
    let name_height = ui.fonts_mut(|f| f.row_height(&name_font));
    let preset_height = ui.fonts_mut(|f| f.row_height(&preset_font));
    let block_top = cy - (name_height + preset_height) * 0.5;
    let name_rect = Rect::from_min_size(pos2(name_left, block_top), vec2(name_width, name_height));
    let editor = neu::show_edit(
        ui,
        theme,
        rename_id(slot),
        name_rect,
        name_font.clone(),
        Align::Min,
    );
    match editor {
        ValueEditOutput::Committed(typed) => {
            state.rename_track(slot, &typed);
        }
        ValueEditOutput::Editing => {}
        ValueEditOutput::Idle | ValueEditOutput::Cancelled => {
            if visible {
                let name_colour = if selected || row_hovered {
                    theme.text_primary
                } else {
                    theme.text_secondary
                };
                paint_truncated(
                    ui,
                    name_rect.min,
                    &track.name,
                    name_font,
                    name_colour,
                    name_width,
                );
                paint_truncated(
                    ui,
                    pos2(name_left + BOX_SIZE.x + BOX_GAP, block_top + name_height),
                    track.preset_name(),
                    preset_font,
                    theme.text_secondary,
                    (name_width - BOX_SIZE.x - BOX_GAP).max(0.0),
                );
            }
        }
    }

    // The mixer insert number box at the start of the preset line (M11). Added after the
    // name block so it is on top of it: a click here never selects or renames.
    let box_rect = Rect::from_min_size(
        pos2(
            name_left,
            block_top + name_height + (preset_height - BOX_SIZE.y) * 0.5,
        ),
        BOX_SIZE,
    );
    let route = state.mixer.route(channel);
    let box_response = place(ui, box_rect, |ui| {
        neu::toggle_button_sized(ui, theme, &mixer::box_text(route), false, BOX_SIZE)
    })
    .inner;
    let box_label = insert_box_name(&track.name);
    box_response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &box_label));
    let box_response = box_response
        .on_hover_text(format!(
            "Mixer insert: {} · click to pick ({} links a free one)",
            state.route_name(route),
            Shortcut::LinkInsert.label()
        ))
        .on_hover_cursor(CursorIcon::PointingHand);
    if let Some(picked) = mixer::route_picker(
        theme,
        &box_response,
        state,
        route,
        &mixer::source_route_choices(),
    ) {
        state.set_route(channel, picked);
        state.select_insert(picked, false);
    }

    // Pencil: opens the rename editor.
    let pencil = ui.interact(pencil_rect, rect_id(ui, slot, "pencil"), Sense::click());
    if visible {
        let colour = if pencil.hovered() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        paint_pencil(
            ui,
            pencil_rect,
            Stroke::new(neu::button::ICON_STROKE, colour),
        );
    }
    let rename = rename_name(&track.name);
    pencil.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &rename));
    if pencil
        .on_hover_text("Rename (or double-click the row) · Enter commits, Esc cancels")
        .on_hover_cursor(CursorIcon::PointingHand)
        .clicked()
    {
        neu::begin_edit(ui, rename_id(slot), &track.name);
    }

    // M / S: the track's mixer strip.
    let mute = state.mixer.strip(channel).mute;
    let response = place(ui, mute_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "M", mute, vec2(SQUARE, SQUARE))
    })
    .inner;
    if response.clicked() {
        state.toggle_mute(channel);
    }
    let mute = state.mixer.strip(channel).mute;
    let mute_label = mute_name(&track.name);
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), mute, &mute_label));
    response.on_hover_text(format!("Mute {}", track.name));

    let solo = state.mixer.strip(channel).solo;
    let response = place(ui, solo_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "S", solo, vec2(SQUARE, SQUARE))
    })
    .inner;
    if response.clicked() {
        state.toggle_solo(channel);
    }
    let solo = state.mixer.strip(channel).solo;
    let solo_label = solo_name(&track.name);
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), solo, &solo_label));
    response.on_hover_text(format!("Solo {}", track.name));

    // LEVEL: the strip's fader as a small unlabelled knob (its tooltip shows the value, the
    // mixer's fader shows the same number).
    let spec = KnobSpec {
        label: "",
        min: MIN_LEVEL_DB,
        max: MAX_LEVEL_DB,
        default: ChannelParam::Level.default_value(channel),
        log: false,
        bipolar: false,
        size: KnobSize::Sm,
        fmt: fmt_level_db,
    };
    let before = state.mixer.get(channel, ChannelParam::Level);
    let mut value = before;
    let note = format!("Level of {} (double-click resets)", track.name);
    let response = place(ui, knob_rect, |ui| {
        neu::knob_noted(ui, theme, &mut value, &spec, Some(&note))
    })
    .inner;
    if response.changed() || value != before {
        state.edit_mixer(channel, ChannelParam::Level, value, response.drag_started());
    }
    let shown = state.mixer.get(channel, ChannelParam::Level);
    let level_label = level_name(&track.name);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(shown), &level_label));

    // Roll: select the track and show the piano roll.
    let on_roll = selected && state.lane_view == LaneView::Roll;
    let response = place(ui, roll_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "Roll", on_roll, vec2(ROLL_WIDTH, SQUARE))
    })
    .inner;
    let roll_label = roll_name(&track.name);
    response
        .widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on_roll, &roll_label));
    if response
        .on_hover_text(format!(
            "Select {} and open it in the piano roll",
            track.name
        ))
        .clicked()
    {
        state.select_track(slot);
        state.set_lane_view(LaneView::Roll);
    }
}

/// A stable id for a row's sub-control (the row's slot, not its index: a reorder keeps
/// hover and press state with the track).
fn rect_id(ui: &Ui, slot: TrackId, part: &str) -> Id {
    ui.scope_id().with(("tracks_row", slot, part))
}

/// Paints `text` at `pos` (top-left) in `font`, cut to `max_width` with an ellipsis.
fn paint_truncated(ui: &Ui, pos: Pos2, text: &str, font: FontId, colour: Color32, max_width: f32) {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font,
            color: colour,
            ..TextFormat::default()
        },
    );
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos, galley, colour);
}

/// A pencil glyph fitted to `rect`: a slanted body with a short tip line.
fn paint_pencil(ui: &Ui, rect: Rect, stroke: Stroke) {
    let r = rect.shrink(2.0);
    let painter = ui.painter();
    // Body: bottom-left to top-right.
    painter.line_segment(
        [pos2(r.min.x, r.max.y), pos2(r.max.x - 2.0, r.min.y + 2.0)],
        stroke,
    );
    // A second edge beside it, so the body reads as a bar.
    painter.line_segment(
        [pos2(r.min.x + 2.5, r.max.y), pos2(r.max.x, r.min.y + 4.5)],
        stroke,
    );
    // The tip cap.
    painter.line_segment(
        [
            pos2(r.max.x - 3.5, r.min.y + 0.5),
            pos2(r.max.x + 1.0, r.min.y + 5.0),
        ],
        stroke,
    );
}

/// `+ Add` | `Dup` | `Del`.
fn action_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        let add = neu::toggle_button(ui, theme, ADD_LABEL, true);
        add.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), ADD_NAME));
        if add
            .on_hover_text(format!(
                "New track with the default preset ({})",
                Shortcut::AddTrack.label()
            ))
            .clicked()
        {
            state.add_track();
        }
        let room = state.free_track_slot().is_some();
        let dup = ui
            .add_enabled_ui(room, |ui| {
                neu::button(ui, theme, DUP_LABEL)
                    .on_hover_text(format!(
                        "Duplicate the selected track's settings, not its notes ({})",
                        Shortcut::DuplicateTrack.label()
                    ))
                    .clicked()
            })
            .inner;
        if dup {
            state.duplicate_track(state.edit_track);
        }
        let can_delete = state.active_track_count() > 1;
        let del = ui
            .add_enabled_ui(can_delete, |ui| {
                neu::button(ui, theme, DEL_LABEL)
                    .on_hover_text(if can_delete {
                        "Delete the selected track (asks first)"
                    } else {
                        "The last track stays"
                    })
                    .clicked()
            })
            .inner;
        if del {
            set_pending_delete(ui, Some(state.edit_track));
        }
    });
}

/// "Delete <name>?" with YES | NO, shown in place of the action rows.
fn delete_confirm(ui: &mut Ui, state: &mut AppState, theme: &Theme, slot: TrackId) {
    let question = delete_question(&state.track(slot).name);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), neu::button::PILL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add(
                egui::Label::new(RichText::new(question).size(12.0).color(theme.text_primary))
                    .truncate(),
            );
        },
    );
    ui.add_space(GROUP_GAP);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        if neu::button(ui, theme, YES_LABEL)
            .on_hover_text("Delete the track and its notes (Ctrl+Z brings it back)")
            .clicked()
        {
            state.remove_track(slot);
            set_pending_delete(ui, None);
        }
        if neu::button(ui, theme, NO_LABEL)
            .on_hover_text("Keep the track")
            .clicked()
        {
            set_pending_delete(ui, None);
        }
    });
}

/// `▲` | `▼` for the selected track, then the hint.
fn move_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), MOVE_ROW),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = PILL_GAP;
            let slots: Vec<TrackId> = state.active_tracks().collect();
            let position = slots.iter().position(|s| *s == state.edit_track);
            let can_up = position.is_some_and(|p| p > 0);
            let can_down = position.is_some_and(|p| p + 1 < slots.len());
            if glyph_button(ui, theme, true, can_up, MOVE_UP_NAME)
                .on_hover_text("Move the selected track up")
                .clicked()
            {
                state.move_track(state.edit_track, true);
            }
            if glyph_button(ui, theme, false, can_down, MOVE_DOWN_NAME)
                .on_hover_text("Move the selected track down")
                .clicked()
            {
                state.move_track(state.edit_track, false);
            }
            // LINK (M11, FL's Ctrl+L): the selected track to the first free insert.
            let link =
                neu::toggle_button_sized(ui, theme, LINK_LABEL, false, vec2(LINK_WIDTH, SQUARE));
            link.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), LINK_NAME));
            if link
                .on_hover_text(format!(
                    "Route the selected track to the first free mixer insert ({})",
                    Shortcut::LinkInsert.label()
                ))
                .clicked()
                && state.link_track_to_free_insert(state.edit_track).is_some()
            {
                state.right_panel = RightPanel::Mixer;
            }
            ui.add(
                egui::Label::new(
                    RichText::new(HINT)
                        .size(PRESET_SIZE)
                        .color(theme.text_secondary),
                )
                .wrap(),
            );
        },
    );
}

/// A [`SQUARE`] button with an up or down triangle: raised-s, inset-s while pressed, the
/// glyph in text-secondary (primary on hover, dimmed while disabled; the kit has no arrow
/// icon, so the glyph is painted here).
fn glyph_button(ui: &mut Ui, theme: &Theme, up: bool, enabled: bool, name: &str) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(SQUARE, SQUARE), sense);
    if ui.is_rect_visible(rect) {
        let pressed = enabled && response.is_pointer_button_down_on();
        let depth = if pressed {
            Depth::InsetS
        } else {
            Depth::RaisedS
        };
        paint_soft_rect(ui.painter(), rect, neu::TOGGLE_SIZED_RADIUS, depth, theme);
        let colour = if !enabled {
            theme.text_secondary.gamma_multiply(neu::DIMMED_OPACITY)
        } else if response.hovered() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let c = rect.center();
        let half = 4.0;
        let (apex, base) = if up {
            (c.y - half * 0.75, c.y + half * 0.75)
        } else {
            (c.y + half * 0.75, c.y - half * 0.75)
        };
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                pos2(c.x, apex),
                pos2(c.x + half, base),
                pos2(c.x - half, base),
            ],
            colour,
            Stroke::NONE,
        ));
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name));
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessible_names() {
        assert_eq!(row_name(0, "Synth"), "Track row 1: Synth");
        assert_eq!(swatch_name("Lead"), "Lead swatch");
        assert_eq!(rename_name("Lead"), "Rename Lead");
        assert_eq!(mute_name("Lead"), "Mute track Lead");
        assert_eq!(solo_name("Lead"), "Solo track Lead");
        assert_eq!(level_name("Lead"), "Level track Lead");
        assert_eq!(roll_name("Lead"), "Roll: Lead");
        assert_eq!(delete_question("Lead"), "Delete Lead?");
        assert_eq!(insert_box_name("Lead"), "Insert box Lead");
    }

    #[test]
    fn the_move_row_fits_its_buttons_and_a_two_line_hint() {
        // ▲ | ▼ | Link, then the hint, which may take two 11 pt lines of the rest.
        let buttons = SQUARE + PILL_GAP + SQUARE + PILL_GAP + LINK_WIDTH + PILL_GAP;
        let inner = crate::ui::RIGHT_COLUMN_WIDTH - 2.0 * 20.0;
        assert!(inner - buttons >= 120.0, "hint room {}", inner - buttons);
        // The move row holds two 11 pt lines and the box fits the preset line: `const`
        // assertions at the top.
    }

    #[test]
    fn a_row_fits_the_column() {
        // bar + chip | name (52: seven 13 pt characters, "Track 8" included; longer names get
        // the ellipsis) | pencil | M | S | LEVEL | Roll inside the 220 pt inner width, with
        // the paddings on the 4 / 8 / 12 scale.
        assert_eq!(KNOB, KnobSize::Sm.block_width());
        let fixed = ROW_PAD_LEFT
            + TRACK_CHIP
            + TRACK_CHIP_GAP
            + CELL_GAP
            + PENCIL
            + CELL_GAP
            + SQUARE
            + CELL_GAP
            + SQUARE
            + CELL_GAP
            + KNOB
            + ROLL_GAP
            + ROLL_WIDTH
            + ROW_PAD_RIGHT;
        let inner = crate::ui::RIGHT_COLUMN_WIDTH - 2.0 * 20.0;
        assert!(inner - fixed >= 48.0, "name column {}", inner - fixed);
        assert!(fixed <= inner, "the row stays inside the column");
    }
}
