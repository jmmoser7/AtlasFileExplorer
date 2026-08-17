//! Wire-port sites: derived from each node's own features, never stored.
//!
//! Both interpreters resolve anchors through [`WireHost`] so a rotated rect
//! keeps its ports on the same local edges, an ellipse keeps them on the
//! axis tips (which lie on the curve), and an open stroke keeps them on
//! the stroke itself. The AABB is only a fallback for area objects that
//! have no richer silhouette facet yet.
//!
//! ## Current kinds
//!
//! | Kind | Class | Ports |
//! |------|-------|-------|
//! | Rect, text, sticky, image, frame, portal, dock strip | Oriented box | Midpoints of the **local** edges, then rotated about the node center |
//! | Ellipse | Oriented box | Same four local-axis extrema — those points lie on the ellipse |
//! | Closed path / compound | Oriented box | Local-edge mids of `node.rect` (silhouette hits are a future facet) |
//! | Line, arc, polyline, bezier, open path | Open stroke | Arclength `t = 0`, `0.5`, `1` (start / mid / end) |
//! | Connector | — | No ports (filtered by the interaction layer) |
//!
//! ## Future kinds
//!
//! New geometry declares a class here; it does not special-case a file
//! format (facet taxonomy). A regular polygon or star would expose
//! vertices or edge mids. A closed freehand / NURBS would ray-cast the
//! local cardinals against the silhouette. A 3D / mesh portal stays an
//! oriented frame — ports belong to the frame, not the mesh. Do not
//! invent a port per vertex on a dense path (Article III).
//!
//! Geometry stays derived (Art. VI.3). The journal stores only
//! `Side` + `t` + node id.

use crate::scene::{
    connector_anchor_point, Node, NodeKind, PathData, PathSeg, ShapeKind, Side, WorldRect,
};

const SAMPLE_QUAD: usize = 8;
const SAMPLE_CUBIC: usize = 12;
const EPS: f32 = 1e-6;

/// One discrete spawn handle on a host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WirePort {
    pub side: Side,
    pub t: f32,
    pub point: [f32; 2],
}

/// Nearest attach site for a world point (grip or edge / stroke).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WireSnap {
    pub side: Side,
    pub t: f32,
    pub point: [f32; 2],
    pub dist: f32,
}

/// Pose a connector end resolves against. Built from a [`Node`] at
/// paint / hit / export time — never persisted.
#[derive(Debug, Clone)]
pub struct WireHost {
    pub rect: WorldRect,
    pub rotation_deg: f32,
    kind: HostKind,
}

#[derive(Debug, Clone)]
enum HostKind {
    Oriented { ellipse: bool },
    Open(OpenStroke),
}

#[derive(Debug, Clone)]
struct OpenStroke {
    pts: Vec<[f32; 2]>,
    cum: Vec<f32>,
    total: f32,
}

impl WireHost {
    /// Unrotated axis-aligned box — the historical AABB host. Used by
    /// tests and by paste remap when only a rect is available.
    pub fn from_rect(rect: WorldRect) -> Self {
        Self {
            rect,
            rotation_deg: 0.0,
            kind: HostKind::Oriented { ellipse: false },
        }
    }

    /// Unrotated ellipse — collision uses the oval, not the AABB.
    pub fn from_ellipse(rect: WorldRect) -> Self {
        Self {
            rect,
            rotation_deg: 0.0,
            kind: HostKind::Oriented { ellipse: true },
        }
    }

    pub fn from_node(node: &Node) -> Self {
        if let Some(stroke) = open_stroke(node) {
            return Self {
                rect: node.rect,
                rotation_deg: node.rotation_deg,
                kind: HostKind::Open(stroke),
            };
        }
        let ellipse = matches!(
            &node.kind,
            NodeKind::Shape(s) if s.shape == ShapeKind::Ellipse
        );
        Self {
            rect: node.rect,
            rotation_deg: node.rotation_deg,
            kind: HostKind::Oriented { ellipse },
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.kind, HostKind::Open(_))
    }

    pub fn is_area(&self) -> bool {
        !self.is_open()
    }

    pub fn is_ellipse(&self) -> bool {
        matches!(self.kind, HostKind::Oriented { ellipse: true })
    }

    /// Default spawn handles. Three on an open stroke, four on an area.
    pub fn ports(&self) -> Vec<WirePort> {
        match &self.kind {
            HostKind::Oriented { .. } => Side::BOX
                .into_iter()
                .map(|side| WirePort {
                    side,
                    t: 0.5,
                    point: self.anchor(side, 0.5),
                })
                .collect(),
            HostKind::Open(_) => vec![
                WirePort {
                    side: Side::Start,
                    t: 0.0,
                    point: self.anchor(Side::Start, 0.0),
                },
                WirePort {
                    side: Side::Mid,
                    t: 0.5,
                    point: self.anchor(Side::Mid, 0.5),
                },
                WirePort {
                    side: Side::End,
                    t: 1.0,
                    point: self.anchor(Side::End, 1.0),
                },
            ],
        }
    }

    /// World point for a stored `(side, t)`.
    ///
    /// Oriented: `side` is a **local** edge; the point is rotated with the
    /// node. Open: `Start` / `End` ignore `t`; `Mid` is arclength `t`.
    /// A box side stored on an open host (old files) snaps to the nearest
    /// of start / mid / end.
    pub fn anchor(&self, side: Side, t: f32) -> [f32; 2] {
        match &self.kind {
            HostKind::Oriented { .. } => self.oriented_anchor(side, t),
            HostKind::Open(stroke) => self.open_anchor(stroke, side, t),
        }
    }

    /// Outward unit direction at the anchor (bezier handles; ortho stubs
    /// cardinalize this). Open mid uses a consistent left-normal of the
    /// tangent — it does not flip when the far end crosses the curve.
    pub fn outward(&self, side: Side, t: f32) -> [f32; 2] {
        match &self.kind {
            HostKind::Oriented { .. } => rotate_vec(side.normal(), self.rotation_deg),
            HostKind::Open(stroke) => open_outward(stroke, self.rect, side, t),
        }
    }

    /// Nearest attach site to `world` (local edge or stroke projection).
    pub fn snap(&self, world: [f32; 2]) -> WireSnap {
        match &self.kind {
            HostKind::Oriented { .. } => self.oriented_snap(world),
            HostKind::Open(stroke) => open_snap(stroke, world),
        }
    }

    fn oriented_anchor(&self, side: Side, t: f32) -> [f32; 2] {
        let local = match side {
            Side::Top | Side::Right | Side::Bottom | Side::Left => {
                connector_anchor_point(self.rect, side, t)
            }
            Side::Start => connector_anchor_point(self.rect, Side::Left, 0.5),
            Side::End => connector_anchor_point(self.rect, Side::Right, 0.5),
            Side::Mid => {
                let (cx, cy) = self.rect.center();
                [cx, cy]
            }
        };
        let (cx, cy) = self.rect.center();
        rotate_pt(local, cx, cy, self.rotation_deg)
    }

    fn open_anchor(&self, stroke: &OpenStroke, side: Side, t: f32) -> [f32; 2] {
        match side {
            Side::Start => stroke.at(0.0).0,
            Side::End => stroke.at(1.0).0,
            Side::Mid => stroke.at(t.clamp(0.0, 1.0)).0,
            box_side => {
                let p = connector_anchor_point(self.rect, box_side, t);
                nearest_feature(stroke, p).point
            }
        }
    }

    fn oriented_snap(&self, world: [f32; 2]) -> WireSnap {
        let (cx, cy) = self.rect.center();
        let local = unrotate_pt(world, cx, cy, self.rotation_deg);
        let mut best = WireSnap {
            side: Side::Top,
            t: 0.5,
            point: [0.0, 0.0],
            dist: f32::INFINITY,
        };
        for side in Side::BOX {
            let (a, b) = local_edge(self.rect, side);
            let (t, q) = project_seg(a, b, local);
            let d = dist(q, local);
            if d < best.dist {
                let world_q = rotate_pt(q, cx, cy, self.rotation_deg);
                best = WireSnap {
                    side,
                    t,
                    point: world_q,
                    dist: d,
                };
            }
        }
        best
    }
}

/// Resolve an anchored end through the node's current pose.
pub fn connector_anchor_on(node: &Node, side: Side, t: f32) -> [f32; 2] {
    WireHost::from_node(node).anchor(side, t)
}

fn open_stroke(node: &Node) -> Option<OpenStroke> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    if shape.shape == ShapeKind::Line {
        return Some(legacy_line(node, shape.flip));
    }
    if shape.shape == ShapeKind::Path {
        if let Some(path) = &shape.path {
            if !path.closed && !path.is_empty() {
                return Some(sample_path(path, node.rect, node.rotation_deg));
            }
        }
    }
    None
}

fn legacy_line(node: &Node, flip: bool) -> OpenStroke {
    let r = node.rect;
    let (a, b) = if flip {
        ([r.x, r.y + r.h], [r.x + r.w, r.y])
    } else {
        ([r.x, r.y], [r.x + r.w, r.y + r.h])
    };
    let (cx, cy) = r.center();
    let a = rotate_pt(a, cx, cy, node.rotation_deg);
    let b = rotate_pt(b, cx, cy, node.rotation_deg);
    OpenStroke::from_pts(vec![a, b])
}

fn sample_path(path: &PathData, rect: WorldRect, rotation_deg: f32) -> OpenStroke {
    let mut pts = Vec::new();
    let mut cur = world_norm(path.start, rect, rotation_deg);
    pts.push(cur);
    for seg in &path.segs {
        match *seg {
            PathSeg::Line { to } => {
                cur = world_norm(to, rect, rotation_deg);
                pts.push(cur);
            }
            PathSeg::Quad { ctrl, to } => {
                let c = world_norm(ctrl, rect, rotation_deg);
                let end = world_norm(to, rect, rotation_deg);
                for i in 1..=SAMPLE_QUAD {
                    let t = i as f32 / SAMPLE_QUAD as f32;
                    pts.push(eval_quad(cur, c, end, t));
                }
                cur = end;
            }
            PathSeg::Cubic { c1, c2, to } => {
                let p1 = world_norm(c1, rect, rotation_deg);
                let p2 = world_norm(c2, rect, rotation_deg);
                let end = world_norm(to, rect, rotation_deg);
                for i in 1..=SAMPLE_CUBIC {
                    let t = i as f32 / SAMPLE_CUBIC as f32;
                    pts.push(eval_cubic(cur, p1, p2, end, t));
                }
                cur = end;
            }
        }
    }
    OpenStroke::from_pts(pts)
}

impl OpenStroke {
    fn from_pts(pts: Vec<[f32; 2]>) -> Self {
        let pts = if pts.len() < 2 {
            let p = pts.first().copied().unwrap_or([0.0, 0.0]);
            vec![p, p]
        } else {
            pts
        };
        let mut cum = vec![0.0; pts.len()];
        for i in 1..pts.len() {
            cum[i] = cum[i - 1] + dist(pts[i], pts[i - 1]);
        }
        let total = cum.last().copied().unwrap_or(0.0);
        Self { pts, cum, total }
    }

    /// Point and unit tangent at arclength fraction `t` (0..=1).
    fn at(&self, t: f32) -> ([f32; 2], [f32; 2]) {
        let t = t.clamp(0.0, 1.0);
        if self.total < EPS {
            return (self.pts[0], [1.0, 0.0]);
        }
        let s = t * self.total;
        for i in 1..self.pts.len() {
            if s <= self.cum[i] + EPS || i + 1 == self.pts.len() {
                let span = (self.cum[i] - self.cum[i - 1]).max(EPS);
                let u = ((s - self.cum[i - 1]) / span).clamp(0.0, 1.0);
                let p = lerp(self.pts[i - 1], self.pts[i], u);
                let tan = norm(sub(self.pts[i], self.pts[i - 1]));
                return (p, tan);
            }
        }
        let n = self.pts.len();
        (self.pts[n - 1], norm(sub(self.pts[n - 1], self.pts[n - 2])))
    }
}

fn open_outward(stroke: &OpenStroke, rect: WorldRect, side: Side, t: f32) -> [f32; 2] {
    match side {
        Side::Start => {
            let tan = stroke.at(0.0).1;
            [-tan[0], -tan[1]]
        }
        Side::End => stroke.at(1.0).1,
        Side::Mid => {
            let tan = stroke.at(t.clamp(0.0, 1.0)).1;
            left_normal(tan)
        }
        box_side => {
            let p = connector_anchor_point(rect, box_side, t);
            let feat = nearest_feature(stroke, p);
            open_outward(stroke, rect, feat.side, feat.t)
        }
    }
}

fn nearest_feature(stroke: &OpenStroke, world: [f32; 2]) -> WireSnap {
    let start = stroke.at(0.0).0;
    let mid = stroke.at(0.5).0;
    let end = stroke.at(1.0).0;
    let candidates = [
        (Side::Start, 0.0, start),
        (Side::Mid, 0.5, mid),
        (Side::End, 1.0, end),
    ];
    let mut best = WireSnap {
        side: Side::Mid,
        t: 0.5,
        point: mid,
        dist: f32::INFINITY,
    };
    for (side, t, p) in candidates {
        let d = dist(p, world);
        if d < best.dist {
            best = WireSnap {
                side,
                t,
                point: p,
                dist: d,
            };
        }
    }
    best
}

fn open_snap(stroke: &OpenStroke, world: [f32; 2]) -> WireSnap {
    let mut best = WireSnap {
        side: Side::Mid,
        t: 0.5,
        point: stroke.at(0.5).0,
        dist: f32::INFINITY,
    };
    if stroke.pts.len() < 2 {
        return best;
    }
    for i in 1..stroke.pts.len() {
        let (u, q) = project_seg(stroke.pts[i - 1], stroke.pts[i], world);
        let d = dist(q, world);
        if d < best.dist {
            let s0 = stroke.cum[i - 1];
            let s1 = stroke.cum[i];
            let s = s0 + (s1 - s0) * u;
            let t = if stroke.total < EPS {
                0.0
            } else {
                (s / stroke.total).clamp(0.0, 1.0)
            };
            let (side, t) = if t <= 0.02 {
                (Side::Start, 0.0)
            } else if t >= 0.98 {
                (Side::End, 1.0)
            } else {
                (Side::Mid, t)
            };
            best = WireSnap {
                side,
                t,
                point: q,
                dist: d,
            };
        }
    }
    best
}

fn local_edge(rect: WorldRect, side: Side) -> ([f32; 2], [f32; 2]) {
    match side {
        Side::Top => ([rect.x, rect.y], [rect.x + rect.w, rect.y]),
        Side::Bottom => (
            [rect.x, rect.y + rect.h],
            [rect.x + rect.w, rect.y + rect.h],
        ),
        Side::Left => ([rect.x, rect.y], [rect.x, rect.y + rect.h]),
        Side::Right => (
            [rect.x + rect.w, rect.y],
            [rect.x + rect.w, rect.y + rect.h],
        ),
        _ => ([rect.x, rect.y], [rect.x + rect.w, rect.y]),
    }
}

fn world_norm(n: [f32; 2], rect: WorldRect, rotation_deg: f32) -> [f32; 2] {
    let p = [rect.x + n[0] * rect.w, rect.y + n[1] * rect.h];
    let (cx, cy) = rect.center();
    rotate_pt(p, cx, cy, rotation_deg)
}

fn rotate_pt(p: [f32; 2], cx: f32, cy: f32, deg: f32) -> [f32; 2] {
    if deg.abs() < f32::EPSILON {
        return p;
    }
    let (sin, cos) = deg.to_radians().sin_cos();
    let dx = p[0] - cx;
    let dy = p[1] - cy;
    [cx + dx * cos - dy * sin, cy + dx * sin + dy * cos]
}

fn unrotate_pt(p: [f32; 2], cx: f32, cy: f32, deg: f32) -> [f32; 2] {
    rotate_pt(p, cx, cy, -deg)
}

fn rotate_vec(v: [f32; 2], deg: f32) -> [f32; 2] {
    if deg.abs() < f32::EPSILON {
        return v;
    }
    let (sin, cos) = deg.to_radians().sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

/// Left normal in screen axes (+x right, +y down): facing +x, left is −y.
fn left_normal(tan: [f32; 2]) -> [f32; 2] {
    [tan[1], -tan[0]]
}

fn eval_quad(a: [f32; 2], c: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        u * u * a[0] + 2.0 * u * t * c[0] + t * t * b[0],
        u * u * a[1] + 2.0 * u * t * c[1] + t * t * b[1],
    ]
}

fn eval_cubic(a: [f32; 2], c1: [f32; 2], c2: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    [
        uu * u * a[0] + 3.0 * uu * t * c1[0] + 3.0 * u * tt * c2[0] + tt * t * b[0],
        uu * u * a[1] + 3.0 * uu * t * c1[1] + 3.0 * u * tt * c2[1] + tt * t * b[1],
    ]
}

fn project_seg(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> (f32, [f32; 2]) {
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    if l2 < EPS {
        return (0.0, a);
    }
    let t = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2;
    let t = t.clamp(0.0, 1.0);
    (t, lerp(a, b, t))
}

fn lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

fn norm(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l < EPS {
        [0.0, 0.0]
    } else {
        [v[0] / l, v[1] / l]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{FontChoice, TextAlign};
    use crate::scene::{
        NodeId, NodeKind, PathData, PathSeg, ShapeKind, ShapeNode, Stroke, TextNode,
    };

    fn shape_node(kind: ShapeKind, rect: WorldRect, rot: f32, path: Option<PathData>) -> Node {
        Node {
            id: NodeId(1),
            rect,
            rotation_deg: rot,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: kind,
                fill: None,
                stroke: Stroke::default(),
                corner: Default::default(),
                flip: false,
                path,
            }),
        }
    }

    #[test]
    fn rotated_rect_top_follows_the_local_edge_not_the_aabb() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 40.0);
        let node = shape_node(ShapeKind::Rect, rect, 90.0, None);
        let host = WireHost::from_node(&node);
        let top = host.anchor(Side::Top, 0.5);
        // Local top mid (50, 0) rotated 90° CW about (50, 20) → (70, 20).
        assert!((top[0] - 70.0).abs() < 1e-4 && (top[1] - 20.0).abs() < 1e-4);
        let aabb_top = connector_anchor_point(rect, Side::Top, 0.5);
        assert!(
            dist(top, aabb_top) > 10.0,
            "rotated top must leave the AABB top, got {top:?} vs {aabb_top:?}"
        );
        let n = host.outward(Side::Top, 0.5);
        // Local (0, −1) rotated 90° CW → (1, 0).
        assert!((n[0] - 1.0).abs() < 1e-4 && n[1].abs() < 1e-4);
    }

    #[test]
    fn ellipse_ports_lie_on_the_ellipse_after_rotation() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 40.0);
        let node = shape_node(ShapeKind::Ellipse, rect, 35.0, None);
        let host = WireHost::from_node(&node);
        let (cx, cy) = rect.center();
        let rx = rect.w * 0.5;
        let ry = rect.h * 0.5;
        for port in host.ports() {
            let local = unrotate_pt(port.point, cx, cy, 35.0);
            let nx = (local[0] - cx) / rx;
            let ny = (local[1] - cy) / ry;
            let resid = nx * nx + ny * ny;
            assert!(
                (resid - 1.0).abs() < 1e-4,
                "port {:?} not on ellipse (resid={resid})",
                port.side
            );
        }
    }

    #[test]
    fn open_path_ports_are_start_mid_end() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Line { to: [1.0, 1.0] }],
            ..PathData::default()
        };
        let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
        let host = WireHost::from_node(&node);
        assert!(host.is_open());
        let ports = host.ports();
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].side, Side::Start);
        assert_eq!(ports[1].side, Side::Mid);
        assert_eq!(ports[2].side, Side::End);
        assert!((ports[0].point[0] - 0.0).abs() < 1e-4 && (ports[0].point[1] - 0.0).abs() < 1e-4);
        assert!((ports[1].point[0] - 50.0).abs() < 1e-4 && (ports[1].point[1] - 25.0).abs() < 1e-4);
        assert!(
            (ports[2].point[0] - 100.0).abs() < 1e-4 && (ports[2].point[1] - 50.0).abs() < 1e-4
        );
    }

    #[test]
    fn legacy_line_ports_follow_the_diagonal() {
        let rect = WorldRect::new(10.0, 20.0, 80.0, 40.0);
        let node = shape_node(ShapeKind::Line, rect, 0.0, None);
        let host = WireHost::from_node(&node);
        let ports = host.ports();
        assert_eq!(ports[0].point, [10.0, 20.0]);
        assert_eq!(ports[2].point, [90.0, 60.0]);
        assert!((ports[1].point[0] - 50.0).abs() < 1e-4);
        assert!((ports[1].point[1] - 40.0).abs() < 1e-4);
    }

    #[test]
    fn old_box_side_on_a_line_snaps_to_a_stroke_feature() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 0.0);
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            ..PathData::default()
        };
        let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
        let host = WireHost::from_node(&node);
        // Historical Right mid of a degenerate line AABB is the end.
        let p = host.anchor(Side::Right, 0.5);
        assert!((p[0] - 100.0).abs() < 1e-3);
        let p = host.anchor(Side::Left, 0.5);
        assert!(p[0].abs() < 1e-3);
    }

    #[test]
    fn text_and_unrotated_rect_share_oriented_mids() {
        let rect = WorldRect::new(0.0, 0.0, 80.0, 20.0);
        let text = Node {
            id: NodeId(2),
            rect,
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            kind: NodeKind::Text(TextNode {
                text: "hi".into(),
                family: FontChoice::Sans,
                size: 14.0,
                color: crate::scene::Rgba::BLACK,
                align: TextAlign::Left,
                fill: None,
            }),
        };
        let a = WireHost::from_node(&text).anchor(Side::Top, 0.5);
        let b = connector_anchor_point(rect, Side::Top, 0.5);
        assert_eq!(a, b);
    }

    #[test]
    fn open_mid_outward_is_stable_left_normal() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 0.0);
        let path = PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            ..PathData::default()
        };
        let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
        let n = WireHost::from_node(&node).outward(Side::Mid, 0.5);
        // Tangent +x; left in screen axes is −y.
        assert!(n[0].abs() < 1e-4 && (n[1] + 1.0).abs() < 1e-4);
    }
}
