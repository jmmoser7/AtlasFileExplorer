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
        let (chamfer, amount) = corner.effective(rect.w, rect.h);
        if amount > 0.0 {
            let world = polyline_world_points(path, rect, rotation_deg);
            if world.len() >= 3 || (world.len() >= 2 && !path.closed) {
                let cmds = filleted_vertex_path(&world, amount, chamfer, path.closed);
                return path_cmds_to_bez_world(&cmds);
            }
        }
    }
    path_data_to_world_bez(path, rect, rotation_deg)
}

fn polyline_world_points(path: &PathData, rect: WorldRect, rotation_deg: f32) -> Vec<[f32; 2]> {
    polyline_norm_points(path)
        .into_iter()
        .map(|p| {
            let pt = world_point(p, rect, rotation_deg);
            [pt.x as f32, pt.y as f32]
        })
        .collect()
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
    let (chamfer, amount) = corner.effective(rect.w, rect.h);
    let mut outline = if amount <= 0.0 {
        verts
    } else {
        let cmds = filleted_vertex_path(&verts, amount, chamfer, true);
        let bez = path_cmds_to_bez_world(&cmds);
        flatten_contours(&bez, tolerance as f64)
            .into_iter()
            .next()
            .unwrap_or(verts)
    };
    // A stroked loop that revisits a point reverses 180° there, which the
    // stroke tessellator turns into a spike.
    let same = |a: &[f32; 2], b: &[f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-3;
    outline.dedup_by(|a, b| same(a, b));
    while outline.len() > 1 && same(&outline[0], &outline[outline.len() - 1]) {
        outline.pop();
    }
    outline
        .into_iter()
        .map(|p| rect.rotate_point(p, rotation_deg))
        .collect()
}

/// The edge a corner grip rides (P1.node.corner-grip): the grip sits
/// `travel` along `dir` from `vertex`, where `travel` is the treatment's
/// tangent distance — the fillet's tangent point, or the chamfer's cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerGripEdge {
    /// The sharp corner the grip measures from (world).
    pub vertex: [f32; 2],
    /// Unit direction along the edge, away from `vertex` (world).
    pub dir: [f32; 2],
    /// Unit normal of the edge toward the inside of the corner.
    pub inward: [f32; 2],
    /// Travel per unit of corner amount.
    pub per_amount: f32,
    /// Largest travel the host allows.
    pub max_travel: f32,
}

impl CornerGripEdge {
    pub fn point(&self, travel: f32) -> [f32; 2] {
        [
            self.vertex[0] + self.dir[0] * travel,
            self.vertex[1] + self.dir[1] * travel,
        ]
    }

    /// Signed distance of `p` along the edge from the vertex.
    pub fn project(&self, p: [f32; 2]) -> f32 {
        (p[0] - self.vertex[0]) * self.dir[0] + (p[1] - self.vertex[1]) * self.dir[1]
    }

    pub fn travel_for_amount(&self, amount: f32) -> f32 {
        (amount.max(0.0) * self.per_amount).min(self.max_travel)
    }

    pub fn amount_for_travel(&self, travel: f32) -> f32 {
        travel.clamp(0.0, self.max_travel) / self.per_amount.max(1e-6)
    }

    /// Largest amount that still changes the outline.
    pub fn max_amount(&self) -> f32 {
        self.amount_for_travel(self.max_travel)
    }
}

/// The grip edge for a corner-capable node: the top edge from the top-left
/// corner for box hosts, the side from the top vertex toward the next one
/// (clockwise) for regular polygons, and the leaving side of the first
/// turning vertex for line polylines.
pub fn corner_grip_edge(node: &Node, chamfer: bool) -> Option<CornerGripEdge> {
    let rect = node.rect;
    let rot = node.rotation_deg;
    let from_vertex = |prev: [f32; 2], cur: [f32; 2], next: [f32; 2]| {
        let vc = crate::wire::vertex_corner(prev, cur, next, chamfer)?;
        let side = (vc.u_in[0] * vc.u_out[1] - vc.u_in[1] * vc.u_out[0]).signum();
        Some(CornerGripEdge {
            vertex: cur,
            dir: vc.u_out,
            inward: [-vc.u_out[1] * side, vc.u_out[0] * side],
            per_amount: vc.per_amount,
            max_travel: vc.max_tangent,
        })
    };
    match &node.kind {
        NodeKind::Shape(s) if s.shape == ShapeKind::RegularPolygon => {
            let sides = clamp_regular_sides(s.sides) as usize;
            let verts: Vec<[f32; 2]> = regular_polygon_vertices(rect, sides as u8)
                .into_iter()
                .map(|p| rect.rotate_point(p, rot))
                .collect();
            let mut edge = from_vertex(verts[sides - 1], verts[0], verts[1])?;
            // Corner amounts clamp to half the short side of the box.
            let limit = rect.w.min(rect.h) * 0.5 * edge.per_amount;
            edge.max_travel = edge.max_travel.min(limit);
            Some(edge)
        }
        NodeKind::Shape(s) if s.shape == ShapeKind::Rect => Some(box_grip_edge(rect, rot)),
        NodeKind::Shape(s) => {
            let path = s.path.as_ref().filter(|p| path_is_line_polyline(p))?;
            let mut pts = polyline_world_points(path, rect, rot);
            pts.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-3);
            if path.closed
                && pts.len() > 1
                && (pts[0][0] - pts[pts.len() - 1][0]).hypot(pts[0][1] - pts[pts.len() - 1][1])
                    < 1e-3
            {
                pts.pop();
            }
            let n = pts.len();
            if n < 3 {
                return None;
            }
            let interior: Box<dyn Iterator<Item = usize>> = if path.closed {
                Box::new(0..n)
            } else {
                Box::new(1..n - 1)
            };
            interior
                .filter_map(|i| from_vertex(pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]))
                .next()
        }
        NodeKind::Image(_) | NodeKind::Frame(_) | NodeKind::Portal(_) => {
            Some(box_grip_edge(rect, rot))
        }
        _ => None,
    }
}

fn box_grip_edge(rect: WorldRect, rotation_deg: f32) -> CornerGripEdge {
    let vertex = rect.rotate_point([rect.x, rect.y], rotation_deg);
    let (s, c) = rotation_deg.to_radians().sin_cos();
    CornerGripEdge {
        vertex,
        dir: [c, s],
        inward: [-s, c],
        per_amount: 1.0,
        max_travel: rect.w.min(rect.h).max(0.0) * 0.5,
    }
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

    /// Finite, inside `bounds`, no zero-length edge, never doubling back, and
    /// turning one way only. A doubled-back vertex is what a mitered stroke
    /// join extends toward infinity.
    fn assert_clean_convex_outline(outline: &[[f32; 2]], bounds: Option<WorldRect>, label: &str) {
        assert!(outline.len() >= 3, "{label}: {} points", outline.len());
        for p in outline {
            assert!(
                p[0].is_finite() && p[1].is_finite(),
                "{label}: non-finite {p:?}"
            );
            if let Some(r) = bounds {
                let eps = 1e-3 * r.w.max(r.h).max(1.0);
                assert!(
                    p[0] >= r.x - eps
                        && p[0] <= r.x + r.w + eps
                        && p[1] >= r.y - eps
                        && p[1] <= r.y + r.h + eps,
                    "{label}: {p:?} escapes {r:?}"
                );
            }
        }
        let n = outline.len();
        let mut turn = 0.0f32;
        for i in 0..n {
            let a = outline[i];
            let b = outline[(i + 1) % n];
            let c = outline[(i + 2) % n];
            let e0 = [b[0] - a[0], b[1] - a[1]];
            let e1 = [c[0] - b[0], c[1] - b[1]];
            let l0 = e0[0].hypot(e0[1]);
            let l1 = e1[0].hypot(e1[1]);
            assert!(l0 > 1e-4, "{label}: zero-length edge after {a:?}");
            let dot = (e0[0] * e1[0] + e0[1] * e1[1]) / (l0 * l1);
            assert!(dot > -0.999, "{label}: outline doubles back at {b:?}");
            let cross = (e0[0] * e1[1] - e0[1] * e1[0]) / (l0 * l1);
            if cross.abs() > 1e-3 {
                if turn == 0.0 {
                    turn = cross.signum();
                }
                assert_eq!(cross.signum(), turn, "{label}: concave turn at {b:?}");
            }
        }
    }

    #[test]
    fn regular_polygon_corner_outline_stays_bounded_for_every_side_count_and_radius() {
        let rects = [
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            WorldRect::new(10.0, -20.0, 240.0, 90.0),
            WorldRect::new(-50.0, 5.0, 60.0, 200.0),
        ];
        let amounts = [
            0.0,
            0.3,
            1.0,
            7.5,
            20.0,
            33.0,
            50.0,
            120.0,
            1.0e4,
            1.0e30,
            f32::MAX,
        ];
        for rect in rects {
            for sides in 3..=12u8 {
                for amount in amounts {
                    for corner in [
                        Corner::Rounded { radius: amount },
                        Corner::Chamfer { cut: amount },
                    ] {
                        for rot in [0.0, 33.0] {
                            let outline =
                                regular_polygon_world_outline(rect, rot, sides, corner, 0.05);
                            let label = format!("{rect:?} sides={sides} {corner:?} rot={rot}");
                            let bounds = (rot == 0.0).then_some(rect);
                            assert_clean_convex_outline(&outline, bounds, &label);
                        }
                    }
                }
                for corner in [
                    Corner::RoundedPercent { percent: 100.0 },
                    Corner::ChamferPercent { percent: 100.0 },
                ] {
                    let outline = regular_polygon_world_outline(rect, 0.0, sides, corner, 0.05);
                    let label = format!("{rect:?} sides={sides} {corner:?}");
                    assert_clean_convex_outline(&outline, Some(rect), &label);
                }
            }
        }
    }

    #[test]
    fn polygon_fillet_radius_is_the_arc_radius() {
        // A true regular hexagon (square box): 120 degree interior angles.
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let radius = 10.0f32;
        let outline = regular_polygon_world_outline(rect, 0.0, 6, Corner::Rounded { radius }, 0.01);
        let v0 = [50.0f32, 0.0];
        let half_interior = 60.0f32.to_radians();
        let tangent = radius / half_interior.tan();
        let center = [v0[0], v0[1] + radius / half_interior.sin()];
        let arc: Vec<_> = outline
            .iter()
            .filter(|p| (p[0] - v0[0]).hypot(p[1] - v0[1]) < tangent * 1.02)
            .collect();
        assert!(arc.len() >= 3, "arc samples near the top vertex: {arc:?}");
        for p in arc {
            let d = (p[0] - center[0]).hypot(p[1] - center[1]);
            assert!(
                (d - radius).abs() < 0.05,
                "{p:?} is {d} from the fillet center, not {radius}"
            );
        }
        let edge = [43.30127f32 / 50.0, 25.0 / 50.0];
        let b0 = [v0[0] + edge[0] * tangent, v0[1] + edge[1] * tangent];
        assert!(
            outline
                .iter()
                .any(|p| (p[0] - b0[0]).hypot(p[1] - b0[1]) < 0.02),
            "the arc must end at the tangent point {b0:?}"
        );
    }

    #[test]
    fn polyline_chamfer_cuts_straight_corners() {
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            closed: false,
            ..Default::default()
        };
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let bez =
            path_data_to_world_bez_with_fillet(&path, rect, 0.0, Corner::Chamfer { cut: 10.0 });
        assert!(
            bez.elements().iter().all(|e| !matches!(
                e,
                vector_ink::kurbo::PathEl::CurveTo(..) | vector_ink::kurbo::PathEl::QuadTo(..)
            )),
            "a chamfer is straight: {:?}",
            bez.elements()
        );
        let flat = flatten_contours(&bez, 0.05);
        let near = |q: [f32; 2]| {
            flat[0]
                .iter()
                .any(|p| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-3)
        };
        assert!(near([90.0, 0.0]) && near([100.0, 10.0]), "{:?}", flat[0]);
        assert!(!near([100.0, 0.0]), "the corner itself is cut away");
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
