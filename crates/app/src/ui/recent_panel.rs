//! The BROWSER column's RECENT section (QOL pass; Deck since M12): the last eight project
//! files as list rows (a file icon, the name, the age in 11 pt `text_3`; the open project is
//! the selected row), most recent first. A click opens the entry
//! ([`AppState::request_open_recent`]: with unsaved changes the DISCARD / KEEP confirm
//! appears above the list first). Below the list a CLEAR LIST xs button and the
//! `Ctrl+Shift+O` hint (the shortcut cycles the list). Rows are list items named
//! [`entry_name`] (`"Recent 1: beat.beatslopper.json"`).

use std::time::SystemTime;

use egui::{Rect, RichText, Stroke, Ui, vec2};
use neu::{ButtonKind, ButtonSize, Icon, TextKind, Theme};

use crate::app::{AppState, PendingAction};
use crate::project::file_label;
use crate::shortcuts::Shortcut;
use crate::ui::project_bar::{DISCARD_QUESTION, PAD};

/// Height of one entry row.
pub const ROW_HEIGHT: f32 = neu::LIST_ROW;
/// Shown while the list is empty.
pub const EMPTY_LABEL: &str = "No recent projects yet";
/// Label of the button that forgets the list.
pub const CLEAR_LIST_LABEL: &str = "Clear list";
/// Gap between rows of the section.
const ROW_GAP: f32 = 8.0;

/// Accessible name of entry `index` (0-based) for `path`: `"Recent 1: beat.beatslopper.json"`.
#[must_use]
pub fn entry_name(index: usize, path: &std::path::Path) -> String {
    format!("Recent {}: {}", index + 1, file_label(path))
}

/// The age of a file as a short string (`"2 min"`, `"3 h"`, `"5 d"`), empty when unknown.
#[must_use]
pub fn age_text(path: &std::path::Path, now: SystemTime) -> String {
    let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) else {
        return String::new();
    };
    let Ok(age) = now.duration_since(modified) else {
        return "now".to_owned();
    };
    let secs = age.as_secs();
    if secs < 60 {
        "now".to_owned()
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86_400 {
        format!("{} h", secs / 3600)
    } else {
        format!("{} d", secs / 86_400)
    }
}

/// Renders the RECENT section body into `ui`.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        ui.add_space(0.0);
        if state.pending_action == Some(PendingAction::OpenRecent) {
            discard_confirm(ui, state, theme);
        }
        list(ui, state, theme);
        footer(ui, state, theme);
        ui.add_space(PAD - ROW_GAP);
    });
}

/// "Discard unsaved changes?" with DISCARD | KEEP, when a recent entry waits for it.
fn discard_confirm(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.add(
            egui::Label::new(RichText::new(DISCARD_QUESTION).size(12.0).color(theme.text)).wrap(),
        );
    });
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = 4.0;
        if neu::button_kind(ui, theme, "Discard", ButtonKind::Primary, ButtonSize::Xs).clicked() {
            state.confirm_pending();
        }
        if neu::button_kind(ui, theme, "Keep", ButtonKind::Secondary, ButtonSize::Xs)
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
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            ui.add(
                egui::Label::new(RichText::new(EMPTY_LABEL).size(12.0).color(theme.text_2)).wrap(),
            );
        });
        return;
    }
    let entries: Vec<std::path::PathBuf> = state.recent.paths().to_vec();
    let now = SystemTime::now();
    let mut open = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    for (index, path) in entries.iter().enumerate() {
        let current = state.project_path.as_deref() == Some(path.as_path());
        let name = entry_name(index, path);
        let label = file_label(path);
        let age = age_text(path, now);
        let response = neu::list_row_named(ui, theme, current, &name, |ui| {
            let (icon_rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
            if ui.is_rect_visible(icon_rect) {
                neu::paint_icon(
                    ui.painter(),
                    Icon::File,
                    Rect::from_center_size(icon_rect.center(), vec2(12.0, 12.0)),
                    Stroke::new(neu::ICON_STROKE, theme.text_2),
                );
            }
            let age_width = if age.is_empty() { 0.0 } else { 40.0 };
            let name_width = (ui.available_width() - age_width).max(20.0);
            let shown = neu::ellipsise(ui.painter(), &label, TextKind::Body, name_width);
            let (rect, _) =
                ui.allocate_exact_size(vec2(name_width, ROW_HEIGHT), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                neu::text::paint_text(
                    ui.painter(),
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    &shown,
                    TextKind::Body,
                    if current { theme.text } else { theme.text_2 },
                );
            }
            if !age.is_empty() {
                neu::text_widget(ui, &age, TextKind::Small, theme.text_3);
            }
        });
        let response = response
            .on_hover_text(path.display().to_string())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            open = Some(path.clone());
        }
    }
    ui.spacing_mut().item_spacing.y = ROW_GAP;
    ui.add_space(0.0);
    if let Some(path) = open {
        state.request_open_recent(path);
    }
}

/// CLEAR LIST and the shortcut hint.
fn footer(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = 6.0;
        let clear = ui
            .add_enabled_ui(!state.recent.is_empty(), |ui| {
                neu::button_kind(
                    ui,
                    theme,
                    CLEAR_LIST_LABEL,
                    ButtonKind::Secondary,
                    ButtonSize::Xs,
                )
                .on_hover_text("Forget the recent projects (the files stay)")
                .clicked()
            })
            .inner;
        if clear {
            state.recent.clear();
        }
        neu::text_widget(
            ui,
            &format!("{} cycles", Shortcut::OpenRecent.label()),
            TextKind::Small,
            theme.text_3,
        );
    });
}
