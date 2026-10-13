//! Image-local Brush, Polygonal Lasso, and active-layer controls.
//! The supplied rect is the prefix of the shared color technical datum.

use super::*;
use crate::selection_tools::menu_selector::{bare_menu, BareMenu};

const GROUP_GAP: f32 = 6.0;

/// Width reserved before the color controls, in board units.
pub fn image_brush_header_width(width_units: f32) -> f32 {
    if width_units >= super::image_presentation::IMAGE_WIDE_THRESHOLD {
        176.0
    } else {
        let inner = (width_units - 2.0 * super::image_presentation::IMAGE_PANEL_INSET).max(0.0);
        let color_min = (inner * 0.56).min(90.0);
        let target: f32 = if width_units < 300.0 { 72.0 } else { 112.0 };
        target.min((inner - 6.0 - color_min).max(0.0))
    }
}

/// Brush shares the color editor's technical datum instead of adding a row.
pub fn image_brush_header_height(_width_units: f32) -> f32 {
    super::image_presentation::IMAGE_DATUM_HEIGHT
}

pub struct ImageBrushEdit<'a> {
    pub id: Id,
    pub enabled: bool,
    pub brush_active: bool,
    pub lasso_active: bool,
    pub layers: &'a [&'a str],
    pub active_layer: usize,
    pub layer_open: &'a mut bool,
}

#[derive(Default)]
pub struct ImageBrushEditResponse {
    pub brush: bool,
    pub lasso: bool,
    pub layer: Option<usize>,
    pub captures: bool,
    pub popups: Vec<Rect>,
}

pub fn image_brush_header(
    ui: &mut egui::Ui,
    rect: Rect,
    edit: ImageBrushEdit<'_>,
    zoom: f32,
    theme: Palette,
) -> ImageBrushEditResponse {
    ui.add_enabled_ui(edit.enabled, |ui| {
        image_brush_header_enabled(ui, rect, edit, zoom, theme)
    })
    .inner
}

/// Shared hit regions; compact images show bare tool glyphs.
pub fn image_brush_header_groups(rect: Rect, zoom: f32) -> [Rect; 3] {
    let width = rect.width() / zoom.max(f32::EPSILON);
    let (widths, gap) = if width >= 175.0 {
        ([44.0, 46.0, 74.0], GROUP_GAP)
    } else if width >= 111.0 {
        ([24.0, 24.0, 52.0], GROUP_GAP)
    } else {
        let tool = width * 0.225;
        let gap = width * 0.05;
        ([tool, tool, (width - 2.0 * (tool + gap)).max(0.0)], gap)
    };
    let mut x = rect.left();
    widths.map(|width| {
        let right = (x + width * zoom).min(rect.right());
        let group = Rect::from_min_max(Pos2::new(x, rect.top()), Pos2::new(right, rect.bottom()));
        x = right + gap * zoom;
        group
    })
}

fn image_brush_header_enabled(
    ui: &mut egui::Ui,
    rect: Rect,
    edit: ImageBrushEdit<'_>,
    zoom: f32,
    theme: Palette,
) -> ImageBrushEditResponse {
    let mut out = ImageBrushEditResponse::default();
    let groups = image_brush_header_groups(rect, zoom);
    let full_labels = rect.width() / zoom.max(f32::EPSILON) >= 175.0;
    let brush = action_button(
        ui,
        edit.id.with("brush"),
        groups[0],
        Icon::Brush,
        "Brush",
        full_labels,
        edit.brush_active,
        zoom,
        theme,
    );
    out.brush = brush.clicked();
    out.captures |= brush.hovered();
    let lasso = action_button(
        ui,
        edit.id.with("lasso"),
        groups[1],
        Icon::Lasso,
        "Lasso",
        full_labels,
        edit.lasso_active,
        zoom,
        theme,
    );
    out.lasso = lasso.clicked();
    out.captures |= lasso.hovered();
    if !edit.layers.is_empty() {
        let selected = edit.active_layer.min(edit.layers.len() - 1);
        let layer_menu = bare_menu(
            ui,
            BareMenu {
                id: edit.id.with("layer"),
                rect: groups[2],
                label: edit.layers[selected],
                options: edit.layers,
                selected,
                open: edit.layer_open,
                zoom,
                theme,
            },
        );
        out.layer = layer_menu.picked;
        out.popups.extend(layer_menu.popup);
        out.captures |= layer_menu.popup.is_some()
            || ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|p| groups[2].contains(p));
    } else {
        *edit.layer_open = false;
        let response = ui
            .interact(groups[2], edit.id.with("layer_empty"), Sense::hover())
            .on_hover_text("The first stroke creates a layer");
        canvas_text::text(
            &ui.painter().with_clip_rect(groups[2]),
            groups[2].center(),
            Align2::CENTER_CENTER,
            "Layer",
            canvas_scale::font(9.0, zoom),
            theme.sub,
        );
        out.captures |= response.hovered();
    }
    out
}

pub(super) fn image_brush_color_datum(
    technical: Rect,
    tab: Rect,
    zoom: f32,
) -> ImageTechnicalLayout {
    let available =
        (technical.width() / zoom - super::image_presentation::IMAGE_PANEL_INSET).max(0.0);
    let eye_width = (if available < 180.0 { 18.0_f32 } else { 24.0 }).min(available * 0.2);
    let gap = 4.0_f32.min(available * 0.04);
    let rgb_gap = 2.0_f32.min(available * 0.02);
    let rgb_width = (if available < 180.0 { 28.0_f32 } else { 40.0 })
        .min(((available - eye_width - 2.0 * (gap + rgb_gap)) / 3.0).max(0.0));
    let rgb_total = 3.0 * rgb_width + 2.0 * rgb_gap;
    let rgb_left = technical.left() + (available - rgb_total) * zoom;
    let recent_left = technical.left() + (eye_width + gap) * zoom;
    let recent_space = ((rgb_left - recent_left) / zoom - gap).max(0.0);
    let count = (((recent_space + 3.0) / 9.0).floor() as usize).min(6);
    let diameter = if count == 0 {
        0.0
    } else {
        ((recent_space - count.saturating_sub(1) as f32 * 3.0) / count as f32).min(18.0)
    };
    let y = technical.center().y;
    let eyedropper = Rect::from_center_size(
        Pos2::new(technical.left() + eye_width * zoom * 0.5, y),
        Vec2::new(eye_width * zoom, technical.height() * 0.78),
    );
    let recent_colors = std::array::from_fn(|index| {
        if index < count {
            Rect::from_center_size(
                Pos2::new(
                    recent_left + (diameter * 0.5 + index as f32 * (diameter + 3.0)) * zoom,
                    y,
                ),
                Vec2::splat(diameter * zoom),
            )
        } else {
            Rect::from_min_size(Pos2::new(recent_left, y), Vec2::ZERO)
        }
    });
    let divider = Rect::from_center_size(
        Pos2::new(rgb_left - gap * zoom * 0.5, y),
        Vec2::new(canvas_scale::px(0.7, zoom), technical.height() * 0.55),
    );
    let rgb_values = std::array::from_fn(|index| {
        Rect::from_center_size(
            Pos2::new(
                rgb_left + (rgb_width * 0.5 + index as f32 * (rgb_width + rgb_gap)) * zoom,
                y,
            ),
            Vec2::new(rgb_width * zoom, technical.height() * 0.78),
        )
    });
    ImageTechnicalLayout {
        panel_buttons: std::array::from_fn(|index| {
            super::image_presentation::blister_icon_rect(tab, index, 7)
        }),
        eyedropper,
        recent_colors,
        divider,
        rgb_values,
    }
}

#[allow(clippy::too_many_arguments)]
fn action_button(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    icon: Icon,
    label: &str,
    show_label: bool,
    active: bool,
    zoom: f32,
    theme: Palette,
) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click()).on_hover_text(label);
    let color = if active {
        theme.accent
    } else if response.hovered() {
        theme.ink
    } else {
        theme.sub
    };
    let size = canvas_scale::px(13.0, zoom)
        .min(rect.height() * 0.72)
        .min(rect.width() * 0.72);
    let center = if show_label {
        Pos2::new(rect.left() + 8.0 * zoom, rect.center().y)
    } else {
        rect.center()
    };
    let glyph = Rect::from_center_size(center, Vec2::splat(size));
    icons::paint(ui.painter(), glyph, icon, color);
    if show_label {
        canvas_text::text(
            &ui.painter().with_clip_rect(rect),
            Pos2::new(glyph.right() + 3.0 * zoom, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            canvas_scale::font(9.0, zoom),
            color,
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_uses_one_color_datum_at_all_review_widths() {
        for width in [220.0, 360.0, 680.0] {
            assert_eq!(image_brush_header_height(width), 28.0);
        }
        assert_eq!(image_brush_header_width(360.0), 112.0);
        assert_eq!(image_brush_header_width(680.0), 176.0);
    }

    #[test]
    fn tool_groups_fit_the_reserved_prefix_at_every_zoom() {
        for width in [220.0, 360.0, 680.0] {
            for zoom in [0.5, 1.0, 2.0] {
                let rect = Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(image_brush_header_width(width) * zoom, 28.0 * zoom),
                );
                let groups = image_brush_header_groups(rect, zoom);
                assert_eq!(groups[0].left(), rect.left());
                assert_eq!(groups[2].right(), rect.right());
                assert!(groups.iter().all(|group| rect.contains_rect(*group)));
                assert!(groups
                    .windows(2)
                    .all(|pair| pair[0].right() < pair[1].left()));
            }
        }
    }
}
