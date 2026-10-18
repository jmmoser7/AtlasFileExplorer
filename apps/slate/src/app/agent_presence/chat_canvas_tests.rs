//! Actual Slate input routing with a focused chat composer and a live canvas.
use super::*;

#[test]
fn focused_chat_remains_open_while_the_human_pans_the_board() {
    let mut f = Gesture::new(1.0);
    f.h.app.presence.chrome.open_chat(&f.who);
    f.h.app
        .presence_save_draft(&f.who, "Keep this draft")
        .unwrap();
    f.paint();
    f.paint();
    f.h.ctx
        .memory_mut(|m| m.request_focus(Id::new(("presence-composer", f.who.as_str()))));
    f.paint();
    assert!(f.h.ctx.wants_keyboard_input());
    f.h.app.set_board_tool(crate::app::board::BoardTool::Pan);
    let point = f.h.app.canvas_rect.left_center() + Vec2::new(80.0, 0.0);
    let before = f.camera();
    f.button(point, true);
    f.input(|i| {
        i.events
            .push(egui::Event::PointerMoved(point + Vec2::new(55.0, 22.0)))
    });
    f.button(point + Vec2::new(55.0, 22.0), false);
    f.paint();
    assert_ne!(
        f.camera(),
        before,
        "focused chat must not freeze the board camera"
    );
    assert_eq!(f.h.app.presence.chrome.active_chat(), Some(f.who.as_str()));
    assert_eq!(
        f.h.app.presence_conversation(&f.who).unwrap().draft,
        "Keep this draft"
    );
}
