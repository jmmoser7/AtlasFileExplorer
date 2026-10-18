//! Broad-phase bounds, including independently authored arrowheads.
use super::*;

/// Axis-aligned bounds of a node for broad-phase queries. Rotated nodes use
/// the AABB of their rotated corners; connectors already store their curve
/// AABB in `Node.rect`.
pub fn node_aabb(node: &Node) -> WorldRect {
    let reach = match &node.kind {
        NodeKind::Shape(s) => s.stroke.arrow_reach(),
        _ => 0.0,
    };
    if node.rotation_deg.abs() < f32::EPSILON {
        let r = node.rect.normalized();
        return WorldRect::new(
            r.x - reach,
            r.y - reach,
            r.w + reach * 2.0,
            r.h + reach * 2.0,
        );
    }
    let corners = node.rect.corners_rotated(node.rotation_deg);
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (x, y) in corners {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    WorldRect::new(
        min_x - reach,
        min_y - reach,
        (max_x - min_x).max(0.0) + reach * 2.0,
        (max_y - min_y).max(0.0) + reach * 2.0,
    )
}
