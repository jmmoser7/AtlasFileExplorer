//! Bounded completion drain and validation before any layer GPU upload.

use super::super::raster::RasterTarget;
use super::*;
use crate::app::preview::upload::rgba_texture_bytes;

#[cfg(test)]
#[path = "uploads_tests.rs"]
mod tests;

impl SlateApp {
    pub(crate) fn poll_image_rasters(&mut self, ctx: &egui::Context) {
        let completions = self.image_rasters.drain(ctx);
        let mut changed = false;
        for completion in completions {
            let size = [completion.w as usize, completion.h as usize];
            if let RasterTarget::Host { node, key } = &completion.target {
                if self.accept_image_layer_opacity_raster(
                    ctx,
                    *node,
                    *key,
                    completion.rgba.as_deref(),
                    size,
                ) {
                    continue;
                }
            }
            let Some(rgba) = completion.rgba else {
                continue;
            };
            if rgba_texture_bytes(ctx, size, &rgba).is_none() {
                continue;
            }
            match completion.target {
                RasterTarget::Host { node, key } => {
                    // Editing/deleting a source while its worker ran must not
                    // upload an obsolete composite over the current one.
                    if !self.host_raster_current(node, key, completion.w.max(completion.h)) {
                        continue;
                    }
                    let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                    let texture = ctx.load_texture(
                        format!("paint-layer-{}", node.0),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.paint_layer_texture_cache
                        .insert(node, PaintLayerTextureCache { key, texture });
                    changed = true;
                }
                RasterTarget::Thumb { key, gen } => {
                    if gen != self.scene_gen || !self.thumb_layer_exists(&key) {
                        continue;
                    }
                    let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                    let texture = ctx.load_texture(
                        format!("paint-layer-thumb-{}-{:?}-{}", key.0 .0, key.1, key.2 .0),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.paint_layer_thumbs
                        .insert(key, PaintLayerThumb { gen, texture });
                    changed = true;
                }
            }
        }
        if changed {
            self.record_image_raster_residency(ctx);
        }
    }

    fn host_raster_current(&self, node: NodeId, key: u128, bucket: u32) -> bool {
        let Some(host) = self.doc().scene.node(node) else {
            return false;
        };
        let NodeKind::Image(img) = &host.kind else {
            return false;
        };
        let Some(result) = self.image_result_id(node) else {
            return false;
        };
        key == paint_layer_cache_key(host, img, &result, self.scene_gen, bucket)
    }

    fn thumb_layer_exists(&self, key: &ThumbKey) -> bool {
        self.doc()
            .scene
            .node(key.0)
            .is_some_and(|node| match &node.kind {
                NodeKind::Image(img) => Self::displayed_layers(img, &key.1)
                    .iter()
                    .any(|layer| layer.id == key.2),
                _ => false,
            })
    }

    /// Scan only after completions change the cache, and log only a change.
    /// This supplies crash evidence without a cache walk on idle/paint frames.
    fn record_image_raster_residency(&self, ctx: &egui::Context) {
        let sizes = self
            .paint_layer_texture_cache
            .values()
            .map(|e| e.texture.size())
            .chain(self.paint_layer_thumbs.values().map(|e| e.texture.size()));
        let (entries, bytes) = sizes.fold((0usize, 0usize), |(n, bytes), size| {
            (
                n + 1,
                bytes.saturating_add(size[0].saturating_mul(size[1]).saturating_mul(4)),
            )
        });
        let id = egui::Id::new("slate-image-raster-residency");
        let changed = ctx.data_mut(|data| {
            let previous = data.get_temp::<(usize, usize)>(id);
            data.insert_temp(id, (entries, bytes));
            previous != Some((entries, bytes))
        });
        if changed {
            atlas_core::session_log::event(
                "image_raster_residency",
                &format!("entries={entries} bytes={bytes}"),
            );
        }
    }
}
