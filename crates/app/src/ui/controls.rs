//! Parameter-bound controls shared by the SYNTH, FX and transport panels, plus the value
//! formatters and the small layout helpers the mock's groups need.
//!
//! Every control here reads `state.params[id]`, hands a copy to the `neu` widget and, when the
//! widget moved it, writes it back through [`AppState::edit_param`] (which clamps, stores,
//! sends `Message::SetParam` and records the undo step: one per drag for knobs, dials and
//! sliders, one per click for the segmented and toggle controls). Nothing else in the UI
//! touches `params` directly.

use egui::{Response, Ui};
use engine::ParamId;
use neu::slider::SliderSpec;
use neu::{DialSize, KnobSize, KnobSpec, Theme};

use crate::app::{AppState, RightPanel, RightTab};

/// Height of a group head row (`.ghead`, `.col > .lbl`): the segmented control's height.
pub const ROW_HEIGHT: f32 = 30.0;
/// Gap between a group's caps label and the control beside it (`.ghead{gap:14px}`).
pub const HEAD_GAP: f32 = 14.0;
/// Gap between knobs in a row (`.knobs{gap:10px}`, `.fx-ctls{gap:10px}`).
pub const KNOB_GAP: f32 = 10.0;
/// Gap between a group's head and its controls (`.group`, `.col`, `.fx-sec{gap:10px}`).
pub const GROUP_GAP: f32 = 10.0;
/// Gap between a panel's head row and its body (`.panel-head{margin-bottom:14px}`).
pub const PANEL_HEAD_GAP: f32 = 14.0;
/// Width of a medium knob block (`design/WINNER.md`: block width 52).
pub const KNOB_BLOCK: f32 = 52.0;

// ---------------------------------------------------------------------------------------------
// Value formatters (plain `fn`s so they fit `KnobSpec::fmt`).

/// `0.7` -> `"70 %"`.
pub fn fmt_percent(v: f32) -> String {
    format!("{} %", (v * 100.0).round() as i32)
}

/// `0.4` -> `"+40 %"`, `-0.4` -> `"-40 %"`, `0.0` -> `"0 %"` (bipolar amounts).
pub fn fmt_percent_signed(v: f32) -> String {
    let pct = (v * 100.0).round() as i32;
    if pct == 0 {
        "0 %".to_owned()
    } else {
        format!("{pct:+} %")
    }
}

/// Detune in cents: `7.0` -> `"+7 c"`, `0.0` -> `"0 c"`.
pub fn fmt_cents(v: f32) -> String {
    let cents = v.round() as i32;
    if cents == 0 {
        "0 c".to_owned()
    } else {
        format!("{cents:+} c")
    }
}

/// Frequency: `2400.0` -> `"2.4 kHz"`, `240.0` -> `"240 Hz"`, `55.5` -> `"55.5 Hz"`.
pub fn fmt_hz(v: f32) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else if v >= 100.0 {
        format!("{v:.0} Hz")
    } else {
        format!("{v:.1} Hz")
    }
}

/// LFO rate: `4.5` -> `"4.5 Hz"`.
pub fn fmt_rate_hz(v: f32) -> String {
    format!("{v:.1} Hz")
}

/// Time in seconds shown as milliseconds: `0.18` -> `"180 ms"`, `1.5` -> `"1.50 s"`.
pub fn fmt_ms(seconds: f32) -> String {
    if seconds >= 1.0 {
        format!("{seconds:.2} s")
    } else {
        format!("{:.0} ms", seconds * 1000.0)
    }
}

/// Linear gain as decibels: `1.0` -> `"0.0 dB"`, `2.0` -> `"+6.0 dB"`, `0.0` -> `"-inf dB"`.
pub fn fmt_db(gain: f32) -> String {
    if gain <= 0.0 {
        return "-inf dB".to_owned();
    }
    fmt_level_db(20.0 * gain.log10())
}

/// A level already in decibels (the mixer faders): `-6.0` -> `"-6.0 dB"`, `0.0` ->
/// `"0.0 dB"`, `3.0` -> `"+3.0 dB"`.
pub fn fmt_level_db(db: f32) -> String {
    if db.abs() < 0.05 {
        "0.0 dB".to_owned()
    } else {
        format!("{db:+.1} dB")
    }
}

/// FM ratio: `2.0` -> `"2.0x"`, `0.5` -> `"0.5x"`.
pub fn fmt_ratio(v: f32) -> String {
    format!("{v:.1}x")
}

/// Bit depth: `12.0` -> `"12 bit"`, `15.6` -> `"16 bit"`.
pub fn fmt_bits(v: f32) -> String {
    format!("{} bit", v.round() as i32)
}

/// Sample-rate divisor: `8.0` -> `"x8"`, `1.0` -> `"x1"`.
pub fn fmt_rate_x(v: f32) -> String {
    format!("x{}", v.round() as i32)
}

/// Pitch offset in semitones: `12.0` -> `"+12 st"`, `-5.0` -> `"-5 st"`, `0.0` -> `"0 st"`.
pub fn fmt_semitones(v: f32) -> String {
    let st = v.round() as i32;
    if st == 0 {
        "0 st".to_owned()
    } else {
        format!("{st:+} st")
    }
}

/// Pan position `-1..=1` (the mixer PAN knobs): `-0.3` -> `"L 30"`, `0.0` -> `"C"`, `0.3` ->
/// `"R 30"`.
pub fn fmt_pan(pan: f32) -> String {
    let pct = (pan * 100.0).round() as i32;
    match pct {
        0 => "C".to_owned(),
        p if p < 0 => format!("L {}", -p),
        p => format!("R {p}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Bound controls.

/// A knob (or dial) bound to one parameter. Built with [`Knob::new`] and the chainable
/// modifiers, shown with [`Knob::show`] or [`Knob::show_dial`].
#[derive(Clone, Copy)]
pub struct Knob {
    pub id: ParamId,
    /// Caps label; `None` uses [`ParamId::label`].
    pub label: Option<&'static str>,
    pub log: bool,
    pub bipolar: bool,
    pub size: KnobSize,
    pub fmt: fn(f32) -> String,
    /// Shown dimmed (still interactive) with this reason in the tooltip, when the parameter
    /// has no effect in the current setting (e.g. PULSE with a non-square wave).
    pub dimmed: Option<&'static str>,
    /// A second tooltip line explaining what the value is (e.g. "Return level of the reverb
    /// bus"), see [`neu::knob_noted`].
    pub note: Option<&'static str>,
}

impl Knob {
    /// A medium, linear, unipolar knob for `id` whose value is formatted by `fmt`.
    #[must_use]
    pub fn new(id: ParamId, fmt: fn(f32) -> String) -> Self {
        Self {
            id,
            label: None,
            log: false,
            bipolar: false,
            size: KnobSize::Md,
            fmt,
            dimmed: None,
            note: None,
        }
    }

    /// Dims the knob (see [`neu::knob_dimmed`]) while `condition` holds, with `reason` added
    /// to the tooltip.
    #[must_use]
    pub fn dimmed_when(mut self, condition: bool, reason: &'static str) -> Self {
        self.dimmed = condition.then_some(reason);
        self
    }

    /// Adds a second tooltip line (see [`neu::knob_noted`]).
    #[must_use]
    pub fn note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }

    /// Overrides the caps label.
    #[must_use]
    pub fn label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }

    /// Maps the drag in log space (frequencies).
    #[must_use]
    pub fn log(mut self) -> Self {
        self.log = true;
        self
    }

    /// Draws the value arc from 12 o'clock (detune, envelope amount).
    #[must_use]
    pub fn bipolar(mut self) -> Self {
        self.bipolar = true;
        self
    }

    /// The small knob size (cap 26, arc 34).
    #[must_use]
    pub fn small(mut self) -> Self {
        self.size = KnobSize::Sm;
        self
    }

    /// The `neu` spec for this knob: range and default come from the parameter.
    #[must_use]
    pub fn spec(&self) -> KnobSpec<'static> {
        let range = self.id.range();
        KnobSpec {
            label: self.label.unwrap_or(self.id.label()),
            min: *range.start(),
            max: *range.end(),
            default: self.id.default_value(),
            log: self.log,
            bipolar: self.bipolar,
            size: self.size,
            fmt: self.fmt,
        }
    }

    /// Shows the knob and writes any change back to `state`.
    pub fn show(self, ui: &mut Ui, state: &mut AppState, theme: &Theme) -> Response {
        let before = state.param(self.id);
        let mut value = before;
        let response = neu::knob_with(ui, theme, &mut value, &self.spec(), self.dimmed, self.note);
        commit(state, self.id, before, value, &response);
        response
    }

    /// Shows the concentric-ring dial for this parameter and writes any change back.
    pub fn show_dial(
        self,
        ui: &mut Ui,
        state: &mut AppState,
        theme: &Theme,
        size: DialSize,
    ) -> Response {
        let before = state.param(self.id);
        let mut value = before;
        let response = neu::dial(ui, theme, &mut value, &self.spec(), size);
        commit(state, self.id, before, value, &response);
        response
    }
}

/// Stores `after` when the widget reported a change or the value moved (double-click resets
/// write the value without necessarily marking the response changed). `drag_started` tells
/// the undo history that a new drag begins even if the previous one ended a moment ago.
fn commit(state: &mut AppState, id: ParamId, before: f32, after: f32, response: &Response) {
    if response.changed() || after != before {
        state.edit_param(id, after, response.drag_started());
    }
}

/// A segmented selector bound to a discrete parameter (`items[i]` is value `i`).
pub fn segmented(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    id: ParamId,
    items: &[&str],
) -> Response {
    let last = items.len().saturating_sub(1);
    let before = (state.param(id).round().max(0.0) as usize).min(last);
    let mut selected = before;
    let response = neu::segmented::segmented(ui, theme, &mut selected, items);
    // Compare values rather than trusting `changed()`: the selection only moved if it differs.
    if selected != before {
        // A discrete choice is always its own undo step.
        state.edit_param(id, selected as f32, true);
    }
    response
}

/// A thin vertical slider bound to a continuous parameter.
pub fn vslider(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    id: ParamId,
    fmt: fn(f32) -> String,
) -> Response {
    let range = id.range();
    let spec = SliderSpec {
        label: id.label(),
        min: *range.start(),
        max: *range.end(),
        default: id.default_value(),
        fmt,
    };
    let before = state.param(id);
    let mut value = before;
    let response = neu::slider::vslider(ui, theme, &mut value, &spec);
    commit(state, id, before, value, &response);
    response
}

/// A pill toggle button bound to a 0/1 parameter; a click flips it.
pub fn toggle(
    ui: &mut Ui,
    state: &mut AppState,
    theme: &Theme,
    id: ParamId,
    label: &str,
) -> Response {
    let on = state.param(id) >= 0.5;
    let response = neu::toggle_button(ui, theme, label, on);
    if response.clicked() {
        state.edit_param(id, if on { 0.0 } else { 1.0 }, true);
    }
    response
}

/// Whether shift is held for a click this frame: the raw modifier state, or the modifiers
/// carried by this frame's pointer-button events (synthetic clicks, e.g. under `egui_kittest`,
/// deliver shift only on the event).
pub fn click_shift(ui: &Ui) -> bool {
    ui.input(|i| {
        i.modifiers.shift
            || i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::PointerButton { modifiers, .. } if modifiers.shift
                )
            })
    })
}

// ---------------------------------------------------------------------------------------------
// Layout helpers.

/// A caps label in a [`ROW_HEIGHT`]-tall row (the mock's `.col > .lbl{height:30px}`), so a
/// column whose head is a plain label lines up with one whose head holds a segmented control.
pub fn group_label(ui: &mut Ui, theme: &Theme, text: &str) -> Response {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        neu::label(ui, theme, text)
    })
    .inner
}

/// A group head row: caps label, [`HEAD_GAP`], then `right` (normally a segmented control),
/// all vertically centred on a [`ROW_HEIGHT`] row.
pub fn group_head<R>(
    ui: &mut Ui,
    theme: &Theme,
    text: &str,
    right: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.spacing_mut().item_spacing.x = HEAD_GAP;
        neu::label(ui, theme, text);
        right(ui)
    })
    .inner
}

/// Items of the right-hand column's tab selector, in [`RightTab`] order. The RECENT panel
/// (QOL) is a sub-view of PROJECT, reached from its clock button and left by its BACK pill,
/// so it has no item of its own and selects the `Proj` tab while shown. The labels are short
/// (M10): the four full words (`Project | Chain | Visual | Tracks`, about 236 pt) overflow
/// the column's 220 pt inner width; these fit with room to spare.
pub const RIGHT_TABS: [&str; 4] = ["Proj", "Chain", "Vis", "Tracks"];

/// The head row shared by the PROJECT, SONG CHAIN, VISUALIZER and TRACKS panels: the
/// `Proj | Chain | Vis | Tracks` tab (`neu::segmented`), whose selected item doubles as the
/// panel title. A click switches [`AppState::right_tab`].
pub fn panel_head(ui: &mut Ui, state: &mut AppState, theme: &Theme, title: &str) {
    // With three tabs the segmented (about 176 pt) and a title no longer fit side by side in
    // the column's 220 pt inner width, and a second row would push the PROJECT panel's status
    // line into the KEYS panel. The selected tab is set in the title style (12/500 caps,
    // text-primary), so it *is* the panel's title; `title` names the panel for the tooltip.
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        let before = match state.right_tab {
            RightTab::Project | RightTab::Recent => 0,
            RightTab::Chain => 1,
            RightTab::Visual => 2,
            RightTab::Tracks => 3,
        };
        let mut selected = before;
        neu::segmented(ui, theme, &mut selected, &RIGHT_TABS).on_hover_text(format!(
            "{title}. Proj: files and export. Chain: the song. Vis: the visualizer. \
             Tracks: the instrument tracks."
        ));
        if selected != before {
            state.right_tab = match selected {
                0 => RightTab::Project,
                1 => RightTab::Chain,
                2 => RightTab::Visual,
                _ => RightTab::Tracks,
            };
        }
    });
}

/// Items of the top row's right-slot tab, in [`RightPanel`] order.
pub const RIGHT_PANEL_TABS: [&str; 2] = ["FX", "Mixer"];

/// The head row shared by the FX rack and the MIXER panel (M6): the title on the left, the
/// PLUGINS pills (`EQ` / `Reverb`, lit while their pop-out window is open; plugins
/// milestone) and the `FX | Mixer` tab (`neu::segmented`) flush right. A click on the tab
/// switches [`AppState::right_panel`]; a pill toggles [`AppState::plugin_open`].
pub fn right_panel_head(ui: &mut Ui, state: &mut AppState, theme: &Theme, title: &str) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        neu::title(ui, theme, title);
        right_aligned(ui, "right_panel_tabs", |ui| {
            ui.spacing_mut().item_spacing.x = KNOB_GAP;
            neu::label(ui, theme, "Plugins");
            for kind in crate::plugins::PluginKind::ALL {
                let open = state.is_plugin_open(kind);
                if neu::toggle_button(ui, theme, kind.pill_label(), open)
                    .on_hover_text(format!(
                        "{} the {} window",
                        if open { "Close" } else { "Open" },
                        kind.title()
                    ))
                    .clicked()
                {
                    state.toggle_plugin(kind);
                }
            }
            ui.add_space(HEAD_GAP - KNOB_GAP);
            let before = match state.right_panel {
                RightPanel::Fx => 0,
                RightPanel::Mixer => 1,
            };
            let mut selected = before;
            neu::segmented(ui, theme, &mut selected, &RIGHT_PANEL_TABS)
                .on_hover_text("FX: the effect parameters. Mixer: levels, pans, sends, meters.");
            if selected != before {
                state.right_panel = if selected == 0 {
                    RightPanel::Fx
                } else {
                    RightPanel::Mixer
                };
            }
        });
    });
}

/// Diameter of a track colour chip (M10).
pub const TRACK_CHIP: f32 = 10.0;
/// Gap between a track chip and the name beside it.
pub const TRACK_CHIP_GAP: f32 = 6.0;

/// Accessible name of a track's colour chip: `"Synth colour"`.
#[must_use]
pub fn track_chip_name(name: &str) -> String {
    format!("{name} colour")
}

/// A [`TRACK_CHIP`]-wide filled circle in `theme.track_colour(colour)` (M10), vertically
/// centred on the current row; accessible as [`track_chip_name`], its tooltip the track
/// name.
pub fn track_chip(ui: &mut Ui, theme: &Theme, colour: u8, name: &str) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(TRACK_CHIP, TRACK_CHIP), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .circle_filled(rect.center(), TRACK_CHIP * 0.5, theme.track_colour(colour));
    }
    let label = track_chip_name(name);
    let response = response.on_hover_text(name.to_owned());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), &label));
    response
}

/// The edit track's tag (M10): its colour chip and its name as a caps label, with a tooltip
/// saying that the group beside it edits that track. Used by the FX rack's per-track groups
/// and the SEQUENCER head.
pub fn track_tag(ui: &mut Ui, state: &AppState, theme: &Theme) -> Response {
    let track = state.edit_track_ui();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = TRACK_CHIP_GAP;
        track_chip(ui, theme, track.colour, &track.name);
        neu::label(ui, theme, &track.name).on_hover_text(format!(
            "Edits track {} (Alt+Up / Alt+Down select another track)",
            track.name
        ))
    })
    .inner
}

/// Pushes `add`'s contents to the right edge of a horizontal `ui`, using the width they had
/// last frame (egui's right-to-left layouts put nested groups at the far left, so measuring
/// is the reliable way). The first frame lays them out flush left; the next one is right.
pub fn right_aligned<R>(ui: &mut Ui, id_salt: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let width_id = ui.scope_id().with(id_salt);
    let known_width: Option<f32> = ui.data(|d| d.get_temp(width_id));
    if let Some(width) = known_width {
        let pad = ui.available_width() - width;
        if pad > 0.0 {
            ui.add_space(pad);
        }
    }
    let scope = ui.scope(add);
    ui.data_mut(|d| d.insert_temp(width_id, scope.response.rect.width()));
    scope.inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatters_match_the_mock() {
        assert_eq!(fmt_percent(0.7), "70 %");
        assert_eq!(fmt_percent(0.0), "0 %");
        assert_eq!(fmt_percent_signed(0.4), "+40 %");
        assert_eq!(fmt_percent_signed(-0.4), "-40 %");
        assert_eq!(fmt_percent_signed(0.0), "0 %");
        assert_eq!(fmt_cents(7.0), "+7 c");
        assert_eq!(fmt_cents(-12.0), "-12 c");
        assert_eq!(fmt_cents(0.0), "0 c");
        assert_eq!(fmt_hz(2400.0), "2.4 kHz");
        assert_eq!(fmt_hz(240.0), "240 Hz");
        assert_eq!(fmt_hz(55.5), "55.5 Hz");
        assert_eq!(fmt_rate_hz(4.5), "4.5 Hz");
        assert_eq!(fmt_ms(0.18), "180 ms");
        assert_eq!(fmt_ms(0.012), "12 ms");
        assert_eq!(fmt_ms(1.5), "1.50 s");
        assert_eq!(fmt_db(1.0), "0.0 dB");
        assert_eq!(fmt_db(2.0), "+6.0 dB");
        assert_eq!(fmt_db(0.5), "-6.0 dB");
        assert_eq!(fmt_db(0.0), "-inf dB");
        assert_eq!(fmt_level_db(-6.0), "-6.0 dB");
        assert_eq!(fmt_level_db(0.0), "0.0 dB");
        assert_eq!(fmt_level_db(-0.04), "0.0 dB");
        assert_eq!(fmt_level_db(3.0), "+3.0 dB");
        assert_eq!(fmt_level_db(-60.0), "-60.0 dB");
        assert_eq!(fmt_pan(-0.3), "L 30");
        assert_eq!(fmt_pan(0.0), "C");
        assert_eq!(fmt_pan(0.004), "C");
        assert_eq!(fmt_pan(0.3), "R 30");
        assert_eq!(fmt_pan(1.0), "R 100");
    }

    #[test]
    fn m8_formatters() {
        assert_eq!(fmt_ratio(2.0), "2.0x");
        assert_eq!(fmt_ratio(0.5), "0.5x");
        assert_eq!(fmt_ratio(1.25), "1.2x");
        assert_eq!(fmt_bits(12.0), "12 bit");
        assert_eq!(fmt_bits(15.6), "16 bit");
        assert_eq!(fmt_bits(4.0), "4 bit");
        assert_eq!(fmt_rate_x(8.0), "x8");
        assert_eq!(fmt_rate_x(1.0), "x1");
        assert_eq!(fmt_rate_x(2.4), "x2");
        assert_eq!(fmt_semitones(12.0), "+12 st");
        assert_eq!(fmt_semitones(-5.0), "-5 st");
        assert_eq!(fmt_semitones(0.0), "0 st");
        assert_eq!(fmt_semitones(0.3), "0 st");
    }

    #[test]
    fn knob_spec_takes_range_and_default_from_the_param() {
        let knob = Knob::new(ParamId::FilterCutoff, fmt_hz).log().label("Cut");
        let spec = knob.spec();
        assert_eq!(spec.label, "Cut");
        assert_eq!(spec.min, 20.0);
        assert_eq!(spec.max, 20_000.0);
        assert_eq!(spec.default, 2400.0);
        assert!(spec.log && !spec.bipolar);
        assert_eq!(Knob::new(ParamId::Osc1Mix, fmt_percent).spec().label, "Mix");
        assert_eq!(
            Knob::new(ParamId::Osc1Detune, fmt_cents)
                .bipolar()
                .small()
                .size,
            KnobSize::Sm
        );
        let pulse = Knob::new(ParamId::Osc1Pulse, fmt_percent);
        assert_eq!(pulse.dimmed_when(false, "Square wave only").dimmed, None);
        assert_eq!(
            pulse.dimmed_when(true, "Square wave only").dimmed,
            Some("Square wave only")
        );
    }
}
