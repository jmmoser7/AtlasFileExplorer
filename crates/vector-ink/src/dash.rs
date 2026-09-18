//! Dash splitting along a polyline by arc length.

use crate::geom::{dist, EPS};

/// Split into on-runs while retaining every interior curve sample. A dash is
/// a subpath, not a chord between its two endpoints. One forward traversal
/// keeps work proportional to input vertices plus dash boundaries.
pub(crate) fn dash_on_runs(points: &[[f32; 2]], pattern: &[f32], phase: f32) -> Vec<Vec<[f32; 2]>> {
    if points.len() < 2 || pattern.is_empty() || !phase.is_finite() {
        return Vec::new();
    }
    if pattern.iter().any(|v| !v.is_finite() || *v <= EPS) {
        return vec![points.to_vec()];
    }
    // SVG repeats odd-length patterns before wrapping the on/off state.
    let count = if pattern.len() % 2 == 0 {
        pattern.len()
    } else {
        pattern.len() * 2
    };
    let period: f32 = pattern.iter().sum::<f32>() * (count / pattern.len()) as f32;
    if !period.is_finite() {
        return Vec::new();
    }
    let mut offset = phase.rem_euclid(period);
    let mut index = 0;
    while offset >= pattern[index % pattern.len()] {
        offset -= pattern[index % pattern.len()];
        index = (index + 1) % count;
    }
    let mut remaining = pattern[index % pattern.len()] - offset;
    let mut runs = Vec::new();
    let mut run = Vec::new();
    for pair in points.windows(2) {
        let length = dist(pair[0], pair[1]);
        if length <= EPS {
            continue;
        }
        let mut used = 0.0;
        while used < length - EPS {
            let step = remaining.min(length - used);
            let point = |at: f32| {
                [
                    pair[0][0] + (pair[1][0] - pair[0][0]) * (at / length),
                    pair[0][1] + (pair[1][1] - pair[0][1]) * (at / length),
                ]
            };
            if index % 2 == 0 {
                if run.is_empty() {
                    run.push(point(used));
                }
                run.push(point(used + step));
            }
            used += step;
            remaining -= step;
            if remaining <= EPS {
                if run.len() >= 2 {
                    runs.push(std::mem::take(&mut run));
                }
                index = (index + 1) % count;
                remaining = pattern[index % pattern.len()];
            }
        }
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    // A dash crossing a closed contour's seam is one run, with no caps at
    // the arbitrary MoveTo vertex.
    if runs.len() > 1
        && dist(points[0], *points.last().unwrap()) < EPS
        && dist(runs[0][0], points[0]) < EPS
        && dist(*runs.last().unwrap().last().unwrap(), points[0]) < EPS
    {
        let first = runs.remove(0);
        runs.last_mut().unwrap().extend_from_slice(&first[1..]);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;

    #[test]
    fn dash_keeps_every_curve_sample_between_its_boundaries() {
        let points = vec![
            [0.0, 0.0],
            [5.0, 5.0],
            [10.0, 0.0],
            [15.0, 5.0],
            [20.0, 0.0],
        ];
        let runs = dash_on_runs(&points, &[40.0, 5.0], 0.0);
        assert_eq!(runs, vec![points]);
        let runs = dash_on_runs(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], &[15.0, 5.0], 0.0);
        assert_eq!(runs, vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]]]);
    }

    #[test]
    fn closed_seam_and_odd_dash_patterns_preserve_continuity() {
        let points = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ];
        let runs = dash_on_runs(&points, &[15.0, 10.0], 0.0);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].contains(&[0.0, 0.0]));
        assert_eq!(runs[0].first(), Some(&[5.0, 10.0]));
        assert_eq!(runs[0].last(), Some(&[10.0, 5.0]));
        let runs = dash_on_runs(&[[0.0, 0.0], [20.0, 0.0]], &[5.0], 5.0);
        assert_eq!(
            runs,
            vec![
                vec![[5.0, 0.0], [10.0, 0.0]],
                vec![[15.0, 0.0], [20.0, 0.0]]
            ]
        );
    }

    #[test]
    fn hundred_unit_line_dash_10_5() {
        let points = vec![pt(0.0, 0.0), pt(100.0, 0.0)];
        let runs = dash_on_runs(&points, &[10.0, 5.0], 0.0);
        assert!(runs.len() >= 5, "runs = {}", runs.len());
        assert!(runs.len() <= 7);
    }
}
