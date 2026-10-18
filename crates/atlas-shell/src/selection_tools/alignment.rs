//! Compact selection-alignment chrome. Scene commands remain in the application.
use crate::{icons, theme::Palette};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};

// The align-widget contract names this screen-space exception to P0.9.
pub const ICON_PX: f32 = 14.0;
pub const ICON_GAP_PX: f32 = 2.0;
pub const FRAME_OUTSET_PX: f32 = 20.0;
pub const HIT_PAD_PX: f32 = 2.0;
pub const DISABLED_ALPHA: f32 = 0.32;

/// Base clearance beyond the width stringer; the alignment row sits one pitch farther out.
pub fn bottom_outset_px(zoom: f32) -> f32 {
    let z = zoom.max(0.05);
    FRAME_OUTSET_PX.max(super::STRINGER_GAP * z + ICON_PX * 0.5 + 10.0 * z)
}

pub struct AlignmentLayout {
    /// Left, centre, right; distribution directly below centre, away from the selection.
    pub bottom: [Rect; 4],
    /// Top, middle, bottom; distribution directly left of middle, away from the selection.
    pub left: [Rect; 4],
}

pub fn layout(bounds: Rect, zoom: f32) -> AlignmentLayout {
    let pitch = ICON_PX + ICON_GAP_PX;
    let bottom = Pos2::new(
        bounds.center().x,
        bounds.bottom() + bottom_outset_px(zoom) + pitch,
    );
    let left = Pos2::new(bounds.left() - FRAME_OUTSET_PX - pitch, bounds.center().y);
    let square = |centre| Rect::from_center_size(centre, Vec2::splat(ICON_PX));
    AlignmentLayout {
        bottom: [
            square(bottom + Vec2::new(-pitch, 0.0)),
            square(bottom),
            square(bottom + Vec2::new(pitch, 0.0)),
            square(bottom + Vec2::new(0.0, pitch)),
        ],
        left: [
            square(left + Vec2::new(0.0, -pitch)),
            square(left),
            square(left + Vec2::new(0.0, pitch)),
            square(left + Vec2::new(-pitch, 0.0)),
        ],
    }
}

pub fn paint_button(
    painter: &egui::Painter,
    rect: Rect,
    icon: icons::Icon,
    enabled: bool,
    hovered: bool,
    palette: &Palette,
    select: Color32,
) {
    let alpha = if enabled { 1.0 } else { DISABLED_ALPHA };
    let surface = if hovered { select } else { palette.panel };
    let ink = if hovered { palette.window } else { palette.ink };
    painter.rect_filled(rect, 2.0, surface.gamma_multiply(alpha));
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0_f32, ink.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
    icons::paint(painter, rect.shrink(1.5), icon, ink.gamma_multiply(alpha));
}

/// Keep hover text beyond the whole T, including the distribution stem.
pub fn paint_label(
    painter: &egui::Painter,
    cluster: [Rect; 4],
    bottom: bool,
    label: &str,
    palette: &Palette,
) {
    let bounds = cluster.into_iter().reduce(|a, b| a.union(b)).unwrap();
    let (anchor, pos) = if bottom {
        (
            Align2::CENTER_TOP,
            bounds.center_bottom() + Vec2::new(0.0, 6.0),
        )
    } else {
        (
            Align2::RIGHT_CENTER,
            bounds.left_center() - Vec2::new(6.0, 0.0),
        )
    };
    painter.text(
        pos,
        anchor,
        label,
        egui::FontId::proportional(10.0),
        palette.ink,
    );
}
