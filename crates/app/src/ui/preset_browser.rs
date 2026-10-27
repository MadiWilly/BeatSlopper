//! The preset browser (M9; the BROWSER column's PRESETS section since M12, WINNER section 5
//! "BROWSER column") over the unified preset list (`crate::presets`: the eight classics,
//! then the 1 091-preset library, the 91 hand-made M9 presets and the 1 000 of the Sound
//! Pack).
//!
//! - [`current_row`]: the section's always-visible second line: `<` (previous), the current
//!   preset's name of the selected track (a button named [`head_name`]; a click opens the
//!   section), the MODIFIED LED ([`MODIFIED_LABEL`]) and `>` (next). Previous / next step
//!   within the category filter and wrap ([`AppState::step_preset`]).
//! - [`show`]: the section body while [`AppState::preset_browser_open`]: the category chips
//!   ([`CATEGORY_LABELS`]: `All` + the eighteen categories with short labels, wrapping), the
//!   match count ([`count_text`]) and the scrolling row list of matching presets (the
//!   selected one on the accent wash with the accent bar; `vinyl` / `drone` tags where the
//!   preset hisses without a note or never dies away; in the `All` view the category as a
//!   plain tag). A click applies the preset to the selected track. Escape (also from inside
//!   the search field) and the CLOSE button collapse the section; the filter and the search
//!   text are kept.
//! - [`search_field`]: the column toolbar's search field (placeholder [`SEARCH_HINT`]; a
//!   `neu::text_field`, so the shortcuts and the piano stay quiet while it has the keyboard).
//!   Typing opens the section.

use egui::{Align2, Key, Rect, Role, Sense, Ui, WidgetInfo, pos2, vec2};
use neu::{ButtonKind, ButtonSize, Icon, LedKind, TagKind, TextKind, Theme};

use crate::app::AppState;
use crate::presets::{self, Category};

/// Accessible name of the PRESETS section head (a click toggles the section).
pub const BROWSE_LABEL: &str = "Presets";
/// Accessible name of the CLOSE button beside the search field while the section is open.
pub const CLOSE_LABEL: &str = "Close";
/// Accessible name of the previous-preset button.
pub const PREV_LABEL: &str = "Previous preset";
/// Accessible name of the next-preset button.
pub const NEXT_LABEL: &str = "Next preset";
/// Placeholder of the search field (its accessible name too).
pub const SEARCH_HINT: &str = "search name or tag";
/// The category row's first item: no category filter.
pub const ALL_LABEL: &str = "All";
/// The category chips: [`ALL_LABEL`] then the eighteen categories in `Category::ALL` order
/// (the eight M9 ones, then the ten of the Sound Pack), shortened so three or four fit a
/// 208 pt column row (the full labels are the tooltips).
pub const CATEGORY_LABELS: [&str; 19] = [
    ALL_LABEL, "Lo Keys", "Lo Gtr", "Lo Pads", "Leads", "EDM Bass", "Plucks", "EDM Pads", "Risers",
    "Pianos", "Organs", "Brass", "Strings", "Bells", "Guitars", "Syn Lead", "Syn Bass", "Atmos",
    "Arps",
];
/// Tag shown on presets whose lo-fi insert hisses even with no note playing.
pub const VINYL_TAG: &str = "vinyl";
/// Tag shown on presets that keep sounding after release.
pub const DRONE_TAG: &str = "drone";
/// Shown in the list when nothing matches.
pub const NO_MATCH: &str = "No preset matches";
/// Longest search text accepted.
pub const SEARCH_CHAR_LIMIT: usize = 32;
/// Label of the MODIFIED LED.
pub const MODIFIED_LABEL: &str = "Modified";

/// Height of one list row.
pub const ROW_HEIGHT: f32 = 28.0;
/// Height of the current-preset row.
pub const CURRENT_ROW: f32 = 36.0;
/// Horizontal padding inside a list row (WINNER list rows: padding 0 8 0 12).
const ROW_PAD_LEFT: f32 = 12.0;
const ROW_PAD_RIGHT: f32 = 8.0;
/// Gap between the name and the tags, and between tags.
const TAG_GAP: f32 = 6.0;
/// Padding around the chips (WINNER: padding 4 8 8, gap 4).
const CHIP_PAD: f32 = 8.0;
const CHIP_GAP: f32 = 4.0;

/// Accessible name of the current preset's name button: `"Preset: Lofi Piano"`.
#[must_use]
pub fn head_name(preset: &str) -> String {
    format!("Preset: {preset}")
}

/// The category the chip `index` of [`CATEGORY_LABELS`] stands for (`None` = All).
#[must_use]
pub fn category_at(index: usize) -> Option<Category> {
    index
        .checked_sub(1)
        .and_then(|i| Category::ALL.get(i).copied())
}

/// The chip index of `category` (`None` = All).
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

// ---------------------------------------------------------------------------------------------
// Search field and the current-preset row.

/// The column toolbar's search field, `width` wide. Takes the keyboard when
/// [`AppState::preset_search_focus`] asks; typing opens the section. Escape inside the field
/// (egui drops the focus) closes the section.
pub fn search_field(ui: &mut Ui, state: &mut AppState, theme: &Theme, width: f32) {
    let response = neu::text_field(
        ui,
        theme,
        &mut state.preset_query,
        SEARCH_HINT,
        Some(Icon::Search),
        width,
    );
    if state.preset_query.chars().count() > SEARCH_CHAR_LIMIT {
        state.preset_query = state.preset_query.chars().take(SEARCH_CHAR_LIMIT).collect();
    }
    if state.preset_search_focus {
        response.request_focus();
        state.preset_search_focus = false;
    }
    if response.changed() || response.gained_focus() {
        state.preset_browser_open = true;
    }
    response.on_hover_text("Search by name or tag (Ctrl+Shift+P)");
}

/// `<` · the current preset's name (a button, [`head_name`]) · MODIFIED LED · `>`, laid out
/// across `width`.
pub fn current_row(ui: &mut Ui, state: &mut AppState, theme: &Theme, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, CURRENT_ROW), Sense::hover());
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(CHIP_PAD, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center))
            .id_salt("preset_current_row"),
    );
    row.spacing_mut().item_spacing = vec2(TAG_GAP, 0.0);
    let ui = &mut row;
    let prev = neu::header_button(ui, theme, Icon::ChevronLeft);
    prev.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), PREV_LABEL));
    if prev
        .on_hover_text("Previous preset in the category (Ctrl+PageUp)")
        .clicked()
    {
        state.step_preset(-1);
    }
    // The name takes what the buttons and the LED leave.
    let right = 24.0 + TAG_GAP + neu::LED_SIZE + TAG_GAP;
    let name_width = (ui.available_width() - right).max(24.0);
    let name = state.preset_name();
    let (name_rect, response) =
        ui.allocate_exact_size(vec2(name_width, CURRENT_ROW), Sense::click());
    let accessible = head_name(name);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &accessible));
    if ui.is_rect_visible(name_rect) {
        let colour = if response.hovered() {
            theme.accent
        } else {
            theme.text
        };
        let shown = neu::ellipsise(ui.painter(), name, TextKind::Value, name_rect.width());
        let mut format = TextKind::Value.format(colour);
        format.font_id = egui::FontId::proportional(12.0);
        let galley = ui
            .painter()
            .layout_job(egui::epaint::text::LayoutJob::single_section(shown, format));
        ui.painter().galley(
            pos2(
                name_rect.min.x,
                name_rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            colour,
        );
    }
    let category = presets::get(state.preset_index()).map_or("", |p| p.category.label());
    if response
        .on_hover_text(format!("{name} · {category}\nClick to browse the library"))
        .clicked()
    {
        state.open_preset_browser(false);
    }
    neu::led_named(
        ui,
        theme,
        state.preset_modified(),
        LedKind::Accent,
        MODIFIED_LABEL,
    )
    .on_hover_text("Lit once a knob moved after the preset was applied");
    let next = neu::header_button(ui, theme, Icon::ChevronRight);
    next.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), NEXT_LABEL));
    if next
        .on_hover_text("Next preset in the category (Ctrl+PageDown)")
        .clicked()
    {
        state.step_preset(1);
    }
}

// ---------------------------------------------------------------------------------------------
// The section body.

/// The section body: category chips, the match count, the list; `height` is what the list
/// may take (it scrolls). Escape closes it.
pub fn show(ui: &mut Ui, state: &mut AppState, theme: &Theme, height: f32) {
    // Escape closes, also from inside the search field (egui drops the field's focus on
    // Escape and the keyboard handler skips Escape while a field has the keyboard).
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        state.close_preset_browser();
        return;
    }
    let top = ui.cursor().min.y;
    category_chips(ui, state, theme);
    let matching = state.preset_matches().len();
    ui.horizontal(|ui| {
        ui.add_space(CHIP_PAD);
        neu::text_widget(
            ui,
            &count_text(matching, presets::count()),
            TextKind::Small,
            theme.text_3,
        )
        .on_hover_text("Matching presets of the category");
    });
    ui.add_space(4.0);
    let used = ui.cursor().min.y - top;
    list(ui, state, theme, (height - used).max(ROW_HEIGHT));
}

/// The category chips, wrapping: small toggle buttons named by [`CATEGORY_LABELS`].
fn category_chips(ui: &mut Ui, state: &mut AppState, theme: &Theme) {
    let before = category_item(state.preset_category);
    let mut selected = before;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 0.0),
        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
        |ui| {
            ui.add_space(CHIP_PAD);
            ui.spacing_mut().item_spacing = vec2(CHIP_GAP, CHIP_GAP);
            for (i, label) in CATEGORY_LABELS.iter().enumerate() {
                let full = category_at(i).map_or("Every category", Category::label);
                if neu::button_kind(
                    ui,
                    theme,
                    label,
                    ButtonKind::Toggle(i == selected),
                    ButtonSize::Small,
                )
                .on_hover_text(format!("{full}: filters the list and the < / > buttons"))
                .clicked()
                {
                    selected = i;
                }
            }
        },
    );
    ui.add_space(CHIP_PAD);
    if selected != before {
        state.preset_category = category_at(selected);
    }
}

/// The scrolling list of matching presets, `height` tall.
fn list(ui: &mut Ui, state: &mut AppState, theme: &Theme, height: f32) {
    let matches = state.preset_matches();
    let current = state.preset_index();
    let show_category = state.preset_category.is_none();
    let mut clicked = None;
    neu::scroll_vertical("preset_browser_list")
        .max_height(height)
        .show(ui, |ui| {
            ui.set_min_height(height);
            ui.spacing_mut().item_spacing.y = 0.0;
            if matches.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(ROW_PAD_LEFT);
                    neu::text_widget(ui, NO_MATCH, TextKind::Body, theme.text_2);
                });
            }
            let width = ui.available_width();
            for &index in &matches {
                if entry(ui, theme, index, index == current, show_category, width) {
                    clicked = Some(index);
                }
            }
        });
    if let Some(index) = clicked {
        state.apply_preset(index);
    }
}

/// One list row: the name (left, ellipsised) and its tags (right); the selected one on the
/// accent wash with the 2 px accent bar. Accessible as a button named after the preset
/// (`toggled` = selected). Returns `true` when clicked.
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
    // The list holds the whole library (1 000+ rows): rows off screen only reserve their
    // space (and stay in the accessibility tree); their tags and tooltip are built only when
    // they show.
    if ui.is_rect_visible(rect) {
        let tags = entry_tags(index, show_category);
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 0.0, theme.accent_wash);
            painter.rect_filled(
                Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
                0.0,
                theme.accent,
            );
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, theme.panel_3);
        }
        // Tags, right-aligned.
        let mut right = rect.max.x - ROW_PAD_RIGHT;
        for tag in tags.iter().rev() {
            let kind = match *tag {
                VINYL_TAG => TagKind::Vinyl,
                DRONE_TAG => TagKind::Drone,
                _ => TagKind::Plain,
            };
            let galley = neu::text::layout(painter, tag, TextKind::Small, theme.text_2);
            let w = galley.size().x + 10.0;
            let tag_rect = Rect::from_min_max(
                pos2(right - w, rect.center().y - neu::TAG_HEIGHT * 0.5),
                pos2(right, rect.center().y + neu::TAG_HEIGHT * 0.5),
            );
            let colour = kind.colour(theme);
            let border = if kind == TagKind::Plain {
                theme.border_2
            } else {
                neu::with_alpha(colour, 0.5)
            };
            neu::paint_control(painter, tag_rect, 3.0, egui::Color32::TRANSPARENT, border);
            neu::text::paint_galley(
                painter,
                tag_rect.center(),
                Align2::CENTER_CENTER,
                galley,
                TextKind::Small,
                colour,
            );
            right = tag_rect.min.x - TAG_GAP;
        }
        let colour = if selected || response.hovered() {
            theme.text
        } else {
            theme.text_2
        };
        let name_width = (right - rect.min.x - ROW_PAD_LEFT).max(0.0);
        let shown = neu::ellipsise(painter, preset.name, TextKind::Body, name_width);
        neu::text::paint_text(
            painter,
            pos2(rect.min.x + ROW_PAD_LEFT, rect.center().y),
            Align2::LEFT_CENTER,
            &shown,
            TextKind::Body,
            colour,
        );
    }
    response
        .on_hover_ui(|ui| {
            ui.label(tooltip_text(index, show_category));
        })
        .clicked()
}

/// The tooltip of preset `index`'s row: name, category, the shown tags, then every tag.
fn tooltip_text(index: usize, show_category: bool) -> String {
    let Some(preset) = presets::get(index) else {
        return String::new();
    };
    let tags = entry_tags(index, show_category);
    let head = if tags.is_empty() {
        format!("{} · {}", preset.name, preset.category.label())
    } else {
        format!(
            "{} · {} · {}",
            preset.name,
            preset.category.label(),
            tags.join(", ")
        )
    };
    format!("{head}\nTags: {}", preset.tags.join(", "))
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
