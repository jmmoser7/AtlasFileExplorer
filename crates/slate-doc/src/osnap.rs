//! Object-snap kinds, applicability, and discrete anchors.
//!
//! This module is the board's snap *syntax*: every [`SnapKind`] declares the
//! geometric [`SnapFacet`]s it needs, every node advertises the facets it
//! actually has, and [`SnapKind::accepts`] is the single reject/apply gate.
//! Evaluation of continuous snaps (Near on a path, Intersection) lives in
//! the app, which already holds kurbo paths — the decision of *whether* a
//! kind may fire on a node is made here, with no renderer in sight
//! (Constitution Art. I).
//!
//! The shipped set is the Rhino persistent simple snaps that survive the
//! 10% rule on a 2D board (Art. III). Construction / 3D / NURBS snaps are
//! listed as [`RejectedSnap`] so a future kind is an explicit amendment,
//! not an omission — they are **not** Document Settings rows. A Rhino view
//! portal owns those as portal-local UI (`P1.portal.local-ui`).

use serde::{Deserialize, Serialize};

use crate::scene::{Node, NodeKind, PathData, PathSeg, ShapeKind, WorldRect};

/// Geometric capabilities a node can advertise to the snap engine.
///
/// A snap kind fires on a node only when [`SnapKind::accepts`] finds the
/// required facets (and, for Tan/Perp, a prior point). New node kinds
/// declare facets; they do not special-case snap kinds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapFacet(u32);

impl SnapFacet {
    pub const NONE: Self = Self(0);
    /// Distinct endpoints, corners, or polyline vertices.
    pub const ENDPOINTS: Self = Self(1 << 0);
    /// Finite edges or path segments that have a midpoint.
    pub const SEGMENTS: Self = Self(1 << 1);
    /// A well-defined centroid (closed shape, text/image/frame bbox).
    pub const CENTER: Self = Self(1 << 2);
    /// A boundary or stroke the cursor can land on (Near).
    pub const BOUNDARY: Self = Self(1 << 3);
    /// Differentiable curve: a unique tangent at almost every point.
    pub const SMOOTH: Self = Self(1 << 4);
    /// Circle / ellipse / circular arc — analytic quadrants and tangents.
    pub const CIRCLE_LIKE: Self = Self(1 << 5);
    /// Rectangular outline (four sides), axis-aligned or rotated.
    pub const RECT_OUTLINE: Self = Self(1 << 6);
    /// Open curve (line, polyline, bezier, connector).
    pub const OPEN_CURVE: Self = Self(1 << 7);
    /// Has edge/stroke geometry that can intersect another.
    pub const INTERSECTABLE: Self = Self(1 << 8);

    pub const fn empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Why a snap kind refuses a node (or a pick).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapReject {
    /// Kind is not in the 2D board set (Knot, OnSrf, …).
    NotShipped,
    /// Node advertises none of the facets this kind requires.
    MissingFacet,
    /// Tan / Perp need a prior point (Rhino: not effective for the first pick).
    NeedsFromPoint,
    /// Hidden, or the node is not a snap target.
    NotATarget,
}

/// Persistent object-snap kinds shipped on the 2D board.
///
/// Order is display order in the Document Settings palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapKind {
    End,
    Mid,
    Center,
    Near,
    Intersection,
    Quadrant,
    Perpendicular,
    Tangent,
}

impl SnapKind {
    pub const ALL: [SnapKind; 8] = [
        SnapKind::End,
        SnapKind::Mid,
        SnapKind::Center,
        SnapKind::Near,
        SnapKind::Intersection,
        SnapKind::Quadrant,
        SnapKind::Perpendicular,
        SnapKind::Tangent,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SnapKind::End => "End",
            SnapKind::Mid => "Mid",
            SnapKind::Center => "Center",
            SnapKind::Near => "Near",
            SnapKind::Intersection => "Intersection",
            SnapKind::Quadrant => "Quadrant",
            SnapKind::Perpendicular => "Perpendicular",
            SnapKind::Tangent => "Tangent",
        }
    }

    /// Short Rhino-style token shown on the live marker.
    pub fn token(self) -> &'static str {
        match self {
            SnapKind::End => "End",
            SnapKind::Mid => "Mid",
            SnapKind::Center => "Cen",
            SnapKind::Near => "Near",
            SnapKind::Intersection => "Int",
            SnapKind::Quadrant => "Quad",
            SnapKind::Perpendicular => "Perp",
            SnapKind::Tangent => "Tan",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            SnapKind::End => {
                "Endpoints, corners, and polyline vertices. Every node with corners or an open path."
            }
            SnapKind::Mid => {
                "Midpoint of an edge or path segment. Rects, frames, images, text, and paths."
            }
            SnapKind::Center => {
                "Centroid of a closed shape or the bounding box of text, images, frames, and portals."
            }
            SnapKind::Near => "Nearest point on a boundary or stroke. Any node with an outline.",
            SnapKind::Intersection => {
                "Crossing of two edges or strokes. Nodes without stroke/edge geometry are rejected."
            }
            SnapKind::Quadrant => {
                "Axis extremes of a circle or ellipse. Rejected on rectangles, polygons, and text."
            }
            SnapKind::Perpendicular => {
                "Foot of the perpendicular from the last placed point onto a boundary. Inactive on the first pick."
            }
            SnapKind::Tangent => {
                "Tangent from the last placed point onto a circle or ellipse. Rejected on polygons, rects, text, and open polylines. Bezier tangent is deferred. Inactive on the first pick."
            }
        }
    }

    /// Stronger kinds win when two candidates sit inside the snap radius.
    /// Rhino: End beats Near. Lower number = stronger.
    pub fn priority(self) -> u8 {
        match self {
            SnapKind::End => 0,
            SnapKind::Intersection => 1,
            SnapKind::Center => 2,
            SnapKind::Mid => 3,
            SnapKind::Quadrant => 4,
            SnapKind::Perpendicular => 5,
            SnapKind::Tangent => 6,
            SnapKind::Near => 7,
        }
    }

    pub fn needs_from_point(self) -> bool {
        matches!(self, SnapKind::Perpendicular | SnapKind::Tangent)
    }

    /// Facets a node must advertise. Tangent accepts SMOOTH *or* CIRCLE_LIKE.
    pub fn required_facets(self) -> SnapFacet {
        match self {
            SnapKind::End => SnapFacet::ENDPOINTS,
            SnapKind::Mid => SnapFacet::SEGMENTS,
            SnapKind::Center => SnapFacet::CENTER,
            SnapKind::Near => SnapFacet::BOUNDARY,
            SnapKind::Intersection => SnapFacet::INTERSECTABLE,
            SnapKind::Quadrant => SnapFacet::CIRCLE_LIKE,
            SnapKind::Perpendicular => SnapFacet::BOUNDARY,
            // Analytic tangents ship for CIRCLE_LIKE only. SMOOTH (beziers)
            // is advertised on paths for a future evaluator; Tangent rejects
            // it until that lands (Art. III).
            SnapKind::Tangent => SnapFacet::CIRCLE_LIKE,
        }
    }

    /// The single apply/reject gate. `from` is the last placed point of the
    /// current gesture (`None` on the first pick).
    pub fn accepts(self, facets: SnapFacet, from: Option<[f32; 2]>) -> Result<(), SnapReject> {
        if facets.empty() {
            return Err(SnapReject::NotATarget);
        }
        if self.needs_from_point() && from.is_none() {
            return Err(SnapReject::NeedsFromPoint);
        }
        if facets.contains(self.required_facets()) {
            Ok(())
        } else {
            Err(SnapReject::MissingFacet)
        }
    }
}

/// Rhino snaps that are not board-wide (Art. III / `P1.portal.local-ui`).
///
/// 3D, NURBS, and construction snaps belong on a Rhino view portal's own
/// settings (Selection inspector / Set portal), never on Document Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectedSnap {
    Knot,
    Point,
    Vertex,
    Project,
    Along,
    AlongParallel,
    Between,
    From,
    PerpFrom,
    TanFrom,
    OnCurve,
    OnSurface,
    OnPolysurface,
    OnMesh,
    Percentage,
}

impl RejectedSnap {
    pub const ALL: [RejectedSnap; 15] = [
        RejectedSnap::Knot,
        RejectedSnap::Point,
        RejectedSnap::Vertex,
        RejectedSnap::Project,
        RejectedSnap::Along,
        RejectedSnap::AlongParallel,
        RejectedSnap::Between,
        RejectedSnap::From,
        RejectedSnap::PerpFrom,
        RejectedSnap::TanFrom,
        RejectedSnap::OnCurve,
        RejectedSnap::OnSurface,
        RejectedSnap::OnPolysurface,
        RejectedSnap::OnMesh,
        RejectedSnap::Percentage,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RejectedSnap::Knot => "Knot",
            RejectedSnap::Point => "Point",
            RejectedSnap::Vertex => "Vertex",
            RejectedSnap::Project => "Project",
            RejectedSnap::Along => "Along",
            RejectedSnap::AlongParallel => "Along parallel",
            RejectedSnap::Between => "Between",
            RejectedSnap::From => "From",
            RejectedSnap::PerpFrom => "PerpFrom",
            RejectedSnap::TanFrom => "TanFrom",
            RejectedSnap::OnCurve => "OnCurve",
            RejectedSnap::OnSurface => "OnSurface",
            RejectedSnap::OnPolysurface => "OnPolysurface",
            RejectedSnap::OnMesh => "OnMesh",
            RejectedSnap::Percentage => "Percent",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            RejectedSnap::Knot => "NURBS knot — the board has no spline knot vector.",
            RejectedSnap::Point => "Point objects and CVs — the board has no point-node kind yet.",
            RejectedSnap::Vertex => "Mesh / SubD vertex — 3D-only.",
            RejectedSnap::Project => {
                "Project to construction plane — the board is already a 2D plane."
            }
            RejectedSnap::Along | RejectedSnap::AlongParallel => {
                "Multi-step tracking line — construction aid, not a persistent snap."
            }
            RejectedSnap::Between => "Two-pick midpoint — use Mid on a segment instead.",
            RejectedSnap::From => "Base-point for typed distance — numeric entry is per-tool.",
            RejectedSnap::PerpFrom | RejectedSnap::TanFrom => {
                "Tracking along a constructed perp/tan — multi-step, not a persistent snap."
            }
            RejectedSnap::OnCurve => "Lock onto one chosen curve — Near covers the 2D case.",
            RejectedSnap::OnSurface | RejectedSnap::OnPolysurface => {
                "Track over a surface — 3D-only."
            }
            RejectedSnap::OnMesh => "Track over a mesh — 3D-only.",
            RejectedSnap::Percentage => "Percent-along-curve markers — not a board need.",
        }
    }
}

/// Persistent object-snap set (Document Settings + `slate-settings.json`).
///
/// `enabled` is Rhino's Disable inverted: when false the remembered kinds
/// stay checked but none fire. Defaults match the previous hardcoded
/// line-tool osnap (End + Mid) plus Center, the other Rhino staple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjectSnapSet {
    pub enabled: bool,
    pub end: bool,
    pub mid: bool,
    pub center: bool,
    pub near: bool,
    pub intersection: bool,
    pub quadrant: bool,
    pub perpendicular: bool,
    pub tangent: bool,
}

impl Default for ObjectSnapSet {
    fn default() -> Self {
        Self {
            enabled: true,
            end: true,
            mid: true,
            center: true,
            near: false,
            intersection: false,
            quadrant: false,
            perpendicular: false,
            tangent: false,
        }
    }
}

impl ObjectSnapSet {
    pub fn is_on(self, kind: SnapKind) -> bool {
        if !self.enabled {
            return false;
        }
        self.is_kind_remembered(kind)
    }

    pub fn set(&mut self, kind: SnapKind, on: bool) {
        match kind {
            SnapKind::End => self.end = on,
            SnapKind::Mid => self.mid = on,
            SnapKind::Center => self.center = on,
            SnapKind::Near => self.near = on,
            SnapKind::Intersection => self.intersection = on,
            SnapKind::Quadrant => self.quadrant = on,
            SnapKind::Perpendicular => self.perpendicular = on,
            SnapKind::Tangent => self.tangent = on,
        }
    }

    pub fn toggle(&mut self, kind: SnapKind) {
        self.set(kind, !self.is_kind_remembered(kind));
    }

    /// The checkbox state, ignoring the master Disable.
    pub fn is_kind_remembered(self, kind: SnapKind) -> bool {
        match kind {
            SnapKind::End => self.end,
            SnapKind::Mid => self.mid,
            SnapKind::Center => self.center,
            SnapKind::Near => self.near,
            SnapKind::Intersection => self.intersection,
            SnapKind::Quadrant => self.quadrant,
            SnapKind::Perpendicular => self.perpendicular,
            SnapKind::Tangent => self.tangent,
        }
    }

    pub fn any_kind_on(self) -> bool {
        self.enabled
            && (self.end
                || self.mid
                || self.center
                || self.near
                || self.intersection
                || self.quadrant
                || self.perpendicular
                || self.tangent)
    }
}

const RECT_LIKE: SnapFacet = SnapFacet(
    SnapFacet::ENDPOINTS.0
        | SnapFacet::SEGMENTS.0
        | SnapFacet::CENTER.0
        | SnapFacet::BOUNDARY.0
        | SnapFacet::RECT_OUTLINE.0
        | SnapFacet::INTERSECTABLE.0,
);

/// Facets this node advertises. Hidden nodes advertise nothing (not a target).
/// Locked nodes keep their facets — Rhino snaps to locked geometry.
pub fn node_facets(node: &Node) -> SnapFacet {
    if node.hidden {
        return SnapFacet::NONE;
    }
    match &node.kind {
        NodeKind::Frame(_)
        | NodeKind::Image(_)
        | NodeKind::Text(_)
        | NodeKind::Portal(_)
        | NodeKind::DockStrip(_) => RECT_LIKE,
        NodeKind::Connector(_) => SnapFacet::ENDPOINTS
            .union(SnapFacet::SEGMENTS)
            .union(SnapFacet::BOUNDARY)
            .union(SnapFacet::OPEN_CURVE)
            .union(SnapFacet::SMOOTH),
        NodeKind::Shape(s) => shape_facets(s.shape, s.path.as_ref()),
    }
}

fn shape_facets(shape: ShapeKind, path: Option<&PathData>) -> SnapFacet {
    match shape {
        ShapeKind::Rect => RECT_LIKE,
        ShapeKind::Ellipse => SnapFacet::CENTER
            .union(SnapFacet::BOUNDARY)
            .union(SnapFacet::SMOOTH)
            .union(SnapFacet::CIRCLE_LIKE),
        ShapeKind::Line => SnapFacet::ENDPOINTS
            .union(SnapFacet::SEGMENTS)
            .union(SnapFacet::BOUNDARY)
            .union(SnapFacet::OPEN_CURVE)
            .union(SnapFacet::INTERSECTABLE),
        ShapeKind::Path => {
            let Some(path) = path else {
                return SnapFacet::NONE;
            };
            let mut f = SnapFacet::ENDPOINTS
                .union(SnapFacet::SEGMENTS)
                .union(SnapFacet::BOUNDARY);
            if path.closed {
                f = f.union(SnapFacet::CENTER);
            } else {
                f = f.union(SnapFacet::OPEN_CURVE);
            }
            if path.segs.iter().any(|s| matches!(s, PathSeg::Line { .. })) {
                f = f.union(SnapFacet::INTERSECTABLE);
            }
            if path
                .segs
                .iter()
                .any(|s| matches!(s, PathSeg::Quad { .. } | PathSeg::Cubic { .. }))
            {
                f = f.union(SnapFacet::SMOOTH);
            }
            f
        }
    }
}

/// A discrete snap location produced without tessellation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapAnchor {
    pub kind: SnapKind,
    pub point: [f32; 2],
}

/// End / Mid / Center / Quadrant anchors for a node. Continuous kinds
/// (Near, Tan, Perp, Int) are evaluated by the app against live curves.
pub fn discrete_anchors(node: &Node, from: Option<[f32; 2]>) -> Vec<SnapAnchor> {
    let facets = node_facets(node);
    if facets.empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    match &node.kind {
        NodeKind::Shape(s) if s.shape == ShapeKind::Ellipse => {
            push_ellipse_discrete(&mut out, node.rect, node.rotation_deg, facets, from);
        }
        NodeKind::Shape(s) if s.shape == ShapeKind::Line => {
            push_legacy_line(&mut out, node, s.flip, facets, from);
        }
        NodeKind::Shape(s) if s.shape == ShapeKind::Path => {
            if let Some(path) = s.path.as_ref() {
                push_path_discrete(&mut out, path, node.rect, node.rotation_deg, facets, from);
            }
        }
        NodeKind::Connector(c) => {
            if let crate::scene::ConnectorEnd::Free { point } = c.a {
                push_if(&mut out, SnapKind::End, facets, from, point);
            }
            if let crate::scene::ConnectorEnd::Free { point } = c.b {
                push_if(&mut out, SnapKind::End, facets, from, point);
            }
        }
        _ => push_rect_discrete(&mut out, node.rect, node.rotation_deg, facets, from),
    }
    out
}

fn push_if(
    out: &mut Vec<SnapAnchor>,
    kind: SnapKind,
    facets: SnapFacet,
    from: Option<[f32; 2]>,
    point: [f32; 2],
) {
    if kind.accepts(facets, from).is_ok() {
        out.push(SnapAnchor { kind, point });
    }
}

fn push_rect_discrete(
    out: &mut Vec<SnapAnchor>,
    rect: WorldRect,
    rotation_deg: f32,
    facets: SnapFacet,
    from: Option<[f32; 2]>,
) {
    let corners = rect.corners_rotated(rotation_deg);
    for p in corners {
        push_if(out, SnapKind::End, facets, from, [p.0, p.1]);
    }
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        push_if(
            out,
            SnapKind::Mid,
            facets,
            from,
            [(a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5],
        );
    }
    let (cx, cy) = rect.center();
    push_if(out, SnapKind::Center, facets, from, [cx, cy]);
}

fn push_ellipse_discrete(
    out: &mut Vec<SnapAnchor>,
    rect: WorldRect,
    rotation_deg: f32,
    facets: SnapFacet,
    from: Option<[f32; 2]>,
) {
    let (cx, cy) = rect.center();
    push_if(out, SnapKind::Center, facets, from, [cx, cy]);
    let local = [
        [cx, rect.y],
        [rect.x + rect.w, cy],
        [cx, rect.y + rect.h],
        [rect.x, cy],
    ];
    for p in local {
        let q = rotate_about(p, [cx, cy], rotation_deg);
        push_if(out, SnapKind::Quadrant, facets, from, q);
    }
}

fn push_legacy_line(
    out: &mut Vec<SnapAnchor>,
    node: &Node,
    flip: bool,
    facets: SnapFacet,
    from: Option<[f32; 2]>,
) {
    let r = node.rect;
    let (a, b) = if flip {
        ([r.x, r.y + r.h], [r.x + r.w, r.y])
    } else {
        ([r.x, r.y], [r.x + r.w, r.y + r.h])
    };
    let c = [r.center().0, r.center().1];
    let a = rotate_about(a, c, node.rotation_deg);
    let b = rotate_about(b, c, node.rotation_deg);
    push_if(out, SnapKind::End, facets, from, a);
    push_if(out, SnapKind::End, facets, from, b);
    push_if(
        out,
        SnapKind::Mid,
        facets,
        from,
        [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5],
    );
}

fn push_path_discrete(
    out: &mut Vec<SnapAnchor>,
    path: &PathData,
    rect: WorldRect,
    rotation_deg: f32,
    facets: SnapFacet,
    from: Option<[f32; 2]>,
) {
    let world = |p: [f32; 2]| {
        let x = rect.x + p[0] * rect.w;
        let y = rect.y + p[1] * rect.h;
        rotate_about([x, y], [rect.center().0, rect.center().1], rotation_deg)
    };
    let start = world(path.start);
    push_if(out, SnapKind::End, facets, from, start);
    let mut prev = path.start;
    let mut verts = vec![start];
    for seg in &path.segs {
        let to = match *seg {
            PathSeg::Line { to } => to,
            PathSeg::Quad { to, .. } => to,
            PathSeg::Cubic { to, .. } => to,
        };
        let w_to = world(to);
        push_if(out, SnapKind::End, facets, from, w_to);
        let mid = match *seg {
            PathSeg::Line { .. } => {
                let a = world(prev);
                [(a[0] + w_to[0]) * 0.5, (a[1] + w_to[1]) * 0.5]
            }
            PathSeg::Quad { ctrl, to } => world(quad_at(prev, ctrl, to, 0.5)),
            PathSeg::Cubic { c1, c2, to } => world(cubic_at(prev, c1, c2, to, 0.5)),
        };
        push_if(out, SnapKind::Mid, facets, from, mid);
        verts.push(w_to);
        prev = to;
    }
    if path.closed && !verts.is_empty() {
        let n = verts.len() as f32;
        let cx = verts.iter().map(|p| p[0]).sum::<f32>() / n;
        let cy = verts.iter().map(|p| p[1]).sum::<f32>() / n;
        push_if(out, SnapKind::Center, facets, from, [cx, cy]);
    }
}

fn rotate_about(p: [f32; 2], c: [f32; 2], deg: f32) -> [f32; 2] {
    if deg.abs() < f32::EPSILON {
        return p;
    }
    let rad = deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = p[0] - c[0];
    let dy = p[1] - c[1];
    [c[0] + dx * cos - dy * sin, c[1] + dx * sin + dy * cos]
}

fn quad_at(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0],
        u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1],
    ]
}

fn cubic_at(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        u * u * u * p0[0] + 3.0 * u * u * t * p1[0] + 3.0 * u * t * t * p2[0] + t * t * t * p3[0],
        u * u * u * p0[1] + 3.0 * u * u * t * p1[1] + 3.0 * u * t * t * p2[1] + t * t * t * p3[1],
    ]
}

/// Nearest point on a (possibly rotated) rectangle outline.
pub fn nearest_on_rect(rect: WorldRect, rotation_deg: f32, p: [f32; 2]) -> [f32; 2] {
    let corners = rect.corners_rotated(rotation_deg);
    let mut best = [corners[0].0, corners[0].1];
    let mut best_d = f32::INFINITY;
    for i in 0..4 {
        let a = [corners[i].0, corners[i].1];
        let b = [corners[(i + 1) % 4].0, corners[(i + 1) % 4].1];
        let q = nearest_on_segment(a, b, p);
        let d = dist2(q, p);
        if d < best_d {
            best_d = d;
            best = q;
        }
    }
    best
}

/// Foot of the perpendicular from `from` onto each rect edge, if it lands
/// on the segment. Used for Perp on RECT_OUTLINE nodes.
pub fn perp_on_rect(rect: WorldRect, rotation_deg: f32, from: [f32; 2]) -> Vec<[f32; 2]> {
    let corners = rect.corners_rotated(rotation_deg);
    let mut out = Vec::new();
    for i in 0..4 {
        let a = [corners[i].0, corners[i].1];
        let b = [corners[(i + 1) % 4].0, corners[(i + 1) % 4].1];
        let q = nearest_on_segment(a, b, from);
        if dist2(q, a) > 1e-4 && dist2(q, b) > 1e-4 {
            out.push(q);
        }
    }
    out
}

pub fn nearest_on_segment(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let vx = b[0] - a[0];
    let vy = b[1] - a[1];
    let len2 = vx * vx + vy * vy;
    if len2 < 1e-12 {
        return a;
    }
    let t = ((p[0] - a[0]) * vx + (p[1] - a[1]) * vy) / len2;
    let t = t.clamp(0.0, 1.0);
    [a[0] + vx * t, a[1] + vy * t]
}

/// Nearest point on a (possibly rotated) ellipse outline.
pub fn nearest_on_ellipse(rect: WorldRect, rotation_deg: f32, p: [f32; 2]) -> [f32; 2] {
    let (cx, cy) = rect.center();
    let rx = (rect.w * 0.5).max(1e-4);
    let ry = (rect.h * 0.5).max(1e-4);
    let local = rotate_about(p, [cx, cy], -rotation_deg);
    let mut theta = (local[1] - cy).atan2(local[0] - cx);
    for _ in 0..8 {
        let (s, c) = theta.sin_cos();
        let ex = rx * c;
        let ey = ry * s;
        let dx = (cx + ex) - local[0];
        let dy = (cy + ey) - local[1];
        let dtheta = -rx * s;
        let dphi = ry * c;
        let num = dx * dtheta + dy * dphi;
        let den = dtheta * dtheta + dphi * dphi + dx * (-rx * c) + dy * (-ry * s);
        if den.abs() < 1e-8 {
            break;
        }
        theta -= num / den;
    }
    let (s, c) = theta.sin_cos();
    rotate_about([cx + rx * c, cy + ry * s], [cx, cy], rotation_deg)
}

/// Tangent points from `from` onto a (possibly rotated) ellipse.
/// Empty when `from` is inside.
pub fn tangents_on_ellipse(rect: WorldRect, rotation_deg: f32, from: [f32; 2]) -> Vec<[f32; 2]> {
    let (cx, cy) = rect.center();
    let rx = (rect.w * 0.5).max(1e-4);
    let ry = (rect.h * 0.5).max(1e-4);
    let local = rotate_about(from, [cx, cy], -rotation_deg);
    let px = (local[0] - cx) / rx;
    let py = (local[1] - cy) / ry;
    let d2 = px * px + py * py;
    if d2 <= 1.0 + 1e-6 {
        return Vec::new();
    }
    let d = d2.sqrt();
    let base = py.atan2(px);
    let beta = (1.0 / d).acos();
    let mut out = Vec::new();
    for sign in [-1.0f32, 1.0] {
        let ang = base + sign * beta;
        let (s, c) = ang.sin_cos();
        out.push(rotate_about(
            [cx + rx * c, cy + ry * s],
            [cx, cy],
            rotation_deg,
        ));
    }
    out
}

/// Perpendicular feet from `from` onto an ellipse.
pub fn perp_on_ellipse(rect: WorldRect, rotation_deg: f32, from: [f32; 2]) -> Vec<[f32; 2]> {
    let (cx, cy) = rect.center();
    let local = rotate_about(from, [cx, cy], -rotation_deg);
    if (local[0] - cx).abs() < 1e-6 && (local[1] - cy).abs() < 1e-6 {
        return Vec::new();
    }
    let near = nearest_on_ellipse(rect, rotation_deg, from);
    let rx = (rect.w * 0.5).max(1e-4);
    let ry = (rect.h * 0.5).max(1e-4);
    let lx = (local[0] - cx) / rx;
    let ly = (local[1] - cy) / ry;
    let len = (lx * lx + ly * ly).sqrt().max(1e-6);
    let far = rotate_about(
        [cx - rx * lx / len, cy - ry * ly / len],
        [cx, cy],
        rotation_deg,
    );
    vec![near, far]
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

/// Segment–segment intersection point (proper crossing or touching).
pub fn segment_intersection(
    a0: [f32; 2],
    a1: [f32; 2],
    b0: [f32; 2],
    b1: [f32; 2],
) -> Option<[f32; 2]> {
    let (x1, y1, x2, y2) = (a0[0], a0[1], a1[0], a1[1]);
    let (x3, y3, x4, y4) = (b0[0], b0[1], b1[0], b1[1]);
    let den = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if den.abs() < 1e-8 {
        return None;
    }
    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / den;
    let u = ((x1 - x3) * (y1 - y2) - (y1 - y3) * (x1 - x2)) / den;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some([x1 + t * (x2 - x1), y1 + t * (y2 - y1)])
    } else {
        None
    }
}

/// Four edge segments of a (possibly rotated) rect, for Intersection.
pub fn rect_segments(rect: WorldRect, rotation_deg: f32) -> [([f32; 2], [f32; 2]); 4] {
    let c = rect.corners_rotated(rotation_deg);
    let p = |i: usize| [c[i].0, c[i].1];
    [(p(0), p(1)), (p(1), p(2)), (p(2), p(3)), (p(3), p(0))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Corner, ShapeNode};

    fn shape(kind: ShapeKind, rect: WorldRect) -> Node {
        Node {
            id: crate::NodeId(1),
            rect,
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: kind,
                fill: None,
                stroke: crate::scene::Stroke::default(),
                corner: Corner::Square,
                flip: false,
                path: None,
            }),
        }
    }

    #[test]
    fn rect_rejects_tangent_and_quadrant() {
        let n = shape(ShapeKind::Rect, WorldRect::new(0.0, 0.0, 80.0, 40.0));
        let f = node_facets(&n);
        assert_eq!(
            SnapKind::Tangent.accepts(f, Some([0.0, 0.0])),
            Err(SnapReject::MissingFacet)
        );
        assert_eq!(
            SnapKind::Quadrant.accepts(f, None),
            Err(SnapReject::MissingFacet)
        );
        assert!(SnapKind::End.accepts(f, None).is_ok());
        assert!(SnapKind::Mid.accepts(f, None).is_ok());
        assert!(SnapKind::Center.accepts(f, None).is_ok());
    }

    #[test]
    fn ellipse_accepts_tan_quad_rejects_end() {
        let n = shape(ShapeKind::Ellipse, WorldRect::new(0.0, 0.0, 80.0, 40.0));
        let f = node_facets(&n);
        assert!(SnapKind::Tangent.accepts(f, Some([200.0, 20.0])).is_ok());
        assert!(SnapKind::Quadrant.accepts(f, None).is_ok());
        assert_eq!(
            SnapKind::End.accepts(f, None),
            Err(SnapReject::MissingFacet)
        );
        assert_eq!(
            SnapKind::Intersection.accepts(f, None),
            Err(SnapReject::MissingFacet)
        );
        assert_eq!(
            SnapKind::Tangent.accepts(f, None),
            Err(SnapReject::NeedsFromPoint)
        );
    }

    #[test]
    fn hidden_node_is_not_a_target() {
        let mut n = shape(ShapeKind::Rect, WorldRect::new(0.0, 0.0, 10.0, 10.0));
        n.hidden = true;
        assert!(node_facets(&n).empty());
        assert!(discrete_anchors(&n, None).is_empty());
    }

    #[test]
    fn locked_node_still_advertises_facets() {
        let mut n = shape(ShapeKind::Rect, WorldRect::new(0.0, 0.0, 10.0, 10.0));
        n.locked = true;
        assert!(SnapKind::End.accepts(node_facets(&n), None).is_ok());
    }

    #[test]
    fn rect_discrete_has_four_ends_four_mids_one_center() {
        let n = shape(ShapeKind::Rect, WorldRect::new(10.0, 20.0, 40.0, 20.0));
        let a = discrete_anchors(&n, None);
        assert_eq!(a.iter().filter(|x| x.kind == SnapKind::End).count(), 4);
        assert_eq!(a.iter().filter(|x| x.kind == SnapKind::Mid).count(), 4);
        assert_eq!(a.iter().filter(|x| x.kind == SnapKind::Center).count(), 1);
    }

    #[test]
    fn tan_from_outside_circle_yields_two_points() {
        let r = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let pts = tangents_on_ellipse(r, 0.0, [200.0, 50.0]);
        assert_eq!(pts.len(), 2);
        for p in &pts {
            let dx = p[0] - 50.0;
            let dy = p[1] - 50.0;
            let rad = (dx * dx + dy * dy).sqrt();
            assert!(
                (rad - 50.0).abs() < 0.6,
                "tangent not on circle: {p:?} r={rad}"
            );
        }
    }

    #[test]
    fn tan_from_inside_ellipse_is_empty() {
        let r = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        assert!(tangents_on_ellipse(r, 0.0, [50.0, 25.0]).is_empty());
    }

    #[test]
    fn end_beats_near_in_priority() {
        assert!(SnapKind::End.priority() < SnapKind::Near.priority());
    }

    #[test]
    fn master_disable_keeps_remembered_kinds() {
        let mut set = ObjectSnapSet::default();
        assert!(set.is_on(SnapKind::End));
        set.enabled = false;
        assert!(!set.is_on(SnapKind::End));
        assert!(set.is_kind_remembered(SnapKind::End));
    }

    #[test]
    fn cubic_path_rejects_tangent_until_evaluator_ships() {
        let mut n = shape(ShapeKind::Path, WorldRect::new(0.0, 0.0, 10.0, 10.0));
        if let NodeKind::Shape(s) = &mut n.kind {
            s.path = Some(PathData {
                start: [0.0, 0.0],
                segs: vec![PathSeg::Cubic {
                    c1: [0.3, 1.0],
                    c2: [0.7, 1.0],
                    to: [1.0, 0.0],
                }],
                closed: false,
                ..Default::default()
            });
        }
        let f = node_facets(&n);
        assert!(f.contains(SnapFacet::SMOOTH));
        assert_eq!(
            SnapKind::Tangent.accepts(f, Some([0.0, 0.0])),
            Err(SnapReject::MissingFacet)
        );
        assert_eq!(
            SnapKind::Intersection.accepts(f, None),
            Err(SnapReject::MissingFacet)
        );
    }

    #[test]
    fn open_path_has_no_center() {
        let mut n = shape(ShapeKind::Path, WorldRect::new(0.0, 0.0, 10.0, 10.0));
        if let NodeKind::Shape(s) = &mut n.kind {
            s.path = Some(PathData {
                start: [0.0, 0.0],
                segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
                closed: false,
                ..Default::default()
            });
        }
        assert_eq!(
            SnapKind::Center.accepts(node_facets(&n), None),
            Err(SnapReject::MissingFacet)
        );
    }

    #[test]
    fn segment_intersection_crosses() {
        let p = segment_intersection([0.0, 0.0], [10.0, 10.0], [0.0, 10.0], [10.0, 0.0])
            .expect("cross");
        assert!((p[0] - 5.0).abs() < 1e-4 && (p[1] - 5.0).abs() < 1e-4);
    }
}
