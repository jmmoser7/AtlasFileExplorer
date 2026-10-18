//! Shared send seam for wired crosstalk and explicitly scoped collaborators.
use super::*;
impl SlateApp {
    pub(crate) fn send_scoped_collaborator_relay(
        &mut self,
        tail: NodeId,
        label: &str,
        text: &str,
    ) -> Result<NodeId, String> {
        if text.is_empty() || text.chars().count() > xt::RELAY_MAX_CHARS {
            return Err("Relay exceeds the conversation limit".into());
        }
        if !self
            .doc()
            .scene
            .node(tail)
            .is_some_and(slate_doc::agent_chat::is_chat_host)
        {
            return Err("Choose a conversation card for this message".into());
        }
        if self.agents.chat_preparing.contains_key(&tail) {
            return Err("This conversation is already preparing a message".into());
        }
        let sent = self.send_agent_prompt_from(tail, Some((text, label)));
        // A deferred relay has already captured its explicit prompt and origin;
        // the human's composer remains independently editable while it prepares.
        if sent.is_none() && self.agents.chat_preparing.contains_key(&tail) {
            return Ok(tail);
        }
        sent.ok_or_else(|| {
            "Conversation is busy, unbound, or waiting for input preparation; inspect its card"
                .into()
        })
    }
}
