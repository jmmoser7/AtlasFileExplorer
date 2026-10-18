//! One overlay engine; authored CAD regions additionally restore their inputs.
use super::{close_ring, from_f64, Polygon, TrimPolys};
use i_overlay::core::{fill_rule::FillRule, overlay_rule::OverlayRule};
use i_overlay::float::overlay::FloatOverlay;

/// Subject minus clip without the authored-CAD coordinate/collinearity cleanup.
///
/// Use for attributed render triangles: moving one shared edge independently
/// can open raster coverage cracks or change its interpolated UV/color field.
/// This retains the overlay engine's float quantization; it does not promise
/// exact arithmetic. Authored region operations use `boolean_difference`.
pub fn boolean_difference_raw(subj: &Polygon, clip: &Polygon) -> TrimPolys {
    let s: Vec<Vec<[f64; 2]>> = subj.iter().map(|c| close_ring(c)).collect();
    let c: Vec<Vec<[f64; 2]>> = clip.iter().map(|c| close_ring(c)).collect();
    overlay_shapes_raw(&s, &c, OverlayRule::Difference)
}

pub(super) fn overlay_shapes(
    subj: &[Vec<[f64; 2]>],
    clip: &[Vec<[f64; 2]>],
    rule: OverlayRule,
) -> TrimPolys {
    let mut out = overlay_shapes_raw(subj, clip, rule);
    if !subj.is_empty() && !clip.is_empty() {
        let sources = subj
            .iter()
            .chain(clip)
            .map(|c| c.iter().copied().map(from_f64).collect())
            .collect::<Vec<_>>();
        crate::clean::clean_boolean_result(&mut out, &sources);
    }
    out
}

fn overlay_shapes_raw(
    subj: &[Vec<[f64; 2]>],
    clip: &[Vec<[f64; 2]>],
    rule: OverlayRule,
) -> TrimPolys {
    if subj.is_empty() {
        return Vec::new();
    }
    if clip.is_empty() {
        return subj
            .iter()
            .map(|c| vec![c.iter().copied().map(from_f64).collect()])
            .collect();
    }
    let mut overlay = FloatOverlay::with_subj_and_clip(subj, clip);
    shapes_from_overlay(overlay.overlay(rule, FillRule::EvenOdd))
}

pub(super) fn shapes_from_overlay(raw: Vec<Vec<Vec<[f64; 2]>>>) -> TrimPolys {
    raw.into_iter()
        .filter_map(|shape| {
            let contours: Polygon = shape
                .into_iter()
                .map(|c| c.into_iter().map(from_f64).collect())
                .filter(|c: &Vec<[f32; 2]>| c.len() >= 3)
                .collect();
            (!contours.is_empty()).then_some(contours)
        })
        .collect()
}
