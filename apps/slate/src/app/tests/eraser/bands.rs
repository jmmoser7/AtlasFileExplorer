//! Flick bands at the view edge and while a stroke is off view.

use super::*;

/// The same when the workers give up on a settling pass's cut: its preview
/// stands in uncut under the band, which stays at the view's edge and
/// while the stroke is panned away and back.
#[test]
fn a_band_over_a_given_up_cut_stays_at_the_view_edge_and_off_view() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_gave_up");
    let (seen, lit) = give_up_a_pass_at_the_view_bottom(&mut h, &mut raster, id, cross);
    let home = h.app.tab().cam.offset;
    h.app.tab_mut().cam.offset.y += 3000.0;
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    h.app.tab_mut().cam.offset = home;
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "gave up");
}

/// Review r16 D1: undoing that pass drops nothing a redo needs: Ctrl+Y
/// brings the uncut stand-in back with its band over the cut.
#[test]
fn redoing_a_given_up_eraser_pass_keeps_its_band() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_gave_up_redo");
    let (seen, lit) = give_up_a_pass_at_the_view_bottom(&mut h, &mut raster, id, cross);
    let erased = h.app.doc().scene.node(id).unwrap().clone();
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Z));
    assert_ne!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "Ctrl+Z restores the bar"
    );
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Y));
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "Ctrl+Y erases the bar again"
    );
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "redo");
}

/// Review r16 R1: hiding a stroke whose flick band waits on its raster
/// keeps the band: the hide ghost and the stroke after Ctrl+Z (unhide)
/// never paint the un-erased stroke.
#[test]
fn hiding_a_flicked_stroke_keeps_its_band() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_hidden");
    let (seen, lit) = flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    let away = h.app.canvas_rect.min + EVec2::new(200.0, 200.0);
    shot(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(away))
    });
    h.app.board_sel = [id].into_iter().collect();
    assert_eq!(h.app.cmd_hide_selection(), 1, "Ctrl+H hides the bar");
    for k in 0..3 {
        shot(&mut h, &mut raster, |_| {});
        let xf = h.app.board_xf();
        for p in seen {
            let r = redness(&raster, &xf, p);
            assert!(
                r < lit,
                "hide ghost frame {k}: {p:?} shows the uncut bar ({r:.2})"
            );
        }
    }
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Z));
    assert!(
        !h.app.doc().scene.node(id).unwrap().hidden,
        "Ctrl+Z shows the bar again"
    );
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "hidden");
}

/// Review r16 note: a flick band with no preview stays while its stroke
/// is panned out of view, and covers the cut when it comes back.
#[test]
fn a_flicked_band_stays_while_its_stroke_is_panned_away() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_panned");
    let (seen, lit) = flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    let home = h.app.tab().cam.offset;
    h.app.tab_mut().cam.offset.y += 3000.0;
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    h.app.tab_mut().cam.offset = home;
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "panned");
}

/// Review r17 R1: Ctrl+Z while a pass's preview still settles keeps the
/// pass's band, dormant, so a later Ctrl+Y never paints the un-erased bar
/// while the redone bar's bitmap is on the workers.
#[test]
fn redoing_a_pass_undone_while_it_settles_keeps_the_cut_dark() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_settle_undo_redo");
    h.app.brush_tiles_enabled = false;
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let key = board_path::node_stamp_key(&before);
    let restored = move |app: &SlateApp| {
        stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
    };
    settle_captured(&mut h, &mut raster, "the bar's own bitmap", restored);
    let lit = half_lit(&h, &raster, cross);
    release_a_settling_pass(&mut h, &mut raster, id);
    let erased = h.app.doc().scene.node(id).unwrap().clone();
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Z));
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before),
        "Ctrl+Z restores the bar"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !(restored(&h.app) && !h.app.erase_settling()) {
        assert!(
            std::time::Instant::now() < deadline,
            "the restored bar never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(&mut h, &mut raster, |_| {});
    }
    let xf = h.app.board_xf();
    for p in cut_points(cross) {
        let r = redness(&raster, &xf, p);
        assert!(r > lit, "after undo: {p:?} is still erased ({r:.2})");
    }
    h.app.brush_tiles.hold_rasters = true;
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Y));
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "Ctrl+Y erases the bar again"
    );
    let p = cut_points(cross);
    band_holds_at_the_edge(&mut h, &mut raster, (id, [p[0], p[1], p[2]]), lit, "redo");
}

/// Review r17 R1: undoing the flick and then the bar's own Add keeps the
/// flick's dormant band, and redoing both brings it back over the cut.
#[test]
fn redoing_an_undone_add_under_a_dormant_band_keeps_it() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_undo_add");
    let (seen, lit) = flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    let erased = h.app.doc().scene.node(id).unwrap().clone();
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Z));
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Z));
    assert!(
        h.app.doc().scene.node(id).is_none(),
        "the second Ctrl+Z removes the bar"
    );
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Y));
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Y));
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "two Ctrl+Y bring the erased bar back"
    );
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "re-added");
}

/// Review r18 D1: deleting a flicked stroke while its band waits on its
/// raster keeps the band, dormant, so Ctrl+Z never paints the un-erased
/// stroke.
#[test]
fn undoing_a_delete_under_a_flick_band_keeps_it() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_undo_delete");
    let (seen, lit) = flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    let erased = h.app.doc().scene.node(id).unwrap().clone();
    h.app.delete_board_nodes(&[id]);
    assert!(
        h.app.doc().scene.node(id).is_none(),
        "Delete removes the bar"
    );
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    assert!(
        !h.app.erase_settling(),
        "the band over the deleted bar asks for frames"
    );
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Z));
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "Ctrl+Z brings the erased bar back"
    );
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "undeleted");
}

/// Review r18 D1: the same when the stroke is deleted while its pass's
/// preview still settles: the settle turns into its bands.
#[test]
fn undoing_a_delete_under_a_settling_pass_keeps_its_band() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_settle_undo_delete");
    h.app.brush_tiles_enabled = false;
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    settle_captured(&mut h, &mut raster, "the bar's own bitmap", |app| {
        stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
    });
    let lit = half_lit(&h, &raster, cross);
    release_a_settling_pass(&mut h, &mut raster, id);
    let erased = h.app.doc().scene.node(id).unwrap().clone();
    h.app.delete_board_nodes(&[id]);
    assert!(
        h.app.doc().scene.node(id).is_none(),
        "Delete removes the bar"
    );
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    assert!(
        !h.app.erase_settle.holds(h.app.tab().id, id),
        "the settle outlives its stroke"
    );
    assert!(
        !h.app.erase_settling(),
        "the band over the deleted bar asks for frames"
    );
    h.app.erase_settle.hold = false;
    h.app.brush_tiles.hold_rasters = true;
    shot(&mut h, &mut raster, ctrl_key(egui::Key::Z));
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "Ctrl+Z brings the erased bar back"
    );
    let p = cut_points(cross);
    band_holds_at_the_edge(
        &mut h,
        &mut raster,
        (id, [p[0], p[1], p[2]]),
        lit,
        "undeleted settle",
    );
}

/// Review r17 R2 (Art. II): a band that lingers over a hidden stroke
/// hashes no stroke content per frame while the scene stays as it is.
#[test]
fn a_band_over_a_hidden_stroke_hashes_nothing_per_frame() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_hash");
    flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    h.app.board_sel = [id].into_iter().collect();
    assert_eq!(h.app.cmd_hide_selection(), 1, "Ctrl+H hides the bar");
    // Past the hide ghost.
    std::thread::sleep(std::time::Duration::from_millis(250));
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    let hashed = board_path::content_hashed_on_this_thread();
    for _ in 0..10 {
        shot(&mut h, &mut raster, |_| {});
    }
    let spent = board_path::content_hashed_on_this_thread() - hashed;
    assert!(
        spent < 10,
        "10 idle frames with a band over the hidden bar hashed {spent} stroke contents"
    );
}

/// Review r17 D1: a freehand eraser pass released before its preview
/// exists, over the flicked bar's far end, keeps the flick's band.
#[test]
fn a_freehand_pass_after_a_flick_keeps_its_band() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_freehand_after");
    let (seen, lit) = flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    let c = h.app.canvas_rect.center();
    let (a, b) = (c + EVec2::new(250.0, -100.0), c + EVec2::new(250.0, 100.0));
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    shot(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(a))
    });
    shot(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(a));
        i.events.push(button(a, true));
    });
    shot(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(b));
        i.events.push(button(b, false));
    });
    assert_eq!(
        erase_marks(h.app.doc().scene.node(id).unwrap()).len(),
        2,
        "the freehand pass commits"
    );
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "freehand after");
}

/// Review r17 D2 (Art. II): a band whose reach is in view while its
/// selected stroke's own ink is culled asks for no frames.
#[test]
fn a_band_over_a_culled_stroke_stops_asking_for_frames() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_culled");
    flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    h.app.board_sel = [id].into_iter().collect();
    let r = h.app.doc().scene.node(id).unwrap().rect;
    let half = h.app.canvas_rect.size() * (0.5 / h.app.tab().cam.z);
    // The view's lower left corner above the bar's right end: inside the
    // flick's reach, clear of the bar's ink and the flick's own segment.
    let corner = Pos2::new(r.x + r.w - 100.0, r.y - 150.0);
    h.app.tab_mut().cam.offset = EVec2::new(corner.x + half.x, corner.y - half.y);
    let view = h.app.board_paint_view(h.app.canvas_rect);
    let node = h.app.doc().scene.node(id).unwrap();
    assert!(!board::paints_in_view(node, &view), "the bar is culled");
    assert!(
        corner.x > cross.x + 400.0,
        "the flick's segment is out of view"
    );
    h.app.brush_tiles.hold_rasters = false;
    for _ in 0..200 {
        shot(&mut h, &mut raster, |_| {});
    }
    assert!(!h.app.erase_settling(), "the band still waits in view");
    let quiet = (0..60).any(|_| {
        let out = h.frame_output(|_| {});
        !out.viewport_output[&egui::ViewportId::ROOT]
            .repaint_delay
            .is_zero()
    });
    assert!(quiet, "the idle board keeps repainting");
}

/// Review r18 R1 (Art. II): a band over a rotated stroke whose ink is out
/// of view asks for no frames.
#[test]
fn a_band_over_a_rotated_stroke_off_view_asks_for_no_frames() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_rotated_off_view");
    h.app.patch_nodes(&[id], |n| n.rotation_deg = 20.0);
    flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    h.app.tab_mut().cam.offset.y += 3000.0;
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    assert!(
        !h.app.erase_settling(),
        "the band over the rotated bar out of view asks for frames"
    );
}

/// Review r20 R1 (Art. II): a band over a wide, blurred, rotated stroke
/// whose ink box meets the view but whose rotated frame lies past the
/// board's paint query asks for no frames, since the board never paints
/// or rasterizes that stroke there.
#[test]
fn a_band_over_a_blurred_rotated_stroke_past_the_paint_query_asks_for_no_frames() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_rotated_blur_query");
    h.app.patch_nodes(&[id], |n| {
        n.rotation_deg = 1.0;
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.width = 200.0;
            s.stroke.gaussian_blur = 48.0;
        }
    });
    flick_the_bar_unseen(&mut h, &mut raster, id, cross);
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let boxed = node.rect.rotated_bounds(node.rotation_deg);
    let z = h.app.tab().cam.z;
    let half = h.app.canvas_rect.size() * (0.5 / z);
    let top = boxed.y + boxed.h + 225.0;
    h.app.tab_mut().cam.offset.y = top + 80.0 / z + half.y;
    let view = h.app.board_paint_view(h.app.canvas_rect);
    assert!(
        (view.y - top).abs() < 0.5,
        "the paint view's top is {} not {top}",
        view.y
    );
    // The ink box pads the rotated frame by w/2 + 4 + 3 blur = 248.
    assert!(
        boxed.y + boxed.h + 248.0 > view.y,
        "the camera sits past the bar's ink box"
    );
    let painted = h.app.board_paint_nodes(h.app.canvas_rect);
    assert!(
        !painted.iter().any(|n| n.id == id),
        "the board paints the bar past its query"
    );
    h.app.brush_tiles.hold_rasters = false;
    for _ in 0..200 {
        shot(&mut h, &mut raster, |_| {});
    }
    assert!(
        !h.app.erase_settling(),
        "the band over the unpainted rotated bar still asks for frames"
    );
    let quiet = (0..60).any(|_| {
        let out = h.frame_output(|_| {});
        !out.viewport_output[&egui::ViewportId::ROOT]
            .repaint_delay
            .is_zero()
    });
    assert!(quiet, "the idle board keeps repainting");
    assert!(
        !board_path::band_in_view(&node, &view),
        "a band sees a bar the board does not paint"
    );
}

/// Review r12 finding 2 (Art. II): a settling stroke panned out of view
/// still takes its cut and stops settling; nothing waits on the workers and
/// the idle board stops repainting.
#[test]
fn an_eraser_settle_ends_when_its_stroke_is_panned_away() {
    let (mut h, mut raster, id, _) = eraser_bar_board("eraser_settle_pan");
    release_a_settling_pass(&mut h, &mut raster, id);
    let c = h.app.canvas_rect.center();
    let out_of_view = |h: &Harness| {
        let xf = h.app.board_xf();
        let (lo, hi) = (xf.s2w(h.app.canvas_rect.min), xf.s2w(h.app.canvas_rect.max));
        let view = slate_doc::scene::WorldRect::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y);
        let r = h.app.doc().scene.node(id).unwrap().rect;
        let ink =
            slate_doc::scene::WorldRect::new(r.x - 60.0, r.y - 60.0, r.w + 120.0, r.h + 120.0);
        !ink.intersects(&view)
    };
    let mut notches = 0;
    while !out_of_view(&h) {
        notches += 1;
        assert!(notches < 200, "Shift + wheel never panned the bar away");
        shot(&mut h, &mut raster, |i| {
            i.modifiers = egui::Modifiers::SHIFT;
            i.events.push(egui::Event::PointerMoved(c));
            i.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: EVec2::new(0.0, -600.0),
                modifiers: egui::Modifiers::SHIFT,
            });
        });
    }
    h.app.erase_settle.hold = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.erase_settling() || !h.app.brush_tiles.last.settled {
        assert!(
            std::time::Instant::now() < deadline,
            "the panned-away pass never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
    }
    assert_eq!(
        h.app.brush_tiles.lines_wanted_len(),
        0,
        "a cut still waits on the workers"
    );
    assert_eq!(
        h.app.brush_tiles.lines_landed_len(),
        0,
        "a landed cut waits in the pool"
    );
    // The pan's own follow-up repaints end within about 20 frames; a
    // settle that still asked for frames never would.
    let quiet = (0..60).any(|_| {
        let out = h.frame_output(|_| {});
        !out.viewport_output[&egui::ViewportId::ROOT]
            .repaint_delay
            .is_zero()
    });
    assert!(quiet, "the idle board keeps repainting");
}
