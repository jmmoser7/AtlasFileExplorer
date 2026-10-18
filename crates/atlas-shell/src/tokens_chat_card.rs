//! Chat-card header dials. World units, scaled at paint with `canvas_scale::px`.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

static HEADER_PREVIEW: AtomicBool = AtomicBool::new(false);

/// Hold the collapse chevron visible while the pointer is on the tuner.
pub fn chat_header_preview_locked() -> bool {
    HEADER_PREVIEW.load(Ordering::Relaxed)
}

pub fn set_chat_header_preview_locked(on: bool) {
    HEADER_PREVIEW.store(on, Ordering::Relaxed);
}

/// Header of a Slate chat card: title, output dot, collapse chevron, `•••`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatCardTokens {
    /// How far the title's x-height top sits below the output dot's top.
    pub title_x_height_offset: f32,
    pub title_size: f32,
    pub dot_diameter: f32,
    /// Inset of the header dots from the card's left and right edges.
    pub dot_inset: f32,
    /// Distance from the card's top edge to the header-dot centers.
    pub dot_top: f32,
    /// Chevron arm half-width. The rise stays in proportion.
    pub chevron_size: f32,
    pub chevron_stroke: f32,
    /// Chevron center, to the left of the `•••` cluster.
    pub chevron_x: f32,
    pub menu_dot_diameter: f32,
    /// Center distance of the three menu dots.
    pub menu_dot_spacing: f32,
    pub chevron_fade_ms: f32,
}

impl Default for ChatCardTokens {
    fn default() -> Self {
        Self {
            title_x_height_offset: 0.0,
            title_size: 13.0,
            dot_diameter: 7.0,
            dot_inset: 6.0,
            dot_top: 10.0,
            chevron_size: 2.4,
            chevron_stroke: 0.55,
            chevron_x: 8.0,
            menu_dot_diameter: 4.2,
            menu_dot_spacing: 5.88,
            chevron_fade_ms: 100.0,
        }
    }
}

impl ChatCardTokens {
    pub fn normalize(&mut self) {
        self.title_x_height_offset = self.title_x_height_offset.clamp(-24.0, 24.0);
        self.title_size = self.title_size.clamp(8.0, 28.0);
        self.dot_diameter = self.dot_diameter.clamp(2.0, 16.0);
        self.dot_inset = self.dot_inset.clamp(0.0, 32.0);
        self.dot_top = self.dot_top.clamp(0.0, 48.0);
        self.chevron_size = self.chevron_size.clamp(0.8, 8.0);
        self.chevron_stroke = self.chevron_stroke.clamp(0.3, 2.4);
        self.chevron_x = self.chevron_x.clamp(0.0, 40.0);
        self.menu_dot_diameter = self.menu_dot_diameter.clamp(1.0, 12.0);
        self.menu_dot_spacing = self.menu_dot_spacing.clamp(2.0, 24.0);
        self.chevron_fade_ms = self.chevron_fade_ms.clamp(0.0, 800.0);
    }

    pub fn round_for_storage(&mut self) {
        for value in [
            &mut self.title_x_height_offset,
            &mut self.title_size,
            &mut self.dot_diameter,
            &mut self.dot_inset,
            &mut self.dot_top,
            &mut self.chevron_size,
            &mut self.chevron_stroke,
            &mut self.chevron_x,
            &mut self.menu_dot_diameter,
            &mut self.menu_dot_spacing,
            &mut self.chevron_fade_ms,
        ] {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
    }
}
