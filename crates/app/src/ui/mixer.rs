//! MIXER panel (M6): six channel strips and a master column in the FX rack's slot.
//!
//! The panel shares the FX rack's head (`controls::right_panel_head`: the title and the
//! `FX | Mixer` tab) and its [`WIDTH`]; the body is laid out by hand from a fixed origin so
//! every strip has the same geometry and the whole thing fits the top row (the SYNTH panel's
//! ~370 pt) without scrolling at the 1280 x 1140 design size:
//!
//! - six strips of [`STRIP_WIDTH`] (64) pt, no gap: a stack of three unlabelled `Sm` knobs
//!   (PAN bipolar, then the DELAY and REVERB sends) centred over the fader column, then the
//!   [`neu::fader`] (level in dB, value above, the channel's caps name below) in its 36 pt
//!   column with the 6x140 [`neu::meter`] painted in the column's right margin beside the
//!   track, and the 28x28 M / S [`neu::toggle_button_sized`] toggles stacked in the 28 pt
//!   column to the left of the fader, top-aligned with the track;
//! - a [`MASTER_WIDTH`] (52) pt master column: the PAN / DLY / REV legends aligned with the
//!   knob rows (the strips' knobs are unlabelled, their tooltips show the value), then the
//!   MASTER gain knob (`Sm`, `MasterGain` in dB), the LIMIT toggle (`LimiterOn`) and the
//!   [`neu::stereo_meter`] down to the faders' bottom.
//!
//! The knobs give the strip their accessible names ("Kick pan", "Kick delay send", "Kick
//! reverb send", "Kick level", "Mute Kick", "Solo Kick") so tests and screen readers can
//! address them. Every value goes through [`AppState::edit_mixer`] (clamp, mirror,
//! `Message::SetMixer`, undo: one step per knob drag, one per M / S click); the meters read
//! [`AppState::shown_meters`], the engine's block peaks decayed by `AppState::update_meters`.

use egui::{
    Align, InnerResponse, Layout, Pos2, Rect, Response, Role, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use engine::mixer::{MAX_LEVEL_DB, MIN_LEVEL_DB};
use engine::{Channel, ChannelParam, ParamId};
use neu::slider::SliderSpec;
use neu::{KnobSize, KnobSpec, METER_TRACK, SLIDER_WIDTH, STEREO_METER_SIZE, TOGGLE_SQUARE, Theme};

use crate::app::AppState;
use crate::ui::controls::{self, Knob, fmt_db, fmt_level_db, fmt_pan, fmt_percent};

/// Width of the panel including its padding: the FX rack's slot.
pub const WIDTH: f32 = super::fx_rack::WIDTH;
/// Width of one channel strip: the M / S column ([`TOGGLE_SQUARE`]`.x`) plus the fader column
/// ([`SLIDER_WIDTH`]).
pub const STRIP_WIDTH: f32 = TOGGLE_SQUARE.x + SLIDER_WIDTH;
/// Width of the master column (the rest of the panel's 436 pt content width).
pub const MASTER_WIDTH: f32 = 52.0;
/// Size of a strip knob (`KnobSize::Sm` ring).
pub const KNOB: f32 = 34.0;
/// Vertical gap between the stacked knobs.
pub const KNOB_GAP: f32 = 2.0;
/// Gap between the knob stack and the fader block.
pub const FADER_GAP: f32 = 2.0;
/// Gap between the M and S toggles (and around the master's LIMIT toggle).
pub const TOGGLE_GAP: f32 = 4.0;
/// Gap between the head row and the strips (tighter than the FX rack's 14 to fit the row).
pub const HEAD_GAP: f32 = 6.0;
/// How far below the faders' value row the master block (knob, LIMIT, meter) starts, so its
/// "MASTER" label clears the synth fader's value text.
pub const MASTER_DROP: f32 = 16.0;
/// Distance from the fader track's centre line to the meter's left edge (half the 14 pt
/// thumb plus 2 pt, leaving 3 pt to the next strip's M / S column).
pub const METER_OFFSET: f32 = 9.0;
/// Caps labels under the faders, in [`Channel::ALL`] order.
pub const STRIP_LABELS: [&str; Channel::COUNT] =
    ["Kick", "Snare", "Clap", "C.Hat", "O.Hat", "Synth"];
/// Accessible channel names, in [`Channel::ALL`] order.
pub const CHANNEL_NAMES: [&str; Channel::COUNT] =
    ["Kick", "Snare", "Clap", "Closed hat", "Open hat", "Synth"];
/// Legends of the three knob rows (master column), top to bottom.
pub const KNOB_LEGENDS: [&str; 3] = ["Pan", "Dly", "Rev"];

/// One of a strip's stacked knobs: which value it edits, how it reads, and whether its arc
/// starts at 12 o'clock.
#[derive(Clone, Copy)]
struct StripKnob {
    param: ChannelParam,
    fmt: fn(f32) -> String,
    bipolar: bool,
}

/// The three knobs of a strip, top to bottom: PAN, then the DELAY and REVERB sends.
const STRIP_KNOBS: [StripKnob; 3] = [
    StripKnob {
        param: ChannelParam::Pan,
        fmt: fmt_pan,
        bipolar: true,
    },
    StripKnob {
        param: ChannelParam::DelaySend,
        fmt: fmt_percent,
        bipolar: false,
    },
    StripKnob {
        param: ChannelParam::ReverbSend,
        fmt: fmt_percent,
        bipolar: false,
    },
];

/// Where a strip's fader ended up: its whole block (value, track, label) and the track column.
#[derive(Debug, Clone, Copy)]
struct FaderRects {
    block: Rect,
    track: Rect,
}

/// Renders the MIXER panel into `ui`, filling the space it is given, and applies any control
/// change to `state`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        // The strips are placed by hand from the cursor: no implicit spacing after the head.
        ui.spacing_mut().item_spacing.y = 0.0;
        controls::right_panel_head(ui, state, theme, "Mixer");
        ui.add_space(HEAD_GAP);
        body(ui, state, theme);
    });
}

/// Accessible name of `channel` ("Closed hat").
#[must_use]
pub fn channel_name(channel: Channel) -> &'static str {
    CHANNEL_NAMES[channel.index()]
}

/// Accessible name of a strip knob or fader: `"Kick pan"`, `"Kick delay send"`,
/// `"Kick reverb send"`, `"Kick level"`.
#[must_use]
pub fn control_name(channel: Channel, param: ChannelParam) -> String {
    let what = match param {
        ChannelParam::Level => "level",
        ChannelParam::Pan => "pan",
        ChannelParam::Mute => "mute",
        ChannelParam::Solo => "solo",
        ChannelParam::DelaySend => "delay send",
        ChannelParam::ReverbSend => "reverb send",
    };
    format!("{} {what}", channel_name(channel))
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

fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let origin = ui.cursor().min;
    let strip_width = ((ui.available_width() - MASTER_WIDTH) / Channel::COUNT as f32)
        .floor()
        .max(STRIP_WIDTH);
    let mut last = None;
    for (i, channel) in Channel::ALL.iter().enumerate() {
        let x = origin.x + i as f32 * strip_width;
        last = Some(strip(
            ui,
            state,
            theme,
            *channel,
            pos2(x, origin.y),
            strip_width,
        ));
    }
    let master_x = origin.x + Channel::COUNT as f32 * strip_width;
    if let Some(fader) = last {
        master(ui, state, theme, pos2(master_x, origin.y), fader);
    }
}

/// One channel strip at `origin`, `width` wide: the knob stack, the fader with its meter and
/// the M / S toggles. Returns the fader's rects so the master column can line up with them.
fn strip(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    channel: Channel,
    origin: Pos2,
    width: f32,
) -> FaderRects {
    let fader_x = origin.x + width - SLIDER_WIDTH;
    let knob_x = fader_x + (SLIDER_WIDTH - KNOB) * 0.5;
    let mut y = origin.y;
    for knob in STRIP_KNOBS {
        let rect = Rect::from_min_size(pos2(knob_x, y), vec2(KNOB, KNOB));
        strip_knob(ui, state, theme, channel, knob, rect);
        y += KNOB + KNOB_GAP;
    }
    y += FADER_GAP - KNOB_GAP;

    let fader = fader(ui, state, theme, channel, pos2(fader_x, y));

    // The meter sits in the fader column's right margin, beside the thumb.
    let level = state.shown_meters.channel[channel.index()];
    let meter = Rect::from_min_size(
        pos2(fader.track.center().x + METER_OFFSET, fader.track.min.y),
        METER_TRACK,
    );
    place(ui, meter, |ui| neu::meter(ui, theme, level, METER_TRACK));

    // M / S in the left column, top-aligned with the track.
    let mute_rect = Rect::from_min_size(pos2(origin.x, fader.track.min.y), TOGGLE_SQUARE);
    let solo_rect = mute_rect.translate(vec2(0.0, TOGGLE_SQUARE.y + TOGGLE_GAP));
    let name = channel_name(channel);
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

    fader
}

/// An unlabelled `Sm` knob for one strip value at `rect` (its tooltip shows the value, the
/// master column's legend names the row); accessible as [`control_name`].
fn strip_knob(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    channel: Channel,
    knob: StripKnob,
    rect: Rect,
) -> Response {
    let StripKnob {
        param,
        fmt,
        bipolar,
    } = knob;
    let range = param.range();
    let spec = KnobSpec {
        label: "",
        min: *range.start(),
        max: *range.end(),
        default: param.default_value(channel),
        log: false,
        bipolar,
        size: KnobSize::Sm,
        fmt,
    };
    let before = state.mixer.get(channel, param);
    let mut value = before;
    let response = place(ui, rect, |ui| neu::knob(ui, theme, &mut value, &spec)).inner;
    if response.changed() || value != before {
        state.edit_mixer(channel, param, value, response.drag_started());
    }
    let shown = state.mixer.get(channel, param);
    let name = control_name(channel, param);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(shown), &name));
    response
}

/// The level fader of `channel` with its top-left at `origin`: -60..=+6 dB, default 0, the
/// channel's caps name as its label; accessible as `"<channel> level"`.
fn fader(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    channel: Channel,
    origin: Pos2,
) -> FaderRects {
    let spec = SliderSpec {
        label: STRIP_LABELS[channel.index()],
        min: MIN_LEVEL_DB,
        max: MAX_LEVEL_DB,
        default: ChannelParam::Level.default_value(channel),
        fmt: fmt_level_db,
    };
    let before = state.mixer.get(channel, ChannelParam::Level);
    let mut value = before;
    let rect = Rect::from_min_max(origin, pos2(origin.x + SLIDER_WIDTH, ui.max_rect().max.y));
    let inner = place(ui, rect, |ui| neu::fader(ui, theme, &mut value, &spec));
    let response = inner.inner;
    if response.changed() || value != before {
        state.edit_mixer(channel, ChannelParam::Level, value, response.drag_started());
    }
    let shown = state.mixer.get(channel, ChannelParam::Level);
    let name = control_name(channel, ChannelParam::Level);
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(shown), &name));
    FaderRects {
        block: inner.response.rect,
        track: response.rect,
    }
}

/// The master column at `origin`, [`MASTER_WIDTH`] wide: the knob-row legends, the MASTER
/// gain knob level with the faders' value row, the LIMIT toggle and the stereo meter down to
/// the faders' bottom edge.
fn master(ui: &mut Ui, state: &mut AppState, theme: &Theme, origin: Pos2, fader: FaderRects) {
    for (i, legend) in KNOB_LEGENDS.iter().enumerate() {
        let y = origin.y + i as f32 * (KNOB + KNOB_GAP);
        let rect = Rect::from_min_size(pos2(origin.x, y), vec2(MASTER_WIDTH, KNOB));
        place(ui, rect, |ui| {
            ui.allocate_ui_with_layout(
                vec2(MASTER_WIDTH, KNOB),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add_space(theme.space_1);
                    neu::label(ui, theme, legend);
                },
            );
        });
    }

    let knob_rect = Rect::from_min_size(
        pos2(
            origin.x + (MASTER_WIDTH - KNOB) * 0.5,
            fader.block.min.y + MASTER_DROP,
        ),
        vec2(KNOB, KNOB * 2.0),
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
        assert_eq!(control_name(Channel::Kick, ChannelParam::Pan), "Kick pan");
        assert_eq!(
            control_name(Channel::ClosedHat, ChannelParam::DelaySend),
            "Closed hat delay send"
        );
        assert_eq!(
            control_name(Channel::Synth, ChannelParam::Level),
            "Synth level"
        );
        assert_eq!(channel_name(Channel::OpenHat), "Open hat");
        assert_eq!(STRIP_LABELS[Channel::ClosedHat.index()], "C.Hat");
    }

    #[test]
    fn six_strips_and_the_master_fill_the_panel_width() {
        let content = WIDTH - 2.0 * 20.0;
        assert_eq!(STRIP_WIDTH, 64.0);
        assert_eq!(Channel::COUNT as f32 * STRIP_WIDTH + MASTER_WIDTH, content);
    }
}
