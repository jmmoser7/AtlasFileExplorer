//! Segment sweep selection within the direct-selection owner.

use super::*;

impl SlateApp {
    /// The segment of curve `id` under `world`: a line, or a single-contour
    /// path whose grips are its anchors (an arc's grips are not, and a
    /// stamped brush stroke has none). Locked and hidden nodes have none.
    pub(super) fn curve_edge_at(&self, id: NodeId, world: Pos2) -> Option<usize> {
        let n = self.doc().scene.node(id)?;
        if n.locked || n.hidden {
            return None;
        }
        let line = board_line::line_endpoints(n).is_some();
        if !line
            && self.closed_form_points(id).is_none()
            && !matches!(self.curve_grips_of(id), Some(CurveGrips::Anchors { .. }))
        {
            return None;
        }
        let (anchors, closed) = self.direct_anchors_of(id)?;
        let bez = bezpath_from_anchors(&anchors, closed);
        let radius = (SEGMENT_HIT_PX / self.board_xf().z.max(0.05)) as f64;
        segment_hit(&bez, to_point(world), radius)
    }

    /// Ctrl+Shift+click with the Select tool on a curve segment (Rhino
    /// sub-object pick): that curve alone is selected, as the chord already
    /// selects a group member, and the edge with its two end vertices is
    /// picked, or taken off again. Returns whether a segment was hit.
    pub(crate) fn pick_curve_edge(&mut self, world: Pos2) -> bool {
        let Some(id) = board_path::board_pick_node_routed(
            &self.doc().scene,
            world.x,
            world.y,
            self.tab().cam.z,
            true,
            self.board_wire_routing,
        ) else {
            return false;
        };
        let Some(seg) = self.curve_edge_at(id, world) else {
            return false;
        };
        let Some((anchors, _)) = self.direct_anchors_of(id) else {
            return false;
        };
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.direct.grip_points.toggle_edge(id, seg, anchors.len());
        true
    }

    /// The explicit curve sub-object chord owns its real edge even when a
    /// dimension chip or property button overlaps that edge.
    pub(crate) fn curve_subselect_under(&self, screen: Pos2) -> bool {
        if !self.ctrl_down
            || !self.shift_down
            || self.board_tool != super::super::board::BoardTool::Select
        {
            return false;
        }
        let world = self.board_xf().s2w(screen);
        board_path::board_pick_node_routed(
            &self.doc().scene,
            world.x,
            world.y,
            self.board_xf().z,
            true,
            self.board_wire_routing,
        )
        .is_some_and(|id| self.curve_edge_at(id, world).is_some())
    }

    /// Rhino's Ctrl+Shift window/crossing picks segments on one curve. Keep
    /// existing picks on that curve; repeated sweeps add rather than toggle.
    pub(crate) fn pick_curve_edges_in_window(&mut self, start: Pos2, end: Pos2) {
        let xf = self.board_xf();
        let rect = Rect::from_two_pos(xf.s2w(start), xf.s2w(end));
        let r = WorldRect::new(rect.min.x, rect.min.y, rect.width(), rect.height());
        let mode = board_path::marquee_mode(start.x, end.x);
        let preferred = self.direct.grip_points.node;
        let mut candidates: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .rev()
            .filter(|n| !n.hidden && !n.locked)
            .map(|n| n.id)
            .collect();
        candidates.sort_by_key(|id| Some(*id) != preferred);
        for id in candidates {
            let Some((anchors, closed)) = self.direct_anchors_of(id) else {
                continue;
            };
            let Some(before) = self.doc().scene.node(id) else {
                continue;
            };
            let count = anchors.len().saturating_sub(usize::from(!closed));
            let hits: Vec<_> = (0..count)
                .filter(|&i| {
                    let mut segment = before.clone();
                    let bez = bezpath_from_anchors(
                        &[anchors[i], anchors[(i + 1) % anchors.len()]],
                        false,
                    );
                    rebuild_from_world_bez(&mut segment, &bez, false, None);
                    if let NodeKind::Shape(s) = &mut segment.kind {
                        s.fill = None;
                    }
                    board_path::marquee_selects_node(
                        &segment,
                        r,
                        xf.z,
                        &self.doc().scene,
                        self.board_wire_routing,
                        mode,
                    )
                })
                .collect();
            if hits.is_empty() {
                continue;
            }
            self.board_sel.clear();
            self.board_sel.insert(id);
            for seg in hits {
                if self.direct.grip_points.node != Some(id)
                    || !self.direct.grip_points.edges.contains_key(&seg)
                {
                    self.direct.grip_points.toggle_edge(id, seg, anchors.len());
                }
            }
            return;
        }
    }
}
