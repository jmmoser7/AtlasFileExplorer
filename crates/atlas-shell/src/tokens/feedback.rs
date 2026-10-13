//! Shared feedback window geometry, in logical screen points.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedbackTokens {
    pub width: f32,
    pub inset: f32,
    pub radius: u8,
    pub canvas_buffer: f32,
    pub section_gap: f32,
    pub label_gap: f32,
    pub description_height: f32,
    pub links_height: f32,
    pub panel_light: [u8; 4],
    pub panel_dark: [u8; 4],
    pub field_light: [u8; 4],
    pub field_dark: [u8; 4],
}

impl Default for FeedbackTokens {
    fn default() -> Self {
        Self {
            width: 520.0,
            inset: 24.0,
            radius: 12,
            canvas_buffer: 12.0,
            section_gap: 17.0,
            label_gap: 7.0,
            description_height: 94.0,
            links_height: 54.0,
            panel_light: [252, 252, 253, 255],
            panel_dark: [30, 35, 42, 255],
            field_light: [255, 255, 255, 255],
            field_dark: [22, 26, 32, 255],
        }
    }
}

impl FeedbackTokens {
    pub fn normalize(&mut self) {
        self.width = self.width.clamp(320.0, 720.0);
        self.inset = self.inset.clamp(12.0, 32.0);
        self.radius = self.radius.min(24);
        self.canvas_buffer = self.canvas_buffer.clamp(0.0, 32.0);
        self.section_gap = self.section_gap.clamp(8.0, 32.0);
        self.label_gap = self.label_gap.clamp(2.0, 16.0);
        self.description_height = self.description_height.clamp(64.0, 240.0);
        self.links_height = self.links_height.clamp(36.0, 120.0);
    }
}
