//! Sends waiting for their same-session conversation.
use super::*;

impl SlateApp {
    pub(in crate::app::board_agent) fn has_queued_agent_send(&self) -> bool {
        !self.agents.life.connecting.is_empty()
    }

    /// Send pressed before the conversation connected: wait, and send once it
    /// does. False when there is no conversation to wait for.
    pub(in crate::app::board_agent) fn queue_agent_send(&mut self, portal: NodeId) -> bool {
        let Some(agent) = self
            .doc()
            .scene
            .node(portal)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return false;
        };
        let channel = agent.channel.clone().or_else(|| {
            (self.agents.bindings.get(&portal) == Some(&agent.session))
                .then(|| self.agents.session(portal))
                .flatten()
                .filter(|state| state.provider == agent.provider && !state.conversation.is_empty())
                .map(|state| state.conversation.clone())
        });
        let Some(channel) = channel else {
            return false;
        };
        if self.agents.connection_rx.is_none() {
            if let Some((session, _)) = self.agent_session_for(portal) {
                self.agents.life.rejoined.insert(session);
            }
            self.load_agent_connection(portal, channel, true);
            if self.agents.connection_rx.is_none() {
                return false;
            }
        }
        self.agents.life.connecting.insert(portal);
        true
    }
}
