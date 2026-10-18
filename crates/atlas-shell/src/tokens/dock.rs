//! Dock defaults and the scoped presentation for bottom-edge Tool capsules.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DockInlineTokens {
    pub open_duration: f32,
    pub primary_light_fill: [u8; 4],
    pub primary_light_ink: [u8; 4],
    pub primary_dark_fill: [u8; 4],
    pub primary_dark_ink: [u8; 4],
    pub feedback_fill: [u8; 4],
    pub feedback_ink: [u8; 4],
}

impl Default for DockInlineTokens {
    fn default() -> Self {
        Self {
            open_duration: 0.20,
            primary_light_fill: [54, 62, 72, 255],
            primary_light_ink: [255, 255, 255, 255],
            primary_dark_fill: [209, 216, 225, 255],
            primary_dark_ink: [21, 26, 32, 255],
            feedback_fill: [181, 31, 67, 255],
            feedback_ink: [255, 255, 255, 255],
        }
    }
}

impl DockInlineTokens {
    pub(super) fn normalize(&mut self) {
        self.open_duration = self.open_duration.clamp(0.05, 0.8);
    }

    pub(super) fn round_for_storage(&mut self) {
        self.open_duration = (self.open_duration * 1_000.0).round() / 1_000.0;
    }

    pub fn primary_colors(&self, dark: bool, feedback: bool) -> (Color32, Color32) {
        let (fill, ink) = if feedback {
            (self.feedback_fill, self.feedback_ink)
        } else if dark {
            (self.primary_dark_fill, self.primary_dark_ink)
        } else {
            (self.primary_light_fill, self.primary_light_ink)
        };
        (
            Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], fill[3]),
            Color32::from_rgba_unmultiplied(ink[0], ink[1], ink[2], ink[3]),
        )
    }
}

impl Default for DockTokens {
    fn default() -> Self {
        Self {
            icon_size: 34.0,
            icon_gap: 10.0,
            icon_text_size: 14.0,
            label_text_size: 11.0,
            squircle_exponent: 4.0,
            popover_width: 260.0,
            popover_max_height: 520.0,
            popover_gap: 10.0,
            popover_padding: 8.0,
            popover_corner_radius: 10.0,
            shadow_offset_x: 0.0,
            shadow_offset_y: 6.0,
            shadow_blur: 20.0,
            shadow_spread: 1.0,
            shadow_opacity: 0.26,
            // Grace while travelling from the primary to its form or palette.
            close_delay: 1.0,
            left_margin: 10.0,
            bottom_margin: 14.0,
            stack_gap: 8.0,
            partition_gap: 8.0,
            partition_extend: 48.0,
            partition_max_thickness: 2.4,
            partition_min_thickness: 0.4,
            partition_opacity: 0.45,
            tracer_width: 1.4,
            tracer_opacity: 0.55,
            tracer_corner_radius: 8.0,
            tracer_border_hit: 10.0,
            dashboard_describe_delay: 0.55,
            describe_fade_duration: 0.28,
            panel_open_duration: 0.18,
            inline: DockInlineTokens::default(),
            hover_chip_gap: 6.0,
            flyout_icon_scale: 0.65,
            palette: DockPaletteTokens::default(),
            advanced: DockAdvancedTokens::default(),
            light: DockThemeTokens::light(),
            dark: DockThemeTokens::dark(),
        }
    }
}
