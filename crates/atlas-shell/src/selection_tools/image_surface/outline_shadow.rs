//! A bounded, translation-independent soft cast for a selected solid.
use super::*;
use egui::emath::TSTransform;
use std::sync::{mpsc, Mutex};

fn cache_id() -> Id {
    Id::new("atlas-shell.outline-interaction-shadow")
}

struct ShadowInput {
    generation: u64,
    owner: Id,
    outline: Vec<Vec2>,
    zoom: f32,
    pixels_per_point: f32,
    dark_mode: bool,
}

impl ShadowInput {
    fn matches(
        &self,
        owner: Id,
        outline: &[Pos2],
        _transform: TSTransform,
        ppp: f32,
        dark: bool,
    ) -> bool {
        self.owner == owner
            && self.pixels_per_point == ppp
            && self.dark_mode == dark
            && self.outline.len() == outline.len()
            && self.outline.iter().zip(outline).all(|(a, b)| {
                // Floating screen projections can differ after translation.
                (*a - (*b - outline[0])).length_sq() <= (0.025 / (ppp * self.zoom)).powi(2)
            })
    }
}

struct CachedOutlineShadow {
    input: Arc<ShadowInput>,
    texture: egui::TextureHandle,
    rect: Rect,
}

struct ShadowResult {
    input: Arc<ShadowInput>,
    pixels: Option<vector_ink::StampImage>,
}
struct ShadowJob {
    rx: mpsc::Receiver<ShadowResult>,
}

#[derive(Default)]
struct ShadowState {
    desired: Option<Arc<ShadowInput>>,
    cached: Option<Arc<CachedOutlineShadow>>,
    job: Option<ShadowJob>,
    generation: u64,
    attempted: u64,
}

/// Same shadow tokens as rectangular media, with the existing vector-ink
/// radial raster owner. One worker and one texture bound this feature.
/// Geometry/DPI/theme changes replace the pending request. Camera pan and zoom
/// project the accepted board-local texture; resize retains that same-owner,
/// same-style shadow at the current bounds until the replacement is ready.
pub fn paint_outline_interaction_shadow(
    painter: &egui::Painter,
    owner: Id,
    outline: &[Pos2],
    transform: TSTransform,
    theme: Palette,
) {
    if outline.len() < 3 || !transform.scaling.is_finite() || transform.scaling <= 0.0 {
        return;
    }
    let ctx = painter.ctx();
    let ppp = ctx.pixels_per_point();
    let state = ctx.data_mut(|d| {
        if let Some(state) = d.get_temp::<Arc<Mutex<ShadowState>>>(cache_id()) {
            state
        } else {
            let state = Arc::new(Mutex::new(ShadowState::default()));
            d.insert_temp(cache_id(), state.clone());
            state
        }
    });
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    if state
        .desired
        .as_ref()
        .is_none_or(|i| !i.matches(owner, outline, transform, ppp, theme.dark_mode))
    {
        state.generation = state.generation.wrapping_add(1);
        state.desired = Some(Arc::new(ShadowInput {
            generation: state.generation,
            owner,
            outline: outline.iter().map(|p| *p - outline[0]).collect(),
            zoom: transform.scaling,
            pixels_per_point: ppp,
            dark_mode: theme.dark_mode,
        }));
    }
    let result = state.job.as_ref().map(|job| job.rx.try_recv());
    match result {
        Some(Ok(result)) => {
            state.job = None;
            if state
                .desired
                .as_ref()
                .is_some_and(|i| i.generation == result.input.generation)
            {
                if let Some(pixels) = result.pixels {
                    let texture = ctx.load_texture(
                        "atlas-shell-field-shadow",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [pixels.width as usize, pixels.height as usize],
                            &pixels.rgba,
                        ),
                        egui::TextureOptions::LINEAR,
                    );
                    let raster_zoom = result.input.zoom;
                    let rect = Rect::from_min_size(
                        Pos2::new(
                            pixels.origin[0] / raster_zoom,
                            pixels.origin[1] / raster_zoom + IMAGE_SHADOW_OFFSET,
                        ),
                        Vec2::new(
                            pixels.width as f32 * pixels.pixel / raster_zoom,
                            pixels.height as f32 * pixels.pixel / raster_zoom,
                        ),
                    );
                    state.cached = Some(Arc::new(CachedOutlineShadow {
                        input: result.input,
                        texture,
                        rect,
                    }));
                }
            }
        }
        Some(Err(mpsc::TryRecvError::Disconnected)) => {
            state.job = None;
        }
        _ => {}
    }
    if state.job.is_none() {
        let input = state.desired.as_ref().unwrap().clone();
        if state.attempted != input.generation {
            state.attempted = input.generation;
            let (tx, rx) = mpsc::sync_channel(1);
            if std::thread::Builder::new()
                .name("atlas-field-shadow".into())
                .spawn(move || {
                    let pixels = rasterize(&input);
                    let _ = tx.send(ShadowResult { input, pixels });
                })
                .is_ok()
            {
                state.job = Some(ShadowJob { rx });
            }
        }
    }
    let cached = state
        .cached
        .clone()
        .filter(|c| c.input.owner == owner && c.input.dark_mode == theme.dark_mode);
    let pending = state.job.is_some();
    drop(state);
    if let Some(cached) = cached {
        painter.image(
            cached.texture.id(),
            projected_rect(&cached, outline, transform),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if pending {
        ctx.request_repaint();
    }
}

/// Preserve the accepted shadow's board-unit blur margin while following current
/// host bounds. The old texture is only a same-host interim during raster work.
fn projected_rect(cached: &CachedOutlineShadow, outline: &[Pos2], transform: TSTransform) -> Rect {
    let before = cached
        .input
        .outline
        .iter()
        .fold(Rect::NOTHING, |bounds, p| {
            bounds.union(Rect::from_min_max(p.to_pos2(), p.to_pos2()))
        });
    let after = outline.iter().fold(Rect::NOTHING, |bounds, p| {
        let p = (*p - outline[0]).to_pos2();
        bounds.union(Rect::from_min_max(p, p))
    });
    let rect = Rect::from_min_max(
        after.min + (cached.rect.min - before.min),
        after.max + (cached.rect.max - before.max),
    );
    transform * rect.translate(outline[0].to_vec2())
}

fn rasterize(input: &ShadowInput) -> Option<vector_ink::StampImage> {
    let blur = canvas_scale::px(IMAGE_SHADOW_BLUR, input.zoom);
    let mut points: Vec<[f32; 2]> = input
        .outline
        .iter()
        .map(|p| [p.x * input.zoom, p.y * input.zoom])
        .collect();
    let mut width = 0.0_f32;
    let mut height = 0.0_f32;
    for axis in 0..2 {
        let low = points.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min);
        let high = points
            .iter()
            .map(|p| p[axis])
            .fold(f32::NEG_INFINITY, f32::max);
        if axis == 0 {
            width = high - low + 2.0 * blur;
        } else {
            height = high - low + 2.0 * blur;
        }
    }
    // The raster owner adds a one-pixel pad on each side.
    const MAX_PIXELS: f32 = 64_000.0;
    const MAX_SIDE: f32 = 510.0;
    let pixel = (1.0 / input.pixels_per_point)
        .max((width * height / MAX_PIXELS).sqrt())
        .max(width / MAX_SIDE)
        .max(height / MAX_SIDE);
    if points.first() != points.last() {
        points.push(points[0]);
    }
    let alpha = if input.dark_mode {
        IMAGE_SHADOW_DARK_ALPHA
    } else {
        IMAGE_SHADOW_LIGHT_ALPHA
    };
    let style = vector_ink::StampStyle {
        diameter: blur * 2.0,
        softness: 1.0,
        rgba: [0, 0, 0, (alpha * 255.0).round() as u8],
        grain: vector_ink::Grain::Smooth,
    };
    vector_ink::stamp_contours_at(&[points], style, pixel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &egui::Context, outline: &[Pos2], transform: TSTransform) -> egui::FullOutput {
        ctx.run(egui::RawInput::default(), |ctx| {
            paint_outline_interaction_shadow(
                &ctx.layer_painter(egui::LayerId::background()),
                Id::new("field"),
                outline,
                transform,
                Palette::for_mode(false),
            );
        })
    }

    fn settled(
        ctx: &egui::Context,
        outline: &[Pos2],
        transform: TSTransform,
    ) -> Arc<CachedOutlineShadow> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let _ = frame(ctx, outline, transform);
            let state = ctx
                .data(|d| d.get_temp::<Arc<Mutex<ShadowState>>>(cache_id()))
                .unwrap();
            let state = state.lock().unwrap();
            if let Some(cached) = state.cached.as_ref().filter(|c| {
                state
                    .desired
                    .as_ref()
                    .is_some_and(|i| i.generation == c.input.generation)
            }) {
                return cached.clone();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "shadow worker must finish"
            );
            drop(state);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn translating_field_outline_reuses_shadow_texture_without_upload_or_worker() {
        let ctx = egui::Context::default();
        let outline = [
            Pos2::new(20.0, 30.0),
            Pos2::new(140.0, 30.0),
            Pos2::new(140.0, 130.0),
            Pos2::new(20.0, 130.0),
        ];
        let before = settled(&ctx, &outline, TSTransform::IDENTITY);
        let transform = TSTransform::from_translation(Vec2::new(357.125, 241.75));
        let output = frame(&ctx, &outline, transform);
        let after = settled(&ctx, &outline, transform);
        assert!(
            Arc::ptr_eq(&before, &after),
            "pan retains the cached shadow"
        );
        assert!(!output
            .textures_delta
            .set
            .iter()
            .any(|(id, _)| *id == before.texture.id()));
        let state = ctx
            .data(|d| d.get_temp::<Arc<Mutex<ShadowState>>>(cache_id()))
            .unwrap();
        assert!(
            state.lock().unwrap().job.is_none(),
            "pan schedules no raster work"
        );
        let translated = outline.map(|p| p + Vec2::new(100.0, -200.0));
        let translated_cache = settled(&ctx, &translated, transform);
        assert!(
            Arc::ptr_eq(&after, &translated_cache),
            "outline translation is also placement only"
        );
        let zoom = TSTransform::new(transform.translation, 1.2);
        let pending = frame(&ctx, &translated, zoom);
        assert!(pending.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Mesh(mesh) if mesh.texture_id == after.texture.id())), "zoom keeps the accepted shadow visible");
        let state = ctx
            .data(|d| d.get_temp::<Arc<Mutex<ShadowState>>>(cache_id()))
            .unwrap();
        {
            let state = state.lock().unwrap();
            assert_eq!(
                state.generation, after.input.generation,
                "zoom cannot change the geometry generation"
            );
            assert_eq!(
                state.attempted, after.input.generation,
                "zoom schedules no worker"
            );
            assert!(state.job.is_none());
        }
        assert!(!pending
            .textures_delta
            .set
            .iter()
            .any(|(id, _)| *id == after.texture.id()));
        let zoomed = settled(&ctx, &translated, zoom);
        assert!(
            Arc::ptr_eq(&after, &zoomed),
            "camera zoom only projects the accepted board-local shadow"
        );
        let changed = translated.map(|p| Pos2::new(p.x * 1.2, p.y));
        let edited = settled(&ctx, &changed, zoom);
        assert!(
            !Arc::ptr_eq(&zoomed, &edited),
            "an actual boundary edit invalidates the shadow"
        );
        let size = edited.texture.size();
        assert!(size[0] <= 512 && size[1] <= 512 && size[0] * size[1] <= 70_000);
    }
    #[test]
    fn pending_resize_keeps_same_host_shadow_at_current_zoom_and_bounds() {
        let ctx = egui::Context::default();
        let outline = [
            Pos2::new(20.0, 30.0),
            Pos2::new(140.0, 30.0),
            Pos2::new(140.0, 130.0),
            Pos2::new(20.0, 130.0),
        ];
        let cached = settled(&ctx, &outline, TSTransform::IDENTITY);
        let state = ctx
            .data(|d| d.get_temp::<Arc<Mutex<ShadowState>>>(cache_id()))
            .unwrap();
        // Hold the worker slot, making the pending frame deterministic.
        let (_tx, rx) = mpsc::sync_channel(1);
        state.lock().unwrap().job = Some(ShadowJob { rx });
        let resized = outline.map(|p| Pos2::new(p.x * 1.6, p.y * 0.8));
        let transform = TSTransform::new(Vec2::new(173.25, 81.75), 2.5);
        let output = frame(&ctx, &resized, transform);
        let painted = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == cached.texture.id() => {
                    Some(mesh.calc_bounds())
                }
                _ => None,
            })
            .expect("pending resize must retain its accepted shadow");
        assert_eq!(painted, projected_rect(&cached, &resized, transform));
        assert_ne!(
            painted,
            transform * cached.rect.translate(outline[0].to_vec2()),
            "the pending shadow follows current host dimensions, not the old rectangle"
        );
        for (owner, theme) in [
            (Id::new("other-node"), Palette::light()),
            (Id::new("field"), Palette::dark()),
        ] {
            let output = ctx.run(Default::default(), |ctx| {
                paint_outline_interaction_shadow(
                    &ctx.layer_painter(egui::LayerId::background()),
                    owner,
                    &resized,
                    transform,
                    theme,
                );
            });
            assert!(
                !output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Mesh(mesh) if mesh.texture_id == cached.texture.id())),
                "a retained shadow cannot cross node or palette identity"
            );
        }
    }
}
