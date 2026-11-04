//! Reconcile a live provider channel without changing membership or grants.
use super::*;

impl SlateApp {
    /// Source-state reconciliation follows only this live membership's actual
    /// request. The provider locator may change; local identity and grants do not.
    pub(crate) fn sync_chat_train_provider_channel(
        &mut self,
        portal: NodeId,
        state: &atlas_agent::AgentSession,
    ) {
        let Some((who, provider)) = self.agent_session_for(portal) else {
            return;
        };
        if provider != "codex"
            || state.provider != "codex"
            || state.conversation.is_empty()
            || state.request.is_empty()
        {
            return;
        }
        if !self
            .presence
            .chat_trains
            .get(&who)
            .is_some_and(|binding| binding.portal == portal && binding.tab == self.tab().id)
            || !self.presence.agents.iter().any(|actor| {
                actor.profile.id == who
                    && actor.tab == self.tab().id
                    && actor.departure.is_none()
                    && actor.grant.is_some()
                    && actor.request == state.request
            })
        {
            return;
        }
        let changed: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                slate_doc::agent_chat::agent(node)
                    .filter(|agent| {
                        agent.session == who
                            && agent.provider == "codex"
                            && agent
                                .channel
                                .as_ref()
                                .is_some_and(|channel| channel != &state.conversation)
                    })
                    .map(|_| node.id)
            })
            .collect();
        if !changed.is_empty() {
            self.patch_nodes(&changed, |node| {
                if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                    agent.channel = Some(state.conversation.clone());
                }
            });
        }
    }
}
