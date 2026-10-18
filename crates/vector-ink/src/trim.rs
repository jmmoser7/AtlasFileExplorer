//! 2D Trim geometry — Rhino's click-the-dying-piece, planar only.
//!
//! Closed regions use `i_overlay` boolean / slice; open curves split at segment intersections.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::slice::FloatSlice;

use crate::geom::EPS;

mod overlay;
pub use overlay::boolean_difference_raw;
use overlay::{overlay_shapes, shapes_from_overlay};
mod fill;
#[cfg(test)]
use fill::signed_area;
pub use fill::{fill_triangles, point_in_mesh, point_in_polygon};

/// A closed region: first contour is the outer, the rest are holes.
pub type Polygon = Vec<Vec<[f32; 2]>>;

/// Remaining closed pieces after a region trim (each may carry holes).
pub type TrimPolys = Vec<Polygon>;

/// A cutting object in world space.
#[derive(Debug, Clone)]
pub enum Cutter {
    /// Closed filled region (outer + optional holes).
    Closed(Polygon),
    /// Finite open polyline.
    Open(Vec<[f32; 2]>),
    /// Infinite line through `origin` along `dir` (unit).
    Infinite { origin: [f32; 2], dir: [f32; 2] },
}

/// Closest open-path span under a click.
#[derive(Debug, Clone, Copy)]
pub struct SpanHit {
    pub span: usize,
    pub dist: f32,
}

const INFINITE: f32 = 100_000.0;

/// Build an infinite-line cutter from a two-point segment.
pub fn infinite_line(a: [f32; 2], b: [f32; 2]) -> Option<Cutter> {
    let dir = [b[0] - a[0], b[1] - a[1]];
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    if len < EPS {
        return None;
    }
    Some(Cutter::Infinite {
        origin: a,
        dir: [dir[0] / len, dir[1] / len],
    })
}

fn to_f64(p: [f32; 2]) -> [f64; 2] {
    [p[0] as f64, p[1] as f64]
}

fn from_f64(p: [f64; 2]) -> [f32; 2] {
    [p[0] as f32, p[1] as f32]
}

fn close_ring(pts: &[[f32; 2]]) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = pts.iter().copied().map(to_f64).collect();
    if let (Some(first), Some(last)) = (out.first().copied(), out.last().copied()) {
        let dx = first[0] - last[0];
        let dy = first[1] - last[1];
        if dx * dx + dy * dy > 1e-12 {
            out.push(first);
        }
    }
    if out.len() >= 2 {
        // i_overlay auto-closes; drop the repeated last point if present.
        let n = out.len();
        if (out[0][0] - out[n - 1][0]).abs() < 1e-9 && (out[0][1] - out[n - 1][1]).abs() < 1e-9 {
            out.pop();
        }
    }
    out
}

/// Subject minus clip. Each argument is a list of closed contours
/// (first = outer). Multiple subject outers are unioned first.
pub fn boolean_difference(subj: &Polygon, clip: &Polygon) -> TrimPolys {
    let s: Vec<Vec<[f64; 2]>> = subj.iter().map(|c| close_ring(c)).collect();
    let c: Vec<Vec<[f64; 2]>> = clip.iter().map(|c| close_ring(c)).collect();
    overlay_shapes(&s, &c, OverlayRule::Difference)
}

/// Subject ∩ clip.
pub fn boolean_intersection(subj: &Polygon, clip: &Polygon) -> TrimPolys {
    let s: Vec<Vec<[f64; 2]>> = subj.iter().map(|c| close_ring(c)).collect();
    let c: Vec<Vec<[f64; 2]>> = clip.iter().map(|c| close_ring(c)).collect();
    overlay_shapes(&s, &c, OverlayRule::Intersect)
}

/// Symmetric difference — used to subtract a face that already lives inside
/// the subject (including a holed remainder) without collapsing the clip.
pub fn boolean_xor(subj: &Polygon, clip: &Polygon) -> TrimPolys {
    let s: Vec<Vec<[f64; 2]>> = subj.iter().map(|c| close_ring(c)).collect();
    let c: Vec<Vec<[f64; 2]>> = clip.iter().map(|c| close_ring(c)).collect();
    overlay_shapes(&s, &c, OverlayRule::Xor)
}

/// Subject ∪ clip.
pub fn boolean_union(subj: &Polygon, clip: &Polygon) -> TrimPolys {
    let s: Vec<Vec<[f64; 2]>> = subj.iter().map(|c| close_ring(c)).collect();
    let c: Vec<Vec<[f64; 2]>> = clip.iter().map(|c| close_ring(c)).collect();
    overlay_shapes(&s, &c, OverlayRule::Union)
}

/// Union of many polygons (each may carry holes). Disjoint inputs stay
/// separate pieces.
pub fn boolean_union_all(polys: &[Polygon]) -> TrimPolys {
    if polys.is_empty() {
        return Vec::new();
    }
    let mut acc = vec![polys[0].clone()];
    for p in &polys[1..] {
        let subj: Vec<Vec<[f64; 2]>> = acc
            .iter()
            .flat_map(|piece| piece.iter().map(|c| close_ring(c)))
            .collect();
        let clip: Vec<Vec<[f64; 2]>> = p.iter().map(|c| close_ring(c)).collect();
        acc = overlay_shapes(&subj, &clip, OverlayRule::Union);
    }
    acc
}

/// Slice a closed polygon with a (possibly infinite) line. Returns the
/// pieces on each side.
pub fn slice_closed_by_line(poly: &Polygon, a: [f32; 2], b: [f32; 2]) -> TrimPolys {
    slice_closed_by_path(poly, &[a, b])
}

/// Slice a closed polygon (holes kept) along the whole of an open
/// polyline. A path that enters without crossing leaves the polygon whole.
pub fn slice_closed_by_path(poly: &Polygon, path: &[[f32; 2]]) -> TrimPolys {
    if poly.is_empty() || poly[0].len() < 3 || path.len() < 2 {
        return Vec::new();
    }
    let rings: Vec<Vec<[f64; 2]>> = poly
        .iter()
        .filter(|r| r.len() >= 3)
        .map(|r| close_ring(r))
        .collect();
    let string: Vec<[f64; 2]> = path.iter().copied().map(to_f64).collect();
    let raw = rings.slice_by(&string, FillRule::EvenOdd);
    let mut out = shapes_from_overlay(raw);
    let mut sources = poly.clone();
    // Each span on its own: the cleaner closes any source of three points.
    sources.extend(path.windows(2).map(|w| w.to_vec()));
    crate::clean::clean_boolean_result(&mut out, &sources);
    out
}

fn cutter_as_path(c: &Cutter) -> Option<Vec<[f32; 2]>> {
    match c {
        Cutter::Infinite { origin, dir } => Some(vec![
            [origin[0] - dir[0] * INFINITE, origin[1] - dir[1] * INFINITE],
            [origin[0] + dir[0] * INFINITE, origin[1] + dir[1] * INFINITE],
        ]),
        Cutter::Open(pts) if pts.len() >= 2 => Some(pts.clone()),
        _ => None,
    }
}

fn cutter_as_closed(c: &Cutter) -> Option<Polygon> {
    match c {
        Cutter::Closed(p) => Some(p.clone()),
        _ => None,
    }
}

/// Split an open polyline at every intersection with `cutters`.
pub fn split_open_at_cutters(target: &[[f32; 2]], cutters: &[Cutter]) -> Vec<Vec<[f32; 2]>> {
    if target.len() < 2 {
        return Vec::new();
    }
    let mut cuts: Vec<(usize, f32, [f32; 2])> = Vec::new();
    for cutter in cutters {
        let segs = cutter_segments(cutter);
        for (si, pair) in target.windows(2).enumerate() {
            let a = pair[0];
            let b = pair[1];
            for (c, d) in &segs {
                if let Some((t, p)) = seg_intersect(a, b, *c, *d) {
                    // Internal target vertices are valid cuts too. Attribute
                    // them to the preceding segment so adjacent segments do
                    // not produce duplicate zero-length pieces.
                    if t > 1e-4 && (t < 1.0 - 1e-4 || si + 1 < target.len() - 1) {
                        cuts.push((si, t, p));
                    }
                }
            }
        }
    }
    cuts.sort_by(|x, y| {
        x.0.cmp(&y.0)
            .then(x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    // Dedup near-identical parameters.
    let mut uniq = Vec::new();
    for c in cuts {
        if uniq
            .last()
            .is_some_and(|p: &(usize, f32, [f32; 2])| p.0 == c.0 && (p.1 - c.1).abs() < 1e-4)
        {
            continue;
        }
        uniq.push(c);
    }
    if uniq.is_empty() {
        return vec![target.to_vec()];
    }
    let mut spans = Vec::new();
    let mut cur = vec![target[0]];
    let mut cut_i = 0;
    for si in 0..target.len() - 1 {
        let a = target[si];
        let b = target[si + 1];
        while cut_i < uniq.len() && uniq[cut_i].0 == si {
            let p = uniq[cut_i].2;
            cur.push(p);
            if cur.len() >= 2 {
                spans.push(std::mem::take(&mut cur));
            }
            cur = vec![p];
            cut_i += 1;
        }
        let _ = a;
        if cur.last().copied() != Some(b) {
            cur.push(b);
        }
    }
    if cur.len() >= 2 {
        spans.push(cur);
    }
    spans
}

fn cutter_segments(c: &Cutter) -> Vec<([f32; 2], [f32; 2])> {
    match c {
        Cutter::Open(pts) => pts.windows(2).map(|w| (w[0], w[1])).collect(),
        Cutter::Infinite { origin, dir } => {
            let a = [origin[0] - dir[0] * INFINITE, origin[1] - dir[1] * INFINITE];
            let b = [origin[0] + dir[0] * INFINITE, origin[1] + dir[1] * INFINITE];
            vec![(a, b)]
        }
        Cutter::Closed(poly) => {
            let mut segs = Vec::new();
            for ring in poly {
                if ring.len() < 2 {
                    continue;
                }
                for w in ring.windows(2) {
                    segs.push((w[0], w[1]));
                }
                if let (Some(&f), Some(&l)) = (ring.first(), ring.last()) {
                    segs.push((l, f));
                }
            }
            segs
        }
    }
}

/// Segment intersection. `t` is the parameter on AB in 0..=1.
fn seg_intersect(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> Option<(f32, [f32; 2])> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < EPS {
        return None;
    }
    let qp = [c[0] - a[0], c[1] - a[1]];
    let t = (qp[0] * s[1] - qp[1] * s[0]) / den;
    let u = (qp[0] * r[1] - qp[1] * r[0]) / den;
    let span = -1e-5..=1.0 + 1e-5;
    if !span.contains(&t) || !span.contains(&u) {
        return None;
    }
    Some((t.clamp(0.0, 1.0), [a[0] + t * r[0], a[1] + t * r[1]]))
}

/// Closest span to `p` (screen/world — same space as the polylines).
pub fn closest_polyline_span(spans: &[Vec<[f32; 2]>], p: [f32; 2]) -> Option<SpanHit> {
    let mut best: Option<SpanHit> = None;
    for (i, span) in spans.iter().enumerate() {
        let d = dist_to_polyline(span, p);
        if best.is_none_or(|b| d < b.dist) {
            best = Some(SpanHit { span: i, dist: d });
        }
    }
    best
}

fn dist_to_polyline(pts: &[[f32; 2]], p: [f32; 2]) -> f32 {
    let mut best = f32::INFINITY;
    for w in pts.windows(2) {
        best = best.min(dist_to_seg(w[0], w[1], p));
    }
    best
}

fn dist_to_seg(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let ab2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if ab2 < EPS {
        0.0
    } else {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / ab2).clamp(0.0, 1.0)
    };
    let q = [a[0] + t * ab[0], a[1] + t * ab[1]];
    let dx = p[0] - q[0];
    let dy = p[1] - q[1];
    (dx * dx + dy * dy).sqrt()
}

/// Extend the start (`from_start`) or end of an open polyline along its
/// end tangent until it hits a cutter. Returns the new polyline.
pub fn extend_polyline_end(
    pts: &[[f32; 2]],
    from_start: bool,
    cutters: &[Cutter],
) -> Option<Vec<[f32; 2]>> {
    if pts.len() < 2 {
        return None;
    }
    let (origin, dir) = if from_start {
        let a = pts[0];
        let b = pts[1];
        let d = [a[0] - b[0], a[1] - b[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if len < EPS {
            return None;
        }
        (a, [d[0] / len, d[1] / len])
    } else {
        let a = pts[pts.len() - 1];
        let b = pts[pts.len() - 2];
        let d = [a[0] - b[0], a[1] - b[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if len < EPS {
            return None;
        }
        (a, [d[0] / len, d[1] / len])
    };
    let ray_b = [origin[0] + dir[0] * INFINITE, origin[1] + dir[1] * INFINITE];
    let mut best: Option<(f32, [f32; 2])> = None;
    for cutter in cutters {
        for (c, d) in cutter_segments(cutter) {
            if let Some((t, p)) = seg_intersect(origin, ray_b, c, d) {
                if t > 1e-4 && best.is_none_or(|(bt, _)| t < bt) {
                    best = Some((t, p));
                }
            }
        }
    }
    let (_, hit) = best?;
    let mut out = pts.to_vec();
    if from_start {
        out.insert(0, hit);
    } else {
        out.push(hit);
    }
    Some(out)
}

/// Open and infinite cutters as paths; closed cutters as regions.
fn sort_cutters(cutters: &[Cutter]) -> (Vec<Vec<[f32; 2]>>, Vec<Polygon>) {
    let mut paths = Vec::new();
    let mut areas = Vec::new();
    for c in cutters {
        if let Some(path) = cutter_as_path(c) {
            paths.push(path);
        } else if let Some(p) = cutter_as_closed(c) {
            areas.push(p);
        }
    }
    (paths, areas)
}

/// Every piece of `target` after slicing along each open path in turn.
fn slice_by_paths(target: &Polygon, paths: &[Vec<[f32; 2]>]) -> TrimPolys {
    let mut pieces: TrimPolys = vec![target.clone()];
    for path in paths {
        let mut next = Vec::new();
        for piece in pieces {
            let sliced = slice_closed_by_path(&piece, path);
            if sliced.is_empty() {
                next.push(piece);
            } else {
                next.extend(sliced);
            }
        }
        pieces = next;
    }
    pieces
}

/// Closed-target trim: click identifies the arrangement face inside the
/// target; that face is subtracted. Open cutters slice along their whole
/// path; area cutters boolean.
pub fn trim_closed_at_click(
    target: &Polygon,
    cutters: &[Cutter],
    click: [f32; 2],
) -> Option<TrimPolys> {
    if !point_in_polygon(target, click) {
        return None;
    }
    let (path_cutters, area_cutters) = sort_cutters(cutters);
    let mut pieces = slice_by_paths(target, &path_cutters);

    if !area_cutters.is_empty() {
        let face = pieces.into_iter().find(|p| point_in_polygon(p, click))?;
        let region = clicked_area_region(&face, &area_cutters, click)?;
        // XOR, not Difference: the clicked face is already a subset of the
        // target (and may itself be holed). T ⊕ (T−C) = T∩C.
        pieces = boolean_xor(target, &region);
        pieces.retain(|p| !point_in_polygon(p, click));
    } else {
        pieces.retain(|p| !point_in_polygon(p, click));
    }

    Some(pieces)
}

/// Closed-target split: every arrangement face is kept. Open cutters slice
/// along their whole path; area cutters partition into inside ∪ outside.
/// Returns `None` when the cutters do not actually divide the target.
pub fn split_closed(target: &Polygon, cutters: &[Cutter]) -> Option<TrimPolys> {
    if target.is_empty() || target[0].len() < 3 {
        return None;
    }
    let (path_cutters, area_cutters) = sort_cutters(cutters);
    if path_cutters.is_empty() && area_cutters.is_empty() {
        return None;
    }

    let mut pieces = slice_by_paths(target, &path_cutters);
    for c in &area_cutters {
        let mut next = Vec::new();
        for piece in pieces {
            let inside = boolean_intersection(&piece, c);
            let outside = boolean_difference(&piece, c);
            if inside.is_empty() && outside.is_empty() {
                next.push(piece);
            } else {
                next.extend(inside);
                next.extend(outside);
            }
        }
        pieces = next;
    }
    pieces.retain(|p| !p.is_empty() && p[0].len() >= 3);
    if pieces.len() < 2 {
        None
    } else {
        Some(pieces)
    }
}

/// The arrangement face of `target` vs area cutters that contains `click`.
fn clicked_area_region(target: &Polygon, cutters: &[Polygon], click: [f32; 2]) -> Option<Polygon> {
    let mut region = target.clone();
    for c in cutters {
        let inside = point_in_polygon(c, click);
        let next = if inside {
            boolean_intersection(&region, c)
        } else {
            boolean_difference(&region, c)
        };
        region = next.into_iter().find(|p| point_in_polygon(p, click))?;
    }
    Some(region)
}

#[cfg(test)]
mod tests;
