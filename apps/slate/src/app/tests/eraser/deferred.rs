//! Deferred eraser passes that empty a stroke, and the eraser HUD.

use super::*;

/// Review r12 finding 3: a nested board portal whose stroke has the host
/// bar's id leaves the host's settle alone.
#[test]
fn a_nested_board_portal_does_not_end_the_hosts_eraser_settle() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_settle_nested");
    let c = h.app.canvas_rect.center();
    nested_bar_portal(
        &mut h,
        &mut raster,
        id,
        (c + EVec2::new(150.0, -220.0), c + EVec2::new(410.0, -110.0)),
    );
    let at = (cross, half_lit(&h, &raster, cross));
    let px = release_a_settling_pass(&mut h, &mut raster, id);
    for k in 0..10 {
        shot_settling(
            &mut h,
            &mut raster,
            at,
            |_| {},
            &format!("portal frame {k}"),
        );
        assert!(
            h.app.erase_settle.holds(h.app.tab().id, id),
            "frame {k}: the settle ended"
        );
    }
    settle_watched(&mut h, &mut raster, at, px, "nested portal");
}

/// Review r12 finding 4 (Art. II): the release's decision over a big
/// paint-layer mark stays within the frame's stamp budget, and the mark
/// takes the pass. This counts only `board_path` stamps: the layer
/// composite that re-stamps the erased mark after the commit still runs on
/// the frame loop and is not counted here (review r14 finding 4, open).
#[test]
fn an_eraser_release_over_a_big_layer_mark_rasterizes_nothing() {
    let zig = [
        Pos2::new(40.0, 40.0),
        Pos2::new(200.0, 560.0),
        Pos2::new(380.0, 40.0),
        Pos2::new(560.0, 560.0),
        Pos2::new(760.0, 40.0),
    ];
    let (mut h, image_id) = image_paint_board(
        "eraser_layer_budget",
        slate_doc::scene::WorldRect::new(0.0, 0.0, 800.0, 600.0),
        0.0,
        &zig,
    );
    let before = layer_marks(&h, image_id)[0].clone();
    assert_eq!(before.len(), 1, "one layer mark");
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.sync_image_paint_for_tool();
    h.app.eraser_width = 40.0;
    h.app.eraser_opacity = 1.0;
    h.frame();
    assert!(
        h.app.image_paint_session().is_some(),
        "the Eraser paints on the image"
    );
    let xf = h.app.board_xf();
    let pass: Vec<Pos2> = [60.0, 300.0, 500.0, 740.0]
        .iter()
        .map(|x| xf.w2s(Pos2::new(*x, 300.0)))
        .collect();
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(pass[0])));
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame_with(|i| i.events.push(button(pass[0], true)));
    for p in &pass[1..] {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(*p)));
    }
    let stamps = board_path::stamps_on_this_thread();
    let last = *pass.last().unwrap();
    h.frame_with(|i| i.events.push(button(last, false)));
    assert_eq!(
        board_path::stamps_on_this_thread(),
        stamps,
        "the release rasterized a stroke"
    );
    let after = layer_marks(&h, image_id)[0].clone();
    assert_eq!(after.len(), 1, "the mark keeps its ink");
    assert_eq!(after[0].id, before[0].id);
    assert_eq!(erase_marks(&after[0]).len(), 1, "the mark takes the pass");
}

/// Review r11 pushback 1: a Shift eraser pass that erases all of a big
/// stroke, released before the frame could afford the coarse check, still
/// removes it ("a stroke with no ink left is removed"): once the workers
/// find no ink left, in the pass's own undo step, with nothing stamped on
/// the frame loop.
#[test]
fn a_deferred_eraser_pass_that_empties_a_big_stroke_removes_it_in_the_same_undo_step() {
    let (mut h, id, a, b) = erasable_zigzag("eraser_deferred_empty");
    assert!(zigzag_picks(&h, id) > 0, "the zigzag picks before the pass");
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let counted = flick_eraser(&mut h, id, a, b, true);
    assert_emptied_stroke_leaves_with_its_pass(&mut h, id, &before, depth, counted);
}

/// The same for a freehand pass that crossed the stroke before its preview
/// existed.
#[test]
fn a_deferred_freehand_eraser_pass_that_empties_a_big_stroke_removes_it_too() {
    let (mut h, id, a, b) = erasable_zigzag("eraser_deferred_empty_freehand");
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let counted = flick_eraser(&mut h, id, a, b, false);
    assert_emptied_stroke_leaves_with_its_pass(&mut h, id, &before, depth, counted);
}

/// Undo before the workers answer: the answer removes nothing, and the
/// stroke is as it was before the pass. The same pass again, left alone,
/// removes it in its own step.
#[test]
fn undoing_a_deferred_eraser_pass_before_its_check_lands_removes_nothing() {
    let (mut h, id, a, b) = erasable_zigzag("eraser_deferred_undo");
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    flick_eraser(&mut h, id, a, b, true);
    assert!(h.app.erase_settling(), "the check is on the workers");
    h.app.board_undo();
    settle_erase(&mut h);
    for _ in 0..20 {
        h.frame();
    }
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before),
        "undo restored the stroke"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(
        h.app.tab().journal.can_redo(),
        "the pass waits to be redone"
    );

    forget_last_pass(&mut h, id);
    let counted = flick_eraser(&mut h, id, a, b, true);
    assert_emptied_stroke_leaves_with_its_pass(&mut h, id, &before, depth, counted);
}

/// An edit after the pass, before the workers answer: the pass is no longer
/// the newest undo step, so its group is not amended. The emptied stroke
/// stays, fully erased and not picking; Ctrl+Z undoes the edit and leaves
/// it, a second Ctrl+Z restores it. The same pass again, left alone,
/// removes it in its own step.
#[test]
fn an_edit_after_a_deferred_eraser_pass_keeps_the_emptied_stroke_out_of_its_undo_step() {
    let (mut h, id, a, b) = erasable_zigzag("eraser_deferred_edit");
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    h.app.brush_width = 20.0;
    h.app.finish_freehand_brush(vec![
        Pos2::new(c.x, c.y + 200.0),
        Pos2::new(c.x + 30.0, c.y + 200.0),
    ]);
    let other = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    flick_eraser(&mut h, id, a, b, true);
    let erased = h.app.doc().scene.node(id).unwrap().clone();
    assert!(h.app.erase_settling(), "the check is on the workers");
    h.app.delete_board_nodes(&[other]);
    assert!(h.app.doc().scene.node(other).is_none(), "the edit");
    settle_erase(&mut h);
    for _ in 0..20 {
        h.frame();
    }
    assert!(
        board_color::erased_result(&erased).1,
        "the pass left no ink"
    );
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "the stroke stays"
    );
    assert_eq!(zigzag_picks(&h, id), 0, "the erased stroke picks");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert!(
        h.app.doc().scene.node(other).is_some(),
        "Ctrl+Z undoes the edit"
    );
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&erased),
        "and leaves the pass"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before),
        "then undoes the pass"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);

    forget_last_pass(&mut h, id);
    let counted = flick_eraser(&mut h, id, a, b, true);
    assert_emptied_stroke_leaves_with_its_pass(&mut h, id, &before, depth, counted);
}

#[test]
fn two_quick_deferred_eraser_passes_remove_both_emptied_strokes() {
    two_quick_deferred_passes("eraser_deferred_two_passes", false);
}

/// The same when pass 1's answer lands while pass 2 is drawn: it joins
/// pass 2 at its release.
#[test]
fn a_deferred_ink_answer_that_lands_during_the_next_pass_joins_that_pass() {
    two_quick_deferred_passes("eraser_deferred_two_passes_landed", true);
}

/// A stroke a later pass takes over leaves the selection with the scene,
/// as it does when its own pass's answer removes it.
#[test]
fn a_taken_over_eraser_removal_clears_the_selection() {
    let (mut h, [a, b], [pass_a, pass_b]) = two_erasable_zigzags("eraser_takeover_selection");
    h.app.brush_tiles.hold_inks = true;
    flick_eraser(&mut h, a, pass_a.0, pass_a.1, true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.brush_tiles.ink_landed(a) {
        assert!(
            std::time::Instant::now() < deadline,
            "A's answer never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.app.board_sel = std::iter::once(a).collect();
    h.app.eraser_anchor = None;
    flick_eraser_then(&mut h, b, pass_b.0, pass_b.1, true, |h| {
        h.app.brush_tiles.hold_inks = false;
    });
    assert!(h.app.doc().scene.node(a).is_none(), "pass 2 took A over");
    assert!(
        !h.app.board_sel.contains(&a),
        "A left the selection with the scene"
    );
}

/// A pass the tab refuses to commit takes nothing over: the earlier
/// pass's answer stays with it, and removes the stroke in that pass's
/// step once the tab accepts edits again.
#[test]
fn a_refused_eraser_commit_keeps_the_earlier_passs_ink_answer() {
    let (mut h, [a, b], [pass_a, pass_b]) = two_erasable_zigzags("eraser_refused_takeover");
    let before_a = h.app.doc().scene.node(a).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app.brush_tiles.hold_inks = true;
    flick_eraser(&mut h, a, pass_a.0, pass_a.1, true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.brush_tiles.ink_landed(a) {
        assert!(
            std::time::Instant::now() < deadline,
            "A's answer never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.app.tab_mut().read_only = true;
    h.app.eraser_anchor = None;
    flick_eraser_then(&mut h, b, pass_b.0, pass_b.1, true, |h| {
        h.app.brush_tiles.hold_inks = false;
    });
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "pass 2 was refused"
    );
    h.app.tab_mut().read_only = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.doc().scene.node(a).is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "A stayed in the scene"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
    }
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "A left in pass 1's step"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(a),
        Some(&before_a),
        "one Ctrl+Z restores A"
    );
}

/// Review r13 finding 3: an eraser pass's preview standing in for a big
/// stroke until its new bitmap lands follows the stroke when it is nudged
/// first: at the new place the cut stays erased and the ink left is lit,
/// and nothing stays behind at the old place. Nothing stamps on the frame
/// loop. With the tiles on and off.
#[test]
fn an_erased_stand_in_follows_a_nudge_before_its_bitmap_lands() {
    for tiled in [true, false] {
        let (mut h, mut raster, id, cross) =
            eraser_bar_board(&format!("eraser_stand_in_nudge_{tiled}"));
        h.app.brush_tiles_enabled = tiled;
        let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
        settle_captured(&mut h, &mut raster, "the bar", |app| {
            tiled || stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
        });
        let lit = half_lit(&h, &raster, cross);
        let c = h.app.canvas_rect.center();
        let press = c + EVec2::new(-300.0, -250.0);
        let last = c + EVec2::new(-300.0, 250.0);
        shot(&mut h, &mut raster, shift_at(press, None));
        shot(&mut h, &mut raster, shift_at(press, Some(true)));
        shot(&mut h, &mut raster, shift_at(last, None));
        let mut waited = 0;
        while !h.app.erase_live.get(&id).is_some_and(|l| l.line_exact()) {
            waited += 1;
            assert!(waited < 2000, "the cut never landed");
            std::thread::sleep(std::time::Duration::from_millis(5));
            shot(&mut h, &mut raster, |i| {
                i.modifiers = egui::Modifiers::SHIFT
            });
        }
        h.app.brush_tiles.hold_rasters = true;
        let counted = (
            board_path::stamp_px_on_this_thread(),
            board_path::stamps_on_this_thread(),
        );
        shot(&mut h, &mut raster, shift_at(last, Some(false)));
        assert!(
            stamp_of(&h, id).is_some_and(|(_, g)| !g.exact && g.seal.is_some()),
            "tiles {tiled}: the preview stands in"
        );
        h.app
            .patch_nodes(&[id], |n| n.rect = n.rect.translated(0.0, 200.0));
        let moved = cross + EVec2::new(0.0, 200.0);
        let xf = h.app.board_xf();
        assert!(
            h.app
                .canvas_rect
                .contains(xf.w2s(moved + EVec2::new(0.0, 60.0))),
            "the nudged bar is in view"
        );
        for k in 0..6 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            shot(&mut h, &mut raster, |_| {});
            let when = format!("tiles {tiled}, frame {k} after the nudge");
            for p in cut_points(moved) {
                let r = redness(&raster, &xf, p);
                assert!(
                    r < lit,
                    "{when}: the cut at the new place shows ink at {p:?} ({r:.2})"
                );
            }
            let r = redness(&raster, &xf, moved + EVec2::new(500.0, -3.0));
            assert!(
                r > lit,
                "{when}: the ink left at the new place is dark ({r:.2})"
            );
            for p in [
                cross + EVec2::new(500.0, -3.0),
                cross - EVec2::new(150.0, 0.0),
            ] {
                let r = redness(&raster, &xf, p);
                assert!(
                    r < lit,
                    "{when}: ink stayed behind at the old place {p:?} ({r:.2})"
                );
            }
        }
        let spent = board_path::stamp_px_on_this_thread() - counted.0;
        let built = board_path::stamps_on_this_thread() - counted.1;
        assert_eq!(
            spent, 0,
            "tiles {tiled}: stamped {spent} px on the frame loop"
        );
        assert_eq!(
            built, 0,
            "tiles {tiled}: rasterized {built} strokes on the frame loop"
        );
        h.app.brush_tiles.hold_rasters = false;
    }
}

#[test]
fn eraser_settings_ride_the_same_hud_and_undo() {
    let mut h = Harness::new("eraser_hud");
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 10.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    h.app.tab_mut().cam.z = 1.0;
    h.app.alt_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(40.0, -50.0)), true, false));
    assert!((h.app.eraser_width - 90.0).abs() < 0.01);
    assert!((h.app.eraser_softness - 0.5).abs() < 0.01);
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(40.0, -50.0)), false, false));
    h.app.alt_down = false;
    h.app.shift_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 50.0)), true, false));
    assert!((h.app.eraser_opacity - 0.5).abs() < 0.02);
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 50.0)), false, false));
    assert!((h.app.eraser_tip().rgba[3] as f32 - 127.5).abs() < 3.0);
    h.app.board_undo();
    assert!((h.app.eraser_opacity - 1.0).abs() < 1e-3);
    h.app.board_undo();
    assert!((h.app.eraser_width - 10.0).abs() < 1e-3);
}
