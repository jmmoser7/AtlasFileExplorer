//! A human opt-in covers only new chat endpoints this live membership creates.
use super::*;
use serde_json::json;

pub(super) const OWN_CHATS: &str = "board-owned-conversations";
pub(super) struct CreatedProposal {
    pub(super) id: String,
    pub(super) grant: String,
    pub(super) call: BoardCall,
    pub(super) nodes: Vec<slate_doc::NodeId>,
    pub(super) prepared: crate::app::dispatch::PreparedBoardCommand,
}
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
    /// The canonical proposal owner checks captured resource metadata before commit.
    pub(crate) fn presence_validate_created_proposal(
        &mut self,
        proposal: &slate_doc::Proposal,
    ) -> Result<(), String> {
        let Some((call, grant, binding, request, tab, prepared)) =
            self.presence.agents.iter().find_map(|a| {
                a.pending_created
                    .as_ref()
                    .filter(|pending| pending.id == proposal.id)
                    .map(|pending| {
                        (
                            pending.call.clone(),
                            a.grant.clone(),
                            a.binding.clone(),
                            a.request.clone(),
                            a.tab,
                            pending.prepared.clone(),
                        )
                    })
            })
        else {
            return Ok(());
        };
        if tab != self.tab().id {
            return Err("This proposal belongs to a different board".into());
        }
        grant
            .ok_or("Collaborator grant revoked")?
            .authorize(&call, &binding, &request)
            .map_err(str::to_owned)?;
        self.check_targeted_command(&prepared)
    }
    /// Called by the canonical proposal owner only after its journal commits.
    pub(crate) fn presence_accepted_creation(&mut self, proposal: &slate_doc::Proposal) {
        let Some(index) = self.presence.agents.iter().position(|a| {
            a.pending_created
                .as_ref()
                .is_some_and(|p| p.id == proposal.id)
        }) else {
            return;
        };
        let mut a = self.presence.agents.remove(index);
        let pending = a.pending_created.take().unwrap();
        if a.departure.is_none()
            && a.tab == self.tab().id
            && pending.call.request_id == a.request
            && a.grant.as_ref().is_some_and(|g| g.id == pending.grant)
        {
            let registered = self.presence_register_created_chats(&mut a, &pending.nodes);
            if let Err(error) = &registered {
                a.status = error.clone();
            }
            let _ = self.presence.links.reply(&a.dir, BoardReply {
                id: pending.call.id, session_id: a.binding.session_id.clone(), request_id: a.request.clone(),
                success: true, result: json!({"status":"applied","created_ids":pending.nodes.iter()
                    .map(|id|id.0.to_string()).collect::<Vec<_>>(),"conversation_scope_updated":registered.unwrap_or(false)}),
            });
        }
        self.presence.agents.insert(index, a);
    }
}
