//! The BROWSER column (M12, WINNER section 5 "BROWSER column"): a 208 pt window with the
//! header "BROWSER" + F9, the search toolbar (36) and three collapsible sections, each a 28 pt
//! head (triangle, section name, a count at the right): PRESETS
//! ([`crate::ui::preset_browser`]), PROJECT ([`crate::ui::project_bar`]) and RECENT
//! ([`crate::ui::recent_panel`]). The sections scroll together in one vertical scroll area.
//! Collapsed (F9, the header's chevron, or the narrow layout's default) it is the 28 pt
//! [`rail`]: the browser button (ON while open) and one icon button per section that opens
//! the column on that section, with the word BROWSER vertical under them.
//!
//! Accessible names: the header chevron "Collapse browser", the rail buttons "Expand
//! browser", "Presets section", "Project section", "Recent section"; the section heads
//! [`preset_browser::BROWSE_LABEL`] ("Presets"), [`PROJECT_LABEL`] ("Project") and
//! [`project_bar::RECENT_LABEL`] ("Recent"), buttons whose selected state is "open"; the
//! search field [`preset_browser::SEARCH_HINT`]; the CLOSE button
//! [`preset_browser::CLOSE_LABEL`] beside it while PRESETS is open.

use egui::{Align2, Rect, Response, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use neu::{ButtonKind, Icon, TextKind, Theme};

use crate::app::{AppState, Panel};
use crate::presets;
use crate::ui::preset_browser;
use crate::ui::project_bar;
use crate::ui::{popout, recent_panel};

/// Width of the open column.
pub const WIDTH: f32 = 208.0;
/// Width of the collapsed rail.
pub const RAIL_WIDTH: f32 = 28.0;
/// Height of a section head.
pub const SECTION_HEAD: f32 = 28.0;
/// Accessible name of the PROJECT section head.
pub const PROJECT_LABEL: &str = "Project";
/// Accessible names of the rail buttons.
pub const RAIL_EXPAND: &str = "Expand browser";
pub const RAIL_PRESETS: &str = "Presets section";
pub const RAIL_PROJECT: &str = "Project section";
pub const RAIL_RECENT: &str = "Recent section";
/// Accessible name of the header's collapse chevron.
pub const COLLAPSE_LABEL: &str = "Collapse browser";
/// Smallest height the PRESETS list keeps when the section is open (the chips and the count
/// sit above it inside the section's height).
const PRESET_LIST_MIN: f32 = 4.0 * preset_browser::ROW_HEIGHT + 100.0;
/// Room the open PROJECT / RECENT sections keep under the PRESETS list.
const PROJECT_BODY_ROOM: f32 = 150.0;
const RECENT_BODY_ROOM: f32 = 120.0;

/// Renders the open column into the available rect (`WIDTH` wide, the full height; the whole
/// window in its own window, where the header has DOCK instead of the chevron).
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let popped = state.is_popped(Panel::Browser);
    let width = if popped { ui.available_width() } else { WIDTH };
    let size = vec2(width, ui.available_height());
    // The header and the body both edit the state; the frame runs them one after the other.
    let cell = std::cell::RefCell::new(state);
    neu::window_frame_sized(
        ui,
        theme,
        size,
        "Browser",
        None,
        |ui| {
            if popped {
                popout::header_control(ui, &mut cell.borrow_mut(), theme, Panel::Browser);
                neu::keycap(ui, theme, "F9");
                return;
            }
            let chevron = neu::header_button(ui, theme, Icon::ChevronLeft);
            chevron
                .widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), COLLAPSE_LABEL));
            if chevron
                .on_hover_text(format!(
                    "Collapse to the rail ({})",
                    crate::shortcuts::Shortcut::ToggleBrowser.label()
                ))
                .clicked()
            {
                cell.borrow_mut().browser_open = false;
            }
            popout::header_control(ui, &mut cell.borrow_mut(), theme, Panel::Browser);
            neu::keycap(ui, theme, "F9");
        },
        |ui| body(ui, &mut cell.borrow_mut(), theme),
    );
}

/// The search toolbar, then the sections in a scroll area.
fn body(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    neu::toolbar(ui, theme, |ui| {
        let close_room = if state.preset_browser_open {
            24.0 + 8.0
        } else {
            0.0
        };
        let width = (ui.available_width() - close_room).max(40.0);
        preset_browser::search_field(ui, state, theme, width);
        if state.preset_browser_open {
            let close = neu::header_button(ui, theme, Icon::Clear);
            close.widget_info(|| {
                WidgetInfo::labeled(Role::Button, ui.is_enabled(), preset_browser::CLOSE_LABEL)
            });
            if close
                .on_hover_text("Close the preset list (Escape)")
                .clicked()
            {
                state.close_preset_browser();
            }
        }
    });
    let total_height = ui.available_height();
    neu::scroll_vertical("browser_sections").show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        // PRESETS: the head, the current preset row, then the body while open.
        let count = state.preset_matches().len();
        if section_head(
            ui,
            theme,
            preset_browser::BROWSE_LABEL,
            &count.to_string(),
            state.preset_browser_open,
            width,
        )
        .clicked()
        {
            if state.preset_browser_open {
                state.close_preset_browser();
            } else {
                state.open_preset_browser(false);
            }
        }
        preset_browser::current_row(ui, state, theme, width);
        if state.preset_browser_open {
            // The list takes the column's height less the other sections' room (their heads
            // and, when open, their bodies), never under four rows: it scrolls inside.
            let others = 3.0 * SECTION_HEAD
                + preset_browser::CURRENT_ROW
                + if state.project_open {
                    PROJECT_BODY_ROOM
                } else {
                    0.0
                }
                + if state.recent_open {
                    RECENT_BODY_ROOM
                } else {
                    0.0
                };
            let height = (total_height - others).max(PRESET_LIST_MIN);
            preset_browser::show(ui, state, theme, height);
        }
        // PROJECT.
        let project_count = if state.dirty { "modified" } else { "saved" };
        if section_head(
            ui,
            theme,
            PROJECT_LABEL,
            project_count,
            state.project_open,
            width,
        )
        .clicked()
        {
            state.project_open = !state.project_open;
        }
        if state.project_open {
            project_bar::show(ui, state, theme);
        }
        // RECENT.
        let recent_count = state.recent.paths().len().to_string();
        if section_head(
            ui,
            theme,
            project_bar::RECENT_LABEL,
            &recent_count,
            state.recent_open,
            width,
        )
        .clicked()
        {
            state.recent_open = !state.recent_open;
        }
        if state.recent_open {
            recent_panel::show(ui, state, theme);
        }
        ui.add_space(8.0);
    });
}

/// A section head: 28 h, padding 0 8, a 5 x 8 triangle (rotated while open), the name in
/// section type (`text_2`), the count in `text_3` at the right; hover `panel_3`. A button
/// named `name` whose selected state is `open`.
pub fn section_head(
    ui: &mut Ui,
    theme: &Theme,
    name: &str,
    count: &str,
    open: bool,
    width: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, SECTION_HEAD), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, 0.0, theme.panel_3);
        }
        painter.hline(
            rect.min.x..=rect.max.x,
            rect.min.y + 0.5,
            Stroke::new(1.0, theme.border),
        );
        // The triangle: pointing right when closed, down when open.
        let c = pos2(rect.min.x + 8.0 + 4.0, rect.center().y);
        let points = if open {
            vec![
                pos2(c.x - 4.0, c.y - 2.5),
                pos2(c.x + 4.0, c.y - 2.5),
                pos2(c.x, c.y + 2.5),
            ]
        } else {
            vec![
                pos2(c.x - 2.5, c.y - 4.0),
                pos2(c.x + 2.5, c.y),
                pos2(c.x - 2.5, c.y + 4.0),
            ]
        };
        painter.add(egui::Shape::convex_polygon(
            points,
            theme.text_2,
            Stroke::NONE,
        ));
        let mut format = TextKind::Section.format(theme.text_2);
        format.color = theme.text_2;
        let galley = painter.layout_job(egui::epaint::text::LayoutJob::single_section(
            TextKind::Section.display_string(name),
            format,
        ));
        painter.galley(
            pos2(
                rect.min.x + 8.0 + 8.0 + 6.0,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            theme.text_2,
        );
        if !count.is_empty() {
            neu::text::paint_text(
                painter,
                pos2(rect.max.x - 8.0, rect.center().y),
                Align2::RIGHT_CENTER,
                count,
                TextKind::Small,
                theme.text_3,
            );
        }
    }
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), open, name));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The collapsed rail: 28 wide, padding 4 0, gap 4: the browser button (ON while open), the
/// presets, project and recent buttons (each opens the column on that section) and the word
/// BROWSER vertical in section type.
pub fn rail(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let size = vec2(RAIL_WIDTH, ui.available_height());
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        neu::paint_panel(
            ui.painter(),
            rect,
            neu::Tone::Panel,
            theme.radius_control,
            theme,
        );
    }
    let inner = Rect::from_min_max(
        pos2(rect.min.x, rect.min.y + 4.0),
        pos2(rect.max.x, rect.max.y - 4.0),
    );
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Center))
            .id_salt("browser_rail"),
    );
    column.spacing_mut().item_spacing = vec2(0.0, 4.0);
    let ui = &mut column;
    let shortcut = crate::shortcuts::Shortcut::ToggleBrowser.label();
    if rail_icon(ui, theme, Icon::Browser, false, RAIL_EXPAND)
        .on_hover_text(format!("Open the browser ({shortcut})"))
        .clicked()
    {
        state.browser_open = true;
    }
    if rail_icon(
        ui,
        theme,
        Icon::Search,
        state.preset_browser_open,
        RAIL_PRESETS,
    )
    .on_hover_text(format!("Presets ({} in the library)", presets::count()))
    .clicked()
    {
        state.browser_open = true;
        state.open_preset_browser(false);
    }
    if rail_icon(ui, theme, Icon::File, state.project_open, RAIL_PROJECT)
        .on_hover_text("Project: new, open, save, export")
        .clicked()
    {
        state.browser_open = true;
        state.project_open = true;
    }
    if rail_icon(ui, theme, Icon::Clock, state.recent_open, RAIL_RECENT)
        .on_hover_text("Recent projects")
        .clicked()
    {
        state.browser_open = true;
        state.recent_open = true;
    }
    // The word BROWSER, vertical, under the buttons.
    let painter = ui.painter();
    let galley = painter.layout_job(egui::epaint::text::LayoutJob::single_section(
        TextKind::Section.display_string("Browser"),
        TextKind::Section.format(theme.text_3),
    ));
    let text_h = galley.size().x;
    let top = ui.cursor().min.y + 8.0;
    if top + text_h <= inner.max.y {
        let shape = egui::Shape::Text(egui::epaint::TextShape {
            pos: pos2(inner.center().x + galley.size().y * 0.5, top),
            galley,
            underline: Stroke::NONE,
            fallback_color: theme.text_3,
            override_text_color: None,
            opacity_factor: 1.0,
            angle: std::f32::consts::FRAC_PI_2,
        });
        painter.add(shape);
    }
}

/// A 24 x 24 ghost / toggle icon button of the rail named `name`.
fn rail_icon(ui: &mut Ui, theme: &Theme, icon: Icon, on: bool, name: &str) -> Response {
    let response = neu::icon_button_kind(
        ui,
        theme,
        icon,
        vec2(24.0, 24.0),
        ButtonKind::GhostToggle(on),
    );
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, name));
    response
}
