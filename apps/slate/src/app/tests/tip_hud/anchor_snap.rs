//! A dragged anchor snaps to another anchor on its curve.

use super::*;

#[test]
fn a_dragged_anchor_snaps_to_another_anchor_on_its_own_curve() {
    let mut h = Harness::new("direct_snap_self");
    let id = add_seg(&mut h.app, Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0));
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    let (anchors, closed) = h.app.direct_anchors_of(id).unwrap();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    // Grab the start anchor a little off its center and carry it next to
    // the end anchor: it lands exactly on the end anchor, not offset by the
    // grab.
    let grab = Pos2::new(2.0, 1.0);
    h.app.board_drag = Some(board::BoardDrag::Direct(
        board_direct::DirectDrag::Anchors {
            node: id,
            before,
            anchors0: anchors.clone(),
            closed,
            indices: vec![0],
            start: grab,
        },
    ));
    h.app
        .update_direct_drag(Pos2::new(99.0, 3.0), egui::Modifiers::NONE);
    let (moved, _) = h.app.direct_anchors_of(id).unwrap();
    assert!(
        (moved[0].point.x - 100.0).abs() < 1e-3 && moved[0].point.y.abs() < 1e-3,
        "anchor at {:?}",
        moved[0].point
    );
}
