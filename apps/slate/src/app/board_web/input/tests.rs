use super::*;
use crate::app::tests::{only_portal, web_board, with_fake_host, FakeWebHost, Harness};

fn focused_page(tag: &str) -> (Harness, FakeWebHost, NodeId, Pos2) {
    let mut h = web_board(tag);
    let host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    h.frame();
    h.app.web_focus(id);
    let node = h.app.doc().scene.node(id).unwrap();
    let point = h.app.board_xf().rect_w2s(node.rect).center();
    (h, host, id, point)
}

fn moves(host: &FakeWebHost) -> Vec<WebInput> {
    host.inputs()
        .into_iter()
        .filter(|event| matches!(event, WebInput::Move { .. }))
        .collect()
}

#[test]
fn moving_hover_is_interactive_but_a_parked_pointer_sends_nothing() {
    let (mut h, host, id, point) = focused_page("web_pointer_dedupe");
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
    assert_eq!(moves(&host).len(), 1);
    let active_at = h.app.web.last_page_input;
    assert!(active_at.is_some());
    assert_eq!(
        h.app.web.focused_frame_interval(id),
        Duration::from_secs_f32(1.0 / INTERACTIVE_FPS)
    );
    for _ in 0..4 {
        h.frame();
    }
    assert_eq!(
        moves(&host).len(),
        1,
        "repaints do not synthesize browser mousemoves"
    );
    assert_eq!(
        h.app.web.last_page_input, active_at,
        "parked input does not extend the burst"
    );
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(point + egui::vec2(3.0, 0.0)))
    });
    assert_eq!(moves(&host).len(), 2);
    assert!(h.app.web.last_page_input > active_at);
}

#[test]
fn button_changes_forward_once_without_repeating_a_held_drag() {
    let (mut h, host, _, point) = focused_page("web_pointer_buttons");
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        });
        h.frame();
    }
    assert_eq!(
        moves(&host).len(),
        3,
        "entry, down, and up each change the sample once"
    );
    assert_eq!(
        host.sent(|event| match event {
            WebInput::Move { buttons, .. } => Some(*buttons),
            _ => None,
        }),
        vec![0, 1, 0]
    );
    assert_eq!(
        host.sent(|event| matches!(event, WebInput::Down { .. }).then_some(()))
            .len(),
        1
    );
    assert_eq!(
        host.sent(|event| matches!(event, WebInput::Up { .. }).then_some(()))
            .len(),
        1
    );
}

#[test]
fn holding_a_button_keeps_capture_fast_without_synthetic_moves() {
    let mut web = WebRuntime {
        focused: Some(NodeId(1)),
        pointer_down: 1,
        last_page_input: None,
        ..Default::default()
    };
    assert_eq!(
        web.focused_frame_interval(NodeId(1)),
        Duration::from_secs_f32(1.0 / INTERACTIVE_FPS)
    );
    assert!(web.pointer_move(10.0, 20.0, 1).is_some());
    assert!(web.pointer_move(10.0, 20.0, 1).is_none());
    web.pointer_down = 0;
    assert_eq!(
        web.focused_frame_interval(NodeId(1)),
        Duration::from_secs_f32(1.0 / FOCUSED_FPS)
    );
}

#[test]
fn pointer_tracking_resets_on_leave_focus_and_source_changes() {
    let (mut h, host, id, point) = focused_page("web_pointer_reset");
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(-100.0, -100.0)))
    });
    h.frame();
    assert_eq!(
        host.sent(|event| matches!(event, WebInput::Leave).then_some(()))
            .len(),
        1
    );
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
    assert_eq!(
        moves(&host).len(),
        2,
        "re-entry forwards the first sample again"
    );
    h.app.web_blur();
    h.app.web_focus(id);
    h.frame();
    assert_eq!(
        moves(&host).len(),
        3,
        "refocus forwards the current pointer"
    );
    h.app
        .bind_web_source(id, "https://example.com/other-dashboard".into());
    h.frame();
    assert_eq!(
        moves(&host).len(),
        4,
        "a new browser receives its first sample"
    );
}

fn key_event(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        pressed,
        physical_key: None,
        repeat: false,
        modifiers,
    }
}

#[test]
fn committed_unicode_and_shortcut_modifiers_reach_the_page_intact() {
    let (mut h, host, _, point) = focused_page("web_complete_text");
    let text = "e\u{301} 👩\u{200d}💻 漢字 \"quoted\"";
    let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    let tool = h.app.board_tool;
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(point));
        input.events.push(key_event(egui::Key::A, true, modifiers));
        input.events.push(egui::Event::Text(text.into()));
    });
    assert_eq!(
        host.sent(|event| match event {
            WebInput::Text(text) => Some(text.clone()),
            _ => None,
        }),
        vec![text]
    );
    assert_eq!(
        host.sent(|event| match event {
            WebInput::Key {
                key,
                pressed,
                modifiers,
            } => Some((*key, *pressed, *modifiers)),
            _ => None,
        }),
        vec![(egui::Key::A, true, modifiers)]
    );
    assert_eq!(h.app.board_tool, tool);
}

#[test]
fn paste_payload_suppresses_its_shortcut_and_duplicate_text_without_losing_other_typing() {
    let text = "dashboard 👩\u{200d}💻\nfilters";
    let input = [
        key_event(egui::Key::V, true, egui::Modifiers::CTRL),
        egui::Event::Paste(text.into()),
        egui::Event::Text(text.into()),
        key_event(egui::Key::V, false, egui::Modifiers::CTRL),
        egui::Event::Text("!".into()),
    ];
    assert_eq!(
        keyboard_events(&input),
        vec![WebInput::Text(text.into()), WebInput::Text("!".into())]
    );
    let shift_insert = [
        key_event(egui::Key::Insert, true, egui::Modifiers::SHIFT),
        egui::Event::Paste(text.into()),
    ];
    assert_eq!(
        keyboard_events(&shift_insert),
        vec![WebInput::Text(text.into())]
    );
    let ordinary = [
        key_event(egui::Key::V, true, egui::Modifiers::NONE),
        egui::Event::Text("v".into()),
    ];
    assert_eq!(
        keyboard_events(&ordinary).len(),
        2,
        "raw keys and committed text have distinct jobs"
    );
}

#[test]
fn paste_reaches_only_a_focused_page_and_leaves_board_content_unchanged() {
    let (mut h, host, _, point) = focused_page("web_complete_paste");
    let count = h.app.doc().scene.nodes.len();
    let text = "table\t漢字 👩\u{200d}💻\nnext row";
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(point));
        input
            .events
            .push(key_event(egui::Key::V, true, egui::Modifiers::CTRL));
        input.events.push(egui::Event::Paste(text.into()));
    });
    assert_eq!(
        host.sent(|event| match event {
            WebInput::Text(text) => Some(text.clone()),
            _ => None,
        }),
        vec![text]
    );
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert!(host
        .sent(|event| matches!(
            event,
            WebInput::Key {
                key: egui::Key::V,
                ..
            }
        )
        .then_some(()))
        .is_empty());
    h.app.web_blur();
    h.frame_with(|input| input.events.push(egui::Event::Text("board-owned".into())));
    assert_eq!(
        host.sent(|event| matches!(event, WebInput::Text(_)).then_some(()))
            .len(),
        1
    );
}

#[test]
fn escape_and_chrome_owned_pointer_do_not_forward_keyboard_payloads() {
    assert!(
        keyboard_events(&[key_event(egui::Key::Escape, true, egui::Modifiers::NONE)]).is_empty()
    );
    let (mut h, host, id, point) = focused_page("web_chrome_keyboard_gate");
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
    let node = h.app.doc().scene.node(id).unwrap();
    let corner = h.app.board_xf().rect_w2s(node.rect).left_top();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(corner));
        input.events.push(egui::Event::Text("chrome-owned".into()));
        input
            .events
            .push(egui::Event::Paste("chrome-owned-paste".into()));
    });
    assert!(host
        .sent(|event| matches!(event, WebInput::Text(_) | WebInput::Key { .. }).then_some(()))
        .is_empty());
}

mod ownership;
