//! Actual Slate input routing with a focused chat composer and a live canvas.
use super::*;

#[test]
fn transcript_shortcuts_precede_board_commands_without_claiming_board_or_composer_keys() {
    let mut f = Gesture::new(1.0);
    f.h.app.presence.chrome.open_chat(&f.who);
    for _ in 0..3 {
        f.paint();
    }
    let message =
        f.h.app
            .presence_conversation(&f.who)
            .unwrap()
            .turns
            .last()
            .unwrap();
    let text_id = Id::new(("presence-message-text", f.who.as_str(), message.id.as_str()));
    let text = f.h.ctx.read_response(text_id.with("text")).unwrap();
    let text_to_global =
        f.h.ctx
            .layer_transform_to_global(text.layer_id)
            .unwrap_or_default();
    let text_rect_global = text_to_global * text.rect;
    let expansion =
        f.h.ctx
            .read_response(Id::new("presence-expansion"))
            .unwrap();
    let expansion_to_global =
        f.h.ctx
            .layer_transform_to_global(expansion.layer_id)
            .unwrap_or_default();
    let chat_rect = expansion_to_global * expansion.rect;
    let visible_text = text_rect_global.intersect(chat_rect);
    assert!(
        visible_text.is_positive(),
        "message selection band is visible in chat"
    );
    let point = visible_text.center();
    // `command` is egui's cross-platform shortcut flag; the app dispatcher
    // also matches the physical Ctrl modifier.
    let ctrl_command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: ctrl_command,
    };
    f.input(|i| {
        i.modifiers = ctrl_command;
        i.events
            .extend([egui::Event::PointerMoved(point), key(egui::Key::A)]);
    });
    assert!(
        f.h.app.board_sel.is_empty(),
        "chat Ctrl+A must not select the board"
    );
    assert!(
        f.h.app.presence.chrome.transcript_selected(&f.who),
        "Ctrl+A on the visible selectable message row must select the transcript"
    );
    f.input(|i| {
        i.modifiers = ctrl_command;
        i.events.push(egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: ctrl_command,
        });
    });
    f.time += 1.0 / 60.0;
    let time = f.time;
    let out = f.h.frame_output(|i| {
        i.time = Some(time);
        i.events.push(egui::Event::Copy);
    });
    assert!(
        out.platform_output.commands.iter().any(|command| matches!(
            command, egui::OutputCommand::CopyText(text)
                if text.contains("Recent request") && text.contains("Recent answer")
        )),
        "native Copy must include both speakers"
    );
    let canvas = f.h.app.canvas_rect;
    let board = [
        canvas.left_center() + Vec2::new(40.0, 0.0),
        canvas.right_center() - Vec2::new(40.0, 0.0),
    ]
    .into_iter()
    .find(|point| !chat_rect.contains(*point))
    .expect("the board pointer must be outside the open chat");
    f.input(|i| {
        i.modifiers = ctrl_command;
        i.events
            .extend([egui::Event::PointerMoved(board), key(egui::Key::A)]);
    });
    assert!(
        f.h.app.board_sel.contains(&f.underlay),
        "an open chat must not steal board Ctrl+A; board pointer={board:?}, chat rect={chat_rect:?}, pointer layer={:?}, focused={:?}, wants_keyboard_input={}",
        f.h.ctx.layer_id_at(board),
        f.h.ctx.memory(|memory| memory.focused()),
        f.h.ctx.wants_keyboard_input(),
    );
    f.h.app.board_sel.retain(|id| *id == f.underlay);
    f.h.ctx
        .memory_mut(|m| m.request_focus(Id::new(("presence-composer", f.who.as_str()))));
    f.paint();
    f.input(|i| {
        i.modifiers = ctrl_command;
        i.events.push(key(egui::Key::A));
    });
    assert_eq!(
        f.h.app.board_sel.len(),
        1,
        "composer Ctrl+A stays in its text editor"
    );
}

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
