//! A node's authored geometry in world space: the one reading of path data,
//! rotation, and outlines that pick, trim, and bumpers share. The artifact
//! writer keeps its own reading (a second interpreter of the model).

use crate::scene::{Crop, Node, NodeKind, PathData, PathSeg, ShapeKind, WorldRect};
use vector_ink::kurbo::{BezPath, Point};
use vector_ink::{flatten_contours, Polygon};

/// The world rect the full uncropped image occupies (crop window + UV crop).
pub fn image_content_rect(rect: WorldRect, crop: Crop) -> WorldRect {
    let c = crop.clamped();
    let w = rect.w / c.w.max(1e-4);
    let h = rect.h / c.h.max(1e-4);
    WorldRect::new(rect.x - c.x * w, rect.y - c.y * h, w, h)
}

/// Rotate `p` about `center` by `delta_deg` (clockwise, y-down — same as
/// [`WorldRect::rotate_point`]).
pub fn orbit_point(center: (f32, f32), p: (f32, f32), delta_deg: f32) -> (f32, f32) {
    if delta_deg.abs() <= 0.01 {
        return p;
    }
    let rad = delta_deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = p.0 - center.0;
    let dy = p.1 - center.1;
    (
        center.0 + dx * cos - dy * sin,
        center.1 + dx * sin + dy * cos,
    )
}

/// World point → unrotated local axes about `(cx, cy)`.
pub fn world_to_local_about(px: f32, py: f32, cx: f32, cy: f32, rotation_deg: f32) -> (f32, f32) {
    if rotation_deg.abs() <= 0.01 {
        return (px, py);
    }
    orbit_point((cx, cy), (px, py), -rotation_deg)
}

/// Inverse of [`world_point`]: world coords → normalized 0..1 in `rect`.
pub fn world_to_local(px: f32, py: f32, rect: WorldRect, rotation_deg: f32) -> (f32, f32) {
    let (cx, cy) = rect.center();
    let (lx, ly) = world_to_local_about(px, py, cx, cy, rotation_deg);
    (
        (lx - rect.x) / rect.w.max(1e-6),
        (ly - rect.y) / rect.h.max(1e-6),
    )
}

/// Normalized rect in `basis` → world rect; child center orbits `pivot` by
/// `basis_rot` (paint layers and grouped nodes).
pub fn child_norm_rect_to_world(
    basis: WorldRect,
    basis_rot: f32,
    pivot: (f32, f32),
    norm: WorldRect,
    child_rot: f32,
) -> (WorldRect, f32) {
    let local = WorldRect::new(
        basis.x + norm.x * basis.w,
        basis.y + norm.y * basis.h,
        norm.w * basis.w,
        norm.h * basis.h,
    );
    let (cx, cy) = local.center();
    let (wx, wy) = orbit_point(pivot, (cx, cy), basis_rot);
    (
        WorldRect::new(wx - local.w * 0.5, wy - local.h * 0.5, local.w, local.h),
        child_rot + basis_rot,
    )
}

/// Inverse of [`child_norm_rect_to_world`].
pub fn child_world_rect_to_norm(
    basis: WorldRect,
    basis_rot: f32,
    pivot: (f32, f32),
    world: WorldRect,
    world_rot: f32,
) -> (WorldRect, f32) {
    let (cx, cy) = world.center();
    let (ux, uy) = orbit_point(pivot, (cx, cy), -basis_rot);
    let local = WorldRect::new(ux - world.w * 0.5, uy - world.h * 0.5, world.w, world.h);
    (
        WorldRect::new(
            (local.x - basis.x) / basis.w.max(1e-6),
            (local.y - basis.y) / basis.h.max(1e-6),
            local.w / basis.w.max(1e-6),
            local.h / basis.h.max(1e-6),
        ),
        world_rot - basis_rot,
    )
}

/// A normalized path point placed in `rect` and rotated about its center.
pub fn world_point(p: [f32; 2], rect: WorldRect, rotation_deg: f32) -> Point {
    let x = rect.x + p[0] * rect.w;
    let y = rect.y + p[1] * rect.h;
    if rotation_deg.abs() <= 0.01 {
        return Point::new(x as f64, y as f64);
    }
    let [x, y] = rect.rotate_point([x, y], rotation_deg);
    Point::new(x as f64, y as f64)
}

/// Normalized path data placed in `rect` and rotated about its center.
pub fn path_data_to_world_bez(path: &PathData, rect: WorldRect, rotation_deg: f32) -> BezPath {
    let at = |p: [f32; 2]| world_point(p, rect, rotation_deg);
    let mut bez = BezPath::new();
    let mut contour = |start: [f32; 2], segs: &[PathSeg], closed: bool| {
        bez.move_to(at(start));
        for seg in segs {
            match *seg {
                PathSeg::Line { to } => bez.line_to(at(to)),
                PathSeg::Quad { ctrl, to } => bez.quad_to(at(ctrl), at(to)),
                PathSeg::Cubic { c1, c2, to } => bez.curve_to(at(c1), at(c2), at(to)),
            }
        }
        if closed {
            bez.close_path();
        }
    };
    contour(path.start, &path.segs, path.closed);
    for extra in &path.extra {
        contour(extra.start, &extra.segs, extra.closed);
    }
    bez
}

/// The world polyline of an open curve: a straight line, or an open path's
/// first contour. `None` for anything closed.
pub fn node_open_polyline(n: &Node, tolerance: f32) -> Option<Vec<[f32; 2]>> {
    let NodeKind::Shape(s) = &n.kind else {
        return None;
    };
    match s.shape {
        ShapeKind::Line => {
            let r = n.rect;
            let (a, b) = if s.flip {
                ([r.x, r.y + r.h], [r.x + r.w, r.y])
            } else {
                ([r.x, r.y], [r.x + r.w, r.y + r.h])
            };
            Some(vec![
                r.rotate_point(a, n.rotation_deg),
                r.rotate_point(b, n.rotation_deg),
            ])
        }
        ShapeKind::Path => {
            let path = s.path.as_ref()?;
            if path.closed {
                return None;
            }
            let bez = path_data_to_world_bez(path, n.rect, n.rotation_deg);
            flatten_contours(&bez, tolerance as f64)
                .into_iter()
                .next()
                .filter(|c| c.len() >= 2)
        }
        _ => None,
    }
}

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
        NodeKind::Shape(s) => match s.shape {
            ShapeKind::Rect | ShapeKind::Ellipse => {
                let outline = if s.shape == ShapeKind::Rect {
                    s.corner.outline(n.rect, tolerance)
                } else {
                    n.rect.ellipse_outline(tolerance)
                };
                Some(vec![outline
                    .into_iter()
                    .map(|p| n.rect.rotate_point(p, n.rotation_deg))
                    .collect()])
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
        },
        NodeKind::Text(_) | NodeKind::Image(_) | NodeKind::Frame(_) => Some(vec![n
            .rect
            .corners_rotated(n.rotation_deg)
            .into_iter()
            .map(|(x, y)| [x, y])
            .collect()]),
        _ => None,
    }
}
