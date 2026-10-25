//! Preset browser (M9): the SYNTH panel's preset chooser over the unified preset list
//! (`crate::presets`: the eight classics, then the 91-preset library).
//!
//! Two pieces, both drawn by `ui/synth_panel.rs`:
//!
//! - [`head_strip`], flush right in the panel head: `<` (previous), the current preset's
//!   name of the selected track in an inset-s well ([`NAME_WIDTH`] wide, cut with an
//!   ellipsis, a click opens the browser), `>` (next) and the `BROWSE` pill (lit while the
//!   browser is open). The MODIFIED lamp the head already had sits after it. Previous / next
//!   step within the browser's category filter and wrap ([`AppState::step_preset`]).
//! - [`show`], the browser itself, which takes the place of the oscillator groups (the
//!   panel body) while [`AppState::preset_browser_open`] is set, so the panel keeps its
//!   size and the page layout does not move (the whole sequencer stays visible at the
//!   1280 x 1140 design size). Top to bottom: the category `segmented` ([`CATEGORY_LABELS`]:
//!   `All` + the eight categories with short labels, so the nine items fit the panel), a row
//!   with the search field (placeholder [`SEARCH_HINT`]; an `egui::TextEdit` in the kit's
//!   inset-s editor pill, like the inline value editors, so the shortcuts and the piano stay
//!   quiet while it has the keyboard, like the TRACKS rename field), the match count and
//!   the `CLOSE` pill, then the scrollable list of matching presets in as many
//!   [`COLUMN_MIN_WIDTH`] columns as fit: the selected preset on an inset-s well with an
//!   accent dot, every entry showing its name (and, in the `All` view, its category) with a
//!   `vinyl` / `drone` tag where the preset hisses without a note or never dies away. A
//!   click applies the preset to the selected track. Escape (also from inside the search
//!   field), `CLOSE` and `BROWSE` close it; the filter and the search text are kept.

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, FontId, Frame, Id, Key, Layout, Margin, Rect, RichText, Role,
    ScrollArea, Sense, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use neu::{Depth, TextKind, Theme, paint_soft_rect};

use crate::app::AppState;
use crate::presets::{self, Category};
use crate::ui::controls::{self, GROUP_GAP};

/// Label of the pill that opens / closes the browser.
pub const BROWSE_LABEL: &str = "Browse";
/// Label of the browser's close pill.
pub const CLOSE_LABEL: &str = "Close";
/// Label (and accessible name) of the previous-preset button.
pub const PREV_LABEL: &str = "<";
/// Label (and accessible name) of the next-preset button.
pub const NEXT_LABEL: &str = ">";
/// Placeholder of the search field.
pub const SEARCH_HINT: &str = "search name or tag";
/// The category row's first item: no category filter.
pub const ALL_LABEL: &str = "All";
/// The category `segmented`'s items: [`ALL_LABEL`] then the eight categories in
/// `Category::ALL` order, shortened so the row fits the SYNTH panel at the minimum window
/// width (the full labels are the items' tooltips).
pub const CATEGORY_LABELS: [&str; 9] = [
    ALL_LABEL, "Lo Keys", "Lo Gtr", "Lo Pads", "Leads", "Bass", "Plucks", "EDM Pads", "Risers",
];
/// Tag shown on presets whose lo-fi insert hisses even with no note playing.
pub const VINYL_TAG: &str = "vinyl";
/// Tag shown on presets that keep sounding after release.
pub const DRONE_TAG: &str = "drone";
/// Shown in the list when nothing matches.
pub const NO_MATCH: &str = "No preset matches";
/// Longest search text accepted.
pub const SEARCH_CHAR_LIMIT: usize = 32;

/// Width of the preset name well in the head strip.
pub const NAME_WIDTH: f32 = 128.0;
/// Width of the search field.
pub const SEARCH_WIDTH: f32 = 220.0;
/// Height of one list entry.
pub const ROW_HEIGHT: f32 = 26.0;
/// Gap between list rows.
pub const ROW_GAP: f32 = 2.0;
/// Narrowest list column; the list shows as many columns of at least this width as fit.
pub const COLUMN_MIN_WIDTH: f32 = 200.0;
/// Gap between list columns.
pub const COLUMN_GAP: f32 = 10.0;
/// Corner radius of a list entry's well.
pub const ROW_RADIUS: f32 = 8.0;
/// Horizontal padding inside a list entry and the name well.
const ENTRY_PAD_X: f32 = 10.0;
/// Diameter of the accent dot marking the selected entry.
const SELECTED_DOT: f32 = 6.0;
/// Font size of the entry tags and the category text.
const TAG_SIZE: f32 = 11.0;

/// Accessible name of the head's preset name well: `"Preset: Lofi Piano"`.
#[must_use]
pub fn head_name(preset: &str) -> String {
    format!("Preset: {preset}")
}

/// The category the segmented item `index` of [`CATEGORY_LABELS`] stands for (`None` = All).
#[must_use]
pub fn category_at(index: usize) -> Option<Category> {
    index
        .checked_sub(1)
        .and_then(|i| Category::ALL.get(i).copied())
}

/// The segmented item index of `category` (`None` = All).
#[must_use]
pub fn category_item(category: Option<Category>) -> usize {
    category.map_or(0, |c| c.index() + 1)
}

/// The match count text: `"12 of 99"`.
#[must_use]
pub fn count_text(matching: usize, total: usize) -> String {
    format!("{matching} of {total}")
}

/// The tags shown beside preset `index`'s name: `vinyl` / `drone` where they apply, and the
/// category label when `show_category` (the `All` view).
#[must_use]
pub fn entry_tags(index: usize, show_category: bool) -> Vec<&'static str> {
    let Some(preset) = presets::get(index) else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    if preset.has_vinyl_noise() {
        tags.push(VINYL_TAG);
    }
    if preset.is_drone() {
        tags.push(DRONE_TAG);
    }
    if show_category {
        tags.push(preset.category.label());
    }
    tags
}

/// The search field's widget id (stable, so a focus request can address it before it is
/// drawn).
fn search_id() -> Id {
    Id::NULL.with("preset_browser_search")
}

// ---------------------------------------------------------------------------------------------
// Head strip.

/// The head's preset strip: `<`, the name well, `>`, `BROWSE`.
pub fn head_strip(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    if neu::button(ui, theme, PREV_LABEL)
        .on_hover_text("Previous preset in the category (Ctrl+PageUp)")
        .clicked()
    {
        state.step_preset(-1);
    }
    name_well(ui, state, theme);
    if neu::button(ui, theme, NEXT_LABEL)
        .on_hover_text("Next preset in the category (Ctrl+PageDown)")
        .clicked()
    {
        state.step_preset(1);
    }
    if neu::toggle_button(ui, theme, BROWSE_LABEL, state.preset_browser_open)
        .on_hover_text("Browse the preset library (Ctrl+Shift+P searches; Escape closes)")
        .clicked()
    {
        if state.preset_browser_open {
            state.close_preset_browser();
        } else {
            state.open_preset_browser(false);
        }
    }
}

/// The current preset's name on an inset-s pill, [`NAME_WIDTH`] wide; a click opens the
/// browser. Accessible as a button named [`head_name`].
fn name_well(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let name = state.preset_name();
    let (rect, response) =
        ui.allocate_exact_size(vec2(NAME_WIDTH, neu::button::PILL_HEIGHT), Sense::click());
    let accessible = head_name(name);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &accessible));
    if ui.is_rect_visible(rect) {
        paint_soft_rect(ui.painter(), rect, theme.radius_pill, Depth::InsetS, theme);
        let colour = if response.hovered() {
            theme.accent
        } else {
            theme.text_primary
        };
        paint_truncated(
            ui,
            rect.shrink2(vec2(ENTRY_PAD_X, 0.0)),
            name,
            FontId::proportional(TextKind::Value.size()),
            colour,
            Align::Center,
        );
    }
    let category = presets::get(state.preset_index()).map_or("", |p| p.category.label());
    if response
        .on_hover_text(format!("{name} · {category}\nClick to browse the library"))
        .clicked()
    {
        state.open_preset_browser(false);
    }
}

// ---------------------------------------------------------------------------------------------
// The browser.

/// The browser body: category row, search row, the list. Escape closes it.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    // Escape closes, also from inside the search field (egui drops the field's focus on
    // Escape and the keyboard handler skips Escape while a field has the keyboard).
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        state.close_preset_browser();
        return;
    }
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = GROUP_GAP;
        category_row(ui, state, theme);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = controls::HEAD_GAP;
            search_field(ui, state, theme);
            let matching = state.preset_matches().len();
            neu::label(ui, theme, &count_text(matching, presets::count()))
                .on_hover_text("Matching presets of the category");
            controls::right_aligned(ui, "preset_browser_close", |ui| {
                if neu::button(ui, theme, CLOSE_LABEL)
                    .on_hover_text("Close the browser (Escape)")
                    .clicked()
                {
                    state.close_preset_browser();
                }
            });
        });
        list(ui, state, theme);
    });
}

/// The category `segmented`: `All` + the eight categories.
fn category_row(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let before = category_item(state.preset_category);
    let mut selected = before;
    neu::segmented(ui, theme, &mut selected, &CATEGORY_LABELS)
        .on_hover_text("Category: filters the list and the < / > buttons");
    if selected != before {
        state.preset_category = category_at(selected);
    }
}

/// The search field: an `egui::TextEdit` on the kit's inset-s editor pill, styled like the
/// inline value editors. Takes the keyboard when [`AppState::preset_search_focus`] asks.
fn search_field(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(SEARCH_WIDTH, controls::ROW_HEIGHT), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_soft_rect(ui.painter(), rect, theme.radius_pill, Depth::InsetS, theme);
    }
    let id = search_id();
    if state.preset_search_focus {
        ui.memory_mut(|m| m.request_focus(id));
        state.preset_search_focus = false;
    }
    let inner = rect.shrink2(vec2(ENTRY_PAD_X, neu::value_edit::EDIT_PAD_Y));
    let font = FontId::proportional(TextKind::Value.size());
    let response = ui
        .scope_builder(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.text_cursor.stroke.color = theme.accent;
                visuals.selection.bg_fill = theme.glow;
                visuals.selection.stroke.color = theme.text_primary;
                ui.add(
                    egui::TextEdit::singleline(&mut state.preset_query)
                        .id(id)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(font)
                        .text_color(theme.text_primary)
                        .hint_text(RichText::new(SEARCH_HINT).color(theme.text_secondary))
                        .desired_width(inner.width())
                        .min_size(inner.size())
                        .char_limit(SEARCH_CHAR_LIMIT),
                )
            },
        )
        .inner;
    response.on_hover_text("Search by name or tag (Ctrl+Shift+P)");
}

/// The scrollable list of matching presets, in as many columns as fit.
fn list(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let matches = state.preset_matches();
    let current = state.preset_index();
    let show_category = state.preset_category.is_none();
    let mut clicked = None;
    ScrollArea::vertical()
        .id_salt("preset_browser_list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Measured inside the scroll area: its scrollbar takes a few points.
            let (columns, column_width) = columns_for(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(COLUMN_GAP, ROW_GAP);
            if matches.is_empty() {
                neu::label(ui, theme, NO_MATCH);
            }
            for row in matches.chunks(columns) {
                ui.horizontal(|ui| {
                    for &index in row {
                        if entry(
                            ui,
                            theme,
                            index,
                            index == current,
                            show_category,
                            column_width,
                        ) {
                            clicked = Some(index);
                        }
                    }
                });
            }
        });
    if let Some(index) = clicked {
        state.apply_preset(index);
    }
}

/// How many list columns fit in `width` (at least one, each at least [`COLUMN_MIN_WIDTH`]
/// wide, [`COLUMN_GAP`] apart) and how wide each one is.
#[must_use]
pub fn columns_for(width: f32) -> (usize, f32) {
    let columns =
        (((width + COLUMN_GAP) / (COLUMN_MIN_WIDTH + COLUMN_GAP)).floor() as usize).max(1);
    let column_width = (width - COLUMN_GAP * (columns - 1) as f32) / columns as f32;
    (columns, column_width)
}

/// One list entry: the name (left, cut with an ellipsis) and its tags (right, 11 pt
/// secondary); the selected one on an inset-s well with an accent dot. Accessible as a
/// button named after the preset (`toggled` = selected). Returns `true` when clicked.
fn entry(
    ui: &mut Ui,
    theme: &Theme,
    index: usize,
    selected: bool,
    show_category: bool,
    width: f32,
) -> bool {
    let Some(preset) = presets::get(index) else {
        return false;
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
    response
        .widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, preset.name));
    let tags = entry_tags(index, show_category);
    if ui.is_rect_visible(rect) {
        if selected {
            paint_soft_rect(ui.painter(), rect, ROW_RADIUS, Depth::InsetS, theme);
            ui.painter().circle_filled(
                pos2(rect.min.x + ENTRY_PAD_X * 0.5, rect.center().y),
                SELECTED_DOT * 0.5,
                theme.accent,
            );
        }
        let tag_text = tags.join(" · ");
        let tag_font = FontId::proportional(TAG_SIZE);
        let tag_width = if tag_text.is_empty() {
            0.0
        } else {
            let galley = ui.painter().layout_no_wrap(
                tag_text.clone(),
                tag_font.clone(),
                theme.text_secondary,
            );
            let tag_rect = Rect::from_min_max(
                pos2(rect.max.x - ENTRY_PAD_X - galley.size().x, rect.min.y),
                pos2(rect.max.x - ENTRY_PAD_X, rect.max.y),
            );
            let pos = Align2::RIGHT_CENTER.anchor_size(tag_rect.right_center(), galley.size());
            ui.painter().galley(pos.min, galley, theme.text_secondary);
            tag_rect.width() + ENTRY_PAD_X
        };
        let colour = if selected || response.hovered() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let name_rect = Rect::from_min_max(
            pos2(rect.min.x + ENTRY_PAD_X, rect.min.y),
            pos2(rect.max.x - ENTRY_PAD_X - tag_width, rect.max.y),
        );
        paint_truncated(
            ui,
            name_rect,
            preset.name,
            FontId::proportional(TextKind::Value.size()),
            colour,
            Align::Min,
        );
    }
    let tooltip = if tags.is_empty() {
        format!("{} · {}", preset.name, preset.category.label())
    } else {
        format!(
            "{} · {} · {}",
            preset.name,
            preset.category.label(),
            tags.join(", ")
        )
    };
    response
        .on_hover_text(format!("{tooltip}\nTags: {}", preset.tags.join(", ")))
        .clicked()
}

/// Paints `text` in `font` and `colour` inside `rect` (vertically centred, `align`ed
/// horizontally), cut to the rect's width with an ellipsis.
fn paint_truncated(ui: &Ui, rect: Rect, text: &str, font: FontId, colour: Color32, align: Align) {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font,
            color: colour,
            ..TextFormat::default()
        },
    );
    job.wrap.max_width = rect.width().max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    let anchor = match align {
        Align::Min => Align2::LEFT_CENTER.anchor_size(rect.left_center(), galley.size()),
        Align::Center => Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size()),
        Align::Max => Align2::RIGHT_CENTER.anchor_size(rect.right_center(), galley.size()),
    };
    ui.painter().galley(anchor.min, galley, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_items_round_trip_and_labels_are_short() {
        assert_eq!(CATEGORY_LABELS.len(), Category::ALL.len() + 1);
        assert_eq!(category_at(0), None);
        assert_eq!(category_item(None), 0);
        for (i, category) in Category::ALL.iter().enumerate() {
            assert_eq!(category_at(i + 1), Some(*category));
            assert_eq!(category_item(Some(*category)), i + 1);
            assert!(
                CATEGORY_LABELS[i + 1].len() <= 8,
                "{} is long",
                CATEGORY_LABELS[i + 1]
            );
        }
        assert_eq!(category_at(CATEGORY_LABELS.len()), None);
        assert_eq!(head_name("Lofi Piano"), "Preset: Lofi Piano");
        assert_eq!(count_text(12, 99), "12 of 99");
    }

    #[test]
    fn columns_fill_the_width_without_overflowing() {
        assert_eq!(columns_for(100.0), (1, 100.0));
        assert_eq!(columns_for(COLUMN_MIN_WIDTH), (1, COLUMN_MIN_WIDTH));
        let (columns, width) = columns_for(2.0 * COLUMN_MIN_WIDTH + COLUMN_GAP);
        assert_eq!(columns, 2);
        assert_eq!(width, COLUMN_MIN_WIDTH);
        let (columns, width) = columns_for(700.0);
        assert_eq!(columns, 3);
        assert!(width >= COLUMN_MIN_WIDTH);
        assert!((width * 3.0 + COLUMN_GAP * 2.0 - 700.0).abs() < 1e-3);
    }

    #[test]
    fn entry_tags_mark_vinyl_drones_and_the_category_in_the_all_view() {
        let dusty = presets::index_of("Dusty Rhodes").unwrap();
        assert_eq!(entry_tags(dusty, true), vec![VINYL_TAG, "Lofi Keys"]);
        assert_eq!(entry_tags(dusty, false), vec![VINYL_TAG]);
        let echo = presets::index_of("Dub Echo").unwrap();
        assert!(entry_tags(echo, false).contains(&DRONE_TAG));
        let crackle = presets::index_of("Vinyl Crackle").unwrap();
        assert_eq!(entry_tags(crackle, false), vec![VINYL_TAG, DRONE_TAG]);
        assert_eq!(entry_tags(0, false), Vec::<&str>::new(), "Bass has no tag");
        assert_eq!(entry_tags(0, true), vec!["EDM Bass"]);
        assert!(entry_tags(presets::count(), true).is_empty());
    }
}
