//! Width chords taper the stroke being drawn.

use super::*;

/// Stated intent: each curve tool remembers its own width.
#[test]
fn changing_the_line_width_leaves_the_arc_alone() {
    let mut h = line_board("line_arc_style");
    draw_arc(&mut h, 300.0);
    let (_, arc_before) = last_stroke(&h);
    let line = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(50.0, 0.0))
        .unwrap();
    restyle(&mut h, line, 9.0, [1, 2, 3]);
    draw_arc(&mut h, 400.0);
    let (_, arc) = last_stroke(&h);
    assert_eq!(arc.width, arc_before.width, "arc kept its own width");
    assert_eq!(arc.color, arc_before.color, "arc kept its own color");
    h.app.set_board_tool(board::BoardTool::Line);
    h.app
        .commit_line(Pos2::new(0.0, 50.0), Pos2::new(50.0, 50.0))
        .unwrap();
    let (_, line_again) = last_stroke(&h);
    assert_eq!(line_again.width, 9.0, "line remembers its own width");
    assert_eq!(line_again.color, Rgba([1, 2, 3, 255]));
}

/// Stated: the chord during a line draw sets the width of the end being
/// placed, which is the line's widest point here.
#[test]
fn the_width_chord_mid_draw_sets_the_line_being_drawn() {
    let mut h = line_board("line_chord_mid");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Line);
    assert!(h.app.line_begin(Pos2::new(0.0, 0.0), false));
    h.app.line_hover(Pos2::new(80.0, 0.0), false);
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    assert!(h.app.line_draft.is_some(), "the chord keeps the draft");
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width;
    h.app.line_release(Pos2::new(100.0, 0.0), false, false);
    let (_, stroke) = last_stroke(&h);
    assert_eq!(stroke.width, wide);
}

/// Stated: the chord mid-draw sets the width of the arc, polyline, or Bézier
/// point being placed, which is the path's widest point here.
#[test]
fn the_width_chord_mid_draw_sets_the_path_being_drawn() {
    let mut h = line_board("path_chord_mid");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Polyline);
    h.app.path_tool_click(Pos2::new(0.0, 0.0));
    h.app.path_tool_click(Pos2::new(60.0, 30.0));
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    assert!(
        h.app.board_path_draft.is_some(),
        "the chord keeps the draft"
    );
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Polyline).width;
    h.app.path_tool_click(Pos2::new(120.0, 0.0));
    assert!(h.app.finish_path_draft());
    assert_eq!(last_stroke(&h).1.width, wide);
}

/// User request (2026-09-26): while drawing, the width chord sets the width
/// of the point being placed and of the points after it. Points already
/// placed keep theirs, so a polyline tapers straight between them, in the
/// preview and once committed.
#[test]
fn the_width_chord_tapers_a_polyline_between_points() {
    let mut h = taper_board("draw_taper_polyline", board::BoardTool::Polyline);
    let tool = slate_doc::StrokeTool::Polyline;
    let narrow = h.app.stroke_for_tool(tool).width;
    h.app.path_tool_click(Pos2::new(0.0, 0.0));
    h.app.path_tool_click(Pos2::new(100.0, 0.0));
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    let wide = h.app.stroke_for_tool(tool).width;
    assert!(wide > narrow + 30.0);
    let draft = h.app.board_path_draft.as_ref().expect("still drawing");
    let tip = h.app.placed_tip(tool);
    let (_, _, preview) =
        board_path::path_draft_preview(draft, Some(Pos2::new(200.0, 0.0)), tip).unwrap();
    assert_eq!(
        preview.iter().map(|t| t.width).collect::<Vec<_>>(),
        vec![narrow, narrow, wide],
        "the preview tapers to the point being placed"
    );

    h.app.path_tool_click(Pos2::new(200.0, 0.0));
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert_eq!(tip_widths(&h, id), vec![narrow, narrow, wide]);
    let up = EVec2::new(0.0, 1.0);
    let half = |h: &Harness, x: f32| ink_half_width(h, id, Pos2::new(x, 0.0), up);
    assert_close(
        half(&h, 50.0),
        narrow / 2.0,
        0.1,
        "a placed span keeps its width",
    );
    assert_close(
        half(&h, 150.0),
        (narrow + wide) / 4.0,
        0.1,
        "a straight taper",
    );
    assert_eq!(
        h.app.stroke_for_tool(tool).width,
        wide,
        "the next point starts at the chosen width"
    );
}

/// User request (2026-09-26): the Line tool's first point keeps the width it
/// was placed with; the chord sets the end being placed.
#[test]
fn the_width_chord_tapers_a_line_from_its_first_point() {
    let mut h = taper_board("draw_taper_line", board::BoardTool::Line);
    let narrow = h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width;
    assert!(h.app.line_begin(Pos2::new(0.0, 0.0), false));
    h.app.line_hover(Pos2::new(80.0, 0.0), false);
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width;
    h.app.line_release(Pos2::new(100.0, 0.0), false, false);
    let (id, stroke) = last_stroke(&h);
    assert_eq!(tip_widths(&h, id), vec![narrow, wide]);
    assert_eq!(stroke.width, wide);
    let up = EVec2::new(0.0, 1.0);
    let mid = ink_half_width(&h, id, Pos2::new(50.0, 0.0), up);
    assert_close(mid, (narrow + wide) / 4.0, 0.1, "a straight taper");
}

/// User request (2026-09-26): an arc's through point, placed after the
/// chord, takes the new width; its start and end keep theirs.
#[test]
fn the_width_chord_sets_the_arc_point_being_placed() {
    let mut h = taper_board("draw_taper_arc", board::BoardTool::Arc);
    let narrow = h.app.stroke_for_tool(slate_doc::StrokeTool::Arc).width;
    h.app.path_tool_click(Pos2::new(0.0, 0.0));
    h.app.path_tool_click(Pos2::new(200.0, 0.0));
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Arc).width;
    h.app.path_tool_click(Pos2::new(100.0, -100.0));
    let (n, s) = curve_shape(&h, h.app.doc().scene.nodes.last().unwrap().id);
    let grips = slate_doc::vertex_style::grip_tips(
        s.path.as_ref().unwrap(),
        &s.stroke,
        n.rect,
        n.rotation_deg,
    )
    .expect("arc grips");
    let widths: Vec<f32> = grips.iter().map(|t| t.width).collect();
    assert_eq!(widths.len(), 3);
    for (got, want) in widths.iter().zip([narrow, wide, narrow]) {
        assert_close(*got, want, 1e-4, "arc grip width");
    }
    let through = ink_half_width(&h, n.id, Pos2::new(100.0, -100.0), EVec2::new(0.0, 1.0));
    assert_close(through, wide / 2.0, 0.1, "the through point is wide");
}

/// User request (2026-09-26): a Bézier span placed across a chord blends
/// smoothly between its anchors' widths, and taking an anchor back and
/// putting it back while drawing keeps its width.
#[test]
fn the_width_chord_tapers_a_bezier_span_smoothly() {
    let mut h = bezier_board("draw_taper_bezier");
    let narrow = h.app.stroke_for_tool(slate_doc::StrokeTool::Bezier).width;
    bezier_place(&mut h, Pos2::new(0.0, 0.0), Pos2::new(30.0, 0.0));
    bezier_place(&mut h, Pos2::new(100.0, 0.0), Pos2::new(130.0, 0.0));
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Bezier).width;
    bezier_place(&mut h, Pos2::new(200.0, 0.0), Pos2::new(230.0, 0.0));
    assert!(h.app.bezier_draft_undo());
    assert!(h.app.bezier_draft_redo());
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert_eq!(tip_widths(&h, id), vec![narrow, narrow, wide]);
    let up = EVec2::new(0.0, 1.0);
    let got = ink_half_width(&h, id, Pos2::new(125.0, 0.0), up);
    let want = (narrow + (wide - narrow) * smoothstep(0.25)) * 0.5;
    assert_close(got, want, 0.4, "a smooth blend, not a straight one");
}
