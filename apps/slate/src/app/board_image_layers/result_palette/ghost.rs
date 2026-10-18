//! Cropped cutout previews reuse their source layer overlay.
use super::*;

impl SlateApp {
    /// A cutout reuses its source overlay, cropped by the same UV and clip
    /// owner as its base. Moving the ghost never schedules another raster.
    pub(crate) fn paint_image_selection_layer_ghost(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        ghost: &Node,
    ) {
        let Some(source) = self.doc().scene.node(ghost.id) else {
            return;
        };
        let (NodeKind::Image(source), NodeKind::Image(picture)) = (&source.kind, &ghost.kind)
        else {
            return;
        };
        let Some(result) = self.image_result_id(ghost.id) else {
            return;
        };
        if Self::displayed_layers(source, &result).is_empty() {
            return;
        }
        let Some(texture) = self
            .paint_layer_texture_cache
            .get(&ghost.id)
            .map(|entry| entry.texture.clone())
        else {
            return;
        };
        let Some(clip) = &ghost.clip else {
            return;
        };
        let window = source.crop.clamped();
        let crop = slate_doc::scene::Crop {
            x: (picture.crop.x - window.x) / window.w,
            y: (picture.crop.y - window.y) / window.h,
            w: picture.crop.w / window.w,
            h: picture.crop.h / window.h,
        };
        crate::app::board::paint_clipped_texture(
            &mut self.path_mesh_cache,
            painter,
            xf,
            &texture,
            ghost,
            clip,
            crop,
            slate_doc::scene::Mirror::default(),
            Color32::WHITE.gamma_multiply(ghost.opacity),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use slate_doc::scene::Crop;

    #[test]
    fn cropped_rotated_mirrored_ghost_reuses_overlay_uvs_without_raster_jobs() {
        let mut h = Harness::new("cutout_overlay_uv");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let path = h.base.join("source.png");
        let item = h.app.doc_mut().add_item(path, "source.png", 0, 0, "");
        let mut image = ImageNode::new(item);
        image.crop = Crop {
            x: 0.2,
            y: 0.1,
            w: 0.6,
            h: 0.8,
        };
        image.flip_x = true;
        image.flip_y = true;
        image.paint_layers.push(PaintLayer::new(PaintLayerId(1)));
        let mut source = h.app.doc_mut().scene.build_node(
            WorldRect::new(-200.0, -160.0, 400.0, 320.0),
            NodeKind::Image(image),
        );
        source.rotation_deg = 35.0;
        source.opacity = 0.7;
        let id = h.app.add_nodes(vec![source])[0];
        let mask = vec![vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]]];
        let mut ghost = h.app.image_selection_cutout_node(id, item, &mask).unwrap();
        ghost.rect.x += 200.0;
        ghost.rect.y -= 80.0;
        let texture = h.ctx.load_texture(
            "source-overlay",
            egui::ColorImage::new([2, 2], Color32::RED),
            egui::TextureOptions::LINEAR,
        );
        let texture_id = texture.id();
        h.app
            .paint_layer_texture_cache
            .insert(id, PaintLayerTextureCache { key: 0, texture });
        let xf = h.app.board_xf();
        let output = h.ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                h.app
                    .paint_image_selection_layer_ghost(ui.painter(), &xf, &ghost);
            });
        });
        let vertices: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id => {
                    Some(mesh.vertices.iter())
                }
                _ => None,
            })
            .flatten()
            .collect();
        assert!(!vertices.is_empty());
        let min_u = vertices
            .iter()
            .map(|v| v.uv.x)
            .fold(f32::INFINITY, f32::min);
        let max_u = vertices
            .iter()
            .map(|v| v.uv.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_v = vertices
            .iter()
            .map(|v| v.uv.y)
            .fold(f32::INFINITY, f32::min);
        let max_v = vertices
            .iter()
            .map(|v| v.uv.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_u - 1.0 / 12.0).abs() < 1e-5);
        assert!((max_u - 11.0 / 12.0).abs() < 1e-5);
        assert!((min_v - 0.1875).abs() < 1e-5);
        assert!((max_v - 0.8125).abs() < 1e-5);
        assert!(vertices
            .iter()
            .all(|v| v.color == Color32::WHITE.gamma_multiply(0.7)));
        assert!(
            h.app.image_rasters.needs_host(id, 0),
            "moving a preview never queues a raster"
        );
    }
}
