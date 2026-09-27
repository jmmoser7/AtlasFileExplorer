//! A node's authored geometry in world space: the one reading of path data,
//! rotation, and outlines that pick, trim, and bumpers share. The artifact
//! writer keeps its own reading (a second interpreter of the model).

use crate::scene::{
    clamp_regular_sides, regular_polygon_vertices, Corner, Crop, Node, NodeKind, PathData, PathSeg,
    ShapeKind, WorldRect,
};
use crate::wire::{filleted_vertex_path, PathCmd};
use vector_ink::kurbo::{BezPath, Point};
use vector_ink::{flatten_contours, point_in_polygon, Polygon};

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
            let (lx, ly) = world_to_local_about(rx, ry, cx, cy, rot);
            assert!(
                (lx - wx).abs() < 1e-4 && (ly - wy).abs() < 1e-4,
                "rot={rot}"
            );
        }
    }
}

fn segments_intersect(a1: [f32; 2], a2: [f32; 2], b1: [f32; 2], b2: [f32; 2]) -> bool {
    fn orient(p: [f32; 2], q: [f32; 2], r: [f32; 2]) -> f32 {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    }
    let d1 = orient(b1, b2, a1);
    let d2 = orient(b1, b2, a2);
    let d3 = orient(a1, a2, b1);
    let d4 = orient(a1, a2, b2);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// Any vertex inside, or any segment crossing a ring edge.
pub fn polyline_intersects_polygon(line: &[[f32; 2]], poly: &Polygon) -> bool {
    if line.is_empty() || poly.is_empty() {
        return false;
    }
    for p in line {
        if point_in_polygon(poly, *p) {
            return true;
        }
    }
    if line.len() < 2 {
        return false;
    }
    for i in 0..line.len() - 1 {
        let a1 = line[i];
        let a2 = line[i + 1];
        for ring in poly {
            if ring.len() < 2 {
                continue;
            }
            for j in 0..ring.len() {
                let b1 = ring[j];
                let b2 = ring[(j + 1) % ring.len()];
                if segments_intersect(a1, a2, b1, b2) {
                    return true;
                }
            }
        }
    }
    false
}

/// Flattened centerlines / outlines used to test ink against a host window.
pub fn node_stroke_polylines(n: &Node, tolerance: f32) -> Vec<Vec<[f32; 2]>> {
    if let Some(open) = node_open_polyline(n, tolerance) {
        return vec![open];
    }
    if let Some(poly) = node_closed_poly(n, tolerance) {
        return poly
            .into_iter()
            .filter_map(|ring| {
                if ring.len() < 2 {
                    return None;
                }
                let mut line = ring;
                line.push(line[0]);
                Some(line)
            })
            .collect();
    }
    vec![]
}

/// Whether world point `p` lies inside the host's visible outline (trim clip when set).
pub fn point_in_node_outline(host: &Node, px: f32, py: f32, tolerance: f32) -> bool {
    node_closed_poly(host, tolerance).is_some_and(|poly| point_in_polygon(&poly, [px, py]))
}

/// Whether any part of `stroke` intersects the host's visible outline.
pub fn stroke_intersects_node_outline(stroke: &Node, host: &Node, tolerance: f32) -> bool {
    let Some(host_poly) = node_closed_poly(host, tolerance) else {
        return false;
    };
    node_stroke_polylines(stroke, tolerance)
        .iter()
        .any(|line| polyline_intersects_polygon(line, &host_poly))
}

#[cfg(test)]
mod paint_window_tests {
    use super::*;
    use crate::scene::{Corner, ImageNode, NodeKind, ShapeKind, ShapeNode, Stroke, WorldRect};

    #[test]
    fn stroke_hits_host_outline_not_far_away() {
        let mut scene = crate::scene::Scene::default();
        let host = scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(crate::ItemId(1))),
        );
        let line = scene.build_node(
            WorldRect::new(10.0, 50.0, 80.0, 0.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Line,
                sides: 6,
                fill: None,
                stroke: Stroke::default(),
                corner: Corner::Square,
                flip: false,
                path: None,
                text: None,
            }),
        );
        assert!(stroke_intersects_node_outline(&line, &host, 0.05));
        let outside = scene.build_node(
            WorldRect::new(200.0, 200.0, 50.0, 0.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Line,
                sides: 6,
                fill: None,
                stroke: Stroke::default(),
                corner: Corner::Square,
                flip: false,
                path: None,
                text: None,
            }),
        );
        assert!(!stroke_intersects_node_outline(&outside, &host, 0.05));
    }
}

/// Arrowhead for a stroke of `stroke_width`: a filled triangle with its tip
/// at `tip` and its base back along `into_curve` (the unit tangent pointing
/// from the endpoint into the curve). Connectors and open curves share it,
/// on the board and in the artifact.
pub fn arrow_head(tip: [f32; 2], into_curve: [f32; 2], stroke_width: f32) -> [[f32; 2]; 3] {
    let len = arrow_len(stroke_width);
    let half = len * 0.4;
    let base = [tip[0] + into_curve[0] * len, tip[1] + into_curve[1] * len];
    let perp = [-into_curve[1], into_curve[0]];
    [
        tip,
        [base[0] + perp[0] * half, base[1] + perp[1] * half],
        [base[0] - perp[0] * half, base[1] - perp[1] * half],
    ]
}

/// Arrowhead length along the curve.
pub fn arrow_len(stroke_width: f32) -> f32 {
    (stroke_width * 4.0).max(10.0)
}

/// Endpoint and inward unit tangent at the end of an open path, for an
/// arrowhead. `None` for an empty or degenerate path.
pub fn path_end_arrow(bez: &BezPath) -> Option<([f32; 2], [f32; 2])> {
    use vector_ink::kurbo::{ParamCurve, ParamCurveDeriv, PathSeg};
    let seg: PathSeg = bez.segments().last()?;
    let end = seg.eval(1.0);
    let d = match seg {
        PathSeg::Line(l) => l.p0 - l.p1,
        PathSeg::Quad(q) => {
            let v = -q.deriv().eval(1.0).to_vec2();
            if v.hypot() > 1e-9 {
                v
            } else {
                q.p0 - q.p2
            }
        }
        PathSeg::Cubic(c) => {
            let v = -c.deriv().eval(1.0).to_vec2();
            if v.hypot() > 1e-9 {
                v
            } else {
                c.p0 - c.p3
            }
        }
    };
    let len = d.hypot();
    if !(len > 1e-9) {
        return None;
    }
    Some((
        [end.x as f32, end.y as f32],
        [(d.x / len) as f32, (d.y / len) as f32],
    ))
}

/// `bez` with its last segment shortened by `by` along its arc length, so a
/// stroke under an arrowhead ends at the head's base and not at its tip.
pub fn trim_end(bez: &BezPath, by: f64) -> BezPath {
    use vector_ink::kurbo::{ParamCurve, ParamCurveArclen, PathEl};
    let Some(last) = bez.segments().last() else {
        return bez.clone();
    };
    let len = last.arclen(1e-3);
    if !(by > 0.0) || len <= by * 1.05 {
        return bez.clone();
    }
    let t = last.inv_arclen(len - by, 1e-3);
    let cut = last.subsegment(0.0..t);
    let mut out = BezPath::new();
    let els = bez.elements();
    let last_idx = els
        .iter()
        .rposition(|e| !matches!(e, PathEl::ClosePath | PathEl::MoveTo(_)))
        .unwrap_or(0);
    for (i, el) in els.iter().enumerate() {
        if i == last_idx {
            out.push(cut.as_path_el());
        } else {
            out.push(*el);
        }
    }
    out
}

/// How far a stroke under an arrowhead stops short of the tip: most of the
/// head's length, so the line tucks under the base.
pub fn arrow_trim(stroke_width: f32) -> f64 {
    (arrow_len(stroke_width) * 0.8) as f64
}
