//! Cardinal rotation and outline picking regressions.
use super::*;
use slate_doc::scene::ShapeKind;

#[test]
fn polygon_shift_rotation_snaps_the_gripped_vertex_for_every_side_count_and_phase() {
    let (mut h, id, _) = closed_form_board("polygon_cardinal", ShapeKind::RegularPolygon);
    for sides in 3..=12 {
        for phase in [0.0, 37.0, 90.0] {
            h.app.patch_nodes(&[id], |n| {
                if let NodeKind::Shape(s) = &mut n.kind {
                    s.sides = sides;
                    s.phase_deg = phase;
                }
                n.rotation_deg = 27.0;
            });
            let n = h.app.doc().scene.node(id).unwrap();
            let center = Pos2::from(n.rect.center());
            for i in 0..sides {
                let local = Pos2::from(
                    slate_doc::scene::regular_polygon_vertex(n.rect, sides, phase, i as usize)
                        .unwrap(),
                );
                let start =
                    board_snap::orbit_point(n.rect.center(), (local.x, local.y), n.rotation_deg);
                let start_angle = (start.1 - center.y).atan2(start.0 - center.x);
                for turn in [-137.0_f32, -48.0, 21.0, 83.0, 173.0] {
                    let world = Pos2::from(board_snap::orbit_point(n.rect.center(), start, turn));
                    let rotation = SlateApp::rotation_drag_value(
                        n,
                        start_angle,
                        i,
                        world,
                        egui::Modifiers::SHIFT,
                    );
                    let point = Pos2::from(board_snap::orbit_point(
                        n.rect.center(),
                        (local.x, local.y),
                        rotation,
                    ));
                    let ray = point - center;
                    assert!(
                        ray.x.abs().min(ray.y.abs()) < 0.002,
                        "sides={sides}, phase={phase}, vertex={i}, turn={turn}, ray={ray:?}"
                    );
                    let free = SlateApp::rotation_drag_value(
                        n,
                        start_angle,
                        i,
                        world,
                        egui::Modifiers::SHIFT.plus(egui::Modifiers::ALT),
                    );
                    assert!(
                        ((free - n.rotation_deg - turn + 180.0).rem_euclid(360.0) - 180.0).abs()
                            < 0.002
                    );
                }
            }
        }
    }
}

#[test]
fn polygon_shift_rotation_through_input_keeps_center_and_one_undo_step() {
    let (mut h, id, _) = closed_form_board("polygon_cardinal_input", ShapeKind::RegularPolygon);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.sides = 4;
            s.phase_deg = 17.0;
        }
        n.rotation_deg = 23.0;
    });
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let center = Pos2::from(before.rect.center());
    let vertex = h.app.closed_form_points(id).unwrap()[2];
    let outward = (vertex - center).normalized();
    let start = vertex + outward * 20.0 + outward.rot90() * 12.0;
    assert_eq!(
        h.app.transform_hit_at(h.app.board_xf().w2s(start)),
        Some((Some(id), board_handles::BoardHitTarget::Rotate(2)))
    );
    let end = Pos2::from(board_snap::orbit_point(
        before.rect.center(),
        (start.x, start.y),
        53.0,
    ));
    let depth = h.app.tab().journal.undo_depth();
    grip_drag(&mut h, start, end, egui::Modifiers::SHIFT);
    let after = h.app.doc().scene.node(id).unwrap();
    let ray = h.app.closed_form_points(id).unwrap()[2] - center;
    assert!(ray.x.abs().min(ray.y.abs()) < 0.002, "{ray:?}");
    assert_eq!(after.rect, before.rect);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rotation_deg,
        before.rotation_deg
    );
}

#[test]
fn polygon_rotation_halo_cannot_start_a_wire_from_an_enlarged_port_target() {
    let (mut h, id, _) =
        closed_form_board("polygon_rotate_wire_overlap", ShapeKind::RegularPolygon);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.sides = 4;
            s.phase_deg = 17.0;
        }
        n.rotation_deg = 23.0;
    });
    h.frame();
    let xf = h.app.board_xf();
    let vertex = h.app.closed_form_points(id).unwrap()[0];
    let start = vertex + egui::vec2(-3.0, -21.0);
    let screen = xf.w2s(start);
    assert_eq!(
        h.app.transform_hit_at(screen),
        Some((Some(id), board_handles::BoardHitTarget::Rotate(0)))
    );
    assert_eq!(h.app.wire_grip_at(screen, &xf), None);
    assert!(matches!(
        h.app.begin_gesture(screen, start, egui::Modifiers::NONE),
        Some(board::BoardDrag::Rotate { vertex: 0, .. })
    ));
}

#[test]
fn polygon_void_does_not_pick_or_offer_invisible_box_resize_even_with_vertex_styles() {
    let (mut h, id, _) = closed_form_board("polygon_outline_pick", ShapeKind::RegularPolygon);
    for styled in [false, true] {
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Shape(s) = &mut n.kind {
                s.sides = 3;
                s.phase_deg = 0.0;
                if styled {
                    s.path = Some(
                        slate_doc::scene::PathData {
                            tips: vec![slate_doc::scene::StrokeSpan::of(&s.stroke); 3],
                            ..Default::default()
                        }
                        .into(),
                    );
                }
            }
            n.rotation_deg = 37.0;
        });
        h.frame();
        let n = h.app.doc().scene.node(id).unwrap();
        let center = n.rect.center();
        let void = Pos2::from(board_snap::orbit_point(
            center,
            (n.rect.x + 2.0, n.rect.y + 2.0),
            n.rotation_deg,
        ));
        assert_eq!(
            board_path::board_pick_node(&h.app.doc().scene, void.x, void.y, 1.0),
            None
        );
        assert_eq!(h.app.transform_hit_at(h.app.board_xf().w2s(void)), None);
        assert_eq!(
            board_path::board_pick_node(&h.app.doc().scene, center.0, center.1, 1.0),
            Some(id)
        );
        let screen = h.app.board_xf().w2s(void);
        press_primary(&mut h, screen, egui::Modifiers::NONE);
        assert!(h.app.board_sel.is_empty());
        h.app.board_sel.insert(id);
    }
}

#[test]
fn polygon_pick_respects_the_cut_at_a_styled_vertex() {
    let (mut h, id, _) = closed_form_board("polygon_cut_pick", ShapeKind::RegularPolygon);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.stroke = slate_doc::scene::Stroke::none();
            s.corner = slate_doc::scene::Corner::Chamfer { cut: 0.0 };
            s.path = Some(
                slate_doc::scene::PathData {
                    corner_amounts: vec![
                        Some(60.0),
                        Some(0.0),
                        Some(0.0),
                        Some(0.0),
                        Some(0.0),
                        Some(0.0),
                    ],
                    ..Default::default()
                }
                .into(),
            );
        }
    });
    let n = h.app.doc().scene.node(id).unwrap();
    let center = Pos2::from(n.rect.center());
    let points = h.app.closed_form_points(id).unwrap();
    let cut = points[0].lerp(center, 0.05);
    assert_eq!(
        board_path::board_pick_node(&h.app.doc().scene, cut.x, cut.y, 1.0),
        None,
        "the removed corner is empty"
    );
    let intact = points[3].lerp(center, 0.05);
    assert_eq!(
        board_path::board_pick_node(&h.app.doc().scene, intact.x, intact.y, 1.0),
        Some(id)
    );
}
