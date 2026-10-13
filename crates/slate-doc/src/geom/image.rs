//! Authored image geometry and closed-shape clipping.

use super::*;
use crate::scene::{PathContour, PathFillRule};
use vector_ink::kurbo::{Circle, PathEl, Shape};

#[cfg(test)]
mod tests;

/// The world rect the full uncropped image occupies (crop window + UV crop).
pub fn image_content_rect(rect: WorldRect, crop: Crop) -> WorldRect {
    let c = crop.clamped();
    let w = rect.w / c.w.max(Crop::MIN_SPAN);
    let h = rect.h / c.h.max(Crop::MIN_SPAN);
    WorldRect::new(rect.x - c.x * w, rect.y - c.y * h, w, h)
}

/// Center a cover crop inside the source's existing authored crop. `aspect`
/// describes the full source pixels; paint layers stay in that content space.
pub fn cover_crop(frame: WorldRect, aspect: f32, source: Crop) -> Crop {
    let source = source.clamped();
    let aspect = aspect.max(1e-6);
    let wanted = frame.w.max(1e-6) / frame.h.max(1e-6);
    let shown = aspect * source.w / source.h;
    let (w, h) = if shown > wanted {
        (source.h * wanted / aspect, source.h)
    } else {
        (source.w, source.w * aspect / wanted)
    };
    Crop {
        x: source.x + (source.w - w) * 0.5,
        y: source.y + (source.h - h) * 0.5,
        w,
        h,
    }
    .clamped()
}

/// Freeze a closed shape's visible boundary in its existing normalized box.
/// Curves and compound paths stay curves; an authored trim intersects that
/// boundary through the same polygon boolean owner as Trim.
pub fn shape_image_clip(node: &Node) -> Option<PathData> {
    let NodeKind::Shape(original) = &node.kind else {
        return None;
    };
    let styled = crate::vertex_style::closed_form_paint_shape(original, node.rect);
    let shape = styled.as_ref().unwrap_or(original);
    let mask = match shape.shape {
        ShapeKind::Line => return None,
        ShapeKind::Ellipse => bez_mask(
            &Circle::new((0.5, 0.5), 0.5).to_path(1e-6),
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
        ),
        ShapeKind::Path => {
            let path = shape.path.as_deref().filter(|path| path.closed)?;
            if !path_is_line_polyline(path)
                || (shape.corner == Corner::Square
                    && path.corner_amounts.iter().all(Option::is_none))
            {
                path.clone()
            } else {
                bez_mask(
                    &path_data_to_world_bez_with_fillet(path, node.rect, 0.0, shape.corner),
                    node.rect,
                )
            }
        }
        ShapeKind::Rect | ShapeKind::RegularPolygon => {
            let points = if shape.shape == ShapeKind::Rect {
                vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
            } else {
                regular_polygon_vertices(
                    WorldRect::new(0.0, 0.0, 1.0, 1.0),
                    shape.sides,
                    shape.phase_deg,
                )
            };
            let path = crate::image_paint::region_path(vec![points])?;
            bez_mask(
                &path_data_to_world_bez_with_fillet(&path, node.rect, 0.0, shape.corner),
                node.rect,
            )
        }
    };
    let Some(trim) = &node.clip else {
        return Some(mask);
    };
    let unit = WorldRect::new(0.0, 0.0, 1.0, 1.0);
    let shape = flatten_contours(&path_data_to_world_bez(&mask, unit, 0.0), 1e-5);
    let trim = flatten_contours(&path_data_to_world_bez(trim, unit, 0.0), 1e-5);
    crate::image_paint::region_path(
        vector_ink::boolean_intersection(&shape, &trim)
            .into_iter()
            .flatten()
            .collect(),
    )
}

/// Keep all contour and curve commands while normalizing to the target box.
fn bez_mask(bez: &BezPath, rect: WorldRect) -> PathData {
    let norm = |p: Point| {
        [
            ((p.x as f32) - rect.x) / rect.w,
            ((p.y as f32) - rect.y) / rect.h,
        ]
    };
    let mut contours = Vec::new();
    let mut current: Option<PathContour> = None;
    for element in bez.elements() {
        match *element {
            PathEl::MoveTo(p) => {
                if let Some(contour) = current.take() {
                    contours.push(contour);
                }
                current = Some(PathContour {
                    start: norm(p),
                    segs: Vec::new(),
                    closed: false,
                });
            }
            PathEl::LineTo(p) => {
                if let Some(c) = &mut current {
                    c.segs.push(PathSeg::Line { to: norm(p) });
                }
            }
            PathEl::QuadTo(ctrl, to) => {
                if let Some(c) = &mut current {
                    c.segs.push(PathSeg::Quad {
                        ctrl: norm(ctrl),
                        to: norm(to),
                    });
                }
            }
            PathEl::CurveTo(c1, c2, to) => {
                if let Some(c) = &mut current {
                    c.segs.push(PathSeg::Cubic {
                        c1: norm(c1),
                        c2: norm(c2),
                        to: norm(to),
                    });
                }
            }
            PathEl::ClosePath => {
                if let Some(c) = &mut current {
                    c.closed = true;
                }
            }
        }
    }
    if let Some(contour) = current {
        contours.push(contour);
    }
    let first = contours.remove(0);
    PathData {
        start: first.start,
        segs: first.segs,
        closed: first.closed,
        extra: contours,
        fill_rule: PathFillRule::EvenOdd,
        ..Default::default()
    }
}
