//! Live brush shift drags and the canvas that covers them.

use super::*;

/// A 60 px opaque red brush board with a freehand stroke and one Shift
/// segment continuing it (straight down from `c + (0, -200)` to
/// `c + (0, 100)`), every frame fed to the raster and the live canvas
/// settled and parked.
pub(crate) fn shift_chain_board(tag: &str) -> (Harness, FrameRaster, NodeId, Pos2) {
    let mut h = brush_board(tag);
    h.app.board_colors.fg.0 = [255, 40, 40, 255];
    h.app.brush_opacity = 1.0;
    h.app.brush_softness = 0.0;
    h.app.brush_width = 60.0;
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let freehand = [
        p(-300.0, -200.0),
        p(-200.0, -225.0),
        p(-100.0, -175.0),
        p(0.0, -200.0),
    ];
    raster_drag(&mut h, &mut raster, egui::Modifiers::NONE, &freehand, false);
    let id = h.app.doc().scene.nodes[0].id;
    raster_drag(
        &mut h,
        &mut raster,
        egui::Modifiers::SHIFT,
        &[p(-60.0, 140.0), p(-30.0, 120.0), p(0.0, 100.0)],
        true,
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the segment extends the stroke"
    );
    settle_brush_live(&mut h, &mut raster);
    (h, raster, id, c)
}

/// Press at `pts[0]`, move through the rest, and release, one frame each;
/// `hold` keeps the button down until the live segment shows its exact
/// stamp before releasing.
pub(crate) fn raster_drag(
    h: &mut Harness,
    raster: &mut FrameRaster,
    mods: egui::Modifiers,
    pts: &[Pos2],
    hold: bool,
) {
    let xf = h.app.board_xf();
    let button = |w: Pos2, pressed: bool| egui::Event::PointerButton {
        pos: xf.w2s(w),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: mods,
    };
    let mut events = vec![
        vec![egui::Event::PointerMoved(xf.w2s(pts[0]))],
        vec![button(pts[0], true)],
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
    if hold {
        wait_brush_live(h, raster, mods, |c| c.line_exact());
    }
    let last = *pts.last().unwrap();
    capture_frame(h, raster, |inp| {
        inp.modifiers = mods;
        inp.events = vec![button(last, false)];
    });
    capture_frame(h, raster, |inp| inp.modifiers = mods);
}

/// Run frames (the workers get a moment between them) until `done` holds
/// for the live canvas.
pub(crate) fn wait_brush_live(
    h: &mut Harness,
    raster: &mut FrameRaster,
    mods: egui::Modifiers,
    done: impl Fn(&board_path::BrushLiveCanvas) -> bool,
) {
    for _ in 0..400 {
        if h.app.brush_live.as_ref().is_some_and(&done) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        capture_frame(h, raster, |inp| inp.modifiers = mods);
    }
    panic!("the live canvas never got there");
}

pub(crate) fn settle_brush_live(h: &mut Harness, raster: &mut FrameRaster) {
    wait_brush_live(h, raster, egui::Modifiers::NONE, |c| c.settled());
}

/// A red, opaque brush pixel under world point `w`.
pub(crate) fn red_at(raster: &FrameRaster, xf: &board::BoardXf, w: Pos2) -> bool {
    let s = xf.w2s(w);
    let p = raster.px[s.y as usize * raster.w + s.x as usize];
    p[0] > 0.6 && p[1] < 0.4
}

/// Start a Shift drag through `pts`, hold it until the live segment shows
/// its exact stamp, and draw that frame into the raster. Returns the
/// whole-stroke stamps and segment pixels the press frame rasterized on
/// the frame thread.
pub(crate) fn hold_shift_drag(
    h: &mut Harness,
    raster: &mut FrameRaster,
    pts: &[Pos2],
) -> (u64, u64) {
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    let mut events = vec![
        vec![egui::Event::PointerMoved(xf.w2s(pts[0]))],
        vec![egui::Event::PointerButton {
            pos: xf.w2s(pts[0]),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        }],
    ];
    events.extend(
        pts[1..]
            .iter()
            .map(|w| vec![egui::Event::PointerMoved(xf.w2s(*w))]),
    );
    let mut press = (0, 0);
    for (i, events) in events.into_iter().enumerate() {
        let before = (
            board_path::stamps_on_this_thread(),
            board_path::stamp_px_on_this_thread(),
        );
        capture_frame(h, raster, |inp| {
            inp.modifiers = shift;
            inp.events = events;
        });
        if i == 1 {
            press = (
                board_path::stamps_on_this_thread() - before.0,
                board_path::stamp_px_on_this_thread() - before.1,
            );
        }
    }
    wait_brush_live(h, raster, shift, |c| c.line_exact());
    let out = capture_frame(h, raster, |inp| inp.modifiers = shift);
    let prims = h.ctx.tessellate(out.shapes, out.pixels_per_point);
    for p in raster.px.iter_mut() {
        *p = [0.0, 0.0, 0.0, 1.0];
    }
    raster.draw(&prims);
    press
}

/// The live canvas holds stroke `id`, and the scene leaves it out for the
/// canvas to paint.
pub(crate) fn canvas_covers(h: &Harness, id: NodeId) -> bool {
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    let (xf, screen, ppp) = (
        h.app.board_xf(),
        h.app.canvas_rect,
        h.ctx.pixels_per_point(),
    );
    h.app
        .brush_live
        .as_ref()
        .is_some_and(|c| c.holds_anchor(id) && c.covers_anchor(id, || key, &xf, screen, ppp))
}
