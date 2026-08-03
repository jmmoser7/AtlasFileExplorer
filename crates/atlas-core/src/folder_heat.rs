//! Folder heatmap metrics with outlier-resistant scaling.
//!
//! Network shares often contain a handful of ancient admin-seeded files that
//! would collapse a naïve min–max colour scale. Aggregation and normalisation
//! here are deliberately robust:
//!
//! - **Within a folder**, date metrics use the *median* of direct-file
//!   timestamps (propagated up through child folders that have no files of
//!   their own). A few 2009 admin dumps cannot pull a living project folder
//!   back to the Paleolithic.
//! - **Across folders**, values are log-compressed when appropriate (size),
//!   then mapped through a Tukey / percentile window (P10–P90, widened by
//!   the IQR fences) so extreme folders saturate instead of flattening the
//!   useful mid-range.

use crate::tree::DirNode;
use crate::types::FileEntry;

/// What a folder heatmap colours by. `Off` is handled by the app (no compute).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FolderHeatMetric {
    #[default]
    Size,
    Created,
    Modified,
}

impl FolderHeatMetric {
    pub fn label(self) -> &'static str {
        match self {
            FolderHeatMetric::Size => "file size",
            FolderHeatMetric::Created => "date created",
            FolderHeatMetric::Modified => "date modified",
        }
    }
}

/// Normalised heat per directory index (`None` = no usable data).
#[derive(Clone, Debug, Default)]
pub struct FolderHeatMap {
    pub heats: Vec<Option<f32>>,
}

/// Build a per-folder heat map for `metric`.
///
/// `dirs` and `entries` must be the live tree / entry tables (same indexing
/// conventions as [`crate::tree::Tree`]).
pub fn compute_folder_heat(
    dirs: &[DirNode],
    entries: &[FileEntry],
    metric: FolderHeatMetric,
) -> FolderHeatMap {
    if dirs.is_empty() {
        return FolderHeatMap::default();
    }

    let raw = match metric {
        FolderHeatMetric::Size => dirs.iter().map(|d| Some(d.desc_bytes as f64)).collect(),
        FolderHeatMetric::Created => aggregate_dates(dirs, entries, |e| e.ctime),
        FolderHeatMetric::Modified => aggregate_dates(dirs, entries, |e| e.mtime),
    };

    let transformed: Vec<Option<f64>> = raw
        .iter()
        .map(|v| {
            v.and_then(|x| {
                if !x.is_finite() || x < 0.0 {
                    return None;
                }
                Some(match metric {
                    // File sizes are roughly log-normal; without the log a
                    // single multi-GB archive folder eats the whole scale.
                    FolderHeatMetric::Size => (1.0 + x).ln(),
                    FolderHeatMetric::Created | FolderHeatMetric::Modified => x,
                })
            })
        })
        .collect();

    let heats = robust_normalize(&transformed);
    FolderHeatMap { heats }
}

/// Bottom-up median timestamps. Direct files dominate; empty dirs inherit the
/// median of their children's values so portals still colour usefully.
fn aggregate_dates(
    dirs: &[DirNode],
    entries: &[FileEntry],
    pick: impl Fn(&FileEntry) -> i64,
) -> Vec<Option<f64>> {
    let n = dirs.len();
    let mut out = vec![None; n];
    if n == 0 {
        return out;
    }
    // Deepest first so children resolve before parents, regardless of whether
    // the caller passed a single rooted tree or a flat test forest.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(dirs[i].depth));

    for di in order {
        let d = &dirs[di];
        let mut samples: Vec<i64> = d
            .files
            .iter()
            .filter_map(|&f| {
                let e = entries.get(f as usize)?;
                if e.dead {
                    return None;
                }
                let t = pick(e);
                (t > 0).then_some(t)
            })
            .collect();
        if samples.is_empty() {
            for &c in &d.child_dirs {
                if let Some(v) = out.get(c as usize).copied().flatten() {
                    samples.push(v.round() as i64);
                }
            }
        }
        out[di] = median_i64(&mut samples).map(|v| v as f64);
    }
    out
}

fn median_i64(values: &mut [i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[mid])
    } else {
        // Upper median: resistant to a single ancient sample on the low side.
        Some(values[mid])
    }
}

/// Map transformed values onto `[0, 1]` using a robust window.
///
/// Window = intersection of the P10–P90 span with Tukey fences
/// `[Q1 − 1.5·IQR, Q3 + 1.5·IQR]`. Values outside saturate at 0 / 1 instead
/// of dragging the scale. Falls back to mid-grey when there is no spread.
pub fn robust_normalize(values: &[Option<f64>]) -> Vec<Option<f32>> {
    let mut finite: Vec<f64> = values
        .iter()
        .filter_map(|v| *v)
        .filter(|v| v.is_finite())
        .collect();
    if finite.is_empty() {
        return values.iter().map(|_| None).collect();
    }
    finite.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let p10 = percentile_sorted(&finite, 0.10);
    let p90 = percentile_sorted(&finite, 0.90);
    let q1 = percentile_sorted(&finite, 0.25);
    let q3 = percentile_sorted(&finite, 0.75);
    let iqr = (q3 - q1).max(0.0);
    let fence_lo = q1 - 1.5 * iqr;
    let fence_hi = q3 + 1.5 * iqr;

    // Prefer the percentile window; never wider than the fences, never
    // inverted. A tiny epsilon stops divide-by-zero on flat distributions.
    let mut lo = p10.max(fence_lo);
    let mut hi = p90.min(fence_hi);
    if hi - lo < 1e-9 {
        lo = finite[0];
        hi = *finite.last().unwrap_or(&lo);
    }
    if hi - lo < 1e-9 {
        return values.iter().map(|v| v.map(|_| 0.5)).collect();
    }

    values
        .iter()
        .map(|v| {
            v.map(|x| {
                let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
                // Gentle ease keeps mid-range folders visually distinct.
                smoothstep(t)
            })
        })
        .collect()
}

fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let p = p.clamp(0.0, 1.0);
    let x = p * (sorted.len() - 1) as f64;
    let i = x.floor() as usize;
    let f = x - i as f64;
    let a = sorted[i];
    let b = sorted[(i + 1).min(sorted.len() - 1)];
    a + (b - a) * f
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::DirNode;
    use crate::types::{Family, FileEntry};
    use eframe::egui::{Pos2, Rect, Vec2};
    use std::path::PathBuf;

    fn empty_dir(name: &str) -> DirNode {
        DirNode {
            name: name.into(),
            rel: name.into(),
            depth: 0,
            child_dirs: Vec::new(),
            files: Vec::new(),
            desc_files: 0,
            desc_bytes: 0,
            desc_matches: 0,
            ctime: 0,
            owner: String::new(),
            collapsed: false,
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 40.0,
            bounds: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0)),
            grid_bounds: None,
            grid_order: Vec::new(),
            placed: true,
            portal_samples: Vec::new(),
        }
    }

    fn entry(size: u64, ctime: i64, mtime: i64) -> FileEntry {
        FileEntry {
            path: PathBuf::from("x"),
            rel: "x".into(),
            name: "x".into(),
            name_lc: "x".into(),
            ext: "png".into(),
            size,
            mtime,
            ctime,
            owner: String::new(),
            family: Family::Image,
            dead: false,
        }
    }

    #[test]
    fn ancient_outlier_does_not_collapse_date_scale() {
        // Nine "normal" folders around 2024, one admin dump from 2009.
        let recent = 1_700_000_000i64; // ~2023-11
        let mut dirs: Vec<DirNode> = (0..10).map(|i| empty_dir(&format!("d{i}"))).collect();
        let mut entries = Vec::new();
        for (i, d) in dirs.iter_mut().enumerate() {
            let t = if i == 0 {
                1_230_000_000 // 2009
            } else {
                recent + (i as i64) * 86_400
            };
            d.files.push(entries.len() as u32);
            d.desc_files = 1;
            entries.push(entry(1000, t, t));
        }

        let map = compute_folder_heat(&dirs, &entries, FolderHeatMetric::Modified);
        let heats: Vec<f32> = map.heats.iter().map(|h| h.expect("heat")).collect();
        assert!(
            heats[0] < 0.15,
            "outlier should sit at the floor, got {}",
            heats[0]
        );
        let lo = heats[1..].iter().cloned().fold(f32::INFINITY, f32::min);
        let hi = heats[1..].iter().cloned().fold(0.0_f32, f32::max);
        assert!(
            hi - lo > 0.25,
            "recent folders should retain spread, got {lo}..{hi}"
        );
    }

    #[test]
    fn within_folder_median_ignores_admin_seed_file() {
        // d0: recent project polluted by one ancient admin seed.
        // d1: folder that is *only* ancient (what a min() aggregator would
        // pretend d0 is). d2..d5: clean recent peers that set the scale.
        let mut dirs: Vec<DirNode> = (0..6).map(|i| empty_dir(&format!("d{i}"))).collect();
        let mut entries = Vec::new();
        for t in [
            1_230_000_000i64,
            1_700_000_000,
            1_700_100_000,
            1_700_200_000,
        ] {
            dirs[0].files.push(entries.len() as u32);
            entries.push(entry(100, t, t));
        }
        dirs[0].desc_files = 4;
        dirs[1].files.push(entries.len() as u32);
        entries.push(entry(100, 1_230_000_000, 1_230_000_000));
        dirs[1].desc_files = 1;
        for (i, d) in dirs.iter_mut().enumerate().skip(2) {
            let base = 1_700_000_000 + (i as i64) * 86_400;
            for k in 0..3 {
                d.files.push(entries.len() as u32);
                entries.push(entry(100, base + k * 3_600, base + k * 3_600));
            }
            d.desc_files = 3;
        }

        let map = compute_folder_heat(&dirs, &entries, FolderHeatMetric::Created);
        let polluted = map.heats[0].expect("polluted");
        let ancient_only = map.heats[1].expect("ancient");
        assert!(
            polluted > ancient_only + 0.35,
            "median must lift the polluted folder off the ancient floor \
             ({polluted} vs ancient-only {ancient_only})"
        );
    }

    #[test]
    fn giant_folder_does_not_flatten_size_heatmap() {
        let mut dirs: Vec<DirNode> = (0..8).map(|i| empty_dir(&format!("d{i}"))).collect();
        for (i, d) in dirs.iter_mut().enumerate() {
            d.desc_bytes = if i == 0 {
                50_000_000_000 // 50 GB archive
            } else {
                (1_000_000 * (i as u64)).max(100_000) // 0.1–7 MB
            };
            d.desc_files = 1;
        }
        let map = compute_folder_heat(&dirs, &[], FolderHeatMetric::Size);
        let heats: Vec<f32> = map.heats.iter().map(|h| h.unwrap()).collect();
        assert!(heats[0] > 0.85, "giant saturates high");
        let others: Vec<f32> = heats[1..].to_vec();
        let spread = others.iter().cloned().fold(0.0_f32, f32::max)
            - others.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!(
            spread > 0.2,
            "log+percentile scale should keep small folders distinct, spread={spread}"
        );
    }

    #[test]
    fn robust_normalize_flat_distribution_is_mid() {
        let vals = vec![Some(5.0), Some(5.0), Some(5.0)];
        let out = robust_normalize(&vals);
        assert!(out
            .iter()
            .all(|v| matches!(v, Some(t) if (*t - 0.5).abs() < 1e-3)));
    }
}
