//! PROJECT panel (M3): NEW / OPEN / SAVE / SAVE AS, EXPORT WAV with the repeats selector,
//! the "Exporting" lamp and the status line.
//!
//! Sits to the right of the SEQUENCER panel ([`WIDTH`] wide). File dialogs come from `rfd`
//! and are only opened from a click (the smoke test never clicks); the state work is done by
//! the `AppState` methods in `crate::project`.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};
use neu::Theme;

use crate::app::{AppState, EXPORT_REPEATS};
use crate::project::{DEFAULT_PROJECT_FILE, DEFAULT_WAV_FILE, file_label};
use crate::ui::controls::{self, GROUP_GAP, PANEL_HEAD_GAP};

/// Width of the panel including its padding.
pub const WIDTH: f32 = 250.0;
/// Gap between the file group and the export group.
const SECTION_GAP: f32 = 16.0;
/// Gap between pills in a row.
const PILL_GAP: f32 = 8.0;

/// Renders the PROJECT panel into `ui`, filling the space it is given.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        head(ui, state, theme);
        ui.add_space(PANEL_HEAD_GAP);
        file_group(ui, state, theme);
        ui.add_space(SECTION_GAP);
        export_group(ui, state, theme);
        ui.add_space(GROUP_GAP);
        status_line(ui, state, theme);
    });
}

fn head(ui: &mut Ui, state: &AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.set_min_height(controls::ROW_HEIGHT);
        neu::title(ui, theme, "Project");
    });
    let name = state
        .project_path
        .as_deref()
        .map_or_else(|| "Unsaved".to_owned(), file_label);
    ui.label(RichText::new(name).size(11.0).color(theme.text_secondary))
        .on_hover_text(
            state
                .project_path
                .as_deref()
                .map_or_else(String::new, |p| p.display().to_string()),
        );
}

fn file_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        if neu::button(ui, theme, "New").clicked() {
            state.new_project();
        }
        if neu::button(ui, theme, "Open").clicked()
            && let Some(path) = pick_project_to_open(state.project_path.as_deref())
        {
            state.load_project_from(path);
        }
    });
    ui.add_space(GROUP_GAP);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        if neu::button(ui, theme, "Save").clicked() {
            match state.project_path.clone() {
                Some(path) => {
                    state.save_project_to(path);
                }
                None => save_as(state),
            }
        }
        if neu::button(ui, theme, "Save as").clicked() {
            save_as(state);
        }
    });
}

fn save_as(state: &mut AppState) {
    if let Some(path) = pick_project_to_save(state.project_path.as_deref()) {
        state.save_project_to(path);
    }
}

fn export_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let names: Vec<String> = EXPORT_REPEATS.iter().map(u32::to_string).collect();
    let items: Vec<&str> = names.iter().map(String::as_str).collect();
    let before = EXPORT_REPEATS
        .iter()
        .position(|r| *r == state.export_repeats)
        .unwrap_or(0);
    let mut selected = before;
    controls::group_head(ui, theme, "Bars", |ui| {
        neu::segmented(ui, theme, &mut selected, &items);
    });
    if selected != before {
        state.export_repeats = EXPORT_REPEATS[selected];
    }
    ui.add_space(GROUP_GAP);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        let exporting = state.is_exporting();
        let clicked = ui
            .add_enabled_ui(!exporting, |ui| {
                neu::button(ui, theme, "Export WAV").clicked()
            })
            .inner;
        if clicked && let Some(path) = pick_wav_to_save(state.project_path.as_deref()) {
            state.start_export(path);
        }
        neu::lamp(ui, theme, exporting, "Exporting");
    });
}

fn status_line(ui: &mut Ui, state: &AppState, theme: &Theme) {
    if let Some(text) = state.status_text() {
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(theme.text_primary)).wrap());
    }
}

// ---------------------------------------------------------------------------------------------
// Dialogs (only ever reached from a click).

/// The directory to start a dialog in: the current project's folder, if any.
fn start_dir(current: Option<&Path>) -> Option<PathBuf> {
    current
        .and_then(Path::parent)
        .filter(|dir| dir.is_dir())
        .map(Path::to_path_buf)
}

fn project_dialog(current: Option<&Path>) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new().add_filter("BeatSlopper project", &["json"]);
    if let Some(dir) = start_dir(current) {
        dialog = dialog.set_directory(dir);
    }
    dialog
}

fn pick_project_to_open(current: Option<&Path>) -> Option<PathBuf> {
    project_dialog(current)
        .set_title("Open project")
        .pick_file()
}

fn pick_project_to_save(current: Option<&Path>) -> Option<PathBuf> {
    let name = current.and_then(Path::file_name).map_or_else(
        || DEFAULT_PROJECT_FILE.to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    project_dialog(current)
        .set_title("Save project as")
        .set_file_name(name)
        .save_file()
}

fn pick_wav_to_save(current: Option<&Path>) -> Option<PathBuf> {
    let name = current
        .and_then(Path::file_stem)
        .map(|stem| stem.to_string_lossy().into_owned())
        .map(|stem| {
            // "beat.beatslopper" -> "beat"
            stem.trim_end_matches(".beatslopper").to_owned() + ".wav"
        })
        .unwrap_or_else(|| DEFAULT_WAV_FILE.to_owned());
    let mut dialog = rfd::FileDialog::new()
        .add_filter("WAV", &["wav"])
        .set_title("Export loop as WAV")
        .set_file_name(name);
    if let Some(dir) = start_dir(current) {
        dialog = dialog.set_directory(dir);
    }
    dialog.save_file()
}
