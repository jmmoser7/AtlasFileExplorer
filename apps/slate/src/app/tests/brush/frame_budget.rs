//! Big-brush and smooth passes stamp nothing on the frame loop.

use super::*;

/// Review r17 note: a late raster of the stroke's own content at another
/// zoom never replaces the bitmap of that content on top.
#[test]
fn a_late_raster_never_replaces_a_bitmap_of_the_same_content() {
    let (mut h, mut raster, id, _) = eraser_bar_board("eraser_late_raster_same_key");
    h.app.brush_tiles_enabled = false;
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    let z = h.app.tab().cam.z;
    let want = board_path::stamp_pixel_for_zoom(z, h.ctx.pixels_per_point());
    let on_top = move |app: &SlateApp| {
        stamp_of_app(app, id).is_some_and(|(k, g)| Some(*k) == key && g.wanted_pixel == want)
    };
    settle_captured(&mut h, &mut raster, "the bar's own bitmap", move |app| {
        on_top(app) && stamp_of_app(app, id).is_some_and(|(_, g)| g.exact)
    });
    h.app.brush_tiles.hold_rasters = true;
    h.app.tab_mut().cam.z = z * 0.5;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.brush_tiles.stroke_landed_len(false) == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the zoomed-out bar's raster never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(&mut h, &mut raster, |_| {});
    }
    assert!(
        on_top(&h.app),
        "the first zoom's bitmap stays on top while the raster waits"
    );
    // Zoomed in, the bar is too big to stamp on the frame loop.
    h.app.tab_mut().cam.z = z * 2.0;
    h.app.brush_tiles.hold_rasters = false;
    shot(&mut h, &mut raster, |_| {});
    assert!(
        on_top(&h.app),
        "the late zoomed-out raster replaced the bar's bitmap"
    );
}

/// The same holds for a brush release: the live canvas stands in for the
/// committed stroke, so the commit frame rasterizes nothing.
#[test]
fn a_big_brush_release_stamps_nothing_on_the_frame_loop() {
    let mut h = line_board("brush_commit_async");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 80.0;
    h.frame();
    let c = h.app.canvas_rect.center();
    let start = c - EVec2::new(650.0, 0.0);
    h.frame_with(pointer_to(start, false));
    let before = board_path::stamps_on_this_thread();
    h.frame_with(primary_button(start, true, false));
    for i in 1..=26 {
        let at = start + EVec2::new(i as f32 * 50.0, (i as f32 * 0.7).sin() * 30.0);
        h.frame_with(pointer_to(at, false));
    }
    let canvas = h.app.brush_live.as_ref().expect("live canvas").texture();
    let nodes = h.app.doc().scene.nodes.len();
    h.frame_with(primary_button(c + EVec2::new(650.0, 0.0), false, false));
    assert_eq!(h.app.doc().scene.nodes.len(), nodes + 1, "stroke committed");
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    let next = h.frame_output(|_| {});
    assert_eq!(
        board_path::stamps_on_this_thread(),
        before,
        "the brush release rasterized on the frame loop"
    );
    assert!(
        painted_textures(&next).contains(&canvas),
        "the committed stroke is not standing in"
    );
    settle_brush(&mut h, "the stroke", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
    });
    assert_eq!(board_path::stamps_on_this_thread(), before);
}

/// And for Smooth: the preview and the commit rebuild the smoothed stroke on
/// the workers, and the commit keeps every other stroke's raster.
#[test]
fn a_smooth_pass_on_a_big_stroke_stamps_nothing_on_the_frame_loop() {
    let mut h = line_board("smooth_commit_async");
    h.frame();
    let id = big_brush_bar(&mut h);
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    h.app.finish_freehand_brush(vec![
        Pos2::new(c.x - 650.0, c.y + 200.0),
        Pos2::new(c.x + 650.0, c.y + 200.0),
    ]);
    let other = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.patch_nodes(&[other], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.gaussian_blur = 4.0;
        }
    });
    settle_brush(&mut h, "the strokes", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
            && stamp_of_app(app, other).is_some_and(|(_, g)| g.exact)
    });
    let kept = stamp_of(&h, other).unwrap().1.tex.id();

    h.app.set_board_tool(board::BoardTool::Smooth);
    h.app.smooth_width = 60.0;
    h.app.smooth_strength = 1.0;
    h.frame();
    let at = h.app.canvas_rect.center();
    h.frame_with(pointer_to(at, false));
    let before = board_path::stamps_on_this_thread();
    h.frame_with(primary_button(at, true, false));
    for i in 1..=8 {
        h.frame_with(pointer_to(at + EVec2::new(i as f32 * 6.0, 0.0), false));
    }
    h.frame_with(primary_button(at + EVec2::new(48.0, 0.0), false, false));
    h.frame();
    assert_eq!(
        board_path::stamps_on_this_thread(),
        before,
        "the smooth pass rasterized on the frame loop"
    );
    let node = h.app.doc().scene.node(id).unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    assert!(
        shape.stroke.gaussian_blur > 0.0,
        "the pass smoothed the bar"
    );
    assert_eq!(
        stamp_of(&h, other).map(|(_, g)| g.tex.id()),
        Some(kept),
        "the commit dropped another stroke's raster"
    );
    settle_brush(&mut h, "the smoothed bar", |app| {
        stamp_of_app(app, id).is_some_and(|(_, g)| g.exact)
    });
    assert_eq!(board_path::stamps_on_this_thread(), before);
}
