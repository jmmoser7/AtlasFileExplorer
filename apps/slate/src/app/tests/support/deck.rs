//! Deck-order frames.

use super::*;

pub(crate) fn deck_frame(h: &mut Harness, rect: slate_doc::scene::WorldRect, order: u32) -> NodeId {
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Frame(slate_doc::scene::FrameNode {
            title: format!("S{order}"),
            order,
            fill: slate_doc::scene::Rgba::WHITE,
            fill_authored: false,
            assignments: Default::default(),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
        }),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    id
}

pub(crate) fn visible_frames(h: &Harness) -> Vec<NodeId> {
    h.app
        .doc()
        .scene
        .frames_in_order()
        .iter()
        .filter(|n| !n.hidden)
        .map(|n| n.id)
        .collect()
}

pub(crate) fn deck_release(
    h: &mut Harness,
    press: Pos2,
    release_world: Pos2,
    release_screen: Pos2,
) {
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(0.0, 0.0), press, egui::Modifiers::default());
    h.app.end_gesture_for_test(
        release_world,
        Some(release_screen),
        egui::Modifiers::default(),
    );
}
