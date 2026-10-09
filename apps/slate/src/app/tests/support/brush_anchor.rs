//! Release a shift segment before its anchor raster lands.

use super::*;

pub(crate) fn release_before_the_anchor_lands(tag: &str, tiled: bool) {
    let (mut h, mut raster, c) = big_brush_raster_board(tag);
    h.app.brush_tiles_enabled = tiled;
    wheel_notch(&mut h, &mut raster);
    if !tiled {
        // The stroke's own bitmap at this zoom, with the camera at rest.
        for i in 0..400 {
            let before = h.app.board_xf();
            std::thread::sleep(std::time::Duration::from_millis(5));
            capture_frame(&mut h, &mut raster, |_| {});
            let still = h.app.board_xf().w2s(c) == before.w2s(c) && before.z == h.app.board_xf().z;
            if still && h.app.brush_stamps.values().any(|(_, g)| g.exact) {
                break;
            }
            assert!(i < 399, "the stroke's bitmap never landed");
        }
    }
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let xf = h.app.board_xf();
    let id = h.app.doc().scene.nodes[0].id;
    h.app
        .brush_live
        .as_mut()
        .expect("parked canvas")
        .hold_anchor = true;
    let shift = egui::Modifiers::SHIFT;
    let (press, to) = (xf.w2s(p(-200.0, 100.0)), xf.w2s(p(300.0, 100.0)));
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: shift,
    };
    let stamps = (
        board_path::stamps_on_this_thread(),
        board_path::stamp_px_on_this_thread(),
    );
    for events in [
        vec![egui::Event::PointerMoved(press), button(press, true)],
        vec![egui::Event::PointerMoved(to)],
    ] {
        capture_frame(&mut h, &mut raster, |inp| {
            inp.modifiers = shift;
            inp.events = events;
        });
    }
    assert!(
        !h.app.brush_live.as_ref().expect("live canvas").settled(),
        "the anchor landed before the release"
    );
    // The old stroke's body, the segment's midpoint, and where a copy of
    // the old bitmap stretched onto the grown rect puts the old end.
    let lit = [p(-225.0, -175.0), p(-75.0, -175.0), p(150.0, -50.0)];
    let dark = [p(300.0, -200.0), p(0.0, 100.0)];
    let check = |h: &Harness, raster: &mut FrameRaster, out: egui::FullOutput, when: &str| {
        draw_now(h, raster, out);
        for w in lit {
            assert!(red_around(raster, &xf, w), "{when}: {w:?} is dark");
        }
        for w in dark {
            assert!(
                !red_around(raster, &xf, w),
                "{when}: {w:?} is lit (a stretched copy)"
            );
        }
    };
    let out = capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = shift;
        inp.events = vec![button(to, false)];
    });
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert!(
        near_px(*v.last().unwrap(), p(300.0, 100.0)),
        "the segment extends the stroke: {v:?}"
    );
    check(&h, &mut raster, out, "release frame");
    for i in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let out = capture_frame(&mut h, &mut raster, |_| {});
        check(&h, &mut raster, out, &format!("anchor held, frame {i}"));
    }
    h.app.brush_live.as_mut().unwrap().hold_anchor = false;
    let mut frames = 0;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let out = capture_frame(&mut h, &mut raster, |_| {});
        check(
            &h,
            &mut raster,
            out,
            &format!("after the hold, frame {frames}"),
        );
        let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
        let landed = if tiled {
            h.app.brush_tiles.last.pending_jobs == 0
        } else {
            stamp_of(&h, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
        };
        let settled = landed && h.app.brush_live.as_ref().is_some_and(|c| !c.asking());
        if settled {
            break;
        }
        frames += 1;
        assert!(frames < 400, "the board never settled");
    }
    let out = capture_frame(&mut h, &mut raster, |_| {});
    check(&h, &mut raster, out, "settled");
    assert_eq!(
        (
            board_path::stamps_on_this_thread(),
            board_path::stamp_px_on_this_thread()
        ),
        stamps,
        "the release stamped on the frame loop"
    );
}

/// Coverage of the red brush over the black board, averaged over the 5 × 5
/// points around world point `w`, estimated from red over `bg`.
pub(crate) fn red_alpha(raster: &FrameRaster, xf: &board::BoardXf, w: Pos2, bg: f32) -> f32 {
    let s = xf.w2s(w);
    let mut r = 0.0;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (x, y) = ((s.x as i64 + dx) as usize, (s.y as i64 + dy) as usize);
            r += raster.px[y * raster.w + x][0];
        }
    }
    (r / 25.0 - bg) / (1.0 - bg)
}

/// A 60 px smooth red brush at 50 % opacity with a freehand stroke from
/// `c + (-300, -200)` to `c + (0, -200)`, settled, then one wheel notch so
/// the next Shift press rebuilds the live canvas.
pub(crate) fn translucent_chain_board(tag: &str) -> (Harness, FrameRaster, NodeId, Pos2) {
    let mut h = brush_board(tag);
    h.app.board_colors.fg.0 = [255, 40, 40, 255];
    h.app.brush_opacity = 0.5;
    h.app.brush_softness = 0.0;
    h.app.brush_width = 60.0;
    h.app.brush_texture = slate_doc::scene::BrushTexture::Smooth;
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let freehand = [
        p(-300.0, -200.0),
        p(-200.0, -200.0),
        p(-100.0, -200.0),
        p(0.0, -200.0),
    ];
    raster_drag(&mut h, &mut raster, egui::Modifiers::NONE, &freehand, false);
    settle_brush_live(&mut h, &mut raster);
    let id = h.app.doc().scene.nodes[0].id;
    wheel_notch(&mut h, &mut raster);
    (h, raster, id, c)
}
