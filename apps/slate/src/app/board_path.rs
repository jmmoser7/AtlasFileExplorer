//! Board vector paths: world ↔ `PathData`, tessellation cache, hit-testing.

use eframe::egui::{self, Color32, Pos2, Shape, Vec2};
use slate_doc::scene::{
    Dash, PathData, PathSeg, Rgba, ShapeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin,
    WidthProfile, WorldRect,
};
use slate_doc::{Node, NodeId, NodeKind};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use vector_ink::kurbo::{self, Arc, BezPath, PathEl, Point};
use vector_ink::{flatten, hit_stroke, stroke_mesh, Cap, InkMesh, Join, StrokeStyle};

use super::board::{rgba32, to_rgba, BoardXf};
use super::SlateApp;

pub(crate) const FEATHER_PX: f32 = 1.25;
/// Screen-px pick slop beyond half the stroke width (D17 / P1.curve.pick).
pub(crate) const PICK_SLOP_PX: f32 = 4.0;
const MIN_PATH_BOUNDS: f32 = 8.0;
const CACHE_CAP: usize = 256;

/// In-progress multi-click / freehand path gestures.
#[derive(Clone, Debug)]
pub enum BoardPathDraft {
    Polyline {
        points: Vec<Pos2>,
    },
    Arc {
        points: Vec<Pos2>,
    },
    Bezier {
        anchors: Vec<(Pos2, Vec2)>,
        /// Active click-drag placing an anchor + handle.
        placing: Option<(Pos2, Vec2)>,
    },
}

#[derive(Clone, Default)]
pub(crate) struct CachedInkMesh {
    vertices: Vec<[f32; 2]>,
    alphas: Vec<f32>,
    indices: Vec<u32>,
}

#[derive(Default)]
pub struct PathMeshCache {
    map: HashMap<(NodeId, u64), CachedInkMesh>,
}

impl PathMeshCache {
    pub(crate) fn get_or_tessellate(
        &mut self,
        node_id: NodeId,
        key: u64,
        build: impl FnOnce() -> InkMesh,
    ) -> CachedInkMesh {
        if self.map.len() > CACHE_CAP {
            self.map.clear();
        }
        if let Some(c) = self.map.get(&(node_id, key)) {
            return c.clone();
        }
        let ink = build();
        let cached = CachedInkMesh {
            vertices: ink.vertices.iter().map(|v| v.pos).collect(),
            alphas: ink.vertices.iter().map(|v| v.alpha).collect(),
            indices: ink.indices,
        };
        self.map.insert((node_id, key), cached.clone());
        cached
    }
}

pub fn zoom_bucket(z: f32) -> i64 {
    (z * 8.0).round() as i64
}

fn to_k(p: Pos2) -> Point {
    Point::new(p.x as f64, p.y as f64)
}

fn from_k(p: Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

fn denorm(p: [f32; 2], rect: WorldRect) -> Pos2 {
    Pos2::new(rect.x + p[0] * rect.w, rect.y + p[1] * rect.h)
}

fn norm(p: Pos2, rect: WorldRect) -> [f32; 2] {
    let w = rect.w.max(1e-6);
    let h = rect.h.max(1e-6);
    [
        ((p.x - rect.x) / w).clamp(0.0, 1.0),
        ((p.y - rect.y) / h).clamp(0.0, 1.0),
    ]
}

fn rotate_world(p: Pos2, rect: WorldRect, deg: f32) -> Pos2 {
    if deg.abs() <= 0.01 {
        return p;
    }
    let (cx, cy) = rect.center();
    let rad = deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = p.x - cx;
    let dy = p.y - cy;
    Pos2::new(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
}

pub fn path_data_to_world_bez(path: &PathData, rect: WorldRect, rotation_deg: f32) -> BezPath {
    let mut bez = BezPath::new();
    let start = rotate_world(denorm(path.start, rect), rect, rotation_deg);
    bez.move_to(to_k(start));
    for seg in &path.segs {
        match *seg {
            PathSeg::Line { to } => {
                bez.line_to(to_k(rotate_world(denorm(to, rect), rect, rotation_deg)));
            }
            PathSeg::Quad { ctrl, to } => {
                bez.quad_to(
                    to_k(rotate_world(denorm(ctrl, rect), rect, rotation_deg)),
                    to_k(rotate_world(denorm(to, rect), rect, rotation_deg)),
                );
            }
            PathSeg::Cubic { c1, c2, to } => {
                bez.curve_to(
                    to_k(rotate_world(denorm(c1, rect), rect, rotation_deg)),
                    to_k(rotate_world(denorm(c2, rect), rect, rotation_deg)),
                    to_k(rotate_world(denorm(to, rect), rect, rotation_deg)),
                );
            }
        }
    }
    if path.closed {
        bez.close_path();
    }
    bez
}

pub fn bounds_of_world_points(pts: &[Pos2]) -> WorldRect {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for p in pts {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let mut w = (max_x - min_x).max(0.0);
    let mut h = (max_y - min_y).max(0.0);
    if w < 1.0 {
        w = 1.0;
        min_x -= 0.5;
    }
    if h < 1.0 {
        h = 1.0;
        min_y -= 0.5;
    }
    WorldRect::new(min_x, min_y, w, h)
}

pub fn bezpath_to_path_data(bez: &BezPath, closed: bool) -> (WorldRect, PathData) {
    let mut pts = Vec::new();
    let mut start = Pos2::ZERO;
    let mut have_start = false;
    for el in bez.elements() {
        match el {
            PathEl::MoveTo(p) => {
                start = from_k(*p);
                if !have_start {
                    pts.push(start);
                    have_start = true;
                }
            }
            PathEl::LineTo(p) => pts.push(from_k(*p)),
            PathEl::QuadTo(p1, p2) => {
                pts.push(from_k(*p1));
                pts.push(from_k(*p2));
            }
            PathEl::CurveTo(p1, p2, p3) => {
                pts.push(from_k(*p1));
                pts.push(from_k(*p2));
                pts.push(from_k(*p3));
            }
            PathEl::ClosePath => {}
        }
    }
    if !have_start {
        return (
            WorldRect::new(0.0, 0.0, MIN_PATH_BOUNDS, MIN_PATH_BOUNDS),
            PathData {
                start: [0.0, 0.0],
                segs: vec![],
                closed,
            },
        );
    }
    let rect = bounds_of_world_points(&pts);
    let start_n = norm(start, rect);
    let mut segs = Vec::new();
    let mut cur = start;
    for el in bez.elements() {
        match el {
            PathEl::MoveTo(p) => cur = from_k(*p),
            PathEl::LineTo(p) => {
                let to = norm(from_k(*p), rect);
                segs.push(PathSeg::Line { to });
                cur = from_k(*p);
            }
            PathEl::QuadTo(p1, p2) => {
                segs.push(PathSeg::Quad {
                    ctrl: norm(from_k(*p1), rect),
                    to: norm(from_k(*p2), rect),
                });
                cur = from_k(*p2);
            }
            PathEl::CurveTo(p1, p2, p3) => {
                segs.push(PathSeg::Cubic {
                    c1: norm(from_k(*p1), rect),
                    c2: norm(from_k(*p2), rect),
                    to: norm(from_k(*p3), rect),
                });
                cur = from_k(*p3);
            }
            PathEl::ClosePath => {}
        }
    }
    let _ = cur;
    (
        rect,
        PathData {
            start: start_n,
            segs,
            closed,
        },
    )
}

pub fn points_to_path_data(points: &[Pos2], closed: bool) -> (WorldRect, PathData) {
    if points.len() < 2 {
        let r = bounds_of_world_points(points);
        return (
            r,
            PathData {
                start: if points.is_empty() {
                    [0.0, 0.0]
                } else {
                    norm(points[0], r)
                },
                segs: vec![],
                closed,
            },
        );
    }
    let mut bez = BezPath::new();
    bez.move_to(to_k(points[0]));
    for p in &points[1..] {
        bez.line_to(to_k(*p));
    }
    if closed {
        bez.close_path();
    }
    bezpath_to_path_data(&bez, closed)
}

pub fn cap_join_profile(stroke: &Stroke) -> (Cap, Join, Option<(f32, f32)>) {
    let cap = match stroke.cap {
        StrokeCap::Butt => Cap::Butt,
        StrokeCap::Round => Cap::Round,
        StrokeCap::Square => Cap::Square,
    };
    let join = match stroke.join {
        StrokeJoin::Miter => Join::Miter,
        StrokeJoin::Round => Join::Round,
        StrokeJoin::Bevel => Join::Bevel,
    };
    let taper = match stroke.profile {
        WidthProfile::Uniform => None,
        WidthProfile::Taper { start, end } => Some((start, end)),
    };
    (cap, join, taper)
}

pub fn stroke_style_world(stroke: &Stroke, zoom: f32) -> StrokeStyle {
    let (cap, join, taper) = cap_join_profile(stroke);
    let w = stroke.width.max(0.0);
    let dash = match stroke.dash {
        Dash::Solid => None,
        Dash::Dashed => Some((vec![12.0, 8.0], 0.0)),
        Dash::Dotted => {
            let on = (w * 1.2).max(2.0 / zoom.max(0.05));
            let off = (w * 2.2).max(4.0 / zoom.max(0.05));
            Some((vec![on, off], 0.0))
        }
    };
    StrokeStyle {
        width: w,
        cap,
        join,
        taper,
        dash,
    }
}

fn path_content_hash(path: &PathData, stroke: &Stroke, rect: WorldRect, bucket: i64) -> u64 {
    let mut h = DefaultHasher::new();
    format!("{path:?}{stroke:?}{rect:?}{bucket}").hash(&mut h);
    h.finish()
}

pub(crate) fn ink_mesh_to_epaint(
    cached: &CachedInkMesh,
    xf: &BoardXf,
    base_color: Color32,
    fade: impl Fn(Color32) -> Color32,
) -> egui::Mesh {
    use egui::epaint::{Vertex, WHITE_UV};
    let mut mesh = egui::Mesh::default();
    for (pos, alpha) in cached.vertices.iter().zip(cached.alphas.iter()) {
        let sp = xf.w2s(Pos2::new(pos[0], pos[1]));
        let c = fade(base_color.gamma_multiply(*alpha));
        mesh.vertices.push(Vertex {
            pos: sp,
            uv: WHITE_UV,
            color: c,
        });
    }
    mesh.indices = cached.indices.clone();
    mesh
}

fn point_in_polygon(x: f32, y: f32, poly: &[[f32; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi).max(1e-12) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Screen-consistent pick slop in world units (~4 px at the current zoom).
pub fn pick_slop_world(zoom: f32) -> f32 {
    PICK_SLOP_PX / zoom.max(0.05)
}

/// World endpoints of a legacy bbox line (`ShapeKind::Line` + `flip`) or a
/// parametric two-point path (delegates to `board_line::line_endpoints`).
pub fn open_curve_endpoints(node: &Node, shape: &ShapeNode) -> Option<(Pos2, Pos2)> {
    if let Some(ep) = super::board_line::line_endpoints(node) {
        return Some(ep);
    }
    if shape.shape != ShapeKind::Line {
        return None;
    }
    let (a, b) = if shape.flip {
        (
            Pos2::new(node.rect.x, node.rect.y + node.rect.h),
            Pos2::new(node.rect.x + node.rect.w, node.rect.y),
        )
    } else {
        (
            Pos2::new(node.rect.x, node.rect.y),
            Pos2::new(node.rect.x + node.rect.w, node.rect.y + node.rect.h),
        )
    };
    Some((
        rotate_world(a, node.rect, node.rotation_deg),
        rotate_world(b, node.rect, node.rotation_deg),
    ))
}

/// Open curves (simple lines, legacy lines, open paths) pick on the stroke
/// only — never on the node AABB (P1.curve.pick).
pub fn shape_uses_stroke_pick(node: &Node, shape: &ShapeNode) -> bool {
    if open_curve_endpoints(node, shape).is_some() {
        return true;
    }
    if shape.shape == ShapeKind::Path {
        if let Some(path) = &shape.path {
            return !path.closed && !path.is_empty();
        }
    }
    false
}

fn bez_from_open_curve(node: &Node, shape: &ShapeNode) -> Option<BezPath> {
    if let Some(path) = shape.path.as_ref() {
        if !path.is_empty() {
            return Some(path_data_to_world_bez(path, node.rect, node.rotation_deg));
        }
    }
    let (a, b) = open_curve_endpoints(node, shape)?;
    let mut bez = BezPath::new();
    bez.move_to(to_k(a));
    bez.line_to(to_k(b));
    Some(bez)
}

/// Stroke-precise point pick for any shape node (open or closed path).
pub fn hit_shape_stroke(node: &Node, shape: &ShapeNode, wx: f32, wy: f32, zoom: f32) -> bool {
    let Some(bez) = bez_from_open_curve(node, shape).or_else(|| {
        shape.path.as_ref().and_then(|path| {
            (!path.is_empty()).then(|| path_data_to_world_bez(path, node.rect, node.rotation_deg))
        })
    }) else {
        return false;
    };
    let style = stroke_style_world(&shape.stroke, zoom);
    let slop = pick_slop_world(zoom);
    if !shape.stroke.is_none() && hit_stroke(&bez, &style, [wx, wy], slop) {
        return true;
    }
    false
}

pub fn hit_path_node(node: &Node, shape: &ShapeNode, wx: f32, wy: f32, zoom: f32) -> bool {
    if shape_uses_stroke_pick(node, shape) {
        return hit_shape_stroke(node, shape, wx, wy, zoom);
    }
    let Some(path) = shape.path.as_ref() else {
        return node.rect.contains_rotated(wx, wy, node.rotation_deg);
    };
    if path.is_empty() {
        return node.rect.contains_rotated(wx, wy, node.rotation_deg);
    }
    let bez = path_data_to_world_bez(path, node.rect, node.rotation_deg);
    let style = stroke_style_world(&shape.stroke, zoom);
    let slop = pick_slop_world(zoom);
    if !shape.stroke.is_none() && hit_stroke(&bez, &style, [wx, wy], slop) {
        return true;
    }
    if path.closed {
        if let Some(fill) = shape.fill {
            if fill.0[3] > 0 {
                let flat = flatten(&bez, 0.25);
                if flat.len() >= 3 && point_in_polygon(wx, wy, &flat) {
                    return true;
                }
            }
        }
    }
    false
}

fn segment_intersects_rect(a: Pos2, b: Pos2, r: WorldRect) -> bool {
    if r.contains(a.x, a.y) || r.contains(b.x, b.y) {
        return true;
    }
    let edges = [
        (
            (r.x, r.y),
            (r.x + r.w, r.y),
        ),
        (
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h),
        ),
        (
            (r.x + r.w, r.y + r.h),
            (r.x, r.y + r.h),
        ),
        (
            (r.x, r.y + r.h),
            (r.x, r.y),
        ),
    ];
    edges
        .iter()
        .any(|(p1, p2)| super::board_snap::segments_intersect(
            (a.x, a.y),
            (b.x, b.y),
            *p1,
            *p2,
        ))
}

/// Marquee selection for board nodes. Open curves intersect on stroke
/// geometry (centerline vs rect), never the node AABB alone (P1.curve.pick).
pub fn marquee_hits_node(node: &Node, marquee: WorldRect, zoom: f32) -> bool {
    match &node.kind {
        NodeKind::Connector(_) => {
            node.rect.x >= marquee.x
                && node.rect.y >= marquee.y
                && node.rect.x + node.rect.w <= marquee.x + marquee.w
                && node.rect.y + node.rect.h <= marquee.y + marquee.h
        }
        NodeKind::Shape(s) => {
            if shape_uses_stroke_pick(node, s) {
                if let Some((a, b)) = open_curve_endpoints(node, s) {
                    if segment_intersects_rect(a, b, marquee) {
                        return true;
                    }
                }
                if let Some(bez) = bez_from_open_curve(node, s) {
                    let flat = flatten(&bez, 0.25);
                    for w in flat.windows(2) {
                        let a = Pos2::new(w[0][0] as f32, w[0][1] as f32);
                        let b = Pos2::new(w[1][0] as f32, w[1][1] as f32);
                        if segment_intersects_rect(a, b, marquee) {
                            return true;
                        }
                    }
                }
                let cx = marquee.x + marquee.w * 0.5;
                let cy = marquee.y + marquee.h * 0.5;
                return hit_shape_stroke(node, s, cx, cy, zoom);
            }
            if hit_path_node(node, s, marquee.x + marquee.w * 0.5, marquee.y + marquee.h * 0.5, zoom)
            {
                return true;
            }
            super::board_snap::marquee_intersects_rotated(marquee, node.rect, node.rotation_deg)
        }
        _ => super::board_snap::marquee_intersects_rotated(marquee, node.rect, node.rotation_deg),
    }
}

pub fn board_pick_node(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
) -> Option<NodeId> {
    board_pick_node_ex(scene, wx, wy, zoom, false)
}

/// Point pick honoring the scene flags: hidden nodes are never hit; locked
/// nodes only when `include_locked` (the Ctrl+Shift+click escape hatch and
/// eyedropper sampling). Connectors hit on their stroke (8 px pick width),
/// never on their AABB.
pub fn board_pick_node_ex(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
    include_locked: bool,
) -> Option<NodeId> {
    for n in scene.nodes.iter().rev() {
        if n.hidden || (n.locked && !include_locked) {
            continue;
        }
        if n.is_frame() {
            continue;
        }
        match &n.kind {
            NodeKind::Connector(c) => {
                if super::board_wire::hit_connector(scene, c, wx, wy, zoom) {
                    return Some(n.id);
                }
                continue;
            }
            NodeKind::Shape(s) => {
                if shape_uses_stroke_pick(n, s) {
                    if hit_shape_stroke(n, s, wx, wy, zoom) {
                        return Some(n.id);
                    }
                    continue;
                }
                if s.shape == ShapeKind::Path {
                    if hit_path_node(n, s, wx, wy, zoom) {
                        return Some(n.id);
                    }
                    continue;
                }
            }
            _ => {}
        }
        if n.rect.contains_rotated(wx, wy, n.rotation_deg) {
            return Some(n.id);
        }
    }
    scene
        .nodes
        .iter()
        .rev()
        .find(|n| {
            n.is_frame() && !n.hidden && (include_locked || !n.locked) && n.rect.contains(wx, wy)
        })
        .map(|n| n.id)
}

pub fn default_draw_stroke(accent: Rgba) -> Stroke {
    Stroke {
        width: 2.0,
        color: accent,
        dash: Dash::Solid,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        profile: WidthProfile::Uniform,
    }
}

/// Draft-curve default (Line, arc, polyline, …): square end caps, miter
/// joins — distinct from expressive ink (`default_draw_stroke`, round).
pub fn default_curve_stroke(color: Rgba) -> Stroke {
    Stroke {
        width: 2.0,
        color,
        dash: Dash::Solid,
        cap: StrokeCap::Square,
        join: StrokeJoin::Miter,
        profile: WidthProfile::Uniform,
    }
}

pub fn arc_through_three_points(p0: Pos2, p1: Pos2, p2: Pos2) -> BezPath {
    let mut path = BezPath::new();
    let a = to_k(p0);
    let b = to_k(p1);
    let c = to_k(p2);
    let d = 2.0_f64 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-4 {
        path.move_to(a);
        path.line_to(c);
        return path;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let center = Point::new(ux, uy);
    let r = ((a.x - ux).powi(2) + (a.y - uy).powi(2)).sqrt();
    if r < 1e-6 {
        path.move_to(a);
        path.line_to(c);
        return path;
    }
    let ang = |p: Point| (p.y - uy).atan2(p.x - ux);
    let a0 = ang(a);
    let a1 = ang(b);
    let a2_end = ang(c);
    let mut sweep = a2_end - a0;
    while sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
    }
    while sweep > std::f64::consts::TAU {
        sweep -= std::f64::consts::TAU;
    }
    let mut mid = a1 - a0;
    while mid < 0.0 {
        mid += std::f64::consts::TAU;
    }
    if mid > sweep {
        sweep -= std::f64::consts::TAU;
        while sweep <= 0.0 {
            sweep += std::f64::consts::TAU;
        }
    }
    let sweep_angle = sweep;
    let arc = Arc::new(center, kurbo::Vec2::new(r, r), a0, sweep_angle, 0.0);
    path.move_to(a);
    for el in arc.append_iter(0.25) {
        match el {
            PathEl::CurveTo(c1, c2, end) => path.curve_to(c1, c2, end),
            PathEl::LineTo(p) => path.line_to(p),
            _ => {}
        }
    }
    path
}

pub fn bezier_anchors_to_bezpath(anchors: &[(Pos2, Vec2)]) -> BezPath {
    let mut path = BezPath::new();
    if anchors.is_empty() {
        return path;
    }
    path.move_to(to_k(anchors[0].0));
    for i in 0..anchors.len().saturating_sub(1) {
        let (a0, out0) = anchors[i];
        let (a1, out1) = anchors[i + 1];
        let c1 = a0 + out0;
        let c2 = a1 - out1;
        path.curve_to(to_k(c1), to_k(c2), to_k(a1));
    }
    path
}

pub fn paint_path_shape(
    app: &mut SlateApp,
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    fade: &impl Fn(Color32) -> Color32,
) {
    if path.is_empty() && !path.closed {
        return;
    }
    let bez = path_data_to_world_bez(path, node.rect, node.rotation_deg);
    if shape.fill.is_some() && path.closed {
        let flat = flatten(&bez, 0.25);
        if flat.len() >= 3 {
            let pts: Vec<Pos2> = flat
                .iter()
                .map(|[x, y]| xf.w2s(Pos2::new(*x, *y)))
                .collect();
            if let Some(fill) = shape.fill {
                painter.add(Shape::Path(egui::epaint::PathShape {
                    points: pts,
                    closed: true,
                    fill: fade(rgba32(fill)),
                    stroke: egui::epaint::PathStroke::NONE,
                }));
            }
        }
    }
    if shape.stroke.is_none() {
        return;
    }
    let bucket = zoom_bucket(xf.z);
    let key = path_content_hash(path, &shape.stroke, node.rect, bucket);
    let style = stroke_style_world(&shape.stroke, xf.z);
    let feather = FEATHER_PX / xf.z.max(0.05);
    let cached = app
        .path_mesh_cache
        .get_or_tessellate(node.id, key, || stroke_mesh(&bez, &style, feather, 0.25));
    let base = fade(rgba32(shape.stroke.color));
    let mesh = ink_mesh_to_epaint(&cached, xf, base, fade);
    painter.add(Shape::mesh(mesh));
}

pub fn paint_path_preview(painter: &egui::Painter, xf: &BoardXf, color: Color32, bez: &BezPath) {
    let style = StrokeStyle {
        width: 2.0_f32.max(1.0 / xf.z.max(0.05_f32)),
        cap: Cap::Round,
        join: Join::Round,
        taper: None,
        dash: None,
    };
    let feather = FEATHER_PX / xf.z.max(0.05);
    let ink = stroke_mesh(bez, &style, feather, 0.25);
    use egui::epaint::{Vertex, WHITE_UV};
    let mut mesh = egui::Mesh::default();
    for v in &ink.vertices {
        mesh.vertices.push(Vertex {
            pos: xf.w2s(Pos2::new(v.pos[0], v.pos[1])),
            uv: WHITE_UV,
            color,
        });
    }
    mesh.indices = ink.indices;
    painter.add(Shape::mesh(mesh));
}

pub fn paint_polyline_preview(
    painter: &egui::Painter,
    xf: &BoardXf,
    pts: &[Pos2],
    cursor: Pos2,
    color: Color32,
) {
    if pts.is_empty() {
        return;
    }
    let mut all = pts.to_vec();
    all.push(cursor);
    if all.len() < 2 {
        return;
    }
    let mut bez = BezPath::new();
    bez.move_to(to_k(all[0]));
    for p in &all[1..] {
        bez.line_to(to_k(*p));
    }
    paint_path_preview(painter, xf, color, &bez);
}

pub fn paint_path_draft(
    painter: &egui::Painter,
    xf: &BoardXf,
    draft: &BoardPathDraft,
    cursor: Option<Pos2>,
    color: Color32,
) {
    match draft {
        BoardPathDraft::Polyline { points } => {
            if let Some(c) = cursor {
                paint_polyline_preview(painter, xf, points, c, color);
            } else if points.len() >= 2 {
                let (r, _) = points_to_path_data(points, false);
                let mut bez = BezPath::new();
                bez.move_to(to_k(points[0]));
                for p in &points[1..] {
                    bez.line_to(to_k(*p));
                }
                let _ = r;
                paint_path_preview(painter, xf, color, &bez);
            }
        }
        BoardPathDraft::Arc { points } => {
            let mut pts = points.clone();
            if let Some(c) = cursor {
                pts.push(c);
            }
            if pts.len() >= 2 {
                let mut bez = BezPath::new();
                bez.move_to(to_k(pts[0]));
                for p in &pts[1..] {
                    bez.line_to(to_k(*p));
                }
                paint_path_preview(painter, xf, color, &bez);
            }
            if pts.len() >= 3 {
                let bez = arc_through_three_points(pts[0], pts[1], pts[2]);
                paint_path_preview(painter, xf, color, &bez);
            }
        }
        BoardPathDraft::Bezier { anchors, placing } => {
            let mut preview = anchors.clone();
            if let Some((a, h)) = placing {
                preview.push((*a, *h));
            }
            if preview.len() >= 2 {
                let bez = bezier_anchors_to_bezpath(&preview);
                paint_path_preview(painter, xf, color, &bez);
            } else if let Some((a, h)) = placing {
                let mut bez = BezPath::new();
                bez.move_to(to_k(*a));
                if h.length_sq() > 1.0 {
                    bez.line_to(to_k(*a + *h));
                }
                paint_path_preview(painter, xf, color, &bez);
            }
        }
    }
}

impl SlateApp {
    pub(crate) fn cancel_path_draft(&mut self) {
        self.board_path_draft = None;
    }

    pub(crate) fn finish_path_draft(&mut self) -> bool {
        let Some(draft) = self.board_path_draft.take() else {
            return false;
        };
        let accent = {
            let p = self.palette();
            to_rgba(p.accent)
        };
        let (rect, path_data, closed) = match draft {
            BoardPathDraft::Polyline { points } => {
                if points.len() < 2 {
                    return false;
                }
                let (r, d) = points_to_path_data(&points, false);
                (r, d, false)
            }
            BoardPathDraft::Bezier {
                anchors,
                placing: _,
            } => {
                if anchors.len() < 2 {
                    return false;
                }
                let bez = bezier_anchors_to_bezpath(&anchors);
                let (r, d) = bezpath_to_path_data(&bez, false);
                (r, d, false)
            }
            BoardPathDraft::Arc { .. } => return false,
        };
        if path_data.is_empty() {
            return false;
        }
        self.commit_path_node(rect, path_data, closed, accent);
        true
    }

    pub(crate) fn commit_path_node(
        &mut self,
        rect: WorldRect,
        path_data: PathData,
        closed: bool,
        accent: Rgba,
    ) {
        let mut path_data = path_data;
        path_data.closed = closed;
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_draw_stroke(accent),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(path_data),
            }),
        );
        let ids = self.add_nodes(vec![node]);
        self.board_sel = ids.into_iter().collect();
        self.board_tool = super::board::BoardTool::Select;
    }

    pub(crate) fn path_tool_click(&mut self, world: Pos2) {
        // Ortho (F8, Shift inverts): draft segments snap to 45° from the
        // last anchor (constraints spec §1).
        let world = if super::board_snap::effective_ortho(self.board_ortho, self.shift_down) {
            match &self.board_path_draft {
                Some(BoardPathDraft::Polyline { points }) if !points.is_empty() => {
                    super::board_snap::ortho_snap_point(*points.last().unwrap(), world)
                }
                _ => world,
            }
        } else {
            world
        };
        match self.board_tool {
            super::board::BoardTool::Polyline => {
                if let Some(BoardPathDraft::Polyline { points }) = &mut self.board_path_draft {
                    points.push(world);
                } else {
                    self.board_path_draft = Some(BoardPathDraft::Polyline {
                        points: vec![world],
                    });
                }
            }
            super::board::BoardTool::Arc => {
                let mut pts = match self.board_path_draft.take() {
                    Some(BoardPathDraft::Arc { points }) => points,
                    _ => Vec::new(),
                };
                pts.push(world);
                if pts.len() >= 3 {
                    let bez = arc_through_three_points(pts[0], pts[1], pts[2]);
                    let (rect, data) = bezpath_to_path_data(&bez, false);
                    let accent = to_rgba(self.palette().accent);
                    self.commit_path_node(rect, data, false, accent);
                    return;
                }
                self.board_path_draft = Some(BoardPathDraft::Arc { points: pts });
            }
            _ => {}
        }
    }

    pub(crate) fn bezier_anchor_release(&mut self, press: Pos2, world: Pos2) {
        let out = world - press;
        let handle = if out.length_sq() > 4.0 {
            out
        } else {
            Vec2::ZERO
        };
        match &mut self.board_path_draft {
            Some(BoardPathDraft::Bezier { anchors, placing }) => {
                anchors.push((press, handle));
                *placing = None;
            }
            _ => {
                self.board_path_draft = Some(BoardPathDraft::Bezier {
                    anchors: vec![(press, handle)],
                    placing: None,
                });
            }
        }
    }

    pub(crate) fn bezier_anchor_move(&mut self, press: Pos2, world: Pos2) {
        let out = world - press;
        match &mut self.board_path_draft {
            Some(BoardPathDraft::Bezier { placing, .. }) => {
                *placing = Some((press, out));
            }
            _ => {
                self.board_path_draft = Some(BoardPathDraft::Bezier {
                    anchors: vec![],
                    placing: Some((press, out)),
                });
            }
        }
    }

    pub(crate) fn finish_freehand_pen(&mut self, points: Vec<Pos2>) {
        if points.len() < 2 {
            return;
        }
        let tol = 1.0 / self.tab().cam.z.max(0.05);
        let flat: Vec<[f32; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
        let bez = vector_ink::fit_polyline(&flat, tol);
        let (rect, data) = bezpath_to_path_data(&bez, false);
        if data.is_empty() {
            return;
        }
        let accent = to_rgba(self.palette().accent);
        self.commit_path_node(rect, data, false, accent);
    }

    pub(crate) fn path_tool_try_finish(&mut self) -> bool {
        if matches!(
            self.board_tool,
            super::board::BoardTool::Polyline | super::board::BoardTool::BezierSpan
        ) {
            return self.finish_path_draft();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_world_points() {
        let pts = vec![
            Pos2::new(10.0, 20.0),
            Pos2::new(100.0, 40.0),
            Pos2::new(120.0, 90.0),
        ];
        let (rect, data) = points_to_path_data(&pts, false);
        let bez = path_data_to_world_bez(&data, rect, 0.0);
        let flat = flatten(&bez, 0.1);
        assert!((flat[0][0] - pts[0].x).abs() < 0.01);
        assert!((flat[0][1] - pts[0].y).abs() < 0.01);
        let last = flat.last().unwrap();
        assert!((last[0] - pts.last().unwrap().x).abs() < 0.01);
        assert!((last[1] - pts.last().unwrap().y).abs() < 0.01);
    }

    #[test]
    fn arc_approximates_circle() {
        let r = 50.0f32;
        let c = Pos2::new(0.0, 0.0);
        let p0 = Pos2::new(c.x + r, c.y);
        let p1 = Pos2::new(c.x, c.y + r);
        let p2 = Pos2::new(c.x - r, c.y);
        let bez = arc_through_three_points(p0, p1, p2);
        let flat = flatten(&bez, 0.05);
        for f in &flat {
            let d = (f[0] * f[0] + f[1] * f[1]).sqrt();
            assert!((d - r).abs() < 2.0, "radius error {d}");
        }
    }

    #[test]
    fn hit_stroke_path_node() {
        let pts = vec![Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0)];
        let (rect, data) = points_to_path_data(&pts, false);
        let node = Node {
            id: NodeId(1),
            rect,
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_draw_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),
            }),
        };
        let shape = match &node.kind {
            NodeKind::Shape(s) => s,
            _ => unreachable!(),
        };
        assert!(hit_path_node(&node, shape, 100.0, 0.0, 1.0));
        assert!(!hit_path_node(&node, shape, 100.0, 50.0, 1.0));
    }

    #[test]
    fn diagonal_line_picks_stroke_not_bbox_interior() {
        let pts = vec![Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0)];
        let (rect, data) = points_to_path_data(&pts, false);
        let node = Node {
            id: NodeId(2),
            rect,
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_curve_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),
            }),
        };
        let shape = match &node.kind {
            NodeKind::Shape(s) => s,
            _ => unreachable!(),
        };
        assert!(hit_path_node(&node, shape, 50.0, 50.0, 1.0));
        assert!(
            !hit_path_node(&node, shape, 50.0, 10.0, 1.0),
            "bbox interior off the stroke must not hit"
        );
        assert!(
            !node.rect.contains(50.0, 10.0) || !hit_path_node(&node, shape, 50.0, 10.0, 1.0)
        );
        let marquee = WorldRect::new(40.0, 5.0, 20.0, 10.0);
        assert!(
            !marquee_hits_node(&node, marquee, 1.0),
            "marquee wholly off stroke must not select"
        );
        let stroke_marquee = WorldRect::new(45.0, 45.0, 10.0, 10.0);
        assert!(marquee_hits_node(&node, stroke_marquee, 1.0));
    }

    #[test]
    fn legacy_line_shape_uses_stroke_pick() {
        let node = Node {
            id: NodeId(3),
            rect: WorldRect::new(0.0, 0.0, 100.0, 100.0),
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Line,
                fill: None,
                stroke: default_curve_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: None,
            }),
        };
        let shape = match &node.kind {
            NodeKind::Shape(s) => s,
            _ => unreachable!(),
        };
        assert!(shape_uses_stroke_pick(&node, shape));
        assert!(hit_shape_stroke(&node, shape, 50.0, 50.0, 1.0));
        assert!(!hit_shape_stroke(&node, shape, 50.0, 10.0, 1.0));
    }
}
