//! Fillet grip tests.

use super::*;

#[test]
fn fillet_drag_outward_grows_authored_radius() {
    let mut h = Harness::new("fillet_outward");
    h.app.ensure_work_tab();
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0);
    let before = slate_doc::scene::Node {
        id: NodeId(1),
        rect,
        rotation_deg: 0.0,
        hidden: false,
        locked: false,
        opacity: 1.0,
        group: None,
        clip: None,
        bumper: None,
        kind: slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            phase_deg: 0.0,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Rounded { radius: 10.0 },
            flip: false,
            path: None,
            text: None,
        }),
    };
    h.app.doc_mut().scene.nodes.push(before.clone());
    let mut mid = before.clone();
    h.app
        .apply_fillet_radius_from_drag(&mut mid, &before, 20.0, false);
    let (_, r1) = match &mid.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.corner.effective(rect.w, rect.h),
        _ => panic!(),
    };
    let mut out = before.clone();
    h.app
        .apply_fillet_radius_from_drag(&mut out, &before, 35.0, false);
    let (_, r2) = match &out.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.corner.effective(rect.w, rect.h),
        _ => panic!(),
    };
    assert!(r1 > 10.0 && r1 < rect.w.min(rect.h) * 0.5);
    assert!(r2 > r1 && r2 < rect.w.min(rect.h) * 0.5);
}

#[test]
fn fillet_drag_on_default_square_portal_keeps_designed_fillet() {
    let mut h = Harness::new("fillet_portal_default");
    h.app.ensure_work_tab();
    let mut portal = slate_doc::scene::PortalNode::unbound_web("page");
    portal.corner = slate_doc::scene::Corner::Square;
    let before = slate_doc::scene::Node {
        id: NodeId(2),
        rect: slate_doc::scene::WorldRect::new(0.0, 0.0, 320.0, 240.0),
        rotation_deg: 0.0,
        hidden: false,
        locked: false,
        opacity: 1.0,
        group: None,
        clip: None,
        bumper: None,
        kind: slate_doc::scene::NodeKind::Portal(portal),
    };
    h.app.doc_mut().scene.nodes.push(before.clone());
    let mut n = before.clone();
    h.app.apply_fillet_radius_from_drag(
        &mut n,
        &before,
        slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET,
        false,
    );
    assert!(
        !matches!(
            slate_doc::scene::corner_of(&n),
            Some(slate_doc::scene::Corner::Rounded { radius: 0.0 })
        ),
        "default portal must not become Rounded{{0}} when dragged to the designed fillet"
    );
}

#[test]
fn fillet_grip_click_on_default_portal_is_a_noop() {
    let mut h = Harness::new("fillet_portal_click");
    h.app.ensure_work_tab();
    let before = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 320.0, 240.0),
        slate_doc::scene::NodeKind::Portal(slate_doc::scene::PortalNode::unbound_web("page")),
    );
    let id = before.id;
    h.app.doc_mut().scene.nodes.push(before.clone());
    h.app.board_sel.insert(id);
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let grip = h
        .app
        .fillet_grip_at(&before, &xf)
        .expect("default portal grip");
    let world = xf.s2w(grip);
    h.app.board_drag = h.app.begin_fillet_drag(grip, world);
    h.app.update_gesture_for_test(world, egui::Modifiers::NONE);
    h.app
        .end_gesture_for_test(world, Some(grip), egui::Modifiers::NONE);
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert!(
        !h.app.tab().journal.can_undo(),
        "a press/release without motion must not add a Patch"
    );
}

#[test]
fn fillet_grip_drag_from_square_starts_at_zero_radius() {
    let mut h = Harness::new("fillet_square_drag");
    h.app.ensure_work_tab();
    let before = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0),
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            phase_deg: 0.0,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let id = before.id;
    h.app.doc_mut().scene.nodes.push(before.clone());
    h.app.board_sel.insert(id);
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let grip = h.app.fillet_grip_at(&before, &xf).expect("square grip");
    let press = xf.s2w(grip);
    h.app.board_drag = h.app.begin_fillet_drag(grip, press);
    let moved = Pos2::new(press.x + 3.0, press.y + 3.0);
    h.app.update_gesture_for_test(moved, egui::Modifiers::NONE);
    h.app
        .end_gesture_for_test(moved, Some(xf.w2s(moved)), egui::Modifiers::NONE);
    let after = h.app.doc().scene.node(id).unwrap();
    let radius = slate_doc::scene::resolved_corner_effective(after, None).1;
    assert!((radius - 3.0).abs() < 0.05, "radius={radius}");
}

#[test]
fn fillet_drag_percent_mode_roundtrip() {
    let mut h = Harness::new("fillet_percent");
    h.app.ensure_work_tab();
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0);
    let before = slate_doc::scene::Node {
        id: NodeId(3),
        rect,
        rotation_deg: 0.0,
        hidden: false,
        locked: false,
        opacity: 1.0,
        group: None,
        clip: None,
        bumper: None,
        kind: slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            phase_deg: 0.0,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::RoundedPercent { percent: 50.0 },
            flip: false,
            path: None,
            text: None,
        }),
    };
    h.app.doc_mut().scene.nodes.push(before.clone());
    let mut n = before.clone();
    h.app
        .apply_fillet_radius_from_drag(&mut n, &before, 30.0, false);
    assert!(matches!(
        slate_doc::scene::corner_of(&n),
        Some(slate_doc::scene::Corner::RoundedPercent { .. })
    ));
    let (_, r) = slate_doc::scene::resolved_corner_effective(&n, None);
    assert!((r - 30.0).abs() < 0.01);
}
