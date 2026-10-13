//! Canonical property capability predicates for every scene interpreter.
use super::*;

/// Stroke editor applies to shapes, images, wires, text boxes, slide frames,
/// and portal frames.
pub fn supports_stroke(node: &Node) -> bool {
    stroke_of(node).is_some()
}

/// Fillet/chamfer applies to rectangles, images, slide frames, and portals.
/// Other shapes store a corner field that is not a user-facing treatment.
pub fn supports_corners(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Shape(s) => {
            matches!(s.shape, ShapeKind::Rect | ShapeKind::RegularPolygon)
                || (s.shape == ShapeKind::Path
                    && s.path
                        .as_ref()
                        .is_some_and(|p| crate::geom::path_is_line_polyline(p)))
        }
        NodeKind::Image(_) => node.clip.is_none(),
        NodeKind::Frame(_) | NodeKind::Portal(_) => true,
        _ => false,
    }
}

/// Photo-filter / ImageAdjust editors apply to placed images, including 3D
/// model viewports (over the live render or frozen poster).
pub fn supports_image_adjust(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Image(_))
}

/// Fill editor authors closed-shape paint, text sticky-note background, and
/// frame/portal frame color. Text ink stays on `TextNode.color`.
pub fn supports_fill(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Shape(s) => {
            s.shape != ShapeKind::Line
                && (s.shape != ShapeKind::Path || s.path.as_ref().is_some_and(|p| p.closed))
        }
        NodeKind::Text(_) | NodeKind::Frame(_) | NodeKind::Portal(_) => true,
        _ => false,
    }
}
