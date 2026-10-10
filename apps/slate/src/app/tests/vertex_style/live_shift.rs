//! Ortho shift steps and a texture change restamping the live segment.

use super::*;

/// Review r7 finding 7: on painted ink Shift is the straight-line modifier,
/// so with Ortho (F8) on a Shift drag still takes 45° steps.
#[test]
fn brush_shift_drag_keeps_45_degree_steps_with_ortho_on() {
    let mut h = brush_board("brush_shift_ortho");
    press_key_with(&mut h, egui::Key::F8, egui::Modifiers::NONE);
    assert!(h.app.board_ortho, "F8 turns Ortho on");
    let a = Pos2::new(100.0, 100.0);
    let end = Pos2::new(260.0, 180.0);
    press_drag_release_frames(
        &mut h,
        &[a, Pos2::new(220.0, 90.0), end],
        egui::Modifiers::SHIFT,
        |_| {},
    );
    let v = path_vertices(&h.app.doc().scene.nodes[0]);
    assert_eq!(v.len(), 2, "a straight segment: {v:?}");
    assert!(on_45(v[0], v[1]), "Ortho did not cancel the steps: {v:?}");
    assert!(near_px(v[1], board_snap::ortho_snap_point(a, end)), "{v:?}");
}

/// Review r7 finding 8: a texture change during a live Shift segment
/// restamps it (each tip's grain is part of the segment's identity).
#[test]
fn a_texture_change_restamps_the_live_shift_segment() {
    let mut h = brush_board("brush_shift_texture");
    let xf = h.app.board_xf();
    let (a, b) = (
        xf.w2s(Pos2::new(100.0, 100.0)),
        xf.w2s(Pos2::new(260.0, 100.0)),
    );
    let shift = egui::Modifiers::SHIFT;
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: shift,
    };
    let exact = |h: &Harness| h.app.brush_live.as_ref().is_some_and(|c| c.line_exact());
    let settle = |h: &mut Harness| {
        for _ in 0..400 {
            if exact(h) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            h.frame_with(|i| i.modifiers = shift);
        }
        false
    };
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerMoved(a));
    });
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(button(a, true));
    });
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerMoved(b));
    });
    assert!(settle(&mut h), "the smooth segment's exact stamp lands");
    h.frame_with(|i| i.modifiers = shift);
    assert!(exact(&h), "a still frame keeps the stamp");
    h.app.brush_texture = slate_doc::scene::BrushTexture::Pencil;
    h.frame_with(|i| i.modifiers = shift);
    assert!(!exact(&h), "the new texture makes the old stamp stale");
    assert!(settle(&mut h), "the segment restamps in the new texture");
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(button(b, false));
    });
}
