//! The BROWSER column's PROJECT section (M3, M4, M5; Deck since M12, WINNER section 5
//! "BROWSER column"): the file row (icon, name, the unsaved marker), NEW / OPEN / SAVE / SAVE
//! AS as wrapping xs buttons, the "Export" label with the repeats `x1 x2 x4 x8` small
//! segmented (in song mode the button reads EXPORT SONG and renders the whole chain, and
//! the selector gives way to a "Whole song" info line), one 11 pt info line, the full-width
//! primary EXPORT WAV button, the inline "Discard unsaved changes?" confirm and the status
//! line.
//!
//! File dialogs come from `rfd` and open only from an explicit action (a click here, the
//! toolbar's project menu or a shortcut dispatched by `BeatSlopperApp::run_shortcut`), and
//! only while `AppState::dialogs_enabled` (never under `--smoke-test` or in tests); the state
//! work is done by the `AppState` methods in `crate::project`. The public action functions
//! ([`save`], [`save_as`], [`request_open`], [`export`]) are what the shortcuts and the menu
//! call too, so every path behaves the same.

use std::path::{Path, PathBuf};

use egui::{Rect, RichText, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use engine::PlayMode;
use neu::{ButtonKind, ButtonSize, Icon, TextKind, Theme};

use crate::app::{AppState, EXPORT_REPEATS, PendingAction, truncate_name};
use crate::project::{DEFAULT_PROJECT_FILE, DEFAULT_WAV_FILE, file_label};
use crate::shortcuts::Shortcut;

/// The export button's label in song mode.
pub const EXPORT_SONG_LABEL: &str = "Export song";
/// The export button's label in pattern mode.
pub const EXPORT_LOOP_LABEL: &str = "Export WAV";
/// The repeats selector's items (`x1 x2 x4 x8`, [`EXPORT_REPEATS`] order).
pub const REPEAT_ITEMS: [&str; 4] = ["x1", "x2", "x4", "x8"];
/// Label beside the repeats selector.
pub const EXPORT_LABEL: &str = "Export";
/// The info line in song mode.
pub const WHOLE_SONG_LABEL: &str = "Whole song";
/// Longest file name shown on the file row.
const NAME_CHARS: usize = 22;
/// The unsaved marker's accessible name (tests look it up by this label).
pub const UNSAVED_LABEL: &str = "Unsaved changes";
/// The inline confirm's question.
pub const DISCARD_QUESTION: &str = "Discard unsaved changes?";
/// Accessible name of the RECENT section head (the project menu's entry opens it too).
pub const RECENT_LABEL: &str = "Recent";
/// Section padding (WINNER: dock padding 8).
pub const PAD: f32 = 8.0;
/// Gap between rows.
const ROW_GAP: f32 = 8.0;
/// Gap between wrapping buttons.
const BUTTON_GAP: f32 = 4.0;

/// Renders the PROJECT section body into `ui` (the column's width).
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        ui.add_space(0.0);
        file_row(ui, state, theme);
        // A pending RECENT entry confirms in the RECENT section.
        if matches!(
            state.pending_action,
            Some(PendingAction::New | PendingAction::Open)
        ) {
            discard_confirm(ui, state, theme);
        } else {
            file_buttons(ui, state, theme);
        }
        export_group(ui, state, theme);
        status_line(ui, state, theme);
        ui.add_space(PAD - ROW_GAP);
    });
}

/// The file icon, the file name (the full path is the tooltip) and the unsaved dot.
fn file_row(ui: &mut Ui, state: &AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = 6.0;
        let (icon_rect, _) = ui.allocate_exact_size(vec2(14.0, 20.0), Sense::hover());
        if ui.is_rect_visible(icon_rect) {
            neu::paint_icon(
                ui.painter(),
                Icon::File,
                Rect::from_center_size(icon_rect.center(), vec2(14.0, 14.0)),
                Stroke::new(neu::ICON_STROKE, theme.text_2),
            );
        }
        let name = state
            .project_path
            .as_deref()
            .map_or_else(|| "Untitled".to_owned(), file_label);
        let shown = truncate_name(&name, NAME_CHARS);
        neu::text_widget(ui, &shown, TextKind::Body, theme.text).on_hover_text(
            state.project_path.as_deref().map_or_else(
                || "No file yet (Ctrl+S saves)".to_owned(),
                |p| p.display().to_string(),
            ),
        );
        if state.dirty {
            let (rect, response) = ui.allocate_exact_size(vec2(7.0, 20.0), Sense::hover());
            if ui.is_rect_visible(rect) {
                ui.painter().circle_filled(rect.center(), 3.5, theme.accent);
            }
            response.widget_info(|| WidgetInfo::labeled(Role::Label, true, UNSAVED_LABEL));
            response.on_hover_text(UNSAVED_LABEL);
        }
    });
}

/// NEW · OPEN · SAVE · SAVE AS as wrapping xs buttons.
fn file_buttons(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 0.0),
        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
        |ui| {
            ui.add_space(PAD);
            ui.spacing_mut().item_spacing = vec2(BUTTON_GAP, BUTTON_GAP);
            if neu::button_kind(ui, theme, "New", ButtonKind::Secondary, ButtonSize::Xs)
                .on_hover_text(Shortcut::New.label())
                .clicked()
            {
                state.request_new();
            }
            if neu::button_kind(ui, theme, "Open", ButtonKind::Secondary, ButtonSize::Xs)
                .on_hover_text(format!(
                    "{} · {}: next recent project",
                    Shortcut::Open.label(),
                    Shortcut::OpenRecent.label()
                ))
                .clicked()
            {
                request_open(state);
            }
            if neu::button_kind(ui, theme, "Save", ButtonKind::Secondary, ButtonSize::Xs)
                .on_hover_text(Shortcut::Save.label())
                .clicked()
            {
                save(state);
            }
            if neu::button_kind(ui, theme, "Save as", ButtonKind::Secondary, ButtonSize::Xs)
                .on_hover_text(Shortcut::SaveAs.label())
                .clicked()
            {
                save_as(state);
            }
        },
    );
}

/// "Discard unsaved changes?" with DISCARD | KEEP, shown in place of the file buttons while
/// a New / Open waits for confirmation.
fn discard_confirm(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.add(
            egui::Label::new(RichText::new(DISCARD_QUESTION).size(12.0).color(theme.text)).wrap(),
        );
    });
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        if neu::button_kind(ui, theme, "Discard", ButtonKind::Primary, ButtonSize::Xs).clicked() {
            discard(state);
        }
        if neu::button_kind(ui, theme, "Keep", ButtonKind::Secondary, ButtonSize::Xs)
            .on_hover_text("Esc")
            .clicked()
        {
            state.cancel_pending();
        }
    });
}

/// "Export" + `x1 x2 x4 x8` (pattern mode) or the "Whole song" line (song mode), an info line
/// and the primary EXPORT button.
fn export_group(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let song = state.bank.mode == PlayMode::Song;
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = 6.0;
        neu::label(ui, theme, EXPORT_LABEL);
        if song {
            neu::text_widget(ui, WHOLE_SONG_LABEL, TextKind::Small, theme.text_2);
        } else {
            let before = EXPORT_REPEATS
                .iter()
                .position(|r| *r == state.export_repeats)
                .unwrap_or(0);
            let mut selected = before;
            neu::segmented_small(ui, theme, &mut selected, &REPEAT_ITEMS)
                .on_hover_text("How many times the pattern repeats in the file");
            if selected != before {
                state.export_repeats = EXPORT_REPEATS[selected];
            }
        }
    });
    let info = if song {
        format!(
            "{} entries · the chain, once",
            usize::from(state.bank.chain_len)
        )
    } else {
        let bars = state.pattern().bars() as u32 * state.export_repeats;
        format!(
            "{} · {} {} at {} BPM",
            state.current_letter(),
            bars,
            if bars == 1 { "bar" } else { "bars" },
            crate::tempo::fmt_bpm(state.bpm)
        )
    };
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        neu::text_widget(ui, &info, TextKind::Small, theme.text_2);
    });
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = 6.0;
        let exporting = state.is_exporting();
        let label = if song {
            EXPORT_SONG_LABEL
        } else {
            EXPORT_LOOP_LABEL
        };
        let width = ui.available_width() - PAD;
        let clicked = ui
            .add_enabled_ui(!exporting, |ui| {
                neu::button_wide(
                    ui,
                    theme,
                    label,
                    ButtonKind::Primary,
                    ButtonSize::Regular,
                    width,
                )
                .on_hover_text(Shortcut::Export.label())
                .clicked()
            })
            .inner;
        if clicked {
            export(state);
        }
    });
    if state.is_exporting() {
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            neu::lamp(ui, theme, true, "Exporting");
        });
    }
}

fn status_line(ui: &mut Ui, state: &AppState, theme: &Theme) {
    if let Some(text) = state.status_text() {
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            ui.add(egui::Label::new(RichText::new(text).size(12.0).color(theme.text)).wrap());
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Actions (buttons, the menu and shortcuts share these).

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

/// EXPORT WAV / EXPORT SONG: asks for a path and renders the current repeats of the edit
/// pattern (pattern mode) or the whole chain (song mode) on a worker; refused while an export
/// is already running.
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
        .set_title("Export as WAV")
        .set_file_name(name);
    if let Some(dir) = start_dir(current) {
        dialog = dialog.set_directory(dir);
    }
    dialog.save_file()
}

/// Where the first line of a section body starts (the section padding), for callers that
/// align with it.
#[must_use]
pub fn body_left(rect: Rect) -> f32 {
    pos2(rect.min.x + PAD, rect.min.y).x
}
