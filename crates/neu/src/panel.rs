//! Raised panel container.

use egui::epaint::Shape;
use egui::{Sense, Ui, UiBuilder};

use crate::Theme;
use crate::shadow::{Depth, soft_rect_shapes};

/// A raised-l panel (radius 24, padding 20) holding `add_contents`.
///
/// The contents are laid out first in a child ui; the panel's fill and two soft shadows are
/// then inserted at a shape index reserved before the contents, so they end up behind them
/// and the panel is exactly as big as what it holds (plus padding). The shadows extend about
/// 18 pt beyond the panel rect, so leave room around panels (the page padding in the mock).
pub fn panel<R>(ui: &mut Ui, theme: &Theme, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let pad = theme.panel_padding;
    let where_to_put_background = ui.painter().add(Shape::Noop);

    let outer_max = ui.available_rect_before_wrap();
    let mut content_max = outer_max.shrink(pad);
    content_max.max.x = content_max.max.x.max(content_max.min.x);
    content_max.max.y = content_max.max.y.max(content_max.min.y);

    let mut content_ui = ui.new_child(UiBuilder::new().max_rect(content_max).layout(*ui.layout()));
    let inner = add_contents(&mut content_ui);

    let outer = content_ui.min_rect().expand(pad);
    if ui.is_rect_visible(outer) {
        let shapes = soft_rect_shapes(outer, theme.radius_panel, Depth::RaisedL, theme);
        ui.painter()
            .set(where_to_put_background, Shape::Vec(shapes));
    }
    ui.allocate_rect(outer, Sense::hover());
    inner
}
