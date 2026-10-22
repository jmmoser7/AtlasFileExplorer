//! One keyboard owner when a focused page coexists with Slate commands/UI.
use super::*;

#[test]
fn page_paste_does_not_duplicate_a_nonempty_selected_board_clipboard() {
    let (mut h, host, id, point) = focused_page("web_paste_board_clipboard");
    h.app.board_sel = [id].into_iter().collect();
    h.app.board_clipboard = vec![h.app.doc().scene.node(id).unwrap().clone()];
    let count = h.app.doc().scene.nodes.len();
    let undo = h.app.tab().journal.undo_depth();
    h.frame_with(|input| {
        input.modifiers = egui::Modifiers::CTRL;
        input.events.push(egui::Event::PointerMoved(point));
        input
            .events
            .push(key_event(egui::Key::V, true, egui::Modifiers::CTRL));
        input.events.push(egui::Event::Paste("page only".into()));
    });
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        count,
        "the board clipboard was not pasted"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), undo);
    assert_eq!(h.app.web.focused, Some(id));
    assert_eq!(
        host.sent(|event| match event {
            WebInput::Text(text) => Some(text.clone()),
            _ => None,
        }),
        vec!["page only"]
    );
}

#[test]
fn focused_page_copy_and_cut_do_not_copy_or_delete_the_board_selection() {
    let (mut h, host, id, point) = focused_page("web_clipboard_one_owner");
    h.app.board_sel = [id].into_iter().collect();
    let mut sentinel = h.app.doc().scene.node(id).unwrap().clone();
    sentinel.id = NodeId(9_999_991);
    h.app.board_clipboard = vec![sentinel];
    let undo = h.app.tab().journal.undo_depth();
    for (key, event) in [
        (egui::Key::C, egui::Event::Copy),
        (egui::Key::X, egui::Event::Cut),
    ] {
        h.frame_with(|input| {
            input.modifiers = egui::Modifiers::CTRL;
            input.events.push(egui::Event::PointerMoved(point));
            input
                .events
                .push(key_event(key, true, egui::Modifiers::CTRL));
            input.events.push(event);
        });
        assert_eq!(h.app.board_clipboard[0].id, NodeId(9_999_991));
        assert!(
            h.app.doc().scene.node(id).is_some(),
            "Cut cannot delete the page's board node"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), undo);
        assert_eq!(
            host.sent(|event| {
                matches!(event, WebInput::Key { key: actual, pressed: true, .. } if *actual == key)
                    .then_some(())
            })
            .len(),
            1
        );
    }
}

#[test]
fn platform_only_clipboard_events_dispatch_each_page_action_once() {
    for event in [egui::Event::Copy, egui::Event::Cut] {
        let events = keyboard_events(&[event.clone(), event]);
        assert_eq!(events.len(), 2, "one press and one release");
        assert!(
            matches!(events[0], WebInput::Key { pressed: true, modifiers, .. } if modifiers.ctrl)
        );
        assert!(matches!(events[1], WebInput::Key { pressed: false, .. }));
    }
}

#[test]
fn undo_redo_alias_and_save_stay_slate_owned_while_arrow_navigation_reaches_page() {
    let (mut h, host, id, point) = focused_page("web_registered_shortcuts");
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.patch_nodes(&[id], |node| node.rect.w += 20.0);
    h.frame_with(|input| {
        input.modifiers = egui::Modifiers::CTRL;
        input.events.push(egui::Event::PointerMoved(point));
        input
            .events
            .push(key_event(egui::Key::Z, true, egui::Modifiers::CTRL));
    });
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect);
    let redo = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    h.frame_with(|input| {
        input.modifiers = redo;
        input.events.push(key_event(egui::Key::Z, true, redo));
    });
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.w, rect.w + 20.0);
    let workbook = h.base.join("keyboard-save.slate");
    h.app.tab_mut().path = Some(workbook.clone());
    h.frame_with(|input| {
        input.modifiers = egui::Modifiers::CTRL;
        input
            .events
            .push(key_event(egui::Key::S, true, egui::Modifiers::CTRL));
    });
    assert!(
        workbook.exists(),
        "the registered Slate Save command still runs"
    );
    h.frame_with(|input| {
        input.modifiers = egui::Modifiers::NONE;
        input
            .events
            .push(key_event(egui::Key::S, false, egui::Modifiers::CTRL));
        input
            .events
            .push(key_event(egui::Key::ArrowLeft, true, egui::Modifiers::NONE));
    });
    assert!(host
        .sent(|event| matches!(
            event,
            WebInput::Key {
                key: egui::Key::Z | egui::Key::S,
                ..
            }
        )
        .then_some(()))
        .is_empty());
    assert_eq!(
        host.sent(|event| matches!(
            event,
            WebInput::Key {
                key: egui::Key::ArrowLeft,
                pressed: true,
                ..
            }
        )
        .then_some(()))
            .len(),
        1
    );
}

#[test]
fn focused_slate_text_edit_owns_text_and_paste_even_with_pointer_over_page() {
    let (mut h, host, _, point) = focused_page("web_slate_text_edit");
    let context = h.ctx.clone();
    let mut draft = String::new();
    for frame in 0..2 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        };
        input.events.push(egui::Event::PointerMoved(point));
        if frame == 1 {
            input.events.push(egui::Event::Text("field ".into()));
            input.events.push(egui::Event::Paste("only".into()));
        }
        let _ = context.run(input, |ctx| {
            // Draw the page before the already-focused field on purpose:
            // keyboard ownership must not depend on TextEdit consuming first.
            h.app.update_app(ctx);
            egui::Window::new("Slate field")
                .fixed_pos(Pos2::new(20.0, 20.0))
                .show(ctx, |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut draft).id(egui::Id::new("web-owned-field")),
                    );
                    if frame == 0 {
                        response.request_focus();
                    }
                });
        });
    }
    assert_eq!(draft, "field only");
    assert!(host
        .sent(|event| matches!(event, WebInput::Text(_) | WebInput::Key { .. }).then_some(()))
        .is_empty());
}

#[test]
fn slate_palette_blocks_page_keyboard_even_if_pointer_remains_inside() {
    let (mut h, host, _, point) = focused_page("web_palette_keyboard_owner");
    h.app.palette_state.open = true;
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(point));
        input.events.push(egui::Event::Text("palette only".into()));
        input
            .events
            .push(egui::Event::Paste("palette paste".into()));
    });
    assert!(host
        .sent(|event| matches!(event, WebInput::Text(_) | WebInput::Key { .. }).then_some(()))
        .is_empty());
}
