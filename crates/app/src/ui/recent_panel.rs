//! RECENT panel (QOL pass): the last eight project files, shown in the right-hand column in
//! place of the PROJECT panel (its RECENT pill opens this view, the BACK pill here returns;
//! the `Project | Chain` tab did not leave room for a third item beside the panel title).
//!
//! One row per entry, most recent first: the file name (the full path is the tooltip), an
//! accent dot on the open project. A click opens the entry ([`AppState::request_open_recent`]:
//! with unsaved changes the DISCARD / KEEP confirm appears above the list first). Below the
//! list a CLEAR LIST pill and the `Ctrl+Shift+O` hint (the shortcut cycles the list). Rows
//! are buttons named [`entry_name`] (`"Recent 1: beat.beatslopper.json"`).

use egui::{Align2, RichText, Sense, Ui, vec2};
use neu::{TextKind, Theme};

use crate::app::{AppState, PendingAction, RightTab};
use crate::project::file_label;
use crate::shortcuts::Shortcut;
use crate::ui::controls::{self, GROUP_GAP, PANEL_HEAD_GAP};
use crate::ui::project_bar::DISCARD_QUESTION;

/// Label of the pill that returns to the PROJECT panel.
pub const BACK_LABEL: &str = "Back";

/// Height of one entry row.
pub const ROW_HEIGHT: f32 = 24.0;
/// Diameter of the dot marking the open project.
const CURRENT_DOT: f32 = 6.0;
/// Gap between the dot and the file name.
const DOT_GAP: f32 = 8.0;
/// Gap between pills in a row.
const PILL_GAP: f32 = 8.0;
/// Shown while the list is empty.
pub const EMPTY_LABEL: &str = "No recent projects yet";
/// Label of the pill that forgets the list.
pub const CLEAR_LIST_LABEL: &str = "Clear list";

/// Accessible name of entry `index` (0-based) for `path`: `"Recent 1: beat.beatslopper.json"`.
#[must_use]
pub fn entry_name(index: usize, path: &std::path::Path) -> String {
    format!("Recent {}: {}", index + 1, file_label(path))
}

/// Renders the RECENT panel into `ui`, filling the space it is given.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        ui.spacing_mut().item_spacing.y = 0.0;
        head(ui, state, theme);
        ui.add_space(PANEL_HEAD_GAP);
        if state.pending_action == Some(PendingAction::OpenRecent) {
            discard_confirm(ui, state, theme);
            ui.add_space(GROUP_GAP);
        }
        list(ui, state, theme);
        ui.add_space(GROUP_GAP);
        footer(ui, state, theme);
    });
}

/// Title on the left, the BACK pill (to the PROJECT panel) flush right.
fn head(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.set_min_height(controls::ROW_HEIGHT);
        neu::title(ui, theme, "Recent");
        controls::right_aligned(ui, "recent_back", |ui| {
            if neu::button(ui, theme, BACK_LABEL)
                .on_hover_text("Back to the PROJECT panel")
                .clicked()
            {
                state.right_tab = RightTab::Project;
            }
        });
    });
}

/// "Discard unsaved changes?" with DISCARD | KEEP, when a recent entry waits for it.
fn discard_confirm(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.add(
        egui::Label::new(
            RichText::new(DISCARD_QUESTION)
                .size(12.0)
                .color(theme.text_primary),
        )
        .wrap(),
    );
    ui.add_space(GROUP_GAP);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        if neu::button(ui, theme, "Discard").clicked() {
            state.confirm_pending();
        }
        if neu::button(ui, theme, "Keep")
            .on_hover_text("Esc")
            .clicked()
        {
            state.cancel_pending();
        }
    });
}

/// The entries, most recent first; a click opens one.
fn list(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    if state.recent.is_empty() {
        ui.add(
            egui::Label::new(
                RichText::new(EMPTY_LABEL)
                    .size(12.0)
                    .color(theme.text_secondary),
            )
            .wrap(),
        );
        return;
    }
    let entries: Vec<std::path::PathBuf> = state.recent.paths().to_vec();
    let mut open = None;
    for (index, path) in entries.iter().enumerate() {
        let current = state.project_path.as_deref() == Some(path.as_path());
        let name = entry_name(index, path);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let hovered = response.hovered();
            let color = if hovered || current {
                theme.text_primary
            } else {
                theme.text_secondary
            };
            let dot = egui::pos2(rect.min.x + CURRENT_DOT * 0.5, rect.center().y);
            if current {
                painter.circle_filled(dot, CURRENT_DOT * 0.5, theme.accent);
            }
            neu::text::paint_text(
                painter,
                egui::pos2(rect.min.x + CURRENT_DOT + DOT_GAP, rect.center().y),
                Align2::LEFT_CENTER,
                &file_label(path),
                TextKind::Value,
                color,
            );
        }
        let response = response
            .on_hover_text(path.display().to_string())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), current, &name)
        });
        if response.clicked() {
            open = Some(path.clone());
        }
    }
    if let Some(path) = open {
        state.request_open_recent(path);
    }
}

/// CLEAR LIST and the shortcut hint.
fn footer(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        let clear = ui
            .add_enabled_ui(!state.recent.is_empty(), |ui| {
                neu::button(ui, theme, CLEAR_LIST_LABEL)
                    .on_hover_text("Forget the recent projects (the files stay)")
                    .clicked()
            })
            .inner;
        if clear {
            state.recent.clear();
        }
        ui.label(
            RichText::new(format!("{} cycles", Shortcut::OpenRecent.label()))
                .size(11.0)
                .color(theme.text_secondary),
        );
    });
}
