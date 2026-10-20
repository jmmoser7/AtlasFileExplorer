//! Selection handles, hover cursors, and rotate zones for the Board canvas.
use super::board::BoardXf;
use atlas_shell::canvas_scale;
use atlas_shell::selection_tools::hit_regions::{self, Domain, Prominence};
use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Stroke as EStroke, Vec2};
use hit_regions::distance_to_segment as dist_to_segment;
use slate_doc::{geom::CornerGripEdge, scene::WorldRect};

/// Screen-px half-size of resize handles (matches board.rs).
#[cfg(test)]
pub const HANDLE_PX: f32 = 5.0;
/// Painted half-size of the live-corner fillet grip (square, same family as
/// resize handles).
pub const FILLET_GRIP_PX: f32 = 1.6;
/// World-unit inset of the corner grip along its edge while the corner is
/// square (and the floor of its resting travel).
pub const FILLET_GRIP_MIN_INSET_WORLD: f32 = 10.0;
/// Windows-style corner hit (diagonal resize).
pub const CORNER_HIT_PX: f32 = hit_regions::CORNER_REACH;
/// Windows-style edge-band hit (axis resize).
pub const EDGE_BAND_PX: f32 = hit_regions::EDGE_REACH;
/// Radius of the rotate hit zone outside each corner.
pub const ROTATE_ZONE_PX: f32 = hit_regions::ROTATE_REACH;
/// How far outside the corner the rotate affordance sits (screen px).
pub const ROTATE_OFFSET_PX: f32 = hit_regions::ROTATE_OFFSET;

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

impl ResizeHandle {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => ResizeHandle::Nw,
            1 => ResizeHandle::N,
            2 => ResizeHandle::Ne,
            3 => ResizeHandle::E,
            4 => ResizeHandle::Se,
            5 => ResizeHandle::S,
            6 => ResizeHandle::Sw,
            _ => ResizeHandle::W,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardHitTarget {
    Body,
    Resize(ResizeHandle),
    Rotate(u8),
    /// In-node fillet radius grip (NW host corner).
    FilletRadius,
}

pub struct SelectionGeom {
    /// True projected object geometry. Transform arithmetic always uses this.
    pub corners: [Pos2; 4],
    pub edges: [Pos2; 4],
    /// Ephemeral interaction frame. Never use it as an authored resize origin.
    pub control_corners: [Pos2; 4],
    pub control_edges: [Pos2; 4],
    pub rotate_points: [Pos2; 4],
    pub projected_shortest: f32,
    pub prominence: Prominence,
    pub zoom: f32,
}

pub fn selection_geom(xf: &BoardXf, rect: WorldRect, rotation_deg: f32) -> SelectionGeom {
    let corners_w = rect.corners_rotated(rotation_deg);
    let corners = corners_w.map(|(x, y)| xf.w2s(Pos2::new(x, y)));
    let edges = hit_regions::edge_midpoints(&corners);
    let projected_shortest = hit_regions::shortest_side(&corners);
    let prominence = Prominence::new(projected_shortest);
    let control_corners = hit_regions::control_corners(&corners);
    let control_edges = hit_regions::edge_midpoints(&control_corners);
    let rotate_points = hit_regions::rotation_points(&control_corners, prominence.rotate);
    SelectionGeom {
        corners,
        edges,
        control_corners,
        control_edges,
        rotate_points,
        projected_shortest,
        prominence,
        zoom: xf.z,
    }
}

mod adaptive;
mod crop;
#[cfg(test)]
pub use adaptive::body_geometry_distance;
pub use adaptive::{body_pick, chrome_bounds, chrome_pick, paint_control_frame, set_control_frame};
#[cfg(test)]
pub use crop::{crop_handle_at, CROP_ARM_PX, CROP_BAR_THICK_PX, CROP_HIT_PAD_PX};
pub use crop::{crop_handle_pick, paint_crop_handles};

#[cfg(test)]
fn handle_rects(geom: &SelectionGeom) -> [(ResizeHandle, Rect); 8] {
    let h = Vec2::splat(canvas_scale::px(HANDLE_PX, geom.zoom));
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

/// Square handle hit (no rotate zones, no edge bands). Crop blisters use
/// their own midpoint test; this remains the tight square.
#[cfg(test)]
pub fn hit_test_resize_handles(screen: Pos2, geom: &SelectionGeom) -> Option<ResizeHandle> {
    handle_rects(geom)
        .into_iter()
        .find(|(_, rect)| {
            rect.expand(canvas_scale::HIT_SLOP_PX * 0.25)
                .contains(screen)
        })
        .map(|(handle, _)| handle)
}

/// Continuous adaptive chrome: corner discs and full-edge bands.
pub fn hit_test_resize_bands(screen: Pos2, geom: &SelectionGeom) -> Option<ResizeHandle> {
    adaptive::resize_pick(screen, geom).map(|(handle, _)| handle)
}

/// Resting travel of the corner grip along its edge: the treatment's tangent
/// point, held at the minimum inset while the corner is square or nearly so.
pub fn corner_grip_rest_travel(edge: &CornerGripEdge, amount: f32) -> f32 {
    edge.travel_for_amount(amount)
        .max(FILLET_GRIP_MIN_INSET_WORLD.min(edge.max_travel))
}

/// Screen position of the corner grip `travel` world units along its edge.
pub fn corner_grip_screen(xf: &BoardXf, edge: &CornerGripEdge, travel: f32) -> Pos2 {
    let [x, y] = edge.point(travel);
    xf.w2s(Pos2::new(x, y))
}

/// Two-headed cursor along an edge direction.
pub fn cursor_along(dir: Vec2) -> CursorIcon {
    let angle = dir.y.atan2(dir.x);
    match ((angle / std::f32::consts::FRAC_PI_4)
        .round()
        .rem_euclid(4.0)) as usize
        % 4
    {
        0 => CursorIcon::ResizeHorizontal,
        1 => CursorIcon::ResizeNwSe,
        2 => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeNeSw,
    }
}

pub fn hit_test_fillet_grip(screen: Pos2, geom: &SelectionGeom, grip: Pos2) -> bool {
    Domain::Resize
        .reach(geom.projected_shortest, hit_regions::FILLET_REACH)
        .and_then(|reach| hit_regions::Field::Disc(grip).pick(screen, screen, reach))
        .is_some()
}

pub fn paint_fillet_grip(
    painter: &egui::Painter,
    grip: Pos2,
    prominence: f32,
    ink: Color32,
    hot: bool,
    handle_hot: Color32,
) {
    let half = FILLET_GRIP_PX * prominence;
    let fill = if hot { handle_hot } else { Color32::WHITE };
    painter.rect(
        Rect::from_center_size(grip, Vec2::splat(half * 2.0)),
        0.0,
        fill,
        EStroke::new(prominence, ink),
        egui::StrokeKind::Inside,
    );
}

mod polygon;
pub use polygon::{
    paint_sides_glyph, SIDES_GLYPH_HIT, SIDES_GLYPH_OFFSET, SIDES_GLYPH_RADIUS, SIDES_VERTEX_HIT,
};

#[cfg(test)]
pub fn hit_test_selection(screen: Pos2, geom: &SelectionGeom) -> Option<BoardHitTarget> {
    hit_test_chrome(screen, geom, true)
}

/// Same as [`hit_test_selection`], but portals and other non-rotatable
/// kinds pass `allow_rotate = false` so the outside-corner halo is inert.
pub fn hit_test_chrome(
    screen: Pos2,
    geom: &SelectionGeom,
    allow_rotate: bool,
) -> Option<BoardHitTarget> {
    chrome_pick(screen, geom, allow_rotate).map(|(target, _)| target)
}

/// Resize cursor from the handle's screen-space direction, so arrows stay
/// perpendicular to the edge (or along the box diagonal) on rotated nodes.
///
/// Edge handles use the outward edge normal (midpoint − center). Corner
/// handles add the two adjacent *unit* edge normals so the arrow is 45° to
/// the box edges even when the box is a wide or tall rectangle — the raw
/// corner−center vector of a wide group box is nearly horizontal and would
/// otherwise show an axis arrow.
pub fn cursor_for_resize(handle: ResizeHandle, geom: &SelectionGeom) -> CursorIcon {
    let center = geom.corners[0] + (geom.corners[2] - geom.corners[0]) * 0.5;
    let n = unit(geom.edges[0] - center);
    let e = unit(geom.edges[1] - center);
    let s = unit(geom.edges[2] - center);
    let w = unit(geom.edges[3] - center);
    let v = match handle {
        ResizeHandle::N => n,
        ResizeHandle::E => e,
        ResizeHandle::S => s,
        ResizeHandle::W => w,
        ResizeHandle::Nw => n + w,
        ResizeHandle::Ne => n + e,
        ResizeHandle::Se => s + e,
        ResizeHandle::Sw => s + w,
    };
    if v.length_sq() < 1e-6 {
        return CursorIcon::Move;
    }
    // Screen space is y-down: 0 = east, positive angles sweep clockwise.
    let angle = v.y.atan2(v.x);
    let octant = ((angle / std::f32::consts::FRAC_PI_4)
        .round()
        .rem_euclid(8.0)) as usize
        % 8;
    match octant {
        0 => CursorIcon::ResizeEast,
        1 => CursorIcon::ResizeSouthEast,
        2 => CursorIcon::ResizeSouth,
        3 => CursorIcon::ResizeSouthWest,
        4 => CursorIcon::ResizeWest,
        5 => CursorIcon::ResizeNorthWest,
        6 => CursorIcon::ResizeNorth,
        _ => CursorIcon::ResizeNorthEast,
    }
}

fn unit(v: Vec2) -> Vec2 {
    let len = v.length();
    if len < 1e-6 {
        Vec2::ZERO
    } else {
        v / len
    }
}

mod rotate_cursor;
pub use rotate_cursor::{cursor_for_rotate, paint_rotate_cursor};

pub fn paint_selection(
    painter: &egui::Painter,
    geom: &SelectionGeom,
    outline: &[Pos2],
    color: Color32,
    outline_w: f32,
) {
    let pts = if outline.is_empty() {
        geom.corners.to_vec()
    } else {
        outline.to_vec()
    };
    painter.add(egui::Shape::closed_line(
        pts,
        EStroke::new(outline_w, color),
    ));
}

#[cfg(test)]
mod tests;
