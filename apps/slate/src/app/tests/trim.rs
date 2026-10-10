//! Trim golden-path tests.

use super::*;

/// GP1 — two crossing lines, preselect, click one half of the horizontal.
#[test]
fn trim_gp1_crossing_lines() {
    let mut h = trim_board("trim_gp1");
    let horiz = add_seg(&mut h.app, Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0));
    let vert = add_seg(&mut h.app, Pos2::new(50.0, 0.0), Pos2::new(50.0, 100.0));
    select_trim(&mut h.app, &[horiz, vert]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert_eq!(h.app.board_tool, board::BoardTool::Trim);
    assert!(h.app.trim.as_ref().is_some_and(|s| s.cutters.len() == 2));
    assert!(h.app.trim_click(Pos2::new(25.0, 50.0), false));
    let n = h.app.doc().scene.node(horiz).expect("horizontal remains");
    match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            assert_eq!(s.shape, slate_doc::scene::ShapeKind::Path);
            let path = s.path.as_ref().unwrap();
            assert!(!path.closed);
            // Remaining half should live on the right of the crossing.
            assert!(n.rect.x + n.rect.w > 50.0);
            assert!(n.rect.x >= 49.0);
        }
        _ => panic!("expected a path"),
    }
    h.app.board_undo();
    let restored = h.app.doc().scene.node(horiz).unwrap();
    assert!(restored.rect.x < 1.0);
    h.frame();
}

/// GP2 — rect cut by a vertical line; click the left half.
#[test]
fn trim_gp2_rect_cut_by_line() {
    let mut h = trim_board("trim_gp2");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let line = add_seg(&mut h.app, Pos2::new(50.0, -10.0), Pos2::new(50.0, 90.0));
    select_trim(&mut h.app, &[rect, line]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(20.0, 40.0), false));
    let n = h.app.doc().scene.node(rect).expect("rect remains");
    match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            assert_eq!(s.shape, slate_doc::scene::ShapeKind::Path);
            assert!(s.path.as_ref().is_some_and(|p| p.closed));
        }
        _ => panic!("expected a path"),
    }
    assert!(
        n.rect.x >= 49.0,
        "left half should be gone, rect={:?}",
        n.rect
    );
    assert_axis_aligned_world(&node_world_contours(&h.app, rect));
    h.frame();
}

/// GP3 — circle inside a rect; click the circle punches a hole.
#[test]
fn trim_gp3_hole_punch() {
    let mut h = trim_board("trim_gp3");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    let circle = add_filled_ellipse(&mut h.app, 30.0, 30.0, 40.0, 40.0);
    select_trim(&mut h.app, &[rect, circle]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(50.0, 50.0), false));
    let n = h.app.doc().scene.node(rect).expect("rect remains");
    match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            let path = s.path.as_ref().expect("rewritten path");
            assert!(
                !path.extra.is_empty()
                    || matches!(path.fill_rule, slate_doc::scene::PathFillRule::EvenOdd),
                "hole punch must be a compound even-odd path"
            );
            let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
            let contours = vector_ink::flatten_contours(&bez, 0.35);
            assert!(
                vector_ink::point_in_polygon(&contours, [5.0, 5.0]),
                "rect corner must remain"
            );
            assert!(
                !vector_ink::point_in_polygon(&contours, [50.0, 50.0]),
                "circle centre must be a hole"
            );
        }
        _ => panic!("expected a path"),
    }
    h.frame();
}

/// GP7 — a closed cutter removes a corner without changing the source stroke.
#[test]
fn trim_gp7_notch_preserves_stroke_and_undo() {
    use slate_doc::scene::{NodeKind, Rgba, ShapeKind, StrokeJoin};
    let mut h = trim_board("trim_gp7");
    let rect = add_filled_rect(&mut h.app, 40.0, 30.0, 920.0, 380.0);
    h.app.patch_nodes(&[rect], |node| {
        if let NodeKind::Shape(shape) = &mut node.kind {
            shape.stroke.width = 8.0;
            shape.stroke.color = Rgba([45, 212, 191, 255]);
            shape.stroke.join = StrokeJoin::Miter;
        }
    });
    let before = h.app.doc().scene.node(rect).unwrap().clone();
    let cutter = add_filled_rect(&mut h.app, 835.0, 0.0, 165.0, 200.0);
    select_trim(&mut h.app, &[cutter]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(900.0, 100.0), false));
    let after = h.app.doc().scene.node(rect).unwrap();
    let (NodeKind::Shape(original), NodeKind::Shape(trimmed)) = (&before.kind, &after.kind) else {
        panic!("trim must preserve a shape");
    };
    assert_eq!(trimmed.shape, ShapeKind::Path);
    assert_eq!(trimmed.stroke, original.stroke);
    assert_eq!(trimmed.fill, original.fill);
    let bez = board_path::path_data_to_world_bez(
        trimmed.path.as_ref().unwrap(),
        after.rect,
        after.rotation_deg,
    );
    let mesh = vector_ink::stroke_mesh(
        &bez,
        &board_path::stroke_style_world(&trimmed.stroke, 1.0),
        0.0,
        0.02,
    );
    let vertices: Vec<_> = mesh.vertices.iter().map(|v| v.pos).collect();
    for p in [
        [750.0, 26.2],
        [831.2, 100.0],
        [900.0, 196.2],
        [963.8, 350.0],
        [100.0, 413.8],
        [36.2, 100.0],
    ] {
        assert!(
            vector_ink::point_in_mesh(&vertices, &mesh.indices, p),
            "stroke missing at {p:?}"
        );
    }
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(rect).unwrap(), &before);
}

/// GP8 — both source and cutter keep their true corners, including after rotation.
#[test]
fn trim_gp8_true_corners_and_rotation() {
    use slate_doc::scene::{Corner, NodeKind, ShapeKind, WorldRect};
    for corner in [
        Corner::Rounded { radius: 15.0 },
        Corner::RoundedPercent { percent: 30.0 },
        Corner::Chamfer { cut: 15.0 },
        Corner::ChamferPercent { percent: 30.0 },
    ] {
        for angle in [0.0, 33.0, 90.0] {
            let mut h = trim_board("trim_gp8");
            let target_rect = WorldRect::new(0.0, 0.0, 200.0, 100.0);
            let target = add_filled_rect(&mut h.app, 0.0, 0.0, 200.0, 100.0);
            // Rotate the arrangement as a whole; each node still uses its own center.
            let cutter_center = target_rect.rotate_point([180.0, 10.0], angle);
            let cutter = add_filled_rect(
                &mut h.app,
                cutter_center[0] - 60.0,
                cutter_center[1] - 50.0,
                120.0,
                100.0,
            );
            h.app.patch_nodes(&[target, cutter], |node| {
                node.rotation_deg = angle;
                if let NodeKind::Shape(shape) = &mut node.kind {
                    shape.corner = corner;
                }
            });
            let before = h.app.doc().scene.node(target).unwrap().clone();
            let cutter_before = h.app.doc().scene.node(cutter).unwrap().clone();
            select_trim(&mut h.app, &[cutter]);
            h.app.set_board_tool(board::BoardTool::Trim);
            let click = target_rect.rotate_point([160.0, 30.0], angle);
            assert!(h.app.trim_click(Pos2::new(click[0], click[1]), false));
            let after = h.app.doc().scene.node(target).unwrap().clone();
            assert_eq!(
                after.rotation_deg, 0.0,
                "world-space result must not rotate twice"
            );
            let NodeKind::Shape(shape) = &after.kind else {
                panic!("shape");
            };
            assert_eq!(shape.shape, ShapeKind::Path);
            let result = h.app.node_closed_poly(&after).unwrap();
            for (point, inside) in [
                ([1.0, 1.0], false),    // untouched upper-left corner stays rounded/chamfered
                ([199.0, 99.0], false), // untouched lower-right corner
                ([15.0, 10.0], true),
                ([121.0, 59.0], true), // material outside the cutter's rounded/chamfered corner
                ([135.0, 55.0], false), // removed overlap
                ([160.0, 30.0], false),
                ([180.0, 80.0], true),
            ] {
                let world = target_rect.rotate_point(point, angle);
                assert_eq!(
                    vector_ink::point_in_polygon(&result, world),
                    inside,
                    "{corner:?}, rotation {angle}, local point {point:?}"
                );
            }
            assert_eq!(h.app.doc().scene.node(cutter).unwrap(), &cutter_before);
            let saved = serde_json::to_string(&after).unwrap();
            assert_eq!(
                serde_json::from_str::<slate_doc::Node>(&saved).unwrap(),
                after
            );
            h.app.board_undo();
            assert_eq!(h.app.doc().scene.node(target).unwrap(), &before);
        }
    }
}

/// GP9 — rotated legacy lines use their painted endpoints and bake rotation once.
#[test]
fn trim_gp9_rotated_line_endpoints() {
    use slate_doc::scene::{NodeKind, ShapeKind};
    let mut h = trim_board("trim_gp9");
    let target = add_filled_rect(&mut h.app, 0.0, 50.0, 100.0, 0.0);
    h.app.patch_nodes(&[target], |node| {
        node.rotation_deg = 45.0;
        if let NodeKind::Shape(shape) = &mut node.kind {
            shape.shape = ShapeKind::Line;
            shape.fill = None;
        }
    });
    let before = h.app.doc().scene.node(target).unwrap().clone();
    let cutter = add_seg(&mut h.app, Pos2::new(50.0, -20.0), Pos2::new(50.0, 120.0));
    select_trim(&mut h.app, &[cutter]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(30.0, 30.0), false));
    let after = h.app.doc().scene.node(target).unwrap();
    let points = h.app.node_open_polyline(after).unwrap();
    assert_eq!(after.rotation_deg, 0.0);
    assert!((points[0][0] - 50.0).abs() < 0.01 && (points[0][1] - 50.0).abs() < 0.01);
    let last = points.last().unwrap();
    assert!((last[0] - 85.35534).abs() < 0.01 && (last[1] - 85.35534).abs() < 0.01);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(target).unwrap(), &before);
}

/// GP4 — Ctrl+T arms Trim; Ctrl+N still opens a tab.
#[test]
fn trim_gp4_ctrl_n_still_new_tab() {
    let mut h = trim_board("trim_gp4");
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.tool.trim"), None);
    assert_eq!(h.app.board_tool, board::BoardTool::Trim);
    let before = h.app.tabs.len();
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.new_tab"), None);
    assert_eq!(h.app.tabs.len(), before + 1);
    h.frame();
}

/// GP5 — two clicks, one undo restores only the last.
#[test]
fn trim_gp5_per_click_undo() {
    let mut h = trim_board("trim_gp5");
    let a = add_seg(&mut h.app, Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0));
    let c1 = add_seg(&mut h.app, Pos2::new(30.0, -10.0), Pos2::new(30.0, 10.0));
    let c2 = add_seg(&mut h.app, Pos2::new(70.0, -10.0), Pos2::new(70.0, 10.0));
    select_trim(&mut h.app, &[a, c1, c2]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(15.0, 0.0), false));
    let after_first = h.app.doc().scene.node(a).unwrap().rect;
    assert!(h.app.trim_click(Pos2::new(85.0, 0.0), false));
    let after_second = h.app.doc().scene.node(a).map(|n| n.rect);
    h.app.board_undo();
    let undone = h.app.doc().scene.node(a).unwrap().rect;
    assert_eq!(undone, after_first, "one undo peels one click");
    let _ = after_second;
    h.frame();
}

/// GP6 — Esc peels TrimParts → PickCutters → Select.
#[test]
fn trim_gp6_esc_stack() {
    let mut h = trim_board("trim_gp6");
    let cutter = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::super::board_trim::TrimPhase::PickCutters
    );
    assert!(h.app.trim_click(Pos2::new(20.0, 20.0), false));
    assert!(h.app.trim.as_ref().unwrap().cutters.contains(&cutter));
    h.app.trim_enter();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::super::board_trim::TrimPhase::TrimParts
    );
    h.app.trim_cancel_step();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::super::board_trim::TrimPhase::PickCutters
    );
    h.app.trim_cancel_step();
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.trim.is_none());
    h.frame();
}

/// Text + circle: the circle chops a hole in the text's clip (D11).
#[test]
fn trim_text_clip_punches_a_hole() {
    use slate_doc::scene::{TextAlign, TextNode, Typeface};
    let mut h = trim_board("trim_text_clip");
    let text = {
        let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 40.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Text(TextNode {
                text: "HELLO".into(),
                family: Typeface::Sans,
                size: 24.0,
                color: slate_doc::scene::Rgba::BLACK,
                align: TextAlign::Left,
                fill: None,
                stroke: Default::default(),
                agent: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    let circle = add_filled_ellipse(&mut h.app, 30.0, 0.0, 40.0, 40.0);
    select_trim(&mut h.app, &[text, circle]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(50.0, 20.0), false));
    let n = h.app.doc().scene.node(text).expect("text remains");
    let clip = n.clip.as_ref().expect("text clip after punch");
    let bez = board_path::path_data_to_world_bez(clip, n.rect, n.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, 0.35);
    assert!(vector_ink::point_in_polygon(&contours, [5.0, 20.0]));
    assert!(!vector_ink::point_in_polygon(&contours, [50.0, 20.0]));
    h.frame();

    // Re-run the punch with a rotated host: clip coordinates stay host-local.
    h.app.board_undo();
    h.app.patch_nodes(&[text], |node| node.rotation_deg = 37.0);
    let before = h.app.doc().scene.node(text).unwrap().clone();
    select_trim(&mut h.app, &[circle]);
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(50.0, 20.0), false));
    let n = h.app.doc().scene.node(text).unwrap();
    assert_eq!(n.rotation_deg, 37.0);
    assert_eq!(n.rect, before.rect);
    let contours = h.app.node_closed_poly(n).unwrap();
    let kept = n.rect.rotate_point([5.0, 20.0], n.rotation_deg);
    assert!(vector_ink::point_in_polygon(&contours, kept));
    assert!(!vector_ink::point_in_polygon(&contours, [50.0, 20.0]));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(text).unwrap(), &before);
}

/// A curved open Bézier cutter divides a rectangle along its whole path;
/// Ctrl+T and a click below the arch remove that piece. One undo restores
/// the rectangle exactly; redo repeats the trim.
#[test]
fn trim_curved_bezier_cutter_removes_the_clicked_side_of_a_rect() {
    let mut h = grip_board("trim_open_closed_bezier");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let arch = add_bezier_arch(&mut h.app);
    let cutter = h
        .app
        .node_open_polyline(h.app.doc().scene.node(arch).unwrap());
    assert!(
        cutter.is_some_and(|pts| pts.len() > 2),
        "the cutter is curved"
    );
    let before = h.app.doc().scene.node(rect).unwrap().clone();
    arm_slice_by_key(&mut h, &[arch], false);
    click_at(&mut h, Pos2::new(50.0, 60.0), egui::Modifiers::NONE);

    let after = h
        .app
        .doc()
        .scene
        .node(rect)
        .expect("the cap remains")
        .clone();
    let s = shape_of(&h.app, rect);
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Path);
    assert!(s.path.as_ref().is_some_and(|p| p.closed));
    assert_eq!(s.fill, Some(slate_doc::scene::Rgba::WHITE), "fill kept");
    let poly = h.app.node_closed_poly(&after).unwrap();
    assert!(vector_ink::point_in_polygon(&poly, [50.0, 5.0]));
    assert!(!vector_ink::point_in_polygon(&poly, [50.0, 60.0]));
    assert!(!vector_ink::point_in_polygon(&poly, [5.0, 60.0]));
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        2,
        "one piece plus the cutter"
    );

    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(h.app.doc().scene.node(rect).unwrap(), &before);
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(h.app.doc().scene.node(rect).unwrap(), &after);
}

/// A three-segment Polyline cutter crossing an ellipse twice cuts off the
/// cap it encloses; the rest of the ellipse stays filled.
#[test]
fn trim_polyline_cutter_crossing_an_ellipse_twice() {
    let mut h = grip_board("trim_open_closed_ellipse");
    let ellipse = add_filled_ellipse(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    let before = h.app.doc().scene.node(ellipse).unwrap().clone();
    let u = commit_polyline(
        &mut h,
        &[
            Pos2::new(30.0, -10.0),
            Pos2::new(30.0, 40.0),
            Pos2::new(70.0, 40.0),
            Pos2::new(70.0, -10.0),
        ],
        false,
    );
    arm_slice_by_key(&mut h, &[u], false);
    click_at(&mut h, Pos2::new(50.0, 20.0), egui::Modifiers::NONE);

    let after = h.app.doc().scene.node(ellipse).unwrap().clone();
    let s = shape_of(&h.app, ellipse);
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Path);
    assert_eq!(s.fill, Some(slate_doc::scene::Rgba::WHITE));
    let poly = h.app.node_closed_poly(&after).unwrap();
    assert!(
        !vector_ink::point_in_polygon(&poly, [50.0, 20.0]),
        "cap gone"
    );
    for keep in [[20.0, 20.0], [80.0, 20.0], [50.0, 80.0], [50.0, 45.0]] {
        assert!(vector_ink::point_in_polygon(&poly, keep), "{keep:?} stays");
    }
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(ellipse).unwrap(), &before);
}
