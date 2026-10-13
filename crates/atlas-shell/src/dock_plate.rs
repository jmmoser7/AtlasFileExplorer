//! Dock icon plate colours: the squircle fill behind every palette icon, and
//! anything that must read as one of them (the suggestion-box button).

use crate::tokens::{DockPaletteTokens, DockThemeTokens};
use eframe::egui::Color32;

/// Dark mode lightens gray; light mode darkens it.
pub(crate) fn associate_shade(color: Color32, dark: bool, tint: f32) -> Color32 {
    let toward = if dark {
        Color32::from_rgba_unmultiplied(255, 255, 255, color.a())
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, color.a())
    };
    mix_icon_fill(color, toward, tint)
}

/// Signed luminance shift. Positive matches [`associate_shade`]; negative
/// flips the direction so a primary plate can sit darker than its host.
fn signed_shade(color: Color32, dark: bool, offset: f32) -> Color32 {
    if offset.abs() < 0.0005 {
        color
    } else if offset > 0.0 {
        associate_shade(color, dark, offset)
    } else {
        associate_shade(color, !dark, -offset)
    }
}

/// Idle fill of a primary dock plate. `primary_fill_mix` blends the
/// absolute icon token toward the secondary capsule plus its offset.
pub(crate) fn primary_plate_fill(
    th: &DockThemeTokens,
    p: &DockPaletteTokens,
    dark: bool,
) -> Color32 {
    let secondary = th.popover_fill_color().gamma_multiply(p.group_fill);
    let linked = signed_shade(secondary, dark, p.primary_fill_offset);
    mix_icon_fill(th.icon_fill_color(), linked, p.primary_fill_mix)
}

/// A primary plate's fill `hover_t` of the way into its hover state.
pub(crate) fn hover_plate_fill(
    base: Color32,
    th: &DockThemeTokens,
    p: &DockPaletteTokens,
    dark: bool,
    hover_t: f32,
) -> Color32 {
    let hover_c = th.icon_hover_color();
    let hover_c = Color32::from_rgba_unmultiplied(
        hover_c.r(),
        hover_c.g(),
        hover_c.b(),
        (hover_c.a() as f32 * p.icon_hover_opacity).round() as u8,
    );
    let hovered_fill = mix_icon_fill(base, hover_c, p.icon_hover_fill);
    let shaded = associate_shade(hovered_fill, dark, p.icon_hover_tint);
    mix_icon_fill(base, shaded, hover_t)
}

pub(crate) fn mix_icon_fill(base: Color32, accent: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (base.r() as f32 + (accent.r() as f32 - base.r() as f32) * t).round() as u8,
        (base.g() as f32 + (accent.g() as f32 - base.g() as f32) * t).round() as u8,
        (base.b() as f32 + (accent.b() as f32 - base.b() as f32) * t).round() as u8,
        (base.a() as f32 + (accent.a() as f32 - base.a() as f32) * t).round() as u8,
    )
}
