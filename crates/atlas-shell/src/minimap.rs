//! Shared canvas minimap overlay.
//!
//! A squircle panel anchored in the lower-right corner of the canvas that
//! shows a simplified picture of the whole content plus the current camera
//! viewport. Apps supply a plain-data [`MinimapModel`] (world-space rects and
//! colors — no dependency on any document model) and react to the returned
//! [`MinimapAction`]; all painting and interaction live here (Constitution
//! Art. X).
//!
//! Performance contract (Constitution Art. II): the content blocks are
//! rasterized into a **cached texture** that is regenerated only when the
//! model's `generation` changes (or the panel is resized). Per-frame painting
//! is just the panel shape, one textured quad, and the live viewport
//! rectangle.
//!
//! Geometry lives under `[minimap]` in `ui-tokens.toml`.

use crate::dock::paint_squircle;
use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2};

/// Everything the minimap needs, in world coordinates. Plain data — apps
/// adapt their scene/tree into this model.
pub struct MinimapModel {
    /// World bounds of all content (blocks should lie within).
    pub bounds: Rect,
    /// Simplified content rects with their display colors.
    pub blocks: Vec<(Rect, Color32)>,
    /// Current camera rect in world space.
    pub viewport: Rect,
    /// Bump whenever `bounds`/`blocks` change; keys the cached texture.
    pub generation: u64,
}

/// Retained minimap state: the cached content texture and interaction state.
/// Keep one per canvas (or per tab) and pass it back every frame.
#[derive(Default)]
pub struct MinimapState {
    texture: Option<TextureHandle>,
    cached_generation: Option<u64>,
    cached_pixels: [usize; 2],
    dragging: bool,
}

/// What the user did to the minimap this frame. All points are **world**
/// coordinates; the app owns the camera and applies the motion.
pub enum MinimapAction {
    None,
    /// Single click: center the camera on this world point.
    JumpTo(Pos2),
    /// Continuous drag: keep the camera centered on this world point.
    DragTo(Pos2),
    /// Scroll over the minimap: zoom the main camera anchored at the world
    /// point under the cursor. `factor > 1` zooms in.
    Zoom {
        world_point: Pos2,
        factor: f32,
    },
}

/// Content inset inside the panel so blocks never touch the squircle border.
const CONTENT_PADDING: f32 = 7.0;
/// Scroll-to-zoom sensitivity (matches the feel of canvas wheel zoom).
const ZOOM_PER_SCROLL_PX: f32 = 0.002;

/// Paint the minimap anchored to the lower-right of `canvas_rect` and report
/// interactions. Call only while the minimap is toggled on — it costs nothing
/// when simply not called.
pub fn minimap_ui(
    ui: &mut Ui,
    canvas_rect: Rect,
    model: &MinimapModel,
    state: &mut MinimapState,
) -> MinimapAction {
    let t = tokens::current();
    let mt = &t.minimap;
    let palette = Palette::for_mode(ui.visuals().dark_mode);
    let th = if palette.bg.r() > 128 {
        &t.dock.light
    } else {
        &t.dock.dark
    };

    if model.bounds.width() <= 0.0 || model.bounds.height() <= 0.0 {
        return MinimapAction::None;
    }

    // Panel: fixed width, height from the content aspect ratio, clamped.
    let aspect = model.bounds.height() / model.bounds.width();
    let height = (mt.width * aspect).clamp(mt.min_height, mt.max_height);
    let panel = Rect::from_min_size(
        canvas_rect.right_bottom() - Vec2::new(mt.width + mt.margin, height + mt.margin),
        Vec2::new(mt.width, height),
    );
    let inner = panel.shrink(CONTENT_PADDING);

    // --- cached content texture (regenerated only on generation change) ---
    let ppp = ui.ctx().pixels_per_point();
    let pixels = [
        (inner.width() * ppp).round().max(1.0) as usize,
        (inner.height() * ppp).round().max(1.0) as usize,
    ];
    if state.cached_generation != Some(model.generation) || state.cached_pixels != pixels {
        let image = rasterize_blocks(model, pixels);
        state.texture = Some(ui.ctx().load_texture(
            "atlas-shell-minimap",
            image,
            egui::TextureOptions::LINEAR,
        ));
        state.cached_generation = Some(model.generation);
        state.cached_pixels = pixels;
    }

    // --- interaction (before painting so hover styling could react) ---
    let resp = ui.interact(
        panel,
        ui.id().with("atlas_shell_minimap"),
        Sense::click_and_drag(),
    );

    // --- panel chrome: soft shadow + dock-style squircle ---
    let painter = ui.painter_at(canvas_rect);
    let shadow_alpha = (t.dock.shadow_opacity.clamp(0.0, 1.0) * 90.0) as u8;
    paint_squircle(
        &painter,
        panel.translate(Vec2::new(0.0, 2.0)).expand(2.5),
        Color32::from_black_alpha(shadow_alpha),
        Stroke::NONE,
        mt.squircle_exponent,
    );
    paint_squircle(
        &painter,
        panel,
        th.popover_fill_color(),
        Stroke::new(1.0_f32, th.border_color()),
        mt.squircle_exponent,
    );

    // --- cached content ---
    if let Some(tex) = &state.texture {
        painter.image(
            tex.id(),
            inner,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    // --- live viewport rectangle (never cached — it moves every frame) ---
    let vp_min = world_to_panel(model.viewport.min, model.bounds, inner);
    let vp_max = world_to_panel(model.viewport.max, model.bounds, inner);
    let vp = Rect::from_min_max(vp_min, vp_max).intersect(inner);
    if vp.is_positive() {
        painter.rect_filled(vp, 2.0, palette.select.gamma_multiply(0.15));
        painter.rect_stroke(
            vp,
            2.0,
            Stroke::new(1.2_f32, palette.select),
            egui::StrokeKind::Inside,
        );
    }

    // --- actions ---
    if resp.dragged() || state.dragging {
        state.dragging = resp.dragged();
        if let Some(pos) = resp.interact_pointer_pos() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            return MinimapAction::DragTo(panel_to_world(pos, model.bounds, inner));
        }
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            return MinimapAction::JumpTo(panel_to_world(pos, model.bounds, inner));
        }
    }
    if resp.hovered() {
        // Consume the scroll so the canvas underneath doesn't also zoom/pan.
        let scroll = ui.input_mut(|i| {
            let s = i.smooth_scroll_delta.y + i.raw_scroll_delta.y;
            if s.abs() > 0.0 {
                i.smooth_scroll_delta.y = 0.0;
                i.raw_scroll_delta.y = 0.0;
            }
            s
        });
        if scroll.abs() > 0.0 {
            let pointer = ui.input(|i| i.pointer.hover_pos()).unwrap_or(vp.center());
            return MinimapAction::Zoom {
                world_point: panel_to_world(pointer, model.bounds, inner),
                factor: (scroll * ZOOM_PER_SCROLL_PX).exp(),
            };
        }
    }
    MinimapAction::None
}

// ---------- pure world ↔ panel mapping ----------

/// Uniform scale + centered offset that fit `bounds` inside `inner`
/// (letterboxed, aspect preserved). Returns `(scale, panel_pos_of_bounds_min)`.
fn fit_transform(bounds: Rect, inner: Rect) -> (f32, Pos2) {
    let sx = inner.width() / bounds.width().max(f32::EPSILON);
    let sy = inner.height() / bounds.height().max(f32::EPSILON);
    let scale = sx.min(sy);
    let content = Vec2::new(bounds.width(), bounds.height()) * scale;
    let origin = inner.min + (inner.size() - content) * 0.5;
    (scale, origin)
}

/// Map a world point into panel (screen) coordinates.
fn world_to_panel(world: Pos2, bounds: Rect, inner: Rect) -> Pos2 {
    let (scale, origin) = fit_transform(bounds, inner);
    origin + (world - bounds.min) * scale
}

/// Map a panel (screen) point back into world coordinates.
fn panel_to_world(panel: Pos2, bounds: Rect, inner: Rect) -> Pos2 {
    let (scale, origin) = fit_transform(bounds, inner);
    bounds.min + (panel - origin) / scale.max(f32::EPSILON)
}

/// Rasterize the model's blocks into an RGBA image of `pixels` size.
/// Runs only on generation change, never per frame.
fn rasterize_blocks(model: &MinimapModel, pixels: [usize; 2]) -> egui::ColorImage {
    let mut image = egui::ColorImage::new(pixels, Color32::TRANSPARENT);
    let image_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(pixels[0] as f32, pixels[1] as f32));
    for &(rect, color) in &model.blocks {
        let a = world_to_panel(rect.min, model.bounds, image_rect);
        let b = world_to_panel(rect.max, model.bounds, image_rect);
        let x0 = a.x.floor().max(0.0) as usize;
        let y0 = a.y.floor().max(0.0) as usize;
        // Every block covers at least one pixel so tiny items stay visible.
        let x1 = (b.x.ceil() as usize).clamp(x0 + 1, pixels[0]);
        let y1 = (b.y.ceil() as usize).clamp(y0 + 1, pixels[1]);
        if x0 >= pixels[0] || y0 >= pixels[1] {
            continue;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                image.pixels[y * pixels[0] + x] = color;
            }
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 0.01
    }

    #[test]
    fn world_panel_round_trip() {
        let bounds = Rect::from_min_max(Pos2::new(-500.0, -200.0), Pos2::new(1500.0, 800.0));
        let inner = Rect::from_min_max(Pos2::new(10.0, 20.0), Pos2::new(210.0, 140.0));
        for world in [
            bounds.min,
            bounds.max,
            bounds.center(),
            Pos2::new(0.0, 0.0),
            Pos2::new(123.0, 456.0),
        ] {
            let back = panel_to_world(world_to_panel(world, bounds, inner), bounds, inner);
            assert!(
                close(back, world),
                "round trip drifted: {world:?} → {back:?}"
            );
        }
    }

    #[test]
    fn fit_preserves_aspect_and_centers() {
        // Content twice as wide as tall inside a square panel: full width,
        // letterboxed vertically, centered.
        let bounds = Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 100.0));
        let inner = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        let (scale, origin) = fit_transform(bounds, inner);
        assert!((scale - 0.5).abs() < 1e-6);
        assert!((origin.x - 0.0).abs() < 1e-6);
        assert!((origin.y - 25.0).abs() < 1e-6, "not vertically centered");
    }

    #[test]
    fn bounds_corners_map_inside_inner() {
        let bounds = Rect::from_min_max(Pos2::new(-10.0, -10.0), Pos2::new(10.0, 30.0));
        let inner = Rect::from_min_max(Pos2::new(5.0, 5.0), Pos2::new(105.0, 55.0));
        for corner in [
            bounds.min,
            bounds.max,
            Pos2::new(bounds.min.x, bounds.max.y),
            Pos2::new(bounds.max.x, bounds.min.y),
        ] {
            let p = world_to_panel(corner, bounds, inner);
            assert!(inner.expand(0.01).contains(p), "{corner:?} mapped to {p:?}");
        }
    }

    #[test]
    fn rasterize_fills_expected_pixels() {
        let model = MinimapModel {
            bounds: Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0)),
            blocks: vec![(
                Rect::from_min_max(Pos2::ZERO, Pos2::new(50.0, 50.0)),
                Color32::RED,
            )],
            viewport: Rect::from_min_max(Pos2::ZERO, Pos2::new(10.0, 10.0)),
            generation: 0,
        };
        let image = rasterize_blocks(&model, [10, 10]);
        assert_eq!(image.pixels[0], Color32::RED);
        assert_eq!(image.pixels[4 * 10 + 4], Color32::RED);
        assert_eq!(image.pixels[9 * 10 + 9], Color32::TRANSPARENT);
    }
}
