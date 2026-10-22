//! Native adapter for the shared continuous selection fields.
use super::*;
use hit_regions::Field;

const CORNERS: [ResizeHandle; 4] = [
    ResizeHandle::Nw,
    ResizeHandle::Ne,
    ResizeHandle::Se,
    ResizeHandle::Sw,
];
const EDGES: [ResizeHandle; 4] = [
    ResizeHandle::N,
    ResizeHandle::E,
    ResizeHandle::S,
    ResizeHandle::W,
];

/// Allows an explicitly selected buried object's coherent outside frame to
/// change without moving the true host or its transform anchors.
pub fn set_control_frame(geom: &mut SelectionGeom, corners: [Pos2; 4]) {
    geom.control_corners = corners;
    geom.control_edges = hit_regions::edge_midpoints(&corners);
    geom.rotate_points = hit_regions::rotation_points(&corners, geom.prominence.rotate);
}

pub fn paint_control_frame(painter: &egui::Painter, geom: &SelectionGeom, color: Color32) {
    if geom
        .corners
        .iter()
        .zip(geom.control_corners)
        .any(|(a, b)| a.distance(b) > 0.5)
    {
        hit_regions::paint_control_frame(
            painter,
            &geom.control_corners,
            color,
            geom.prominence.resize,
        );
    }
}

pub(super) fn resize_pick(screen: Pos2, geom: &SelectionGeom) -> Option<(ResizeHandle, f32)> {
    if hit_regions::in_move_core(screen, &geom.corners) {
        return None;
    }
    let mut best = None;
    let mut consider = |handle, score| {
        if best.is_none_or(|(_, previous)| score < previous) {
            best = Some((handle, score));
        }
    };
    if let Some(reach) = Domain::Resize.reach(geom.projected_shortest, CORNER_HIT_PX) {
        for (i, corner) in geom.control_corners.iter().enumerate() {
            if let Some(distance) = Field::Disc(*corner).pick(screen, screen, reach) {
                consider(CORNERS[i], 0.06 + 0.6 * distance);
            }
        }
    }
    if let Some(reach) = Domain::Resize.reach(geom.projected_shortest, EDGE_BAND_PX) {
        for (i, &handle) in EDGES.iter().enumerate() {
            let field = Field::Segment {
                a: geom.control_corners[i],
                b: geom.control_corners[(i + 1) % 4],
            };
            if let Some(distance) = field.pick(screen, screen, reach) {
                consider(handle, 0.65 + 0.25 * distance);
            }
        }
    }
    best
}

/// Scores match the accepted sandbox: corners have priority over edge bands;
/// an outside rotation arc can win where its interior is closer than a band.
/// Scene stacking and host-geometry ties are resolved by the board owner.
pub fn chrome_pick(
    screen: Pos2,
    geom: &SelectionGeom,
    allow_rotate: bool,
) -> Option<(BoardHitTarget, f32)> {
    let mut best =
        resize_pick(screen, geom).map(|(handle, score)| (BoardHitTarget::Resize(handle), score));
    if !allow_rotate || hit_regions::in_move_core(screen, &geom.corners) {
        return best;
    }
    let Some(reach) = Domain::Rotate.reach(geom.projected_shortest, ROTATE_ZONE_PX) else {
        return best;
    };
    let center =
        geom.control_corners[0] + (geom.control_corners[2] - geom.control_corners[0]) * 0.5;
    for (i, corner) in geom.control_corners.iter().enumerate() {
        let outward = *corner - center;
        let field = Field::Arc {
            center: *corner,
            bearing: outward.y.atan2(outward.x),
            radius: hit_regions::rotation_radius(geom.prominence.rotate),
        };
        if let Some(distance) = field.pick(screen, screen, reach) {
            let score = 0.15 + 0.6 * distance;
            if best.is_none_or(|(_, previous)| score < previous) {
                best = Some((BoardHitTarget::Rotate(i as u8), score));
            }
        }
    }
    best
}

pub fn body_pick(screen: Pos2, geom: &SelectionGeom) -> Option<(BoardHitTarget, f32)> {
    hit_regions::body_contains(screen, &geom.corners).then_some((BoardHitTarget::Body, 1.0))
}

/// Conservative layout exclusion for an entire foreground transform field.
pub fn chrome_bounds(geom: &SelectionGeom, allow_rotate: bool) -> Rect {
    let resize = Domain::Resize
        .reach(geom.projected_shortest, CORNER_HIT_PX)
        .map_or(0.0, |r| hit_regions::cursor_reach(r, 0.0));
    let rotate = if allow_rotate {
        Domain::Rotate
            .reach(geom.projected_shortest, ROTATE_ZONE_PX)
            .map_or(0.0, |r| {
                hit_regions::rotation_radius(geom.prominence.rotate)
                    + hit_regions::cursor_reach(r, 0.0)
            })
    } else {
        0.0
    };
    Rect::from_points(&geom.control_corners).expand(resize.max(rotate))
}

#[cfg(test)]
pub fn body_geometry_distance(screen: Pos2, geom: &SelectionGeom) -> f32 {
    hit_regions::body_distance(screen, &geom.corners)
}
