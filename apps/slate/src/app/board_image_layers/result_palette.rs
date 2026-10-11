use super::*;

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
        let Some(from) = Self::selected_layers(old_img, &result)
            .iter()
            .position(|layer| layer.id == layer_id)
        else {
            return false;
        };
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        Self::prepare_result_stack(img, result.clone());
        let stack = Self::selected_stack_mut(img, &result);
        let to = before_layer
            .and_then(|id| stack.layers.iter().position(|layer| layer.id == id))
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
    pub(crate) fn duplicate_paint_layer(
        &mut self,
        image: NodeId,
        layer_id: PaintLayerId,
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
        if let Some(session) = &mut self.image_paint {
            if session.image == image && session.result == result {
                session.layer_index = index;
                session.focus = ImageStripFocus::Layer(index);
            }
        }
        Some(duplicate.id)
    }

    /// Paint layer controls inside the shared image blister and return the
    /// screen-space hit regions to the host palette.
    pub(crate) fn image_layer_palette_body(
        &mut self,
        ui: &mut egui::Ui,
        presentation: &atlas_shell::selection_tools::ImagePresentationLayout,
        image: NodeId,
        z: f32,
        theme: atlas_shell::Palette,
    ) -> Vec<egui::Rect> {
        let Some(session) = self.image_paint.clone().filter(|s| s.image == image) else {
            return Vec::new();
        };
        let Some(host) = self.doc().scene.node(image).cloned() else {
            return Vec::new();
        };
        let NodeKind::Image(img) = &host.kind else {
            return Vec::new();
        };
        if self.image_result_id(image).as_ref() != Some(&session.result) {
            return Vec::new();
        }
        let layers = Self::selected_layers(img, &session.result);
        let recents: Vec<[u8; 3]> = self
            .doc()
            .view
            .recent_colors
            .iter()
            .flatten()
            .take(ViewState::RECENT_COLOR_LIMIT)
            .copied()
            .collect();
        let layout = atlas_shell::selection_tools::image_layer_palette_layout(
            presentation,
            layers.len(),
            recents.len(),
            z,
        );
        let active = match session.focus {
            ImageStripFocus::Layer(i) if i < layers.len() => Some(i),
            _ => None,
        };
        let opacity = match self.layer_opacity_drag {
            Some((drag_image, layer, value)) if drag_image == image && Some(layer) == active => {
                value
            }
            _ => active.map_or(1.0, |i| layers[i].opacity),
        };
        let display_img = image_for_result(img, &session.result);
        let thumbs = self.paint_layer_thumbs(ui.ctx(), &host, &display_img, &session.result);
        let edit = atlas_shell::selection_tools::image_layer_palette(
            ui, &layout, &thumbs, active, opacity, &recents, z, theme,
        );
        let layer_ids: Vec<_> = layers.iter().map(|layer| layer.id).collect();
        for (index, layer_id) in layer_ids.iter().copied().enumerate() {
            let Some(center) = layout.layers.get(index).copied() else {
                continue;
            };
            let hit = egui::Rect::from_center_size(center, egui::Vec2::splat(layout.radius * 2.0));
            let rename_id = ui.id().with(("paint-layer-name", image.0, layer_id.0));
            let response = ui.interact(
                hit,
                ui.id().with(("paint-layer-actions", image.0, layer_id.0)),
                egui::Sense::click_and_drag(),
            );
            let default_name = result_layers(img, &session.result)
                .and_then(|stack| stack.layer_meta(layer_id))
                .map(|meta| meta.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("Layer {}", index + 1));
            response.context_menu(|ui| {
                let mut name = ui
                    .data(|data| data.get_temp::<String>(rename_id))
                    .unwrap_or(default_name);
                if ui.text_edit_singleline(&mut name).changed() {
                    ui.data_mut(|data| data.insert_temp(rename_id, name.clone()));
                }
                if ui.button("Save name").clicked() {
                    self.rename_paint_layer(image, layer_id, name);
                    ui.data_mut(|data| data.remove_temp::<String>(rename_id));
                    ui.close_menu();
                }
                if ui.button("Duplicate layer").clicked() {
                    self.duplicate_paint_layer(image, layer_id);
                    ui.close_menu();
                }
            });
            if response.drag_started() && ui.input(|input| input.modifiers.alt) {
                self.duplicate_paint_layer(image, layer_id);
            } else if response.dragged() && !ui.input(|input| input.modifiers.alt) {
                if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                    if presentation.main.contains(pointer) {
                        let nearest = layout
                            .layers
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                a.distance_sq(pointer).total_cmp(&b.distance_sq(pointer))
                            })
                            .map(|(i, _)| i);
                        if let Some(target) = nearest.filter(|target| *target != index) {
                            self.reorder_paint_layer(
                                image,
                                layer_id,
                                layer_ids.get(target).copied(),
                            );
                        }
                    }
                }
            }
            if response.drag_stopped() && !ui.input(|input| input.modifiers.alt) {
                if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                    if !presentation.main.contains(pointer) {
                        self.copy_paint_layer_to_canvas(image, layer_id, pointer);
                    }
                }
            }
        }
        let mut hits: Vec<egui::Rect> = layout
            .layers
            .iter()
            .map(|center| {
                egui::Rect::from_center_size(*center, egui::Vec2::splat(layout.radius * 2.0))
            })
            .chain(layout.colors.iter().map(|center| {
                egui::Rect::from_center_size(*center, egui::Vec2::splat(layout.color_radius * 2.0))
            }))
            .collect();
        hits.extend([layout.track, layout.add]);
        if edit.add {
            self.on_image_paint_add_clicked(image);
        } else if let Some(index) = edit.layer {
            self.image_paint = Some(ImagePaintSession {
                image,
                result: session.result.clone(),
                layer_index: index,
                focus: ImageStripFocus::Layer(index),
            });
        }
        if let Some(rgb) = edit.color.and_then(|index| recents.get(index)) {
            self.adopt_board_color_rgb(*rgb);
        }
        if let (Some(value), Some(layer)) = (edit.opacity, active) {
            self.layer_opacity_drag = Some((image, layer, value));
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            if let Some((drag_image, layer, value)) = self.layer_opacity_drag.take() {
                self.set_paint_layer_opacity(drag_image, layer, value);
            }
        }
        hits
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
        let display_img = image_for_result(img, &result);
        if display_img.paint_layers.is_empty() {
            return;
        }
        let ppp = ui.ctx().pixels_per_point();
        let longest_px = (host.rect.w.max(host.rect.h) * xf.z * ppp)
            .ceil()
            .clamp(1.0, 4096.0);
        let zoom_bucket = (longest_px as u32).max(1).next_power_of_two();
        let key = paint_layer_cache_key(host, &display_img, &result, self.scene_gen, zoom_bucket);
        let stale = self
            .paint_layer_texture_cache
            .get(&host.id)
            .is_none_or(|entry| entry.key != key);
        if stale {
            let aspect = host.rect.w.max(1e-6) / host.rect.h.max(1e-6);
            let (w, h) = if aspect >= 1.0 {
                (zoom_bucket, (zoom_bucket as f32 / aspect).ceil() as u32)
            } else {
                ((zoom_bucket as f32 * aspect).ceil() as u32, zoom_bucket)
            };
            let mut stamps = std::mem::take(&mut self.paint_layer_stamps);
            let raster = slate_artifact::rasterize_paint_layers(
                host,
                &display_img,
                w,
                h,
                Some(self.doc()),
                &mut stamps,
            );
            self.paint_layer_stamps = stamps;
            if let Some(rgba) = raster {
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                let texture = ui.ctx().load_texture(
                    format!("paint-layer-{}", host.id.0),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.paint_layer_texture_cache
                    .insert(host.id, PaintLayerTextureCache { key, texture });
            }
        }
        let Some(texture) = self
            .paint_layer_texture_cache
            .get(&host.id)
            .map(|entry| entry.texture.clone())
        else {
            return;
        };
        let tint = Color32::WHITE.gamma_multiply(host_alpha.clamp(0.0, 1.0));
        if let Some(clip) = &host.clip {
            super::super::board::paint_clipped_texture(
                painter,
                xf,
                &texture,
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
                &texture,
                outline,
                &outline_world,
                host.rect,
                slate_doc::scene::Crop::full(),
                tint,
                host.rotation_deg,
            );
        }
    }

    /// One preview texture per layer of `host`: that layer's ink alone,
    /// center-cropped square. Rebuilt only when the scene generation moves.
    fn paint_layer_thumbs(
        &mut self,
        ctx: &egui::Context,
        host: &Node,
        img: &ImageNode,
        result: &ImageResultId,
    ) -> Vec<Option<egui::TextureId>> {
        const THUMB_PX: u32 = 48;
        let gen = self.scene_gen;
        let aspect = host.rect.w.max(1e-6) / host.rect.h.max(1e-6);
        let (w, h) = if aspect >= 1.0 {
            ((THUMB_PX as f32 * aspect).ceil() as u32, THUMB_PX)
        } else {
            (THUMB_PX, (THUMB_PX as f32 / aspect).ceil() as u32)
        };
        img.paint_layers
            .iter()
            .map(|layer| {
                let key = (host.id, result.clone(), layer.id);
                if self
                    .paint_layer_thumbs
                    .get(&key)
                    .is_none_or(|t| t.gen != gen)
                {
                    let mut alone = img.clone();
                    alone.paint_layers = vec![PaintLayer {
                        opacity: 1.0,
                        visible: true,
                        ..layer.clone()
                    }];
                    let svg =
                        slate_artifact::paint_layers_svg_with_doc(host, &alone, w, h, self.doc());
                    let rgba = slate_artifact::rasterize_paint_layers_svg(&svg, w, h)?;
                    let (x0, y0) = ((w - THUMB_PX) / 2, (h - THUMB_PX) / 2);
                    let side = THUMB_PX as usize;
                    let mut square = Vec::with_capacity(side * side * 4);
                    for row in 0..side {
                        let start = ((y0 as usize + row) * w as usize + x0 as usize) * 4;
                        square.extend_from_slice(&rgba[start..start + side * 4]);
                    }
                    let texture = ctx.load_texture(
                        format!(
                            "paint-layer-thumb-{}-{:?}-{}",
                            host.id.0, result, layer.id.0
                        ),
                        egui::ColorImage::from_rgba_unmultiplied([side, side], &square),
                        egui::TextureOptions::LINEAR,
                    );
                    self.paint_layer_thumbs
                        .insert(key, PaintLayerThumb { gen, texture });
                }
                self.paint_layer_thumbs.get(&key).map(|t| t.texture.id())
            })
            .collect()
    }
}
