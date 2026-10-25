//! Segmented selector (`.seg` in `design/preview-a.html`, selection chip per the design
//! cleanup recorded in `design/WINNER.md` "As-built tweaks").
//!
//! An inset-m pill track, [`SEGMENTED_HEIGHT`] tall with [`SEGMENTED_TRACK_PADDING`] at either
//! end, holding one plain 12 pt text item per option with [`SEGMENTED_ITEM_PADDING`] on each
//! side and [`SEGMENTED_ITEM_GAP`] between items. The selected item sits on a raised accent
//! chip: a [`SEGMENTED_CHIP_HEIGHT`] pill the width of the item, painted with the on-glow
//! treatment (accent fill, accent glow, dark shadow, like the theme toggle's knob) and the
//! item text in the on-accent colour, 12/500 (a faux medium). The other items are 12/400 in
//! the secondary colour and turn primary on hover. The chip slides between items over
//! [`SEGMENTED_CHIP_ANIM_SECS`]; with `Style::animation_time` at zero (the kit's reduced-motion
//! switch, egui has no system setting for it) it jumps.
//!
//! A click selects; Left/Right move the selection while an item has keyboard focus, and the
//! focused item wears the kit's 1.5 pt accent focus ring. The returned response is the whole
//! track and reports `changed()` only when the selection moved. A track with a single item
//! offers no choice, so it shows no chip (the item reads in the primary colour instead of
//! lighting up like an ON button).

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    Align2, FontId, Key, Modifiers, Rect, Response, Role, Sense, Ui, WidgetInfo, pos2, vec2,
};

use crate::Theme;
use crate::shadow::{Depth, paint_focus_rect, paint_soft_rect, paint_soft_rect_with_fill};

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
/// Height of the selection chip (4 pt inside the track at the top and bottom).
pub const SEGMENTED_CHIP_HEIGHT: f32 = 22.0;
/// Seconds the chip takes to slide from one item to another.
pub const SEGMENTED_CHIP_ANIM_SECS: f32 = 0.12;

/// The selection chip's rect for an item occupying `item_rect` (the item's width, centred
/// vertically, [`SEGMENTED_CHIP_HEIGHT`] tall).
pub fn chip_rect(item_rect: Rect) -> Rect {
    Rect::from_center_size(
        item_rect.center(),
        vec2(item_rect.width(), SEGMENTED_CHIP_HEIGHT),
    )
}

/// An inset pill track with one plain-text item per option; the selected item sits on a
/// raised accent chip. `changed()` when the selection moved.
pub fn segmented(ui: &mut Ui, theme: &Theme, selected: &mut usize, items: &[&str]) -> Response {
    segmented_padded(ui, theme, selected, items, SEGMENTED_ITEM_PADDING)
}

/// [`segmented`] with `item_padding` on each side of every item instead of
/// [`SEGMENTED_ITEM_PADDING`]: for a track of single-letter items (the sequencer's A..H
/// pattern tabs) where the 8 pt padding alone is a third of the track. Everything else is
/// the same control.
pub fn segmented_padded(
    ui: &mut Ui,
    theme: &Theme,
    selected: &mut usize,
    items: &[&str],
    item_padding: f32,
) -> Response {
    let item_padding = item_padding.max(0.0);
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
        .map(|g| g.size().x + 2.0 * item_padding)
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
        let w = galley.size().x + 2.0 * item_padding;
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
                focused = Some(next);
            }
        }
    }

    let has_chip = items.len() > 1;
    if ui.is_rect_visible(rect) {
        paint_soft_rect(&painter, rect, theme.radius_pill, Depth::InsetM, theme);

        // The chip slides to the selected item (its centre and width are animated; a fresh
        // control starts in place). `animation_time` 0 means reduced motion: jump.
        let target = chip_rect(item_rects[*selected]);
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
        let chip =
            Rect::from_center_size(pos2(cx, target.center().y), vec2(cw, SEGMENTED_CHIP_HEIGHT));
        let chip_fill = if item_responses[*selected].hovered() {
            theme.accent_hover
        } else {
            theme.accent
        };
        if has_chip {
            paint_soft_rect_with_fill(
                &painter,
                chip,
                theme.radius_pill,
                Depth::OnGlow,
                chip_fill,
                theme,
            );
        }
        if let Some(i) = focused {
            let ring = if i == *selected {
                chip
            } else {
                chip_rect(item_rects[i])
            };
            paint_focus_rect(&painter, ring, theme.radius_pill, theme);
        }

        for (i, (galley, item_rect)) in galleys.into_iter().zip(&item_rects).enumerate() {
            let on = i == *selected;
            let color = if on && has_chip {
                theme.on_accent
            } else if on || item_responses[i].hovered() {
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
        assert_eq!(chip.min.y - item.min.y, 4.0);
        assert_eq!(item.max.y - chip.max.y, 4.0);
    }
}
