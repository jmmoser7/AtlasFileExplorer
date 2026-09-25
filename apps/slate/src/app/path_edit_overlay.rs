//! Shared path-edit anchor / handle adornment (Direct Selection + draft tools).

use eframe::egui::{self, Color32, Pos2, Rect, Stroke as EStroke, Vec2};

/// Screen-constant anchor square half-size (~7 px squares). Path-edit handles
/// are a named P0.9 exception (see canvas-scale.mdc).
pub const ANCHOR_PX: f32 = 3.5;

#[derive(Clone, Copy, Debug)]
pub struct PathEditAnchorColors {
    pub select: Color32,
    pub bg: Color32,
    pub accent: Color32,
    pub sub: Color32,
}

#[derive(Clone, Copy, Debug)]
pub struct PathEditAnchorPaint {
    pub point: Pos2,
    pub handle_in: Option<Pos2>,
    pub handle_out: Option<Pos2>,
    pub selected: bool,
    pub smooth_hint: bool,
    /// Hollow square (e.g. close-path hover on the first anchor).
    pub close_hint: bool,
}

/// Anchor squares, optional handle lines, and round handle knobs.
pub fn paint_path_edit_anchors(
    painter: &egui::Painter,
    path_line: Option<&[Pos2]>,
    anchors: &[PathEditAnchorPaint],
    colors: PathEditAnchorColors,
) {
    if let Some(pts) = path_line {
        if pts.len() >= 2 {
            painter.add(egui::Shape::line(
                pts.to_vec(),
                EStroke::new(1.0_f32, colors.select.gamma_multiply(0.6)),
            ));
        }
    }
    for a in anchors {
        for hp in [a.handle_in, a.handle_out].into_iter().flatten() {
            painter.line_segment([a.point, hp], EStroke::new(1.0_f32, colors.accent));
            painter.circle_filled(hp, 3.0, colors.accent);
        }
    }
    for a in anchors {
        let r = Rect::from_center_size(a.point, Vec2::splat(ANCHOR_PX * 2.0));
        if a.close_hint {
            painter.rect_filled(r, 0.0, colors.bg);
            painter.rect_stroke(
                r,
                0.0,
                EStroke::new(1.2_f32, colors.select),
                egui::StrokeKind::Inside,
            );
        } else if a.selected {
            painter.rect_filled(r, 0.0, colors.select);
        } else {
            painter.rect_filled(r, 0.0, colors.bg);
            painter.rect_stroke(
                r,
                0.0,
                EStroke::new(1.2_f32, colors.select),
                egui::StrokeKind::Inside,
            );
        }
        if a.smooth_hint && !a.selected && !a.close_hint {
            painter.circle_stroke(a.point, ANCHOR_PX + 2.5, EStroke::new(0.6_f32, colors.sub));
        }
    }
}
