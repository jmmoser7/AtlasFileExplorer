//! Text-draft boards, keystrokes, and commit checks.

use super::*;

pub(crate) fn recolor_text(h: &mut Harness, id: NodeId, color: Rgba) {
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Text(t) = &mut n.kind {
            t.color = color;
        }
    });
}

pub(crate) fn draft_color_at(h: &mut Harness, world: Pos2) -> Rgba {
    h.app.place_text_at(world);
    let color = h.app.text_box_draft.as_ref().unwrap().color;
    h.app.cancel_text_box_draft();
    color
}

pub(crate) fn text_draft_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.frame();
    h
}

/// Real keystrokes into the focused draft, then a frame with the pointer over
/// the board so egui hit-tests every registered widget rect.
pub(crate) fn type_into_draft(h: &mut Harness, text: &str) {
    h.frame();
    let text = text.to_string();
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(720.0, 450.0)));
        input.events.push(egui::Event::Text(text));
    });
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(724.0, 452.0)));
    });
}

pub(crate) fn assert_draft_rect_sane(h: &Harness) -> slate_doc::scene::WorldRect {
    let r = h
        .app
        .text_box_draft
        .as_ref()
        .expect("draft still open")
        .rect;
    for v in [r.x, r.y, r.w, r.h] {
        assert!(v.is_finite(), "draft rect must stay finite: {r:?}");
    }
    assert!(
        r.w > 0.0 && r.h > 0.0,
        "draft rect must be non-empty: {r:?}"
    );
    r
}

/// A saved workbook, so new text documents have a folder beside it.
pub(crate) fn quote_board(tag: &str) -> Harness {
    let mut h = text_draft_board(tag);
    h.app.tab_mut().path = Some(h.base.join("quote.slate"));
    h
}

pub(crate) fn press_quote(h: &mut Harness, quote: char) {
    let modifiers = if quote == '"' {
        egui::Modifiers::SHIFT
    } else {
        egui::Modifiers::NONE
    };
    h.frame_with(|input| {
        input.modifiers = modifiers;
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(700.0, 420.0)));
        input.events.push(key_event(egui::Key::Quote, modifiers));
        input.events.push(egui::Event::Text(quote.to_string()));
    });
}

/// Arm Text, click the board at `screen` and type `text` into the draft.
pub(crate) fn compose_text(h: &mut Harness, screen: Pos2, text: &str) {
    h.app.set_board_tool(board::BoardTool::Text);
    h.frame();
    click_board(h, screen);
    assert!(h.app.text_box_draft.is_some(), "the click opens a draft");
    h.frame();
    let typed = text.to_string();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(screen));
        input.events.push(egui::Event::Text(typed));
    });
    pause(h);
    assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer, text);
}

/// The typed words are on the board as one text node, nothing is still
/// being edited, no editor popup is open, and it cost exactly one undo step.
pub(crate) fn assert_text_committed_once(h: &Harness, undo_before: usize, text: &str) {
    let nodes = &h.app.doc().scene.nodes;
    assert_eq!(nodes.len(), 1, "exactly one text node");
    match &nodes[0].kind {
        NodeKind::Text(t) => assert_eq!(t.text, text, "the typed words survive"),
        other => panic!("expected a text node, got {other:?}"),
    }
    assert!(h.app.text_box_draft.is_none(), "the draft is finished");
    assert!(
        h.app.text_edit.is_none(),
        "no editor reopens on the new node"
    );
    assert_eq!(
        h.app.shape_properties.panel, None,
        "no text/color editor pops up"
    );
    assert!(h.app.board_menu.is_none());
    assert!(!h.app.palette_state.open);
    assert_eq!(h.app.tab().journal.undo_depth(), undo_before + 1);
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
}
