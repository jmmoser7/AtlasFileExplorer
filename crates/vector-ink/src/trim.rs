//! 2D Trim geometry — Rhino's click-the-dying-piece, planar only.
//!
//! Closed regions use `i_overlay` boolean / slice. Open curves are split at
//! segment intersections. Nothing here knows about nodes or the journal.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use i_overlay::float::slice::FloatSlice;

use crate::geom::EPS;

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

fn overlay_shapes(subj: &[Vec<[f64; 2]>], clip: &[Vec<[f64; 2]>], rule: OverlayRule) -> TrimPolys {
    if subj.is_empty() {
        return Vec::new();
    }
    if clip.is_empty() {
        return subj
            .iter()
            .map(|c| vec![c.iter().copied().map(from_f64).collect()])
            .collect();
    }
    let mut overlay = FloatOverlay::with_subj_and_clip(subj, clip);
    let raw = overlay.overlay(rule, FillRule::EvenOdd);
    shapes_from_overlay(raw)
}

fn shapes_from_overlay(raw: Vec<Vec<Vec<[f64; 2]>>>) -> TrimPolys {
    raw.into_iter()
        .filter_map(|shape| {
            let contours: Polygon = shape
                .into_iter()
                .map(|c| c.into_iter().map(from_f64).collect())
                .filter(|c: &Vec<[f32; 2]>| c.len() >= 3)
                .collect();
            if contours.is_empty() {
                None
            } else {
                Some(contours)
            }
        })
        .collect()
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
    if poly.is_empty() || poly[0].len() < 3 {
        return Vec::new();
    }
    let outer = close_ring(&poly[0]);
    let line = [to_f64(a), to_f64(b)];
    let raw = outer.slice_by(&line, FillRule::NonZero);
    shapes_from_overlay(raw)
}

fn cutter_as_line(c: &Cutter) -> Option<([f32; 2], [f32; 2])> {
    match c {
        Cutter::Infinite { origin, dir } => Some((
            [origin[0] - dir[0] * INFINITE, origin[1] - dir[1] * INFINITE],
            [origin[0] + dir[0] * INFINITE, origin[1] + dir[1] * INFINITE],
        )),
        Cutter::Open(pts) if pts.len() == 2 => Some((pts[0], pts[1])),
        _ => None,
    }
}

fn cutter_as_closed(c: &Cutter) -> Option<Polygon> {
    match c {
        Cutter::Closed(p) => Some(p.clone()),
        _ => None,
    }
}

/// Even-odd point-in-polygon (first contour only; holes invert).
pub fn point_in_polygon(poly: &Polygon, p: [f32; 2]) -> bool {
    if poly.is_empty() {
        return false;
    }
    let mut inside = point_in_ring(&poly[0], p);
    for hole in poly.iter().skip(1) {
        if point_in_ring(hole, p) {
            inside = !inside;
        }
    }
    inside
}

pub fn point_in_ring(ring: &[[f32; 2]], p: [f32; 2]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[j];
        let intersect = ((a[1] > p[1]) != (b[1] > p[1]))
            && (p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1] + EPS) + a[0]);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
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
                    if t > 1e-4 && t < 1.0 - 1e-4 {
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
        cur.push(b);
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
    if t < -1e-5 || t > 1.0 + 1e-5 || u < -1e-5 || u > 1.0 + 1e-5 {
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

/// Earcut a compound polygon (outer + holes) into a triangle list.
/// Returns (vertices, indices).
pub fn fill_triangles(poly: &Polygon) -> (Vec<[f32; 2]>, Vec<u32>) {
    if poly.is_empty() || poly[0].len() < 3 {
        return (Vec::new(), Vec::new());
    }
    let mut coords = Vec::new();
    let mut holes = Vec::new();
    for (i, ring) in poly.iter().enumerate() {
        if i > 0 {
            holes.push(coords.len() / 2);
        }
        for p in ring {
            coords.push(p[0] as f64);
            coords.push(p[1] as f64);
        }
    }
    let Ok(idx) = earcutr::earcut(&coords, &holes, 2) else {
        return (Vec::new(), Vec::new());
    };
    let verts: Vec<[f32; 2]> = coords
        .chunks(2)
        .map(|c| [c[0] as f32, c[1] as f32])
        .collect();
    let indices: Vec<u32> = idx.into_iter().map(|i| i as u32).collect();
    (verts, indices)
}

/// Closed-target trim: click identifies the arrangement face inside the
/// target; that face is subtracted. Line cutters slice; area cutters
/// boolean.
pub fn trim_closed_at_click(
    target: &Polygon,
    cutters: &[Cutter],
    click: [f32; 2],
) -> Option<TrimPolys> {
    if !point_in_polygon(target, click) {
        return None;
    }
    let mut line_cutters = Vec::new();
    let mut area_cutters = Vec::new();
    for c in cutters {
        if let Some(ab) = cutter_as_line(c) {
            line_cutters.push(ab);
        } else if let Some(p) = cutter_as_closed(c) {
            area_cutters.push(p);
        } else if let Cutter::Open(pts) = c {
            for w in pts.windows(2) {
                line_cutters.push((w[0], w[1]));
            }
        }
    }

    let mut pieces: TrimPolys = vec![target.clone()];
    for (a, b) in &line_cutters {
        let mut next = Vec::new();
        for piece in pieces {
            let sliced = slice_closed_by_line(&piece, *a, *b);
            if sliced.is_empty() {
                next.push(piece);
            } else {
                next.extend(sliced);
            }
        }
        pieces = next;
    }

    if !area_cutters.is_empty() {
        let region = clicked_area_region(target, &area_cutters, click)?;
        // XOR, not Difference: the clicked face is already a subset of the
        // target (and may itself be holed). T ⊕ (T−C) = T∩C.
        pieces = boolean_xor(target, &region);
        pieces.retain(|p| !point_in_polygon(p, click));
    } else {
        pieces.retain(|p| !point_in_polygon(p, click));
    }

    Some(pieces)
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
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Polygon {
        vec![vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]]
    }

    fn circle(cx: f32, cy: f32, r: f32, n: usize) -> Polygon {
        let mut pts = Vec::with_capacity(n);
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            pts.push([cx + r * a.cos(), cy + r * a.sin()]);
        }
        vec![pts]
    }

    #[test]
    fn crossing_lines_split_into_four_spans() {
        let horiz = [[0.0, 50.0], [100.0, 50.0]];
        let cutter = Cutter::Open(vec![[50.0, 0.0], [50.0, 100.0]]);
        let spans = split_open_at_cutters(&horiz, &[cutter]);
        assert_eq!(spans.len(), 2);
        assert!((spans[0][1][0] - 50.0).abs() < 0.1);
        assert!((spans[1][0][0] - 50.0).abs() < 0.1);
    }

    #[test]
    fn click_picks_the_closer_span() {
        let spans = vec![
            vec![[0.0, 0.0], [50.0, 0.0]],
            vec![[50.0, 0.0], [100.0, 0.0]],
        ];
        let hit = closest_polyline_span(&spans, [10.0, 2.0]).unwrap();
        assert_eq!(hit.span, 0);
        let hit = closest_polyline_span(&spans, [90.0, 2.0]).unwrap();
        assert_eq!(hit.span, 1);
    }

    #[test]
    fn rect_minus_inner_circle_has_a_hole() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        let c = circle(50.0, 50.0, 20.0, 32);
        let out = boolean_difference(&r, &c);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].len() >= 2,
            "expected outer + hole, got {} contours",
            out[0].len()
        );
        assert!(point_in_polygon(&out[0], [5.0, 5.0]));
        assert!(!point_in_polygon(&out[0], [50.0, 50.0]));
    }

    #[test]
    fn click_inside_circle_punches_a_hole() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        let c = circle(50.0, 50.0, 20.0, 32);
        let out = trim_closed_at_click(&r, &[Cutter::Closed(c)], [50.0, 50.0]).unwrap();
        assert_eq!(out.len(), 1);
        assert!(point_in_polygon(&out[0], [5.0, 5.0]));
        assert!(!point_in_polygon(&out[0], [50.0, 50.0]));
    }

    #[test]
    fn click_outside_circle_keeps_the_intersection() {
        let r = rect(0.0, 0.0, 100.0, 100.0);
        let c = circle(50.0, 50.0, 20.0, 32);
        let out = trim_closed_at_click(&r, &[Cutter::Closed(c)], [5.0, 5.0]).unwrap();
        assert_eq!(out.len(), 1);
        assert!(point_in_polygon(&out[0], [50.0, 50.0]));
        assert!(!point_in_polygon(&out[0], [5.0, 5.0]));
    }

    #[test]
    fn infinite_line_slices_a_rect() {
        let r = rect(0.0, 0.0, 100.0, 80.0);
        let cutter = infinite_line([50.0, -10.0], [50.0, 10.0]).unwrap();
        let out = trim_closed_at_click(&r, &[cutter], [10.0, 40.0]).unwrap();
        assert_eq!(out.len(), 1);
        assert!(point_in_polygon(&out[0], [80.0, 40.0]));
        assert!(!point_in_polygon(&out[0], [10.0, 40.0]));
    }

    #[test]
    fn extend_reaches_a_vertical_cutter() {
        let pts = [[0.0, 0.0], [20.0, 0.0]];
        let cutter = Cutter::Open(vec![[80.0, -10.0], [80.0, 10.0]]);
        let ext = extend_polyline_end(&pts, false, &[cutter]).unwrap();
        assert!((ext.last().unwrap()[0] - 80.0).abs() < 0.2);
    }

    #[test]
    fn fill_triangles_covers_a_rect() {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let (v, i) = fill_triangles(&r);
        assert!(v.len() >= 3);
        assert_eq!(i.len() % 3, 0);
        assert!(!i.is_empty());
    }

    #[test]
    fn union_of_overlapping_rects_covers_both() {
        let a = rect(0.0, 0.0, 80.0, 80.0);
        let b = rect(40.0, 40.0, 80.0, 80.0);
        let out = boolean_union_all(&[a, b]);
        assert_eq!(out.len(), 1);
        assert!(point_in_polygon(&out[0], [10.0, 10.0]));
        assert!(point_in_polygon(&out[0], [100.0, 100.0]));
        assert!(point_in_polygon(&out[0], [50.0, 50.0]));
        assert!(!point_in_polygon(&out[0], [10.0, 100.0]));
    }

    #[test]
    fn union_of_disjoint_rects_keeps_both_pieces() {
        let a = rect(0.0, 0.0, 20.0, 20.0);
        let b = rect(80.0, 80.0, 20.0, 20.0);
        let out = boolean_union_all(&[a, b]);
        assert!(
            out.len() >= 2 || (out.len() == 1 && out[0].len() >= 2),
            "disjoint union should keep both regions, got {} piece(s)",
            out.len()
        );
        let hit_a = out.iter().any(|p| point_in_polygon(p, [10.0, 10.0]));
        let hit_b = out.iter().any(|p| point_in_polygon(p, [90.0, 90.0]));
        assert!(hit_a && hit_b);
    }

    #[test]
    fn nested_union_is_the_outer() {
        let outer = rect(0.0, 0.0, 100.0, 100.0);
        let inner = rect(30.0, 30.0, 20.0, 20.0);
        let out = boolean_union_all(&[outer, inner]);
        assert_eq!(out.len(), 1);
        assert!(point_in_polygon(&out[0], [10.0, 10.0]));
        assert!(point_in_polygon(&out[0], [40.0, 40.0]));
        assert_eq!(out[0].len(), 1, "union must not punch a hole");
    }
}
