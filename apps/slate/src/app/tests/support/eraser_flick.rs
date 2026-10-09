//! Eraser flicks that empty a zigzag stroke.

use super::*;

/// A painted zigzag across the view at 150 % on a 1.5 px/pt display: too
/// big for the frame's raster budget even as the eraser's coarse check,
/// yet inside the band of the user's eraser (207 wide, softness 0.09) run
/// along its middle. The eraser is smooth: the user's pencil grain leaves
/// ink after one pass. Its tiles are settled and the Eraser is armed.
/// Returns it and the screen ends of a pass that covers all of it.
pub(crate) fn erasable_zigzag(tag: &str) -> (Harness, NodeId, Pos2, Pos2) {
    let mut h = line_board(tag);
    h.ctx.set_pixels_per_point(1.5);
    h.frame_with(|i| i.max_texture_side = Some(8192));
    h.app.tab_mut().cam.z = 1.5;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 20.0;
    let zig = [
        (-300.0, -65.0),
        (-150.0, 65.0),
        (0.0, -65.0),
        (150.0, 65.0),
        (300.0, -65.0),
    ];
    h.app.finish_freehand_brush(
        zig.iter()
            .map(|(x, y)| Pos2::new(c.x + x, c.y + y))
            .collect(),
    );
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    settle_brush(&mut h, "the zigzag", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
    });
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 207.0;
    h.app.eraser_softness = 0.09;
    h.app.eraser_texture = Default::default();
    h.frame();
    let s = h.app.canvas_rect.center();
    (
        h,
        id,
        s - EVec2::new(540.0, 0.0),
        s + EVec2::new(540.0, 0.0),
    )
}

/// An eraser pass from screen `a` to `b` on consecutive frames (press, one
/// move, release), with Shift held when `shift`. Stroke `id` has no preview
/// yet when it is released. Returns the frame-loop stamp px and stamps
/// counted just before the release frame.
pub(crate) fn flick_eraser(
    h: &mut Harness,
    id: NodeId,
    a: Pos2,
    b: Pos2,
    shift: bool,
) -> (u64, u64) {
    flick_eraser_then(h, id, a, b, shift, |_| {})
}

/// [`flick_eraser`], running `before_release` just before the release
/// frame.
pub(crate) fn flick_eraser_then(
    h: &mut Harness,
    id: NodeId,
    a: Pos2,
    b: Pos2,
    shift: bool,
    before_release: impl FnOnce(&mut Harness),
) -> (u64, u64) {
    let modifiers = if shift {
        egui::Modifiers::SHIFT
    } else {
        egui::Modifiers::NONE
    };
    let at = move |s: Pos2, button: Option<bool>| {
        move |i: &mut egui::RawInput| {
            i.modifiers = modifiers;
            i.events.push(egui::Event::PointerMoved(s));
            if let Some(pressed) = button {
                i.events.push(egui::Event::PointerButton {
                    pos: s,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                });
            }
        }
    };
    h.frame_with(at(a, None));
    h.frame_with(at(a, Some(true)));
    h.frame_with(at(b, None));
    assert!(
        !h.app.erase_live.contains_key(&id),
        "the pass is released before its preview exists"
    );
    before_release(h);
    let counted = (
        board_path::stamp_px_on_this_thread(),
        board_path::stamps_on_this_thread(),
    );
    h.frame_with(at(b, Some(false)));
    assert!(h.app.board_drag.is_none(), "the pass is released");
    counted
}

/// Grid points over the zigzag where the board picks stroke `id`.
pub(crate) fn zigzag_picks(h: &Harness, id: NodeId) -> usize {
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let mut picks = 0;
    for i in -40..=40 {
        for j in -10..=10 {
            let (x, y) = (c.x + i as f32 * 8.0, c.y + j as f32 * 8.0);
            if h.app.board_pick_node(x, y) == Some(id) {
                picks += 1;
            }
        }
    }
    picks
}

/// After an eraser release that left stroke `id` with no ink but could not
/// tell within the frame's raster budget: the stroke is still in the scene
/// with the pass's mark, and its erased ink does not pick. Frame by frame
/// until the workers find no ink left, nothing stamps on the frame loop
/// (counted from `counted`, before the release frame). Then the stroke is
/// gone in the pass's own undo step: one Ctrl+Z restores `before`, one
/// Ctrl+Shift+Z removes it again.
pub(crate) fn assert_emptied_stroke_leaves_with_its_pass(
    h: &mut Harness,
    id: NodeId,
    before: &slate_doc::Node,
    depth: usize,
    counted: (u64, u64),
) {
    let node = h
        .app
        .doc()
        .scene
        .node(id)
        .expect("the release defers the check");
    assert_eq!(erase_marks(node).len(), 1, "the release commits the mark");
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the pass is one undo step"
    );
    assert_eq!(zigzag_picks(h, id), 0, "the erased stroke picks");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut frames = 0;
    while h.app.doc().scene.node(id).is_some() {
        if std::time::Instant::now() > deadline {
            let gone = board_color::erased_result(h.app.doc().scene.node(id).unwrap()).1;
            panic!(
                "the emptied stroke stayed in the scene for {frames} frames \
                 (the pass left no ink: {gone})"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
        frames += 1;
    }
    let spent = board_path::stamp_px_on_this_thread() - counted.0;
    let built = board_path::stamps_on_this_thread() - counted.1;
    assert_eq!(
        spent, 0,
        "release to removal stamped {spent} px on the frame loop"
    );
    assert_eq!(
        built, 0,
        "release to removal rasterized {built} strokes on the frame loop"
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "the removal joined the pass's undo step"
    );
    press_key_with(h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(before),
        "one undo restores the stroke"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    press_key_with(
        h,
        egui::Key::Z,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    );
    assert!(
        h.app.doc().scene.node(id).is_none(),
        "one redo removes it again"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}

/// Frames until no eraser pass waits on the workers.
pub(crate) fn settle_erase(h: &mut Harness) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.erase_settling() || !h.app.brush_tiles.last.settled {
        assert!(
            std::time::Instant::now() < deadline,
            "the pass never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
    }
}

/// Drop what the last pass left, its Shift start and its rasters, so the
/// next pass over stroke `id` starts at its press and is released before
/// its preview exists again.
pub(crate) fn forget_last_pass(h: &mut Harness, id: NodeId) {
    h.app.eraser_anchor = None;
    h.app.brush_tiles.clear();
    h.app.brush_stamps.clear();
    settle_brush(h, "the stroke", |app| {
        !app.brush_tiles.tiles_with(id).is_empty()
    });
}

/// [`erasable_zigzag`] moved 110 world units up, and a second zigzag like
/// it 110 below the view's middle, both settled. Returns them and the
/// screen ends of a pass along each that leaves the other alone.
pub(crate) fn two_erasable_zigzags(tag: &str) -> (Harness, [NodeId; 2], [(Pos2, Pos2); 2]) {
    let (mut h, a, s0, s1) = erasable_zigzag(tag);
    h.app
        .patch_nodes(&[a], |n| n.rect = n.rect.translated(0.0, -110.0));
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 20.0;
    let zig = [
        (-300.0, -65.0),
        (-150.0, 65.0),
        (0.0, -65.0),
        (150.0, 65.0),
        (300.0, -65.0),
    ];
    h.app.finish_freehand_brush(
        zig.iter()
            .map(|(x, y)| Pos2::new(c.x + x, c.y + 110.0 + y))
            .collect(),
    );
    let b = h.app.doc().scene.nodes.last().unwrap().id;
    settle_brush(&mut h, "the zigzags", |app| {
        !app.brush_tiles.tiles_with(a).is_empty() && !app.brush_tiles.tiles_with(b).is_empty()
    });
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.frame();
    let dy = EVec2::new(0.0, 110.0 * h.app.tab().cam.z);
    (h, [a, b], [(s0 - dy, s1 - dy), (s0 + dy, s1 + dy)])
}

/// Review r13 finding 1: pass 1 empties big stroke A, then pass 2 empties
/// stroke B before A's ink check is taken in. Both leave the scene in pass
/// 2's undo step, with nothing stamped on the frame loop. With `landed`,
/// A's answer lands during pass 2's drag and joins pass 2 at its release;
/// otherwise it is still held then, and joins once it lands. The first
/// Ctrl+Z brings B back as it was and A as pass 1 left it, with no ink;
/// the second restores A's ink.
pub(crate) fn two_quick_deferred_passes(tag: &str, landed: bool) {
    let (mut h, [a, b], [pass_a, pass_b]) = two_erasable_zigzags(tag);
    let before_a = h.app.doc().scene.node(a).unwrap().clone();
    let before_b = h.app.doc().scene.node(b).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app.brush_tiles.hold_inks = true;
    let counted = flick_eraser(&mut h, a, pass_a.0, pass_a.1, true);
    let erased_a = h
        .app
        .doc()
        .scene
        .node(a)
        .expect("pass 1 defers A's check")
        .clone();
    assert_eq!(erase_marks(&erased_a).len(), 1, "pass 1 commits its mark");
    assert_eq!(
        h.app.doc().scene.node(b),
        Some(&before_b),
        "pass 1 leaves B alone"
    );
    if landed {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !h.app.brush_tiles.ink_landed(a) {
            assert!(
                std::time::Instant::now() < deadline,
                "A's answer never landed"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    h.app.eraser_anchor = None;
    flick_eraser_then(&mut h, b, pass_b.0, pass_b.1, true, |h| {
        if landed {
            h.app.brush_tiles.hold_inks = false;
        }
    });
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 2,
        "each pass is one undo step"
    );
    let node_b = h.app.doc().scene.node(b).expect("pass 2 defers B's check");
    assert_eq!(erase_marks(node_b).len(), 1, "pass 2 commits its mark");
    assert_eq!(
        h.app.doc().scene.node(a).is_none(),
        landed,
        "A leaves at pass 2's release exactly when its answer had landed"
    );
    h.app.brush_tiles.hold_inks = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut frames = 0;
    while h.app.doc().scene.node(a).is_some() || h.app.doc().scene.node(b).is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "after {frames} frames the emptied strokes are still in the scene: A {}, B {}",
            h.app.doc().scene.node(a).is_some(),
            h.app.doc().scene.node(b).is_some()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame();
        frames += 1;
    }
    let spent = board_path::stamp_px_on_this_thread() - counted.0;
    let built = board_path::stamps_on_this_thread() - counted.1;
    assert_eq!(spent, 0, "the passes stamped {spent} px on the frame loop");
    assert_eq!(
        built, 0,
        "the passes rasterized {built} strokes on the frame loop"
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 2,
        "the removals joined pass 2's undo step"
    );
    assert!(
        board_color::erased_result(&erased_a).1,
        "pass 1 left A no ink"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(b),
        Some(&before_b),
        "Ctrl+Z brings B back"
    );
    assert_eq!(
        h.app.doc().scene.node(a),
        Some(&erased_a),
        "and A as pass 1 left it, with no ink"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(a),
        Some(&before_a),
        "the second Ctrl+Z restores A"
    );
    assert_eq!(h.app.doc().scene.node(b), Some(&before_b));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
