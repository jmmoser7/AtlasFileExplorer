//! Layer filters around cached outline geometry and explicit control families.
use super::*;

#[test]
fn frames_and_hidden_underlays_do_not_make_a_foreground_port_interior_only() {
    for frame in [true, false] {
        let mut h = grip_board("regions_ineligible_underlays");
        let lower = if frame {
            h.seed_frame(None)
        } else {
            let id = card(&mut h, WorldRect::new(120.0, 120.0, 400.0, 240.0));
            h.app.patch_nodes(&[id], |node| node.hidden = true);
            id
        };
        let front = card(&mut h, WorldRect::new(200.0, 200.0, 240.0, 112.0));
        h.app.board_sel.clear();
        h.frame();
        let xf = h.app.board_xf();
        let scene = &h.app.doc().scene;
        assert!(
            scene.outline_overlaps(front, 0.25 / xf.z).contains(&lower),
            "fixture warms a real overlap that eligibility must filter"
        );
        let host = h.app.wire_host(scene.node(front).unwrap());
        let anchor = xf.w2s(Pos2::from(host.anchor(Side::Right, 0.5)));
        let outward = EVec2::from(host.outward(Side::Right, 0.5));
        let point = anchor + outward * 2.0;
        assert_eq!(
            h.app.wire_grip_at(point, &xf).map(|p| (p.0, p.1)),
            Some((front, Side::Right)),
            "frame={frame}: a non-occluding underlay removed the normal exterior port field"
        );
        assert!(matches!(
            h.app
                .begin_gesture_for_test(point, xf.s2w(point), egui::Modifiers::NONE),
            Some(board::BoardDrag::Wire(_))
        ));
    }
}

#[test]
fn a_compact_front_hosts_dormant_wire_domain_still_protects_its_transform_fields() {
    for expected in [Hit::Resize(Handle::Se), Hit::Rotate(2)] {
        let mut h = grip_board("regions_dormant_wire_front_transform");
        let lower = card(&mut h, WorldRect::new(0.0, 0.0, 240.0, 112.0));
        let front = card(&mut h, WorldRect::new(0.0, 0.0, 48.0, 36.0));
        h.app.board_sel.clear();
        h.frame();
        let xf = h.app.board_xf();
        let lower_host = h.app.wire_host(h.app.doc().scene.node(lower).unwrap());
        let point = xf.w2s(Pos2::from(lower_host.anchor(Side::Right, 0.5)));
        let geom = h
            .app
            .node_interaction_geom(h.app.doc().scene.node(front).unwrap(), &xf);
        let feature = if matches!(expected, Hit::Resize(_)) {
            geom.control_corners[2]
        } else {
            geom.rotate_points[2]
        };
        let delta = (point - feature) / xf.z;
        h.app.patch_nodes(&[front], |node| {
            node.rect.x += delta.x;
            node.rect.y += delta.y;
            node.hidden = true;
        });
        h.frame();
        assert_eq!(
            h.app.wire_grip_at(point, &xf).map(|p| (p.0, p.1)),
            Some((lower, Side::Right)),
            "fixture must expose a genuine competing rear port"
        );
        h.app.patch_nodes(&[front], |node| node.hidden = false);
        h.frame();
        let scene = &h.app.doc().scene;
        assert!(scene.outline_overlaps(front, 0.25 / xf.z).contains(&lower));
        let geom = h.app.node_interaction_geom(scene.node(front).unwrap(), &xf);
        assert_eq!(geom.prominence.wire, 0.0);
        assert_eq!(
            board_handles::chrome_pick(point, &geom, true).map(|p| p.0),
            Some(expected)
        );
        assert_eq!(h.app.transform_hit_at(point), Some((Some(front), expected)));
        assert!(
            h.app.wire_grip_at(point, &xf).is_none(),
            "a dormant front Wire domain must not open a hole through its {expected:?} field"
        );
        let drag = h
            .app
            .begin_gesture_for_test(point, xf.s2w(point), egui::Modifiers::NONE);
        assert!(match expected {
            Hit::Resize(_) =>
                matches!(drag, Some(board::BoardDrag::Resize { id, .. }) if id == front),
            Hit::Rotate(_) =>
                matches!(drag, Some(board::BoardDrag::Rotate { id, .. }) if id == front),
            _ => unreachable!(),
        });
    }
}

#[test]
fn a_rear_polygons_explicit_rotation_halo_cannot_override_a_retained_front_body() {
    let (mut h, polygon, vertices) = closed_form_board(
        "regions_polygon_halo_under_body",
        slate_doc::scene::ShapeKind::RegularPolygon,
    );
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(polygon).unwrap();
    let center = xf.w2s(Pos2::from(node.rect.center()));
    let vertex = xf.w2s(vertices[0]);
    let point = vertex + (vertex - center).normalized() * 18.0;
    assert_eq!(
        h.app.polygon_rotate_at(node, point, &xf),
        Some(0),
        "fixture retains the explicit polygon halo"
    );
    let world = xf.s2w(point);
    let front = card(
        &mut h,
        WorldRect::new(world.x - 80.0, world.y - 60.0, 160.0, 120.0),
    );
    h.frame();
    assert!(h
        .app
        .doc()
        .scene
        .outline_overlaps(front, 0.25 / xf.z)
        .contains(&polygon));
    assert_eq!(
        h.app.transform_hit_at(point),
        Some((Some(front), Hit::Body))
    );
    let drag = h
        .app
        .begin_gesture_for_test(point, world, egui::Modifiers::NONE);
    assert!(matches!(drag, Some(board::BoardDrag::Move { ids, .. }) if ids == vec![front]));
}

#[test]
fn an_unselected_image_fully_covered_by_a_nonrotating_portal_has_no_rear_rotate_halo() {
    let mut h = grip_board("regions_buried_image_portal_halo");
    let rect = WorldRect::new(0.0, 0.0, 120.0, 80.0);
    let rear = card(&mut h, rect);
    let mut portal = slate_doc::scene::PortalNode::unbound_web("Cover");
    portal.corner = slate_doc::scene::Corner::Rounded { radius: 0.0 };
    let node = h
        .app
        .doc_mut()
        .scene
        .build_node(rect, NodeKind::Portal(portal));
    let front = h.app.add_nodes(vec![node])[0];
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let geom = board_handles::selection_geom(&xf, rect, 0.0);
    let point = geom.rotate_points[0];
    assert_eq!(
        board_handles::chrome_pick(point, &geom, true).map(|p| p.0),
        Some(Hit::Rotate(0))
    );
    assert!(
        board_handles::chrome_pick(point, &geom, false).is_none(),
        "probe must lie beyond the portal's resize field"
    );
    assert!(h
        .app
        .doc()
        .scene
        .outline_covering_nodes(rear, 0.25 / xf.z)
        .contains(&front));
    assert!(
        h.app.transform_hit_at(point).is_none(),
        "an invisible rear host must not lend rotation to the front portal"
    );
    assert!(!matches!(
        h.app
            .begin_gesture_for_test(point, xf.s2w(point), egui::Modifiers::NONE),
        Some(board::BoardDrag::Rotate { .. })
    ));
    h.app.board_sel = std::iter::once(rear).collect();
    h.frame();
    let node = h.app.doc().scene.node(rear).unwrap();
    let selected = h.app.node_interaction_geom(node, &xf);
    assert_ne!(selected.control_corners, geom.control_corners);
    assert!(
        h.app
            .interaction_move_header(node, &selected, &xf)
            .is_some(),
        "explicit rear selection retains its outside move affordance"
    );
}

#[test]
fn a_compact_displaced_fillet_grip_stays_under_the_press_and_tracks_edge_motion_continuously() {
    let mut h = grip_board("regions_compact_fillet_capture");
    let id = card(&mut h, WorldRect::new(0.0, 0.0, 48.0, 36.0));
    h.app.patch_nodes(&[id], |node| {
        slate_doc::scene::set_corner(node, slate_doc::scene::Corner::Rounded { radius: 18.0 });
    });
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    let idle = h
        .app
        .fillet_grip_at(&before, &xf)
        .expect("compact fillet is relevant");
    let idle_grips = h.app.corner_grips(&before, &xf);
    assert_eq!(idle_grips, vec![(None, idle)]);
    let edge = h.app.node_grip_edge(&before, None).unwrap();
    let direction = EVec2::from(edge.dir);
    let press = xf.s2w(idle);
    h.app.board_drag = h.app.begin_fillet_drag(idle, press);
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::FilletRadius { id: owner, .. }) if owner == id)
    );
    assert_eq!(
        h.app.corner_grips(&before, &xf),
        idle_grips,
        "capturing the grip cannot move its displayed point"
    );
    assert_eq!(h.app.fillet_grip_at(&before, &xf), Some(idle));
    h.app.update_fillet_drag(press, false);
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before),
        "stationary capture must preserve the authored radius"
    );
    for distance in [0.1, 0.5, 1.0, 2.0, 3.0] {
        let cursor = press - direction * distance;
        h.app.update_fillet_drag(cursor, false);
        let node = h.app.doc().scene.node(id).unwrap();
        let displayed = h.app.fillet_grip_at(node, &xf).unwrap();
        assert!(
            displayed.distance(xf.w2s(cursor)) < 0.001,
            "held grip jumped away from the pointer at {distance}: {displayed:?}"
        );
        assert!((h.app.node_grip_amount(node, None) - (18.0 - distance)).abs() < 0.001);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
    assert!(h.app.cancel_node_drag());
    assert!(h.app.board_drag.is_none());
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert_eq!(h.app.fillet_grip_at(&before, &xf), Some(idle));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
