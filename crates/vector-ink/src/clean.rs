//! Snap boolean output back onto the input geometry.
//!
//! Every float overlay (i_overlay included) quantizes through integer
//! space. Uncut edges then come back a few ulps off, which reads as a
//! warped border. CAD practice — Rhino, Clipper cleanup, polybool — is
//! to treat the overlay as a topology oracle and restore coordinates
//! that already existed on the inputs: source vertices, source H/V
//! lines, then projection onto source edges. Collinear mid-edge
//! vertices are dropped.

use crate::trim::TrimPolys;

/// World-unit snap. Dense enough to catch overlay quantization, loose
/// enough that a 48-gon ellipse (chord ~2 world units at r=20) is not
/// collapsed onto a neighbor.
pub const SNAP: f32 = 0.05;

pub fn clean_boolean_result(result: &mut TrimPolys, sources: &[Vec<[f32; 2]>]) {
    if sources.is_empty() {
        return;
    }
    let edges = collect_edges(sources);
    let snap = snap_eps(&edges);
    let verts = collect_verts(sources);
    let (h_lines, v_lines) = collect_axis_lines(&edges, snap);
    for piece in result.iter_mut() {
        for ring in piece.iter_mut() {
            for p in ring.iter_mut() {
                *p = snap_point(*p, &verts, &h_lines, &v_lines, &edges, snap);
            }
            *ring = collapse_collinear(ring, snap);
        }
        piece.retain(|r| r.len() >= 3);
    }
    result.retain(|p| !p.is_empty());
}

fn snap_eps(edges: &[([f32; 2], [f32; 2])]) -> f32 {
    let mut min_e = SNAP;
    for (a, b) in edges {
        let l = dist2(*a, *b).sqrt();
        if l > 1e-4 {
            min_e = min_e.min(0.4 * l);
        }
    }
    min_e.clamp(1e-4, SNAP)
}

fn collect_verts(sources: &[Vec<[f32; 2]>]) -> Vec<[f32; 2]> {
    sources.iter().flatten().copied().collect()
}

fn collect_edges(sources: &[Vec<[f32; 2]>]) -> Vec<([f32; 2], [f32; 2])> {
    let mut edges = Vec::new();
    for ring in sources {
        if ring.len() < 2 {
            continue;
        }
        for w in ring.windows(2) {
            edges.push((w[0], w[1]));
        }
        if ring.len() >= 3 {
            edges.push((ring[ring.len() - 1], ring[0]));
        }
    }
    edges
}

fn collect_axis_lines(edges: &[([f32; 2], [f32; 2])], snap: f32) -> (Vec<f32>, Vec<f32>) {
    let mut h = Vec::new();
    let mut v = Vec::new();
    for (a, b) in edges {
        if (a[1] - b[1]).abs() < snap {
            push_unique(&mut h, 0.5 * (a[1] + b[1]), snap);
        }
        if (a[0] - b[0]).abs() < snap {
            push_unique(&mut v, 0.5 * (a[0] + b[0]), snap);
        }
    }
    (h, v)
}

fn push_unique(lines: &mut Vec<f32>, x: f32, snap: f32) {
    if !lines.iter().any(|y| (x - *y).abs() < snap) {
        lines.push(x);
    }
}

fn snap_point(
    p: [f32; 2],
    verts: &[[f32; 2]],
    h_lines: &[f32],
    v_lines: &[f32],
    edges: &[([f32; 2], [f32; 2])],
    snap: f32,
) -> [f32; 2] {
    let mut q = p;
    if let Some(x) = nearest_line(q[0], v_lines, snap) {
        q[0] = x;
    }
    if let Some(y) = nearest_line(q[1], h_lines, snap) {
        q[1] = y;
    }
    let snap2 = snap * snap;
    for v in verts {
        if dist2(q, *v) < snap2 {
            return *v;
        }
    }
    let mut best: Option<([f32; 2], f32)> = None;
    for (a, b) in edges {
        let (proj, d2) = project_seg(q, *a, *b);
        if d2 < snap2 && best.is_none_or(|(_, bd)| d2 < bd) {
            best = Some((proj, d2));
        }
    }
    if let Some((proj, _)) = best {
        return proj;
    }
    q
}

fn nearest_line(x: f32, lines: &[f32], snap: f32) -> Option<f32> {
    let mut best: Option<(f32, f32)> = None;
    for &l in lines {
        let d = (x - l).abs();
        if d < snap && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((l, d));
        }
    }
    best.map(|(l, _)| l)
}

fn collapse_collinear(ring: &[[f32; 2]], snap: f32) -> Vec<[f32; 2]> {
    let n = ring.len();
    if n < 3 {
        return ring.to_vec();
    }
    let snap2 = snap * snap;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = ring[(i + n - 1) % n];
        let b = ring[i];
        let c = ring[(i + 1) % n];
        if dist2(a, c) > snap2 && point_to_seg_dist2(b, a, c) < snap2 {
            continue;
        }
        if out.last().is_some_and(|p| dist2(*p, b) < snap2) {
            continue;
        }
        out.push(b);
    }
    if out.len() >= 2 && dist2(out[0], *out.last().unwrap()) < snap2 {
        out.pop();
    }
    out
}

fn project_seg(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> ([f32; 2], f32) {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let den = ab[0] * ab[0] + ab[1] * ab[1];
    if den < 1e-12 {
        return (a, dist2(p, a));
    }
    let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / den).clamp(0.0, 1.0);
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    (q, dist2(p, q))
}

fn point_to_seg_dist2(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    project_seg(p, a, b).1
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_restores_a_warped_rect_edge() {
        let sources = vec![vec![[0.0, 0.0], [80.0, 0.0], [80.0, 80.0], [0.0, 80.0]]];
        let mut out = vec![vec![vec![
            [0.0004, -0.0003],
            [80.001, 0.0002],
            [79.999, 80.0005],
            [0.0001, 79.999],
        ]]];
        clean_boolean_result(&mut out, &sources);
        let ring = &out[0][0];
        assert_eq!(ring.len(), 4);
        for p in ring {
            assert!(
                (p[0] == 0.0 || p[0] == 80.0) && (p[1] == 0.0 || p[1] == 80.0),
                "unclean vertex {p:?}"
            );
        }
    }

    #[test]
    fn collinear_midpoints_drop() {
        let sources = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]];
        let mut out = vec![vec![vec![
            [0.0, 0.0],
            [5.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ]]];
        clean_boolean_result(&mut out, &sources);
        assert_eq!(out[0][0].len(), 4);
    }
}
