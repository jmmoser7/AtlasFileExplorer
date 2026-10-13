//! Image editor surfaces and cached, softly blurred interaction shadows.
use super::{canvas_scale, ImagePresentationLayout};
use crate::theme::Palette;
use eframe::egui::{self, emath::Rot2, Color32, Id, Pos2, Rect, Stroke, Vec2};
use std::sync::Arc;

mod frame;
pub use frame::{image_frame_geometry, ImageFrameGeometry};

const IMAGE_SHADOW_BLUR: f32 = 28.0;
const IMAGE_SHADOW_OFFSET: f32 = 5.0;
const IMAGE_SHADOW_DARK_ALPHA: f32 = 0.48;
const IMAGE_SHADOW_LIGHT_ALPHA: f32 = 0.18;
const PANEL_SHADOW_BLUR: f32 = 16.0;
const APRON_RADIUS: f32 = 8.0;
const BLISTER_SHOULDER_RUN: f32 = 28.0;

#[derive(Clone, Copy, PartialEq)]
struct ShadowKey {
    rect: Rect,
    radius: f32,
    rotation_deg: f32,
    shadow: egui::Shadow,
    pixels_per_point: f32,
}

#[derive(Clone)]
struct CachedShadow {
    key: ShadowKey,
    mesh: Arc<egui::Mesh>,
}

/// Chrome only: does not alter the image, its layers, or its export.
pub fn paint_image_interaction_shadow(
    painter: &egui::Painter,
    id: Id,
    rect: Rect,
    radius: f32,
    rotation_deg: f32,
    zoom: f32,
    theme: Palette,
) {
    let shadow = egui::Shadow {
        offset: [
            0,
            canvas_scale::px(IMAGE_SHADOW_OFFSET, zoom).min(127.0) as i8,
        ],
        blur: canvas_scale::px(IMAGE_SHADOW_BLUR, zoom).min(255.0) as u8,
        spread: 0,
        color: Color32::BLACK.gamma_multiply(if theme.dark_mode {
            IMAGE_SHADOW_DARK_ALPHA
        } else {
            IMAGE_SHADOW_LIGHT_ALPHA
        }),
    };
    paint_cached_shadow(painter, id, rect, radius, rotation_deg, shadow);
}

fn paint_cached_shadow(
    painter: &egui::Painter,
    id: Id,
    rect: Rect,
    radius: f32,
    rotation_deg: f32,
    shadow: egui::Shadow,
) {
    let ctx = painter.ctx();
    let key = ShadowKey {
        rect,
        radius,
        rotation_deg,
        shadow,
        pixels_per_point: ctx.pixels_per_point(),
    };
    let cached = ctx.data(|d| d.get_temp::<CachedShadow>(id));
    let mesh = if let Some(cached) = cached.filter(|cached| cached.key == key) {
        cached.mesh
    } else {
        let shape = egui::Shadow {
            offset: [0, 0],
            ..shadow
        }
        .as_shape(rect, radius);
        let primitives = ctx.tessellate(
            vec![egui::epaint::ClippedShape {
                clip_rect: Rect::EVERYTHING,
                shape: shape.into(),
            }],
            key.pixels_per_point,
        );
        let rotation = Rot2::from_angle(rotation_deg.to_radians());
        let offset = Vec2::new(shadow.offset[0] as f32, shadow.offset[1] as f32);
        let mut mesh = egui::Mesh::default();
        for primitive in primitives {
            if let egui::epaint::Primitive::Mesh(mut part) = primitive.primitive {
                for vertex in &mut part.vertices {
                    vertex.pos = rect.center() + rotation * (vertex.pos - rect.center()) + offset;
                }
                mesh.append(part);
            }
        }
        let mesh = Arc::new(mesh);
        ctx.data_mut(|d| {
            d.insert_temp(
                id,
                CachedShadow {
                    key,
                    mesh: mesh.clone(),
                },
            )
        });
        mesh
    };
    painter.add(egui::Shape::Mesh(mesh));
}

/// The solid apron follows the image's vertical edges. Only the image-
/// overlapping tab fades out while an editor is active.
pub fn paint_image_blister(
    painter: &egui::Painter,
    layout: &ImagePresentationLayout,
    zoom: f32,
    theme: Palette,
    tab_opacity: f32,
) {
    paint_blister_surface(
        painter,
        layout.overall,
        layout.tab,
        zoom,
        theme,
        tab_opacity,
    );
}

/// The one blister surface for image and rectangular portal property palettes.
pub fn paint_blister_surface(
    painter: &egui::Painter,
    panel: Rect,
    tab: Rect,
    zoom: f32,
    theme: Palette,
    tab_opacity: f32,
) {
    let radius = canvas_scale::px(APRON_RADIUS, zoom).round().min(255.0) as u8;
    // Clip the apron shadow at the image edge: editing must never shade its pixels.
    let below = painter.with_clip_rect(painter.clip_rect().intersect(Rect::from_min_max(
        Pos2::new(f32::NEG_INFINITY, panel.top()),
        Pos2::new(f32::INFINITY, f32::INFINITY),
    )));
    paint_cached_shadow(
        &below,
        painter.layer_id().id.with("image_apron_shadow"),
        panel,
        radius as f32,
        0.0,
        egui::Shadow {
            offset: [0, canvas_scale::px(3.0, zoom).min(127.0) as i8],
            blur: canvas_scale::px(PANEL_SHADOW_BLUR, zoom).min(255.0) as u8,
            spread: 0,
            color: Color32::BLACK.gamma_multiply(if theme.dark_mode { 0.30 } else { 0.10 }),
        },
    );
    painter.rect_filled(
        panel,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius,
            se: radius,
        },
        theme.card,
    );
    painter.line_segment(
        [
            Pos2::new(panel.left() + radius as f32, panel.bottom()),
            Pos2::new(panel.right() - radius as f32, panel.bottom()),
        ],
        Stroke::new(canvas_scale::px(0.8, zoom), theme.border_strong),
    );
    paint_blister_tab(painter, tab, zoom, theme, false, tab_opacity);
}

/// An opaque selection tab becomes bare line icons as a tool is selected.
pub fn paint_image_tab(
    painter: &egui::Painter,
    layout: &ImagePresentationLayout,
    zoom: f32,
    theme: Palette,
    hovered: bool,
    opacity: f32,
) {
    paint_blister_tab(painter, layout.tab, zoom, theme, hovered, opacity);
}

pub fn paint_blister_tab(
    painter: &egui::Painter,
    tab: Rect,
    zoom: f32,
    theme: Palette,
    hovered: bool,
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    let mut backing = painter.clone();
    backing.multiply_opacity(opacity.clamp(0.0, 1.0));
    let fill = if hovered {
        theme.card_hover
    } else {
        theme.card
    };
    crate::tabs::paint_tab_bubble(
        &backing,
        tab,
        fill,
        fill,
        crate::tabs::TabBubble::Sloped {
            run: canvas_scale::px(BLISTER_SHOULDER_RUN, zoom),
        },
        crate::tabs::TabHost::Bottom,
    );
}

#[derive(Clone)]
struct CornerBacking {
    key: (Rect, f32, bool, Color32),
    mesh: Arc<egui::Mesh>,
}

/// Fill only the two negative spaces outside the host's lower corner arcs.
/// The image remains untouched, including transparent pixels within its frame.
pub fn paint_blister_corner_backing(
    painter: &egui::Painter,
    id: Id,
    host: Rect,
    radius: f32,
    chamfer: bool,
    theme: Palette,
) {
    let radius = radius.min(host.width() * 0.5).min(host.height() * 0.5);
    if radius <= 0.0 {
        return;
    }
    let key = (host, radius, chamfer, theme.card);
    let cached = painter
        .ctx()
        .data(|data| data.get_temp::<CornerBacking>(id));
    let mesh = if let Some(cached) = cached.filter(|cached| cached.key == key) {
        cached.mesh
    } else {
        let mut mesh = egui::Mesh::default();
        let segments = if chamfer { 1 } else { 32 };
        for right in [false, true] {
            let reflect = |point: Pos2| {
                if right {
                    Pos2::new(host.right() - (point.x - host.left()), point.y)
                } else {
                    point
                }
            };
            let outer = mesh.vertices.len() as u32;
            mesh.colored_vertex(reflect(host.left_bottom()), theme.card);
            let center = Pos2::new(host.left() + radius, host.bottom() - radius);
            for index in 0..=segments {
                let angle = std::f32::consts::PI
                    - index as f32 / segments as f32 * std::f32::consts::FRAC_PI_2;
                mesh.colored_vertex(reflect(center + radius * Vec2::angled(angle)), theme.card);
                if index > 0 {
                    mesh.add_triangle(outer, outer + index, outer + index + 1);
                }
            }
        }
        let mesh = Arc::new(mesh);
        painter.ctx().data_mut(|data| {
            data.insert_temp(
                id,
                CornerBacking {
                    key,
                    mesh: mesh.clone(),
                },
            )
        });
        mesh
    };
    painter.add(egui::Shape::Mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_tab_paints_no_backing_but_full_tab_still_has_its_silhouette() {
        let ctx = egui::Context::default();
        let layout = super::super::image_presentation_layout(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 320.0)),
            1.0,
            super::super::ImagePanelKind::Brush,
        );
        for theme in [Palette::light(), Palette::dark()] {
            for opacity in [0.0, 1.0] {
                let out = ctx.run(egui::RawInput::default(), |ctx| {
                    let painter = ctx.layer_painter(egui::LayerId::background());
                    paint_image_tab(&painter, &layout, 1.0, theme, false, opacity);
                });
                assert_eq!(out.shapes.is_empty(), opacity == 0.0);
            }
        }
    }

    #[test]
    fn resting_shadow_reuses_its_mesh_and_geometry_changes_invalidate_it() {
        let ctx = egui::Context::default();
        let id = Id::new("image-shadow-cache-test");
        let rect = Rect::from_min_size(Pos2::new(80.0, 90.0), Vec2::new(640.0, 320.0));
        let frame = |rect, angle| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                paint_image_interaction_shadow(
                    &ctx.layer_painter(egui::LayerId::background()),
                    id,
                    rect,
                    8.0,
                    angle,
                    1.0,
                    Palette::light(),
                );
            });
            ctx.data(|d| d.get_temp::<CachedShadow>(id)).unwrap().mesh
        };
        let first = frame(rect, 0.0);
        let resting = frame(rect, 0.0);
        assert!(!first.is_empty());
        assert!(
            Arc::ptr_eq(&first, &resting),
            "resting frames must not retessellate the blur"
        );
        assert!(!Arc::ptr_eq(&first, &frame(rect, 30.0)));
        assert!(!Arc::ptr_eq(
            &first,
            &frame(rect.translate(Vec2::splat(20.0)), 0.0)
        ));
    }
}
