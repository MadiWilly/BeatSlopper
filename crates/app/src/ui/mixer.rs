//! MIXER panel (M6, FL layout since M11): the source channel strips, the sixteen insert
//! buses and a master column in the FX rack's slot.
//!
//! The panel shares the FX rack's head (`controls::plugins_head`: the title and the plugin
//! pills; M12 rehosts the whole panel in the MIXER view until the M13 drawer) and its [`WIDTH`]; the body is laid out by hand from a fixed origin so
//! every strip has the same geometry and the whole thing fits the top row (about 303 pt of
//! body at the 1280 x 1140 design size) without vertical scrolling. Three docks, left to
//! right, like FL Studio's mixer:
//!
//! - **Sources** ([`SOURCE_DOCK_WIDTH`]): one compact [`SOURCE_STRIP_WIDTH`] (36) pt strip
//!   per source channel (the five drums, then every *active* instrument track:
//!   [`visible_channels`]) in a horizontal `ScrollArea` that shows four at a time: the
//!   track's colour chip, the caps label, the insert **number box** (`6`, or `M` for straight
//!   to the master; a click selects that insert in the middle dock and opens the picker, the
//!   wheel steps it), the LEVEL and PAN `Sm` knobs (FL's channel-rack knobs; neutral by
//!   default since M11) and the 28 pt `M` / `S` squares. The source sends are gone from the
//!   panel: a mix's sends live on the inserts now.
//! - **Inserts** ([`INSERTS_DOCK_WIDTH`]): the sixteen [`INSERT_STRIP_WIDTH`] (108) pt insert
//!   strips in a horizontal `ScrollArea` (two fit at a time; the dock scrolls to the rest):
//!   the head (colour chip, `N · name` caps label, pencil; an insert still on its default
//!   name shows the track routed to it, `Kick` or `Kick +2`, via
//!   `AppState::insert_display_name`; a click selects the insert,
//!   Shift-click extends the selection, a double-click or the pencil renames inline), three
//!   **slot pills** (`- - -` when empty, else the kind's name; a click opens the kind menu
//!   Off / Comp / Drive / Lo-fi / Chorus / EQ3 / EQ; a loaded slot has a 16 pt knob-glyph
//!   square that swaps the strip's knob block for that slot's parameters, or for the EQ, the
//!   seven-band parametric EQ plugin, opens the EQ window on that slot), the knob block (3 x 2
//!   `Sm` knobs with 9 pt caps labels: LOW / MID / HIGH and FREQ / DLY / REV, or the open
//!   slot's parameters from `SlotKind::params()`), the **ROUTE** pill (`MASTER` or the
//!   number of a higher insert; the picker only offers what the engine accepts), and the
//!   fader row: the `M` / `S` squares, the level fader (its track fitted to the room left),
//!   the post-fader peak meter and, when a compressor is loaded, a thin gain-reduction bar.
//!   The insert the edit track routes to (or the primary selection) carries an accent bar in
//!   its head; selected inserts sit on an inset well and their faders move together; an
//!   insert nothing is routed to with every slot off is drawn dimmed.
//! - **Master** ([`MASTER_WIDTH`]): pinned at the panel's right edge outside both scroll
//!   areas: the MASTER gain knob (`Sm`, `MasterGain` in dB), the LIMIT toggle (`LimiterOn`)
//!   and the [`neu::stereo_meter`] down to the faders' bottom.
//!
//! Accessible names (tests): sources `"Kick level"`, `"Kick pan"`, `"Mute Kick"`, `"Solo
//! Kick"`, the number box [`source_box_name`] (`"Insert of Kick"`); inserts the head
//! [`insert_head_name`] (`"Insert 6: Insert 6"`), the swatch [`insert_swatch_name`], the
//! pencil [`insert_rename_name`], the slot pills [`slot_name`] (`"Insert 6 slot 1"`) and
//! their edit squares [`slot_edit_name`], the kind menu entries [`kind_pick_name`] (`"Slot
//! kind Comp"`), the knobs [`insert_control_name`] (`"Insert 6 low"`, `"Insert 6 reverb
//! send"`, `"Insert 6 level"`) and [`slot_control_name`] (`"Insert 6 slot 1 thresh"`), the
//! route pill [`route_name`] (`"Route of insert 6"`), the picker entries [`pick_name`]
//! (`"Pick MASTER"`, `"Pick insert 7"`), `"Mute insert 6"` / `"Solo insert 6"`, the meters
//! `"Level"` and [`GR_METER_NAME`]. Every value goes through `AppState` (`edit_mixer`,
//! `edit_insert`, `set_slot_kind`, `edit_slot_param`, `set_route`, `rename_insert`, ...), so
//! undo and the engine messages are the state's business; the meters read
//! `AppState::shown_meters`.

use egui::{
    Align, Align2, CursorIcon, FontId, Id, InnerResponse, Layout, Pos2, Rect, Response, Role,
    ScrollArea, Sense, Stroke, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use engine::inserts::{MASTER, MAX_INSERT, SLOT_COUNT, SlotParam, insert_index};
use engine::mixer::{MAX_LEVEL_DB, MIN_LEVEL_DB};
use engine::{Channel, ChannelParam, DrumRow, InsertParam, ParamId, SlotKind};
use neu::slider::SliderSpec;
use neu::{
    Depth, KnobSize, KnobSpec, METER_TRACK, SLIDER_WIDTH, STEREO_METER_SIZE, TOGGLE_SQUARE,
    TextKind, Theme, ValueEditOutput, paint_soft_rect,
};

use crate::app::AppState;
use crate::ui::controls::{
    self, Knob, TRACK_CHIP, fmt_db, fmt_hz, fmt_level_db, fmt_ms, fmt_pan, fmt_percent,
};

/// Width of the panel including its padding: the FX rack's slot.
pub const WIDTH: f32 = super::fx_rack::WIDTH;
/// The panel's content width (inside the 20 pt padding).
pub const CONTENT_WIDTH: f32 = WIDTH - 2.0 * 20.0;
/// Width of one source strip: the `Sm` knob plus a point each side.
pub const SOURCE_STRIP_WIDTH: f32 = 36.0;
/// Source strips visible at a time in the left dock (the rest scroll).
pub const SOURCE_DOCK_STRIPS: usize = 4;
/// Width of the source dock.
pub const SOURCE_DOCK_WIDTH: f32 = SOURCE_STRIP_WIDTH * SOURCE_DOCK_STRIPS as f32;
/// Gap between the source dock and the inserts dock.
pub const DOCK_GAP: f32 = 4.0;
/// Width of one insert strip: the 3 x 2 knob grid ([`GRID_WIDTH`]) plus a point each side.
pub const INSERT_STRIP_WIDTH: f32 = GRID_WIDTH + 2.0;
/// Width of the master column.
pub const MASTER_WIDTH: f32 = 60.0;
// A `Sm` knob, the M / S squares and the number box fit a source strip; two insert strips are
// visible at a time in the inserts dock.
const _: () = assert!(SOURCE_STRIP_WIDTH >= KNOB + 2.0);
const _: () = assert!(SOURCE_STRIP_WIDTH >= TOGGLE_SQUARE.x);
const _: () = assert!(SOURCE_STRIP_WIDTH >= BOX_SIZE.x);
const _: () = assert!(INSERTS_DOCK_WIDTH >= 2.0 * INSERT_STRIP_WIDTH);
/// Width of the inserts dock: what the source dock and the master leave.
pub const INSERTS_DOCK_WIDTH: f32 = CONTENT_WIDTH - SOURCE_DOCK_WIDTH - DOCK_GAP - MASTER_WIDTH;
/// Size of a strip knob (`KnobSize::Sm` ring).
pub const KNOB: f32 = 34.0;
/// Gap between neighbouring knobs and between the stacked rows of a strip.
pub const KNOB_GAP: f32 = 2.0;
/// Height of the 9 pt caps label under a grid knob.
pub const KNOB_LABEL: f32 = 10.0;
/// Font size of the grid knob labels.
pub const KNOB_LABEL_SIZE: f32 = 9.0;
/// Columns and rows of an insert's knob grid (six knobs: EQ3 + sends, or a slot's params).
pub const GRID_COLS: usize = 3;
pub const GRID_ROWS: usize = 2;
/// Width of the knob grid: three `Sm` knobs and two gaps.
pub const GRID_WIDTH: f32 = GRID_COLS as f32 * KNOB + (GRID_COLS as f32 - 1.0) * KNOB_GAP;
/// Height of one grid row: a knob and its label.
pub const GRID_ROW: f32 = KNOB + KNOB_LABEL;
/// Height of the knob grid.
pub const GRID_HEIGHT: f32 = GRID_ROWS as f32 * GRID_ROW + (GRID_ROWS as f32 - 1.0) * KNOB_GAP;
/// Height of an insert strip's head row, of a slot pill and of the route pill.
pub const ROW_H: f32 = 16.0;
/// Width of a slot's kind pill (the edit square takes the rest of the grid width).
pub const SLOT_PILL_WIDTH: f32 = GRID_WIDTH - SLOT_EDIT - KNOB_GAP;
/// Side of the square that opens a loaded slot's knobs.
pub const SLOT_EDIT: f32 = ROW_H;
/// Gap between the stacked blocks of a strip (head, pills, grid, route, fader).
pub const BLOCK_GAP: f32 = 2.0;
/// Gap between the M and S toggles (and around the master's LIMIT toggle).
pub const TOGGLE_GAP: f32 = 4.0;
/// Gap between the head row and the strips (tighter than the FX rack's 14 to fit the row).
pub const HEAD_GAP: f32 = 6.0;
/// How far below the faders' value row the master block (knob, LIMIT, meter) starts.
pub const MASTER_DROP: f32 = 16.0;
/// Distance from a fader track's centre line to the meter's left edge (half the 14 pt
/// thumb plus 2 pt).
pub const METER_OFFSET: f32 = 9.0;
/// Width of the gain-reduction bar beside an insert meter, and its gap from the meter.
pub const GR_WIDTH: f32 = 3.0;
pub const GR_GAP: f32 = 2.0;
/// Reduction (dB) that fills the gain-reduction bar.
pub const GR_FULL_DB: f32 = 24.0;
/// The accent bar of the highlighted insert (in its head row).
pub const ACCENT_BAR_WIDTH: f32 = 3.0;
/// Height of a source strip's number box (a [`SLOT_PILL_WIDTH`]-independent 28 x 16 pill).
pub const BOX_SIZE: egui::Vec2 = vec2(28.0, 16.0);
/// Text of an empty slot pill.
pub const EMPTY_SLOT_LABEL: &str = "- - -";
/// Accessible name of an insert's gain-reduction bar.
pub const GR_METER_NAME: &str = "Gain reduction";
/// Caps labels under the drum faders, in [`DrumRow::ALL`] order (a track strip's label is
/// the track's name).
pub const DRUM_STRIP_LABELS: [&str; DrumRow::COUNT] = ["Kick", "Snare", "Clap", "C.Hat", "O.Hat"];
/// Accessible drum channel names, in [`DrumRow::ALL`] order (a track channel is named after
/// the track).
pub const DRUM_CHANNEL_NAMES: [&str; DrumRow::COUNT] =
    ["Kick", "Snare", "Clap", "Closed hat", "Open hat"];
/// Labels of the insert knob grid in its EQ3 + sends view, row by row.
pub const GRID_LABELS: [&str; 6] = ["Low", "Mid", "High", "Freq", "Dly", "Rev"];
/// The insert values the knob grid edits in its EQ3 + sends view, in [`GRID_LABELS`] order.
pub const GRID_PARAMS: [InsertParam; 6] = [
    InsertParam::Eq3Low,
    InsertParam::Eq3Mid,
    InsertParam::Eq3High,
    InsertParam::Eq3MidFreq,
    InsertParam::DelaySend,
    InsertParam::ReverbSend,
];

/// Where a strip's fader ended up: its whole block (value, track) and the track column.
#[derive(Debug, Clone, Copy)]
struct FaderRects {
    block: Rect,
    track: Rect,
}

// ---------------------------------------------------------------------------------------------
// Names.

/// Accessible name of `channel` ("Closed hat"; a track channel is the track's name).
#[must_use]
pub fn channel_name(state: &AppState, channel: Channel) -> String {
    match channel.track() {
        Some(slot) => state.track(slot).name.clone(),
        None => DRUM_CHANNEL_NAMES[channel.index()].to_owned(),
    }
}

/// Caps label of `channel`'s strip (`"C.Hat"`; a track channel is the track's name).
#[must_use]
pub fn strip_label(state: &AppState, channel: Channel) -> String {
    match channel.track() {
        Some(slot) => state.track(slot).name.clone(),
        None => DRUM_STRIP_LABELS[channel.index()].to_owned(),
    }
}

/// Accessible name of a source strip knob or fader: `"Kick pan"`, `"Kick level"`, `"Synth
/// level"` (the sends are named for the engine's legacy strip sends the panel no longer
/// shows).
#[must_use]
pub fn control_name(state: &AppState, channel: Channel, param: ChannelParam) -> String {
    let what = match param {
        ChannelParam::Level => "level",
        ChannelParam::Pan => "pan",
        ChannelParam::Mute => "mute",
        ChannelParam::Solo => "solo",
        ChannelParam::DelaySend => "delay send",
        ChannelParam::ReverbSend => "reverb send",
    };
    format!("{} {what}", channel_name(state, channel))
}

/// Accessible name of a source's insert number box: `"Insert of Kick"`.
#[must_use]
pub fn source_box_name(name: &str) -> String {
    format!("Insert of {name}")
}

/// Visible text of a number box: `"M"` for the master, else the insert number.
#[must_use]
pub fn box_text(insert: u8) -> String {
    if insert == MASTER || insert > MAX_INSERT {
        "M".to_owned()
    } else {
        insert.to_string()
    }
}

/// Accessible name of an insert's head: `"Insert 6: Drums"`.
#[must_use]
pub fn insert_head_name(insert: u8, name: &str) -> String {
    format!("Insert {insert}: {name}")
}

/// Visible caps text of an insert's head: `"6 · Drums"`.
#[must_use]
pub fn insert_head_text(insert: u8, name: &str) -> String {
    format!("{insert} · {name}")
}

/// Accessible name of an insert's colour chip: `"Insert 6 swatch"`.
#[must_use]
pub fn insert_swatch_name(insert: u8) -> String {
    format!("Insert {insert} swatch")
}

/// Accessible name of an insert's pencil: `"Rename insert 6"`.
#[must_use]
pub fn insert_rename_name(insert: u8) -> String {
    format!("Rename insert {insert}")
}

/// Accessible name of a slot pill: `"Insert 6 slot 1"` (slots are 1-based in names).
#[must_use]
pub fn slot_name(insert: u8, slot: u8) -> String {
    format!("Insert {insert} slot {}", slot + 1)
}

/// Accessible name of a loaded slot's edit square: `"Edit insert 6 slot 1"`.
#[must_use]
pub fn slot_edit_name(insert: u8, slot: u8) -> String {
    format!("Edit insert {insert} slot {}", slot + 1)
}

/// Accessible name of a kind menu entry: `"Slot kind Comp"`.
#[must_use]
pub fn kind_pick_name(kind: SlotKind) -> String {
    format!("Slot kind {}", kind.label())
}

/// Accessible name of an insert's own value: `"Insert 6 low"`, `"Insert 6 mid hz"`,
/// `"Insert 6 delay send"`, `"Insert 6 level"`, `"Mute insert 6"`, `"Solo insert 6"`.
#[must_use]
pub fn insert_control_name(insert: u8, param: InsertParam) -> String {
    match param {
        InsertParam::Mute => format!("Mute insert {insert}"),
        InsertParam::Solo => format!("Solo insert {insert}"),
        InsertParam::DelaySend => format!("Insert {insert} delay send"),
        InsertParam::ReverbSend => format!("Insert {insert} reverb send"),
        InsertParam::Route => route_name(insert),
        _ => format!("Insert {insert} {}", param.label().to_lowercase()),
    }
}

/// Accessible name of a slot parameter knob: `"Insert 6 slot 1 thresh"`.
#[must_use]
pub fn slot_control_name(insert: u8, slot: u8, param: &SlotParam) -> String {
    format!(
        "Insert {insert} slot {} {}",
        slot + 1,
        param.label.to_lowercase()
    )
}

/// Accessible name of an insert's route pill: `"Route of insert 6"`.
#[must_use]
pub fn route_name(insert: u8) -> String {
    format!("Route of insert {insert}")
}

/// Visible text of a route pill: `"To MASTER"` or `"To 12"` (the kit font has no arrow glyph).
#[must_use]
pub fn route_text(route: u8) -> String {
    format!(
        "To {}",
        if route == MASTER {
            "MASTER".to_owned()
        } else {
            route.to_string()
        }
    )
}

/// Accessible name of a picker entry: `"Pick MASTER"` / `"Pick insert 7"`.
#[must_use]
pub fn pick_name(insert: u8) -> String {
    if insert == MASTER {
        "Pick MASTER".to_owned()
    } else {
        format!("Pick insert {insert}")
    }
}

/// The strips the source dock shows, left to right: the five drum channels, then the
/// channel of every active track in slot order (M10).
#[must_use]
pub fn visible_channels(state: &AppState) -> Vec<Channel> {
    Channel::ALL
        .iter()
        .copied()
        .filter(|channel| match channel.track() {
            Some(slot) => state.track_active(slot),
            None => true,
        })
        .collect()
}

/// Width of `count` source strips.
#[must_use]
pub fn sources_width(count: usize) -> f32 {
    count as f32 * SOURCE_STRIP_WIDTH
}

/// Width of all sixteen insert strips.
#[must_use]
pub fn inserts_width() -> f32 {
    MAX_INSERT as f32 * INSERT_STRIP_WIDTH
}

/// The inserts a source's number box may pick: the master (`0`) then `1..=16`.
#[must_use]
pub fn source_route_choices() -> Vec<u8> {
    std::iter::once(MASTER).chain(1..=MAX_INSERT).collect()
}

/// The targets insert `insert`'s ROUTE pill may pick: the master (`0`) then every
/// higher-numbered insert (the engine refuses the rest, so the picker never offers them).
#[must_use]
pub fn insert_route_choices(insert: u8) -> Vec<u8> {
    std::iter::once(MASTER)
        .chain((insert + 1)..=MAX_INSERT)
        .collect()
}

/// The display formatter for a slot parameter, by its unit.
#[must_use]
pub fn slot_fmt(param: &SlotParam) -> fn(f32) -> String {
    match param.unit {
        "dB" => fmt_level_db,
        ":1" => fmt_ratio_plain,
        "ms" => fmt_ms_value,
        "Hz" => fmt_hz,
        "x" => fmt_times,
        _ if param.max <= 1.0 => fmt_percent,
        _ => fmt_plain_int,
    }
}

fn fmt_ratio_plain(v: f32) -> String {
    format!("{v:.1}:1")
}

fn fmt_ms_value(v: f32) -> String {
    fmt_ms(v / 1000.0)
}

fn fmt_times(v: f32) -> String {
    format!("{}x", v.round() as i32)
}

fn fmt_plain_int(v: f32) -> String {
    format!("{}", v.round() as i32)
}

// ---------------------------------------------------------------------------------------------
// Layout.

/// Renders the MIXER panel into `ui`, filling the space it is given, and applies any control
/// change to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        // The strips are placed by hand from the cursor: no implicit spacing after the head.
        ui.spacing_mut().item_spacing.y = 0.0;
        controls::plugins_head(ui, theme, "Mixer");
        ui.add_space(HEAD_GAP);
        body(ui, state, theme);
    });
}

/// Lays `add` out in a child ui pinned to `rect` (top-down); the parent's cursor only moves
/// past the child afterwards, so the caller positions everything from one origin.
fn place<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        add,
    )
}

/// A horizontal `ScrollArea` filling `rect` whose content is `content_width` wide, with
/// `add` placing everything by hand from the content's origin.
fn dock<R>(
    ui: &mut Ui,
    rect: Rect,
    id_salt: &str,
    content_width: f32,
    add: impl FnOnce(&mut Ui, Pos2) -> R,
) -> R {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ScrollArea::horizontal()
                .id_salt(id_salt)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let origin = ui.cursor().min;
                    // Claim the content size up front so the scroll area knows how wide the
                    // strips are; everything is placed from `origin` by hand.
                    ui.allocate_rect(
                        Rect::from_min_size(origin, vec2(content_width, rect.height())),
                        Sense::hover(),
                    );
                    add(ui, origin)
                })
                .inner
        },
    )
    .inner
}

/// The three docks. The sources and the inserts each scroll horizontally inside their dock;
/// the master is pinned at the panel's right edge.
fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let channels = visible_channels(state);
    let width = ui.available_width();
    let height = ui.available_height();
    let origin = ui.cursor().min;
    let inserts_room =
        (width - SOURCE_DOCK_WIDTH - DOCK_GAP - MASTER_WIDTH).max(INSERT_STRIP_WIDTH);

    let sources_rect = Rect::from_min_size(origin, vec2(SOURCE_DOCK_WIDTH, height));
    dock(
        ui,
        sources_rect,
        "mixer_sources",
        sources_width(channels.len()),
        |ui, origin| {
            for (i, channel) in channels.iter().enumerate() {
                let x = origin.x + i as f32 * SOURCE_STRIP_WIDTH;
                source_strip(ui, state, theme, *channel, pos2(x, origin.y));
            }
        },
    );

    let inserts_origin = pos2(origin.x + SOURCE_DOCK_WIDTH + DOCK_GAP, origin.y);
    let inserts_rect = Rect::from_min_size(inserts_origin, vec2(inserts_room, height));
    let fader = dock(
        ui,
        inserts_rect,
        "mixer_inserts",
        inserts_width(),
        |ui, origin| {
            let mut last = None;
            for n in 1..=MAX_INSERT {
                let x = origin.x + f32::from(n - 1) * INSERT_STRIP_WIDTH;
                last = Some(insert_strip(ui, state, theme, n, pos2(x, origin.y), height));
            }
            last
        },
    );
    if let Some(fader) = fader {
        master(
            ui,
            state,
            theme,
            pos2(inserts_origin.x + inserts_room, origin.y),
            fader,
        );
    }
    // The body's own footprint, whatever the docks took.
    ui.allocate_rect(
        Rect::from_min_size(origin, vec2(width, height)),
        Sense::hover(),
    );
}

// ---------------------------------------------------------------------------------------------
// Source strips.

/// One source strip at `origin`: chip, caps label, number box, LEVEL and PAN knobs, M / S.
fn source_strip(ui: &mut Ui, state: &mut AppState, theme: &Theme, channel: Channel, origin: Pos2) {
    let name = channel_name(state, channel);
    let label = strip_label(state, channel);
    let cx = origin.x + SOURCE_STRIP_WIDTH * 0.5;
    let mut y = origin.y;

    // The track's colour chip (drums leave the row empty so the labels line up).
    if let Some(slot) = channel.track() {
        let track = state.track(slot).clone();
        let chip =
            Rect::from_center_size(pos2(cx, y + TRACK_CHIP * 0.5), vec2(TRACK_CHIP, TRACK_CHIP));
        place(ui, chip, |ui| {
            controls::track_chip(ui, theme, track.colour, &track.name);
        });
    }
    y += TRACK_CHIP + BLOCK_GAP;

    // The caps label, cut to the strip.
    let label_rect = Rect::from_min_size(
        pos2(origin.x, y),
        vec2(SOURCE_STRIP_WIDTH, KNOB_LABEL + 4.0),
    );
    place(ui, label_rect, |ui| {
        controls::caps_label_truncated(ui, theme, &label, SOURCE_STRIP_WIDTH).on_hover_text(&name);
    });
    y += KNOB_LABEL + 4.0 + BLOCK_GAP;

    // The insert number box.
    let box_rect = Rect::from_center_size(pos2(cx, y + BOX_SIZE.y * 0.5), BOX_SIZE);
    number_box(ui, state, theme, channel, &name, box_rect);
    y += BOX_SIZE.y + BLOCK_GAP;

    // LEVEL then PAN.
    let knob_x = cx - KNOB * 0.5;
    source_knob(
        ui,
        state,
        theme,
        channel,
        ChannelParam::Level,
        Rect::from_min_size(pos2(knob_x, y), vec2(KNOB, KNOB)),
    );
    y += KNOB + KNOB_GAP;
    source_knob(
        ui,
        state,
        theme,
        channel,
        ChannelParam::Pan,
        Rect::from_min_size(pos2(knob_x, y), vec2(KNOB, KNOB)),
    );
    y += KNOB + BLOCK_GAP;

    // M / S, centred in the strip.
    let mute_rect = Rect::from_min_size(pos2(cx - TOGGLE_SQUARE.x * 0.5, y), TOGGLE_SQUARE);
    let solo_rect = mute_rect.translate(vec2(0.0, TOGGLE_SQUARE.y + TOGGLE_GAP));
    let mute = state.mixer.strip(channel).mute;
    let response = place(ui, mute_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "M", mute, TOGGLE_SQUARE)
    })
    .inner;
    if response.clicked() {
        state.toggle_mute(channel);
    }
    let mute = state.mixer.strip(channel).mute;
    response.widget_info(|| {
        WidgetInfo::selected(Role::Button, ui.is_enabled(), mute, format!("Mute {name}"))
    });

    let solo = state.mixer.strip(channel).solo;
    let response = place(ui, solo_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "S", solo, TOGGLE_SQUARE)
    })
    .inner;
    if response.clicked() {
        state.toggle_solo(channel);
    }
    let solo = state.mixer.strip(channel).solo;
    response.widget_info(|| {
        WidgetInfo::selected(Role::Button, ui.is_enabled(), solo, format!("Solo {name}"))
    });
}

/// A source's insert number box at `rect`: a raised pill reading the insert number (`M` for
/// the master). A click selects that insert and opens the picker (MASTER, then 1..=16 with
/// the inserts' names); the wheel steps the number. Accessible as [`source_box_name`].
fn number_box(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    channel: Channel,
    name: &str,
    rect: Rect,
) {
    let route = state.mixer.route(channel);
    let text = box_text(route);
    let response = place(ui, rect, |ui| {
        neu::toggle_button_sized(ui, theme, &text, false, rect.size())
    })
    .inner;
    let label = source_box_name(name);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &label));
    let response = response
        .on_hover_text(format!(
            "Mixer insert: {} goes to {} · click to pick, wheel to step",
            name,
            state.route_name(route)
        ))
        .on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() {
        state.select_insert(route, false);
    }
    let steps = neu::wheel_steps(ui, &response);
    if steps != 0 {
        let target = (i32::from(route) + steps).clamp(0, i32::from(MAX_INSERT)) as u8;
        state.set_route(channel, target);
    }
    if let Some(picked) = route_picker(theme, &response, state, route, &source_route_choices()) {
        state.set_route(channel, picked);
        state.select_insert(picked, false);
    }
}

/// Side of a picker's number squares and the gap between them.
pub const PICK_SQUARE: f32 = 28.0;
pub const PICK_GAP: f32 = 4.0;
/// Number squares per picker row (FL's number box lists the inserts in a compact grid).
pub const PICK_COLUMNS: usize = 4;

/// The picker menu under `anchor` (opened by its click): a `MASTER` pill, then the inserts
/// of `choices` as rows of [`PICK_COLUMNS`] [`PICK_SQUARE`] squares reading their numbers
/// (the insert's name is the tooltip), the current one lit; accessible as [`pick_name`].
/// Returns the pick, if any. The TRACKS panel and the sequencer rows reuse it for their
/// number boxes.
pub fn route_picker(
    theme: &Theme,
    anchor: &Response,
    state: &AppState,
    current: u8,
    choices: &[u8],
) -> Option<u8> {
    let mut picked = None;
    egui::Popup::menu(anchor).show(|ui| {
        ui.spacing_mut().item_spacing = vec2(PICK_GAP, PICK_GAP);
        ui.add_space(2.0);
        if choices.contains(&MASTER) {
            let response = neu::toggle_button(ui, theme, "Master", current == MASTER);
            let label = pick_name(MASTER);
            let on = current == MASTER;
            response
                .widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, &label));
            if response.on_hover_text("Straight to the master").clicked() {
                picked = Some(MASTER);
            }
        }
        let inserts: Vec<u8> = choices.iter().copied().filter(|n| *n != MASTER).collect();
        for row in inserts.chunks(PICK_COLUMNS) {
            ui.horizontal(|ui| {
                for &n in row {
                    let on = n == current;
                    let response = neu::toggle_button_sized(
                        ui,
                        theme,
                        &n.to_string(),
                        on,
                        vec2(PICK_SQUARE, PICK_SQUARE),
                    );
                    let label = pick_name(n);
                    response.widget_info(|| {
                        WidgetInfo::selected(Role::Button, ui.is_enabled(), on, &label)
                    });
                    if response
                        .on_hover_text(format!("{n} · {}", state.insert_display_name(n)))
                        .clicked()
                    {
                        picked = Some(n);
                    }
                }
            });
        }
        ui.add_space(2.0);
    });
    picked
}

/// An unlabelled `Sm` knob for one source strip value at `rect` (its tooltip names it);
/// accessible as [`control_name`].
fn source_knob(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    channel: Channel,
    param: ChannelParam,
    rect: Rect,
) -> Response {
    let range = param.range();
    let bipolar = param == ChannelParam::Pan;
    let spec = KnobSpec {
        label: "",
        min: *range.start(),
        max: *range.end(),
        default: param.default_value(channel),
        log: false,
        bipolar,
        size: KnobSize::Sm,
        fmt: if bipolar { fmt_pan } else { fmt_level_db },
    };
    let name = control_name(state, channel, param);
    let before = state.mixer.get(channel, param);
    let mut value = before;
    let response = place(ui, rect, |ui| {
        neu::knob_noted(ui, theme, &mut value, &spec, Some(&name))
    })
    .inner;
    if response.changed() || value != before {
        state.edit_mixer(channel, param, value, response.drag_started());
    }
    let shown = state.mixer.get(channel, param);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(shown), &name));
    response
}

// ---------------------------------------------------------------------------------------------
// Insert strips.

/// The inline rename editor's id for insert `insert` (global, like the TRACKS panel's).
fn rename_id(insert: u8) -> Id {
    Id::NULL.with(("mixer_insert_rename", insert))
}

/// Whether the inline rename editor of insert `insert` is open.
#[must_use]
pub fn is_renaming(ui: &Ui, insert: u8) -> bool {
    neu::is_editing(ui, rename_id(insert))
}

/// One insert strip at `origin`, `height` tall: head, slot pills, the knob grid, the route
/// pill and the fader row. Returns the fader's rects so the master column lines up.
fn insert_strip(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    insert: u8,
    origin: Pos2,
    height: f32,
) -> FaderRects {
    let x = origin.x + 1.0;
    let mut y = origin.y;
    let idle = state.insert_is_idle(insert);
    let highlighted = state.highlighted_insert() == insert;
    let selected = state.is_insert_selected(insert);

    insert_head(
        ui,
        state,
        theme,
        insert,
        Rect::from_min_size(pos2(x, y), vec2(GRID_WIDTH, ROW_H)),
        idle,
        highlighted,
        selected,
    );
    y += ROW_H + BLOCK_GAP;

    for slot in 0..SLOT_COUNT as u8 {
        slot_row(
            ui,
            state,
            theme,
            insert,
            slot,
            Rect::from_min_size(pos2(x, y), vec2(GRID_WIDTH, ROW_H)),
        );
        y += ROW_H + KNOB_GAP;
    }
    y += BLOCK_GAP - KNOB_GAP;

    knob_grid(ui, state, theme, insert, pos2(x, y));
    y += GRID_HEIGHT + BLOCK_GAP;

    route_pill(
        ui,
        state,
        theme,
        insert,
        Rect::from_min_size(pos2(x, y), vec2(GRID_WIDTH, ROW_H)),
    );
    y += ROW_H + BLOCK_GAP;

    fader_row(ui, state, theme, insert, pos2(x, y), origin.y + height)
}

/// The head row: the accent bar (highlighted), the inset well (selected), the colour chip,
/// the `N · name` caps label (a click selects, Shift-click extends, a double-click renames)
/// and the pencil.
#[allow(clippy::too_many_arguments)]
fn insert_head(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    insert: u8,
    rect: Rect,
    idle: bool,
    highlighted: bool,
    selected: bool,
) {
    let ui_insert = state.insert_ui(insert).clone();
    let shown_name = state.insert_display_name(insert);
    let visible = ui.is_rect_visible(rect);
    if visible && selected {
        paint_soft_rect(ui.painter(), rect, 6.0, Depth::InsetS, theme);
    }
    if visible && highlighted {
        let bar = Rect::from_min_size(
            pos2(rect.min.x, rect.center().y - 6.0),
            vec2(ACCENT_BAR_WIDTH, 12.0),
        );
        paint_soft_rect(
            ui.painter(),
            bar,
            ACCENT_BAR_WIDTH * 0.5,
            Depth::OnGlow,
            theme,
        );
    }
    let pencil = 10.0;
    let chip_rect = Rect::from_center_size(
        pos2(rect.min.x + 6.0 + TRACK_CHIP * 0.5, rect.center().y),
        vec2(TRACK_CHIP, TRACK_CHIP),
    );
    let pencil_rect = Rect::from_center_size(
        pos2(rect.max.x - 2.0 - pencil * 0.5, rect.center().y),
        vec2(pencil, pencil),
    );
    let name_left = chip_rect.max.x + 4.0;
    let name_rect = Rect::from_min_max(
        pos2(name_left, rect.min.y),
        pos2(pencil_rect.min.x - 2.0, rect.max.y),
    );

    // The name block: the insert's button.
    let block = ui.interact(name_rect, head_id(ui, insert, "head"), Sense::click());
    let head_name = insert_head_name(insert, &shown_name);
    block.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, &head_name));
    let sources: Vec<String> = state
        .insert_sources(insert)
        .into_iter()
        .map(|c| channel_name(state, c))
        .collect();
    let block = block
        .on_hover_text(format!(
            "{}\n{}\nClick selects, Shift-click extends, double-click renames",
            head_name,
            if sources.is_empty() {
                "No source routed here".to_owned()
            } else {
                format!("Sources: {}", sources.join(", "))
            }
        ))
        .on_hover_cursor(CursorIcon::PointingHand);
    if block.double_clicked() {
        neu::begin_edit(ui, rename_id(insert), &shown_name);
    } else if block.clicked() {
        state.select_insert(insert, controls::click_shift(ui));
    }

    // The colour chip: a click cycles the swatch.
    let chip = ui.interact(chip_rect, head_id(ui, insert, "chip"), Sense::click());
    if visible {
        let colour = theme.track_colour(ui_insert.colour);
        let colour = if idle {
            colour.gamma_multiply(neu::DIMMED_OPACITY)
        } else {
            colour
        };
        ui.painter()
            .circle_filled(chip_rect.center(), TRACK_CHIP * 0.5, colour);
        if chip.hovered() {
            ui.painter().circle_stroke(
                chip_rect.center(),
                TRACK_CHIP * 0.5 + 2.5,
                Stroke::new(1.5, colour),
            );
        }
    }
    let swatch = insert_swatch_name(insert);
    chip.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &swatch));
    if chip
        .on_hover_text("Colour: a click cycles the eight swatches")
        .on_hover_cursor(CursorIcon::PointingHand)
        .clicked()
    {
        state.set_insert_colour(insert, ui_insert.colour.wrapping_add(1));
    }

    // The name, or the rename editor over it.
    let font = TextKind::Label.font_id();
    let editor = neu::show_edit(ui, theme, rename_id(insert), name_rect, font, Align::Min);
    match editor {
        ValueEditOutput::Committed(typed) => {
            state.rename_insert(insert, &typed);
        }
        ValueEditOutput::Editing => {}
        ValueEditOutput::Idle | ValueEditOutput::Cancelled => {
            if visible {
                let colour = if idle {
                    theme.text_secondary.gamma_multiply(neu::DIMMED_OPACITY)
                } else if selected || highlighted || block.hovered() {
                    theme.text_primary
                } else {
                    theme.text_secondary
                };
                paint_caps_truncated(
                    ui,
                    name_rect.left_center(),
                    &insert_head_text(insert, &shown_name),
                    TextKind::Label,
                    colour,
                    name_rect.width(),
                );
            }
        }
    }

    // The pencil.
    let pencil_response = ui.interact(pencil_rect, head_id(ui, insert, "pencil"), Sense::click());
    if visible {
        let colour = if pencil_response.hovered() {
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
    let rename = insert_rename_name(insert);
    pencil_response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &rename));
    if pencil_response
        .on_hover_text("Rename (or double-click the name) · Enter commits, Esc cancels")
        .on_hover_cursor(CursorIcon::PointingHand)
        .clicked()
    {
        neu::begin_edit(ui, rename_id(insert), &shown_name);
    }
}

/// A stable id for an insert head's sub-control.
fn head_id(ui: &Ui, insert: u8, part: &str) -> Id {
    ui.scope_id().with(("mixer_insert", insert, part))
}

/// One slot row: the kind pill (`- - -` or the kind name; a click opens the kind menu) and,
/// for a loaded slot, the edit square that shows its knobs in the grid. For the parametric
/// EQ the square opens the EQ window on this slot instead (lit while it shows this slot),
/// so the EQ shapes only this insert.
fn slot_row(ui: &mut Ui, state: &mut AppState, theme: &Theme, insert: u8, slot: u8, rect: Rect) {
    let kind = state.mixer.insert(insert).slot(slot).kind;
    let is_eq = kind == SlotKind::ParametricEq;
    let open = if is_eq {
        state.is_slot_eq_open(insert, slot)
    } else {
        state.open_slot == Some((insert, slot))
    };
    let pill_rect = Rect::from_min_size(rect.min, vec2(SLOT_PILL_WIDTH, ROW_H));
    let text = if kind == SlotKind::Off {
        EMPTY_SLOT_LABEL.to_owned()
    } else {
        kind.label().to_owned()
    };
    let pill = place(ui, pill_rect, |ui| {
        neu::toggle_button_sized(ui, theme, &text, open, pill_rect.size())
    })
    .inner;
    let name = slot_name(insert, slot);
    pill.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &name));
    let pill = pill.on_hover_text(format!(
        "Slot {}: {} · click to pick an effect",
        slot + 1,
        if kind == SlotKind::Off {
            "empty"
        } else {
            kind.label()
        }
    ));
    let mut picked = None;
    egui::Popup::menu(&pill).show(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add_space(4.0);
        for candidate in SlotKind::ALL {
            let on = candidate == kind;
            let response = neu::toggle_button(ui, theme, candidate.label(), on);
            let label = kind_pick_name(candidate);
            response
                .widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, &label));
            let response = match candidate {
                SlotKind::Eq3 => response.on_hover_text("Three-band EQ, three knobs"),
                SlotKind::ParametricEq => response
                    .on_hover_text("Seven-band parametric EQ in its own window, on this insert"),
                _ => response,
            };
            if response.clicked() {
                picked = Some(candidate);
            }
        }
        ui.add_space(4.0);
    });
    if let Some(candidate) = picked {
        let stored = state.set_slot_kind(insert, slot, candidate);
        // A freshly loaded effect shows its knobs (the EQ its window) at once.
        if stored == SlotKind::ParametricEq {
            if !state.is_slot_eq_open(insert, slot) {
                state.open_slot_eq(insert, slot);
            }
        } else if stored != SlotKind::Off {
            state.open_slot = Some((insert, slot));
        }
    }

    if kind != SlotKind::Off {
        let edit_rect = Rect::from_min_size(
            pos2(pill_rect.max.x + KNOB_GAP, rect.min.y),
            vec2(SLOT_EDIT, SLOT_EDIT),
        );
        let edit = place(ui, edit_rect, |ui| {
            neu::toggle_button_sized(ui, theme, "", open, edit_rect.size())
        })
        .inner;
        if ui.is_rect_visible(edit_rect) {
            let colour = if open {
                theme.on_accent
            } else if edit.hovered() {
                theme.text_primary
            } else {
                theme.text_secondary
            };
            paint_knob_glyph(ui, edit_rect, colour);
        }
        let label = slot_edit_name(insert, slot);
        edit.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), open, &label));
        let hint = match (is_eq, open) {
            (true, true) => "Close this insert's EQ window",
            (true, false) => "Open this EQ (it shapes only this insert)",
            (false, true) => "Back to the EQ and sends",
            (false, false) => "Show this effect's knobs",
        };
        if edit.on_hover_text(hint).clicked() {
            if is_eq {
                state.open_slot_eq(insert, slot);
            } else {
                state.toggle_open_slot(insert, slot);
            }
        }
    }
}

/// The 3 x 2 knob block at `origin`: the open slot's parameters when one of this insert's
/// slots is open, else LOW / MID / HIGH and FREQ / DLY / REV.
fn knob_grid(ui: &mut Ui, state: &mut AppState, theme: &Theme, insert: u8, origin: Pos2) {
    let open = state.open_slot.filter(|(n, s)| {
        *n == insert && state.mixer.insert(insert).slot(*s).kind.param_count() > 0
    });
    for (index, &param) in GRID_PARAMS.iter().enumerate() {
        let col = index % GRID_COLS;
        let row = index / GRID_COLS;
        let knob_rect = Rect::from_min_size(
            pos2(
                origin.x + col as f32 * (KNOB + KNOB_GAP),
                origin.y + row as f32 * (GRID_ROW + KNOB_GAP),
            ),
            vec2(KNOB, KNOB),
        );
        let label = match open {
            Some((_, slot)) => slot_knob(ui, state, theme, insert, slot, index as u8, knob_rect),
            None => insert_knob(ui, state, theme, insert, param, knob_rect),
        };
        if let Some(label) = label
            && ui.is_rect_visible(knob_rect)
        {
            ui.painter().text(
                pos2(knob_rect.center().x, knob_rect.max.y + 1.0),
                Align2::CENTER_TOP,
                label.to_uppercase(),
                FontId::proportional(KNOB_LABEL_SIZE),
                theme.text_secondary,
            );
        }
    }
}

/// One EQ3 / send knob of insert `insert` at `rect`; accessible as [`insert_control_name`].
/// Returns the 9 pt label to paint under it.
fn insert_knob(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    insert: u8,
    param: InsertParam,
    rect: Rect,
) -> Option<String> {
    let range = param.range();
    let (fmt, bipolar, log): (fn(f32) -> String, bool, bool) = match param {
        InsertParam::Eq3Low | InsertParam::Eq3Mid | InsertParam::Eq3High => {
            (fmt_level_db, true, false)
        }
        InsertParam::Eq3MidFreq => (fmt_hz, false, true),
        _ => (fmt_percent, false, false),
    };
    let spec = KnobSpec {
        label: "",
        min: *range.start(),
        max: *range.end(),
        default: param.default_value(insert),
        log,
        bipolar,
        size: KnobSize::Sm,
        fmt,
    };
    let name = insert_control_name(insert, param);
    let before = state.mixer.get_insert(insert, param);
    let mut value = before;
    let response = place(ui, rect, |ui| {
        neu::knob_noted(ui, theme, &mut value, &spec, Some(&name))
    })
    .inner;
    if response.changed() || value != before {
        state.edit_insert(insert, param, value, response.drag_started());
    }
    let shown = state.mixer.get_insert(insert, param);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(shown), &name));
    let position = GRID_PARAMS.iter().position(|p| *p == param)?;
    Some(GRID_LABELS[position].to_owned())
}

/// Parameter `index` of slot `slot` of insert `insert` at `rect` (nothing for an index the
/// kind does not have); accessible as [`slot_control_name`]. Returns the label to paint.
fn slot_knob(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    insert: u8,
    slot: u8,
    index: u8,
    rect: Rect,
) -> Option<String> {
    let current = *state.mixer.insert(insert).slot(slot);
    let param = current.param(usize::from(index))?;
    let spec = KnobSpec {
        label: "",
        min: param.min,
        max: param.max,
        default: param.default,
        log: param.unit == "Hz",
        bipolar: param.min < 0.0 && param.max > 0.0,
        size: KnobSize::Sm,
        fmt: slot_fmt(param),
    };
    let name = slot_control_name(insert, slot, param);
    let before = current.get(usize::from(index));
    let mut value = before;
    let response = place(ui, rect, |ui| {
        neu::knob_noted(ui, theme, &mut value, &spec, Some(&name))
    })
    .inner;
    if response.changed() || value != before {
        state.edit_slot_param(insert, slot, index, value, response.drag_started());
    }
    let shown = state
        .mixer
        .insert(insert)
        .slot(slot)
        .get(usize::from(index));
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(shown), &name));
    Some(param.label.to_owned())
}

/// The ROUTE pill at `rect`: `→ MASTER` or `→ N`; a click opens the picker with the master
/// and every higher-numbered insert. Accessible as [`route_name`].
fn route_pill(ui: &mut Ui, state: &mut AppState, theme: &Theme, insert: u8, rect: Rect) {
    let route = state.mixer.insert(insert).route;
    let text = route_text(route);
    let response = place(ui, rect, |ui| {
        neu::toggle_button_sized(ui, theme, &text, false, rect.size())
    })
    .inner;
    let name = route_name(insert);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &name));
    let response = response
        .on_hover_text(format!(
            "Route to: {} · the master or a higher-numbered insert (groups)",
            state.route_name(route)
        ))
        .on_hover_cursor(CursorIcon::PointingHand);
    if let Some(picked) = route_picker(
        theme,
        &response,
        state,
        route,
        &insert_route_choices(insert),
    ) {
        state.set_insert(insert, InsertParam::Route, f32::from(picked));
    }
}

/// The fader row of insert `insert` from `origin` down to `bottom`: M / S, the fader (its
/// track fitted to the room), the peak meter and the gain-reduction bar.
fn fader_row(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    insert: u8,
    origin: Pos2,
    bottom: f32,
) -> FaderRects {
    let group_width = TOGGLE_SQUARE.x + KNOB_GAP + SLIDER_WIDTH + KNOB_GAP;
    let x = origin.x + ((GRID_WIDTH - group_width) * 0.5).max(0.0);
    let fader_x = x + TOGGLE_SQUARE.x + KNOB_GAP;
    let value_height = ui.fonts_mut(|f| f.row_height(&TextKind::Value.font_id()));
    let track_height = (bottom - origin.y - value_height - neu::slider::SLIDER_GAP).max(0.0);

    let name = insert_control_name(insert, InsertParam::Level);
    let spec = SliderSpec {
        label: &name,
        min: MIN_LEVEL_DB,
        max: MAX_LEVEL_DB,
        default: 0.0,
        fmt: fmt_level_db,
    };
    let before = state.mixer.get_insert(insert, InsertParam::Level);
    let mut value = before;
    let rect = Rect::from_min_max(
        pos2(fader_x, origin.y),
        pos2(fader_x + SLIDER_WIDTH, bottom),
    );
    let inner = place(ui, rect, |ui| {
        neu::fader_sized(ui, theme, &mut value, &spec, track_height, false)
    });
    let response = inner.inner;
    if response.changed() || value != before {
        state.move_insert_fader(insert, value, response.drag_started());
    }
    let shown = state.mixer.get_insert(insert, InsertParam::Level);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(shown), &name));
    let fader = FaderRects {
        block: inner.response.rect,
        track: response.rect,
    };

    // The meter in the fader column's right margin, the GR bar beside it.
    let level = state.shown_meters.insert[insert_index(insert)];
    let meter_size = vec2(METER_TRACK.x, fader.track.height());
    let meter = Rect::from_min_size(
        pos2(fader.track.center().x + METER_OFFSET, fader.track.min.y),
        meter_size,
    );
    place(ui, meter, |ui| neu::meter(ui, theme, level, meter_size));
    let compressed = state
        .mixer
        .insert(insert)
        .slots
        .iter()
        .any(|s| s.kind == SlotKind::Compressor);
    if compressed {
        let gr = state.shown_meters.insert_gr[insert_index(insert)];
        let bar = Rect::from_min_size(
            pos2(meter.max.x + GR_GAP, meter.min.y),
            vec2(GR_WIDTH, meter.height()),
        );
        gr_meter(ui, theme, bar, gr);
    }

    // M / S in the left column, top-aligned with the track.
    let mute_rect = Rect::from_min_size(pos2(x, fader.track.min.y), TOGGLE_SQUARE);
    let solo_rect = mute_rect.translate(vec2(0.0, TOGGLE_SQUARE.y + TOGGLE_GAP));
    let mute = state.mixer.insert(insert).mute;
    let response = place(ui, mute_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "M", mute, TOGGLE_SQUARE)
    })
    .inner;
    if response.clicked() {
        state.toggle_insert_mute(insert);
    }
    let mute = state.mixer.insert(insert).mute;
    let label = insert_control_name(insert, InsertParam::Mute);
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), mute, &label));

    let solo = state.mixer.insert(insert).solo;
    let response = place(ui, solo_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "S", solo, TOGGLE_SQUARE)
    })
    .inner;
    if response.clicked() {
        state.toggle_insert_solo(insert);
    }
    let solo = state.mixer.insert(insert).solo;
    let label = insert_control_name(insert, InsertParam::Solo);
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), solo, &label));

    fader
}

/// A thin gain-reduction bar at `rect`: an inset track filled from the top by `gr_db` over
/// [`GR_FULL_DB`] in the accent colour. Accessible as [`GR_METER_NAME`].
fn gr_meter(ui: &mut Ui, theme: &Theme, rect: Rect, gr_db: f32) {
    let response = ui.interact(
        rect,
        ui.scope_id().with(("gr_meter", rect.min.x as i32)),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        paint_soft_rect(ui.painter(), rect, GR_WIDTH * 0.5, Depth::InsetS, theme);
        let fraction = if gr_db.is_finite() {
            (gr_db / GR_FULL_DB).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if fraction > 0.0 {
            let fill = Rect::from_min_size(rect.min, vec2(rect.width(), rect.height() * fraction));
            ui.painter().rect_filled(fill, GR_WIDTH * 0.5, theme.accent);
        }
    }
    response
        .on_hover_text(format!("Gain reduction: {gr_db:.1} dB"))
        .widget_info(|| WidgetInfo::labeled(Role::Label, ui.is_enabled(), GR_METER_NAME));
}

/// A small knob glyph (ring plus pointer) centred in `rect`, for the slot edit squares.
fn paint_knob_glyph(ui: &Ui, rect: Rect, colour: egui::Color32) {
    let c = rect.center();
    let r = rect.width() * 0.28;
    let painter = ui.painter();
    painter.circle_stroke(c, r, Stroke::new(1.5, colour));
    painter.line_segment([c, pos2(c.x, c.y - r)], Stroke::new(1.5, colour));
}

/// A pencil glyph fitted to `rect` (the TRACKS panel's, at a smaller size).
fn paint_pencil(ui: &Ui, rect: Rect, stroke: Stroke) {
    let r = rect.shrink(1.0);
    let painter = ui.painter();
    painter.line_segment(
        [pos2(r.min.x, r.max.y), pos2(r.max.x - 1.5, r.min.y + 1.5)],
        stroke,
    );
    painter.line_segment(
        [pos2(r.min.x + 2.0, r.max.y), pos2(r.max.x, r.min.y + 3.5)],
        stroke,
    );
}

/// Paints `text` in `kind`'s caps style at `pos` (left centre), cut to `max_width` with an
/// ellipsis.
fn paint_caps_truncated(
    ui: &Ui,
    pos: Pos2,
    text: &str,
    kind: TextKind,
    colour: egui::Color32,
    max_width: f32,
) {
    use egui::epaint::text::LayoutJob;
    let mut job = LayoutJob::single_section(kind.display_string(text), kind.format(colour));
    job.wrap.max_width = max_width.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    let top_left = pos2(pos.x, pos.y - galley.size().y * 0.5);
    ui.painter().galley(top_left, galley, colour);
}

// ---------------------------------------------------------------------------------------------
// Master.

/// The master column at `origin`, [`MASTER_WIDTH`] wide: the MASTER gain knob level with the
/// faders' value row, the LIMIT toggle and the stereo meter down to the faders' bottom edge.
fn master(ui: &mut Ui, state: &mut AppState, theme: &Theme, origin: Pos2, fader: FaderRects) {
    // The Deck knob block (56 wide with its label and value) takes the whole column.
    let knob_rect = Rect::from_min_size(
        pos2(
            origin.x,
            (fader.block.min.y + MASTER_DROP).min(fader.track.min.y),
        ),
        vec2(MASTER_WIDTH, KNOB * 2.0),
    );
    let gain = place(ui, knob_rect, |ui| {
        Knob::new(ParamId::MasterGain, fmt_db)
            .label("Master")
            .small()
            .show(ui, state, theme)
    })
    .response
    .rect;

    let limit_rect = Rect::from_min_size(
        pos2(origin.x, gain.max.y + TOGGLE_GAP),
        vec2(MASTER_WIDTH, TOGGLE_SQUARE.y),
    );
    let on = state.limiter_on();
    let response = place(ui, limit_rect, |ui| {
        neu::toggle_button_sized(ui, theme, "Limit", on, limit_rect.size())
    })
    .inner;
    if response.clicked() {
        state.edit_param(ParamId::LimiterOn, if on { 0.0 } else { 1.0 }, true);
    }

    let top = limit_rect.max.y + TOGGLE_GAP;
    let height = (fader.block.max.y - top).max(STEREO_METER_SIZE.x);
    let size = vec2(STEREO_METER_SIZE.x, height);
    let meter_rect = Rect::from_min_size(pos2(origin.x + (MASTER_WIDTH - size.x) * 0.5, top), size);
    let levels = (state.shown_meters.master[0], state.shown_meters.master[1]);
    place(ui, meter_rect, |ui| {
        neu::stereo_meter(ui, theme, levels, size)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_channel_and_parameter() {
        let (mut state, _engine) = AppState::headless(neu::ThemeMode::Dark);
        assert_eq!(
            control_name(&state, Channel::Kick, ChannelParam::Pan),
            "Kick pan"
        );
        assert_eq!(
            control_name(&state, Channel::ClosedHat, ChannelParam::DelaySend),
            "Closed hat delay send"
        );
        assert_eq!(
            control_name(&state, Channel::Synth, ChannelParam::Level),
            "Synth level"
        );
        assert_eq!(channel_name(&state, Channel::OpenHat), "Open hat");
        assert_eq!(strip_label(&state, Channel::ClosedHat), "C.Hat");
        assert_eq!(strip_label(&state, Channel::Synth), "Synth");
        // A track strip is named after the track (M10).
        state.rename_track(0, "Lead");
        assert_eq!(
            control_name(&state, Channel::Synth, ChannelParam::Level),
            "Lead level"
        );
        assert_eq!(strip_label(&state, Channel::Track(0)), "Lead");
        // M11 names.
        assert_eq!(source_box_name("Kick"), "Insert of Kick");
        assert_eq!(box_text(0), "M");
        assert_eq!(box_text(6), "6");
        assert_eq!(box_text(17), "M");
        assert_eq!(insert_head_name(6, "Drums"), "Insert 6: Drums");
        assert_eq!(insert_head_text(6, "Drums"), "6 · Drums");
        assert_eq!(insert_swatch_name(6), "Insert 6 swatch");
        assert_eq!(insert_rename_name(6), "Rename insert 6");
        assert_eq!(slot_name(6, 0), "Insert 6 slot 1");
        assert_eq!(slot_edit_name(6, 2), "Edit insert 6 slot 3");
        assert_eq!(kind_pick_name(SlotKind::Compressor), "Slot kind Comp");
        assert_eq!(insert_control_name(6, InsertParam::Eq3Low), "Insert 6 low");
        assert_eq!(
            insert_control_name(6, InsertParam::Eq3MidFreq),
            "Insert 6 mid hz"
        );
        assert_eq!(
            insert_control_name(6, InsertParam::ReverbSend),
            "Insert 6 reverb send"
        );
        assert_eq!(insert_control_name(6, InsertParam::Level), "Insert 6 level");
        assert_eq!(insert_control_name(6, InsertParam::Mute), "Mute insert 6");
        assert_eq!(insert_control_name(6, InsertParam::Solo), "Solo insert 6");
        assert_eq!(
            slot_control_name(6, 0, &engine::inserts::COMPRESSOR_PARAMS[0]),
            "Insert 6 slot 1 thresh"
        );
        assert_eq!(route_name(6), "Route of insert 6");
        assert_eq!(route_text(0), "To MASTER");
        assert_eq!(route_text(12), "To 12");
        assert_eq!(pick_name(0), "Pick MASTER");
        assert_eq!(pick_name(7), "Pick insert 7");
    }

    #[test]
    fn only_active_tracks_have_strips() {
        let (mut state, _engine) = AppState::headless(neu::ThemeMode::Dark);
        let mut drums: Vec<Channel> = DrumRow::ALL.iter().map(|r| Channel::from_row(*r)).collect();
        drums.push(Channel::Track(0));
        assert_eq!(visible_channels(&state), drums);
        let slot = state.add_track().unwrap();
        drums.push(Channel::from_track(slot));
        assert_eq!(visible_channels(&state), drums);
        state.remove_track(0);
        assert_eq!(visible_channels(&state).len(), DrumRow::COUNT + 1);
        assert_eq!(
            visible_channels(&state).last(),
            Some(&Channel::from_track(slot))
        );
    }

    /// The width budget: the three docks fill the panel's content width exactly, the
    /// source dock shows four strips, the inserts dock at least two whole insert strips,
    /// and an insert strip's fader group and knob grid fit inside it.
    #[test]
    fn the_docks_fit_the_panel_width() {
        assert_eq!(CONTENT_WIDTH, 436.0);
        assert_eq!(
            SOURCE_DOCK_WIDTH + DOCK_GAP + INSERTS_DOCK_WIDTH + MASTER_WIDTH,
            CONTENT_WIDTH
        );
        assert_eq!(SOURCE_DOCK_WIDTH, 144.0);
        // M12: the Deck mini knob is 22; the 34 cell keeps the M11 strip geometry until the
        // M13 mixer drawer lays the strips out again.
        assert!(KNOB >= KnobSize::Sm.block_width());
        assert_eq!(INSERT_STRIP_WIDTH, 108.0);
        // The constant invariants (a knob, the M / S squares and the number box fit a source
        // strip; two inserts are visible at a time) are `const` assertions at the top.
        assert!(inserts_width() > INSERTS_DOCK_WIDTH, "the inserts scroll");
        let fader_group = TOGGLE_SQUARE.x + KNOB_GAP + SLIDER_WIDTH + KNOB_GAP;
        let meters = METER_OFFSET + METER_TRACK.x + GR_GAP + GR_WIDTH - SLIDER_WIDTH * 0.5;
        assert!(fader_group + meters.max(0.0) <= GRID_WIDTH);
        assert_eq!(SLOT_PILL_WIDTH + KNOB_GAP + SLOT_EDIT, GRID_WIDTH);
        // The default six sources overflow the four-strip dock; five do not.
        assert!(sources_width(DrumRow::COUNT + 1) > SOURCE_DOCK_WIDTH);
        assert!(sources_width(SOURCE_DOCK_STRIPS) <= SOURCE_DOCK_WIDTH);
    }

    /// The vertical budget at the design size: head, three pills, the grid and the route
    /// pill leave the fader at least a usable track in the 303 pt body.
    #[test]
    fn the_strip_blocks_leave_the_fader_a_track() {
        let body_height = 303.0;
        let above_fader = ROW_H
            + BLOCK_GAP
            + SLOT_COUNT as f32 * ROW_H
            + (SLOT_COUNT as f32 - 1.0) * KNOB_GAP
            + BLOCK_GAP
            + GRID_HEIGHT
            + BLOCK_GAP
            + ROW_H
            + BLOCK_GAP;
        let value_row = 16.0 + neu::slider::SLIDER_GAP;
        let track = body_height - above_fader - value_row;
        assert!(track >= 80.0, "fader track {track}");
        // The source strip stacks inside the same height.
        let source = TRACK_CHIP
            + BLOCK_GAP
            + KNOB_LABEL
            + 4.0
            + BLOCK_GAP
            + BOX_SIZE.y
            + BLOCK_GAP
            + 2.0 * KNOB
            + KNOB_GAP
            + BLOCK_GAP
            + 2.0 * TOGGLE_SQUARE.y
            + TOGGLE_GAP;
        assert!(source <= body_height, "source strip {source}");
    }

    #[test]
    fn route_choices_follow_the_engine_rule() {
        assert_eq!(source_route_choices().len(), 17);
        assert_eq!(source_route_choices()[0], MASTER);
        assert_eq!(insert_route_choices(16), vec![MASTER]);
        assert_eq!(insert_route_choices(14), vec![MASTER, 15, 16]);
        assert_eq!(insert_route_choices(1).len(), 16);
        assert!(!insert_route_choices(6).contains(&6));
        assert!(!insert_route_choices(6).contains(&5));
    }

    #[test]
    fn slot_formatters_follow_the_unit() {
        let comp = engine::inserts::COMPRESSOR_PARAMS;
        assert_eq!(slot_fmt(&comp[0])(-18.0), fmt_level_db(-18.0));
        assert_eq!(slot_fmt(&comp[1])(4.0), "4.0:1");
        assert_eq!(slot_fmt(&comp[2])(10.0), fmt_ms(0.01));
        assert_eq!(slot_fmt(&comp[5])(1.0), "100 %");
        let lofi = engine::inserts::LOFI_PARAMS;
        assert_eq!(slot_fmt(&lofi[0])(8.0), "8");
        assert_eq!(slot_fmt(&lofi[1])(2.0), "2x");
        assert_eq!(slot_fmt(&lofi[2])(10000.0), fmt_hz(10000.0));
    }
}
