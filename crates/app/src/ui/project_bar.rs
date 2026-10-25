//! PROJECT panel (M3, M4): NEW / OPEN / SAVE / SAVE AS, UNDO / REDO, EXPORT WAV with the
//! repeats selector, the "Exporting" lamp, the "Unsaved changes" marker, the inline
//! "Discard unsaved changes?" confirm and the status line.
//!
//! Sits to the right of the SEQUENCER panel ([`WIDTH`] wide). File dialogs come from `rfd`
//! and open only from an explicit action (a click here or a shortcut dispatched by
//! `BeatSlopperApp::run_shortcut`), and only while `AppState::dialogs_enabled` (never under
//! `--smoke-test` or in tests); the state work is done by the `AppState` methods in
//! `crate::project`. The public action functions ([`save`], [`save_as`], [`request_open`],
//! [`export`]) are what the shortcuts call too, so both paths behave the same.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};
use neu::Theme;

use crate::app::{AppState, EXPORT_REPEATS, PendingAction};
use crate::project::{DEFAULT_PROJECT_FILE, DEFAULT_WAV_FILE, file_label};
use crate::shortcuts::Shortcut;
use crate::ui::controls::{self, GROUP_GAP, PANEL_HEAD_GAP};

/// Width of the panel including its padding.
pub const WIDTH: f32 = 250.0;
/// Gap between the file group and the export group.
const SECTION_GAP: f32 = 16.0;
/// Gap between pills in a row.
const PILL_GAP: f32 = 8.0;
/// The unsaved marker's text (tests look it up by this label).
pub const UNSAVED_LABEL: &str = "Unsaved changes";
/// The inline confirm's question.
pub const DISCARD_QUESTION: &str = "Discard unsaved changes?";

/// Renders the PROJECT panel into `ui`, filling the space it is given.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::panel(ui, theme, |ui| {
        ui.set_min_size(ui.available_size());
        head(ui, state, theme);
        ui.add_space(PANEL_HEAD_GAP);
        if state.pending_action.is_some() {
            discard_confirm(ui, state, theme);
        } else {
            file_group(ui, state, theme);
        }
        ui.add_space(GROUP_GAP);
        history_group(ui, state, theme);
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
        .map_or_else(|| "Untitled".to_owned(), file_label);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        ui.label(RichText::new(name).size(11.0).color(theme.text_secondary))
            .on_hover_text(
                state
                    .project_path
                    .as_deref()
                    .map_or_else(String::new, |p| p.display().to_string()),
            );
        if state.dirty {
            ui.label(
                RichText::new(UNSAVED_LABEL)
                    .size(11.0)
                    .color(theme.text_secondary),
            );
        }
    });
}

/// NEW | OPEN over SAVE | SAVE AS.
fn file_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        if neu::button(ui, theme, "New")
            .on_hover_text(Shortcut::New.label())
            .clicked()
        {
            state.request_new();
        }
        if neu::button(ui, theme, "Open")
            .on_hover_text(Shortcut::Open.label())
            .clicked()
        {
            request_open(state);
        }
    });
    ui.add_space(GROUP_GAP);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        if neu::button(ui, theme, "Save")
            .on_hover_text(Shortcut::Save.label())
            .clicked()
        {
            save(state);
        }
        if neu::button(ui, theme, "Save as")
            .on_hover_text(Shortcut::SaveAs.label())
            .clicked()
        {
            save_as(state);
        }
    });
}

/// "Discard unsaved changes?" with DISCARD | KEEP, shown in place of the file group while a
/// New / Open waits for confirmation.
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
            discard(state);
        }
        if neu::button(ui, theme, "Keep")
            .on_hover_text("Esc")
            .clicked()
        {
            state.cancel_pending();
        }
    });
}

/// UNDO | REDO pills, greyed out while there is nothing to step to.
fn history_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        let undo = ui
            .add_enabled_ui(state.undo.can_undo(), |ui| {
                neu::button(ui, theme, "Undo")
                    .on_hover_text(Shortcut::Undo.label())
                    .clicked()
            })
            .inner;
        if undo {
            state.undo();
        }
        let redo = ui
            .add_enabled_ui(state.undo.can_redo(), |ui| {
                neu::button(ui, theme, "Redo")
                    .on_hover_text(Shortcut::Redo.label())
                    .clicked()
            })
            .inner;
        if redo {
            state.redo();
        }
    });
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
                neu::button(ui, theme, "Export WAV")
                    .on_hover_text(Shortcut::Export.label())
                    .clicked()
            })
            .inner;
        if clicked {
            export(state);
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
// Actions (buttons and shortcuts share these).

/// SAVE: writes to the current path, or behaves like [`save_as`] without one.
pub fn save(state: &mut AppState) {
    match state.project_path.clone() {
        Some(path) => {
            state.save_project_to(path);
        }
        None => save_as(state),
    }
}

/// SAVE AS: asks for a path, then saves.
pub fn save_as(state: &mut AppState) {
    if !state.dialogs_enabled {
        return;
    }
    if let Some(path) = pick_project_to_save(state.project_path.as_deref()) {
        state.save_project_to(path);
    }
}

/// OPEN: asks for a file and loads it, unless unsaved changes first need the DISCARD / KEEP
/// confirm (then nothing happens until [`discard`]).
pub fn request_open(state: &mut AppState) {
    if state.request_open() {
        open_dialog(state);
    }
}

/// DISCARD: performs the pending New (in the state) or Open (dialog).
pub fn discard(state: &mut AppState) {
    if state.confirm_pending() == Some(PendingAction::Open) {
        open_dialog(state);
    }
}

fn open_dialog(state: &mut AppState) {
    if !state.dialogs_enabled {
        return;
    }
    if let Some(path) = pick_project_to_open(state.project_path.as_deref()) {
        state.load_project_from(path);
    }
}

/// EXPORT WAV: asks for a path and renders the current repeats on a worker; refused while an
/// export is already running.
pub fn export(state: &mut AppState) {
    if state.is_exporting() {
        state.set_status("Already exporting");
        return;
    }
    if !state.dialogs_enabled {
        return;
    }
    if let Some(path) = pick_wav_to_save(state.project_path.as_deref()) {
        state.start_export(path);
    }
}

// ---------------------------------------------------------------------------------------------
// Dialogs (only ever reached from an explicit action while dialogs are enabled).

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
