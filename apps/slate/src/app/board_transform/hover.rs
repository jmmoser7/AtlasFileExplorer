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
        let glyph = self.polygon_sides_glyph_at(screen, &xf).map(|g| g.id);
        let group_corner = (self.board_sel.len() >= 2
            && !self.selection_all_simple_lines()
            && body.is_none_or(|id| self.board_sel.contains(&id)))
        .then(|| self.board_group_bounds())
        .flatten()
        .and_then(|bounds| {
            let geom = board_handles::selection_geom(&xf, bounds, 0.0);
            let (hit, _) =
                board_handles::chrome_pick(screen, &geom, self.selection_allows_rotation())?;
            Self::interaction_corner(&geom, hit).map(|p| p.distance(screen))
        });
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .scan(false, |covered, node| {
                if *covered {
                    return None;
                }
                let geom = self.node_interaction_geom(node, &xf);
                *covered = body == Some(node.id)
                    || (self.node_offers_bbox_transform(node)
                        && board_handles::chrome_pick(
                            screen,
                            &geom,
                            Self::node_allows_rotation(node),
                        )
                        .is_some());
                Some(node)
            })
            .filter(|n| self.board_sel.contains(&n.id))
            .find_map(|n| {
                if glyph == Some(n.id) {
                    return None;
                }
                let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                let (vertex, grip) = self
                    .corner_grips(n, &xf)
                    .into_iter()
                    .filter(|(_, grip)| board_handles::hit_test_fillet_grip(screen, &geom, *grip))
                    .min_by(|a, b| a.1.distance(screen).total_cmp(&b.1.distance(screen)))?;
                let reach = grip.distance(screen);
                if group_corner.is_some_and(|d| d <= reach)
                    || self
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
        if self.agent_grip_owns(screen, &xf)
            || self.polygon_sides_glyph_at(screen, &xf).is_some()
            || self.fillet_grip_hit_at(screen).is_some()
        {
            return None;
        }
        if let Some((id, hit)) = self.chat_south_resize(screen, &xf) {
            return Some((Some(id), hit));
        }
        let mut group_interior = false;
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
                group_interior = gb.contains(world.x, world.y);
            }
        }
        let mut best: Option<(&Node, board_handles::BoardHitTarget, Option<Pos2>)> = None;
        for n in self.doc().scene.nodes.iter().rev() {
            // Other selected members' compact cages cannot steal this body.
            // The actual host keeps its own transform and body arbitration.
            if group_interior && self.board_sel.contains(&n.id) && body != Some(n.id) {
                continue;
            }
            if !self.node_offers_bbox_transform(n) {
                if body == Some(n.id) {
                    break;
                }
                continue;
            }
            let geom = self.node_interaction_geom(n, &xf);
            if let Some(vertex) = self.polygon_rotate_at(n, screen, &xf) {
                if self.node_controls_buried(n, &xf) {
                    continue;
                }
                if best.is_none() {
                    best = Some((n, board_handles::BoardHitTarget::Rotate(vertex), None));
                }
                continue;
            }
            let polygon =
                matches!(&n.kind, NodeKind::Shape(s) if s.shape == ShapeKind::RegularPolygon);
            if self
                .interaction_move_header(n, &geom, &xf)
                .is_some_and(|r| r.contains(screen))
            {
                if best.is_none() {
                    best = Some((n, board_handles::BoardHitTarget::Body, None));
                }
                continue;
            }
            let chrome = board_handles::chrome_pick(
                screen,
                &geom,
                !polygon && Self::node_allows_rotation(n),
            )
            .map(|(hit, _)| hit);
            if let Some(hit) = chrome {
                if self.node_controls_buried(n, &xf) {
                    continue;
                }
                if polygon
                    && matches!(&n.kind, NodeKind::Shape(s)
                    if !crate::app::board_path::hit_path_node(n, s, world.x, world.y, xf.z))
                {
                    continue;
                }
                let corner = Self::interaction_corner(&geom, hit);
                if let Some((previous, _, Some(anchor))) = best {
                    if corner.is_some_and(|p| p.distance(anchor) <= 0.01)
                        && !self.doc().outline_queries().node_outlines_overlap(
                            n.id,
                            previous.id,
                            0.25 / xf.z.max(0.001),
                        )
                    {
                        let distance = |node: &Node| {
                            self.doc().outline_queries().node_outline_distance(
                                node.id,
                                [world.x, world.y],
                                0.25 / xf.z.max(0.001),
                            )
                        };
                        if distance(n) + 1e-5 < distance(previous) {
                            best = Some((n, hit, corner));
                        }
                    }
                } else if best.is_none() {
                    best = Some((n, hit, corner));
                }
            } else if best.is_none()
                && (body == Some(n.id)
                    || (geom.projected_shortest
                        < atlas_shell::selection_tools::hit_regions::MOVE_SIZE
                        && board_handles::body_pick(screen, &geom).is_some()
                        && self.doc().outline_queries().node_outline_distance(
                            n.id,
                            [world.x, world.y],
                            0.25 / xf.z.max(0.001),
                        ) <= (atlas_shell::selection_tools::hit_regions::MOVE_SIZE
                            - geom.projected_shortest)
                            * 0.5
                            / xf.z.max(0.001)))
                && !self.node_controls_buried(n, &xf)
            {
                best = Some((n, board_handles::BoardHitTarget::Body, None));
            }
        }
        best.map(|(node, hit, _)| (Some(node.id), hit))
            .or_else(|| group_interior.then_some((None, board_handles::BoardHitTarget::Body)))
    }

    fn interaction_corner(
        geom: &board_handles::SelectionGeom,
        hit: board_handles::BoardHitTarget,
    ) -> Option<Pos2> {
        use board_handles::{BoardHitTarget::Resize, ResizeHandle::*};
        let i = match hit {
            Resize(Nw) => 0,
            Resize(Ne) => 1,
            Resize(Se) => 2,
            Resize(Sw) => 3,
            _ => return None,
        };
        Some(geom.control_corners[i])
    }

    pub(crate) fn node_controls_buried(&self, node: &Node, xf: &BoardXf) -> bool {
        !self.board_sel.contains(&node.id)
            && !self
                .doc()
                .outline_queries()
                .outline_covering_nodes(node.id, 0.25 / xf.z.max(0.001))
                .is_empty()
    }

    fn interaction_covering_nodes(
        &self,
        node: &Node,
        xf: &BoardXf,
    ) -> Option<std::sync::Arc<[NodeId]>> {
        if self.board_sel.len() != 1 || !self.board_sel.contains(&node.id) || node.is_frame() {
            return None;
        }
        let covering = self
            .doc()
            .outline_queries()
            .outline_covering_nodes(node.id, 0.25 / xf.z.max(0.001));
        (!covering.is_empty()).then_some(covering)
    }

    /// One geometry for hover, press and painted control frames. It never
    /// changes the authored corners used by resize, snap or undo arithmetic.
    pub(crate) fn node_interaction_geom(
        &self,
        node: &Node,
        xf: &BoardXf,
    ) -> board_handles::SelectionGeom {
        let mut geom = board_handles::selection_geom(xf, node.rect, node.rotation_deg);
        if let Some(covers) = self.interaction_covering_nodes(node, xf) {
            let origin = geom.corners[0];
            let x = (geom.corners[1] - origin).normalized();
            let y = (geom.corners[3] - origin).normalized();
            let mut min = Vec2::new(f32::INFINITY, f32::INFINITY);
            let mut max = -min;
            let covering_points = covers
                .iter()
                .filter_map(|id| self.doc().scene.node(*id))
                .flat_map(|cover| {
                    cover
                        .rect
                        .corners_rotated(cover.rotation_deg)
                        .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
                });
            for p in geom.control_corners.into_iter().chain(covering_points) {
                let delta = p - origin;
                let local = Vec2::new(delta.dot(x), delta.dot(y));
                min = min.min(local);
                max = max.max(local);
            }
            let gap = Vec2::splat(25.0);
            min -= gap;
            max += gap;
            let point = |a: f32, b: f32| origin + x * a + y * b;
            board_handles::set_control_frame(
                &mut geom,
                [
                    point(min.x, min.y),
                    point(max.x, min.y),
                    point(max.x, max.y),
                    point(min.x, max.y),
                ],
            );
        }
        geom
    }

    pub(crate) fn interaction_move_header(
        &self,
        node: &Node,
        geom: &board_handles::SelectionGeom,
        xf: &BoardXf,
    ) -> Option<egui::Rect> {
        self.interaction_covering_nodes(node, xf)?;
        let index = self
            .doc()
            .scene
            .nodes
            .iter()
            .position(|n| n.id == node.id)?;
        let bounds = egui::Rect::from_points(&geom.control_corners);
        let size = Vec2::new((bounds.width() - 36.0).clamp(70.0, 130.0), 20.0);
        for gap in [3.0, 23.0, 43.0] {
            for center in [
                Pos2::new(bounds.center().x, bounds.top() - size.y * 0.5 - gap),
                Pos2::new(bounds.center().x, bounds.bottom() + size.y * 0.5 + gap),
                Pos2::new(bounds.left() - size.x * 0.5 - gap, bounds.center().y),
                Pos2::new(bounds.right() + size.x * 0.5 + gap, bounds.center().y),
            ] {
                let rect = egui::Rect::from_center_size(center, size);
                if !self.canvas_rect.contains_rect(rect) {
                    continue;
                }
                let blocked = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.id != node.id && !n.hidden && !n.is_frame())
                    .any(|(other_index, other)| {
                        let other_geom =
                            board_handles::selection_geom(xf, other.rect, other.rotation_deg);
                        // Conservative full rectangles avoid missing narrow obstructions
                        // between sparse probe points. No partial-clear header is offered.
                        rect.intersects(xf.rect_w2s(other.rect.rotated_bounds(other.rotation_deg)))
                            || (other_index > index
                                && self.node_offers_bbox_transform(other)
                                && rect.intersects(board_handles::chrome_bounds(
                                    &other_geom,
                                    Self::node_allows_rotation(other),
                                )))
                    });
                if !blocked {
                    return Some(rect);
                }
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
        let tiny = n.rect.w.min(n.rect.h) * xf.z
            < atlas_shell::selection_tools::hit_regions::COMPACT_FRAME;
        (tiny && matches!(self.transform_hit_at(screen),Some((Some(owner),board_handles::BoardHitTarget::Body)) if owner==id)).then_some(id)
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
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_transform_entry");
        self.board_hover_hit = None;
        self.board_hover_node = None;
        self.board_sides_hover = None;
        let Some(p) = pointer else { return };
        let maximize = self.pointer_on_portal_maximize(p, xf);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_portal_maximize");
        if maximize {
            return;
        }
        let fillet = self.fillet_grip_hit_at(p);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_fillet_grip");
        if let Some((id, vertex)) = fillet {
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
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_polygon_sides");
        let glyph = self.polygon_sides_glyph_at(p, xf).is_some();
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_polygon_glyph");
        if glyph {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            return;
        }
        let curve = self.curve_grip_under(p);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_curve_grip");
        if curve {
            return;
        }
        let transform = self.transform_hit_at(p);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_transform_hit");
        let Some((node, hit)) = transform else {
            return;
        };
        if hit == board_handles::BoardHitTarget::Body {
            self.board_hover_hit = Some(hit);
            self.board_hover_node = node;
            ctx.set_cursor_icon(egui::CursorIcon::Move);
            return;
        }
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
                .map(|n| self.node_interaction_geom(n, xf)),
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
