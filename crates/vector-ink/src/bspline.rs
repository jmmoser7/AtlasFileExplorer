//! Cubic B-splines for freehand curves.
//!
//! The pen and the smoothing brush both fit a clamped cubic B-spline to
//! arc-length-parameterized samples: uniform knots, least squares with the
//! end samples interpolated, light fairing on the control polygon's second
//! differences, and a few rounds of parameter correction. The knot count
//! grows until every sample is within tolerance, so the curve is as simple
//! as the stroke allows. This is the approach of Rhino's Sketch followed by
//! Rebuild/FitCrv, cut down to what the board uses.
//!
//! The board never stores a spline. [`CubicBSpline::to_bezpath`] extracts
//! the exact cubic Bézier segments by knot insertion, so storage, painting,
//! and SVG export stay cubic Bézier paths (Constitution Art. IV).

use kurbo::{BezPath, ParamCurve, ParamCurveArclen};

/// Default bending weight for [`SplineFit`]: light enough to leave the
/// stroke's shape to the data term, heavy enough to keep a nearly
/// interpolating fit from wiggling.
pub const DEFAULT_FAIRING: f64 = 0.01;

/// The first guess at knot spans is one per this many tolerances of stroke
/// length; the fit adds spans from there until it meets the tolerance.
const TOLERANCES_PER_SPAN: f64 = 48.0;
const PARAM_CORRECTIONS: usize = 2;

/// A clamped cubic B-spline. `knots.len() == ctrl.len() + 4`, and the end
/// knots have multiplicity four, so the curve starts on the first control
/// point and ends on the last.
#[derive(Debug, Clone, PartialEq)]
pub struct CubicBSpline {
    pub ctrl: Vec<[f64; 2]>,
    pub knots: Vec<f64>,
}

/// Options for [`CubicBSpline::fit`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplineFit {
    /// Largest allowed distance from a sample to the curve.
    pub tolerance: f32,
    /// Longest knot span in arc length; `INFINITY` leaves spacing to the
    /// tolerance alone.
    pub max_span: f32,
    /// Weight of the control polygon's bending energy against the data.
    pub fairing: f64,
}

impl SplineFit {
    pub fn new(tolerance: f32) -> Self {
        Self {
            tolerance,
            max_span: f32::INFINITY,
            fairing: DEFAULT_FAIRING,
        }
    }
}

impl CubicBSpline {
    /// Least-squares fit to an ordered run of samples. The end samples are
    /// interpolated exactly. `None` when the run has no length.
    pub fn fit(points: &[[f32; 2]], opts: &SplineFit) -> Option<Self> {
        let mut q: Vec<[f64; 2]> = Vec::with_capacity(points.len());
        for p in points {
            let p = [f64::from(p[0]), f64::from(p[1])];
            if !(p[0].is_finite() && p[1].is_finite()) {
                continue;
            }
            if q.last().is_some_and(|l| dist(*l, p) <= 1e-9) {
                continue;
            }
            q.push(p);
        }
        if q.len() < 2 {
            return None;
        }
        let mut u = Vec::with_capacity(q.len());
        let mut acc = 0.0;
        u.push(0.0);
        for w in q.windows(2) {
            acc += dist(w[0], w[1]);
            u.push(acc);
        }
        let total = acc;
        if total <= 1e-9 {
            return None;
        }
        for v in &mut u {
            *v /= total;
        }
        let tol = f64::from(opts.tolerance).max(1e-6);
        let max_spans = (q.len() - 1).max(1);
        let by_tol = (total / (TOLERANCES_PER_SPAN * tol)).ceil();
        let by_span = if opts.max_span.is_finite() && opts.max_span > 0.0 {
            (total / f64::from(opts.max_span)).ceil()
        } else {
            1.0
        };
        let mut spans = (by_tol.max(by_span) as usize).clamp(1, max_spans);
        let fairing = opts.fairing.max(1e-9);
        loop {
            let (spline, err) = fit_spans(&q, &u, spans, fairing)?;
            if err <= tol || spans >= max_spans {
                return Some(spline);
            }
            spans = (spans * 3 / 2).max(spans + 1).min(max_spans);
        }
    }

    /// Refit an existing single-contour path, for editing. Each run of
    /// tangent-continuous segments is resampled every `spacing` of arc
    /// length and fitted on its own; runs meet with C0 joints at the path's
    /// corners, so a corner is kept until something smooths it. A closing
    /// segment is part of the contour. `None` for multi-contour or empty
    /// paths.
    pub fn fit_path(path: &BezPath, opts: &SplineFit, spacing: f32) -> Option<Self> {
        let spacing = f64::from(spacing).max(1e-3);
        if path
            .elements()
            .iter()
            .filter(|e| matches!(e, kurbo::PathEl::MoveTo(_)))
            .count()
            > 1
        {
            return None;
        }
        let segs: Vec<kurbo::PathSeg> = path.segments().filter(|s| s.arclen(1e-3) > 1e-9).collect();
        let mut runs: Vec<Vec<kurbo::PathSeg>> = Vec::new();
        for seg in segs {
            let smooth = runs
                .last()
                .and_then(|r| r.last())
                .is_some_and(|prev| tangents_agree(end_tangent(prev), start_tangent(&seg)));
            match runs.last_mut() {
                Some(run) if smooth => run.push(seg),
                _ => runs.push(vec![seg]),
            }
        }
        let mut out: Option<CubicBSpline> = None;
        for run in runs {
            let mut pts: Vec<[f32; 2]> = Vec::new();
            for seg in &run {
                let len = seg.arclen(1e-3);
                let steps = (len / spacing).ceil().max(1.0) as usize;
                let from = usize::from(!pts.is_empty());
                for i in from..=steps {
                    let s = len * i as f64 / steps as f64;
                    let t = if i == steps {
                        1.0
                    } else {
                        seg.inv_arclen(s, 1e-4)
                    };
                    let p = seg.eval(t);
                    pts.push([p.x as f32, p.y as f32]);
                }
            }
            let piece = CubicBSpline::fit(&pts, opts)?;
            match out.as_mut() {
                Some(s) => s.join(&piece),
                None => out = Some(piece),
            }
        }
        out
    }

    /// Parameter domain `(start, end)`.
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[3], self.knots[self.ctrl.len()])
    }

    /// The point at parameter `u` (clamped to the domain).
    pub fn eval(&self, u: f64) -> [f64; 2] {
        let (a, b) = self.domain();
        let u = u.clamp(a, b);
        let span = self.find_span(u);
        let n = basis(&self.knots, span, u);
        let mut p = [0.0; 2];
        for (j, w) in n.iter().enumerate() {
            let c = self.ctrl[span - 3 + j];
            p[0] += w * c[0];
            p[1] += w * c[1];
        }
        p
    }

    /// Greville abscissa of control point `i`: the parameter where that
    /// control point has the most say.
    pub fn greville(&self, i: usize) -> f64 {
        (self.knots[i + 1] + self.knots[i + 2] + self.knots[i + 3]) / 3.0
    }

    /// Append `other` with a C0 joint. `other` must start where `self`
    /// ends; the joint knot gets multiplicity three, so a cusp stays sharp.
    pub fn join(&mut self, other: &CubicBSpline) {
        let (_, end) = self.domain();
        let (start, _) = other.domain();
        let shift = end - start;
        self.knots.pop();
        self.knots
            .extend(other.knots.iter().skip(4).map(|k| k + shift));
        self.ctrl.extend(other.ctrl.iter().skip(1).copied());
    }

    /// Insert one knot at `u` without changing the curve (Boehm).
    pub fn insert_knot(&mut self, u: f64) {
        let n = self.ctrl.len();
        let k = self.find_span(u);
        let t = &self.knots;
        let mut ctrl = Vec::with_capacity(n + 1);
        ctrl.extend_from_slice(&self.ctrl[..=k - 3]);
        for i in k - 2..=k {
            let a = (u - t[i]) / (t[i + 3] - t[i]);
            let (p, q) = (self.ctrl[i - 1], self.ctrl[i]);
            ctrl.push([p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a]);
        }
        ctrl.extend_from_slice(&self.ctrl[k..]);
        self.ctrl = ctrl;
        self.knots.insert(k + 1, u);
    }

    /// The exact cubic Bézier segments of this spline, one per non-empty
    /// knot span, as one open contour.
    pub fn to_bezpath(&self) -> BezPath {
        let mut s = self.clone();
        let (a, b) = s.domain();
        let mut interior: Vec<f64> = s
            .knots
            .iter()
            .copied()
            .filter(|k| *k > a && *k < b)
            .collect();
        interior.dedup();
        for u in interior {
            let mult = s.knots.iter().filter(|k| **k == u).count();
            for _ in mult..3 {
                s.insert_knot(u);
            }
        }
        let mut path = BezPath::new();
        let Some(first) = s.ctrl.first() else {
            return path;
        };
        path.move_to(pt(*first));
        for c in s.ctrl[1..].as_chunks::<3>().0 {
            path.curve_to(pt(c[0]), pt(c[1]), pt(c[2]));
        }
        path
    }

    fn find_span(&self, u: f64) -> usize {
        let n = self.ctrl.len();
        if u >= self.knots[n] {
            return n - 1;
        }
        if u <= self.knots[3] {
            return 3;
        }
        let (mut lo, mut hi) = (3, n);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.knots[mid] <= u {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Derivatives at `u` by central differences; accurate enough for the
    /// Newton steps of parameter correction.
    fn ders(&self, u: f64) -> ([f64; 2], [f64; 2], [f64; 2]) {
        let (a, b) = self.domain();
        let h = (b - a) * 1e-4;
        let u = u.clamp(a + h, b - h);
        let p0 = self.eval(u - h);
        let p1 = self.eval(u);
        let p2 = self.eval(u + h);
        let d1 = [(p2[0] - p0[0]) / (2.0 * h), (p2[1] - p0[1]) / (2.0 * h)];
        let d2 = [
            (p2[0] - 2.0 * p1[0] + p0[0]) / (h * h),
            (p2[1] - 2.0 * p1[1] + p0[1]) / (h * h),
        ];
        (p1, d1, d2)
    }

    /// Move each interior sample's parameter to its closest point on the
    /// curve (one Newton step).
    fn correct_params(&self, q: &[[f64; 2]], u: &mut [f64]) {
        let last = u.len() - 1;
        for k in 1..last {
            let (p, d1, d2) = self.ders(u[k]);
            let r = [p[0] - q[k][0], p[1] - q[k][1]];
            let num = r[0] * d1[0] + r[1] * d1[1];
            let den = d1[0] * d1[0] + d1[1] * d1[1] + r[0] * d2[0] + r[1] * d2[1];
            if den.abs() > 1e-12 {
                u[k] = (u[k] - num / den).clamp(0.0, 1.0);
            }
        }
    }
}

/// Joints whose tangents differ by less than this are smooth.
const SMOOTH_JOINT_COS: f64 = 0.9994; // ~2°

fn tangents_agree(a: kurbo::Vec2, b: kurbo::Vec2) -> bool {
    let (la, lb) = (a.hypot(), b.hypot());
    la > 1e-12 && lb > 1e-12 && a.dot(b) / (la * lb) >= SMOOTH_JOINT_COS
}

fn start_tangent(seg: &kurbo::PathSeg) -> kurbo::Vec2 {
    let pts: Vec<kurbo::Point> = match *seg {
        kurbo::PathSeg::Line(l) => vec![l.p0, l.p1],
        kurbo::PathSeg::Quad(q) => vec![q.p0, q.p1, q.p2],
        kurbo::PathSeg::Cubic(c) => vec![c.p0, c.p1, c.p2, c.p3],
    };
    pts[1..]
        .iter()
        .map(|p| *p - pts[0])
        .find(|v| v.hypot() > 1e-9)
        .unwrap_or_default()
}

fn end_tangent(seg: &kurbo::PathSeg) -> kurbo::Vec2 {
    start_tangent(&seg.reverse()) * -1.0
}

/// Uniform clamped knots for `spans` knot spans on `[0, 1]`.
fn uniform_knots(spans: usize) -> Vec<f64> {
    let mut k = vec![0.0; 4];
    k.extend((1..spans).map(|i| i as f64 / spans as f64));
    k.extend([1.0; 4]);
    k
}

fn fit_spans(
    q: &[[f64; 2]],
    u0: &[f64],
    spans: usize,
    fairing: f64,
) -> Option<(CubicBSpline, f64)> {
    let knots = uniform_knots(spans);
    let mut u = u0.to_vec();
    let mut spline = solve(q, &u, knots, fairing)?;
    for _ in 0..PARAM_CORRECTIONS {
        spline.correct_params(q, &mut u);
        spline = solve(q, &u, spline.knots, fairing)?;
    }
    spline.correct_params(q, &mut u);
    let err = q
        .iter()
        .zip(&u)
        .map(|(p, &t)| dist(*p, spline.eval(t)))
        .fold(0.0, f64::max);
    Some((spline, err))
}

/// Normal equations of the faired least-squares fit, end control points
/// pinned to the end samples, solved by banded Cholesky.
fn solve(q: &[[f64; 2]], u: &[f64], knots: Vec<f64>, fairing: f64) -> Option<CubicBSpline> {
    let n = knots.len() - 4;
    let (p0, pn) = (q[0], q[q.len() - 1]);
    let fixed = |i: usize| -> Option<[f64; 2]> {
        if i == 0 {
            Some(p0)
        } else if i == n - 1 {
            Some(pn)
        } else {
            None
        }
    };
    let free = n - 2;
    let mut band = Band::new(free, 3);
    let mut rhs = vec![[0.0_f64; 2]; free];
    let probe = CubicBSpline {
        ctrl: vec![[0.0; 2]; n],
        knots,
    };
    for (qk, &uk) in q.iter().zip(u) {
        let span = probe.find_span(uk);
        let b = basis(&probe.knots, span, uk);
        let mut target = *qk;
        for (j, w) in b.iter().enumerate() {
            if let Some(p) = fixed(span - 3 + j) {
                target[0] -= w * p[0];
                target[1] -= w * p[1];
            }
        }
        for a in 0..4 {
            let ia = span - 3 + a;
            if fixed(ia).is_some() {
                continue;
            }
            rhs[ia - 1][0] += b[a] * target[0];
            rhs[ia - 1][1] += b[a] * target[1];
            for c in 0..=a {
                let ic = span - 3 + c;
                if fixed(ic).is_none() {
                    band.add(ia - 1, ic - 1, b[a] * b[c]);
                }
            }
        }
    }
    let lambda = fairing * q.len() as f64 / n as f64;
    for i in 1..n - 1 {
        let terms = [(i - 1, 1.0), (i, -2.0), (i + 1, 1.0)];
        let mut pinned = [0.0; 2];
        for (j, c) in terms {
            if let Some(p) = fixed(j) {
                pinned[0] += c * p[0];
                pinned[1] += c * p[1];
            }
        }
        for (a, ca) in terms {
            if fixed(a).is_some() {
                continue;
            }
            rhs[a - 1][0] -= lambda * ca * pinned[0];
            rhs[a - 1][1] -= lambda * ca * pinned[1];
            for (b, cb) in terms {
                if b <= a && fixed(b).is_none() {
                    band.add(a - 1, b - 1, lambda * ca * cb);
                }
            }
        }
    }
    band.cholesky()?;
    let x = band.solve(rhs.iter().map(|r| r[0]).collect());
    let y = band.solve(rhs.iter().map(|r| r[1]).collect());
    let mut ctrl = Vec::with_capacity(n);
    ctrl.push(p0);
    ctrl.extend(x.into_iter().zip(y).map(|(x, y)| [x, y]));
    ctrl.push(pn);
    Some(CubicBSpline {
        ctrl,
        knots: probe.knots,
    })
}

/// The four non-zero cubic basis functions on `span` at `u` (Cox–de Boor).
fn basis(knots: &[f64], span: usize, u: f64) -> [f64; 4] {
    let mut n = [1.0, 0.0, 0.0, 0.0];
    let mut left = [0.0; 4];
    let mut right = [0.0; 4];
    for j in 1..=3 {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let temp = n[r] / (right[r + 1] + left[j - r]);
            n[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        n[j] = saved;
    }
    n
}

/// Symmetric banded matrix stored as its lower band, factored in place.
struct Band {
    n: usize,
    w: usize,
    a: Vec<f64>,
}

impl Band {
    fn new(n: usize, w: usize) -> Self {
        Self {
            n,
            w,
            a: vec![0.0; n * (w + 1)],
        }
    }

    fn at(&self, i: usize, j: usize) -> f64 {
        self.a[i * (self.w + 1) + (i - j)]
    }

    fn add(&mut self, i: usize, j: usize, v: f64) {
        debug_assert!(i >= j && i - j <= self.w);
        self.a[i * (self.w + 1) + (i - j)] += v;
    }

    fn set(&mut self, i: usize, j: usize, v: f64) {
        self.a[i * (self.w + 1) + (i - j)] = v;
    }

    fn cholesky(&mut self) -> Option<()> {
        for i in 0..self.n {
            for j in i.saturating_sub(self.w)..=i {
                let mut s = self.at(i, j);
                for k in i.saturating_sub(self.w)..j {
                    s -= self.at(i, k) * self.at(j, k);
                }
                if i == j {
                    if !(s > 0.0 && s.is_finite()) {
                        return None;
                    }
                    self.set(i, i, s.sqrt());
                } else {
                    let d = self.at(j, j);
                    self.set(i, j, s / d);
                }
            }
        }
        Some(())
    }

    fn solve(&self, mut b: Vec<f64>) -> Vec<f64> {
        for i in 0..self.n {
            let lo = i.saturating_sub(self.w);
            let s: f64 = (lo..i).zip(&b[lo..i]).map(|(k, x)| self.at(i, k) * x).sum();
            b[i] = (b[i] - s) / self.at(i, i);
        }
        for i in (0..self.n).rev() {
            let hi = (i + self.w + 1).min(self.n);
            let s: f64 = (i + 1..hi)
                .zip(&b[i + 1..hi])
                .map(|(k, x)| self.at(k, i) * x)
                .sum();
            b[i] = (b[i] - s) / self.at(i, i);
        }
        b
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn pt(p: [f64; 2]) -> kurbo::Point {
    kurbo::Point::new(p[0], p[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{ParamCurve, PathEl};

    fn cubics(path: &BezPath) -> Vec<kurbo::CubicBez> {
        path.segments()
            .map(|s| match s {
                kurbo::PathSeg::Cubic(c) => c,
                other => panic!("not cubic: {other:?}"),
            })
            .collect()
    }

    fn arc(n: usize) -> Vec<[f32; 2]> {
        (0..=n)
            .map(|i| {
                let a = std::f32::consts::PI * i as f32 / n as f32;
                [100.0 * a.cos(), 100.0 * a.sin()]
            })
            .collect()
    }

    #[test]
    fn fit_meets_tolerance_and_pins_the_ends() {
        let pts = arc(200);
        let s = CubicBSpline::fit(&pts, &SplineFit::new(0.5)).unwrap();
        assert_eq!(s.ctrl[0], [100.0, 0.0]);
        let last = *pts.last().unwrap();
        let end = *s.ctrl.last().unwrap();
        assert!((end[0] - f64::from(last[0])).abs() < 1e-9);
        let bez = s.to_bezpath();
        let flat = crate::flatten(&bez, 0.01);
        for p in &pts {
            let d = flat
                .windows(2)
                .map(|w| crate::geom::dist_to_segment(*p, w[0], w[1]))
                .fold(f32::INFINITY, f32::min);
            assert!(d <= 0.5 + 1e-3, "deviation {d}");
        }
    }

    #[test]
    fn simpler_strokes_get_fewer_spans() {
        let tight = CubicBSpline::fit(&arc(200), &SplineFit::new(0.1)).unwrap();
        let loose = CubicBSpline::fit(&arc(200), &SplineFit::new(4.0)).unwrap();
        assert!(loose.ctrl.len() < tight.ctrl.len());
    }

    #[test]
    fn max_span_caps_knot_spacing() {
        let mut opts = SplineFit::new(10.0);
        opts.max_span = 20.0;
        let s = CubicBSpline::fit(&arc(200), &opts).unwrap();
        let spans = s.ctrl.len() - 3;
        assert!(spans as f32 >= std::f32::consts::PI * 100.0 / 20.0);
    }

    #[test]
    fn knot_insertion_keeps_the_curve() {
        let s = CubicBSpline::fit(&arc(100), &SplineFit::new(0.5)).unwrap();
        let mut t = s.clone();
        t.insert_knot(0.37);
        t.insert_knot(0.37);
        for i in 0..=50 {
            let u = i as f64 / 50.0;
            assert!(dist(s.eval(u), t.eval(u)) < 1e-9);
        }
    }

    #[test]
    fn bezier_extraction_is_exact_and_c2_on_uniform_knots() {
        let s = CubicBSpline::fit(&arc(100), &SplineFit::new(0.05)).unwrap();
        let bez = s.to_bezpath();
        let segs = cubics(&bez);
        assert_eq!(segs.len(), s.ctrl.len() - 3, "one cubic per knot span");
        let spans = segs.len() as f64;
        for (i, c) in segs.iter().enumerate() {
            for j in 0..=10 {
                let t = j as f64 / 10.0;
                let u = (i as f64 + t) / spans;
                let p = c.eval(t);
                assert!(dist([p.x, p.y], s.eval(u)) < 1e-9);
            }
        }
        for w in segs.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(((a.p3 - a.p2) - (b.p1 - b.p0)).hypot() < 1e-9);
            let d2a = a.p1.to_vec2() - 2.0 * a.p2.to_vec2() + a.p3.to_vec2();
            let d2b = b.p0.to_vec2() - 2.0 * b.p1.to_vec2() + b.p2.to_vec2();
            assert!((d2a - d2b).hypot() < 1e-9);
        }
    }

    #[test]
    fn join_keeps_a_sharp_joint() {
        let a = CubicBSpline::fit(
            &[[0.0, 0.0], [50.0, 0.0], [100.0, 0.0]],
            &SplineFit::new(0.5),
        )
        .unwrap();
        let b = CubicBSpline::fit(
            &[[100.0, 0.0], [100.0, 50.0], [100.0, 100.0]],
            &SplineFit::new(0.5),
        )
        .unwrap();
        let mut j = a.clone();
        j.join(&b);
        assert_eq!(j.knots.len(), j.ctrl.len() + 4);
        let bez = j.to_bezpath();
        let segs = cubics(&bez);
        let corner = segs
            .iter()
            .position(|c| (c.p3 - kurbo::Point::new(100.0, 0.0)).hypot() < 1e-9)
            .unwrap();
        let tin = (segs[corner].p3 - segs[corner].p2).normalize();
        let tout = (segs[corner + 1].p1 - segs[corner + 1].p0).normalize();
        assert!(tin.dot(tout).abs() < 1e-6, "the corner stays square");
        assert!(matches!(bez.elements()[0], PathEl::MoveTo(_)));
    }

    #[test]
    fn two_samples_fit_a_straight_cubic() {
        let s = CubicBSpline::fit(&[[0.0, 0.0], [30.0, 0.0]], &SplineFit::new(1.0)).unwrap();
        let segs = cubics(&s.to_bezpath());
        assert_eq!(segs.len(), 1);
        assert!((segs[0].p1.x - 10.0).abs() < 1e-6 && segs[0].p1.y.abs() < 1e-9);
        assert!((segs[0].p2.x - 20.0).abs() < 1e-6);
    }

    #[test]
    fn fit_path_reproduces_a_sparse_bezier_and_keeps_its_corner() {
        let mut bez = BezPath::new();
        bez.move_to((0.0, 0.0));
        bez.curve_to((50.0, -150.0), (150.0, -150.0), (200.0, 0.0));
        bez.curve_to((250.0, -150.0), (350.0, -150.0), (400.0, 0.0));
        let mut opts = SplineFit::new(0.1);
        opts.max_span = 20.0;
        let s = CubicBSpline::fit_path(&bez, &opts, 1.0).unwrap();
        assert!(s.ctrl.len() > 20, "enough control points for a brush");
        let out = s.to_bezpath();
        let flat = crate::flatten(&out, 0.005);
        for seg in bez.segments() {
            for i in 0..=40 {
                let p = seg.eval(i as f64 / 40.0);
                let p = [p.x as f32, p.y as f32];
                let d = flat
                    .windows(2)
                    .map(|w| crate::geom::dist_to_segment(p, w[0], w[1]))
                    .fold(f32::INFINITY, f32::min);
                assert!(d <= 0.12, "refit moved the path by {d}");
            }
        }
        let segs = cubics(&out);
        let corner = segs
            .iter()
            .position(|c| (c.p3 - kurbo::Point::new(200.0, 0.0)).hypot() < 1e-6)
            .expect("the corner is a vertex");
        let tin = (segs[corner].p3 - segs[corner].p2).normalize();
        let tout = (segs[corner + 1].p1 - segs[corner + 1].p0).normalize();
        assert!(tin.dot(tout) < 0.0, "the cusp stays a cusp until smoothed");
    }

    #[test]
    fn fit_path_follows_a_closed_contour_back_to_its_start() {
        let mut bez = BezPath::new();
        bez.move_to((0.0, 0.0));
        bez.line_to((100.0, 0.0));
        bez.line_to((100.0, 100.0));
        bez.close_path();
        let s = CubicBSpline::fit_path(&bez, &SplineFit::new(0.1), 1.0).unwrap();
        assert_eq!(s.ctrl.first(), s.ctrl.last());
        assert!(CubicBSpline::fit_path(&BezPath::new(), &SplineFit::new(0.1), 1.0).is_none());
    }

    #[test]
    fn degenerate_runs_do_not_fit() {
        assert!(CubicBSpline::fit(&[], &SplineFit::new(1.0)).is_none());
        assert!(CubicBSpline::fit(&[[1.0, 1.0], [1.0, 1.0]], &SplineFit::new(1.0)).is_none());
    }
}
