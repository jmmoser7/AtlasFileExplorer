//! Live cutout preview for the canonical image-mask drag owner.
use super::*;

impl SlateApp {
    pub(super) fn image_mask_drag_ghost(&mut self) -> Option<Node> {
        let drag = self.image_segments.drag.as_ref()?;
        if !drag.moved
            || !drag.can_extract
            || drag.tab != self.tab().id
            || self.segment_source_key(drag.host).as_deref() != Some(drag.source_key.as_str())
        {
            return None;
        }
        let mut ghost = drag.preview.clone()?;
        ghost.rect.x += drag.delta.x;
        ghost.rect.y += drag.delta.y;
        ghost.locked = false;
        ghost.hidden = false;
        ghost.group = None;
        Some(ghost)
    }

    pub(super) fn paint_sticker_ghost(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        if let Some(ghost) = self.image_mask_drag_ghost() {
            self.paint_board_node(ui, painter, xf, &ghost, false);
            self.paint_image_selection_layer_ghost(painter, xf, &ghost);
        }
    }
}

#[cfg(test)]
impl SlateApp {
    /// Model-free input and review fixture using the current tagged runtime.
    pub(crate) fn install_stub_highlight(&mut self, host: NodeId) -> Pos2 {
        let node = self.doc().scene.node(host).unwrap().clone();
        let NodeKind::Image(img) = &node.kind else {
            panic!("highlight needs an image");
        };
        let basis = slate_doc::image_paint::image_content_rect(&node, img);
        let point = [0.3, 0.55];
        let world = slate_doc::geom::orbit_point(
            node.rect.center(),
            (basis.x + point[0] * basis.w, basis.y + point[1] * basis.h),
            node.rotation_deg,
        );
        let screen = self.board_xf().w2s(Pos2::new(world.0, world.1));
        let source_key = self.segment_source_key(host).unwrap();
        let mask = vec![
            vec![
                [0.15, 0.15],
                [0.8, 0.15],
                [0.8, 0.4],
                [0.5, 0.4],
                [0.5, 0.85],
                [0.15, 0.85],
            ],
            vec![[0.2, 0.3], [0.3, 0.3], [0.3, 0.4], [0.2, 0.4]],
        ];
        self.image_segments.clear();
        self.image_segments.pinned = None;
        self.image_segments.hover = Some(Hover {
            tab: self.tab().id,
            host,
            gen: self.scene_gen,
            source_key: source_key.clone(),
            point_norm: point,
            prompts: vec![PointPrompt {
                point,
                positive: true,
            }],
            screen,
            since: Instant::now() - LINGER,
        });
        self.image_segments.result = Some(SegmentResult {
            host,
            gen: self.scene_gen,
            source_key,
            local: region_node(slate_doc::image_paint::region_path(mask.clone()).unwrap()),
            candidates: std::sync::Arc::new(vec![atlas_segment::Candidate {
                quality: 1.0,
                contours: mask.clone(),
            }]),
            mask: std::sync::Arc::new(mask),
            selected: 0,
        });
        screen
    }
}
