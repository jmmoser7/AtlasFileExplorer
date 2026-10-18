use super::super::*;

impl SlateApp {
    /// Current displayed layer choices for the image Paint layer menu. The
    /// identities remain stable across reorder operations; labels come from
    /// result metadata where available.
    pub(crate) fn image_paint_layer_choices(&self, image: NodeId) -> Vec<(u64, String)> {
        let Some(result) = self.image_result_id(image) else {
            return Vec::new();
        };
        let Some(node) = self.doc().scene.node(image) else {
            return Vec::new();
        };
        let NodeKind::Image(img) = &node.kind else {
            return Vec::new();
        };
        let raw = Self::displayed_layers(img, &result);
        let layers: Vec<PaintLayer> = if matches!(&result, ImageResultId::Output(_))
            && result_layers(img, &result).is_none()
        {
            raw.iter().rev().cloned().collect()
        } else {
            raw.to_vec()
        };
        layers
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                let name = result_layers(img, &result)
                    .and_then(|stack| stack.layer_meta(layer.id))
                    .map(|meta| meta.name.trim())
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Layer {}", index + 1));
                (layer.id.0, name)
            })
            .collect()
    }

    /// Select a displayed layer by its current menu index without committing
    /// a document edit. This is the shared focus path for the row and Brush.
    pub(crate) fn select_image_paint_layer(&mut self, image: NodeId, index: usize) -> bool {
        let Some(result) = self.image_result_id(image) else {
            return false;
        };
        let Some(node) = self.doc().scene.node(image) else {
            return false;
        };
        let NodeKind::Image(img) = &node.kind else {
            return false;
        };
        let layers = Self::displayed_layers(img, &result);
        let count = layers.len();
        if index >= count {
            return false;
        }
        self.image_paint = Some(ImagePaintSession {
            image,
            result,
            layer_index: index,
            focus: ImageStripFocus::Layer(index),
            layer_drag: None,
        });
        true
    }

    /// Paint layer controls inside the shared image blister and return the
    /// screen-space hit regions to the host palette.
    pub(crate) fn image_layer_palette_body(
        &mut self,
        ui: &mut egui::Ui,
        presentation: &atlas_shell::selection_tools::ImagePresentationLayout,
        image: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
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
        let raw_layers = Self::displayed_layers(img, &session.result);
        let layers: Vec<PaintLayer> = if matches!(&session.result, ImageResultId::Output(_))
            && result_layers(img, &session.result).is_none()
        {
            raw_layers.iter().rev().cloned().collect()
        } else {
            raw_layers.to_vec()
        };
        let layout = atlas_shell::selection_tools::image_layer_palette_layout(
            presentation,
            layers.len(),
            0,
            z,
            match session.focus {
                ImageStripFocus::Layer(index) if index < layers.len() => Some(index),
                _ => None,
            },
        );
        let active = match session.focus {
            ImageStripFocus::Layer(i) if i < layers.len() => Some(i),
            _ => None,
        };
        let opacity = self
            .layer_opacity_drag
            .as_ref()
            .filter(|drag| {
                drag.image == image
                    && drag.result == session.result
                    && active
                        .and_then(|index| layers.get(index))
                        .is_some_and(|layer| layer.id == drag.layer)
            })
            .map_or_else(
                || active.map_or(1.0, |i| layers[i].opacity),
                |drag| drag.value,
            );
        let thumbs = self.paint_layer_thumbs(ui.ctx(), &host, img, &session.result, &layers);
        let layer_ids: Vec<u64> = layers.iter().map(|layer| layer.id.0).collect();
        let layer_names: Vec<String> = layers
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                result_layers(img, &session.result)
                    .and_then(|stack| stack.layer_meta(layer.id))
                    .map(|meta| meta.name.trim())
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Layer {}", index + 1))
            })
            .collect();
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let target = pointer
            .filter(|pointer| layout.capsule.contains(*pointer))
            .and_then(|pointer| {
                layout
                    .layers
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        a.distance_sq(pointer).total_cmp(&b.distance_sq(pointer))
                    })
                    .and_then(|(visible, _)| layout.layer_indices.get(visible).copied())
                    .and_then(|index| layers.get(index).map(|layer| layer.id.0))
            });
        let escape_pressed = ui.input(|input| input.key_pressed(egui::Key::Escape));
        let drag = if escape_pressed {
            None
        } else {
            session.layer_drag.and_then(|(source, alt)| {
                pointer.map(
                    |pointer| atlas_shell::selection_tools::ImageLayerPaletteDrag {
                        source: source.0,
                        target: if alt { None } else { target },
                        pointer,
                        clip: self.canvas_rect,
                    },
                )
            })
        };
        let palette = ui
            .push_id(
                ("image-result-layers", self.tab().id, image, &session.result),
                |ui| {
                    atlas_shell::selection_tools::image_layer_palette(
                        ui,
                        &layout,
                        &thumbs,
                        &layer_ids,
                        drag,
                        active,
                        opacity,
                        &layer_names,
                        z,
                        theme,
                    )
                },
            )
            .inner;
        let atlas_shell::selection_tools::ImageLayerPaletteEdit {
            edit,
            layer_responses,
        } = palette;
        for (visible_index, layer_index) in layout.layer_indices.iter().copied().enumerate() {
            let Some(layer_id) = layers.get(layer_index).map(|layer| layer.id) else {
                continue;
            };
            let Some(response) = layer_responses.get(visible_index).cloned() else {
                continue;
            };
            let default_name = result_layers(img, &session.result)
                .and_then(|stack| stack.layer_meta(layer_id))
                .map(|meta| meta.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("Layer {}", layer_index + 1));
            self.image_layer_context_menu(
                &response,
                image,
                &session.result,
                &layers[layer_index],
                &default_name,
            );
            let cancel_layer_drag = ui.input(|input| input.key_pressed(egui::Key::Escape));
            if cancel_layer_drag {
                self.cancel_image_layer_drag();
            }
            if response.drag_started() {
                let alt = ui.input(|input| input.modifiers.alt);
                if let Some(session) = self.image_paint.as_mut() {
                    session.layer_drag = Some((layer_id, alt));
                }
            }
            if response.dragged() {
                if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                    if layout.capsule.contains(pointer) {
                        if let Some((target, center)) = layout
                            .layers
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                a.distance_sq(pointer).total_cmp(&b.distance_sq(pointer))
                            })
                            .and_then(|(visible, center)| {
                                layout
                                    .layer_indices
                                    .get(visible)
                                    .map(|index| (*index, *center))
                            })
                        {
                            if target != layer_index {
                                ui.painter().circle_stroke(
                                    center,
                                    layout.radius * 1.2,
                                    egui::Stroke::new(1.0_f32, theme.accent),
                                );
                            }
                        }
                    }
                }
            }
            if response.drag_stopped() && !cancel_layer_drag {
                let drag = self
                    .image_paint
                    .as_mut()
                    .filter(|session| session.image == image)
                    .and_then(|session| session.layer_drag.take());
                let Some((_, alt)) = drag.filter(|(id, _)| *id == layer_id) else {
                    continue;
                };
                if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                    if layout.capsule.contains(pointer) {
                        let target = layout
                            .layers
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                a.distance_sq(pointer).total_cmp(&b.distance_sq(pointer))
                            })
                            .and_then(|(visible, _)| layout.layer_indices.get(visible).copied())
                            .and_then(|index| layers.get(index).map(|layer| layer.id));
                        if alt {
                            self.dispatch(ui.ctx(), atlas_commands::CommandId("board.image.layer.duplicate"), Some(serde_json::json!({
                                "image": image.0, "layer": layer_id.0, "before": target.map(|id| id.0),
                            }).to_string()));
                        } else if target != Some(layer_id) {
                            self.dispatch(ui.ctx(), atlas_commands::CommandId("board.image.layer.reorder"), Some(serde_json::json!({
                                "image": image.0, "layer": layer_id.0, "before": target.map(|id| id.0),
                            }).to_string()));
                        }
                    } else {
                        let canvas = self.canvas_rect;
                        let xf = self.board_xf();
                        let world = xf.s2w(pointer);
                        let over_host =
                            slate_doc::geom::point_in_node_outline(&host, world.x, world.y, 0.0);
                        if !over_host
                            && !layout.capsule.contains(pointer)
                            && canvas.contains(pointer)
                        {
                            let center =
                                self.resolve_point_snap(world, &[image], None, false, false);
                            let at = Pos2::new(
                                center.x - host.rect.w * 0.5,
                                center.y - host.rect.h * 0.5,
                            );
                            self.copy_paint_layer_to_canvas(image, layer_id, at, ui.ctx().clone());
                        }
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
        if layout.page_count > 1 {
            hits.extend([layout.page_prev, layout.page_next]);
        }
        if edit.add {
            self.on_image_paint_add_clicked(image);
        } else if let Some(index) = edit
            .layer
            .and_then(|visible| layout.layer_indices.get(visible).copied())
        {
            self.select_image_paint_layer(image, index);
        }
        if edit.previous_page || edit.next_page {
            let page = if edit.previous_page {
                layout.page.saturating_sub(1)
            } else {
                (layout.page + 1).min(layout.page_count.saturating_sub(1))
            };
            if let Some(index) = page
                .checked_mul(layout.page_capacity.max(1))
                .filter(|index| *index < layers.len())
            {
                self.select_image_paint_layer(image, index);
            }
        }
        let cancelled_id = super::opacity::opacity_cancelled_id(self.tab().id, image);
        if escape_pressed {
            self.cancel_image_layer_opacity_for_escape(ui.ctx());
            ui.data_mut(|data| data.insert_temp(cancelled_id, true));
        }
        let cancelled = ui
            .data(|data| data.get_temp::<bool>(cancelled_id))
            .unwrap_or(false);
        if !cancelled {
            if let (Some(value), Some(layer)) =
                (edit.opacity, active.and_then(|index| layers.get(index)))
            {
                self.layer_opacity_drag = Some(super::opacity::LayerOpacityDrag {
                    image,
                    result: session.result,
                    layer: layer.id,
                    value,
                });
            }
            if !ui.input(|input| input.pointer.primary_down()) {
                self.finish_image_layer_opacity(ui.ctx());
            }
        } else if !ui.input(|input| input.pointer.primary_down()) {
            ui.data_mut(|data| data.remove_temp::<bool>(cancelled_id));
        }
        hits
    }

    /// One preview texture per layer of `host`: that layer's ink alone,
    /// center-cropped square. Rebuilt only when the scene generation moves.
    fn paint_layer_thumbs(
        &mut self,
        ctx: &egui::Context,
        host: &Node,
        img: &ImageNode,
        result: &ImageResultId,
        layers: &[PaintLayer],
    ) -> Vec<Option<egui::TextureId>> {
        const THUMB_PX: u32 = 48;
        const THUMB_MAX: f32 = 2048.0;
        let gen = self.scene_gen;
        let aspect = host.rect.w.max(1e-6) / host.rect.h.max(1e-6);
        let (w, h) = if aspect >= 1.0 {
            (
                (THUMB_PX as f32 * aspect).ceil().min(THUMB_MAX) as u32,
                THUMB_PX,
            )
        } else {
            (
                THUMB_PX,
                (THUMB_PX as f32 / aspect).ceil().min(THUMB_MAX) as u32,
            )
        };
        let workbook = self.tab().path.clone();
        let mut doc = None;
        layers
            .iter()
            .map(|layer| {
                let key = (host.id, result.clone(), layer.id);
                if self
                    .paint_layer_thumbs
                    .get(&key)
                    .is_none_or(|t| t.gen != gen)
                    && self.image_rasters.needs_thumb(&key, gen)
                {
                    let doc = doc
                        .get_or_insert_with(|| {
                            let mut sources = img.clone();
                            sources.paint_layers = layers.to_vec();
                            sources.result_layers.clear();
                            super::super::raster::snapshot_for_image(self.doc(), &sources)
                        })
                        .clone();
                    let mut alone = img.clone();
                    alone.paint_layers = vec![PaintLayer {
                        opacity: 1.0,
                        visible: true,
                        ..layer.clone()
                    }];
                    self.image_rasters.request_thumb(
                        key.clone(),
                        gen,
                        super::super::raster::RasterInput::new(
                            host.clone(),
                            alone,
                            doc,
                            workbook.clone(),
                            (w, h),
                        ),
                        THUMB_PX,
                        ctx.clone(),
                    );
                }
                self.paint_layer_thumbs.get(&key).map(|t| t.texture.id())
            })
            .collect()
    }
}
