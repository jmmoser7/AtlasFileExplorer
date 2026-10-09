//! Eraser passes over curves, pencil strokes, and image paint.

use super::*;

/// Eraser: a Shift pass takes 45° steps; Tab locks its direction; the
/// end of the pass clears the lock.
#[test]
fn eraser_shift_pass_takes_45_degree_steps_and_tab_locks_it() {
    let mut h = brush_board("eraser_shift_45");
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.shift_down = true;
    let a = Pos2::new(0.0, 0.0);
    h.app.board_drag = Some(h.app.begin_erase(a, true));
    h.app.update_erase(Pos2::new(100.0, 37.0));
    let last = |h: &Harness| match &h.app.board_drag {
        Some(board::BoardDrag::Erase { points, .. }) => *points.last().unwrap(),
        _ => panic!("erase drag"),
    };
    assert!(on_45(a, last(&h)), "{:?}", last(&h));
    h.app.shift_down = false;
    h.app.update_erase(Pos2::new(30.0, 40.0));
    assert!(
        near(last(&h), Pos2::new(30.0, 40.0)),
        "Shift released: free"
    );
    assert!(h.app.toggle_segment_lock(Some(Pos2::new(30.0, 40.0))));
    h.app.update_erase(Pos2::new(500.0, -20.0));
    assert!(on_ray(a, EVec2::new(0.6, 0.8), last(&h)), "{:?}", last(&h));
    assert!(h.app.toggle_segment_lock(None));
    assert!(h.app.draft_lock.is_none(), "Tab again releases");
    assert!(h.app.toggle_segment_lock(Some(Pos2::new(30.0, 40.0))));
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
    assert!(
        h.app.draft_lock.is_none(),
        "the end of the pass clears the lock"
    );
}

/// The same Eraser Shift pass and Tab lock, driven through real frames:
/// Shift+press, drag, let go of Shift, Tab, Tab again, Tab, release.
#[test]
fn eraser_shift_pass_and_tab_lock_through_frames() {
    let mut h = brush_board("eraser_shift_45_frames");
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.frame();
    let xf = h.app.board_xf();
    let (shift, none) = (egui::Modifiers::SHIFT, egui::Modifiers::NONE);
    let move_to = |h: &mut Harness, w: Pos2, m: egui::Modifiers| {
        h.frame_with(|i| {
            i.modifiers = m;
            i.events.push(egui::Event::PointerMoved(xf.w2s(w)));
        });
    };
    let button = |h: &mut Harness, w: Pos2, m: egui::Modifiers, pressed: bool| {
        h.frame_with(|i| {
            i.modifiers = m;
            i.events.push(egui::Event::PointerButton {
                pos: xf.w2s(w),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: m,
            });
        });
    };
    let last = |h: &Harness| match &h.app.board_drag {
        Some(board::BoardDrag::Erase {
            points,
            straight: true,
            ..
        }) => *points.last().unwrap(),
        _ => panic!("a straight erase pass"),
    };
    let a = Pos2::new(100.0, 100.0);
    move_to(&mut h, a, shift);
    button(&mut h, a, shift, true);
    move_to(&mut h, Pos2::new(200.0, 137.0), shift);
    assert!(on_45(a, last(&h)), "Shift: 45° steps, {:?}", last(&h));
    let free = Pos2::new(130.0, 140.0);
    move_to(&mut h, free, none);
    assert!(
        near_px(last(&h), free),
        "Shift released: free, {:?}",
        last(&h)
    );
    press_key_with(&mut h, egui::Key::Tab, none);
    let dir = EVec2::new(0.6, 0.8);
    let lock = h.app.draft_lock.expect("Tab locks the pass");
    assert!((lock - dir).length() < 1.0e-3, "{lock:?}");
    move_to(&mut h, Pos2::new(600.0, 80.0), none);
    assert!(on_ray(a, dir, last(&h)), "{:?} stays on the ray", last(&h));
    press_key_with(&mut h, egui::Key::Tab, none);
    assert!(h.app.draft_lock.is_none(), "Tab again releases");
    press_key_with(&mut h, egui::Key::Tab, none);
    assert!(h.app.draft_lock.is_some(), "Tab locks again");
    button(&mut h, Pos2::new(600.0, 80.0), none, false);
    h.frame();
    assert!(h.app.board_drag.is_none(), "the release ends the pass");
    assert!(
        h.app.draft_lock.is_none(),
        "the end of the pass clears the lock"
    );
}

/// tx1 on the board: the eraser shows the vector curves it crosses at 30 %
/// opacity while the drag lasts, and the release removes each one whole;
/// one Ctrl+Z restores it.
#[test]
fn the_eraser_dims_crossed_curves_and_removes_them_whole() {
    let mut h = brush_board("tx1_board_curve");
    h.app.set_board_tool(board::BoardTool::Pen);
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    h.app
        .finish_freehand_pen(vec![p(-300.0, 0.0), p(300.0, 0.0)]);
    let id = h.app.doc().scene.nodes.last().expect("the curve").id;
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &before.kind else {
        panic!("a shape")
    };
    assert!(!s.stroke.paints_as_stamp(), "a vector curve");
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 40.0;
    let mut raster = FrameRaster::new(1440, 900);
    shot(&mut h, &mut raster, |_| {});
    let xf = h.app.board_xf();
    // The brightest pixel within two screen rows of `w`.
    let bright = |raster: &FrameRaster, w: Pos2| {
        let s = xf.w2s(w);
        (-2..=2)
            .map(|dy| {
                let px = raster.px[(s.y as i64 + dy) as usize * raster.w + s.x as usize];
                (px[0] + px[1] + px[2]) / 3.0
            })
            .fold(0.0, f32::max)
    };
    let (on, off) = (p(-200.0, 0.0), p(-200.0, 60.0));
    let lit = bright(&raster, on) - bright(&raster, off);
    assert!(lit > 0.4, "the curve paints: {lit}");
    let depth = h.app.tab().journal.undo_depth();
    let pass = [p(0.0, -120.0), p(0.0, -40.0), p(0.0, 40.0), p(0.0, 120.0)];
    let s: Vec<Pos2> = pass.iter().map(|w| xf.w2s(*w)).collect();
    shot(&mut h, &mut raster, pointer_to(s[0], false));
    shot(&mut h, &mut raster, primary_button(s[0], true, false));
    for at in &s[1..] {
        shot(&mut h, &mut raster, pointer_to(*at, false));
    }
    let dim = (bright(&raster, on) - bright(&raster, off)) / lit;
    assert!(
        (dim - 0.3).abs() < 0.06,
        "the crossed curve shows at {dim:.2} of its ink"
    );
    assert!(
        h.app.doc().scene.node(id).is_some(),
        "nothing is removed before release"
    );
    shot(&mut h, &mut raster, primary_button(s[3], false, false));
    shot(&mut h, &mut raster, |_| {});
    assert!(
        h.app.doc().scene.node(id).is_none(),
        "the release removes the curve whole"
    );
    assert!(
        (bright(&raster, on) - bright(&raster, off)).abs() < 0.05,
        "the removed curve still paints"
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the pass is one undo step"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before),
        "one undo restores the curve"
    );
}

/// r7-10 (user, 28 September 2026): "a full erase removes the stroke";
/// ink remaining is measured on the untextured tip coverage, so the paper
/// grain a Pencil eraser leaves never keeps a stroke alive. A freehand
/// pass that covers the whole stroke, released once its preview shows,
/// removes it in the pass's undo step.
#[test]
fn a_full_pencil_eraser_pass_removes_the_stroke() {
    let (mut h, id, c) = pencil_eraser_board("r7_10_pencil_full_erase");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    let pass: Vec<Pos2> = dense(&[p(-140.0, 0.0), p(140.0, 0.0)])
        .iter()
        .map(|w| xf.w2s(*w))
        .collect();
    h.frame_with(pointer_to(pass[0], false));
    h.frame_with(primary_button(pass[0], true, false));
    for s in &pass[1..] {
        h.frame_with(pointer_to(*s, false));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.erase_live.contains_key(&id) {
        assert!(std::time::Instant::now() < deadline, "no live preview");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
    }
    h.frame_with(primary_button(*pass.last().unwrap(), false, false));
    settle_erase(&mut h);
    assert!(
        h.app.doc().scene.node(id).is_none(),
        "the fully erased stroke stayed: grain residue kept it"
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the pass is one undo step"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before),
        "one undo restores the stroke"
    );
}

/// r7-10 past the frame's raster budget: the user's Pencil eraser over
/// the big zigzag leaves it no untextured ink, so once the workers check,
/// the stroke leaves in the pass's own undo step, nothing stamped on the
/// frame loop (tx3, r7-3..r7-7 keep the off-frame path).
#[test]
fn a_deferred_pencil_eraser_pass_that_empties_a_big_stroke_removes_it() {
    use slate_doc::scene::BrushTexture;
    let (mut h, id, a, b) = erasable_zigzag("r7_10_pencil_deferred");
    let hud = Pos2::new(a.x + 200.0, a.y + 300.0);
    h.app.eraser_width = 20.0;
    pick_style_row(
        &mut h,
        hud,
        board_tip_hud::TipChoice::Texture(BrushTexture::Pencil),
    );
    h.app.eraser_width = 207.0;
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let counted = flick_eraser(&mut h, id, a, b, false);
    assert_emptied_stroke_leaves_with_its_pass(&mut h, id, &before, depth, counted);
}

/// A Shift release that cannot extend its anchor (an undo moved the chain's
/// end during the drag) adds a new stroke. The live canvas still holds the
/// anchor as it was, so it stands in for neither stroke.
#[test]
fn a_shift_release_that_adds_a_stroke_beside_the_anchor_holds_no_stand_in() {
    let (mut h, mut raster, id, c) = shift_chain_board("r12_slop_hold");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    hold_shift_drag(
        &mut h,
        &mut raster,
        &[p(60.0, 140.0), p(200.0, 100.0), p(300.0, 100.0)],
    );
    assert!(h
        .app
        .brush_live
        .as_ref()
        .is_some_and(|c| c.anchor == Some(id)));
    h.app.board_undo();
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert!(near_px(*v.last().unwrap(), p(0.0, -200.0)), "undone: {v:?}");
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = shift;
        inp.events = vec![egui::Event::PointerButton {
            pos: xf.w2s(p(300.0, 100.0)),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: shift,
        }];
    });
    let nodes = &h.app.doc().scene.nodes;
    assert_eq!(nodes.len(), 2, "the release added a stroke");
    let added = nodes.iter().find(|n| n.id != id).unwrap();
    let key = board_path::node_stamp_key(added);
    let canvas = h.app.brush_live.as_ref().expect("live canvas");
    assert!(canvas.held.is_none(), "the canvas holds {:?}", canvas.held);
    assert!(
        !canvas.stands_in(added.id, key),
        "the canvas stands in for the added stroke"
    );
}

#[test]
fn image_paint_eraser_spot_hits_layer_strokes() {
    let mut h = Harness::new("image_paint_eraser");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.sync_image_paint_for_tool();
    h.app
        .finish_freehand_brush(vec![Pos2::new(50.0, 50.0), Pos2::new(150.0, 50.0)]);
    let stroke_id = h.app.doc().scene.node(image_id).unwrap();
    let slate_doc::scene::NodeKind::Image(img) = &stroke_id.kind else {
        panic!("image");
    };
    let stroke_id = img.paint_layers[0].nodes[0].id;
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.sync_image_paint_for_tool();
    h.app.eraser_width = 30.0;
    h.app.board_drag = Some(h.app.begin_erase(Pos2::new(100.0, 50.0), false));
    h.app.update_erase(Pos2::new(100.0, 50.0));
    let Some(board::BoardDrag::Erase { spot, .. }) = h.app.board_drag.take() else {
        panic!("erase drag");
    };
    assert!(spot.contains(&stroke_id));
}

#[test]
fn image_paint_eraser_removes_vector_stroke_and_undo_restores_it() {
    let mut h = Harness::new("image_paint_vector_eraser");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.sync_image_paint_for_tool();
    h.app
        .finish_freehand_pen(vec![Pos2::new(40.0, 50.0), Pos2::new(160.0, 50.0)]);
    let stroke_id = {
        let host = h.app.doc().scene.node(image_id).unwrap();
        let slate_doc::scene::NodeKind::Image(img) = &host.kind else {
            panic!("image");
        };
        img.paint_layers[0].nodes[0].id
    };
    h.app
        .finish_erase(vec![stroke_id], vec![Pos2::new(100.0, 50.0)], Vec::new());
    assert!(slate_doc::image_paint::find_layer_node(&h.app.doc().scene, stroke_id).is_none());
    h.app.board_undo();
    assert!(slate_doc::image_paint::find_layer_node(&h.app.doc().scene, stroke_id).is_some());
}
