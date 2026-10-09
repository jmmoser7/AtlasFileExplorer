//! A painted bar and the frames that capture an eraser pass.

use super::*;

/// A painted bar across the middle of the view, too big to rasterize on the
/// frame loop.
pub(crate) fn big_brush_bar(h: &mut Harness) -> NodeId {
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 80.0;
    h.app.finish_freehand_brush(vec![
        Pos2::new(c.x - 650.0, c.y),
        Pos2::new(c.x, c.y + 4.0),
        Pos2::new(c.x + 650.0, c.y),
    ]);
    h.app.doc().scene.nodes.last().unwrap().id
}

/// The user's eraser (207 wide, pencil, softness 0.09) at 150 % on a 1.5
/// px/pt display, over a settled red bar too big to rasterize on the frame
/// loop, with every frame fed to a raster. Returns the bar and the world
/// point where a vertical pass at screen x `cx - 300` crosses its middle.
pub(crate) fn eraser_bar_board(tag: &str) -> (Harness, FrameRaster, NodeId, Pos2) {
    let mut h = line_board(tag);
    let mut raster = FrameRaster::new(1440, 900);
    h.ctx.set_pixels_per_point(1.5);
    capture_frame(&mut h, &mut raster, |i| i.max_texture_side = Some(8192));
    h.app.tab_mut().cam.z = 1.5;
    h.app.board_colors.fg.0 = [255, 40, 40, 255];
    capture_frame(&mut h, &mut raster, |_| {});
    let cw = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let id = big_brush_bar(&mut h);
    settle_captured(&mut h, &mut raster, "the bar", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
    });
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 207.0;
    h.app.eraser_softness = 0.09;
    h.app.eraser_texture = slate_doc::scene::BrushTexture::Pencil;
    capture_frame(&mut h, &mut raster, |_| {});
    let x = h
        .app
        .board_xf()
        .s2w(h.app.canvas_rect.center() - EVec2::new(300.0, 0.0))
        .x;
    let cross = Pos2::new(x, cw.y + 4.0 * (1.0 - (cw.x - x) / 650.0));
    (h, raster, id, cross)
}

/// Run one frame and draw it into `raster`.
pub(crate) fn shot(
    h: &mut Harness,
    raster: &mut FrameRaster,
    prepare: impl FnOnce(&mut egui::RawInput),
) {
    let out = capture_frame(h, raster, prepare);
    draw_now(h, raster, out);
}

/// [`settle_brush`] with every frame fed to `raster`; the settled frame is
/// drawn.
pub(crate) fn settle_captured(
    h: &mut Harness,
    raster: &mut FrameRaster,
    what: &str,
    ready: impl Fn(&SlateApp) -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let out = capture_frame(h, raster, |_| {});
        if h.app.brush_tiles.last.settled && ready(&h.app) {
            draw_now(h, raster, out);
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{what} never settled");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A frame with Shift held and the pointer at `s`, pressing or releasing
/// the primary button when `button` says so.
pub(crate) fn shift_at(s: Pos2, button: Option<bool>) -> impl FnOnce(&mut egui::RawInput) {
    move |i: &mut egui::RawInput| {
        i.modifiers = egui::Modifiers::SHIFT;
        i.events.push(egui::Event::PointerMoved(s));
        if let Some(pressed) = button {
            i.events.push(egui::Event::PointerButton {
                pos: s,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::SHIFT,
            });
        }
    }
}

pub(crate) fn erase_marks(n: &slate_doc::Node) -> Vec<slate_doc::scene::EraseMark> {
    match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.path.as_ref().unwrap().erase.clone(),
        _ => panic!("a path"),
    }
}

/// The erase marks stroke `id` has once the eraser drag in progress commits.
pub(crate) fn erase_after_pass(h: &Harness, id: NodeId) -> Vec<slate_doc::scene::EraseMark> {
    let Some(board::BoardDrag::Erase { points, .. }) = &h.app.board_drag else {
        panic!("an erase pass");
    };
    let tip = h.app.eraser_tip();
    let span = slate_doc::scene::StrokeSpan {
        width: tip.diameter,
        softness: tip.softness,
        color: slate_doc::scene::Rgba([0, 0, 0, tip.rgba[3]]),
        texture: h.app.eraser_texture,
    };
    let before = h.app.doc().scene.node(id).unwrap();
    erase_marks(&board_color::with_erase_mark(before, points, span))
}

/// Mean red over green in the 5 × 5 screen pixels around world point `w`:
/// about 0.84 on the lit bar, 0 on the eraser's gray band or the board.
pub(crate) fn redness(raster: &FrameRaster, xf: &board::BoardXf, w: Pos2) -> f32 {
    let s = xf.w2s(w);
    let mut sum = 0.0;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (x, y) = ((s.x as i64 + dx) as usize, (s.y as i64 + dy) as usize);
            let p = raster.px[y * raster.w + x];
            sum += p[0] - p[1];
        }
    }
    sum / 25.0
}

/// World points inside the eraser's core where the pass crosses the bar.
pub(crate) fn cut_points(cross: Pos2) -> [Pos2; 5] {
    [
        cross,
        cross + EVec2::new(0.0, 25.0),
        cross - EVec2::new(0.0, 25.0),
        cross + EVec2::new(45.0, 0.0),
        cross - EVec2::new(45.0, 0.0),
    ]
}

/// After an eraser release: frame by frame until the board settles, no
/// point where the pass cut shows the bar un-erased, while the bar away
/// from the pass stays lit. Then the settled pixels are the ones the
/// committed stroke rasterizes to from scratch.
pub(crate) fn assert_never_uncut_after_release(
    h: &mut Harness,
    raster: &mut FrameRaster,
    cross: Pos2,
) {
    let xf = h.app.board_xf();
    let far = cross + EVec2::new(500.0, -3.0);
    let lit = 0.5 * redness(raster, &xf, far);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut frames = 0;
    loop {
        shot(h, raster, |_| {});
        frames += 1;
        for p in cut_points(cross) {
            let r = redness(raster, &xf, p);
            assert!(
                r < lit,
                "frame {frames} after release: {p:?} shows the uncut bar ({r:.2})"
            );
        }
        if h.app.brush_tiles.last.settled && !h.app.erase_settling() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the erased bar never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        redness(raster, &xf, far) > lit,
        "the bar away from the pass went dark"
    );
    assert_settled_as_committed(h, raster, cross);
}

/// The settled pixels around world point `cross` are the ones the committed
/// stroke rasterizes to from scratch.
pub(crate) fn assert_settled_as_committed(h: &mut Harness, raster: &mut FrameRaster, cross: Pos2) {
    let xf = h.app.board_xf();
    let settled = raster.px.clone();
    h.app.brush_tiles.clear();
    h.app.brush_stamps.clear();
    settle_captured(h, raster, "the rebuilt bar", |_| true);
    let s = xf.w2s(cross);
    let mut worst = 0.0f32;
    for y in (s.y as usize - 150)..(s.y as usize + 150) {
        for x in (s.x as usize - 150)..(s.x as usize + 150) {
            let (a, b) = (settled[y * raster.w + x], raster.px[y * raster.w + x]);
            for k in 0..3 {
                worst = worst.max((a[k] - b[k]).abs());
            }
        }
    }
    assert!(
        worst <= 2.0 / 255.0,
        "the settled pass differs from the committed stroke's own raster by {worst}"
    );
}

/// Half the redness of the bar, lit, at `cross` moved 500 world units along
/// it: above it a point shows the bar, below it the bar is erased there.
pub(crate) fn half_lit(h: &Harness, raster: &FrameRaster, cross: Pos2) -> f32 {
    0.5 * redness(raster, &h.app.board_xf(), cross + EVec2::new(500.0, -3.0))
}

/// Pass 1 of the settle tests on [`eraser_bar_board`]: a smooth Shift pass
/// down screen x `cx - 300` across the bar, released while its final cut
/// is held on the workers ([`board_path::EraseSettle::hold`]). Returns the
/// frame-loop stamp px counted just before the release.
pub(crate) fn release_a_settling_pass(
    h: &mut Harness,
    raster: &mut FrameRaster,
    id: NodeId,
) -> u64 {
    let c = h.app.canvas_rect.center();
    let press = c + EVec2::new(-300.0, -250.0);
    let first = c + EVec2::new(-300.0, -120.0);
    let last = c + EVec2::new(-300.0, 250.0);
    shot(h, raster, shift_at(press, None));
    shot(h, raster, shift_at(press, Some(true)));
    shot(h, raster, shift_at(first, None));
    let mut waited = 0;
    while !h.app.erase_live.get(&id).is_some_and(|l| l.line_exact()) {
        waited += 1;
        assert!(waited < 2000, "the first cut never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(h, raster, |i| i.modifiers = egui::Modifiers::SHIFT);
    }
    shot(h, raster, shift_at(last, None));
    h.app.erase_settle.hold = true;
    let px = board_path::stamp_px_on_this_thread();
    shot(h, raster, shift_at(last, Some(false)));
    assert_eq!(
        erase_marks(h.app.doc().scene.node(id).unwrap()).len(),
        1,
        "pass 1 commits"
    );
    assert!(
        h.app.erase_settle.holds(h.app.tab().id, id),
        "pass 1 settles"
    );
    px
}
