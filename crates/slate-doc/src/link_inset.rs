//! Top and bottom link dots sit the same distance in from the edge as the
//! side dots (`agent_chat::PORT_INSET`).

use crate::agent_chat::PORT_INSET;
use crate::crosstalk::is_coding_card;
use crate::scene::{Node, Side};
use crate::wire_host::WireHost;

/// Anchor of a connector end. Coding-card top and bottom ports move inward
/// by [`PORT_INSET`]; every other anchor stays on the edge.
pub fn anchor(node: &Node, side: Side, t: f32) -> [f32; 2] {
    let host = WireHost::from_node(node);
    let point = host.anchor(side, t);
    if !is_coding_card(node) || (t - 0.5).abs() > 0.001 || !matches!(side, Side::Top | Side::Bottom)
    {
        return point;
    }
    let outward = host.outward(side, t);
    [
        point[0] - outward[0] * PORT_INSET,
        point[1] - outward[1] * PORT_INSET,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_chat::PORT_INSET;
    use crate::scene::{NodeKind, PortalNode, Scene, WorldRect};

    fn coding_card() -> Node {
        let mut portal = PortalNode::unbound_agent("Chat", "cursor");
        portal.agent.as_mut().unwrap().chat.linear = true;
        Scene::default().build_node(
            WorldRect::new(10.0, 20.0, 320.0, 180.0),
            NodeKind::Portal(portal),
        )
    }

    #[test]
    fn top_and_bottom_match_the_side_inset() {
        let node = coding_card();
        let rect = node.rect;
        let top = anchor(&node, Side::Top, 0.5);
        let bottom = anchor(&node, Side::Bottom, 0.5);
        let side = PORT_INSET;
        assert!(
            (top[1] - (rect.y + side)).abs() < 0.01,
            "top inset {} != side inset {side}",
            top[1] - rect.y
        );
        assert!((rect.y + rect.h - bottom[1] - side).abs() < 0.01);
        assert!((top[0] - (rect.x + rect.w * 0.5)).abs() < 0.01);
        assert!((bottom[0] - top[0]).abs() < 0.01);
    }
}
