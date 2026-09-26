//! Selection handles, hover cursors, and rotate zones for the Board canvas.

use super::board::BoardXf;
use atlas_shell::canvas_scale;
use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Stroke as EStroke, Vec2};
use slate_doc::geom::CornerGripEdge;
use slate_doc::scene::WorldRect;

/// Screen-px half-size of resize handles (matches board.rs).
pub const HANDLE_PX: f32 = 5.0;
/// Painted half-size of the live-corner fillet grip (square, same family as
/// resize handles).
pub const FILLET_GRIP_PX: f32 = 1.6;
/// Half-size of the fillet grip's hit box before slop, and the size whose
/// legibility decides whether the grip shows at all. Larger than the painted
/// square so a small grip stays easy to grab.
pub const FILLET_GRIP_HIT_PX: f32 = 4.0;
/// World-unit inset of the corner grip along its edge while the corner is
/// square (and the floor of its resting travel).
pub const FILLET_GRIP_MIN_INSET_WORLD: f32 = 10.0;
/// Hover fill shared by the live fillet grip and crop handles.
pub const GRIP_HANDLE_HOT: Color32 = Color32::from_rgb(210, 230, 255);
/// Windows-style corner hit (diagonal resize).
pub const CORNER_HIT_PX: f32 = 12.0;
/// Windows-style edge-band hit (axis resize).
pub const EDGE_BAND_PX: f32 = 6.0;
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
    pub corners: [Pos2; 4],
    pub edges: [Pos2; 4],
    pub rotate_points: [Pos2; 4],
    pub zoom: f32,
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
        c + outward * canvas_scale::px(ROTATE_OFFSET_PX, xf.z)
    });
    SelectionGeom {
        corners,
        edges,
        rotate_points,
        zoom: xf.z,
    }
}

/// Photoshop crop chrome, in screen px before `canvas_scale`.
pub const CROP_ARM_PX: f32 = 12.0;
pub const CROP_BAR_LEN_PX: f32 = 18.0;
pub const CROP_BAR_THICK_PX: f32 = 5.0;
/// Extra hit outside the painted bracket or bar.
pub const CROP_HIT_PAD_PX: f32 = 4.0;

/// Crop handle under `screen` and the pointer's distance to that corner or
/// edge. A corner zone is a disc of the bracket arm plus pad; an edge zone
/// is a band along the whole side, half the bar thickness plus pad on each
/// side. Both reach outside the box, and a corner beats any edge it
/// overlaps. [`canvas_scale::hit_px`] carries the zoom and never lets a
/// zone shrink below the screen slop.
pub fn crop_handle_pick(screen: Pos2, geom: &SelectionGeom) -> Option<(ResizeHandle, f32)> {
    let z = geom.zoom;
    let corner_hit = canvas_scale::hit_px(CROP_ARM_PX + CROP_HIT_PAD_PX, z);
    let edge_hit = canvas_scale::hit_px(CROP_BAR_THICK_PX * 0.5 + CROP_HIT_PAD_PX, z);
    let nearest = |hits: [(ResizeHandle, f32); 4], reach: f32| {
        hits.into_iter()
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    };
    let c = geom.corners;
    nearest(
        [
            (ResizeHandle::Nw, screen.distance(c[0])),
            (ResizeHandle::Ne, screen.distance(c[1])),
            (ResizeHandle::Se, screen.distance(c[2])),
            (ResizeHandle::Sw, screen.distance(c[3])),
        ],
        corner_hit,
    )
    .or_else(|| {
        nearest(
            [
                (ResizeHandle::N, dist_to_segment(screen, c[0], c[1])),
                (ResizeHandle::E, dist_to_segment(screen, c[1], c[2])),
                (ResizeHandle::S, dist_to_segment(screen, c[2], c[3])),
                (ResizeHandle::W, dist_to_segment(screen, c[3], c[0])),
            ],
            edge_hit,
        )
    })
}

#[cfg(test)]
pub fn crop_handle_at(screen: Pos2, geom: &SelectionGeom) -> Option<ResizeHandle> {
    crop_handle_pick(screen, geom).map(|(handle, _)| handle)
}

/// White corner brackets and edge bars, with a dark edge so they read on the picture.
pub fn paint_crop_handles(
    painter: &egui::Painter,
    geom: &SelectionGeom,
    hot: Option<ResizeHandle>,
) {
    let z = geom.zoom;
    let arm = canvas_scale::px(CROP_ARM_PX, z);
    let half_len = canvas_scale::px(CROP_BAR_LEN_PX, z) * 0.5;
    let half_thick = canvas_scale::px(CROP_BAR_THICK_PX, z) * 0.5;
    let white = Color32::WHITE;
    let ink = Color32::from_black_alpha(200);
    let stroke = canvas_scale::px(2.0, z);
    let under = canvas_scale::px(3.5, z);
    for i in 0..4 {
        let corner = geom.corners[i];
        let prev = geom.corners[(i + 3) % 4];
        let next = geom.corners[(i + 1) % 4];
        let a = corner + unit(prev - corner) * arm;
        let b = corner + unit(next - corner) * arm;
        let handle = [
            ResizeHandle::Nw,
            ResizeHandle::Ne,
            ResizeHandle::Se,
            ResizeHandle::Sw,
        ][i];
        let color = if hot == Some(handle) {
            GRIP_HANDLE_HOT
        } else {
            white
        };
        for end in [a, b] {
            painter.line_segment([corner, end], egui::Stroke::new(under, ink));
            painter.line_segment([corner, end], egui::Stroke::new(stroke, color));
        }
        let mid = geom.edges[i];
        let along = unit(next - corner);
        let normal = Vec2::new(-along.y, along.x);
        let bar = vec![
            mid + along * half_len + normal * half_thick,
            mid - along * half_len + normal * half_thick,
            mid - along * half_len - normal * half_thick,
            mid + along * half_len - normal * half_thick,
        ];
        let edge = [
            ResizeHandle::N,
            ResizeHandle::E,
            ResizeHandle::S,
            ResizeHandle::W,
        ][i];
        let fill = if hot == Some(edge) {
            GRIP_HANDLE_HOT
        } else {
            white
        };
        painter.add(egui::Shape::convex_polygon(
            bar,
            fill,
            egui::Stroke::new(canvas_scale::px(1.0, z), ink),
        ));
    }
}

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
pub fn hit_test_resize_handles(screen: Pos2, geom: &SelectionGeom) -> Option<ResizeHandle> {
    handle_rects(geom)
        .into_iter()
        .find(|(_, rect)| {
            rect.expand(canvas_scale::HIT_SLOP_PX * 0.25)
                .contains(screen)
        })
        .map(|(handle, _)| handle)
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-8 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// Windows-window chrome: corners first, then the full edge band.
pub fn hit_test_resize_bands(screen: Pos2, geom: &SelectionGeom) -> Option<ResizeHandle> {
    let corner_handles = [
        ResizeHandle::Nw,
        ResizeHandle::Ne,
        ResizeHandle::Se,
        ResizeHandle::Sw,
    ];
    let corner_hit = canvas_scale::hit_px(CORNER_HIT_PX, geom.zoom);
    for (i, corner) in geom.corners.iter().enumerate() {
        if screen.distance(*corner) <= corner_hit {
            return Some(corner_handles[i]);
        }
    }
    let edge_spans = [
        (ResizeHandle::N, geom.corners[0], geom.corners[1]),
        (ResizeHandle::E, geom.corners[1], geom.corners[2]),
        (ResizeHandle::S, geom.corners[2], geom.corners[3]),
        (ResizeHandle::W, geom.corners[3], geom.corners[0]),
    ];
    let edge_hit = canvas_scale::hit_px(EDGE_BAND_PX, geom.zoom);
    let mut best: Option<(ResizeHandle, f32)> = None;
    for (handle, a, b) in edge_spans {
        let d = dist_to_segment(screen, a, b);
        if d <= edge_hit && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((handle, d));
        }
    }
    best.map(|(h, _)| h)
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
    let half = (canvas_scale::px(FILLET_GRIP_HIT_PX, geom.zoom) + canvas_scale::HIT_SLOP_PX * 0.5)
        .max(canvas_scale::HIT_SLOP_PX);
    Rect::from_center_size(grip, Vec2::splat(half * 2.0)).contains(screen)
}

pub fn paint_fillet_grip(painter: &egui::Painter, grip: Pos2, zoom: f32, ink: Color32, hot: bool) {
    let half = canvas_scale::px(FILLET_GRIP_PX, zoom);
    let fill = if hot { GRIP_HANDLE_HOT } else { Color32::WHITE };
    painter.rect(
        Rect::from_center_size(grip, Vec2::splat(half * 2.0)),
        0.0,
        fill,
        EStroke::new(canvas_scale::px(1.0, zoom), ink),
        egui::StrokeKind::Inside,
    );
}

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
    if let Some(handle) = hit_test_resize_bands(screen, geom) {
        return Some(BoardHitTarget::Resize(handle));
    }
    if allow_rotate {
        for (i, rp) in geom.rotate_points.iter().enumerate() {
            if screen.distance(*rp) <= canvas_scale::hit_px(ROTATE_ZONE_PX, geom.zoom) {
                return Some(BoardHitTarget::Rotate(i as u8));
            }
        }
    }
    None
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

/// egui exposes no native rotate cursor: hide the OS cursor over rotate
/// zones and paint [`paint_rotate_cursor`] at the pointer instead.
pub fn cursor_for_rotate() -> CursorIcon {
    CursorIcon::None
}

/// Windows-style 90° corner rotate cursor: a quarter-arc with an arrowhead,
/// painted at the pointer (pair with `CursorIcon::None`).
pub fn paint_rotate_cursor(painter: &egui::Painter, pos: Pos2, color: Color32) {
    let r = 8.0;
    let n = 12;
    let start = 200.0f32.to_radians();
    let sweep = 90.0f32.to_radians();
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = start + sweep * i as f32 / n as f32;
            pos + Vec2::angled(a) * r
        })
        .collect();
    // Halo first, for contrast over arbitrary board content.
    painter.add(egui::Shape::line(
        pts.clone(),
        EStroke::new(3.5_f32, Color32::from_black_alpha(120)),
    ));
    painter.add(egui::Shape::line(pts.clone(), EStroke::new(1.8_f32, color)));
    let end = *pts.last().unwrap();
    let end_angle = start + sweep;
    let tangent = Vec2::angled(end_angle + std::f32::consts::FRAC_PI_2);
    let outward = Vec2::angled(end_angle);
    let s = 4.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            end + tangent * s,
            end + outward * s * 0.9,
            end - outward * s * 0.9,
        ],
        color,
        EStroke::NONE,
    ));
}

pub fn paint_selection(
    painter: &egui::Painter,
    geom: &SelectionGeom,
    outline: &[Pos2],
    color: Color32,
    hover: Option<BoardHitTarget>,
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
    let radius = corner.distance(rotate_point);
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
        EStroke::new(1.5_f32, color),
        4.0,
        3.0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_shape() -> slate_doc::scene::ShapeNode {
        slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,
            text: None,
        }
    }

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

    #[test]
    fn rotated_node_maps_n_handle_to_horizontal_cursor() {
        let xf = BoardXf {
            center: Pos2::ZERO,
            offset: Vec2::ZERO,
            z: 1.0,
        };
        // Rotated 90° the "N" edge sits on the left/right of the screen, so
        // its resize cursor must be horizontal, not vertical.
        let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 100.0, 50.0), 90.0);
        let c = cursor_for_resize(ResizeHandle::N, &geom);
        assert!(
            matches!(c, CursorIcon::ResizeEast | CursorIcon::ResizeWest),
            "expected horizontal resize cursor, got {c:?}"
        );
        // Unrotated it stays vertical.
        let geom0 = selection_geom(&xf, WorldRect::new(0.0, 0.0, 100.0, 50.0), 0.0);
        assert_eq!(
            cursor_for_resize(ResizeHandle::N, &geom0),
            CursorIcon::ResizeNorth
        );
    }

    #[test]
    fn wide_box_corner_uses_diagonal_cursor() {
        let xf = BoardXf {
            center: Pos2::ZERO,
            offset: Vec2::ZERO,
            z: 1.0,
        };
        // A wide group AABB: corner−center is nearly horizontal, but the
        // cursor must stay 45° (Windows / single-object convention).
        let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 200.0, 40.0), 0.0);
        assert_eq!(
            cursor_for_resize(ResizeHandle::Nw, &geom),
            CursorIcon::ResizeNorthWest
        );
        assert_eq!(
            cursor_for_resize(ResizeHandle::Se, &geom),
            CursorIcon::ResizeSouthEast
        );
        assert_eq!(
            cursor_for_resize(ResizeHandle::Ne, &geom),
            CursorIcon::ResizeNorthEast
        );
        assert_eq!(
            cursor_for_resize(ResizeHandle::Sw, &geom),
            CursorIcon::ResizeSouthWest
        );
    }

    #[test]
    fn edge_band_hits_between_handles() {
        let xf = BoardXf {
            center: Pos2::ZERO,
            offset: Vec2::ZERO,
            z: 1.0,
        };
        let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 100.0, 50.0), 0.0);
        // Midway along the top edge, well outside the N handle square.
        let on_edge = Pos2::new(25.0, 0.0);
        assert_eq!(hit_test_resize_bands(on_edge, &geom), Some(ResizeHandle::N));
        assert_eq!(hit_test_resize_handles(on_edge, &geom), None);
        assert_eq!(crop_handle_at(geom.edges[0], &geom), Some(ResizeHandle::N));
        assert_eq!(
            crop_handle_at(geom.edges[0] + Vec2::new(0.0, -40.0), &geom),
            None
        );
        assert_eq!(
            crop_handle_at(geom.corners[0], &geom),
            Some(ResizeHandle::Nw)
        );
        assert_eq!(
            hit_test_chrome(on_edge, &geom, true),
            Some(BoardHitTarget::Resize(ResizeHandle::N))
        );
    }

    #[test]
    fn crop_hit_zones_center_slop_boundary_and_outside() {
        let at = |z: f32| {
            let xf = BoardXf {
                center: Pos2::ZERO,
                offset: Vec2::ZERO,
                z,
            };
            selection_geom(&xf, WorldRect::new(0.0, 0.0, 200.0, 150.0), 0.0)
        };
        let geom = at(1.0);
        let corner_reach = canvas_scale::hit_px(CROP_ARM_PX + CROP_HIT_PAD_PX, 1.0);
        let edge_reach = canvas_scale::hit_px(CROP_BAR_THICK_PX * 0.5 + CROP_HIT_PAD_PX, 1.0);
        assert!(edge_reach >= canvas_scale::HIT_SLOP_PX);

        // Handle centers.
        let edges = [
            ResizeHandle::N,
            ResizeHandle::E,
            ResizeHandle::S,
            ResizeHandle::W,
        ];
        let corners = [
            ResizeHandle::Nw,
            ResizeHandle::Ne,
            ResizeHandle::Se,
            ResizeHandle::Sw,
        ];
        for i in 0..4 {
            assert_eq!(crop_handle_at(geom.edges[i], &geom), Some(edges[i]));
            assert_eq!(crop_handle_at(geom.corners[i], &geom), Some(corners[i]));
        }

        // Slop boundary, outside the box: just inside hits, just past misses.
        let w = Pos2::new(0.0, 75.0);
        assert_eq!(
            crop_handle_at(w + Vec2::new(-(edge_reach - 0.25), 0.0), &geom),
            Some(ResizeHandle::W)
        );
        assert_eq!(
            crop_handle_at(w + Vec2::new(-(edge_reach + 0.25), 0.0), &geom),
            None
        );
        let se = geom.corners[2];
        let diag = Vec2::splat(std::f32::consts::FRAC_1_SQRT_2);
        assert_eq!(
            crop_handle_at(se + diag * (corner_reach - 0.25), &geom),
            Some(ResizeHandle::Se)
        );
        assert_eq!(
            crop_handle_at(se + diag * (corner_reach + 0.25), &geom),
            None
        );

        // Full edge length, far from the painted bar, inside and outside.
        for y in [corner_reach + 1.0, 40.0, 110.0, 150.0 - corner_reach - 1.0] {
            assert_eq!(
                crop_handle_at(Pos2::new(-4.0, y), &geom),
                Some(ResizeHandle::W)
            );
            assert_eq!(
                crop_handle_at(Pos2::new(4.0, y), &geom),
                Some(ResizeHandle::W)
            );
        }

        // Corners win where a corner and an edge zone overlap.
        assert_eq!(
            crop_handle_at(Pos2::new(-3.0, 5.0), &geom),
            Some(ResizeHandle::Nw)
        );

        // Zoomed out, the zones keep the screen slop; zoomed in they cover
        // the bigger painted bar.
        let small = at(0.25);
        let w_small = Pos2::new(0.0, 150.0 * 0.25 * 0.5);
        assert_eq!(
            crop_handle_at(
                w_small + Vec2::new(-(canvas_scale::HIT_SLOP_PX - 0.5), 0.0),
                &small
            ),
            Some(ResizeHandle::W)
        );
        let big = at(3.0);
        let bar_edge = canvas_scale::px(CROP_BAR_THICK_PX, 3.0) * 0.5;
        assert_eq!(
            crop_handle_at(big.edges[3] + Vec2::new(-bar_edge, 0.0), &big),
            Some(ResizeHandle::W)
        );
    }

    #[test]
    fn rotate_zone_is_outside_the_corner() {
        let xf = BoardXf {
            center: Pos2::ZERO,
            offset: Vec2::ZERO,
            z: 1.0,
        };
        let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 100.0, 50.0), 0.0);
        assert_eq!(
            hit_test_chrome(geom.rotate_points[0], &geom, true),
            Some(BoardHitTarget::Rotate(0))
        );
        assert_eq!(
            hit_test_chrome(geom.rotate_points[0], &geom, false),
            None,
            "portals and other non-rotatable kinds must not offer rotate"
        );
        // The corner itself stays diagonal resize so aspect keys still apply.
        assert_eq!(
            hit_test_chrome(geom.corners[0], &geom, true),
            Some(BoardHitTarget::Resize(ResizeHandle::Nw))
        );
    }

    #[test]
    fn fillet_radius_clamps_to_short_side() {
        let rect = WorldRect::new(0.0, 0.0, 80.0, 40.0);
        let node = slate_doc::scene::Scene::default()
            .build_node(rect, slate_doc::scene::NodeKind::Shape(rect_shape()));
        let edge = slate_doc::geom::corner_grip_edge(&node, false).unwrap();
        let r = edge.amount_for_travel(edge.project([100.0, 100.0]));
        assert!((r - 20.0).abs() < 1e-4);
    }

    #[test]
    fn handle_squares_and_rotate_offset_track_zoom() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        let g1 = selection_geom(
            &BoardXf {
                center: Pos2::ZERO,
                offset: Vec2::ZERO,
                z: 1.0,
            },
            rect,
            0.0,
        );
        let g2 = selection_geom(
            &BoardXf {
                center: Pos2::ZERO,
                offset: Vec2::ZERO,
                z: 2.0,
            },
            rect,
            0.0,
        );
        let a = handle_rects(&g1)[0].1;
        let b = handle_rects(&g2)[0].1;
        assert!(
            (b.width() - a.width() * 2.0).abs() < 1e-3,
            "handle square froze: {} → {}",
            a.width(),
            b.width()
        );
        let off1 = g1.rotate_points[0].distance(g1.corners[0]);
        let off2 = g2.rotate_points[0].distance(g2.corners[0]);
        assert!(
            (off2 - off1 * 2.0).abs() < 1e-3,
            "rotate offset froze: {off1} → {off2}"
        );
    }

    #[test]
    fn fillet_grip_paints_at_40_percent_and_keeps_a_generous_hit() {
        assert!(
            (FILLET_GRIP_PX - 0.4 * 4.0).abs() < 1e-6,
            "painted half-size is 40% of the former 4 units, got {FILLET_GRIP_PX}"
        );
        let at = |z: f32| {
            let xf = BoardXf {
                center: Pos2::ZERO,
                offset: Vec2::ZERO,
                z,
            };
            selection_geom(&xf, WorldRect::new(0.0, 0.0, 400.0, 300.0), 0.0)
        };
        let grip = Pos2::new(50.0, 0.0);
        let geom = at(1.0);
        for off in [
            Vec2::new(7.5, 0.0),
            Vec2::new(-7.5, 7.5),
            Vec2::new(0.0, -7.5),
        ] {
            assert!(
                hit_test_fillet_grip(grip + off, &geom, grip),
                "the hit box still reaches 8 px at zoom 1 ({off:?})"
            );
        }
        assert!(!hit_test_fillet_grip(
            grip + Vec2::new(8.5, 0.0),
            &geom,
            grip
        ));
        let small = at(0.25);
        assert!(
            hit_test_fillet_grip(grip + Vec2::new(7.5, 0.0), &small, grip),
            "zoomed out, the reach never falls under the screen slop"
        );
        let big = at(4.0);
        assert!(hit_test_fillet_grip(
            grip + Vec2::new(19.5, 0.0),
            &big,
            grip
        ));
    }

    #[test]
    fn fillet_grip_screen_round_trips_the_radius_reader() {
        use slate_doc::scene::WorldRect;

        let xf = BoardXf {
            center: Pos2::ZERO,
            offset: Vec2::ZERO,
            z: 1.0,
        };
        for (w, h) in [(100.0, 100.0), (400.0, 100.0), (100.0, 400.0)] {
            for rot in [0.0, 37.0, 90.0, 180.0] {
                let rect = WorldRect::new(0.0, 0.0, w, h);
                let mut node = slate_doc::scene::Scene::default()
                    .build_node(rect, slate_doc::scene::NodeKind::Shape(rect_shape()));
                node.rotation_deg = rot;
                let edge = slate_doc::geom::corner_grip_edge(&node, false).unwrap();
                let max = w.min(h) * 0.5;
                for r in [0.0, 8.0, 24.0, (max * 0.45).floor()] {
                    let travel = corner_grip_rest_travel(&edge, r);
                    let grip = corner_grip_screen(&xf, &edge, travel);
                    let world = xf.s2w(grip);
                    let grab = travel - edge.project([world.x, world.y]);
                    let read = edge.amount_for_travel(edge.project([world.x, world.y]) + grab);
                    let expected = r.max(FILLET_GRIP_MIN_INSET_WORLD);
                    assert!(
                        (read - expected).abs() < 0.08,
                        "w={w} h={h} rot={rot} r={r}: anchored read {read}"
                    );
                }
            }
        }
    }
}
