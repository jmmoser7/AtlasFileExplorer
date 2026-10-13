//! Overflow peeling for the shared dock accordion layout.
use super::{icon_line_along, icon_line_cap};

/// Peel one column at a time from the widest group so icons stay in a
/// sideways row and only the overflow steps onto the next line.
pub(super) fn accordion_peel_inners(
    icon_counts: &[usize],
    start_inners: &[f32],
    icon: f32,
    gap: f32,
    group_pad: f32,
    group_gap: f32,
    budget: f32,
) -> Vec<f32> {
    let n = start_inners.len();
    let mut inners = start_inners.to_vec();
    if n == 0 {
        return inners;
    }
    let frame_along = |inners: &[f32]| -> f32 {
        inners.iter().map(|w| w + group_pad * 2.0).sum::<f32>()
            + group_gap * n.saturating_sub(1) as f32
    };
    let cols_of = |inner: f32, count: usize| -> usize {
        let cap = icon_line_cap(icon, gap, inner);
        if count > 0 {
            cap.min(count)
        } else {
            cap
        }
    };
    let mut guard = 0;
    while frame_along(&inners) > budget + 0.5 && guard < 256 {
        guard += 1;
        let mut best: Option<usize> = None;
        let mut best_cols = 1usize;
        for i in 0..n {
            let cols = cols_of(inners[i], icon_counts[i]);
            if cols > 1 && cols >= best_cols {
                best_cols = cols;
                best = Some(i);
            }
        }
        let Some(i) = best else {
            break;
        };
        inners[i] = icon_line_along(best_cols - 1, icon, gap);
    }
    inners
}
