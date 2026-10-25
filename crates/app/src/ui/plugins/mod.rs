//! The plugin windows' contents (plugins milestone): [`eq_window`] and [`reverb_window`]
//! render into any `Ui` from a [`PluginShared`], so `egui_kittest` can drive them as plain
//! panels; [`viewport_ui`] is the thin wrapper the deferred viewports call (background,
//! page padding, geometry tracking, close request, shortcut forwarding).

use egui::{CornerRadius, Frame, Margin, Response, Ui, ViewportClass, ViewportId};
use neu::Theme;

use crate::plugins::{PluginEdit, PluginKind, PluginShared};
use crate::shortcuts;

pub mod eq_graph;
pub mod eq_window;
pub mod reverb_window;

/// Page padding inside a plugin window (the panel's shadows need the room).
pub const PAGE_PAD: i8 = 12;
/// Width of the preset column in both windows.
pub const PRESET_COLUMN_WIDTH: f32 = 176.0;
/// Gap between stacked preset pills.
pub const PRESET_GAP: f32 = 6.0;
/// Gap between the graph / plot and the controls below it.
pub const SECTION_GAP: f32 = 12.0;
/// Gap between the left column and the preset column.
pub const COLUMN_GAP: f32 = 18.0;
/// Height of a knob block (label, ring, value) in the control rows.
pub const KNOB_BLOCK_HEIGHT: f32 = 84.0;

/// Everything a deferred plugin viewport draws. `class` tells an embedded fallback (no
/// native windows) from a real window; geometry is only recorded for real windows.
pub fn viewport_ui(ui: &mut Ui, shared: &mut PluginShared, kind: PluginKind, class: ViewportClass) {
    let (close, outer, inner) = ui.input(|i| {
        let v = i.viewport();
        (
            v.close_requested(),
            v.outer_rect.map(|r| r.min),
            v.inner_rect.map(|r| r.size()),
        )
    });
    if class != ViewportClass::EmbeddedWindow {
        shared.record_geometry(kind, outer, inner);
    }
    if close {
        shared.push_edit(PluginEdit::Close(kind));
    }
    // Space / Ctrl+Z / ... keep working while a plugin window has focus.
    let taken = ui.input_mut(shortcuts::take_shortcuts);
    shared.shortcuts.extend(taken);

    let surface = shared.theme.surface;
    ui.painter()
        .rect_filled(ui.max_rect(), CornerRadius::ZERO, surface);
    Frame::NONE
        .inner_margin(Margin::same(PAGE_PAD))
        .show(ui, |ui| match kind {
            PluginKind::Eq => eq_window::show(ui, shared),
            PluginKind::Reverb => reverb_window::show(ui, shared),
        });

    if !shared.edits.is_empty() || !shared.shortcuts.is_empty() {
        // The main window applies the edits: wake it even if it is idle.
        ui.ctx().request_repaint_of(ViewportId::ROOT);
    }
}

/// A scrollable column of preset pills (`neu::toggle_button` rows), `names` long, with
/// `selected` lit. Returns the index clicked this frame, if any.
pub fn preset_list(
    ui: &mut Ui,
    theme: &Theme,
    names: impl Iterator<Item = &'static str>,
    selected: Option<usize>,
    height: f32,
) -> Option<usize> {
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .id_salt("preset_list")
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = PRESET_GAP;
            // Room for the pills' raised shadows at the left edge.
            ui.add_space(2.0);
            for (i, name) in names.enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    if neu::toggle_button(ui, theme, name, selected == Some(i)).clicked() {
                        clicked = Some(i);
                    }
                });
            }
            ui.add_space(6.0);
        });
    clicked
}

/// Shows a knob bound to `value`, reporting the edit through `commit` when it moved.
pub fn knob_edit(
    ui: &mut Ui,
    theme: &Theme,
    value: f32,
    spec: &neu::KnobSpec<'_>,
    commit: impl FnOnce(f32, bool),
) -> Response {
    let mut v = value;
    let response = neu::knob(ui, theme, &mut v, spec);
    if response.changed() || v != value {
        commit(v, response.drag_started());
    }
    response
}

/// `0.71` -> `"0.71"` (Q values).
pub fn fmt_q(v: f32) -> String {
    format!("{v:.2}")
}

/// `12.0` -> `"12 ms"` (pre-delay).
pub fn fmt_ms_plain(v: f32) -> String {
    format!("{} ms", v.round() as i32)
}

/// `1.84` -> `"1.8 s"`, `0.4` -> `"400 ms"` (RT60).
pub fn fmt_seconds(v: f32) -> String {
    if v >= 1.0 {
        format!("{v:.1} s")
    } else {
        format!("{} ms", (v * 1000.0).round() as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatters() {
        assert_eq!(fmt_q(0.7066), "0.71");
        assert_eq!(fmt_ms_plain(12.4), "12 ms");
        assert_eq!(fmt_seconds(1.84), "1.8 s");
        assert_eq!(fmt_seconds(0.4), "400 ms");
    }
}
