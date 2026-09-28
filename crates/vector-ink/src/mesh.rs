//! Feathered stroke mesh tessellation. Each section owns two boundary rails;
//! joins keep the inner intersection fixed while only the outer rail turns.

use crate::geom::{
    add, cumulative_arclength, dot, half_width_at, normalize, perp_left, scale, sub, EPS,
    MITER_LIMIT,
};
use crate::{Cap, InkMesh, InkVertex, Join, StrokeStyle, TintPiece};

#[derive(Clone, Copy)]
struct Edge {
    pos: [f32; 2],
    // Outward offset per unit of stroke expansion (miter normals need not be unit length).
    normal: [f32; 2],
}

#[derive(Clone, Copy)]
struct Station {
    edges: [Edge; 2],
    half: f32,
    /// Index of the input point this section belongs to (its color).
    at: usize,
}

fn section(pos: [f32; 2], normal: [f32; 2], half: f32) -> Station {
    Station {
        edges: [-1.0, 1.0].map(|sign| Edge {
            pos: add(pos, scale(normal, sign * half)),
            normal: scale(normal, sign),
        }),
        half,
        at: 0,
    }
}

/// Tag the sections pushed since `from` with input point `at`.
fn tag(out: &mut [Station], from: usize, at: usize) {
    for station in &mut out[from..] {
        station.at = at;
    }
}

fn arc_steps(radius: f32, angle: f32, tolerance: f64) -> usize {
    let step = 2.0 * (1.0 - (tolerance / radius.max(EPS) as f64).min(1.0)).acos();
    let steps = ((angle.abs() as f64 / step.max(0.0001)).ceil() as usize).max(1);
    // A round tip with one or two facets reads as a spike once the stroke is wide.
    if angle.abs() >= 0.4 {
        steps.max(8)
    } else {
        steps
    }
}

/// `widths`, when present, holds a full width per point and replaces
/// `style.width` there; the taper still scales it. An open run takes
/// `ends[0]` at its start and `ends[1]` at its end.
fn stations(
    points: &[[f32; 2]],
    widths: Option<&[f32]>,
    style: &StrokeStyle,
    ends: [Cap; 2],
    closed: bool,
    tolerance: f64,
    feather: f32,
) -> Vec<Station> {
    if points.len() < 2 || widths.is_some_and(|w| w.len() != points.len()) {
        return Vec::new();
    }
    let lengths = cumulative_arclength(points);
    let total = *lengths.last().unwrap();
    if total <= EPS {
        return Vec::new();
    }
    let n = points.len();
    let half = |i: usize| {
        let styled = half_width_at(style, lengths[i] / total);
        match widths {
            None => styled,
            Some(widths) => styled / (style.width * 0.5) * widths[i].max(0.0) * 0.5,
        }
    };
    let mut out = Vec::with_capacity(n + 16);
    if closed {
        for i in 0..n {
            let incoming = normalize(sub(points[i], points[(i + n - 1) % n])).unwrap_or([1.0, 0.0]);
            let outgoing = normalize(sub(points[(i + 1) % n], points[i])).unwrap_or(incoming);
            let from = out.len();
            push_join(
                &mut out,
                points[i],
                incoming,
                outgoing,
                half(i),
                style.join,
                tolerance,
                feather,
            );
            tag(&mut out, from, i);
        }
    } else {
        let first = normalize(sub(points[1], points[0])).unwrap_or([1.0, 0.0]);
        push_cap(
            &mut out,
            points[0],
            first,
            half(0),
            ends[0],
            true,
            tolerance,
            feather,
        );
        // Keep every adaptive curve sample, including on tapered strokes. The
        // former 64-station resampling discarded curvature and sharp vertices.
        for i in 1..n - 1 {
            let incoming = normalize(sub(points[i], points[i - 1])).unwrap_or(first);
            let outgoing = normalize(sub(points[i + 1], points[i])).unwrap_or(incoming);
            let from = out.len();
            push_join(
                &mut out,
                points[i],
                incoming,
                outgoing,
                half(i),
                style.join,
                tolerance,
                feather,
            );
            tag(&mut out, from, i);
        }
        let last = normalize(sub(points[n - 1], points[n - 2])).unwrap_or(first);
        let from = out.len();
        push_cap(
            &mut out,
            points[n - 1],
            last,
            half(n - 1),
            ends[1],
            false,
            tolerance,
            feather,
        );
        tag(&mut out, from, n - 1);
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_run(
    mesh: &mut InkMesh,
    points: &[[f32; 2]],
    widths: Option<&[f32]>,
    colors: Option<&[[f32; 4]]>,
    style: &StrokeStyle,
    ends: [Cap; 2],
    feather: f32,
    closed: bool,
    tolerance: f64,
) {
    if style.width <= 0.0 || !feather.is_finite() || feather < 0.0 {
        return;
    }
    let colors = colors.filter(|c| c.len() == points.len());
    let (points, widths, colors) =
        limit_chords(points, widths, colors, chord_limit(tolerance, feather));
    let mut sections = stations(
        &points,
        widths.as_deref(),
        style,
        ends,
        closed,
        tolerance,
        feather,
    );
    if sections.len() < 2 {
        return;
    }
    if closed {
        sections.push(sections[0]);
    }
    emit_strip(mesh, &sections, feather, colors.as_deref());
}

/// The quads between consecutive sections of one run, with the colors of
/// their two sections (`stroke_pieces_tinted`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_pieces(
    points: &[[f32; 2]],
    widths: Option<&[f32]>,
    colors: &[[f32; 4]],
    style: &StrokeStyle,
    ends: [Cap; 2],
    closed: bool,
    tolerance: f64,
) -> Vec<TintPiece> {
    if colors.len() != points.len() {
        return Vec::new();
    }
    let mut sections = stations(points, widths, style, ends, closed, tolerance, 0.0);
    if sections.len() < 2 {
        return Vec::new();
    }
    if closed {
        sections.push(sections[0]);
    }
    let center = |s: &Station| scale(add(s.edges[0].pos, s.edges[1].pos), 0.5);
    sections
        .windows(2)
        .map(|pair| {
            let (a, b) = (&pair[0], &pair[1]);
            TintPiece {
                quad: [
                    a.edges[0].pos,
                    b.edges[0].pos,
                    b.edges[1].pos,
                    a.edges[1].pos,
                ],
                from: center(a),
                to: center(b),
                colors: [colors[a.at], colors[b.at]],
            }
        })
        .collect()
}

/// Export consumes the same boundary rails as the mesh, including caps/joins.
/// Closed strokes have two opposite-winding contours, not a bridged seam.
pub(crate) fn run_outline(
    points: &[[f32; 2]],
    widths: Option<&[f32]>,
    style: &StrokeStyle,
    ends: [Cap; 2],
    closed: bool,
    tolerance: f64,
) -> Vec<Vec<[f32; 2]>> {
    let sections = stations(points, widths, style, ends, closed, tolerance, 0.0);
    if sections.len() < 2 {
        return Vec::new();
    }
    let mut first: Vec<_> = sections.iter().map(|s| s.edges[0].pos).collect();
    let second: Vec<_> = sections.iter().rev().map(|s| s.edges[1].pos).collect();
    if closed {
        vec![first, second]
    } else {
        first.extend(second);
        vec![first]
    }
}

#[allow(clippy::too_many_arguments)]
fn push_cap(
    out: &mut Vec<Station>,
    pos: [f32; 2],
    tangent: [f32; 2],
    half: f32,
    cap: Cap,
    start: bool,
    tolerance: f64,
    feather: f32,
) {
    let normal = perp_left(tangent);
    match cap {
        Cap::Butt => out.push(section(pos, normal, half)),
        Cap::Square => {
            let extended = section(
                add(pos, scale(tangent, if start { -half } else { half })),
                normal,
                half,
            );
            let end = section(pos, normal, half);
            if start {
                out.extend([extended, end]);
            } else {
                out.extend([end, extended]);
            }
        }
        Cap::Round => {
            let count = arc_steps(half + feather * 0.5, std::f32::consts::FRAC_PI_2, tolerance);
            for i in 0..=count {
                // Longitudinal sections travel from tip to end (start cap), or
                // end to tip (end cap). No rotating diameters/bow-tie triangles.
                let fraction = if start {
                    i as f32 / count as f32
                } else {
                    1.0 - i as f32 / count as f32
                };
                let angle = fraction * std::f32::consts::FRAC_PI_2;
                let (sin, cos) = if fraction == 0.0 {
                    (0.0, 1.0)
                } else if fraction == 1.0 {
                    (1.0, 0.0)
                } else {
                    angle.sin_cos()
                };
                let axial = scale(tangent, if start { -cos } else { cos });
                let edges = [-1.0, 1.0].map(|sign| {
                    let outward = add(axial, scale(normal, sign * sin));
                    Edge {
                        pos: add(pos, scale(outward, half)),
                        normal: outward,
                    }
                });
                out.push(Station { edges, half, at: 0 });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_join(
    out: &mut Vec<Station>,
    pos: [f32; 2],
    incoming: [f32; 2],
    outgoing: [f32; 2],
    half: f32,
    join: Join,
    tolerance: f64,
    feather: f32,
) {
    let a = perp_left(incoming);
    let b = perp_left(outgoing);
    let cosine = dot(incoming, outgoing).clamp(-1.0, 1.0);
    let cross = incoming[0] * outgoing[1] - incoming[1] * outgoing[0];
    let miter = scale(add(a, b), 1.0 / (1.0 + cosine).max(EPS));
    let miter_length = dot(miter, miter).sqrt();
    if (join == Join::Miter && miter_length <= MITER_LIMIT) || cosine > 0.99999 {
        out.push(section(pos, miter, half));
        return;
    }
    let angle = cross.atan2(cosine);
    let count = if join == Join::Round {
        arc_steps(half + feather * 0.5, angle, tolerance)
    } else {
        1
    };
    // Positive turns have the +normal rail on the inside. Its intersection
    // is shared by every join section, preventing interior overlap/buildup.
    let inner = if cross >= 0.0 { 1 } else { 0 };
    let sign = if inner == 1 { 1.0 } else { -1.0 };
    let inner_normal = scale(miter, sign * MITER_LIMIT / miter_length.max(MITER_LIMIT));
    let inside = Edge {
        pos: add(pos, scale(inner_normal, half)),
        normal: inner_normal,
    };
    for i in 0..=count {
        let (sin, cos) = (angle * i as f32 / count as f32).sin_cos();
        let normal = [a[0] * cos - a[1] * sin, a[0] * sin + a[1] * cos];
        let mut station = section(pos, normal, half);
        station.edges[inner] = inside;
        out.push(station);
    }
}

fn chord_limit(tolerance: f64, feather: f32) -> f32 {
    if feather <= 2.0 {
        return f32::MAX;
    }
    (tolerance as f32 * 8.0).max(0.75)
}

type Chords = (Vec<[f32; 2]>, Option<Vec<f32>>, Option<Vec<[f32; 4]>>);

/// Split long chords; per-point widths and colors are interpolated onto the
/// new points.
fn limit_chords(
    points: &[[f32; 2]],
    widths: Option<&[f32]>,
    colors: Option<&[[f32; 4]]>,
    max_len: f32,
) -> Chords {
    let widths = widths.filter(|w| w.len() == points.len());
    let colors = colors.filter(|c| c.len() == points.len());
    if points.len() < 2 || !max_len.is_finite() {
        return (
            points.to_vec(),
            widths.map(<[f32]>::to_vec),
            colors.map(<[[f32; 4]]>::to_vec),
        );
    }
    let mut out = Vec::with_capacity(points.len());
    let mut out_widths = widths.map(|w| vec![w[0]]);
    let mut out_colors = colors.map(|c| vec![c[0]]);
    out.push(points[0]);
    for (k, pair) in points.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let delta = sub(b, a);
        let len = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
        let steps = (len / max_len).ceil() as usize;
        if steps > 1 {
            for i in 1..steps {
                let t = i as f32 / steps as f32;
                out.push(add(a, scale(delta, t)));
                if let (Some(ow), Some(w)) = (out_widths.as_mut(), widths) {
                    ow.push(w[k] + (w[k + 1] - w[k]) * t);
                }
                if let (Some(oc), Some(c)) = (out_colors.as_mut(), colors) {
                    oc.push(std::array::from_fn(|j| {
                        c[k][j] + (c[k + 1][j] - c[k][j]) * t
                    }));
                }
            }
        }
        out.push(b);
        if let (Some(ow), Some(w)) = (out_widths.as_mut(), widths) {
            ow.push(w[k + 1]);
        }
        if let (Some(oc), Some(c)) = (out_colors.as_mut(), colors) {
            oc.push(c[k + 1]);
        }
    }
    (out, out_widths, out_colors)
}

fn emit_strip(mesh: &mut InkMesh, stations: &[Station], feather: f32, colors: Option<&[[f32; 4]]>) {
    // One fringe quad per station. Extra alpha rings overlap along the stroke
    // and stack into bright seams.
    for (i, station) in stations.iter().enumerate() {
        let base = mesh.vertices.len() as u32;
        let inset = (feather * 0.5).min(station.half);
        for (side, amount, alpha) in [
            (0, feather * 0.5, 0.0),
            (0, -inset, 1.0),
            (1, -inset, 1.0),
            (1, feather * 0.5, 0.0),
        ] {
            let edge = station.edges[side];
            mesh.vertices.push(InkVertex {
                pos: add(edge.pos, scale(edge.normal, amount)),
                alpha,
            });
            if let Some(colors) = colors {
                mesh.colors.push(colors[station.at]);
            }
        }
        if i > 0 {
            let prev = base - 4;
            for (a, b, c) in [
                (0, 1, 5),
                (0, 5, 4),
                (1, 2, 6),
                (1, 6, 5),
                (2, 3, 7),
                (2, 7, 6),
            ] {
                mesh.indices
                    .extend_from_slice(&[prev + a, prev + b, prev + c]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{kurbo::BezPath, point_in_mesh, stroke_mesh, Cap, Join, StrokeStyle};

    fn mesh_area(mesh: &crate::InkMesh) -> f32 {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let a = mesh.vertices[t[0] as usize].pos;
                let b = mesh.vertices[t[1] as usize].pos;
                let c = mesh.vertices[t[2] as usize].pos;
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn round_caps_and_joins_cover_the_region_once() {
        let style = StrokeStyle {
            width: 20.0,
            cap: Cap::Round,
            join: Join::Round,
            taper: None,
            dash: None,
        };
        let capsule = stroke_mesh(&BezPath::from_svg("M0 0H100").unwrap(), &style, 0.0, 0.005);
        let expected = 2000.0 + std::f32::consts::PI * 100.0;
        assert!(
            (mesh_area(&capsule) - expected).abs() < 1.0,
            "capsule must not paint overlapping diameters: {} vs {expected}",
            mesh_area(&capsule)
        );
        for svg in ["M0 0H50V50", "M0 0H50V-50"] {
            let mesh = stroke_mesh(&BezPath::from_svg(svg).unwrap(), &style, 0.0, 0.005);
            let expected = 2000.0 + (1.25 * std::f32::consts::PI - 1.0) * 100.0;
            assert!(
                (mesh_area(&mesh) - expected).abs() < 1.0,
                "join must not overlap its inner facets: {} vs {expected}",
                mesh_area(&mesh)
            );
        }
    }

    #[test]
    fn butt_ends_meeting_mid_segment_tile_the_whole_stroke() {
        use crate::{stroke_mesh_ends, TipEase};
        let style = StrokeStyle {
            width: 20.0,
            cap: Cap::Round,
            join: Join::Round,
            taper: None,
            dash: None,
        };
        let path = |svg: &str| BezPath::from_svg(svg).unwrap();
        let whole = stroke_mesh(&path("M0 0H100V100"), &style, 0.0, 0.005);
        let widths = [20.0, 20.0, 20.0];
        let (round, butt) = (Cap::Round, Cap::Butt);
        let ends = |svg: &str, ends: [Cap; 2], widths: &[f32]| {
            let p = path(svg);
            stroke_mesh_ends(
                &p,
                &style,
                ends,
                Some(widths),
                None,
                TipEase::Linear,
                0.0,
                0.005,
            )
        };
        let first = ends("M0 0H100V50", [round, butt], &widths);
        let second = ends("M100 50V100", [butt, round], &widths[..2]);
        let tiled = mesh_area(&first) + mesh_area(&second);
        assert!(
            (tiled - mesh_area(&whole)).abs() < 0.5,
            "no gap and no overlap: {tiled} vs {}",
            mesh_area(&whole)
        );
    }

    #[test]
    fn tapered_stroke_retains_curve_samples_beyond_sixty_four_stations() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        let points: Vec<_> = (1..300)
            .map(|i| [i as f32, (i as f32 * 0.3).sin() * 8.0])
            .collect();
        for p in &points {
            path.line_to((p[0] as f64, p[1] as f64));
        }
        let style = StrokeStyle {
            width: 1.0,
            cap: Cap::Butt,
            join: Join::Round,
            taper: Some(crate::Taper::Linear(1.0, 0.5)),
            dash: None,
        };
        let mesh = stroke_mesh(&path, &style, 0.0, 0.02);
        let verts: Vec<_> = mesh.vertices.iter().map(|v| v.pos).collect();
        for p in points.iter().take(points.len() - 1) {
            assert!(
                point_in_mesh(&verts, &mesh.indices, *p),
                "lost curve sample {p:?}"
            );
        }
    }

    #[test]
    fn trimmed_closed_shapes_keep_uniform_width_on_every_edge() {
        let subject = vec![vec![
            [40.0, 30.0],
            [960.0, 30.0],
            [960.0, 410.0],
            [40.0, 410.0],
        ]];
        for cutter in [
            vec![[835.0, 0.0], [1000.0, 0.0], [1000.0, 200.0], [835.0, 200.0]],
            vec![
                [400.0, 130.0],
                [600.0, 130.0],
                [600.0, 300.0],
                [400.0, 300.0],
            ],
        ] {
            let pieces = crate::boolean_difference(&subject, &vec![cutter]);
            assert_eq!(pieces.len(), 1);
            for reverse in [false, true] {
                let mut path = BezPath::new();
                for ring in &pieces[0] {
                    let mut ring = ring.clone();
                    if reverse {
                        ring.reverse();
                    }
                    path.move_to((ring[0][0] as f64, ring[0][1] as f64));
                    for p in &ring[1..] {
                        path.line_to((p[0] as f64, p[1] as f64));
                    }
                    path.close_path();
                }
                for join in [Join::Miter, Join::Bevel, Join::Round] {
                    let style = StrokeStyle {
                        width: 8.0,
                        cap: Cap::Butt,
                        join,
                        taper: None,
                        dash: None,
                    };
                    let mesh = stroke_mesh(&path, &style, 0.0, 0.02);
                    let verts: Vec<_> = mesh.vertices.iter().map(|v| v.pos).collect();
                    for ring in &pieces[0] {
                        for i in 0..ring.len() {
                            let a = ring[i];
                            let b = ring[(i + 1) % ring.len()];
                            let normal = crate::geom::perp_left(
                                crate::geom::normalize(crate::geom::sub(b, a)).unwrap(),
                            );
                            for t in [0.1, 0.5, 0.9] {
                                for offset in [-3.8, 0.0, 3.8, -4.2, 4.2] {
                                    let p = [
                                        a[0] + (b[0] - a[0]) * t + normal[0] * offset,
                                        a[1] + (b[1] - a[1]) * t + normal[1] * offset,
                                    ];
                                    assert_eq!(
                                        point_in_mesh(&verts, &mesh.indices, p),
                                        offset.abs() < style.width * 0.5,
                                        "{join:?}, reverse={reverse}, edge {a:?} -> {b:?}, t={t}, offset={offset}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn round_strokes_cover_each_edge_including_the_closed_seam() {
        let style = StrokeStyle {
            width: 1.5,
            cap: Cap::Round,
            join: Join::Round,
            taper: None,
            dash: None,
        };
        for svg in ["M4 4H20V20H4Z", "M4 4V20H20V4Z"] {
            let mesh = stroke_mesh(&BezPath::from_svg(svg).unwrap(), &style, 0.0, 0.02);
            let verts: Vec<_> = mesh.vertices.iter().map(|v| v.pos).collect();
            for t in 5..20 {
                for offset in [-0.5, 0.0, 0.5] {
                    for p in [
                        [t as f32, 4.0 + offset],
                        [t as f32, 20.0 + offset],
                        [4.0 + offset, t as f32],
                        [20.0 + offset, t as f32],
                    ] {
                        assert!(
                            point_in_mesh(&verts, &mesh.indices, p),
                            "missing edge {p:?} in {svg}"
                        );
                    }
                }
            }
            assert!(!point_in_mesh(&verts, &mesh.indices, [12.0, 12.0]));
        }
    }

    #[test]
    fn round_join_preserves_incoming_width_and_rounds_the_outer_corner() {
        let style = StrokeStyle {
            width: 2.0,
            cap: Cap::Round,
            join: Join::Round,
            taper: None,
            dash: None,
        };
        let mesh = stroke_mesh(&BezPath::from_svg("M0 0H10V10").unwrap(), &style, 0.0, 0.02);
        let verts: Vec<_> = mesh.vertices.iter().map(|v| v.pos).collect();
        for p in [[8.0, -0.8], [10.7, -0.7], [10.8, 2.0]] {
            assert!(
                point_in_mesh(&verts, &mesh.indices, p),
                "missing join ink {p:?}"
            );
        }
        assert!(!point_in_mesh(&verts, &mesh.indices, [10.9, -0.9]));
    }
}
