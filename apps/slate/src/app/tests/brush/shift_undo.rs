//! Brush shift after undo, redo, delete, and hide.

use super::*;

/// D03: the Shift anchor follows the journal. After Ctrl+Z takes back a
/// Shift segment, the next Shift drag previews from the stroke's end as it
/// is now and extends that same stroke; one undo takes the new segment back.
#[test]
fn brush_shift_after_undo_starts_at_the_strokes_real_end() {
    let mut h = brush_board("brush_shift_undo_anchor");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    let id = h.app.doc().scene.nodes[0].id;
    let drawn = path_vertices(h.app.doc().scene.node(id).unwrap());
    let end = *drawn.last().unwrap();
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(200.0, 200.0),
            Pos2::new(260.0, 200.0),
            Pos2::new(300.0, 200.0),
        ],
        egui::Modifiers::SHIFT,
        |_| {},
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()).len(),
        drawn.len() + 1
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()),
        drawn,
        "undone"
    );
    let raw = Pos2::new(420.0, 330.0);
    press_drag_release_frames(
        &mut h,
        &[Pos2::new(300.0, 320.0), Pos2::new(360.0, 330.0), raw],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, _, node) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(
                near_px(from, end),
                "the preview starts at {from:?}, the stroke ends at {end:?}"
            );
            assert_eq!(node, Some(id), "the segment continues the stroke");
            let shown = h.app.brush_live.as_ref().and_then(|c| c.live_line_start());
            assert!(
                shown.is_some_and(|s| near_px(Pos2::new(s[0], s[1]), end)),
                "the live canvas starts the segment at {shown:?}"
            );
        },
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the segment extends the stroke"
    );
    let v = path_vertices(h.app.doc().scene.node(id).unwrap());
    assert_eq!(v.len(), drawn.len() + 1, "{v:?}");
    assert!(
        near_px(v[v.len() - 1], board_snap::ortho_snap_point(end, raw)),
        "{v:?}"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()),
        drawn,
        "one undo takes the segment back"
    );
}

/// D03 after a redo: Ctrl+Z takes a Shift segment back and Ctrl+Y restores
/// it, so the next Shift drag starts at the redone end and extends the
/// same stroke.
#[test]
fn brush_shift_after_redo_starts_at_the_redone_end() {
    let mut h = brush_board("brush_shift_redo");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    let id = h.app.doc().scene.nodes[0].id;
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(200.0, 200.0),
            Pos2::new(260.0, 200.0),
            Pos2::new(300.0, 200.0),
        ],
        egui::Modifiers::SHIFT,
        |_| {},
    );
    let extended = path_vertices(h.app.doc().scene.node(id).unwrap());
    let redone_end = *extended.last().unwrap();
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()).len(),
        extended.len() - 1
    );
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()),
        extended,
        "redone"
    );
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(300.0, 320.0),
            Pos2::new(360.0, 330.0),
            Pos2::new(420.0, 330.0),
        ],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, _, node) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(
                near_px(from, redone_end),
                "starts at {from:?}, redone end {redone_end:?}"
            );
            assert_eq!(node, Some(id));
        },
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the segment extends the stroke"
    );
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()).len(),
        extended.len() + 1
    );
}

/// D03, current rule (the user has not ruled on it; review r10 Q1):
/// deleting the newest mark makes the newest remaining visible mark the
/// start. Rename and flip this test if the rule becomes "the press".
#[test]
fn brush_shift_after_deleting_the_newest_mark_starts_at_the_newest_remaining_mark() {
    let mut h = brush_board("brush_shift_delete_newest");
    let (first, second) = two_brush_marks(&mut h);
    h.app.delete_board_nodes(&[second]);
    assert!(h.app.doc().scene.node(second).is_none());
    assert_shift_continues(&mut h, first, Pos2::new(300.0, 320.0));
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

/// D03, current rule (review r10 Q1): hiding the newest mark (Ctrl+H) makes
/// the newest remaining visible mark the start, and the hidden stroke is
/// left alone. Rename and flip this test if the rule becomes "the press".
#[test]
fn brush_shift_after_hiding_the_newest_mark_starts_at_the_newest_remaining_mark() {
    let mut h = brush_board("brush_shift_hide_newest");
    let (first, second) = two_brush_marks(&mut h);
    let hidden = h.app.doc().scene.node(second).unwrap().clone();
    h.app.board_sel = std::iter::once(second).collect();
    assert_eq!(h.app.cmd_hide_selection(), 1);
    assert_shift_continues(&mut h, first, Pos2::new(300.0, 320.0));
    let after = h.app.doc().scene.node(second).unwrap();
    assert!(after.hidden);
    assert_eq!(
        path_vertices(after),
        path_vertices(&hidden),
        "the hidden stroke is untouched"
    );
}

/// D03 with no mark left: after Ctrl+Z twice takes back the segment and the
/// stroke, the next Shift drag starts at its own press.
#[test]
fn brush_shift_after_undoing_the_whole_stroke_starts_at_the_press() {
    let mut h = brush_board("brush_shift_undo_all");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(200.0, 200.0),
            Pos2::new(260.0, 200.0),
            Pos2::new(300.0, 200.0),
        ],
        egui::Modifiers::SHIFT,
        |_| {},
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert!(h.app.doc().scene.nodes.is_empty(), "the stroke is gone");
    let (press, raw) = (Pos2::new(300.0, 320.0), Pos2::new(420.0, 330.0));
    press_drag_release_frames(
        &mut h,
        &[press, Pos2::new(360.0, 330.0), raw],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, _, node) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(
                near_px(from, press),
                "the preview starts at {from:?}, not the press"
            );
            assert_eq!(node, None);
        },
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let v = path_vertices(&h.app.doc().scene.nodes[0]);
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(near_px(v[0], press), "{v:?}");
    assert!(
        near_px(v[1], board_snap::ortho_snap_point(press, raw)),
        "{v:?}"
    );
}

/// D03 across marks: undoing the newest stroke leaves the one before it as
/// the most recent mark, so the next Shift drag continues that one.
#[test]
fn brush_shift_after_undoing_the_newest_mark_continues_the_one_before() {
    let mut h = brush_board("brush_shift_undo_newest");
    let first = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &first, egui::Modifiers::NONE, |_| {});
    let id = h.app.doc().scene.nodes[0].id;
    let drawn = path_vertices(h.app.doc().scene.node(id).unwrap());
    let second = [
        Pos2::new(40.0, 160.0),
        Pos2::new(80.0, 180.0),
        Pos2::new(120.0, 160.0),
    ];
    press_drag_release_frames(&mut h, &second, egui::Modifiers::NONE, |_| {});
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(300.0, 320.0),
            Pos2::new(360.0, 330.0),
            Pos2::new(420.0, 330.0),
        ],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, _, node) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(near_px(from, *drawn.last().unwrap()), "starts at {from:?}");
            assert_eq!(node, Some(id));
        },
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the segment extends the first stroke"
    );
    assert_eq!(
        path_vertices(h.app.doc().scene.node(id).unwrap()).len(),
        drawn.len() + 1
    );
}
