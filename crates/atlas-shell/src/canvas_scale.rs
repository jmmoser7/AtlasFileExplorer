//! One rule for every object that lives on a Slate board or a File Atlas
//! canvas (pattern **P0.9**): screen size is `designed × zoom`.
//!
//! Designed sizes are the value at zoom = 1. There is no ceiling. A clamp
//! that freezes a fillet, a stroke, or a typeface while the host keeps
//! moving is the defect this module exists to prevent. When something is
//! too small to read or to see, drop it — do not hold a screen constant.
//!
//! **Named exceptions** (a contract must say so): window chrome in this
//! crate, and pointer-attached chrome (`P2.GhostFollow`). Hit slop may
//! stay screen-constant so a tiny object remains clickable; the *visible*
//! graphic still tracks the camera.
//!
//! Type goes through [`crate::canvas_text`]. Linear geometry (radii,
//! strokes, insets, handle squares, tab fillets) goes through [`px`].

use crate::canvas_text;
use eframe::egui::FontId;

/// Screen pixels for a designed size that lives on the canvas.
#[inline]
pub fn px(designed: f32, zoom: f32) -> f32 {
    designed * zoom
}

/// Proportional typeface at a designed size. Paint it through
/// [`crate::canvas_text::text`], not `Painter::text`.
#[inline]
pub fn font(designed: f32, zoom: f32) -> FontId {
    FontId::proportional(px(designed, zoom))
}

/// Hit slop in screen pixels — pointer-attached, not a painted size.
pub const HIT_SLOP_PX: f32 = 8.0;

/// Usable hit radius: the painted size, but never smaller than [`HIT_SLOP_PX`].
#[inline]
pub fn hit_px(designed: f32, zoom: f32) -> f32 {
    px(designed, zoom).max(HIT_SLOP_PX)
}

/// Drop this graphic rather than freeze it at a readable screen size.
#[inline]
pub fn too_small(screen_px: f32) -> bool {
    !canvas_text::legible(screen_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_sizes_double_when_zoom_doubles() {
        for designed in [1.5_f32, 5.0, 6.7, 14.0, 22.0] {
            for zoom in [0.05_f32, 0.5, 1.0, 4.0, 40.0] {
                let a = px(designed, zoom);
                let b = px(designed, zoom * 2.0);
                assert!(
                    (b - a * 2.0).abs() < 1e-4,
                    "{designed}px at z={zoom} stopped tracking: {a} → {b}"
                );
            }
        }
    }
}
