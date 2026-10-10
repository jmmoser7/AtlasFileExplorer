use serde::{Deserialize, Serialize};

/// Bottom toast stack (File Atlas + Slate).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ToastTokens {
    pub max_width_px: f32,
    pub max_width_fraction: f32,
    pub min_width_px: f32,
    pub above_palette_gap: f32,
    pub stack_gap: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub text_size: f32,
    pub corner_radius: f32,
}

impl Default for ToastTokens {
    fn default() -> Self {
        Self {
            max_width_px: 560.0,
            max_width_fraction: 0.6,
            min_width_px: 280.0,
            above_palette_gap: 14.0,
            stack_gap: 8.0,
            pad_x: 14.0,
            pad_y: 10.0,
            text_size: 13.0,
            corner_radius: 8.0,
        }
    }
}

impl ToastTokens {
    pub fn normalize(&mut self) {
        self.max_width_px = self.max_width_px.clamp(200.0, 900.0);
        self.max_width_fraction = self.max_width_fraction.clamp(0.35, 0.9);
        self.min_width_px = self.min_width_px.clamp(160.0, 560.0);
        self.above_palette_gap = self.above_palette_gap.clamp(4.0, 48.0);
        self.stack_gap = self.stack_gap.clamp(2.0, 24.0);
        self.pad_x = self.pad_x.clamp(6.0, 32.0);
        self.pad_y = self.pad_y.clamp(4.0, 24.0);
        self.text_size = self.text_size.clamp(10.0, 18.0);
        self.corner_radius = self.corner_radius.clamp(4.0, 16.0);
    }

    pub fn round_for_storage(&mut self) {
        for value in [
            &mut self.max_width_px,
            &mut self.max_width_fraction,
            &mut self.min_width_px,
            &mut self.above_palette_gap,
            &mut self.stack_gap,
            &mut self.pad_x,
            &mut self.pad_y,
            &mut self.text_size,
            &mut self.corner_radius,
        ] {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
    }
}
