//! Selection controls retain egui popup and cancelled-click ownership.
use super::*;
use crate::app::board_properties::tests::{board, strip_button, text_node};

fn pointer(point: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: point,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn input_frame(
    h: &mut Harness,
    events: Vec<egui::Event>,
    overlay: Option<Rect>,
) -> egui::FullOutput {
    let time = h.ctx.input(|i| i.time) + 1.0 / 60.0;
    h.ctx.clone().run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            h.app.update_app(ctx);
            if let Some(rect) = overlay {
                chrome::popup_area(
                    ctx,
                    Id::new("text-strip-test-overlay"),
                    chrome::selection_chrome_layer(),
                )
                .fixed_pos(rect.min)
                .constrain(false)
                .fade_in(false)
                .show(ctx, |ui| {
                    ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                });
            }
        },
    )
}

fn fixture() -> (Harness, Rect) {
    let mut h = board();
    text_node(&mut h, WorldRect::new(-120.0, -30.0, 240.0, 60.0));
    for _ in 0..14 {
        input_frame(&mut h, vec![], None);
    }
    let button = strip_button(&h, Panel::Text);
    (h, button)
}

#[test]
fn a_popup_over_the_text_squircle_owns_the_click() {
    let (mut h, button) = fixture();
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let point = button.center();
    let cover = button.expand(4.0);
    for _ in 0..3 {
        input_frame(&mut h, vec![egui::Event::PointerMoved(point)], Some(cover));
    }
    for pressed in [true, false] {
        input_frame(&mut h, vec![pointer(point, pressed)], Some(cover));
    }
    assert_eq!(
        h.app.shape_properties.panel, None,
        "the covered strip must not activate"
    );
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn releasing_a_text_squircle_press_outside_does_not_activate_it() {
    let (mut h, button) = fixture();
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let start = button.center();
    let outside = start + Vec2::new(0.0, 90.0);
    input_frame(&mut h, vec![egui::Event::PointerMoved(start)], None);
    input_frame(&mut h, vec![pointer(start, true)], None);
    input_frame(&mut h, vec![egui::Event::PointerMoved(outside)], None);
    input_frame(&mut h, vec![pointer(outside, false)], None);
    assert_eq!(
        h.app.shape_properties.panel, None,
        "a cancelled click must not activate Text"
    );
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
