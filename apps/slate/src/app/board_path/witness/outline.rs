use atlas_agent::presence::{WorldPoint, WORLD_COORD_LIMIT};
use slate_doc::scene::{Node, NodeKind, ShapeKind};

const OUTLINE_POINT_LIMIT: usize = 4_096;

/// The shared closed-shape outline, made into a continuous route including
/// its closing edge. Parametric source geometry stays on the original node.
pub(super) fn closed_shape_route(node: &Node, tolerance: f64) -> Option<Vec<WorldPoint>> {
    if node.clip.is_some()
        || ![
            node.rect.x,
            node.rect.y,
            node.rect.w,
            node.rect.h,
            node.rotation_deg,
        ]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    if !matches!(
        shape.shape,
        ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon
    ) {
        return None;
    }

    // Bound shared outline tessellation before it allocates. The sagitta
    // sampler's count grows roughly with sqrt(radius / tolerance).
    let radius = node.rect.w.abs().max(node.rect.h.abs()) * 0.5;
    let safe_tolerance = tolerance
        .max(f64::from(radius) / OUTLINE_POINT_LIMIT as f64)
        .min(f64::from(f32::MAX)) as f32;
    let outline = slate_doc::geom::node_closed_poly(node, safe_tolerance)?;
    if outline.len() != 1 || outline[0].is_empty() || outline[0].len() > OUTLINE_POINT_LIMIT {
        return None;
    }
    let mut route = outline[0]
        .iter()
        .map(|point| {
            if !point[0].is_finite()
                || !point[1].is_finite()
                || f64::from(point[0]).abs() > WORLD_COORD_LIMIT
                || f64::from(point[1]).abs() > WORLD_COORD_LIMIT
            {
                return None;
            }
            WorldPoint::new(f64::from(point[0]), f64::from(point[1])).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    if route.first() != route.last() {
        route.push(*route.first()?);
    }
    (route.len() <= OUTLINE_POINT_LIMIT && route.len() >= 2).then_some(route)
}
