//! Watch a settling eraser pass, its stamp, and undo.

use super::*;

/// One frame of a settling pass: the pass-1 cut around `cross` stays
/// erased, and the bar away from it stays lit (above `lit`).
pub(crate) fn shot_settling(
    h: &mut Harness,
    raster: &mut FrameRaster,
    (cross, lit): (Pos2, f32),
    prepare: impl FnOnce(&mut egui::RawInput),
    when: &str,
) {
    shot(h, raster, prepare);
    let xf = h.app.board_xf();
    for p in cut_points(cross) {
        let r = redness(raster, &xf, p);
        assert!(r < lit, "{when}: {p:?} shows the uncut bar ({r:.2})");
    }
    let far = cross + EVec2::new(500.0, -3.0);
    let r = redness(raster, &xf, far);
    assert!(
        r > lit,
        "{when}: the bar away from the pass is blank ({r:.2})"
    );
}

/// Let the held pass-1 cut land, then [`shot_settling`] every frame until
/// the board settles; nothing stamped on the frame loop since `px`, and the
/// settled pixels are the committed stroke's own.
pub(crate) fn settle_watched(
    h: &mut Harness,
    raster: &mut FrameRaster,
    at: (Pos2, f32),
    px: u64,
    what: &str,
) {
    for k in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot_settling(h, raster, at, |_| {}, &format!("{what}: held frame {k}"));
    }
    h.app.erase_settle.hold = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    // The tiles settle a frame after the settle ends: they take the stroke
    // back then.
    let (mut frames, mut ended) = (0, false);
    while !(ended && h.app.brush_tiles.last.settled) {
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: the bar never settled"
        );
        ended = !h.app.erase_settling();
        std::thread::sleep(std::time::Duration::from_millis(5));
        frames += 1;
        shot_settling(
            h,
            raster,
            at,
            |_| {},
            &format!("{what}: frame {frames} after the cut"),
        );
    }
    let spent = board_path::stamp_px_on_this_thread() - px;
    assert_eq!(spent, 0, "{what}: stamped {spent} px on the frame loop");
    assert_settled_as_committed(h, raster, at.0);
}

/// Stroke `id`'s cached bitmap for the open tab.
pub(crate) fn stamp_of(h: &Harness, id: NodeId) -> Option<&(u64, board_path::BrushStampGpu)> {
    stamp_of_app(&h.app, id)
}

/// A frame pressing `key` with Ctrl held.
pub(crate) fn ctrl_key(key: egui::Key) -> Box<dyn FnOnce(&mut egui::RawInput)> {
    let ctrl = egui::Modifiers::CTRL;
    Box::new(move |i| {
        i.modifiers = ctrl;
        i.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        });
    })
}

/// Settle a pass on the bar into its stand-in with the tiles off, undo it
/// (the restored ink shows every frame), with `zoom` halve the zoom until
/// the restored bar is exact there, then with `redo` redo it (the cut
/// stays erased every frame, the first five with stroke bitmaps held).
pub(crate) fn undo_a_settled_eraser_pass(tag: &str, redo: bool, zoom: bool) {
    let (mut h, mut raster, id, cross) = eraser_bar_board(tag);
    h.app.brush_tiles_enabled = false;
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let key = board_path::node_stamp_key(&before);
    settle_captured(&mut h, &mut raster, "the bar's own bitmap", |app| {
        stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
    });
    let lit = half_lit(&h, &raster, cross);
    release_a_settling_pass(&mut h, &mut raster, id);
    h.app.erase_settle.hold = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.erase_settle.holds(h.app.tab().id, id) {
        assert!(std::time::Instant::now() < deadline, "the cut never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(&mut h, &mut raster, |_| {});
    }
    let erased = h.app.doc().scene.node(id).unwrap().clone();
    let mut prepare = ctrl_key(egui::Key::Z);
    let mut frames = 0;
    loop {
        shot(
            &mut h,
            &mut raster,
            std::mem::replace(&mut prepare, Box::new(|_| {})),
        );
        assert_eq!(
            h.app.doc().scene.node(id),
            Some(&before),
            "Ctrl+Z restores the bar"
        );
        let xf = h.app.board_xf();
        for p in cut_points(cross) {
            let r = redness(&raster, &xf, p);
            assert!(
                r > lit,
                "frame {frames} after undo: {p:?} is still erased ({r:.2})"
            );
        }
        let current = stamp_of(&h, id).is_some_and(|(k, g)| g.exact && Some(*k) == key);
        if current && !h.app.erase_settling() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the restored bar never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        frames += 1;
    }
    if zoom {
        h.app.tab_mut().cam.z *= 0.5;
        let want = board_path::stamp_pixel_for_zoom(h.app.tab().cam.z, h.ctx.pixels_per_point());
        let mut frames = 0;
        loop {
            shot(&mut h, &mut raster, |_| {});
            let xf = h.app.board_xf();
            for p in cut_points(cross) {
                let r = redness(&raster, &xf, p);
                assert!(
                    r > lit,
                    "frame {frames} after the zoom: {p:?} is erased ({r:.2})"
                );
            }
            let exact = stamp_of(&h, id)
                .is_some_and(|(k, g)| g.exact && Some(*k) == key && g.wanted_pixel == want);
            if exact {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the zoomed bar never settled"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
            frames += 1;
        }
    }
    if !redo {
        return;
    }
    let key = board_path::node_stamp_key(&erased);
    h.app.brush_tiles.hold_rasters = true;
    let mut prepare = ctrl_key(egui::Key::Y);
    let mut frames = 0;
    loop {
        if frames == 5 {
            h.app.brush_tiles.hold_rasters = false;
        }
        shot(
            &mut h,
            &mut raster,
            std::mem::replace(&mut prepare, Box::new(|_| {})),
        );
        assert_eq!(
            h.app.doc().scene.node(id),
            Some(&erased),
            "Ctrl+Y erases the bar again"
        );
        let xf = h.app.board_xf();
        for p in cut_points(cross) {
            let r = redness(&raster, &xf, p);
            assert!(
                r < lit,
                "frame {frames} after redo: {p:?} shows the uncut bar ({r:.2})"
            );
        }
        let current = stamp_of(&h, id).is_some_and(|(k, g)| g.exact && Some(*k) == key);
        if frames >= 5 && current {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the redone bar never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        frames += 1;
    }
}

/// [`stamp_of`] from inside a readiness check.
pub(crate) fn stamp_of_app(
    app: &SlateApp,
    id: NodeId,
) -> Option<&(u64, board_path::BrushStampGpu)> {
    app.brush_stamps.get(&app.stroke_cache_id(id))
}

/// [`release_a_settling_pass`], then let its cut land with stroke bitmaps
/// held: the settled preview stands in for the bar.
pub(crate) fn settle_into_stand_in(h: &mut Harness, raster: &mut FrameRaster, id: NodeId) {
    release_a_settling_pass(h, raster, id);
    h.app.brush_tiles.hold_rasters = true;
    h.app.erase_settle.hold = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.erase_settle.holds(h.app.tab().id, id) {
        assert!(std::time::Instant::now() < deadline, "the cut never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(h, raster, |_| {});
    }
    assert!(
        stamp_of(h, id).is_some_and(|(_, g)| !g.exact && g.seal.is_some()),
        "the preview stands in"
    );
}
