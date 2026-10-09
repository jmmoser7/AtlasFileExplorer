//! Sending a prompt shows thinking, and a failed send names the failure.

use super::*;

#[test]
fn a_failed_agent_send_names_the_failure() {
    let mut h = agent_board("agent_named_fail");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.app.ai.config.workspace_dir = Some(std::path::PathBuf::from("/definitely/not/here"));
    *h.app.agents.prompt_mut(id) = "what is 2+2?".into();
    h.app.send_agent_prompt(id);
    let reason = h
        .app
        .agent_failure_reason(id)
        .expect("failure must be a named state, never a blank");
    assert!(
        reason.contains("AI workspace"),
        "named the missing workspace: {reason}"
    );
    assert!(!h.app.agent_is_awaiting(id));
}

/// Train cards default to message-pair presentation (`bind_program` since agent-pairs).
#[test]
fn sending_a_prompt_shows_thinking_not_silence_in_pair_mode() {
    let (mut h, id) = send_thinking_harness("agent_thinking_pair");
    assert!(matches!(
        h.app.doc().scene.node(id).and_then(slate_doc::agent_chat::agent),
        Some(a) if a.chat.detail == slate_doc::agent_chat::Detail::Pair
    ));
    h.app.send_agent_prompt(id);
    assert_send_shows_thinking(&h);
    // First pair send on a non-linear provider spawns the tail card (draft is false).
    assert!(
        h.app.doc().scene.nodes.iter().any(|n| {
            slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id))
        }),
        "pair presentation still trains on the first send"
    );
}

/// Summary presentation spawns the next train card; awaiting follows the tail.
#[test]
fn sending_a_prompt_shows_thinking_not_silence_in_summary_mode() {
    use slate_doc::scene::NodeKind;
    let (mut h, id) = send_thinking_harness("agent_thinking_summary");
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.detail = slate_doc::agent_chat::Detail::Summary;
        }
    });
    h.app.send_agent_prompt(id);
    assert_send_shows_thinking(&h);
}
