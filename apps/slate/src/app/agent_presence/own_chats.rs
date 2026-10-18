//! A human opt-in covers only new chat endpoints this live membership creates.
use super::*;

pub(super) const OWN_CHATS: &str = "board-owned-conversations";
impl SlateApp {
    pub(super) fn presence_set_own_chats(
        &mut self,
        who: &str,
        enabled: bool,
    ) -> Result<(), String> {
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .ok_or("Collaborator is not present")?;
        if a.parent.is_some() {
            return Err("Only a primary collaborator can manage its own chats".into());
        }
        if !enabled {
            let owned = a.owned_conversations.clone();
            let descendants = self.presence.descendants(who);
            self.stop_presence(who, false)?;
            for child in self
                .presence
                .agents
                .iter_mut()
                .filter(|child| descendants.contains(&child.profile.id))
            {
                if let Some(grant) = child.grant.as_mut() {
                    grant.conversation_ids.retain(|id| !owned.contains(id));
                    grant.readable_sources.retain(|s| s != OWN_CHATS);
                }
            }
        }
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
            .unwrap();
        let grant = a.grant.as_mut().ok_or("Board grant revoked")?;
        grant.readable_sources.retain(|s| s != OWN_CHATS);
        if enabled {
            grant.readable_sources.push(OWN_CHATS.into());
        } else {
            grant
                .conversation_ids
                .retain(|id| !a.owned_conversations.contains(id));
            a.owned_conversations.clear();
        }
        if !a.request.is_empty() {
            self.presence
                .links
                .set_session(&a.dir, a.binding.clone(), grant.clone(), a.request.clone())
                .map_err(str::to_owned)?;
        }
        Ok(())
    }
    pub(super) fn presence_register_created_chats(
        &mut self,
        a: &mut Collaborator,
        nodes: &[slate_doc::NodeId],
    ) -> Result<bool, String> {
        let Some(grant) = a.grant.as_ref() else {
            return Err("Board grant revoked".into());
        };
        if a.parent.is_some() || !grant.readable_sources.iter().any(|s| s == OWN_CHATS) {
            return Ok(false);
        }
        let mut next = grant.clone();
        let mut owned = a.owned_conversations.clone();
        for node in nodes.iter().filter_map(|id| self.doc().scene.node(*id)) {
            if !slate_doc::agent_chat::is_chat_host(node) {
                continue;
            }
            let session = &slate_doc::agent_chat::agent(node).unwrap().session;
            if !next.conversation_ids.contains(session) {
                if next.conversation_ids.len() >= 128 {
                    return Err("Conversation scope is full".into());
                }
                next.conversation_ids.push(session.clone());
                owned.push(session.clone());
            }
        }
        self.presence
            .links
            .set_session(&a.dir, a.binding.clone(), next.clone(), a.request.clone())
            .map_err(str::to_owned)?;
        a.grant = Some(next);
        a.owned_conversations = owned;
        Ok(true)
    }
}
