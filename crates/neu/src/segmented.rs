//! Segmented selector (`.seg` in `design/preview-a.html`).
//!
//! An inset-m pill track, [`SEGMENTED_HEIGHT`] tall with [`SEGMENTED_TRACK_PADDING`] at either
//! end, holding one plain 12 pt text item per option with [`SEGMENTED_ITEM_PADDING`] on each
//! side and [`SEGMENTED_ITEM_GAP`] between items. The selected item reads as 12/500 in the
//! primary colour (painted as a faux medium, no filled block); the others are 12/400 in the
//! secondary colour and turn primary on hover. A click selects; Left/Right move the selection
//! while an item has keyboard focus. The returned response is the whole track and reports
//! `changed()` only when the selection moved.

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align2, FontId, Key, Modifiers, Rect, Response, Role, Sense, Ui, WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::shadow::{Depth, paint_soft_rect};

/// Height of the track (`.seg` `height`).
pub const SEGMENTED_HEIGHT: f32 = 30.0;
/// Horizontal padding inside the track (`.seg` `padding: 0 6px`).
pub const SEGMENTED_TRACK_PADDING: f32 = 6.0;
/// Horizontal padding of each item (`.seg span` `padding: 0 8px`).
pub const SEGMENTED_ITEM_PADDING: f32 = 8.0;
/// Gap between items (`.seg` `gap`).
pub const SEGMENTED_ITEM_GAP: f32 = 2.0;
/// Item text size (`.seg span` `font-size`).
pub const SEGMENTED_TEXT_SIZE: f32 = 12.0;

/// An inset pill track with one plain-text item per option; the selected item is bold.
/// `changed()` when the selection moved.
pub fn segmented(ui: &mut Ui, theme: &Theme, selected: &mut usize, items: &[&str]) -> Response {
    let painter = ui.painter().clone();
    let font = FontId::proportional(SEGMENTED_TEXT_SIZE);
    let galleys: Vec<_> = items
        .iter()
        .map(|item| {
            painter.layout_job(LayoutJob::single_section(
                (*item).to_owned(),
                TextFormat {
                    font_id: font.clone(),
                    color: theme.text_secondary,
                    ..TextFormat::default()
                },
            ))
        })
        .collect();
    let items_width: f32 = galleys
        .iter()
        .map(|g| g.size().x + 2.0 * SEGMENTED_ITEM_PADDING)
        .sum();
    let gaps = SEGMENTED_ITEM_GAP * items.len().saturating_sub(1) as f32;
    let width = 2.0 * SEGMENTED_TRACK_PADDING + items_width + gaps;

    let before = *selected;
    let (rect, mut response) =
        ui.allocate_exact_size(vec2(width, SEGMENTED_HEIGHT), Sense::hover());
    response
        .widget_info(|| WidgetInfo::labeled(Role::RadioGroup, ui.is_enabled(), items.join(" / ")));
    if items.is_empty() {
        if ui.is_rect_visible(rect) {
            paint_soft_rect(&painter, rect, theme.radius_pill, Depth::InsetM, theme);
        }
        return response;
    }
    *selected = (*selected).min(items.len() - 1);

    // Item hit rects, left to right.
    let mut item_rects = Vec::with_capacity(items.len());
    let mut x = rect.min.x + SEGMENTED_TRACK_PADDING;
    for galley in &galleys {
        let w = galley.size().x + 2.0 * SEGMENTED_ITEM_PADDING;
        item_rects.push(Rect::from_min_size(
            pos2(x, rect.min.y),
            vec2(w, SEGMENTED_HEIGHT),
        ));
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
            }
        }
    }

    if ui.is_rect_visible(rect) {
        paint_soft_rect(&painter, rect, theme.radius_pill, Depth::InsetM, theme);
        for (i, (galley, item_rect)) in galleys.into_iter().zip(&item_rects).enumerate() {
            let on = i == *selected;
            let color = if on || item_responses[i].hovered() {
                theme.text_primary
            } else {
                theme.text_secondary
            };
            let text_rect = Align2::CENTER_CENTER.anchor_size(item_rect.center(), galley.size());
            if on {
                // Faux medium: the same galley twice with a sub-point offset (see `text`).
                painter.galley(text_rect.min + vec2(0.35, 0.0), galley.clone(), color);
            }
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
