//! Shared visible Play/Pause affordance for real video hosts.
//! Callers own playback and input routing; geometry and paint have one owner.
use crate::{
    canvas_scale,
    icons::{self, Icon},
    theme::Palette,
};
use eframe::egui::{self, Color32, Painter, Rect, Stroke};

/// Centered control in screen coordinates. Visible geometry obeys P0.9.
pub fn rect(host: Rect, zoom: f32) -> Rect {
    Rect::from_center_size(
        host.center(),
        egui::Vec2::splat(canvas_scale::px(48.0, zoom)),
    )
}

/// Always-visible, compact media control. Failure/retry text belongs to the host.
pub fn paint(painter: &Painter, host: Rect, zoom: f32, playing: bool, theme: Palette) {
    let control = rect(host, zoom);
    if canvas_scale::too_small(control.width()) {
        return;
    }
    let center = control.center();
    let px = |size| canvas_scale::px(size, zoom);
    painter.circle_filled(
        center,
        control.width() * 0.5,
        Color32::from_black_alpha(150),
    );
    painter.circle_stroke(
        center,
        control.width() * 0.5,
        Stroke::new(px(1.0), theme.border),
    );
    icons::paint(
        painter,
        Rect::from_center_size(center, egui::Vec2::splat(px(24.0))),
        if playing { Icon::Pause } else { Icon::Play },
        Color32::WHITE,
    );
}
