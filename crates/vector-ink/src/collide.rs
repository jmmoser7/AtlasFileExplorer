//! Contact geometry between flattened bodies.
//!
//! A body is either a *band* — every point within `radius` of its polylines
//! (a ring, a wall, a stroke) — or a *solid*: a filled polygon grown by
//! `radius`. [`separation`] answers how far to push one body so it no longer
//! overlaps another, measured at the deepest overlapping point.

use crate::trim::{point_in_polygon, Polygon};

type P = [f32; 2];

/// Below this, a closest-point distance has no usable direction.
const DEGENERATE: f32 = 1e-5;

/// A collision body in world units.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    segs: Vec<[P; 2]>,
    /// Filled region for inside tests. `None` for a band.
    solid: Option<Polygon>,
    radius: f32,
    /// `[min_x, min_y, max_x, max_y]` of the segments, not inflated.
    bounds: [f32; 4],
    /// A solid made of one convex ring, which separates exactly on its axes.
    convex: bool,
}

impl Body {
    /// A band of `radius` around each polyline. `closed` joins each last
    /// vertex back to its first.
    pub fn band(lines: &[Vec<P>], closed: bool, radius: f32) -> Option<Body> {
        let mut segs = Vec::new();
        for line in lines {
            push_segments(&mut segs, line, closed);
        }
        Body::from_parts(segs, None, radius)
    }

    /// A filled polygon (even-odd contours) grown outward by `radius`.
    pub fn solid(poly: Polygon, radius: f32) -> Option<Body> {
        let mut segs = Vec::new();
        for ring in &poly {
            push_segments(&mut segs, ring, true);
        }
        Body::from_parts(segs, Some(poly), radius)
    }

    fn from_parts(segs: Vec<[P; 2]>, solid: Option<Polygon>, radius: f32) -> Option<Body> {
        if segs.is_empty() {
            return None;
        }
        let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for s in &segs {
            for p in s {
                bounds[0] = bounds[0].min(p[0]);
                bounds[1] = bounds[1].min(p[1]);
                bounds[2] = bounds[2].max(p[0]);
                bounds[3] = bounds[3].max(p[1]);
            }
        }
        let convex = solid
            .as_ref()
            .is_some_and(|poly| poly.len() == 1 && is_convex(&poly[0]));
        Some(Body {
            segs,
            solid,
            radius: radius.max(0.0),
            bounds,
            convex,
        })
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    pub fn is_solid(&self) -> bool {
        self.solid.is_some()
    }

    /// Bounds grown by the radius.
    pub fn aabb(&self) -> [f32; 4] {
        let r = self.radius;
        [
            self.bounds[0] - r,
            self.bounds[1] - r,
            self.bounds[2] + r,
            self.bounds[3] + r,
        ]
    }

    pub fn center(&self) -> P {
        [
            (self.bounds[0] + self.bounds[2]) * 0.5,
            (self.bounds[1] + self.bounds[3]) * 0.5,
        ]
    }

    pub fn translate(&mut self, d: P) {
        if d == [0.0, 0.0] {
            return;
        }
        for s in &mut self.segs {
            for p in s.iter_mut() {
                p[0] += d[0];
                p[1] += d[1];
            }
        }
        if let Some(poly) = &mut self.solid {
            for ring in poly.iter_mut() {
                for p in ring.iter_mut() {
                    p[0] += d[0];
                    p[1] += d[1];
                }
            }
        }
        self.bounds[0] += d[0];
        self.bounds[2] += d[0];
        self.bounds[1] += d[1];
        self.bounds[3] += d[1];
    }

    /// Signed distance from `p` to this body's centerline set (negative
    /// inside a solid), and the unit direction that increases it. `hint`
    /// orients the direction when `p` lies exactly on a segment.
    fn signed(&self, p: P, hint: P) -> (f32, P) {
        let mut best = f32::MAX;
        let mut near = p;
        let mut along = [1.0, 0.0];
        for s in &self.segs {
            let q = closest_on_segment(s, p);
            let d = dist(p, q);
            if d < best {
                best = d;
                near = q;
                along = sub(s[1], s[0]);
            }
        }
        let inside = self
            .solid
            .as_ref()
            .is_some_and(|poly| point_in_polygon(poly, p));
        let mut dir = if best > DEGENERATE {
            scale(sub(p, near), 1.0 / best)
        } else {
            let perp = normalize([-along[1], along[0]]);
            if dot(perp, hint) < 0.0 {
                scale(perp, -1.0)
            } else {
                perp
            }
        };
        if inside {
            dir = scale(dir, -1.0);
            best = -best;
        }
        (best, dir)
    }
}

fn push_segments(segs: &mut Vec<[P; 2]>, line: &[P], closed: bool) {
    for w in line.windows(2) {
        if w[0] != w[1] {
            segs.push([w[0], w[1]]);
        }
    }
    if closed && line.len() > 2 {
        let (a, b) = (line[line.len() - 1], line[0]);
        if a != b {
            segs.push([a, b]);
        }
    }
}

/// Whether the inflated bounds of two bodies overlap.
pub fn aabb_overlap(a: &Body, b: &Body) -> bool {
    let (p, q) = (a.aabb(), b.aabb());
    p[0] <= q[2] && q[0] <= p[2] && p[1] <= q[3] && q[1] <= p[3]
}

/// The translation that moves `b` out of `a`, taken at the deepest point of
/// overlap. `None` when they do not touch.
///
/// `motion` is how far `b` moved relative to `a` in the step that produced
/// this overlap (zero when unknown). A band has no inside, so a point that
/// is close enough to have crossed one in that step is pushed back against
/// the motion — which keeps a puck inside a thin ring even when one step
/// carried it past the centerline.
pub fn separation(a: &Body, b: &Body, motion: P) -> Option<P> {
    if !aabb_overlap(a, b) {
        return None;
    }
    if a.convex && b.convex {
        return convex_separation(a, b);
    }
    let reach = a.radius + b.radius;
    let moving = dot(motion, motion) > DEGENERATE * DEGENERATE;
    let crossable = reach + dot(motion, motion).sqrt();
    let back = if moving {
        normalize(scale(motion, -1.0))
    } else {
        normalize(sub(b.center(), a.center()))
    };
    let mut best_depth = 0.0_f32;
    let mut best: Option<P> = None;
    // A point `p` of `b` measured against `a`: push `b` along `dir`. A band
    // point inside a solid goes back out the way it came in, since the
    // nearest edge may be the far one.
    let mut against_a = |p: P, d: f32, dir: P| {
        let (depth, dir) = if moving && !a.is_solid() && d < crossable && dot(dir, back) < 0.0 {
            (reach + d, scale(dir, -1.0))
        } else if moving && d < 0.0 && !b.is_solid() {
            (reach + ray_exit(a, p, back), back)
        } else {
            (reach - d, dir)
        };
        if depth > best_depth {
            best_depth = depth;
            best = Some(scale(dir, depth));
        }
    };
    let a_box = a.aabb();
    let b_box = b.aabb();
    for &p in b.segs.iter().flat_map(|s| s.iter()) {
        if in_box(a_box, p, b.radius) {
            let (d, dir) = a.signed(p, back);
            against_a(p, d, dir);
        }
    }
    let mut from_b = Vec::new();
    for &q in a.segs.iter().flat_map(|s| s.iter()) {
        if in_box(b_box, q, a.radius) {
            from_b.push(q);
        }
    }
    for sa in &a.segs {
        if !seg_in_box(sa, b_box, a.radius) {
            continue;
        }
        for sb in &b.segs {
            if !seg_in_box(sb, a_box, b.radius) {
                continue;
            }
            let (pa, pb) = closest_between(sa, sb);
            if dist(pa, pb) >= reach {
                continue;
            }
            let (d, dir) = a.signed(pb, back);
            against_a(pb, d, dir);
            from_b.push(pa);
        }
    }
    // A point of `a` measured against `b`: push `b` opposite the direction
    // that would carry that point out of `b`.
    let ahead = scale(back, -1.0);
    for q in from_b {
        let (d, dir) = b.signed(q, ahead);
        let (depth, push) = if moving && !b.is_solid() && d < crossable && dot(dir, ahead) < 0.0 {
            (reach + d, dir)
        } else if moving && d < 0.0 && !a.is_solid() {
            (reach + ray_exit(b, q, ahead), back)
        } else {
            (reach - d, scale(dir, -1.0))
        };
        if depth > best_depth {
            best_depth = depth;
            best = Some(scale(push, depth));
        }
    }
    best
}

/// Separating axes over both rings' edge normals, each ring grown by its
/// radius along the axis. Exact for convex pairs; the deepest vertex is not,
/// because it measures against whichever edge happens to be nearest.
fn convex_separation(a: &Body, b: &Body) -> Option<P> {
    fn ring(body: &Body) -> &[P] {
        &body.solid.as_ref().unwrap()[0]
    }
    let (ra, rb) = (ring(a), ring(b));
    let mut best: Option<(f32, P)> = None;
    for s in a.segs.iter().chain(&b.segs) {
        let e = sub(s[1], s[0]);
        let n = normalize([-e[1], e[0]]);
        let (a0, a1) = project(ra, n);
        let (b0, b1) = project(rb, n);
        let forward = a1 + a.radius - (b0 - b.radius);
        let backward = b1 + b.radius - (a0 - a.radius);
        if forward <= 0.0 || backward <= 0.0 {
            return None;
        }
        let (depth, dir) = if forward < backward {
            (forward, n)
        } else {
            (backward, scale(n, -1.0))
        };
        if best.is_none_or(|(d, _)| depth < d) {
            best = Some((depth, dir));
        }
    }
    best.map(|(d, dir)| scale(dir, d))
}

fn project(ring: &[P], n: P) -> (f32, f32) {
    ring.iter()
        .map(|&p| dot(p, n))
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

fn is_convex(ring: &[P]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut sign = 0.0_f32;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let c = ring[(i + 2) % ring.len()];
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cross.abs() <= 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

/// Distance along `dir` from `p` to the nearest boundary crossing of `body`.
fn ray_exit(body: &Body, p: P, dir: P) -> f32 {
    let mut best = f32::MAX;
    for s in &body.segs {
        let e = sub(s[1], s[0]);
        let denom = dir[0] * e[1] - dir[1] * e[0];
        if denom.abs() <= f32::EPSILON {
            continue;
        }
        let w = sub(s[0], p);
        let t = (w[0] * e[1] - w[1] * e[0]) / denom;
        let u = (w[0] * dir[1] - w[1] * dir[0]) / denom;
        if t > 0.0 && (0.0..=1.0).contains(&u) {
            best = best.min(t);
        }
    }
    if best == f32::MAX {
        0.0
    } else {
        best
    }
}

fn in_box(b: [f32; 4], p: P, pad: f32) -> bool {
    p[0] >= b[0] - pad && p[0] <= b[2] + pad && p[1] >= b[1] - pad && p[1] <= b[3] + pad
}

fn seg_in_box(s: &[P; 2], b: [f32; 4], pad: f32) -> bool {
    let lo = [s[0][0].min(s[1][0]) - pad, s[0][1].min(s[1][1]) - pad];
    let hi = [s[0][0].max(s[1][0]) + pad, s[0][1].max(s[1][1]) + pad];
    lo[0] <= b[2] && hi[0] >= b[0] && lo[1] <= b[3] && hi[1] >= b[1]
}

fn closest_on_segment(s: &[P; 2], p: P) -> P {
    let ab = sub(s[1], s[0]);
    let len2 = dot(ab, ab);
    if len2 <= 0.0 {
        return s[0];
    }
    let t = (dot(sub(p, s[0]), ab) / len2).clamp(0.0, 1.0);
    add(s[0], scale(ab, t))
}

/// Closest points between two segments (the crossing point when they cross).
fn closest_between(a: &[P; 2], b: &[P; 2]) -> (P, P) {
    if let Some(x) = crossing(a, b) {
        return (x, x);
    }
    let candidates = [
        (a[0], closest_on_segment(b, a[0])),
        (a[1], closest_on_segment(b, a[1])),
        (closest_on_segment(a, b[0]), b[0]),
        (closest_on_segment(a, b[1]), b[1]),
    ];
    candidates
        .into_iter()
        .min_by(|x, y| dist(x.0, x.1).total_cmp(&dist(y.0, y.1)))
        .unwrap()
}

fn crossing(a: &[P; 2], b: &[P; 2]) -> Option<P> {
    let r = sub(a[1], a[0]);
    let s = sub(b[1], b[0]);
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() <= f32::EPSILON {
        return None;
    }
    let qp = sub(b[0], a[0]);
    let t = (qp[0] * s[1] - qp[1] * s[0]) / denom;
    let u = (qp[0] * r[1] - qp[1] * r[0]) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| add(a[0], scale(r, t)))
}

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn scale(a: P, k: f32) -> P {
    [a[0] * k, a[1] * k]
}
fn dot(a: P, b: P) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}
fn dist(a: P, b: P) -> f32 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}
fn normalize(a: P) -> P {
    let len = dot(a, a).sqrt();
    if len <= DEGENERATE {
        [1.0, 0.0]
    } else {
        scale(a, 1.0 / len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f32, y: f32, s: f32) -> Polygon {
        vec![vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]]
    }

    fn circle(cx: f32, cy: f32, r: f32) -> Vec<P> {
        (0..64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                [cx + a.cos() * r, cy + a.sin() * r]
            })
            .collect()
    }

    #[test]
    fn apart_bodies_do_not_touch() {
        let a = Body::solid(square(0.0, 0.0, 10.0), 0.0).unwrap();
        let b = Body::solid(square(20.0, 0.0, 10.0), 0.0).unwrap();
        assert_eq!(separation(&a, &b, [0.0, 0.0]), None);
    }

    #[test]
    fn overlapping_solids_push_along_the_shallow_axis() {
        let a = Body::solid(square(0.0, 0.0, 10.0), 0.0).unwrap();
        let b = Body::solid(square(8.0, 1.0, 10.0), 0.0).unwrap();
        let push = separation(&a, &b, [0.0, 0.0]).unwrap();
        assert!((push[0] - 2.0).abs() < 1e-3, "{push:?}");
        assert!(push[1].abs() < 1e-3, "{push:?}");
    }

    #[test]
    fn a_buffer_keeps_bodies_apart_before_they_meet() {
        let a = Body::solid(square(0.0, 0.0, 10.0), 5.0).unwrap();
        let b = Body::solid(square(12.0, 0.0, 10.0), 0.0).unwrap();
        let push = separation(&a, &b, [0.0, 0.0]).unwrap();
        assert!((push[0] - 3.0).abs() < 1e-3, "{push:?}");
    }

    #[test]
    fn a_ring_pushes_an_inner_body_back_inside() {
        let ring = Body::band(&[circle(0.0, 0.0, 50.0)], true, 1.0).unwrap();
        let puck = Body::solid(square(43.5, -3.0, 5.5), 0.0).unwrap();
        let push = separation(&ring, &puck, [0.0, 0.0]).unwrap();
        assert!(push[0] < 0.0, "pushed inward: {push:?}");
    }

    #[test]
    fn order_does_not_matter() {
        let ring = Body::band(&[circle(0.0, 0.0, 50.0)], true, 1.0).unwrap();
        let puck = Body::solid(square(43.5, -3.0, 5.5), 0.0).unwrap();
        let push = separation(&puck, &ring, [0.0, 0.0]).unwrap();
        assert!(push[0] > 0.0, "ring pushed away from the puck: {push:?}");
        let moving = separation(&puck, &ring, [-4.0, 0.0]).unwrap();
        assert!(moving[0] > 0.0, "{moving:?}");
    }

    #[test]
    fn a_puck_that_overshot_a_thin_ring_goes_back_the_way_it_came() {
        let ring = Body::band(&[circle(0.0, 0.0, 50.0)], true, 1.0).unwrap();
        let puck = Body::solid(square(45.0, -3.0, 5.5), 0.0).unwrap();
        let push = separation(&ring, &puck, [4.0, 0.0]).unwrap();
        assert!(push[0] < -1.0, "pushed back inside: {push:?}");
    }

    #[test]
    fn a_ring_leaves_a_body_in_its_middle_alone() {
        let ring = Body::band(&[circle(0.0, 0.0, 50.0)], true, 1.0).unwrap();
        let puck = Body::solid(square(-5.0, -5.0, 10.0), 0.0).unwrap();
        assert_eq!(separation(&ring, &puck, [0.0, 0.0]), None);
    }

    #[test]
    fn crossing_walls_push_back_against_the_motion() {
        let a = Body::band(&[vec![[-10.0, 0.0], [10.0, 0.0]]], false, 1.0).unwrap();
        let b = Body::band(&[vec![[0.0, -10.0], [0.0, 10.0]]], false, 1.0).unwrap();
        let push = separation(&a, &b, [0.0, -1.0]).unwrap();
        assert!(push[1] > 0.0 && push[0].abs() < 1e-3, "{push:?}");
    }

    #[test]
    fn translate_moves_bounds_and_geometry() {
        let mut a = Body::solid(square(0.0, 0.0, 10.0), 0.0).unwrap();
        a.translate([5.0, 0.0]);
        let b = Body::solid(square(16.0, 0.0, 10.0), 0.0).unwrap();
        assert_eq!(separation(&a, &b, [0.0, 0.0]), None);
        a.translate([2.0, 0.0]);
        assert!(separation(&a, &b, [0.0, 0.0]).is_some());
    }
}
