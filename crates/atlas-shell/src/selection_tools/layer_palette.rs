//! Shared layer palette painting for generic and image-specific hosts.
use super::*;

#[allow(clippy::too_many_arguments)]
pub fn layer_palette(
    ui: &mut egui::Ui,
    layout: &LayerPaletteLayout,
    thumbs: &[Option<egui::TextureId>],
    active: Option<usize>,
    opacity: f32,
    colors: &[[u8; 3]],
    zoom: f32,
    theme: Palette,
) -> LayerPaletteEdit {
    layer_palette_impl(
        ui, layout, thumbs, active, opacity, colors, zoom, theme, true, false, None, None, None,
    )
    .edit
}

/// Paints image-layer controls on the shared image blister, without adding an
/// independent capsule behind them.
#[allow(clippy::too_many_arguments)]
pub fn image_layer_palette(
    ui: &mut egui::Ui,
    layout: &LayerPaletteLayout,
    thumbs: &[Option<egui::TextureId>],
    layer_ids: &[u64],
    drag: Option<ImageLayerPaletteDrag>,
    active: Option<usize>,
    opacity: f32,
    layer_names: &[String],
    zoom: f32,
    theme: Palette,
) -> ImageLayerPaletteEdit {
    layer_palette_impl(
        ui,
        layout,
        thumbs,
        active,
        opacity,
        &[],
        zoom,
        theme,
        false,
        true,
        Some(layer_ids),
        drag,
        Some(layer_names),
    )
}
#[allow(clippy::too_many_arguments)]
fn layer_palette_impl(
    ui: &mut egui::Ui,
    layout: &LayerPaletteLayout,
    thumbs: &[Option<egui::TextureId>],
    active: Option<usize>,
    opacity: f32,
    colors: &[[u8; 3]],
    zoom: f32,
    theme: Palette,
    paint_surface: bool,
    drag_layers: bool,
    stable_layer_ids: Option<&[u64]>,
    drag: Option<ImageLayerPaletteDrag>,
    layer_names: Option<&[String]>,
) -> ImageLayerPaletteEdit {
    if paint_surface {
        paint_capsule(ui, layout.capsule, zoom, theme);
    }
    let mut out = ImageLayerPaletteEdit::default();
    let centers = stable_layer_ids.map_or_else(
        || layout.layers.clone(),
        |ids| {
            super::image_layer_layout::image_layer_slip_centers(
                ui.ctx(),
                ui.id(),
                layout,
                ids,
                drag,
            )
        },
    );
    let source = drag.and_then(|drag| {
        layout.layer_indices.iter().position(|index| {
            stable_layer_ids.and_then(|ids| ids.get(*index)) == Some(&drag.source)
        })
    });
    let mut render_order: Vec<usize> = (0..layout.layers.len()).collect();
    if let Some(source) = source {
        render_order.retain(|visible| *visible != source);
        render_order.push(source);
    }
    let mut responses = vec![None; layout.layers.len()];
    for i in render_order {
        let canonical = &layout.layers[i];
        let original = layout.layer_indices.get(i).copied().unwrap_or(i);
        let center = centers.get(i).copied().unwrap_or(*canonical);
        let hit = Rect::from_center_size(center, Vec2::splat(layout.radius * 2.0));
        let sense = if drag_layers {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let stable_id = stable_layer_ids
            .and_then(|ids| ids.get(original))
            .copied()
            .unwrap_or(i as u64);
        let response = ui
            .interact(hit, ui.id().with(("paint_layer", stable_id)), sense)
            .on_hover_ui(|ui| {
                ui.label(
                    layer_names
                        .and_then(|names| names.get(original))
                        .map(String::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Layer {}", original + 1)),
                );
            });
        if response.clicked() {
            out.edit.layer = Some(i);
        }
        responses[i] = Some(response.clone());
        let painter = if stable_layer_ids
            .and_then(|ids| ids.get(original))
            .is_some_and(|id| drag.is_some_and(|drag| drag.source == *id))
        {
            drag.map_or_else(
                || ui.painter().clone(),
                |drag| {
                    ui.ctx()
                        .layer_painter(ui.layer_id())
                        .with_clip_rect(drag.clip)
                },
            )
        } else {
            ui.painter().clone()
        };
        let thumb = thumbs.get(original).copied().flatten();
        let radio = FilterRadio {
            label: "",
            fill: [theme.panel.r(), theme.panel.g(), theme.panel.b()],
            fill_b: None,
            thumb,
        };
        if stable_layer_ids.is_some() && thumb.is_some() {
            paint_alpha_checker(&painter, center, layout.radius, zoom, theme);
        }
        paint_filter_radio(
            &painter,
            center,
            layout.radius,
            &radio,
            active == Some(original),
            response.hovered(),
            zoom,
            theme,
        );
        let font = egui::FontId::proportional(layout.radius * 0.95);
        if thumb.is_none() && canvas_text::legible(font.size) {
            canvas_text::text(
                &painter,
                center,
                Align2::CENTER_CENTER,
                original + 1,
                font,
                theme.ink,
            );
        }
    }
    out.layer_responses = responses.into_iter().flatten().collect();
    for (i, (center, rgb)) in layout.colors.iter().zip(colors).enumerate() {
        let original = layout.color_indices.get(i).copied().unwrap_or(i);
        let hit = Rect::from_center_size(*center, Vec2::splat(layout.color_radius * 2.4));
        let response = ui.interact(hit, ui.id().with(("paint_color", i)), Sense::click());
        let ring = if response.hovered() {
            theme.ink
        } else {
            theme.border_strong
        };
        ui.painter().circle(
            *center,
            layout.color_radius,
            Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
            Stroke::new(canvas_scale::px(0.6, zoom), ring),
        );
        if response.clicked() {
            out.edit.color = Some(original);
        }
    }
    let mut fraction = opacity.clamp(0.0, 1.0);
    let display = (fraction * 100.0).round();
    if !paint_surface && layout.track.is_positive() {
        let rail = layout.track;
        let filled = Rect::from_min_max(
            rail.left_top(),
            Pos2::new(egui::lerp(rail.x_range(), fraction), rail.bottom()),
        );
        ui.painter()
            .rect_filled(rail, rail.height(), theme.border_strong);
        ui.painter()
            .rect_filled(filled, rail.height(), theme.accent);
    }
    let changed = if paint_surface {
        capsule_buffer(
            ui,
            ui.id().with("layer_opacity"),
            layout.track,
            &mut fraction,
            display,
            0.0..=100.0,
            "%",
            |v| v / 100.0,
            |v| format!("{}%", number((v * 100.0).round())),
            zoom,
            theme,
        )
    } else {
        buffer(
            ui,
            ui.id().with("layer_opacity"),
            layout.track,
            &mut fraction,
            display,
            0.0..=100.0,
            "%",
            |v| v / 100.0,
            |v| format!("{}%", number((v * 100.0).round())),
            zoom,
            theme,
            false,
        )
    };
    if changed {
        out.edit.opacity = Some(fraction);
    }
    if !paint_surface && layout.opacity_value.is_positive() {
        canvas_text::text(
            ui.painter(),
            layout.opacity_value.center(),
            Align2::CENTER_CENTER,
            format!("{}%", (fraction * 100.0).round() as u32),
            canvas_scale::font(10.0, zoom),
            theme.sub,
        );
    }
    let response = ui
        .interact(layout.add, ui.id().with("paint_layer_add"), Sense::click())
        .on_hover_text("Add paint layer");
    paint_filter_add(ui.painter(), layout.add, response.hovered(), zoom, theme);
    out.edit.add = response.clicked();
    if layout.page_count > 1 {
        let previous = ui.interact(
            layout.page_prev,
            ui.id().with("paint_layer_page_prev"),
            Sense::click(),
        );
        let next = ui.interact(
            layout.page_next,
            ui.id().with("paint_layer_page_next"),
            Sense::click(),
        );
        canvas_text::text(
            ui.painter(),
            layout.page_prev.center(),
            Align2::CENTER_CENTER,
            "‹",
            canvas_scale::font(11.0, zoom),
            if previous.hovered() {
                theme.ink
            } else {
                theme.sub
            },
        );
        canvas_text::text(
            ui.painter(),
            layout.page_next.center(),
            Align2::CENTER_CENTER,
            "›",
            canvas_scale::font(11.0, zoom),
            if next.hovered() { theme.ink } else { theme.sub },
        );
        out.edit.previous_page = previous.clicked();
        out.edit.next_page = next.clicked();
    }
    out
}

fn paint_alpha_checker(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    zoom: f32,
    theme: Palette,
) {
    let cell = 6.0 * zoom;
    if cell <= 0.0 {
        return;
    }
    let bounds = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    let pale = theme.bg;
    let dark = theme.border;
    let mut y = bounds.top();
    let mut row = 0;
    while y + cell <= bounds.bottom() {
        let mut x = bounds.left();
        let mut column = 0;
        while x + cell <= bounds.right() {
            let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(cell));
            let inside = [
                rect.left_top(),
                rect.right_top(),
                rect.left_bottom(),
                rect.right_bottom(),
            ]
            .into_iter()
            .all(|corner| corner.distance_sq(center) <= radius * radius);
            if inside {
                painter.rect_filled(rect, 0.0, if (row + column) % 2 == 0 { pale } else { dark });
            }
            x += cell;
            column += 1;
        }
        y += cell;
        row += 1;
    }
}
