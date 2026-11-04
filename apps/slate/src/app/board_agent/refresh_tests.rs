use super::*;
use crate::app::tests::Harness;

#[test]
fn local_chat_queues_only_its_existing_same_session_conversation_and_refresh_yields() {
    let mut h = Harness::new("local-chat-refresh-queue");
    h.app.ensure_work_tab();
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    let chat = super::train_setup::train(&mut h, Pos2::ZERO, "codex");
    let session = h.app.agent_session_for(chat).unwrap().0;
    h.app.agents.sessions.insert(
        chat,
        std::sync::Arc::new(AgentSession {
            provider: "codex".into(),
            conversation: "current-thread".into(),
            ..Default::default()
        }),
    );
    h.app
        .agents
        .bindings
        .insert(chat, "different-local-session".into());
    assert!(!h.app.queue_agent_send(chat));
    h.app.agents.bindings.insert(chat, session.clone());
    let (_tx, rx) = unbounded();
    h.app.agents.connection_rx = Some(rx);
    h.app.agents.connection_pending = Some(chat);
    h.app.agents.connection_background = true;
    assert!(h.app.queue_agent_send(chat));
    assert!(h.app.agent_connecting(chat));
    h.app.agents.connection_rx = None;
    h.app.agents.connection_pending = None;
    h.app.agents.connection_tick = None;
    h.app.agents.life.hold_loads = true;
    h.app.board_sel = std::iter::once(chat).collect();
    h.app.refresh_agent_connection();
    assert!(
        h.app.agents.life.loads.is_empty(),
        "auto-refresh must yield to queued send"
    );
    assert_eq!(h.app.agent_session_for(chat).unwrap().0, session);
}
