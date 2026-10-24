//! Segmented selector (M2 stub: the neu subagent implements the real painting; keep the
//! signature).

use egui::{Response, Ui};

use crate::Theme;

/// An inset pill track with one plain-text item per option; the selected item is bold.
/// `changed()` when the selection moved.
pub fn segmented(ui: &mut Ui, _theme: &Theme, selected: &mut usize, items: &[&str]) -> Response {
    let mut response = ui
        .horizontal(|ui| {
            for (i, item) in items.iter().enumerate() {
                if ui.selectable_label(*selected == i, *item).clicked() {
                    *selected = i;
                }
            }
        })
        .response;
    if items.is_empty() {
        return response;
    }
    *selected = (*selected).min(items.len() - 1);
    response.mark_changed();
    response
}
