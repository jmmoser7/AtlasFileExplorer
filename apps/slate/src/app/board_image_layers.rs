//! Image paint layers (trace paper). Contract: `image-paint-layers.md`.

use super::{board::BoardTool, board::BoardXf, SlateApp};
use eframe::egui::{self, Color32, Pos2};
use slate_doc::{
    image_paint::{
        find_layer_node, layer_node_from_world, layer_node_kind_allowed, layer_node_to_world,
        LayerNodeRef, PaintLayerId,
    },
    scene::{ImageNode, Node, NodeKind, Rgba, SceneCmd, ShapeKind, WorldRect},
    NodeId, PaintLayer, ViewState,
};

/// Which chip owns the shared intensity slider in the Filters capsule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStripFocus {
    Filter,
    Layer(usize),
}

#[derive(Clone, Debug)]
pub struct ImagePaintSession {
    pub image: NodeId,
    pub layer_index: usize,
    pub focus: ImageStripFocus,
}

pub fn tool_hosts_on_image(tool: BoardTool) -> bool {
    matches!(
        tool,
        BoardTool::RectShape
            | BoardTool::Ellipse
            | BoardTool::Line
            | BoardTool::Arc
            | BoardTool::Polyline
            | BoardTool::BezierSpan
            | BoardTool::Pen
            | BoardTool::Text
            | BoardTool::Brush
            | BoardTool::Sticky
            | BoardTool::Eraser
    )
}

/// Drop one image onto another: Replace base vs add as a layer child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageDropChoice {
    Replace,
    AddLayer,
}

#[derive(Clone, Debug)]
pub enum ImageDropSource {
    Node(NodeId),
    Item(slate_doc::ItemId),
}

#[derive(Clone, Debug)]
pub struct ImageDropOffer {
    pub target: NodeId,
    pub source: ImageDropSource,
    pub highlight: Option<ImageDropChoice>,
}

impl SlateApp {
    pub fn image_paint_session(&self) -> Option<&ImagePaintSession> {
        self.image_paint.as_ref()
    }

    pub fn clear_image_paint_session(&mut self) {
        self.image_paint = None;
    }

    pub fn paint_layer_host(&self) -> Option<NodeId> {
        self.image_paint.as_ref().map(|s| s.image)
    }

    pub fn supports_image_paint(id: NodeId, app: &SlateApp) -> bool {
        let Some(node) = app.doc().scene.node(id) else {
            return false;
        };
        let NodeKind::Image(img) = &node.kind else {
            return false;
        };
        let Some(item) = app.doc().item(img.item) else {
            return false;
        };
        if slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Image {
            return false;
        }
        true
    }

    pub fn agent_picture_draw_mode(&self, image: NodeId) -> bool {
        self.image_paint
            .as_ref()
            .is_some_and(|s| s.image == image && tool_hosts_on_image(self.board_tool))
    }

    pub fn sync_image_paint_for_tool(&mut self) {
        if !tool_hosts_on_image(self.board_tool) {
            return;
        }
        if self.board_sel.len() != 1 {
            return;
        }
        let id = *self.board_sel.iter().next().unwrap();
        if !Self::supports_image_paint(id, self) {
            return;
        }
        if self.image_paint.as_ref().map(|s| s.image) != Some(id) {
            self.ensure_paint_layer(id);
        }
    }

    fn next_paint_layer_id(img: &ImageNode) -> PaintLayerId {
        let next = img
            .paint_layers
            .iter()
            .map(|l| l.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        PaintLayerId(next)
    }

    pub fn ensure_paint_layer(&mut self, image: NodeId) {
        let node = match self.doc().scene.node(image) {
            Some(n) => n.clone(),
            None => return,
        };
        let NodeKind::Image(img) = &node.kind else {
            return;
        };
        let layer_index = if img.paint_layers.is_empty() {
            self.add_paint_layer_on_image(image);
            0
        } else {
            self.image_paint
                .as_ref()
                .filter(|s| s.image == image)
                .map(|s| s.layer_index)
                .unwrap_or(img.paint_layers.len().saturating_sub(1))
        };
        self.image_paint = Some(ImagePaintSession {
            image,
            layer_index,
            focus: ImageStripFocus::Layer(layer_index),
        });
    }

    pub fn add_paint_layer_on_image(&mut self, image: NodeId) -> Option<usize> {
        if self.refuse_read_only_edit() {
            return None;
        }
        let before = self.doc().scene.node(image)?.clone();
        let NodeKind::Image(ref img) = before.kind else {
            return None;
        };
        let id = Self::next_paint_layer_id(img);
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return None;
        };
        img.paint_layers.push(PaintLayer::new(id));
        let index = img.paint_layers.len() - 1;
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return None;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.layer.add"),
            Some(format!("layer:{}", id.0)),
        );
        Some(index)
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
        let Some(layer) = img.paint_layers.get(index) else {
            return;
        };
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return;
        };
        img.paint_layers[index].opacity = opacity.clamp(0.0, 1.0);
        if (img.paint_layers[index].opacity - layer.opacity).abs() < 1e-4 {
            return;
        }
        let _ = self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]);
        self.push_history(
            atlas_commands::CommandId("board.image.layer.opacity"),
            Some(format!("layer:{index}")),
        );
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
        if index >= img.paint_layers.len() {
            return;
        }
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return;
        };
        img.paint_layers.remove(index);
        let remaining = img.paint_layers.len();
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

    pub fn point_in_image_paint_window(&self, image: NodeId, world: Pos2) -> bool {
        let Some(node) = self.doc().scene.node(image) else {
            return false;
        };
        slate_doc::geom::point_in_node_outline(
            node,
            world.x,
            world.y,
            super::board_trim::trim_tokens::GEOMETRY_TOLERANCE,
        )
    }

    /// When hosting, route scene Adds into the active paint layer instead.
    pub fn try_commit_layer_nodes(&mut self, nodes: Vec<Node>) -> bool {
        let Some(session) = self.image_paint.clone() else {
            return false;
        };
        if nodes.is_empty() {
            return false;
        }
        if !nodes.iter().all(|n| layer_node_kind_allowed(&n.kind)) {
            return false;
        }
        let Some(host) = self.doc().scene.node(session.image).cloned() else {
            return false;
        };
        let NodeKind::Image(ref host_img) = host.kind else {
            return false;
        };
        if !nodes
            .iter()
            .all(|n| self.stroke_intersects_image_window(&host, host_img, n))
        {
            return false;
        }
        let locals: Vec<Node> = nodes
            .iter()
            .map(|n| layer_node_from_world(&host, host_img, n))
            .collect();
        let Some(before) = self.doc().scene.node(session.image).cloned() else {
            return false;
        };
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        let Some(layer) = img.paint_layers.get_mut(session.layer_index) else {
            return false;
        };
        layer.nodes.extend(locals);
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return false;
        }
        true
    }

    fn stroke_intersects_image_window(&self, host: &Node, _img: &ImageNode, node: &Node) -> bool {
        slate_doc::geom::stroke_intersects_node_outline(
            node,
            host,
            super::board_trim::trim_tokens::GEOMETRY_TOLERANCE,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_image_paint_layers(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        host: &Node,
        img: &ImageNode,
        _outline: &[Pos2],
        srect: egui::Rect,
        host_alpha: f32,
        _z: f32,
    ) {
        if img.paint_layers.is_empty() {
            return;
        }
        let clip = srect.intersect(painter.clip_rect());
        let sub = painter.with_clip_rect(clip);
        for layer in &img.paint_layers {
            if !layer.visible {
                continue;
            }
            for local in &layer.nodes {
                let mut world = layer_node_to_world(host, img, local);
                world.opacity = (world.opacity * layer.opacity * host_alpha).clamp(0.0, 1.0);
                self.paint_board_node(ui, &sub, xf, &world, false);
            }
        }
    }

    pub fn on_image_paint_add_clicked(&mut self, image: NodeId) {
        if let Some(idx) = self.add_paint_layer_on_image(image) {
            self.image_paint = Some(ImagePaintSession {
                image,
                layer_index: idx,
                focus: ImageStripFocus::Layer(idx),
            });
            if self.board_tool == BoardTool::Select {
                self.board_tool = BoardTool::Brush;
            }
        }
    }

    pub(crate) fn eraser_hits_active_layer_at(&self, world: Pos2) -> Vec<NodeId> {
        let Some(session) = self.image_paint.as_ref() else {
            return Vec::new();
        };
        let Some(host) = self.doc().scene.node(session.image) else {
            return Vec::new();
        };
        let NodeKind::Image(img) = &host.kind else {
            return Vec::new();
        };
        let Some(layer) = img.paint_layers.get(session.layer_index) else {
            return Vec::new();
        };
        let zoom = self.tab().cam.z;
        let slop = (self.eraser_width * 0.5).max(1.0);
        let mut hits = Vec::new();
        for local in &layer.nodes {
            let n = layer_node_to_world(host, img, local);
            if n.hidden || n.locked {
                continue;
            }
            if Self::eraser_hits_world_node(&n, world, slop, zoom) {
                hits.push(local.id);
            }
        }
        hits
    }

    fn eraser_hits_world_node(n: &Node, world: Pos2, slop: f32, zoom: f32) -> bool {
        let NodeKind::Shape(s) = &n.kind else {
            return false;
        };
        match s.shape {
            ShapeKind::Path => {
                let Some(path) = s.path.as_ref() else {
                    return false;
                };
                if path.is_empty() {
                    return false;
                }
                if s.stroke.paints_as_stamp() {
                    let rect = n.rect.normalized();
                    let ink = s.stroke.width * 0.5
                        + path.tips.iter().map(|t| t.width * 0.5).fold(0.0, f32::max);
                    return world.x >= rect.x - ink - slop
                        && world.x <= rect.x + rect.w + ink + slop
                        && world.y >= rect.y - ink - slop
                        && world.y <= rect.y + rect.h + ink + slop;
                }
                let bez = super::board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
                let style = super::board_path::stroke_style_world(&s.stroke, zoom);
                vector_ink::hit_stroke(&bez, &style, [world.x, world.y], slop)
            }
            ShapeKind::Line => {
                let (a, b) = super::board_color::line_endpoints(n.rect, s.flip, n.rotation_deg);
                super::board_color::dist_point_segment(world, a, b)
                    <= slop + s.stroke.width.max(1.0) * 0.5
            }
            _ => false,
        }
    }

    pub(crate) fn collect_layer_erase_spot(
        &self,
        spot: &mut Vec<NodeId>,
        points: &[Pos2],
        straight: bool,
    ) {
        let Some(session) = self.image_paint.as_ref() else {
            return;
        };
        let Some(host) = self.doc().scene.node(session.image) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        let Some(layer) = img.paint_layers.get(session.layer_index) else {
            return;
        };
        let tail: &[Pos2] = if straight || points.len() < 2 {
            points
        } else {
            &points[points.len() - 2..]
        };
        let r = self.eraser_width * 0.5;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in tail {
            x0 = x0.min(p.x - r);
            y0 = y0.min(p.y - r);
            x1 = x1.max(p.x + r);
            y1 = y1.max(p.y + r);
        }
        if !x0.is_finite() {
            return;
        }
        let ink = super::settings::STROKE_WIDTH_MAX * 0.5;
        let _reach_pad = ink;
        for local in &layer.nodes {
            if spot.contains(&local.id) {
                continue;
            }
            let n = layer_node_to_world(host, img, local);
            let NodeKind::Shape(s) = &n.kind else {
                continue;
            };
            if s.shape != ShapeKind::Path || s.path.is_none() || !s.stroke.paints_as_stamp() {
                continue;
            };
            let ink = s.stroke.width * 0.5
                + s.path
                    .as_ref()
                    .map(|p| p.tips.iter().map(|t| t.width * 0.5).fold(0.0, f32::max))
                    .unwrap_or(0.0);
            let rect = n.rect.normalized();
            let reach = if n.rotation_deg.abs() > 0.01 {
                ink + rect.w.max(rect.h)
            } else {
                ink
            };
            if rect.x - reach <= x1
                && rect.x + rect.w + reach >= x0
                && rect.y - reach <= y1
                && rect.y + rect.h + reach >= y0
            {
                spot.push(local.id);
            }
        }
    }

    /// Eraser release for paint-layer strokes (active layer only).
    pub(crate) fn finish_erase_layer_spot(
        &mut self,
        spot: &[NodeId],
        points: &[Pos2],
        span: slate_doc::scene::StrokeSpan,
        live: &std::collections::HashMap<NodeId, super::board_path::EraseLive>,
    ) -> (Vec<SceneCmd>, usize) {
        let mut by_host: std::collections::HashMap<NodeId, Node> = std::collections::HashMap::new();
        let mut removes: Vec<LayerNodeRef> = Vec::new();
        let mut touched = 0usize;
        for id in spot {
            if live.get(id).is_some_and(|l| !l.changed) {
                continue;
            }
            let Some(loc) = find_layer_node(&self.doc().scene, *id) else {
                continue;
            };
            let Some(session) = self.image_paint.as_ref() else {
                continue;
            };
            if session.image != loc.image || session.layer_index != loc.layer_index {
                continue;
            }
            let host = by_host
                .entry(loc.image)
                .or_insert_with(|| self.doc().scene.node(loc.image).unwrap().clone());
            let host_snapshot = host.clone();
            let NodeKind::Image(ref snap_img) = host_snapshot.kind else {
                continue;
            };
            let NodeKind::Image(ref mut img) = host.kind else {
                continue;
            };
            let Some(layer) = img.paint_layers.get_mut(loc.layer_index) else {
                continue;
            };
            let Some(local) = layer.nodes.get_mut(loc.node_index) else {
                continue;
            };
            let mut world = layer_node_to_world(&host_snapshot, snap_img, local);
            let NodeKind::Shape(shape) = &mut world.kind else {
                continue;
            };
            let Some(path) = shape.path.as_mut() else {
                continue;
            };
            std::sync::Arc::make_mut(path)
                .erase
                .push(slate_doc::scene::EraseMark {
                    points: points
                        .iter()
                        .map(|p| {
                            super::board_path::world_to_node_norm(
                                *p,
                                world.rect,
                                world.rotation_deg,
                            )
                        })
                        .collect(),
                    tips: vec![span],
                });
            let (ink, gone) = super::board_color::erased_result(&world);
            if !ink {
                continue;
            }
            touched += 1;
            if gone {
                removes.push(loc);
            } else {
                *local = layer_node_from_world(&host_snapshot, snap_img, &world);
            }
        }
        removes.sort_by(|a, b| {
            b.layer_index
                .cmp(&a.layer_index)
                .then(b.node_index.cmp(&a.node_index))
        });
        for loc in removes {
            if let NodeKind::Image(ref mut img) = by_host.get_mut(&loc.image).unwrap().kind {
                if let Some(layer) = img.paint_layers.get_mut(loc.layer_index) {
                    if loc.node_index < layer.nodes.len() {
                        layer.nodes.remove(loc.node_index);
                    }
                }
            }
        }
        let mut cmds = Vec::new();
        for (image, after) in by_host {
            let before = self.doc().scene.node(image).unwrap().clone();
            if before != after {
                cmds.push(SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                });
            }
        }
        (cmds, touched)
    }

    pub(crate) fn paint_image_paint_recent_colors(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        host: &Node,
        srect: egui::Rect,
    ) {
        let Some(session) = self.image_paint.as_ref() else {
            return;
        };
        if session.image != host.id {
            return;
        }
        let z = xf.z;
        let recents = self.doc().view.recent_colors.clone().unwrap_or_default();
        let n = recents.len().min(ViewState::RECENT_COLOR_LIMIT);
        if n == 0 {
            return;
        }
        let gap = atlas_shell::canvas_scale::px(6.0, z);
        let radius = atlas_shell::canvas_scale::px(8.0, z);
        let row_w = n as f32 * (radius * 2.0 + gap) - gap;
        let center = egui::pos2(srect.center().x, srect.bottom() + gap + radius);
        let mut x = center.x - row_w * 0.5 + radius;
        let pointer = ui.ctx().pointer_latest_pos();
        for (i, rgb) in recents.iter().take(n).enumerate() {
            let at = egui::pos2(x + i as f32 * (radius * 2.0 + gap), center.y);
            let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            painter.circle_filled(at, radius, color);
            painter.circle_stroke(
                at,
                radius,
                egui::Stroke::new(atlas_shell::canvas_scale::px(1.0, z), egui::Color32::WHITE),
            );
            if pointer.is_some_and(|p| (p - at).length() <= radius * 1.2)
                && ui.input(|i| i.pointer.primary_clicked())
            {
                self.adopt_board_color_rgb(*rgb);
            }
        }
    }

    pub fn adopt_board_color_rgb(&mut self, rgb: [u8; 3]) {
        let a = self.board_colors.fg.0[3];
        self.board_colors.fg = Rgba([rgb[0], rgb[1], rgb[2], a]);
        self.settings.board_fg = Some(self.board_colors.fg.0);
        self.tab_mut().doc.view.remember_color(rgb);
    }

    pub fn replace_image_item_preserve_layers(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        remove_source: Option<NodeId>,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(before) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let NodeKind::Image(ref img) = before.kind else {
            return false;
        };
        if img.item == item {
            return false;
        }
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        img.item = item;
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return false;
        }
        if let Some(id) = remove_source.filter(|id| *id != target) {
            self.delete_board_nodes(&[id]);
        }
        self.push_history(
            atlas_commands::CommandId("board.image.replace"),
            Some("replace base".into()),
        );
        true
    }

    pub fn add_dropped_image_as_layer(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        remove_source: Option<NodeId>,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        self.ensure_paint_layer(target);
        let session = self.image_paint.clone().unwrap();
        let Some(host) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let NodeKind::Image(ref host_img) = host.kind else {
            return false;
        };
        let child = self.doc_mut().scene.build_node(
            WorldRect::new(host.rect.x, host.rect.y, host.rect.w, host.rect.h),
            NodeKind::Image(ImageNode::new(item)),
        );
        let locals = vec![layer_node_from_world(&host, host_img, &child)];
        let Some(before) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        let Some(layer) = img.paint_layers.get_mut(session.layer_index) else {
            return false;
        };
        layer.nodes.extend(locals);
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return false;
        }
        if let Some(id) = remove_source.filter(|id| *id != target) {
            self.delete_board_nodes(&[id]);
        }
        self.push_history(
            atlas_commands::CommandId("board.image.layer.add_image"),
            Some("add as layer".into()),
        );
        true
    }

    pub(crate) fn update_image_drop_offer(&mut self, world: Pos2, ids: &[NodeId]) {
        self.image_drop = None;
        if ids.len() != 1 || self.board_tool != BoardTool::Select {
            return;
        };
        let source_id = ids[0];
        let Some(source) = self.doc().scene.node(source_id) else {
            return;
        };
        let NodeKind::Image(src_img) = &source.kind else {
            return;
        };
        if src_img.item.is_none() {
            return;
        };
        let target = self.image_under_point(world, source_id);
        let Some(target) = target else {
            return;
        };
        self.image_drop = Some(ImageDropOffer {
            target,
            source: ImageDropSource::Node(source_id),
            highlight: None,
        });
    }

    fn image_under_point(&self, world: Pos2, skip: NodeId) -> Option<NodeId> {
        let pick = super::board_path::board_pick_node_routed(
            &self.doc().scene,
            world.x,
            world.y,
            self.tab().cam.z,
            false,
            self.board_wire_routing,
        )?;
        if pick == skip {
            return None;
        }
        let node = self.doc().scene.node(pick)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        if img.item.is_none() {
            return None;
        };
        if !Self::supports_image_paint(pick, self) {
            return None;
        }
        Some(pick)
    }

    pub(crate) fn paint_image_drop_capsules(&mut self, ui: &egui::Ui, painter: &egui::Painter) {
        let Some(offer) = self.image_drop.clone() else {
            return;
        };
        let Some(pointer) = ui.ctx().pointer_latest_pos() else {
            return;
        };
        let z = self.tab().cam.z;
        let labels = [
            ("Replace", ImageDropChoice::Replace),
            ("Add as layer", ImageDropChoice::AddLayer),
        ];
        let pad = atlas_shell::canvas_scale::px(10.0, z);
        let h = atlas_shell::canvas_scale::px(22.0, z);
        let gap = atlas_shell::canvas_scale::px(6.0, z);
        let palette = self.palette();
        let mut rects = Vec::new();
        let mut x = pointer.x + pad;
        let y = pointer.y + pad;
        let mut highlight = None;
        for (label, choice) in labels {
            let galley = painter.layout(
                label.to_string(),
                atlas_shell::canvas_scale::font(12.0, z),
                palette.ink,
                f32::INFINITY,
            );
            let w = galley.size().x + pad * 2.0;
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
            if rect.contains(pointer) {
                highlight = Some(choice);
            }
            rects.push((rect, label, choice));
            x += w + gap;
        }
        if let Some(d) = &mut self.image_drop {
            d.highlight = highlight;
        }
        for (rect, label, choice) in rects {
            let hot = highlight == Some(choice);
            painter.rect_filled(
                rect,
                h * 0.5,
                if hot { palette.accent } else { palette.card },
            );
            atlas_shell::canvas_text::text(
                painter,
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                atlas_shell::canvas_scale::font(12.0, z),
                if hot { Color32::WHITE } else { palette.ink },
            );
        }
    }

    pub(crate) fn try_commit_image_drop(&mut self, ids: &[NodeId], before: &[Node]) -> bool {
        let Some(offer) = self.image_drop.take() else {
            return false;
        };
        let Some(choice) = offer.highlight else {
            return false;
        };
        let item = match offer.source {
            ImageDropSource::Node(id) => {
                let Some(node) = self.doc().scene.node(id) else {
                    return false;
                };
                let NodeKind::Image(img) = &node.kind else {
                    return false;
                };
                img.item
            }
            ImageDropSource::Item(item) => item,
        };
        if item.is_none() {
            return false;
        }
        let source_node = match offer.source {
            ImageDropSource::Node(id) => Some(id),
            ImageDropSource::Item(_) => None,
        };
        let ok = match choice {
            ImageDropChoice::Replace => {
                self.replace_image_item_preserve_layers(offer.target, item, source_node)
            }
            ImageDropChoice::AddLayer => {
                self.add_dropped_image_as_layer(offer.target, item, source_node)
            }
        };
        if ok {
            // Revert the move gesture — drop choice owns the mutation.
            let scene = &mut self.doc_mut().scene;
            for (id, b) in ids.iter().zip(before.iter()) {
                if let Some(n) = scene.node_mut(*id) {
                    *n = b.clone();
                }
            }
            return true;
        }
        false
    }
}
