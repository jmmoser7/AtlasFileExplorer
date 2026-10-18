//! Sampled stroke geometry and vector math for the wire-host owner.
use super::*;

pub(super) fn open_stroke(node: &Node) -> Option<OpenStroke> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    if shape.shape == ShapeKind::Line {
        return Some(legacy_line(node, shape.flip));
    }
    if shape.shape == ShapeKind::Path {
        if let Some(path) = &shape.path {
            if !path.is_empty() {
                return Some(sample_path(path, node.rect, node.rotation_deg));
            }
        }
    }
    None
}

pub(super) fn legacy_line(node: &Node, flip: bool) -> OpenStroke {
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

pub(super) fn sample_path(path: &PathData, rect: WorldRect, rotation_deg: f32) -> OpenStroke {
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
    if path.closed {
        if let Some(&first) = pts.first() {
            if pts.last().is_none_or(|p| dist(*p, first) > EPS) {
                pts.push(first);
            }
        }
    }
    OpenStroke::from_pts(pts)
}

impl OpenStroke {
    pub(super) fn from_pts(pts: Vec<[f32; 2]>) -> Self {
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
    pub(super) fn at(&self, t: f32) -> ([f32; 2], [f32; 2]) {
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

pub(super) fn open_outward(stroke: &OpenStroke, rect: WorldRect, side: Side, t: f32) -> [f32; 2] {
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

pub(super) fn nearest_feature(stroke: &OpenStroke, world: [f32; 2]) -> WireSnap {
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

pub(super) fn open_snap(stroke: &OpenStroke, world: [f32; 2]) -> WireSnap {
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

pub(super) fn local_edge(rect: WorldRect, side: Side) -> ([f32; 2], [f32; 2]) {
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

pub(super) fn world_norm(n: [f32; 2], rect: WorldRect, rotation_deg: f32) -> [f32; 2] {
    let p = [rect.x + n[0] * rect.w, rect.y + n[1] * rect.h];
    let (cx, cy) = rect.center();
    rotate_pt(p, cx, cy, rotation_deg)
}

pub(super) fn rotate_pt(p: [f32; 2], cx: f32, cy: f32, deg: f32) -> [f32; 2] {
    if deg.abs() < f32::EPSILON {
        return p;
    }
    let (sin, cos) = deg.to_radians().sin_cos();
    let dx = p[0] - cx;
    let dy = p[1] - cy;
    [cx + dx * cos - dy * sin, cy + dx * sin + dy * cos]
}

pub(super) fn unrotate_pt(p: [f32; 2], cx: f32, cy: f32, deg: f32) -> [f32; 2] {
    rotate_pt(p, cx, cy, -deg)
}

pub(super) fn rotate_vec(v: [f32; 2], deg: f32) -> [f32; 2] {
    if deg.abs() < f32::EPSILON {
        return v;
    }
    let (sin, cos) = deg.to_radians().sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

/// Left normal in screen axes (+x right, +y down): facing +x, left is −y.
pub(super) fn left_normal(tan: [f32; 2]) -> [f32; 2] {
    [tan[1], -tan[0]]
}

pub(super) fn eval_quad(a: [f32; 2], c: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        u * u * a[0] + 2.0 * u * t * c[0] + t * t * b[0],
        u * u * a[1] + 2.0 * u * t * c[1] + t * t * b[1],
    ]
}

pub(super) fn eval_cubic(a: [f32; 2], c1: [f32; 2], c2: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    [
        uu * u * a[0] + 3.0 * uu * t * c1[0] + 3.0 * u * tt * c2[0] + tt * t * b[0],
        uu * u * a[1] + 3.0 * uu * t * c1[1] + 3.0 * u * tt * c2[1] + tt * t * b[1],
    ]
}

pub(super) fn project_seg(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> (f32, [f32; 2]) {
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    if l2 < EPS {
        return (0.0, a);
    }
    let t = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2;
    let t = t.clamp(0.0, 1.0);
    (t, lerp(a, b, t))
}

pub(super) fn lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

pub(super) fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

pub(super) fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

pub(super) fn norm(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l < EPS {
        [0.0, 0.0]
    } else {
        [v[0] / l, v[1] / l]
    }
}
