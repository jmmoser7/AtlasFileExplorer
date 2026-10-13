#[test]
fn open_curve_cuts_at_internal_vertices_without_empty_spans() {
    let target = [[0.0, 0.0], [20.0, 0.0], [40.0, 0.0], [60.0, 0.0]];
    let cutter = super::Cutter::Closed(rect(20.0, -10.0, 20.0, 20.0));
    let spans = super::split_open_at_cutters(&target, &[cutter]);
    assert_eq!(
        spans,
        vec![
            vec![target[0], target[1]],
            vec![target[1], target[2]],
            vec![target[2], target[3]]
        ]
    );
}
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
    assert!(point_in_mesh(&v, &i, [5.0, 5.0]));
    assert!(!point_in_mesh(&v, &i, [20.0, 20.0]));
}

#[test]
fn fill_triangles_covers_a_concave_union() {
    // Two offset rects — the Join case that egui's triangle-fan tears.
    let a = rect(0.0, 0.0, 80.0, 80.0);
    let b = rect(40.0, 40.0, 80.0, 80.0);
    let out = boolean_union_all(&[a, b]);
    assert_eq!(out.len(), 1);
    let (v, i) = fill_triangles(&out[0]);
    assert!(point_in_mesh(&v, &i, [10.0, 10.0]), "first lobe");
    assert!(point_in_mesh(&v, &i, [100.0, 100.0]), "second lobe");
    assert!(point_in_mesh(&v, &i, [50.0, 50.0]), "overlap");
    assert!(
        !point_in_mesh(&v, &i, [10.0, 100.0]),
        "notch must stay empty"
    );
    assert!(!point_in_mesh(&v, &i, [100.0, 10.0]), "other notch");
}

#[test]
fn fill_triangles_covers_disjoint_islands() {
    let a = rect(0.0, 0.0, 20.0, 20.0);
    let b = rect(80.0, 80.0, 20.0, 20.0);
    let (v, i) = fill_triangles(&[a[0].clone(), b[0].clone()]);
    assert!(point_in_mesh(&v, &i, [10.0, 10.0]));
    assert!(point_in_mesh(&v, &i, [90.0, 90.0]));
    assert!(!point_in_mesh(&v, &i, [50.0, 50.0]));
}

#[test]
fn fill_triangles_strips_a_closing_duplicate() {
    let closed = vec![vec![
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [0.0, 10.0],
        [0.0, 0.0],
    ]];
    let (v, i) = fill_triangles(&closed);
    assert!(point_in_mesh(&v, &i, [5.0, 5.0]));
    assert_eq!(i.len() % 3, 0);
}

#[test]
fn fill_triangles_honors_a_hole() {
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let c = circle(50.0, 50.0, 20.0, 32);
    let out = boolean_difference(&r, &c);
    assert_eq!(out.len(), 1);
    let (v, i) = fill_triangles(&out[0]);
    assert!(point_in_mesh(&v, &i, [5.0, 5.0]));
    assert!(!point_in_mesh(&v, &i, [50.0, 50.0]));
}

fn edges_axis_aligned(ring: &[[f32; 2]]) {
    let n = ring.len();
    assert!(n >= 3, "ring too short: {ring:?}");
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let dx = (b[0] - a[0]).abs();
        let dy = (b[1] - a[1]).abs();
        assert!(
            dx < 1e-3 || dy < 1e-3,
            "edge {a:?} → {b:?} is not axis-aligned"
        );
    }
}

fn ring_has(ring: &[[f32; 2]], p: [f32; 2]) -> bool {
    ring.iter()
        .any(|q| (q[0] - p[0]).abs() < 1e-3 && (q[1] - p[1]).abs() < 1e-3)
}

#[test]
fn union_of_axis_aligned_rects_keeps_axis_aligned_edges() {
    let a = rect(0.0, 0.0, 80.0, 80.0);
    let b = rect(40.0, 40.0, 80.0, 80.0);
    let out = boolean_union_all(&[a, b]);
    assert_eq!(out.len(), 1);
    let ring = &out[0][0];
    edges_axis_aligned(ring);
    for p in [
        [0.0, 0.0],
        [80.0, 0.0],
        [80.0, 40.0],
        [120.0, 40.0],
        [120.0, 120.0],
        [40.0, 120.0],
        [40.0, 80.0],
        [0.0, 80.0],
    ] {
        assert!(ring_has(ring, p), "missing corner {p:?} in {ring:?}");
    }
}

#[test]
fn difference_of_axis_aligned_rects_keeps_axis_aligned_edges() {
    let a = rect(0.0, 0.0, 80.0, 80.0);
    let b = rect(40.0, 40.0, 80.0, 80.0);
    let out = boolean_difference(&a, &b);
    assert_eq!(out.len(), 1);
    edges_axis_aligned(&out[0][0]);
}

#[test]
fn split_axis_aligned_rect_keeps_exact_halves() {
    let r = rect(0.0, 0.0, 100.0, 80.0);
    let cutter = infinite_line([50.0, -10.0], [50.0, 10.0]).unwrap();
    let out = split_closed(&r, &[cutter]).expect("split");
    assert_eq!(out.len(), 2);
    for piece in &out {
        edges_axis_aligned(&piece[0]);
        for v in &piece[0] {
            let x_ok = (v[0] - 0.0).abs() < 1e-3
                || (v[0] - 50.0).abs() < 1e-3
                || (v[0] - 100.0).abs() < 1e-3;
            let y_ok = (v[1] - 0.0).abs() < 1e-3 || (v[1] - 80.0).abs() < 1e-3;
            assert!(x_ok && y_ok, "unexpected vertex {v:?}");
        }
    }
}

#[test]
fn union_does_not_collapse_an_ellipse() {
    let e = circle(50.0, 50.0, 20.0, 48);
    let r = rect(50.0, 50.0, 40.0, 40.0);
    let out = boolean_union(&e, &r);
    assert_eq!(out.len(), 1);
    assert!(
        out[0][0].len() >= 20,
        "ellipse was over-simplified: {} verts",
        out[0][0].len()
    );
}

#[test]
fn union_keeps_a_stroke_ribbon_outside_a_rect() {
    use crate::{stroke_ribbon, Cap, Join, StrokeStyle};
    let style = StrokeStyle {
        width: 2.0,
        cap: Cap::Round,
        join: Join::Round,
        taper: None,
        dash: None,
    };
    let ribbon = stroke_ribbon(&[[0.0, 40.0], [140.0, 40.0]], &style).expect("ribbon");
    assert!(
        point_in_polygon(&ribbon, [10.0, 40.0]),
        "ribbon itself must cover the centerline: n={} lens={:?}",
        ribbon.len(),
        ribbon.iter().map(|c| c.len()).collect::<Vec<_>>()
    );
    let min_x = ribbon
        .iter()
        .flatten()
        .map(|p| p[0])
        .fold(f32::INFINITY, f32::min);
    let rect = rect(40.0, 20.0, 40.0, 40.0);
    let out = boolean_union(&rect, &ribbon);
    assert!(
        out.iter().any(|p| point_in_polygon(p, [10.0, 40.0])),
        "union dropped the ribbon wing (ribbon min x {min_x}): {out:?}"
    );
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

#[test]
fn split_closed_line_keeps_both_halves() {
    let r = rect(0.0, 0.0, 100.0, 80.0);
    let cutter = infinite_line([50.0, -10.0], [50.0, 10.0]).unwrap();
    let out = split_closed(&r, &[cutter]).expect("split");
    assert_eq!(out.len(), 2);
    let left = out.iter().any(|p| point_in_polygon(p, [10.0, 40.0]));
    let right = out.iter().any(|p| point_in_polygon(p, [80.0, 40.0]));
    assert!(left && right);
}

#[test]
fn split_closed_inner_circle_keeps_disk_and_ring() {
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let c = circle(50.0, 50.0, 20.0, 32);
    let out = split_closed(&r, &[Cutter::Closed(c)]).expect("split");
    assert_eq!(out.len(), 2);
    let disk = out.iter().any(|p| point_in_polygon(p, [50.0, 50.0]));
    let ring = out.iter().any(|p| point_in_polygon(p, [5.0, 5.0]));
    assert!(disk && ring);
    let disk_only = out
        .iter()
        .find(|p| point_in_polygon(p, [50.0, 50.0]))
        .unwrap();
    assert!(
        !point_in_polygon(disk_only, [5.0, 5.0]),
        "disk must not include the outer corner"
    );
}

#[test]
fn split_closed_miss_is_none() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let far = Cutter::Closed(rect(80.0, 80.0, 10.0, 10.0));
    assert!(split_closed(&r, &[far]).is_none());
}

/// A finely sampled arch from (-20, 40) over y = 17.5 to (120, 40),
/// as a flattened Bézier cutter reaches trim.
fn arch() -> Vec<[f32; 2]> {
    (0..=64)
        .map(|i| {
            let t = i as f32 / 64.0;
            let u = 1.0 - t;
            let x = u * u * u * -20.0
                + 3.0 * u * u * t * 10.0
                + 3.0 * u * t * t * 90.0
                + t * t * t * 120.0;
            let y = u * u * u * 40.0
                + 3.0 * u * u * t * 10.0
                + 3.0 * u * t * t * 10.0
                + t * t * t * 40.0;
            [x, y]
        })
        .collect()
}

fn area(poly: &Polygon) -> f32 {
    poly.iter().map(|r| signed_area(r).abs()).sum::<f32>()
        - 2.0
            * poly
                .iter()
                .skip(1)
                .map(|r| signed_area(r).abs())
                .sum::<f32>()
}

/// User, 28 September 2026 ("allow triming betwee open and closed
/// forms"): an open cutter divides a closed target along its whole path.
#[test]
fn curved_open_cutter_trims_a_closed_target() {
    let r = rect(0.0, 0.0, 100.0, 80.0);
    let out = trim_closed_at_click(&r, &[Cutter::Open(arch())], [50.0, 60.0]).expect("trim");
    assert_eq!(out.len(), 1, "the cap above the arch remains");
    assert!(point_in_polygon(&out[0], [50.0, 5.0]));
    assert!(!point_in_polygon(&out[0], [50.0, 60.0]));
    assert!(!point_in_polygon(&out[0], [5.0, 60.0]));
}

#[test]
fn polyline_cutter_crossing_a_circle_twice_splits_off_a_cap() {
    let c = circle(50.0, 50.0, 50.0, 96);
    let u = vec![[30.0, -10.0], [30.0, 40.0], [70.0, 40.0], [70.0, -10.0]];
    let out = split_closed(&c, &[Cutter::Open(u.clone())]).expect("split");
    assert_eq!(out.len(), 2);
    let cap = out
        .iter()
        .find(|p| point_in_polygon(p, [50.0, 20.0]))
        .unwrap();
    assert!(!point_in_polygon(cap, [50.0, 80.0]));
    assert!(!point_in_polygon(cap, [20.0, 20.0]));
    let total: f32 = out.iter().map(area).sum();
    assert!((total - area(&c)).abs() < 1.0, "pieces tile the circle");

    let trimmed = trim_closed_at_click(&c, &[Cutter::Open(u)], [50.0, 20.0]).unwrap();
    assert_eq!(trimmed.len(), 1);
    assert!(point_in_polygon(&trimmed[0], [20.0, 20.0]));
    assert!(point_in_polygon(&trimmed[0], [50.0, 80.0]));
    assert!(!point_in_polygon(&trimmed[0], [50.0, 20.0]));
}

#[test]
fn zigzag_cutter_crossing_several_times_makes_every_piece() {
    let r = rect(0.0, 0.0, 100.0, 40.0);
    // Each of the four legs crosses the rect from edge to edge.
    let zig = vec![
        [10.0, -10.0],
        [30.0, 50.0],
        [50.0, -10.0],
        [70.0, 50.0],
        [90.0, -10.0],
    ];
    let out = split_closed(&r, &[Cutter::Open(zig)]).expect("split");
    assert_eq!(out.len(), 5);
    let total: f32 = out.iter().map(area).sum();
    assert!((total - 4000.0).abs() < 1.0);
}

#[test]
fn dangling_open_cutter_does_not_divide() {
    let r = rect(0.0, 0.0, 100.0, 80.0);
    let stub = vec![[-10.0, 40.0], [20.0, 40.0], [40.0, 50.0]];
    assert!(split_closed(&r, &[Cutter::Open(stub)]).is_none());
}

#[test]
fn open_cutter_keeps_the_target_hole() {
    let mut holed = rect(0.0, 0.0, 100.0, 100.0);
    holed.push(rect(40.0, 60.0, 20.0, 20.0).remove(0));
    let bend = vec![[-10.0, 30.0], [50.0, 40.0], [110.0, 30.0]];
    let out = split_closed(&holed, &[Cutter::Open(bend)]).expect("split");
    assert_eq!(out.len(), 2);
    let lower = out
        .iter()
        .find(|p| point_in_polygon(p, [10.0, 90.0]))
        .unwrap();
    assert!(!point_in_polygon(lower, [50.0, 70.0]), "the hole survives");
}

/// Open and closed cutters together: the clicked face is bounded by both.
#[test]
fn mixed_open_and_closed_cutters_bound_the_clicked_face() {
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let cutters = [
        infinite_line([50.0, -10.0], [50.0, 10.0]).unwrap(),
        Cutter::Closed(circle(50.0, 50.0, 20.0, 64)),
    ];
    let out = trim_closed_at_click(&r, &cutters, [40.0, 50.0]).expect("trim");
    let kept = |p: [f32; 2]| out.iter().any(|piece| point_in_polygon(piece, p));
    assert!(!kept([40.0, 50.0]), "the clicked half-disk is gone");
    assert!(kept([60.0, 50.0]), "the other half-disk stays");
    assert!(kept([10.0, 10.0]) && kept([90.0, 90.0]));
}

/// A closed cutter already divides an open target.
#[test]
fn closed_cutter_divides_an_open_target() {
    let c = circle(50.0, 50.0, 30.0, 64);
    let spans = split_open_at_cutters(&[[0.0, 50.0], [100.0, 50.0]], &[Cutter::Closed(c)]);
    assert_eq!(spans.len(), 3);
    assert!((spans[0].last().unwrap()[0] - 20.0).abs() < 0.2);
    assert!((spans[2][0][0] - 80.0).abs() < 0.2);
}
