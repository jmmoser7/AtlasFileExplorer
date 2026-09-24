//! Board vector paths: world ↔ `PathData`, tessellation cache, hit-testing.

#[path = "board_path/tiles.rs"]
pub(crate) mod tiles;

use eframe::egui::{self, Color32, Pos2, Shape, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    Dash, PathData, PathSeg, Rgba, ShapeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin, StrokeSpan,
    WidthProfile, WorldRect,
};
use slate_doc::{Node, NodeId, NodeKind};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc as Shared;
use vector_ink::kurbo::{self, Arc, BezPath, PathEl, Point};
use vector_ink::{
    flatten, flatten_contours, hit_stroke, stamp_segment, stamp_tipped, stroke_mesh,
    tipped_contours, Cap, InkMesh, Join, StampStyle, StrokeStyle, TipPoint,
};

use super::board::{rgba32, BoardXf};
use super::SlateApp;

pub(crate) const FEATHER_PX: f32 = 1.25;
/// Screen-px pick slop beyond half the stroke width (D17 / P1.curve.pick).
pub(crate) const PICK_SLOP_PX: f32 = 4.0;
const MIN_PATH_BOUNDS: f32 = 8.0;
// Bound geometry by its cost, not by the number of visible paths. A 256-entry
// FIFO misses on every frame of a 257-path board. The entry limit separately
// bounds metadata, including empty meshes, while allowing 10k stroke+fill pairs.
const CACHE_BYTES: usize = 64 * 1024 * 1024;
const CACHE_ENTRIES: usize = 32_768;

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

#[derive(Default)]
pub(crate) struct CachedInkMesh {
    vertices: Vec<[f32; 2]>,
    alphas: Vec<f32>,
    indices: Vec<u32>,
}

type FillTriangles = (Vec<[f32; 2]>, Vec<u32>);

#[derive(Clone)]
enum CachedGeometry {
    Stroke(Shared<CachedInkMesh>),
    Fill(Shared<FillTriangles>),
}

impl CachedGeometry {
    fn bytes(&self) -> usize {
        match self {
            Self::Stroke(mesh) => {
                mesh.vertices.capacity() * std::mem::size_of::<[f32; 2]>()
                    + mesh.alphas.capacity() * std::mem::size_of::<f32>()
                    + mesh.indices.capacity() * std::mem::size_of::<u32>()
            }
            Self::Fill(tris) => {
                tris.0.capacity() * std::mem::size_of::<[f32; 2]>()
                    + tris.1.capacity() * std::mem::size_of::<u32>()
            }
        }
    }
}

// The final flag separates fill and stroke hashes for the same node.
type GeometryKey = (NodeId, u64, bool);

struct GeometryEntry {
    geometry: CachedGeometry,
    referenced: bool,
}

pub struct PathMeshCache {
    map: HashMap<GeometryKey, GeometryEntry>,
    clock: VecDeque<GeometryKey>,
    resident_bytes: usize,
    budget_bytes: usize,
    entry_limit: usize,
    /// Cache misses this paint — reset at the start of `board_canvas`.
    pub tess_misses: u32,
}

impl Default for PathMeshCache {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            clock: VecDeque::new(),
            resident_bytes: 0,
            budget_bytes: CACHE_BYTES,
            entry_limit: CACHE_ENTRIES,
            tess_misses: 0,
        }
    }
}

impl PathMeshCache {
    fn get(&mut self, key: GeometryKey) -> Option<CachedGeometry> {
        let entry = self.map.get_mut(&key)?;
        entry.referenced = true;
        Some(entry.geometry.clone())
    }

    fn insert(&mut self, key: GeometryKey, geometry: CachedGeometry) {
        let bytes = geometry.bytes();
        // One exceptionally complex shape must not flush everything else.
        // Its caller still owns and paints the result for this frame.
        if bytes > self.budget_bytes || self.entry_limit == 0 {
            return;
        }
        // Clock replacement gives recently reused entries a second chance.
        // Each resident entry has exactly one queue slot; hits only set a bit,
        // so neither geometry nor recency records allocate on a warm paint.
        while self.resident_bytes + bytes > self.budget_bytes || self.map.len() >= self.entry_limit
        {
            let old = self
                .clock
                .pop_front()
                .expect("cache entry has a clock slot");
            let entry = self
                .map
                .get_mut(&old)
                .expect("clock slot has a cache entry");
            if std::mem::take(&mut entry.referenced) {
                self.clock.push_back(old);
            } else if let Some(entry) = self.map.remove(&old) {
                self.resident_bytes -= entry.geometry.bytes();
            }
        }
        self.resident_bytes += bytes;
        self.map.insert(
            key,
            GeometryEntry {
                geometry,
                referenced: false,
            },
        );
        self.clock.push_back(key);
    }

    pub(crate) fn get_or_tessellate(
        &mut self,
        node_id: NodeId,
        key: u64,
        build: impl FnOnce() -> InkMesh,
    ) -> Shared<CachedInkMesh> {
        let key = (node_id, key, false);
        if let Some(CachedGeometry::Stroke(c)) = self.get(key) {
            return c;
        }
        self.tess_misses = self.tess_misses.saturating_add(1);
        let ink = build();
        let cached = Shared::new(CachedInkMesh {
            vertices: ink.vertices.iter().map(|v| v.pos).collect(),
            alphas: ink.vertices.iter().map(|v| v.alpha).collect(),
            indices: ink.indices,
        });
        self.insert(key, CachedGeometry::Stroke(cached.clone()));
        cached
    }

    pub(crate) fn get_or_fill_tris(
        &mut self,
        node_id: NodeId,
        key: u64,
        build: impl FnOnce() -> (Vec<[f32; 2]>, Vec<u32>),
    ) -> Shared<FillTriangles> {
        let key = (node_id, key, true);
        if let Some(CachedGeometry::Fill(t)) = self.get(key) {
            return t;
        }
        self.tess_misses = self.tess_misses.saturating_add(1);
        let t = Shared::new(build());
        self.insert(key, CachedGeometry::Fill(t.clone()));
        t
    }

    #[cfg(test)]
    pub(crate) fn stroke_len(&self) -> usize {
        self.map.keys().filter(|(_, _, fill)| !fill).count()
    }
}

pub fn zoom_bucket(z: f32) -> i64 {
    (z * 8.0).round() as i64
}

/// Bound visible curve error at the upper edge of the mesh's zoom bucket.
/// Both fill and stroke must refine when zooming; a fixed world tolerance
/// becomes a many-pixel facet at close range.
pub(crate) fn curve_tolerance(zoom: f32) -> f64 {
    const CURVE_ERROR_PX: f64 = 0.15;
    let upper_zoom = ((zoom_bucket(zoom) as f64 + 0.5) / 8.0).max(0.05);
    CURVE_ERROR_PX / upper_zoom
}

fn to_k(p: Pos2) -> Point {
    Point::new(p.x as f64, p.y as f64)
}

fn from_k(p: Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
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

pub use slate_doc::geom::path_data_to_world_bez;

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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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

fn hash_f32(h: &mut impl Hasher, v: f32) {
    v.to_bits().hash(h);
}

fn hash_xy(h: &mut impl Hasher, p: [f32; 2]) {
    hash_f32(h, p[0]);
    hash_f32(h, p[1]);
}

fn hash_path_data(h: &mut impl Hasher, path: &PathData) {
    hash_xy(h, path.start);
    path.closed.hash(h);
    (path.fill_rule as u8).hash(h);
    path.tips.len().hash(h);
    for tip in &path.tips {
        hash_f32(h, tip.width);
        hash_f32(h, tip.softness);
        tip.color.0.hash(h);
    }
    path.erase.len().hash(h);
    for mark in &path.erase {
        mark.points.len().hash(h);
        for p in &mark.points {
            hash_xy(h, *p);
        }
        for tip in &mark.tips {
            hash_f32(h, tip.width);
            hash_f32(h, tip.softness);
            tip.color.0.hash(h);
        }
    }
    path.segs.len().hash(h);
    for seg in &path.segs {
        match seg {
            PathSeg::Line { to } => {
                0u8.hash(h);
                hash_xy(h, *to);
            }
            PathSeg::Quad { ctrl, to } => {
                1u8.hash(h);
                hash_xy(h, *ctrl);
                hash_xy(h, *to);
            }
            PathSeg::Cubic { c1, c2, to } => {
                2u8.hash(h);
                hash_xy(h, *c1);
                hash_xy(h, *c2);
                hash_xy(h, *to);
            }
        }
    }
    path.extra.len().hash(h);
    for extra in &path.extra {
        hash_xy(h, extra.start);
        extra.closed.hash(h);
        extra.segs.len().hash(h);
        for seg in &extra.segs {
            match seg {
                PathSeg::Line { to } => {
                    0u8.hash(h);
                    hash_xy(h, *to);
                }
                PathSeg::Quad { ctrl, to } => {
                    1u8.hash(h);
                    hash_xy(h, *ctrl);
                    hash_xy(h, *to);
                }
                PathSeg::Cubic { c1, c2, to } => {
                    2u8.hash(h);
                    hash_xy(h, *c1);
                    hash_xy(h, *c2);
                    hash_xy(h, *to);
                }
            }
        }
    }
}

fn hash_stroke(h: &mut impl Hasher, stroke: &Stroke) {
    hash_f32(h, stroke.width);
    stroke.color.0.hash(h);
    (stroke.dash as u8).hash(h);
    (stroke.cap as u8).hash(h);
    (stroke.join as u8).hash(h);
    match stroke.profile {
        WidthProfile::Uniform => 0u8.hash(h),
        WidthProfile::Taper { start, end } => {
            1u8.hash(h);
            hash_f32(h, start);
            hash_f32(h, end);
        }
    }
    hash_f32(h, stroke.softness);
    stroke.stamp.hash(h);
    if let Some(from) = stroke.tween_from {
        hash_f32(h, from.width);
        hash_f32(h, from.softness);
        from.color.0.hash(h);
    }
    // Bump when the stroke fringe or cap tessellation changes, so a live
    // session drops meshes built by the previous fringe.
    3u8.hash(h);
}

fn path_content_hash(
    path: &PathData,
    stroke: &Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    bucket: i64,
) -> u64 {
    let mut h = DefaultHasher::new();
    hash_path_data(&mut h, path);
    hash_stroke(&mut h, stroke);
    hash_f32(&mut h, rect.x);
    hash_f32(&mut h, rect.y);
    hash_f32(&mut h, rect.w);
    hash_f32(&mut h, rect.h);
    hash_f32(&mut h, rotation_deg);
    bucket.hash(&mut h);
    h.finish()
}

fn path_fill_hash(path: &PathData, rect: WorldRect, rotation_deg: f32, bucket: i64) -> u64 {
    let mut h = DefaultHasher::new();
    hash_path_data(&mut h, path);
    hash_f32(&mut h, rect.x);
    hash_f32(&mut h, rect.y);
    hash_f32(&mut h, rect.w);
    hash_f32(&mut h, rect.h);
    hash_f32(&mut h, rotation_deg);
    bucket.hash(&mut h);
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
    mesh.vertices.reserve(cached.vertices.len());
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

fn shape_has_fill(shape: &ShapeNode) -> bool {
    shape.fill.is_some_and(|f| f.0[3] > 0)
}

/// Open curves and unfilled paths pick on the stroke only — never on the
/// node AABB (P1.curve.pick). A closed path with a real fill is an area.
pub fn shape_uses_stroke_pick(node: &Node, shape: &ShapeNode) -> bool {
    if open_curve_endpoints(node, shape).is_some() {
        return true;
    }
    if shape.shape == ShapeKind::Path {
        if let Some(path) = &shape.path {
            if path.is_empty() && shape.stroke.paints_as_stamp() {
                return true;
            }
            return !path.is_empty() && !shape_has_fill(shape);
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

/// A spot-erased point of a stamped stroke is not ink, so it does not pick.
fn erased_at(node: &Node, shape: &ShapeNode, wx: f32, wy: f32) -> bool {
    let Some(path) = shape.path.as_ref() else {
        return false;
    };
    if path.erase.is_empty() || !shape.stroke.paints_as_stamp() {
        return false;
    }
    let marks = stamped_erase_marks(node, shape, path);
    vector_ink::erase_coverage_at([wx, wy], &marks) >= 0.9
}

/// World-space eraser passes of a stamped path.
pub(crate) fn stamped_erase_marks(
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
) -> Vec<Vec<TipPoint>> {
    let base = stamp_style(StrokeSpan::of(&shape.stroke));
    path.erase
        .iter()
        .map(|mark| {
            mark.points
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let w = slate_doc::geom::world_point(*p, node.rect, node.rotation_deg);
                    let tip = mark.tips.get(i).or(mark.tips.first()).copied();
                    TipPoint {
                        pos: [w.x as f32, w.y as f32],
                        tip: tip.map(stamp_style).unwrap_or(base),
                    }
                })
                .collect()
        })
        .collect()
}

/// World point to `rect`-normalized coordinates, undoing `rotation_deg`.
/// Unlike path normalization this does not clamp: an eraser pass may reach
/// past the path's centerline bounds.
pub(crate) fn world_to_node_norm(p: Pos2, rect: WorldRect, rotation_deg: f32) -> [f32; 2] {
    let local = rotate_world(p, rect, -rotation_deg);
    [
        (local.x - rect.x) / rect.w.max(1e-6),
        (local.y - rect.y) / rect.h.max(1e-6),
    ]
}

fn hit_brush_dab(node: &Node, shape: &ShapeNode, wx: f32, wy: f32, zoom: f32) -> bool {
    let Some(path) = shape.path.as_ref() else {
        return false;
    };
    if !path.is_empty() || shape.stroke.is_none() || !shape.stroke.paints_as_stamp() {
        return false;
    }
    let (cx, cy) = node.rect.center();
    let reach = shape.stroke.width.max(0.0) * 0.5 + pick_slop_world(zoom);
    let dx = wx - cx;
    let dy = wy - cy;
    dx * dx + dy * dy <= reach * reach
}

/// Stroke-precise point pick for any shape node (open or closed path).
pub fn hit_shape_stroke(node: &Node, shape: &ShapeNode, wx: f32, wy: f32, zoom: f32) -> bool {
    if erased_at(node, shape, wx, wy) {
        return false;
    }
    if hit_brush_dab(node, shape, wx, wy, zoom) {
        return true;
    }
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
    if erased_at(node, shape, wx, wy) {
        return false;
    }
    if hit_brush_dab(node, shape, wx, wy, zoom) {
        return true;
    }
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
                let contours = flatten_contours(&bez, 0.25);
                if vector_ink::point_in_polygon(&contours, [wx, wy]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Topmost closed shape whose interior contains the point. Used when a
/// double-click misses a stroke-only pick, so an unfilled closed path still
/// opens text editing.
pub fn closed_text_target(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
) -> Option<slate_doc::NodeId> {
    scene.nodes.iter().rev().find_map(|n| {
        if n.hidden || n.locked || n.is_frame() {
            return None;
        }
        let NodeKind::Shape(shape) = &n.kind else {
            return None;
        };
        hit_closed_text(n, shape, wx, wy).then_some(n.id)
    })
}

fn hit_closed_text(node: &Node, shape: &ShapeNode, wx: f32, wy: f32) -> bool {
    if !slate_doc::scene::shape_hosts_text(shape) {
        return false;
    }
    match shape.shape {
        ShapeKind::Line => false,
        ShapeKind::Rect => node.rect.contains_rotated(wx, wy, node.rotation_deg),
        ShapeKind::Ellipse => ellipse_contains(node, wx, wy),
        ShapeKind::Path => {
            let Some(path) = shape.path.as_ref() else {
                return false;
            };
            if !path.closed {
                return false;
            }
            let bez = path_data_to_world_bez(path, node.rect, node.rotation_deg);
            let contours = flatten_contours(&bez, 0.25);
            vector_ink::point_in_polygon(&contours, [wx, wy])
        }
    }
}

fn ellipse_contains(node: &Node, wx: f32, wy: f32) -> bool {
    let (cx, cy) = node.rect.center();
    let rad = (-node.rotation_deg).to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = wx - cx;
    let dy = wy - cy;
    let lx = dx * cos - dy * sin;
    let ly = dx * sin + dy * cos;
    let rx = node.rect.w * 0.5;
    let ry = node.rect.h * 0.5;
    rx > f32::EPSILON && ry > f32::EPSILON && (lx / rx).powi(2) + (ly / ry).powi(2) <= 1.0
}

/// Flattened world polylines for a path node, one vec per contour.
/// Closed contours include the closing seam (last ≈ first).
pub fn path_world_contours(node: &Node, path: &PathData, zoom: f32) -> Vec<Vec<Pos2>> {
    if path.is_empty() && !path.closed {
        return Vec::new();
    }
    let bez = path_data_to_world_bez(path, node.rect, node.rotation_deg);
    flatten_contours(&bez, curve_tolerance(zoom))
        .into_iter()
        .map(|c| c.into_iter().map(|p| Pos2::new(p[0], p[1])).collect())
        .collect()
}

/// Selection / hover ring on the path itself — never the node AABB.
pub fn paint_path_stroke_outline(
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    path: &PathData,
    stroke: EStroke,
) {
    for contour in path_world_contours(node, path, xf.z) {
        if contour.len() < 2 {
            continue;
        }
        let pts: Vec<Pos2> = contour.iter().map(|p| xf.w2s(*p)).collect();
        if path.closed {
            painter.add(Shape::closed_line(pts, stroke));
        } else {
            painter.add(Shape::line(pts, stroke));
        }
    }
}

fn segment_intersects_rect(a: Pos2, b: Pos2, r: WorldRect) -> bool {
    if r.contains(a.x, a.y) || r.contains(b.x, b.y) {
        return true;
    }
    let edges = [
        ((r.x, r.y), (r.x + r.w, r.y)),
        ((r.x + r.w, r.y), (r.x + r.w, r.y + r.h)),
        ((r.x + r.w, r.y + r.h), (r.x, r.y + r.h)),
        ((r.x, r.y + r.h), (r.x, r.y)),
    ];
    edges
        .iter()
        .any(|(p1, p2)| super::board_snap::segments_intersect((a.x, a.y), (b.x, b.y), *p1, *p2))
}

/// Screen-x of the sweep: pointer at or right of the press is Window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarqueeMode {
    Window,
    Crossing,
}

pub fn marquee_mode(start_screen_x: f32, pointer_x: f32) -> MarqueeMode {
    if pointer_x >= start_screen_x {
        MarqueeMode::Window
    } else {
        MarqueeMode::Crossing
    }
}

/// Window keeps a node only when its pick geometry lies entirely inside.
/// Crossing is [`marquee_hits_node`].
pub fn marquee_selects_node(
    node: &Node,
    marquee: WorldRect,
    zoom: f32,
    scene: &slate_doc::scene::Scene,
    routing: slate_doc::WireRouting,
    mode: MarqueeMode,
) -> bool {
    // Wires keep the approved crossing rule in docs/keymap/specs/connectors.md:
    // a routed stroke through the box is selected either drag direction.
    // select-sweep D17's window containment is still proposed, so it does not
    // apply to connectors.
    if matches!(node.kind, NodeKind::Connector(_)) {
        return marquee_hits_node(node, marquee, zoom, scene, routing);
    }
    match mode {
        MarqueeMode::Crossing => marquee_hits_node(node, marquee, zoom, scene, routing),
        MarqueeMode::Window => marquee_contains_node(node, marquee, scene, routing),
    }
}

fn flattened_inside(bez: &BezPath, marquee: WorldRect) -> bool {
    let mut any = false;
    for contour in flatten_contours(bez, 0.25) {
        for p in contour {
            any = true;
            if !marquee.contains(p[0] as f32, p[1] as f32) {
                return false;
            }
        }
    }
    any
}

fn rotated_rect_inside(marquee: WorldRect, rect: WorldRect, rotation_deg: f32) -> bool {
    rect.corners_rotated(rotation_deg)
        .iter()
        .all(|(x, y)| marquee.contains(*x, *y))
}

/// Window marquee: stroke-pick geometry and connectors must lie entirely
/// inside; other nodes must have every rotated-rect corner inside.
pub fn marquee_contains_node(
    node: &Node,
    marquee: WorldRect,
    scene: &slate_doc::scene::Scene,
    routing: slate_doc::WireRouting,
) -> bool {
    match &node.kind {
        NodeKind::Connector(c) => {
            let Some(path) = slate_doc::connector_route_in_scene(
                scene,
                Some(node.id),
                &c.a,
                &c.b,
                c.effective_routing(routing),
            ) else {
                return false;
            };
            let bez = super::board_wire::connector_path_kurbo(&path);
            flattened_inside(&bez, marquee)
        }
        NodeKind::Shape(s) => {
            if shape_uses_stroke_pick(node, s) {
                let Some(bez) = bez_from_open_curve(node, s) else {
                    return false;
                };
                return flattened_inside(&bez, marquee);
            }
            if s.shape == ShapeKind::Path {
                if let Some(path) = s.path.as_ref() {
                    if !path.is_empty() {
                        let bez = path_data_to_world_bez(path, node.rect, node.rotation_deg);
                        return flattened_inside(&bez, marquee);
                    }
                }
                return false;
            }
            rotated_rect_inside(marquee, node.rect, node.rotation_deg)
        }
        _ => rotated_rect_inside(marquee, node.rect, node.rotation_deg),
    }
}

/// Marquee selection for board nodes. Open curves intersect on stroke
/// geometry (centerline vs rect), never the node AABB alone (P1.curve.pick).
pub fn marquee_hits_node(
    node: &Node,
    marquee: WorldRect,
    zoom: f32,
    scene: &slate_doc::scene::Scene,
    routing: slate_doc::WireRouting,
) -> bool {
    match &node.kind {
        NodeKind::Connector(c) => {
            let Some(path) = slate_doc::connector_route_in_scene(
                scene,
                Some(node.id),
                &c.a,
                &c.b,
                c.effective_routing(routing),
            ) else {
                return false;
            };
            let bez = super::board_wire::connector_path_kurbo(&path);
            let half = c.stroke.width.max(0.0) * 0.5;
            let region = WorldRect::new(
                marquee.x - half,
                marquee.y - half,
                marquee.w + half * 2.0,
                marquee.h + half * 2.0,
            );
            flatten(&bez, 0.25).windows(2).any(|p| {
                segment_intersects_rect(
                    Pos2::new(p[0][0], p[0][1]),
                    Pos2::new(p[1][0], p[1][1]),
                    region,
                )
            })
        }
        NodeKind::Shape(s) => {
            if shape_uses_stroke_pick(node, s) {
                if let Some((a, b)) = open_curve_endpoints(node, s) {
                    if segment_intersects_rect(a, b, marquee) {
                        return true;
                    }
                }
                if let Some(bez) = bez_from_open_curve(node, s) {
                    for contour in flatten_contours(&bez, 0.25) {
                        for w in contour.windows(2) {
                            let a = Pos2::new(w[0][0], w[0][1]);
                            let b = Pos2::new(w[1][0], w[1][1]);
                            if segment_intersects_rect(a, b, marquee) {
                                return true;
                            }
                        }
                    }
                }
                let cx = marquee.x + marquee.w * 0.5;
                let cy = marquee.y + marquee.h * 0.5;
                return hit_shape_stroke(node, s, cx, cy, zoom);
            }
            if s.shape == ShapeKind::Path {
                if hit_path_node(
                    node,
                    s,
                    marquee.x + marquee.w * 0.5,
                    marquee.y + marquee.h * 0.5,
                    zoom,
                ) {
                    return true;
                }
                if let Some(path) = s.path.as_ref() {
                    if !path.is_empty() {
                        let bez = path_data_to_world_bez(path, node.rect, node.rotation_deg);
                        for contour in flatten_contours(&bez, 0.25) {
                            for w in contour.windows(2) {
                                let a = Pos2::new(w[0][0], w[0][1]);
                                let b = Pos2::new(w[1][0], w[1][1]);
                                if segment_intersects_rect(a, b, marquee) {
                                    return true;
                                }
                            }
                        }
                    }
                }
                return false;
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
    board_pick_node_routed(
        scene,
        wx,
        wy,
        zoom,
        include_locked,
        slate_doc::WireRouting::Bezier,
    )
}

/// Point pick with the session wire routing. A node under the pointer, and
/// the pick slop just outside that node, beats a wire.
pub fn board_pick_node_routed(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
    include_locked: bool,
    routing: slate_doc::WireRouting,
) -> Option<NodeId> {
    let eligible = |n: &Node| !n.hidden && (!n.locked || include_locked) && !n.is_frame();
    for n in scene.nodes.iter().rev().filter(|n| eligible(n)) {
        if matches!(n.kind, NodeKind::Connector(_)) {
            continue;
        }
        if let NodeKind::Shape(s) = &n.kind {
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
        if n.rect.contains_rotated(wx, wy, n.rotation_deg) {
            return Some(n.id);
        }
    }
    for n in scene.nodes.iter().rev().filter(|n| eligible(n)) {
        let NodeKind::Connector(c) = &n.kind else {
            continue;
        };
        if super::board_wire::hit_connector_routed(scene, n.id, c, wx, wy, zoom, routing) {
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

#[cfg(test)]
pub fn default_draw_stroke(accent: Rgba) -> Stroke {
    Stroke {
        width: 2.0,
        color: accent,
        dash: Dash::Solid,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        profile: WidthProfile::Uniform,
        softness: 0.0,
        stamp: false,
        tween_from: None,
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
        softness: 0.0,
        stamp: false,
        tween_from: None,
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
    if path.is_empty()
        && !path.closed
        && !(shape.stroke.paints_as_stamp() && !shape.stroke.is_none())
    {
        return;
    }
    if shape.stroke.paints_as_stamp() && !shape.stroke.is_none() {
        paint_stamped_stroke(app, painter, xf, node, shape, path, fade);
        return;
    }
    // Both a fill and a stroke may miss together. Build their shared path only
    // then; a warm paint does not allocate a BezPath or walk its segments twice.
    let mut bez = None;
    if shape.fill.is_some() && path.closed {
        // egui PathShape fills with a triangle fan from vertex 0 — convex
        // only (emilk/egui#513). Join/Trim boolean results are concave, so
        // every closed path fill goes through cached earcut.
        let fill_key = path_fill_hash(path, node.rect, node.rotation_deg, zoom_bucket(xf.z));
        let triangles = app.path_mesh_cache.get_or_fill_tris(node.id, fill_key, || {
            let bez = bez
                .get_or_insert_with(|| path_data_to_world_bez(path, node.rect, node.rotation_deg));
            let contours = vector_ink::flatten_contours(bez, curve_tolerance(xf.z));
            vector_ink::fill_triangles(&contours)
        });
        let (verts, idx) = triangles.as_ref();
        if !idx.is_empty() {
            if let Some(fill) = shape.fill {
                let mut mesh = egui::Mesh::default();
                mesh.vertices.reserve(verts.len());
                let color = fade(rgba32(fill));
                for v in verts {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: xf.w2s(Pos2::new(v[0], v[1])),
                        uv: Pos2::ZERO,
                        color,
                    });
                }
                mesh.indices = idx.clone();
                painter.add(Shape::mesh(mesh));
            }
        }
    }
    if shape.stroke.is_none() {
        return;
    }
    let bucket = zoom_bucket(xf.z);
    let key = path_content_hash(path, &shape.stroke, node.rect, node.rotation_deg, bucket);
    let cached = app.path_mesh_cache.get_or_tessellate(node.id, key, || {
        let bez =
            bez.get_or_insert_with(|| path_data_to_world_bez(path, node.rect, node.rotation_deg));
        let style = stroke_style_world(&shape.stroke, xf.z);
        let (ink_width, soft) = shape.stroke.paint_profile();
        let mut style = style;
        style.width = ink_width;
        let feather = soft
            + if soft <= 0.0 {
                FEATHER_PX / xf.z.max(0.05)
            } else {
                0.0
            };
        stroke_mesh(bez, &style, feather, curve_tolerance(xf.z))
    });
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
    let ink = stroke_mesh(bez, &style, feather, curve_tolerance(xf.z));
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

/// World units per stamp pixel at `zoom`: one physical screen pixel, snapped
/// down to a power of two so small zoom changes reuse the same bitmap.
pub(crate) fn stamp_pixel_for_zoom(zoom: f32, pixels_per_point: f32) -> f32 {
    let screen = 1.0 / (zoom * pixels_per_point).max(1.0e-3);
    2f32.powf(screen.log2().floor().clamp(-5.0, 8.0))
}

fn stamp_style(tip: StrokeSpan) -> StampStyle {
    StampStyle {
        diameter: tip.width.max(0.0),
        softness: tip.softness,
        rgba: tip.color.0,
    }
}

/// World-space tipped contours for a stamped brush path. A path with no
/// segments is one dab at the node center.
pub(crate) fn stamped_contours(
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    tolerance: f64,
) -> Vec<Vec<TipPoint>> {
    let base = stamp_style(StrokeSpan::of(&shape.stroke));
    let tips: Vec<StampStyle> = path
        .paint_tips(&shape.stroke)
        .into_iter()
        .map(stamp_style)
        .collect();
    let mut contours = if path.is_empty() {
        Vec::new()
    } else {
        let bez = path_data_to_world_bez(path, node.rect, node.rotation_deg);
        tipped_contours(&bez, &tips, base, tolerance)
    };
    contours.retain(|c| !c.is_empty());
    if contours.is_empty() {
        let (cx, cy) = node.rect.center();
        contours.push(vec![TipPoint {
            pos: [cx, cy],
            tip: tips.first().copied().unwrap_or(base),
        }]);
    }
    contours
}

/// Stamps rebuilt per frame for a zoom change. A stroke without any bitmap
/// always builds, so a commit never flickers.
const STAMP_REBUILDS_PER_FRAME: u32 = 3;
const STAMP_CACHE_BYTES: usize = 384 * 1024 * 1024;

/// Cached radial stamp for one committed stroke.
pub struct BrushStampGpu {
    pub tex: egui::TextureHandle,
    pub origin: [f32; 2],
    pub size: [f32; 2],
    /// The resolution this bitmap was requested at (before size coarsening).
    pub wanted_pixel: f32,
    pub bytes: usize,
    pub used: u64,
}

fn paint_stamped_stroke(
    app: &mut SlateApp,
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    fade: &impl Fn(Color32) -> Color32,
) {
    let want = stamp_pixel_for_zoom(xf.z, painter.ctx().pixels_per_point());
    if let Some(super::board::BoardDrag::Erase {
        points,
        spot,
        straight,
        ..
    }) = &app.board_drag
    {
        if spot.contains(&node.id) {
            let (points, straight) = (points.clone(), *straight);
            let tip = app.eraser_tip();
            if !app.erase_live.contains_key(&node.id) {
                if let Some(live) = EraseLive::new(painter, node, shape, path, want) {
                    app.erase_live.insert(node.id, live);
                }
            }
            if let Some(live) = app.erase_live.get_mut(&node.id) {
                live.feed(&points, tip, straight);
                live.paint(painter, xf, fade(Color32::WHITE));
                return;
            }
        }
    }
    let key = path_content_hash(path, &shape.stroke, node.rect, node.rotation_deg, 0) ^ 0x57A5;
    let (same_shape, same_res) = match app.brush_stamps.get(&node.id) {
        Some((cached, gpu)) => (*cached == key, gpu.wanted_pixel == want),
        None => (false, false),
    };
    if !(same_shape && same_res) {
        let rebuild_allowed = !same_shape || app.brush_stamp_rebuilds < STAMP_REBUILDS_PER_FRAME;
        if rebuild_allowed {
            if same_shape {
                app.brush_stamp_rebuilds += 1;
            }
            let contours = stamped_contours(node, shape, path, (want as f64 * 0.5).max(0.05));
            let Some(mut stamp) = stamp_tipped(&contours, want) else {
                app.brush_stamps.remove(&node.id);
                return;
            };
            vector_ink::apply_erase(&mut stamp, &stamped_erase_marks(node, shape, path));
            let gpu = upload_stamp(painter, &format!("brush-stamp-{}", node.id.0), stamp, want);
            app.brush_stamps.insert(node.id, (key, gpu));
            evict_brush_stamps(&mut app.brush_stamps, app.frame_no);
        } else {
            painter.ctx().request_repaint();
        }
    }
    if let Some((_, gpu)) = app.brush_stamps.get_mut(&node.id) {
        gpu.used = app.frame_no;
        paint_stamp_quad(painter, xf, gpu, fade(Color32::WHITE));
    }
}

fn evict_brush_stamps(cache: &mut HashMap<NodeId, (u64, BrushStampGpu)>, frame: u64) {
    let total: usize = cache.values().map(|(_, g)| g.bytes).sum();
    if total <= STAMP_CACHE_BYTES {
        return;
    }
    let mut old: Vec<(u64, NodeId, usize)> = cache
        .iter()
        .filter(|(_, (_, g))| g.used + 1 < frame)
        .map(|(id, (_, g))| (g.used, *id, g.bytes))
        .collect();
    old.sort_by_key(|(used, _, _)| *used);
    let mut total = total;
    for (_, id, bytes) in old {
        if total <= STAMP_CACHE_BYTES {
            break;
        }
        cache.remove(&id);
        total -= bytes;
    }
}

fn premultiplied(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.chunks_mut(4) {
        let a = px[3] as f32 / 255.0;
        px[0] = (px[0] as f32 * a).round() as u8;
        px[1] = (px[1] as f32 * a).round() as u8;
        px[2] = (px[2] as f32 * a).round() as u8;
    }
    out
}

fn upload_stamp(
    painter: &egui::Painter,
    name: &str,
    stamp: vector_ink::StampImage,
    wanted_pixel: f32,
) -> BrushStampGpu {
    let image = egui::ColorImage::from_rgba_premultiplied(
        [stamp.width as usize, stamp.height as usize],
        &premultiplied(&stamp.rgba),
    );
    let tex = painter
        .ctx()
        .load_texture(name, image, egui::TextureOptions::LINEAR);
    BrushStampGpu {
        tex,
        origin: stamp.origin,
        size: [
            stamp.width as f32 * stamp.pixel,
            stamp.height as f32 * stamp.pixel,
        ],
        wanted_pixel,
        bytes: stamp.rgba.len(),
        used: 0,
    }
}

fn paint_stamp_quad(painter: &egui::Painter, xf: &BoardXf, gpu: &BrushStampGpu, tint: Color32) {
    let min = xf.w2s(Pos2::new(gpu.origin[0], gpu.origin[1]));
    let max = xf.w2s(Pos2::new(
        gpu.origin[0] + gpu.size[0],
        gpu.origin[1] + gpu.size[1],
    ));
    painter.image(
        gpu.tex.id(),
        egui::Rect::from_min_max(min, max),
        egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        tint,
    );
}

/// The brush drag's own canvas, aligned to the screen at one physical pixel
/// per bitmap pixel. Freehand segments are added as they arrive and only the
/// touched region uploads. A straight preview restores the region it drew
/// last frame from `base` (the canvas as it was before the preview) and
/// re-stamps just the live segment, so the frame cost is that segment's area.
///
/// When a Shift line continues an earlier stroke, that stroke is stamped into
/// the canvas first and hidden from the scene paint for the drag. The
/// preview is then one bitmap with the committed result's max-coverage
/// joint, not two overlapping images.
pub struct BrushLiveCanvas {
    img: vector_ink::StampImage,
    base: Vec<u8>,
    tex: egui::TextureHandle,
    view: [u32; 6],
    pub anchor: Option<NodeId>,
    freehand_done: usize,
    line_key: Option<u64>,
    line_dirty: Option<[u32; 4]>,
}

fn view_key(xf: &BoardXf, screen: egui::Rect, ppp: f32) -> [u32; 6] {
    [
        xf.offset.x.to_bits(),
        xf.offset.y.to_bits(),
        xf.z.to_bits(),
        screen.width().to_bits(),
        screen.height().to_bits(),
        ppp.to_bits(),
    ]
}

impl BrushLiveCanvas {
    /// Reuse `slot` while the camera and anchor are unchanged; otherwise
    /// build a fresh canvas with the anchor stroke's contours stamped in.
    pub fn ensure<'a>(
        slot: &'a mut Option<BrushLiveCanvas>,
        painter: &egui::Painter,
        xf: &BoardXf,
        screen: egui::Rect,
        anchor_id: Option<NodeId>,
        anchor_contours: impl FnOnce() -> Vec<Vec<TipPoint>>,
    ) -> &'a mut BrushLiveCanvas {
        let ppp = painter.ctx().pixels_per_point();
        let view = view_key(xf, screen, ppp);
        let reuse = slot
            .as_ref()
            .is_some_and(|c| c.view == view && c.anchor == anchor_id);
        if !reuse {
            let w = (screen.width() * ppp).ceil().max(1.0) as u32;
            let h = (screen.height() * ppp).ceil().max(1.0) as u32;
            let origin = xf.s2w(screen.min);
            let mut img = vector_ink::StampImage {
                width: w,
                height: h,
                origin: [origin.x, origin.y],
                pixel: 1.0 / (xf.z * ppp).max(1.0e-3),
                rgba: vec![0u8; (w as usize) * (h as usize) * 4],
            };
            if anchor_id.is_some() {
                for contour in &anchor_contours() {
                    match contour.as_slice() {
                        [] => {}
                        [only] => stamp_segment(&mut img, *only, *only),
                        pts => {
                            for s in pts.windows(2) {
                                stamp_segment(&mut img, s[0], s[1]);
                            }
                        }
                    }
                }
            }
            let tex = painter.ctx().load_texture(
                "brush-live",
                egui::ColorImage::from_rgba_premultiplied(
                    [w as usize, h as usize],
                    &premultiplied(&img.rgba),
                ),
                egui::TextureOptions::LINEAR,
            );
            *slot = Some(BrushLiveCanvas {
                base: img.rgba.clone(),
                img,
                tex,
                view,
                anchor: anchor_id,
                freehand_done: 0,
                line_key: None,
                line_dirty: None,
            });
        }
        slot.as_mut().expect("canvas just ensured")
    }

    fn segment_box(&self, a: TipPoint, b: TipPoint) -> Option<[u32; 4]> {
        segment_box(&self.img, a, b)
    }

    fn upload(&mut self, dirty: [u32; 4]) {
        upload_region(&mut self.tex, &self.img.rgba, self.img.width, dirty);
    }

    /// Stamp freehand points not yet on the canvas.
    pub fn add_freehand(&mut self, points: &[Pos2], tip: StampStyle) {
        let at = |p: Pos2| TipPoint {
            pos: [p.x, p.y],
            tip,
        };
        let mut dirty: Option<[u32; 4]> = None;
        let start = self.freehand_done.max(1);
        let mut segs: Vec<(TipPoint, TipPoint)> = Vec::new();
        if self.freehand_done == 0 && !points.is_empty() {
            segs.push((at(points[0]), at(points[0])));
        }
        for i in start..points.len() {
            segs.push((at(points[i - 1]), at(points[i])));
        }
        for (a, b) in segs {
            stamp_segment(&mut self.img, a, b);
            if let Some(bx) = self.segment_box(a, b) {
                dirty = Some(union_box(dirty, bx));
            }
        }
        self.freehand_done = points.len();
        if let Some(d) = dirty {
            self.upload(d);
        }
    }

    /// Show one straight segment on top of the base canvas.
    pub fn set_line(&mut self, a: TipPoint, b: TipPoint) {
        let key = {
            let mut h = DefaultHasher::new();
            for v in [
                a.pos[0],
                a.pos[1],
                b.pos[0],
                b.pos[1],
                a.tip.diameter,
                b.tip.diameter,
            ] {
                hash_f32(&mut h, v);
            }
            hash_f32(&mut h, a.tip.softness);
            hash_f32(&mut h, b.tip.softness);
            a.tip.rgba.hash(&mut h);
            b.tip.rgba.hash(&mut h);
            h.finish()
        };
        if self.line_key == Some(key) {
            return;
        }
        self.line_key = Some(key);
        let stride = self.img.width as usize * 4;
        if let Some([x0, y0, x1, y1]) = self.line_dirty {
            for y in y0 as usize..y1 as usize {
                let row = y * stride + x0 as usize * 4..y * stride + x1 as usize * 4;
                self.img.rgba[row.clone()].copy_from_slice(&self.base[row]);
            }
        }
        stamp_segment(&mut self.img, a, b);
        let new_box = self.segment_box(a, b);
        let dirty = match (self.line_dirty, new_box) {
            (Some(old), Some(new)) => Some(union_box(Some(old), new)),
            (old, new) => old.or(new),
        };
        self.line_dirty = new_box;
        if let Some(d) = dirty {
            self.upload(d);
        }
    }

    pub fn paint(&self, painter: &egui::Painter, xf: &BoardXf) {
        let size = [
            self.img.width as f32 * self.img.pixel,
            self.img.height as f32 * self.img.pixel,
        ];
        let min = xf.w2s(Pos2::new(self.img.origin[0], self.img.origin[1]));
        let max = xf.w2s(Pos2::new(
            self.img.origin[0] + size[0],
            self.img.origin[1] + size[1],
        ));
        painter.image(
            self.tex.id(),
            egui::Rect::from_min_max(min, max),
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

fn union_box(a: Option<[u32; 4]>, b: [u32; 4]) -> [u32; 4] {
    match a {
        None => b,
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
    }
}

/// Pixel box `[x0, y0, x1, y1)` a tipped segment can touch in `img`.
fn segment_box(img: &vector_ink::StampImage, a: TipPoint, b: TipPoint) -> Option<[u32; 4]> {
    let px = img.pixel;
    let r = a.tip.diameter.max(b.tip.diameter) * 0.5 / px + 2.0;
    let ax = (a.pos[0] - img.origin[0]) / px;
    let ay = (a.pos[1] - img.origin[1]) / px;
    let bx = (b.pos[0] - img.origin[0]) / px;
    let by = (b.pos[1] - img.origin[1]) / px;
    let x0 = (ax.min(bx) - r).floor().max(0.0);
    let y0 = (ay.min(by) - r).floor().max(0.0);
    let x1 = (ax.max(bx) + r).ceil().min(img.width as f32);
    let y1 = (ay.max(by) + r).ceil().min(img.height as f32);
    (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, x1 as u32, y1 as u32])
}

/// Upload the `dirty` box of straight-alpha `rgba` into `tex`.
fn upload_region(tex: &mut egui::TextureHandle, rgba: &[u8], width: u32, dirty: [u32; 4]) {
    let [x0, y0, x1, y1] = dirty;
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    if w == 0 || h == 0 {
        return;
    }
    let stride = width as usize * 4;
    let mut sub = Vec::with_capacity(w * h * 4);
    for y in y0 as usize..y1 as usize {
        let row = y * stride + x0 as usize * 4;
        sub.extend_from_slice(&rgba[row..row + w * 4]);
    }
    tex.set_partial(
        [x0 as usize, y0 as usize],
        egui::ColorImage::from_rgba_premultiplied([w, h], &premultiplied(&sub)),
        egui::TextureOptions::LINEAR,
    );
}

/// Live spot erase on one stamped stroke during an eraser drag. `ink` is the
/// stroke as committed (earlier passes applied); `mask` holds this pass with
/// max coverage; the texture shows `ink * (1 - mask)`, uploading only the
/// region the eraser touched.
pub struct EraseLive {
    ink: Vec<u8>,
    mask: vector_ink::StampImage,
    shown: Vec<u8>,
    tex: egui::TextureHandle,
    done: usize,
    line_box: Option<[u32; 4]>,
    /// Some visible ink lies under the pass, so release journals it.
    pub changed: bool,
}

impl EraseLive {
    fn new(
        painter: &egui::Painter,
        node: &Node,
        shape: &ShapeNode,
        path: &PathData,
        pixel: f32,
    ) -> Option<EraseLive> {
        let contours = stamped_contours(node, shape, path, (pixel as f64 * 0.5).max(0.05));
        let mut img = stamp_tipped(&contours, pixel)?;
        vector_ink::apply_erase(&mut img, &stamped_erase_marks(node, shape, path));
        let tex = painter.ctx().load_texture(
            format!("erase-live-{}", node.id.0),
            egui::ColorImage::from_rgba_premultiplied(
                [img.width as usize, img.height as usize],
                &premultiplied(&img.rgba),
            ),
            egui::TextureOptions::LINEAR,
        );
        let mask = vector_ink::StampImage {
            width: img.width,
            height: img.height,
            origin: img.origin,
            pixel: img.pixel,
            rgba: vec![0u8; img.rgba.len()],
        };
        Some(EraseLive {
            shown: img.rgba.clone(),
            ink: img.rgba,
            mask,
            tex,
            done: 0,
            line_box: None,
            changed: false,
        })
    }

    /// Bring the mask up to date with the eraser's `points`. A straight pass
    /// is only its first and last point and replaces last frame's line.
    fn feed(&mut self, points: &[Pos2], tip: StampStyle, straight: bool) {
        let at = |p: Pos2| TipPoint {
            pos: [p.x, p.y],
            tip,
        };
        let mut dirty: Option<[u32; 4]> = None;
        if straight {
            let (Some(first), Some(last)) = (points.first(), points.last()) else {
                return;
            };
            if let Some([x0, y0, x1, y1]) = self.line_box.take() {
                let stride = self.mask.width as usize * 4;
                for y in y0 as usize..y1 as usize {
                    self.mask.rgba[y * stride + x0 as usize * 4..y * stride + x1 as usize * 4]
                        .fill(0);
                }
                dirty = Some([x0, y0, x1, y1]);
                self.changed = false;
            }
            let (a, b) = (at(*first), at(*last));
            stamp_segment(&mut self.mask, a, b);
            if let Some(bx) = segment_box(&self.mask, a, b) {
                self.line_box = Some(bx);
                dirty = Some(union_box(dirty, bx));
            }
        } else {
            if self.done == 0 {
                if let Some(p) = points.first() {
                    stamp_segment(&mut self.mask, at(*p), at(*p));
                    dirty = segment_box(&self.mask, at(*p), at(*p));
                }
            }
            for i in self.done.max(1)..points.len() {
                let (a, b) = (at(points[i - 1]), at(points[i]));
                stamp_segment(&mut self.mask, a, b);
                if let Some(bx) = segment_box(&self.mask, a, b) {
                    dirty = Some(union_box(dirty, bx));
                }
            }
            self.done = points.len();
        }
        let Some(d) = dirty else {
            return;
        };
        let [x0, y0, x1, y1] = d;
        let w = self.mask.width;
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 4) as usize;
                self.shown[i..i + 4].copy_from_slice(&self.ink[i..i + 4]);
                if self.ink[i + 3] > 0 && self.mask.rgba[i + 3] > 0 {
                    self.changed = true;
                }
            }
        }
        vector_ink::multiply_by_mask(&mut self.shown, &self.mask.rgba, w, Some(d));
        upload_region(&mut self.tex, &self.shown, w, d);
    }

    fn paint(&self, painter: &egui::Painter, xf: &BoardXf, tint: Color32) {
        let m = &self.mask;
        let min = xf.w2s(Pos2::new(m.origin[0], m.origin[1]));
        let max = xf.w2s(Pos2::new(
            m.origin[0] + m.width as f32 * m.pixel,
            m.origin[1] + m.height as f32 * m.pixel,
        ));
        painter.image(
            self.tex.id(),
            egui::Rect::from_min_max(min, max),
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            tint,
        );
    }
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
            // Click order is start → end → middle. The through-point is the
            // last pick so dragging it changes bulge only (endpoints stay).
            match pts.as_slice() {
                [start, end] => {
                    let mut bez = BezPath::new();
                    bez.move_to(to_k(*start));
                    bez.line_to(to_k(*end));
                    paint_path_preview(painter, xf, color, &bez);
                }
                [start, end, mid, ..] => {
                    let bez = arc_through_three_points(*start, *mid, *end);
                    paint_path_preview(painter, xf, color, &bez);
                }
                _ => {}
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

/// Capture and fit tolerances are screen-space, independent of board zoom.
pub const FREEHAND_SAMPLE_SPACING_PX: f32 = 0.5;
pub const FREEHAND_FIT_ERROR_PX: f32 = 0.5;
pub(crate) fn append_freehand_endpoint(points: &mut Vec<Pos2>, end: Pos2) {
    if points.last().copied() != Some(end) {
        points.push(end);
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
        let (rect, path_data, closed) = match draft {
            BoardPathDraft::Polyline { mut points } => {
                if points.len() < 2 {
                    return false;
                }
                let closed = points.len() >= 4 && points.first() == points.last();
                if closed {
                    points.pop();
                }
                let (r, d) = points_to_path_data(&points, closed);
                (r, d, closed)
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
        self.commit_path_node(rect, path_data, closed);
        true
    }

    pub(crate) fn commit_path_node(&mut self, rect: WorldRect, path_data: PathData, closed: bool) {
        let mut path_data = path_data;
        path_data.closed = closed;
        let stroke = self.stroke_for_new_curve();
        let fill = if closed {
            self.fill_for_new_shape()
        } else {
            None
        };
        let opacity = self.opacity_for_new_node();
        let mut node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill,
                stroke,
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(path_data),

                text: None,
            }),
        );
        node.opacity = opacity;
        self.note_last_style(&node);
        let ids = self.add_nodes(vec![node]);
        self.board_sel = ids.into_iter().collect();
        self.board_tool = super::board::BoardTool::Select;
    }

    pub(crate) fn path_tool_click(&mut self, world: Pos2) {
        // Ortho (F8, Shift inverts): draft segments snap to 45Â° from the
        // last anchor (constraints spec Â§1).
        let from = match &self.board_path_draft {
            Some(BoardPathDraft::Polyline { points }) => points.last().copied(),
            Some(BoardPathDraft::Arc { points }) => points.last().copied(),
            Some(BoardPathDraft::Bezier { anchors, .. }) => anchors.last().map(|(p, _)| *p),
            None => None,
        };
        let world = self.resolve_point_snap(world, &[], from, self.shift_down, from.is_some());
        match self.board_tool {
            super::board::BoardTool::Polyline => {
                if let Some(BoardPathDraft::Polyline { points }) = &mut self.board_path_draft {
                    if points.last().copied() != Some(world) {
                        points.push(world);
                    }
                    if points.len() >= 4 && points.first() == points.last() {
                        self.finish_path_draft();
                    }
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
                    // start, end, middle → through-point is the last pick
                    let bez = arc_through_three_points(pts[0], pts[2], pts[1]);
                    let (rect, data) = bezpath_to_path_data(&bez, false);
                    self.commit_path_node(rect, data, false);
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
        let tol = FREEHAND_FIT_ERROR_PX / self.tab().cam.z.max(f32::EPSILON);
        let flat: Vec<[f32; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
        let bez = vector_ink::fit_polyline(&flat, tol);
        let (rect, data) = bezpath_to_path_data(&bez, false);
        if data.is_empty() {
            return;
        }
        self.commit_path_node(rect, data, false);
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
    fn curve_error_stays_subpixel_and_fill_cache_refines_with_zoom() {
        for zoom in [0.05, 0.25, 1.0, 8.0, 64.0] {
            assert!(curve_tolerance(zoom) * zoom as f64 <= 0.15);
        }
        let (rect, path) = points_to_path_data(&[Pos2::ZERO, Pos2::new(30.0, 40.0)], true);
        assert_ne!(
            path_fill_hash(&path, rect, 0.0, zoom_bucket(1.0)),
            path_fill_hash(&path, rect, 0.0, zoom_bucket(8.0))
        );
    }

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
    fn start_end_middle_arc_keeps_the_endpoints() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(100.0, 0.0);
        let mid = Pos2::new(50.0, 40.0);
        let bez = arc_through_three_points(start, mid, end);
        let flat = flatten(&bez, 0.05);
        let first = flat.first().copied().unwrap();
        let last = flat.last().copied().unwrap();
        assert!((first[0] - start.x).abs() < 0.5);
        assert!((first[1] - start.y).abs() < 0.5);
        assert!((last[0] - end.x).abs() < 0.5);
        assert!((last[1] - end.y).abs() < 0.5);
        let swapped = arc_through_three_points(start, end, mid);
        let swapped_end = flatten(&swapped, 0.05).last().copied().unwrap();
        assert!(
            (swapped_end[0] - mid.x).abs() < 1.5,
            "the old start-end-as-through mapping must not be used"
        );
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
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_draw_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),

                text: None,
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
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_curve_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),

                text: None,
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
        assert!(!node.rect.contains(50.0, 10.0) || !hit_path_node(&node, shape, 50.0, 10.0, 1.0));
        let marquee = WorldRect::new(40.0, 5.0, 20.0, 10.0);
        assert!(
            !marquee_hits_node(
                &node,
                marquee,
                1.0,
                &slate_doc::scene::Scene::default(),
                slate_doc::WireRouting::Bezier
            ),
            "marquee wholly off stroke must not select"
        );
        let stroke_marquee = WorldRect::new(45.0, 45.0, 10.0, 10.0);
        assert!(marquee_hits_node(
            &node,
            stroke_marquee,
            1.0,
            &slate_doc::scene::Scene::default(),
            slate_doc::WireRouting::Bezier
        ));
    }

    #[test]
    fn window_keeps_only_a_fully_enclosed_stroke() {
        let pts = vec![Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0)];
        let (rect, data) = points_to_path_data(&pts, false);
        let node = Node {
            id: NodeId(7),
            rect,
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_curve_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),
                text: None,
            }),
        };
        let scene = slate_doc::scene::Scene::default();
        let routing = slate_doc::WireRouting::Bezier;
        let mid = WorldRect::new(40.0, 40.0, 20.0, 20.0);
        assert!(
            marquee_selects_node(&node, mid, 1.0, &scene, routing, MarqueeMode::Crossing),
            "a stroke through the box is a crossing hit"
        );
        assert!(
            !marquee_selects_node(&node, mid, 1.0, &scene, routing, MarqueeMode::Window),
            "a stroke through the box is not a window hit"
        );
        let full = WorldRect::new(-1.0, 40.0, 102.0, 20.0);
        assert!(marquee_selects_node(
            &node,
            full,
            1.0,
            &scene,
            routing,
            MarqueeMode::Window
        ));
    }

    #[test]
    fn closed_polyline_picks_stroke_not_bbox_or_interior() {
        let pts = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(80.0, 0.0),
            Pos2::new(0.0, 80.0),
        ];
        let (rect, data) = points_to_path_data(&pts, true);
        let node = Node {
            id: NodeId(4),
            rect,
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_curve_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),

                text: None,
            }),
        };
        let shape = match &node.kind {
            NodeKind::Shape(s) => s,
            _ => unreachable!(),
        };
        assert!(shape_uses_stroke_pick(&node, shape));
        assert!(hit_path_node(&node, shape, 40.0, 0.0, 1.0), "on the base");
        assert!(
            hit_path_node(&node, shape, 40.0, 40.0, 1.0),
            "on the closing seam"
        );
        assert!(
            !hit_path_node(&node, shape, 20.0, 20.0, 1.0),
            "unfilled interior must not hit"
        );
        assert!(
            hit_closed_text(&node, shape, 20.0, 20.0),
            "double-click interior of a closed path opens text"
        );
        assert!(
            !hit_path_node(&node, shape, 80.0, 80.0, 1.0),
            "AABB corner off the stroke must not hit"
        );
        let ghost = WorldRect::new(70.0, 70.0, 12.0, 12.0);
        assert!(
            !marquee_hits_node(
                &node,
                ghost,
                1.0,
                &slate_doc::scene::Scene::default(),
                slate_doc::WireRouting::Bezier
            ),
            "marquee in empty AABB space must not select a closed polyline"
        );
    }

    #[test]
    fn closed_polyline_does_not_hit_outside_its_bbox() {
        // Triangle far from the world origin. A ClosePath→(0,0) ghost, a
        // concatenated extra contour, or an unclamped infinite-line test
        // would all fire out here.
        let pts = vec![
            Pos2::new(400.0, 300.0),
            Pos2::new(480.0, 300.0),
            Pos2::new(400.0, 380.0),
        ];
        let (rect, data) = points_to_path_data(&pts, true);
        assert!(rect.x > 300.0 && rect.y > 200.0);
        let bez = path_data_to_world_bez(&data, rect, 0.0);
        for contour in flatten_contours(&bez, 0.25) {
            for p in contour {
                assert!(
                    rect.contains(p[0], p[1]),
                    "flatten point {p:?} left node.rect {rect:?}"
                );
            }
        }
        let node = Node {
            id: NodeId(5),
            rect,
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: default_curve_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),

                text: None,
            }),
        };
        let shape = match &node.kind {
            NodeKind::Shape(s) => s,
            _ => unreachable!(),
        };
        assert!(hit_path_node(&node, shape, 440.0, 300.0, 1.0), "base");
        assert!(
            hit_path_node(&node, shape, 400.0, 340.0, 1.0),
            "closing seam"
        );
        assert!(!hit_path_node(&node, shape, 0.0, 0.0, 1.0), "world origin");
        assert!(
            !hit_path_node(&node, shape, 200.0, 150.0, 1.0),
            "along the line from the first vertex toward the origin"
        );
        assert!(
            !hit_path_node(&node, shape, 200.0, 300.0, 1.0),
            "infinite extension of the base, outside the AABB"
        );
        assert!(
            !hit_path_node(&node, shape, 500.0, 400.0, 1.0),
            "far corner outside the AABB"
        );
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
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Line,
                fill: None,
                stroke: default_curve_stroke(Rgba::BLACK),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: None,

                text: None,
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

    #[test]
    fn path_content_hash_is_stable_and_bucket_sensitive() {
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            closed: false,
            ..Default::default()
        };
        let stroke = default_curve_stroke(Rgba::BLACK);
        let rect = WorldRect::new(0.0, 0.0, 10.0, 10.0);
        let a = path_content_hash(&path, &stroke, rect, 0.0, 8);
        let b = path_content_hash(&path, &stroke, rect, 0.0, 8);
        let c = path_content_hash(&path, &stroke, rect, 0.0, 9);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn path_mesh_cache_evicts_oldest_instead_of_clearing() {
        let mut cache = PathMeshCache {
            entry_limit: 256,
            ..Default::default()
        };
        let empty = || InkMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for i in 0..300u64 {
            cache.get_or_tessellate(NodeId(i), i, empty);
        }
        let n = cache.stroke_len();
        assert_eq!(n, 256, "empty meshes must still obey the metadata bound");
        assert!(
            n > 0,
            "nuclear clear would leave the cache empty after a burst"
        );
        assert!(!cache.map.contains_key(&(NodeId(0), 0, false)));
    }

    fn triangle_ink() -> InkMesh {
        InkMesh {
            vertices: [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]
                .into_iter()
                .map(|pos| vector_ink::InkVertex { pos, alpha: 1.0 })
                .collect(),
            indices: vec![0, 1, 2],
        }
    }

    fn triangle_fill() -> FillTriangles {
        (vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]], vec![0, 1, 2])
    }

    #[test]
    fn ten_thousand_visible_paths_stay_warm_in_paint_order() {
        let mut cache = PathMeshCache::default();
        for id in 0..10_000 {
            cache.get_or_fill_tris(NodeId(id), 0, triangle_fill);
            cache.get_or_tessellate(NodeId(id), 0, triangle_ink);
        }
        assert_eq!(cache.tess_misses, 20_000);
        cache.tess_misses = 0;
        for _ in 0..3 {
            for id in 0..10_000 {
                cache.get_or_fill_tris(NodeId(id), 0, || panic!("warm fill rebuilt"));
                cache.get_or_tessellate(NodeId(id), 0, || panic!("warm stroke rebuilt"));
            }
        }
        assert_eq!(cache.tess_misses, 0);
        assert!(cache.resident_bytes <= CACHE_BYTES);
        assert!(cache.map.len() <= CACHE_ENTRIES);
        assert_eq!(cache.clock.len(), cache.map.len());
    }

    #[test]
    fn cache_hits_share_geometry_without_copying_vectors() {
        let mut cache = PathMeshCache::default();
        let stroke = cache.get_or_tessellate(NodeId(1), 7, triangle_ink);
        let hit = cache.get_or_tessellate(NodeId(1), 7, || panic!("cache miss"));
        assert!(Shared::ptr_eq(&stroke, &hit));
        let fill = cache.get_or_fill_tris(NodeId(1), 7, triangle_fill);
        let hit = cache.get_or_fill_tris(NodeId(1), 7, || panic!("cache miss"));
        assert!(Shared::ptr_eq(&fill, &hit));
    }

    #[test]
    fn stroke_and_fill_share_a_byte_budget_and_hits_get_a_second_chance() {
        let mut cache = PathMeshCache {
            budget_bytes: 84, // One 48-byte stroke and one 36-byte fill.
            ..Default::default()
        };
        cache.get_or_tessellate(NodeId(1), 0, triangle_ink);
        cache.get_or_fill_tris(NodeId(2), 0, triangle_fill);
        assert_eq!(cache.resident_bytes, 84);
        cache.get_or_tessellate(NodeId(1), 0, || panic!("cache miss"));
        cache.get_or_fill_tris(NodeId(3), 0, triangle_fill);
        assert!(cache.map.contains_key(&(NodeId(1), 0, false)));
        assert!(!cache.map.contains_key(&(NodeId(2), 0, true)));
        assert_eq!(cache.resident_bytes, 84);
        for id in 4..100 {
            cache.get_or_fill_tris(NodeId(id), 0, triangle_fill);
            assert!(cache.resident_bytes <= cache.budget_bytes);
            assert_eq!(cache.clock.len(), cache.map.len());
        }
    }

    #[test]
    fn oversized_geometry_renders_without_flushing_the_cache() {
        let mut cache = PathMeshCache {
            budget_bytes: 48,
            ..Default::default()
        };
        let resident = cache.get_or_tessellate(NodeId(1), 0, triangle_ink);
        let large = cache.get_or_tessellate(NodeId(2), 0, || InkMesh {
            vertices: vec![
                vector_ink::InkVertex {
                    pos: [1.0, 2.0],
                    alpha: 1.0
                };
                20
            ],
            indices: vec![],
        });
        assert_eq!(large.vertices.len(), 20);
        assert_eq!(cache.resident_bytes, 48);
        assert_eq!(cache.map.len(), 1);
        let hit = cache.get_or_tessellate(NodeId(1), 0, || panic!("resident flushed"));
        assert!(Shared::ptr_eq(&resident, &hit));
    }

    #[test]
    fn rotating_a_warm_path_rebuilds_the_stroke_geometry() {
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            ..Default::default()
        };
        let stroke = default_curve_stroke(Rgba::BLACK);
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let mut cache = PathMeshCache::default();
        let mut paint = |rotation| {
            let key = path_content_hash(&path, &stroke, rect, rotation, zoom_bucket(1.0));
            cache.get_or_tessellate(NodeId(1), key, || {
                let bez = path_data_to_world_bez(&path, rect, rotation);
                stroke_mesh(&bez, &stroke_style_world(&stroke, 1.0), FEATHER_PX, 0.25)
            })
        };
        let initial = paint(0.0);
        let rotated = paint(90.0);
        let restored = paint(0.0);
        assert_ne!(initial.vertices, rotated.vertices);
        assert!(Shared::ptr_eq(&initial, &restored));
        assert_eq!(cache.tess_misses, 2);
    }
}
