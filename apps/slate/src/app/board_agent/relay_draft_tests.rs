//! Relays share submission and preparation without claiming the human composer.
use super::*;

fn board(tag: &str) -> (super::super::tests::Harness, NodeId) {
    let mut h = test_support::board(tag);
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    let portal = test_support::train(&mut h, Pos2::ZERO, "cursor");
    *h.app.agents.prompt_mut(portal) = "My unfinished human message".into();
    h.app.agents.composer_editing = Some(portal);
    h.app.agents.composer_focus = Some(portal);
    (h, portal)
}

#[test]
fn an_immediate_collaborator_relay_preserves_draft_selection_and_caret() {
    let (mut h, portal) = board("presence_relay_draft");
    let selection = h.app.board_sel.clone();
    h.app
        .send_scoped_collaborator_relay(portal, "Dot", "Please inspect the results")
        .unwrap();
    assert_eq!(
        h.app.agents.prompt(portal),
        Some("My unfinished human message")
    );
    assert_eq!(h.app.board_sel, selection);
    assert_eq!(h.app.agents.composer_editing, Some(portal));
    assert_eq!(h.app.agents.composer_focus, Some(portal));
    let request = &h.app.agents.dispatched.last().unwrap().1;
    assert_eq!(request.prompt, "Please inspect the results");
    assert_eq!(request.relayed_from.as_deref(), Some("Dot"));
}

#[test]
fn a_deferred_relay_preserves_new_human_edits_and_captures_its_original_sender() {
    let (mut h, portal) = board("presence_relay_deferred_draft");
    h.app.agents.chat_preparing.insert(
        portal,
        runtime::PendingChatSend {
            doc: h.app.tab().id,
            portal,
            composer: portal,
            prompt: "Read this carefully".into(),
            workspace: h.base.clone(),
            origin: runtime::ChatSendOrigin {
                typing_here: false,
                relayed_from: Some("Dot".into()),
                preserve_composer: true,
            },
            inputs: input_prep::InputPreparation {
                inputs: Default::default(),
                replacements: vec![],
                bake_dependencies: vec![],
            },
        },
    );
    *h.app.agents.prompt_mut(portal) = "Human edited this while the image prepared".into();
    let selection = h.app.board_sel.clone();
    h.app.pump_pending_chat_sends(&h.ctx);
    assert_eq!(
        h.app.agents.prompt(portal),
        Some("Human edited this while the image prepared")
    );
    assert_eq!(h.app.board_sel, selection);
    assert_eq!(h.app.agents.composer_editing, Some(portal));
    let request = &h.app.agents.dispatched.last().unwrap().1;
    assert_eq!(request.prompt, "Read this carefully");
    assert_eq!(request.relayed_from.as_deref(), Some("Dot"));
}

#[test]
fn failed_input_preparation_keeps_the_human_draft_and_sends_nothing() {
    let (mut h, portal) = board("presence_relay_failed_input");
    let token = crate::app::board_image_layers::LayerBakeReadiness::new();
    token.resolve(&Err("Disk full".into()));
    h.app.agents.chat_preparing.insert(
        portal,
        runtime::PendingChatSend {
            doc: h.app.tab().id,
            portal,
            composer: portal,
            prompt: "Read this carefully".into(),
            workspace: h.base.clone(),
            origin: runtime::ChatSendOrigin {
                typing_here: false,
                relayed_from: Some("Dot".into()),
                preserve_composer: true,
            },
            inputs: input_prep::InputPreparation {
                inputs: Default::default(),
                replacements: vec![],
                bake_dependencies: vec![token],
            },
        },
    );
    h.app.pump_pending_chat_sends(&h.ctx);
    assert!(h.app.agents.dispatched.is_empty());
    assert_eq!(
        h.app.agents.prompt(portal),
        Some("My unfinished human message")
    );
    assert_eq!(h.app.agents.composer_editing, Some(portal));
    assert!(h.app.agents.chat_preparing.is_empty());
}
