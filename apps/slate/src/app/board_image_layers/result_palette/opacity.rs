//! Transient opacity scrubs over three cached raster groups. Raster work runs
//! only when the focused source changes; pointer movement changes GPU alpha.

use super::super::*;
use std::collections::HashSet;
use std::sync::Arc;

#[cfg(test)]
#[path = "opacity_tests.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct LayerOpacityDrag {
    pub image: NodeId,
    pub result: ImageResultId,
    pub layer: PaintLayerId,
    pub value: f32,
}

#[derive(Clone)]
pub(super) struct OpacityPreview {
    image: NodeId,
    result: Arc<ImageResultId>,
    layer: PaintLayerId,
    source: u128,
    generation: u64,
    bucket: u32,
    keys: [u128; 3],
    present: [bool; 3],
    settling: bool,
    pub textures: [Option<egui::TextureHandle>; 3],
    pub opacity: f32,
}

fn cache_id() -> egui::Id {
    egui::Id::new("image-layer-opacity-preview")
}

fn jobs_id() -> egui::Id {
    cache_id().with("pending")
}

pub(super) fn opacity_cancelled_id(tab: u64, image: NodeId) -> egui::Id {
    cache_id().with(("cancelled", tab, image.0))
}

fn preview_source(
    host: &Node,
    img: &ImageNode,
    result: &ImageResultId,
    gen: u64,
    bucket: u32,
    layer: PaintLayerId,
) -> u128 {
    paint_layer_cache_key(host, img, result, gen, bucket)
        .wrapping_mul(31)
        .wrapping_add(layer.0 as u128)
}

impl SlateApp {
    pub(crate) fn cancel_image_layer_opacity(&mut self) -> bool {
        self.layer_opacity_drag.take().is_some()
    }

    pub(crate) fn cancel_image_layer_opacity_for_escape(&mut self, ctx: &egui::Context) {
        if let Some(drag) = self.layer_opacity_drag.take() {
            ctx.data_mut(|data| {
                data.insert_temp(opacity_cancelled_id(self.tab().id, drag.image), true)
            });
        }
    }

    pub(crate) fn finish_image_layer_opacity(&mut self, ctx: &egui::Context) {
        let Some(drag) = self.layer_opacity_drag.take() else {
            return;
        };
        if self.image_result_id(drag.image).as_ref() != Some(&drag.result) {
            return;
        }
        if !self.set_paint_layer_opacity_by_id(drag.image, &drag.result, drag.layer, drag.value) {
            return;
        }
        // The groups exclude this layer's opacity, so the commit does not
        // invalidate them. Keep the last preview visible as the host refreshes.
        let Some(mut cache) = ctx.data(|data| data.get_temp::<OpacityPreview>(cache_id())) else {
            return;
        };
        if cache.image != drag.image
            || cache.result.as_ref() != &drag.result
            || cache.layer != drag.layer
        {
            return;
        }
        let Some(host) = self.doc().scene.node(drag.image) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        cache.generation = self.scene_gen;
        cache.settling = true;
        cache.source = preview_source(
            host,
            img,
            &drag.result,
            self.scene_gen,
            cache.bucket,
            drag.layer,
        );
        ctx.data_mut(|data| data.insert_temp(cache_id(), cache));
    }

    /// Prewarm only the focused layer/result. At most three textures are kept.
    pub(super) fn image_layer_opacity_preview(
        &mut self,
        ctx: &egui::Context,
        host: &Node,
        img: &ImageNode,
        result: &ImageResultId,
        full_bucket: u32,
        full_size: (u32, u32),
    ) -> Option<OpacityPreview> {
        let session = self.image_paint.as_ref().filter(|session| {
            session.image == host.id
                && &session.result == result
                && self.shape_properties.panel
                    == Some(super::super::super::board_properties::Panel::Layers)
        })?;
        let index = match session.focus {
            ImageStripFocus::Layer(index) => index,
            _ => return None,
        };
        let raw = Self::session_layer_raw_index(img, result, index)?;
        let active = Self::displayed_layers(img, result).get(raw)?;
        // Interactive previews are bounded independently of the final host
        // raster. The full-resolution composite resumes after release.
        let bucket = full_bucket.min(1024);
        let ratio = bucket as f32 / full_bucket.max(1) as f32;
        let size = (
            (full_size.0 as f32 * ratio).ceil().max(1.0) as u32,
            (full_size.1 as f32 * ratio).ceil().max(1.0) as u32,
        );
        let source = preview_source(host, img, result, self.scene_gen, bucket, active.id);
        let mut cache = ctx.data(|data| data.get_temp::<OpacityPreview>(cache_id()));
        if cache.as_ref().is_none_or(|cache| cache.source != source) {
            let layers = Self::renderer_layers(img, result);
            let at = layers.iter().position(|layer| layer.id == active.id)?;
            cache = Some(OpacityPreview {
                image: host.id,
                result: Arc::new(result.clone()),
                layer: active.id,
                source,
                generation: self.scene_gen,
                bucket,
                keys: std::array::from_fn(|part| {
                    source.wrapping_mul(31).wrapping_add(part as u128 + 1)
                }),
                present: [
                    layers[..at]
                        .iter()
                        .any(|layer| layer.visible && !layer.nodes.is_empty()),
                    active.visible && !active.nodes.is_empty(),
                    layers[at + 1..]
                        .iter()
                        .any(|layer| layer.visible && !layer.nodes.is_empty()),
                ],
                textures: std::array::from_fn(|_| None),
                opacity: active.opacity,
                settling: false,
            });
        }
        let mut cache = cache?;
        let mut source_doc = None;
        for part in 0..3 {
            if !cache.present[part]
                || cache.textures[part].is_some()
                || !self.image_rasters.needs_host(host.id, cache.keys[part])
            {
                continue;
            }
            let mut layers = Self::renderer_layers(img, result);
            let at = layers.iter().position(|layer| layer.id == active.id)?;
            layers = match part {
                0 => {
                    layers.truncate(at);
                    layers
                }
                1 => vec![PaintLayer {
                    opacity: 1.0,
                    ..active.clone()
                }],
                _ => layers.split_off(at + 1),
            };
            let mut image = img.clone();
            image.paint_layers = layers;
            image.result_layers.clear();
            let doc = source_doc
                .get_or_insert_with(|| {
                    let mut sources = img.clone();
                    sources.paint_layers = Self::renderer_layers(img, result);
                    sources.result_layers.clear();
                    super::super::raster::snapshot_for_image(self.doc(), &sources)
                })
                .clone();
            self.image_rasters.request_host(
                host.id,
                cache.keys[part],
                super::super::raster::RasterInput::new(
                    host.clone(),
                    image,
                    doc,
                    self.tab().path.clone(),
                    size,
                ),
                ctx.clone(),
            );
            ctx.data_mut(|data| {
                let pending = data.get_temp_mut_or_default::<HashSet<(NodeId, u128)>>(jobs_id());
                pending.insert((host.id, cache.keys[part]));
            });
        }
        let drag = self.layer_opacity_drag.as_ref().filter(|drag| {
            drag.image == host.id && &drag.result == result && drag.layer == active.id
        });
        cache.opacity = drag.map_or(active.opacity, |drag| drag.value);
        if self
            .paint_layer_texture_cache
            .get(&host.id)
            .is_some_and(|entry| {
                entry.key == paint_layer_cache_key(host, img, result, self.scene_gen, full_bucket)
            })
        {
            cache.settling = false;
        }
        let ready = (0..3).all(|part| !cache.present[part] || cache.textures[part].is_some());
        let showing = drag.is_some() || cache.settling;
        ctx.data_mut(|data| data.insert_temp(cache_id(), cache.clone()));
        (ready && showing).then_some(cache)
    }

    /// Consume preview completions before the ordinary host-composite cache.
    /// Retired requests are discarded even after focus or result has changed.
    pub(super) fn accept_image_layer_opacity_raster(
        &self,
        ctx: &egui::Context,
        node: NodeId,
        key: u128,
        rgba: Option<&[u8]>,
        size: [usize; 2],
    ) -> bool {
        let pending = ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<HashSet<(NodeId, u128)>>(jobs_id())
                .remove(&(node, key))
        });
        if !pending {
            return false;
        }
        let Some(mut cache) = ctx.data(|data| data.get_temp::<OpacityPreview>(cache_id())) else {
            return true;
        };
        if cache.image != node
            || cache.generation != self.scene_gen
            || self.image_result_id(node).as_ref() != Some(cache.result.as_ref())
        {
            return true;
        }
        if let Some(part) = cache.keys.iter().position(|candidate| *candidate == key) {
            if let Some(rgba) =
                rgba.filter(|rgba| rgba.len() == size[0].saturating_mul(size[1]).saturating_mul(4))
            {
                let pixels = egui::ColorImage::from_rgba_unmultiplied(size, rgba);
                cache.textures[part] = Some(ctx.load_texture(
                    format!("paint-opacity-{}-{part}", node.0),
                    pixels,
                    egui::TextureOptions::LINEAR,
                ));
            } else {
                cache.present[part] = false;
            }
            ctx.data_mut(|data| data.insert_temp(cache_id(), cache));
        }
        true
    }

    pub(crate) fn set_paint_layer_opacity_by_id(
        &mut self,
        image: NodeId,
        result: &ImageResultId,
        layer_id: PaintLayerId,
        opacity: f32,
    ) -> bool {
        if self.refuse_read_only_edit() || self.image_result_id(image).as_ref() != Some(result) {
            return false;
        }
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return false;
        };
        let NodeKind::Image(old_img) = &before.kind else {
            return false;
        };
        let Some(old) = Self::selected_layers(old_img, result)
            .iter()
            .find(|layer| layer.id == layer_id)
        else {
            return false;
        };
        let opacity = opacity.clamp(0.0, 1.0);
        if !opacity.is_finite() || (old.opacity - opacity).abs() < 1e-4 {
            return false;
        }
        let mut after = before.clone();
        let NodeKind::Image(img) = &mut after.kind else {
            return false;
        };
        Self::prepare_result_stack(img, result.clone());
        let Some(layer) = Self::selected_layers_mut(img, result)
            .iter_mut()
            .find(|layer| layer.id == layer_id)
        else {
            return false;
        };
        layer.opacity = opacity;
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return false;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.layer.opacity"),
            Some(format!("layer:{}", layer_id.0)),
        );
        true
    }
}
