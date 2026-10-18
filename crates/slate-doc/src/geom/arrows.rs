//! Open-curve arrow geometry shared by every interpreter.
use super::*;

/// Arrowhead for a stroke of `stroke_width`: a filled triangle with its tip
/// at `tip` and its base back along `into_curve` (the unit tangent pointing
/// from the endpoint into the curve). Connectors and open curves share it,
/// on the board and in the artifact.
pub fn arrow_head(tip: [f32; 2], into_curve: [f32; 2], stroke_width: f32) -> [[f32; 2]; 3] {
    let len = arrow_len(stroke_width);
    arrow_head_sized(tip, into_curve, [len, len * 0.8])
}

pub fn arrow_head_sized(tip: [f32; 2], into_curve: [f32; 2], size: [f32; 2]) -> [[f32; 2]; 3] {
    let [len, width] = size;
    let half = width * 0.5;
    let base = [tip[0] + into_curve[0] * len, tip[1] + into_curve[1] * len];
    let perp = [-into_curve[1], into_curve[0]];
    [
        tip,
        [base[0] + perp[0] * half, base[1] + perp[1] * half],
        [base[0] - perp[0] * half, base[1] - perp[1] * half],
    ]
}

/// Arrowhead length along the curve.
pub fn arrow_len(stroke_width: f32) -> f32 {
    (stroke_width * 4.0).max(10.0)
}

/// Endpoint of an open path and the unit direction from it into the curve,
/// for an arrowhead of a stroke `stroke_width` wide. The direction aims at
/// the last point of the curve one head length from the tip, so the head's
/// base sits on the curve even where the end bends or hooks. `None` for an
/// empty or degenerate path.
pub fn path_end_arrow(bez: &BezPath, stroke_width: f32) -> Option<([f32; 2], [f32; 2])> {
    path_end_arrow_length(bez, arrow_len(stroke_width))
}

fn path_end_arrow_length(bez: &BezPath, length: f32) -> Option<([f32; 2], [f32; 2])> {
    use vector_ink::kurbo::{ParamCurve, PathSeg};
    let segs: Vec<PathSeg> = bez.segments().collect();
    let tip = segs.last()?.eval(1.0);
    let need = length.max(0.1) as f64;
    let mut back = tip;
    for seg in segs.iter().rev() {
        let start = seg.eval(0.0);
        if (start - tip).hypot() < need {
            back = start;
            continue;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if (seg.eval(mid) - tip).hypot() >= need {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        back = seg.eval(lo);
        break;
    }
    let d = back - tip;
    let len = d.hypot();
    if len.is_nan() || len <= 1e-9 {
        return None;
    }
    Some((
        [tip.x as f32, tip.y as f32],
        [(d.x / len) as f32, (d.y / len) as f32],
    ))
}

/// `bez` shortened by `by` along its arc length, the trailing segments that
/// fall inside `by` dropped, plus how many were dropped and the kept
/// fraction of the cut segment's arc length. `None` when there is nothing
/// to trim: a closed or too-short path, or `by <= 0`.
fn trim_end_parts(bez: &BezPath, by: f64) -> Option<(BezPath, usize, f32)> {
    use vector_ink::kurbo::{ParamCurve, ParamCurveArclen, PathEl, PathSeg};
    if by.is_nan()
        || by <= 0.0
        || bez
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::ClosePath))
    {
        return None;
    }
    let segs: Vec<PathSeg> = bez.segments().collect();
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(1e-3)).collect();
    if segs.is_empty() || lens.iter().sum::<f64>() <= by * 1.05 {
        return None;
    }
    let mut left = by;
    let mut cut = segs.len() - 1;
    while lens[cut] <= left && cut > 0 {
        left -= lens[cut];
        cut -= 1;
    }
    let keep = (lens[cut] - left).max(0.0);
    let t = segs[cut].inv_arclen(keep, 1e-3);
    let mut out = BezPath::new();
    let mut seg_i = 0;
    for el in bez.elements() {
        match el {
            PathEl::MoveTo(_) => {
                if seg_i <= cut {
                    out.push(*el);
                }
            }
            _ => {
                if seg_i < cut {
                    out.push(*el);
                } else if seg_i == cut {
                    out.push(segs[cut].subsegment(0.0..t).as_path_el());
                }
                seg_i += 1;
            }
        }
    }
    let frac = if lens[cut] > 1e-9 {
        keep / lens[cut]
    } else {
        0.0
    };
    Some((out, segs.len() - 1 - cut, frac as f32))
}

/// `bez` shortened by `by` along its arc length, so a stroke under an
/// arrowhead ends at the head's base and not at its tip. Segments shorter
/// than what is left to trim are dropped whole.
pub fn trim_end(bez: &BezPath, by: f64) -> BezPath {
    trim_end_parts(bez, by).map_or_else(|| bez.clone(), |(out, _, _)| out)
}

/// [`trim_end`] for a per-vertex stroke: widths and colors of dropped
/// vertices go with them, and the new last vertex takes the tip found at
/// the cut.
pub fn trim_tipped_end(t: &TippedStroke, by: f64) -> TippedStroke {
    use vector_ink::kurbo::PathEl;
    let single = t
        .bez
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count()
        == 1;
    let parts = trim_end_parts(&t.bez, by).filter(|_| single);
    let Some((bez, dropped, frac)) = parts.filter(|_| t.widths.len() >= 2) else {
        return t.clone();
    };
    let n = t.widths.len();
    let cut_end = n - 1 - dropped;
    let keep_to = |v: &[f32]| -> Vec<f32> {
        let mut out = v[..cut_end].to_vec();
        out.push(v[cut_end - 1] + (v[cut_end] - v[cut_end - 1]) * frac);
        out
    };
    let colors = t.colors.as_ref().filter(|c| c.len() == n).map(|c| {
        let mut out = c[..cut_end].to_vec();
        let (a, b) = (c[cut_end - 1], c[cut_end]);
        out.push(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * frac));
        out
    });
    TippedStroke {
        bez,
        widths: keep_to(&t.widths),
        colors,
        ease: t.ease,
    }
}

/// How far a stroke under an arrowhead stops short of the tip: most of the
/// head's length, so the line tucks under the base.
pub fn arrow_trim(stroke_width: f32) -> f64 {
    (arrow_len(stroke_width) * 0.8) as f64
}

/// [`path_end_arrow`] for the path's first point.
pub fn path_start_arrow(bez: &BezPath, stroke_width: f32) -> Option<([f32; 2], [f32; 2])> {
    path_end_arrow(&bez.reverse_subpaths(), stroke_width)
}

/// The tip and inward direction of the head at end `i` (0 = first point,
/// 1 = last) of an open path, for a stroke `stroke_width` wide there.
pub fn path_arrow(bez: &BezPath, i: usize, stroke_width: f32) -> Option<([f32; 2], [f32; 2])> {
    if i == 0 {
        path_start_arrow(bez, stroke_width)
    } else {
        path_end_arrow(bez, stroke_width)
    }
}

pub fn path_arrow_sized(bez: &BezPath, end: usize, size: [f32; 2]) -> Option<([f32; 2], [f32; 2])> {
    if end == 0 {
        path_end_arrow_length(&bez.reverse_subpaths(), size[0])
    } else {
        path_end_arrow_length(bez, size[0])
    }
}

pub fn trim_arrow_sizes(bez: &BezPath, arrows: [bool; 2], sizes: [[f32; 2]; 2]) -> BezPath {
    let mut out = if arrows[1] {
        trim_end(bez, (sizes[1][0] * 0.8) as f64)
    } else {
        bez.clone()
    };
    if arrows[0] {
        out = trim_end(&out.reverse_subpaths(), (sizes[0][0] * 0.8) as f64).reverse_subpaths();
    }
    out
}

pub fn trim_tipped_arrow_sizes(
    t: &TippedStroke,
    arrows: [bool; 2],
    sizes: [[f32; 2]; 2],
) -> TippedStroke {
    let mut out = if arrows[1] {
        trim_tipped_end(t, (sizes[1][0] * 0.8) as f64)
    } else {
        t.clone()
    };
    if arrows[0] {
        out = reverse_tipped(&trim_tipped_end(
            &reverse_tipped(&out),
            (sizes[0][0] * 0.8) as f64,
        ));
    }
    out
}

/// Head tip and base colors sampled from the same curve as the body.
pub fn arrow_colors(t: &TippedStroke, end: usize, length: f32) -> Option<[[f32; 4]; 2]> {
    let oriented = if end == 0 {
        reverse_tipped(t)
    } else {
        t.clone()
    };
    let tip = *oriented.colors.as_ref()?.last()?;
    let cut = trim_tipped_end(&oriented, length as f64);
    let base = *cut.colors.as_ref()?.last()?;
    Some([tip, base])
}

/// `bez` shortened under the arrowheads `arrows` (first point, last point)
/// of a stroke `widths` wide at those ends.
pub fn trim_arrow_ends(bez: &BezPath, arrows: [bool; 2], widths: [f32; 2]) -> BezPath {
    let mut out = if arrows[1] {
        trim_end(bez, arrow_trim(widths[1]))
    } else {
        bez.clone()
    };
    if arrows[0] {
        out = trim_end(&out.reverse_subpaths(), arrow_trim(widths[0])).reverse_subpaths();
    }
    out
}

fn reverse_tipped(t: &TippedStroke) -> TippedStroke {
    let rev = |v: &[f32]| v.iter().rev().copied().collect::<Vec<_>>();
    TippedStroke {
        bez: t.bez.reverse_subpaths(),
        widths: rev(&t.widths),
        colors: t.colors.as_ref().map(|c| c.iter().rev().copied().collect()),
        ease: t.ease,
    }
}

/// [`trim_arrow_ends`] for a per-vertex stroke, each head sized to the
/// width at its own end ([`trim_tipped_end`]).
pub fn trim_tipped_arrow_ends(t: &TippedStroke, arrows: [bool; 2]) -> TippedStroke {
    let head = |t: &TippedStroke, first: bool| {
        let w = if first {
            t.widths.first()
        } else {
            t.widths.last()
        };
        arrow_trim(w.copied().unwrap_or(0.0))
    };
    let mut out = if arrows[1] {
        trim_tipped_end(t, head(t, false))
    } else {
        t.clone()
    };
    if arrows[0] {
        let by = head(&out, true);
        out = reverse_tipped(&trim_tipped_end(&reverse_tipped(&out), by));
    }
    out
}

#[cfg(test)]
mod arrow_tests {
    use super::*;
    use vector_ink::kurbo::{ParamCurve, ParamCurveArclen, Point};

    /// A long run, then a tiny hook the way a hand ends a freehand stroke.
    fn hooked() -> BezPath {
        let mut bez = BezPath::new();
        bez.move_to((0.0, 0.0));
        for k in 1..=40 {
            bez.line_to((k as f64 * 5.0, 0.0));
        }
        bez.line_to((200.6, 1.2));
        bez.line_to((200.8, 2.2));
        bez
    }

    #[test]
    fn an_arrow_aims_along_the_curve_not_the_last_hook() {
        let (tip, into) = path_end_arrow(&hooked(), 10.0).unwrap();
        assert_eq!(tip, [200.8, 2.2]);
        assert!(into[0] < -0.99, "points back along the run, got {into:?}");
        let [_, b, c] = arrow_head(tip, into, 10.0);
        let base = [(b[0] + c[0]) * 0.5, (b[1] + c[1]) * 0.5];
        assert!(
            base[1].abs() < 3.0,
            "the base sits on the curve, got {base:?}"
        );
    }

    #[test]
    fn an_arrow_on_an_arc_sets_its_base_on_the_arc() {
        let arc = vector_ink::kurbo::Arc::new(
            Point::new(0.0, 0.0),
            (60.0, 60.0),
            std::f64::consts::PI,
            -std::f64::consts::PI,
            0.0,
        );
        let bez = vector_ink::kurbo::Shape::to_path(&arc, 0.01);
        let (tip, into) = path_end_arrow(&bez, 10.0).unwrap();
        let [_, b, c] = arrow_head(tip, into, 10.0);
        let base = Point::new(((b[0] + c[0]) * 0.5) as f64, ((b[1] + c[1]) * 0.5) as f64);
        let off = (base.to_vec2().hypot() - 60.0).abs();
        assert!(off < 0.5, "base {off} px off the arc");
    }

    #[test]
    fn trimming_crosses_short_segments() {
        let bez = hooked();
        let by = arrow_trim(10.0);
        let trimmed = trim_end(&bez, by);
        let before: f64 = bez.segments().map(|s| s.arclen(1e-3)).sum();
        let after: f64 = trimmed.segments().map(|s| s.arclen(1e-3)).sum();
        assert!(
            (before - after - by).abs() < 1e-2,
            "{before} - {after} vs {by}"
        );
        let end = trimmed.segments().last().unwrap().eval(1.0);
        assert!(
            end.x < 200.0 - by * 0.5,
            "the body ends under the head, at {end:?}"
        );
    }

    #[test]
    fn trimming_a_tipped_stroke_keeps_one_tip_per_vertex() {
        let bez = hooked();
        let n = bez.segments().count() + 1;
        let t = TippedStroke {
            bez,
            widths: (0..n).map(|i| i as f32).collect(),
            colors: Some((0..n).map(|i| [i as f32, 0.0, 0.0, 1.0]).collect()),
            ease: vector_ink::TipEase::default(),
        };
        let cut = trim_tipped_end(&t, arrow_trim(10.0));
        let vertices = cut.bez.segments().count() + 1;
        assert!(vertices < n, "short trailing segments are gone");
        assert_eq!(cut.widths.len(), vertices);
        assert_eq!(cut.colors.as_ref().unwrap().len(), vertices);
        let last = *cut.widths.last().unwrap();
        assert!(last > (vertices - 2) as f32 && last <= (vertices - 1) as f32);
    }
}
