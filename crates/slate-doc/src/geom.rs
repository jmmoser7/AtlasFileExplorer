//! A node's authored geometry in world space: the one reading of path data,
//! rotation, and outlines that pick, trim, and bumpers share. The artifact
//! writer keeps its own reading (a second interpreter of the model).

use crate::scene::{
    clamp_regular_sides, regular_polygon_vertices, Corner, Crop, Node, NodeKind, PathData, PathSeg,
    ShapeKind, Stroke, WorldRect,
};
use crate::wire::{
    filleted_vertex_path, filleted_vertex_path_each, filleted_vertex_path_params_each, PathCmd,
};
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
        let (chamfer, amounts) = polyline_vertex_amounts(path, rect, corner);
        if amounts.iter().any(|a| *a > 0.0) {
            let world = polyline_world_points(path, rect, rotation_deg);
            if world.len() >= 3 || (world.len() >= 2 && !path.closed) {
                let cmds = filleted_vertex_path_each(&world, &amounts, chamfer, path.closed);
                return path_cmds_to_bez_world(&cmds);
            }
        }
    }
    path_data_to_world_bez(path, rect, rotation_deg)
}

/// A line polyline's chamfer flag and corner amount per vertex: each
/// vertex's override, else the shape's corner (absolute amounts are clamped
/// per corner by the fillet, not by the box).
fn polyline_vertex_amounts(path: &PathData, rect: WorldRect, corner: Corner) -> (bool, Vec<f32>) {
    let (chamfer, amount) = corner.vertex_effective(rect.w, rect.h);
    let amounts = (0..=path.segs.len())
        .map(|i| path.vertex_corner_amount(i, amount))
        .collect();
    (chamfer, amounts)
}

/// The world path a per-vertex (tipped) stroke follows, and for each of its
/// on-curve vertices the vertex parameter of `path` it sits at: vertex `i` is
/// `i`, a point on the segment leaving vertex `i` is `i` plus its fraction of
/// that segment. Without fillets the two paths are one. A filleted line
/// polyline gains vertices along its edges and fillets; each fillet's middle
/// sits at its corner's own parameter (`filleted_vertex_path_params`).
pub fn tipped_stroke_world_path(
    path: &PathData,
    rect: WorldRect,
    rotation_deg: f32,
    corner: Corner,
) -> (BezPath, Vec<f32>) {
    if path_is_line_polyline(path) {
        let (chamfer, amounts) = polyline_vertex_amounts(path, rect, corner);
        if amounts.iter().any(|a| *a > 0.0) {
            let world = polyline_world_points(path, rect, rotation_deg);
            if world.len() >= 3 || (world.len() >= 2 && !path.closed) {
                let tracked = filleted_vertex_path_params_each(
                    &world,
                    &amounts,
                    chamfer,
                    path.closed,
                    FILLETED_TIP_STEPS,
                );
                let cmds: Vec<PathCmd> = tracked.iter().map(|(cmd, _)| *cmd).collect();
                let params = tracked.iter().map(|(_, at)| *at).collect();
                return (path_cmds_to_bez_world(&cmds), params);
            }
        }
    }
    let bez = path_data_to_world_bez(path, rect, rotation_deg);
    let vertices = bez
        .elements()
        .iter()
        .filter(|el| !matches!(el, vector_ink::kurbo::PathEl::ClosePath))
        .count();
    (bez, (0..vertices).map(|i| i as f32).collect())
}

/// A hard vector stroke with per-vertex widths or colors, as both
/// interpreters paint it: the path it follows, one full width per on-curve
/// vertex of that path, straight RGBA (`0..=1`) per vertex when the colors
/// vary, and how tips blend between vertices.
#[derive(Clone)]
pub struct TippedStroke {
    pub bez: BezPath,
    pub widths: Vec<f32>,
    pub colors: Option<Vec<[f32; 4]>>,
    pub ease: vector_ink::TipEase,
}

/// The per-vertex stroke of `path` placed in `rect`, or `None` when it
/// paints one width and one color (`PathData::vector_widths`,
/// `PathData::vector_colors`). `corner` fillets a line polyline, as for its
/// plain stroke.
pub fn tipped_stroke(
    path: &PathData,
    stroke: &Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    corner: Corner,
) -> Option<TippedStroke> {
    let widths = path.vector_widths(stroke);
    let colors = path.vector_colors();
    if widths.is_none() && colors.is_none() {
        return None;
    }
    let widths = widths.unwrap_or_else(|| vec![stroke.width.max(0.0); path.tips.len()]);
    let (bez, params) = tipped_stroke_world_path(path, rect, rotation_deg, corner);
    let mut ease = tip_ease(&bez);
    let mut between = vector_ink::TipEase::Linear;
    // A filleted polyline eases from polyline vertex to polyline vertex, not
    // between the fillet ends it paints through: its values are eased by
    // vertex parameter and its straight edges are densified to carry them.
    let (bez, params) = if ease == vector_ink::TipEase::Smooth && path_is_line_polyline(path) {
        (between, ease) = (ease, vector_ink::TipEase::Linear);
        densify_lines(&bez, &params, FILLETED_TIP_STEPS)
    } else {
        (bez, params)
    };
    let colors = colors.map(|colors| {
        let channels: [Vec<f32>; 4] =
            std::array::from_fn(|i| colors.iter().map(|c| c.0[i] as f32 / 255.0).collect());
        params
            .iter()
            .map(|at| std::array::from_fn(|i| eased_value_at_param(&channels[i], *at, between)))
            .collect()
    });
    Some(TippedStroke {
        bez,
        widths: params
            .iter()
            .map(|at| eased_value_at_param(&widths, *at, between))
            .collect(),
        colors,
        ease,
    })
}

/// Pieces each straight edge, and each half of each fillet, of a filleted
/// polyline's tipped stroke is cut into, so a linear blend between them
/// follows the smoothstep with no kink at a fillet's middle or ends.
const FILLETED_TIP_STEPS: usize = 16;

/// `bez` with each line segment cut into `steps` equal pieces, and the
/// vertex parameter of every on-curve vertex (`params` for the old ones).
fn densify_lines(bez: &BezPath, params: &[f32], steps: usize) -> (BezPath, Vec<f32>) {
    use vector_ink::kurbo::PathEl;
    let mut out = BezPath::new();
    let mut at = Vec::with_capacity(params.len() * steps);
    let (mut k, mut last) = (0usize, Point::ZERO);
    for el in bez.elements() {
        if let (PathEl::LineTo(p), Some(&from), Some(&to)) =
            (el, params.get(k.wrapping_sub(1)), params.get(k))
        {
            for s in 1..steps {
                let t = s as f64 / steps as f64;
                out.line_to(last.lerp(*p, t));
                at.push(from + (to - from) * t as f32);
            }
        }
        out.push(*el);
        if let Some(p) = el.end_point() {
            if !matches!(el, PathEl::ClosePath) {
                at.push(params.get(k).copied().unwrap_or(0.0));
                k += 1;
            }
            last = p;
        }
    }
    (out, at)
}

/// `values` (one per path vertex) at vertex parameter `at`, straight between
/// vertices. Parameter `values.len()` is the first vertex again, the far end
/// of a closed path's closing segment.
pub fn value_at_param(values: &[f32], at: f32) -> f32 {
    eased_value_at_param(values, at, vector_ink::TipEase::Linear)
}

/// [`value_at_param`], blended between vertices by `ease`.
fn eased_value_at_param(values: &[f32], at: f32, ease: vector_ink::TipEase) -> f32 {
    let Some(&first) = values.first() else {
        return 0.0;
    };
    let n = values.len();
    let at = at.clamp(0.0, n as f32);
    let i = (at.floor() as usize).min(n);
    let f = at - i as f32;
    let get = |k: usize| if k >= n { first } else { values[k] };
    if f <= 0.0 {
        return get(i);
    }
    get(i) + (get(i + 1) - get(i)) * ease.weight(f)
}

/// How a path blends per-vertex tips (P1.curve.tip-chord): polylines,
/// lines and circular arcs straight by arc length; any other curve (a Bézier
/// span, a Pen curve, a filleted polyline as it paints) smoothly, so its
/// width and color have no chines. Geometry, not tool provenance, decides,
/// as for the curve's grips.
pub fn tip_ease(bez: &BezPath) -> vector_ink::TipEase {
    let curved = bez.elements().iter().any(|el| {
        matches!(
            el,
            vector_ink::kurbo::PathEl::QuadTo(..) | vector_ink::kurbo::PathEl::CurveTo(..)
        )
    });
    if curved && arc_grip_points(bez).is_none() {
        vector_ink::TipEase::Smooth
    } else {
        vector_ink::TipEase::Linear
    }
}

/// Center and radius of the circle through three points; `None` when they
/// are collinear or coincide.
pub fn circle_through(a: Point, b: Point, c: Point) -> Option<(Point, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-4 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let r = ((a.x - ux).powi(2) + (a.y - uy).powi(2)).sqrt();
    (r >= 1e-6).then_some((Point::new(ux, uy), r))
}

/// How far a committed curve may stray from one circle and still read as an
/// arc: the Arc tool's own cubic fitting tolerance (0.25) plus float slack.
const ARC_FIT_TOLERANCE: f64 = 0.3;

/// Start, through point and end of `bez` when it is one open circular arc
/// made of cubic spans, as the Arc tool writes it. The through point is the
/// middle of the sweep. Geometry, not tool provenance, decides.
pub fn arc_grip_points(bez: &BezPath) -> Option<[Point; 3]> {
    use vector_ink::kurbo::{CubicBez, ParamCurve, PathEl, PathSeg as KSeg};
    let els = bez.elements();
    let moves = els
        .iter()
        .filter(|el| matches!(el, PathEl::MoveTo(_)))
        .count();
    if moves != 1 || els.iter().any(|el| matches!(el, PathEl::ClosePath)) {
        return None;
    }
    let spans: Vec<CubicBez> = bez
        .segments()
        .map(|seg| match seg {
            KSeg::Cubic(c) => Some(c),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let n = spans.len();
    let (start, end) = (spans.first()?.p0, spans.last()?.p3);
    let mid = if n.is_multiple_of(2) {
        spans[n / 2].p0
    } else {
        spans[n / 2].eval(0.5)
    };
    let (center, r) = circle_through(start, mid, end)?;
    let on_circle = spans.iter().all(|span| {
        [0.25, 0.5, 0.75, 1.0]
            .iter()
            .all(|t| ((span.eval(*t) - center).hypot() - r).abs() <= ARC_FIT_TOLERANCE)
    });
    on_circle.then_some([start, mid, end])
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
    phase_deg: f32,
    corner: Corner,
    tolerance: f32,
) -> Vec<[f32; 2]> {
    let sides = clamp_regular_sides(sides);
    let verts = regular_polygon_vertices(rect, sides, phase_deg);
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
            let verts: Vec<[f32; 2]> = regular_polygon_vertices(rect, sides as u8, s.phase_deg)
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
            let pts = polyline_distinct_vertices(path, rect, rot);
            let n = pts.len();
            polyline_interior(path.closed, n)
                .filter_map(|i| from_vertex(pts[(i + n - 1) % n].1, pts[i].1, pts[(i + 1) % n].1))
                .next()
        }
        NodeKind::Image(_) | NodeKind::Frame(_) | NodeKind::Portal(_) => {
            Some(box_grip_edge(rect, rot))
        }
        _ => None,
    }
}

/// A line polyline's distinct world vertices, each with its index in path
/// order. A closed path's repeated closing point is dropped.
fn polyline_distinct_vertices(
    path: &PathData,
    rect: WorldRect,
    rotation_deg: f32,
) -> Vec<(usize, [f32; 2])> {
    let same = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-3;
    let mut pts: Vec<(usize, [f32; 2])> = polyline_world_points(path, rect, rotation_deg)
        .into_iter()
        .enumerate()
        .collect();
    pts.dedup_by(|a, b| same(a.1, b.1));
    if path.closed && pts.len() > 1 && same(pts[0].1, pts[pts.len() - 1].1) {
        pts.pop();
    }
    pts
}

/// Vertices that can carry a corner: all of a closed loop, the inner ones
/// of an open path.
fn polyline_interior(closed: bool, n: usize) -> std::ops::Range<usize> {
    match (n < 3, closed) {
        (true, _) => 0..0,
        (false, true) => 0..n,
        (false, false) => 1..n - 1,
    }
}

/// Per-vertex grips for a line polyline (P1.node.corner-grip): one per
/// turning vertex, keyed by its index in path order. Each rides its
/// vertex's incoming segment, back from the vertex.
pub fn polyline_vertex_grip_edges(node: &Node, chamfer: bool) -> Vec<(usize, CornerGripEdge)> {
    let NodeKind::Shape(s) = &node.kind else {
        return Vec::new();
    };
    let Some(path) = s.path.as_ref().filter(|p| path_is_line_polyline(p)) else {
        return Vec::new();
    };
    let pts = polyline_distinct_vertices(path, node.rect, node.rotation_deg);
    let n = pts.len();
    polyline_interior(path.closed, n)
        .filter_map(|i| {
            let (at, cur) = pts[i];
            let vc = crate::wire::vertex_corner(
                pts[(i + n - 1) % n].1,
                cur,
                pts[(i + 1) % n].1,
                chamfer,
            )?;
            let side = (vc.u_in[0] * vc.u_out[1] - vc.u_in[1] * vc.u_out[0]).signum();
            Some((
                at,
                CornerGripEdge {
                    vertex: cur,
                    dir: [-vc.u_in[0], -vc.u_in[1]],
                    inward: [-vc.u_in[1] * side, vc.u_in[0] * side],
                    per_amount: vc.per_amount,
                    max_travel: vc.max_tangent,
                },
            ))
        })
        .collect()
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

    /// P1.curve.tip-chord: a polyline blends its tips straight while its
    /// corners are sharp or chamfered, and smoothly once they are filleted;
    /// a circular arc stays straight along its sweep.
    #[test]
    fn a_filleted_polyline_blends_its_tips_smoothly() {
        let tip = |width| crate::scene::StrokeSpan {
            width,
            softness: 0.0,
            color: crate::scene::Rgba::opaque(0, 0, 0),
            texture: Default::default(),
        };
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            tips: vec![tip(2.0), tip(10.0), tip(4.0)],
            ..Default::default()
        };
        let stroke = crate::scene::Stroke {
            width: 10.0,
            ..Default::default()
        };
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let width_at = |corner, x: f64| {
            let t = tipped_stroke(&path, &stroke, rect, 0.0, corner).unwrap();
            assert_eq!(t.ease, vector_ink::TipEase::Linear, "{corner:?}");
            let k = t
                .bez
                .elements()
                .iter()
                .filter_map(|el| el.end_point())
                .position(|p| (p - Point::new(x, 0.0)).hypot() < 1e-6);
            k.map(|k| t.widths[k])
        };
        assert_eq!(
            width_at(Corner::Square, 25.0),
            None,
            "sharp: one straight edge"
        );
        assert_eq!(width_at(Corner::Chamfer { cut: 20.0 }, 25.0), None);
        // A 20-unit fillet starts at x = 80, so the edge is cut every 5 units.
        let smooth = 2.0 + 8.0 * vector_ink::TipEase::Smooth.weight(0.25);
        let got = width_at(Corner::Rounded { radius: 20.0 }, 25.0).expect("densified");
        assert!((got - smooth).abs() < 1e-4, "{got} != {smooth}");
        let got = width_at(Corner::Rounded { radius: 20.0 }, 0.0).unwrap();
        assert_eq!(got, 2.0, "a vertex keeps its own");

        let mut arc = BezPath::new();
        arc.move_to((0.0, 0.0));
        arc.extend(
            vector_ink::kurbo::Arc::new(
                (50.0, 0.0),
                (50.0, 50.0),
                std::f64::consts::PI,
                -std::f64::consts::PI,
                0.0,
            )
            .append_iter(0.1),
        );
        assert_eq!(
            tip_ease(&arc),
            vector_ink::TipEase::Linear,
            "an arc tweens along its sweep"
        );
    }

    /// User, 28 September 2026 (tp4): "it works but taper produces kink at
    /// mid fillet". A tapered polyline's width is C1 through each fillet:
    /// no slope jump at its middle, where the corner's width sits, nor at
    /// its tangent points.
    #[test]
    fn a_filleted_taper_has_no_kink_through_the_fillet() {
        use vector_ink::kurbo::{ParamCurve, ParamCurveArclen};
        let tip = |width| crate::scene::StrokeSpan {
            width,
            softness: 0.0,
            color: crate::scene::Rgba::opaque(0, 0, 0),
            texture: Default::default(),
        };
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            tips: vec![tip(2.0), tip(20.0), tip(4.0)],
            ..Default::default()
        };
        let stroke = crate::scene::Stroke {
            width: 20.0,
            ..Default::default()
        };
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let t = tipped_stroke(&path, &stroke, rect, 0.0, Corner::Rounded { radius: 20.0 }).unwrap();
        assert_eq!(
            t.ease,
            vector_ink::TipEase::Linear,
            "the joints carry the blend"
        );
        // The painters blend linearly by arc length between joints.
        let mut joints: Vec<(Point, f64)> = Vec::new();
        let mut s = 0.0;
        for seg in t.bez.segments() {
            if joints.is_empty() {
                joints.push((seg.eval(0.0), 0.0));
            }
            s += seg.arclen(1e-7);
            joints.push((seg.eval(1.0), s));
        }
        assert_eq!(joints.len(), t.widths.len());
        // Slope of the width along the piece ending at joint `k`.
        let slope = |k: usize| {
            (t.widths[k] - t.widths[k - 1]) as f64 / (joints[k].1 - joints[k - 1].1).max(1e-9)
        };
        let at = |p: Point| {
            joints
                .iter()
                .position(|(q, _)| (*q - p).hypot() < 1e-3)
                .unwrap_or_else(|| panic!("{p:?} is a joint"))
        };
        let d = std::f64::consts::FRAC_1_SQRT_2;
        let mid = at(Point::new(80.0 + 20.0 * d, 20.0 - 20.0 * d));
        assert!(
            (t.widths[mid] - 20.0).abs() < 1e-4,
            "the fillet's middle keeps its corner's width: {}",
            t.widths[mid]
        );
        let steepest = (1..joints.len())
            .map(|k| slope(k).abs())
            .fold(0.0, f64::max);
        for (what, k) in [
            ("tangent point in", at(Point::new(80.0, 0.0))),
            ("middle", mid),
            ("tangent point out", at(Point::new(100.0, 20.0))),
        ] {
            let (before, after) = (slope(k), slope(k + 1));
            assert!(
                (after - before).abs() < 0.1 * steepest,
                "{what}: slope {before} then {after} (steepest {steepest})"
            );
        }
    }

    /// A closed polyline's first corner is reached by its closing edge: the
    /// joints of that fillet blend toward the first vertex's width, not
    /// back across the whole path.
    #[test]
    fn a_closed_filleted_taper_keeps_its_first_corner() {
        let tip = |width| crate::scene::StrokeSpan {
            width,
            softness: 0.0,
            color: crate::scene::Rgba::opaque(0, 0, 0),
            texture: Default::default(),
        };
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
                PathSeg::Line { to: [0.0, 1.0] },
            ],
            closed: true,
            tips: vec![tip(20.0), tip(2.0), tip(2.0), tip(2.0)],
            ..Default::default()
        };
        let stroke = crate::scene::Stroke {
            width: 20.0,
            ..Default::default()
        };
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let t = tipped_stroke(&path, &stroke, rect, 0.0, Corner::Rounded { radius: 20.0 }).unwrap();
        let near_first: Vec<f32> = t
            .bez
            .elements()
            .iter()
            .filter_map(|el| el.end_point())
            .zip(&t.widths)
            .filter(|(p, _)| p.x < 20.0 + 1e-6 && p.y < 20.0 + 1e-6)
            .map(|(_, w)| *w)
            .collect();
        assert!(near_first.len() > 4, "the first fillet is cut into joints");
        assert!(
            near_first.iter().all(|w| *w > 15.0),
            "the first corner's fillet stays near its width: {near_first:?}"
        );
    }

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

    /// A polyline fillet keeps its authored radius when its box gets thin:
    /// only each corner's own edges clamp it, never half the short side.
    #[test]
    fn line_polyline_fillet_clamps_per_corner_not_to_the_box() {
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [0.5, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            closed: false,
            ..Default::default()
        };
        let rect = WorldRect::new(0.0, 0.0, 400.0, 40.0);
        let world = [[0.0, 0.0], [200.0, 0.0], [400.0, 40.0]];
        for (corner, chamfer) in [
            (Corner::Rounded { radius: 30.0 }, false),
            (Corner::Chamfer { cut: 30.0 }, true),
        ] {
            let drawn = path_data_to_world_bez_with_fillet(&path, rect, 0.0, corner);
            let want = path_cmds_to_bez_world(&crate::wire::filleted_vertex_path(
                &world, 30.0, chamfer, false,
            ));
            assert_eq!(drawn.elements().len(), want.elements().len(), "{corner:?}");
            for (a, b) in drawn.elements().iter().zip(want.elements()) {
                assert!(
                    format!("{a:?}") == format!("{b:?}")
                        || a.end_point()
                            .zip(b.end_point())
                            .is_some_and(|(p, q)| (p - q).hypot() < 1e-3),
                    "{corner:?}: {a:?} != {b:?}"
                );
            }
        }
        // Percent amounts stay relative to the box, as before.
        let percent = Corner::RoundedPercent { percent: 50.0 };
        let drawn = path_data_to_world_bez_with_fillet(&path, rect, 0.0, percent);
        let want = path_cmds_to_bez_world(&crate::wire::filleted_vertex_path(
            &world, 10.0, false, false,
        ));
        assert_eq!(format!("{drawn:?}"), format!("{want:?}"));
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
                                regular_polygon_world_outline(rect, rot, sides, 0.0, corner, 0.05);
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
                    let outline =
                        regular_polygon_world_outline(rect, 0.0, sides, 0.0, corner, 0.05);
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
        let outline =
            regular_polygon_world_outline(rect, 0.0, 6, 0.0, Corner::Rounded { radius }, 0.01);
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

    fn flat_near(bez: &BezPath, q: [f32; 2]) -> bool {
        flatten_contours(bez, 0.05)
            .iter()
            .flatten()
            .any(|p| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-2)
    }

    /// An open U: (0,0) → (1,0) → (1,1) → (0,1), vertices 0..=3.
    fn open_u() -> PathData {
        PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
                PathSeg::Line { to: [0.0, 1.0] },
            ],
            closed: false,
            ..Default::default()
        }
    }

    #[test]
    fn polyline_vertex_override_sets_one_corner_and_zero_stays_sharp() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let mut path = open_u();
        path.corner_amounts = vec![None, Some(20.0), Some(0.0), None];
        let bez =
            path_data_to_world_bez_with_fillet(&path, rect, 0.0, Corner::Chamfer { cut: 10.0 });
        assert!(
            flat_near(&bez, [80.0, 0.0]) && flat_near(&bez, [100.0, 20.0]),
            "vertex 1 is cut at its override of 20"
        );
        assert!(!flat_near(&bez, [100.0, 0.0]), "vertex 1 is cut away");
        assert!(
            flat_near(&bez, [100.0, 100.0]),
            "vertex 2's override of 0 keeps it sharp"
        );

        // A shared corner of zero still honors an override.
        let mut path = open_u();
        path.corner_amounts = vec![None, Some(15.0), None, None];
        let bez = path_data_to_world_bez_with_fillet(&path, rect, 0.0, Corner::Square);
        assert!(
            flat_near(&bez, [85.0, 0.0]) && flat_near(&bez, [100.0, 15.0]),
            "vertex 1 rounds to radius 15 at its tangent points"
        );
        assert!(
            flat_near(&bez, [100.0, 100.0]),
            "vertex 2 follows the shape"
        );

        // A closed square: vertex 0 is a corner too.
        let square = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
                PathSeg::Line { to: [0.0, 1.0] },
            ],
            closed: true,
            corner_amounts: vec![Some(10.0), None, None, None],
            ..Default::default()
        };
        let bez = path_data_to_world_bez_with_fillet(
            &square,
            rect,
            0.0,
            Corner::ChamferPercent { percent: 0.0 },
        );
        assert!(flat_near(&bez, [10.0, 0.0]) && flat_near(&bez, [0.0, 10.0]));
        assert!(flat_near(&bez, [100.0, 0.0]) && flat_near(&bez, [0.0, 100.0]));

        // A list that does not match the vertices is ignored.
        let mut path = open_u();
        path.corner_amounts = vec![None, Some(20.0)];
        let bez =
            path_data_to_world_bez_with_fillet(&path, rect, 0.0, Corner::Chamfer { cut: 10.0 });
        assert!(flat_near(&bez, [90.0, 0.0]) && flat_near(&bez, [100.0, 90.0]));
    }

    fn polyline_node(path: PathData, rect: WorldRect, corner: Corner) -> Node {
        let mut scene = crate::scene::Scene::default();
        scene.build_node(
            rect,
            NodeKind::Shape(crate::scene::ShapeNode {
                shape: ShapeKind::Line,
                sides: 6,
                phase_deg: 0.0,
                fill: None,
                stroke: crate::scene::Stroke::default(),
                corner,
                flip: false,
                path: Some(std::sync::Arc::new(path)),
                text: None,
            }),
        )
    }

    #[test]
    fn polyline_vertex_grips_ride_the_incoming_segment_at_the_tangent_distance() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let node = polyline_node(open_u(), rect, Corner::Rounded { radius: 10.0 });
        let grips = polyline_vertex_grip_edges(&node, false);
        let at: Vec<usize> = grips.iter().map(|(i, _)| *i).collect();
        assert_eq!(
            at,
            vec![1, 2],
            "one grip per turning vertex; ends stay sharp"
        );
        let (_, e1) = grips[0];
        assert_eq!(e1.vertex, [100.0, 0.0]);
        assert_eq!(e1.dir, [-1.0, 0.0], "vertex 1 rides its incoming segment");
        assert_eq!(e1.inward, [0.0, 1.0]);
        let p = e1.point(e1.travel_for_amount(10.0));
        assert!((p[0] - 90.0).abs() < 1e-4 && p[1].abs() < 1e-4, "{p:?}");
        assert!((e1.max_travel - 50.0).abs() < 1e-4);
        let (_, e2) = grips[1];
        assert_eq!(e2.dir, [0.0, -1.0], "vertex 2 rides its incoming segment");
        let p = e2.point(e2.travel_for_amount(10.0));
        assert!(
            (p[0] - 100.0).abs() < 1e-4 && (p[1] - 90.0).abs() < 1e-4,
            "{p:?}"
        );

        // A 120 degree turn: the tangent distance is r·tan(turn / 2).
        let wedge = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line {
                    to: [0.5, 0.866_025_4],
                },
            ],
            closed: false,
            ..Default::default()
        };
        let node = polyline_node(wedge, rect, Corner::Rounded { radius: 10.0 });
        let grips = polyline_vertex_grip_edges(&node, false);
        assert_eq!(grips.len(), 1);
        let (i, e) = grips[0];
        assert_eq!(i, 1);
        let t = e.travel_for_amount(5.0);
        assert!((t - 5.0 * 60f32.to_radians().tan()).abs() < 1e-3, "{t}");
        let p = e.point(t);
        assert!(
            (p[0] - (100.0 - t)).abs() < 1e-3 && p[1].abs() < 1e-3,
            "{p:?}"
        );

        // Closed: every vertex turns, including the start.
        let mut square = open_u();
        square.closed = true;
        let node = polyline_node(square, rect, Corner::Rounded { radius: 10.0 });
        let at: Vec<usize> = polyline_vertex_grip_edges(&node, true)
            .iter()
            .map(|(i, _)| *i)
            .collect();
        assert_eq!(at, vec![0, 1, 2, 3]);
    }

    #[test]
    fn regular_polygon_default_is_six_sides() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let v = crate::scene::regular_polygon_vertices(rect, 6, 0.0);
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
                phase_deg: 0.0,
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
                phase_deg: 0.0,
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

/// Endpoint of an open path and the unit direction from it into the curve,
/// for an arrowhead of a stroke `stroke_width` wide. The direction aims at
/// the last point of the curve one head length from the tip, so the head's
/// base sits on the curve even where the end bends or hooks. `None` for an
/// empty or degenerate path.
pub fn path_end_arrow(bez: &BezPath, stroke_width: f32) -> Option<([f32; 2], [f32; 2])> {
    use vector_ink::kurbo::{ParamCurve, PathSeg};
    let segs: Vec<PathSeg> = bez.segments().collect();
    let tip = segs.last()?.eval(1.0);
    let need = arrow_len(stroke_width) as f64;
    let mut back = tip;
    for seg in segs.iter().rev() {
        let start = seg.eval(0.0);
        if (start - tip).hypot() < need {
            back = start;
            continue;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if (seg.eval(mid) - tip).hypot() >= need {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        back = seg.eval(lo);
        break;
    }
    let d = back - tip;
    let len = d.hypot();
    if len.is_nan() || len <= 1e-9 {
        return None;
    }
    Some((
        [tip.x as f32, tip.y as f32],
        [(d.x / len) as f32, (d.y / len) as f32],
    ))
}

/// `bez` shortened by `by` along its arc length, the trailing segments that
/// fall inside `by` dropped, plus how many were dropped and the kept
/// fraction of the cut segment's arc length. `None` when there is nothing
/// to trim: a closed or too-short path, or `by <= 0`.
fn trim_end_parts(bez: &BezPath, by: f64) -> Option<(BezPath, usize, f32)> {
    use vector_ink::kurbo::{ParamCurve, ParamCurveArclen, PathEl, PathSeg};
    if by.is_nan()
        || by <= 0.0
        || bez
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::ClosePath))
    {
        return None;
    }
    let segs: Vec<PathSeg> = bez.segments().collect();
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(1e-3)).collect();
    if segs.is_empty() || lens.iter().sum::<f64>() <= by * 1.05 {
        return None;
    }
    let mut left = by;
    let mut cut = segs.len() - 1;
    while lens[cut] <= left && cut > 0 {
        left -= lens[cut];
        cut -= 1;
    }
    let keep = (lens[cut] - left).max(0.0);
    let t = segs[cut].inv_arclen(keep, 1e-3);
    let mut out = BezPath::new();
    let mut seg_i = 0;
    for el in bez.elements() {
        match el {
            PathEl::MoveTo(_) => {
                if seg_i <= cut {
                    out.push(*el);
                }
            }
            _ => {
                if seg_i < cut {
                    out.push(*el);
                } else if seg_i == cut {
                    out.push(segs[cut].subsegment(0.0..t).as_path_el());
                }
                seg_i += 1;
            }
        }
    }
    let frac = if lens[cut] > 1e-9 {
        keep / lens[cut]
    } else {
        0.0
    };
    Some((out, segs.len() - 1 - cut, frac as f32))
}

/// `bez` shortened by `by` along its arc length, so a stroke under an
/// arrowhead ends at the head's base and not at its tip. Segments shorter
/// than what is left to trim are dropped whole.
pub fn trim_end(bez: &BezPath, by: f64) -> BezPath {
    trim_end_parts(bez, by).map_or_else(|| bez.clone(), |(out, _, _)| out)
}

/// [`trim_end`] for a per-vertex stroke: widths and colors of dropped
/// vertices go with them, and the new last vertex takes the tip found at
/// the cut.
pub fn trim_tipped_end(t: &TippedStroke, by: f64) -> TippedStroke {
    use vector_ink::kurbo::PathEl;
    let single = t
        .bez
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count()
        == 1;
    let parts = trim_end_parts(&t.bez, by).filter(|_| single);
    let Some((bez, dropped, frac)) = parts.filter(|_| t.widths.len() >= 2) else {
        return t.clone();
    };
    let n = t.widths.len();
    let cut_end = n - 1 - dropped;
    let keep_to = |v: &[f32]| -> Vec<f32> {
        let mut out = v[..cut_end].to_vec();
        out.push(v[cut_end - 1] + (v[cut_end] - v[cut_end - 1]) * frac);
        out
    };
    let colors = t.colors.as_ref().filter(|c| c.len() == n).map(|c| {
        let mut out = c[..cut_end].to_vec();
        let (a, b) = (c[cut_end - 1], c[cut_end]);
        out.push(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * frac));
        out
    });
    TippedStroke {
        bez,
        widths: keep_to(&t.widths),
        colors,
        ease: t.ease,
    }
}

/// How far a stroke under an arrowhead stops short of the tip: most of the
/// head's length, so the line tucks under the base.
pub fn arrow_trim(stroke_width: f32) -> f64 {
    (arrow_len(stroke_width) * 0.8) as f64
}

/// [`path_end_arrow`] for the path's first point.
pub fn path_start_arrow(bez: &BezPath, stroke_width: f32) -> Option<([f32; 2], [f32; 2])> {
    path_end_arrow(&bez.reverse_subpaths(), stroke_width)
}

/// The tip and inward direction of the head at end `i` (0 = first point,
/// 1 = last) of an open path, for a stroke `stroke_width` wide there.
pub fn path_arrow(bez: &BezPath, i: usize, stroke_width: f32) -> Option<([f32; 2], [f32; 2])> {
    if i == 0 {
        path_start_arrow(bez, stroke_width)
    } else {
        path_end_arrow(bez, stroke_width)
    }
}

/// `bez` shortened under the arrowheads `arrows` (first point, last point)
/// of a stroke `widths` wide at those ends.
pub fn trim_arrow_ends(bez: &BezPath, arrows: [bool; 2], widths: [f32; 2]) -> BezPath {
    let mut out = if arrows[1] {
        trim_end(bez, arrow_trim(widths[1]))
    } else {
        bez.clone()
    };
    if arrows[0] {
        out = trim_end(&out.reverse_subpaths(), arrow_trim(widths[0])).reverse_subpaths();
    }
    out
}

fn reverse_tipped(t: &TippedStroke) -> TippedStroke {
    let rev = |v: &[f32]| v.iter().rev().copied().collect::<Vec<_>>();
    TippedStroke {
        bez: t.bez.reverse_subpaths(),
        widths: rev(&t.widths),
        colors: t.colors.as_ref().map(|c| c.iter().rev().copied().collect()),
        ease: t.ease,
    }
}

/// [`trim_arrow_ends`] for a per-vertex stroke, each head sized to the
/// width at its own end ([`trim_tipped_end`]).
pub fn trim_tipped_arrow_ends(t: &TippedStroke, arrows: [bool; 2]) -> TippedStroke {
    let head = |t: &TippedStroke, first: bool| {
        let w = if first {
            t.widths.first()
        } else {
            t.widths.last()
        };
        arrow_trim(w.copied().unwrap_or(0.0))
    };
    let mut out = if arrows[1] {
        trim_tipped_end(t, head(t, false))
    } else {
        t.clone()
    };
    if arrows[0] {
        let by = head(&out, true);
        out = reverse_tipped(&trim_tipped_end(&reverse_tipped(&out), by));
    }
    out
}

#[cfg(test)]
mod arrow_tests {
    use super::*;
    use vector_ink::kurbo::{ParamCurve, ParamCurveArclen, Point};

    /// A long run, then a tiny hook the way a hand ends a freehand stroke.
    fn hooked() -> BezPath {
        let mut bez = BezPath::new();
        bez.move_to((0.0, 0.0));
        for k in 1..=40 {
            bez.line_to((k as f64 * 5.0, 0.0));
        }
        bez.line_to((200.6, 1.2));
        bez.line_to((200.8, 2.2));
        bez
    }

    #[test]
    fn an_arrow_aims_along_the_curve_not_the_last_hook() {
        let (tip, into) = path_end_arrow(&hooked(), 10.0).unwrap();
        assert_eq!(tip, [200.8, 2.2]);
        assert!(into[0] < -0.99, "points back along the run, got {into:?}");
        let [_, b, c] = arrow_head(tip, into, 10.0);
        let base = [(b[0] + c[0]) * 0.5, (b[1] + c[1]) * 0.5];
        assert!(
            base[1].abs() < 3.0,
            "the base sits on the curve, got {base:?}"
        );
    }

    #[test]
    fn an_arrow_on_an_arc_sets_its_base_on_the_arc() {
        let arc = vector_ink::kurbo::Arc::new(
            Point::new(0.0, 0.0),
            (60.0, 60.0),
            std::f64::consts::PI,
            -std::f64::consts::PI,
            0.0,
        );
        let bez = vector_ink::kurbo::Shape::to_path(&arc, 0.01);
        let (tip, into) = path_end_arrow(&bez, 10.0).unwrap();
        let [_, b, c] = arrow_head(tip, into, 10.0);
        let base = Point::new(((b[0] + c[0]) * 0.5) as f64, ((b[1] + c[1]) * 0.5) as f64);
        let off = (base.to_vec2().hypot() - 60.0).abs();
        assert!(off < 0.5, "base {off} px off the arc");
    }

    #[test]
    fn trimming_crosses_short_segments() {
        let bez = hooked();
        let by = arrow_trim(10.0);
        let trimmed = trim_end(&bez, by);
        let before: f64 = bez.segments().map(|s| s.arclen(1e-3)).sum();
        let after: f64 = trimmed.segments().map(|s| s.arclen(1e-3)).sum();
        assert!(
            (before - after - by).abs() < 1e-2,
            "{before} - {after} vs {by}"
        );
        let end = trimmed.segments().last().unwrap().eval(1.0);
        assert!(
            end.x < 200.0 - by * 0.5,
            "the body ends under the head, at {end:?}"
        );
    }

    #[test]
    fn trimming_a_tipped_stroke_keeps_one_tip_per_vertex() {
        let bez = hooked();
        let n = bez.segments().count() + 1;
        let t = TippedStroke {
            bez,
            widths: (0..n).map(|i| i as f32).collect(),
            colors: Some((0..n).map(|i| [i as f32, 0.0, 0.0, 1.0]).collect()),
            ease: vector_ink::TipEase::default(),
        };
        let cut = trim_tipped_end(&t, arrow_trim(10.0));
        let vertices = cut.bez.segments().count() + 1;
        assert!(vertices < n, "short trailing segments are gone");
        assert_eq!(cut.widths.len(), vertices);
        assert_eq!(cut.colors.as_ref().unwrap().len(), vertices);
        let last = *cut.widths.last().unwrap();
        assert!(last > (vertices - 2) as f32 && last <= (vertices - 1) as f32);
    }
}
