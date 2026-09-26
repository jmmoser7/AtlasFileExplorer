//! Bounding-box chrome: Windows-style hover resize, optional rotate.
//!
//! Select-tool edges and corners are live without a prior selection. Portals
//! and simple lines do not rotate. Aspect-ratio keys stay in `board_snap`
//! (Shift / Ctrl) — this module only decides what the pointer is aimed at.

use super::board::{BoardDrag, BoardXf};
use super::{board_handles, SlateApp};
use eframe::egui::{self, Pos2, Vec2};
use slate_doc::scene::{Corner, Node, NodeKind};
use slate_doc::NodeId;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct FilletRequest {
    pub ids: Vec<NodeId>,
    pub radius: f32,
    /// One polyline corner of the first id instead of the shared amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertex: Option<usize>,
}

impl SlateApp {
    /// Portals stay axis-aligned; connectors and simple lines have no bbox
    /// rotate. Everything else with a rectangular frame can rotate.
    pub(crate) fn node_allows_rotation(node: &Node) -> bool {
        !matches!(
            node.kind,
            NodeKind::Portal(_) | NodeKind::Connector(_) | NodeKind::DockStrip(_)
        ) && !Self::node_uses_curve_grips(node)
    }

    pub(crate) fn node_supports_fillet_grip(&self, node: &Node) -> bool {
        slate_doc::scene::supports_corners(node) && !self.frame_chrome_suppressed(node.id)
    }

    pub(crate) fn node_item_path<'a>(&'a self, node: &Node) -> Option<&'a std::path::Path> {
        match &node.kind {
            NodeKind::Image(i) => self.viewed_doc().item(i.item).map(|it| it.path.as_path()),
            _ => None,
        }
    }

    pub(crate) fn node_resolved_corner(&self, node: &Node) -> Corner {
        slate_doc::scene::resolved_corner(node, self.node_item_path(node))
    }

    pub(crate) fn node_fillet_radius_world(&self, node: &Node) -> f32 {
        self.node_resolved_corner(node)
            .effective(node.rect.w, node.rect.h)
            .1
    }

    #[cfg(test)]
    pub(crate) fn apply_fillet_radius_from_drag(
        &self,
        node: &mut Node,
        before: &Node,
        radius: f32,
        shift: bool,
    ) {
        let path = self.node_item_path(before);
        Self::apply_fillet_radius_to_node(node, before, radius, shift, path);
    }

    pub(crate) fn apply_fillet_radius_to_node(
        node: &mut Node,
        before: &Node,
        mut radius: f32,
        shift: bool,
        path: Option<&std::path::Path>,
    ) {
        if shift {
            radius = radius.round();
        }
        let before_radius = slate_doc::scene::resolved_corner(before, path)
            .effective(before.rect.w, before.rect.h)
            .1;
        if (radius - before_radius).abs() < 1e-4 {
            *node = before.clone();
            return;
        }
        slate_doc::scene::edit_corner(node, path, |resolved| {
            let (chamfer, is_percent, _) = resolved.parameters();
            Corner::from_parameters(chamfer, false, radius.max(0.0)).with_mode(
                is_percent,
                before.rect.w,
                before.rect.h,
            )
        });
    }

    /// Per-vertex overrides follow a shared edit; a vertex edit sets only its
    /// own corner.
    pub(crate) fn apply_corner_amount(
        node: &mut Node,
        before: &Node,
        vertex: Option<usize>,
        amount: f32,
        path: Option<&std::path::Path>,
    ) {
        let Some(v) = vertex else {
            Self::apply_fillet_radius_to_node(node, before, amount, false, path);
            slate_doc::scene::clear_vertex_corner_amounts(node);
            return;
        };
        *node = before.clone();
        let shared = slate_doc::scene::resolved_corner(before, path)
            .effective(before.rect.w, before.rect.h)
            .1;
        if (amount - Self::vertex_amount(before, v, shared)).abs() >= 1e-4 {
            slate_doc::scene::set_vertex_corner_amount(node, v, amount);
        }
    }

    fn vertex_amount(node: &Node, vertex: usize, shared: f32) -> f32 {
        match &node.kind {
            NodeKind::Shape(s) => s
                .path
                .as_ref()
                .map_or(shared, |p| p.vertex_corner_amount(vertex, shared)),
            _ => shared,
        }
    }

    /// The corner amount a grip shows: its vertex's, or the shared one.
    pub(crate) fn node_grip_amount(&self, node: &Node, vertex: Option<usize>) -> f32 {
        let shared = self.node_fillet_radius_world(node);
        vertex.map_or(shared, |v| Self::vertex_amount(node, v, shared))
    }

    /// Screen position of the first visible corner grip on `node`.
    pub(crate) fn fillet_grip_at(&self, node: &Node, xf: &BoardXf) -> Option<Pos2> {
        self.corner_grips(node, xf).first().map(|g| g.1)
    }

    /// Every visible corner grip on `node`, keyed by polyline vertex
    /// (`None` is the shared grip).
    pub(crate) fn corner_grips(&self, node: &Node, xf: &BoardXf) -> Vec<(Option<usize>, Pos2)> {
        if !self.board_sel.contains(&node.id)
            || self.board_crop.is_some()
            || !self.node_supports_fillet_grip(node)
        {
            return Vec::new();
        }
        let geom = board_handles::selection_geom(xf, node.rect, node.rotation_deg);
        let grip_px = atlas_shell::canvas_scale::px(board_handles::FILLET_GRIP_HIT_PX, geom.zoom);
        if atlas_shell::canvas_scale::too_small(grip_px) {
            return Vec::new();
        }
        self.node_grip_edges(node)
            .into_iter()
            .map(|(vertex, edge)| {
                let travel = match &self.board_drag {
                    Some(BoardDrag::FilletRadius {
                        id,
                        vertex: held,
                        pointer_travel,
                        ..
                    }) if *id == node.id && *held == vertex => *pointer_travel,
                    _ => board_handles::corner_grip_rest_travel(
                        &edge,
                        self.node_grip_amount(node, vertex),
                    ),
                };
                (vertex, board_handles::corner_grip_screen(xf, &edge, travel))
            })
            .collect()
    }

    /// The grips `node` offers: one per polyline corner when it is the only
    /// selection, otherwise the shared grip.
    fn node_grip_edges(
        &self,
        node: &Node,
    ) -> Vec<(Option<usize>, slate_doc::geom::CornerGripEdge)> {
        if self.board_sel.len() == 1 {
            let (chamfer, _) = self
                .node_resolved_corner(node)
                .effective(node.rect.w, node.rect.h);
            let each = slate_doc::geom::polyline_vertex_grip_edges(node, chamfer);
            if !each.is_empty() {
                return each.into_iter().map(|(v, e)| (Some(v), e)).collect();
            }
        }
        self.node_corner_grip_edge(node)
            .map(|e| (None, e))
            .into_iter()
            .collect()
    }

    /// The edge the grip for `vertex` rides (`None`: the shared grip).
    pub(crate) fn node_grip_edge(
        &self,
        node: &Node,
        vertex: Option<usize>,
    ) -> Option<slate_doc::geom::CornerGripEdge> {
        let Some(v) = vertex else {
            return self.node_corner_grip_edge(node);
        };
        let (chamfer, _) = self
            .node_resolved_corner(node)
            .effective(node.rect.w, node.rect.h);
        slate_doc::geom::polyline_vertex_grip_edges(node, chamfer)
            .into_iter()
            .find_map(|(i, e)| (i == v).then_some(e))
    }

    /// The edge the corner grip rides on `node` (P1.node.corner-grip).
    pub(crate) fn node_corner_grip_edge(
        &self,
        node: &Node,
    ) -> Option<slate_doc::geom::CornerGripEdge> {
        let (chamfer, _) = self
            .node_resolved_corner(node)
            .effective(node.rect.w, node.rect.h);
        slate_doc::geom::corner_grip_edge(node, chamfer)
    }

    /// The topmost selected host whose corner grip is under `screen`, with
    /// the polyline vertex that grip sets.
    pub(crate) fn fillet_grip_hit_at(&self, screen: Pos2) -> Option<(NodeId, Option<usize>)> {
        let xf = self.board_xf();
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .filter(|n| self.board_sel.contains(&n.id))
            .find_map(|n| {
                let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                self.corner_grips(n, &xf)
                    .into_iter()
                    .find(|(_, grip)| board_handles::hit_test_fillet_grip(screen, &geom, *grip))
                    .map(|(vertex, _)| (n.id, vertex))
            })
    }

    /// The other selected hosts one corner grip edits alongside `id`.
    pub(crate) fn corner_grip_peers(&self, id: NodeId) -> Vec<Node> {
        self.board_sel
            .iter()
            .filter(|p| **p != id)
            .filter_map(|p| self.doc().scene.node(*p))
            .filter(|n| !n.locked && self.node_supports_fillet_grip(n))
            .cloned()
            .collect()
    }

    pub(crate) fn begin_fillet_drag(&mut self, screen: Pos2, world: Pos2) -> Option<BoardDrag> {
        let (id, vertex) = self.fillet_grip_hit_at(screen)?;
        let before = self.doc().scene.node(id)?.clone();
        let edge = self.node_grip_edge(&before, vertex)?;
        let start_amount = self.node_grip_amount(&before, vertex);
        let press_travel = edge.project([world.x, world.y]);
        Some(BoardDrag::FilletRadius {
            id,
            vertex,
            before,
            peers: if vertex.is_none() {
                self.corner_grip_peers(id)
            } else {
                Vec::new()
            },
            start_amount,
            press_travel,
            pointer_travel: press_travel.clamp(0.0, edge.max_travel),
            press: screen,
            max_px: 0.0,
        })
    }

    /// The corner amount follows the pointer's travel along the grip edge
    /// from its press-time value, so a grab never jumps. The held grip paints
    /// under the pointer; it settles to its resting travel on release.
    pub(crate) fn update_fillet_drag(&mut self, world: Pos2, shift: bool) {
        let Some(BoardDrag::FilletRadius {
            id,
            vertex,
            before,
            peers,
            start_amount,
            press_travel,
            press,
            max_px,
            ..
        }) = &self.board_drag
        else {
            return;
        };
        let moved = press.distance(self.board_xf().w2s(world));
        // A press that has not moved yet leaves the corner as it was.
        if *max_px <= 0.0 && moved < 1e-3 {
            return;
        }
        let (id, vertex, start_amount, press_travel) = (*id, *vertex, *start_amount, *press_travel);
        let before = before.clone();
        let peers = peers.clone();
        let Some(edge) = self.node_grip_edge(&before, vertex) else {
            return;
        };
        let projected = edge.project([world.x, world.y]);
        let delta = projected - press_travel;
        let mut radius = edge.amount_for_travel(edge.travel_for_amount(start_amount) + delta);
        if shift {
            radius = radius.round().min(edge.max_amount().floor());
        }
        let path = self
            .node_item_path(&before)
            .map(std::path::Path::to_path_buf);
        if let Some(n) = self.doc_mut().scene.node_mut(id) {
            Self::apply_corner_amount(n, &before, vertex, radius, path.as_deref());
        }
        for peer in &peers {
            let Some(max) = self.node_corner_grip_edge(peer).map(|e| e.max_amount()) else {
                continue;
            };
            let path = self.node_item_path(peer).map(std::path::Path::to_path_buf);
            if let Some(n) = self.doc_mut().scene.node_mut(peer.id) {
                Self::apply_corner_amount(n, peer, None, radius.min(max), path.as_deref());
            }
        }
        if let Some(BoardDrag::FilletRadius {
            max_px,
            pointer_travel,
            ..
        }) = &mut self.board_drag
        {
            *max_px = max_px.max(moved);
            *pointer_travel = projected.clamp(0.0, edge.max_travel);
        }
    }

    /// Typed corner amount from the grip's inline field: one journaled patch,
    /// clamped to what the host can show. With a `vertex`, only that corner
    /// of the first id.
    pub(crate) fn shape_fillet_command(&mut self, detail: Option<&str>) -> bool {
        let Some(req) = detail.and_then(|s| serde_json::from_str::<FilletRequest>(s).ok()) else {
            return false;
        };
        if !req.radius.is_finite() || req.radius < 0.0 || self.refuse_read_only_edit() {
            return false;
        }
        let ids = match req.vertex {
            Some(_) => &req.ids[..req.ids.len().min(1)],
            None => &req.ids[..],
        };
        let mut cmds = Vec::new();
        for &id in ids {
            let Some(before) = self.doc().scene.node(id).cloned() else {
                continue;
            };
            if before.locked || !slate_doc::scene::supports_corners(&before) {
                continue;
            }
            let Some(edge) = self.node_grip_edge(&before, req.vertex) else {
                continue;
            };
            let radius = req.radius.min(edge.max_amount());
            let mut after = before.clone();
            let path = self.node_item_path(&before);
            Self::apply_corner_amount(&mut after, &before, req.vertex, radius, path);
            if after != before {
                cmds.push(slate_doc::scene::SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                });
            }
        }
        !cmds.is_empty() && self.commit_scene(cmds)
    }

    fn node_offers_bbox_transform(&self, node: &Node) -> bool {
        if node.hidden || matches!(node.kind, NodeKind::Connector(_)) {
            return false;
        }
        if Self::node_uses_curve_grips(node) {
            return false;
        }
        if node.locked && !self.board_sel.contains(&node.id) {
            return false;
        }
        // Unfilled paths are strokes. Their AABB is not transform chrome —
        // neither on hover nor once selected. Zoom-scaled edge bands would
        // otherwise fire well outside the box (P1.curve.pick).
        if let NodeKind::Shape(s) = &node.kind {
            if super::board_path::shape_uses_stroke_pick(node, s) {
                return false;
            }
        }
        true
    }

    fn selection_allows_rotation(&self) -> bool {
        self.board_sel.iter().any(|id| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(Self::node_allows_rotation)
        })
    }

    /// Topmost bounding-box chrome under `screen`. `None` node id = the
    /// multi-selection group box.
    pub(crate) fn transform_hit_at(
        &self,
        screen: Pos2,
    ) -> Option<(Option<NodeId>, board_handles::BoardHitTarget)> {
        let xf = self.board_xf();
        if self.agent_output_at(screen, &xf).is_some()
            || self.agent_artifact_at(screen, &xf).is_some()
        {
            return None;
        }
        if self.board_sel.len() >= 2 && !self.selection_all_simple_lines() {
            if let Some(gb) = self.board_group_bounds() {
                let geom = board_handles::selection_geom(&xf, gb, 0.0);
                if let Some(hit) =
                    board_handles::hit_test_chrome(screen, &geom, self.selection_allows_rotation())
                {
                    return Some((None, hit));
                }
            }
        }
        for n in self.doc().scene.nodes.iter().rev() {
            if !self.node_offers_bbox_transform(n) {
                continue;
            }
            let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
            if let Some(hit) =
                board_handles::hit_test_chrome(screen, &geom, Self::node_allows_rotation(n))
            {
                return Some((Some(n.id), hit));
            }
        }
        None
    }

    /// Hover cursors only. Selection is not required. Edge / rotate hover
    /// never paints selection chrome (P1.node.transform).
    pub(crate) fn hover_transform_chrome(
        &mut self,
        pointer: Option<Pos2>,
        xf: &BoardXf,
        ctx: &egui::Context,
        wire_grip_hovered: bool,
    ) {
        self.board_hover_hit = None;
        self.board_hover_node = None;
        let Some(p) = pointer else { return };
        if self.pointer_on_portal_maximize(p, xf) {
            return;
        }
        if let Some((id, vertex)) = self.fillet_grip_hit_at(p) {
            self.board_hover_hit = Some(board_handles::BoardHitTarget::FilletRadius);
            self.board_hover_node = Some(id);
            self.board_hover_grip_vertex = vertex;
            if let Some(edge) = self
                .doc()
                .scene
                .node(id)
                .and_then(|n| self.node_grip_edge(n, vertex))
            {
                ctx.set_cursor_icon(board_handles::cursor_along(Vec2::from(edge.dir)));
            }
            return;
        }
        let Some((node, hit)) = self.transform_hit_at(p) else {
            return;
        };
        self.board_hover_hit = Some(hit);
        self.board_hover_node = node;
        if wire_grip_hovered {
            return;
        }
        let geom = match node {
            Some(id) => self
                .doc()
                .scene
                .node(id)
                .map(|n| board_handles::selection_geom(xf, n.rect, n.rotation_deg)),
            None => self
                .board_group_bounds()
                .map(|gb| board_handles::selection_geom(xf, gb, 0.0)),
        };
        let Some(geom) = geom else { return };
        match hit {
            board_handles::BoardHitTarget::Resize(h) => {
                ctx.set_cursor_icon(board_handles::cursor_for_resize(h, &geom));
            }
            board_handles::BoardHitTarget::Rotate(_) => {
                ctx.set_cursor_icon(board_handles::cursor_for_rotate());
            }
            board_handles::BoardHitTarget::Body => {}
            board_handles::BoardHitTarget::FilletRadius => {}
        }
    }

    /// Body under the pointer, excluding edge / rotate chrome and the
    /// current selection. Edge hover is cursor-only.
    pub(crate) fn hover_preview_target(&self, world: Option<Pos2>) -> Option<NodeId> {
        if matches!(
            self.board_hover_hit,
            Some(board_handles::BoardHitTarget::Resize(_))
                | Some(board_handles::BoardHitTarget::Rotate(_))
                | Some(board_handles::BoardHitTarget::FilletRadius)
        ) {
            return None;
        }
        let w = world?;
        let id = super::board_path::board_pick_node(&self.doc().scene, w.x, w.y, self.tab().cam.z)?;
        if self.board_sel.contains(&id) {
            return None;
        }
        let n = self.doc().scene.node(id)?;
        if !self.settings.hover_highlight(n.kind.kind_name()) {
            return None;
        }
        // Stroke-pick paths already passed `board_pick_node`. Area objects
        // still require bbox-transform eligibility so locked / hidden /
        // connector chrome stays out.
        let stroke = match &n.kind {
            NodeKind::Shape(s) => super::board_path::shape_uses_stroke_pick(n, s),
            _ => false,
        };
        if !stroke && !self.node_offers_bbox_transform(n) {
            return None;
        }
        Some(id)
    }

    pub(crate) fn tick_hover_preview(
        &mut self,
        target: Option<NodeId>,
        dt: f32,
        ctx: &egui::Context,
    ) {
        let tokens = atlas_shell::tokens::current().board_preview.clone();
        if let Some(id) = target {
            self.board_hover_glow.entry(id).or_insert(0.0);
        }
        let mut animating = false;
        self.board_hover_glow.retain(|id, progress| {
            let goal = if target == Some(*id) { 1.0 } else { 0.0 };
            let span = if goal > *progress {
                tokens.highlight_in
            } else {
                tokens.highlight_out
            };
            let next = if span < 0.001 {
                goal
            } else if goal > *progress {
                (*progress + dt / span).min(1.0)
            } else {
                (*progress - dt / span).max(0.0)
            };
            if (next - *progress).abs() > 1e-4 && next != goal {
                animating = true;
            }
            *progress = next;
            next > 0.001
        });
        if animating {
            ctx.request_repaint();
        }
    }

    pub(crate) fn paint_hover_preview(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        color: egui::Color32,
    ) {
        let tokens = atlas_shell::tokens::current().board_preview.clone();
        for (id, progress) in &self.board_hover_glow {
            if *progress <= 0.001 {
                continue;
            }
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            if slate_doc::agent_chat::agent(n).is_some() {
                continue;
            }
            if !self.settings.hover_highlight(n.kind.kind_name()) {
                continue;
            }
            let eased = ease_in_out_cubic(*progress);
            let stroke = egui::Stroke::new(
                atlas_shell::canvas_scale::px(tokens.hover_line_weight, xf.z),
                color.gamma_multiply(tokens.hover_opacity * eased),
            );
            if let NodeKind::Shape(s) = &n.kind {
                if super::board_path::shape_uses_stroke_pick(n, s) {
                    if let Some(path) = s.path.as_ref() {
                        super::board_path::paint_path_stroke_outline(
                            painter, xf, n, s, path, stroke,
                        );
                        continue;
                    }
                }
            }
            let outline = self.node_screen_outline(painter.ctx(), xf, n);
            painter.add(egui::Shape::closed_line(outline, stroke));
        }
    }

    /// Press on bounding-box chrome — selected, group, or an unselected node.
    /// A wire-grip press is not a resize, even though it sits on the edge.
    pub(crate) fn begin_transform_drag(&mut self, screen: Pos2, world: Pos2) -> Option<BoardDrag> {
        let xf = self.board_xf();
        if self.pointer_on_portal_maximize(screen, &xf) {
            return None;
        }
        if self.wire_grip_at(screen, &xf).is_some() {
            return None;
        }
        let (node, hit) = self.transform_hit_at(screen)?;
        match (node, hit) {
            (None, board_handles::BoardHitTarget::Resize(h)) => {
                let gb = self.board_group_bounds()?;
                let before: Vec<Node> = self
                    .board_sel
                    .iter()
                    .filter_map(|i| self.doc().scene.node(*i).cloned())
                    .collect();
                if self.alt_scale_copies() {
                    let (ids, before) = self.stage_unjournaled_duplicates(&before);
                    let gb = self.board_group_bounds().unwrap_or(gb);
                    return Some(BoardDrag::GroupResize {
                        ids,
                        before,
                        group_before: gb,
                        handle: h as u8,
                        dup: true,
                    });
                }
                let ids: Vec<NodeId> = before.iter().map(|n| n.id).collect();
                Some(BoardDrag::GroupResize {
                    ids,
                    before,
                    group_before: gb,
                    handle: h as u8,
                    dup: false,
                })
            }
            (None, board_handles::BoardHitTarget::Rotate(_)) => {
                let gb = self.board_group_bounds()?;
                let before: Vec<Node> = self
                    .board_sel
                    .iter()
                    .filter_map(|i| self.doc().scene.node(*i).cloned())
                    .collect();
                let ids: Vec<NodeId> = before.iter().map(|n| n.id).collect();
                let (cx, cy) = gb.center();
                let start_angle = (world.y - cy).atan2(world.x - cx);
                Some(BoardDrag::GroupRotate {
                    ids,
                    before,
                    center: (cx, cy),
                    start_angle,
                })
            }
            (Some(id), board_handles::BoardHitTarget::Resize(h)) => {
                // Shift/Ctrl on an unselected node is add-to-selection, not
                // a hover-resize steal (P1.node.select). Shift on a selected
                // node still means free-aspect resize.
                if (self.shift_down || self.ctrl_down) && !self.board_sel.contains(&id) {
                    return None;
                }
                let n = self.doc().scene.node(id).cloned()?;
                if !self.board_sel.contains(&id) {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                if self.alt_scale_copies() {
                    let (ids, before) = self.stage_unjournaled_duplicates(std::slice::from_ref(&n));
                    let (id, before) = (ids.into_iter().next()?, before.into_iter().next()?);
                    return Some(BoardDrag::Resize {
                        id,
                        before,
                        handle: h as u8,
                        dup: true,
                    });
                }
                Some(BoardDrag::Resize {
                    id,
                    before: n,
                    handle: h as u8,
                    dup: false,
                })
            }
            (Some(id), board_handles::BoardHitTarget::Rotate(_)) => {
                let n = self.doc().scene.node(id).cloned()?;
                if !Self::node_allows_rotation(&n) {
                    return None;
                }
                if (self.shift_down || self.ctrl_down) && !self.board_sel.contains(&id) {
                    return None;
                }
                if !self.board_sel.contains(&id) {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                let (cx, cy) = n.rect.center();
                let start_angle = (world.y - cy).atan2(world.x - cx);
                Some(BoardDrag::Rotate {
                    id,
                    before: n,
                    start_angle,
                })
            }
            (_, board_handles::BoardHitTarget::Body) => None,
            (_, board_handles::BoardHitTarget::FilletRadius) => None,
        }
    }
}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Numeric sizing uses the same rectangle mapping as group/handle scaling.
/// `bounds` may be tighter than a path's normalization rectangle. Preserve its
/// world-space center when rotation makes the two centers differ.
pub(crate) fn resize_measured_node(
    node: &mut Node,
    bounds: slate_doc::scene::WorldRect,
    sx: f32,
    sy: f32,
) {
    let old_center = node.rect.center();
    let target = super::board_snap::orbit_point(old_center, bounds.center(), node.rotation_deg);
    let rect = super::board_snap::remap_group_scale(node.rect, sx, sy, bounds.center());
    let actual = super::board_snap::orbit_point(rect.center(), bounds.center(), node.rotation_deg);
    node.rect = rect.translated(target.0 - actual.0, target.1 - actual.1);
}
