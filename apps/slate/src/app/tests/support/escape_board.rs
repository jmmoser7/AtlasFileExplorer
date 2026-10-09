//! Hold a drag, then escape and release.

use super::*;

/// Three 160×120 rects in a row, the camera at 1:1 on the world origin.
/// At that size the rotate zones sit clear of the mid-edge wire grips.
pub(crate) fn esc_drag_board(tag: &str) -> (Harness, [NodeId; 3]) {
    let mut h = align_board(tag);
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::vec2(0.0, 0.0);
    let ids = [-300.0, 0.0, 300.0].map(|x| {
        let id = add_rect(&mut h.app, x, 0.0);
        h.app.doc_mut().scene.node_mut(id).unwrap().rect =
            slate_doc::scene::WorldRect::new(x, 0.0, 160.0, 120.0);
        id
    });
    h.frame();
    (h, ids)
}

/// Real frames: press at `from`, then move to `to` with the button held.
/// The drag is still live when this returns.
pub(crate) fn hold_drag(h: &mut Harness, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let xf = h.app.board_xf();
    let (a, b) = (xf.w2s(from), xf.w2s(to));
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerMoved(a));
    });
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
    });
    for t in [0.25, 0.5, 0.75, 1.0] {
        let p = a + (b - a) * t;
        h.frame_with(|i| {
            i.modifiers = modifiers;
            i.events.push(egui::Event::PointerMoved(p));
        });
    }
}

/// Esc with the button still down (egui aborts its drag here, so no
/// `drag_stopped` follows), then the release.
pub(crate) fn escape_then_release(h: &mut Harness, at: Pos2, modifiers: egui::Modifiers) {
    let p = h.app.board_xf().w2s(at);
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(key_event(egui::Key::Escape, modifiers));
    });
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
    });
    h.frame();
}

pub(crate) fn scene_nodes(h: &Harness) -> Vec<slate_doc::scene::Node> {
    h.app.doc().scene.nodes.clone()
}
