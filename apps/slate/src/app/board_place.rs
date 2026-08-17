//! Shared place-rect constraints and armed-tool chrome (P2.GhostFollow).
//!
//! Contract: `docs/keymap/contracts/tool-arming.md`. DragScale preview and
//! commit both call [`place_rect`] so a future DragRect command reuses the
//! same Shift / aspect table instead of copying `frame_drag_rect`.
//! The ghost glyph is screen-space (P0.9) but its hotspot is the snapped
//! world point from `resolve_point_snap` / `resolve_draw_rect`.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke as EStroke, Vec2};
use slate_doc::scene::WorldRect;

use super::board::{BoardTool, MIN_DRAW};

/// Feel constants pinned by the contract's Feel-constants table (P0.6).
pub mod place_tokens {
    use eframe::egui::Vec2;

    /// `place.ghost_size` — armed silhouette long edge, screen px (D09).
    pub const GHOST_SIZE: f32 = 22.0;
    /// `place.ghost_offset` — silhouette offset from the pointer hotspot.
    pub const GHOST_OFFSET: Vec2 = Vec2::new(14.0, 14.0);
    /// `place.ghost_alpha` — silhouette opacity.
    pub const GHOST_ALPHA: f32 = 0.55;
    pub const GHOST_STROKE: f32 = 1.5;
    pub const GHOST_ROUNDING: f32 = 3.0;
    /// `draft.drag_threshold` — screen px of pointer travel before release
    /// splits ClickPlace from DragScale (D04 / P2.DragShape).
    pub const DRAG_THRESHOLD: f32 = 4.0;
    /// Click-place size for the rectangle tool (world units).
    pub const RECT_DEFAULT_W: f32 = 180.0;
    pub const RECT_DEFAULT_H: f32 = 120.0;
    /// Click-place size for the ellipse tool (a circle).
    pub const ELLIPSE_DEFAULT_W: f32 = 160.0;
    pub const ELLIPSE_DEFAULT_H: f32 = 160.0;
    /// Portal title-bar height inside the 22 px glyph.
    pub const PORTAL_BAR: f32 = 3.0;
    pub const TEXT_H: f32 = 12.0;
    pub const STICKY_SIZE: f32 = 16.0;
}

/// How Shift reshapes a DragScale rect. One table, every DragRect tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaceConstraint {
    /// Unmodified = free rect; Shift unused.
    Free,
    /// Unmodified = free; Shift → square (P1.shape.aspect).
    SquareOnShift,
    /// Unmodified = `ratio` from |dx| (Frame preset); Shift → square.
    PresetAspect { ratio: f32 },
    /// Unmodified = free; Shift locks `ratio` (portals: 16:9).
    ShiftLocksAspect { ratio: f32 },
}

/// Constraint the DragRect family uses. `None` for tools that do not draw a
/// rect (Text / Sticky click-place, Line, …).
pub fn constraint_for(tool: BoardTool, frame_aspect: f32) -> Option<PlaceConstraint> {
    match tool {
        BoardTool::Frame => Some(PlaceConstraint::PresetAspect {
            ratio: frame_aspect.max(0.001),
        }),
        BoardTool::RectShape | BoardTool::Ellipse => Some(PlaceConstraint::SquareOnShift),
        BoardTool::RepoLens
        | BoardTool::StatusBoard
        | BoardTool::AgentPortal
        | BoardTool::WebPortal => Some(PlaceConstraint::ShiftLocksAspect { ratio: 16.0 / 9.0 }),
        _ => None,
    }
}

/// World rect for a press-at-`start` / cursor-at-`end` DragScale.
///
/// The origin and aspect rules match the functions this replaced
/// (`frame_drag_rect`, `repo_lens_drag_rect`, `constrain_draw_rect`) so
/// existing click/drag tests stay honest.
pub fn place_rect(start: Pos2, end: Pos2, constraint: PlaceConstraint, shift: bool) -> WorldRect {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    match constraint {
        PlaceConstraint::Free => WorldRect::new(start.x, start.y, dx, dy).normalized(),
        PlaceConstraint::SquareOnShift => square_or_free(start, dx, dy, shift),
        PlaceConstraint::PresetAspect { ratio } => {
            if shift {
                square_or_free(start, dx, dy, true)
            } else {
                aspect_from_dx(start, dx, dy, ratio)
            }
        }
        PlaceConstraint::ShiftLocksAspect { ratio } => {
            if shift {
                aspect_from_dx(start, dx, dy, ratio)
            } else {
                WorldRect::new(start.x, start.y, dx, dy).normalized()
            }
        }
    }
}

fn square_or_free(start: Pos2, dx: f32, dy: f32, square: bool) -> WorldRect {
    let raw = WorldRect::new(start.x, start.y, dx, dy);
    if !square {
        return raw.normalized();
    }
    let r = raw.normalized();
    let side = r.w.max(r.h);
    WorldRect::new(r.x, r.y, side, side)
}

/// Size from |dx|, height from `aspect` (= w/h). `dy` only picks the side
/// of `start` the rect grows into — Frame and Shift-locked portals.
fn aspect_from_dx(start: Pos2, dx: f32, dy: f32, aspect: f32) -> WorldRect {
    let w = dx.abs().max(MIN_DRAW);
    let h = (w / aspect.max(0.001)).max(MIN_DRAW);
    let (x, y) = if dx >= 0.0 {
        (start.x, if dy >= 0.0 { start.y } else { start.y - h })
    } else {
        (start.x - w, if dy >= 0.0 { start.y } else { start.y - h })
    };
    WorldRect::new(x, y, w, h)
}

/// Small screen-space silhouette while the tool is armed (D09).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhostKind {
    RoundedRect,
    Ellipse,
    Portal,
    TextBox,
    Sticky,
}

pub fn ghost_kind(tool: BoardTool) -> Option<GhostKind> {
    match tool {
        BoardTool::Frame | BoardTool::RectShape => Some(GhostKind::RoundedRect),
        BoardTool::Ellipse => Some(GhostKind::Ellipse),
        BoardTool::RepoLens
        | BoardTool::StatusBoard
        | BoardTool::AgentPortal
        | BoardTool::WebPortal => Some(GhostKind::Portal),
        BoardTool::Text => Some(GhostKind::TextBox),
        BoardTool::Sticky => Some(GhostKind::Sticky),
        _ => None,
    }
}

/// Accent pointer painted at the hotspot (`CursorIcon::None` + this).
pub fn paint_armed_pointer(painter: &egui::Painter, pos: Pos2, color: Color32) {
    let tip = pos;
    let left = pos + Vec2::new(1.2, 16.5);
    let right = pos + Vec2::new(12.0, 12.0);
    let outline = Color32::from_black_alpha(150);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, left, right],
        color,
        EStroke::new(1.6_f32, outline),
    ));
}

/// 22 px (or smaller) silhouette offset from the hotspot. Chrome, not a node.
pub fn paint_ghost(
    painter: &egui::Painter,
    pointer: Pos2,
    kind: GhostKind,
    accent: Color32,
    portal: Color32,
    sticky_fill: Color32,
) {
    use place_tokens::*;
    let origin = pointer + GHOST_OFFSET;
    let stroke = accent.gamma_multiply(GHOST_ALPHA);
    let fill = accent.gamma_multiply(GHOST_ALPHA * 0.22);
    match kind {
        GhostKind::RoundedRect => {
            let r = Rect::from_min_size(origin, Vec2::splat(GHOST_SIZE));
            painter.rect_filled(r, GHOST_ROUNDING, fill);
            painter.rect_stroke(
                r,
                GHOST_ROUNDING,
                EStroke::new(GHOST_STROKE, stroke),
                egui::StrokeKind::Inside,
            );
        }
        GhostKind::Ellipse => {
            let r = Rect::from_min_size(origin, Vec2::splat(GHOST_SIZE));
            painter.add(egui::epaint::EllipseShape {
                center: r.center(),
                radius: r.size() * 0.5,
                fill,
                stroke: EStroke::new(GHOST_STROKE, stroke),
            });
        }
        GhostKind::Portal => {
            let r = Rect::from_min_size(origin, Vec2::new(GHOST_SIZE, 16.0));
            let bar_c = portal.gamma_multiply(GHOST_ALPHA);
            painter.rect_filled(r, GHOST_ROUNDING, fill);
            painter.rect_stroke(
                r,
                GHOST_ROUNDING,
                EStroke::new(GHOST_STROKE, portal.gamma_multiply(GHOST_ALPHA + 0.15)),
                egui::StrokeKind::Inside,
            );
            let bar = Rect::from_min_size(r.min, Vec2::new(r.width(), PORTAL_BAR));
            painter.rect_filled(bar, GHOST_ROUNDING, bar_c);
        }
        GhostKind::TextBox => {
            let r = Rect::from_min_size(origin, Vec2::new(GHOST_SIZE, TEXT_H));
            painter.rect_filled(r, 2.0, fill);
            painter.rect_stroke(
                r,
                2.0,
                EStroke::new(GHOST_STROKE, stroke),
                egui::StrokeKind::Inside,
            );
            let inset = r.shrink2(Vec2::new(3.0, 4.0));
            painter.line_segment(
                [inset.left_center(), inset.right_center()],
                EStroke::new(1.0_f32, stroke),
            );
        }
        GhostKind::Sticky => {
            let r = Rect::from_min_size(origin, Vec2::splat(STICKY_SIZE));
            painter.rect_filled(r, 2.0, sticky_fill.gamma_multiply(GHOST_ALPHA));
            painter.rect_stroke(
                r,
                2.0,
                EStroke::new(GHOST_STROKE, stroke),
                egui::StrokeKind::Inside,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_kind_covers_the_create_family() {
        assert_eq!(ghost_kind(BoardTool::Frame), Some(GhostKind::RoundedRect));
        assert_eq!(
            ghost_kind(BoardTool::RectShape),
            Some(GhostKind::RoundedRect)
        );
        assert_eq!(ghost_kind(BoardTool::Ellipse), Some(GhostKind::Ellipse));
        assert_eq!(ghost_kind(BoardTool::WebPortal), Some(GhostKind::Portal));
        assert_eq!(ghost_kind(BoardTool::RepoLens), Some(GhostKind::Portal));
        assert_eq!(ghost_kind(BoardTool::StatusBoard), Some(GhostKind::Portal));
        assert_eq!(ghost_kind(BoardTool::AgentPortal), Some(GhostKind::Portal));
        assert_eq!(ghost_kind(BoardTool::Text), Some(GhostKind::TextBox));
        assert_eq!(ghost_kind(BoardTool::Sticky), Some(GhostKind::Sticky));
        assert!(ghost_kind(BoardTool::Line).is_none());
        assert!(ghost_kind(BoardTool::Select).is_none());
        assert!(ghost_kind(BoardTool::Brush).is_none());
    }

    #[test]
    fn rect_shift_makes_square_from_the_normalized_origin() {
        let r = place_rect(
            Pos2::new(0.0, 0.0),
            Pos2::new(120.0, 40.0),
            PlaceConstraint::SquareOnShift,
            true,
        );
        assert!((r.w - r.h).abs() < 0.01);
        assert!((r.w - 120.0).abs() < 0.01);
        assert!((r.x).abs() < 0.01 && (r.y).abs() < 0.01);
    }

    #[test]
    fn frame_preset_aspect_sizes_from_dx() {
        let aspect = 16.0 / 9.0;
        let r = place_rect(
            Pos2::new(10.0, 20.0),
            Pos2::new(10.0 + 160.0, 20.0 + 10.0),
            PlaceConstraint::PresetAspect { ratio: aspect },
            false,
        );
        assert!((r.w - 160.0).abs() < 0.01);
        assert!((r.h - 90.0).abs() < 0.01);
        assert!((r.x - 10.0).abs() < 0.01 && (r.y - 20.0).abs() < 0.01);
    }

    #[test]
    fn frame_shift_is_square_not_preset() {
        let r = place_rect(
            Pos2::new(0.0, 0.0),
            Pos2::new(80.0, 20.0),
            PlaceConstraint::PresetAspect { ratio: 16.0 / 9.0 },
            true,
        );
        assert!((r.w - r.h).abs() < 0.01);
        assert!((r.w - 80.0).abs() < 0.01);
    }

    #[test]
    fn portal_shift_locks_16_9() {
        let r = place_rect(
            Pos2::new(0.0, 0.0),
            Pos2::new(320.0, 20.0),
            PlaceConstraint::ShiftLocksAspect { ratio: 16.0 / 9.0 },
            true,
        );
        assert!((r.w - 320.0).abs() < 0.01);
        assert!((r.h - 180.0).abs() < 0.01);
    }

    #[test]
    fn portal_unmodified_is_free_aspect() {
        let r = place_rect(
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 40.0),
            PlaceConstraint::ShiftLocksAspect { ratio: 16.0 / 9.0 },
            false,
        );
        assert!((r.w - 100.0).abs() < 0.01);
        assert!((r.h - 40.0).abs() < 0.01);
    }

    #[test]
    fn free_constraint_is_the_normalized_drag() {
        let r = place_rect(
            Pos2::new(10.0, 20.0),
            Pos2::new(40.0, 5.0),
            PlaceConstraint::Free,
            true,
        );
        assert!((r.x - 10.0).abs() < 0.01);
        assert!((r.y - 5.0).abs() < 0.01);
        assert!((r.w - 30.0).abs() < 0.01);
        assert!((r.h - 15.0).abs() < 0.01);
    }

    #[test]
    fn negative_drag_grows_toward_the_cursor() {
        let r = place_rect(
            Pos2::new(100.0, 100.0),
            Pos2::new(40.0, 50.0),
            PlaceConstraint::SquareOnShift,
            false,
        );
        assert!((r.x - 40.0).abs() < 0.01);
        assert!((r.y - 50.0).abs() < 0.01);
        assert!((r.w - 60.0).abs() < 0.01);
        assert!((r.h - 50.0).abs() < 0.01);
    }
}
