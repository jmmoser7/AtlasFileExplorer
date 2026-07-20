//! Small 2D vector helpers (f32).

pub(crate) const MITER_LIMIT: f32 = 4.0;
pub(crate) const ROUND_SEGMENTS: usize = 8;
pub(crate) const EPS: f32 = 1e-6;

#[inline]
pub(crate) fn pt(x: f32, y: f32) -> [f32; 2] {
    [x, y]
}

#[inline]
pub(crate) fn from_kurbo(p: kurbo::Point) -> [f32; 2] {
    [p.x as f32, p.y as f32]
}

#[inline]
pub(crate) fn to_kurbo(p: [f32; 2]) -> kurbo::Point {
    kurbo::Point::new(p[0] as f64, p[1] as f64)
}

#[inline]
pub(crate) fn is_finite_pt(p: [f32; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

#[inline]
pub(crate) fn len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

#[inline]
pub(crate) fn normalize(v: [f32; 2]) -> Option<[f32; 2]> {
    let l = len(v);
    if l < EPS {
        None
    } else {
        Some([v[0] / l, v[1] / l])
    }
}

#[inline]
pub(crate) fn perp_left(v: [f32; 2]) -> [f32; 2] {
    [-v[1], v[0]]
}

#[inline]
pub(crate) fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

#[inline]
pub(crate) fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

#[inline]
pub(crate) fn scale(v: [f32; 2], s: f32) -> [f32; 2] {
    [v[0] * s, v[1] * s]
}

#[inline]
pub(crate) fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

#[inline]
pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
pub(crate) fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    len(sub(a, b))
}

/// Distance from point `p` to segment `a`–`b`.
pub(crate) fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = sub(b, a);
    let ab_len_sq = dot(ab, ab);
    if ab_len_sq < EPS * EPS {
        return dist(p, a);
    }
    let t = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / ab_len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    dist(p, proj)
}

pub(crate) fn cumulative_arclength(points: &[[f32; 2]]) -> Vec<f32> {
    let mut acc = vec![0.0; points.len()];
    for i in 1..points.len() {
        acc[i] = acc[i - 1] + dist(points[i - 1], points[i]);
    }
    acc
}

pub(crate) fn half_width_at(style: &crate::StrokeStyle, t: f32) -> f32 {
    if style.width <= 0.0 || !style.width.is_finite() {
        return 0.0;
    }
    let base = style.width * 0.5;
    match style.taper {
        None => base,
        Some((a, b)) => base * lerp(a, b, t.clamp(0.0, 1.0)),
    }
}
