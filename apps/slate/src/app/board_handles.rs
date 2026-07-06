//! Selection handles, hover cursors, and rotate zones for the Board canvas.

use super::board::BoardXf;
use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Stroke as EStroke, Vec2};
use slate_doc::scene::WorldRect;

/// Screen-px half-size of resize handles (matches board.rs).
pub const HANDLE_PX: f32 = 5.0;
/// Radius of the rotate hit zone outside each corner.
pub const ROTATE_ZONE_PX: f32 = 12.0;
/// How far outside the corner the rotate affordance sits (screen px).
pub const ROTATE_OFFSET_PX: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeHandle {
    Nw = 0,
    N = 1,
    Ne = 2,
    E = 3,
    Se = 4,
    S = 5,
    Sw = 6,
    W = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardHitTarget {
    Body,
    Resize(ResizeHandle),
    Rotate(u8),
}

pub struct SelectionGeom {
    pub corners: [Pos2; 4],
    pub edges: [Pos2; 4],
    pub rotate_points: [Pos2; 4],
}

pub fn selection_geom(xf: &BoardXf, rect: WorldRect, rotation_deg: f32) -> SelectionGeom {
    let corners_w = rect.corners_rotated(rotation_deg);
    let corners = corners_w.map(|(x, y)| xf.w2s(Pos2::new(x, y)));
    let edges = [
        corners[0] + (corners[1] - corners[0]) * 0.5,
        corners[1] + (corners[2] - corners[1]) * 0.5,
        corners[2] + (corners[3] - corners[2]) * 0.5,
        corners[3] + (corners[0] - corners[3]) * 0.5,
    ];
    let center = corners[0] + (corners[2] - corners[0]) * 0.5;
    let rotate_points = corners.map(|c| {
        let outward = (c - center).normalized();
        c + outward * ROTATE_OFFSET_PX
    });
    SelectionGeom {
        corners,
        edges,
        rotate_points,
    }
}

fn handle_rects(geom: &SelectionGeom) -> [(ResizeHandle, Rect); 8] {
    let h = Vec2::splat(HANDLE_PX);
    [
        (
            ResizeHandle::Nw,
            Rect::from_center_size(geom.corners[0], h * 2.0),
        ),
        (
            ResizeHandle::N,
            Rect::from_center_size(geom.edges[0], h * 2.0),
        ),
        (
            ResizeHandle::Ne,
            Rect::from_center_size(geom.corners[1], h * 2.0),
        ),
        (
            ResizeHandle::E,
            Rect::from_center_size(geom.edges[1], h * 2.0),
        ),
        (
            ResizeHandle::Se,
            Rect::from_center_size(geom.corners[2], h * 2.0),
        ),
        (
            ResizeHandle::S,
            Rect::from_center_size(geom.edges[2], h * 2.0),
        ),
        (
            ResizeHandle::Sw,
            Rect::from_center_size(geom.corners[3], h * 2.0),
        ),
        (
            ResizeHandle::W,
            Rect::from_center_size(geom.edges[3], h * 2.0),
        ),
    ]
}

pub fn hit_test_selection(screen: Pos2, geom: &SelectionGeom) -> Option<BoardHitTarget> {
    for (i, rp) in geom.rotate_points.iter().enumerate() {
        if screen.distance(*rp) <= ROTATE_ZONE_PX + 2.0 {
            return Some(BoardHitTarget::Rotate(i as u8));
        }
    }
    for (handle, rect) in handle_rects(geom) {
        if rect.expand(2.0).contains(screen) {
            return Some(BoardHitTarget::Resize(handle));
        }
    }
    None
}

pub fn cursor_for_resize(handle: ResizeHandle) -> CursorIcon {
    match handle {
        ResizeHandle::Nw => CursorIcon::ResizeNorthWest,
        ResizeHandle::N => CursorIcon::ResizeNorth,
        ResizeHandle::Ne => CursorIcon::ResizeNorthEast,
        ResizeHandle::E => CursorIcon::ResizeEast,
        ResizeHandle::Se => CursorIcon::ResizeSouthEast,
        ResizeHandle::S => CursorIcon::ResizeSouth,
        ResizeHandle::Sw => CursorIcon::ResizeSouthWest,
        ResizeHandle::W => CursorIcon::ResizeWest,
    }
}

/// Windows-style rotate cursor is not exposed by egui; the arc affordance carries
/// the affordance while the OS cursor stays neutral.
pub fn cursor_for_rotate() -> CursorIcon {
    CursorIcon::Grab
}

pub fn paint_selection(
    painter: &egui::Painter,
    geom: &SelectionGeom,
    color: Color32,
    hover: Option<BoardHitTarget>,
) {
    let outline: Vec<Pos2> = geom.corners.to_vec();
    painter.add(egui::Shape::closed_line(
        outline.clone(),
        EStroke::new(1.5, color),
    ));

    for (handle, rect) in handle_rects(geom) {
        let fill = match hover {
            Some(BoardHitTarget::Resize(h)) if h == handle => color,
            _ => color.gamma_multiply(0.85),
        };
        painter.rect_filled(rect, 1.0, fill);
    }

    if let Some(BoardHitTarget::Rotate(i)) = hover {
        paint_rotate_affordance(
            painter,
            geom.corners[i as usize],
            geom.rotate_points[i as usize],
            color,
        );
    }
}

/// Semi-circular arc hint outside a corner (Office-style rotate affordance).
pub fn paint_rotate_affordance(
    painter: &egui::Painter,
    corner: Pos2,
    rotate_point: Pos2,
    color: Color32,
) {
    let center = corner;
    let radius = corner.distance(rotate_point).max(8.0);
    let base = (rotate_point - corner).angle();
    let n = 14;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let a = base - 0.55 + t * 1.1;
            center + Vec2::angled(a) * radius
        })
        .collect();
    painter.add(egui::Shape::dashed_line(
        &pts,
        EStroke::new(1.5, color),
        4.0,
        3.0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_handles_cover_corners_and_edges() {
        let xf = BoardXf {
            center: Pos2::ZERO,
            offset: Vec2::ZERO,
            z: 1.0,
        };
        let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 100.0, 50.0), 0.0);
        assert!(
            hit_test_selection(geom.corners[0], &geom)
                == Some(BoardHitTarget::Resize(ResizeHandle::Nw))
        );
        assert!(
            hit_test_selection(geom.edges[1], &geom)
                == Some(BoardHitTarget::Resize(ResizeHandle::E))
        );
    }
}
