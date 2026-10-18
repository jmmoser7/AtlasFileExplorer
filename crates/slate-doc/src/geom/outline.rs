//! Visible closed-node outlines shared by pick, trim, and bumpers.
use super::*;

/// The world region of a closed node: a trim clip when it has one, else the
/// rect (with its corner treatment), ellipse, closed path, or the rotated
/// box of a text, image, or frame.
pub fn node_closed_poly(n: &Node, tolerance: f32) -> Option<Polygon> {
    if let Some(clip) = &n.clip {
        let bez = path_data_to_world_bez(clip, n.rect, n.rotation_deg);
        let contours = flatten_contours(&bez, tolerance as f64);
        return (!contours.is_empty()).then_some(contours);
    }
    match &n.kind {
        NodeKind::Shape(s) => {
            if let Some(styled) = crate::vertex_style::closed_form_paint_shape(s, n.rect) {
                let bez = path_data_to_world_bez_with_fillet(
                    styled.path.as_deref()?,
                    n.rect,
                    n.rotation_deg,
                    styled.corner,
                );
                return Some(flatten_contours(&bez, tolerance as f64));
            }
            match s.shape {
                ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon => {
                    let outline = match s.shape {
                        ShapeKind::Rect => s
                            .corner
                            .outline(n.rect, tolerance)
                            .into_iter()
                            .map(|p| n.rect.rotate_point(p, n.rotation_deg))
                            .collect(),
                        ShapeKind::Ellipse => n
                            .rect
                            .ellipse_outline(tolerance)
                            .into_iter()
                            .map(|p| n.rect.rotate_point(p, n.rotation_deg))
                            .collect(),
                        ShapeKind::RegularPolygon => regular_polygon_world_outline(
                            n.rect,
                            n.rotation_deg,
                            s.sides,
                            s.phase_deg,
                            s.corner,
                            tolerance,
                        ),
                        _ => unreachable!(),
                    };
                    Some(vec![outline])
                }
                ShapeKind::Path => {
                    let path = s.path.as_ref()?;
                    if !path.closed {
                        return None;
                    }
                    let bez = path_data_to_world_bez(path, n.rect, n.rotation_deg);
                    let contours = flatten_contours(&bez, tolerance as f64);
                    (!contours.is_empty()).then_some(contours)
                }
                ShapeKind::Line => None,
            }
        }
        NodeKind::Text(_) | NodeKind::Image(_) | NodeKind::Frame(_) => Some(vec![n
            .rect
            .corners_rotated(n.rotation_deg)
            .into_iter()
            .map(|(x, y)| [x, y])
            .collect()]),
        _ => None,
    }
}
