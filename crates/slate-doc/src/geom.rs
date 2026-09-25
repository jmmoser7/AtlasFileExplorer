//! A node's authored geometry in world space: the one reading of path data,
//! rotation, and outlines that pick, trim, and bumpers share. The artifact
//! writer keeps its own reading (a second interpreter of the model).

use crate::scene::{
    clamp_regular_sides, regular_polygon_vertices, Corner, Node, NodeKind, PathData, PathSeg,
    ShapeKind, WorldRect,
};
use crate::wire::{filleted_vertex_path, PathCmd};
use vector_ink::kurbo::{BezPath, Point};
use vector_ink::{flatten_contours, Polygon};

pub fn path_is_line_polyline(path: &PathData) -> bool {
    path.extra.is_empty()
        && !path.segs.is_empty()
        && path.segs.iter().all(|s| matches!(s, PathSeg::Line { .. }))
}

fn polyline_norm_points(path: &PathData) -> Vec<[f32; 2]> {
    let mut pts = vec![path.start];
    for seg in &path.segs {
        if let PathSeg::Line { to } = *seg {
            pts.push(to);
        }
    }
    pts
}

fn path_cmds_to_bez_world(cmds: &[PathCmd]) -> BezPath {
    let mut bez = BezPath::new();
    for cmd in cmds {
        match *cmd {
            PathCmd::Move(p) => bez.move_to(Point::new(p[0] as f64, p[1] as f64)),
            PathCmd::Line(p) => bez.line_to(Point::new(p[0] as f64, p[1] as f64)),
            PathCmd::Cubic { c1, c2, to } => bez.curve_to(
                Point::new(c1[0] as f64, c1[1] as f64),
                Point::new(c2[0] as f64, c2[1] as f64),
                Point::new(to[0] as f64, to[1] as f64),
            ),
        }
    }
    bez
}

/// Vertex fillet for line-only polylines (P1.shape.properties Corners on polyline).
pub fn path_data_to_world_bez_with_fillet(
    path: &PathData,
    rect: WorldRect,
    rotation_deg: f32,
    corner: Corner,
) -> BezPath {
    if path_is_line_polyline(path) {
        let (_, radius) = corner.effective(rect.w, rect.h);
        if radius > 0.0 {
            let world: Vec<[f32; 2]> = polyline_norm_points(path)
                .into_iter()
                .map(|p| {
                    let pt = world_point(p, rect, rotation_deg);
                    [pt.x as f32, pt.y as f32]
                })
                .collect();
            if world.len() >= 3 || (world.len() >= 2 && !path.closed) {
                let cmds = filleted_vertex_path(&world, radius, path.closed);
                return path_cmds_to_bez_world(&cmds);
            }
        }
    }
    path_data_to_world_bez(path, rect, rotation_deg)
}

pub fn regular_polygon_world_outline(
    rect: WorldRect,
    rotation_deg: f32,
    sides: u8,
    corner: Corner,
    tolerance: f32,
) -> Vec<[f32; 2]> {
    let sides = clamp_regular_sides(sides);
    let verts = regular_polygon_vertices(rect, sides);
    let (_, radius) = corner.effective(rect.w, rect.h);
    let outline = if radius <= 0.0 {
        verts
    } else {
        let cmds = filleted_vertex_path(&verts, radius, true);
        let bez = path_cmds_to_bez_world(&cmds);
        flatten_contours(&bez, tolerance as f64)
            .into_iter()
            .next()
            .unwrap_or(verts)
    };
    outline
        .into_iter()
        .map(|p| rect.rotate_point(p, rotation_deg))
        .collect()
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

/// Inverse of rotating a world point about `(cx, cy)` by `rotation_deg`.
pub fn world_to_local(px: f32, py: f32, cx: f32, cy: f32, rotation_deg: f32) -> (f32, f32) {
    if rotation_deg.abs() < f32::EPSILON {
        return (px, py);
    }
    let rad = (-rotation_deg).to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = px - cx;
    let dy = py - cy;
    (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Corner, PathData, PathSeg, WorldRect};

    #[test]
    fn polyline_fillet_zero_is_sharp_polyline() {
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [0.5, 0.0] },
                PathSeg::Line { to: [0.5, 0.5] },
            ],
            closed: false,
            ..Default::default()
        };
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let bez = path_data_to_world_bez_with_fillet(&path, rect, 0.0, Corner::Square);
        let flat = flatten_contours(&bez, 0.25);
        assert_eq!(flat.len(), 1);
        assert!(flat[0].len() >= 3);
    }

    #[test]
    fn regular_polygon_default_is_six_sides() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let v = crate::scene::regular_polygon_vertices(rect, 6);
        assert_eq!(v.len(), 6);
    }

    #[test]
    fn world_to_local_inverts_world_point() {
        let rect = WorldRect::new(10.0, 20.0, 80.0, 40.0);
        for rot in [0.0, 15.0, 90.0, 133.0] {
            let (cx, cy) = rect.center();
            let wx = rect.x + 0.25 * rect.w;
            let wy = rect.y + 0.75 * rect.h;
            let [rx, ry] = rect.rotate_point([wx, wy], rot);
            let (lx, ly) = world_to_local(rx, ry, cx, cy, rot);
            assert!(
                (lx - wx).abs() < 1e-4 && (ly - wy).abs() < 1e-4,
                "rot={rot}"
            );
        }
    }
}
