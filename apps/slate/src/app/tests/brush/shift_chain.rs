//! Chained brush-shift presses and live-canvas rebuilds.

use super::*;

/// Tab during a Pen Shift drag locks the segment direction; the release
/// commits on that ray and ends the lock.
#[test]
fn pen_tab_locks_the_shift_segment_direction() {
    let mut h = pen_board("pen_tab_lock");
    let a = Pos2::new(100.0, 100.0);
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerMoved(xf.w2s(a)));
    });
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerButton {
            pos: xf.w2s(a),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        });
    });
    let aim = Pos2::new(200.0, 200.0);
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerMoved(xf.w2s(aim)));
    });
    press_key_with(&mut h, egui::Key::Tab, shift);
    let lock = h.app.draft_lock.expect("Tab locks the Pen's Shift segment");
    let dir = EVec2::new(1.0, 1.0).normalized();
    assert!((lock - dir).length() < 1.0e-3, "{lock:?}");
    let off = Pos2::new(400.0, 120.0);
    h.frame_with(|i| {
        i.modifiers = egui::Modifiers::NONE;
        i.events.push(egui::Event::PointerMoved(xf.w2s(off)));
    });
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: xf.w2s(off),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
    });
    h.frame();
    let v = path_vertices(&h.app.doc().scene.nodes[0]);
    assert!(near_px(v[0], a) && on_ray(a, dir, v[1]), "{v:?}");
    assert!(h.app.draft_lock.is_none(), "the release ends the lock");
}

/// tip18 / tip19: after a stroke, Shift+drag previews from that stroke's
/// end and the release extends the same path.
#[test]
fn brush_shift_drag_after_a_stroke_continues_it() {
    let mut h = brush_board("brush_shift_drag_chain");
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
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let first = h.app.doc().scene.nodes[0].id;
    let from = h.app.brush_line_anchor().expect("anchor").pos;
    let end = Pos2::new(300.0, 200.0);
    press_drag_release_frames(
        &mut h,
        &[Pos2::new(200.0, 200.0), Pos2::new(250.0, 150.0), end],
        egui::Modifiers::SHIFT,
        |h| {
            assert!(h.app.brush_straight.is_some());
            assert!(h.app.brush_live.as_ref().is_some_and(|c| c.showing_line()));
        },
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the segment extends the stroke"
    );
    let v = path_vertices(h.app.doc().scene.node(first).unwrap());
    let end = board_snap::ortho_snap_point(from, end);
    assert!(
        near_px(v[v.len() - 2], from) && near_px(v[v.len() - 1], end),
        "{v:?}"
    );
}

/// tip18 with a big brush: a Shift press starts from the canvas the last
/// mark left (freehand or segment), so neither the first segment nor any
/// later one re-stamps the stroke on the frame loop, however long the chain
/// grows, and the earlier marks stay on screen while the chain is hidden
/// from the scene paint.
#[test]
fn chained_brush_shift_presses_do_not_restamp_the_chain() {
    let mut h = brush_board("brush_shift_chain_reuse");
    h.app.board_colors.fg.0 = [255, 40, 40, 255];
    h.app.brush_opacity = 1.0;
    h.app.brush_softness = 0.0;
    h.app.brush_width = 60.0;
    let shift = egui::Modifiers::SHIFT;
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    let xf = h.app.board_xf();
    let c = xf.s2w(h.app.canvas_rect.center());
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let lit = |raster: &FrameRaster, w: Pos2| {
        let s = xf.w2s(w);
        let p = raster.px[s.y as usize * raster.w + s.x as usize];
        p[0] > 0.6 && p[1] < 0.4
    };
    // Every frame feeds the raster: it only knows the texture uploads it saw.
    let button = |w: Pos2, pressed: bool, modifiers: egui::Modifiers| egui::Event::PointerButton {
        pos: xf.w2s(w),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    let drag = |h: &mut Harness, raster: &mut FrameRaster, mods: egui::Modifiers, pts: &[Pos2]| {
        let mut events = vec![
            vec![egui::Event::PointerMoved(xf.w2s(pts[0]))],
            vec![button(pts[0], true, mods)],
        ];
        events.extend(
            pts[1..]
                .iter()
                .map(|w| vec![egui::Event::PointerMoved(xf.w2s(*w))]),
        );
        for events in events {
            capture_frame(h, raster, |inp| {
                inp.modifiers = mods;
                inp.events = events;
            });
        }
        if mods.shift {
            assert!(h.app.brush_live.as_ref().is_some_and(|c| c.showing_line()));
        }
        let last = *pts.last().unwrap();
        capture_frame(h, raster, |inp| {
            inp.modifiers = mods;
            inp.events = vec![button(last, false, mods)];
        });
        capture_frame(h, raster, |inp| inp.modifiers = mods);
    };
    let freehand = [
        p(-300.0, -200.0),
        p(-200.0, -225.0),
        p(-100.0, -175.0),
        p(0.0, -200.0),
    ];
    drag(&mut h, &mut raster, egui::Modifiers::NONE, &freehand);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let id = h.app.doc().scene.nodes[0].id;
    let before = board_path::stamps_on_this_thread();
    let ends = [p(0.0, 100.0), p(300.0, 100.0), p(300.0, -150.0)];
    for end in ends {
        let press = end + EVec2::new(-60.0, 40.0);
        drag(
            &mut h,
            &mut raster,
            shift,
            &[press, press + (end - press) * 0.5, end],
        );
    }
    assert_eq!(
        board_path::stamps_on_this_thread(),
        before,
        "a Shift press re-stamped the stroke on the frame loop"
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "every segment extends the stroke"
    );
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert!(
        near_px(v[v.len() - 1], ends[2]) && near_px(v[v.len() - 4], freehand[3]),
        "{v:?}"
    );

    let (end, press) = (p(-100.0, -150.0), p(100.0, -60.0));
    for (i, events) in [
        vec![egui::Event::PointerMoved(xf.w2s(press))],
        vec![button(press, true, shift)],
        vec![egui::Event::PointerMoved(xf.w2s(end))],
    ]
    .into_iter()
    .enumerate()
    {
        let out = capture_frame(&mut h, &mut raster, |inp| {
            inp.modifiers = shift;
            inp.events = events;
        });
        if i == 2 {
            snapshot(&mut h, &mut raster, out, "shift-chain-reuse");
        }
    }
    assert!(h.app.brush_straight.is_some());
    // Clear of the Shift status label above the pointer.
    for w in [
        p(-260.0, -200.0),
        p(0.0, -50.0),
        p(150.0, 100.0),
        p(300.0, 0.0),
        p(100.0, -150.0),
    ] {
        assert!(lit(&raster, w), "{w:?} is dark while the chain grows");
    }
    assert_eq!(board_path::stamps_on_this_thread(), before);
}

/// tip18's reuse gate, negative side: undoing the segment changes the
/// stroke, so the next Shift press rebuilds the canvas from the stroke as
/// it now is and the undone segment is gone from the screen.
#[test]
fn brush_shift_after_undo_rebuilds_the_live_canvas() {
    let (mut h, mut raster, id, c) = shift_chain_board("tip18_gate_undo");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let z = egui::Event::Key {
        key: egui::Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    };
    capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = egui::Modifiers::CTRL;
        inp.events = vec![z];
    });
    capture_frame(&mut h, &mut raster, |_| {});
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert!(near_px(*v.last().unwrap(), p(0.0, -200.0)), "undone: {v:?}");
    let (stamps, resumes, asks) = (
        board_path::stamps_on_this_thread(),
        board_path::resumes_on_this_thread(),
        h.app.brush_tiles.line_tags_issued(),
    );
    let press = hold_shift_drag(
        &mut h,
        &mut raster,
        &[p(60.0, 140.0), p(200.0, 100.0), p(300.0, 100.0)],
    );
    assert_eq!(
        board_path::resumes_on_this_thread(),
        resumes,
        "resumed a stale canvas"
    );
    assert_eq!(
        press,
        (0, 0),
        "the rebuild stamped on the frame loop at the press"
    );
    assert_eq!(
        board_path::stamps_on_this_thread(),
        stamps,
        "the rebuild stamps the stroke as it now is on the raster workers"
    );
    assert!(
        canvas_covers(&h, id),
        "the canvas does not hold the stroke in the scene's place"
    );
    assert!(
        h.app.brush_tiles.line_tags_issued() >= asks + 2,
        "the workers were not asked for the stroke and the segment"
    );
    let xf = h.app.board_xf();
    assert!(
        red_at(&raster, &xf, p(-260.0, -200.0)),
        "the freehand stroke is missing"
    );
    for w in [p(0.0, -100.0), p(0.0, -50.0), p(0.0, 0.0)] {
        assert!(
            !red_at(&raster, &xf, w),
            "undone segment still on screen at {w:?}"
        );
    }
}

/// tip18's reuse gate, negative side: switching to Select drops the
/// anchor, so after moving the stroke the next Shift press starts fresh
/// and nothing of the stroke's old place stays on the live canvas.
#[test]
fn brush_shift_after_a_select_drag_rebuilds_the_live_canvas() {
    let (mut h, mut raster, id, c) = shift_chain_board("tip18_gate_select");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    h.app.set_board_tool(board::BoardTool::Select);
    raster_drag(
        &mut h,
        &mut raster,
        egui::Modifiers::NONE,
        &[p(0.0, -50.0), p(0.0, 0.0), p(0.0, 100.0), p(0.0, 200.0)],
        false,
    );
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert!(near_px(*v.last().unwrap(), p(0.0, 350.0)), "moved: {v:?}");
    h.app.set_board_tool(board::BoardTool::Brush);
    capture_frame(&mut h, &mut raster, |_| {});
    let resumes = board_path::resumes_on_this_thread();
    hold_shift_drag(
        &mut h,
        &mut raster,
        &[p(200.0, -100.0), p(300.0, -100.0), p(400.0, -100.0)],
    );
    assert_eq!(
        board_path::resumes_on_this_thread(),
        resumes,
        "resumed a stale canvas"
    );
    let xf = h.app.board_xf();
    assert!(
        red_at(&raster, &xf, p(300.0, -100.0)),
        "the new segment is missing"
    );
    for w in [p(0.0, -150.0), p(0.0, -50.0), p(-260.0, -200.0)] {
        assert!(
            !red_at(&raster, &xf, w),
            "the stroke's old place still on screen at {w:?}"
        );
    }
}

/// tip18's reuse gate, negative side: a wheel zoom in and back out at
/// another pointer leaves a different camera, so the next Shift press
/// rebuilds the canvas at that camera and the chain draws where it is now.
#[test]
fn brush_shift_after_zooming_away_and_back_rebuilds_the_live_canvas() {
    let (mut h, mut raster, id, c) = shift_chain_board("tip18_gate_zoom");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let before = h.app.board_xf();
    let z = h.app.tab().cam.z;
    for (at, dy) in [
        (Pos2::new(300.0, 450.0), 120.0),
        (Pos2::new(1100.0, 450.0), -120.0),
    ] {
        capture_frame(&mut h, &mut raster, |inp| {
            inp.events = vec![
                egui::Event::PointerMoved(at),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: EVec2::new(0.0, dy),
                    modifiers: egui::Modifiers::NONE,
                },
            ];
        });
        capture_frame(&mut h, &mut raster, |_| {});
    }
    let xf = h.app.board_xf();
    assert!((h.app.tab().cam.z - z).abs() < z * 0.01, "zoomed back");
    let old = before.w2s(p(0.0, -50.0));
    assert!(
        (xf.w2s(p(0.0, -50.0)) - old).length() > 150.0,
        "the camera must end somewhere else"
    );
    let (stamps, resumes, asks) = (
        board_path::stamps_on_this_thread(),
        board_path::resumes_on_this_thread(),
        h.app.brush_tiles.line_tags_issued(),
    );
    let press = hold_shift_drag(
        &mut h,
        &mut raster,
        &[p(60.0, 140.0), p(200.0, 100.0), p(300.0, 100.0)],
    );
    assert_eq!(
        board_path::resumes_on_this_thread(),
        resumes,
        "resumed a stale canvas"
    );
    assert_eq!(
        press,
        (0, 0),
        "the rebuild stamped on the frame loop at the press"
    );
    assert_eq!(
        board_path::stamps_on_this_thread(),
        stamps,
        "the rebuild stamps the chain at the new camera on the raster workers"
    );
    assert!(
        canvas_covers(&h, id),
        "the canvas does not hold the chain in the scene's place"
    );
    assert!(
        h.app.brush_tiles.line_tags_issued() >= asks + 2,
        "the workers were not asked for the chain and the segment"
    );
    let xf = h.app.board_xf();
    for w in [p(-260.0, -200.0), p(0.0, -50.0), p(200.0, 100.0)] {
        assert!(red_at(&raster, &xf, w), "{w:?} is dark after the zoom");
    }
    let stale = xf.s2w(old);
    assert!(
        !red_at(&raster, &xf, stale),
        "the segment is still drawn at its old screen place"
    );
}

/// A brush texture picked in the size HUD's style row is tool state: the
/// chain is unchanged, so the next Shift press may resume the parked
/// canvas, the chain stays as stamped, and the new segment's end tip takes
/// the new texture.
#[test]
fn brush_shift_after_a_style_row_texture_change_keeps_the_chain() {
    use board_tip_hud::{palette_band_y, palette_slot};
    let (mut h, mut raster, id, c) = shift_chain_board("tip18_gate_texture");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    h.app.alt_down = true;
    let press = Pos2::new(700.0, 300.0);
    assert!(h.app.drive_brush_hud(Some(press), true, true));
    let r = h.app.active_tip().0 * 0.5;
    let n = h.app.tip_choices().len();
    let pencil = (0..n)
        .find(|i| {
            h.app.tip_choices()[*i]
                == board_tip_hud::TipChoice::Texture(slate_doc::scene::BrushTexture::Pencil)
        })
        .expect("a pencil slot");
    let band = palette_band_y(press, r);
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(press.x, band + 1.0)), true, false));
    let slot = palette_slot(press, r, pencil, n);
    assert!(h.app.drive_brush_hud(Some(slot), true, false));
    assert!(h.app.drive_brush_hud(Some(slot), false, false));
    h.app.alt_down = false;
    assert_eq!(h.app.brush_texture, slate_doc::scene::BrushTexture::Pencil);
    assert_eq!(
        board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap()),
        key,
        "the style row changed the committed stroke"
    );
    capture_frame(&mut h, &mut raster, |_| {});
    let (stamps, resumes) = (
        board_path::stamps_on_this_thread(),
        board_path::resumes_on_this_thread(),
    );
    hold_shift_drag(
        &mut h,
        &mut raster,
        &[p(60.0, 140.0), p(200.0, 100.0), p(300.0, 100.0)],
    );
    assert_eq!(board_path::resumes_on_this_thread(), resumes + 1);
    assert_eq!(board_path::stamps_on_this_thread(), stamps);
    let xf = h.app.board_xf();
    for w in [p(-260.0, -200.0), p(0.0, -50.0)] {
        assert!(red_at(&raster, &xf, w), "{w:?} lost its stamp");
    }
    let end = xf.w2s(p(300.0, 100.0));
    capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = egui::Modifiers::SHIFT;
        inp.events = vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::SHIFT,
        }];
    });
    let node = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Shape(s) = &node.kind else {
        panic!("a shape")
    };
    let tips = &s.path.as_ref().unwrap().tips;
    assert_eq!(
        tips.last().map(|t| t.texture),
        Some(slate_doc::scene::BrushTexture::Pencil),
        "the new segment ends in the picked texture"
    );
    assert_eq!(
        tips[tips.len() - 2].texture,
        slate_doc::scene::BrushTexture::Smooth,
        "the chain keeps its texture"
    );
}
