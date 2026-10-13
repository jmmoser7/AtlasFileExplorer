//! The shared asynchronous asset pipeline for hosted-layer and selected-mask copies.
use super::*;

impl SlateApp {
    /// Bake one hosted vector layer to a standalone PNG-backed image.
    pub(crate) fn copy_paint_layer_to_canvas(
        &mut self,
        host_id: NodeId,
        layer_id: slate_doc::image_paint::PaintLayerId,
        at: Pos2,
        ctx: egui::Context,
    ) -> Option<()> {
        if self.refuse_read_only_edit() {
            return None;
        }
        let host = self.doc().scene.node(host_id)?.clone();
        let NodeKind::Image(img) = &host.kind else {
            return None;
        };
        let result = self.image_result_id(host_id)?;
        let layer = super::SlateApp::selected_layers(img, &result)
            .iter()
            .find(|layer| layer.id == layer_id)?;
        if !layer.visible || layer.nodes.is_empty() {
            return None;
        }
        let mut resolved = image_for_result(img, &result);
        let layer = layer.clone();
        resolved.paint_layers = vec![layer.clone()];
        self.queue_image_layer_copy(host, result, layer, resolved, at, ctx, None)
    }

    /// Bake a selected mask into tight bounds. `at` is the cutout origin.
    /// The source stack is untouched; rasterization and encoding stay on the worker.
    pub(crate) fn copy_image_selection_to_canvas(
        &mut self,
        host_id: NodeId,
        cutout: Node,
        at: Pos2,
        ctx: egui::Context,
    ) -> Option<()> {
        if self.refuse_read_only_edit() {
            return None;
        }
        if cutout.id != host_id {
            return None;
        }
        let NodeKind::Image(picture) = &cutout.kind else {
            return None;
        };
        let result = self.image_result_id(host_id)?;
        // Freeze the selected source stack. The shared composite worker crops
        // its base and full-source layer coordinates into this tight window.
        let layer = PaintLayer::new(PaintLayerId::NONE);
        let resolved = picture.clone();
        self.queue_image_layer_copy(
            cutout.clone(),
            result,
            layer,
            resolved,
            at,
            ctx,
            Some(cutout),
        )?;
        self.push_history(
            atlas_commands::CommandId("board.image.selection.copy"),
            None,
        );
        Some(())
    }

    /// One normalized child payload for hosted cutouts and independent copies.
    pub(super) fn selection_cutout_child(
        &mut self,
        source: &ImageNode,
        item: slate_doc::ItemId,
        mask: &[Vec<[f32; 2]>],
    ) -> Option<Node> {
        let image = selection_cutout_image(source, item);
        let mut child = self
            .doc_mut()
            .scene
            .build_node(WorldRect::new(0.0, 0.0, 1.0, 1.0), NodeKind::Image(image));
        child.clip = clip_path_for_layer_node(&child, mask);
        child.clip.as_ref()?;
        Some(child)
    }

    /// The shared tight image/clip geometry for drag preview and async copies.
    pub(crate) fn image_selection_cutout_node(
        &self,
        host_id: NodeId,
        item: slate_doc::ItemId,
        mask: &[Vec<[f32; 2]>],
    ) -> Option<Node> {
        let host = self.doc().scene.node(host_id)?;
        let NodeKind::Image(source) = &host.kind else {
            return None;
        };
        let visible = slate_doc::image_paint::visible_window_norm(host, source);
        let clipped: Vec<Vec<[f32; 2]>> =
            vector_ink::boolean_intersection(&mask.to_vec(), &visible)
                .into_iter()
                .flatten()
                .collect();
        let (min_x, min_y, max_x, max_y) = clipped.iter().flatten().fold(
            (
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ),
            |(x0, y0, x1, y1), p| (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])),
        );
        if max_x - min_x <= 1e-6 || max_y - min_y <= 1e-6 {
            return None;
        }
        let crop = slate_doc::scene::Crop {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        };
        let mut local = host.clone();
        local.rect = WorldRect::new(crop.x, crop.y, crop.w, crop.h);
        local.rotation_deg = 0.0;
        local.clip = clip_path_for_layer_node(&local, &clipped);
        local.clip.as_ref()?;
        let selected = self.image_for_selected_result(host_id)?;
        let mut picture = selection_cutout_image(&selected, item);
        picture.paint_layers = selected.paint_layers;
        picture.crop = crop;
        local.kind = NodeKind::Image(picture);
        local.locked = false;
        local.hidden = false;
        local.group = None;
        local.bumper = None;
        Some(layer_node_to_world(host, source, &local))
    }

    #[allow(clippy::too_many_arguments)]
    fn queue_image_layer_copy(
        &mut self,
        host: Node,
        result: ImageResultId,
        layer: PaintLayer,
        resolved: ImageNode,
        at: Pos2,
        ctx: egui::Context,
        cutout: Option<Node>,
    ) -> Option<()> {
        let aspect = host.rect.w.max(1e-3) / host.rect.h.max(1e-3);
        let (w, h) = if aspect >= 1.0 {
            (2048u32, (2048.0 / aspect).round().clamp(1.0, 2048.0) as u32)
        } else {
            ((2048.0 * aspect).round().clamp(1.0, 2048.0) as u32, 2048u32)
        };
        let workbook = self.tab().path.clone();
        let doc = super::super::raster::snapshot_for_image(self.doc(), &resolved);
        let tab_id = self.tab().id;
        let request_id = atlas_ai::agent::request_id();
        let filename = format!("layer-{request_id}.png");
        let dir = atlas_core::workbook_assets::paste_dir(
            workbook.as_deref(),
            &atlas_core::index::data_dir(),
        );
        let path = dir.join(&filename);
        let locator = slate_doc::scene::source_locator(workbook.as_deref(), &path);
        let stored = if locator.starts_with("assets/") {
            std::path::PathBuf::from(&locator)
        } else {
            path.clone()
        };
        let item = self.doc_mut().add_item(
            stored,
            filename.clone(),
            0,
            0,
            atlas_core::thumbs::cache_key(&path.to_string_lossy(), 0, 0),
        );
        let mut placeholder = self.doc_mut().scene.build_node(
            slate_doc::scene::WorldRect::new(at.x, at.y, host.rect.w, host.rect.h),
            NodeKind::Image(ImageNode::new(item)),
        );
        placeholder.rotation_deg = host.rotation_deg;
        let composite = cutout.is_some();
        if let Some(mut node) = cutout {
            node.id = placeholder.id;
            node.rect.x = at.x;
            node.rect.y = at.y;
            node.kind = placeholder.kind;
            // The standalone PNG owns selection alpha, including soft edges.
            node.clip = None;
            placeholder = node;
        }
        let placeholder_id = placeholder.id;
        let completion = LayerBakeCompletion {
            tab_id,
            host: host.id,
            result,
            layer: layer.id,
            placeholder: placeholder_id,
            item,
            path: path.clone(),
            at,
            saved: Err("Layer bake has not finished".into()),
            readiness: LayerBakeReadiness::new(),
        };
        let request = LayerBakeRequest {
            completion,
            host_node: host,
            image: resolved,
            doc,
            composite,
            workbook,
            w,
            h,
            dir,
            ctx,
        };
        let readiness = request.completion.readiness.clone();
        if self.layer_bakes.request_tx.try_send(request).is_err() {
            self.toast("Couldn't start the paint layer copy.");
            self.doc_mut()
                .items
                .retain(|candidate| candidate.id != item);
            return None;
        }
        self.layer_bakes.register_readiness(tab_id, item, readiness);
        if self.add_nodes(vec![placeholder]).is_empty() {
            return None;
        }
        Some(())
    }
}

fn selection_cutout_image(source: &ImageNode, item: slate_doc::ItemId) -> ImageNode {
    let mut image = ImageNode::new(item);
    image.adjust = source.adjust;
    image.flip_x = source.flip_x;
    image.flip_y = source.flip_y;
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use slate_doc::scene::{Crop, ImageAdjust, Rgba, ShapeKind, ShapeNode, Stroke};

    #[test]
    fn tight_mask_worker_retains_selected_visible_layers_holes_mirror_filters_and_opacity() {
        let mut h = Harness::new("tight_mask_worker");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let path = h.base.join("gradient.png");
        image::RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([x as u8 * 4, y as u8 * 4, 80, 255])
        })
        .save(&path)
        .unwrap();
        let item = h.app.add_paths(&[path])[0];
        let mut picture = ImageNode::new(item);
        picture.crop = Crop {
            x: 0.2,
            y: 0.1,
            w: 0.6,
            h: 0.8,
        };
        picture.flip_x = true;
        picture.adjust = ImageAdjust {
            brightness: 0.5,
            ..Default::default()
        };
        let mark = h.app.doc_mut().scene.build_node(
            WorldRect::new(0.25, 0.25, 0.5, 0.12),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                sides: 4,
                phase_deg: 0.0,
                fill: Some(Rgba::opaque(200, 100, 50)),
                stroke: Stroke::none(),
                corner: Default::default(),
                flip: false,
                path: None,
                text: None,
            }),
        );
        let visible = PaintLayer {
            opacity: 0.5,
            nodes: vec![mark.clone()],
            ..PaintLayer::new(PaintLayerId(10))
        };
        let mut hidden = PaintLayer::new(PaintLayerId(11));
        hidden.visible = false;
        hidden.nodes.push(mark);
        hidden.nodes[0].rect = WorldRect::new(0.0, 0.0, 1.0, 1.0);
        slate_doc::image_paint::ensure_result_layers_mut(&mut picture, ImageResultId::Item(item))
            .layers = vec![hidden, visible];
        let unrelated = picture.result_layers[0].layers[0].clone();
        slate_doc::image_paint::ensure_result_layers_mut(
            &mut picture,
            ImageResultId::Output("inactive-result".into()),
        )
        .layers = vec![unrelated];
        let mut host = h.app.doc_mut().scene.build_node(
            WorldRect::new(-200.0, -160.0, 400.0, 320.0),
            NodeKind::Image(picture),
        );
        host.rotation_deg = 35.0;
        host.opacity = 0.7;
        let id = h.app.add_nodes(vec![host.clone()])[0];
        let mask = vec![
            vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]],
            vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]],
        ];
        let tight = h.app.image_selection_cutout_node(id, item, &mask).unwrap();
        assert!((tight.rect.w - (400.0 / 0.6 * 0.5)).abs() < 0.01);
        assert!((tight.rect.h - 200.0).abs() < 0.01);
        assert_eq!(tight.rotation_deg, 35.0);
        assert_eq!(tight.clip.as_ref().unwrap().extra.len(), 1);
        let NodeKind::Image(tight_image) = &tight.kind else {
            panic!("image")
        };
        assert_eq!(tight_image.paint_layers.len(), 2);
        assert_eq!(tight_image.paint_layers[0].id, PaintLayerId(10));
        assert!(!tight_image.paint_layers[1].visible);
        assert!(tight_image.result_layers.is_empty());
        // Capture the existing worker request, then run its actual bake on a
        // worker at a small resolution. No inference, UI loop, or network.
        let (tx, rx) = crossbeam_channel::bounded(1);
        h.app.layer_bakes.request_tx = tx;
        let at = Pos2::new(500.0, 100.0);
        h.app
            .copy_image_selection_to_canvas(id, tight.clone(), at, h.ctx.clone())
            .unwrap();
        let mut request = rx.try_recv().unwrap();
        assert!(request.composite);
        assert_eq!(request.image.paint_layers, tight_image.paint_layers);
        let placed = h.app.doc().scene.nodes.last().unwrap();
        assert_eq!(placed.rect.w, tight.rect.w);
        assert_eq!(placed.rect.h, tight.rect.h);
        assert_eq!(placed.rotation_deg, 35.0);
        assert_eq!(placed.opacity, 0.7);
        assert!(
            placed.clip.is_none(),
            "selection alpha is baked once into PNG"
        );
        assert_eq!(h.app.doc().scene.node(id), Some(&host));
        // A later result/layer edit cannot change the already prepared job.
        let NodeKind::Image(current) = &mut h.app.doc_mut().scene.node_mut(id).unwrap().kind else {
            panic!("image")
        };
        current.result_layers.clear();
        current.paint_layers.clear();
        request.w = 64;
        request.h = 64;
        let pixels = std::thread::spawn(move || {
            super::super::bake_layer(&request).unwrap();
            image::open(request.completion.path).unwrap().into_rgba8()
        })
        .join()
        .unwrap();
        assert_eq!(
            pixels.get_pixel(32, 32)[3],
            0,
            "the inner contour is transparent"
        );
        let left = pixels.get_pixel(8, 55).0;
        let right = pixels.get_pixel(55, 55).0;
        assert!(
            (left[0] as i32 - 86).abs() <= 3,
            "crop and mirror: {left:?}"
        );
        assert!((right[0] as i32 - 40).abs() <= 3, "single crop: {right:?}");
        assert_eq!(left[2], 40, "brightness is applied once");
        assert_eq!(left[3], 255, "source opacity belongs to the placed node");
        let layered = pixels.get_pixel(8, 8).0;
        assert!(
            (layered[0] as i32 - 143).abs() <= 3,
            "visible layer alpha: {layered:?}"
        );
        assert!(
            (layered[2] as i32 - 45).abs() <= 2,
            "base filter does not filter authored ink: {layered:?}"
        );
    }
}
