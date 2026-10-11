//! Train, return, and input dots keep separate hits on every chat card.

use super::test_support::*;
use super::*;
use eframe::egui::Pos2;

fn drag_screen(h: &mut super::super::tests::Harness, from: Pos2, to: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(from)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        })
    });
    let p = from + (to - from) * 0.5;
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(to)));
}

fn release_screen(h: &mut super::super::tests::Harness, at: Pos2) {
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    h.frame();
}

fn child_of(h: &super::super::tests::Harness, id: NodeId) -> Option<NodeId> {
    h.app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
        .map(|n| n.id)
}

fn dots(h: &super::super::tests::Harness, id: NodeId) -> (Pos2, Pos2) {
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap();
    let screen = xf.rect_w2s(node.rect);
    let train = super::output_circle_center(screen, xf.z);
    let ret = h.app.chat_return_dot(&h.ctx, id, node.rect.w, screen, xf.z);
    (train, ret)
}

fn feed_return(h: &mut super::super::tests::Harness, id: NodeId) {
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    let link = h.app.agent_output_link(id).unwrap();
    std::fs::create_dir_all(&link).unwrap();
    std::fs::write(h.base.join("notes.md"), "hello").unwrap();
    std::fs::write(
        link.join("return.json"),
        r#"{"id":"notes","title":"Notes","items":[{"path":"notes.md"}]}"#,
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    h.app.agent_output_roots(id, &link);
    while h.app.agents.sources.outputs(&link).is_none() {
        let _ = h.app.agents.sources.poll(&link, None);
        assert!(std::time::Instant::now() < deadline, "no worker snapshot");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.app.agent_output_roots(id, &link);
    assert!(h.app.agent_has_outputs(id), "the card has a return");
}

fn card_with_return(tag: &str) -> (super::super::tests::Harness, NodeId) {
    let mut h = board(tag);
    let id = train(&mut h, Pos2::ZERO, "local");
    feed_return(&mut h, id);
    center_on(&mut h, id);
    h.frame();
    (h, id)
}

#[test]
fn hovering_the_train_dot_never_shows_the_output_list() {
    let (mut h, id) = card_with_return("train_hover");
    let (train_at, ret) = dots(&h, id);
    assert!(
        (train_at.y - ret.y).abs() > 8.0,
        "train and return are different dots"
    );
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(train_at)));
    assert_eq!(
        h.app.agents.artifact_popup, None,
        "hovering the train dot never shows the output list"
    );
    assert!(
        painted_at(&out)
            .iter()
            .all(|(text, _)| !text.contains("notes")),
        "the train dot does not list files"
    );
    assert_eq!(h.app.agent_output_at(train_at, &h.app.board_xf()), Some(id));
    assert_eq!(h.app.agent_output_at(ret, &h.app.board_xf()), None);
}

#[test]
fn hovering_the_return_dot_lists_outputs_and_a_click_places_one() {
    let (mut h, id) = card_with_return("return_hover");
    let (_, ret) = dots(&h, id);
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(ret)));
    assert_eq!(h.app.agents.artifact_popup, Some((id, true)));
    assert!(
        painted_at(&out)
            .iter()
            .any(|(text, _)| text.contains("notes")),
        "hovering the return dot shows the outputs"
    );
    let row = painted_at(&out)
        .into_iter()
        .find(|(text, _)| text.contains("notes"))
        .map(|(_, rect)| rect.center())
        .unwrap();
    let before = h.app.doc().scene.nodes.len();
    press(&mut h, row);
    assert!(
        h.app.doc().scene.nodes.len() > before,
        "clicking an output places it on the board"
    );
}

#[test]
fn dragging_the_train_dot_spawns_on_home_and_downstream() {
    let mut h = board("train_spawn_both");
    let root = train(&mut h, Pos2::ZERO, "ollama");
    h.app.set_board_tool(super::super::board::BoardTool::Select);
    center_on(&mut h, root);
    h.frame();
    let (train_at, _) = dots(&h, root);
    let drop = train_at + egui::vec2(160.0, 80.0);
    drag_screen(&mut h, train_at, drop);
    release_screen(&mut h, drop);
    assert!(
        child_of(&h, root).is_some(),
        "dragging the home train dot spawns a card"
    );
    let downstream = next_card(&mut h, root);
    center_on(&mut h, downstream);
    h.frame();
    let (train_at, _) = dots(&h, downstream);
    let drop = train_at + egui::vec2(160.0, 40.0);
    drag_screen(&mut h, train_at, drop);
    release_screen(&mut h, drop);
    assert!(
        child_of(&h, downstream).is_some(),
        "dragging a downstream train dot spawns a card"
    );
}
