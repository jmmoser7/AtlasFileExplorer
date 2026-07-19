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
}

impl Default for UiTokens {
    fn default() -> Self {
        Self {
            schema_version: 2,
            topbar: TopBarTokens::default(),
            dock: DockTokens::default(),
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
