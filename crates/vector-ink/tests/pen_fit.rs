//! Pen freehand fit acceptance (pen.md GP3): a jittery curve fits as one
//! curvature-continuous spline within tolerance, never as a polyline, and a
//! deliberate corner keeps its cusp.

use vector_ink::fit_polyline_spaced;
use vector_ink::kurbo::{BezPath, PathEl, Point};

const TOL: f32 = 2.0;
const SPACING: f32 = 1.75;

/// Deterministic jitter in `-amp..=amp`.
fn jitter(seed: &mut u64, amp: f32) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let unit = ((*seed >> 33) as f32) / ((1u64 << 31) as f32);
    (unit * 2.0 - 1.0) * amp
}

fn jittery_sine() -> Vec<[f32; 2]> {
    let mut seed = 7;
    (0..=300)
        .map(|i| {
            let x = i as f32 * 2.0;
            let y = 40.0 * (std::f32::consts::TAU * x / 200.0).sin();
            [x + jitter(&mut seed, 0.8), y + jitter(&mut seed, 0.8)]
        })
        .collect()
}

fn sharp_v() -> Vec<[f32; 2]> {
    let mut seed = 11;
    let mut pts = Vec::new();
    for i in 0..=50 {
        let t = i as f32 / 50.0;
        pts.push([100.0 * t, 120.0 * t]);
    }
    for i in 1..=50 {
        let t = i as f32 / 50.0;
        pts.push([100.0 + 100.0 * t, 120.0 - 120.0 * t]);
    }
    let last = pts.len() - 1;
    for (i, p) in pts.iter_mut().enumerate() {
        if i != 0 && i != 50 && i != last {
            p[0] += jitter(&mut seed, 0.3);
            p[1] += jitter(&mut seed, 0.3);
        }
    }
    pts
}

type Cubic = [Point; 4];

/// The cubic segments of a single-contour path; panics on any straight
/// or quadratic segment, which is what a segmented pen stroke looks like.
fn cubics(path: &BezPath) -> Vec<Cubic> {
    let mut out = Vec::new();
    let mut cur = None;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                assert!(cur.is_none(), "one contour");
                cur = Some(p);
            }
            PathEl::CurveTo(c1, c2, p3) => {
                out.push([cur.expect("starts with MoveTo"), c1, c2, p3]);
                cur = Some(p3);
            }
            PathEl::LineTo(_) => panic!("polyline segment in a fitted pen stroke: {path:?}"),
            PathEl::QuadTo(..) => panic!("quadratic segment in a fitted pen stroke"),
            PathEl::ClosePath => {}
        }
    }
    out
}

fn max_deviation(samples: &[[f32; 2]], path: &BezPath) -> f32 {
    let flat = vector_ink::flatten(path, 0.01);
    samples
        .iter()
        .map(|&p| {
            flat.windows(2)
                .map(|w| seg_dist(p, w[0], w[1]))
                .fold(f32::INFINITY, f32::min)
        })
        .fold(0.0, f32::max)
}

fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
}

fn len(v: Point) -> f64 {
    v.to_vec2().hypot()
}

/// Parametric C1 and C2 across the joint of two equal-interval cubics.
fn assert_c2(a: &Cubic, b: &Cubic, joint: usize) {
    let d1a = a[3] - a[2];
    let d1b = b[1] - b[0];
    let scale = len(Point::new(d1a.x, d1a.y)).max(1.0);
    assert!(
        (d1a - d1b).hypot() <= 1e-3 * scale,
        "joint {joint}: first derivative jumps {d1a:?} vs {d1b:?}"
    );
    let d2a = a[1].to_vec2() - 2.0 * a[2].to_vec2() + a[3].to_vec2();
    let d2b = b[0].to_vec2() - 2.0 * b[1].to_vec2() + b[2].to_vec2();
    assert!(
        (d2a - d2b).hypot() <= 1e-3 * scale,
        "joint {joint}: second derivative jumps {d2a:?} vs {d2b:?}"
    );
}

fn assert_live_handles(segs: &[Cubic]) {
    for (i, c) in segs.iter().enumerate() {
        let chord = (c[3] - c[0]).hypot();
        assert!(chord > 1e-6, "segment {i} is degenerate");
        let h1 = (c[1] - c[0]).hypot();
        let h2 = (c[3] - c[2]).hypot();
        assert!(
            h1 >= 0.05 * chord && h2 >= 0.05 * chord,
            "segment {i} looks like a polyline span: handles {h1:.3}/{h2:.3}, chord {chord:.3}"
        );
    }
}

#[test]
fn jittery_sine_fits_one_curvature_continuous_spline_within_tolerance() {
    let samples = jittery_sine();
    let path = fit_polyline_spaced(&samples, TOL, SPACING);
    let segs = cubics(&path);
    assert!(segs.len() >= 4, "a 3-period sine needs several spans");
    for (i, w) in segs.windows(2).enumerate() {
        assert_c2(&w[0], &w[1], i);
    }
    let dev = max_deviation(&samples, &path);
    assert!(dev <= TOL, "max deviation {dev} > tolerance {TOL}");
    assert_live_handles(&segs);
}

#[test]
fn sharp_v_keeps_its_cusp_and_stays_smooth_elsewhere() {
    let samples = sharp_v();
    let path = fit_polyline_spaced(&samples, TOL, SPACING);
    let segs = cubics(&path);
    let tip = Point::new(100.0, 120.0);
    let cusp = segs
        .iter()
        .take(segs.len() - 1)
        .position(|c| (c[3] - tip).hypot() <= TOL as f64)
        .expect("a vertex sits on the V's tip");
    let a = segs[cusp];
    let b = segs[cusp + 1];
    let tin = (a[3] - a[2]).normalize();
    let tout = (b[1] - b[0]).normalize();
    let turn = tin.dot(tout).clamp(-1.0, 1.0).acos().to_degrees();
    assert!(turn >= 60.0, "the cusp was rounded: turn {turn:.1} deg");
    for (i, w) in segs.windows(2).enumerate() {
        if i != cusp {
            assert_c2(&w[0], &w[1], i);
        }
    }
    let dev = max_deviation(&samples, &path);
    assert!(dev <= TOL, "max deviation {dev} > tolerance {TOL}");
    assert_live_handles(&segs);
}
