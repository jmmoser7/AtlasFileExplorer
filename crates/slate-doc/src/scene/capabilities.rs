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

// Shared scene style access, used by every property-edit surface.
pub fn stroke_of(node: &Node) -> Option<Stroke> {
    match &node.kind {
        NodeKind::Shape(s) => Some(s.stroke),
        NodeKind::Image(i) => Some(i.stroke),
        NodeKind::Connector(c) => Some(c.stroke),
        NodeKind::Frame(f) => Some(f.stroke),
        NodeKind::Text(t) => Some(t.stroke),
        // TWIN: docs/audit/deviations.md DV-21 — an agent portal keeps its
        // outline in `agent.chat.stroke`; every other portal uses
        // `PortalNode::stroke`. Collapse onto the field.
        NodeKind::Portal(p) if p.kind == PortalKind::Agent => Some(match p.agent.as_ref() {
            Some(a) => a.chat.stroke.unwrap_or(Stroke {
                width: 0.0,
                ..Stroke::default()
            }),
            None => p.stroke,
        }),
        NodeKind::Portal(p) => Some(p.stroke),
        _ => None,
    }
}

pub fn set_stroke(node: &mut Node, stroke: Stroke) {
    match &mut node.kind {
        NodeKind::Shape(s) => s.stroke = stroke,
        NodeKind::Image(i) => i.stroke = stroke,
        // A wire's color is authored only through `ConnectorNode::set_color`.
        NodeKind::Connector(c) => {
            let color = c.color;
            c.stroke = stroke;
            c.set_color(color);
        }
        NodeKind::Frame(f) => f.stroke = stroke,
        NodeKind::Text(t) => t.stroke = stroke,
        // TWIN: see `stroke_of` above (DV-21).
        NodeKind::Portal(p) if p.kind == PortalKind::Agent => match p.agent.as_mut() {
            Some(a) => a.chat.stroke = Some(stroke),
            None => p.stroke = stroke,
        },
        NodeKind::Portal(p) => p.stroke = stroke,
        _ => {}
    }
}

pub fn fill_of(node: &Node) -> Option<Rgba> {
    match &node.kind {
        NodeKind::Shape(s) => s.fill,
        NodeKind::Text(t) => t.fill,
        NodeKind::Frame(f) => Some(f.fill),
        NodeKind::Portal(p) => Some(p.fill),
        _ => None,
    }
}
pub fn set_fill(node: &mut Node, fill: Option<Rgba>) {
    match &mut node.kind {
        NodeKind::Shape(s) => s.fill = fill,
        NodeKind::Text(t) => t.fill = fill,
        NodeKind::Frame(f) => {
            if let Some(c) = fill {
                f.fill = c;
                f.fill_authored = true;
            }
        }
        NodeKind::Portal(p) => {
            if let Some(c) = fill {
                p.fill = c;
                if let Some(a) = p.agent.as_mut().filter(|_| p.kind == PortalKind::Agent) {
                    a.chat.custom_fill = true;
                }
            }
        }
        _ => {}
    }
}
