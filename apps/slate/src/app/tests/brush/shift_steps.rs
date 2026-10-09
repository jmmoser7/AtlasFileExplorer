//! Shift steps, tab lock, and pen shift connect.

use super::*;

/// Review r10 finding 1 (D03, Art. VI): node ids are numbered per
/// document, so the newest mark in one tab is not a mark in another. A
/// Shift drag in a new tab whose node has the same number as the mark
/// starts at the press and leaves that node alone. Back in the first tab,
/// the newest mark is the one just drawn in the other tab, so the segment
/// starts at the press there too.
#[test]
fn brush_shift_in_another_tab_starts_at_the_press_and_leaves_its_node() {
    let mut h = brush_board("brush_shift_other_tab");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    let mark = h.app.doc().scene.nodes[0].clone();
    let tab_a = h.app.active_tab;
    h.app.new_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    stamped_node_numbered(&mut h, &mark, egui::vec2(0.0, 200.0), mark.id);
    assert_shift_starts_at_the_press(&mut h, Pos2::new(300.0, 320.0));
    h.app.switch_tab(tab_a);
    h.frame();
    assert_eq!(h.app.doc().scene.nodes, vec![mark.clone()]);
    assert_shift_starts_at_the_press(&mut h, Pos2::new(300.0, 120.0));
}

/// Review r10 finding 4: a locked newest mark still gives the segment its
/// start, but the release cannot extend it, so the preview must not either.
/// The live canvas starts empty, the locked stroke keeps painting from the
/// scene, and the release adds exactly one node.
#[test]
fn brush_shift_from_a_locked_mark_starts_at_its_end_as_a_new_stroke() {
    let mut h = brush_board("brush_shift_locked");
    h.app.brush_opacity = 0.5;
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    let id = h.app.doc().scene.nodes[0].id;
    h.app.board_sel = std::iter::once(id).collect();
    assert_eq!(h.app.cmd_lock_selection(), 1);
    let locked = h.app.doc().scene.node(id).unwrap().clone();
    assert!(locked.locked);
    let end = *path_vertices(&locked).last().unwrap();
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(300.0, 320.0),
            Pos2::new(360.0, 330.0),
            Pos2::new(420.0, 330.0),
        ],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, ..) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(
                near_px(from, end),
                "starts at {from:?}, the mark ends at {end:?}"
            );
            let canvas = h.app.brush_live.as_ref().expect("live canvas");
            assert_eq!(canvas.anchor, None, "the canvas starts empty");
            assert_eq!(
                h.app.brush_straight_extends(),
                None,
                "the scene keeps painting it"
            );
        },
    );
    let nodes = &h.app.doc().scene.nodes;
    assert_eq!(nodes.len(), 2, "one new node");
    assert_eq!(nodes[0], locked, "the locked stroke is unchanged");
    assert!(near_px(path_vertices(&nodes[1])[0], end));
}

/// Brush: a Shift click connects to the click at any angle (tip19) and
/// never changes opacity; a Shift drag takes 45° steps; Tab locks it.
#[test]
fn brush_shift_click_connects_and_shift_drag_takes_45_degree_steps() {
    let mut h = brush_board("brush_shift_45");
    h.app.brush_opacity = 0.7;
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(40.0, 40.0),
            Pos2::new(80.0, 60.0),
            Pos2::new(120.0, 40.0),
        ],
        egui::Modifiers::NONE,
        |_| {},
    );
    let first = h.app.doc().scene.nodes[0].id;
    let from = h.app.brush_line_anchor().expect("anchor").pos;
    let click = Pos2::new(230.0, 83.0);
    click_at(&mut h, click, egui::Modifiers::SHIFT);
    assert!(
        (h.app.brush_opacity - 0.7).abs() < 1.0e-6,
        "Shift never steps opacity"
    );
    let v = path_vertices(h.app.doc().scene.node(first).unwrap());
    assert!(
        near_px(v[v.len() - 2], from) && near_px(v[v.len() - 1], click),
        "{v:?}"
    );

    // Shift drag from the new end: 45° steps, and the same stroke grows.
    let from = click;
    let raw = Pos2::new(420.0, 150.0);
    press_drag_release_frames(
        &mut h,
        &[Pos2::new(300.0, 300.0), Pos2::new(350.0, 250.0), raw],
        egui::Modifiers::SHIFT,
        |_| {},
    );
    let v = path_vertices(h.app.doc().scene.node(first).unwrap());
    let end = v[v.len() - 1];
    assert!(on_45(from, end), "{end:?}");
    assert!(
        near_px(end, board_snap::ortho_snap_point(from, raw)),
        "{end:?}"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

/// Brush: Tab during a Shift drag locks the segment direction; the
/// release commits on that ray and ends the lock.
#[test]
fn brush_tab_locks_the_shift_segment_direction() {
    let mut h = brush_board("brush_tab_lock");
    let a = Pos2::new(100.0, 100.0);
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerMoved(xf.w2s(a)));
    });
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerButton {
            pos: xf.w2s(a),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        });
    });
    let aim = Pos2::new(200.0, 200.0);
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerMoved(xf.w2s(aim)));
    });
    press_key_with(&mut h, egui::Key::Tab, shift);
    let lock = h.app.draft_lock.expect("Tab locks the Shift segment");
    let dir = EVec2::new(1.0, 1.0).normalized();
    assert!((lock - dir).length() < 1.0e-3, "{lock:?}");
    let off = Pos2::new(400.0, 120.0);
    h.frame_with(|i| {
        i.modifiers = egui::Modifiers::NONE;
        i.events.push(egui::Event::PointerMoved(xf.w2s(off)));
    });
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: xf.w2s(off),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
    });
    h.frame();
    let v = path_vertices(&h.app.doc().scene.nodes[0]);
    assert!(near_px(v[0], a) && on_ray(a, dir, v[1]), "{v:?}");
    assert!(h.app.draft_lock.is_none(), "the release ends the lock");
}

/// User, 28 September 2026 (Pen add-item form): "shift for strate line" /
/// "at 45 dgree intervals". Shift+drag with the Pen previews a straight
/// segment in 45° steps from the end of the last Pen stroke, not from the
/// press, and the release extends that stroke as one path; one undo takes
/// the segment back.
#[test]
fn pen_shift_drag_extends_the_last_pen_stroke_in_45_degree_steps() {
    let mut h = pen_board("pen_shift_drag");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "one Pen stroke");
    let id = h.app.doc().scene.nodes[0].id;
    let drawn = path_vertices(h.app.doc().scene.node(id).unwrap());
    let end = *drawn.last().unwrap();
    arm_pen(&mut h);
    let raw = Pos2::new(420.0, 150.0);
    let mut previewed = 0;
    press_drag_release_frames(
        &mut h,
        &[Pos2::new(300.0, 300.0), Pos2::new(350.0, 250.0), raw],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, to, _) = h.app.pen_line_preview().expect("a live straight segment");
            assert!(
                near_px(from, end),
                "the preview starts at {from:?}, the stroke ends at {end:?}"
            );
            assert!(on_45(from, to), "the preview takes 45° steps: {to:?}");
            previewed += 1;
        },
    );
    assert_eq!(previewed, 2, "every move frame previews the segment");
    assert!(
        h.app.pen_line_preview().is_none(),
        "the release ends the preview"
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the segment extends the stroke"
    );
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert_eq!(v.len(), drawn.len() + 1, "{v:?}");
    let last = v[v.len() - 1];
    assert!(on_45(end, last), "{last:?}");
    assert!(
        near_px(last, board_snap::ortho_snap_point(end, raw)),
        "{last:?}"
    );
    let node = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Shape(s) = &node.kind else {
        panic!("a shape");
    };
    assert!(
        !s.stroke.paints_as_stamp(),
        "the Pen stays a hard vector stroke"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()),
        drawn,
        "one undo takes the segment back"
    );
}

/// With no Pen stroke yet, a Shift drag starts at its press. A Shift click
/// then connects the end of that stroke to exactly the click point, at any
/// angle.
#[test]
fn pen_shift_starts_at_the_press_without_a_stroke_and_shift_click_connects() {
    let mut h = pen_board("pen_shift_press");
    let (press, raw) = (Pos2::new(100.0, 100.0), Pos2::new(260.0, 180.0));
    press_drag_release_frames(
        &mut h,
        &[press, Pos2::new(180.0, 150.0), raw],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, ..) = h.app.pen_line_preview().expect("a live straight segment");
            assert!(near_px(from, press), "{from:?}");
        },
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let id = h.app.doc().scene.nodes[0].id;
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    let end = board_snap::ortho_snap_point(press, raw);
    assert!(
        v.len() == 2 && near_px(v[0], press) && near_px(v[1], end),
        "{v:?}"
    );

    arm_pen(&mut h);
    let click = Pos2::new(230.0, 83.0);
    click_at(&mut h, click, egui::Modifiers::SHIFT);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the click extends the stroke"
    );
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert_eq!(v.len(), 3, "{v:?}");
    assert!(near_px(v[1], end) && near_px(v[2], click), "{v:?}");
}
