use super::raster::{RasterInput, RasterTarget, ThumbKey};
use super::*;

#[path = "result_palette/focus.rs"]
mod focus;
#[path = "result_palette/ghost.rs"]
mod ghost;
#[path = "result_palette/opacity.rs"]
pub(crate) mod opacity;
#[path = "result_palette/palette_ui.rs"]
mod palette_ui;

impl SlateApp {
    /// Rename a layer by identity so list reordering cannot redirect the edit.
    pub(crate) fn rename_paint_layer(
        &mut self,
        image: NodeId,
        layer_id: PaintLayerId,
        name: String,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return false;
        };
        let NodeKind::Image(ref old_img) = before.kind else {
            return false;
        };
        let Some(result) = self.image_result_id(image) else {
            return false;
        };
        if !Self::selected_layers(old_img, &result)
            .iter()
            .any(|layer| layer.id == layer_id)
        {
            return false;
        }
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        Self::prepare_result_stack(img, result.clone());
        let stack = Self::selected_stack_mut(img, &result);
        stack.normalize_metadata();
        let Some(meta) = stack
            .metadata
            .iter_mut()
            .find(|meta| meta.layer == layer_id)
        else {
            return false;
        };
        let name = name.trim();
        if meta.name == name {
            return false;
        }
        meta.name = name.to_owned();
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }])
    }

    /// Move a layer before another stable layer id (or to the end when None).
    pub(crate) fn reorder_paint_layer(
        &mut self,
        image: NodeId,
        layer_id: PaintLayerId,
        before_layer: Option<PaintLayerId>,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return false;
        };
        let NodeKind::Image(ref old_img) = before.kind else {
            return false;
        };
        let Some(result) = self.image_result_id(image) else {
            return false;
        };
        if Self::selected_layers(old_img, &result)
            .iter()
            .all(|layer| layer.id != layer_id)
        {
            return false;
        }
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        Self::prepare_result_stack(img, result.clone());
        let stack = Self::selected_stack_mut(img, &result);
        let Some(from) = stack.layers.iter().position(|layer| layer.id == layer_id) else {
            return false;
        };
        let to = before_layer
            .and_then(|id| stack.layers.iter().position(|layer| layer.id == id))
            .map(|to| if from < to { to.saturating_sub(1) } else { to })
            .unwrap_or_else(|| stack.layers.len().saturating_sub(1));
        if !stack.move_layer(from, to) {
            return false;
        }
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }])
    }

    /// Duplicate by stable id. Child nodes and prompt references receive fresh
    /// scene ids before the new layer is journaled into the selected result.
    pub(crate) fn duplicate_paint_layer_before(
        &mut self,
        image: NodeId,
        layer_id: PaintLayerId,
        before_layer: Option<PaintLayerId>,
    ) -> Option<PaintLayerId> {
        if self.refuse_read_only_edit() {
            return None;
        }
        let before = self.doc().scene.node(image)?.clone();
        let NodeKind::Image(ref old_img) = before.kind else {
            return None;
        };
        let result = self.image_result_id(image)?;
        let source = Self::selected_layers(old_img, &result)
            .iter()
            .find(|layer| layer.id == layer_id)?
            .clone();
        let source_name = result_layers(old_img, &result)
            .and_then(|stack| stack.layer_meta(layer_id))
            .map(|meta| meta.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Layer".into());
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return None;
        };
        Self::prepare_result_stack(img, result.clone());
        let mut copied_img = ImageNode::new(img.item);
        let copied = source.clone();
        copied_img.result_layers.push(ImageResultLayers {
            result: result.clone(),
            layers: vec![copied],
            metadata: Vec::new(),
        });
        let duplicate_id = Self::next_paint_layer_id(img);
        slate_doc::image_paint::fresh_layer_node_ids(&mut copied_img, || {
            self.doc_mut()
                .scene
                .build_node(
                    WorldRect::new(0.0, 0.0, 1.0, 1.0),
                    NodeKind::Image(ImageNode::new(slate_doc::ItemId::NONE)),
                )
                .id
        });
        let mut duplicate = copied_img.result_layers.pop()?.layers.pop()?;
        duplicate.id = duplicate_id;
        let stack = Self::selected_stack_mut(img, &result);
        let index = stack.add_layer(duplicate.clone());
        if let Some(target) = before_layer {
            if let Some(target_index) = stack.layers.iter().position(|layer| layer.id == target) {
                stack.move_layer(index, target_index);
            }
        }
        let meta = stack
            .metadata
            .iter_mut()
            .find(|meta| meta.layer == duplicate.id)?;
        meta.name = format!("{source_name} copy");
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return None;
        }
        let index = Self::selected_layers(
            match &self.doc().scene.node(image)?.kind {
                NodeKind::Image(img) => img,
                _ => return None,
            },
            &result,
        )
        .iter()
        .position(|layer| layer.id == duplicate.id)
        .unwrap_or(index);
        if let Some(session) = &mut self.image_paint {
            if session.image == image && session.result == result {
                session.layer_index = index;
                session.focus = ImageStripFocus::Layer(index);
            }
        }
        Some(duplicate.id)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_image_paint_layers(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        host: &Node,
        img: &ImageNode,
        outline: &[Pos2],
        _srect: egui::Rect,
        host_alpha: f32,
        _z: f32,
    ) {
        let Some(result) = self.image_result_id(host.id) else {
            return;
        };
        let layers = Self::displayed_layers(img, &result);
        if layers.is_empty() {
            return;
        }
        let ppp = ui.ctx().pixels_per_point();
        let longest_px = (host.rect.w.max(host.rect.h) * xf.z * ppp)
            .ceil()
            .clamp(1.0, 4096.0);
        let zoom_bucket = (longest_px as u32).max(1).next_power_of_two();
        let key = paint_layer_cache_key(host, img, &result, self.scene_gen, zoom_bucket);
        let aspect = host.rect.w.max(1e-6) / host.rect.h.max(1e-6);
        let size = if aspect >= 1.0 {
            (zoom_bucket, (zoom_bucket as f32 / aspect).ceil() as u32)
        } else {
            ((zoom_bucket as f32 * aspect).ceil() as u32, zoom_bucket)
        };
        let preview =
            self.image_layer_opacity_preview(ui.ctx(), host, img, &result, zoom_bucket, size);
        let stale = self
            .paint_layer_texture_cache
            .get(&host.id)
            .is_none_or(|entry| entry.key != key);
        if stale && self.image_rasters.needs_host(host.id, key) {
            let mut display_img = img.clone();
            display_img.paint_layers = Self::renderer_layers(img, &result);
            display_img.result_layers.clear();
            let doc = super::raster::snapshot_for_image(self.doc(), &display_img);
            self.image_rasters.request_host(
                host.id,
                key,
                RasterInput::new(
                    host.clone(),
                    display_img,
                    doc,
                    self.tab().path.clone(),
                    size,
                ),
                ui.ctx().clone(),
            );
        }
        if let Some(preview) = preview {
            for (part, texture) in preview.textures.iter().enumerate() {
                if let Some(texture) = texture {
                    let alpha = if part == 1 {
                        host_alpha * preview.opacity
                    } else {
                        host_alpha
                    };
                    self.paint_image_layer_texture(painter, xf, texture, host, outline, alpha);
                }
            }
            return;
        }
        let Some(texture) = self
            .paint_layer_texture_cache
            .get(&host.id)
            .map(|entry| entry.texture.clone())
        else {
            return;
        };
        self.paint_image_layer_texture(painter, xf, &texture, host, outline, host_alpha);
    }

    fn paint_image_layer_texture(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        texture: &egui::TextureHandle,
        host: &Node,
        outline: &[Pos2],
        alpha: f32,
    ) {
        let tint = Color32::WHITE.gamma_multiply(alpha.clamp(0.0, 1.0));
        if let Some(clip) = &host.clip {
            super::super::board::paint_clipped_texture(
                &mut self.path_mesh_cache,
                painter,
                xf,
                texture,
                host,
                clip,
                slate_doc::scene::Crop::full(),
                slate_doc::scene::Mirror::default(),
                tint,
            );
        } else if outline.len() >= 3 {
            let outline_world = outline
                .iter()
                .map(|point| {
                    let world = xf.s2w(*point);
                    (world.x, world.y)
                })
                .collect::<Vec<_>>();
            super::super::board::textured_polygon_world(
                painter,
                texture,
                outline,
                &outline_world,
                host.rect,
                slate_doc::scene::Crop::full(),
                tint,
                host.rotation_deg,
            );
        }
    }

    pub(crate) fn poll_image_rasters(&mut self, ctx: &egui::Context) {
        for completion in self.image_rasters.drain() {
            if let RasterTarget::Host { node, key } = &completion.target {
                if self.accept_image_layer_opacity_raster(
                    ctx,
                    *node,
                    *key,
                    completion.rgba.as_deref(),
                    [completion.w as usize, completion.h as usize],
                ) {
                    continue;
                }
            }
            let Some(rgba) = completion.rgba else {
                continue;
            };
            let size = [completion.w as usize, completion.h as usize];
            if rgba.len() != size[0].saturating_mul(size[1]).saturating_mul(4) {
                continue;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            match completion.target {
                RasterTarget::Host { node, key } => {
                    if self.doc().scene.node(node).is_none() {
                        continue;
                    }
                    let texture = ctx.load_texture(
                        format!("paint-layer-{}", node.0),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.paint_layer_texture_cache
                        .insert(node, PaintLayerTextureCache { key, texture });
                }
                RasterTarget::Thumb { key, gen } => {
                    if gen != self.scene_gen || !self.thumb_layer_exists(&key) {
                        continue;
                    }
                    let texture = ctx.load_texture(
                        format!("paint-layer-thumb-{}-{:?}-{}", key.0 .0, key.1, key.2 .0),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.paint_layer_thumbs
                        .insert(key, PaintLayerThumb { gen, texture });
                }
            }
        }
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

    pub fn set_paint_layer_opacity(&mut self, image: NodeId, index: usize, opacity: f32) {
        if self.refuse_read_only_edit() {
            return;
        }
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return;
        };
        let NodeKind::Image(ref img) = before.kind else {
            return;
        };
        let Some(result) = self.image_result_id(image) else {
            return;
        };
        if self
            .image_paint
            .as_ref()
            .is_some_and(|session| session.image == image && session.result != result)
        {
            return;
        }
        let Some(raw_index) = Self::session_layer_raw_index(img, &result, index) else {
            return;
        };
        let Some(layer) = Self::selected_layers(img, &result).get(raw_index) else {
            return;
        };
        self.set_paint_layer_opacity_by_id(image, &result, layer.id, opacity);
    }

    pub fn delete_paint_layer(&mut self, image: NodeId, index: usize) {
        if self.refuse_read_only_edit() {
            return;
        }
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return;
        };
        let NodeKind::Image(ref img) = before.kind else {
            return;
        };
        let Some(result) = self.image_result_id(image) else {
            return;
        };
        if self
            .image_paint
            .as_ref()
            .is_some_and(|session| session.image == image && session.result != result)
        {
            return;
        }
        let Some(raw_index) = Self::session_layer_raw_index(img, &result, index) else {
            return;
        };
        let Some(layer_id) = Self::selected_layers(img, &result)
            .get(raw_index)
            .map(|layer| layer.id)
        else {
            return;
        };
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return;
        };
        Self::prepare_result_stack(img, result.clone());
        let stack = Self::selected_stack_mut(img, &result);
        let Some(current_index) = stack.layers.iter().position(|layer| layer.id == layer_id) else {
            return;
        };
        stack.remove_layer(current_index);
        let remaining = stack.layers.len();
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.layer.delete"),
            Some(format!("layer:{index}")),
        );
        if let Some(session) = &mut self.image_paint {
            if session.image == image {
                if remaining == 0 {
                    session.layer_index = 0;
                    session.focus = ImageStripFocus::Filter;
                } else {
                    session.layer_index = session.layer_index.min(remaining - 1);
                    session.focus = ImageStripFocus::Layer(session.layer_index);
                }
            }
        }
    }
}
