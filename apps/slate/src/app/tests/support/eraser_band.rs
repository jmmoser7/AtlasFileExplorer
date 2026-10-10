//! Flick bands held at the view edge.

use super::*;

/// Put the bar's centerline just below the view with its ink still in it,
/// tiles off so the bar paints its own bitmap, and return the world points
/// in that ink along the pass at `cross` and the redness half of it lit
/// shows.
pub(crate) fn bar_at_the_view_bottom(
    h: &mut Harness,
    raster: &mut FrameRaster,
    id: NodeId,
    cross: Pos2,
) -> ([Pos2; 3], f32) {
    let seen = place_bar_at_the_view_bottom(h, id, cross);
    h.app.brush_tiles_enabled = false;
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    settle_captured(h, raster, "the bar at the edge", |app| {
        stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
    });
    let xf = h.app.board_xf();
    let lit = 0.5 * redness(raster, &xf, seen[0] + EVec2::new(500.0, 0.0));
    for p in seen {
        let r = redness(raster, &xf, p);
        assert!(
            r > lit,
            "the bar's ink shows at {p:?} before the pass ({r:.2})"
        );
    }
    (seen, lit)
}

pub(crate) fn place_bar_at_the_view_bottom(h: &mut Harness, id: NodeId, cross: Pos2) -> [Pos2; 3] {
    let r = h.app.doc().scene.node(id).unwrap().rect;
    let z = h.app.tab().cam.z;
    let half = 0.5 * h.app.canvas_rect.height() / z;
    h.app.tab_mut().cam.offset.y = r.y - 6.0 - half;
    let xf = h.app.board_xf();
    let view = xf.s2w(h.app.canvas_rect.max);
    assert!(r.y > view.y, "the bar's centerline is out of view");
    let up = EVec2::new(0.0, -20.0);
    let seen = [
        cross + up,
        cross + up + EVec2::new(45.0, 0.0),
        cross + up - EVec2::new(45.0, 0.0),
    ];
    for p in seen {
        assert!(h.app.canvas_rect.contains(xf.w2s(p)), "{p:?} is in view");
    }
    seen
}

/// Every frame, the points `seen` stay erased: six with stroke bitmaps
/// held, then until stroke `id`'s own bitmap is current.
pub(crate) fn band_holds_at_the_edge(
    h: &mut Harness,
    raster: &mut FrameRaster,
    (id, seen): (NodeId, [Pos2; 3]),
    lit: f32,
    what: &str,
) {
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    // The eraser's cursor would cover the points.
    let away = h.app.canvas_rect.min + EVec2::new(200.0, 200.0);
    shot(h, raster, |i| {
        i.events.push(egui::Event::PointerMoved(away))
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut frames = 0;
    loop {
        if frames == 6 {
            h.app.brush_tiles.hold_rasters = false;
        }
        shot(h, raster, |_| {});
        let xf = h.app.board_xf();
        for p in seen {
            let r = redness(raster, &xf, p);
            assert!(
                r < lit,
                "{what}: frame {frames} after release: {p:?} shows the uncut bar ({r:.2})"
            );
        }
        let current = stamp_of(h, id).is_some_and(|(k, g)| g.exact && Some(*k) == key);
        if frames >= 6 && current && !h.app.erase_settling() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: the erased bar never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        frames += 1;
    }
}

/// [`bar_at_the_view_bottom`], then a Shift pass whose final cut the
/// workers lose past the retries, released with stroke bitmaps held: the
/// preview stands in uncut under the band, which holds for four frames.
/// Returns the points in view along the pass and the redness half lit.
pub(crate) fn give_up_a_pass_at_the_view_bottom(
    h: &mut Harness,
    raster: &mut FrameRaster,
    id: NodeId,
    cross: Pos2,
) -> ([Pos2; 3], f32) {
    let (seen, lit) = bar_at_the_view_bottom(h, raster, id, cross);
    let c = h.app.canvas_rect.center();
    let press = Pos2::new(c.x - 300.0, h.app.canvas_rect.min.y + 80.0);
    let first = Pos2::new(c.x - 300.0, h.app.canvas_rect.max.y - 60.0);
    let last = Pos2::new(c.x - 300.0, h.app.canvas_rect.max.y - 4.0);
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
    h.app.brush_tiles.lose_lines = true;
    shot(h, raster, shift_at(last, None));
    h.app.brush_tiles.hold_rasters = true;
    shot(h, raster, shift_at(last, Some(false)));
    assert!(
        h.app.erase_settle.holds(h.app.tab().id, id),
        "the pass settles"
    );
    let away = h.app.canvas_rect.min + EVec2::new(200.0, 200.0);
    shot(h, raster, |i| {
        i.events.push(egui::Event::PointerMoved(away))
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut frames = 0;
    while h.app.erase_settle.holds(h.app.tab().id, id) {
        assert!(
            std::time::Instant::now() < deadline,
            "the workers never gave up"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(h, raster, |_| {});
        frames += 1;
    }
    h.app.brush_tiles.lose_lines = false;
    let xf = h.app.board_xf();
    for k in 0..4 {
        shot(h, raster, |_| {});
        for p in seen {
            let r = redness(raster, &xf, p);
            assert!(
                r < lit,
                "frame {k} after giving up ({frames} settling): {p:?} shows the uncut bar ({r:.2})"
            );
        }
    }
    (seen, lit)
}

/// Tiles off and the bar's own bitmap current, then a Shift flick down
/// screen x `cx - 300` released before its preview exists, with stroke
/// bitmaps held. Returns points along the cut and the redness half lit.
pub(crate) fn flick_the_bar_unseen(
    h: &mut Harness,
    raster: &mut FrameRaster,
    id: NodeId,
    cross: Pos2,
) -> ([Pos2; 3], f32) {
    h.app.brush_tiles_enabled = false;
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    settle_captured(h, raster, "the bar's own bitmap", |app| {
        stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
    });
    let lit = half_lit(h, raster, cross);
    h.app.brush_tiles.hold_rasters = true;
    let c = h.app.canvas_rect.center();
    let press = c + EVec2::new(-300.0, -250.0);
    let last = c + EVec2::new(-300.0, 250.0);
    shot(h, raster, shift_at(press, None));
    shot(h, raster, shift_at(press, Some(true)));
    shot(h, raster, shift_at(last, None));
    assert!(
        !h.app.erase_live.contains_key(&id),
        "no preview before the release"
    );
    shot(h, raster, shift_at(last, Some(false)));
    assert_eq!(
        erase_marks(h.app.doc().scene.node(id).unwrap()).len(),
        1,
        "the flick commits"
    );
    let p = cut_points(cross);
    ([p[0], p[1], p[2]], lit)
}
