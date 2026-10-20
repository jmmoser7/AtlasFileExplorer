use super::*;

fn rect_shape() -> slate_doc::scene::ShapeNode {
    slate_doc::scene::ShapeNode {
        shape: slate_doc::scene::ShapeKind::Rect,
        fill: None,
        stroke: slate_doc::scene::Stroke::default(),
        corner: slate_doc::scene::Corner::Square,
        sides: slate_doc::scene::default_regular_sides(),
        phase_deg: 0.0,
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
        hit_test_selection(geom.control_corners[0], &geom)
            == Some(BoardHitTarget::Resize(ResizeHandle::Nw))
    );
    assert!(
        hit_test_selection(geom.control_edges[1], &geom)
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
    let on_edge = Pos2::new(25.0, geom.control_corners[0].y);
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
        hit_test_chrome(geom.control_corners[0], &geom, true),
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
fn authored_handle_squares_scale_but_pointer_rotate_offset_is_screen_based() {
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
    let off1 = g1.rotate_points[0].distance(g1.control_corners[0]);
    let off2 = g2.rotate_points[0].distance(g2.control_corners[0]);
    assert!(
        (off2 - off1).abs() < 1e-3,
        "ephemeral rotation reach depends on projected size, not camera scale: {off1} → {off2}"
    );
}

#[test]
fn fillet_grip_keeps_native_paint_and_uses_a_cursor_expanding_screen_hit() {
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
            "the adaptive fillet disc reaches the pointer ({off:?})"
        );
    }
    assert!(!hit_test_fillet_grip(
        grip + Vec2::new(15.0, 0.0),
        &geom,
        grip
    ));
    let small = at(0.25);
    assert!(
        hit_test_fillet_grip(grip + Vec2::new(7.5, 0.0), &small, grip),
        "same generous reach while the projected host remains large"
    );
    let big = at(4.0);
    assert!(
        !hit_test_fillet_grip(grip + Vec2::new(19.5, 0.0), &big, grip),
        "larger zoom must not inflate pointer hit reach without bound"
    );
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

#[test]
fn same_projected_size_has_same_controls_across_world_sizes_and_zoom() {
    let at = |world_scale, zoom| {
        selection_geom(
            &BoardXf {
                center: Pos2::ZERO,
                offset: Vec2::ZERO,
                z: zoom,
            },
            WorldRect::new(0.0, 0.0, 48.0 * world_scale, 36.0 * world_scale),
            31.0,
        )
    };
    let a = at(1.0, 1.0);
    let b = at(8.0, 0.125);
    assert_eq!(a.control_corners, b.control_corners);
    assert_eq!(a.prominence, b.prominence);
    for point in a.control_corners.into_iter().chain(a.control_edges) {
        assert_eq!(chrome_pick(point, &a, true), chrome_pick(point, &b, true));
    }
}

#[test]
fn extra_small_loses_resize_before_rotation_and_point_retains_move() {
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let small = selection_geom(&xf, WorldRect::new(0.0, 0.0, 24.0, 18.0), 0.0);
    assert_eq!(
        hit_test_resize_bands(small.control_corners[0], &small),
        None
    );
    assert_eq!(
        chrome_pick(small.rotate_points[0], &small, true).map(|p| p.0),
        Some(BoardHitTarget::Rotate(0))
    );
    assert!(!hit_test_fillet_grip(
        small.corners[0],
        &small,
        small.corners[0]
    ));
    let point = selection_geom(&xf, WorldRect::new(0.0, 0.0, 1.0, 1.0), 0.0);
    for p in point.control_corners.into_iter().chain(point.rotate_points) {
        assert_eq!(chrome_pick(p, &point, true), None);
    }
    assert_eq!(
        body_pick(Pos2::new(6.0, 6.0), &point).map(|p| p.0),
        Some(BoardHitTarget::Body)
    );
    assert!(body_pick(Pos2::new(12.0, 12.0), &point).is_none());
}

#[test]
fn zooming_a_point_restores_fine_controls_without_authored_size_cutoffs() {
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 128.0,
    };
    let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 1.0, 1.0), 0.0);
    assert_eq!(
        geom.prominence,
        Prominence {
            wire: 1.0,
            resize: 1.0,
            rotate: 1.0
        }
    );
    assert_eq!(
        chrome_pick(geom.control_corners[0], &geom, true).map(|p| p.0),
        Some(BoardHitTarget::Resize(ResizeHandle::Nw))
    );
    assert!(hit_test_fillet_grip(
        Pos2::new(15.0, 0.0),
        &geom,
        Pos2::new(10.0, 0.0)
    ));
}

#[test]
fn extending_a_buried_control_frame_never_changes_drag_geometry() {
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let mut geom = selection_geom(&xf, WorldRect::new(20.0, 30.0, 48.0, 36.0), 27.0);
    let actual = geom.corners;
    let edges = geom.edges;
    let envelope = geom.control_corners.map(|p| p + Vec2::splat(12.0));
    let host_distance = body_geometry_distance(envelope[0], &geom);
    set_control_frame(&mut geom, envelope);
    assert_eq!(geom.corners, actual);
    assert_eq!(geom.edges, edges);
    assert_eq!(geom.control_corners, envelope);
    assert_eq!(body_geometry_distance(envelope[0], &geom), host_distance);
    assert!(chrome_pick(envelope[0], &geom, true).is_some());
}

#[test]
fn full_edge_fields_expand_near_the_cursor_and_leave_a_move_core() {
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let geom = selection_geom(&xf, WorldRect::new(0.0, 0.0, 240.0, 112.0), 0.0);
    for x in [40.0, 80.0, 120.0, 160.0, 200.0] {
        assert_eq!(
            chrome_pick(Pos2::new(x, -8.5), &geom, true).map(|p| p.0),
            Some(BoardHitTarget::Resize(ResizeHandle::N))
        );
    }
    let center = Pos2::new(120.0, 56.0);
    assert_eq!(chrome_pick(center, &geom, true), None);
    assert_eq!(
        body_pick(center, &geom).map(|p| p.0),
        Some(BoardHitTarget::Body)
    );
}
