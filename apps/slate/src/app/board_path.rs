//! Board vector paths: world ↔ `PathData`, tessellation cache, hit-testing.

#[path = "board_path/tiles.rs"]
pub(crate) mod tiles;

use eframe::egui::{self, Color32, Pos2, Shape, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    Dash, GroupToken, PathData, PathSeg, Rgba, ShapeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin,
    StrokeSpan, WidthProfile, WorldRect,
};
use slate_doc::vertex_style::PlacedTip;
use slate_doc::{Node, NodeId, NodeKind, StrokeTool};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc as Shared;
use vector_ink::kurbo::{self, Arc, BezPath, PathEl, Point};
use vector_ink::{
    bezpath_from_anchors, classify_kind, drag_handle, flatten, flatten_contours, hit_stroke,
    stamp_segment, stroke_mesh, tipped_contours, Anchor, AnchorKind, Cap, HandleEnd, InkMesh, Join,
    StampStyle, StrokeStyle, TipPoint,
};

use super::board::{rgba32, BoardXf};
use super::path_edit_overlay::{
    paint_path_edit_anchors, path_edit_hit, PathEditAnchorColors, PathEditAnchorPaint, PathEditHit,
};
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

/// In-progress multi-click / freehand path gestures. `tips` holds the tool
/// tip (width, color, opacity) each point was placed with, one per point,
/// so a tip chord mid-draw blends from the placed points to the next
/// (P1.curve.tip-chord).
#[derive(Clone, Debug)]
pub enum BoardPathDraft {
    Polyline {
        points: Vec<Pos2>,
        tips: Vec<PlacedTip>,
    },
    Arc {
        points: Vec<Pos2>,
        tips: Vec<PlacedTip>,
    },
    Bezier {
        anchors: Vec<(Pos2, BezierHandles)>,
        tips: Vec<PlacedTip>,
        /// Active click-drag placing an anchor + handles.
        placing: Option<(Pos2, BezierHandles)>,
        /// Anchors taken back by Ctrl+Z while drawing, with their tips,
        /// newest last. Placing a new anchor clears it. Never journaled.
        redo: Vec<((Pos2, BezierHandles), PlacedTip)>,
        /// The latest press placed an anchor (rather than editing one). A
        /// double-click whose second press placed an anchor is two anchors,
        /// not a finish.
        last_press_placed: bool,
        /// Pointer dwell on the start anchor (closing the span).
        close_hover: CloseHover,
    },
}

/// The tips a drawn curve was placed with (P1.curve.tip-chord): none (the
/// tool's tip everywhere), one per grip, or one per vertex.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DrawnTips<'a> {
    None,
    Grips(&'a [PlacedTip]),
    Vertices(&'a [PlacedTip]),
}

/// Hover on a span's start anchor this long before the closed preview shows
/// and a click closes the span (bezier-span.md `bezier.close_hover`).
pub const BEZIER_CLOSE_HOVER_S: f64 = 0.35;

/// Dwell of the pointer on the start anchor of a Bézier draft. Draft state,
/// never journaled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CloseHover {
    /// When the pointer arrived on the start anchor.
    pub since: Option<f64>,
    /// The closed preview is showing: a click on the start closes the span.
    pub ready: bool,
}

/// Incoming / outgoing Bézier control offsets from an anchor point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BezierHandles {
    pub handle_in: Vec2,
    pub handle_out: Vec2,
}

/// Screen-constant minimum drag before a handle is authored (P0 / draft tools).
pub const DRAFT_DRAG_THRESHOLD_PX: f32 = 4.0;

#[derive(Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct CachedInkMesh {
    vertices: Vec<[f32; 2]>,
    alphas: Vec<f32>,
    indices: Vec<u32>,
    /// Per-vertex stroke colors (`InkMesh::colors`); empty paints the
    /// caller's one color.
    colors: Vec<Color32>,
}

impl From<InkMesh> for CachedInkMesh {
    fn from(ink: InkMesh) -> Self {
        CachedInkMesh {
            vertices: ink.vertices.iter().map(|v| v.pos).collect(),
            alphas: ink.vertices.iter().map(|v| v.alpha).collect(),
            indices: ink.indices,
            colors: ink
                .colors
                .iter()
                .map(|c| {
                    let [r, g, b, a] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                    Color32::from_rgba_unmultiplied(r, g, b, a)
                })
                .collect(),
        }
    }
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
                    + mesh.colors.capacity() * std::mem::size_of::<Color32>()
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

/// The path shape a styled closed form paints as, with the shape and box
/// it was derived from.
struct ClosedFormPaint {
    source: ShapeNode,
    rect: WorldRect,
    styled: Option<Shared<ShapeNode>>,
}

pub struct PathMeshCache {
    map: HashMap<GeometryKey, GeometryEntry>,
    clock: VecDeque<GeometryKey>,
    resident_bytes: usize,
    budget_bytes: usize,
    entry_limit: usize,
    closed_forms: HashMap<NodeId, ClosedFormPaint>,
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
            closed_forms: HashMap::new(),
            tess_misses: 0,
        }
    }
}

impl PathMeshCache {
    /// The path shape closed form `id` paints as while it stores per-vertex
    /// style (`vertex_style::closed_form_paint_shape`), derived again only
    /// when the shape or its box changes, so a steady frame allocates
    /// nothing (Art. II).
    pub(crate) fn closed_form_paint_shape(
        &mut self,
        id: NodeId,
        shape: &ShapeNode,
        rect: WorldRect,
    ) -> Option<Shared<ShapeNode>> {
        if shape.path.is_none()
            || slate_doc::vertex_style::closed_form_vertex_count(shape).is_none()
        {
            return None;
        }
        if let Some(entry) = self.closed_forms.get(&id) {
            if entry.rect == rect && entry.source == *shape {
                return entry.styled.clone();
            }
        }
        note_closed_form_derive();
        let styled = slate_doc::vertex_style::closed_form_paint_shape(shape, rect).map(Shared::new);
        if self.closed_forms.len() >= CACHE_ENTRIES && !self.closed_forms.contains_key(&id) {
            self.closed_forms.clear();
        }
        self.closed_forms.insert(
            id,
            ClosedFormPaint {
                source: shape.clone(),
                rect,
                styled: styled.clone(),
            },
        );
        styled
    }

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
        let cached = Shared::new(CachedInkMesh::from(build()));
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

pub fn shape_path_world_bez(node: &Node, shape: &ShapeNode, path: &PathData) -> BezPath {
    if shape.shape == ShapeKind::Path && slate_doc::geom::path_is_line_polyline(path) {
        slate_doc::geom::path_data_to_world_bez_with_fillet(
            path,
            node.rect,
            node.rotation_deg,
            shape.corner,
        )
    } else {
        path_data_to_world_bez(path, node.rect, node.rotation_deg)
    }
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

/// World-space contours as one compound path: the first is the primary
/// contour, the rest become `extra`, all normalized to their shared bounds.
pub fn contours_to_path_data(contours: &[(BezPath, bool)]) -> (WorldRect, PathData) {
    let parts: Vec<(WorldRect, PathData)> = contours
        .iter()
        .map(|(bez, closed)| bezpath_to_path_data(bez, *closed))
        .collect();
    let Some(((first_rect, first), rest)) = parts.split_first() else {
        return bezpath_to_path_data(&BezPath::new(), false);
    };
    let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
    let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (r, _) in &parts {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.w);
        y1 = y1.max(r.y + r.h);
    }
    let rect = WorldRect::new(x0, y0, x1 - x0, y1 - y0);
    let renorm = |p: [f32; 2], from: WorldRect| {
        norm(
            Pos2::new(from.x + p[0] * from.w, from.y + p[1] * from.h),
            rect,
        )
    };
    let segs = |segs: &[PathSeg], from: WorldRect| -> Vec<PathSeg> {
        segs.iter()
            .map(|s| match *s {
                PathSeg::Line { to } => PathSeg::Line {
                    to: renorm(to, from),
                },
                PathSeg::Quad { ctrl, to } => PathSeg::Quad {
                    ctrl: renorm(ctrl, from),
                    to: renorm(to, from),
                },
                PathSeg::Cubic { c1, c2, to } => PathSeg::Cubic {
                    c1: renorm(c1, from),
                    c2: renorm(c2, from),
                    to: renorm(to, from),
                },
            })
            .collect()
    };
    let data = PathData {
        start: renorm(first.start, *first_rect),
        segs: segs(&first.segs, *first_rect),
        closed: first.closed,
        extra: rest
            .iter()
            .map(|(r, d)| slate_doc::scene::PathContour {
                start: renorm(d.start, *r),
                segs: segs(&d.segs, *r),
                closed: d.closed,
            })
            .collect(),
        ..Default::default()
    };
    (rect, data)
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

pub(crate) fn ink_cap(cap: StrokeCap) -> Cap {
    match cap {
        StrokeCap::Butt => Cap::Butt,
        StrokeCap::Round => Cap::Round,
        StrokeCap::Square => Cap::Square,
    }
}

pub fn cap_join_profile(stroke: &Stroke) -> (Cap, Join, Option<vector_ink::Taper>) {
    let cap = ink_cap(stroke.cap);
    let join = match stroke.join {
        StrokeJoin::Miter => Join::Miter,
        StrokeJoin::Round => Join::Round,
        StrokeJoin::Bevel => Join::Bevel,
    };
    (cap, join, stroke.profile.ink_taper())
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
        (tip.texture as u8).hash(h);
    }
    hash_erase_marks(h, &path.erase);
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
    path.corner_amounts.len().hash(h);
    for amount in &path.corner_amounts {
        hash_f32(h, amount.unwrap_or(-1.0));
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

fn hash_erase_marks(h: &mut impl Hasher, marks: &[slate_doc::scene::EraseMark]) {
    marks.len().hash(h);
    for mark in marks {
        mark.points.len().hash(h);
        for p in &mark.points {
            hash_xy(h, *p);
        }
        for tip in &mark.tips {
            hash_f32(h, tip.width);
            hash_f32(h, tip.softness);
            tip.color.0.hash(h);
            (tip.texture as u8).hash(h);
        }
    }
}

/// The eraser pass an eraser preview shows: journal group `pass` of
/// document `tab`. The preview is an honest picture of its stroke while
/// that group is applied, whatever else changed since (a move, a vertex
/// edit); an undo of the pass breaks the seal and a redo mends it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct EraseSeal {
    pub tab: u64,
    pub pass: GroupToken,
}

impl EraseSeal {
    /// The pass was undone. Only the open document's journal answers: a
    /// seal of another document holds.
    fn undone(&self, app: &SlateApp) -> bool {
        self.tab == app.tab().id && !app.tab().journal.is_applied(self.pass)
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
        WidthProfile::Ends { tip } => {
            2u8.hash(h);
            hash_f32(h, tip);
        }
    }
    stroke.arrow_end.hash(h);
    stroke.arrow_start.hash(h);
    stroke.cap_start.map(|c| c as u8).hash(h);
    stroke.cap_end.map(|c| c as u8).hash(h);
    (stroke.texture as u8).hash(h);
    hash_f32(h, stroke.softness);
    hash_f32(h, stroke.gaussian_blur);
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

#[cfg(test)]
thread_local! {
    static CONTENT_HASHED_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CLOSED_FORM_DERIVES_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Styled closed-form paint shapes derived on this thread so far.
#[cfg(test)]
pub(crate) fn closed_form_derives_on_this_thread() -> u64 {
    CLOSED_FORM_DERIVES_HERE.with(|n| n.get())
}

fn note_closed_form_derive() {
    #[cfg(test)]
    CLOSED_FORM_DERIVES_HERE.with(|n| n.set(n.get() + 1));
}

/// Stroke contents hashed on this thread so far.
#[cfg(test)]
pub(crate) fn content_hashed_on_this_thread() -> u64 {
    CONTENT_HASHED_HERE.with(|n| n.get())
}

fn path_content_hash(
    path: &PathData,
    stroke: &Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    corner: slate_doc::scene::Corner,
    bucket: i64,
) -> u64 {
    #[cfg(test)]
    CONTENT_HASHED_HERE.with(|n| n.set(n.get() + 1));
    let mut h = DefaultHasher::new();
    hash_path_data(&mut h, path);
    hash_stroke(&mut h, stroke);
    hash_f32(&mut h, rect.x);
    hash_f32(&mut h, rect.y);
    hash_f32(&mut h, rect.w);
    hash_f32(&mut h, rect.h);
    hash_f32(&mut h, rotation_deg);
    if slate_doc::geom::path_is_line_polyline(path) {
        let (chamfer, amount) = corner.vertex_effective(rect.w, rect.h);
        chamfer.hash(&mut h);
        hash_f32(&mut h, amount);
    }
    bucket.hash(&mut h);
    h.finish()
}

fn path_fill_hash(
    path: &PathData,
    rect: WorldRect,
    rotation_deg: f32,
    corner: slate_doc::scene::Corner,
    bucket: i64,
) -> u64 {
    let mut h = DefaultHasher::new();
    hash_path_data(&mut h, path);
    hash_f32(&mut h, rect.x);
    hash_f32(&mut h, rect.y);
    hash_f32(&mut h, rect.w);
    hash_f32(&mut h, rect.h);
    hash_f32(&mut h, rotation_deg);
    if slate_doc::geom::path_is_line_polyline(path) {
        let (chamfer, amount) = corner.vertex_effective(rect.w, rect.h);
        chamfer.hash(&mut h);
        hash_f32(&mut h, amount);
    }
    bucket.hash(&mut h);
    h.finish()
}

/// `base_color` and the mesh's own tints come unfaded; `fade` applies the
/// node's opacity to each vertex exactly once.
pub(crate) fn ink_mesh_to_epaint(
    cached: &CachedInkMesh,
    xf: &BoardXf,
    base_color: Color32,
    fade: impl Fn(Color32) -> Color32,
) -> egui::Mesh {
    use egui::epaint::{Vertex, WHITE_UV};
    let mut mesh = egui::Mesh::default();
    mesh.vertices.reserve(cached.vertices.len());
    let tinted = cached.colors.len() == cached.vertices.len();
    for (i, (pos, alpha)) in cached.vertices.iter().zip(cached.alphas.iter()).enumerate() {
        let sp = xf.w2s(Pos2::new(pos[0], pos[1]));
        let base = if tinted {
            cached.colors[i]
        } else {
            base_color
        };
        let c = fade(base.gamma_multiply(*alpha));
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
            return Some(shape_path_world_bez(node, shape, path));
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

/// Opacity rides in the stroke color's alpha, so a 0 % stroke still picks
/// by its geometry. Only a shape whose visible fill already picks treats a
/// transparent stroke as no stroke.
fn stroke_picks(shape: &ShapeNode) -> bool {
    let stroke = &shape.stroke;
    let filled = shape.fill.is_some_and(|f| f.0[3] > 0)
        && shape.path.as_ref().is_some_and(|p| p.closed);
    stroke.width > 0.0 && (!stroke.is_none() || stroke.paints_as_stamp() || !filled)
}

/// The ink disc of a single-click brush stroke: world center and radius.
/// `None` for anything that is not a lone stamp dab.
fn brush_dab_disc(node: &Node, shape: &ShapeNode) -> Option<(Pos2, f32)> {
    let path = shape.path.as_ref()?;
    if !path.is_empty() || !stroke_picks(shape) || !shape.stroke.paints_as_stamp() {
        return None;
    }
    let (cx, cy) = node.rect.center();
    Some((Pos2::new(cx, cy), shape.stroke.width.max(0.0) * 0.5))
}

fn hit_brush_dab(node: &Node, shape: &ShapeNode, wx: f32, wy: f32, zoom: f32) -> bool {
    let Some((c, r)) = brush_dab_disc(node, shape) else {
        return false;
    };
    let reach = r + pick_slop_world(zoom);
    c.distance_sq(Pos2::new(wx, wy)) <= reach * reach
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
        shape
            .path
            .as_ref()
            .and_then(|path| (!path.is_empty()).then(|| shape_path_world_bez(node, shape, path)))
    }) else {
        return false;
    };
    let style = stroke_style_world(&shape.stroke, zoom);
    let slop = pick_slop_world(zoom);
    if stroke_picks(shape) && hit_stroke(&bez, &style, [wx, wy], slop) {
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
    let bez = shape_path_world_bez(node, shape, path);
    let style = stroke_style_world(&shape.stroke, zoom);
    let slop = pick_slop_world(zoom);
    if stroke_picks(shape) && hit_stroke(&bez, &style, [wx, wy], slop) {
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
        ShapeKind::RegularPolygon => {
            let outline = slate_doc::geom::regular_polygon_world_outline(
                node.rect,
                node.rotation_deg,
                shape.sides,
                shape.phase_deg,
                shape.corner,
                0.25,
            );
            vector_ink::point_in_polygon(&vec![outline], [wx, wy])
        }
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
pub fn path_world_contours(
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    zoom: f32,
) -> Vec<Vec<Pos2>> {
    if path.is_empty() && !path.closed {
        return Vec::new();
    }
    let bez = shape_path_world_bez(node, shape, path);
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
    shape: &ShapeNode,
    path: &PathData,
    stroke: EStroke,
) {
    for contour in path_world_contours(node, shape, path, xf.z) {
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
            if let Some((c, r)) = brush_dab_disc(node, s) {
                return marquee.contains(c.x - r, c.y - r) && marquee.contains(c.x + r, c.y + r);
            }
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
            if let Some((c, r)) = brush_dab_disc(node, s) {
                let near = Pos2::new(
                    c.x.clamp(marquee.x, marquee.x + marquee.w),
                    c.y.clamp(marquee.y, marquee.y + marquee.h),
                );
                return near.distance_sq(c) <= r * r;
            }
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
        gaussian_blur: 0.0,
        arrow_end: false,
        arrow_start: false,
        cap_start: None,
        cap_end: None,
        texture: Default::default(),
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
        gaussian_blur: 0.0,
        arrow_end: false,
        arrow_start: false,
        cap_start: None,
        cap_end: None,
        texture: Default::default(),
    }
}

/// Start, through point and end of `bez` when it is one open circular arc
/// (`slate_doc::geom::arc_grip_points`).
pub fn arc_grip_points(bez: &BezPath) -> Option<[Pos2; 3]> {
    slate_doc::geom::arc_grip_points(bez).map(|pts| pts.map(from_k))
}

pub fn arc_through_three_points(p0: Pos2, p1: Pos2, p2: Pos2) -> BezPath {
    let mut path = BezPath::new();
    let a = to_k(p0);
    let b = to_k(p1);
    let c = to_k(p2);
    let Some((center, r)) = slate_doc::geom::circle_through(a, b, c) else {
        path.move_to(a);
        path.line_to(c);
        return path;
    };
    let (ux, uy) = (center.x, center.y);
    let ang = |p: Point| (p.y - uy).atan2(p.x - ux);
    let a0 = ang(a);
    let a1 = ang(b);
    let a2_end = ang(c);
    // CCW sweep from start → end in (0, τ]; pick the arc that contains the
    // through-point. Do not re-normalize a negative sweep — kurbo uses the
    // sign to take the long arc when the middle lies on that side of the chord.
    let mut sweep_ccw = a2_end - a0;
    while sweep_ccw <= 0.0 {
        sweep_ccw += std::f64::consts::TAU;
    }
    while sweep_ccw > std::f64::consts::TAU {
        sweep_ccw -= std::f64::consts::TAU;
    }
    let mut mid_ccw = a1 - a0;
    while mid_ccw < 0.0 {
        mid_ccw += std::f64::consts::TAU;
    }
    while mid_ccw >= std::f64::consts::TAU {
        mid_ccw -= std::f64::consts::TAU;
    }
    let sweep_angle = if mid_ccw <= sweep_ccw + 1e-10 {
        sweep_ccw
    } else {
        sweep_ccw - std::f64::consts::TAU
    };
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

/// Open span through the draft anchors. A segment whose facing handles are
/// both zero is straight (`vector_ink::bezpath_from_anchors`).
pub fn bezier_anchors_to_bezpath(anchors: &[(Pos2, BezierHandles)]) -> BezPath {
    bezpath_from_anchors(&bezier_draft_to_ink(anchors), false)
}

/// Closed span through the draft anchors. The closing segment arrives on the
/// start anchor through its own incoming handle, so a weighted start joins
/// smoothly and a start with no weights can kink.
pub fn bezier_anchors_to_closed_bezpath(anchors: &[(Pos2, BezierHandles)]) -> BezPath {
    bezpath_from_anchors(&bezier_draft_to_ink(anchors), true)
}

/// Draft anchors (handle offsets) as `vector_ink` anchors (absolute handle
/// points). A zero offset is no handle.
pub(crate) fn bezier_draft_to_ink(anchors: &[(Pos2, BezierHandles)]) -> Vec<Anchor> {
    anchors
        .iter()
        .map(|(p, h)| {
            let handle = |off: Vec2| (off.length_sq() > 0.0).then(|| to_k(*p + off));
            let mut a = Anchor {
                point: to_k(*p),
                handle_in: handle(h.handle_in),
                handle_out: handle(h.handle_out),
                kind: AnchorKind::Corner,
            };
            a.kind = classify_kind(&a);
            a
        })
        .collect()
}

fn bezier_draft_from_ink(anchors: &[Anchor]) -> Vec<(Pos2, BezierHandles)> {
    anchors
        .iter()
        .map(|a| {
            let p = from_k(a.point);
            let offset = |h: Option<Point>| h.map_or(Vec2::ZERO, |h| from_k(h) - p);
            (
                p,
                BezierHandles {
                    handle_in: offset(a.handle_in),
                    handle_out: offset(a.handle_out),
                },
            )
        })
        .collect()
}

/// Painted adornment for the placed draft anchors (plus the anchor being
/// placed). Shared by paint and by the draft's press hit-test.
pub(crate) fn bezier_draft_overlay(
    xf: &BoardXf,
    anchors: &[(Pos2, BezierHandles)],
    placing: Option<(Pos2, BezierHandles)>,
    close_first_anchor: bool,
) -> Vec<PathEditAnchorPaint> {
    let knob = |p: Pos2, off: Vec2| (off.length_sq() > 0.0).then(|| xf.w2s(p + off));
    let mut overlay: Vec<PathEditAnchorPaint> = anchors
        .iter()
        .enumerate()
        .map(|(i, (p, h))| PathEditAnchorPaint {
            point: xf.w2s(*p),
            handle_in: knob(*p, h.handle_in),
            handle_out: knob(*p, h.handle_out),
            selected: false,
            smooth_hint: h.handle_in.length_sq() > 0.0
                && h.handle_out.length_sq() > 0.0
                && (h.handle_in + h.handle_out).length_sq() < 1e-4,
            close_hint: i == 0 && close_first_anchor,
        })
        .collect();
    if let Some((a, h)) = placing {
        overlay.push(PathEditAnchorPaint {
            point: xf.w2s(a),
            handle_in: knob(a, h.handle_in),
            handle_out: knob(a, h.handle_out),
            selected: true,
            smooth_hint: false,
            close_hint: false,
        });
    }
    overlay
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
        let fill_key = path_fill_hash(
            path,
            node.rect,
            node.rotation_deg,
            shape.corner,
            zoom_bucket(xf.z),
        );
        let triangles = app.path_mesh_cache.get_or_fill_tris(node.id, fill_key, || {
            let bez = bez.get_or_insert_with(|| shape_path_world_bez(node, shape, path));
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
    let key = path_content_hash(
        path,
        &shape.stroke,
        node.rect,
        node.rotation_deg,
        shape.corner,
        bucket,
    );
    let cached = app.path_mesh_cache.get_or_tessellate(node.id, key, || {
        let bez = bez.get_or_insert_with(|| shape_path_world_bez(node, shape, path));
        vector_stroke_ink_for(node, bez, shape, path, xf.z)
    });
    let mesh = ink_mesh_to_epaint(&cached, xf, rgba32(shape.stroke.color), fade);
    painter.add(Shape::mesh(mesh));
}

/// The arrowhead at end `end` (0 = first point, 1 = last) of an open curve
/// ([`slate_doc::geom::arrow_head`] aimed by [`slate_doc::geom::path_arrow`]),
/// added to its stroke ink with the same feathered edge so it caches with
/// the stroke. `color` is that end's tip on a stroke with per-vertex colors.
pub(crate) fn push_arrow_ink(
    ink: &mut InkMesh,
    bez: &BezPath,
    end: usize,
    width: f32,
    feather: f32,
    color: Option<[f32; 4]>,
) {
    let Some((tip, into)) = slate_doc::geom::path_arrow(bez, end, width) else {
        return;
    };
    let tri = slate_doc::geom::arrow_head(tip, into, width);
    let unit = |v: [f32; 2]| {
        let l = v[0].hypot(v[1]).max(1e-6);
        [v[0] / l, v[1] / l]
    };
    let first = ink.vertices.len() as u32;
    for p in tri {
        ink.vertices.push(vector_ink::InkVertex { pos: p, alpha: 1.0 });
    }
    for i in 0..3 {
        let p = tri[i];
        let a = unit([tri[(i + 1) % 3][0] - p[0], tri[(i + 1) % 3][1] - p[1]]);
        let b = unit([tri[(i + 2) % 3][0] - p[0], tri[(i + 2) % 3][1] - p[1]]);
        let out = unit([-(a[0] + b[0]), -(a[1] + b[1])]);
        let half = ((a[0] * b[0] + a[1] * b[1]).clamp(-1.0, 1.0).acos() * 0.5).sin();
        let d = feather / half.max(0.2);
        ink.vertices.push(vector_ink::InkVertex {
            pos: [p[0] + out[0] * d, p[1] + out[1] * d],
            alpha: 0.0,
        });
    }
    ink.indices.extend([first, first + 1, first + 2]);
    for i in 0..3u32 {
        let j = (i + 1) % 3;
        ink.indices.extend([first + i, first + j, first + 3 + j]);
        ink.indices.extend([first + i, first + 3 + j, first + 3 + i]);
    }
    if !ink.colors.is_empty() {
        let c = color.or_else(|| ink.colors.last().copied()).unwrap_or([1.0; 4]);
        ink.colors.resize(ink.vertices.len(), c);
    }
}

/// World-space mesh of a vector path stroke. A path with per-vertex tips
/// (`PathData::vector_widths`) paints its varying width.
#[cfg(test)]
pub(crate) fn vector_stroke_ink(
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    zoom: f32,
) -> InkMesh {
    vector_stroke_ink_for(
        node,
        &shape_path_world_bez(node, shape, path),
        shape,
        path,
        zoom,
    )
}

fn vector_stroke_ink_for(
    node: &Node,
    bez: &BezPath,
    shape: &ShapeNode,
    path: &PathData,
    zoom: f32,
) -> InkMesh {
    let arrows = if path.closed {
        [false; 2]
    } else {
        shape.stroke.arrows()
    };
    let ends = shape.stroke.end_caps().map(ink_cap);
    let full = bez;
    let trimmed_body;
    let bez = if arrows.contains(&true) {
        let w = shape.stroke.width;
        trimmed_body = slate_doc::geom::trim_arrow_ends(bez, arrows, [w, w]);
        &trimmed_body
    } else {
        bez
    };
    let mut style = stroke_style_world(&shape.stroke, zoom);
    let (ink_width, soft) = shape.stroke.paint_profile();
    style.width = ink_width;
    let feather = soft
        + if soft <= 0.0 {
            FEATHER_PX / zoom.max(0.05)
        } else {
            0.0
        };
    let tolerance = curve_tolerance(zoom);
    let tipped = (soft <= 0.0)
        .then(|| {
            slate_doc::geom::tipped_stroke(
                path,
                &shape.stroke,
                node.rect,
                node.rotation_deg,
                shape.corner,
            )
        })
        .flatten();
    match tipped {
        Some(t) => {
            let body = slate_doc::geom::trim_tipped_arrow_ends(&t, arrows);
            let mut ink = vector_ink::stroke_mesh_ends(
                &body.bez,
                &style,
                ends,
                Some(&body.widths),
                body.colors.as_deref(),
                body.ease,
                feather,
                tolerance,
            );
            for end in (0..2).filter(|&i| arrows[i]) {
                let pick = |v: &[f32]| if end == 0 { v.first() } else { v.last() }.copied();
                let head = pick(&t.widths).unwrap_or(shape.stroke.width);
                let color = t.colors.as_ref().and_then(|c| {
                    if end == 0 { c.first() } else { c.last() }.copied()
                });
                push_arrow_ink(&mut ink, &t.bez, end, head, feather, color);
            }
            ink
        }
        None => {
            let mut ink = vector_ink::stroke_mesh_ends(
                bez,
                &style,
                ends,
                None,
                None,
                vector_ink::TipEase::Linear,
                feather,
                tolerance,
            );
            for end in (0..2).filter(|&i| arrows[i]) {
                push_arrow_ink(&mut ink, full, end, shape.stroke.width, feather, None);
            }
            ink
        }
    }
}

/// The Pen's live stroke as one mesh: its samples, each with the tip it was
/// drawn with (`tips`), then the pointer with `cursor_tip`. It blends
/// between them like any stroke tool's draft (`draft_stroke_ink`). The
/// board paints the same stroke in pieces (`DraftInkCache::paint_pen`).
#[cfg(test)]
pub(crate) fn pen_preview_ink(
    pts: &[Pos2],
    tips: &[PlacedTip],
    cursor: Pos2,
    cursor_tip: PlacedTip,
    zoom: f32,
) -> (InkMesh, Color32) {
    if pts.is_empty() {
        return (InkMesh::default(), Color32::TRANSPARENT);
    }
    let mut bez = BezPath::new();
    bez.move_to(to_k(pts[0]));
    for p in pts[1..].iter().chain(std::iter::once(&cursor)) {
        bez.line_to(to_k(*p));
    }
    let tips: Vec<PlacedTip> = (0..pts.len())
        .map(|i| tips.get(i).copied().unwrap_or(cursor_tip))
        .chain(std::iter::once(cursor_tip))
        .collect();
    draft_stroke_ink(&bez, false, &tips, zoom)
}

/// Blending between two tips of a live freehand stroke.
pub trait TipBlend: Copy + PartialEq {
    fn blend(a: Self, b: Self, t: f32) -> Self;
    fn width(&self) -> f32;
}

impl TipBlend for PlacedTip {
    fn blend(a: Self, b: Self, t: f32) -> Self {
        PlacedTip::lerp(a, b, t)
    }
    fn width(&self) -> f32 {
        self.width
    }
}

impl TipBlend for StrokeSpan {
    fn blend(a: Self, b: Self, t: f32) -> Self {
        slate_doc::vertex_style::lerp_span(a, b, t)
    }
    fn width(&self) -> f32 {
        self.width
    }
}

/// Screen px over which a freehand stroke blends into a tip changed
/// mid-stroke (at least the wider tip's own width).
pub(crate) const TIP_BLEND_PX: f32 = 24.0;

/// A freehand stroke's samples and the tip each is drawn with
/// (P1.curve.tip-chord). A tip change mid-stroke does not step: the samples
/// after it blend from the tip last drawn to the new one by smoothstep over
/// [`TIP_BLEND_PX`], and each blend's ends are fit breaks, so the committed
/// curve has a vertex at both and paints the blend the preview showed.
#[derive(Clone, Debug)]
pub struct FreehandTips<T> {
    pub points: Vec<Pos2>,
    pub tips: Vec<T>,
    along: Vec<f32>,
    target: T,
    /// Blend in progress: the tip it leaves, where it starts, its length.
    ramp: Option<(T, f32, f32)>,
    /// Sample indices where the fit splits: every blend's two ends.
    pub breaks: Vec<usize>,
}

impl<T: TipBlend> FreehandTips<T> {
    pub fn new(at: Pos2, tip: T) -> Self {
        FreehandTips {
            points: vec![at],
            tips: vec![tip],
            along: vec![0.0],
            target: tip,
            ramp: None,
            breaks: Vec::new(),
        }
    }

    pub fn last(&self) -> Pos2 {
        *self.points.last().expect("a freehand stroke has a sample")
    }

    /// The tip a sample at `at` would be drawn with, the tool's tip now
    /// being `tip`.
    pub fn tip_at(&self, at: Pos2, tip: T) -> T {
        if tip != self.target {
            return *self.tips.last().expect("a sample");
        }
        match self.ramp {
            Some((from, start, len)) => {
                let d = self.along.last().copied().unwrap_or(0.0) + (at - self.last()).length();
                T::blend(from, tip, ease((d - start) / len))
            }
            None => tip,
        }
    }

    /// Add a sample at `at`; `tip` is the tool's tip now.
    pub fn push(&mut self, at: Pos2, tip: T, zoom: f32) {
        let last = self.points.len() - 1;
        if tip != self.target {
            let from = self.tips[last];
            let len = (TIP_BLEND_PX / zoom.max(f32::EPSILON))
                .max(from.width())
                .max(tip.width());
            self.ramp = Some((from, self.along[last], len));
            if self.breaks.last() != Some(&last) {
                self.breaks.push(last);
            }
            self.target = tip;
        }
        let d = self.along[last] + (at - self.points[last]).length();
        self.points.push(at);
        self.along.push(d);
        let shown = match self.ramp {
            Some((from, start, len)) if d - start < len => {
                T::blend(from, self.target, ease((d - start) / len))
            }
            Some(_) => {
                self.ramp = None;
                self.breaks.push(self.points.len() - 1);
                self.target
            }
            None => self.target,
        };
        self.tips.push(shown);
    }

    /// Move the last sample to `at` (the release point), keeping its tip.
    pub fn end_at(&mut self, at: Pos2, zoom: f32) {
        let before = self.points.len();
        let tip = self.target;
        let min = FREEHAND_SAMPLE_SPACING_PX / zoom.max(f32::EPSILON);
        if before > 1 && (self.last() - at).length() < min {
            let i = before - 1;
            self.along[i] = self.along[i - 1] + (at - self.points[i - 1]).length();
            self.points[i] = at;
        } else if self.last() != at {
            self.push(at, tip, zoom);
        }
    }

    /// A stroke whose tips step where they change, with a break at each
    /// step's last old sample. `None` when `tips` does not fit `points`.
    #[cfg(test)]
    pub fn stepped(points: Vec<Pos2>, tips: Vec<T>) -> Option<Self> {
        if points.is_empty() || tips.len() != points.len() {
            return None;
        }
        let mut along = vec![0.0];
        for w in points.windows(2) {
            along.push(along.last().copied().unwrap_or(0.0) + (w[1] - w[0]).length());
        }
        let breaks = (0..tips.len() - 1)
            .filter(|&i| tips[i] != tips[i + 1])
            .collect();
        Some(FreehandTips {
            target: *tips.last().expect("a sample"),
            points,
            tips,
            along,
            ramp: None,
            breaks,
        })
    }

    /// The tip the stroke paints at arc length `d` from its start.
    fn tip_along(&self, d: f32) -> T {
        let k = self.along.partition_point(|x| *x <= d);
        if k == 0 {
            return self.tips[0];
        }
        if k >= self.along.len() {
            return *self.tips.last().expect("a sample");
        }
        let (a, b) = (self.along[k - 1], self.along[k]);
        let t = if b > a { (d - a) / (b - a) } else { 1.0 };
        T::blend(self.tips[k - 1], self.tips[k], t)
    }

    /// Fit the stroke, splitting at its breaks, and give each fitted vertex
    /// the tip drawn at its spot. `None` when nothing fits.
    pub fn fit(&self, tol: f32, spacing: f32) -> Option<(BezPath, Vec<T>)> {
        if self.points.len() < 2 {
            return None;
        }
        let flat: Vec<[f32; 2]> = self.points.iter().map(|p| [p.x, p.y]).collect();
        let end = flat.len() - 1;
        let mut cuts = vec![0];
        cuts.extend(self.breaks.iter().copied().filter(|b| *b > 0 && *b < end));
        cuts.push(end);
        cuts.dedup();
        let mut bez = BezPath::new();
        let mut tips = vec![self.tips[0]];
        bez.move_to(to_k(self.points[0]));
        for w in cuts.windows(2) {
            let (s, e) = (w[0], w[1]);
            let run = vector_ink::fit_polyline_spaced(&flat[s..=e], tol, spacing);
            let els: Vec<PathEl> = run
                .elements()
                .iter()
                .skip(1)
                .filter(|el| !matches!(el, PathEl::MoveTo(_) | PathEl::ClosePath))
                .copied()
                .collect();
            let mut lengths = Vec::with_capacity(els.len());
            let (mut acc, mut prev) = (0.0_f64, to_k(self.points[s]));
            for el in &els {
                let seg = match *el {
                    PathEl::LineTo(p) => kurbo::PathSeg::Line(kurbo::Line::new(prev, p)),
                    PathEl::QuadTo(c, p) => kurbo::PathSeg::Quad(kurbo::QuadBez::new(prev, c, p)),
                    PathEl::CurveTo(c1, c2, p) => {
                        kurbo::PathSeg::Cubic(kurbo::CubicBez::new(prev, c1, c2, p))
                    }
                    _ => continue,
                };
                acc += kurbo::ParamCurveArclen::arclen(&seg, 1e-3);
                prev = kurbo::ParamCurve::end(&seg);
                lengths.push(acc);
            }
            let (a, b) = (self.along[s], self.along[e]);
            for (el, len) in els.iter().zip(lengths) {
                bez.push(*el);
                let f = if acc > 1e-9 { (len / acc) as f32 } else { 1.0 };
                tips.push(if f >= 1.0 {
                    self.tips[e]
                } else {
                    self.tip_along(a + (b - a) * f)
                });
            }
        }
        (tips.len() >= 2).then_some((bez, tips))
    }
}

fn ease(t: f32) -> f32 {
    vector_ink::TipEase::Smooth.weight(t)
}

/// World units per stamp pixel at `zoom`: one physical screen pixel, snapped
/// down to a power of two so small zoom changes reuse the same bitmap.
pub(crate) fn stamp_pixel_for_zoom(zoom: f32, pixels_per_point: f32) -> f32 {
    let screen = 1.0 / (zoom * pixels_per_point).max(1.0e-3);
    2f32.powf(screen.log2().floor().clamp(-5.0, 8.0))
}

fn stamp_style(tip: StrokeSpan) -> StampStyle {
    tip.stamp_style()
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

/// Stamp bitmap pixels the frame loop may rasterize in one frame. Anything
/// larger builds on the raster workers while a stand-in paints (Art. II):
/// a big stroke's commit, erase, smooth, or zoom never stalls the frame.
const SYNC_STAMP_PX: f32 = 256.0 * 256.0;

/// Bitmap pixels stroke `node` needs at `pixel` world units per pixel.
fn stamp_area_px(node: &Node, shape: &ShapeNode, pixel: f32) -> f32 {
    let r = tiles::ink_rect(node, shape);
    let px = pixel.max(1.0e-3);
    ((r[2] - r[0]) / px).max(1.0) * ((r[3] - r[1]) / px).max(1.0)
}

/// Spend this frame's synchronous raster budget on `area` pixels, if it fits.
pub(crate) fn take_sync_budget(app: &mut SlateApp, area: f32) -> bool {
    if app.stamp_sync_px + area > SYNC_STAMP_PX {
        return false;
    }
    app.stamp_sync_px += area;
    app.stamp_sync_builds += 1;
    true
}

/// [`stamp_key`] for a stamped path node.
pub(crate) fn node_stamp_key(node: &Node) -> Option<u64> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    Some(stamp_key(node, shape, shape.path.as_ref()?))
}

/// A stroke's stamp content key: path, style, and placement.
fn stamp_key(node: &Node, shape: &ShapeNode, path: &PathData) -> u64 {
    stamp_key_at(node, shape, path, node.rect)
}

/// [`stamp_key`] with the stroke placed at `rect`.
fn stamp_key_at(node: &Node, shape: &ShapeNode, path: &PathData, rect: WorldRect) -> u64 {
    path_content_hash(
        path,
        &shape.stroke,
        rect,
        node.rotation_deg,
        shape.corner,
        0,
    ) ^ 0x57A5
}

/// Start the live eraser preview for strokes the pass reached. Small
/// strokes build on the frame loop within [`SYNC_STAMP_PX`]; others build on
/// the raster workers and keep painting as committed until theirs lands.
pub(crate) fn ensure_erase_live(app: &mut SlateApp, painter: &egui::Painter, xf: &BoardXf) {
    let Some(super::board::BoardDrag::Erase { spot, .. }) = &app.board_drag else {
        return;
    };
    let waiting: Vec<NodeId> = spot
        .iter()
        .filter(|id| !app.erase_live.contains_key(id))
        .copied()
        .collect();
    let want = stamp_pixel_for_zoom(xf.z, painter.ctx().pixels_per_point());
    for id in waiting {
        let Some(node) = app.doc().scene.node(id).cloned() else {
            continue;
        };
        start_erase_live(app, painter, &node, want);
    }
}

/// A straight eraser pass released before a reached stroke's final cut
/// landed (Art. II). The pass's erase marks are committed at release and
/// nothing stamps on the frame loop for them. Until its cut lands, a stroke
/// with a preview paints that preview; a stroke released before its
/// preview existed paints as the scene has it until its new raster lands.
/// The eraser's band covers the part of the pass not cut yet, as during
/// the drag. A stroke any pass changed without knowing whether ink is left
/// is removed once the workers find none: in that pass's undo step, or in
/// the step of a later eraser pass that committed first.
///
/// A settling stroke leaves only to something that shows the same or newer
/// content: its preview once the cut lands (as its stand-in), a newer
/// pass's preview once that pass commits, or its own new raster. Strokes
/// settle per document, so a tab switch keeps them, and closing the tab
/// drops them.
#[derive(Default)]
pub struct EraseSettle {
    /// The document whose ink checks are pending.
    tab: Option<u64>,
    /// Previews waiting for their pass's final cut, by document and stroke.
    live: HashMap<(u64, NodeId), Settling>,
    /// Bands over passes a stroke's paint does not show cut yet.
    waiting: Vec<Waiting>,
    /// Strokes a pass changed past the frame's raster budget, straight or
    /// freehand, while the workers find out whether any ink is left: the
    /// committed content key and the pass's journal group.
    checks: Vec<(NodeId, u64, GroupToken)>,
    /// Test hook: settling previews, and the release, take no landed cut.
    #[cfg(test)]
    pub(crate) hold: bool,
}

/// A band over straight pass `seg` of stroke `id` in document `tab`, until
/// the stroke shows its raster for its content in view.
struct Waiting {
    tab: u64,
    id: NodeId,
    seg: Seg,
    /// The journal group of the pass the band covers.
    pass: GroupToken,
    /// The stroke's committed content keys the band covers: its own
    /// pass's, then each later pass's released over it.
    keys: Vec<u64>,
    /// Painted this frame: its pass is applied, and its stroke is in the
    /// scene, shows one of `keys`, and is not hidden, or still ghosts out.
    shown: bool,
    /// The stroke's ink is in the open document's view this frame, waiting
    /// on its raster.
    seen: bool,
    /// The scene revision the stroke's content key was last read at, and
    /// that key: it is read again only once the scene changes.
    checked: Option<((u64, u64), u64)>,
}

/// A released pass's preview while its final cut is on the workers.
struct Settling {
    /// The stroke's committed content key.
    key: u64,
    /// Each content key the stroke was committed under since the release,
    /// the pass's own first and `key` last.
    keys: Vec<u64>,
    /// What the preview shows once its cut lands: the stroke's content
    /// key at release, placed at `rect`, with its pass journal group
    /// `pass` applied.
    shows: u64,
    rect: WorldRect,
    pass: GroupToken,
    live: EraseLive,
    /// Later straight passes released before their preview existed,
    /// placed for `rect`, each with its journal group and the index in
    /// `keys` of its own content key: the band covers them until the
    /// stroke's raster for `key` lands.
    bands: Vec<(Seg, GroupToken, usize)>,
    /// How far the stroke moved from `rect`: the preview follows it.
    shift: [f32; 2],
    /// The stroke is not hidden, or still ghosts out, this frame.
    shown: bool,
}

impl Settling {
    /// Its preview's line and each later band, as bands of stroke `id` in
    /// document `tab` that wait on the stroke's raster.
    fn take_bands(&mut self, tab: u64, id: NodeId, shown: bool, seen: bool) -> Vec<Waiting> {
        let mut bands = std::mem::take(&mut self.bands);
        bands.extend(self.live.line.map(|seg| (seg, self.pass, 0)));
        bands
            .into_iter()
            .map(|(seg, pass, from)| Waiting {
                tab,
                id,
                seg: shift_seg(seg, self.shift),
                pass,
                keys: self.keys[from..].to_vec(),
                shown,
                seen,
                checked: None,
            })
            .collect()
    }
}

fn shift_seg((a, b): Seg, d: [f32; 2]) -> Seg {
    let at = |p: TipPoint| TipPoint {
        pos: [p.pos[0] + d[0], p.pos[1] + d[1]],
        ..p
    };
    (at(a), at(b))
}

impl EraseSettle {
    pub(crate) fn is_empty(&self) -> bool {
        self.live.is_empty() && self.waiting.is_empty() && self.checks.is_empty()
    }

    /// A settling preview, an ink check, or a band in view that waits on
    /// its stroke's raster, as of the last frame.
    #[cfg(test)]
    pub(crate) fn settling(&self) -> bool {
        !self.live.is_empty() || !self.checks.is_empty() || self.waiting.iter().any(|w| w.seen)
    }

    /// Stroke `id`, committed under content `key` by journal group `token`,
    /// leaves the scene in that group if the workers find no ink left.
    pub(crate) fn check(&mut self, id: NodeId, key: u64, token: GroupToken) {
        self.checks.retain(|c| c.0 != id);
        self.checks.push((id, key, token));
    }

    pub(crate) fn take_checks(&mut self) -> Vec<(NodeId, u64, GroupToken)> {
        std::mem::take(&mut self.checks)
    }

    /// Take out the checks the workers have answered: stroke, committed
    /// content key, pass, and whether ink is left. The others stay as they
    /// are, and cost nothing while they wait.
    pub(crate) fn take_answered(
        &mut self,
        tiles: &mut tiles::BrushTiles,
    ) -> Vec<(NodeId, u64, GroupToken, bool)> {
        let mut answered = Vec::new();
        self.checks
            .retain(|&(id, key, token)| match tiles.ink_answer(id, key) {
                Some(left) => {
                    answered.push((id, key, token, left));
                    false
                }
                None => true,
            });
        answered
    }

    pub(crate) fn has_checks(&self) -> bool {
        !self.checks.is_empty()
    }

    /// Eraser line jobs, `(lane, tag)`, a settling preview still wants.
    pub(crate) fn jobs(&self) -> Vec<(u64, u64)> {
        self.live
            .iter()
            .filter_map(|((_, id), s)| s.live.job(tiles::erase_lane(*id)))
            .collect()
    }

    /// A settling preview paints stroke `id` of document `tab`, not the
    /// tiles.
    pub(crate) fn holds(&self, tab: u64, id: NodeId) -> bool {
        self.live.contains_key(&(tab, id))
    }

    /// The release and settling previews take no landed cut (test hook).
    pub(crate) fn held(&self) -> bool {
        #[cfg(test)]
        return self.hold;
        #[cfg(not(test))]
        false
    }

    /// Decide ink checks for document `tab`, dropping another document's
    /// (its strokes stay as committed). Settling strokes stay with their
    /// document.
    pub(crate) fn here(&mut self, tab: u64, tiles: &mut tiles::BrushTiles) -> &mut Self {
        if self.tab != Some(tab) {
            for (id, ..) in self.checks.drain(..) {
                tiles.forget_ink(id);
            }
            self.tab = Some(tab);
        }
        self
    }

    /// Document `tab` closed: drop its settling strokes, their cuts on the
    /// workers, and its ink checks.
    pub(crate) fn forget_tab(&mut self, tab: u64, tiles: &mut tiles::BrushTiles) {
        self.live.retain(|&(t, id), s| {
            if t == tab {
                s.live.forget(tiles, tiles::erase_lane(id));
            }
            t != tab
        });
        self.waiting.retain(|w| w.tab != tab);
        if self.tab == Some(tab) {
            for (id, ..) in self.checks.drain(..) {
                tiles.forget_ink(id);
            }
            self.tab = None;
        }
    }

    /// Stroke `node` of document `tab`, committed under content `key` by
    /// journal group `pass`, paints `live` until its final cut lands. It
    /// replaces what settled for the stroke before: `live` was built on the
    /// stroke as committed.
    pub(crate) fn hold(
        &mut self,
        tab: u64,
        node: &Node,
        (key, pass): (u64, GroupToken),
        live: EraseLive,
        tiles: &mut tiles::BrushTiles,
    ) {
        self.release(tab, node.id, tiles);
        let s = Settling {
            key,
            keys: vec![key],
            shows: key,
            rect: node.rect,
            pass,
            live,
            bands: Vec::new(),
            shift: [0.0; 2],
            shown: true,
        };
        self.live.insert((tab, node.id), s);
    }

    /// Stroke `node` of document `tab`, committed under content `key` by
    /// journal group `pass` with no preview, keeps the band over the
    /// straight pass along `points` until its new raster lands. A preview
    /// still settling for it keeps showing the earlier passes under the
    /// band, and so does an earlier band.
    pub(crate) fn wait(
        &mut self,
        tab: u64,
        node: &Node,
        (key, pass): (u64, GroupToken),
        points: &[Pos2],
        tip: StampStyle,
    ) {
        self.extend_keys(tab, node.id, key);
        let Some(seg) = straight_seg(points, tip) else {
            return;
        };
        if let Some(s) = self.live.get_mut(&(tab, node.id)) {
            let back = [s.rect.x - node.rect.x, s.rect.y - node.rect.y];
            s.bands.push((shift_seg(seg, back), pass, s.keys.len() - 1));
            return;
        }
        self.waiting.push(Waiting {
            tab,
            id: node.id,
            seg,
            pass,
            keys: vec![key],
            shown: true,
            seen: true,
            checked: None,
        });
    }

    /// Stroke `id` of document `tab` was committed under content `key` by
    /// a later eraser pass with no preview, straight or freehand: the
    /// stroke's settling preview and bands cover that content too.
    pub(crate) fn extend_keys(&mut self, tab: u64, id: NodeId, key: u64) {
        if let Some(s) = self.live.get_mut(&(tab, id)) {
            s.key = key;
            s.keys.push(key);
            return;
        }
        for w in self.waiting.iter_mut().filter(|w| (w.tab, w.id) == (tab, id)) {
            w.keys.push(key);
        }
    }

    /// Stop settling stroke `id` of document `tab`: a newer pass's preview
    /// shows it.
    pub(crate) fn release(&mut self, tab: u64, id: NodeId, tiles: &mut tiles::BrushTiles) {
        if let Some(mut s) = self.live.remove(&(tab, id)) {
            s.live.forget(tiles, tiles::erase_lane(id));
        }
        self.waiting.retain(|w| (w.tab, w.id) != (tab, id));
    }

    /// Each settling stroke of document `tab`, whose journal is `journal`,
    /// that paints and that no live preview paints: its applied passes,
    /// and where along each the uncut part starts.
    fn rests<'a>(
        &'a self,
        tab: u64,
        live: &'a HashMap<NodeId, EraseLive>,
        journal: &'a slate_doc::scene::SceneJournal,
    ) -> impl Iterator<Item = (Seg, TipPoint, bool)> + Clone + 'a {
        let settling = self
            .live
            .iter()
            .filter(move |(k, s)| k.0 == tab && s.shown && !live.contains_key(&k.1))
            .flat_map(move |(_, s)| {
                let cut = s.live.line.and_then(|seg| {
                    let (from, cut) = s.live.uncovered(seg)?;
                    let seg = shift_seg(seg, s.shift);
                    Some((seg, shift_seg((from, from), s.shift).0, cut))
                });
                let bands = s
                    .bands
                    .iter()
                    .filter(move |b| journal.is_applied(b.1))
                    .map(move |b| {
                        let seg = shift_seg(b.0, s.shift);
                        (seg, seg.0, false)
                    });
                cut.into_iter().chain(bands)
            });
        let waiting = self
            .waiting
            .iter()
            .filter(move |w| w.tab == tab && w.shown && !live.contains_key(&w.id))
            .map(|w| (w.seg, w.seg.0, false));
        settling.chain(waiting)
    }
}

/// Where settling preview `s` paints for stroke `node` of the open
/// document, whose journal is `journal`: shifted by how far the stroke
/// moved, or at its own place after any other change. `None` once its
/// pass was undone.
fn settling_fit(
    node: &Node,
    s: &Settling,
    journal: &slate_doc::scene::SceneJournal,
) -> Option<[f32; 2]> {
    if !journal.is_applied(s.pass) {
        return None;
    }
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    let path = shape.path.as_ref()?;
    let (r, o) = (node.rect, s.rect);
    let moved = (r.x, r.y) != (o.x, o.y)
        && (r.w, r.h) == (o.w, o.h)
        && stamp_key_at(node, shape, path, o) == s.key;
    Some(if moved { [r.x - o.x, r.y - o.y] } else { [0.0; 2] })
}

/// Stroke `node` while its released straight pass waits for the final cut:
/// its preview ([`tend_erase_settle`] ends the settle).
fn paint_settling_erase(
    app: &SlateApp,
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    fade: &impl Fn(Color32) -> Color32,
) -> bool {
    let Some(s) = app.erase_settle.live.get(&(app.tab().id, node.id)) else {
        return false;
    };
    let Some(shift) = settling_fit(node, s, &app.tab().journal) else {
        return false;
    };
    s.live.paint(painter, xf, fade(Color32::WHITE), shift);
    true
}

/// A band over stroke `n` counts as in `view` only while the board paints
/// the stroke there: it is a candidate of the paint query
/// ([`super::board::in_paint_query`]) and passes the paint cull
/// ([`super::board::paints_in_view`]). Since that cull keeps every rotated
/// candidate, a rotated stroke also needs its rotated ink box
/// ([`tiles::ink_rect`]) to meet the view.
pub(crate) fn band_in_view(n: &Node, view: &WorldRect) -> bool {
    if !super::board::paints_in_view(n, view) || !super::board::in_paint_query(n, view) {
        return false;
    }
    match &n.kind {
        NodeKind::Shape(shape) if n.rotation_deg.abs() > 0.01 => {
            let r = tiles::ink_rect(n, shape);
            WorldRect::new(r[0], r[1], r[2] - r[0], r[3] - r[1]).intersects(view)
        }
        _ => true,
    }
}

/// Pump every settling preview, visible or not, and end the settle of
/// strokes whose cut or new raster has landed, or whose pass was undone or
/// that left the scene since the release. A settled preview becomes the
/// stroke's stand-in until its new raster lands, in its own document too
/// when that is not the open one; so it does when the workers gave up. A
/// band stays, in view or not, until its stroke shows its raster for its
/// content in view, the stroke changes other than by an eraser pass, or
/// its pass is undone and a new edit drops the redo; it paints only where
/// its stroke paints. While its stroke is out of the scene and its pass
/// applied, a band waits unseen. Undoing a pass that still settles, or
/// removing its stroke, turns its preview's line and bands into such
/// bands. A band reads its stroke's content only once the scene changed
/// since it last did. Frames are asked for only while this document has a
/// cut on the workers or a band waiting on a raster for a stroke whose ink
/// is in view ([`band_in_view`]).
fn tend_erase_settle(app: &mut SlateApp, painter: &egui::Painter, xf: &BoardXf) {
    if app.erase_settle.is_empty() {
        return;
    }
    let tab = app.tab().id;
    app.erase_settle.here(tab, &mut app.brush_tiles);
    let want = stamp_pixel_for_zoom(xf.z, painter.ctx().pixels_per_point());
    let view = app.board_paint_view(painter.clip_rect());
    let revision = (app.scene_gen, app.doc().scene.scene_gen());
    let open = |app: &SlateApp, t: u64| app.tabs.iter().any(|x| x.id == t);
    let paints = |app: &SlateApp, n: &Node| {
        !n.hidden || app.hide_ghosts.iter().any(|(g, _)| g.id == n.id)
    };
    let mut busy = false;
    let mut waiting = std::mem::take(&mut app.erase_settle.waiting);
    waiting.retain_mut(|w| {
        w.seen = false;
        if w.tab != tab {
            return open(app, w.tab);
        }
        let journal = &app.tab().journal;
        if !journal.is_applied(w.pass) {
            w.shown = false;
            return journal.is_undone(w.pass);
        }
        let Some(n) = app.doc().scene.node(w.id) else {
            w.shown = false;
            return true;
        };
        let key = match w.checked {
            Some((at, key)) if at == revision => key,
            _ => match node_stamp_key(n).filter(|k| w.keys.contains(k)) {
                Some(key) => {
                    w.checked = Some((revision, key));
                    key
                }
                None => return false,
            },
        };
        w.seen = band_in_view(n, &view);
        if w.seen && stroke_raster_current(app, n, key, want) {
            return false;
        }
        busy |= w.seen;
        w.shown = paints(app, n);
        true
    });
    app.erase_settle.waiting = waiting;
    let held = app.erase_settle.held();
    let mut live = std::mem::take(&mut app.erase_settle.live);
    live.retain(|&(t, id), s| {
        let lane = tiles::erase_lane(id);
        let mut gone = false;
        let fit = if t == tab {
            let n = app.doc().scene.node(id);
            gone = n.is_none();
            s.shown = n.is_some_and(|n| paints(app, n));
            n.and_then(|n| settling_fit(n, s, &app.tab().journal))
        } else {
            open(app, t).then_some(s.shift)
        };
        let Some(shift) = fit else {
            s.live.forget(&mut app.brush_tiles, lane);
            let journal = &app.tab().journal;
            if t == tab && (journal.is_undone(s.pass) || (gone && journal.is_applied(s.pass))) {
                let bands = s.take_bands(t, id, false, false);
                app.erase_settle.waiting.extend(bands);
            }
            return false;
        };
        if !held {
            s.live.take_landed(&mut app.brush_tiles, lane);
            s.live.ask_next(&mut app.brush_tiles, lane);
        }
        s.shift = shift;
        busy |= t == tab && s.live.in_flight();
        true
    });
    for ((t, id), mut s) in live.extract_if(|_, s| s.live.settled() || s.live.gave_up()) {
        // The tiles take the stroke back on the next frame.
        busy |= t == tab;
        let bands = s.take_bands(t, id, s.shown, t == tab);
        s.live.forget(&mut app.brush_tiles, tiles::erase_lane(id));
        let gpu = s.live.into_stand_in(s.rect, app.frame_no);
        let seal = EraseSeal { tab: t, pass: s.pass };
        insert_erase_stand_in(app, id, s.shows, seal, gpu);
        app.erase_settle.waiting.extend(bands);
    }
    app.erase_settle.live = live;
    if busy {
        painter.ctx().request_repaint();
    }
}

/// Stroke `node` of the open document paints its exact raster for content
/// `key` at `want`.
fn stroke_raster_current(app: &SlateApp, node: &Node, key: u64, want: f32) -> bool {
    if app.brush_tiles_enabled && tiles::plain_stamp(app, node).is_some() {
        return app.brush_tiles.last.settled;
    }
    app.brush_stamps
        .get(&app.stroke_cache_id(node.id))
        .is_some_and(|(k, g)| g.exact && *k == key && g.wanted_pixel == want)
}

/// The straight eraser pass's vector stand-in: the eraser's band, in its
/// preview color, over the part of the segment that some reached stroke
/// does not show its exact cut for yet, so the preview follows the pointer
/// on frames the workers have not caught up with. A released pass keeps
/// its band while it settles ([`EraseSettle`]).
pub(crate) fn paint_erase_band(app: &mut SlateApp, painter: &egui::Painter, xf: &BoardXf) {
    tend_erase_settle(app, painter, xf);
    let mut band = std::mem::take(&mut app.erase_band);
    band.begin();
    let rgba = app.eraser_preview_color().to_srgba_unmultiplied();
    let far = |seg: Seg, p: TipPoint| (p.pos[0] - seg.1.pos[0]).hypot(p.pos[1] - seg.1.pos[1]);
    let mut paint = |seg: Seg, from: TipPoint, cut: bool| {
        let shade = |p: TipPoint| TipPoint {
            tip: StampStyle { rgba, ..p.tip },
            ..p
        };
        let start = if cut { Cap::Butt } else { Cap::Round };
        band.paint(painter, xf, (shade(from), shade(seg.1)), [start, Cap::Round]);
    };
    if let Some(super::board::BoardDrag::Erase {
        points,
        spot,
        straight: true,
        ..
    }) = &app.board_drag
    {
        let tip = app.eraser_tip();
        let rest = straight_seg(points, tip).and_then(|seg| {
            spot.iter()
                .filter_map(|id| match app.erase_live.get(id) {
                    Some(live) => live.uncovered(seg),
                    None => Some((seg.0, false)),
                })
                .max_by(|a, b| far(seg, a.0).total_cmp(&far(seg, b.0)))
                .map(|(from, cut)| (seg, from, cut))
        });
        if let Some((seg, from, cut)) = rest {
            paint(seg, from, cut);
        }
    }
    let rests = app
        .erase_settle
        .rests(app.tab().id, &app.erase_live, &app.tab().journal);
    for (i, (seg, from, cut)) in rests.clone().enumerate() {
        let d = far(seg, from);
        let beaten = rests.clone().enumerate().any(|(j, (s, f, _))| {
            j != i && s == seg && (far(s, f) > d || (far(s, f) == d && j < i))
        });
        if !beaten {
            paint(seg, from, cut);
        }
    }
    drop(rests);
    app.erase_band = band;
}

fn start_erase_live(app: &mut SlateApp, painter: &egui::Painter, node: &Node, want: f32) {
    let NodeKind::Shape(shape) = &node.kind else {
        return;
    };
    let Some(path) = shape.path.as_ref() else {
        return;
    };
    let key = stamp_key(node, shape, path);
    let cache = app.stroke_cache_id(node.id);
    if let Some(r) = app.brush_tiles.take_stroke(cache) {
        if r.key == key && r.pixel == want {
            if let Some(stamp) = r.stamp {
                let live = EraseLive::from_stamp(painter, node.id, stamp, r.image);
                app.erase_live.insert(node.id, live);
            }
            return;
        }
    }
    if take_sync_budget(app, stamp_area_px(node, shape, want)) {
        if let Some(stamp) = stroke_stamp(node, shape, path, want) {
            let live = EraseLive::from_stamp(painter, node.id, stamp, None);
            app.erase_live.insert(node.id, live);
        }
        return;
    }
    let tab = app.tab().id;
    app.brush_tiles.request_stroke(cache, tab, node, key, want);
    painter.ctx().request_repaint();
}

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
    /// Built from the stroke's content at `wanted_pixel`. A bitmap that is
    /// not exact (an older key, the eraser preview at release) only stands
    /// in while the exact one builds.
    pub exact: bool,
    /// The node's rect when this bitmap was built. A stand-in maps onto the
    /// node's current rect when only its placement changed since (its key
    /// holds at this rect), and paints here otherwise.
    pub rect: WorldRect,
    /// An eraser preview standing in: it shows these erase passes, so it
    /// paints only for a stroke that still has them.
    pub seal: Option<EraseSeal>,
    /// Bitmaps of other contents behind this one, two deep: what an eraser
    /// stand-in replaced, and what the bitmaps after it replaced, so an
    /// undo or a redo finds them.
    pub fallback: Option<Box<(u64, BrushStampGpu)>>,
    /// The open document it was cached for: closing it frees the bitmap.
    pub tab: u64,
}

impl BrushStampGpu {
    fn total_bytes(&self) -> usize {
        self.bytes + self.fallback.as_ref().map_or(0, |f| f.1.total_bytes())
    }
}

/// Stroke `id`'s eraser preview `gpu` stands in for its content `key`,
/// with its pass `seal` applied, until the exact bitmap lands. What it
/// replaces stays behind it, two bitmaps deep, for an undo.
pub(crate) fn insert_erase_stand_in(
    app: &mut SlateApp,
    id: NodeId,
    key: u64,
    seal: EraseSeal,
    mut gpu: BrushStampGpu,
) {
    let id = super::board_slate::stroke_cache_id_in(seal.tab, id);
    let mut before = app.brush_stamps.remove(&id);
    if let Some(f) = before.as_mut().and_then(|(_, g)| g.fallback.as_mut()) {
        f.1.fallback = None;
    }
    gpu.seal = Some(seal);
    gpu.fallback = before.map(Box::new);
    gpu.tab = seal.tab;
    app.brush_stamps.insert(id, (key, gpu));
    evict_brush_stamps(&mut app.brush_stamps, app.frame_no);
}

/// Take cache id `id`'s bitmaps out for a new one of content `key`. Once
/// an eraser stand-in started a chain, the bitmaps of other contents stay
/// behind the new one, two deep, so an undo or a redo of that pass finds
/// them; a lone bitmap goes.
fn stand_in_fallback(app: &mut SlateApp, id: NodeId, key: u64) -> Option<Box<(u64, BrushStampGpu)>> {
    let (k, top) = app.brush_stamps.remove(&id)?;
    if top.seal.is_none() && top.fallback.is_none() {
        return None;
    }
    let mut kept = Vec::new();
    let mut next = Some(Box::new((k, top)));
    while let Some(entry) = next {
        let (k, mut g) = *entry;
        next = g.fallback.take();
        if k != key && kept.len() < 2 {
            kept.push((k, g));
        }
    }
    kept.into_iter().rev().fold(None, |rest, (k, mut g)| {
        g.fallback = rest;
        Some(Box::new((k, g)))
    })
}

/// Cache id `id`, content `key`: the bitmap behind the top one that shows
/// exactly this content comes forward, and so does the first one still
/// honest when the top is an eraser stand-in whose pass was undone. The
/// others keep their order behind it, so a redo brings the stand-in back.
/// A stroke with no honest bitmap left has none.
fn promote_stamp_fallback(app: &mut SlateApp, id: NodeId, key: u64) {
    let undone = |g: &BrushStampGpu| g.seal.is_some_and(|s| s.undone(app));
    let Some((k, top)) = app.brush_stamps.get(&id) else {
        return;
    };
    let top_ok = !undone(top);
    if top_ok && *k == key {
        return;
    }
    let behind = || {
        std::iter::successors(top.fallback.as_deref(), |(_, g)| g.fallback.as_deref())
            .zip(1..)
    };
    let pick = behind()
        .find(|((k, g), _)| *k == key && !undone(g))
        .or_else(|| behind().find(|((_, g), _)| !top_ok && !undone(g)))
        .map(|(_, i)| i);
    let Some(i) = pick else {
        if !top_ok {
            app.brush_stamps.remove(&id);
        }
        return;
    };
    let Some(top) = app.brush_stamps.remove(&id) else {
        return;
    };
    let mut chain = Vec::new();
    let mut next = Some(Box::new(top));
    while let Some(entry) = next {
        let (k, mut g) = *entry;
        next = g.fallback.take();
        chain.push((k, g));
    }
    let (k, mut g) = chain.remove(i);
    g.fallback = chain.into_iter().rev().fold(None, |rest, (k, mut g)| {
        g.fallback = rest;
        Some(Box::new((k, g)))
    });
    app.brush_stamps.insert(id, (k, g));
}

/// A stroke of the open document has something to paint while its exact
/// bitmap builds. `key` is its content key, when known.
pub(crate) fn has_stand_in(app: &SlateApp, id: NodeId, key: Option<u64>) -> bool {
    app.brush_stamps.contains_key(&app.stroke_cache_id(id))
        || app
            .brush_live
            .as_ref()
            .is_some_and(|c| c.stands_in(id, key))
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
    // A nested board numbers its strokes as the host does: they go by
    // their cache id, never meet the host's eraser or live canvas, and no
    // tile paints them.
    let nested = app.slate_nesting();
    let cache = app.stroke_cache_id(node.id);
    let want = stamp_pixel_for_zoom(xf.z, painter.ctx().pixels_per_point());
    if let Some(super::board::BoardDrag::Erase {
        points,
        spot,
        straight,
        ..
    }) = &app.board_drag
    {
        if !nested && spot.contains(&node.id) {
            let (points, straight) = (points.clone(), *straight);
            let tip = app.eraser_tip();
            if !app.erase_live.contains_key(&node.id) {
                start_erase_live(app, painter, node, want);
            }
            if let Some(live) = app.erase_live.get_mut(&node.id) {
                live.feed(&points, tip, straight);
                live.pump(
                    &mut app.brush_tiles,
                    tiles::erase_lane(node.id),
                    painter.ctx(),
                );
                live.paint(painter, xf, fade(Color32::WHITE), [0.0; 2]);
                return;
            }
        }
    }
    if !nested && paint_settling_erase(app, painter, xf, node, fade) {
        return;
    }
    let key = stamp_key(node, shape, path);
    promote_stamp_fallback(app, cache, key);
    let exact = |app: &SlateApp| {
        app.brush_stamps
            .get(&cache)
            .is_some_and(|(k, g)| g.exact && *k == key && g.wanted_pixel == want)
    };
    let tab = app.tab().id;
    if !exact(app) {
        if let Some(r) = app.brush_tiles.take_stroke(cache) {
            let current = r.key == key && r.pixel == want;
            // A stale bitmap replaces neither an eraser stand-in whose pass
            // is applied nor a bitmap of this very content.
            let shown = app.brush_stamps.get(&cache).is_some_and(|(k, g)| {
                *k == key || g.seal.is_some_and(|s| !s.undone(app))
            });
            match r.stamp {
                Some(stamp) if current || !shown => {
                    let name = format!("brush-stamp-{}", cache.0);
                    let mut gpu = upload_stamp(painter, &name, stamp, r.image, r.pixel, r.rect);
                    gpu.exact = current;
                    gpu.fallback = stand_in_fallback(app, cache, r.key);
                    gpu.tab = tab;
                    app.brush_stamps.insert(cache, (r.key, gpu));
                    evict_brush_stamps(&mut app.brush_stamps, app.frame_no);
                }
                Some(_) => {}
                None if current => {
                    app.brush_stamps.remove(&cache);
                    return;
                }
                None => {}
            }
        }
    }
    if !exact(app) {
        if take_sync_budget(app, stamp_area_px(node, shape, want)) {
            let Some(stamp) = stroke_stamp(node, shape, path, want) else {
                app.brush_stamps.remove(&cache);
                return;
            };
            let name = format!("brush-stamp-{}", cache.0);
            let mut gpu = upload_stamp(painter, &name, stamp, None, want, node.rect);
            gpu.fallback = stand_in_fallback(app, cache, key);
            gpu.tab = tab;
            app.brush_stamps.insert(cache, (key, gpu));
            evict_brush_stamps(&mut app.brush_stamps, app.frame_no);
        } else {
            // Tiles already rasterize a plain stroke of the host off the
            // frame loop.
            let tiled =
                !nested && app.brush_tiles_enabled && tiles::plain_stamp(app, node).is_some();
            if !tiled {
                app.brush_tiles.request_stroke(cache, tab, node, key, want);
            }
            painter.ctx().request_repaint();
        }
    }
    let stand_in = app
        .brush_live
        .as_ref()
        .filter(|c| !nested && !exact(app) && c.stands_in(node.id, Some(key)))
        .map(|c| c.awaits_anchor());
    if let Some(adds) = stand_in {
        // A canvas that adds only this drag's segments paints over the
        // stroke as it was: the tiles show that, or its last bitmap where
        // it was.
        if adds && !(app.brush_tiles_enabled && tiles::plain_stamp(app, node).is_some()) {
            if let Some((_, gpu)) = app.brush_stamps.get_mut(&cache) {
                gpu.used = app.frame_no;
                paint_stamp_quad(painter, xf, gpu, gpu.rect, fade(Color32::WHITE));
            }
        }
        if let Some(canvas) = app.brush_live.as_mut() {
            canvas.paint(painter, xf);
        }
        return;
    }
    if let Some((built, gpu)) = app.brush_stamps.get_mut(&cache) {
        gpu.used = app.frame_no;
        let moved = gpu.rect == node.rect || stamp_key_at(node, shape, path, gpu.rect) == *built;
        let rect = if moved { node.rect } else { gpu.rect };
        paint_stamp_quad(painter, xf, gpu, rect, fade(Color32::WHITE));
    }
}

/// One committed stroke's stamp bitmap at `pixel` world units per pixel:
/// tipped dabs, the stroke's blur, then erase marks.
pub(crate) fn stroke_stamp(
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    pixel: f32,
) -> Option<vector_ink::StampImage> {
    note_stamp_on_this_thread();
    let contours = stamped_contours(node, shape, path, (pixel as f64 * 0.5).max(0.05));
    vector_ink::stamp_blurred(
        &contours,
        &stamped_erase_marks(node, shape, path),
        pixel,
        shape.stroke.gaussian_blur,
    )
}

#[cfg(test)]
thread_local! {
    static STAMPS_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Count a whole-stroke stamp rasterized on the calling thread. The raster
/// workers are other threads, so on the frame thread this counts only
/// frame-loop work.
#[cfg(test)]
pub(crate) fn note_stamp_on_this_thread() {
    STAMPS_HERE.with(|n| n.set(n.get() + 1));
}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn note_stamp_on_this_thread() {}

/// Whole-stroke stamps rasterized on this thread so far.
#[cfg(test)]
pub(crate) fn stamps_on_this_thread() -> u64 {
    STAMPS_HERE.with(|n| n.get())
}

#[cfg(test)]
thread_local! {
    static NODE_CLONES_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Count a scene node cloned for a stamped stroke's bitmap on the calling
/// thread.
#[cfg(test)]
fn note_node_clone() {
    NODE_CLONES_HERE.with(|n| n.set(n.get() + 1));
}

#[cfg(not(test))]
#[inline(always)]
fn note_node_clone() {}

/// Scene nodes cloned for stamped strokes' bitmaps on this thread so far.
#[cfg(test)]
pub(crate) fn node_clones_on_this_thread() -> u64 {
    NODE_CLONES_HERE.with(|n| n.get())
}

#[cfg(test)]
thread_local! {
    static STAMP_PX_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static RESUMES_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Live-canvas resumes (a parked canvas reused without a rebuild) on this
/// thread so far.
#[cfg(test)]
pub(crate) fn resumes_on_this_thread() -> u64 {
    RESUMES_HERE.with(|n| n.get())
}

/// Count `px` pixels of live-canvas segment boxes stamped on the calling
/// thread; on the frame thread that is frame-loop raster work.
#[cfg(test)]
fn note_stamp_px(px: u64) {
    STAMP_PX_HERE.with(|n| n.set(n.get() + px));
}

#[cfg(not(test))]
#[inline(always)]
fn note_stamp_px(_px: u64) {}

/// Live-canvas segment pixels stamped on this thread so far.
#[cfg(test)]
pub(crate) fn stamp_px_on_this_thread() -> u64 {
    STAMP_PX_HERE.with(|n| n.get())
}

#[cfg(test)]
thread_local! {
    static LINE_TEX_ALLOCS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static BASE_COPIES: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) };
}

/// Count a Shift segment texture allocated or replaced whole on this thread.
#[cfg(test)]
fn note_line_tex_alloc() {
    LINE_TEX_ALLOCS.with(|n| n.set(n.get() + 1));
}

#[cfg(not(test))]
#[inline(always)]
fn note_line_tex_alloc() {}

/// Shift segment textures allocated or replaced whole on this thread so far.
#[cfg(test)]
pub(crate) fn line_tex_allocs_on_this_thread() -> u64 {
    LINE_TEX_ALLOCS.with(|n| n.get())
}

/// Count one copy of `px` canvas pixels into a line job's base.
#[cfg(test)]
fn note_base_copy(px: u64) {
    BASE_COPIES.with(|n| {
        let (count, total) = n.get();
        n.set((count + 1, total + px));
    });
}

#[cfg(not(test))]
#[inline(always)]
fn note_base_copy(_px: u64) {}

/// Line-job base copies made on this thread so far, and their pixels.
#[cfg(test)]
pub(crate) fn base_copies_on_this_thread() -> (u64, u64) {
    BASE_COPIES.with(|n| n.get())
}

fn box_area(b: [u32; 4]) -> u64 {
    (b[2] - b[0]) as u64 * (b[3] - b[1]) as u64
}

fn evict_brush_stamps(cache: &mut HashMap<NodeId, (u64, BrushStampGpu)>, frame: u64) {
    let total: usize = cache.values().map(|(_, g)| g.total_bytes()).sum();
    if total <= STAMP_CACHE_BYTES {
        return;
    }
    let mut old: Vec<(u64, NodeId, usize)> = cache
        .iter()
        .filter(|(_, (_, g))| g.used + 1 < frame)
        .map(|(id, (_, g))| (g.used, *id, g.total_bytes()))
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

/// One straight RGBA color premultiplied as [`premultiplied`] does it.
fn stamp_color(rgba: [u8; 4]) -> Color32 {
    let [r, g, b, a]: [u8; 4] = premultiplied(&rgba).try_into().expect("four bytes");
    Color32::from_rgba_premultiplied(r, g, b, a)
}

/// Upload a stamp bitmap. `image` is its premultiplied texture when a worker
/// already made it.
fn upload_stamp(
    painter: &egui::Painter,
    name: &str,
    stamp: vector_ink::StampImage,
    image: Option<egui::ColorImage>,
    wanted_pixel: f32,
    rect: WorldRect,
) -> BrushStampGpu {
    let image = image.unwrap_or_else(|| {
        egui::ColorImage::from_rgba_premultiplied(
            [stamp.width as usize, stamp.height as usize],
            &premultiplied(&stamp.rgba),
        )
    });
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
        exact: true,
        rect,
        seal: None,
        fallback: None,
        tab: 0,
    }
}

/// Paint a stamp bitmap for a node now at `rect`. A bitmap built for another
/// rect (a stand-in while the node moves or resizes) maps onto the new one.
/// When the content changed too, pass the rect it was built at: an appended
/// path's grown rect would stretch the old stroke over the new bounds.
fn paint_stamp_quad(
    painter: &egui::Painter,
    xf: &BoardXf,
    gpu: &BrushStampGpu,
    rect: WorldRect,
    tint: Color32,
) {
    let (mut origin, mut size) = (gpu.origin, gpu.size);
    if rect != gpu.rect {
        let (old, new) = (gpu.rect, rect);
        let sx = if old.w.abs() > 1.0e-3 { new.w / old.w } else { 1.0 };
        let sy = if old.h.abs() > 1.0e-3 { new.h / old.h } else { 1.0 };
        origin = [
            new.x + (origin[0] - old.x) * sx,
            new.y + (origin[1] - old.y) * sy,
        ];
        size = [size[0] * sx, size[1] * sy];
    }
    let min = xf.w2s(Pos2::new(origin[0], origin[1]));
    let max = xf.w2s(Pos2::new(origin[0] + size[0], origin[1] + size[1]));
    painter.image(
        gpu.tex.id(),
        egui::Rect::from_min_max(min, max),
        egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        tint,
    );
}

/// The brush drag's own canvas, aligned to the screen at one physical pixel
/// per bitmap pixel. Freehand segments are added as they arrive and only the
/// touched region uploads.
///
/// A Shift segment is never stamped on the frame loop (Art. II): its exact
/// stamp over the canvas pixels under it builds on the raster workers, one
/// job at a time, generation-tagged, and the newest landed result shows. A
/// frame the exact stamp does not cover yet paints the segment's vector
/// mesh at its tips instead, so the preview follows the pointer every
/// frame; when the landed stamp is a shorter run of the same segment, it
/// shows with the mesh only past its end. A committed segment is stamped
/// into the canvas the same way.
///
/// When a Shift line continues an earlier stroke, that stroke is stamped into
/// the canvas first, on the raster workers too, and hidden from the scene
/// paint once the canvas holds it ([`Self::covers_anchor`]); until then the
/// scene keeps painting it. The preview is then one bitmap with the
/// committed result's max-coverage joint, not two overlapping images.
///
/// Between strokes the canvas is parked, not dropped: the next stroke clears
/// only the pixels the last one touched, so a stroke start costs its own area
/// rather than a full-window allocation and upload.
pub struct BrushLiveCanvas {
    /// Raw coverage and depth; the grain is applied per upload, as the
    /// committed stamp applies it once to the finished stroke.
    img: vector_ink::StampImage,
    grain: vector_ink::Grain,
    tex: egui::TextureHandle,
    view: [u32; 6],
    pub anchor: Option<NodeId>,
    freehand_done: usize,
    /// Every pixel box drawn since the last reset.
    touched: Option<[u32; 4]>,
    idle: bool,
    /// The stroke the last drag committed, and its content key once seen.
    /// Until the next stroke starts, the parked canvas stands in for it
    /// while its exact bitmap or tiles build.
    pub held: Option<(NodeId, Option<u64>)>,
    /// The canvas shows the held stroke and nothing else, so the next Shift
    /// segment that continues it starts from these pixels.
    reusable: bool,
    /// Bumped whenever `img` changes: a raster built on older pixels is
    /// dropped when it lands.
    gen: u64,
    /// The Shift segment the preview shows.
    line: Option<Seg>,
    /// Committed Shift segments not stamped into `img` yet, oldest first.
    commits: Vec<Seg>,
    /// The newest exact preview stamp the workers returned.
    exact: Option<LineExact>,
    /// Canvas-sized; the exact preview's box of it shows that stamp.
    line_tex: egui::TextureHandle,
    inflight: Option<LineAsk>,
    /// The anchor stroke's segments, not in `img` yet.
    anchor_segs: Vec<Seg>,
    /// The segments start at the end of a stamped mark.
    chained: bool,
    /// Line jobs lost in a row; past [`LINE_RETRIES`] the canvas stops
    /// asking until it is rebuilt.
    losses: u32,
    broken: bool,
    meshes: SegMeshes,
    /// Pixel buffers of a dropped raster, reused for the next job's base.
    spare: Spare,
    /// Test hook: the next line job panics on its worker.
    #[cfg(test)]
    pub(crate) panic_next: bool,
    /// Test hook: every line job panics on its worker.
    #[cfg(test)]
    pub(crate) panic_always: bool,
    /// Test hook: ask the workers for no preview stamp.
    #[cfg(test)]
    pub(crate) hold_previews: bool,
    /// Test hook: a landed anchor stamp is not taken in.
    #[cfg(test)]
    pub(crate) hold_anchor: bool,
}

/// Lost line jobs the live canvas asks again before it gives up.
const LINE_RETRIES: u32 = 2;

/// A straight segment between two tipped ends.
type Seg = (TipPoint, TipPoint);

/// A preview stamp from the raster workers: `seg` stamped over the canvas
/// pixels in box `bx` as they were at generation `gen`. Its pixels are in
/// the canvas's line texture.
struct LineExact {
    seg: Seg,
    gen: u64,
    bx: [u32; 4],
    raw: vector_ink::StampImage,
    image: Shared<egui::ColorImage>,
}

/// What a line job on the live canvas is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AskKind {
    /// The anchor stroke, into a clear canvas.
    Anchor,
    /// Committed segments, into the canvas.
    Commit,
    /// The live segment's exact preview.
    Preview,
}

/// The one Shift segment job on the raster workers.
struct LineAsk {
    tag: u64,
    gen: u64,
    segs: Vec<Seg>,
    kind: AskKind,
    bx: [u32; 4],
}

/// Shift segments to stamp over `base`, a copy of the owner's pixels under
/// them (an empty `rgba` is a clear box). Runs on a raster worker
/// ([`line_raster`]). `lane` names the owner: the brush canvas or one
/// stroke's eraser preview.
pub(crate) struct LineJob {
    pub tag: u64,
    pub lane: u64,
    base: vector_ink::StampImage,
    segs: Vec<Seg>,
    /// An eraser pass: the stamped segments come out of this ink.
    cut: Option<InkCut>,
    #[cfg(test)]
    panic: bool,
}

/// A stroke's ink, `width` pixels wide, and the box of it an eraser
/// segment job covers.
struct InkCut {
    ink: Shared<Vec<u8>>,
    width: u32,
    bx: [u32; 4],
}

impl InkCut {
    /// The ink in the box with `mask` (the grained coverage of the stamped
    /// box) taken out, and whether `raw` coverage met any ink.
    fn apply(&self, raw: &[u8], mask: &[u8]) -> (Vec<u8>, bool) {
        let [x0, y0, x1, y1] = self.bx;
        let (w, bw) = (self.width as usize, (x1 - x0) as usize);
        let mut shown = Vec::with_capacity(bw * (y1 - y0) as usize * 4);
        for y in y0 as usize..y1 as usize {
            shown
                .extend_from_slice(&self.ink[(y * w + x0 as usize) * 4..(y * w + x1 as usize) * 4]);
        }
        let changed = shown
            .chunks_exact(4)
            .zip(raw.chunks_exact(4))
            .any(|(i, m)| i[3] > 0 && m[3] > 0);
        vector_ink::multiply_by_mask(&mut shown, mask, bw as u32, None);
        (shown, changed)
    }
}

/// A finished [`LineJob`]: raw coverage and depth for the owner, and the
/// grained, premultiplied pixels for upload. An eraser job also returns
/// the cut ink as straight alpha and whether the cut met any ink.
pub(crate) struct LineRaster {
    pub tag: u64,
    pub lane: u64,
    raw: vector_ink::StampImage,
    image: Shared<egui::ColorImage>,
    shown: Vec<u8>,
    changed: bool,
}

/// Stamp a [`LineJob`]. Worker threads run it; the frame thread only when
/// no worker can.
pub(crate) fn line_raster(job: LineJob) -> LineRaster {
    #[cfg(test)]
    if job.panic {
        panic!("test hook: this line job is lost");
    }
    let mut img = job.base;
    if img.rgba.is_empty() {
        img.rgba = vec![0; img.width as usize * img.height as usize * 4];
    }
    for (a, b) in &job.segs {
        stamp_segment(&mut img, *a, *b);
    }
    note_stamp_px(img.width as u64 * img.height as u64);
    let grain = job
        .segs
        .last()
        .map_or(vector_ink::Grain::Smooth, |s| s.1.tip.grain);
    let rows = vector_ink::finished_region(&img, grain, [0, 0, img.width, img.height]);
    let (shown, changed) = match &job.cut {
        Some(cut) => cut.apply(&img.rgba, &rows),
        None => (Vec::new(), false),
    };
    let image = egui::ColorImage::from_rgba_premultiplied(
        [img.width as usize, img.height as usize],
        &premultiplied(if job.cut.is_some() { &shown } else { &rows }),
    );
    LineRaster {
        tag: job.tag,
        lane: job.lane,
        raw: img,
        image: Shared::new(image),
        shown,
        changed,
    }
}

/// Buffers of a landed raster, kept for the next region copy.
type Spare = (Vec<u8>, Vec<u8>, vector_ink::StampSide);

fn spare_of(raw: vector_ink::StampImage) -> Spare {
    (raw.rgba, raw.depth, raw.side)
}

/// Copy of `img`'s pixels in box `b` as its own image on the world, into
/// the reused buffers `(rgba, depth, side)`.
fn copy_region(
    img: &vector_ink::StampImage,
    b: [u32; 4],
    (mut rgba, mut depth, side): Spare,
) -> vector_ink::StampImage {
    rgba.clear();
    depth.clear();
    note_base_copy(box_area(b));
    let w = img.width as usize;
    for y in b[1] as usize..b[3] as usize {
        rgba.extend_from_slice(&img.rgba[(y * w + b[0] as usize) * 4..(y * w + b[2] as usize) * 4]);
        if !img.depth.is_empty() {
            depth.extend_from_slice(&img.depth[y * w + b[0] as usize..y * w + b[2] as usize]);
        }
    }
    vector_ink::StampImage {
        width: b[2] - b[0],
        height: b[3] - b[1],
        origin: [
            img.origin[0] + b[0] as f32 * img.pixel,
            img.origin[1] + b[1] as f32 * img.pixel,
        ],
        pixel: img.pixel,
        rgba,
        depth,
        side: img.side.crop_into(img.width, b, side),
    }
}

/// `raw` is exactly box `b`'s size: a raster built for another box is never
/// written into or shown over this one.
fn fits_box(raw: &vector_ink::StampImage, b: [u32; 4]) -> bool {
    let (w, h) = (b[2].saturating_sub(b[0]), b[3].saturating_sub(b[1]));
    raw.width == w
        && raw.height == h
        && raw.rgba.len() == w as usize * h as usize * 4
        && (raw.depth.is_empty() || raw.depth.len() == w as usize * h as usize)
}

/// `new` runs from the same start along the same direction as `old`, at
/// least as far, with one tip throughout: `old`'s stamp is a prefix of it.
fn extends(old: Seg, new: Seg) -> bool {
    let (a, ob, nb) = (old.0, old.1, new.1);
    if new.0 != a || ob.tip != a.tip || nb.tip != a.tip {
        return false;
    }
    let o = [ob.pos[0] - a.pos[0], ob.pos[1] - a.pos[1]];
    let n = [nb.pos[0] - a.pos[0], nb.pos[1] - a.pos[1]];
    let (lo, ln) = (o[0].hypot(o[1]), n[0].hypot(n[1]));
    let cross = o[0] * n[1] - o[1] * n[0];
    lo > 0.0 && ln >= lo && cross.abs() <= 1.0e-4 * lo * ln && o[0] * n[0] + o[1] * n[1] > 0.0
}

fn seg_mesh_key(seg: Seg, ends: [Cap; 2], zoom: f32) -> u64 {
    let mut h = DefaultHasher::new();
    for p in [seg.0, seg.1] {
        hash_xy(&mut h, p.pos);
        hash_f32(&mut h, p.tip.diameter);
        hash_f32(&mut h, p.tip.softness);
        p.tip.rgba.hash(&mut h);
    }
    (ends[0] == Cap::Round, ends[1] == Cap::Round).hash(&mut h);
    hash_f32(&mut h, zoom);
    h.finish()
}

/// Paint texel box `px` of a `size`-texel texture laid on the world at
/// `origin`, `pixel` world units per texel.
fn paint_texels(
    painter: &egui::Painter,
    xf: &BoardXf,
    tex: egui::TextureId,
    (origin, pixel, size): ([f32; 2], f32, [u32; 2]),
    px: [u32; 4],
    tint: Color32,
) {
    if px[2] <= px[0] || px[3] <= px[1] {
        return;
    }
    let world = |x: u32, y: u32| Pos2::new(origin[0] + x as f32 * pixel, origin[1] + y as f32 * pixel);
    let (w, h) = (size[0].max(1) as f32, size[1].max(1) as f32);
    let uv = egui::Rect::from_min_max(
        Pos2::new(px[0] as f32 / w, px[1] as f32 / h),
        Pos2::new(px[2] as f32 / w, px[3] as f32 / h),
    );
    painter.image(
        tex,
        egui::Rect::from_min_max(xf.w2s(world(px[0], px[1])), xf.w2s(world(px[2], px[3]))),
        uv,
        tint,
    );
}

/// [`paint_texels`] for the whole texture except box `b`, which another
/// texture paints.
fn paint_around(
    painter: &egui::Painter,
    xf: &BoardXf,
    tex: egui::TextureId,
    img: ([f32; 2], f32, [u32; 2]),
    b: [u32; 4],
    tint: Color32,
) {
    let [w, h] = img.2;
    for part in [
        [0, 0, w, b[1]],
        [0, b[3], w, h],
        [0, b[1], b[0], b[3]],
        [b[2], b[1], w, b[3]],
    ] {
        paint_texels(painter, xf, tex, img, part, tint);
    }
}

/// A straight pass's one segment: its first and last point at `tip`.
fn straight_seg(points: &[Pos2], tip: StampStyle) -> Option<Seg> {
    let at = |p: &Pos2| TipPoint {
        pos: [p.x, p.y],
        tip,
    };
    Some((at(points.first()?), at(points.last()?)))
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
    /// build a fresh canvas and ask the raster workers to stamp the anchor
    /// stroke's contours into it (Art. II). A parked canvas that still shows
    /// exactly the anchor (content `anchor_key`, under this camera) is
    /// picked up as it is, so a chain of Shift segments never re-stamps the
    /// chain at all.
    #[allow(clippy::too_many_arguments)]
    pub fn ensure<'a>(
        slot: &'a mut Option<BrushLiveCanvas>,
        tiles: &mut tiles::BrushTiles,
        painter: &egui::Painter,
        xf: &BoardXf,
        screen: egui::Rect,
        anchor_id: Option<NodeId>,
        anchor_key: Option<u64>,
        anchor_contours: impl FnOnce() -> Vec<Vec<TipPoint>>,
    ) -> &'a mut BrushLiveCanvas {
        let ppp = painter.ctx().pixels_per_point();
        let view = view_key(xf, screen, ppp);
        let live = slot
            .as_ref()
            .is_some_and(|c| !c.idle && c.view == view && c.anchor == anchor_id);
        if live {
            return slot.as_mut().expect("live canvas");
        }
        if slot
            .as_ref()
            .is_some_and(|c| c.resumable(view, anchor_id, anchor_key))
        {
            // Committed segments still on the workers are part of the canvas.
            let canvas = slot.as_mut().expect("parked canvas");
            canvas.held = None;
            canvas.reusable = false;
            canvas.anchor = anchor_id;
            canvas.chained = true;
            canvas.freehand_done = 0;
            canvas.idle = false;
            canvas.line = None;
            canvas.exact = None;
            #[cfg(test)]
            RESUMES_HERE.with(|n| n.set(n.get() + 1));
            return canvas;
        }
        // Whatever the canvas had on the workers is for pixels about to go.
        if let Some(ask) = slot.as_mut().and_then(|c| c.inflight.take()) {
            tiles.forget_line(tiles::BRUSH_LANE, ask.tag);
        }
        let w = (screen.width() * ppp).ceil().max(1.0) as u32;
        let h = (screen.height() * ppp).ceil().max(1.0) as u32;
        let fits = slot
            .as_ref()
            .is_some_and(|c| c.img.width == w && c.img.height == h);
        if !fits {
            let blank = || egui::ColorImage::new([w as usize, h as usize], Color32::TRANSPARENT);
            let ctx = painter.ctx();
            let tex = ctx.load_texture("brush-live", blank(), egui::TextureOptions::LINEAR);
            let line_tex = ctx.load_texture("brush-live-line", blank(), egui::TextureOptions::LINEAR);
            note_line_tex_alloc();
            *slot = Some(BrushLiveCanvas {
                img: vector_ink::StampImage {
                    width: w,
                    height: h,
                    origin: [0.0, 0.0],
                    pixel: 1.0,
                    rgba: vec![0u8; (w as usize) * (h as usize) * 4],
                    depth: Vec::new(),
                    side: Default::default(),
                },
                grain: vector_ink::Grain::Smooth,
                tex,
                view,
                anchor: None,
                freehand_done: 0,
                touched: None,
                idle: true,
                held: None,
                reusable: false,
                gen: 0,
                line: None,
                commits: Vec::new(),
                exact: None,
                line_tex,
                inflight: None,
                anchor_segs: Vec::new(),
                chained: false,
                losses: 0,
                broken: false,
                meshes: SegMeshes::default(),
                spare: Default::default(),
                #[cfg(test)]
                panic_next: false,
                #[cfg(test)]
                panic_always: false,
                #[cfg(test)]
                hold_previews: false,
                #[cfg(test)]
                hold_anchor: false,
            });
        }
        let canvas = slot.as_mut().expect("canvas just ensured");
        // An empty canvas is empty under any camera, so only the mapping moves.
        canvas.clear_touched();
        canvas.gen += 1;
        canvas.held = None;
        canvas.reusable = false;
        let origin = xf.s2w(screen.min);
        canvas.img.origin = [origin.x, origin.y];
        canvas.img.pixel = 1.0 / (xf.z * ppp).max(1.0e-3);
        canvas.view = view;
        canvas.anchor = anchor_id;
        canvas.chained = anchor_id.is_some();
        canvas.freehand_done = 0;
        canvas.line = None;
        canvas.commits.clear();
        canvas.exact = None;
        canvas.idle = false;
        canvas.losses = 0;
        canvas.broken = false;
        canvas.anchor_segs.clear();
        if anchor_id.is_some() {
            for contour in anchor_contours() {
                match contour.as_slice() {
                    [] => {}
                    [only] => canvas.anchor_segs.push((*only, *only)),
                    pts => canvas
                        .anchor_segs
                        .extend(pts.windows(2).map(|s| (s[0], s[1]))),
                }
            }
        }
        canvas
    }

    /// Parked with exactly stroke `id` (content `key`) under camera `view`,
    /// so a Shift segment continuing it can start from these pixels.
    fn resumable(&self, view: [u32; 6], id: Option<NodeId>, key: Option<u64>) -> bool {
        id.is_some()
            && key.is_some()
            && self.idle
            && self.reusable
            && !self.broken
            && self.view == view
            && self.held == id.map(|id| (id, key))
            && self.anchor_segs.is_empty()
    }

    /// The anchor stroke `id` is stamped into this canvas.
    pub fn holds_anchor(&self, id: NodeId) -> bool {
        self.anchor == Some(id) && self.anchor_segs.is_empty() && !self.broken
    }

    /// A Shift preview continuing stroke `id` (content `key`) under this
    /// camera paints that stroke from this canvas this frame, so the scene
    /// leaves it out. While the workers still stamp it in, the scene paints
    /// it. `key` is asked only for a parked canvas holding `id`.
    pub fn covers_anchor(
        &self,
        id: NodeId,
        key: impl FnOnce() -> Option<u64>,
        xf: &BoardXf,
        screen: egui::Rect,
        ppp: f32,
    ) -> bool {
        let view = view_key(xf, screen, ppp);
        if !self.idle {
            return self.view == view && self.holds_anchor(id);
        }
        self.held.is_some_and(|(held, _)| held == id) && self.resumable(view, Some(id), key())
    }

    /// Stop previewing. The anchor stroke paints from the scene again.
    pub fn park(&mut self) {
        self.idle = true;
        self.anchor = None;
        self.line = None;
    }

    #[cfg(test)]
    pub(crate) fn texture(&self) -> egui::TextureId {
        self.tex.id()
    }

    /// The drag on this canvas committed as stroke `id` (content `key`). The
    /// canvas is reusable when it holds nothing else: a new stroke, or a
    /// Shift segment that continued `id` itself. A release that added `id`
    /// beside the anchor instead leaves the anchor in the canvas too, so the
    /// canvas stands in for nothing.
    pub fn hold(&mut self, id: NodeId, key: Option<u64>) {
        if self.anchor.is_some_and(|x| x != id) {
            self.held = None;
            self.reusable = false;
            return;
        }
        self.held = Some((id, key));
        self.reusable = !self.idle && key.is_some() && !self.broken;
    }

    /// [`Self::hold`] for the Shift segment `a`–`b`, which the canvas takes
    /// in exactly: from the preview stamp when it shows `a`–`b` (the release
    /// may land off the last previewed point), else from the workers.
    pub fn hold_line(&mut self, id: NodeId, key: Option<u64>, a: TipPoint, b: TipPoint) {
        self.hold(id, key);
        if self.held.is_some() {
            self.commits.push((a, b));
            self.adopt_exact();
        }
    }

    /// A committed segment the preview stamp already shows goes straight in.
    fn adopt_exact(&mut self) {
        let ready = self.commits.len() == 1
            && self
                .exact
                .as_ref()
                .is_some_and(|e| e.seg == self.commits[0] && e.gen == self.gen);
        if ready {
            let e = self.exact.take().expect("checked above");
            self.take_in(e.bx, e.raw, e.image, e.seg.1.tip.grain);
            self.commits.clear();
        }
    }

    /// This canvas stands in for stroke `id` as it is now (content `key`,
    /// when known) until its exact bitmap lands: the tile budget and the
    /// paint both ask this. An edit since the commit means the canvas no
    /// longer shows the stroke. While [`Self::awaits_anchor`], the canvas
    /// shows only what this drag added.
    pub fn stands_in(&self, id: NodeId, key: Option<u64>) -> bool {
        !self.broken
            && self.held.is_some_and(|(held, seen)| {
                held == id && (seen.is_none() || key.is_none() || seen == key)
            })
    }

    /// The workers have not stamped the anchor stroke into the canvas yet.
    pub fn awaits_anchor(&self) -> bool {
        !self.anchor_segs.is_empty()
    }

    /// Stamp a segment and remember its box for the next reset.
    fn stamp(&mut self, a: TipPoint, b: TipPoint) -> Option<[u32; 4]> {
        self.grain = b.tip.grain;
        self.gen += 1;
        stamp_segment(&mut self.img, a, b);
        let bx = self.segment_box(a, b)?;
        note_stamp_px(box_area(bx));
        self.touched = Some(union_box(self.touched, bx));
        Some(bx)
    }

    fn clear_touched(&mut self) {
        let Some(b) = self.touched.take() else {
            return;
        };
        self.gen += 1;
        vector_ink::clear_stamp_box(&mut self.img, b, true);
        self.upload(b);
    }

    fn region(&mut self, b: [u32; 4]) -> vector_ink::StampImage {
        copy_region(&self.img, b, std::mem::take(&mut self.spare))
    }

    /// Write a landed raster of box `b` into the canvas and its texture.
    fn take_in(
        &mut self,
        b: [u32; 4],
        raw: vector_ink::StampImage,
        image: Shared<egui::ColorImage>,
        grain: vector_ink::Grain,
    ) {
        if !fits_box(&raw, b) {
            self.spare = spare_of(raw);
            return;
        }
        let w = self.img.width as usize;
        let bw = (b[2] - b[0]) as usize;
        if !raw.depth.is_empty() && self.img.depth.is_empty() {
            self.img.depth = vec![0; w * self.img.height as usize];
        }
        for (i, y) in (b[1] as usize..b[3] as usize).enumerate() {
            self.img.rgba[(y * w + b[0] as usize) * 4..(y * w + b[2] as usize) * 4]
                .copy_from_slice(&raw.rgba[i * bw * 4..(i + 1) * bw * 4]);
            if !raw.depth.is_empty() {
                self.img.depth[y * w + b[0] as usize..y * w + b[2] as usize]
                    .copy_from_slice(&raw.depth[i * bw..(i + 1) * bw]);
            }
        }
        let (width, height) = (self.img.width, self.img.height);
        self.img.side.paste(width, height, b, &raw.side);
        self.grain = grain;
        self.gen += 1;
        self.touched = Some(union_box(self.touched, b));
        self.tex.set_partial(
            [b[0] as usize, b[1] as usize],
            image,
            egui::TextureOptions::LINEAR,
        );
        self.spare = spare_of(raw);
    }

    fn segment_box(&self, a: TipPoint, b: TipPoint) -> Option<[u32; 4]> {
        segment_box(&self.img, a, b)
    }

    fn upload(&mut self, dirty: [u32; 4]) {
        let sub = vector_ink::finished_region(&self.img, self.grain, dirty);
        upload_rows(&mut self.tex, dirty, &sub);
    }

    /// Stamp freehand points not yet on the canvas, each with its own tip
    /// (`tips`, one per point).
    pub fn add_freehand(&mut self, points: &[Pos2], tips: &[StrokeSpan]) {
        let at = |i: usize| TipPoint {
            pos: [points[i].x, points[i].y],
            tip: stamp_style(tips[i.min(tips.len().saturating_sub(1))]),
        };
        if tips.is_empty() {
            return;
        }
        let mut dirty: Option<[u32; 4]> = None;
        let start = self.freehand_done.max(1);
        let mut segs: Vec<(TipPoint, TipPoint)> = Vec::new();
        if self.freehand_done == 0 && !points.is_empty() {
            segs.push((at(0), at(0)));
        }
        for i in start..points.len() {
            segs.push((at(i - 1), at(i)));
        }
        for (a, b) in segs {
            if let Some(bx) = self.stamp(a, b) {
                dirty = Some(union_box(dirty, bx));
            }
        }
        self.freehand_done = points.len();
        if let Some(d) = dirty {
            self.upload(d);
        }
    }

    #[cfg(test)]
    pub fn showing_line(&self) -> bool {
        !self.idle && self.line.is_some()
    }

    #[cfg(test)]
    pub fn live_line_end(&self) -> Option<[f32; 2]> {
        self.line.filter(|_| !self.idle).map(|(_, b)| b.pos)
    }

    #[cfg(test)]
    pub fn live_line_start(&self) -> Option<[f32; 2]> {
        self.line.filter(|_| !self.idle).map(|(a, _)| a.pos)
    }

    /// The preview shows the live segment's exact stamp.
    #[cfg(test)]
    pub fn line_exact(&self) -> bool {
        !self.idle
            && self.commits.is_empty()
            && self
                .exact
                .as_ref()
                .is_some_and(|e| Some(e.seg) == self.line && e.gen == self.gen)
    }

    /// Every committed segment and the anchor stroke are stamped into the
    /// canvas, and nothing is on the workers.
    #[cfg(test)]
    pub fn settled(&self) -> bool {
        self.commits.is_empty() && self.anchor_segs.is_empty() && self.inflight.is_none()
    }

    /// A line job is out on the workers.
    #[cfg(test)]
    pub fn asking(&self) -> bool {
        self.inflight.is_some()
    }

    /// The canvas gave up on the workers until its next rebuild.
    #[cfg(test)]
    pub fn given_up(&self) -> bool {
        self.broken
    }

    /// Show one straight segment on top of the canvas. [`Self::pump`]
    /// asks the workers for its stamp.
    pub fn set_line(&mut self, a: TipPoint, b: TipPoint) {
        self.line = Some((a, b));
    }

    /// Take in the Shift segment raster that landed. The board calls this
    /// before the scene paints, so a frame never paints the anchor stroke
    /// from both the scene and the canvas.
    pub fn take_landed(&mut self, tiles: &mut tiles::BrushTiles) {
        let Some(tag) = self.inflight.as_ref().map(|a| a.tag) else {
            return;
        };
        #[cfg(test)]
        if self.hold_anchor
            && self
                .inflight
                .as_ref()
                .is_some_and(|a| a.kind == AskKind::Anchor)
        {
            return;
        }
        match tiles.take_line(tiles::BRUSH_LANE, tag) {
            None => {}
            Some(tiles::LineLanded::Raster(r)) => {
                let ask = self.inflight.take().expect("checked above");
                self.losses = 0;
                self.land(ask, r);
            }
            Some(tiles::LineLanded::Lost) => {
                // [`Self::pump`] asks for the same work again.
                self.inflight = None;
                self.losses += 1;
                if self.losses > LINE_RETRIES {
                    self.give_up();
                }
            }
        }
    }

    /// The workers keep losing this canvas's jobs: stop asking and stop
    /// standing in for any stroke, so the scene paints the strokes and the
    /// preview stays the vector stand-in until the next rebuild. The canvas
    /// drops its pixels, which the scene now paints.
    fn give_up(&mut self) {
        self.clear_touched();
        self.broken = true;
        self.anchor_segs.clear();
        self.commits.clear();
        self.exact = None;
        self.held = None;
        self.reusable = false;
    }

    /// Ask the workers for the next Shift segment raster when none is out:
    /// the anchor stroke first, then committed segments, then the live
    /// segment's preview.
    ///
    /// A drag released before its anchor landed asks nothing more: the
    /// canvas then only adds its segments, as meshes, over the stroke as
    /// the scene shows it, until the stroke's own raster lands.
    pub fn pump(&mut self, tiles: &mut tiles::BrushTiles, ctx: &egui::Context) {
        if self.inflight.is_none() && !self.broken {
            if !self.anchor_segs.is_empty() {
                if self.held.is_none() {
                    let segs = self.anchor_segs.clone();
                    self.ask(tiles, segs, AskKind::Anchor);
                }
            } else if !self.commits.is_empty() {
                let segs = self.commits.clone();
                self.ask(tiles, segs, AskKind::Commit);
            } else if let Some(seg) = self.line {
                let current = self
                    .exact
                    .as_ref()
                    .is_some_and(|e| e.seg == seg && e.gen == self.gen);
                #[cfg(test)]
                let current = current || self.hold_previews;
                if !current {
                    self.ask(tiles, vec![seg], AskKind::Preview);
                }
            }
        }
        if self.inflight.is_some() {
            ctx.request_repaint();
        }
    }

    fn ask(&mut self, tiles: &mut tiles::BrushTiles, segs: Vec<Seg>, kind: AskKind) {
        let bx = segs
            .iter()
            .filter_map(|(a, b)| self.segment_box(*a, *b))
            .reduce(|u, b| union_box(Some(u), b));
        let Some(bx) = bx else {
            // Off the canvas: nothing to stamp or show.
            match kind {
                AskKind::Anchor => self.anchor_segs.clear(),
                AskKind::Commit => self.commits.clear(),
                AskKind::Preview => {}
            }
            return;
        };
        // Pixels outside everything drawn since the last reset are clear, so
        // such a box goes to the workers as a clear base, not a copy.
        let clear = self.touched.is_none_or(|t| {
            t[0] >= bx[2] || bx[0] >= t[2] || t[1] >= bx[3] || bx[1] >= t[3]
        });
        let base = if clear {
            let px = self.img.pixel;
            vector_ink::StampImage {
                width: bx[2] - bx[0],
                height: bx[3] - bx[1],
                origin: [
                    self.img.origin[0] + bx[0] as f32 * px,
                    self.img.origin[1] + bx[1] as f32 * px,
                ],
                pixel: px,
                rgba: Vec::new(),
                depth: Vec::new(),
                side: self.img.side.continuing(),
            }
        } else {
            self.region(bx)
        };
        let tag = tiles.next_line_tag();
        let job = LineJob {
            tag,
            lane: tiles::BRUSH_LANE,
            base,
            segs: segs.clone(),
            cut: None,
            #[cfg(test)]
            panic: std::mem::take(&mut self.panic_next) || self.panic_always,
        };
        let ask = LineAsk {
            tag,
            gen: self.gen,
            segs,
            kind,
            bx,
        };
        match tiles.request_line(job) {
            Ok(()) => self.inflight = Some(ask),
            Err(job) => {
                let r = line_raster(job);
                self.land(ask, r);
            }
        }
    }

    fn land(&mut self, ask: LineAsk, r: LineRaster) {
        if ask.gen != self.gen || !fits_box(&r.raw, ask.bx) {
            self.spare = spare_of(r.raw);
            return;
        }
        let grain = ask.segs.last().map_or(self.grain, |s| s.1.tip.grain);
        match ask.kind {
            AskKind::Anchor => {
                if self.anchor_segs == ask.segs && self.held.is_none() {
                    self.take_in(ask.bx, r.raw, r.image, grain);
                    self.anchor_segs.clear();
                } else {
                    self.spare = spare_of(r.raw);
                }
            }
            AskKind::Commit => {
                if self.commits.starts_with(&ask.segs) {
                    self.take_in(ask.bx, r.raw, r.image, grain);
                    self.commits.drain(..ask.segs.len());
                } else {
                    self.spare = spare_of(r.raw);
                }
            }
            AskKind::Preview => {
                self.line_tex.set_partial(
                    [ask.bx[0] as usize, ask.bx[1] as usize],
                    Shared::clone(&r.image),
                    egui::TextureOptions::LINEAR,
                );
                if let Some(old) = self.exact.take() {
                    self.spare = spare_of(old.raw);
                }
                self.exact = Some(LineExact {
                    seg: ask.segs[0],
                    gen: ask.gen,
                    bx: ask.bx,
                    raw: r.raw,
                    image: r.image,
                });
                self.adopt_exact();
            }
        }
    }

    /// The canvas, the live segment's exact stamp where it has landed, and
    /// the vector stand-in for any segment the stamps do not cover yet.
    pub fn paint(&mut self, painter: &egui::Painter, xf: &BoardXf) {
        let exact = self
            .exact
            .as_ref()
            .filter(|e| self.commits.is_empty() && e.gen == self.gen);
        // A stand-in that starts on a stamped end (the chain's, or the exact
        // stamp's) starts square there: a round start would paint over that
        // end's dab a second time.
        let start = if self.chained || !self.commits.is_empty() {
            Cap::Butt
        } else {
            Cap::Round
        };
        let (shown, rest) = match (exact, self.line) {
            (Some(e), Some(seg)) if e.seg == seg => (Some(e.bx), None),
            (Some(e), Some(seg)) if extends(e.seg, seg) => {
                (Some(e.bx), Some(((e.seg.1, seg.1), [Cap::Butt, Cap::Round])))
            }
            (_, line) => (None, line.map(|seg| (seg, [start, Cap::Round]))),
        };
        let img = (self.img.origin, self.img.pixel, [self.img.width, self.img.height]);
        let (w, h) = (self.img.width, self.img.height);
        let white = Color32::WHITE;
        match shown {
            Some(b) => {
                paint_around(painter, xf, self.tex.id(), img, b, white);
                paint_texels(painter, xf, self.line_tex.id(), img, b, white);
            }
            None => paint_texels(painter, xf, self.tex.id(), img, [0, 0, w, h], white),
        }
        self.meshes.begin();
        for i in 0..self.commits.len() {
            let first = if i == 0 && !self.chained {
                Cap::Round
            } else {
                Cap::Butt
            };
            self.meshes
                .paint(painter, xf, self.commits[i], [first, Cap::Round]);
        }
        if let Some((seg, ends)) = rest {
            self.meshes.paint(painter, xf, seg, ends);
        }
    }
}

/// Vector stand-ins for straight segments whose exact stamp has not landed:
/// each segment's mesh at its tips, built once per segment and zoom, and
/// kept while consecutive frames paint it.
#[derive(Default)]
pub struct SegMeshes {
    last: Vec<(u64, DraftMesh)>,
    next: Vec<(u64, DraftMesh)>,
}

impl SegMeshes {
    /// Meshes painted this frame.
    #[cfg(test)]
    pub(crate) fn painted(&self) -> usize {
        self.next.len()
    }

    /// Start a frame: meshes it does not paint are dropped at the next one.
    fn begin(&mut self) {
        std::mem::swap(&mut self.last, &mut self.next);
        self.next.clear();
    }

    fn paint(&mut self, painter: &egui::Painter, xf: &BoardXf, seg: Seg, ends: [Cap; 2]) {
        let key = seg_mesh_key(seg, ends, xf.z);
        let mut mesh = match self.last.iter().position(|(k, _)| *k == key) {
            Some(i) => self.last.swap_remove(i).1,
            None => {
                // The stamp is opaque out to (1 - softness) of the radius and
                // clear at the rim; the mesh's feather spans that fade.
                let fade = |p: TipPoint| p.tip.softness.clamp(0.0, 1.0) * p.tip.diameter * 0.5;
                let soft = fade(seg.0).max(fade(seg.1));
                let tip = |p: TipPoint| PlacedTip {
                    width: (p.tip.diameter - soft).max(p.tip.diameter * 0.5),
                    color: Rgba(p.tip.rgba),
                    opacity: 1.0,
                };
                let mut bez = BezPath::new();
                bez.move_to((seg.0.pos[0] as f64, seg.0.pos[1] as f64));
                bez.line_to((seg.1.pos[0] as f64, seg.1.pos[1] as f64));
                let (ink, _) =
                    draft_stroke_ink_ends(&bez, false, &[tip(seg.0), tip(seg.1)], xf.z, ends, soft);
                // Premultiplied as the stamp texture is, in bytes: egui's
                // unmultiplied colors premultiply in linear light, which
                // paints a translucent stand-in brighter than its stamp.
                let colors: Vec<Color32> = ink
                    .colors
                    .iter()
                    .map(|c| stamp_color(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)))
                    .collect();
                let mut mesh = CachedInkMesh::from(ink);
                mesh.colors = colors;
                DraftMesh {
                    key,
                    mesh,
                    color: stamp_color(seg.0.tip.rgba),
                    screen: None,
                }
            }
        };
        paint_draft_mesh(painter, xf, &mut mesh);
        self.next.push((key, mesh));
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
    upload_rows(tex, dirty, &sub);
}

/// Upload straight-alpha rows `sub` covering `dirty` into `tex`.
fn upload_rows(tex: &mut egui::TextureHandle, dirty: [u32; 4], sub: &[u8]) {
    let [x0, y0, x1, y1] = dirty;
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    if w == 0 || h == 0 || sub.len() != w * h * 4 {
        return;
    }
    tex.set_partial(
        [x0 as usize, y0 as usize],
        egui::ColorImage::from_rgba_premultiplied([w, h], &premultiplied(sub)),
        egui::TextureOptions::LINEAR,
    );
}

/// Live spot erase on one stamped stroke during an eraser drag. `ink` is the
/// stroke's committed bitmap ([`stroke_stamp`]: blur and earlier passes
/// applied), built once; `mask` holds this pass with
/// max coverage; the texture shows `ink * (1 - mask)`, uploading only the
/// region the eraser touched.
///
/// A straight (Shift) pass is never stamped on the frame loop (Art. II): as
/// for the brush's Shift segment ([`BrushLiveCanvas`]), one job at a time
/// cuts the segment out of the ink under it on the raster workers, and the
/// newest landed cut shows. A frame the cut does not cover yet paints the
/// eraser's band over the segment instead ([`paint_erase_band`]). The
/// release takes in the cut that shows the final segment, or leaves the
/// preview standing in until it lands ([`EraseSettle`]).
pub struct EraseLive {
    ink: Shared<Vec<u8>>,
    /// This pass's raw coverage and depth.
    mask: vector_ink::StampImage,
    /// The mask with the eraser's grain applied, for a textured eraser.
    grained: Vec<u8>,
    shown: Vec<u8>,
    tex: egui::TextureHandle,
    /// The mask's size; the landed cut's box of it shows that cut. Made
    /// with the first straight-pass job; a freehand pass never needs it.
    line_tex: Option<egui::TextureHandle>,
    ctx: egui::Context,
    done: usize,
    /// The straight pass's segment, not yet in `mask` or `shown`.
    line: Option<Seg>,
    /// The newest cut the workers returned for a straight pass.
    exact: Option<EraseExact>,
    /// The one straight-pass job on the workers: its tag, segment, and box.
    inflight: Option<(u64, Seg, [u32; 4])>,
    /// Straight-pass jobs lost in a row.
    losses: u32,
    /// Some visible ink lies under the pass, so release journals it.
    pub changed: bool,
}

/// A straight pass's cut from the raster workers: `seg` stamped into a clear
/// mask over box `bx`, and the ink there with that mask taken out. Its
/// pixels are in the preview's line texture.
struct EraseExact {
    seg: Seg,
    bx: [u32; 4],
    raw: vector_ink::StampImage,
    image: Shared<egui::ColorImage>,
    shown: Vec<u8>,
    changed: bool,
}

/// What an eraser release did with a preview's straight pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EraseSettled {
    /// A freehand pass: the mask already holds it.
    Freehand,
    /// The final segment is in the mask.
    Done,
    /// The final cut is still on the workers and too big to stamp here.
    Pending,
}

impl EraseLive {
    /// A preview over the stroke's committed bitmap `img`. `image` is its
    /// premultiplied texture when a worker already made it.
    fn from_stamp(
        painter: &egui::Painter,
        id: NodeId,
        img: vector_ink::StampImage,
        image: Option<egui::ColorImage>,
    ) -> EraseLive {
        let image = image.unwrap_or_else(|| {
            egui::ColorImage::from_rgba_premultiplied(
                [img.width as usize, img.height as usize],
                &premultiplied(&img.rgba),
            )
        });
        let tex = painter.ctx().load_texture(
            format!("erase-live-{}", id.0),
            image,
            egui::TextureOptions::LINEAR,
        );
        let mask = vector_ink::StampImage {
            width: img.width,
            height: img.height,
            origin: img.origin,
            pixel: img.pixel,
            rgba: vec![0u8; img.rgba.len()],
            depth: Vec::new(),
            side: vector_ink::StampSide::for_mask(),
        };
        EraseLive {
            shown: img.rgba.clone(),
            ink: Shared::new(img.rgba),
            mask,
            grained: Vec::new(),
            tex,
            line_tex: None,
            ctx: painter.ctx().clone(),
            done: 0,
            line: None,
            exact: None,
            inflight: None,
            losses: 0,
            changed: false,
        }
    }

    /// The straight-pass job on the workers, as `(lane, tag)`.
    pub(crate) fn job(&self, lane: u64) -> Option<(u64, u64)> {
        self.inflight.map(|(tag, ..)| (lane, tag))
    }

    /// A straight-pass job is on the workers.
    pub(crate) fn in_flight(&self) -> bool {
        self.inflight.is_some()
    }

    /// Any ink is left after this pass.
    pub(crate) fn left_ink(&self) -> bool {
        self.shown.iter().skip(3).step_by(4).any(|a| *a > 8)
    }

    /// The preview as the stroke's stand-in once the pass commits: it shows
    /// the committed result until the exact bitmap lands.
    pub(crate) fn into_stand_in(self, rect: WorldRect, frame: u64) -> BrushStampGpu {
        let m = &self.mask;
        BrushStampGpu {
            origin: m.origin,
            size: [m.width as f32 * m.pixel, m.height as f32 * m.pixel],
            wanted_pixel: m.pixel,
            bytes: self.shown.len(),
            used: frame,
            exact: false,
            rect,
            tex: self.tex,
            seal: None,
            fallback: None,
            tab: 0,
        }
    }

    /// Bring the mask up to date with the eraser's `points`. A straight pass
    /// is only its first and last point: it becomes the segment
    /// [`Self::pump`] asks the workers to cut, and nothing stamps here.
    fn feed(&mut self, points: &[Pos2], tip: StampStyle, straight: bool) {
        let at = |p: Pos2| TipPoint {
            pos: [p.x, p.y],
            tip,
        };
        if straight {
            self.line = straight_seg(points, tip);
            return;
        }
        let mut dirty: Option<[u32; 4]> = None;
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
        if let Some(d) = dirty {
            note_stamp_px(box_area(d));
            self.reveal(d, tip.grain);
        }
    }

    /// Take the straight pass's cut from the workers when it lands, then ask
    /// for the segment the pass shows now; the newest segment wins.
    pub(crate) fn pump(&mut self, tiles: &mut tiles::BrushTiles, lane: u64, ctx: &egui::Context) {
        self.take_landed(tiles, lane);
        self.ask_next(tiles, lane);
        if self.inflight.is_some() {
            ctx.request_repaint();
        }
    }

    /// Ask for the segment the pass shows now, when no job is out and the
    /// newest cut is of another one.
    pub(crate) fn ask_next(&mut self, tiles: &mut tiles::BrushTiles, lane: u64) {
        // Past the retries the band stands in and the release cuts once here.
        if self.inflight.is_none() && self.losses <= LINE_RETRIES {
            if let Some(seg) = self.line {
                if !self.exact.as_ref().is_some_and(|e| e.seg == seg) {
                    self.ask(tiles, lane, seg);
                }
            }
        }
    }

    /// Take the straight pass's cut if it has landed, asking for nothing.
    pub(crate) fn take_landed(&mut self, tiles: &mut tiles::BrushTiles, lane: u64) {
        let Some((tag, seg, bx)) = self.inflight else {
            return;
        };
        match tiles.take_line(lane, tag) {
            None => {}
            Some(tiles::LineLanded::Raster(r)) => {
                self.inflight = None;
                self.losses = 0;
                self.land(seg, bx, r);
            }
            Some(tiles::LineLanded::Lost) => {
                self.inflight = None;
                self.losses += 1;
            }
        }
    }

    /// Drop the job on the workers: nobody will take it.
    pub(crate) fn forget(&mut self, tiles: &mut tiles::BrushTiles, lane: u64) {
        if let Some((tag, ..)) = self.inflight.take() {
            tiles.forget_line(lane, tag);
        }
    }

    fn ask(&mut self, tiles: &mut tiles::BrushTiles, lane: u64, seg: Seg) {
        let Some(bx) = segment_box(&self.mask, seg.0, seg.1) else {
            return;
        };
        let m = &self.mask;
        if self.line_tex.is_none() {
            let clear =
                egui::ColorImage::new([m.width as usize, m.height as usize], Color32::TRANSPARENT);
            let tex = self
                .ctx
                .load_texture("erase-live-line", clear, egui::TextureOptions::LINEAR);
            self.line_tex = Some(tex);
            note_line_tex_alloc();
        }
        let tag = tiles.next_line_tag();
        let job = LineJob {
            tag,
            lane,
            base: vector_ink::StampImage {
                width: bx[2] - bx[0],
                height: bx[3] - bx[1],
                origin: [
                    m.origin[0] + bx[0] as f32 * m.pixel,
                    m.origin[1] + bx[1] as f32 * m.pixel,
                ],
                pixel: m.pixel,
                rgba: Vec::new(),
                depth: Vec::new(),
                side: m.side.continuing(),
            },
            segs: vec![seg],
            cut: Some(InkCut {
                ink: Shared::clone(&self.ink),
                width: m.width,
                bx,
            }),
            #[cfg(test)]
            panic: false,
        };
        match tiles.request_line(job) {
            Ok(()) => self.inflight = Some((tag, seg, bx)),
            Err(job) => {
                let r = line_raster(job);
                self.land(seg, bx, r);
            }
        }
    }

    fn land(&mut self, seg: Seg, bx: [u32; 4], r: LineRaster) {
        let Some(line_tex) = self.line_tex.as_mut().filter(|_| fits_box(&r.raw, bx)) else {
            return;
        };
        line_tex.set_partial(
            [bx[0] as usize, bx[1] as usize],
            Shared::clone(&r.image),
            egui::TextureOptions::LINEAR,
        );
        self.exact = Some(EraseExact {
            seg,
            bx,
            raw: r.raw,
            image: r.image,
            shown: r.shown,
            changed: r.changed,
        });
    }

    /// The landed cut to show for the straight pass: one of its segment, or
    /// of a shorter run of it.
    fn shown_exact(&self) -> Option<&EraseExact> {
        let line = self.line?;
        self.exact
            .as_ref()
            .filter(|e| e.seg == line || extends(e.seg, line))
    }

    /// Where along straight segment `seg` this stroke's preview stops
    /// showing the exact cut: `None` when it shows all of it (or `seg` misses
    /// the stroke), else the start of the rest and whether a cut runs up to it.
    fn uncovered(&self, seg: Seg) -> Option<(TipPoint, bool)> {
        segment_box(&self.mask, seg.0, seg.1)?;
        match self.exact.as_ref() {
            Some(e) if e.seg == seg => None,
            Some(e) if extends(e.seg, seg) => Some((e.seg.1, true)),
            _ => Some((seg.0, false)),
        }
    }

    /// Release of a straight pass along `points`: the cut that shows its
    /// final segment goes into the preview. Without one, the segment stamps
    /// here only when `fits` grants its box from the frame's raster budget
    /// (or the workers keep losing the job); otherwise the pass stays
    /// [`EraseSettled::Pending`] and [`Self::pump`] keeps asking for the cut.
    pub(crate) fn settle_line(
        &mut self,
        points: &[Pos2],
        tip: StampStyle,
        fits: &mut dyn FnMut(f32) -> bool,
    ) -> EraseSettled {
        if self.line.is_none() {
            return EraseSettled::Freehand;
        }
        let Some(seg) = straight_seg(points, tip) else {
            self.line = None;
            return EraseSettled::Done;
        };
        if let Some(e) = self.exact.take_if(|e| e.seg == seg) {
            self.take_in(e);
        } else if let Some(bx) = segment_box(&self.mask, seg.0, seg.1) {
            if self.losses <= LINE_RETRIES && !fits(box_area(bx) as f32) {
                self.line = Some(seg);
                return EraseSettled::Pending;
            }
            stamp_segment(&mut self.mask, seg.0, seg.1);
            note_stamp_px(box_area(bx));
            self.reveal(bx, tip.grain);
        }
        self.line = None;
        self.inflight = None;
        EraseSettled::Done
    }

    /// A pending straight pass took in its final cut: the preview shows the
    /// committed result.
    pub(crate) fn settled(&mut self) -> bool {
        let Some(line) = self.line else {
            return true;
        };
        let Some(e) = self.exact.take_if(|e| e.seg == line) else {
            return false;
        };
        self.take_in(e);
        self.line = None;
        true
    }

    /// The workers lost this pass's cut past the retries.
    pub(crate) fn gave_up(&self) -> bool {
        self.inflight.is_none() && self.losses > LINE_RETRIES
    }

    fn take_in(&mut self, e: EraseExact) {
        let [x0, y0, x1, y1] = e.bx;
        let w = self.mask.width as usize;
        let bw = (x1 - x0) as usize;
        if !e.raw.depth.is_empty() && self.mask.depth.is_empty() {
            self.mask.depth = vec![0; w * self.mask.height as usize];
        }
        for (i, y) in (y0 as usize..y1 as usize).enumerate() {
            let row = (y * w + x0 as usize) * 4..(y * w + x1 as usize) * 4;
            self.mask.rgba[row.clone()].copy_from_slice(&e.raw.rgba[i * bw * 4..(i + 1) * bw * 4]);
            self.shown[row].copy_from_slice(&e.shown[i * bw * 4..(i + 1) * bw * 4]);
            if !e.raw.depth.is_empty() {
                self.mask.depth[y * w + x0 as usize..y * w + x1 as usize]
                    .copy_from_slice(&e.raw.depth[i * bw..(i + 1) * bw]);
            }
        }
        let (width, height) = (self.mask.width, self.mask.height);
        self.mask.side.paste(width, height, e.bx, &e.raw.side);
        self.changed = e.changed;
        self.tex.set_partial(
            [x0 as usize, y0 as usize],
            e.image,
            egui::TextureOptions::LINEAR,
        );
    }

    /// Recompute `shown` as ink minus the mask over box `d`, and upload it.
    fn reveal(&mut self, d: [u32; 4], grain: vector_ink::Grain) {
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
        if grain == vector_ink::Grain::Smooth {
            vector_ink::multiply_by_mask(&mut self.shown, &self.mask.rgba, w, Some(d));
        } else {
            if self.grained.len() != self.mask.rgba.len() {
                self.grained = vec![0; self.mask.rgba.len()];
            }
            let rows = vector_ink::finished_region(&self.mask, grain, d);
            let span = (x1 - x0) as usize * 4;
            for (row, y) in rows.chunks_exact(span).zip(y0..y1) {
                let at = ((y * w + x0) * 4) as usize;
                self.grained[at..at + span].copy_from_slice(row);
            }
            vector_ink::multiply_by_mask(&mut self.shown, &self.grained, w, Some(d));
        }
        upload_region(&mut self.tex, &self.shown, w, d);
    }

    /// The stroke minus the pass, moved by `shift`; for a straight pass,
    /// the landed cut in its box and the stroke as committed around it.
    fn paint(&self, painter: &egui::Painter, xf: &BoardXf, tint: Color32, shift: [f32; 2]) {
        let m = &self.mask;
        let origin = [m.origin[0] + shift[0], m.origin[1] + shift[1]];
        let img = (origin, m.pixel, [m.width, m.height]);
        let exact = self.shown_exact().zip(self.line_tex.as_ref());
        let Some((e, line_tex)) = exact else {
            paint_texels(
                painter,
                xf,
                self.tex.id(),
                img,
                [0, 0, m.width, m.height],
                tint,
            );
            return;
        };
        paint_around(painter, xf, self.tex.id(), img, e.bx, tint);
        paint_texels(painter, xf, line_tex.id(), img, e.bx, tint);
    }

    #[cfg(test)]
    pub(crate) fn texture(&self) -> egui::TextureId {
        self.tex.id()
    }

    #[cfg(test)]
    pub(crate) fn live_line_end(&self) -> Option<[f32; 2]> {
        self.line.map(|(_, b)| b.pos)
    }

    /// The preview shows the straight pass's exact cut.
    #[cfg(test)]
    pub(crate) fn line_exact(&self) -> bool {
        self.line.is_some() && self.uncovered(self.line.unwrap()).is_none()
    }

    #[cfg(test)]
    pub(crate) fn mask(&self) -> &vector_ink::StampImage {
        &self.mask
    }

    #[cfg(test)]
    pub(crate) fn shown(&self) -> &[u8] {
        &self.shown
    }
}

pub struct PathDraftPaintStyle {
    /// The tip the point being placed takes: the tool's tip now.
    pub tip: PlacedTip,
    pub overlay: PathEditAnchorColors,
    pub zoom: f32,
    /// When true, the first committed anchor draws hollow (close-path hover).
    pub close_first_anchor: bool,
}

/// The draft's preview: the path it would commit with the pointer at
/// `cursor`, whether that path closes, and the tip at each of its grips.
/// Placed points keep the tip they were placed with; the point being
/// placed takes `tip`, the tool's tip now.
pub fn path_draft_preview(
    draft: &BoardPathDraft,
    cursor: Option<Pos2>,
    tip: PlacedTip,
) -> Option<(BezPath, bool, Vec<PlacedTip>)> {
    let polyline = |pts: &[Pos2]| {
        let mut bez = BezPath::new();
        bez.move_to(to_k(pts[0]));
        for p in &pts[1..] {
            bez.line_to(to_k(*p));
        }
        bez
    };
    let with_cursor = |points: &[Pos2], tips: &[PlacedTip]| {
        let (mut pts, mut ws) = (points.to_vec(), tips.to_vec());
        if let Some(c) = cursor {
            pts.push(c);
            ws.push(tip);
        }
        (pts, ws)
    };
    match draft {
        BoardPathDraft::Polyline { points, tips } => {
            let (pts, ws) = with_cursor(points, tips);
            (pts.len() >= 2).then(|| (polyline(&pts), false, ws))
        }
        BoardPathDraft::Arc { points, tips } => {
            // Click order is start → end → middle. The through-point is the
            // last pick so dragging it changes bulge only (endpoints stay).
            let (pts, ws) = with_cursor(points, tips);
            match (pts.as_slice(), ws.as_slice()) {
                ([start, end], [w0, w1]) => {
                    Some((polyline(&[*start, *end]), false, vec![*w0, *w1]))
                }
                ([start, end, mid, ..], [w0, w1, w2, ..]) => Some((
                    arc_through_three_points(*start, *mid, *end),
                    false,
                    vec![*w0, *w2, *w1],
                )),
                _ => None,
            }
        }
        BoardPathDraft::Bezier {
            anchors,
            tips,
            placing,
            close_hover,
            ..
        } => {
            if close_hover.ready {
                return Some((
                    bezier_anchors_to_closed_bezpath(anchors),
                    true,
                    tips.clone(),
                ));
            }
            let (mut span, mut ws) = (anchors.clone(), tips.clone());
            if let Some(p) = placing {
                span.push(*p);
                ws.push(tip);
            }
            if span.len() >= 2 {
                return Some((bezier_anchors_to_bezpath(&span), false, ws));
            }
            let c = cursor?;
            let ((a, h), from) = match placing {
                Some(p) => (*p, tip),
                None => (*anchors.last()?, *tips.last()?),
            };
            let mut bez = BezPath::new();
            bez.move_to(to_k(a));
            bez.curve_to(to_k(a + h.handle_out), to_k(c), to_k(c));
            Some((bez, false, vec![from, tip]))
        }
    }
}

/// A draft stroke's mesh and color: blended between its grip tips the way
/// the committed curve will paint (`vertex_style::set_grip_placed_tips`,
/// `geom::tipped_stroke`), else one tip. Each tip paints at the opacity it
/// was placed with; the mesh carries straight per-vertex colors when the
/// colors vary, and the color is the first tip's.
pub(crate) fn draft_stroke_ink(
    bez: &BezPath,
    closed: bool,
    tips: &[PlacedTip],
    zoom: f32,
) -> (InkMesh, Color32) {
    draft_stroke_ink_ends(bez, closed, tips, zoom, [Cap::Round; 2], 0.0)
}

/// [`draft_stroke_ink`] with `ends` capping an open stroke's start and end,
/// and its edge fading over at least `soft_edge` world units.
fn draft_stroke_ink_ends(
    bez: &BezPath,
    closed: bool,
    tips: &[PlacedTip],
    zoom: f32,
    ends: [Cap; 2],
    soft_edge: f32,
) -> (InkMesh, Color32) {
    let floor = 1.0 / zoom.max(0.05);
    let tips: Vec<PlacedTip> = tips
        .iter()
        .map(|t| PlacedTip {
            width: t.width.max(floor),
            color: t.rgba(),
            opacity: 1.0,
        })
        .collect();
    let Some(&first) = tips.first() else {
        return (InkMesh::default(), Color32::TRANSPARENT);
    };
    let style = StrokeStyle {
        width: tips.iter().map(|t| t.width).fold(floor, f32::max),
        cap: Cap::Round,
        join: Join::Round,
        taper: None,
        dash: None,
    };
    let feather = (FEATHER_PX / zoom.max(0.05)).max(soft_edge);
    let tolerance = curve_tolerance(zoom);
    let tipped = tips.iter().any(|t| *t != first).then(|| {
        let (rect, mut data) = bezpath_to_path_data(bez, closed);
        let mut stroke = Stroke {
            width: style.width,
            ..default_curve_stroke(first.color)
        };
        slate_doc::vertex_style::set_grip_placed_tips(&mut data, &mut stroke, rect, 0.0, &tips)?;
        let square = slate_doc::scene::Corner::Square;
        slate_doc::geom::tipped_stroke(&data, &stroke, rect, 0.0, square)
    });
    let ink = match tipped.flatten() {
        Some(t) => vector_ink::stroke_mesh_ends(
            &t.bez,
            &style,
            ends,
            Some(&t.widths),
            t.colors.as_deref(),
            t.ease,
            feather,
            tolerance,
        ),
        None => vector_ink::stroke_mesh_ends(
            bez,
            &style,
            ends,
            None,
            None,
            vector_ink::TipEase::Linear,
            feather,
            tolerance,
        ),
    };
    (ink, rgba32(first.color))
}

/// Settled Pen samples in each piece of its live preview.
const PEN_PIECE: usize = 64;

/// A draft mesh, the key of what it was built from, and its screen mesh
/// for the camera it was last painted at.
struct DraftMesh {
    key: u64,
    mesh: CachedInkMesh,
    color: Color32,
    screen: Option<(u64, Shared<egui::Mesh>)>,
}

/// The live draft previews' meshes (Art. II): a frame whose draft, tips,
/// and zoom are unchanged repaints them without tessellating, and without
/// allocating while the camera holds still. A Pen stroke paints as pieces
/// of [`PEN_PIECE`] settled samples, each built once, then the piece still
/// being drawn, the only one a pointer move rebuilds. Pieces meet with butt
/// ends in the middle of a segment, at that point's tip, so together they
/// cover the stroke as one mesh would.
#[derive(Default)]
pub(crate) struct DraftInkCache {
    draft: Option<DraftMesh>,
    pen: Vec<Option<DraftMesh>>,
    /// Pieces below this are settled and built for `pen_stroke`. A live
    /// stroke only appends samples, so they are painted without hashing.
    pen_settled: usize,
    /// The live stroke's first sample, its tip, and the zoom.
    pen_stroke: u64,
    /// Draft meshes built so far; the board counts each frame's as
    /// tessellation misses.
    pub(crate) builds: u32,
    /// Pen pieces whose samples were hashed so far.
    #[cfg(test)]
    pub(crate) pen_hashes: u32,
}

fn draft_mesh<'a>(
    slot: &'a mut Option<DraftMesh>,
    builds: &mut u32,
    key: u64,
    build: impl FnOnce() -> (InkMesh, Color32),
) -> &'a mut DraftMesh {
    if slot.as_ref().is_none_or(|m| m.key != key) {
        *builds = builds.wrapping_add(1);
        let (ink, color) = build();
        *slot = Some(DraftMesh {
            key,
            mesh: ink.into(),
            color,
            screen: None,
        });
    }
    slot.as_mut().expect("filled above")
}

fn paint_draft_mesh(painter: &egui::Painter, xf: &BoardXf, m: &mut DraftMesh) {
    if m.mesh.indices.is_empty() {
        return;
    }
    let mut h = DefaultHasher::new();
    hash_pos(&mut h, xf.center);
    hash_xy(&mut h, [xf.offset.x, xf.offset.y]);
    hash_f32(&mut h, xf.z);
    let view = h.finish();
    let screen = match &m.screen {
        Some((key, mesh)) if *key == view => mesh.clone(),
        _ => {
            let mesh = Shared::new(ink_mesh_to_epaint(&m.mesh, xf, m.color, |c| c));
            m.screen = Some((view, mesh.clone()));
            mesh
        }
    };
    painter.add(Shape::Mesh(screen));
}

fn hash_tip(h: &mut impl Hasher, tip: &PlacedTip) {
    hash_f32(h, tip.width);
    tip.color.0.hash(h);
    hash_f32(h, tip.opacity);
}

fn hash_pos(h: &mut impl Hasher, p: Pos2) {
    hash_xy(h, [p.x, p.y]);
}

fn hash_anchor(h: &mut impl Hasher, (p, handles): &(Pos2, BezierHandles)) {
    hash_pos(h, *p);
    hash_xy(h, [handles.handle_in.x, handles.handle_in.y]);
    hash_xy(h, [handles.handle_out.x, handles.handle_out.y]);
}

/// Everything [`path_draft_preview`] reads, so the key changes exactly
/// when the preview can.
fn hash_path_draft(
    h: &mut impl Hasher,
    draft: &BoardPathDraft,
    cursor: Option<Pos2>,
    tip: &PlacedTip,
) {
    let reads_cursor = match draft {
        BoardPathDraft::Polyline { points, tips } | BoardPathDraft::Arc { points, tips } => {
            matches!(draft, BoardPathDraft::Arc { .. }).hash(h);
            points.iter().for_each(|p| hash_pos(h, *p));
            tips.iter().for_each(|t| hash_tip(h, t));
            true
        }
        BoardPathDraft::Bezier {
            anchors,
            tips,
            placing,
            close_hover,
            ..
        } => {
            2u8.hash(h);
            anchors.iter().for_each(|a| hash_anchor(h, a));
            tips.iter().for_each(|t| hash_tip(h, t));
            placing.is_some().hash(h);
            if let Some(p) = placing {
                hash_anchor(h, p);
            }
            close_hover.ready.hash(h);
            !close_hover.ready && anchors.len() + usize::from(placing.is_some()) < 2
        }
    };
    hash_tip(h, tip);
    if reads_cursor {
        cursor.is_some().hash(h);
        if let Some(c) = cursor {
            hash_pos(h, c);
        }
    }
}

/// Piece `index` of a live Pen stroke of `pts` drawn with `tip(i)`: the
/// samples from `index * PEN_PIECE` on, starting mid-segment after the
/// first piece. A settled piece ends in the middle of the segment after
/// its last sample; the last piece ends at `cursor`.
fn pen_piece(
    pts: &[Pos2],
    tip: impl Fn(usize) -> PlacedTip,
    cursor: (Pos2, PlacedTip),
    index: usize,
    settled: bool,
) -> (BezPath, Vec<PlacedTip>, [Cap; 2]) {
    let from = index * PEN_PIECE;
    let mid = |i: usize| {
        (
            pts[i].lerp(pts[i + 1], 0.5),
            PlacedTip::lerp(tip(i), tip(i + 1), 0.5),
        )
    };
    let start = if index == 0 {
        (pts[0], tip(0))
    } else {
        mid(from)
    };
    let (to, end) = if settled {
        (from + PEN_PIECE, mid(from + PEN_PIECE))
    } else {
        (pts.len() - 1, cursor)
    };
    let mut bez = BezPath::new();
    bez.move_to(to_k(start.0));
    let mut tips = Vec::with_capacity(to - from + 2);
    tips.push(start.1);
    for i in from + 1..=to {
        bez.line_to(to_k(pts[i]));
        tips.push(tip(i));
    }
    bez.line_to(to_k(end.0));
    tips.push(end.1);
    let ends = [
        if index == 0 { Cap::Round } else { Cap::Butt },
        if settled { Cap::Butt } else { Cap::Round },
    ];
    (bez, tips, ends)
}

impl DraftInkCache {
    /// Paint a path draft's preview ([`path_draft_preview`] with the
    /// pointer at `cursor` and the tool's tip now `tip`).
    pub(crate) fn paint_path(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        draft: &BoardPathDraft,
        cursor: Option<Pos2>,
        tip: PlacedTip,
    ) {
        let mut h = DefaultHasher::new();
        0u8.hash(&mut h);
        hash_path_draft(&mut h, draft, cursor, &tip);
        hash_f32(&mut h, xf.z);
        let build = || match path_draft_preview(draft, cursor, tip) {
            Some((bez, closed, tips)) => draft_stroke_ink(&bez, closed, &tips, xf.z),
            None => (InkMesh::default(), Color32::TRANSPARENT),
        };
        let m = draft_mesh(&mut self.draft, &mut self.builds, h.finish(), build);
        paint_draft_mesh(painter, xf, m);
    }

    /// Paint the Line tool's rubber band from `a` to `b`, with a tip at
    /// each end.
    pub(crate) fn paint_line(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        a: Pos2,
        b: Pos2,
        tips: [PlacedTip; 2],
    ) {
        let mut h = DefaultHasher::new();
        1u8.hash(&mut h);
        hash_pos(&mut h, a);
        hash_pos(&mut h, b);
        tips.iter().for_each(|t| hash_tip(&mut h, t));
        hash_f32(&mut h, xf.z);
        let m = draft_mesh(&mut self.draft, &mut self.builds, h.finish(), || {
            draft_stroke_ink(&line_bez(a, b), false, &tips, xf.z)
        });
        paint_draft_mesh(painter, xf, m);
    }

    /// Paint the Pen's live stroke: its samples `pts`, each with the tip it
    /// was drawn with (`tips`), then the pointer with `cursor_tip`.
    pub(crate) fn paint_pen(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        pts: &[Pos2],
        tips: &[PlacedTip],
        cursor: Pos2,
        cursor_tip: PlacedTip,
    ) {
        if pts.is_empty() {
            self.forget_pen();
            return;
        }
        let tip = |i: usize| tips.get(i).copied().unwrap_or(cursor_tip);
        let settled = pts.len().saturating_sub(2) / PEN_PIECE;
        let mut h = DefaultHasher::new();
        hash_pos(&mut h, pts[0]);
        hash_tip(&mut h, &tip(0));
        hash_f32(&mut h, xf.z);
        let stroke = h.finish();
        if stroke != self.pen_stroke || settled < self.pen_settled {
            self.pen_stroke = stroke;
            self.pen_settled = 0;
        }
        self.pen.resize_with(settled + 1, || None);
        for (index, slot) in self.pen.iter_mut().enumerate() {
            if index < self.pen_settled {
                if let Some(m) = slot {
                    paint_draft_mesh(painter, xf, m);
                }
                continue;
            }
            #[cfg(test)]
            {
                self.pen_hashes += 1;
            }
            let is_settled = index < settled;
            let from = index * PEN_PIECE;
            let to = if is_settled {
                from + PEN_PIECE + 1
            } else {
                pts.len() - 1
            };
            let mut h = DefaultHasher::new();
            (index == 0, is_settled).hash(&mut h);
            for i in from..=to {
                hash_pos(&mut h, pts[i]);
                hash_tip(&mut h, &tip(i));
            }
            if !is_settled {
                hash_pos(&mut h, cursor);
                hash_tip(&mut h, &cursor_tip);
            }
            hash_f32(&mut h, xf.z);
            let m = draft_mesh(slot, &mut self.builds, h.finish(), || {
                let (bez, piece_tips, ends) =
                    pen_piece(pts, tip, (cursor, cursor_tip), index, is_settled);
                draft_stroke_ink_ends(&bez, false, &piece_tips, xf.z, ends, 0.0)
            });
            paint_draft_mesh(painter, xf, m);
        }
        self.pen_settled = settled;
    }

    /// Drop the Pen's pieces once no Pen stroke is live.
    pub(crate) fn forget_pen(&mut self) {
        self.pen.clear();
        self.pen_settled = 0;
    }

    #[cfg(test)]
    pub(crate) fn pen_pieces(&self) -> usize {
        self.pen.len()
    }

    /// The path or Line draft's cached mesh and color.
    #[cfg(test)]
    pub(crate) fn draft_mesh(&self) -> Option<(&CachedInkMesh, Color32)> {
        self.draft.as_ref().map(|m| (&m.mesh, m.color))
    }

    /// The path or Line draft's screen mesh as last painted.
    #[cfg(test)]
    pub(crate) fn draft_screen_mesh(&self) -> Option<Shared<egui::Mesh>> {
        self.draft.as_ref()?.screen.as_ref().map(|(_, m)| m.clone())
    }
}

fn line_bez(a: Pos2, b: Pos2) -> BezPath {
    let mut bez = BezPath::new();
    bez.move_to(to_k(a));
    bez.line_to(to_k(b));
    bez
}

pub fn paint_path_draft(
    painter: &egui::Painter,
    xf: &BoardXf,
    draft: &BoardPathDraft,
    cursor: Option<Pos2>,
    style: PathDraftPaintStyle,
    ink: &mut DraftInkCache,
) {
    ink.paint_path(painter, xf, draft, cursor, style.tip);
    if let BoardPathDraft::Bezier {
        anchors, placing, ..
    } = draft
    {
        let overlay = bezier_draft_overlay(xf, anchors, *placing, style.close_first_anchor);
        paint_path_edit_anchors(painter, None, &overlay, style.overlay);
    }
}

/// Capture and fit tolerances are screen-space, independent of board zoom.
pub const FREEHAND_SAMPLE_SPACING_PX: f32 = 1.75;
pub const FREEHAND_FIT_ERROR_PX: f32 = 2.0;

impl SlateApp {
    pub(crate) fn cancel_path_draft(&mut self) {
        self.board_path_draft = None;
    }

    pub(crate) fn finish_path_draft(&mut self) -> bool {
        let Some(draft) = self.board_path_draft.take() else {
            return false;
        };
        let (tool, rect, path_data, closed, tips) = match draft {
            BoardPathDraft::Polyline {
                mut points,
                mut tips,
            } => {
                if points.len() < 2 {
                    return false;
                }
                let closed = points.len() >= 4 && points.first() == points.last();
                if closed {
                    points.pop();
                    tips.truncate(points.len());
                }
                let (r, d) = points_to_path_data(&points, closed);
                (StrokeTool::Polyline, r, d, closed, tips)
            }
            BoardPathDraft::Bezier { anchors, tips, .. } => {
                if anchors.len() < 2 {
                    return false;
                }
                let bez = bezier_anchors_to_bezpath(&anchors);
                let (r, d) = bezpath_to_path_data(&bez, false);
                (StrokeTool::Bezier, r, d, false, tips)
            }
            BoardPathDraft::Arc { .. } => return false,
        };
        if path_data.is_empty() {
            return false;
        }
        self.commit_drawn_path(tool, rect, path_data, closed, DrawnTips::Grips(&tips));
        true
    }

    pub(crate) fn commit_path_node(
        &mut self,
        tool: StrokeTool,
        rect: WorldRect,
        path_data: PathData,
        closed: bool,
    ) {
        self.commit_drawn_path(tool, rect, path_data, closed, DrawnTips::None);
    }

    /// The tip `tool` places a point with now (P1.curve.tip-chord).
    pub(crate) fn placed_tip(&self, tool: StrokeTool) -> PlacedTip {
        let stroke = self.stroke_for_tool(tool);
        PlacedTip {
            width: stroke.width,
            color: stroke.color,
            opacity: self.opacity_for_tool(tool),
        }
    }

    /// Commit a drawn curve with the tips its points were placed with
    /// (P1.curve.tip-chord). The tool keeps its current tip for the next
    /// curve. Returns the new node's id.
    pub(crate) fn commit_drawn_path(
        &mut self,
        tool: StrokeTool,
        rect: WorldRect,
        path_data: PathData,
        closed: bool,
        tips: DrawnTips<'_>,
    ) -> Vec<NodeId> {
        let mut path_data = path_data;
        path_data.closed = closed;
        let current = self.placed_tip(tool);
        let mut stroke = self.stroke_for_tool(tool);
        if let Some(widest) = path_data.tips.iter().map(|t| t.width).reduce(f32::max) {
            stroke.width = widest;
        }
        let mut opacity = current.opacity;
        let placed = match tips {
            DrawnTips::None => None,
            DrawnTips::Grips(tips) => slate_doc::vertex_style::set_grip_placed_tips(
                &mut path_data,
                &mut stroke,
                rect,
                0.0,
                tips,
            ),
            DrawnTips::Vertices(tips) => {
                let (op, spans) = slate_doc::vertex_style::placed_spans(&stroke, tips);
                slate_doc::vertex_style::set_vertex_tips(&mut path_data, &mut stroke, spans)
                    .then_some(op)
            }
        };
        if let Some(op) = placed {
            opacity = op;
        }
        let fill = if closed {
            self.fill_for_new_shape()
        } else {
            None
        };
        let mut node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill,
                stroke,
                corner: slate_doc::scene::Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(path_data.into()),

                text: None,
            }),
        );
        node.opacity = opacity;
        self.note_tool_style(tool, &node);
        // The next stroke starts with the tip last chosen, not the widest.
        self.keep_tool_tip(tool, current);
        let ids = self.commit_created_nodes(vec![node]);
        self.select_created_nodes(ids.clone());
        self.board_tool = super::board::BoardTool::Select;
        ids
    }

    pub(crate) fn path_tool_click(&mut self, world: Pos2) {
        // Ortho (F8, Shift inverts): draft segments snap to 45° from the
        // last anchor (constraints spec §1); a Tab lock holds the direction.
        // Placing the point ends the lock.
        let from = self.pending_segment_origin();
        let world = self.resolve_segment_point(from, world, self.shift_down);
        self.draft_lock = None;
        match self.board_tool {
            super::board::BoardTool::Polyline => {
                let tip = self.placed_tip(StrokeTool::Polyline);
                if let Some(BoardPathDraft::Polyline { points, tips }) = &mut self.board_path_draft
                {
                    if points.last().copied() != Some(world) {
                        points.push(world);
                        tips.push(tip);
                    }
                    if points.len() >= 4 && points.first() == points.last() {
                        self.finish_path_draft();
                    }
                } else {
                    self.board_path_draft = Some(BoardPathDraft::Polyline {
                        points: vec![world],
                        tips: vec![tip],
                    });
                }
            }
            super::board::BoardTool::Arc => {
                let (mut pts, mut tips) = match self.board_path_draft.take() {
                    Some(BoardPathDraft::Arc { points, tips }) => (points, tips),
                    _ => (Vec::new(), Vec::new()),
                };
                pts.push(world);
                tips.push(self.placed_tip(StrokeTool::Arc));
                if pts.len() >= 3 {
                    // start, end, middle → through-point is the last pick
                    let bez = arc_through_three_points(pts[0], pts[2], pts[1]);
                    let (rect, data) = bezpath_to_path_data(&bez, false);
                    let grips = [tips[0], tips[2], tips[1]];
                    self.commit_drawn_path(
                        StrokeTool::Arc,
                        rect,
                        data,
                        false,
                        DrawnTips::Grips(&grips),
                    );
                    return;
                }
                self.board_path_draft = Some(BoardPathDraft::Arc { points: pts, tips });
            }
            _ => {}
        }
    }

    pub(crate) fn bezier_anchor_press(&mut self, press: Pos2) {
        match &mut self.board_path_draft {
            Some(BoardPathDraft::Bezier { placing, .. }) => {
                *placing = Some((press, BezierHandles::default()));
            }
            _ => {
                self.board_path_draft = Some(BoardPathDraft::Bezier {
                    anchors: vec![],
                    tips: vec![],
                    placing: Some((press, BezierHandles::default())),
                    redo: vec![],
                    last_press_placed: true,
                    close_hover: CloseHover::default(),
                });
            }
        }
    }

    /// Track the pointer dwelling on the start anchor of a span of two or
    /// more anchors, outside any drag. Returns the seconds left before the
    /// closed preview shows, while that is still pending.
    pub(crate) fn bezier_close_hover(&mut self, pointer: Option<Pos2>, now: f64) -> Option<f64> {
        let on_start = self.board_drag.is_none()
            && pointer.is_some_and(|p| self.bezier_draft_hit(p) == Some(PathEditHit::Anchor(0)));
        let Some(BoardPathDraft::Bezier {
            anchors,
            close_hover,
            ..
        }) = &mut self.board_path_draft
        else {
            return None;
        };
        if !on_start || anchors.len() < 2 {
            *close_hover = CloseHover::default();
            return None;
        }
        let since = *close_hover.since.get_or_insert(now);
        let left = BEZIER_CLOSE_HOVER_S - (now - since);
        close_hover.ready = left <= 0.0;
        (!close_hover.ready).then_some(left)
    }

    /// The closed preview is showing on the span's start anchor.
    pub(crate) fn bezier_close_ready(&self) -> bool {
        matches!(
            &self.board_path_draft,
            Some(BoardPathDraft::Bezier { close_hover, .. }) if close_hover.ready
        )
    }

    /// Commit the draft as a closed path (a click on the start anchor while
    /// the closed preview shows). One journaled add, like any finish.
    pub(crate) fn close_bezier_draft(&mut self) -> bool {
        let Some(BoardPathDraft::Bezier { anchors, tips, .. }) = self.board_path_draft.take()
        else {
            return false;
        };
        if anchors.len() < 2 {
            return false;
        }
        let bez = bezier_anchors_to_closed_bezpath(&anchors);
        let (rect, data) = bezpath_to_path_data(&bez, true);
        if data.is_empty() {
            return false;
        }
        let tips = DrawnTips::Grips(&tips);
        self.commit_drawn_path(StrokeTool::Bezier, rect, data, true, tips);
        true
    }

    /// A press on a placed draft anchor or handle knob, by the shared
    /// path-edit hit rules. Such a press edits instead of placing (D17).
    pub(crate) fn bezier_draft_hit(&self, screen: Pos2) -> Option<PathEditHit> {
        let Some(BoardPathDraft::Bezier {
            anchors,
            placing: None,
            ..
        }) = &self.board_path_draft
        else {
            return None;
        };
        let xf = self.board_xf();
        path_edit_hit(&bezier_draft_overlay(&xf, anchors, None, false), screen)
    }

    /// Live drag of a placed draft anchor or handle, from the press-time
    /// anchors. The dragged point keeps its offset from the press and snaps
    /// itself, not the cursor, to board snaps and the draft's other anchors
    /// (a handle never to its own anchor). Handles follow
    /// `vector_ink::drag_handle` with the finished curve's modifiers
    /// (`board_direct::handle_drag_mode`).
    pub(crate) fn bezier_draft_edit(
        &mut self,
        hit: PathEditHit,
        start: Pos2,
        anchors0: &[(Pos2, BezierHandles)],
        world: Pos2,
        mods: egui::Modifiers,
    ) {
        let (i, point0) = match hit {
            PathEditHit::Anchor(i) => (i, anchors0.get(i).map(|a| a.0)),
            PathEditHit::Handle(i, end) => (
                i,
                anchors0.get(i).map(|(p, h)| {
                    *p + match end {
                        HandleEnd::In => h.handle_in,
                        HandleEnd::Out => h.handle_out,
                    }
                }),
            ),
        };
        let carried = point0.unwrap_or(start) + (world - start);
        let others: Vec<Pos2> = anchors0
            .iter()
            .enumerate()
            .filter(|(k, _)| *k != i)
            .map(|(_, a)| a.0)
            .collect();
        let snapped = self.snap_with_own_points(carried, &[], &others);
        let edited = match hit {
            PathEditHit::Anchor(i) => {
                let mut edited = anchors0.to_vec();
                if let Some(a) = edited.get_mut(i) {
                    a.0 = snapped;
                }
                edited
            }
            PathEditHit::Handle(i, end) => {
                let mut ink = bezier_draft_to_ink(anchors0);
                drag_handle(
                    &mut ink,
                    i,
                    end,
                    to_k(snapped),
                    super::board_direct::handle_drag_mode(mods),
                );
                bezier_draft_from_ink(&ink)
            }
        };
        if let Some(BoardPathDraft::Bezier { anchors, .. }) = &mut self.board_path_draft {
            *anchors = edited;
        }
    }

    /// Ctrl+Z while drawing a span: take back the last anchor; with none
    /// left, leave drawing. Returns whether the draft owned the keypress.
    pub(crate) fn bezier_draft_undo(&mut self) -> bool {
        if self.board_tool != super::board::BoardTool::BezierSpan {
            return false;
        }
        let tip = self.placed_tip(StrokeTool::Bezier);
        let Some(BoardPathDraft::Bezier {
            anchors,
            tips,
            placing,
            redo,
            ..
        }) = &mut self.board_path_draft
        else {
            return false;
        };
        if placing.is_some() {
            return true;
        }
        match anchors.pop() {
            Some(last) => redo.push((last, tips.pop().unwrap_or(tip))),
            None => {
                self.board_path_draft = None;
                self.set_board_tool(super::board::BoardTool::Select);
            }
        }
        true
    }

    /// Ctrl+Y / Ctrl+Shift+Z while drawing: put back the last anchor taken.
    pub(crate) fn bezier_draft_redo(&mut self) -> bool {
        if self.board_tool != super::board::BoardTool::BezierSpan {
            return false;
        }
        let Some(BoardPathDraft::Bezier {
            anchors,
            tips,
            placing,
            redo,
            ..
        }) = &mut self.board_path_draft
        else {
            return false;
        };
        if placing.is_none() {
            if let Some((next, tip)) = redo.pop() {
                anchors.push(next);
                tips.push(tip);
            }
        }
        true
    }

    /// A double-click ends a span only when its second press landed on a
    /// placed anchor: two quick clicks in different places are two anchors
    /// (D04).
    pub(crate) fn bezier_double_click_finishes(&self) -> bool {
        !matches!(
            &self.board_path_draft,
            Some(BoardPathDraft::Bezier {
                last_press_placed: true,
                ..
            })
        )
    }

    /// A press on a placed anchor or handle starts an edit, not a placement.
    pub(crate) fn bezier_note_edit_press(&mut self) {
        if let Some(BoardPathDraft::Bezier {
            last_press_placed, ..
        }) = &mut self.board_path_draft
        {
            *last_press_placed = false;
        }
    }

    /// Esc on a path draft: a span of two or more anchors commits as it
    /// stands (D12); anything else cancels.
    pub(crate) fn escape_path_draft(&mut self) {
        let commit = matches!(
            &self.board_path_draft,
            Some(BoardPathDraft::Bezier { anchors, .. }) if anchors.len() >= 2
        );
        if !(commit && self.finish_path_draft()) {
            self.cancel_path_draft();
        }
    }

    pub(crate) fn bezier_anchor_release(&mut self, press: Pos2, world: Pos2, alt: bool) {
        self.draft_lock = None;
        let zoom = self.tab().cam.z.max(f32::EPSILON);
        let thresh_sq = (DRAFT_DRAG_THRESHOLD_PX / zoom).powi(2);
        let out = world - press;
        let handles = if out.length_sq() > thresh_sq {
            if alt {
                BezierHandles {
                    handle_out: out,
                    handle_in: Vec2::ZERO,
                }
            } else {
                BezierHandles {
                    handle_out: out,
                    handle_in: -out,
                }
            }
        } else {
            BezierHandles::default()
        };
        let tip = self.placed_tip(StrokeTool::Bezier);
        match &mut self.board_path_draft {
            Some(BoardPathDraft::Bezier {
                anchors,
                tips,
                placing,
                redo,
                last_press_placed,
                ..
            }) => {
                anchors.push((press, handles));
                tips.push(tip);
                *placing = None;
                redo.clear();
                *last_press_placed = true;
            }
            _ => {
                self.board_path_draft = Some(BoardPathDraft::Bezier {
                    anchors: vec![(press, handles)],
                    tips: vec![tip],
                    placing: None,
                    redo: vec![],
                    last_press_placed: true,
                    close_hover: CloseHover::default(),
                });
            }
        }
    }

    pub(crate) fn bezier_anchor_move(&mut self, press: Pos2, world: Pos2, alt: bool) {
        let out = world - press;
        let handle_in = if alt {
            self.board_path_draft
                .as_ref()
                .and_then(|d| match d {
                    BoardPathDraft::Bezier { placing, .. } => placing.map(|(_, h)| h.handle_in),
                    _ => None,
                })
                .unwrap_or(Vec2::ZERO)
        } else {
            -out
        };
        let handles = BezierHandles {
            handle_out: out,
            handle_in,
        };
        match &mut self.board_path_draft {
            Some(BoardPathDraft::Bezier { placing, .. }) => {
                *placing = Some((press, handles));
            }
            _ => {
                self.board_path_draft = Some(BoardPathDraft::Bezier {
                    anchors: vec![],
                    tips: vec![],
                    placing: Some((press, handles)),
                    redo: vec![],
                    last_press_placed: true,
                    close_hover: CloseHover::default(),
                });
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn finish_freehand_pen(&mut self, points: Vec<Pos2>) {
        self.finish_freehand_pen_widths(points, &[]);
    }

    /// `widths` holds the Pen width at each point, stepping where it
    /// changed; each step is a fit break at its last narrow sample.
    #[cfg(test)]
    pub(crate) fn finish_freehand_pen_widths(&mut self, points: Vec<Pos2>, widths: &[f32]) {
        let base = self.placed_tip(StrokeTool::Pen);
        let tips: Vec<PlacedTip> = (0..points.len())
            .map(|i| PlacedTip {
                width: widths.get(i).copied().unwrap_or(base.width),
                ..base
            })
            .collect();
        if let Some(stroke) = FreehandTips::stepped(points, tips) {
            self.finish_freehand_pen_stroke(&stroke);
        }
    }

    /// Commit a Pen stroke: one fit, split at its tip blends, one tip per
    /// fitted vertex as the stroke drew it (P1.curve.tip-chord).
    pub(crate) fn finish_freehand_pen_stroke(&mut self, stroke: &FreehandTips<PlacedTip>) {
        let zoom = self.tab().cam.z.max(f32::EPSILON);
        let tol = FREEHAND_FIT_ERROR_PX / zoom;
        let spacing = FREEHAND_SAMPLE_SPACING_PX / zoom;
        let Some((bez, tips)) = stroke.fit(tol, spacing) else {
            return;
        };
        let (rect, data) = bezpath_to_path_data(&bez, false);
        if data.is_empty() {
            return;
        }
        let tips = DrawnTips::Vertices(&tips);
        let ids = self.commit_drawn_path(StrokeTool::Pen, rect, data, false, tips);
        if let (Some(&id), Some(&end)) = (ids.first(), stroke.points.last()) {
            self.set_pen_anchor(end, Some(id));
        }
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

    /// A Shift segment stamped by a worker over a copy of the canvas pixels
    /// under it gives the pixels stamping the whole canvas would, so the
    /// canvas that stands in after release shows the committed segment.
    #[test]
    fn a_line_raster_matches_stamping_the_whole_canvas() {
        let tip = StampStyle {
            diameter: 90.0,
            softness: 0.09,
            rgba: [200, 40, 40, 180],
            grain: vector_ink::Grain::Pencil,
        };
        let at = |x: f32, y: f32| TipPoint { pos: [x, y], tip };
        let mut full = vector_ink::StampImage {
            width: 640,
            height: 480,
            origin: [123.37, -45.11],
            pixel: 0.43,
            rgba: vec![0; 640 * 480 * 4],
            depth: Vec::new(),
            side: Default::default(),
        };
        stamp_segment(&mut full, at(150.0, 0.0), at(250.0, 80.0));
        let (a, b) = (at(250.0, 80.0), at(380.0, 100.0));
        let bx = segment_box(&full, a, b).expect("on the canvas");
        let job = LineJob {
            tag: 1,
            lane: tiles::BRUSH_LANE,
            base: copy_region(&full, bx, Default::default()),
            segs: vec![(a, b)],
            cut: None,
            panic: false,
        };
        let r = line_raster(job);
        stamp_segment(&mut full, a, b);
        let whole = vector_ink::finished_region(&full, tip.grain, bx);
        let raw = copy_region(&full, bx, Default::default());
        let finished = r.image.as_raw();
        let worst = |x: &[u8], y: &[u8]| x.iter().zip(y).map(|(p, q)| p.abs_diff(*q)).max().unwrap_or(0);
        assert_eq!(r.raw.rgba.len(), raw.rgba.len());
        assert!(worst(&r.raw.rgba, &raw.rgba) <= 1, "raw coverage differs");
        assert_eq!(r.raw.depth.len(), raw.depth.len());
        assert!(worst(&r.raw.depth, &raw.depth) <= 1, "depth differs");
        let expect = premultiplied(&whole);
        assert!(worst(finished, &expect) <= 1, "finished pixels differ");
    }

    /// Line jobs outlive the live canvas that asked for them. When the
    /// canvas is replaced (the board resized) with a slow job on the
    /// workers, the new canvas asks and takes its own quick job, and the
    /// old job lands after that, its raster is dropped: the new canvas
    /// never adopts it, and nothing waits in the pool. A raster of another
    /// box's size is never written in.
    #[test]
    fn a_replaced_live_canvas_drops_the_old_canvas_line_job() {
        let ctx = egui::Context::default();
        let mut tiles = tiles::BrushTiles::default();
        let mut slot: Option<BrushLiveCanvas> = None;
        let tip = |diameter: f32| StampStyle {
            diameter,
            softness: 0.5,
            rgba: [200, 40, 40, 255],
            grain: vector_ink::Grain::Smooth,
        };
        let at = |x: f32, y: f32, d: f32| TipPoint {
            pos: [x, y],
            tip: tip(d),
        };
        let xf = BoardXf {
            center: Pos2::new(400.0, 300.0),
            offset: Vec2::ZERO,
            z: 1.0,
        };
        let screen = |w: f32, h: f32| egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h));
        let frame = |slot: &mut Option<BrushLiveCanvas>,
                         tiles: &mut tiles::BrushTiles,
                         size: egui::Rect,
                         seg: Seg| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                if let Some(c) = slot.as_mut() {
                    c.take_landed(tiles);
                }
                let c = BrushLiveCanvas::ensure(slot, tiles, &painter, &xf, size, None, None, Vec::new);
                c.set_line(seg.0, seg.1);
                c.pump(tiles, ctx);
            });
        };
        // Canvas A's job covers most of the canvas with a wide soft tip;
        // canvas B's is a few pixels, so it lands and is taken first.
        let long = (at(-350.0, -250.0, 300.0), at(350.0, 250.0, 300.0));
        let short = (at(-10.0, 0.0, 4.0), at(10.0, 0.0, 4.0));
        frame(&mut slot, &mut tiles, screen(800.0, 600.0), long);
        let a = slot.as_ref().unwrap().inflight.as_ref().map(|a| a.tag);
        assert!(a.is_some(), "canvas A asked");
        frame(&mut slot, &mut tiles, screen(700.0, 500.0), short);
        let b = slot.as_ref().unwrap().inflight.as_ref().map(|a| a.tag);
        assert!(b.is_some() && b != a, "canvas B asked with its own tag");
        for _ in 0..2000 {
            if slot.as_ref().unwrap().exact.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            frame(&mut slot, &mut tiles, screen(700.0, 500.0), short);
        }
        let c = slot.as_ref().unwrap();
        let e = c.exact.as_ref().expect("canvas B's stamp landed");
        assert_eq!(e.seg, short, "canvas B shows its own segment");
        assert_eq!(Some(e.bx), c.segment_box(short.0, short.1));
        assert!(fits_box(&e.raw, e.bx), "canvas B took a raster of another size");
        assert_eq!(tiles.lines_wanted_len(), 0, "a job is still awaited");
        // Canvas A's job lands in this window; its raster must not stay.
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            assert_eq!(tiles.lines_landed_len(), 0, "canvas A's raster waits in the pool");
        }

        // A raster of the wrong size is never written in.
        let b = slot.as_mut().unwrap();
        let before = b.exact.as_ref().map(|e| e.bx);
        let wrong = line_raster(LineJob {
            tag: 0,
            lane: tiles::BRUSH_LANE,
            base: copy_region(&b.img, [0, 0, 10, 10], Default::default()),
            segs: vec![short],
            cut: None,
            panic: false,
        });
        let ask = LineAsk {
            tag: 0,
            gen: b.gen,
            segs: vec![long],
            kind: AskKind::Preview,
            bx: b.segment_box(long.0, long.1).unwrap(),
        };
        b.land(ask, wrong);
        assert_eq!(b.exact.as_ref().map(|e| e.bx), before, "a wrong-sized raster showed");
    }

    /// An eraser segment cut by a worker out of the ink under it gives the
    /// pixels the frame-loop pass gave: the segment stamped into the whole
    /// stroke's mask, its grain applied, and the ink multiplied by it.
    #[test]
    fn an_eraser_line_raster_matches_cutting_the_whole_stroke() {
        let tip = StampStyle {
            diameter: 90.0,
            softness: 0.09,
            rgba: [0, 0, 0, 200],
            grain: vector_ink::Grain::Pencil,
        };
        let (w, h) = (640u32, 480u32);
        let (origin, pixel) = ([123.37, -45.11], 0.43);
        let ink: Vec<u8> = (0..w * h)
            .flat_map(|i| [30, 90, 200, (i % 251) as u8])
            .collect();
        let mut mask = vector_ink::StampImage {
            width: w,
            height: h,
            origin,
            pixel,
            rgba: vec![0; (w * h * 4) as usize],
            depth: Vec::new(),
            side: vector_ink::StampSide::for_mask(),
        };
        let at = |x: f32, y: f32| TipPoint { pos: [x, y], tip };
        let (a, b) = (at(180.0, 20.0), at(360.0, 110.0));
        let bx = segment_box(&mask, a, b).expect("on the stroke");
        let job = LineJob {
            tag: 1,
            lane: tiles::erase_lane(NodeId(7)),
            base: vector_ink::StampImage {
                width: bx[2] - bx[0],
                height: bx[3] - bx[1],
                origin: [
                    origin[0] + bx[0] as f32 * pixel,
                    origin[1] + bx[1] as f32 * pixel,
                ],
                pixel,
                rgba: Vec::new(),
                depth: Vec::new(),
                side: vector_ink::StampSide::for_mask(),
            },
            segs: vec![(a, b)],
            cut: Some(InkCut {
                ink: Shared::new(ink.clone()),
                width: w,
                bx,
            }),
            panic: false,
        };
        let r = line_raster(job);
        stamp_segment(&mut mask, a, b);
        let grained = vector_ink::finished_region(&mask, tip.grain, [0, 0, w, h]);
        let mut shown = ink.clone();
        vector_ink::multiply_by_mask(&mut shown, &grained, w, Some(bx));
        let whole = copy_region(
            &vector_ink::StampImage {
                rgba: shown,
                ..mask.clone()
            },
            bx,
            Default::default(),
        );
        let worst = |x: &[u8], y: &[u8]| {
            x.iter()
                .zip(y)
                .map(|(p, q)| p.abs_diff(*q))
                .max()
                .unwrap_or(0)
        };
        assert_eq!(r.shown.len(), whole.rgba.len());
        assert!(worst(&r.shown, &whole.rgba) <= 1, "cut pixels differ");
        assert!(r.changed, "the cut met ink");
        assert!(
            worst(r.image.as_raw(), &premultiplied(&whole.rgba)) <= 1,
            "upload pixels differ"
        );
    }

    /// Two strokes that differ only in one tip's texture, or one erase mark
    /// tip's texture, have different stamp keys.
    #[test]
    fn a_tip_texture_changes_the_stamp_key() {
        let mut scene = slate_doc::scene::Scene::default();
        let (rect, mut path) = points_to_path_data(&[Pos2::ZERO, Pos2::new(80.0, 20.0)], false);
        let stroke = Stroke {
            stamp: true,
            ..default_curve_stroke(Rgba::BLACK)
        };
        let span = StrokeSpan::of(&stroke);
        path.tips = vec![span, span];
        path.erase = vec![slate_doc::scene::EraseMark {
            points: vec![[10.0, 5.0], [30.0, 5.0]],
            tips: vec![span, span],
        }];
        let node = |path: PathData, scene: &mut slate_doc::scene::Scene| {
            scene.build_node(
                rect,
                NodeKind::Shape(ShapeNode {
                    shape: ShapeKind::Path,
                    fill: None,
                    stroke: stroke.clone(),
                    corner: slate_doc::scene::Corner::Square,
                    sides: slate_doc::scene::default_regular_sides(),
                    phase_deg: 0.0,
                    flip: false,
                    path: Some(path.into()),
                    text: None,
                }),
            )
        };
        let key = |n: &Node| node_stamp_key(n).expect("a stamped path");
        let plain = key(&node(path.clone(), &mut scene));
        let mut tipped = path.clone();
        tipped.tips[1].texture = slate_doc::scene::BrushTexture::Pencil;
        assert_ne!(key(&node(tipped, &mut scene)), plain, "path tip texture");
        let mut erased = path;
        erased.erase[0].tips[0].texture = slate_doc::scene::BrushTexture::Pencil;
        assert_ne!(key(&node(erased, &mut scene)), plain, "erase mark tip texture");
    }

    #[test]
    fn curve_error_stays_subpixel_and_fill_cache_refines_with_zoom() {
        for zoom in [0.05, 0.25, 1.0, 8.0, 64.0] {
            assert!(curve_tolerance(zoom) * zoom as f64 <= 0.15);
        }
        let (rect, path) = points_to_path_data(&[Pos2::ZERO, Pos2::new(30.0, 40.0)], true);
        let corner = slate_doc::scene::Corner::Square;
        assert_ne!(
            path_fill_hash(&path, rect, 0.0, corner, zoom_bucket(1.0)),
            path_fill_hash(&path, rect, 0.0, corner, zoom_bucket(8.0))
        );
    }

    /// The board paints a pen stroke's per-vertex tips as varying width,
    /// the same widths the artifact writer outlines.
    #[test]
    fn board_paints_tipped_vector_strokes_at_their_vertex_widths() {
        let (rect, mut path) = points_to_path_data(
            &[
                Pos2::new(0.0, 0.0),
                Pos2::new(50.0, 0.0),
                Pos2::new(100.0, 0.0),
            ],
            false,
        );
        let color = Rgba::BLACK;
        let tip = |width| StrokeSpan {
            width,
            softness: 0.0,
            color,
            texture: Default::default(),
        };
        path.tips = vec![tip(2.0), tip(2.0), tip(10.0)];
        let shape = ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: Stroke {
                width: 10.0,
                color,
                cap: StrokeCap::Butt,
                ..Default::default()
            },
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(path.clone())),
            text: None,
        };
        let mut scene = slate_doc::scene::Scene::default();
        let node = scene.build_node(rect, NodeKind::Shape(shape.clone()));
        let ink = vector_stroke_ink(&node, &shape, &path, 1.0);
        let verts: Vec<[f32; 2]> = ink.vertices.iter().map(|v| v.pos).collect();
        let at = |x: f32, y: f32| {
            vector_ink::point_in_mesh(&verts, &ink.indices, [rect.x + x, rect.y + y])
        };
        let mid_y = 0.0 - rect.y;
        assert!(at(25.0, mid_y + 0.5));
        assert!(!at(25.0, mid_y + 3.0), "first span stays narrow");
        assert!(at(98.0, mid_y + 4.5), "the rest widens");
    }

    /// The Pen's live pieces cover its stroke as the one mesh does: the
    /// same area, so no gap or overlap where they meet, and a stroke too
    /// short to settle a piece is exactly that one mesh.
    #[test]
    fn pen_preview_pieces_tile_the_one_mesh_stroke() {
        let pts: Vec<Pos2> = (0..300)
            .map(|i| Pos2::new(i as f32 * 2.0, (i as f32 * 0.07).sin() * 30.0))
            .collect();
        let tips: Vec<PlacedTip> = (0..300)
            .map(|i| PlacedTip {
                width: 4.0 + (i % 50) as f32 * 0.2,
                color: Rgba([(i % 256) as u8, 40, 200, 255]),
                opacity: 0.6,
            })
            .collect();
        let cursor = (Pos2::new(610.0, 0.0), tips[299]);
        let area = |ink: &InkMesh| -> f64 {
            ink.indices
                .chunks(3)
                .map(|t| {
                    let [a, b, c] = [0, 1, 2].map(|k| ink.vertices[t[k] as usize].pos);
                    let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
                    (cross as f64).abs() * 0.5
                })
                .sum()
        };
        let tip = |i: usize| tips[i];
        let settled = (pts.len() - 2) / PEN_PIECE;
        assert!(settled >= 4);
        let tiled: f64 = (0..=settled)
            .map(|j| {
                let (bez, t, ends) = pen_piece(&pts, tip, cursor, j, j < settled);
                area(&draft_stroke_ink_ends(&bez, false, &t, 1.0, ends, 0.0).0)
            })
            .sum();
        let whole = area(&pen_preview_ink(&pts, &tips, cursor.0, cursor.1, 1.0).0);
        assert!(
            (tiled - whole).abs() < whole * 1e-3,
            "pieces {tiled} vs one mesh {whole}"
        );

        let short = &pts[..40];
        let (bez, t, ends) = pen_piece(short, tip, cursor, 0, false);
        let (a, _) = draft_stroke_ink_ends(&bez, false, &t, 1.0, ends, 0.0);
        let (b, _) = pen_preview_ink(short, &tips[..40], cursor.0, cursor.1, 1.0);
        let verts = |m: &InkMesh| -> Vec<([f32; 2], f32)> {
            m.vertices.iter().map(|v| (v.pos, v.alpha)).collect()
        };
        assert_eq!(verts(&a), verts(&b));
        assert_eq!((a.indices, a.colors), (b.indices, b.colors));
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

    fn min_dist_to_polyline(p: Pos2, flat: &[[f32; 2]]) -> f32 {
        let mut best = f32::MAX;
        for w in flat.windows(2) {
            let (a, b) = (w[0], w[1]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let len_sq = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if len_sq < 1e-8 {
                0.0
            } else {
                ((p.x - a[0]) * ab[0] + (p.y - a[1]) * ab[1]) / len_sq
            }
            .clamp(0.0, 1.0);
            let q = [a[0] + t * ab[0], a[1] + t * ab[1]];
            let dx = p.x - q[0];
            let dy = p.y - q[1];
            best = best.min((dx * dx + dy * dy).sqrt());
        }
        best
    }

    #[test]
    fn arc_through_mid_on_both_sides_of_chord() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(100.0, 0.0);
        for y in [40.0_f32, -40.0, 4.0, -4.0, 200.0] {
            let mid = Pos2::new(50.0, y);
            let bez = arc_through_three_points(start, mid, end);
            let flat = flatten(&bez, 0.05);
            let d = min_dist_to_polyline(mid, &flat);
            assert!(d < 1.5, "through-point should lie on arc (y={y}, d={d})");
            let first = flat.first().copied().unwrap();
            let last = flat.last().copied().unwrap();
            assert!((first[0] - start.x).abs() < 0.5);
            assert!((last[0] - end.x).abs() < 0.5);
        }
    }

    #[test]
    fn arc_collinear_through_point_is_straight() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(100.0, 0.0);
        let mid = Pos2::new(50.0, 0.0);
        let bez = arc_through_three_points(start, mid, end);
        assert_eq!(bez.elements().len(), 2);
        let flat = flatten(&bez, 0.05);
        for f in &flat {
            assert!(f[1].abs() < 0.01);
        }
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
    fn bezier_drag_threshold_is_screen_px() {
        for zoom in [0.25_f32, 1.0, 4.0, 16.0] {
            let thresh = DRAFT_DRAG_THRESHOLD_PX / zoom;
            let below = Vec2::new(thresh * 0.99, 0.0);
            let above = Vec2::new(thresh * 1.01, 0.0);
            assert!(below.length_sq() <= thresh.powi(2));
            assert!(above.length_sq() > thresh.powi(2));
        }
    }

    #[test]
    fn bezier_symmetric_and_corner_handles() {
        let a = Pos2::new(0.0, 0.0);
        let b = Pos2::new(100.0, 0.0);
        let out = Vec2::new(20.0, 30.0);
        let smooth = BezierHandles {
            handle_out: out,
            handle_in: -out,
        };
        let corner = BezierHandles {
            handle_out: out,
            handle_in: Vec2::ZERO,
        };
        let bez = bezier_anchors_to_bezpath(&[(a, smooth), (b, corner)]);
        assert!(bez.elements().len() >= 2);
        assert!(matches!(
            bez.elements().last(),
            Some(PathEl::CurveTo(_, _, _))
        ));
    }

    #[test]
    fn bezier_rubber_band_needs_one_anchor_and_cursor() {
        let anchors = vec![(Pos2::ZERO, BezierHandles::default())];
        let mut bez = BezPath::new();
        let last = anchors[0].0;
        let c = Pos2::new(40.0, 10.0);
        bez.move_to(to_k(last));
        bez.curve_to(to_k(last), to_k(c), to_k(c));
        assert!(bez.elements().len() >= 2);
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
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),

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
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),

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
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),
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

    /// A one-click brush dab: a single vertex stamped at 20 world units.
    fn brush_dab(at: Pos2) -> Node {
        let (rect, data) = points_to_path_data(&[at], false);
        let mut stroke = default_curve_stroke(Rgba::BLACK);
        stroke.width = 20.0;
        stroke.stamp = true;
        stroke.softness = 0.5;
        Node {
            id: NodeId(9),
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
                stroke,
                corner: slate_doc::scene::Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),
                text: None,
            }),
        }
    }

    #[test]
    fn a_brush_dab_answers_both_sweep_directions() {
        let node = brush_dab(Pos2::new(100.0, 100.0));
        let scene = slate_doc::scene::Scene::default();
        let routing = slate_doc::WireRouting::Bezier;
        let sweep = |r: WorldRect, mode| marquee_selects_node(&node, r, 1.0, &scene, routing, mode);
        // A box around the whole dab, well off its center.
        let around = WorldRect::new(60.0, 70.0, 70.0, 50.0);
        assert!(
            sweep(around, MarqueeMode::Window),
            "window around a dab selects it"
        );
        assert!(
            sweep(around, MarqueeMode::Crossing),
            "crossing around a dab selects it"
        );
        // A box over the dab's ink but not its center: crossing only.
        let rim = WorldRect::new(106.0, 90.0, 30.0, 20.0);
        assert!(
            sweep(rim, MarqueeMode::Crossing),
            "crossing the ink selects"
        );
        assert!(
            !sweep(rim, MarqueeMode::Window),
            "window needs the whole dab"
        );
        // Around the center point only, cutting the ink: not a window hit.
        let tight = WorldRect::new(96.0, 96.0, 8.0, 8.0);
        assert!(!sweep(tight, MarqueeMode::Window));
        assert!(sweep(tight, MarqueeMode::Crossing));
        let away = WorldRect::new(130.0, 130.0, 30.0, 30.0);
        assert!(!sweep(away, MarqueeMode::Window));
        assert!(!sweep(away, MarqueeMode::Crossing));
    }

    #[test]
    fn a_brush_dab_picks_on_its_ink() {
        let mut scene = slate_doc::scene::Scene::default();
        scene.nodes.push(brush_dab(Pos2::new(100.0, 100.0)));
        for (x, y) in [
            (100.0, 100.0),
            (108.0, 100.0),
            (100.0, 91.0),
            (106.0, 106.0),
        ] {
            assert_eq!(
                board_pick_node(&scene, x, y, 1.0),
                Some(NodeId(9)),
                "click at {x},{y}"
            );
        }
        assert_eq!(board_pick_node(&scene, 120.0, 100.0, 1.0), None);
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
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),

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
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),

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
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
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
        let corner = slate_doc::scene::Corner::Square;
        let a = path_content_hash(&path, &stroke, rect, 0.0, corner, 8);
        let b = path_content_hash(&path, &stroke, rect, 0.0, corner, 8);
        let c = path_content_hash(&path, &stroke, rect, 0.0, corner, 9);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn polyline_mesh_keys_tell_fillet_from_chamfer() {
        use slate_doc::scene::Corner;
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            closed: false,
            ..Default::default()
        };
        let stroke = default_curve_stroke(Rgba::BLACK);
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let fillet = Corner::Rounded { radius: 10.0 };
        let chamfer = Corner::Chamfer { cut: 10.0 };
        assert_ne!(
            path_content_hash(&path, &stroke, rect, 0.0, fillet, 8),
            path_content_hash(&path, &stroke, rect, 0.0, chamfer, 8),
            "switching to Chamfer must not reuse the filleted stroke mesh"
        );
        assert_ne!(
            path_fill_hash(&path, rect, 0.0, fillet, 8),
            path_fill_hash(&path, rect, 0.0, chamfer, 8)
        );
    }

    #[test]
    fn polyline_mesh_keys_tell_vertex_corner_overrides_apart() {
        use slate_doc::scene::Corner;
        let plain = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            closed: false,
            ..Default::default()
        };
        let mut overridden = plain.clone();
        overridden.corner_amounts = vec![None, Some(20.0), None];
        let stroke = default_curve_stroke(Rgba::BLACK);
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let corner = Corner::Rounded { radius: 10.0 };
        assert_ne!(
            path_content_hash(&plain, &stroke, rect, 0.0, corner, 8),
            path_content_hash(&overridden, &stroke, rect, 0.0, corner, 8),
            "a vertex override must not reuse the uniform stroke mesh"
        );
        assert_ne!(
            path_fill_hash(&plain, rect, 0.0, corner, 8),
            path_fill_hash(&overridden, rect, 0.0, corner, 8)
        );
    }

    #[test]
    fn path_mesh_cache_evicts_oldest_instead_of_clearing() {
        let mut cache = PathMeshCache {
            entry_limit: 256,
            ..Default::default()
        };
        let empty = InkMesh::default;
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
            colors: Vec::new(),
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
            colors: Vec::new(),
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
            let key = path_content_hash(
                &path,
                &stroke,
                rect,
                rotation,
                slate_doc::scene::Corner::Square,
                zoom_bucket(1.0),
            );
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
