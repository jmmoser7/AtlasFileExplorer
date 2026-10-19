//! Transform hover ownership and cursor feedback.
use super::*;

impl SlateApp {
    /// The topmost selected host whose corner grip is under `screen`, with
    /// the polyline vertex that grip sets. Corner grips ride the segments
    /// and curve grips sit on the vertices, so where their reaches overlap
    /// the nearer one takes the press (a tie goes to the vertex).
    pub(crate) fn fillet_grip_hit_at(&self, screen: Pos2) -> Option<(NodeId, Option<usize>)> {
        let xf = self.board_xf();
        let world = xf.s2w(screen);
        let body = self.board_pick_node(world.x, world.y);
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .scan(false, |covered, node| {
                if *covered {
                    return None;
                }
                *covered = body == Some(node.id);
                Some(node)
            })
            .filter(|n| self.board_sel.contains(&n.id))
            .find_map(|n| {
                let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                let (vertex, grip) = self
                    .corner_grips(n, &xf)
                    .into_iter()
                    .filter(|(_, grip)| board_handles::hit_test_fillet_grip(screen, &geom, *grip))
                    .min_by(|a, b| a.1.distance(screen).total_cmp(&b.1.distance(screen)))?;
                let reach = grip.distance(screen);
                if self
                    .curve_grip_distance(n.id, screen)
                    .is_some_and(|d| d <= reach)
                {
                    return None;
                }
                Some((n.id, vertex))
            })
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
        let world = xf.s2w(screen);
        let body = self.board_pick_node(world.x, world.y);
        if let Some(id) = self.tiny_body_owns_pointer(screen) {
            return Some((Some(id), board_handles::BoardHitTarget::Body));
        }
        if self.agent_grip_owns(screen, &xf)
            || self.polygon_sides_glyph_at(screen, &xf).is_some()
            || self.fillet_grip_hit_at(screen).is_some()
        {
            return None;
        }
        if let Some((id, hit)) = self.chat_south_resize(screen, &xf) {
            return Some((Some(id), hit));
        }
        if self.board_sel.len() >= 2
            && !self.selection_all_simple_lines()
            && body.is_none_or(|id| self.board_sel.contains(&id))
        {
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
                if body == Some(n.id) {
                    return None;
                }
                continue;
            }
            let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
            if let Some(vertex) = self.polygon_rotate_at(n, screen, &xf) {
                return Some((Some(n.id), board_handles::BoardHitTarget::Rotate(vertex)));
            }
            let polygon =
                matches!(&n.kind, NodeKind::Shape(s) if s.shape == ShapeKind::RegularPolygon);
            if let Some(hit) = board_handles::hit_test_chrome(
                screen,
                &geom,
                !polygon && Self::node_allows_rotation(n),
            ) {
                if polygon
                    && matches!(&n.kind, NodeKind::Shape(s)
                    if !crate::app::board_path::hit_path_node(n, s, world.x, world.y, xf.z))
                {
                    continue;
                }
                return Some((Some(n.id), hit));
            }
            if body == Some(n.id) {
                return Some((Some(n.id), board_handles::BoardHitTarget::Body));
            }
        }
        None
    }

    /// Keep a small object's visible body available for moving when its
    /// ordinary transform hit bands cover the whole object.
    pub(crate) fn tiny_body_owns_pointer(&self, screen: Pos2) -> Option<NodeId> {
        let xf = self.board_xf();
        let world = xf.s2w(screen);
        let id = self.board_pick_node(world.x, world.y)?;
        let n = self.doc().scene.node(id)?;
        // Curves keep their explicit endpoint/anchor edit contract.
        if n.is_frame()
            || Self::node_uses_curve_grips(n)
            || self.curve_grips_of(id).is_some()
            || !n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
        {
            return None;
        }
        let band = canvas_scale::hit_px(board_handles::CORNER_HIT_PX, xf.z);
        (n.rect.w.min(n.rect.h) * xf.z <= 2.0 * band).then_some(id)
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
        self.board_sides_hover = None;
        let Some(p) = pointer else { return };
        if self.pointer_on_portal_maximize(p, xf) {
            return;
        }
        if let Some(id) = self.tiny_body_owns_pointer(p) {
            self.board_hover_hit = Some(board_handles::BoardHitTarget::Body);
            self.board_hover_node = Some(id);
            ctx.set_cursor_icon(egui::CursorIcon::Move);
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
        self.board_sides_hover = self.polygon_sides_hover_at(p, xf);
        if self.polygon_sides_glyph_at(p, xf).is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            return;
        }
        if self.curve_grip_under(p) {
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
}
