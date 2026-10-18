//! Display variants for real-window viewport regression measurements.
use super::*;

/// Display passes the screenshot must match live; `true` adds a photo filter.
pub(super) const CAPTURE_CASES: [(ModelDisplay, bool); 6] = [
    (ModelDisplay::Shaded, false),
    (ModelDisplay::Arctic, false),
    (ModelDisplay::HiddenLine, false),
    (ModelDisplay::Material, false),
    (ModelDisplay::Depth, false),
    (ModelDisplay::Shaded, true),
];

pub(super) fn benchmark_mode() -> ModelDisplay {
    std::env::var("BENCH_MODEL_DISPLAY")
        .ok()
        .and_then(|s| ModelDisplay::from_key(&s))
        .unwrap_or_default()
}

impl Bench {
    pub(super) fn apply_capture_case(&mut self, case: usize) {
        let (mode, filtered) = CAPTURE_CASES[case];
        self.app.set_model_display(self.model, mode);
        let adjust = if filtered {
            ImageAdjust {
                brightness: 1.15,
                contrast: 0.9,
                sepia: 0.4,
                hue_deg: 25.0,
                ..ImageAdjust::default()
            }
        } else {
            ImageAdjust::default()
        };
        self.app.patch_nodes(&[self.model], |n| {
            slate_doc::scene::set_adjust(n, adjust);
        });
    }
}
