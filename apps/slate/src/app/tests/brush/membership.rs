//! Brushes use the same geometric frame ownership and movement as every node.
use super::*;

#[test]
fn a_brush_stroke_inside_a_frame_is_carried_with_that_frame() {
    let mut h = brush_board("brush-frame-membership");
    let frame = deck_frame(&mut h, WorldRect::new(-200.0, -200.0, 400.0, 400.0), 0);
    h.app.board_sel.clear();
    press_drag_release_frames(
        &mut h,
        &[Pos2::new(-50.0, -70.0), Pos2::new(20.0, 30.0)],
        egui::Modifiers::NONE,
        |_| {},
    );
    let stroke = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|node| matches!(&node.kind, NodeKind::Shape(shape) if shape.stroke.stamp))
        .expect("one brush stroke")
        .id;
    assert_eq!(h.app.doc().scene.frame_of(stroke), Some(frame));
    assert!(h.app.expand_with_members(&[frame]).contains(&stroke));
    let before = h.app.doc().scene.node(stroke).unwrap().rect;
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.board_sel = [frame].into();
    press_key(&mut h, egui::Key::ArrowRight);
    let after = h.app.doc().scene.node(stroke).unwrap().rect;
    assert!(
        after.x > before.x,
        "moving the frame carries the brush stroke"
    );
    assert_eq!(h.app.doc().scene.frame_of(stroke), Some(frame));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(stroke).unwrap().rect, before);
}
