//! Stroking: mesh, outline, bounds, and subpath extraction.

use kurbo::{BezPath, PathEl};

use crate::dash::dash_on_runs;
use crate::flatten::flatten;
use crate::geom::{from_kurbo, half_width_at, is_finite_pt, to_kurbo, EPS};
use crate::mesh::tessellate_run;
use crate::{InkMesh, StrokeStyle};

pub(crate) fn valid_style(style: &StrokeStyle) -> bool {
    style.width.is_finite() && style.width > 0.0
}

/// Tessellate a stroked path into a feathered AA mesh.
pub fn stroke_mesh(path: &BezPath, style: &StrokeStyle, feather: f32, tolerance: f64) -> InkMesh {
    if !valid_style(style) || !feather.is_finite() || feather < 0.0 || tolerance <= 0.0 {
        return InkMesh::default();
    }
    let mut mesh = InkMesh::default();
    mesh.vertices.reserve(256);
    mesh.indices.reserve(512);

    for sub in subpaths(path, tolerance) {
        if sub.points.len() < 2 {
            continue;
        }

        if sub.closed && style.dash.is_none() {
            tessellate_run(
                &mut mesh,
                &sub.points,
                style,
                feather,
                true,
                style.cap,
                style.cap,
            );
            continue;
        }

        let poly = if sub.closed {
            let mut pts = sub.points.clone();
            if pts.len() >= 2 {
                pts.push(pts[0]);
            }
            pts
        } else {
            sub.points.clone()
        };

        let runs: Vec<Vec<[f32; 2]>> = if let Some((ref pattern, phase)) = style.dash {
            dash_on_runs(&poly, pattern, phase)
        } else {
            vec![poly]
        };

        for run in runs {
            if run.len() < 2 {
                continue;
            }
            tessellate_run(&mut mesh, &run, style, feather, false, style.cap, style.cap);
        }
    }

    mesh
}

struct SubPath {
    points: Vec<[f32; 2]>,
    closed: bool,
}

fn subpaths(path: &BezPath, tolerance: f64) -> Vec<SubPath> {
    if path.elements().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut chunk = BezPath::new();
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                if !chunk.elements().is_empty() {
                    out.push(finish_chunk(&chunk, tolerance));
                    chunk = BezPath::new();
                }
                chunk.move_to(*p);
            }
            PathEl::LineTo(p) => chunk.line_to(*p),
            PathEl::QuadTo(p1, p2) => chunk.quad_to(*p1, *p2),
            PathEl::CurveTo(p1, p2, p3) => chunk.curve_to(*p1, *p2, *p3),
            PathEl::ClosePath => {
                chunk.close_path();
                out.push(finish_chunk(&chunk, tolerance));
                chunk = BezPath::new();
            }
        }
    }
    if !chunk.elements().is_empty() {
        out.push(finish_chunk(&chunk, tolerance));
    }
    out
}

fn finish_chunk(chunk: &BezPath, tolerance: f64) -> SubPath {
    let closed = chunk
        .elements()
        .last()
        .map(|e| matches!(e, PathEl::ClosePath))
        .unwrap_or(false);
    let mut points = flatten(chunk, tolerance);
    if closed && points.len() >= 2 && dist2(points[0], *points.last().unwrap()) < EPS * EPS {
        points.pop();
    }
    SubPath { points, closed }
}

#[inline]
fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

/// The stroked region as a closed outline path (for SVG export).
pub fn stroke_outline(path: &BezPath, style: &StrokeStyle, tolerance: f64) -> BezPath {
    if !valid_style(style) || tolerance <= 0.0 {
        return BezPath::new();
    }
    let mut outline = BezPath::new();
    for sub in subpaths(path, tolerance) {
        if sub.points.len() < 2 {
            continue;
        }
        append_run_outline(&mut outline, &sub.points, style, sub.closed);
    }
    outline
}

fn append_run_outline(out: &mut BezPath, points: &[[f32; 2]], style: &StrokeStyle, closed: bool) {
    use crate::geom::{add, cumulative_arclength, normalize, perp_left, scale, sub};

    let arc = cumulative_arclength(points);
    let total = *arc.last().unwrap_or(&0.0);
    if total <= EPS {
        return;
    }

    let mut left = Vec::new();
    let mut right = Vec::new();
    let n = points.len();

    let emit_at = |i: usize, left: &mut Vec<[f32; 2]>, right: &mut Vec<[f32; 2]>| {
        let t_frac = arc[i] / total;
        let half = half_width_at(style, t_frac);
        let (t_in, t_out) = if closed {
            let prev = points[(i + n - 1) % n];
            let next = points[(i + 1) % n];
            (
                normalize(sub(points[i], prev)).unwrap_or([1.0, 0.0]),
                normalize(sub(next, points[i])).unwrap_or([1.0, 0.0]),
            )
        } else if i == 0 {
            (
                normalize(sub(points[1], points[0])).unwrap_or([1.0, 0.0]),
                normalize(sub(points[1], points[0])).unwrap_or([1.0, 0.0]),
            )
        } else if i == n - 1 {
            (
                normalize(sub(points[n - 1], points[n - 2])).unwrap_or([1.0, 0.0]),
                normalize(sub(points[n - 1], points[n - 2])).unwrap_or([1.0, 0.0]),
            )
        } else {
            (
                normalize(sub(points[i], points[i - 1])).unwrap_or([1.0, 0.0]),
                normalize(sub(points[i + 1], points[i])).unwrap_or([1.0, 0.0]),
            )
        };
        let n_avg = normalize(add(perp_left(t_in), perp_left(t_out))).unwrap_or(perp_left(t_out));
        let p = points[i];
        left.push(add(p, scale(n_avg, half)));
        right.push(add(p, scale(n_avg, -half)));
    };

    for i in 0..n {
        emit_at(i, &mut left, &mut right);
    }

    if left.is_empty() {
        return;
    }

    out.move_to(to_kurbo(left[0]));
    for p in &left[1..] {
        out.line_to(to_kurbo(*p));
    }
    for p in right.iter().rev() {
        out.line_to(to_kurbo(*p));
    }
    out.close_path();
}

/// Bounding box of the stroked path including width and feather.
pub fn stroke_bounds(
    path: &BezPath,
    style: &StrokeStyle,
    feather: f32,
) -> Option<([f32; 2], [f32; 2])> {
    if path.elements().is_empty() || !valid_style(style) || !feather.is_finite() {
        return None;
    }
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    let mut any = false;
    for el in path.elements() {
        if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
            expand_pt(&mut min, &mut max, &mut any, from_kurbo(*p), style, feather);
        } else if let PathEl::QuadTo(_, p2) = el {
            expand_pt(
                &mut min,
                &mut max,
                &mut any,
                from_kurbo(*p2),
                style,
                feather,
            );
        } else if let PathEl::CurveTo(_, _, p3) = el {
            expand_pt(
                &mut min,
                &mut max,
                &mut any,
                from_kurbo(*p3),
                style,
                feather,
            );
        }
    }
    if !any {
        return None;
    }
    Some((min, max))
}

fn expand_pt(
    min: &mut [f32; 2],
    max: &mut [f32; 2],
    any: &mut bool,
    p: [f32; 2],
    style: &StrokeStyle,
    feather: f32,
) {
    if !is_finite_pt(p) {
        return;
    }
    *any = true;
    let pad = style.width * 0.5 + feather * 0.5;
    min[0] = min[0].min(p[0] - pad);
    min[1] = min[1].min(p[1] - pad);
    max[0] = max[0].max(p[0] + pad);
    max[1] = max[1].max(p[1] + pad);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cap, Join, StrokeStyle};

    fn line_path(x0: f32, y0: f32, x1: f32, y1: f32) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((x0 as f64, y0 as f64));
        p.line_to((x1 as f64, y1 as f64));
        p
    }

    #[test]
    fn horizontal_line_mesh_symmetric_alphas() {
        let path = line_path(0.0, 0.0, 100.0, 0.0);
        let style = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let mesh = stroke_mesh(&path, &style, 1.0, 0.01);
        assert!(!mesh.vertices.is_empty());
        for v in &mesh.vertices {
            assert!(v.alpha == 0.0 || v.alpha == 1.0);
            assert!((v.pos[1].abs() - 0.0).abs() <= 2.5 || v.pos[1].abs() <= 3.5);
        }
        let min_x = mesh
            .vertices
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = mesh
            .vertices
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - 0.0).abs() < 0.1);
        assert!((max_x - 100.0).abs() < 0.1);
    }

    #[test]
    fn taper_half_widths_decrease() {
        let path = line_path(0.0, 0.0, 100.0, 0.0);
        let style = StrokeStyle {
            width: 10.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: Some((1.0, 0.0)),
            dash: None,
        };
        let mesh = stroke_mesh(&path, &style, 0.5, 0.01);
        let mut samples: Vec<(f32, f32)> = mesh
            .vertices
            .iter()
            .filter(|v| v.alpha == 1.0)
            .map(|v| (v.pos[0], v.pos[1].abs()))
            .collect();
        samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut last_x = -1.0f32;
        let mut last_half = f32::MAX;
        for (x, y) in samples {
            if (x - last_x).abs() < 2.0 {
                continue;
            }
            last_x = x;
            if y > last_half + 0.01 {
                panic!("half width grew at x={x}: {y} vs {last_half}");
            }
            last_half = y;
        }
        assert!(last_x > 50.0);
    }

    #[test]
    fn round_cap_more_vertices_than_butt() {
        let path = line_path(0.0, 0.0, 50.0, 0.0);
        let butt = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let round = StrokeStyle {
            cap: Cap::Round,
            ..butt.clone()
        };
        let m_butt = stroke_mesh(&path, &butt, 1.0, 0.01);
        let m_round = stroke_mesh(&path, &round, 1.0, 0.01);
        assert!(m_round.vertices.len() > m_butt.vertices.len());
    }

    #[test]
    fn closed_triangle_no_extra_caps_vs_open() {
        let mut tri = BezPath::new();
        tri.move_to((0.0, 0.0));
        tri.line_to((40.0, 0.0));
        tri.line_to((20.0, 35.0));
        tri.close_path();
        let style = StrokeStyle {
            width: 3.0,
            cap: Cap::Round,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let closed_mesh = stroke_mesh(&tri, &style, 0.5, 0.01);
        assert!(!closed_mesh.vertices.is_empty());

        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((40.0, 0.0));
        open.line_to((20.0, 35.0));
        let open_mesh = stroke_mesh(&open, &style, 0.5, 0.01);
        assert_ne!(closed_mesh.vertices.len(), open_mesh.vertices.len());
    }

    #[test]
    fn degenerate_empty_no_panic() {
        let empty = BezPath::new();
        let style = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        assert!(stroke_mesh(&empty, &style, 1.0, 0.01).vertices.is_empty());
        assert!(stroke_bounds(&empty, &style, 1.0).is_none());
        let mut one = BezPath::new();
        one.move_to((0.0, 0.0));
        assert!(stroke_mesh(&one, &style, 1.0, 0.01).vertices.is_empty());
        let zero_w = StrokeStyle {
            width: 0.0,
            ..style.clone()
        };
        let path = line_path(0.0, 0.0, 10.0, 0.0);
        assert!(stroke_mesh(&path, &zero_w, 1.0, 0.01).vertices.is_empty());
        let nan_w = StrokeStyle {
            width: f32::NAN,
            ..style.clone()
        };
        assert!(stroke_mesh(&path, &nan_w, 1.0, 0.01).vertices.is_empty());
        assert!(!crate::hit_stroke(&path, &nan_w, [5.0, 0.0], 0.0));
    }

    #[test]
    fn bounds_match_width_and_feather() {
        let path = line_path(0.0, 0.0, 100.0, 0.0);
        let style = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let feather = 1.0;
        let (min, max) = stroke_bounds(&path, &style, feather).unwrap();
        let pad = style.width * 0.5 + feather * 0.5;
        assert!((min[1] - (-pad)).abs() < 0.01);
        assert!((max[1] - pad).abs() < 0.01);
    }
}
