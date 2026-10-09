//! Closed-form corner picks, strip buttons, and paint paths.

use super::*;

/// User (28 September 2026, "alow for vrtex selection on closed forms like
/// sqares and polygons"): a click without travel on a rectangle corner
/// picks it; Alt+right-drag then sizes only that corner, blending along
/// both edges that meet there; one undo step, one redo step.
#[test]
fn a_rectangle_corner_click_picks_it_and_alt_sizes_only_that_corner() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_rect_pick", ShapeKind::Rect);
    let rect0 = h.app.doc().scene.node(id).unwrap().rect;
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[1]), egui::Modifiers::NONE);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![1])),
        "the click picks corner 1"
    );
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect,
        rect0,
        "a click does not resize"
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth,
        "a pick is not an edit"
    );

    let before = closed_tips(&h, id, 4);
    let away = xf.w2s(Pos2::new((v[0].x + v[1].x) * 0.5, v[2].y + 160.0));
    hud_scrub(&mut h, away, EVec2::new(40.0, 0.0), egui::Modifiers::ALT);
    let tips = closed_tips(&h, id, 4);
    assert!(tips[1].0 > before[1].0 + 10.0, "corner 1 widens: {tips:?}");
    for k in [0, 2, 3] {
        assert_eq!(tips[k], before[k], "corner {k} keeps its width");
    }
    assert_eq!(shape_kind_of(&h, id), ShapeKind::Rect, "still a rectangle");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    let widths = closed_painted_widths(&h, id);
    assert_eq!(widths.len(), 4, "one painted width per corner: {widths:?}");
    assert!(widths[1] > widths[0] + 10.0, "{widths:?}");
    assert!(
        widths[0] == widths[2] && widths[2] == widths[3],
        "{widths:?}"
    );
    for mid in [0.5, 1.5] {
        let w = slate_doc::geom::value_at_param(&widths, mid);
        assert!(
            w > widths[0] && w < widths[1],
            "both edges at corner 1 blend: {w}"
        );
    }
    for mid in [2.5, 3.5] {
        let w = slate_doc::geom::value_at_param(&widths, mid);
        assert_eq!(w, widths[0], "the far edges stay at the base width");
    }

    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(closed_tips(&h, id, 4), before, "undo");
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(closed_tips(&h, id, 4), tips, "redo");
}

/// A press-drag on a rectangle corner still resizes it (rectangle resize
/// is approved behavior); only a click without travel picks the corner.
#[test]
fn a_rectangle_corner_drag_still_resizes() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_rect_resize", ShapeKind::Rect);
    let rect0 = h.app.doc().scene.node(id).unwrap().rect;
    let xf = h.app.board_xf();
    let at = xf.w2s(v[2]);
    crop_pointer(&mut h, at, None);
    crop_pointer(&mut h, at, Some(true));
    crop_pointer(&mut h, at + EVec2::new(20.0, 15.0), None);
    crop_pointer(&mut h, at + EVec2::new(40.0, 30.0), None);
    crop_pointer(&mut h, at + EVec2::new(40.0, 30.0), Some(false));
    h.frame();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    assert!(
        (rect.w - (rect0.w + 40.0)).abs() < 1.0,
        "{rect:?} from {rect0:?}"
    );
    assert!(
        (rect.h - (rect0.h + 30.0)).abs() < 1.0,
        "{rect:?} from {rect0:?}"
    );
    assert_eq!(shape_kind_of(&h, id), ShapeKind::Rect);
}

/// Two polygon vertices picked (click, then Shift+click): Ctrl+right-drag
/// colors only those vertices, and the polygon stays a regular polygon.
#[test]
fn polygon_vertex_picks_take_the_ctrl_color_only_there() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_poly_color", ShapeKind::RegularPolygon);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[0]), egui::Modifiers::NONE);
    press_primary(&mut h, xf.w2s(v[2]), egui::Modifiers::SHIFT);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![0, 2])),
        "click, then Shift+click"
    );
    assert!(
        h.app.board_sel.contains(&id),
        "Shift+click on a vertex keeps the polygon selected"
    );
    assert!(
        h.app.text_edit.is_none(),
        "a quick second click on a vertex is a pick, not a double-click into text"
    );
    let before = closed_tips(&h, id, 6);
    let depth = h.app.tab().journal.undo_depth();
    let away = xf.w2s(Pos2::new(v[0].x, v[3].y + 200.0));
    hud_open_wheel(&mut h, away);
    h.app.set_active_rgb([10, 200, 30]);
    hud_release_wheel(&mut h, away);
    let tips = closed_tips(&h, id, 6);
    for k in 0..6 {
        if k == 0 || k == 2 {
            assert_eq!(tips[k].1[..3], [10, 200, 30], "vertex {k} takes the color");
        } else {
            assert_eq!(tips[k], before[k], "vertex {k} keeps its color");
        }
    }
    assert_eq!(shape_kind_of(&h, id), ShapeKind::RegularPolygon);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![0, 2])),
        "the picks outlast the HUD"
    );
    let t = h.ctx.input(|i| i.time);
    h.frame_with(|i| i.time = Some(t + 1.0));
    press_primary(&mut h, xf.w2s(v[2]), egui::Modifiers::SHIFT);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![0])),
        "Shift+click removes a pick"
    );
}

/// The strip's Corners amount at one picked rectangle corner rounds that
/// corner only; the rectangle's own corner stays square.
#[test]
fn strip_corner_rounding_at_a_picked_rectangle_corner_sets_only_that_corner() {
    use slate_doc::scene::{Corner, ShapeKind};
    let (mut h, id, v) = closed_form_board("closed_rect_corner", ShapeKind::Rect);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[2]), egui::Modifiers::NONE);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(
        h.app.shape_property_points(),
        vec![2],
        "the strip edits the pick"
    );
    let depth = h.app.tab().journal.undo_depth();
    h.app
        .preview_shape_property(board_properties::Property::CornerAmount(12.0));
    h.app.apply_shape_preview(&h.ctx, true);
    h.frame();
    assert_eq!(
        closed_corner_overrides(&h, id, 4),
        vec![None, None, Some(12.0), None]
    );
    let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("a shape")
    };
    assert_eq!(s.corner, Corner::Square, "the shared corner is unchanged");
    assert_eq!(s.shape, ShapeKind::Rect);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(closed_corner_overrides(&h, id, 4), vec![None; 4], "undo");
}

/// Shape-selection-toolbar D13: the Corners amount reads the first picked
/// corner. A rectangle or polygon stores its overrides without segments,
/// so the reading must come through the closed form, not the stored path.
#[test]
fn corners_panel_reads_a_picked_closed_form_corner_override() {
    use slate_doc::scene::ShapeKind;
    for (tag, kind, n) in [
        ("closed_rect_corner_read", ShapeKind::Rect, 4),
        ("closed_poly_corner_read", ShapeKind::RegularPolygon, 6),
    ] {
        let (mut h, id, v) = closed_form_board(tag, kind);
        let xf = h.app.board_xf();
        press_primary(&mut h, xf.w2s(v[2]), egui::Modifiers::NONE);
        for _ in 0..3 {
            h.frame();
        }
        assert_eq!(h.app.shape_property_points(), vec![2], "{kind:?}");
        h.app
            .preview_shape_property(board_properties::Property::CornerAmount(12.0));
        h.app.apply_shape_preview(&h.ctx, true);
        h.frame();
        assert_eq!(
            closed_corner_overrides(&h, id, n)[2],
            Some(12.0),
            "{kind:?}"
        );
        let node = h.app.doc().scene.node(id).unwrap().clone();
        assert_eq!(
            h.app.corners_panel_reading(&node).2,
            12.0,
            "{kind:?}: the Corners amount reads the picked corner, not the shared 0"
        );
        assert_eq!(h.app.node_grip_amount(&node, Some(2)), 12.0, "{kind:?}");
        assert_eq!(h.app.node_grip_amount(&node, Some(1)), 0.0, "{kind:?}");

        h.app
            .preview_shape_property(board_properties::Property::CornerAmount(0.0));
        h.app.apply_shape_preview(&h.ctx, true);
        h.frame();
        let node = h.app.doc().scene.node(id).unwrap().clone();
        assert_eq!(
            h.app.corners_panel_reading(&node).2,
            0.0,
            "{kind:?}: the corner can go back to square"
        );
    }
}

/// Art. II: a styled closed form derives its paint path once per change;
/// a steady frame reuses it.
#[test]
fn a_styled_rectangle_derives_its_paint_path_once() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_rect_paint_cache", ShapeKind::Rect);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[2]), egui::Modifiers::NONE);
    for _ in 0..3 {
        h.frame();
    }
    h.app
        .preview_shape_property(board_properties::Property::CornerAmount(12.0));
    h.app.apply_shape_preview(&h.ctx, true);
    assert_eq!(closed_corner_overrides(&h, id, 4)[2], Some(12.0));
    let before = board_path::closed_form_derives_on_this_thread();
    h.frame();
    h.frame();
    assert_eq!(
        board_path::closed_form_derives_on_this_thread() - before,
        1,
        "two painted frames derive the styled rectangle once"
    );
    h.app
        .preview_shape_property(board_properties::Property::CornerAmount(20.0));
    h.app.apply_shape_preview(&h.ctx, true);
    let before = board_path::closed_form_derives_on_this_thread();
    h.frame();
    h.frame();
    assert_eq!(
        board_path::closed_form_derives_on_this_thread() - before,
        1,
        "an edit derives it again, once"
    );
}

/// Art. II: a nested board numbers its nodes from one too. A host styled
/// rectangle and a nested one with the same id keep their own paint paths
/// instead of evicting each other every frame.
#[test]
fn a_nested_styled_rectangle_with_the_host_id_keeps_its_own_paint_path() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("closed_rect_nested_cache", ShapeKind::Rect);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[2]), egui::Modifiers::NONE);
    h.app
        .preview_shape_property(board_properties::Property::CornerAmount(12.0));
    h.app.apply_shape_preview(&h.ctx, true);
    assert_eq!(closed_corner_overrides(&h, id, 4)[2], Some(12.0));
    // The nested load starts on a worker at the drop and can land on the
    // very next frame, so paint the host's edit and count before the drop.
    h.frame();
    let host_warm = board_path::closed_form_derives_on_this_thread();

    let mut nested = h.app.doc().scene.node(id).unwrap().clone();
    if let NodeKind::Shape(s) = &mut nested.kind {
        std::sync::Arc::make_mut(s.path.as_mut().unwrap()).corner_amounts[2] = Some(30.0);
    }
    let wb = h.base.join("child.slate");
    let mut child = SlateDoc::new("Child");
    child.scene.nodes.push(nested);
    child.save_to(&wb).unwrap();
    h.app.tab_mut().path = Some(h.base.join("parent.slate"));
    let c = h.app.canvas_rect.center();
    let ctx = h.ctx.clone();
    h.app
        .apply_workbook_drop(&ctx, board_slate::WorkbookDropChoice::Insert, wb, xf.s2w(c));
    let portal = h.app.doc().scene.nodes.last().unwrap().id;
    assert_ne!(portal, id);
    let (a, b) = (
        xf.s2w(c + EVec2::new(150.0, -220.0)),
        xf.s2w(c + EVec2::new(410.0, -110.0)),
    );
    h.app.patch_nodes(&[portal], |n| {
        n.rect = WorldRect::new(a.x, a.y, b.x - a.x, b.y - a.y);
    });
    h.app.board_sel.clear();
    h.frame();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.slate_boards_ready() == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the nested board never loaded"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
    }
    h.frame();
    h.frame();
    assert!(
        board_path::closed_form_derives_on_this_thread() > host_warm,
        "the nested rectangle derives its own paint path"
    );
    let before = board_path::closed_form_derives_on_this_thread();
    h.frame();
    h.frame();
    assert_eq!(
        board_path::closed_form_derives_on_this_thread() - before,
        0,
        "two steady frames derive neither rectangle again"
    );
}

/// Shape-selection-toolbar D13: only a painted path-edit grip (a curve
/// anchor, a handle knob, or a line end point) wins a press under a strip
/// button. A rectangle corner under the strip's bottom edge (zoomed out)
/// leaves the button its click.
#[test]
fn a_strip_button_over_a_rectangle_corner_keeps_its_click() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, _) = closed_form_board("closed_rect_strip_corner", ShapeKind::Rect);
    h.app.tab_mut().cam.z = 0.4;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    h.app.patch_nodes(&[id], |n| {
        n.rect = WorldRect::new(c.x - 20.0, c.y - 90.0, 40.0, 180.0);
    });
    for _ in 0..10 {
        h.frame();
    }
    let xf = h.app.board_xf();
    let corner = xf.w2s(Pos2::new(c.x - 20.0, c.y - 90.0));
    let press = h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .map(|r| r.shrink(0.5).clamp(corner))
        .find(|p| p.distance(corner) < 6.5)
        .expect("a strip button reaches the corner");
    assert_eq!(
        h.app.hovered_vertex(press),
        Some((id, 0)),
        "the press is within the corner's hit radius"
    );
    let depth = h.app.tab().journal.undo_depth();
    press_primary(&mut h, press, egui::Modifiers::NONE);
    assert!(
        h.app.shape_properties.panel.is_some(),
        "the button opened its panel"
    );
    assert_eq!(h.app.picked_vertices(), None, "the corner was not picked");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// Shape-selection-toolbar D13 / D12: a grip press that picks an anchor
/// narrows the strip to per-vertex controls, so an open Fill panel, whose
/// squircle is no longer offered, closes. The press commits the pending
/// Fill preview once; the close journals nothing more.
#[test]
fn a_grip_pick_closes_a_strip_panel_it_no_longer_offers() {
    let mut h = bezier_board("grip_pick_closes_fill");
    let (id, [_, _, c]) = closed_bezier(&mut h);
    h.app.shape_properties.panel = Some(board_properties::Panel::Fill);
    h.frame();
    h.frame();
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Fill),
        "Fill is offered while nothing is picked"
    );
    h.app
        .preview_shape_property(board_properties::Property::FillRgb([200, 40, 40]));
    let xf = h.app.board_xf();
    let press = xf.w2s(c);
    assert!(
        !h.app
            .shape_properties
            .chrome_hits
            .iter()
            .any(|r| r.contains(press)),
        "the anchor lies outside the strip and its panel"
    );
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
    assert_ne!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Fill),
        "Fill is not offered with an anchor picked, so its panel closes"
    );
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

/// Shape-selection-toolbar D13: Stroke stays offered for a picked anchor, so
/// a press on another anchor keeps the Stroke panel up and picks it.
#[test]
fn a_grip_pick_keeps_a_stroke_panel_that_is_still_offered() {
    let mut h = bezier_board("grip_pick_keeps_stroke");
    let (id, [_, b, c]) = closed_bezier(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(b), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1])));
    h.app.shape_properties.panel = Some(board_properties::Panel::Stroke);
    h.frame();
    h.frame();
    let press = xf.w2s(c);
    assert!(
        !h.app
            .shape_properties
            .chrome_hits
            .iter()
            .any(|r| r.contains(press)),
        "the other anchor lies outside the strip and its panel"
    );
    press_primary(&mut h, press, egui::Modifiers::NONE);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(h.app.picked_vertices(), Some((id, vec![2])));
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Stroke),
        "a grip press is not a click-away and Stroke is still offered"
    );
}

/// Shape-selection-toolbar D11 / D13: a pending per-vertex Stroke preview is
/// committed by the next grip press as exactly one journaled step, on the
/// vertex that was picked when it was made.
#[test]
fn a_grip_press_commits_a_pending_stroke_preview_once() {
    let mut h = bezier_board("grip_press_commits_stroke");
    let (id, [_, b, c]) = closed_bezier(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(b), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1])));
    h.app.shape_properties.panel = Some(board_properties::Panel::Stroke);
    h.frame();
    h.frame();
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    h.app
        .preview_shape_property(board_properties::Property::StrokeWidth(14.0));
    press_primary(&mut h, xf.w2s(c), egui::Modifiers::NONE);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(h.app.picked_vertices(), Some((id, vec![2])));
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the preview journals exactly one step"
    );
    assert_ne!(scene_nodes(&h), before, "the width landed");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(scene_nodes(&h), before, "one Ctrl+Z restores");
}
