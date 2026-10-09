//! Stroke style tests mixed into the ink section.

use super::*;

/// Stated: Alt+right-drag sizes the pen, line, arc, polyline, and Bézier
/// through the brush's size chord. Each changes only its own width, the
/// vertical drag offers no softness, and the HUD reads the width.
#[test]
fn alt_right_drag_sizes_every_stroke_tool() {
    let mut h = line_board("stroke_size_chord");
    h.app.tab_mut().cam.z = 1.0;
    h.app.brush_width = 10.0;
    for (tool, slot) in STROKE_TOOLS {
        h.app.set_board_tool(tool);
        let before = h.app.stroke_for_tool(slot);
        assert!(h.app.board_tool_takes_width_chord(), "{tool:?}");
        width_chord(&mut h, 40.0);
        let during = h.app.stroke_for_tool(slot);
        assert!(during.width > before.width + 1.0, "{tool:?} got wider");
        assert_eq!(during.softness, 0.0, "{tool:?} stays hard");
        let label = h.app.size_hud_label().unwrap_or_default();
        let px = format!("{:.0} px", during.width);
        assert!(
            label.starts_with(&px),
            "{tool:?} HUD reads {label:?}, expected it to start with {px}"
        );
        release_chord(&mut h);
        assert_eq!(h.app.stroke_for_tool(slot).width, during.width);
        let saved = h.app.doc().view.create_style.clone().unwrap_or_default();
        assert_eq!(
            saved.tool(slot).stroke.map(|s| s.width),
            Some(during.width),
            "{tool:?} width is saved with the workbook"
        );
    }
    assert_eq!(h.app.brush_width, 10.0, "the brush size is untouched");

    h.app.set_board_tool(board::BoardTool::Line);
    let kept = h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width;
    width_chord(&mut h, 60.0);
    h.app.cancel_brush_hud();
    h.app.alt_down = false;
    assert_eq!(
        h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width,
        kept,
        "Escape restores the width"
    );
}

/// r7-9 (user, 28 September 2026): "Watercolor builds where a stroke
/// crosses itself." One Watercolor stroke that crosses its own first leg
/// paints that crossing as two strokes along the same path, split between
/// the two passes, would; the pair is drawn 256 world units lower, one
/// period of the paper, so both sit on the same grain.
#[test]
fn one_watercolor_stroke_builds_where_it_crosses_itself() {
    use slate_doc::scene::BrushTexture;
    let mut h = brush_board("r7_9_self_crossing");
    h.app.board_colors.fg.0 = [30, 90, 200, 255];
    h.app.brush_opacity = 0.8;
    h.app.brush_softness = 0.1;
    h.app.brush_width = 24.0;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let c = Pos2::new(c.x.round(), c.y.round() - 200.0);
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let hud = h.app.board_xf().w2s(p(-300.0, 150.0));
    pick_style_row(
        &mut h,
        hud,
        board_tip_hud::TipChoice::Texture(BrushTexture::Watercolor),
    );
    let first = [p(-150.0, 0.0), p(100.0, 0.0), p(100.0, 80.0), p(0.0, 80.0)];
    let second = [p(0.0, 80.0), p(0.0, -80.0)];
    let one: Vec<Pos2> = first.iter().chain(&second[1..]).copied().collect();
    press_drag_release_frames(&mut h, &dense(&one), egui::Modifiers::NONE, |_| {});
    let low = |w: &Pos2| *w + EVec2::new(0.0, 256.0);
    for leg in [&first[..], &second[..]] {
        let leg: Vec<Pos2> = leg.iter().map(low).collect();
        press_drag_release_frames(&mut h, &dense(&leg), egui::Modifiers::NONE, |_| {});
    }
    let nodes = &h.app.doc().scene.nodes;
    assert_eq!(nodes.len(), 3, "one crossing stroke and a pair");
    let (whole, a, b) = (
        stamp_at_one(&nodes[0]),
        stamp_at_one(&nodes[1]),
        stamp_at_one(&nodes[2]),
    );
    let mut built = 0;
    for w in around(p(0.0, 0.0), 3) {
        let one = stamp_px(&whole, w)[3] as i32;
        let lower = low(&w);
        let pair = vector_ink::over_px(stamp_px(&b, lower), stamp_px(&a, lower))[3] as i32;
        let single = stamp_px(&a, lower)[3] as i32;
        assert!(
            (one - pair).abs() <= 6,
            "at {w:?} the crossing paints {one}, two strokes paint {pair}"
        );
        built += (pair - single >= 25) as usize;
    }
    assert!(
        built >= 25,
        "two strokes do not build at the crossing ({built} px)"
    );
}

/// Review r18 R1: a rotated stroke whose rotated ink meets the view
/// paints, though its unrotated box is out of view, and a band over it
/// counts as in view only while its rotated ink box meets the view
/// (review r19 R1).
#[test]
fn a_rotated_stroke_partly_in_view_still_paints() {
    let (mut h, mut raster, id, _) = eraser_bar_board("rotated_stroke_partly_in_view");
    h.app.patch_nodes(&[id], |n| n.rotation_deg = 90.0);
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let upright = node.rect.rotated_bounds(node.rotation_deg);
    let (cx, cy) = upright.center();
    h.app.tab_mut().cam.offset = EVec2::new(cx, cy - 500.0);
    let view = h.app.board_paint_view(h.app.canvas_rect);
    let flat = node.rect.normalized();
    assert!(
        flat.y - 40.0 > view.y + view.h,
        "the bar's unrotated ink is out of view"
    );
    assert!(
        cy - 400.0 > view.y && cy - upright.h * 0.5 > view.y,
        "the upright bar's top end is in view"
    );
    assert!(
        board::paints_in_view(&node, &view),
        "the upright bar is culled"
    );
    settle_captured(&mut h, &mut raster, "the upright bar", |_| true);
    let r = redness(&raster, &h.app.board_xf(), Pos2::new(cx, cy - 400.0));
    assert!(r > 0.5, "the upright bar is blank in view ({r:.2})");
    let edge = upright.x + upright.w;
    let beside = |gap: f32| WorldRect::new(edge + gap, upright.y, 500.0, upright.h);
    assert!(
        board_path::band_in_view(&node, &beside(30.0)),
        "a band misses the upright bar's ink edge"
    );
    assert!(
        !board_path::band_in_view(&node, &beside(45.0)),
        "a band sees a view clear of the upright bar's ink"
    );
}

/// Review r14 finding 2 (Art. II) and note N1: a nested board portal's
/// strokes paint without cloning a scene node per frame.
#[test]
fn nested_board_strokes_clone_no_node_per_frame() {
    let (mut h, mut raster, id, _) = eraser_bar_board("eraser_nested_clones");
    let c = h.app.canvas_rect.center();
    nested_bar_portal(
        &mut h,
        &mut raster,
        id,
        (c + EVec2::new(150.0, -220.0), c + EVec2::new(410.0, -110.0)),
    );
    let clones = board_path::node_clones_on_this_thread();
    for _ in 0..10 {
        shot(&mut h, &mut raster, |_| {});
    }
    let cloned = board_path::node_clones_on_this_thread() - clones;
    assert_eq!(
        cloned, 0,
        "10 frames with a nested board cloned {cloned} nodes"
    );
}

/// Review r13 finding 2 (Art. II): while a deferred ink check waits for
/// its answer, the check itself neither hashes nor clones the stroke on
/// any frame. A waiting band's own validity check is separate.
#[test]
fn a_pending_ink_check_does_no_stroke_work_per_frame() {
    let (mut h, id, a, b) = erasable_zigzag("eraser_deferred_idle");
    h.app.brush_tiles.hold_inks = true;
    flick_eraser(&mut h, id, a, b, true);
    assert!(h.app.erase_settling(), "the check waits for its answer");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.brush_tiles.ink_landed(id) {
        assert!(
            std::time::Instant::now() < deadline,
            "the answer never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let work = board_color::settle_work_on_this_thread();
    for _ in 0..10 {
        h.frame();
    }
    let spent = board_color::settle_work_on_this_thread() - work;
    assert_eq!(
        spent, 0,
        "10 frames hashed or cloned the waiting stroke {spent} times"
    );
    h.app.brush_tiles.hold_inks = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.doc().scene.node(id).is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the emptied stroke stayed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
    }
}

#[test]
fn erasing_all_of_a_painted_stroke_removes_it() {
    let mut h = Harness::new("eraser_all");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 10.0;
    h.app.finish_freehand_brush(vec![Pos2::new(40.0, 40.0)]);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 60.0;
    h.app.board_drag = Some(h.app.begin_erase(Pos2::new(40.0, 40.0), false));
    let Some(board::BoardDrag::Erase {
        touched,
        points,
        spot,
        ..
    }) = h.app.board_drag.take()
    else {
        panic!("erase drag");
    };
    h.app.finish_erase(touched, points, spot);
    assert!(h.app.doc().scene.node(id).is_none());
}
