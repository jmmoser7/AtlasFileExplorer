//! Coding-card wire ports and drag targets.
use super::*;

impl SlateApp {
    // ----- ports -----

    /// A coding card's crosstalk port under `screen`.
    pub(crate) fn crosstalk_port_at(
        &self,
        node: &Node,
        screen: Pos2,
        xf: &BoardXf,
    ) -> Option<Side> {
        if node.hidden
            || !xt::is_coding_card(node)
            || self.agent_in_choose_phase(node.id)
            || self.portal_capture_owns_viewport(node.id)
        {
            return None;
        }
        let reach = canvas_scale::px(PORT_REACH, xf.z);
        [Side::Top, Side::Bottom].into_iter().find(|side| {
            let p = slate_doc::connector_anchor_on(node, *side, 0.5);
            self.port_reaches(screen, xf, *side, p, reach)
        })
    }
    /// The topmost crosstalk port under `screen`.
    pub(crate) fn crosstalk_port_under(
        &self,
        screen: Pos2,
        xf: &BoardXf,
    ) -> Option<(NodeId, Side)> {
        let world = xf.s2w(screen);
        for node in self.doc().scene.nodes.iter().rev().filter(|n| !n.hidden) {
            if self.portal_capture_owns_viewport(node.id) {
                if node
                    .rect
                    .contains_rotated(world.x, world.y, node.rotation_deg)
                {
                    return None;
                }
                continue;
            }
            if let Some(side) = self.crosstalk_port_at(node, screen, xf) {
                return Some((node.id, side));
            }
        }
        None
    }

    /// A wire drag that began on a crosstalk port.
    pub(crate) fn is_crosstalk_port(&self, from: (NodeId, Side, f32)) -> bool {
        matches!(from.1, Side::Top | Side::Bottom)
            && (from.2 - 0.5).abs() < 1e-3
            && self
                .doc()
                .scene
                .node(from.0)
                .is_some_and(xt::is_coding_card)
    }

    /// Where a crosstalk drag lands: a partner's port within the wire snap,
    /// else the port facing the source when released on its body.
    pub(crate) fn crosstalk_snap(
        &self,
        world: Pos2,
        from: NodeId,
        snap: f32,
    ) -> Option<(NodeId, Side, f32)> {
        if self.portal_capture_owns_viewport(from) {
            return None;
        }
        let scene = &self.doc().scene;
        let source = scene.node(from)?;
        for node in scene
            .nodes
            .iter()
            .rev()
            .filter(|n| n.id != from && !n.hidden)
        {
            if self.portal_capture_owns_viewport(node.id) {
                if node
                    .rect
                    .contains_rotated(world.x, world.y, node.rotation_deg)
                {
                    return None;
                }
                continue;
            }
            if !xt::is_coding_card(node) || self.agent_in_choose_phase(node.id) {
                continue;
            }
            let port = [Side::Top, Side::Bottom].into_iter().find(|side| {
                let p = slate_doc::connector_anchor_on(node, *side, 0.5);
                Pos2::new(p[0], p[1]).distance(world) <= snap
            });
            let body = node.rect.contains(world.x, world.y);
            match (port, body) {
                (Some(side), _) => return Some((node.id, side, 0.5)),
                (None, true) => return Some((node.id, xt::facing(source.rect, node.rect).1, 0.5)),
                _ => {}
            }
        }
        None
    }
}
