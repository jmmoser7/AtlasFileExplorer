//! Actual Slate input routing with a focused chat composer and a live canvas.
use super::*;
use crate::app::tests::{only_portal, with_fake_host};

#[test]
fn foreground_chat_owns_input_over_a_focused_web_page() {
    let mut f = Gesture::new(1.0);
    f.h.app.presence.chrome.open_chat(&f.who);
    for _ in 0..3 {
        f.paint();
    }
    let host = with_fake_host(&mut f.h);
    f.h.app
        .paste_web_url("https://example.com/dashboard", Pos2::ZERO);
    let (id, _) = only_portal(&f.h);
    f.h.app.web_allow_origin(id);
    f.paint();
    f.h.app.web_focus(id);
    f.paint();

    let composer_id = Id::new(("presence-composer", f.who.as_str()));
    let composer = f.h.ctx.read_response(composer_id).unwrap();
    let to_global =
        f.h.ctx
            .layer_transform_to_global(composer.layer_id)
            .unwrap_or_default();
    let point = (to_global * composer.rect).center();
    // Hover first so egui resolves the live chat layer before the press.
    f.input(|input| input.events.push(egui::Event::PointerMoved(point)));
    f.paint();
    assert_eq!(
        f.h.ctx.layer_id_at(point),
        Some(composer.layer_id),
        "test pointer must land on the foreground chat over the live page"
    );
    host.0.borrow_mut().inputs.clear();
    f.input(|input| {
        input.events.extend([
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    });
    f.paint();
    assert_eq!(
        f.h.ctx.memory(|memory| memory.focused()),
        Some(composer_id),
        "the real chat composer should receive the click"
    );
    f.input(|input| {
        input.events.push(egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
    });
    f.paint();
    assert!(
        host.0.borrow().keyboard_releases > 0,
        "focused composer should return native keyboard focus to Slate"
    );
    f.input(|input| input.events.push(egui::Event::Text("chat text".into())));
    f.paint();
    assert_eq!(
        f.h.app.presence_conversation(&f.who).unwrap().draft,
        "chat text",
        "typed text should reach the chat draft"
    );
    let leaked = host
        .inputs()
        .into_iter()
        .filter(|event| {
            !matches!(
                event,
                crate::app::board_web::WebInput::Move { .. }
                    | crate::app::board_web::WebInput::Leave
            )
        })
        .collect::<Vec<_>>();
    assert!(
        leaked.is_empty(),
        "pointer/text from foreground chat reached the page: {leaked:?}"
    );

    // Composer focus remains authoritative when the pointer leaves the popup.
    host.0.borrow_mut().inputs.clear();
    let page = f.h.app.doc().scene.node(id).unwrap();
    let page_rect = f.h.app.board_xf().rect_w2s(page.rect);
    let chat_rect =
        f.h.ctx
            .memory(|memory| memory.area_rect(Id::new("presence-expansion")))
            .unwrap();
    let outside = [(0.2, 0.4), (0.8, 0.4), (0.2, 0.75), (0.8, 0.75)]
        .into_iter()
        .map(|(x, y)| {
            egui::pos2(
                page_rect.left() + page_rect.width() * x,
                page_rect.top() + page_rect.height() * y,
            )
        })
        .find(|point| f.h.ctx.screen_rect().contains(*point) && !chat_rect.contains(*point))
        .expect("live page exposes a point outside the foreground chat");
    f.input(|input| {
        input.events.extend([
            egui::Event::PointerMoved(outside),
            egui::Event::Text("still chat".into()),
        ]);
    });
    f.paint();
    assert_eq!(
        f.h.app.presence_conversation(&f.who).unwrap().draft,
        "chat textstill chat",
        "keyboard input should continue updating the composer draft after pointer exit"
    );
    assert!(
        host.inputs()
            .iter()
            .all(|event| matches!(event, crate::app::board_web::WebInput::Move { .. })),
        "composer text reached the page after pointer exit"
    );
    assert!(
        host.inputs()
            .iter()
            .any(|event| matches!(event, crate::app::board_web::WebInput::Move { .. })),
        "ordinary pointer input outside the popup was blocked"
    );

    // With egui focus released, browser keyboard input flows again.
    f.h.ctx
        .memory_mut(|memory| memory.surrender_focus(composer_id));
    f.input(|input| input.events.push(egui::Event::Text("page text".into())));
    f.paint();
    assert!(
        host.inputs()
            .iter()
            .any(|event| matches!(event, crate::app::board_web::WebInput::Text(text) if text == "page text")),
        "ordinary page keyboard input was blocked"
    );
}

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
