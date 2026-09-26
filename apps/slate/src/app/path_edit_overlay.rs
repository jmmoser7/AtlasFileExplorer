//! Shared path-edit anchor / handle adornment and its hit rules (Direct
//! Selection, selected Bézier grips, and draft tools).

use eframe::egui::{self, Color32, Pos2, Rect, Stroke as EStroke, Vec2};
use vector_ink::HandleEnd;

/// Screen-constant anchor square half-size (~7 px squares). Path-edit handles
/// are a named P0.9 exception (see canvas-scale.mdc).
pub const ANCHOR_PX: f32 = 3.5;

/// Anchor / handle-knob pick radius (screen px).
pub const HIT_PX: f32 = 7.0;

/// What a press landed on in a painted path-edit overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathEditHit {
    Handle(usize, HandleEnd),
    Anchor(usize),
}

/// Only what is painted can be picked, within [`HIT_PX`]. The nearest
/// anchor square or handle knob wins; an anchor wins a tie, so a short
/// handle never hides its own anchor.
pub fn path_edit_hit(anchors: &[PathEditAnchorPaint], screen: Pos2) -> Option<PathEditHit> {
    match (
        nearest_handle(anchors, screen),
        nearest_anchor(anchors, screen),
    ) {
        (Some((dh, i, end)), Some((da, _))) if dh < da => Some(PathEditHit::Handle(i, end)),
        (_, Some((_, i))) => Some(PathEditHit::Anchor(i)),
        (Some((_, i, end)), None) => Some(PathEditHit::Handle(i, end)),
        (None, None) => None,
    }
}

/// Screen distance to what [`path_edit_hit`] would pick, so a neighboring
/// grip can yield to a nearer one.
pub fn path_edit_hit_distance(anchors: &[PathEditAnchorPaint], screen: Pos2) -> Option<f32> {
    let handle = nearest_handle(anchors, screen).map(|(d, ..)| d);
    let anchor = nearest_anchor(anchors, screen).map(|(d, _)| d);
    match (handle, anchor) {
        (Some(h), Some(a)) => Some(h.min(a)),
        (h, a) => h.or(a),
    }
}

/// Nearest anchor square within [`HIT_PX`].
pub fn hit_anchor(anchors: &[PathEditAnchorPaint], screen: Pos2) -> Option<usize> {
    nearest_anchor(anchors, screen).map(|(_, i)| i)
}

fn nearest_handle(
    anchors: &[PathEditAnchorPaint],
    screen: Pos2,
) -> Option<(f32, usize, HandleEnd)> {
    let mut best: Option<(f32, usize, HandleEnd)> = None;
    for (i, a) in anchors.iter().enumerate() {
        for (knob, end) in [(a.handle_in, HandleEnd::In), (a.handle_out, HandleEnd::Out)] {
            let Some(knob) = knob else { continue };
            let d = knob.distance(screen);
            if d <= HIT_PX && best.is_none_or(|(bd, ..)| d < bd) {
                best = Some((d, i, end));
            }
        }
    }
    best
}

fn nearest_anchor(anchors: &[PathEditAnchorPaint], screen: Pos2) -> Option<(f32, usize)> {
    let mut best: Option<(f32, usize)> = None;
    for (i, a) in anchors.iter().enumerate() {
        let d = a.point.distance(screen);
        if d <= HIT_PX && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best
}

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
