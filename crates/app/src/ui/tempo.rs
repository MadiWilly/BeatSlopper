//! The transport's BPM group (QOL pass): caps label, `(-)` button, the tempo in the mock's
//! 22/300 display type (the token sheet's "BPM display"), `(+)` button, and the TAP pill
//! centred under that row (beside it, the pill left the mini visualizer strip no room).
//!
//! The tempo reads as a whole number or with one decimal ([`crate::tempo::fmt_bpm`]). A click
//! on it opens the kit's inline editor (`neu::readout`: type `140`, `128.5` or `128 bpm`,
//! Enter commits, Escape cancels; clamped to 60..=200), the buttons and the wheel over the
//! group nudge by one BPM, and TAP sets the tempo from the mean of the last four taps
//! ([`AppState::tap_tempo_at`]), lighting up briefly after each tap. The value is accessible as
//! the button [`BPM_VALUE_NAME`].

use std::time::Duration;

use egui::{Align, Align2, Layout, Sense, Ui, vec2};
use neu::stepper::STEPPER_GAP;
use neu::{Icon, TextKind, Theme, stepper_button};

use crate::app::{AppState, MAX_BPM, MIN_BPM};
use crate::shortcuts::Shortcut;
use crate::tempo::{TAP_FLASH, fmt_bpm};

/// Accessible name of the tempo readout.
pub const BPM_VALUE_NAME: &str = "BPM value";
/// Label of the tap-tempo pill.
pub const TAP_LABEL: &str = "Tap";
/// Narrowest tempo readout (`"128.5"` in the display type is about 52 wide).
pub const BPM_VALUE_MIN_WIDTH: f32 = 56.0;
/// Vertical gap between the caps label and the button row (matches `neu::stepper`).
const LABEL_GAP: f32 = 6.0;

/// Renders the BPM group and applies any tempo change to `state`.
pub fn bpm_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let group = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = LABEL_GAP;
        neu::label(ui, theme, "BPM");
        let row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = STEPPER_GAP;
            if stepper_button(ui, theme, Icon::Minus)
                .on_hover_text(Shortcut::BpmDown.label())
                .clicked()
            {
                state.nudge_bpm(-1);
            }
            bpm_value(ui, state, theme);
            if stepper_button(ui, theme, Icon::Plus)
                .on_hover_text(Shortcut::BpmUp.label())
                .clicked()
            {
                state.nudge_bpm(1);
            }
        });
        // The TAP pill sits under the stepper row, centred on it: the bar's row is as tall as
        // the MASTER dial, so the group has the vertical room, while beside the stepper the
        // pill left the mini visualizer strip too little width at the design width.
        let row_width = row.response.rect.width();
        ui.allocate_ui_with_layout(
            vec2(row_width, neu::button::PILL_HEIGHT),
            Layout::top_down(Align::Center),
            |ui| {
                let now = ui.input(|i| i.time);
                let flashing = state.tap.is_flashing(now);
                if neu::toggle_button(ui, theme, TAP_LABEL, flashing)
                    .on_hover_text("Tap tempo: tap along; the last four taps are averaged")
                    .clicked()
                {
                    state.tap_tempo_at(now);
                    ui.ctx()
                        .request_repaint_after(Duration::from_secs_f64(TAP_FLASH));
                } else if flashing {
                    ui.ctx()
                        .request_repaint_after(Duration::from_secs_f64(TAP_FLASH));
                }
            },
        );
    });
    let steps = neu::wheel_steps(ui, &group.response);
    if steps != 0 {
        state.nudge_bpm(steps);
    }
}

/// The tempo readout in the display type: a click opens the inline editor.
fn bpm_value(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let text = fmt_bpm(state.bpm);
    let font = TextKind::Display.font_id();
    let galley = ui
        .painter()
        .layout_no_wrap(text.clone(), font.clone(), theme.text_primary);
    let size = vec2(galley.size().x.max(BPM_VALUE_MIN_WIDTH), galley.size().y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let typed = neu::readout(
        ui,
        theme,
        response.id.with("readout"),
        rect.center(),
        Align2::CENTER_CENTER,
        &text,
        font,
        theme.text_primary,
        BPM_VALUE_NAME,
    );
    if let Some(typed) = typed
        && let Some(bpm) = neu::parse_value(&typed, MIN_BPM, MAX_BPM, false, fmt_bpm)
    {
        state.set_bpm(bpm);
    }
}
