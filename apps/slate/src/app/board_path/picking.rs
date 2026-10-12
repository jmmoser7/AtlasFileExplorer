//! Scene pick routing: precise shape geometry before frame fallbacks.

use super::*;

pub fn board_pick_node(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
) -> Option<NodeId> {
    board_pick_node_ex(scene, wx, wy, zoom, false)
}

/// Point pick honoring the scene flags: hidden nodes are never hit; locked
/// nodes only when `include_locked` (the Ctrl+Shift+click escape hatch and
/// eyedropper sampling). Connectors hit on their stroke (8 px pick width),
/// never on their AABB.
pub fn board_pick_node_ex(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
    include_locked: bool,
) -> Option<NodeId> {
    board_pick_node_routed(
        scene,
        wx,
        wy,
        zoom,
        include_locked,
        slate_doc::WireRouting::Bezier,
    )
}

/// Point pick with the session wire routing. A node under the pointer, and
/// the pick slop just outside that node, beats a wire.
pub fn board_pick_node_routed(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
    include_locked: bool,
    routing: slate_doc::WireRouting,
) -> Option<NodeId> {
    let eligible = |n: &Node| !n.hidden && (!n.locked || include_locked) && !n.is_frame();
    for n in scene.nodes.iter().rev().filter(|n| eligible(n)) {
        if matches!(n.kind, NodeKind::Connector(_)) {
            continue;
        }
        if let NodeKind::Shape(s) = &n.kind {
            if shape_uses_stroke_pick(n, s) {
                if hit_shape_stroke(n, s, wx, wy, zoom) {
                    return Some(n.id);
                }
                continue;
            }
            if s.shape != ShapeKind::Line {
                if hit_path_node(n, s, wx, wy, zoom) {
                    return Some(n.id);
                }
                continue;
            }
        }
        if n.rect.contains_rotated(wx, wy, n.rotation_deg) {
            return Some(n.id);
        }
    }
    for n in scene.nodes.iter().rev().filter(|n| eligible(n)) {
        let NodeKind::Connector(c) = &n.kind else {
            continue;
        };
        if super::super::board_wire::hit_connector_routed(scene, n.id, c, wx, wy, zoom, routing) {
            return Some(n.id);
        }
    }
    scene
        .nodes
        .iter()
        .rev()
        .find(|n| {
            n.is_frame() && !n.hidden && (include_locked || !n.locked) && n.rect.contains(wx, wy)
        })
        .map(|n| n.id)
}

/// The ink disc of a single-click brush stroke: world center and radius.
/// `None` for anything that is not a lone stamp dab.
pub(super) fn brush_dab_disc(node: &Node, shape: &ShapeNode) -> Option<(Pos2, f32)> {
    let path = shape.path.as_ref()?;
    if !path.is_empty() || !stroke_picks(shape) || !shape.stroke.paints_as_stamp() {
        return None;
    }
    let (cx, cy) = node.rect.center();
    Some((Pos2::new(cx, cy), shape.stroke.width.max(0.0) * 0.5))
}
