//! Click-away routing must exercise canvas space, clear of participant chrome.
use super::*;

pub(super) fn click_empty_board(h: &mut Harness) {
    let canvas = h.app.canvas_rect;
    let point = canvas.right_bottom() + egui::vec2(-48.0, -48.0);
    assert!(canvas.contains(point));
    let capsule = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("slate-participants")))
        .expect("the participant capsule is present");
    assert!(
        !capsule.contains(point),
        "click-away must miss participant chrome"
    );
    let world = h.app.board_xf().s2w(point);
    assert_eq!(
        h.app.board_pick_node(world.x, world.y),
        None,
        "click-away needs empty canvas"
    );
    click(h, point);
}
