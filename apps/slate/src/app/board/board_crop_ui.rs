//! Crop pointer handling and the crop overlay.

use super::*;

impl SlateApp {
    pub(super) fn board_zoom_at(&mut self, pointer: Pos2, factor: f32) {
        let xf = self.board_xf();
        let world_before = xf.s2w(pointer);
        let cam = &mut self.tab_mut().cam;
        cam.z = (cam.z * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let cam_z = cam.z;
        let center = self.canvas_rect.center();
        self.tab_mut().cam.offset = world_before.to_vec2() - (pointer - center) / cam_z;
        self.tab_mut().grid_fade_armed = true;
    }

    pub(super) fn board_snap_threshold(&self) -> f32 {
        board_snap::SNAP_SCREEN_PX / self.tab().cam.z
    }

    pub(crate) fn snap_scope(&self) -> board_snap::SnapScope {
        let z = self.tab().cam.z.max(0.05);
        let (reach_px, lane_px) = self.board_snap_reach.screen_px();
        let xf = self.board_xf();
        let r = self.canvas_rect;
        let a = xf.s2w(r.min);
        let b = xf.s2w(r.max);
        let x0 = a.x.min(b.x);
        let y0 = a.y.min(b.y);
        board_snap::SnapScope {
            threshold: self.board_snap_threshold(),
            reach: if reach_px.is_finite() {
                reach_px / z
            } else {
                f32::INFINITY
            },
            lane: lane_px / z,
            view: WorldRect::new(x0, y0, (a.x - b.x).abs(), (a.y - b.y).abs()),
        }
    }

    /// Smart-guide snap sources: hidden nodes are out, **locked nodes stay
    /// in** (Rhino: locked still snaps), and connectors' derived AABBs never
    /// act as alignment targets. Unfilled paths are strokes — their AABB
    /// is not a guide (a closed polyline's box is mostly empty space).
    pub(crate) fn board_node_rects(&self) -> Vec<(NodeId, WorldRect)> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                if n.hidden || matches!(n.kind, NodeKind::Connector(_)) {
                    return false;
                }
                if let NodeKind::Shape(s) = &n.kind {
                    if s.shape == ShapeKind::Path && !s.fill.is_some_and(|f| f.0[3] > 0) {
                        return false;
                    }
                }
                true
            })
            .map(|n| (n.id, n.rect))
            .collect()
    }

    // ----- crop mode ---------------------------------------------------------------

    /// Whether the node is an image whose crop can be edited on canvas —
    /// the same eligibility as the inspector's Crop section: textured media
    /// (images / PDF pages / video posters / doc thumbnails), never 3D
    /// model viewports or text snippet cards.
    pub fn croppable_image(&self, id: NodeId) -> bool {
        let Some(n) = self.doc().scene.node(id) else {
            return false;
        };
        let NodeKind::Image(img) = &n.kind else {
            return false;
        };
        let Some(item) = self.doc().item(img.item) else {
            return false;
        };
        !matches!(
            slate_doc::media_kind(&item.path),
            slate_doc::MediaKind::Model
                | slate_doc::MediaKind::Text
                | slate_doc::MediaKind::Workbook
        ) && !self.model3d.external.contains(&item.cache_key)
    }

    /// Enter crop mode on an eligible image node (selects it and switches
    /// to the Select tool). Entering on another node switches to it.
    pub fn enter_crop_mode(&mut self, id: NodeId) {
        if !self.croppable_image(id) {
            return;
        }
        self.board_crop = Some(id);
        self.board_sel.insert(id);
        self.board_tool = BoardTool::Select;
        self.board_menu = None;
    }

    /// Crop is on, and the press landed on a selected croppable image.
    pub(super) fn press_on_selected_crop(&self, world: Pos2) -> bool {
        self.board_sel.iter().any(|id| {
            self.croppable_image(*id)
                && self
                    .doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| n.rect.contains_rotated(world.x, world.y, n.rotation_deg))
        })
    }

    /// The crop handle under the pointer across every selected croppable
    /// image. Corners beat edges, then the nearest wins, so press, cursor,
    /// and hover paint always agree.
    pub(super) fn crop_handle_under(
        &self,
        screen: Pos2,
    ) -> Option<(
        NodeId,
        board_handles::ResizeHandle,
        board_handles::SelectionGeom,
    )> {
        let xf = self.board_xf();
        let mut best: Option<((u8, f32), NodeId, board_handles::ResizeHandle, _)> = None;
        for id in &self.board_sel {
            if !self.croppable_image(*id) {
                continue;
            }
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
            let Some((handle, dist)) = board_handles::crop_handle_pick(screen, &geom) else {
                continue;
            };
            let rank = ((handle as u8) % 2, dist);
            if best
                .as_ref()
                .is_some_and(|(r, ..)| r.0 < rank.0 || (r.0 == rank.0 && r.1 <= rank.1))
            {
                continue;
            }
            best = Some((rank, *id, handle, geom));
        }
        best.map(|(_, id, handle, geom)| (id, handle, geom))
    }

    /// Crop is on and the pointer is on one of its handles or on a selected
    /// croppable image: the press belongs to crop mode, not to a click-away.
    pub(crate) fn crop_owns_pointer(&self, screen: Pos2) -> bool {
        self.board_crop.is_some()
            && (self.crop_handle_under(screen).is_some()
                || self.press_on_selected_crop(self.board_xf().s2w(screen)))
    }

    /// Crop handle under the pointer. Hit wins over wires and resize.
    pub(super) fn begin_crop_blister(&self, screen: Pos2) -> Option<BoardDrag> {
        let (id, handle, _) = self.crop_handle_under(screen)?;
        let before = self.doc().scene.node(id)?.clone();
        let peers = self
            .board_sel
            .iter()
            .filter(|peer| **peer != id && self.croppable_image(**peer))
            .filter_map(|peer| self.doc().scene.node(*peer).cloned())
            .collect();
        Some(BoardDrag::CropEdge {
            id,
            before,
            handle: handle as u8,
            peers,
        })
    }

    /// Per-frame crop-mode validity: exits when the node vanished, stopped
    /// being croppable, or a non-Select tool was picked.
    pub(super) fn sync_crop_mode(&mut self) {
        let Some(id) = self.board_crop else {
            return;
        };
        if self.board_tool != BoardTool::Select {
            self.board_crop = None;
        } else if !self.croppable_image(id) || !self.board_sel.contains(&id) {
            self.board_crop = self
                .board_sel
                .iter()
                .copied()
                .find(|next| self.croppable_image(*next));
        }
    }

    /// Crop-mode adornment: ghosted full image and scrim on the image under
    /// the pointer, and a side blister on each edge of every selected image.
    pub(super) fn paint_crop_overlay(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        let Some(mut id) = self.board_crop else {
            return;
        };
        if let Some(p) = ui.ctx().pointer_latest_pos() {
            let world = xf.s2w(p);
            if let Some(under) =
                self.board_sel.iter().copied().find(|cand| {
                    self.croppable_image(*cand)
                        && self.doc().scene.node(*cand).is_some_and(|n| {
                            n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
                        })
                })
            {
                id = under;
            }
        }
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let NodeKind::Image(img) = &node.kind else {
            return;
        };
        let palette = self.palette();
        let rot = node.rotation_deg;
        let (cx, cy) = node.rect.center();
        let content = board_crop::content_rect(node.rect, img.crop);

        // Points are computed in the node's local (unrotated) space, then
        // rotated about the node rect center — the same frame the crop math
        // and the node painter use.
        let rotate_w = |x: f32, y: f32| -> (f32, f32) {
            if rot.abs() < f32::EPSILON {
                return (x, y);
            }
            let rad = rot.to_radians();
            let (sin, cos) = rad.sin_cos();
            let dx = x - cx;
            let dy = y - cy;
            (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
        };
        let screen_of = |x: f32, y: f32| -> Pos2 {
            let (wx, wy) = rotate_w(x, y);
            xf.w2s(Pos2::new(wx, wy))
        };
        let quad_screen = |r: WorldRect| -> Vec<Pos2> {
            vec![
                screen_of(r.x, r.y),
                screen_of(r.x + r.w, r.y),
                screen_of(r.x + r.w, r.y + r.h),
                screen_of(r.x, r.y + r.h),
            ]
        };

        // Ghosted full image over the content rect (dimmed).
        let desired_px = (xf
            .rect_w2s(content)
            .width()
            .max(xf.rect_w2s(content).height()))
            * ui.ctx().pixels_per_point();
        if let Some(tex) = self.board_texture(ui.ctx(), node.id, img.item, &img.adjust, desired_px)
        {
            let vertices = node_texture_vertices(
                xf,
                node.rect,
                content,
                node.rotation_deg,
                Corner::Square,
                Crop::full(),
                img.mirror(),
            );
            paint_node_texture(
                painter,
                &tex,
                &vertices,
                Color32::WHITE.gamma_multiply(0.35),
            );
        }

        // Scrim between the content rect and the crop window (the masked
        // area of the ghost).
        let scrim = palette.bg.gamma_multiply(0.55);
        let right = node.rect.x + node.rect.w;
        let bottom = node.rect.y + node.rect.h;
        let bands = [
            WorldRect::new(content.x, content.y, content.w, node.rect.y - content.y),
            WorldRect::new(content.x, bottom, content.w, content.y + content.h - bottom),
            WorldRect::new(content.x, node.rect.y, node.rect.x - content.x, node.rect.h),
            WorldRect::new(
                right,
                node.rect.y,
                content.x + content.w - right,
                node.rect.h,
            ),
        ];
        for band in bands {
            if band.w > 0.01 && band.h > 0.01 {
                painter.add(egui::Shape::convex_polygon(
                    quad_screen(band),
                    scrim,
                    EStroke::NONE,
                ));
            }
        }

        let hot_pick = ui
            .ctx()
            .pointer_latest_pos()
            .and_then(|p| self.crop_handle_under(p));
        let mut hint_at = None;
        for id in self.board_sel.clone() {
            if !self.croppable_image(id) {
                continue;
            }
            let Some(n) = self.doc().scene.node(id) else {
                continue;
            };
            let geom = board_handles::selection_geom(xf, n.rect, n.rotation_deg);
            painter.add(egui::Shape::closed_line(
                geom.corners.to_vec(),
                EStroke::new(canvas_scale::px(1.0, geom.zoom), Color32::WHITE),
            ));
            let hot = hot_pick
                .as_ref()
                .filter(|(hot_id, ..)| *hot_id == id)
                .map(|(_, handle, _)| *handle);
            board_handles::paint_crop_handles(painter, &geom, hot, self.palette().select_fill);
            hint_at = Some((geom.edges[2], geom.zoom));
        }
        if let Some((at, z)) = hint_at {
            let hint = canvas_scale::px(11.0, z);
            if canvas_text::legible(hint) {
                canvas_text::text(
                    painter,
                    at + Vec2::new(0.0, canvas_scale::px(14.0, z)),
                    Align2::CENTER_TOP,
                    "Drag an edge to crop · Enter / Esc to finish",
                    FontId::proportional(hint),
                    palette.sub,
                );
            }
        }
    }
}
