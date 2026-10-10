//! Shift release before the anchor lands, and layer marks.

use super::*;

/// D11: committing a stroke never blanks earlier strokes. A Shift segment
/// released while the rebuilt canvas still waits for the anchor stroke's
/// stamp keeps the chain and the new segment on screen, with no stretched
/// copy of the old bitmap over the grown bounds, and nothing stamps on the
/// frame loop.
#[test]
fn brush_shift_released_before_the_anchor_lands_keeps_the_segment() {
    release_before_the_anchor_lands("r12_release_before_anchor", true);
}

/// [`brush_shift_released_before_the_anchor_lands_keeps_the_segment`] with
/// the stroke on its own bitmap instead of the tiles: the old bitmap shows
/// where it was, not mapped onto the extended stroke's rect.
#[test]
fn brush_shift_released_before_the_anchor_lands_does_not_stretch_the_old_bitmap() {
    release_before_the_anchor_lands("r12_release_before_anchor_untiled", false);
}

/// D11: a canvas whose line jobs keep getting lost gives up. It drops the
/// anchor stroke it already holds, so the scene's copy paints alone at the
/// brush's opacity, and it is never resumed: the next Shift press rebuilds
/// it and the chain stays on screen.
#[test]
fn a_canvas_that_gives_up_paints_the_chain_once_and_is_not_resumed() {
    let (mut h, mut raster, id, c) = translucent_chain_board("r12_give_up");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    let button = |w: Pos2, pressed: bool| egui::Event::PointerButton {
        pos: xf.w2s(w),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: shift,
    };
    let press = |h: &mut Harness, raster: &mut FrameRaster| {
        capture_frame(h, raster, |inp| {
            inp.modifiers = shift;
            inp.events = vec![egui::Event::PointerMoved(xf.w2s(p(100.0, -100.0)))];
        });
        capture_frame(h, raster, |inp| {
            inp.modifiers = shift;
            inp.events = vec![button(p(100.0, -100.0), true)];
        })
    };
    press(&mut h, &mut raster);
    wait_brush_live(&mut h, &mut raster, shift, |c| {
        c.holds_anchor(id) && c.line_exact()
    });
    h.app.brush_live.as_mut().unwrap().panic_always = true;
    capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = shift;
        inp.events = vec![egui::Event::PointerMoved(xf.w2s(p(250.0, -200.0)))];
    });
    wait_brush_live(&mut h, &mut raster, shift, |c| c.given_up());
    let out = capture_frame(&mut h, &mut raster, |inp| inp.modifiers = shift);
    draw_now(&h, &mut raster, out);
    let bg = {
        let s = xf.w2s(p(0.0, 150.0));
        raster.px[s.y as usize * raster.w + s.x as usize][0]
    };
    for w in [p(-250.0, -200.0), p(-150.0, -200.0)] {
        let a = red_alpha(&raster, &xf, w, bg);
        assert!(
            (a - 0.5).abs() < 0.06,
            "{w:?}: the chain's body is {a} after the give-up"
        );
    }
    capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = shift;
        inp.events = vec![button(p(250.0, -200.0), false)];
    });
    capture_frame(&mut h, &mut raster, |_| {});
    capture_frame(&mut h, &mut raster, |_| {});
    h.app.brush_live.as_mut().unwrap().panic_always = false;
    let resumes = board_path::resumes_on_this_thread();
    let out = press(&mut h, &mut raster);
    assert_eq!(
        board_path::resumes_on_this_thread(),
        resumes,
        "resumed a canvas that gave up"
    );
    draw_now(&h, &mut raster, out);
    for w in [p(-250.0, -200.0), p(-150.0, -200.0)] {
        let a = red_alpha(&raster, &xf, w, bg);
        assert!(
            a > 0.4,
            "{w:?}: the chain's body is {a} on the next press frame"
        );
    }
}

/// tip18 with a stroke selected under the press: its body, corner, and edge
/// belong to the Shift drag, which previews every frame and extends the
/// last mark.
#[test]
fn brush_shift_drag_over_a_selected_stroke_still_previews() {
    let mut h = brush_board("tip18_sel");
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(40.0, 40.0),
            Pos2::new(80.0, 60.0),
            Pos2::new(120.0, 40.0),
        ],
        egui::Modifiers::NONE,
        |_| {},
    );
    let first = h.app.doc().scene.nodes[0].id;
    h.app.board_sel = std::iter::once(first).collect();
    h.frame();
    let rect = h.app.doc().scene.node(first).unwrap().rect;
    for start in [
        Pos2::new(80.0, 60.0),
        Pos2::new(rect.x + rect.w, rect.y + rect.h),
        Pos2::new(rect.x + rect.w * 0.5, rect.y),
    ] {
        let before = path_vertices(h.app.doc().scene.node(first).unwrap()).len();
        press_drag_release_frames(
            &mut h,
            &[
                start,
                start + EVec2::new(60.0, 80.0),
                start + EVec2::new(140.0, 160.0),
            ],
            egui::Modifiers::SHIFT,
            |h| {
                assert!(h.app.brush_straight.is_some(), "press at {start:?}");
                assert!(h.app.brush_live.as_ref().is_some_and(|c| c.showing_line()));
            },
        );
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        let after = path_vertices(h.app.doc().scene.node(first).unwrap()).len();
        assert_eq!(after, before + 1, "press at {start:?}");
    }
}

/// Review r9 finding 8: once the image paint session ends (the image is
/// deselected, Brush still armed), a Shift drag never writes into the
/// image's layer mark; it paints a new mark on the board.
#[test]
fn brush_shift_after_the_paint_session_ends_leaves_the_layer_mark() {
    let (mut h, image_id) = image_paint_board(
        "tip18_img_ended",
        slate_doc::scene::WorldRect::new(0.0, 0.0, 400.0, 300.0),
        0.0,
        &[
            Pos2::new(40.0, 40.0),
            Pos2::new(80.0, 60.0),
            Pos2::new(120.0, 40.0),
        ],
    );
    let drawn = layer_marks(&h, image_id);
    assert_eq!(drawn.iter().map(Vec::len).sum::<usize>(), 1);
    h.app.board_sel.clear();
    h.frame();
    assert!(h.app.image_paint_session().is_none(), "the session ended");
    assert_eq!(h.app.board_tool, board::BoardTool::Brush);
    // Current rule (review r10 Q1): the segment still starts at the layer
    // mark's end, as a new board node.
    let host = h.app.doc().scene.node(image_id).unwrap();
    let NodeKind::Image(img) = &host.kind else {
        panic!("the image")
    };
    let world = slate_doc::image_paint::layer_node_to_world(host, img, &drawn[0][0]);
    let end = *path_vertices(&world).last().unwrap();
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(200.0, 200.0),
            Pos2::new(250.0, 150.0),
            Pos2::new(300.0, 250.0),
        ],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, ..) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(
                near_px(from, end),
                "starts at {from:?}, the layer mark ends at {end:?}"
            );
        },
    );
    assert_eq!(
        layer_marks(&h, image_id),
        drawn,
        "the layer mark is unchanged"
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        2,
        "the segment is a board node"
    );
    let v = path_vertices(&h.app.doc().scene.nodes[1]);
    assert!(
        near_px(v[0], end),
        "the board node starts at {:?}, not {end:?}",
        v[0]
    );
}

/// Review r9 finding 8, layer variant: after the palette's `+` makes a new
/// layer active, a Shift drag paints a new mark there and leaves the mark
/// on the previous layer alone.
#[test]
fn brush_shift_after_switching_layers_leaves_the_other_layer() {
    let (mut h, image_id) = image_paint_board(
        "tip18_img_layer",
        slate_doc::scene::WorldRect::new(0.0, 0.0, 400.0, 300.0),
        0.0,
        &[
            Pos2::new(40.0, 40.0),
            Pos2::new(80.0, 60.0),
            Pos2::new(120.0, 40.0),
        ],
    );
    let drawn = layer_marks(&h, image_id)[0].clone();
    h.app.on_image_paint_add_clicked(image_id);
    h.frame();
    assert_eq!(h.app.image_paint_session().map(|s| s.layer_index), Some(1));
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(200.0, 200.0),
            Pos2::new(250.0, 150.0),
            Pos2::new(300.0, 250.0),
        ],
        egui::Modifiers::SHIFT,
        |_| {},
    );
    let marks = layer_marks(&h, image_id);
    assert_eq!(marks[0], drawn, "the first layer's mark is unchanged");
    assert_eq!(
        marks[1].len(),
        1,
        "the segment is a new mark on the active layer"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "and not on the board");
}
