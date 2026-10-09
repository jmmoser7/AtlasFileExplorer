//! Closed-polyline pick and near miss the empty bbox.

use super::*;

/// Closed unfilled polylines pick on the stroke, not the AABB or interior.
#[test]
fn closed_polyline_pick_and_near_ignore_empty_bbox() {
    let mut h = line_board("closed_pick");
    h.app.tab_mut().cam.z = 1.0;
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(80.0, 0.0),
        Pos2::new(0.0, 80.0),
    ];
    let (rect, data) = board_path::points_to_path_data(&pts, true);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Path,
            fill: None,
            stroke: board_path::default_curve_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(data.into()),

            text: None,
        }),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let scene = &h.app.doc().scene;
    assert_eq!(
        board_path::board_pick_node(scene, 40.0, 0.0, 1.0),
        Some(id),
        "base stroke"
    );
    assert!(
        board_path::board_pick_node(scene, 20.0, 20.0, 1.0).is_none(),
        "unfilled interior must not hover-select"
    );
    assert!(
        board_path::board_pick_node(scene, 80.0, 80.0, 1.0).is_none(),
        "empty AABB corner must not hover-select"
    );
    h.app.board_osnap = Default::default();
    h.app.board_osnap.near = true;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    let ghost = Pos2::new(80.0, 80.0);
    assert!(
        board_osnap::pick(
            &h.app.doc().scene,
            ghost,
            8.0,
            h.app.board_osnap,
            &[],
            None,
            slate_doc::WireRouting::Bezier,
        )
        .is_none(),
        "Near must not fire on empty AABB space of a closed polyline"
    );
    h.frame();
}

#[test]
fn closed_polyline_pick_and_near_ignore_points_outside_the_bbox() {
    let mut h = line_board("closed_outside");
    h.app.tab_mut().cam.z = 1.0;
    let pts = [
        Pos2::new(400.0, 300.0),
        Pos2::new(480.0, 300.0),
        Pos2::new(400.0, 380.0),
    ];
    let (rect, data) = board_path::points_to_path_data(&pts, true);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Path,
            fill: None,
            stroke: board_path::default_curve_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(data.into()),

            text: None,
        }),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let scene = &h.app.doc().scene;
    assert_eq!(
        board_path::board_pick_node(scene, 440.0, 300.0, 1.0),
        Some(id)
    );
    for (x, y, why) in [
        (0.0, 0.0, "world origin"),
        (200.0, 150.0, "line toward origin"),
        (200.0, 300.0, "infinite base extension"),
        (500.0, 400.0, "outside AABB"),
    ] {
        assert!(
            board_path::board_pick_node(scene, x, y, 1.0).is_none(),
            "pick at ({x},{y}) {why}"
        );
    }
    h.app.board_osnap = Default::default();
    h.app.board_osnap.end = true;
    h.app.board_osnap.mid = true;
    h.app.board_osnap.center = true;
    h.app.board_osnap.near = true;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    for ghost in [
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 150.0),
        Pos2::new(200.0, 300.0),
        Pos2::new(500.0, 400.0),
    ] {
        assert!(
            board_osnap::pick(
                &h.app.doc().scene,
                ghost,
                8.0,
                h.app.board_osnap,
                &[],
                None,
                slate_doc::WireRouting::Bezier,
            )
            .is_none(),
            "snap at {ghost:?} must not bind to a far-away polyline"
        );
    }
    h.frame();
}
