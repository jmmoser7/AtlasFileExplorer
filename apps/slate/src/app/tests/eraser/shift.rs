//! Eraser shift passes, spot erase, and frame-loop stamps.

use super::*;

/// Review r10 note: layer commands address marks by index. One Eraser pass
/// through frames crosses a Line mark at the bottom of the layer (removed)
/// and two brush marks above it (patched). Every edit lands as one undo
/// step, and one Ctrl+Z restores the layer.
#[test]
fn one_eraser_pass_over_a_layer_line_below_brush_marks_commits_every_edit() {
    let mut h = Harness::new("eraser_layer_order");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 400.0, 300.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    let select_image = |h: &mut Harness, tool: board::BoardTool| {
        h.app.board_sel = std::iter::once(image_id).collect();
        h.app.set_board_tool(tool);
        h.app.sync_image_paint_for_tool();
        h.frame();
        assert!(
            h.app.image_paint_session().is_some(),
            "{tool:?} paints on the image"
        );
    };
    select_image(&mut h, board::BoardTool::Line);
    let (a, b) = (Pos2::new(40.0, 150.0), Pos2::new(360.0, 150.0));
    assert!(h.app.line_begin(a, false));
    h.app.line_release(a, true, false);
    h.app.line_hover(b, false);
    h.app.line_begin(b, false);
    h.app.line_release(b, false, false);
    assert_eq!(
        layer_marks(&h, image_id).concat().len(),
        1,
        "the line is a layer mark"
    );
    select_image(&mut h, board::BoardTool::Brush);
    h.app.brush_width = 12.0;
    h.app.brush_opacity = 1.0;
    for x in [200.0, 260.0] {
        press_drag_release_frames(
            &mut h,
            &[Pos2::new(x, 60.0), Pos2::new(x, 150.0), Pos2::new(x, 240.0)],
            egui::Modifiers::NONE,
            |_| {},
        );
    }
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "every mark is on the layer"
    );
    let before = layer_marks(&h, image_id);
    assert_eq!(before.len(), 1);
    let before = before[0].clone();
    assert_eq!(before.len(), 3, "the line and two brush marks");
    let stamped =
        |n: &slate_doc::Node| matches!(&n.kind, NodeKind::Shape(s) if s.stroke.paints_as_stamp());
    assert!(!stamped(&before[0]) && stamped(&before[1]) && stamped(&before[2]));
    select_image(&mut h, board::BoardTool::Eraser);
    h.app.eraser_width = 30.0;
    h.app.eraser_opacity = 1.0;
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(150.0, 150.0),
            Pos2::new(230.0, 150.0),
            Pos2::new(300.0, 150.0),
        ],
        egui::Modifiers::NONE,
        |_| {},
    );
    let after = layer_marks(&h, image_id)[0].clone();
    assert!(
        !after.iter().any(|n| n.id == before[0].id),
        "the line is erased"
    );
    assert_eq!(after.len(), 2, "both brush marks keep ink");
    for (was, now) in before[1..].iter().zip(&after) {
        assert_eq!(was.id, now.id);
        assert_ne!(was, now, "brush mark {:?} took the pass", was.id);
    }
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        layer_marks(&h, image_id)[0],
        before,
        "one undo restores the layer"
    );
}

#[test]
fn the_eraser_spot_erases_painted_ink_and_keeps_the_stroke() {
    let mut h = Harness::new("eraser_spot");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 20.0;
    h.app
        .finish_freehand_brush(vec![Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0)]);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 30.0;
    h.app.board_drag = Some(h.app.begin_erase(Pos2::new(100.0, -20.0), false));
    h.app.update_erase(Pos2::new(100.0, 20.0));
    let Some(board::BoardDrag::Erase {
        touched,
        points,
        spot,
        ..
    }) = h.app.board_drag.take()
    else {
        panic!("erase drag");
    };
    assert!(touched.is_empty(), "painted strokes are not removed whole");
    assert_eq!(spot, vec![id]);
    h.app.finish_erase(touched, points, spot);
    let node = h.app.doc().scene.node(id).expect("stroke survives");
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    let path = shape.path.as_ref().unwrap();
    assert_eq!(path.erase.len(), 1);
    // The gap no longer picks; the ink either side still does.
    let z = h.app.tab().cam.z;
    assert!(!board_path::hit_path_node(node, shape, 100.0, 0.0, z));
    assert!(board_path::hit_path_node(node, shape, 20.0, 0.0, z));
    // One undo restores the ink.
    h.app.board_undo();
    let node = h.app.doc().scene.node(id).unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    assert!(shape.path.as_ref().unwrap().erase.is_empty());
}

/// "ran f4 erasor lock up interface on commit of comand": releasing an
/// eraser pass over a big painted stroke rasterizes nothing on the frame
/// loop. The pass's live preview stands in for the stroke until its new
/// raster lands from the workers, and the un-erased stroke never paints
/// again.
#[test]
fn an_eraser_release_on_a_big_stroke_stamps_nothing_on_the_frame_loop() {
    let mut h = line_board("eraser_commit_async");
    h.frame();
    let id = big_brush_bar(&mut h);
    settle_brush(&mut h, "the bar", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
    });
    let textures = h.app.brush_tiles.tile_textures();
    let unerased: Vec<egui::TextureId> = h
        .app
        .brush_tiles
        .tiles_with(id)
        .iter()
        .filter_map(|k| textures.get(k).copied())
        .collect();

    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 40.0;
    h.frame();
    let c = h.app.canvas_rect.center();
    let (top, bottom) = (c - EVec2::new(0.0, 90.0), c + EVec2::new(0.0, 90.0));
    h.frame_with(pointer_to(top, false));
    let press = board_path::stamps_on_this_thread();
    let line_texes = board_path::line_tex_allocs_on_this_thread();
    h.frame_with(primary_button(top, true, false));
    for i in 1..=9 {
        h.frame_with(pointer_to(top + (bottom - top) * (i as f32 / 9.0), false));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.erase_live.contains_key(&id) {
        assert!(std::time::Instant::now() < deadline, "no live preview");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame_with(pointer_to(bottom, false));
    }
    assert_eq!(
        board_path::stamps_on_this_thread(),
        press,
        "the pass built its preview on the frame loop"
    );
    let preview = h.app.erase_live[&id].texture();

    let released = h.frame_output(primary_button(bottom, false, false));
    let next = h.frame_output(|_| {});
    assert_eq!(
        board_path::stamps_on_this_thread(),
        press,
        "the release rasterized on the frame loop"
    );
    // Review r12 finding 6: only a straight pass needs a line texture.
    assert_eq!(
        board_path::line_tex_allocs_on_this_thread(),
        line_texes,
        "the freehand pass allocated a line texture"
    );
    let node = h.app.doc().scene.node(id).expect("the bar survives");
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    assert_eq!(
        shape.path.as_ref().unwrap().erase.len(),
        1,
        "pass committed"
    );
    for (when, out) in [("release", &released), ("next frame", &next)] {
        let painted = painted_textures(out);
        assert!(
            painted.contains(&preview),
            "{when}: the erased bar is not standing in"
        );
        assert!(
            !unerased.iter().any(|t| painted.contains(t)),
            "{when}: the un-erased bar flashed back"
        );
    }

    settle_brush(&mut h, "the erased bar", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
    });
    let settled = h.frame_output(|_| {});
    assert!(
        !painted_textures(&settled).contains(&preview),
        "the stand-in outlived the new tiles"
    );
    assert_eq!(
        board_path::stamps_on_this_thread(),
        press,
        "the new raster was built on the frame loop"
    );
}

/// Esc on a Shift eraser pass drops its cuts on the workers too, so no
/// stroke-sized raster waits in the pool for the next pass.
#[test]
fn escaping_an_eraser_shift_pass_drops_its_line_jobs() {
    let mut h = line_board("eraser_shift_escape");
    h.frame_with(|i| i.max_texture_side = Some(8192));
    h.app.tab_mut().cam.z = 1.5;
    h.frame();
    let id = big_brush_bar(&mut h);
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 207.0;
    h.frame();
    let shift = egui::Modifiers::SHIFT;
    let c = h.app.canvas_rect.center();
    let press = c + EVec2::new(-300.0, -250.0);
    let at = |h: &mut Harness, s: Pos2| {
        h.frame_with(|i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerMoved(s));
        });
    };
    at(&mut h, press);
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        });
    });
    at(&mut h, c + EVec2::new(-200.0, 150.0));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.erase_live.contains_key(&id) {
        assert!(std::time::Instant::now() < deadline, "no live preview");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame_with(|i| i.modifiers = shift);
    }
    at(&mut h, c + EVec2::new(100.0, 200.0));
    assert!(
        h.app.brush_tiles.lines_wanted_len() > 0,
        "a cut is on the workers"
    );
    press_key_with(&mut h, egui::Key::Escape, shift);
    assert!(h.app.board_drag.is_none(), "Esc ends the pass");
    let before = h.app.doc().scene.node(id).unwrap().clone();
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
    }
    assert_eq!(h.app.brush_tiles.lines_wanted_len(), 0);
    assert_eq!(h.app.brush_tiles.lines_landed_len(), 0);
    assert_eq!(
        h.app.doc().scene.node(id).unwrap(),
        &before,
        "nothing erased"
    );
}

/// The Eraser's Shift start is a world point of the document it erased;
/// another tab starts its pass at the press.
#[test]
fn eraser_shift_start_does_not_cross_tabs() {
    let mut h = brush_board("eraser_anchor_tabs");
    let anchor = Pos2::new(470.0, 60.0);
    h.app.eraser_anchor = Some((h.app.tab().id, anchor));
    let first = h.app.active_tab;
    let press = Pos2::new(10.0, 20.0);
    let start = |h: &mut Harness| match h.app.begin_erase(press, true) {
        board::BoardDrag::Erase { points, .. } => points[0],
        _ => panic!("erase drag"),
    };
    h.app.new_tab();
    h.frame();
    h.app.set_board_tool(board::BoardTool::Eraser);
    assert_eq!(start(&mut h), press, "another tab starts at the press");
    h.app.erase_live.clear();
    h.app.switch_tab(first);
    h.frame();
    assert_eq!(start(&mut h), anchor, "the erasing tab keeps its start");
}

/// Art. II at the user's eraser (207 wide, pencil, softness 0.09, 150 %,
/// 1.5 px/pt): a Shift pass across a big painted stroke stamps nothing on
/// the frame loop on its move frames, yet the preview follows the pointer
/// on every one; the exact cut lands from the workers, and the release
/// takes it in and commits the same erase mark as the pass.
#[test]
fn eraser_shift_move_frames_stamp_nothing_on_the_frame_loop() {
    let mut h = line_board("eraser_shift_budget");
    h.ctx.set_pixels_per_point(1.5);
    h.frame_with(|i| i.max_texture_side = Some(8192));
    h.app.tab_mut().cam.z = 1.5;
    h.frame();
    let id = big_brush_bar(&mut h);
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 207.0;
    h.app.eraser_softness = 0.09;
    h.app.eraser_texture = slate_doc::scene::BrushTexture::Pencil;
    h.frame();
    let shift = egui::Modifiers::SHIFT;
    let c = h.app.canvas_rect.center();
    let press = c + EVec2::new(-300.0, -250.0);
    let first = c + EVec2::new(-200.0, 150.0);
    let at = |h: &mut Harness, s: Pos2| {
        h.frame_with(|i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerMoved(s));
        });
    };
    at(&mut h, press);
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        });
    });
    at(&mut h, first);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.erase_live.contains_key(&id) {
        assert!(std::time::Instant::now() < deadline, "no live preview");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame_with(|i| i.modifiers = shift);
    }
    let from = h.app.board_xf().s2w(press);
    let (allocs, asks) = (
        board_path::line_tex_allocs_on_this_thread(),
        h.app.brush_tiles.line_tags_issued(),
    );
    let mut spent_per_move = Vec::new();
    for k in 1..=8 {
        let s = first + EVec2::new(40.0 * k as f32, 20.0 * k as f32);
        let before = board_path::stamp_px_on_this_thread();
        at(&mut h, s);
        spent_per_move.push(board_path::stamp_px_on_this_thread() - before);
        let end = board_snap::ortho_snap_point(from, h.app.board_xf().s2w(s));
        let shown = h.app.erase_live[&id]
            .live_line_end()
            .expect("the preview shows the pass");
        assert!(
            near_px(Pos2::new(shown[0], shown[1]), end),
            "move {k}: the preview shows {shown:?}, the pointer asks {end:?}"
        );
        assert_eq!(
            h.app.erase_band.painted(),
            1,
            "move {k}: the band stands in"
        );
    }
    assert!(
        spent_per_move.iter().all(|px| *px == 0),
        "move frames stamped {spent_per_move:?} px on the frame loop"
    );
    let mut waited = 0;
    while !h.app.erase_live[&id].line_exact() {
        waited += 1;
        assert!(waited < 400, "the exact cut never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame_with(|i| i.modifiers = shift);
    }
    h.frame_with(|i| i.modifiers = shift);
    assert_eq!(
        h.app.erase_band.painted(),
        0,
        "the exact cut replaces the band"
    );
    // Each landed cut goes into the preview's one line texture, allocated
    // with its first cut.
    assert!(
        h.app.brush_tiles.line_tags_issued() > asks,
        "the moves asked for no cut"
    );
    assert_eq!(
        board_path::line_tex_allocs_on_this_thread(),
        allocs,
        "a move or a landed cut allocated a texture"
    );
    let Some(board::BoardDrag::Erase { points, .. }) = &h.app.board_drag else {
        panic!("a straight erase pass");
    };
    let before_node = h.app.doc().scene.node(id).unwrap().clone();
    let tip = h.app.eraser_tip();
    let span = slate_doc::scene::StrokeSpan {
        width: tip.diameter,
        softness: tip.softness,
        color: slate_doc::scene::Rgba([0, 0, 0, tip.rgba[3]]),
        texture: h.app.eraser_texture,
    };
    let expected = board_color::with_erase_mark(&before_node, points, span);
    let last = first + EVec2::new(40.0 * 8.0, 20.0 * 8.0);
    let before = board_path::stamp_px_on_this_thread();
    let stamps = board_path::stamps_on_this_thread();
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: shift,
        });
    });
    h.frame();
    assert_eq!(
        board_path::stamp_px_on_this_thread(),
        before,
        "the release stamped"
    );
    assert_eq!(
        board_path::stamps_on_this_thread(),
        stamps,
        "the release rasterized"
    );
    let erase = |n: &slate_doc::Node| match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.path.as_ref().unwrap().erase.clone(),
        _ => panic!("a path"),
    };
    let node = h.app.doc().scene.node(id).expect("the bar survives");
    assert_eq!(erase(node), erase(&expected), "the committed erase mark");
}
