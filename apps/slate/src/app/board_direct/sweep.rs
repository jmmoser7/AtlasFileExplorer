//! Segment sweep selection within the direct-selection owner.

use super::*;

impl SlateApp {
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
