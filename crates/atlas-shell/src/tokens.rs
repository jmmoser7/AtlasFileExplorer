//! Canonical, live-reloadable design tokens for shared chrome.
//!
//! The checked-in source of truth is `../ui-tokens.toml`. Normal builds embed
//! that file. A build with the `ui-tuner` feature can replace the in-memory
//! values while the app runs and save them back to the TOML file.

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::sync::{OnceLock, RwLock};

const EMBEDDED_TOKENS: &str = include_str!("../ui-tokens.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UiTokens {
    pub schema_version: u32,
    pub topbar: TopBarTokens,
    pub dock: DockTokens,
    pub home: HomeTokens,
}

impl Default for UiTokens {
    fn default() -> Self {
        Self {
            schema_version: 2,
            topbar: TopBarTokens::default(),
            dock: DockTokens::default(),
            home: HomeTokens::default(),
        }
    }
}

/// Cover Flow home shelf geometry and motion (see `home.rs`).
///
/// Layout is sigmoidal: `x(o) = side_step·o + center_bulge·tanh(o/bulge_width)`
/// opens a wide gap around the focused cover and packs side covers tightly.
/// Rotation and depth saturate with their own widths.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HomeTokens {
    /// Cover size as a fraction of the canvas height (covers are square).
    pub cover_frac: f32,
    /// Cover size clamp (px).
    pub cover_min: f32,
    pub cover_max: f32,
    /// Vertical center of the rack as a fraction of the canvas height.
    pub center_y_frac: f32,
    /// Asymptotic side-cover spacing (× cover size).
    pub side_step_frac: f32,
    /// Extra gap pushed outward around the focused cover (× cover size).
    pub center_bulge_frac: f32,
    /// Sigmoid width of the center gap — smaller = sharper falloff.
    pub bulge_width: f32,
    /// Saturating side-cover rotation (degrees; negative flips inward).
    pub angle_max_deg: f32,
    /// Sigmoid width of the rotation ramp — smaller = flips sooner.
    pub angle_width: f32,
    /// Corner fillet radius as a fraction of the card size (rounded corners).
    /// (Field name kept for saved-token compatibility.)
    pub corner_bevel_frac: f32,
    /// Ambient-occlusion halo reach behind each card (px).
    pub ao_size: f32,
    /// Ambient-occlusion strength (0 = off).
    pub ao_strength: f32,
    /// Saturating side-cover depth push-back (px).
    pub depth_max: f32,
    /// Sigmoid width of the depth ramp.
    pub depth_width: f32,
    /// Perspective focal length (px).
    pub focal: f32,
    /// Free-inertia velocity damping (1/s).
    pub friction: f32,
    /// Detent spring stiffness (1/s²).
    pub spring_stiffness: f32,
    /// Detent spring damping (1/s).
    pub spring_damping: f32,
    /// Below this |velocity| inertia hands over to the detent spring.
    pub snap_velocity: f32,
    /// Scroll pixels per album step.
    pub wheel_px_per_album: f32,
}

impl Default for HomeTokens {
    fn default() -> Self {
        Self {
            cover_frac: 0.48,
            cover_min: 180.0,
            cover_max: 340.0,
            center_y_frac: 0.46,
            side_step_frac: 0.16,
            center_bulge_frac: 0.52,
            bulge_width: 0.6,
            angle_max_deg: 62.0,
            angle_width: 0.55,
            corner_bevel_frac: 0.045,
            ao_size: 26.0,
            ao_strength: 0.55,
            depth_max: 90.0,
            depth_width: 0.9,
            focal: 900.0,
            friction: 4.2,
            spring_stiffness: 64.0,
            spring_damping: 15.0,
            snap_velocity: 0.9,
            wheel_px_per_album: 60.0,
        }
    }
}

impl HomeTokens {
    pub fn normalize(&mut self) {
        self.cover_frac = self.cover_frac.clamp(0.15, 0.85);
        self.cover_min = self.cover_min.clamp(60.0, 500.0);
        self.cover_max = self.cover_max.max(self.cover_min);
        self.center_y_frac = self.center_y_frac.clamp(0.2, 0.75);
        self.side_step_frac = self.side_step_frac.clamp(0.02, 0.8);
        self.center_bulge_frac = self.center_bulge_frac.clamp(0.0, 1.5);
        self.bulge_width = self.bulge_width.clamp(0.1, 3.0);
        self.angle_max_deg = self.angle_max_deg.clamp(-85.0, 85.0);
        self.angle_width = self.angle_width.clamp(0.1, 3.0);
        self.corner_bevel_frac = self.corner_bevel_frac.clamp(0.0, 0.2);
        self.ao_size = self.ao_size.clamp(0.0, 120.0);
        self.ao_strength = self.ao_strength.clamp(0.0, 1.0);
        self.depth_max = self.depth_max.clamp(0.0, 600.0);
        self.depth_width = self.depth_width.clamp(0.1, 4.0);
        self.focal = self.focal.clamp(200.0, 4000.0);
        self.friction = self.friction.clamp(0.2, 20.0);
        self.spring_stiffness = self.spring_stiffness.clamp(4.0, 400.0);
        self.spring_damping = self.spring_damping.clamp(1.0, 60.0);
        self.snap_velocity = self.snap_velocity.clamp(0.05, 5.0);
        self.wheel_px_per_album = self.wheel_px_per_album.clamp(10.0, 400.0);
    }

    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
        for value in [
            &mut self.cover_frac,
            &mut self.cover_min,
            &mut self.cover_max,
            &mut self.center_y_frac,
            &mut self.side_step_frac,
            &mut self.center_bulge_frac,
            &mut self.bulge_width,
            &mut self.angle_max_deg,
            &mut self.angle_width,
            &mut self.corner_bevel_frac,
            &mut self.ao_size,
            &mut self.ao_strength,
            &mut self.depth_max,
            &mut self.depth_width,
            &mut self.focal,
            &mut self.friction,
            &mut self.spring_stiffness,
            &mut self.spring_damping,
            &mut self.snap_velocity,
            &mut self.wheel_px_per_album,
        ] {
            round3(value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DockTokens {
    pub icon_size: f32,
    pub icon_gap: f32,
    pub icon_text_size: f32,
    pub label_text_size: f32,
    pub squircle_exponent: f32,
    pub popover_width: f32,
    pub popover_max_height: f32,
    pub popover_gap: f32,
    pub popover_padding: f32,
    pub popover_corner_radius: f32,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_opacity: f32,
    pub close_delay: f32,
    pub left_margin: f32,
    pub bottom_margin: f32,
    /// Gap between stacked open popovers.
    pub stack_gap: f32,
    /// Distance from the icon strip toward the canvas for the partition line.
    pub partition_gap: f32,
    /// How far the partition extends past the icon strip (along its axis).
    pub partition_extend: f32,
    /// Stroke thickness at the partition midpoint.
    pub partition_max_thickness: f32,
    /// Stroke thickness at the partition ends.
    pub partition_min_thickness: f32,
    pub partition_opacity: f32,
    pub tracer_width: f32,
    pub tracer_opacity: f32,
    pub tracer_corner_radius: f32,
    /// Hover hit band around a popover border that reveals the tracer.
    pub tracer_border_hit: f32,
    /// Seconds before a Dashboard hover chip expands to show `description`.
    pub dashboard_describe_delay: f32,
    /// Seconds to fade Dashboard description in the label chip.
    pub describe_fade_duration: f32,
    /// Seconds for hover preview / pinned panel ease-in.
    pub panel_open_duration: f32,
    /// Gap between icon top and label chip / preview anchor.
    pub hover_chip_gap: f32,
    pub light: DockThemeTokens,
    pub dark: DockThemeTokens,
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
            close_delay: 0.2,
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
            hover_chip_gap: 6.0,
            light: DockThemeTokens::light(),
            dark: DockThemeTokens::dark(),
        }
    }
}

impl DockTokens {
    pub fn normalize(&mut self) {
        self.icon_size = self.icon_size.max(18.0);
        self.icon_gap = self.icon_gap.max(0.0);
        self.squircle_exponent = self.squircle_exponent.clamp(2.0, 8.0);
        self.popover_width = self.popover_width.max(140.0);
        self.popover_max_height = self.popover_max_height.max(120.0);
        self.popover_padding = self.popover_padding.max(0.0);
        self.popover_corner_radius = self.popover_corner_radius.max(0.0);
        self.shadow_opacity = self.shadow_opacity.clamp(0.0, 1.0);
        self.close_delay = self.close_delay.clamp(0.0, 2.0);
        self.stack_gap = self.stack_gap.max(0.0);
        self.partition_gap = self.partition_gap.max(0.0);
        self.partition_extend = self.partition_extend.max(0.0);
        self.partition_max_thickness = self.partition_max_thickness.max(0.0);
        self.partition_min_thickness = self
            .partition_min_thickness
            .clamp(0.0, self.partition_max_thickness);
        self.partition_opacity = self.partition_opacity.clamp(0.0, 1.0);
        self.tracer_width = self.tracer_width.max(0.0);
        self.tracer_opacity = self.tracer_opacity.clamp(0.0, 1.0);
        self.tracer_corner_radius = self.tracer_corner_radius.max(0.0);
        self.tracer_border_hit = self.tracer_border_hit.max(2.0);
        self.dashboard_describe_delay = self.dashboard_describe_delay.clamp(0.0, 2.0);
        self.describe_fade_duration = self.describe_fade_duration.clamp(0.05, 1.0);
        self.panel_open_duration = self.panel_open_duration.clamp(0.05, 0.8);
        self.hover_chip_gap = self.hover_chip_gap.clamp(2.0, 24.0);
    }

    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
        for value in [
            &mut self.icon_size,
            &mut self.icon_gap,
            &mut self.icon_text_size,
            &mut self.label_text_size,
            &mut self.squircle_exponent,
            &mut self.popover_width,
            &mut self.popover_max_height,
            &mut self.popover_gap,
            &mut self.popover_padding,
            &mut self.popover_corner_radius,
            &mut self.shadow_offset_x,
            &mut self.shadow_offset_y,
            &mut self.shadow_blur,
            &mut self.shadow_spread,
            &mut self.shadow_opacity,
            &mut self.close_delay,
            &mut self.left_margin,
            &mut self.bottom_margin,
            &mut self.stack_gap,
            &mut self.partition_gap,
            &mut self.partition_extend,
            &mut self.partition_max_thickness,
            &mut self.partition_min_thickness,
            &mut self.partition_opacity,
            &mut self.tracer_width,
            &mut self.tracer_opacity,
            &mut self.tracer_corner_radius,
            &mut self.tracer_border_hit,
            &mut self.dashboard_describe_delay,
            &mut self.describe_fade_duration,
            &mut self.panel_open_duration,
            &mut self.hover_chip_gap,
        ] {
            round3(value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DockThemeTokens {
    pub icon_fill: [u8; 4],
    pub icon_hover: [u8; 4],
    pub icon_active: [u8; 4],
    pub popover_fill: [u8; 4],
    pub border: [u8; 4],
    pub text: [u8; 4],
    pub muted_text: [u8; 4],
}

impl DockThemeTokens {
    fn light() -> Self {
        Self {
            icon_fill: [248, 249, 250, 238],
            icon_hover: [232, 235, 239, 255],
            icon_active: [215, 235, 242, 255],
            popover_fill: [248, 249, 250, 248],
            border: [215, 220, 226, 255],
            text: [24, 25, 27, 255],
            muted_text: [112, 116, 122, 255],
        }
    }

    fn dark() -> Self {
        Self {
            icon_fill: [18, 21, 25, 238],
            icon_hover: [34, 39, 46, 255],
            icon_active: [22, 55, 58, 255],
            popover_fill: [18, 20, 22, 248],
            border: [54, 60, 66, 255],
            text: [235, 238, 241, 255],
            muted_text: [145, 150, 156, 255],
        }
    }

    pub fn icon_fill_color(&self) -> Color32 {
        rgba(self.icon_fill)
    }
    pub fn icon_hover_color(&self) -> Color32 {
        rgba(self.icon_hover)
    }
    pub fn icon_active_color(&self) -> Color32 {
        rgba(self.icon_active)
    }
    pub fn popover_fill_color(&self) -> Color32 {
        rgba(self.popover_fill)
    }
    pub fn border_color(&self) -> Color32 {
        rgba(self.border)
    }
    pub fn text_color(&self) -> Color32 {
        rgba(self.text)
    }
    pub fn muted_text_color(&self) -> Color32 {
        rgba(self.muted_text)
    }
}

impl Default for DockThemeTokens {
    fn default() -> Self {
        Self::dark()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TopBarTokens {
    pub height: f32,
    pub tab_top_inset: f32,
    pub tab_top_radius: f32,
    pub tab_shoulder_radius: f32,
    pub tab_horizontal_padding: f32,
    pub tab_close_width: f32,
    pub tab_title_chars: usize,
    pub tab_text_size: f32,
    pub tab_min_width: f32,
    pub tab_max_width: f32,
    pub plus_hit_width: f32,
    pub plus_radius: f32,
    pub plus_text_size: f32,
    pub icon_zone_width: f32,
    pub icon_size: f32,
    pub window_button_width: f32,
    pub glow_outer_width: f32,
    pub glow_outer_opacity: f32,
    pub glow_middle_width: f32,
    pub glow_middle_opacity: f32,
    pub glow_core_width: f32,
    pub glow_core_opacity: f32,
    pub inner_highlight_opacity: f32,
    pub portal: PortalMenuTokens,
    pub light: TopBarThemeTokens,
    pub dark: TopBarThemeTokens,
}

impl Default for TopBarTokens {
    fn default() -> Self {
        Self {
            height: 30.0,
            tab_top_inset: 4.0,
            tab_top_radius: 5.0,
            tab_shoulder_radius: 7.0,
            tab_horizontal_padding: 10.0,
            tab_close_width: 15.0,
            tab_title_chars: 36,
            tab_text_size: 12.0,
            tab_min_width: 108.0,
            tab_max_width: 280.0,
            plus_hit_width: 26.0,
            plus_radius: 8.0,
            plus_text_size: 13.0,
            icon_zone_width: 28.0,
            icon_size: 18.0,
            window_button_width: 40.0,
            glow_outer_width: 4.5,
            glow_outer_opacity: 0.10,
            glow_middle_width: 2.4,
            glow_middle_opacity: 0.28,
            glow_core_width: 1.0,
            glow_core_opacity: 0.88,
            inner_highlight_opacity: 0.12,
            portal: PortalMenuTokens::default(),
            light: TopBarThemeTokens::light(),
            dark: TopBarThemeTokens::dark(),
        }
    }
}

impl TopBarTokens {
    /// Keep hand-edited or live-edited values inside safe rendering bounds.
    pub fn normalize(&mut self) {
        self.height = self.height.max(1.0);
        self.tab_top_inset = self.tab_top_inset.clamp(0.0, (self.height - 1.0).max(0.0));
        self.tab_top_radius = self.tab_top_radius.max(0.5);
        self.tab_shoulder_radius = self.tab_shoulder_radius.max(0.5);
        if self.tab_min_width > self.tab_max_width {
            std::mem::swap(&mut self.tab_min_width, &mut self.tab_max_width);
        }
        for opacity in [
            &mut self.glow_outer_opacity,
            &mut self.glow_middle_opacity,
            &mut self.glow_core_opacity,
            &mut self.inner_highlight_opacity,
            &mut self.light.active_top_mix,
            &mut self.light.divider_strength,
            &mut self.light.accent_white_mix,
            &mut self.dark.active_top_mix,
            &mut self.dark.divider_strength,
            &mut self.dark.accent_white_mix,
            &mut self.portal.shadow_opacity,
        ] {
            *opacity = opacity.clamp(0.0, 1.0);
        }
        self.portal.width = self.portal.width.max(120.0);
        self.portal.submenu_width = self.portal.submenu_width.max(120.0);
        self.portal.row_height = self.portal.row_height.max(18.0);
        self.portal.panel_padding = self.portal.panel_padding.max(0.0);
        self.portal.corner_radius = self.portal.corner_radius.max(0.0);
        self.portal.panel_offset_x = self.portal.panel_offset_x.clamp(-100.0, 100.0);
        self.portal.close_delay = self.portal.close_delay.clamp(0.0, 2.0);
    }

    /// Keep the checked-in TOML readable after slider edits.
    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }

        for value in [
            &mut self.height,
            &mut self.tab_top_inset,
            &mut self.tab_top_radius,
            &mut self.tab_shoulder_radius,
            &mut self.tab_horizontal_padding,
            &mut self.tab_close_width,
            &mut self.tab_text_size,
            &mut self.tab_min_width,
            &mut self.tab_max_width,
            &mut self.plus_hit_width,
            &mut self.plus_radius,
            &mut self.plus_text_size,
            &mut self.icon_zone_width,
            &mut self.icon_size,
            &mut self.window_button_width,
            &mut self.glow_outer_width,
            &mut self.glow_outer_opacity,
            &mut self.glow_middle_width,
            &mut self.glow_middle_opacity,
            &mut self.glow_core_width,
            &mut self.glow_core_opacity,
            &mut self.inner_highlight_opacity,
            &mut self.light.active_top_mix,
            &mut self.light.divider_strength,
            &mut self.light.accent_white_mix,
            &mut self.dark.active_top_mix,
            &mut self.dark.divider_strength,
            &mut self.dark.accent_white_mix,
            &mut self.portal.width,
            &mut self.portal.submenu_width,
            &mut self.portal.row_height,
            &mut self.portal.panel_padding,
            &mut self.portal.corner_radius,
            &mut self.portal.panel_offset_x,
            &mut self.portal.panel_gap,
            &mut self.portal.submenu_gap,
            &mut self.portal.header_text_size,
            &mut self.portal.row_text_size,
            &mut self.portal.shortcut_text_size,
            &mut self.portal.chevron_text_size,
            &mut self.portal.separator_gap,
            &mut self.portal.shadow_offset_x,
            &mut self.portal.shadow_offset_y,
            &mut self.portal.shadow_blur,
            &mut self.portal.shadow_spread,
            &mut self.portal.shadow_opacity,
            &mut self.portal.close_delay,
        ] {
            round3(value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PortalMenuTokens {
    pub width: f32,
    pub submenu_width: f32,
    pub row_height: f32,
    pub panel_padding: f32,
    pub corner_radius: f32,
    pub panel_offset_x: f32,
    pub panel_gap: f32,
    pub submenu_gap: f32,
    pub header_text_size: f32,
    pub row_text_size: f32,
    pub shortcut_text_size: f32,
    pub chevron_text_size: f32,
    pub separator_gap: f32,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_opacity: f32,
    pub close_delay: f32,
    pub light: PortalMenuThemeTokens,
    pub dark: PortalMenuThemeTokens,
}

impl Default for PortalMenuTokens {
    fn default() -> Self {
        Self {
            width: 220.0,
            submenu_width: 250.0,
            row_height: 30.0,
            panel_padding: 10.0,
            corner_radius: 12.0,
            panel_offset_x: 5.0,
            panel_gap: 5.0,
            submenu_gap: 6.0,
            header_text_size: 12.5,
            row_text_size: 12.0,
            shortcut_text_size: 11.0,
            chevron_text_size: 15.0,
            separator_gap: 7.0,
            shadow_offset_x: 0.0,
            shadow_offset_y: 5.0,
            shadow_blur: 18.0,
            shadow_spread: 1.0,
            shadow_opacity: 0.28,
            close_delay: 0.18,
            light: PortalMenuThemeTokens::light(),
            dark: PortalMenuThemeTokens::dark(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PortalMenuThemeTokens {
    pub fill: [u8; 4],
    pub border: [u8; 4],
    pub hover: [u8; 4],
    pub text: [u8; 4],
    pub muted_text: [u8; 4],
}

impl PortalMenuThemeTokens {
    fn light() -> Self {
        Self {
            fill: [248, 249, 250, 250],
            border: [218, 221, 225, 255],
            hover: [231, 233, 236, 255],
            text: [24, 25, 27, 255],
            muted_text: [112, 116, 122, 255],
        }
    }

    fn dark() -> Self {
        Self {
            fill: [18, 20, 22, 250],
            border: [54, 57, 61, 255],
            hover: [39, 40, 43, 255],
            text: [239, 236, 226, 255],
            muted_text: [169, 166, 158, 255],
        }
    }

    pub fn fill_color(&self) -> Color32 {
        rgba(self.fill)
    }

    pub fn border_color(&self) -> Color32 {
        rgba(self.border)
    }

    pub fn hover_color(&self) -> Color32 {
        rgba(self.hover)
    }

    pub fn text_color(&self) -> Color32 {
        rgba(self.text)
    }

    pub fn muted_text_color(&self) -> Color32 {
        rgba(self.muted_text)
    }
}

impl Default for PortalMenuThemeTokens {
    fn default() -> Self {
        Self::dark()
    }
}

fn rgba(value: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(value[0], value[1], value[2], value[3])
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TopBarThemeTokens {
    pub bar: [u8; 4],
    pub bar_top: [u8; 4],
    pub inactive: [u8; 4],
    pub inactive_hover: [u8; 4],
    pub active_top_mix: f32,
    pub divider_strength: f32,
    pub accent_white_mix: f32,
}

impl TopBarThemeTokens {
    fn light() -> Self {
        Self {
            bar: [0xe4, 0xe7, 0xeb, 0xff],
            bar_top: [0xec, 0xee, 0xf2, 0xff],
            inactive: [0xd2, 0xd6, 0xdc, 0xff],
            inactive_hover: [0xea, 0xec, 0xf0, 0xff],
            active_top_mix: 0.08,
            divider_strength: 0.55,
            accent_white_mix: 0.20,
        }
    }

    fn dark() -> Self {
        Self {
            bar: [0x12, 0x16, 0x1c, 0xff],
            bar_top: [0x19, 0x1f, 0x27, 0xff],
            inactive: [0x19, 0x1e, 0x26, 0xff],
            inactive_hover: [0x21, 0x28, 0x32, 0xff],
            active_top_mix: 0.10,
            divider_strength: 0.65,
            accent_white_mix: 0.42,
        }
    }

    pub fn bar_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.bar[0], self.bar[1], self.bar[2], self.bar[3])
    }

    pub fn bar_top_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(
            self.bar_top[0],
            self.bar_top[1],
            self.bar_top[2],
            self.bar_top[3],
        )
    }

    pub fn inactive_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(
            self.inactive[0],
            self.inactive[1],
            self.inactive[2],
            self.inactive[3],
        )
    }

    pub fn inactive_hover_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(
            self.inactive_hover[0],
            self.inactive_hover[1],
            self.inactive_hover[2],
            self.inactive_hover[3],
        )
    }
}

impl Default for TopBarThemeTokens {
    fn default() -> Self {
        Self::dark()
    }
}

fn parse_embedded() -> UiTokens {
    let mut tokens = toml::from_str(EMBEDDED_TOKENS).unwrap_or_else(|error| {
        eprintln!("invalid atlas-shell/ui-tokens.toml ({error}); using factory defaults");
        UiTokens::default()
    });
    tokens.topbar.normalize();
    tokens.dock.normalize();
    tokens.home.normalize();
    tokens
}

fn store() -> &'static RwLock<UiTokens> {
    static STORE: OnceLock<RwLock<UiTokens>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(parse_embedded()))
}

/// Current tokens, including unsaved changes made by the live tuner.
pub fn current() -> UiTokens {
    store().read().expect("UI token lock poisoned").clone()
}

/// Replace live tokens. Used by the feature-gated UI tuner.
pub fn replace(mut tokens: UiTokens) {
    tokens.topbar.normalize();
    tokens.dock.normalize();
    tokens.home.normalize();
    *store().write().expect("UI token lock poisoned") = tokens;
}

/// Values embedded from the checked-in token file when this build was made.
pub fn embedded() -> UiTokens {
    parse_embedded()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_tokens_parse() {
        let tokens: UiTokens = toml::from_str(EMBEDDED_TOKENS).unwrap();
        assert!(tokens.topbar.height > 0.0);
        assert!(tokens.topbar.tab_max_width >= tokens.topbar.tab_min_width);
        assert!(tokens.dock.popover_width > 0.0);
    }
}
