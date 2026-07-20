//! Dash splitting along a polyline by arc length.

use crate::geom::{dist, lerp, pt, EPS};

/// Split a polyline into contiguous "on" dash runs (deterministic).
pub(crate) fn dash_on_runs(points: &[[f32; 2]], pattern: &[f32], phase: f32) -> Vec<Vec<[f32; 2]>> {
    if points.len() < 2 || pattern.is_empty() {
        return Vec::new();
    }
    let period: f32 = pattern.iter().copied().sum();
    if period <= EPS {
        return Vec::new();
    }
    for &p in pattern {
        if p <= 0.0 || !p.is_finite() {
            return vec![points.to_vec()];
        }
    }
    if !phase.is_finite() {
        return Vec::new();
    }

    let total_len = polyline_length(points);
    if total_len <= EPS {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let mut s = 0.0f32;
    let mut phase_off = phase.rem_euclid(period);
    let mut pat_i = 0usize;
    let mut on = true;

    while phase_off >= pattern[pat_i] - EPS {
        phase_off -= pattern[pat_i];
        on = !on;
        pat_i = (pat_i + 1) % pattern.len();
    }
    let mut seg_remaining = pattern[pat_i] - phase_off;
    let mut in_run = on;
    let mut run_start = s;
    if !on {
        s += seg_remaining;
        pat_i = (pat_i + 1) % pattern.len();
        seg_remaining = pattern[pat_i];
        in_run = true;
        run_start = s;
    }

    while s < total_len - EPS {
        let step = seg_remaining.min(total_len - s);
        let s_next = s + step;
        if in_run {
            let a = point_at_length(points, run_start);
            let b = point_at_length(points, s_next);
            if dist(a, b) > EPS {
                runs.push(vec![a, b]);
            }
        }
        s = s_next;
        seg_remaining -= step;
        if seg_remaining <= EPS {
            in_run = !in_run;
            if in_run {
                run_start = s;
            }
            pat_i = (pat_i + 1) % pattern.len();
            seg_remaining = pattern[pat_i];
        }
    }

    merge_collinear_runs(&mut runs, points);
    runs
}

fn polyline_length(points: &[[f32; 2]]) -> f32 {
    points.windows(2).map(|w| dist(w[0], w[1])).sum()
}

fn point_at_length(points: &[[f32; 2]], mut s: f32) -> [f32; 2] {
    if points.is_empty() {
        return pt(0.0, 0.0);
    }
    if s <= 0.0 {
        return points[0];
    }
    for w in points.windows(2) {
        let seg = dist(w[0], w[1]);
        if seg <= EPS {
            continue;
        }
        if s <= seg {
            let t = s / seg;
            return [lerp(w[0][0], w[1][0], t), lerp(w[0][1], w[1][1], t)];
        }
        s -= seg;
    }
    *points.last().unwrap()
}

fn merge_collinear_runs(runs: &mut Vec<Vec<[f32; 2]>>, _points: &[[f32; 2]]) {
    if runs.len() <= 1 {
        return;
    }
    let mut merged: Vec<Vec<[f32; 2]>> = Vec::with_capacity(runs.len());
    for run in runs.drain(..) {
        if run.len() < 2 {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if dist(*last.last().unwrap(), run[0]) < EPS {
                last.extend_from_slice(&run[1..]);
                continue;
            }
        }
        merged.push(run);
    }
    *runs = merged;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hundred_unit_line_dash_10_5() {
        let points = vec![pt(0.0, 0.0), pt(100.0, 0.0)];
        let runs = dash_on_runs(&points, &[10.0, 5.0], 0.0);
        assert!(runs.len() >= 5, "runs = {}", runs.len());
        assert!(runs.len() <= 7);
    }
}
