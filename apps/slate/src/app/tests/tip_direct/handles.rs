//! Bezier handle drags, caps, and the property strip.

use super::*;

/// P1.curve.vertex-style: the style row reads the effective end caps. Two
/// equal end caps are the curve's end cap; `cap` still caps the dashes.
#[test]
fn curve_style_reads_two_equal_end_caps_as_the_curves_cap() {
    use board_tip_hud::{curve_style_of, CurveStyle};
    use slate_doc::scene::{Stroke, StrokeCap, StrokeEnd};
    let mut s = Stroke::none();
    s.width = 4.0;
    s.cap = StrokeCap::Butt;
    assert_eq!(curve_style_of(&s), Some(CurveStyle::Square));
    let round = StrokeEnd {
        cap: StrokeCap::Round,
        arrow: false,
        narrow: false,
    };
    s.set_end(0, round);
    assert_eq!(curve_style_of(&s), None, "the ends were set apart");
    s.set_end(1, round);
    assert_eq!(s.cap, StrokeCap::Butt, "the reader never rewrites cap");
    assert_eq!(
        curve_style_of(&s),
        Some(CurveStyle::Round),
        "Round on both ends"
    );

    let narrow = StrokeEnd {
        cap: StrokeCap::Round,
        arrow: false,
        narrow: true,
    };
    let mut s = Stroke::none();
    s.width = 4.0;
    s.cap = StrokeCap::Butt;
    s.set_end(0, narrow);
    s.set_end(1, narrow);
    assert_eq!(
        curve_style_of(&s),
        Some(CurveStyle::TaperBoth),
        "narrow at both ends"
    );
    assert_eq!(s.cap, StrokeCap::Butt);
}

/// es1 (user pass, 28 September 2026: "Releasing on one restyles the whole
/// curve, even with points picked"): with only an interior point picked,
/// the row still restyles the whole curve.
#[test]
fn an_interior_pick_still_restyles_the_whole_curve() {
    let mut h = grip_board("end_condition_interior");
    let (id, pts) = hud_polyline(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(pts[1]), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1])));
    let away = xf.w2s(pts[1] + EVec2::new(0.0, 160.0));
    hud_row_release(&mut h, away, 4);
    let Some(NodeKind::Shape(s)) = h.app.doc().scene.node(id).map(|n| &n.kind) else {
        panic!("a shape");
    };
    assert!(matches!(
        s.stroke.profile,
        slate_doc::scene::WidthProfile::Ends { .. }
    ));
    hud_row_release(&mut h, away, 2);
    let Some(NodeKind::Shape(s)) = h.app.doc().scene.node(id).map(|n| &n.kind) else {
        panic!("a shape");
    };
    assert!(s.stroke.arrow_end, "one head, at the curve's end");
    assert_eq!(s.stroke.profile, slate_doc::scene::WidthProfile::Uniform);
}

/// User (27 September 2026): with control points picked, the property strip
/// sits beside those points, under Select and Direct Select alike.
#[test]
fn the_property_strip_sits_beside_the_picked_vertices() {
    // No editor is open, so every chrome hit is a strip squircle.
    fn strip_rect(h: &Harness) -> ERect {
        let hits = &h.app.shape_properties.chrome_hits;
        assert!(!hits.is_empty(), "the strip painted");
        hits.iter().fold(hits[0], |r, b| r.union(*b))
    }
    let mut h = grip_board("strip_at_picks");
    let (id, pts) = hud_polyline(&mut h);
    let xf = h.app.board_xf();
    for _ in 0..8 {
        h.frame();
    }
    let whole = strip_rect(&h);
    assert!(
        (whole.center().x - xf.w2s(pts[1]).x).abs() < 2.0,
        "whole curve: centered"
    );

    press_primary(&mut h, xf.w2s(pts[2]), egui::Modifiers::NONE);
    for _ in 0..8 {
        h.frame();
    }
    let at = xf.w2s(pts[2]);
    let strip = strip_rect(&h);
    assert!(
        (strip.center().x - at.x).abs() < 2.0,
        "{strip:?} beside {at:?}"
    );
    assert!(strip.bottom() < at.y, "above the picked point");

    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.app.direct.anchors = [0].into_iter().collect();
    for _ in 0..8 {
        h.frame();
    }
    let at = xf.w2s(pts[0]);
    let strip = strip_rect(&h);
    assert!(
        (strip.center().x - at.x).abs() < 2.0,
        "Direct Select: {strip:?} beside {at:?}"
    );
    assert_eq!(h.app.shape_property_points(), vec![0]);
}

/// tip31 (user: "works for polyline should also work for handels of bezier
/// span"): a handle keeps its grab offset, snaps its tip to other anchors
/// and never to its own anchor.
#[test]
fn a_dragged_handle_keeps_its_grab_offset_and_snaps_its_tip() {
    let mut h = bezier_board("handle_snap_select");
    let (id, [_, b, c]) = handle_bezier(&mut h);
    let knob = b + EVec2::new(40.0, 0.0);
    let grab = EVec2::new(4.0, 0.0);
    let handle_out = |h: &Harness| {
        kpt(h.app.direct_anchors_of(id).unwrap().0[1]
            .handle_out
            .unwrap())
    };

    select_drag(
        &mut h,
        knob + grab,
        knob + grab + EVec2::new(0.0, 30.0),
        egui::Modifiers::NONE,
    );
    let got = handle_out(&h);
    assert!(
        near(got, knob + EVec2::new(0.0, 30.0)),
        "no jump to the cursor: {got:?}"
    );

    let from = handle_out(&h) + grab;
    let to = b + grab + EVec2::new(3.0, 2.0);
    select_drag(&mut h, from, to, egui::Modifiers::NONE);
    let got = handle_out(&h);
    assert!(
        near(got, b + EVec2::new(3.0, 2.0)),
        "the handle's own anchor is not a target: {got:?}"
    );

    let from = handle_out(&h) + grab;
    select_drag(
        &mut h,
        from,
        c + grab + EVec2::new(2.0, -1.5),
        egui::Modifiers::NONE,
    );
    assert!(
        near(handle_out(&h), c),
        "the tip snaps to anchor c: {:?}",
        handle_out(&h)
    );
}

/// tip31 while drafting a Bézier span: the dragged handle keeps its offset
/// and snaps its tip to another placed anchor, never its own.
#[test]
fn a_draft_bezier_handle_keeps_its_grab_offset_and_snaps_its_tip() {
    let mut h = bezier_board("handle_snap_draft");
    bezier_place(&mut h, Pos2::ZERO, Pos2::new(40.0, 0.0));
    bezier_place(&mut h, Pos2::new(200.0, 0.0), Pos2::new(200.0, 0.0));
    let grab = EVec2::new(4.0, 0.0);
    let knob = Pos2::new(40.0, 0.0);
    press_drag_release(
        &mut h,
        &[
            knob + grab,
            knob + grab + EVec2::new(0.0, 15.0),
            knob + grab + EVec2::new(0.0, 30.0),
        ],
        egui::Modifiers::NONE,
    );
    let a = bezier_draft(&h);
    assert!(
        (a[0].1.handle_out - EVec2::new(40.0, 30.0)).length() < 0.01,
        "no jump to the cursor: {:?}",
        a[0].1.handle_out
    );
    let tip = a[0].0 + a[0].1.handle_out;
    press_drag_release(
        &mut h,
        &[tip + grab, Pos2::new(3.0, 2.0) + grab],
        egui::Modifiers::NONE,
    );
    let a = bezier_draft(&h);
    assert!(
        (a[0].1.handle_out - EVec2::new(3.0, 2.0)).length() < 0.01,
        "its own anchor is not a target: {:?}",
        a[0].1.handle_out
    );
    let tip = a[0].0 + a[0].1.handle_out;
    let target = Pos2::new(200.0, 0.0) + EVec2::new(-2.0, 1.5);
    press_drag_release(&mut h, &[tip + grab, target + grab], egui::Modifiers::NONE);
    let a = bezier_draft(&h);
    assert!(
        near(a[0].0 + a[0].1.handle_out, Pos2::new(200.0, 0.0)),
        "snaps to the other anchor: {:?}",
        a[0].1.handle_out
    );
    assert!(h.app.doc().scene.nodes.is_empty(), "still drafting");
}

/// pp2 (user pass, 28 September 2026) against the Shift handle lock (user,
/// 28 September 2026: "shift lmb to lock direction of grp handel on
/// scaling"): a Shift click without travel on an anchor or handle knob
/// toggles the pick and changes no geometry; a Shift drag leaves the picks.
#[test]
fn shift_click_toggles_a_grip_pick_and_shift_drag_does_not() {
    let mut h = bezier_board("shift_click_handle_pick");
    let (id, [a, b, _]) = handle_bezier(&mut h);
    let xf = h.app.board_xf();
    let knob = b + EVec2::new(40.0, 0.0);
    press_primary(&mut h, xf.w2s(a), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0])));
    let before = scene_nodes(&h);
    press_primary(&mut h, xf.w2s(knob), egui::Modifiers::SHIFT);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![0, 1])),
        "a Shift+click on a handle knob adds its anchor"
    );
    assert!(
        !h.app.palette_state.open,
        "two quick grip picks are not a canvas double-click"
    );
    press_primary(&mut h, xf.w2s(knob), egui::Modifiers::SHIFT);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![0])),
        "a second Shift+click removes it"
    );
    press_primary(&mut h, xf.w2s(b), egui::Modifiers::SHIFT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 1])));
    assert_eq!(scene_nodes(&h), before, "picking moves nothing");

    grip_drag(
        &mut h,
        knob,
        knob + EVec2::new(30.0, 20.0),
        egui::Modifiers::SHIFT,
    );
    assert_ne!(scene_nodes(&h), before, "the Shift drag edited the handle");
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![0, 1])),
        "a Shift drag is not a pick"
    );
}

/// User, 28 September 2026: "shift lmb to lock direction of grp handel on
/// scaling". Shift+drag keeps the handle's angle and changes only its
/// length; a snap lands where it projects onto the ray. One undo per drag.
#[test]
fn shift_drag_of_a_handle_keeps_its_angle_and_changes_only_length() {
    let mut h = bezier_board("shift_drag_handle_lock");
    let (id, [_, b, c]) = handle_bezier(&mut h);
    // Angle the out handle first with a plain drag.
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0),
        b + EVec2::new(30.0, 30.0),
        egui::Modifiers::NONE,
    );
    let (in0, out0) = handle_pair(&h, id, 1);
    assert!(near_eps(out0, b + EVec2::new(30.0, 30.0), 0.01), "{out0:?}");
    let depth = h.app.tab().journal.undo_depth();

    let grab = EVec2::new(2.0, -1.0);
    grip_drag(
        &mut h,
        out0 + grab,
        out0 + grab + EVec2::new(25.0, -5.0),
        egui::Modifiers::SHIFT,
    );
    let (in1, out1) = handle_pair(&h, id, 1);
    let (v0, v1) = (out0 - b, out1 - b);
    assert!(
        (v1.angle() - v0.angle()).abs() < 1e-4,
        "the angle holds: {v0:?} -> {v1:?}"
    );
    let want = v0.length() + EVec2::new(25.0, -5.0).dot(v0.normalized());
    assert!(
        (v1.length() - want).abs() < 0.01,
        "only the length follows the pointer: {} != {want}",
        v1.length()
    );
    assert!(
        near_eps(in1, in0, 0.01),
        "the opposite handle stays: {in1:?}"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(handle_pair(&h, id, 1), (in0, out0), "one Ctrl+Z restores");

    // A horizontal handle snapping at anchor c lands on c's projection.
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    let (_, out) = handle_pair(&h, id, 1);
    assert!(near_eps(out, b + EVec2::new(40.0, 0.0), 0.01), "{out:?}");
    let grab = EVec2::new(3.0, 0.0);
    grip_drag(
        &mut h,
        out + grab,
        c + grab + EVec2::new(2.0, -1.5),
        egui::Modifiers::SHIFT,
    );
    let (_, out) = handle_pair(&h, id, 1);
    assert!(
        near_eps(out, Pos2::new(c.x, b.y), 0.01),
        "the snap to c projects onto the ray: {out:?}"
    );
}

/// P1.curve.grips: every non-zero handle is draggable, even where the
/// property strip over a picked anchor covers its knob. A press on the
/// knob drags the handle; it does not open a strip panel.
#[test]
fn a_handle_knob_under_the_property_strip_still_drags() {
    let mut h = bezier_board("knob_under_strip");
    let (id, [_, b, _]) = handle_bezier(&mut h);
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0),
        b + EVec2::new(0.0, -25.0),
        egui::Modifiers::NONE,
    );
    let (_, out0) = handle_pair(&h, id, 1);
    assert!(near_eps(out0, b + EVec2::new(0.0, -25.0), 0.01), "{out0:?}");
    let xf = h.app.board_xf();
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
    assert_eq!(
        h.app.hovered_vertex(press),
        Some((id, 1)),
        "the press lands on the knob's grip"
    );
    let depth = h.app.tab().journal.undo_depth();
    let from = xf.s2w(press);
    grip_drag(
        &mut h,
        from,
        from + EVec2::new(0.0, -20.0),
        egui::Modifiers::SHIFT,
    );
    assert_eq!(h.app.shape_properties.panel, None, "no strip panel opened");
    let (_, out1) = handle_pair(&h, id, 1);
    let (v0, v1) = (out0 - b, out1 - b);
    assert!(
        (v1.length() - (v0.length() + 20.0)).abs() < 0.5,
        "the handle lengthens: {v0:?} -> {v1:?}"
    );
    assert!(
        (v1.angle() - v0.angle()).abs() < 1e-3,
        "Shift keeps its direction: {v0:?} -> {v1:?}"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
}

/// P1.curve.grips with a strip panel open: Stroke is offered for a picked
/// anchor, and its handle knob under a strip button still drags.
#[test]
fn a_handle_knob_under_the_property_strip_drags_with_a_panel_open() {
    let mut h = bezier_board("knob_under_strip_panel");
    let (id, [_, b, _]) = handle_bezier(&mut h);
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0),
        b + EVec2::new(0.0, -25.0),
        egui::Modifiers::NONE,
    );
    let (_, out0) = handle_pair(&h, id, 1);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(b), egui::Modifiers::NONE);
    for _ in 0..8 {
        h.frame();
    }
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1])), "b is picked");
    h.app.shape_properties.panel = Some(board_properties::Panel::Stroke);
    h.frame();
    h.frame();
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
        egui::Modifiers::SHIFT,
    );
    let (_, out1) = handle_pair(&h, id, 1);
    let (v0, v1) = (out0 - b, out1 - b);
    assert!(
        (v1.length() - (v0.length() + 20.0)).abs() < 0.5,
        "the handle lengthens: {v0:?} -> {v1:?}"
    );
    assert!(
        (v1.angle() - v0.angle()).abs() < 1e-3,
        "Shift keeps its direction: {v0:?} -> {v1:?}"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Stroke),
        "a grip press is not a click-away"
    );
}
