//! Rough prototype: slow drift grows a highlight, a quick retreat undoes it.
//! Off unless `SLATE_SEGMENT_GROW=1` or [`super::runtime::ImageSegmentRuntime::grow_debug`].

use super::SlateApp;
use eframe::egui::Pos2;
use std::sync::OnceLock;

const SLOW_PX: f32 = 3.0;
const RETREAT_PX: f32 = 14.0;
const HISTORY: usize = 8;

fn env_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var("SLATE_SEGMENT_GROW").ok().as_deref() == Some("1"))
}

impl SlateApp {
    /// Derived only. Does nothing unless the debug flag is on.
    pub(super) fn segment_grow_step(&mut self, screen: Pos2, zoom: f32) {
        if !self.image_segments.grow_debug && !env_enabled() {
            return;
        }
        let Some(last) = self.image_segments.grow_last else {
            self.image_segments.grow_last = Some(screen);
            return;
        };
        self.image_segments.grow_last = Some(screen);
        let travel = screen.distance(last);
        if travel >= RETREAT_PX {
            self.retreat_segment_mask();
            return;
        }
        if travel < 0.4 || travel > SLOW_PX {
            return;
        }
        let factor = 1.0 + 0.03 / zoom.max(0.25);
        self.expand_segment_mask(factor);
    }

    fn expand_segment_mask(&mut self, factor: f32) {
        let Some(result) = self.image_segments.result.clone() else {
            return;
        };
        if self.image_segments.grow_past.len() >= HISTORY {
            self.image_segments.grow_past.remove(0);
        }
        self.image_segments.grow_past.push(result.mask.clone());
        let next = expand_contours(&result.mask, factor);
        self.replace_segment_mask(result, next);
    }

    fn retreat_segment_mask(&mut self) {
        let Some(prev) = self.image_segments.grow_past.pop() else {
            return;
        };
        let Some(result) = self.image_segments.result.clone() else {
            return;
        };
        self.replace_segment_mask(result, (*prev).clone());
    }

    fn replace_segment_mask(
        &mut self,
        mut result: super::SegmentResult,
        contours: Vec<Vec<[f32; 2]>>,
    ) {
        let Some(path) = slate_doc::image_paint::region_path(contours.clone()) else {
            return;
        };
        result.local = super::region_node(path);
        result.mask = std::sync::Arc::new(contours);
        self.image_segments.result = Some(result);
    }
}

fn expand_contours(contours: &[Vec<[f32; 2]>], factor: f32) -> Vec<Vec<[f32; 2]>> {
    let Some(outer) = contours.first() else {
        return Vec::new();
    };
    let n = outer.len().max(1) as f32;
    let sum = outer
        .iter()
        .fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
    let c = [sum[0] / n, sum[1] / n];
    contours
        .iter()
        .map(|ring| {
            ring.iter()
                .map(|p| {
                    [
                        (c[0] + (p[0] - c[0]) * factor).clamp(0.0, 1.0),
                        (c[1] + (p[1] - c[1]) * factor).clamp(0.0, 1.0),
                    ]
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_step_grows_and_a_retreat_restores_the_previous_mask() {
        let ring = vec![vec![[0.2, 0.2], [0.6, 0.2], [0.6, 0.6], [0.2, 0.6]]];
        let grown = expand_contours(&ring, 1.1);
        let before = ring[0][1][0];
        let after = grown[0][1][0];
        assert!(after > before, "the outline moves outward");
        assert!(grown[0].iter().all(|p| (0.0..=1.0).contains(&p[0])));
    }
}
