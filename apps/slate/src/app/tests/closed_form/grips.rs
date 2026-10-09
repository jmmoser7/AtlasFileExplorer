//! Zoomed grips, strip buttons, and closed-form vertex export.

use super::*;

/// Shape-selection-toolbar D11 / D13 below the strip's LOD: zoomed out the
/// strip is not painted but the screen-constant grips still take presses.
/// A grip press that narrows the strip commits the pending Fill preview
/// once before its panel closes; nothing is discarded.
#[test]
fn a_zoomed_out_grip_pick_commits_the_fill_preview_it_closes() {
    let mut h = bezier_board("grip_pick_zoomed_out_fill");
    let (id, [_, _, c]) = closed_bezier(&mut h);
    h.app.shape_properties.panel = Some(board_properties::Panel::Fill);
    h.frame();
    h.frame();
    h.app
        .preview_shape_property(board_properties::Property::FillRgb([200, 40, 40]));
    h.app.tab_mut().cam.z = 0.3;
    for _ in 0..3 {
        h.frame();
    }
    let xf = h.app.board_xf();
    let press = xf.w2s(c);
    assert!(h.app.canvas_rect.contains(press), "the anchor is on screen");
    let depth = h.app.tab().journal.undo_depth();
    press_primary(&mut h, press, egui::Modifiers::NONE);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![2])),
        "the anchor is picked"
    );
    assert_eq!(h.app.shape_properties.panel, None, "Fill closes");
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the pending Fill was committed once"
    );
    let (_, s) = only_shape(&h);
    assert_eq!(
        s.fill.map(|f| [f.0[0], f.0[1], f.0[2]]),
        Some([200, 40, 40])
    );
}

/// Shape-selection-toolbar D11 below the strip's LOD: a whole-curve Stroke
/// width preview is committed by the grip press itself, before the press
/// picks the anchor, so every vertex takes the width and the panel stays.
#[test]
fn a_zoomed_out_grip_press_commits_a_whole_curve_stroke_preview() {
    let mut h = bezier_board("grip_press_zoomed_out_stroke");
    let (id, [_, _, c]) = closed_bezier(&mut h);
    h.app.shape_properties.panel = Some(board_properties::Panel::Stroke);
    h.frame();
    h.frame();
    assert_eq!(h.app.picked_vertices(), None, "nothing is picked yet");
    h.app
        .preview_shape_property(board_properties::Property::StrokeWidth(14.0));
    h.app.tab_mut().cam.z = 0.3;
    for _ in 0..3 {
        h.frame();
    }
    let xf = h.app.board_xf();
    let press = xf.w2s(c);
    assert!(h.app.canvas_rect.contains(press), "the anchor is on screen");
    let depth = h.app.tab().journal.undo_depth();
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(press)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
    });
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the press commits the preview as exactly one step"
    );
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
    });
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![2])),
        "the anchor is picked"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "nothing more");
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape");
    };
    let tips = slate_doc::vertex_style::grip_tips(
        s.path.as_ref().unwrap(),
        &s.stroke,
        n.rect,
        n.rotation_deg,
    )
    .expect("curve grips");
    assert_eq!(tips.len(), 3);
    for (k, tip) in tips.iter().enumerate() {
        assert_close(tip.width, 14.0, 1e-3, &format!("vertex {k} width"));
    }
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Stroke),
        "Stroke is still offered for the picked anchor"
    );
}

/// Shape-selection-toolbar D11 below the strip's LOD: a press on empty
/// canvas commits a pending Fill preview once and closes its panel, and
/// the board still gets the press, which clears the selection.
#[test]
fn a_zoomed_out_click_away_commits_the_fill_preview_and_clears_the_selection() {
    let mut h = bezier_board("click_away_zoomed_out_fill");
    let (id, _) = closed_bezier(&mut h);
    h.app.shape_properties.panel = Some(board_properties::Panel::Fill);
    h.frame();
    h.frame();
    h.app
        .preview_shape_property(board_properties::Property::FillRgb([200, 40, 40]));
    h.app.tab_mut().cam.z = 0.3;
    for _ in 0..3 {
        h.frame();
    }
    let xf = h.app.board_xf();
    let away = xf.w2s(Pos2::new(-400.0, 400.0));
    assert!(
        h.app.canvas_rect.contains(away),
        "the empty spot is on screen"
    );
    assert!(
        h.app.doc().scene.nodes.iter().all(|n| {
            let r = xf.rect_w2s(n.rect).expand(24.0);
            !r.contains(away)
        }),
        "nothing lies under the press"
    );
    let depth = h.app.tab().journal.undo_depth();
    press_primary(&mut h, away, egui::Modifiers::NONE);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the pending Fill was committed once"
    );
    let (_, s) = only_shape(&h);
    assert_eq!(
        s.fill.map(|f| [f.0[0], f.0[1], f.0[2]]),
        Some([200, 40, 40])
    );
    assert_eq!(h.app.shape_properties.panel, None, "Fill closes");
    assert!(
        !h.app.board_sel.contains(&id),
        "the board took the press and cleared the selection"
    );
}

/// Shape-selection-toolbar D13: only a vertex pick closes a panel the strip
/// stops offering. Double-clicking a grouped rectangle edits its text with
/// the group still selected, and the Text panel stays up.
#[test]
fn text_editing_a_grouped_shape_keeps_the_text_panel() {
    let mut h = grip_board("grouped_text_panel");
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let a = add_rect(&mut h.app, c.x - 120.0, c.y - 30.0);
    let b = add_rect(&mut h.app, c.x + 40.0, c.y - 30.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.group"), None));
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let on_a = xf.w2s(Pos2::new(c.x - 80.0, c.y));
    click_board(&mut h, on_a);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(h.app.board_sel, [a, b].into_iter().collect(), "the group");
    pause(&mut h);
    click_board(&mut h, on_a);
    click_board(&mut h, on_a);
    for _ in 0..4 {
        h.frame();
    }
    assert!(
        h.app.text_edit.as_ref().is_some_and(|(id, _)| *id == a),
        "the double-click edits the member's text"
    );
    assert_eq!(
        h.app.board_sel,
        [a, b].into_iter().collect(),
        "still the group"
    );
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Text),
        "the Text panel stays up"
    );
}

/// P1.curve.grips under Direct Select: a soft brush stroke is not a Select
/// grip target, but Direct Select paints its anchors and handle knobs, so a
/// knob under a strip button still drags.
#[test]
fn a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button() {
    let mut h = bezier_board("direct_knob_soft_stroke");
    let (id, [_, b, _]) = handle_bezier(&mut h);
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0),
        b + EVec2::new(0.0, -25.0),
        egui::Modifiers::NONE,
    );
    let (_, out0) = handle_pair(&h, id, 1);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.stroke.softness = 0.5;
        }
    });
    assert!(
        h.app.curve_grips_of(id).is_none(),
        "not a Select grip target"
    );
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.frame();
    let xf = h.app.board_xf();
    // The first press targets the curve; the second, past the
    // double-click interval, picks the anchor.
    press_primary(&mut h, xf.w2s(b), egui::Modifiers::NONE);
    let t = h.ctx.input(|i| i.time);
    h.frame_with(|i| i.time = Some(t + 1.0));
    press_primary(&mut h, xf.w2s(b), egui::Modifiers::NONE);
    for _ in 0..8 {
        h.frame();
    }
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1])), "b is picked");
    let knob = xf.w2s(out0);
    let press = h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .map(|r| r.shrink(0.5).clamp(knob))
        .find(|p| p.distance(knob) < 5.0)
        .expect("a strip button covers the knob");
    assert_eq!(h.app.hovered_vertex(press), Some((id, 1)));
    let depth = h.app.tab().journal.undo_depth();
    let from = xf.s2w(press);
    grip_drag(
        &mut h,
        from,
        from + EVec2::new(0.0, -20.0),
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.shape_properties.panel, None, "no strip panel opened");
    let (_, out1) = handle_pair(&h, id, 1);
    assert!(
        near_eps(out1, out0 + EVec2::new(0.0, -20.0), 0.5),
        "the handle follows the drag: {out0:?} -> {out1:?}"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
}

/// Shape-selection-toolbar D13, line D13: a simple line's end point is a
/// painted path-edit grip (P1.curve.grips), so where a strip button covers
/// it (zoomed out) the press drags the end point and the button stays idle.
#[test]
fn a_line_end_point_under_a_strip_button_drags() {
    let mut h = grip_board("line_end_under_strip");
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.app.tab_mut().cam.z = 0.4;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let (top, bottom) = (c + EVec2::new(0.0, -90.0), c + EVec2::new(0.0, 90.0));
    let id = commit_polyline(&mut h, &[top, bottom], false);
    for _ in 0..10 {
        h.frame();
    }
    let ends = |h: &Harness| board_line::line_endpoints(h.app.doc().scene.node(id).unwrap());
    assert!(ends(&h).is_some(), "a simple line");
    let xf = h.app.board_xf();
    let grip = xf.w2s(top);
    let press = h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .map(|r| r.shrink(0.5).clamp(grip))
        .find(|p| p.distance(grip) < 6.5)
        .expect("a strip button reaches the end point");
    assert_eq!(
        h.app.hovered_vertex(press),
        Some((id, 0)),
        "the press is within the end point's hit radius"
    );
    let before = ends(&h).unwrap();
    let depth = h.app.tab().journal.undo_depth();
    let from = xf.s2w(press);
    grip_drag(
        &mut h,
        from,
        from + EVec2::new(-60.0, -40.0),
        egui::Modifiers::NONE,
    );
    assert_eq!(
        h.app.shape_properties.panel, None,
        "the button did not fire"
    );
    let after = ends(&h).unwrap();
    assert!(
        (after.0 - before.0).length() > 30.0,
        "the end point moved: {before:?} -> {after:?}"
    );
    assert!(near_eps(after.1, before.1, 0.01), "the other end stays");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
}

/// Polygon D13: a click on a vertex picks it, and the hover + / − beside
/// the vertex still add or remove a side.
#[test]
fn polygon_side_glyphs_still_step_sides_beside_a_picked_vertex() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_poly_glyphs", ShapeKind::RegularPolygon);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[1]), egui::Modifiers::NONE);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![1])),
        "a click on the vertex picks it"
    );
    let sides = |h: &Harness| match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Shape(s) => s.sides,
        _ => 0,
    };
    assert_eq!(sides(&h), 6, "picking a vertex is not a side step");
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(xf.w2s(v[1]))));
    h.frame();
    let glyphs = h.app.polygon_sides_glyphs(&xf);
    let plus = glyphs
        .iter()
        .find(|g| g.add)
        .expect("a + beside the hovered vertex")
        .center;
    press_primary(&mut h, plus, egui::Modifiers::NONE);
    assert_eq!(sides(&h), 7, "+ adds a side");
    assert_eq!(shape_kind_of(&h, id), ShapeKind::RegularPolygon);
}

/// Direct Select (A): a closed form's vertices are anchors to pick; the
/// HUD edits the picked one; an anchor drag does not turn the rectangle
/// into a path (a rectangle's corners cannot move on their own).
#[test]
fn direct_select_picks_closed_form_vertices_as_anchors() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_rect_direct", ShapeKind::Rect);
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.frame();
    let xf = h.app.board_xf();
    let inside = xf.w2s(Pos2::new((v[0].x + v[1].x) * 0.5, (v[0].y + v[2].y) * 0.5));
    press_primary(&mut h, inside, egui::Modifiers::NONE);
    assert_eq!(h.app.direct.node, Some(id), "A targets the rectangle");
    press_primary(&mut h, xf.w2s(v[3]), egui::Modifiers::NONE);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![3])),
        "the anchor is picked"
    );
    let before = closed_tips(&h, id, 4);
    let away = xf.w2s(Pos2::new(v[0].x, v[2].y + 160.0));
    hud_scrub(&mut h, away, EVec2::new(40.0, 0.0), egui::Modifiers::ALT);
    let tips = closed_tips(&h, id, 4);
    assert!(tips[3].0 > before[3].0 + 10.0, "{tips:?}");
    assert_eq!(&tips[..3], &before[..3]);

    let rect0 = h.app.doc().scene.node(id).unwrap().rect;
    let at = xf.w2s(v[3]);
    crop_pointer(&mut h, at, None);
    crop_pointer(&mut h, at, Some(true));
    crop_pointer(&mut h, at + EVec2::new(-20.0, 20.0), None);
    crop_pointer(&mut h, at + EVec2::new(-40.0, 40.0), None);
    crop_pointer(&mut h, at + EVec2::new(-40.0, 40.0), Some(false));
    h.frame();
    assert_eq!(
        shape_kind_of(&h, id),
        ShapeKind::Rect,
        "no silent Rect → Path"
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect0);
}

/// Per-vertex stroke on a closed form survives save and reload unchanged,
/// and the HTML export paints the color blend the board paints (Art. IV:
/// two interpreters of one model).
#[test]
fn closed_form_vertex_stroke_round_trips_and_exports_as_the_board_paints() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_rect_export", ShapeKind::Rect);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[1]), egui::Modifiers::NONE);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(
        h.app.shape_property_points(),
        vec![1],
        "the strip edits the pick"
    );
    for edit in [
        board_properties::Property::StrokeWidth(16.0),
        board_properties::Property::StrokeRgb([255, 0, 0]),
    ] {
        h.app.preview_shape_property(edit);
        h.app.apply_shape_preview(&h.ctx, true);
        h.frame();
    }
    let tips = closed_tips(&h, id, 4);
    assert_eq!(tips[1].1[..3], [255, 0, 0], "{tips:?}");
    assert!(tips[1].0 > tips[0].0 + 10.0, "{tips:?}");
    assert_eq!(tips[0], tips[2], "{tips:?}");
    assert_eq!(shape_kind_of(&h, id), ShapeKind::Rect);

    let saved = h.base.join("closed.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, saved.clone());
    let mut h2 = Harness::new("closed_rect_export_reload");
    h2.app.open_doc_at(saved);
    h2.frame();
    let reloaded = h2
        .app
        .doc()
        .scene
        .node(id)
        .cloned()
        .expect("the rectangle reloads");
    assert_eq!(
        &reloaded,
        h.app.doc().scene.node(id).unwrap(),
        "save and load round-trip"
    );

    let (n, styled) = closed_paint_shape(&h, id);
    let mesh = board_path::vector_stroke_ink(&n, &styled, styled.path.as_ref().unwrap(), 1.0);
    let html = slate_artifact::render_html(h2.app.doc(), &slate_artifact::AssetMap::default());
    assert!(
        html.contains("<linearGradient"),
        "the blend exports as gradients"
    );
    let local = |p: Pos2| [p.x - n.rect.x, p.y - n.rect.y];
    let along = |a: Pos2, b: Pos2, t: f32| a + (b - a) * t;
    for p in [
        along(v[0], v[1], 0.2),
        along(v[0], v[1], 0.5),
        along(v[0], v[1], 0.8),
        along(v[1], v[2], 0.3),
        along(v[1], v[2], 0.7),
    ] {
        let board = mesh_color_at(&mesh, p);
        let export =
            export_color_at(&html, local(p)).unwrap_or_else(|| panic!("the export paints {p:?}"));
        assert_color_close(export, board, 3.0, &format!("export at {p:?}"));
    }
    let mid_top = mesh_color_at(&mesh, along(v[0], v[1], 0.5));
    assert!(
        mid_top[0] > 60.0 && mid_top[0] < 240.0,
        "the top edge blends toward the red corner: {mid_top:?}"
    );
}
