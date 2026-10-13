//! Property-editor focus during secondary-pointer navigation.
use super::tests::{board, rectangle};
use super::*;
use crate::app::tests::Harness;

fn empty_canvas_point(h: &Harness, id: NodeId) -> Pos2 {
    let xf = h.app.board_xf();
    let r = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let canvas = h.app.canvas_rect;
    let p = Pos2::new(
        (r.max.x + 90.0).min(canvas.max.x - 24.0),
        (r.max.y + 70.0).min(canvas.max.y - 24.0),
    );
    assert!(canvas.contains(p));
    assert!(!r.expand(20.0).contains(p));
    p
}

#[test]
fn right_drag_pan_keeps_the_property_editor_open() {
    let mut h = board();
    let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
    h.frame();
    h.frame();
    h.app.shape_properties.panel = Some(Panel::Fill);
    let p = empty_canvas_point(&h, id);
    let before = h.app.tab().cam.offset;
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
    assert!(h.app.board_sel.contains(&id));
    let end = p + Vec2::new(80.0, -36.0);
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(end)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    h.frame();
    assert_eq!(
        h.app.shape_properties.panel,
        Some(Panel::Fill),
        "right-drag pan must leave the editor open"
    );
    assert!(
        h.app.board_sel.contains(&id),
        "right-drag pan must keep the node selected"
    );
    assert!(
        (h.app.tab().cam.offset - before).length() > 10.0,
        "the gesture must actually pan the board"
    );
}

#[test]
fn secondary_click_without_a_drag_still_collapses_the_editor() {
    let mut h = board();
    let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
    h.frame();
    h.frame();
    h.app.shape_properties.panel = Some(Panel::Fill);
    let p = empty_canvas_point(&h, id);
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert!(h.app.shape_properties.panel.is_none());
    assert!(h.app.board_sel.is_empty());
}
