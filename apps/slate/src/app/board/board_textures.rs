//! Texture lookup and selection glyphs.

use super::*;

impl SlateApp {
    // ----- textures -------------------------------------------------------------

    /// Effective photo filter for paint, including in-progress strip previews.
    pub(super) fn model_adjust_for_paint(&self, node_id: NodeId) -> ImageAdjust {
        self.shape_properties
            .preview
            .iter()
            .find(|n| n.id == node_id)
            .and_then(slate_doc::scene::adjust_of)
            .or_else(|| {
                self.doc()
                    .scene
                    .node(node_id)
                    .and_then(slate_doc::scene::adjust_of)
            })
            .unwrap_or_default()
    }

    /// Texture for an image node, applying non-destructive adjustments via
    /// the fx cache. Falls back to the plain thumb while pixels are pending.
    ///
    /// `desired_px` is the node's on-screen size (physical px, longest edge).
    /// Every image, filtered or not, queues the lazy full-resolution preview
    /// through `item_texture`. A live hover or slider scrub filters the
    /// thumbnail so the frame stays cheap. A committed adjustment filters the
    /// sharp preview once, when that decode is resident.
    pub(super) fn board_texture(
        &mut self,
        ctx: &egui::Context,
        node_id: NodeId,
        item: ItemId,
        adjust: &ImageAdjust,
        desired_px: f32,
    ) -> Option<egui::TextureHandle> {
        let plain = self.item_texture(item, desired_px);
        if adjust.is_identity() {
            return plain;
        }
        let (key, _, _, _) = self.resolved_item_preview(item)?;
        if key.is_empty() {
            return plain;
        }
        let committed = self
            .doc()
            .scene
            .node(node_id)
            .and_then(slate_doc::scene::adjust_of)
            == Some(*adjust);
        let source_px = if committed {
            self.preview_cache.get(&key).map(|e| e.px).unwrap_or(0)
        } else {
            0
        };
        match self.adjusted_texture(ctx, &key, adjust, source_px) {
            Some(tex) => Some(tex),
            None => {
                self.request_thumb(item);
                plain
            }
        }
    }

    /// The picture resident under `key`, filtered by `adjust` the way the
    /// artifact's CSS filters it, cached. None while no pixels are resident.
    pub(super) fn adjusted_texture(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        adjust: &ImageAdjust,
        source_px: u32,
    ) -> Option<egui::TextureHandle> {
        let fx_key = (key.to_string(), adjust.cache_hash(), source_px);
        if let Some(t) = self.fx_textures.get(&fx_key) {
            return Some(t.clone());
        }
        let out = {
            let pixels = if source_px > 0 {
                self.preview_cache.get(key).map(|e| &e.pixels)
            } else {
                None
            };
            super::super::imagefx::adjusted(pixels.or_else(|| self.thumb_pixels.get(key))?, adjust)
        };
        let tex = ctx.load_texture(
            format!("slate-fx-{}-{}-{}", fx_key.0, fx_key.1, fx_key.2),
            out,
            egui::TextureOptions::LINEAR,
        );
        if self.fx_textures.len() > 256 {
            self.fx_textures.clear();
        }
        self.fx_textures.insert(fx_key, tex.clone());
        Some(tex)
    }

    /// An agent picture's shown result, through the same preview queue and
    /// filters as a placed picture.
    pub(crate) fn agent_picture_texture(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
        adjust: &ImageAdjust,
        desired_px: f32,
    ) -> Option<egui::TextureHandle> {
        if path.as_os_str().is_empty() {
            return None;
        }
        let plain = self.linked_image_texture(path.to_path_buf(), "shown", desired_px);
        if adjust.is_identity() {
            return plain;
        }
        let key = super::super::preview::linked_image_key(path, "shown");
        let source_px = self.preview_cache.get(&key).map(|e| e.px).unwrap_or(0);
        self.adjusted_texture(ctx, &key, adjust, source_px)
            .or(plain)
    }

    /// Natural pixel dimensions for an item, scaled to a sensible board size.
    pub(crate) fn image_natural_size(&self, item: ItemId) -> (f32, f32) {
        let (mut w, mut h) = match self.doc().item(item) {
            Some(it) => self
                .thumb_pixels
                .get(&it.cache_key)
                .map(|img| (img.width() as f32, img.height() as f32))
                // A drop lands before its thumbnail does, so read the header
                // rather than boxing the image at the default aspect. Never on
                // a placeholder: one header byte hydrates the whole file.
                .or_else(|| {
                    let path = &it.path;
                    (slate_doc::media_kind(path) == slate_doc::MediaKind::Image
                        && !atlas_core::cloud::is_dehydrated(path))
                    .then(|| image::image_dimensions(path).ok())
                    .flatten()
                    .map(|(pw, ph)| (pw as f32, ph as f32))
                })
                .unwrap_or((IMAGE_W, IMAGE_H)),
            None => (IMAGE_W, IMAGE_H),
        };
        if w <= 0.0 || h <= 0.0 {
            w = IMAGE_W;
            h = IMAGE_H;
        }
        let max_dim = 320.0;
        let scale = (max_dim / w.max(h)).min(1.0);
        (w * scale, h * scale)
    }

    pub(super) fn paint_board_grid(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        palette: &atlas_shell::theme::Palette,
        xf: &BoardXf,
        alpha: f32,
    ) {
        if alpha <= 0.001 {
            return;
        }
        let dot = palette.grid_dot.gamma_multiply(alpha);
        let step = board_snap::GRID_WORLD * xf.z;
        if step < 6.0 {
            return;
        }
        let origin = xf.w2s(Pos2::ZERO);
        let x0 = origin.x + ((rect.left() - origin.x) / step).floor() * step;
        let y0 = origin.y + ((rect.top() - origin.y) / step).floor() * step;
        let mut y = y0;
        while y < rect.bottom() {
            let mut x = x0;
            while x < rect.right() {
                painter.circle_filled(Pos2::new(x, y), 1.0, dot);
                x += step;
            }
            y += step;
        }
    }

    /// Axis-aligned world bounds of the multi-selection (union of each
    /// member's rotated-corner bounds). `None` when nothing is selected.
    pub(crate) fn board_group_bounds(&self) -> Option<WorldRect> {
        board_snap::union_rect(
            &self
                .board_sel
                .iter()
                .filter_map(|id| self.doc().scene.node(*id))
                .map(|n| n.rect.rotated_bounds(n.rotation_deg))
                .collect::<Vec<_>>(),
        )
    }

    /// Subtle selection highlight that follows the painted shape, not its
    /// axis-aligned box. Paths use the stroke; closed primitives get a faint
    /// fill plus the silhouette. `rotate_hover` paints the single-select
    /// rotate hint.
    pub(super) fn paint_selected_node(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        n: &Node,
        select_tint: Color32,
        outline_w: f32,
        rotate_hover: bool,
    ) {
        if slate_doc::agent_chat::agent(n).is_some() {
            return;
        }
        if matches!(n.kind, NodeKind::Connector(_)) {
            self.paint_connector_selection(painter, xf, n);
            return;
        }
        if Self::node_uses_curve_grips(n) {
            self.paint_line_grips(painter, xf, n);
            return;
        }
        if let NodeKind::Shape(s) = &n.kind {
            if let Some(path) = s.path.as_ref() {
                if !path.is_empty() {
                    board_path::paint_path_stroke_outline(
                        painter,
                        xf,
                        n,
                        s,
                        path,
                        EStroke::new(outline_w, select_tint),
                    );
                    self.paint_node_fillet_grip(painter, xf, n, select_tint);
                    return;
                }
            }
        }
        let outline = self.node_screen_outline(painter.ctx(), xf, n);
        if outline.len() >= 3 {
            painter.add(egui::Shape::convex_polygon(
                outline.clone(),
                select_tint.gamma_multiply(0.16),
                EStroke::NONE,
            ));
        }
        if rotate_hover {
            let geom = board_handles::selection_geom(xf, n.rect, n.rotation_deg);
            board_handles::paint_selection(
                painter,
                &geom,
                &outline,
                select_tint,
                self.board_hover_hit,
                outline_w,
            );
            self.paint_node_fillet_grip(painter, xf, n, select_tint);
        } else {
            painter.add(egui::Shape::closed_line(
                outline,
                EStroke::new(outline_w, select_tint),
            ));
            self.paint_node_fillet_grip(painter, xf, n, select_tint);
        }
        self.paint_node_sides_glyphs(painter, xf, n, select_tint);
    }

    pub(super) fn paint_node_sides_glyphs(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        n: &Node,
        select_tint: Color32,
    ) {
        if self.board_sides_hover.is_none_or(|(id, _)| id != n.id) {
            return;
        }
        let pointer = painter.ctx().pointer_hover_pos();
        for g in self.polygon_sides_glyphs(xf) {
            let hot = pointer.is_some_and(|p| {
                p.distance(g.center)
                    <= canvas_scale::hit_px(board_handles::SIDES_GLYPH_RADIUS, xf.z)
            });
            board_handles::paint_sides_glyph(
                painter,
                g.center,
                g.radius,
                xf.z,
                g.add,
                select_tint,
                if hot {
                    self.palette().select_fill
                } else {
                    Color32::WHITE
                },
            );
        }
    }

    pub(super) fn paint_node_fillet_grip(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        n: &Node,
        select_tint: Color32,
    ) {
        for (vertex, grip) in self.corner_grips(n, xf) {
            let hot = (self.board_hover_node == Some(n.id)
                && self.board_hover_grip_vertex == vertex
                && matches!(
                    self.board_hover_hit,
                    Some(board_handles::BoardHitTarget::FilletRadius)
                ))
                || matches!(
                    self.board_drag,
                    Some(BoardDrag::FilletRadius { id, vertex: held, .. })
                        if id == n.id && held == vertex
                );
            board_handles::paint_fillet_grip(
                painter,
                grip,
                xf.z,
                select_tint,
                hot,
                self.palette().select_fill,
            );
        }
    }
}
