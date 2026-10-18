//! Shared generator animation. Hosts supply their existing painted outline.
use super::*;

/// The phase used by the image generator's indeterminate sweep and busy pulse.
pub fn phase(time: f64) -> f32 {
    (time * 0.6).rem_euclid(1.0) as f32
}

pub fn sweep_rect(body: Rect, zoom: f32, phase: f32) -> Rect {
    let height = 3.0 * zoom;
    let span = body.width() * 0.3;
    let x = body.left() - span + (body.width() + span) * phase;
    Rect::from_min_max(
        Pos2::new(x.max(body.left()), body.bottom() - height),
        Pos2::new((x + span).min(body.right()), body.bottom()),
    )
}

pub fn paint_sweep(painter: &egui::Painter, body: Rect, zoom: f32, time: f64, theme: Palette) {
    painter.rect_filled(sweep_rect(body, zoom, phase(time)), 0.0, theme.accent);
}

/// Busy state remains visible after selection moves to another object.
/// The caller uses the same corner/clip outline that paints the host itself.
pub fn paint_busy_perimeter(
    ctx: &egui::Context,
    painter: &egui::Painter,
    outline: &[Pos2],
    zoom: f32,
    theme: Palette,
) {
    if outline.len() < 3 || zoom <= 0.0 {
        return;
    }
    let phase = phase(ctx.input(|i| i.time));
    let alpha = 0.35 + 0.45 * (0.5 - 0.5 * (phase * std::f32::consts::TAU).cos());
    painter.add(egui::Shape::closed_line(
        outline.to_vec(),
        Stroke::new(1.5 * zoom, theme.accent.gamma_multiply(alpha)),
    ));
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_tracks_the_same_world_edge_at_every_zoom() {
        for zoom in [0.25, 0.525, 1.0, 2.25] {
            let body = Rect::from_min_size(Pos2::new(23.0, 31.0), Vec2::new(240.0, 180.0) * zoom);
            for phase in [0.0, 0.25, 0.75, 1.0] {
                let rect = sweep_rect(body, zoom, phase);
                assert!(body.contains_rect(rect));
                assert!((rect.height() / zoom - 3.0).abs() < 0.001);
                assert_eq!(rect.bottom(), body.bottom());
            }
        }
        assert!((phase(1.0 / 0.6) - phase(0.0)).abs() < 0.0001);
    }
}
