//! Eraser settle, flick, and undo while a cut is in flight.

use super::*;

/// Review r10 finding 3: an eraser Shift release whose final cut landed on
/// the workers after the last paint takes that cut in. The release frame
/// stamps nothing on the frame loop and commits the pass's erase mark.
#[test]
fn eraser_shift_release_takes_the_cut_that_landed_since_the_last_paint() {
    let (mut h, _, id, _) = eraser_bar_board("eraser_shift_landed");
    let c = h.app.canvas_rect.center();
    let press = c + EVec2::new(-300.0, -250.0);
    let first = c + EVec2::new(-300.0, 150.0);
    let last = c + EVec2::new(-300.0, 250.0);
    h.frame_with(shift_at(press, None));
    h.frame_with(shift_at(press, Some(true)));
    h.frame_with(shift_at(first, None));
    let mut waited = 0;
    while !h.app.erase_live.get(&id).is_some_and(|l| l.line_exact()) {
        waited += 1;
        assert!(waited < 2000, "the first cut never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame_with(|i| i.modifiers = egui::Modifiers::SHIFT);
    }
    let t = std::time::Instant::now();
    h.frame_with(shift_at(last, None));
    let move_ms = t.elapsed().as_secs_f64() * 1.0e3;
    assert!(
        !h.app.erase_live[&id].line_exact(),
        "the move asked for a new cut"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.brush_tiles.lines_landed_len() == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the new cut never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let expected = erase_after_pass(&h, id);
    let (px, stamps) = (
        board_path::stamp_px_on_this_thread(),
        board_path::stamps_on_this_thread(),
    );
    let t = std::time::Instant::now();
    h.frame_with(shift_at(last, Some(false)));
    let ms = t.elapsed().as_secs_f64() * 1.0e3;
    let spent = board_path::stamp_px_on_this_thread() - px;
    eprintln!("landed-cut release frame: {ms:.1} ms (move {move_ms:.1} ms), {spent} px stamped");
    assert_eq!(spent, 0, "the release stamped {spent} px on the frame loop");
    assert_eq!(
        board_path::stamps_on_this_thread(),
        stamps,
        "the release rasterized a stroke"
    );
    let node = h.app.doc().scene.node(id).expect("the bar survives");
    assert_eq!(erase_marks(node), expected, "the committed erase mark");
}

/// A Shift flick (press, one move, release on consecutive frames) across a
/// big stroke: the release frame rasterizes nothing on the frame loop, the
/// stroke never shows un-erased where the pass cut, and once the rasters
/// land the board shows the committed erase.
#[test]
fn an_eraser_shift_flick_stamps_nothing_on_release_and_never_shows_the_uncut_stroke() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_shift_flick");
    let c = h.app.canvas_rect.center();
    let press = c + EVec2::new(-300.0, -250.0);
    let last = c + EVec2::new(-300.0, 250.0);
    shot(&mut h, &mut raster, shift_at(press, None));
    let xf = h.app.board_xf();
    let lit = redness(&raster, &xf, cross);
    assert!(lit > 0.6, "the bar is lit before the pass ({lit:.2})");
    shot(&mut h, &mut raster, shift_at(press, Some(true)));
    shot(&mut h, &mut raster, shift_at(last, None));
    let expected = erase_after_pass(&h, id);
    let (px, stamps) = (
        board_path::stamp_px_on_this_thread(),
        board_path::stamps_on_this_thread(),
    );
    let t = std::time::Instant::now();
    let out = capture_frame(&mut h, &mut raster, shift_at(last, Some(false)));
    let ms = t.elapsed().as_secs_f64() * 1.0e3;
    draw_now(&h, &mut raster, out);
    let spent = board_path::stamp_px_on_this_thread() - px;
    let built = board_path::stamps_on_this_thread() - stamps;
    eprintln!("flick release frame: {ms:.1} ms, {spent} px, {built} stroke stamps");
    assert_eq!(spent, 0, "the release stamped {spent} px on the frame loop");
    assert_eq!(built, 0, "the release rasterized a stroke");
    let node = h.app.doc().scene.node(id).expect("the bar survives");
    assert_eq!(
        erase_marks(node),
        expected,
        "the release commits the erase mark at once"
    );
    for p in cut_points(cross) {
        let r = redness(&raster, &xf, p);
        assert!(
            r < 0.42,
            "the release frame shows the uncut bar at {p:?} ({r:.2})"
        );
    }
    assert_never_uncut_after_release(&mut h, &mut raster, cross);
}

/// The same with the preview built: the move asks for the final cut and the
/// release comes on the next frame, before that cut lands. The preview and
/// the band stand in until it does; nothing stamps on the frame loop.
#[test]
fn an_eraser_shift_release_with_its_cut_in_flight_keeps_the_preview() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_shift_inflight");
    let c = h.app.canvas_rect.center();
    let press = c + EVec2::new(-300.0, -250.0);
    let first = c + EVec2::new(-300.0, -120.0);
    let last = c + EVec2::new(-300.0, 250.0);
    capture_frame(&mut h, &mut raster, shift_at(press, None));
    capture_frame(&mut h, &mut raster, shift_at(press, Some(true)));
    capture_frame(&mut h, &mut raster, shift_at(first, None));
    let mut waited = 0;
    while !h.app.erase_live.get(&id).is_some_and(|l| l.line_exact()) {
        waited += 1;
        assert!(waited < 2000, "the first cut never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = egui::Modifiers::SHIFT
        });
    }
    let t = std::time::Instant::now();
    let out = capture_frame(&mut h, &mut raster, shift_at(last, None));
    let move_ms = t.elapsed().as_secs_f64() * 1.0e3;
    draw_now(&h, &mut raster, out);
    let expected = erase_after_pass(&h, id);
    let (px, stamps) = (
        board_path::stamp_px_on_this_thread(),
        board_path::stamps_on_this_thread(),
    );
    let t = std::time::Instant::now();
    let out = capture_frame(&mut h, &mut raster, shift_at(last, Some(false)));
    let ms = t.elapsed().as_secs_f64() * 1.0e3;
    draw_now(&h, &mut raster, out);
    let spent = board_path::stamp_px_on_this_thread() - px;
    eprintln!("in-flight release frame: {ms:.1} ms (move {move_ms:.1} ms), {spent} px stamped");
    assert_eq!(spent, 0, "the release stamped {spent} px on the frame loop");
    assert_eq!(
        board_path::stamps_on_this_thread(),
        stamps,
        "the release rasterized a stroke"
    );
    let node = h.app.doc().scene.node(id).expect("the bar survives");
    assert_eq!(erase_marks(node), expected, "the committed erase mark");
    let xf = h.app.board_xf();
    for p in cut_points(cross) {
        let r = redness(&raster, &xf, p);
        assert!(
            r < 0.42,
            "the release frame shows the uncut bar at {p:?} ({r:.2})"
        );
    }
    assert_never_uncut_after_release(&mut h, &mut raster, cross);
}

/// Review r12 finding 1: a second eraser pass over a stroke whose first
/// pass still settles never shows the first pass's cut un-erased, nor the
/// stroke blank: (i) a Shift pass released unchanged, over ink the first
/// erased; (ii) a Shift flick released before its preview exists; (iii) a
/// pass pressed, then Esc.
#[test]
fn a_second_eraser_pass_over_a_settling_stroke_never_shows_the_uncut_stroke() {
    for variant in ["unchanged", "flick", "escape"] {
        let (mut h, mut raster, id, cross) =
            eraser_bar_board(&format!("eraser_settle_second_{variant}"));
        let at = (cross, half_lit(&h, &raster, cross));
        if variant == "unchanged" {
            // A grained pass leaves specks of ink for pass 2 to take.
            h.app.eraser_texture = slate_doc::scene::BrushTexture::Smooth;
        }
        let px = release_a_settling_pass(&mut h, &mut raster, id);
        let c = h.app.canvas_rect.center();
        let shift = egui::Modifiers::SHIFT;
        let mut watch =
            |h: &mut Harness, prepare: Box<dyn FnOnce(&mut egui::RawInput)>, when: &str| {
                shot_settling(h, &mut raster, at, prepare, &format!("{variant}: {when}"));
            };
        type Prepare = Box<dyn FnOnce(&mut egui::RawInput)>;
        let wait = |h: &mut Harness,
                    watch: &mut dyn FnMut(&mut Harness, Prepare, &str),
                    ready: &dyn Fn(&Harness) -> bool| {
            let mut waited = 0;
            while !ready(h) {
                waited += 1;
                assert!(waited < 2000, "{variant}: pass 2's preview never came");
                std::thread::sleep(std::time::Duration::from_millis(5));
                watch(h, Box::new(move |i| i.modifiers = shift), "pass 2 drag");
            }
        };
        match variant {
            "unchanged" => {
                // Narrower, back along pass 1: only ink it erased.
                h.app.eraser_width = 100.0;
                let back = c + EVec2::new(-300.0, -250.0);
                watch(&mut h, Box::new(shift_at(back, None)), "pass 2 hover");
                watch(&mut h, Box::new(shift_at(back, Some(true))), "pass 2 press");
                wait(&mut h, &mut watch, &|h| {
                    h.app.erase_live.get(&id).is_some_and(|l| l.line_exact())
                });
                watch(
                    &mut h,
                    Box::new(shift_at(back, Some(false))),
                    "pass 2 release",
                );
                let node = h.app.doc().scene.node(id).unwrap();
                assert_eq!(erase_marks(node).len(), 1, "pass 2 changed nothing");
            }
            "flick" => {
                let to = c + EVec2::new(-100.0, -250.0);
                watch(&mut h, Box::new(shift_at(to, None)), "pass 2 hover");
                watch(&mut h, Box::new(shift_at(to, Some(true))), "pass 2 press");
                watch(&mut h, Box::new(shift_at(to, None)), "pass 2 move");
                // Pass 1's ink check left the stroke's raster ready, so pass
                // 2's preview started at once: drop it, as when that raster
                // has not landed yet.
                if let Some(mut l) = h.app.erase_live.remove(&id) {
                    l.forget(&mut h.app.brush_tiles, board_path::tiles::erase_lane(id));
                }
                watch(
                    &mut h,
                    Box::new(shift_at(to, Some(false))),
                    "pass 2 release",
                );
                let node = h.app.doc().scene.node(id).unwrap();
                assert_eq!(erase_marks(node).len(), 2, "pass 2 commits at once");
            }
            _ => {
                let to = c + EVec2::new(-150.0, -250.0);
                watch(&mut h, Box::new(shift_at(to, None)), "pass 2 hover");
                watch(&mut h, Box::new(shift_at(to, Some(true))), "pass 2 press");
                wait(&mut h, &mut watch, &|h| h.app.erase_live.contains_key(&id));
                let esc = move |i: &mut egui::RawInput| {
                    i.modifiers = shift;
                    i.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: shift,
                    });
                };
                watch(&mut h, Box::new(esc), "Esc");
                assert!(h.app.board_drag.is_none(), "Esc ends pass 2");
                let node = h.app.doc().scene.node(id).unwrap();
                assert_eq!(erase_marks(node).len(), 1, "Esc changed nothing");
            }
        }
        assert!(
            h.app.erase_settle.holds(h.app.tab().id, id),
            "{variant}: pass 1 still settles"
        );
        settle_watched(&mut h, &mut raster, at, px, variant);
    }
}

/// The same across a tab switch: another tab for a frame after the
/// release, then back.
#[test]
fn a_tab_switch_during_an_eraser_settle_never_shows_the_uncut_stroke() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_settle_tabs");
    let at = (cross, half_lit(&h, &raster, cross));
    let px = release_a_settling_pass(&mut h, &mut raster, id);
    let first = h.app.active_tab;
    h.app.new_tab();
    shot(&mut h, &mut raster, |_| {});
    h.app.switch_tab(first);
    shot_settling(&mut h, &mut raster, at, |_| {}, "back on the tab");
    assert!(
        h.app.erase_settle.holds(h.app.tab().id, id),
        "the pass still settles"
    );
    settle_watched(&mut h, &mut raster, at, px, "tab switch");
}

/// The same when the stroke moves while its pass settles: the preview and
/// its band move with it.
#[test]
fn moving_a_stroke_during_its_eraser_settle_never_shows_the_uncut_stroke() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_settle_move");
    let lit = half_lit(&h, &raster, cross);
    let px = release_a_settling_pass(&mut h, &mut raster, id);
    h.app.patch_nodes(&[id], |n| n.rect.x += 60.0);
    let at = (cross + EVec2::new(60.0, 0.0), lit);
    shot_settling(&mut h, &mut raster, at, |_| {}, "the move frame");
    assert!(
        h.app.erase_settle.holds(h.app.tab().id, id),
        "the pass still settles"
    );
    settle_watched(&mut h, &mut raster, at, px, "move");
}

/// Review r12 note N3: an undo after a settled pass became the stroke's
/// stand-in paints the restored ink, not the erased stand-in, until the
/// restored raster lands. Tiles are off so the stroke keeps a bitmap of
/// its own.
#[test]
fn undoing_an_eraser_pass_after_it_settles_paints_the_restored_ink() {
    undo_a_settled_eraser_pass("eraser_settle_undo", false, false);
}

/// Review r14 finding 7: Ctrl+Y after that undo paints the erased stroke
/// again, not the restored ink, until the redone stroke's raster lands.
#[test]
fn redoing_an_undone_eraser_pass_paints_the_erased_ink() {
    undo_a_settled_eraser_pass("eraser_settle_redo", true, false);
}

/// Review r16 D1: the same when the restored stroke's bitmap was rebuilt
/// at another zoom between the undo and the redo.
#[test]
fn redoing_an_eraser_pass_after_a_zoom_paints_the_erased_ink() {
    undo_a_settled_eraser_pass("eraser_settle_redo_zoom", true, true);
}

/// Review r14 finding 1(a): Delete on a picked end vertex of a stroke whose
/// eraser pass settled into its stand-in keeps that stand-in: a vertex edit
/// is no undo, so the stroke's bitmap from before the pass never paints
/// again and the cut stays erased until the edited stroke's raster lands.
#[test]
fn deleting_a_vertex_before_an_erased_bitmap_lands_never_paints_the_uncut_stroke() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_stand_in_vertex_delete");
    h.app.brush_tiles_enabled = false;
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    settle_captured(&mut h, &mut raster, "the bar's own bitmap", |app| {
        stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
    });
    let uncut = stamp_of(&h, id).unwrap().1.tex.id();
    let lit = half_lit(&h, &raster, cross);
    settle_into_stand_in(&mut h, &mut raster, id);
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.board_sel = [id].into_iter().collect();
    h.app.direct.grip_points = Default::default();
    let end = world_anchor_points(&h, id).len() - 1;
    h.app.direct.grip_points.pick(id, end, false);
    assert!(
        h.app.delete_picked_vertices(),
        "Delete takes the end vertex"
    );
    h.app.board_sel.clear();
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).expect("the bar stays"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut frames = 0;
    loop {
        if frames == 6 {
            h.app.brush_tiles.hold_rasters = false;
        }
        shot(&mut h, &mut raster, |_| {});
        assert_ne!(
            stamp_of(&h, id).map(|(_, g)| g.tex.id()),
            Some(uncut),
            "frame {frames} after the delete: the bar's bitmap from before the pass is back"
        );
        let xf = h.app.board_xf();
        for p in cut_points(cross) {
            let r = redness(&raster, &xf, p);
            assert!(
                r < lit,
                "frame {frames} after the delete: {p:?} shows the uncut bar ({r:.2})"
            );
        }
        let current = stamp_of(&h, id).is_some_and(|(k, g)| g.exact && Some(*k) == key);
        if frames >= 6 && current && !h.app.erase_settling() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the edited bar never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        frames += 1;
    }
}

/// Review r14 finding 1(b): another tab whose stroke has the bar's node id
/// never paints the bar's bitmaps nor ends its stand-in; back on the bar's
/// tab the cut stays erased.
#[test]
fn another_tabs_stroke_with_the_same_id_never_takes_the_eraser_stand_in() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_stand_in_other_tab");
    h.app.brush_tiles_enabled = false;
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let key = board_path::node_stamp_key(&before);
    settle_captured(&mut h, &mut raster, "the bar's own bitmap", |app| {
        stamp_of_app(app, id).is_some_and(|(k, g)| g.exact && Some(*k) == key)
    });
    let uncut = stamp_of(&h, id).unwrap().1.tex.id();
    let lit = half_lit(&h, &raster, cross);
    settle_into_stand_in(&mut h, &mut raster, id);
    let stand_in = stamp_of(&h, id).unwrap().1.tex.id();
    let first = h.app.active_tab;
    h.app.new_tab();
    let mut other = before.clone();
    other.opacity = 0.5;
    h.app.doc_mut().scene.nodes.push(other);
    h.app.note_scene_change();
    for k in 0..5 {
        let out = capture_frame(&mut h, &mut raster, |_| {});
        let painted = painted_textures(&out);
        assert!(
            !painted.contains(&uncut) && !painted.contains(&stand_in),
            "frame {k} on the other tab: it paints the bar's bitmap"
        );
        draw_now(&h, &mut raster, out);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.app.switch_tab(first);
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut frames = 0;
    loop {
        if frames == 6 {
            h.app.brush_tiles.hold_rasters = false;
        }
        shot(&mut h, &mut raster, |_| {});
        let xf = h.app.board_xf();
        for p in cut_points(cross) {
            let r = redness(&raster, &xf, p);
            assert!(
                r < lit,
                "frame {frames} back on the tab: {p:?} shows the uncut bar ({r:.2})"
            );
        }
        let current = stamp_of(&h, id).is_some_and(|(k, g)| g.exact && Some(*k) == key);
        if frames >= 6 && current {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the erased bar never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        frames += 1;
    }
}

/// Review r14 finding 6: closing a tab while one of its eraser passes
/// settles frees that settle and its cut on the workers at once.
#[test]
fn closing_a_tab_during_an_eraser_settle_frees_it() {
    let (mut h, mut raster, id, _) = eraser_bar_board("eraser_settle_close");
    release_a_settling_pass(&mut h, &mut raster, id);
    let first = h.app.active_tab;
    h.app.new_tab();
    shot(&mut h, &mut raster, |_| {});
    h.app.force_close_tab(first);
    assert!(
        h.app.erase_settle.is_empty(),
        "the closed tab's settle is kept"
    );
    assert_eq!(
        h.app.brush_tiles.lines_wanted_len(),
        0,
        "the closed tab's cut still waits on the workers"
    );
}

/// Review r14 finding 3: a Shift flick released before its preview exists
/// keeps its band while the stroke's ink shows at the view's edge, even
/// with the stroke's centerline out of view.
#[test]
fn a_band_at_the_view_edge_stays_while_its_ink_shows() {
    let (mut h, mut raster, id, cross) = eraser_bar_board("eraser_band_view_edge");
    let (seen, lit) = bar_at_the_view_bottom(&mut h, &mut raster, id, cross);
    h.app.brush_tiles.hold_rasters = true;
    let c = h.app.canvas_rect.center();
    let press = Pos2::new(c.x - 300.0, h.app.canvas_rect.min.y + 80.0);
    let last = Pos2::new(c.x - 300.0, h.app.canvas_rect.max.y - 4.0);
    shot(&mut h, &mut raster, shift_at(press, None));
    shot(&mut h, &mut raster, shift_at(press, Some(true)));
    shot(&mut h, &mut raster, shift_at(last, None));
    assert!(
        !h.app.erase_live.contains_key(&id),
        "no preview before the release"
    );
    shot(&mut h, &mut raster, shift_at(last, Some(false)));
    assert_eq!(
        erase_marks(h.app.doc().scene.node(id).unwrap()).len(),
        1,
        "the flick commits"
    );
    band_holds_at_the_edge(&mut h, &mut raster, (id, seen), lit, "flick");
}
