//! Brush and pen marks, softness, and the straight shift line.

use super::*;

#[test]
fn brush_hud_scrubs_size_and_softness_and_escape_restores() {
    let mut h = Harness::new("brush_hud");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.alt_down = true;
    h.app.brush_width = 10.0;
    h.app.brush_softness = 0.0;
    h.app.tab_mut().cam.z = 1.0;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(40.0, -50.0)), true, false));
    assert!((h.app.brush_width - 90.0).abs() < 0.01);
    assert!((h.app.brush_softness - 0.5).abs() < 0.01);
    h.app.cancel_brush_hud();
    assert!((h.app.brush_width - 10.0).abs() < 0.01);
    assert!(h.app.brush_softness.abs() < 0.01);
    assert!(h.app.brush_hud.is_none());

    h.app.alt_down = false;
    h.app.ctrl_down = true;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(20.0, 20.0)), true, true));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Wheel { .. })
    ));
    h.app.cancel_brush_hud();
}

#[test]
fn brush_size_hud_opens_when_alt_is_already_held() {
    let mut h = Harness::new("brush_hold");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.alt_down = true;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(4.0, 4.0)), true, false));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Size { .. })
    ));
    h.app.cancel_brush_hud();
}

#[test]
fn brush_stroke_keeps_softness_and_remembers_its_color() {
    let mut h = Harness::new("brush_soft");
    h.app.brush_softness = 0.5;
    h.app.brush_width = 8.0;
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 0.0),
        Pos2::new(30.0, 12.0),
        Pos2::new(60.0, 0.0),
    ]);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("brush commits a path");
    };
    assert!((shape.stroke.softness - 0.5).abs() < 1e-4);
    assert!(shape.stroke.paints_as_stamp());
    assert!(shape.stroke.stamp);
    let rgb = [
        shape.stroke.color.0[0],
        shape.stroke.color.0[1],
        shape.stroke.color.0[2],
    ];
    assert_eq!(
        h.app
            .doc()
            .view
            .recent_colors
            .as_ref()
            .unwrap()
            .first()
            .copied(),
        Some(rgb)
    );
}

#[test]
fn a_brush_click_commits_one_round_dab() {
    let mut h = Harness::new("brush_dab");
    h.app.finish_freehand_brush(vec![Pos2::new(12.0, 8.0)]);
    let dab = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &dab.kind else {
        panic!("a click commits a dab");
    };
    assert!(shape.path.as_ref().is_some_and(|p| p.segs.is_empty()));
    assert!(shape.stroke.stamp);
}

/// Pen, line, arc, polyline, and Bézier strokes are hard vector strokes: a
/// soft, blurred brush stroke that became the last edited style must not
/// leak its softness, stamp, or blur into them.
#[test]
fn vector_tools_never_inherit_brush_softness_or_blur() {
    let mut h = line_board("vector_no_soft");
    h.app.brush_softness = 0.5;
    h.app.brush_width = 40.0;
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 0.0),
        Pos2::new(30.0, 12.0),
        Pos2::new(60.0, 0.0),
    ]);
    let brush = h.app.doc().scene.nodes.last().unwrap().id;
    // A Shift chain or an inspector edit patches the brush stroke by itself.
    h.app.patch_nodes(&[brush], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.gaussian_blur = 3.0;
        }
    });
    for (tool, stroke) in committed_vector_strokes(&mut h) {
        assert_eq!(stroke.softness, 0.0, "{tool:?} inherited brush softness");
        assert_eq!(stroke.gaussian_blur, 0.0, "{tool:?} inherited blur");
        assert!(!stroke.stamp, "{tool:?} became a raster stamp");
        assert!(stroke.tween_from.is_none(), "{tool:?} inherited a tween");
        assert!(!stroke.paints_as_stamp(), "{tool:?} paints as a stamp");
        assert!(stroke.width > 0.0, "{tool:?} has no width");
    }
}

/// Stated intent: brush color, size, and blur never reach the Pen. The Pen
/// draws with its own last color and width, and a hard edge.
#[test]
fn pen_keeps_its_own_style_when_the_brush_changes() {
    let mut h = line_board("pen_own_style");
    draw_pen(&mut h, 0.0);
    let (pen, _) = last_stroke(&h);
    restyle(&mut h, pen, 5.0, [10, 200, 30]);

    h.app.board_colors.fg = Rgba([250, 20, 20, 255]);
    h.app.brush_width = 40.0;
    h.app.brush_softness = 0.6;
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 100.0),
        Pos2::new(30.0, 112.0),
        Pos2::new(60.0, 100.0),
    ]);
    let (brush, _) = last_stroke(&h);
    h.app.patch_nodes(&[brush], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.gaussian_blur = 4.0;
        }
    });

    draw_pen(&mut h, 200.0);
    let (_, stroke) = last_stroke(&h);
    assert_eq!(stroke.width, 5.0, "pen width is its own");
    assert_eq!(
        stroke.color,
        Rgba([10, 200, 30, 255]),
        "pen color is its own"
    );
    assert_eq!(stroke.softness, 0.0);
    assert_eq!(stroke.gaussian_blur, 0.0);
    assert!(!stroke.stamp);
}

/// A Pen that has never drawn does not start from the brush color either.
#[test]
fn a_fresh_pen_does_not_take_the_brush_color_or_size() {
    let mut h = line_board("pen_fresh_style");
    h.app.board_colors.fg = Rgba([250, 20, 20, 255]);
    h.app.brush_width = 40.0;
    draw_pen(&mut h, 0.0);
    let (_, stroke) = last_stroke(&h);
    assert_ne!(stroke.color, Rgba([250, 20, 20, 255]));
    assert_ne!(stroke.width, 40.0);
    assert!(stroke.width > 0.0);
}

/// Stated: the chord mid-stroke changes the pen's width from that point on,
/// stored as variable width (per-vertex tips). The scrub itself draws nothing.
#[test]
fn the_width_chord_mid_stroke_widens_the_rest_of_the_pen_stroke() {
    let mut h = line_board("pen_chord_mid");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Pen);
    let mods = egui::Modifiers::NONE;
    let narrow = h.app.stroke_for_tool(slate_doc::StrokeTool::Pen).width;
    h.app.board_drag = h.app.begin_gesture_for_test(Pos2::ZERO, Pos2::ZERO, mods);
    for i in 1..=10 {
        h.app
            .update_gesture_for_test(Pos2::new(i as f32 * 10.0, 0.0), mods);
    }
    width_chord(&mut h, 40.0);
    let count = pen_point_count(&h);
    h.app.update_gesture_for_test(Pos2::new(100.0, 60.0), mods);
    assert_eq!(pen_point_count(&h), count, "the scrub does not draw");
    release_chord(&mut h);
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Pen).width;
    assert!(wide > narrow + 1.0);
    for i in 11..=20 {
        h.app
            .update_gesture_for_test(Pos2::new(i as f32 * 10.0, 0.0), mods);
    }
    h.app
        .end_gesture_for_test(Pos2::new(200.0, 0.0), None, mods);

    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("the pen commits a path");
    };
    let path = shape.path.as_ref().unwrap();
    assert_eq!(path.tips.len(), path.segs.len() + 1, "one tip per vertex");
    assert!((path.tips.first().unwrap().width - narrow).abs() < 1e-3);
    assert!((path.tips.last().unwrap().width - wide).abs() < 1e-3);
    assert!(path.tips.iter().all(|t| t.softness == 0.0));
    assert!((shape.stroke.width - wide).abs() < 1e-3);
    assert!(
        !shape.stroke.paints_as_stamp(),
        "still a hard vector stroke"
    );
    let widths = path.vector_widths(&shape.stroke).expect("varying width");
    assert!((widths[0] - narrow).abs() < 1e-3);

    draw_pen(&mut h, 300.0);
    let (_, plain) = last_stroke(&h);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        unreachable!()
    };
    assert!(shape.path.as_ref().unwrap().tips.is_empty());
    assert_eq!(plain.width, wide, "the next stroke starts at the new width");
}

/// Chosen: the pen's cursor is a hard circle of the pen's own width and
/// color, the same disc the brush shows for its tip.
#[test]
fn the_pen_cursor_is_a_hard_circle_of_its_width() {
    let mut h = line_board("pen_cursor");
    h.app.tab_mut().cam.z = 2.0;
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.set_tool_width(slate_doc::StrokeTool::Pen, 8.0);
    let pen = h.app.stroke_for_tool(slate_doc::StrokeTool::Pen);
    let (r, softness, ink) = h.app.width_cursor_disc().expect("the pen shows a disc");
    assert_eq!(r, 8.0, "radius is half the width, zoomed");
    assert_eq!(softness, 0.0, "the pen is hard");
    assert_eq!(ink, board::rgba32(pen.color));
    h.app.set_board_tool(board::BoardTool::Line);
    assert!(
        h.app.width_cursor_disc().is_none(),
        "the line uses a crosshair"
    );
}

#[test]
fn ctrl_z_reverts_a_brush_size_change_until_another_action() {
    let mut h = Harness::new("brush_undo_size");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 10.0;
    let before = h.app.brush_setting_snapshot();
    h.app.brush_width = 40.0;
    h.app.push_brush_setting_undo(before);
    h.app.board_undo();
    assert!((h.app.brush_width - 10.0).abs() < 1e-3);
    h.app.brush_width = 10.0;
    let before = h.app.brush_setting_snapshot();
    h.app.brush_width = 40.0;
    h.app.push_brush_setting_undo(before);
    h.app
        .finish_freehand_brush(vec![Pos2::new(0.0, 0.0), Pos2::new(30.0, 0.0)]);
    h.app.board_undo();
    assert!((h.app.brush_width - 40.0).abs() < 1e-3);
}

/// tip18: Shift+press, drag through several points, release. Every frame
/// of the drag previews the straight segment; the release commits it.
#[test]
fn brush_shift_drag_previews_and_commits_a_straight_line() {
    let mut h = brush_board("brush_shift_drag");
    let a = Pos2::new(100.0, 100.0);
    let end = Pos2::new(260.0, 180.0);
    press_drag_release_frames(
        &mut h,
        &[a, Pos2::new(140.0, 160.0), Pos2::new(220.0, 90.0), end],
        egui::Modifiers::SHIFT,
        |h| {
            assert!(
                h.app.brush_straight.is_some(),
                "the Shift press owns the drag"
            );
            assert!(
                h.app.brush_live.as_ref().is_some_and(|c| c.showing_line()),
                "the live canvas shows the straight segment"
            );
        },
    );
    assert!(h.app.brush_straight.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "one straight stroke");
    let v = path_vertices(&h.app.doc().scene.nodes[0]);
    assert_eq!(v.len(), 2, "a straight segment: {v:?}");
    // A Shift drag takes 45° steps from its start.
    let end = board_snap::ortho_snap_point(a, end);
    assert!(near_px(v[0], a) && near_px(v[1], end), "{v:?}");
}
