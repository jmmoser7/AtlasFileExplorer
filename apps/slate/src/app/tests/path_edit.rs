//! Path edit, eraser undo, and connector tests from the keymap wave.

use super::*;

/// Eraser: three touched strokes are removed as one journal group — one
/// undo restores all of them.
#[test]
fn eraser_release_is_one_undo_group() {
    let mut h = Harness::new("eraser");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let ids: Vec<NodeId> = (0..3)
        .map(|i| add_stroke(&mut h.app, 0.0, i as f32 * 50.0))
        .collect();

    // The eraser circle over the middle of the first stroke hits it.
    let hits = h.app.eraser_hits_at(Pos2::new(50.0, 0.5));
    assert_eq!(hits, vec![ids[0]]);

    h.app
        .finish_erase(ids.clone(), vec![Pos2::new(50.0, 0.5)], Vec::new());
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 3, "one undo restores all");
    h.frame();
}

#[test]
fn smooth_pass_increases_stamp_blur_and_undo_restores() {
    let mut h = Harness::new("smooth_blur");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(10.0, 10.0), Pos2::new(80.0, 40.0)]);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Smooth);
    h.app.smooth_width = 40.0;
    h.app.smooth_strength = 1.0;
    let drag = h.app.begin_smooth(Pos2::new(40.0, 25.0), false);
    h.app.finish_smooth(drag);
    let blur_after = h.app.doc().scene.node(id).unwrap();
    let slate_doc::scene::NodeKind::Shape(after) = &blur_after.kind else {
        panic!("shape");
    };
    assert!(after.stroke.gaussian_blur > 0.0, "blur should increase");
    h.app.board_undo();
    let blur_before = h.app.doc().scene.node(id).unwrap();
    let slate_doc::scene::NodeKind::Shape(before) = &blur_before.kind else {
        panic!("shape");
    };
    assert_eq!(before.stroke.gaussian_blur, 0.0);
    h.frame();
}

/// Hidden and locked semantics: hit-testing, select-all, and the escape
/// hatches (show all / unlock all / force pick).
#[test]
fn hidden_and_locked_leave_selection_paths() {
    let mut h = Harness::new("flags");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 200.0, 0.0);

    h.app.board_sel = [a].into_iter().collect();
    assert_eq!(h.app.cmd_hide_selection(), 1);
    assert!(h.app.board_sel.is_empty(), "hide clears the selection");
    assert!(board_path::board_pick_node(&h.app.doc().scene, 40.0, 30.0, 1.0).is_none());

    h.app.board_sel = [b].into_iter().collect();
    assert_eq!(h.app.cmd_lock_selection(), 1);
    assert!(board_path::board_pick_node(&h.app.doc().scene, 240.0, 30.0, 1.0).is_none());
    // The Ctrl+Shift+click escape hatch still reaches it.
    assert_eq!(
        board_path::board_pick_node_ex(&h.app.doc().scene, 240.0, 30.0, 1.0, true),
        Some(b)
    );

    assert_eq!(h.app.hidden_locked_counts(), (1, 1));
    assert_eq!(h.app.cmd_show_all_hidden(), 1);
    assert_eq!(h.app.cmd_unlock_all(), 1);
    assert_eq!(h.app.hidden_locked_counts(), (0, 0));
    // Both journaled: two undos restore the flags.
    h.app.board_undo();
    h.app.board_undo();
    assert_eq!(h.app.hidden_locked_counts(), (1, 1));
    h.frame();
}

/// Deleting a node degrades wires anchored to it to Free ends in the same
/// undo group; undo restores the anchor.
#[test]
fn delete_degrades_connector_ends_to_free() {
    use slate_doc::scene::{ConnectorEnd, NodeKind, Side};
    let mut h = Harness::new("wire_degrade");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 300.0, 0.0);
    let wire = h
        .app
        .add_connector(
            ConnectorEnd::Anchored {
                node: a,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: b,
                side: Side::Left,
                t: 0.5,
            },
        )
        .expect("wire added");

    h.app.delete_board_nodes(&[b]);
    let conn = match &h.app.doc().scene.node(wire).unwrap().kind {
        NodeKind::Connector(c) => c.clone(),
        _ => panic!("connector"),
    };
    assert!(matches!(conn.a, ConnectorEnd::Anchored { node, .. } if node == a));
    match conn.b {
        ConnectorEnd::Free { point } => assert_eq!(point, [300.0, 30.0]),
        other => panic!("must degrade to Free, got {other:?}"),
    }

    h.app.board_undo();
    let conn = match &h.app.doc().scene.node(wire).unwrap().kind {
        NodeKind::Connector(c) => c.clone(),
        _ => panic!("connector"),
    };
    assert!(matches!(conn.b, ConnectorEnd::Anchored { node, .. } if node == b));
    h.frame();
}

/// Ctrl+J over two open paths joins nearest endpoints into one node that
/// keeps the first path's style — one Remove+Add group (one undo).
#[test]
pub(super) fn join_two_open_paths_keeps_first_style() {
    use slate_doc::scene::{NodeKind, ShapeKind};
    let mut h = Harness::new("join");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_stroke(&mut h.app, 0.0, 0.0);
    let b = add_stroke(&mut h.app, 150.0, 0.0);
    h.app.board_sel = [a, b].into_iter().collect();

    assert!(h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let joined = &h.app.doc().scene.nodes[0];
    match &joined.kind {
        NodeKind::Shape(s) => {
            assert_eq!(s.shape, ShapeKind::Path);
            let p = s.path.as_ref().unwrap();
            assert!(!p.closed);
            assert_eq!(p.point_count(), 4, "two 2-anchor paths bridged");
        }
        _ => panic!("joined node must be a path shape"),
    }
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2, "one undo splits back");
    h.frame();
}

/// Review r19 R1: a rotated polyline whose miter spike alone reaches into
/// view, past half its width beyond its rotated box, still paints it.
#[test]
fn a_rotated_path_miter_spike_at_the_view_edge_still_paints() {
    use slate_doc::scene::{PathData, PathSeg, ShapeKind, ShapeNode, StrokeCap};
    let mut h = line_board("rotated_miter_spike");
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |i| i.max_texture_side = Some(8192));
    let mut stroke = board_path::default_curve_stroke(slate_doc::scene::Rgba([255, 40, 40, 255]));
    stroke.width = 60.0;
    stroke.cap = StrokeCap::Butt;
    let rect = WorldRect::new(0.0, 0.0, 229.4, 400.0);
    let mut node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke,
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.0, 0.0],
                segs: vec![
                    PathSeg::Line { to: [0.5, 1.0] },
                    PathSeg::Line { to: [1.0, 0.0] },
                ],
                closed: false,
                ..Default::default()
            })),
            text: None,
        }),
    );
    node.rotation_deg = 1.0;
    let id = h.app.add_nodes(vec![node])[0];
    let node = h.app.doc().scene.node(id).unwrap().clone();
    // The arms meet at about 32 degrees, so the miter reaches about 109
    // units past the vertex at (114.7, 400); 80 units past it is on the spike.
    let (cx, cy) = rect.center();
    let (sin, cos) = 1.0_f32.to_radians().sin_cos();
    let (lx, ly) = (114.7 - cx, 480.0 - cy);
    let spike = Pos2::new(cx + lx * cos - ly * sin, cy + lx * sin + ly * cos);
    h.app.tab_mut().cam.z = 4.0;
    let half = h.app.canvas_rect.size() * (0.5 / 4.0);
    h.app.tab_mut().cam.offset = EVec2::new(spike.x, 460.0 + half.y);
    let xf = h.app.board_xf();
    assert!(
        h.app.canvas_rect.shrink(4.0).contains(xf.w2s(spike)),
        "the spike is off screen"
    );
    let view = h.app.board_paint_view(h.app.canvas_rect);
    let boxed = node.rect.rotated_bounds(node.rotation_deg);
    assert!(
        boxed.y + boxed.h + 30.0 < view.y,
        "the rotated box's half-width pad reaches the view"
    );
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    let r = redness(&raster, &h.app.board_xf(), spike);
    assert!(
        r > 0.5,
        "the rotated polyline's miter spike is blank in view ({r:.2})"
    );
}
