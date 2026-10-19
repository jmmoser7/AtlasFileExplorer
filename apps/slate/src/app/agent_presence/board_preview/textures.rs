//! Pixel preparation calls existing image/filter/brush owners, never opens sources.
use super::*;
use crate::app::imagefx;
use std::sync::Arc;

pub(super) enum Source {
    Pixels(Arc<egui::ColorImage>),
    Adjusted(Arc<egui::ColorImage>, slate_doc::scene::ImageAdjust, usize),
    Stroke(Box<Node>, f32),
}
impl Source {
    pub(super) fn pixels(self) -> Result<egui::ColorImage, String> {
        let image = match self {
            Self::Pixels(pixels) => (*pixels).clone(),
            Self::Adjusted(pixels, adjust, edge) => {
                imagefx::adjusted_thumbnail(&pixels, edge, &adjust)
            }
            Self::Stroke(node, pixel) => {
                let NodeKind::Shape(shape) = &node.kind else {
                    return Err("Stroke preview changed".into());
                };
                let path = shape.path.as_ref().ok_or("Stroke path is unavailable")?;
                let stamp = crate::app::board_path::stroke_stamp(&node, shape, path, pixel)
                    .ok_or("Stroke pixels are unavailable")?;
                egui::ColorImage::from_rgba_unmultiplied(
                    [stamp.width as usize, stamp.height as usize],
                    &stamp.rgba,
                )
            }
        };
        if image.pixels.len() > 16 * 1024 * 1024 {
            return Err("Preview texture exceeds the pixel limit".into());
        }
        Ok(image)
    }
}

pub(super) fn remember(
    app: &mut SlateApp,
    ctx: &egui::Context,
    xf: &BoardXf,
    node: &Node,
    sources: &mut HashMap<egui::TextureId, Source>,
) -> Result<bool, String> {
    if let NodeKind::Image(image) = &node.kind {
        if app
            .image_item_path(node.id)
            .as_deref()
            .is_some_and(atlas_core::secrets::is_machine_private)
        {
            return Err(
                "Machine-private source content is never available to board previews".into(),
            );
        }
        let desired = xf.rect_w2s(node.rect).size().max_elem();
        let generated = image.agent.is_some() && image.item.is_none();
        let (texture, key) = if generated {
            let Some(path) = app.agent_shown_path(node.id) else {
                return Ok(false);
            };
            let key = crate::app::preview::linked_image_key(&path, "shown");
            (
                app.agent_picture_texture(ctx, &path, &image.adjust, desired),
                key,
            )
        } else {
            let Some((key, _, _, _)) = app.resolved_item_preview(image.item) else {
                return Ok(false);
            };
            (
                app.board_texture(ctx, node.id, image.item, &image.adjust, desired),
                key,
            )
        };
        if let Some(texture) = texture {
            let pixels = app
                .preview_cache
                .get(&key)
                .map(|e| Arc::clone(&e.pixels))
                .or_else(|| app.thumb_pixels.get(&key).cloned());
            if let Some(pixels) = pixels {
                if pixels.pixels.len() > 16 * 1024 * 1024 {
                    return Err("Preview source exceeds the pixel budget".into());
                }
                let source = if image.adjust.is_identity() {
                    Source::Pixels(pixels)
                } else {
                    Source::Adjusted(
                        pixels,
                        image.adjust,
                        texture.size()[0].max(texture.size()[1]),
                    )
                };
                sources.insert(texture.id(), source);
            } else {
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    }
    if let NodeKind::Shape(shape) = &node.kind {
        if shape.path.is_some() {
            if let Some((_, gpu)) = app.brush_stamps.get(&app.stroke_cache_id(node.id)) {
                sources.insert(
                    gpu.tex.id(),
                    Source::Stroke(Box::new(node.clone()), gpu.wanted_pixel),
                );
            }
        }
    }
    if sources.len() > 256 {
        return Err("Too many textures for one preview; choose a smaller frame".into());
    }
    Ok(true)
}
