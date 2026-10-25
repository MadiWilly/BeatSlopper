//! The app shell pieces: windows with headers, toolbars, tab strips, the hint bar, the drawer
//! handle, list rows, menus / dropdowns, tooltips with keycaps and always-visible scroll areas
//! (`design/round3/WINNER.md` section 4 "Window", "Tabs", "List rows", "Dropdown",
//! "Tooltips", "Scrollbars").

use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, Frame, Id, Layout, Rect, Response, Role,
    ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::button::{FaceState, ICON_STROKE, Icon, control_painter, paint_icon};
use crate::chrome::{keycap_size, paint_keycap};
use crate::flat::{self, flat_rect_shape, paint_control, paint_focus_ring, paint_shadow_floating};
use crate::text::{self, TextKind};

/// Window header height (28), window toolbar height (36), app toolbar height (40), tab strip
/// (28), list row (28), menu item (28), tooltip (26), drawer handle (4, hit 8).
pub const WINDOW_HEADER: f32 = 28.0;
pub const WINDOW_TOOLBAR: f32 = 36.0;
pub const APP_TOOLBAR: f32 = 40.0;
pub const TAB_STRIP: f32 = 28.0;
pub const LIST_ROW: f32 = 28.0;
pub const MENU_ITEM: f32 = 28.0;
pub const MENU_MIN_WIDTH: f32 = 180.0;
pub const TOOLTIP_HEIGHT: f32 = 26.0;
pub const DRAWER_HANDLE: f32 = 4.0;
pub const DRAWER_HANDLE_HIT: f32 = 8.0;

// ---------------------------------------------------------------------------------------------
// Window.

/// A window: `panel` fill, 1 px `border`, radius 4, clipped; a 28 h `panel_2` header (padding
/// 0 4 0 8, gap 8) with `title` in title type, a `border` hairline below, and `right` laid out
/// right-to-left in the header (keycap, collapse / close as [`crate::header_button`]s). `body`
/// fills the rest of the available rect, clipped (scroll areas go inside it). Returns the
/// body's result.
pub fn window_frame<R>(
    ui: &mut Ui,
    theme: &Theme,
    title: &str,
    right: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    let size = ui.available_size_before_wrap();
    window_frame_sized(ui, theme, size, title, None, right, body)
}

/// [`window_frame`] of an explicit `size` with an optional sub-title (12 `text_2`, after the
/// title). The window is `size`; a `size.y` of exactly [`WINDOW_HEADER`] draws the collapsed
/// header only (no bottom hairline).
pub fn window_frame_sized<R>(
    ui: &mut Ui,
    theme: &Theme,
    size: Vec2,
    title: &str,
    sub: Option<&str>,
    right: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    let (rect, _response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter().clone();
    let collapsed = rect.height() <= WINDOW_HEADER + 0.5;
    painter.add(flat_rect_shape(
        rect,
        theme.radius_control,
        theme.panel,
        theme.border,
    ));
    let header = Rect::from_min_size(
        rect.min,
        vec2(rect.width(), WINDOW_HEADER.min(rect.height())),
    );
    let r = theme.radius_control as u8;
    painter.rect_filled(
        header.shrink(1.0),
        CornerRadius {
            nw: r,
            ne: r,
            sw: if collapsed { r } else { 0 },
            se: if collapsed { r } else { 0 },
        },
        theme.panel_2,
    );
    if !collapsed {
        flat::paint_bottom_hairline(&painter, header, theme);
    }
    // Title and sub-title.
    let title_rect = text::paint_text(
        &painter,
        pos2(header.min.x + 8.0, header.center().y),
        Align2::LEFT_CENTER,
        title,
        TextKind::Title,
        theme.text,
    );
    if let Some(sub) = sub {
        let mut format = TextKind::Label.format(theme.text_2);
        format.extra_letter_spacing = 0.0;
        let galley = painter.layout_job(egui::epaint::text::LayoutJob::single_section(
            sub.to_owned(),
            format,
        ));
        painter.galley(
            pos2(
                title_rect.max.x + 8.0,
                header.center().y - galley.size().y * 0.5,
            ),
            galley,
            theme.text_2,
        );
    }
    // Right slot.
    let right_rect = Rect::from_min_max(
        pos2(header.min.x + 8.0, header.min.y),
        pos2(header.max.x - 4.0, header.max.y),
    );
    let mut right_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(right_rect)
            .layout(Layout::right_to_left(Align::Center))
            .id_salt((title, "header")),
    );
    right_ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
    right(&mut right_ui);

    // Body.
    let body_rect = Rect::from_min_max(
        pos2(rect.min.x + 1.0, header.max.y),
        pos2(rect.max.x - 1.0, rect.max.y - 1.0),
    );
    let mut body_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(body_rect)
            .layout(Layout::top_down(Align::Min))
            .id_salt((title, "body")),
    );
    body_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
    if collapsed {
        body_ui.disable();
        body_ui.set_invisible();
    }
    body(&mut body_ui)
}

/// A window toolbar row: 36 h, `panel_2`, padding 0 8, gap 8, `border` hairline below. Fills
/// the available width.
pub fn toolbar<R>(ui: &mut Ui, theme: &Theme, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    bar(
        ui,
        theme,
        WINDOW_TOOLBAR,
        theme.panel_2,
        8.0,
        8.0,
        true,
        0.0,
        add_contents,
    )
}

/// The top app toolbar: 40 h, `panel`, 1 px `border`, radius 4, padding 0 6, gap 5.
pub fn app_toolbar<R>(ui: &mut Ui, theme: &Theme, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    bar(
        ui,
        theme,
        APP_TOOLBAR,
        theme.panel,
        6.0,
        5.0,
        false,
        theme.radius_control,
        add_contents,
    )
}

#[allow(clippy::too_many_arguments)]
fn bar<R>(
    ui: &mut Ui,
    theme: &Theme,
    height: f32,
    fill: Color32,
    padding: f32,
    gap: f32,
    hairline: bool,
    radius: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if hairline {
        ui.painter().rect_filled(rect, 0.0, fill);
        flat::paint_bottom_hairline(ui.painter(), rect, theme);
    } else {
        ui.painter()
            .add(flat_rect_shape(rect, radius, fill, theme.border));
    }
    let inner = Rect::from_min_max(
        pos2(rect.min.x + padding, rect.min.y),
        pos2(
            rect.max.x - padding,
            rect.max.y - if hairline { 1.0 } else { 0.0 },
        ),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing = vec2(gap, 0.0);
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    add_contents(&mut child)
}

// ---------------------------------------------------------------------------------------------
// Tabs.

/// Horizontal padding of a tab item (12; 8 at narrow).
pub const TAB_PADDING: f32 = 12.0;

/// A tab strip: 28 h, `panel_2`, `border` hairline below, fills the available width. Items
/// are `(label, keycap)`: button type, `text_2`, hover `text`, the active one `text` with a
/// 2 px accent underline inset 6; the keycap sits 6 after the text. A click selects (Left /
/// Right move the selection while a tab has focus); `changed()` when the selection moved.
/// Items are accessible as tabs named by their label.
pub fn tab_strip(
    ui: &mut Ui,
    theme: &Theme,
    selected: &mut usize,
    items: &[(&str, Option<&str>)],
) -> Response {
    tab_strip_padded(ui, theme, selected, items, TAB_PADDING)
}

/// [`tab_strip`] with an explicit item padding (8 at narrow).
pub fn tab_strip_padded(
    ui: &mut Ui,
    theme: &Theme,
    selected: &mut usize,
    items: &[(&str, Option<&str>)],
    padding: f32,
) -> Response {
    let before = *selected;
    let width = ui.available_width();
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, TAB_STRIP), Sense::hover());
    response.widget_info(|| {
        let names: Vec<&str> = items.iter().map(|(l, _)| *l).collect();
        WidgetInfo::labeled(Role::TabList, ui.is_enabled(), names.join(" / "))
    });
    let painter = control_painter(ui);
    if ui.is_rect_visible(rect) {
        painter.rect_filled(rect, 0.0, theme.panel_2);
        flat::paint_bottom_hairline(&painter, rect, theme);
    }
    if items.is_empty() {
        return response;
    }
    *selected = (*selected).min(items.len() - 1);

    let mut x = rect.min.x;
    let mut item_rects = Vec::with_capacity(items.len());
    for (label, key) in items {
        let text_w = text::layout(&painter, label, TextKind::Button, Color32::PLACEHOLDER)
            .size()
            .x;
        let key_w = key.map_or(0.0, |k| 6.0 + keycap_size(&painter, k).x);
        let w = padding * 2.0 + text_w + key_w;
        item_rects.push(Rect::from_min_size(pos2(x, rect.min.y), vec2(w, TAB_STRIP)));
        x += w;
    }
    let item_responses: Vec<Response> = item_rects
        .iter()
        .enumerate()
        .map(|(i, r)| ui.interact(*r, response.id.with(i), Sense::click()))
        .collect();
    let mut focused = None;
    for (i, r) in item_responses.iter().enumerate() {
        if r.clicked() {
            *selected = i;
            r.request_focus();
        }
        if r.has_focus() {
            focused = Some(i);
        }
    }
    if let Some(i) = focused {
        let delta = ui.input_mut(|input| {
            input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) as i64
                - input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) as i64
        });
        if delta != 0 {
            let next = (i as i64 + delta).clamp(0, items.len() as i64 - 1) as usize;
            *selected = next;
            if next != i {
                ui.memory_mut(|m| m.request_focus(item_responses[next].id));
            }
        }
    }
    if ui.is_rect_visible(rect) {
        for (i, ((label, key), item_rect)) in items.iter().zip(&item_rects).enumerate() {
            let active = i == *selected;
            let colour = if active || item_responses[i].hovered() {
                theme.text
            } else {
                theme.text_2
            };
            let text_rect = text::paint_text(
                &painter,
                pos2(item_rect.min.x + padding, item_rect.center().y),
                Align2::LEFT_CENTER,
                label,
                TextKind::Button,
                colour,
            );
            if let Some(k) = key {
                paint_keycap(
                    &painter,
                    pos2(text_rect.max.x + 6.0, item_rect.center().y),
                    k,
                    theme,
                );
            }
            if active {
                let underline = Rect::from_min_max(
                    pos2(item_rect.min.x + 6.0, item_rect.max.y - 3.0),
                    pos2(item_rect.max.x - 6.0, item_rect.max.y - 1.0),
                );
                painter.rect_filled(underline, 1.0, theme.accent);
            }
            let state = FaceState::of(&item_responses[i]);
            if state.focused {
                paint_focus_ring(&painter, item_rect.shrink(2.0), 3.0, theme);
            }
        }
    }
    for (i, r) in item_responses.iter().enumerate() {
        r.widget_info(|| {
            WidgetInfo::selected(Role::Tab, ui.is_enabled(), i == *selected, items[i].0)
        });
    }
    if *selected != before {
        response.mark_changed();
    }
    response
}

// ---------------------------------------------------------------------------------------------
// Hint bar, drawer handle, list row.

/// The status / hint line: `(keycap, label)` pairs (12 `text_2` labels, gap 6 after the
/// keycap, 12 between pairs), 28 h, laid out left to right and clipped to the available width.
/// Accessible as a label reading the pairs joined ("Space Play / Stop · Ctrl+L Link").
pub fn hint_bar(ui: &mut Ui, theme: &Theme, hints: &[(&str, &str)]) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let mut x = rect.min.x;
        for (key, label) in hints {
            let cap = paint_keycap(&painter, pos2(x, rect.center().y), key, theme);
            let mut format = TextKind::Label.format(theme.text_2);
            format.extra_letter_spacing = 0.0;
            let galley = painter.layout_job(egui::epaint::text::LayoutJob::single_section(
                (*label).to_owned(),
                format,
            ));
            painter.galley(
                pos2(cap.max.x + 6.0, rect.center().y - galley.size().y * 0.5),
                galley.clone(),
                theme.text_2,
            );
            x = cap.max.x + 6.0 + galley.size().x + 12.0;
        }
    }
    let name = hints
        .iter()
        .map(|(k, l)| format!("{k} {l}"))
        .collect::<Vec<_>>()
        .join(" · ");
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &name));
    response
}

/// The mixer drawer's grab bar: a full-width 4 pt `border` gutter with an 8 pt drag hit
/// (`border_2` while hovered, `accent` while dragged); the vertical-resize cursor. The caller
/// reads `drag_delta()`. Named "Drawer handle".
pub fn drawer_handle(ui: &mut Ui, theme: &Theme) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, DRAWER_HANDLE_HIT), Sense::drag());
    if ui.is_rect_visible(rect) {
        let colour = if response.dragged() {
            theme.accent
        } else if response.hovered() {
            theme.border_2
        } else {
            theme.border
        };
        let bar = Rect::from_center_size(rect.center(), vec2(rect.width(), DRAWER_HANDLE));
        ui.painter().rect_filled(bar, 2.0, colour);
        let grip = Rect::from_center_size(rect.center(), vec2(32.0, 2.0));
        ui.painter().rect_filled(grip, 1.0, theme.text_3);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Splitter, ui.is_enabled(), "Drawer handle"));
    response.on_hover_cursor(CursorIcon::ResizeVertical)
}

/// A list row: 28 h, padding 0 8 0 12, fills the available width; hover `panel_3`; selected
/// `accent_wash` + a 2 px accent bar at the left edge. `add_contents` lays the row out left to
/// right (13 body text, tags, LEDs, 11 `text_3` meta). Senses clicks on the whole row;
/// accessible as a list item (selected state) named by the first label the contents add, or
/// "Row".
pub fn list_row(
    ui: &mut Ui,
    theme: &Theme,
    selected: bool,
    add_contents: impl FnOnce(&mut Ui),
) -> Response {
    list_row_named(ui, theme, selected, "Row", add_contents)
}

/// [`list_row`] with an explicit accessible name.
pub fn list_row_named(
    ui: &mut Ui,
    theme: &Theme,
    selected: bool,
    name: &str,
    add_contents: impl FnOnce(&mut Ui),
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, LIST_ROW), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
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
    }
    let inner = Rect::from_min_max(
        pos2(rect.min.x + 12.0, rect.min.y),
        pos2(rect.max.x - 8.0, rect.max.y),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center))
            .id_salt((response.id, "row")),
    );
    child.spacing_mut().item_spacing = vec2(6.0, 0.0);
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    add_contents(&mut child);
    response.widget_info(|| WidgetInfo::selected(Role::ListItem, ui.is_enabled(), selected, name));
    response
}

// ---------------------------------------------------------------------------------------------
// Menu, dropdown.

/// One entry of a [`menu`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem<'a> {
    /// A clickable entry with an optional keycap, greyed when not `enabled`.
    Entry {
        label: &'a str,
        key: Option<&'a str>,
        enabled: bool,
    },
    /// A 1 px `border` separator with 4 above and below.
    Separator,
}

impl<'a> MenuItem<'a> {
    /// An enabled entry without a keycap.
    pub fn item(label: &'a str) -> Self {
        MenuItem::Entry {
            label,
            key: None,
            enabled: true,
        }
    }

    /// An enabled entry with a keycap.
    pub fn keyed(label: &'a str, key: &'a str) -> Self {
        MenuItem::Entry {
            label,
            key: Some(key),
            enabled: true,
        }
    }

    /// A disabled entry.
    pub fn disabled(label: &'a str, key: Option<&'a str>) -> Self {
        MenuItem::Entry {
            label,
            key,
            enabled: false,
        }
    }

    fn height(&self) -> f32 {
        match self {
            MenuItem::Entry { .. } => MENU_ITEM,
            MenuItem::Separator => 9.0,
        }
    }
}

/// The size a [`menu`] of `items` takes.
pub fn menu_size(painter: &egui::Painter, items: &[MenuItem<'_>]) -> Vec2 {
    let mut width: f32 = MENU_MIN_WIDTH;
    let mut height = 8.0;
    for item in items {
        height += item.height();
        if let MenuItem::Entry { label, key, .. } = item {
            let w = text::layout(painter, label, TextKind::Body, Color32::PLACEHOLDER)
                .size()
                .x;
            let k = key.map_or(0.0, |k| 8.0 + keycap_size(painter, k).x);
            width = width.max(20.0 + w + k);
        }
    }
    vec2(width, height)
}

/// The one menu style: min 180 w, `panel_2`, `border_2`, radius 4, the floating shadow,
/// padding 4 0; items 28 h, padding 0 10, 13 body text, keycap right-aligned; hover `panel_4`;
/// disabled `text_3`; separators 1 px `border` with 4 above and below. Paints where it is
/// called (see [`dropdown`] for the floating version). Returns the index into `items` of the
/// entry clicked this frame. Entries are accessible as menu items named by their label.
pub fn menu(ui: &mut Ui, theme: &Theme, items: &[MenuItem<'_>]) -> Option<usize> {
    let size = menu_size(ui.painter(), items);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Menu, ui.is_enabled(), "Menu"));
    let painter = ui.painter().clone();
    if ui.is_rect_visible(rect) {
        paint_shadow_floating(&painter, rect, theme.radius_control, theme);
        paint_control(
            &painter,
            rect,
            theme.radius_control,
            theme.panel_2,
            theme.border_2,
        );
    }
    let mut picked = None;
    let mut y = rect.min.y + 4.0;
    for (i, item) in items.iter().enumerate() {
        let item_rect = Rect::from_min_size(
            pos2(rect.min.x + 1.0, y),
            vec2(rect.width() - 2.0, item.height()),
        );
        y += item.height();
        match item {
            MenuItem::Separator => {
                painter.hline(
                    (item_rect.min.x + 1.0)..=(item_rect.max.x - 1.0),
                    item_rect.center().y,
                    Stroke::new(1.0, theme.border),
                );
            }
            MenuItem::Entry {
                label,
                key,
                enabled,
            } => {
                let r = ui.interact(item_rect, response.id.with(i), Sense::click());
                let r = if *enabled {
                    r
                } else {
                    r.on_hover_cursor(CursorIcon::NotAllowed)
                };
                if *enabled && r.clicked() {
                    picked = Some(i);
                }
                if ui.is_rect_visible(item_rect) {
                    if *enabled && r.hovered() {
                        painter.rect_filled(item_rect, 0.0, theme.panel_4);
                    }
                    let colour = if *enabled { theme.text } else { theme.text_3 };
                    text::paint_text(
                        &painter,
                        pos2(item_rect.min.x + 9.0, item_rect.center().y),
                        Align2::LEFT_CENTER,
                        label,
                        TextKind::Body,
                        colour,
                    );
                    if let Some(k) = key {
                        let w = keycap_size(&painter, k).x;
                        paint_keycap(
                            &painter,
                            pos2(item_rect.max.x - 9.0 - w, item_rect.center().y),
                            k,
                            theme,
                        );
                    }
                }
                let name = (*label).to_owned();
                r.widget_info(|| WidgetInfo::labeled(Role::MenuItem, *enabled, &name));
            }
        }
    }
    picked
}

/// Dropdown height (28) and small height (22).
pub const DROPDOWN_HEIGHT: f32 = 28.0;
pub const DROPDOWN_SMALL: f32 = 22.0;

/// A dropdown: a 28 h field (`panel_2`, `border_2`, radius 4, padding 0 6 0 8, 13 value text,
/// a 6 x 6 `text_2` chevron in a 10 box) showing `value`; a click opens a [`menu`] of
/// `items` below it (the popup closes on a click). `width` is the control's width; `small`
/// draws the 22 h variant (12 pt text). `colour` draws a 3 px track-colour bar inside the
/// left edge (the track variant). Returns the index picked this frame. The field is a button
/// named `name` whose accessible value is `value`.
#[allow(clippy::too_many_arguments)]
pub fn dropdown(
    ui: &mut Ui,
    theme: &Theme,
    name: &str,
    value: &str,
    items: &[&str],
    width: f32,
    small: bool,
    colour: Option<Color32>,
) -> Option<usize> {
    let height = if small {
        DROPDOWN_SMALL
    } else {
        DROPDOWN_HEIGHT
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = control_painter(ui);
        let state = FaceState::of(&response);
        let fill = if state.hovered || state.pressed {
            theme.panel_3
        } else {
            theme.panel_2
        };
        paint_control(&painter, rect, 4.0, fill, theme.border_2);
        if state.focused {
            paint_focus_ring(&painter, rect, 4.0, theme);
        }
        let mut x = rect.min.x + if small { 6.0 } else { 8.0 };
        if let Some(c) = colour {
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(rect.min.x + 1.0, rect.min.y + 1.0),
                    pos2(rect.min.x + 4.0, rect.max.y - 1.0),
                ),
                CornerRadius {
                    nw: 3,
                    sw: 3,
                    ne: 0,
                    se: 0,
                },
                c,
            );
            x += 4.0;
        }
        let chevron = Rect::from_center_size(
            pos2(rect.max.x - 6.0 - 5.0, rect.center().y),
            vec2(10.0, 10.0),
        );
        let font = egui::FontId::proportional(if small { 12.0 } else { 13.0 });
        let max_w = (chevron.min.x - 6.0 - x).max(0.0);
        let shown = crate::chrome::ellipsise(&painter, value, TextKind::Value, max_w);
        let galley = painter.layout_job(egui::epaint::text::LayoutJob::single_section(
            shown,
            egui::epaint::text::TextFormat {
                font_id: font,
                color: theme.text,
                ..Default::default()
            },
        ));
        painter.galley(
            pos2(x, rect.center().y - galley.size().y * 0.5),
            galley,
            theme.text,
        );
        paint_icon(
            &painter,
            Icon::ChevronDown,
            chevron.shrink(2.0),
            Stroke::new(ICON_STROKE, theme.text_2),
        );
    }
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::ComboBox, ui.is_enabled(), name);
        info.current_text_value = Some(value.to_owned());
        info
    });
    let entries: Vec<MenuItem<'_>> = items.iter().map(|i| MenuItem::item(i)).collect();
    egui::Popup::menu(&response)
        .frame(Frame::NONE)
        .gap(4.0)
        .show(|ui| menu(ui, theme, &entries))
        .and_then(|r| r.inner)
}

/// Opens a [`menu`] of `items` under `anchor` when it was clicked (the project / overflow
/// menus); closes on a click. Returns the index picked this frame.
pub fn menu_popup(
    ui: &Ui,
    theme: &Theme,
    anchor: &Response,
    items: &[MenuItem<'_>],
) -> Option<usize> {
    let _ = ui;
    egui::Popup::menu(anchor)
        .frame(Frame::NONE)
        .gap(4.0)
        .show(|ui| menu(ui, theme, items))
        .and_then(|r| r.inner)
}

// ---------------------------------------------------------------------------------------------
// Tooltip.

/// A tooltip with a keycap: `panel_4` face, `border_2`, radius 4, 26 h, padding 0 8, 12 / 400
/// `text` plus the keycap after an 8 gap, the floating shadow (from `Theme::apply`), after
/// the 300 ms delay. `key` empty draws no keycap. The tooltip's accessible text is
/// `"text (key)"`.
pub fn tooltip_with_keycap(response: Response, theme: &Theme, text: &str, key: &str) -> Response {
    let theme = theme.clone();
    let text = text.to_owned();
    let key = key.to_owned();
    response.on_hover_ui(move |ui| {
        let painter = ui.painter().clone();
        let where_face = painter.add(egui::Shape::Noop);
        let mut format = TextKind::Value.format(theme.text);
        format.font_id = egui::FontId::proportional(12.0);
        let galley = painter.layout_job(egui::epaint::text::LayoutJob::single_section(
            text.clone(),
            format,
        ));
        let key_w = if key.is_empty() {
            0.0
        } else {
            8.0 + keycap_size(&painter, &key).x
        };
        let size = vec2(galley.size().x + key_w, TOOLTIP_HEIGHT - 2.0);
        let (rect, r) = ui.allocate_exact_size(size, Sense::hover());
        let name = if key.is_empty() {
            text.clone()
        } else {
            format!("{text} ({key})")
        };
        r.widget_info(|| WidgetInfo::labeled(Role::Label, true, &name));
        painter.galley(
            pos2(rect.min.x, rect.center().y - galley.size().y * 0.5),
            galley.clone(),
            theme.text,
        );
        if !key.is_empty() {
            paint_keycap(
                &painter,
                pos2(rect.min.x + galley.size().x + 8.0, rect.center().y),
                &key,
                &theme,
            );
        }
        // Cover egui's popup frame (panel fill, border) with the Deck tooltip face.
        let margin = ui.style().spacing.window_margin;
        let face = Rect::from_min_max(
            pos2(
                rect.min.x - margin.left as f32,
                rect.min.y - margin.top as f32,
            ),
            pos2(
                rect.max.x + margin.right as f32,
                rect.max.y + margin.bottom as f32,
            ),
        );
        painter.set(
            where_face,
            flat_rect_shape(face, theme.radius_control, theme.panel_4, theme.border_2),
        );
    })
}

// ---------------------------------------------------------------------------------------------
// Scroll areas.

/// A vertical scroll area whose scrollbar is always drawn (Deck: every scroller shows its
/// bar, with a full-length thumb when nothing scrolls).
pub fn scroll_vertical(id_salt: impl std::hash::Hash + std::fmt::Debug) -> ScrollArea {
    ScrollArea::vertical()
        .id_salt(id_salt)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
}

/// A horizontal scroll area whose scrollbar is always drawn.
pub fn scroll_horizontal(id_salt: impl std::hash::Hash + std::fmt::Debug) -> ScrollArea {
    ScrollArea::horizontal()
        .id_salt(id_salt)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
}

/// A two-way scroll area whose scrollbars are always drawn.
pub fn scroll_both(id_salt: impl std::hash::Hash + std::fmt::Debug) -> ScrollArea {
    ScrollArea::both()
        .id_salt(id_salt)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
}

/// A group: a 24 h head (`title` in title type, `head_right` laid out right-to-left) over
/// `body`, in a `panel` frame with `border`, radius 4, padding 6 8 8, gap 6 (the SYNTH & FX
/// groups). Fills the available width.
pub fn group<R>(
    ui: &mut Ui,
    theme: &Theme,
    title: &str,
    head_right: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    let where_frame = ui.painter().add(egui::Shape::Noop);
    let outer_max = ui.available_rect_before_wrap();
    let content = Rect::from_min_max(
        pos2(outer_max.min.x + 8.0, outer_max.min.y + 6.0),
        pos2(outer_max.max.x - 8.0, outer_max.max.y - 8.0),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(content)
            .layout(Layout::top_down(Align::Min))
            .id_salt((title, "group")),
    );
    child.spacing_mut().item_spacing = vec2(6.0, 6.0);
    let head_w = child.available_width();
    let (head, head_response) = child.allocate_exact_size(vec2(head_w, 24.0), Sense::hover());
    head_response.widget_info(|| WidgetInfo::labeled(Role::Label, true, title));
    text::paint_text(
        child.painter(),
        head.left_center(),
        Align2::LEFT_CENTER,
        title,
        TextKind::Title,
        theme.text,
    );
    let mut right_ui = child.new_child(
        UiBuilder::new()
            .max_rect(head)
            .layout(Layout::right_to_left(Align::Center))
            .id_salt((title, "group_head")),
    );
    right_ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
    head_right(&mut right_ui);
    let out = body(&mut child);
    let outer = Rect::from_min_max(
        pos2(outer_max.min.x, outer_max.min.y),
        pos2(outer_max.max.x, child.min_rect().max.y + 8.0),
    );
    ui.painter().set(
        where_frame,
        flat_rect_shape(outer, theme.radius_control, theme.panel, theme.border),
    );
    ui.allocate_rect(outer, Sense::hover());
    out
}

/// A panel-2 well (scope, EQ curve): `panel_2`, `border_2`, radius 4. Returns the inner rect
/// (1 px inside the border) for the caller to draw into.
pub fn well(ui: &mut Ui, theme: &Theme, size: Vec2) -> (Rect, Response) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_control(
            ui.painter(),
            rect,
            theme.radius_control,
            theme.panel_2,
            theme.border_2,
        );
    }
    (rect.shrink(1.0), response)
}

/// The id a [`dropdown`] / [`menu_popup`] of `anchor` opens under (for tests and callers that
/// close it).
pub fn popup_id_of(anchor: &Response) -> Id {
    egui::Popup::default_response_id(anchor)
}

/// Paints the 1 px accent playhead line at `x` over `y_range` (the roll, the timeline).
pub fn paint_playhead(
    painter: &egui::Painter,
    x: f32,
    y_range: std::ops::RangeInclusive<f32>,
    theme: &Theme,
) {
    painter.vline(x, y_range, Stroke::new(1.0, theme.accent));
}

/// Paints a `border` hairline around `rect` (a bare frame).
pub fn paint_hairline_frame(painter: &egui::Painter, rect: Rect, radius: f32, theme: &Theme) {
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, theme.border),
        StrokeKind::Inside,
    );
}
