//! Segmented selector (`design/round3/WINNER.md` section 4, "Segmented").
//!
//! A `panel_2` box track with a 1 px `border_2`, radius 4, [`SEGMENTED_HEIGHT`] (28) tall
//! with [`SEGMENTED_TRACK_PADDING`] (2) all round, holding one 12 / 500 text item per option
//! ([`SEGMENTED_ITEM_PADDING`] 8 each side, [`SEGMENTED_ITEM_GAP`] 2 between). Items are 24
//! tall (radius 3): `text_2`, `text` on hover; the selected item sits on a `panel_4` chip
//! ([`chip_rect`]) in `text` with a 2 px accent underline inset 4 each side. The chip slides
//! between items over [`SEGMENTED_CHIP_ANIM_SECS`]; with `Style::animation_time` at zero it
//! jumps. The small variant ([`segmented_small`]) is 22 tall with 18 pt items, padding 6,
//! 11 pt text.
//!
//! A click selects; Left / Right move the selection while an item has keyboard focus, and the
//! focused item wears the 2 px accent focus ring. The returned response is the whole track and
//! reports `changed()` only when the selection moved. A track with a single item offers no
//! choice, so it shows no chip (the item reads in `text`).

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, FontId, Key, Modifiers, Rect, Response, Role, Sense, Ui, WidgetInfo, pos2,
    vec2,
};

use crate::Theme;
use crate::button::control_painter;
use crate::flat::{paint_control, paint_focus_ring};

/// Height of the track (28).
pub const SEGMENTED_HEIGHT: f32 = 28.0;
/// Padding inside the track (2, all round).
pub const SEGMENTED_TRACK_PADDING: f32 = 2.0;
/// Horizontal padding of each item (8).
pub const SEGMENTED_ITEM_PADDING: f32 = 8.0;
/// Gap between items (2).
pub const SEGMENTED_ITEM_GAP: f32 = 2.0;
/// Item text size (12).
pub const SEGMENTED_TEXT_SIZE: f32 = 12.0;
/// Height of the selection chip / item (24: 2 inside the track at the top and bottom).
pub const SEGMENTED_CHIP_HEIGHT: f32 = 24.0;
/// Seconds the chip takes to slide from one item to another.
pub const SEGMENTED_CHIP_ANIM_SECS: f32 = 0.12;
/// Underline: 2 px accent, inset 4 each side.
pub const SEGMENTED_UNDERLINE: f32 = 2.0;
pub const SEGMENTED_UNDERLINE_INSET: f32 = 4.0;
/// The small variant: 22 track, 18 items, padding 6, 11 pt.
pub const SEGMENTED_SMALL_HEIGHT: f32 = 22.0;
pub const SEGMENTED_SMALL_CHIP_HEIGHT: f32 = 18.0;
pub const SEGMENTED_SMALL_ITEM_PADDING: f32 = 6.0;
pub const SEGMENTED_SMALL_TEXT_SIZE: f32 = 11.0;

/// The selection chip's rect for an item occupying `item_rect` (the item's width, centred
/// vertically, [`SEGMENTED_CHIP_HEIGHT`] tall).
pub fn chip_rect(item_rect: Rect) -> Rect {
    chip_rect_of(item_rect, SEGMENTED_CHIP_HEIGHT)
}

fn chip_rect_of(item_rect: Rect, height: f32) -> Rect {
    Rect::from_center_size(item_rect.center(), vec2(item_rect.width(), height))
}

/// A box track with one text item per option; the selected item sits on a `panel_4` chip
/// with an accent underline. `changed()` when the selection moved.
pub fn segmented(ui: &mut Ui, theme: &Theme, selected: &mut usize, items: &[&str]) -> Response {
    segmented_padded(ui, theme, selected, items, SEGMENTED_ITEM_PADDING)
}

/// [`segmented`] with `item_padding` on each side of every item instead of
/// [`SEGMENTED_ITEM_PADDING`] (the pattern tabs A..H).
pub fn segmented_padded(
    ui: &mut Ui,
    theme: &Theme,
    selected: &mut usize,
    items: &[&str],
    item_padding: f32,
) -> Response {
    segmented_impl(
        ui,
        theme,
        selected,
        items,
        item_padding,
        SEGMENTED_HEIGHT,
        SEGMENTED_CHIP_HEIGHT,
        SEGMENTED_TEXT_SIZE,
        4.0,
    )
}

/// The small segmented (22 h, 18 items, padding 6, 11 pt): the rack toolbar's Len / Bar.
pub fn segmented_small(
    ui: &mut Ui,
    theme: &Theme,
    selected: &mut usize,
    items: &[&str],
) -> Response {
    segmented_impl(
        ui,
        theme,
        selected,
        items,
        SEGMENTED_SMALL_ITEM_PADDING,
        SEGMENTED_SMALL_HEIGHT,
        SEGMENTED_SMALL_CHIP_HEIGHT,
        SEGMENTED_SMALL_TEXT_SIZE,
        3.0,
    )
}

#[allow(clippy::too_many_arguments)]
fn segmented_impl(
    ui: &mut Ui,
    theme: &Theme,
    selected: &mut usize,
    items: &[&str],
    item_padding: f32,
    height: f32,
    chip_height: f32,
    text_size: f32,
    radius: f32,
) -> Response {
    let item_padding = item_padding.max(0.0);
    let painter = control_painter(ui);
    let font = FontId::proportional(text_size);
    let galleys: Vec<_> = items
        .iter()
        .map(|item| {
            painter.layout_job(LayoutJob::single_section(
                (*item).to_owned(),
                TextFormat {
                    font_id: font.clone(),
                    color: theme.text_2,
                    ..TextFormat::default()
                },
            ))
        })
        .collect();
    let items_width: f32 = galleys
        .iter()
        .map(|g| g.size().x + 2.0 * item_padding)
        .sum();
    let gaps = SEGMENTED_ITEM_GAP * items.len().saturating_sub(1) as f32;
    let width = 2.0 * SEGMENTED_TRACK_PADDING + items_width + gaps;

    let before = *selected;
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    response
        .widget_info(|| WidgetInfo::labeled(Role::RadioGroup, ui.is_enabled(), items.join(" / ")));
    if items.is_empty() {
        if ui.is_rect_visible(rect) {
            paint_control(&painter, rect, radius, theme.panel_2, theme.border_2);
        }
        return response;
    }
    *selected = (*selected).min(items.len() - 1);

    // Item hit rects, left to right (the full track height; the chip is inset).
    let mut item_rects = Vec::with_capacity(items.len());
    let mut x = rect.min.x + SEGMENTED_TRACK_PADDING;
    for galley in &galleys {
        let w = galley.size().x + 2.0 * item_padding;
        item_rects.push(Rect::from_min_size(pos2(x, rect.min.y), vec2(w, height)));
        x += w + SEGMENTED_ITEM_GAP;
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
            input.count_and_consume_key(Modifiers::NONE, Key::ArrowRight) as i64
                - input.count_and_consume_key(Modifiers::NONE, Key::ArrowLeft) as i64
        });
        if delta != 0 {
            let n = items.len() as i64;
            let next = (i as i64 + delta).clamp(0, n - 1) as usize;
            *selected = next;
            if next != i {
                ui.memory_mut(|m| m.request_focus(item_responses[next].id));
                focused = Some(next);
            }
        }
    }

    let has_chip = items.len() > 1;
    if ui.is_rect_visible(rect) {
        paint_control(&painter, rect, radius, theme.panel_2, theme.border_2);

        // The chip slides to the selected item (its centre and width are animated; a fresh
        // control starts in place). `animation_time` 0 means reduced motion: jump.
        let target = chip_rect_of(item_rects[*selected], chip_height);
        let anim = if ui.style().animation_time <= 0.0 {
            0.0
        } else {
            SEGMENTED_CHIP_ANIM_SECS
        };
        let cx =
            ui.ctx()
                .animate_value_with_time(response.id.with("chip_x"), target.center().x, anim);
        let cw = ui
            .ctx()
            .animate_value_with_time(response.id.with("chip_w"), target.width(), anim);
        let chip = Rect::from_center_size(pos2(cx, target.center().y), vec2(cw, chip_height));
        if has_chip {
            painter.rect_filled(chip, radius - 1.0, theme.panel_4);
            let underline = Rect::from_min_max(
                pos2(
                    chip.min.x + SEGMENTED_UNDERLINE_INSET,
                    chip.max.y - SEGMENTED_UNDERLINE,
                ),
                pos2(chip.max.x - SEGMENTED_UNDERLINE_INSET, chip.max.y),
            );
            painter.rect_filled(underline, 1.0, theme.accent);
        }
        if let Some(i) = focused
            && !item_responses[i].is_pointer_button_down_on()
        {
            let ring = if i == *selected {
                chip
            } else {
                chip_rect_of(item_rects[i], chip_height)
            };
            paint_focus_ring(&painter, ring, radius - 1.0, theme);
        }

        for (i, (galley, item_rect)) in galleys.into_iter().zip(&item_rects).enumerate() {
            let on = i == *selected;
            let color = if on || item_responses[i].hovered() {
                theme.text
            } else {
                theme.text_2
            };
            let text_rect = Align2::CENTER_CENTER.anchor_size(item_rect.center(), galley.size());
            // Weight 500: the same galley twice with a sub-point offset (see `text`).
            painter.galley(text_rect.min + vec2(0.35, 0.0), galley.clone(), color);
            painter.galley(text_rect.min, galley, color);
        }
    }

    for (i, r) in item_responses.iter().enumerate() {
        r.widget_info(|| {
            WidgetInfo::selected(Role::RadioButton, ui.is_enabled(), i == *selected, items[i])
        });
    }

    if *selected != before {
        response.mark_changed();
    }
    response
}

/// The colour of the chip under the selected item (`panel_4`), for tests.
pub fn chip_colour(theme: &Theme) -> Color32 {
    theme.panel_4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_is_centred_in_the_track_and_as_wide_as_the_item() {
        let item = Rect::from_min_size(pos2(10.0, 100.0), vec2(44.0, SEGMENTED_HEIGHT));
        let chip = chip_rect(item);
        assert_eq!(chip.width(), 44.0);
        assert_eq!(chip.height(), SEGMENTED_CHIP_HEIGHT);
        assert_eq!(chip.center(), item.center());
        assert_eq!(chip.min.y - item.min.y, 2.0);
        assert_eq!(item.max.y - chip.max.y, 2.0);
    }
}
