//! Crop paint and picking share one adaptive screen-space size.
use super::*;

/// Photoshop crop chrome, in screen px before `canvas_scale`.
pub const CROP_ARM_PX: f32 = 12.0;
pub const CROP_BAR_LEN_PX: f32 = 18.0;
pub const CROP_BAR_THICK_PX: f32 = 5.0;
/// Extra hit outside the painted bracket or bar.
pub const CROP_HIT_PAD_PX: f32 = 4.0;

/// Scale paint and hit reach together when screen-space handles would overlap.
/// A corner gets at most a quarter of the shortest side, leaving a body to pan.
fn handle_scale(geom: &SelectionGeom) -> f32 {
    let shortest = (0..4)
        .map(|i| geom.corners[i].distance(geom.corners[(i + 1) % 4]))
        .fold(f32::INFINITY, f32::min);
    (shortest / (4.0 * canvas_scale::hit_px(CROP_ARM_PX + CROP_HIT_PAD_PX, geom.zoom)))
        .clamp(0.0, 1.0)
}

/// Crop handle under `screen` and the pointer's distance to that corner or
/// edge. A corner zone is a disc of the bracket arm plus pad; an edge zone
/// is a band along the whole side, half the bar thickness plus pad on each
/// side. Both reach outside the box, and a corner beats any edge it
/// overlaps. [`canvas_scale::hit_px`] carries the zoom and never lets a
/// zone shrink below the screen slop unless neighboring handles would overlap.
pub fn crop_handle_pick(screen: Pos2, geom: &SelectionGeom) -> Option<(ResizeHandle, f32)> {
    let z = geom.zoom;
    let scale = handle_scale(geom);
    let corner_hit = canvas_scale::hit_px(CROP_ARM_PX + CROP_HIT_PAD_PX, z) * scale;
    let edge_hit = canvas_scale::hit_px(CROP_BAR_THICK_PX * 0.5 + CROP_HIT_PAD_PX, z) * scale;
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
    handle_hot: Color32,
) {
    let z = geom.zoom;
    let scale = handle_scale(geom);
    let arm = canvas_scale::px(CROP_ARM_PX, z) * scale;
    let half_len = canvas_scale::px(CROP_BAR_LEN_PX, z) * scale * 0.5;
    let half_thick = canvas_scale::px(CROP_BAR_THICK_PX, z) * scale * 0.5;
    let white = Color32::WHITE;
    let ink = Color32::from_black_alpha(200);
    let stroke = canvas_scale::px(2.0, z) * scale;
    let under = canvas_scale::px(3.5, z) * scale;
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
            handle_hot
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
        let fill = if hot == Some(edge) { handle_hot } else { white };
        painter.add(egui::Shape::convex_polygon(
            bar,
            fill,
            egui::Stroke::new(canvas_scale::px(1.0, z) * scale, ink),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_crop_controls_leave_body_and_all_eight_handles_distinct() {
        for zoom in [0.1, 0.5, 1.0, 3.0] {
            for rotation in [0.0, 31.0, 90.0] {
                let xf = BoardXf {
                    center: Pos2::ZERO,
                    offset: Vec2::ZERO,
                    z: zoom,
                };
                let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 24.0, 18.0), rotation);
                let center = geom.corners[0] + (geom.corners[2] - geom.corners[0]) * 0.5;
                assert!(handle_scale(&geom) < 1.0);
                assert_eq!(
                    crop_handle_at(center, &geom),
                    None,
                    "body remains available at z={zoom}"
                );
                for (i, corner) in geom.corners.iter().enumerate() {
                    assert_eq!(
                        crop_handle_at(*corner, &geom),
                        Some(
                            [
                                ResizeHandle::Nw,
                                ResizeHandle::Ne,
                                ResizeHandle::Se,
                                ResizeHandle::Sw,
                            ][i]
                        )
                    );
                }
                for (i, edge) in geom.edges.iter().enumerate() {
                    assert_eq!(
                        crop_handle_at(*edge, &geom),
                        Some(
                            [
                                ResizeHandle::N,
                                ResizeHandle::E,
                                ResizeHandle::S,
                                ResizeHandle::W,
                            ][i]
                        )
                    );
                }
                let painted_bar = canvas_scale::px(CROP_BAR_LEN_PX, zoom) * handle_scale(&geom);
                assert!(painted_bar < geom.corners[0].distance(geom.corners[1]));
            }
        }
    }
}
