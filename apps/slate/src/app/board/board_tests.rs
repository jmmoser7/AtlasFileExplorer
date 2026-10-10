//! Board geometry and texture unit tests.

use super::*;

#[test]
fn a_rotated_frame_keeps_its_title_edge_on_top() {
    // Letter portrait, 8.5 × 11 at 72 u per inch.
    let rect = WorldRect::new(100.0, 50.0, 612.0, 792.0);
    let [a, b] = upper_edge(rect, 0.0);
    assert_eq!((a, b), (Pos2::new(100.0, 50.0), Pos2::new(712.0, 50.0)));
    for rotation in [90.0, -90.0, 180.0, 270.0, 30.0, -30.0, 135.0] {
        let [a, b] = upper_edge(rect, rotation);
        let bounds = rect.rotated_bounds(rotation);
        let top = a.y.min(b.y);
        assert!((top - bounds.y).abs() < 0.01, "{rotation}° edge is the top");
        assert!(b.x > a.x, "{rotation}° label reads left to right");
        let corners = rect.corners_rotated(rotation);
        let mid = a.lerp(b, 0.5);
        let center = Pos2::new(rect.center().0, rect.center().1);
        let up = Vec2::new((b - a).y, -(b - a).x).normalized();
        assert!(
            (mid - center).dot(up) > 0.0,
            "{rotation}° normal is outward"
        );
        assert!(corners
            .iter()
            .all(|&(x, y)| (Pos2::new(x, y) - center).dot(up) <= (mid - center).dot(up) + 0.01));
    }
    // Turned to landscape, the title runs along the long 792 u side.
    for rotation in [90.0, -90.0] {
        let [a, b] = upper_edge(rect, rotation);
        assert!(((b - a).length() - 792.0).abs() < 0.01);
        assert!((a.y - b.y).abs() < 0.01);
    }
}

fn frame(rect: WorldRect, rotation_deg: f32) -> Node {
    let mut node = slate_doc::scene::Scene::default().build_node(
        rect,
        NodeKind::Frame(slate_doc::scene::FrameNode {
            title: "Slide 2".into(),
            order: 1,
            fill: Rgba::WHITE,
            fill_authored: false,
            assignments: Default::default(),
            stroke: slate_doc::scene::Stroke::none(),
            corner: Corner::Square,
        }),
    );
    node.rotation_deg = rotation_deg;
    node
}

/// The screenshot case: a 223.57 × 289.326 portrait frame (and a Letter
/// 8.5 × 11 one) turned to landscape. The title's bottom-left sits at the
/// rotated frame's visual top-left, inset and lifted exactly as on an
/// unrotated frame, horizontal, never offset toward the old portrait corner.
#[test]
fn a_quarter_turned_frame_title_sits_at_its_visual_top_left() {
    let xf = BoardXf {
        center: Pos2::new(400.0, 300.0),
        offset: Vec2::new(50.0, -20.0),
        z: 1.7,
    };
    let size = Vec2::new(90.0, 20.0);
    let (inset, lift) = (2.0, 6.0);
    for (w, h) in [(223.57, 289.326), (612.0, 792.0)] {
        let rect = WorldRect::new(100.0, 50.0, w, h);
        for rotation in [0.0, 90.0, -90.0, 180.0, 270.0] {
            let node = frame(rect, rotation);
            let bounds = xf.rect_w2s(rect.rotated_bounds(rotation));
            let (center, angle) = frame_label_placement(&xf, &node, false, inset, lift, size);
            assert_eq!(angle, 0.0, "{w}×{h} at {rotation}° is horizontal");
            let bottom_left = center + Vec2::new(-size.x, size.y) * 0.5;
            let want = bounds.left_top() + Vec2::new(inset * xf.z, -lift * xf.z);
            assert!(
                (bottom_left - want).length() < 0.01,
                "{w}×{h} at {rotation}°: {bottom_left:?} vs {want:?}"
            );
            // The tag label mirrors it at the visual top-right.
            let (center, angle) = frame_label_placement(&xf, &node, true, inset, lift, size);
            assert_eq!(angle, 0.0);
            let bottom_right = center + Vec2::new(size.x, size.y) * 0.5;
            let want = bounds.right_top() + Vec2::new(-inset * xf.z, -lift * xf.z);
            assert!(
                (bottom_right - want).length() < 0.01,
                "{w}×{h} at {rotation}° tags"
            );
        }
    }
    // Off-axis turns still read left to right, riding the upper edge.
    for rotation in [30.0, -30.0, 135.0, -150.0] {
        let node = frame(WorldRect::new(100.0, 50.0, 223.57, 289.326), rotation);
        let (_, angle) = frame_label_placement(&xf, &node, false, inset, lift, size);
        assert!(angle.cos() > 0.0, "{rotation}° reads left to right");
        assert!(
            angle.abs() <= std::f32::consts::FRAC_PI_4 + 1e-4,
            "{rotation}°"
        );
    }
}

#[test]
fn sheet_viewport_shows_a_dozen_and_scrolls_the_rest() {
    let view = Vec2::new(IMAGE_W, IMAGE_H);
    let dozen = sheet_viewport(view, 12, 12, Vec2::ZERO);
    assert!(dozen.max_scroll.length() < 0.01);
    assert!((dozen.col_w - SHEET_COL_WORLD).abs() < 0.01);
    assert!((dozen.row_h - SHEET_ROW_WORLD).abs() < 0.01);

    let short = sheet_viewport(view, 3, 4, Vec2::ZERO);
    assert!(short.max_scroll.length() < 0.01);
    assert!((short.col_w - IMAGE_W / 3.0).abs() < 0.01);

    let long = sheet_viewport(view, 4, 40, Vec2::new(0.0, 10_000.0));
    assert!(long.max_scroll.y > SHEET_ROW_WORLD);
    assert!((long.scroll.y - long.max_scroll.y).abs() < 0.01);
    assert!((long.row_h - SHEET_ROW_WORLD).abs() < 0.01);

    let tall = sheet_viewport(Vec2::new(IMAGE_W, IMAGE_H * 2.0), 4, 40, Vec2::ZERO);
    assert!(tall.max_scroll.y > 0.0);
    let visible = IMAGE_H * 2.0 / tall.row_h;
    assert!(visible > 20.0);
}

#[test]
fn single_drop_centers_on_point() {
    let rects = grid_drop_rects(&[(100.0, 80.0)], Pos2::new(10.0, 20.0));
    assert_eq!(rects.len(), 1);
    let (cx, cy) = rects[0].center();
    assert!((cx - 10.0).abs() < 1e-3 && (cy - 20.0).abs() < 1e-3);
}

#[test]
fn rotated_host_texture_maps_outline_to_full_uv() {
    let rect = WorldRect::new(10.0, 20.0, 100.0, 50.0);
    let corners = rect.corners_rotated(90.0);
    let expected = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];
    for (corner, expected) in corners.into_iter().zip(expected) {
        let uv = host_texture_uv(rect, 90.0, Crop::full(), corner);
        assert!((uv.x - expected.x).abs() < 1e-4);
        assert!((uv.y - expected.y).abs() < 1e-4);
    }
}

/// Every vertex samples the texel at its own node-local position: undo
/// the node rotation on the painted point and read it against `tex_rect`.
fn assert_rigid_texture(
    xf: &BoardXf,
    node_rect: WorldRect,
    tex_rect: WorldRect,
    rotation_deg: f32,
    vertices: &[(Pos2, Pos2)],
) {
    let (cx, cy) = node_rect.center();
    for (pos, uv) in vertices {
        let w = xf.s2w(*pos);
        let (lx, ly) = slate_doc::geom::world_to_local_about(w.x, w.y, cx, cy, rotation_deg);
        let want = Pos2::new(
            (lx - tex_rect.x) / tex_rect.w,
            (ly - tex_rect.y) / tex_rect.h,
        );
        assert!(
            (uv.x - want.x).abs() < 1e-3 && (uv.y - want.y).abs() < 1e-3,
            "vertex {pos:?}: uv {uv:?}, expected {want:?}"
        );
    }
}

fn vertex_with_uv(vertices: &[(Pos2, Pos2)], uv: Pos2) -> Pos2 {
    vertices
        .iter()
        .find(|(_, v)| (*v - uv).length() < 1e-3)
        .map(|(p, _)| *p)
        .unwrap_or_else(|| panic!("no vertex samples {uv:?}: {vertices:?}"))
}

/// A 2:1 picture turned 90°: its texture turns with it. UV corners sit
/// on the rotated corners and the texel spacing stays 2:1 (no stretch).
#[test]
fn a_quarter_turned_picture_turns_its_pixels_rigidly() {
    let xf = BoardXf {
        center: Pos2::new(400.0, 300.0),
        offset: Vec2::new(30.0, -10.0),
        z: 1.5,
    };
    let rect = WorldRect::new(10.0, 20.0, 200.0, 100.0);
    let vertices = node_texture_vertices(
        &xf,
        rect,
        rect,
        90.0,
        Corner::Square,
        Crop::full(),
        Mirror::default(),
    );
    assert_rigid_texture(&xf, rect, rect, 90.0, &vertices);
    let corners = rect.corners_rotated(90.0);
    let uv_corners = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];
    for ((wx, wy), uv) in corners.into_iter().zip(uv_corners) {
        let at = vertex_with_uv(&vertices, uv);
        assert!((at - xf.w2s(Pos2::new(wx, wy))).length() < 1e-2, "{uv:?}");
    }
    let top = xf.s2w(vertex_with_uv(&vertices, uv_corners[1]))
        - xf.s2w(vertex_with_uv(&vertices, uv_corners[0]));
    let side = xf.s2w(vertex_with_uv(&vertices, uv_corners[2]))
        - xf.s2w(vertex_with_uv(&vertices, uv_corners[1]));
    assert!((top.length() - 200.0).abs() < 1e-2, "texture width {top:?}");
    assert!(
        (side.length() - 100.0).abs() < 1e-2,
        "texture height {side:?}"
    );
    // The image's top edge now runs down the screen.
    assert!(top.x.abs() < 1e-2 && top.y > 0.0, "{top:?}");
}

#[test]
fn a_rotated_cropped_picture_keeps_its_crop_window_rigid() {
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let rect = WorldRect::new(0.0, 0.0, 120.0, 60.0);
    let crop = Crop {
        x: 0.25,
        y: 0.1,
        w: 0.5,
        h: 0.6,
    };
    for rotation in [30.0, 90.0, 180.0, -135.0] {
        let vertices = node_texture_vertices(
            &xf,
            rect,
            rect,
            rotation,
            Corner::Rounded { radius: 12.0 },
            crop,
            Mirror::default(),
        );
        let (cx, cy) = rect.center();
        for (pos, uv) in &vertices {
            let (lx, ly) = slate_doc::geom::world_to_local_about(pos.x, pos.y, cx, cy, rotation);
            let want = Pos2::new(
                crop.x + (lx - rect.x) / rect.w * crop.w,
                crop.y + (ly - rect.y) / rect.h * crop.h,
            );
            assert!(
                (*uv - want).length() < 1e-3,
                "{rotation}°: {uv:?} vs {want:?}"
            );
        }
    }
}

/// Crop mode's ghost lays the whole source on the content rect and turns
/// about the node's center, not the content rect's.
#[test]
fn the_crop_ghost_turns_about_the_node_center() {
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let rect = WorldRect::new(100.0, 50.0, 100.0, 60.0);
    let crop = Crop {
        x: 0.5,
        y: 0.25,
        w: 0.5,
        h: 0.5,
    };
    let content = board_crop::content_rect(rect, crop);
    let vertices = node_texture_vertices(
        &xf,
        rect,
        content,
        90.0,
        Corner::Square,
        Crop::full(),
        Mirror::default(),
    );
    assert_rigid_texture(&xf, rect, content, 90.0, &vertices);
}

/// A mirrored picture reads its crop window backwards along the flipped
/// axis, turned rigidly with the node like any other texture.
#[test]
fn a_mirrored_picture_reads_its_crop_window_backwards() {
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let rect = WorldRect::new(10.0, 20.0, 200.0, 100.0);
    let crop = Crop {
        x: 0.25,
        y: 0.1,
        w: 0.5,
        h: 0.6,
    };
    let mirror = Mirror { x: true, y: false };
    for rotation in [0.0, 90.0, -30.0] {
        let vertices =
            node_texture_vertices(&xf, rect, rect, rotation, Corner::Square, crop, mirror);
        let corners = rect.corners_rotated(rotation);
        let top_left = xf.w2s(Pos2::new(corners[0].0, corners[0].1));
        let top_right = xf.w2s(Pos2::new(corners[1].0, corners[1].1));
        assert!(
            (vertex_with_uv(&vertices, Pos2::new(0.75, 0.1)) - top_left).length() < 1e-2,
            "{rotation}°"
        );
        assert!(
            (vertex_with_uv(&vertices, Pos2::new(0.25, 0.1)) - top_right).length() < 1e-2,
            "{rotation}°"
        );
    }
}

#[test]
fn grid_drop_caps_at_ten_columns_and_centers() {
    let sizes = vec![(100.0, 80.0); 12];
    let rects = grid_drop_rects(&sizes, Pos2::new(0.0, 0.0));
    assert_eq!(rects.len(), 12);
    // 10 columns max: item 10 wraps to the second row.
    assert!((rects[0].y - rects[9].y).abs() < 1e-3);
    assert!(rects[10].y > rects[0].y);
    // Cell pitch = max natural width + 16px gap.
    assert!(((rects[1].x - rects[0].x) - 116.0).abs() < 1e-3);
    // The whole grid is centered on the drop point.
    let min_x = rects.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
    let max_x = rects
        .iter()
        .map(|r| r.x + r.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = rects.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
    let max_y = rects
        .iter()
        .map(|r| r.y + r.h)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(((min_x + max_x) * 0.5).abs() < 1e-3);
    assert!(((min_y + max_y) * 0.5).abs() < 1e-3);
}

#[test]
fn group_scale_anchor_is_opposite_corner_or_center() {
    let gb = WorldRect::new(0.0, 0.0, 100.0, 50.0);
    assert_eq!(group_scale_anchor(gb, 0, false), (100.0, 50.0)); // Nw → Se
    assert_eq!(group_scale_anchor(gb, 4, false), (0.0, 0.0)); // Se → Nw
    assert_eq!(group_scale_anchor(gb, 3, false), (0.0, 25.0)); // E → W edge
    assert_eq!(group_scale_anchor(gb, 0, true), (50.0, 25.0)); // Ctrl → center
}

#[test]
fn ellipse_outline_stays_smoother_than_a_fixed_polygon() {
    let rect = Rect::from_center_size(Pos2::ZERO, Vec2::splat(360.0));
    let pts = ellipse_outline(rect);
    assert!(
        pts.len() > 64,
        "a 180px radius circle must not fall back to a coarse polygon, got {}",
        pts.len()
    );
    let radius = 180.0_f32;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let error = radius - mid.to_vec2().length();
        assert!(
            error <= ELLIPSE_CHORD_PX + 0.02,
            "chord error {error} exceeds {ELLIPSE_CHORD_PX}"
        );
    }
}
