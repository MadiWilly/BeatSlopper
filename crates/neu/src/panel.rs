//! Flat panel container (Deck: `panel` fill, 1 px `border`, radius 4, padding 8).

use egui::epaint::Shape;
use egui::{Sense, Ui, UiBuilder};

use crate::Theme;
use crate::flat::{Tone, flat_rect_shape};

/// A flat frame (`panel` fill, 1 px `border` hairline, radius 4, padding 8) holding
/// `add_contents`: the body of a dock, a group or a window without a header
/// ([`crate::shell::window_frame`] adds the header).
///
/// The contents are laid out first in a child ui; the frame is then inserted at a shape index
/// reserved before the contents, so it ends up behind them and the panel is exactly as big as
/// what it holds (plus padding). Nothing extends beyond the panel rect.
pub fn panel<R>(ui: &mut Ui, theme: &Theme, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    panel_toned(ui, theme, Tone::Panel, add_contents)
}

/// [`panel`] in another tone (`Tone::Panel2` for wells and headers).
pub fn panel_toned<R>(
    ui: &mut Ui,
    theme: &Theme,
    tone: Tone,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
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
        ui.painter().set(
            where_to_put_background,
            flat_rect_shape(
                outer,
                theme.radius_control,
                tone.colour(theme),
                theme.border,
            ),
        );
    }
    ui.allocate_rect(outer, Sense::hover());
    inner
}
