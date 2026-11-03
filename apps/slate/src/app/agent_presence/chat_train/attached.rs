//! A model's explicitly attached agent owns a host context, never a self-wire.
use super::*;
use crate::app::application_connector::native::BindingKey;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct AttachedModel {
    pub(super) node: NodeId,
    key: BindingKey,
}

impl SlateApp {
    pub(super) fn attached_native_model(&self, node: NodeId) -> Option<AttachedModel> {
        let host = self.doc().scene.node(node)?;
        if !matches!(&host.kind, NodeKind::Image(_)) {
            return None;
        }
        let agent = slate_doc::agent_chat::agent(host)?;
        if !Self::native_model_harness(&agent.provider)
            || agent.view != atlas_agent::PortalView::Chat
            || agent.chat.end.is_some()
        {
            return None;
        }
        let binding = self.headless_model_agent_context(node)?;
        Some(AttachedModel {
            node,
            key: binding.key(),
        })
    }

    pub(super) fn has_native_chat_context(&self, portal: NodeId, session: &str) -> bool {
        self.chat_train_wires(session)
            .iter()
            .any(|wire| wire.native)
            || self.attached_native_model(portal).is_some()
    }

    pub(super) fn attached_membership_matches(&self, portal: NodeId, session: &str) -> bool {
        let Some(attached) = self.attached_native_model(portal) else {
            return false;
        };
        self.presence
            .chat_trains
            .get(session)
            .is_some_and(|binding| {
                binding.portal == portal
                    && binding.tab == self.tab().id
                    && binding.attached.as_ref() == Some(&attached)
            })
    }

    /// Cheap UI state only. Opening Board access remains an explicit command.
    pub(crate) fn attached_model_native_access(&self, node: NodeId) -> Option<bool> {
        self.attached_native_model(node)?;
        let (session, provider) = self.agent_session_for(node)?;
        Self::native_model_harness(&provider).then(|| {
            self.chat_membership_can_run(&session)
                && self.attached_membership_matches(node, &session)
                && self.presence.agents.iter().any(|actor| {
                    actor.profile.id == session
                        && actor.grant.as_ref().is_some_and(|grant| {
                            grant.allows_node(&node.0.to_string())
                                && grant
                                    .commands
                                    .iter()
                                    .any(|command| NATIVE_COMMANDS.contains(&command.as_str()))
                        })
                })
        })
    }
}

#[cfg(test)]
mod tests;
