//! The wide user brush, the eraser bar setup, and wheel notches.

use super::*;

/// A 24-wide, opaque, smooth red brush stroke across the middle of the
/// view, with the Eraser armed at 80 wide and Pencil picked in its style
/// row by events. Returns the stroke and the world center.
pub(crate) fn pencil_eraser_board(tag: &str) -> (Harness, NodeId, Pos2) {
    use slate_doc::scene::BrushTexture;
    let mut h = brush_board(tag);
    h.app.board_colors.fg.0 = [255, 30, 30, 255];
    h.app.brush_opacity = 1.0;
    h.app.brush_softness = 0.0;
    h.app.brush_width = 24.0;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    press_drag_release_frames(
        &mut h,
        &dense(&[p(-80.0, 0.0), p(80.0, 0.0)]),
        egui::Modifiers::NONE,
        |_| {},
    );
    let id = h.app.doc().scene.nodes.last().expect("the stroke").id;
    settle_brush(&mut h, "the stroke", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
    });
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 80.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    let hud = h.app.board_xf().w2s(p(0.0, 200.0));
    pick_style_row(
        &mut h,
        hud,
        board_tip_hud::TipChoice::Texture(BrushTexture::Pencil),
    );
    (h, id, c)
}

/// The user's brush (207 wide, pencil, softness 0.09) on a 1.5 px/pt
/// display at 150 %, after one freehand stroke whose end the next Shift
/// segment continues.
pub(crate) fn big_brush_board(tag: &str) -> (Harness, Pos2) {
    let (mut h, c) = big_brush_setup(tag);
    h.frame();
    press_drag_release_frames(&mut h, &big_brush_stroke(c), egui::Modifiers::NONE, |_| {});
    (h, c)
}

/// [`big_brush_board`] with every frame from the stroke on fed to a raster.
pub(crate) fn big_brush_raster_board(tag: &str) -> (Harness, FrameRaster, Pos2) {
    let (mut h, c) = big_brush_setup(tag);
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    raster_drag(
        &mut h,
        &mut raster,
        egui::Modifiers::NONE,
        &big_brush_stroke(c),
        false,
    );
    (h, raster, c)
}

/// The user's brush on a red foreground, and the world point at the
/// window's center.
pub(crate) fn big_brush_setup(tag: &str) -> (Harness, Pos2) {
    let mut h = brush_board(tag);
    h.ctx.set_pixels_per_point(1.5);
    h.frame_with(|i| i.max_texture_side = Some(8192));
    h.app.board_colors.fg.0 = [255, 40, 40, 255];
    h.app.brush_width = 207.0;
    h.app.brush_softness = 0.09;
    h.app.brush_texture = slate_doc::scene::BrushTexture::Pencil;
    h.app.tab_mut().cam.z = 1.5;
    let c = h.app.board_xf().s2w(Pos2::new(720.0, 450.0));
    (h, c)
}

pub(crate) fn big_brush_stroke(c: Pos2) -> [Pos2; 3] {
    [
        c + EVec2::new(-300.0, -200.0),
        c + EVec2::new(-150.0, -150.0),
        c + EVec2::new(0.0, -200.0),
    ]
}

/// Red, averaged over the 5 × 5 points around world point `w`: grain
/// leaves single pixels dark in a lit stroke.
pub(crate) fn red_around(raster: &FrameRaster, xf: &board::BoardXf, w: Pos2) -> bool {
    let s = xf.w2s(w);
    let (mut r, mut g) = (0.0, 0.0);
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (x, y) = ((s.x as i64 + dx) as usize, (s.y as i64 + dy) as usize);
            let p = raster.px[y * raster.w + x];
            r += p[0];
            g += p[1];
        }
    }
    r - g > 0.3 * 25.0
}

/// Tessellate `out` and draw it over black, with no more frames run.
pub(crate) fn draw_now(h: &Harness, raster: &mut FrameRaster, out: egui::FullOutput) {
    let prims = h.ctx.tessellate(out.shapes, out.pixels_per_point);
    for p in raster.px.iter_mut() {
        *p = [0.0, 0.0, 0.0, 1.0];
    }
    raster.draw(&prims);
}

/// One wheel notch out at the window's center, drawn with its tiles landed.
pub(crate) fn wheel_notch(h: &mut Harness, raster: &mut FrameRaster) {
    let z = h.app.tab().cam.z;
    capture_frame(h, raster, |inp| {
        inp.events = vec![
            egui::Event::PointerMoved(Pos2::new(720.0, 450.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: EVec2::new(0.0, -120.0),
                modifiers: egui::Modifiers::NONE,
            },
        ];
    });
    let out = capture_frame(h, raster, |_| {});
    rasterize(h, raster, out);
    assert!(
        (h.app.tab().cam.z - z).abs() > z * 0.01,
        "the wheel moved the camera"
    );
}
