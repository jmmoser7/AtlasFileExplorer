//! Bounded image-blister geometry for result and paint-layer controls.

use super::{ImagePresentationLayout, LayerPaletteEdit, LayerPaletteLayout};
use eframe::egui::{Context, Id, Pos2, Rect, Vec2};

/// Main-field height shared with `ImagePanelKind::Layers` presentation.
pub(super) fn image_layer_palette_height(_content_width_units: f32) -> f32 {
    32.0
}

/// An image palette edit together with the shared layer-circle interactions.
#[derive(Clone, Debug, Default)]
pub struct ImageLayerPaletteEdit {
    pub edit: LayerPaletteEdit,
    pub layer_responses: Vec<eframe::egui::Response>,
}

/// Transient pointer state for slipping an image layer through the palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLayerPaletteDrag {
    pub source: u64,
    pub target: Option<u64>,
    pub pointer: Pos2,
    pub clip: Rect,
}

/// Maps each visible layer identity to its animated screen-space slot.
pub fn image_layer_slip_centers(
    ctx: &Context,
    parent: Id,
    layout: &super::LayerPaletteLayout,
    layer_ids: &[u64],
    drag: Option<ImageLayerPaletteDrag>,
) -> Vec<Pos2> {
    let source = drag.and_then(|drag| {
        layout
            .layer_indices
            .iter()
            .position(|index| layer_ids.get(*index) == Some(&drag.source))
    });
    let target = drag.and_then(|drag| {
        drag.target.and_then(|target| {
            layout
                .layer_indices
                .iter()
                .position(|index| layer_ids.get(*index) == Some(&target))
        })
    });
    let count = layout.layers.len();
    if count == 0 {
        return Vec::new();
    }
    let left = layout.layers[0].x;
    let right = layout.layers[count - 1].x;
    layout
        .layers
        .iter()
        .enumerate()
        .map(|(visible, canonical)| {
            let stable_id = layout
                .layer_indices
                .get(visible)
                .and_then(|index| layer_ids.get(*index))
                .copied()
                .unwrap_or(visible as u64);
            if Some(visible) == source {
                if let Some(drag) = drag {
                    let fraction = if right > left {
                        ((drag.pointer.x - left) / (right - left)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    ctx.animate_value_with_time(
                        parent.with(("image-layer-slip-slot", stable_id)),
                        fraction,
                        0.0,
                    );
                    return drag.pointer;
                }
            }
            let mut slot = visible;
            if let (Some(source), Some(target)) = (source, target) {
                if source < target && visible > source && visible < target {
                    slot -= 1;
                } else if target < source && visible >= target && visible < source {
                    slot += 1;
                }
            }
            let fraction = if count > 1 {
                slot as f32 / (count - 1) as f32
            } else {
                0.0
            };
            let animated = ctx.animate_value_with_time(
                parent.with(("image-layer-slip-slot", stable_id)),
                fraction,
                0.16,
            );
            Pos2::new(left + (right - left) * animated, canonical.y)
        })
        .collect()
}

/// Layer circles and Add share the top datum; opacity uses the shallow field
/// immediately below it. Paging keeps the circles within the image width.
pub fn image_layer_palette_layout(
    presentation: &ImagePresentationLayout,
    layers: usize,
    _colors: usize,
    zoom: f32,
    active_layer: Option<usize>,
) -> LayerPaletteLayout {
    let header = presentation.layer_header.unwrap_or(presentation.main);
    let host = header.union(presentation.main);
    let z = zoom.max(f32::EPSILON);
    let capsule = Rect::from_min_max(
        Pos2::new(host.left() + 12.0 * z, host.top()),
        Pos2::new(host.right() - 12.0 * z, host.bottom()),
    );
    let width = capsule.width().max(0.0);
    let radius = 20.0 * z;
    let diameter = radius * 2.0;
    let pitch = diameter + 12.0 * z;
    let add_d = diameter;
    let gap = 12.0 * z;
    let nav_d = 12.0 * z;
    let nav_gap = 6.0 * z;
    let value_w = 30.0 * z;
    let value_gap = 8.0 * z;
    let row_fixed = add_d + gap;
    let capacity = |with_nav: bool| {
        let nav = if with_nav {
            2.0 * (nav_d + nav_gap)
        } else {
            0.0
        };
        ((width - row_fixed - nav + gap).max(0.0) / pitch).floor() as usize
    };
    let no_nav = capacity(false);
    let page_count = if layers > no_nav.max(1) { 2 } else { 1 };
    let page_capacity = if page_count > 1 {
        capacity(true).max(1)
    } else {
        layers.max(1)
    };
    let page_count = layers.div_ceil(page_capacity).max(1);
    let page = active_layer
        .filter(|index| *index < layers)
        .map(|index| index / page_capacity)
        .unwrap_or(0)
        .min(page_count - 1);
    let first = page * page_capacity;
    let layer_indices: Vec<usize> = (first..(first + page_capacity).min(layers)).collect();
    let nav_w = if page_count > 1 { nav_d + nav_gap } else { 0.0 };
    let y = header.center().y;
    let mut x = capsule.left();
    let page_prev = if page_count > 1 {
        let rect = Rect::from_center_size(Pos2::new(x + nav_d * 0.5, y), Vec2::splat(nav_d));
        x += nav_w;
        rect
    } else {
        Rect::NOTHING
    };
    let centers: Vec<Pos2> = (0..layer_indices.len())
        .map(|index| Pos2::new(x + radius + index as f32 * pitch, y))
        .collect();
    x += centers.len() as f32 * pitch;
    if !centers.is_empty() {
        x -= 12.0 * z;
        x += gap;
    }
    let page_next = if page_count > 1 {
        let rect = Rect::from_center_size(Pos2::new(x + nav_d * 0.5, y), Vec2::splat(nav_d));
        x += nav_w;
        rect
    } else {
        Rect::NOTHING
    };
    if layers == 0 || centers.is_empty() {
        x = capsule.left() + if page_count > 1 { nav_w } else { 0.0 };
    }
    let add = Rect::from_center_size(Pos2::new(x + add_d * 0.5, y), Vec2::splat(add_d));
    let opacity_y = presentation.main.center().y;
    let opacity_value = Rect::from_min_max(
        Pos2::new(capsule.right() - value_w, opacity_y - 9.0 * z),
        Pos2::new(capsule.right(), opacity_y + 9.0 * z),
    );
    let track = Rect::from_min_max(
        Pos2::new(capsule.left(), opacity_y - 0.75 * z),
        Pos2::new(
            (opacity_value.left() - value_gap).max(capsule.left()),
            opacity_y + 0.75 * z,
        ),
    );
    LayerPaletteLayout {
        capsule,
        layers: centers,
        layer_indices,
        radius,
        colors: Vec::new(),
        color_indices: Vec::new(),
        color_radius: 0.0,
        track,
        opacity_value,
        add,
        page_prev,
        page_next,
        page,
        page_count,
        page_capacity,
    }
}
