//! Conversation relay admission/contact/side effect; no scene command or staging.
use super::super::*;
use serde_json::{json, Value};
use slate_doc::scene::Node;

pub(in super::super) struct PreparedRelay {
    pub(in super::super) tab: u64,
    workbook: Option<PathBuf>,
    pub(in super::super) control_epoch: u64,
    pub(in super::super) contact: WorldPoint,
    recipient: String,
    text: String,
    endpoint: RelayEndpoint,
}
enum RelayEndpoint {
    Agent {
        binding: BoardLinkSession,
        position: WorldPoint,
        request: String,
    },
    Portal {
        node: Box<Node>,
        inputs: atlas_agent::InputSnapshot,
    },
}
impl PreparedRelay {
    pub(in super::super) fn node(&self) -> Option<slate_doc::NodeId> {
        match &self.endpoint {
            RelayEndpoint::Portal { node, .. } => Some(node.id),
            _ => None,
        }
    }
    pub(in super::super) fn witness_id(&self) -> String {
        // Presence motion uses opaque witness IDs; an agent recipient is not a
        // fabricated Scene node and never enters board reservations or previews.
        self.node()
            .map_or_else(|| self.recipient.clone(), |id| id.0.to_string())
    }
    pub(in super::super) fn expected_matches(
        &self,
        scene: &slate_doc::Scene,
    ) -> Result<(), String> {
        if let RelayEndpoint::Portal { node, .. } = &self.endpoint {
            if scene.node(node.id) != Some(node.as_ref()) {
                return Err("Conversation card changed during approach".into());
            }
        }
        Ok(())
    }
}
fn check_inputs(
    grant: &CollaboratorGrant,
    inputs: &atlas_agent::InputSnapshot,
) -> Result<(), String> {
    if inputs.context.len() + inputs.wired.len() > 128 {
        return Err("This conversation has too many inputs for a bounded relay".into());
    }
    if inputs.context.iter().chain(&inputs.wired).any(|item| {
        !grant.allows_node(&item.node.to_string())
            || ((!item.images.is_empty() || item.depth.is_some())
                && !grant
                    .readable_sources
                    .iter()
                    .any(|s| s == "board-file-inputs"))
    }) {
        return Err("This conversation's inputs exceed the collaborator grant".into());
    }
    Ok(())
}
impl SlateApp {
    pub(in super::super) fn prepare_presence_relay(
        &self,
        from: &Collaborator,
        targets: &[String],
        arguments: &Value,
    ) -> Result<PreparedRelay, String> {
        if !targets.is_empty() {
            return Err(
                "Conversation recipients belong in arguments.recipient, not board node targets"
                    .into(),
            );
        }
        let recipient = arguments["recipient"]
            .as_str()
            .ok_or("Choose one explicitly permitted conversation")?;
        let text = arguments["text"]
            .as_str()
            .filter(|s| s.len() <= 16_384)
            .ok_or("Supply a bounded message")?;
        submission::validate_message_text(text)?;
        let grant = from.grant.as_ref().ok_or("Grant revoked")?;
        if !grant.conversation_ids.iter().any(|id| id == recipient) {
            return Err("Conversation is outside the collaborator grant".into());
        }
        if from.relays >= 32 {
            return Err(
                "This session reached its 32-message relay limit; start a fresh human task".into(),
            );
        }
        let (endpoint, contact) = if let Some(a) = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == recipient)
        {
            if a.tab != from.tab
                || a.departure.is_some()
                || a.paused
                || a.relays >= 32
                || self.presence.task_busy(recipient)
            {
                return Err("Recipient is busy, paused, retired, or on another board".into());
            }
            let position = a
                .position(&self.presence.roster)
                .ok_or("Recipient has retired")?;
            (
                RelayEndpoint::Agent {
                    binding: a.binding.clone(),
                    position,
                    request: a.request.clone(),
                },
                WorldPoint {
                    x: position.x - 40.0,
                    y: position.y,
                },
            )
        } else {
            let node = self
                .doc()
                .scene
                .nodes
                .iter()
                .rev()
                .find(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.session == recipient))
                .ok_or("Conversation no longer exists")?;
            if !grant.allows_node(&node.id.0.to_string())
                || self.human_owns_presence_targets(&[node.id])
            {
                return Err("Conversation card is outside the grant or held by the human".into());
            }
            let inputs = self.agent_input_snapshot(node.id)?;
            check_inputs(grant, &inputs)?;
            let contact = witness::body_contact(node)?;
            (
                RelayEndpoint::Portal {
                    node: Box::new(node.clone()),
                    inputs,
                },
                contact,
            )
        };
        Ok(PreparedRelay {
            tab: from.tab,
            workbook: self.tab().path.clone(),
            control_epoch: from.control_epoch,
            contact,
            recipient: recipient.into(),
            text: text.into(),
            endpoint,
        })
    }
    /// Cheap live frame fences. Input resolution and deep target checks happen
    /// only at admission/contact (plus the witness scene-generation cache).
    pub(in super::super) fn validate_presence_relay(
        &self,
        from: &Collaborator,
        call: &BoardCall,
        relay: &PreparedRelay,
    ) -> Result<(), String> {
        let grant = from.grant.as_ref().ok_or("Grant revoked")?;
        grant
            .authorize(call, &from.binding, &from.request)
            .map_err(str::to_owned)?;
        if from.departure.is_some()
            || from.tab != self.tab().id
            || relay.tab != from.tab
            || self.tab().path != relay.workbook
            || self.tab().read_only
            || from.request.is_empty()
            || from.control_epoch != relay.control_epoch
            || from.pending_harness.is_some()
            || from.relays >= 32
            || !grant.conversation_ids.contains(&relay.recipient)
            || self
                .pending_stage_proposals()
                .iter()
                .any(|p| p.status == slate_doc::ProposalStatus::RecoveryRequired)
        {
            return Err("Conversation relay authority or board changed".into());
        }
        match &relay.endpoint {
            RelayEndpoint::Agent {
                binding,
                position,
                request,
            } => {
                let a = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == relay.recipient)
                    .ok_or("Recipient retired")?;
                if a.tab != relay.tab
                    || a.departure.is_some()
                    || a.binding != *binding
                    || a.request != *request
                    || a.paused
                    || a.pending_harness.is_some()
                    || a.relays >= 32
                    || self.presence.task_busy(&relay.recipient)
                    || a.position(&self.presence.roster) != Some(*position)
                {
                    return Err("Recipient moved, paused, or changed tasks during approach".into());
                }
            }
            RelayEndpoint::Portal { node, inputs } => {
                if !grant.allows_node(&node.id.0.to_string())
                    || self.human_owns_presence_targets(&[node.id])
                {
                    return Err(
                        "Conversation card is outside the grant or held by the human".into(),
                    );
                }
                check_inputs(grant, inputs)?;
                if !self.doc().scene.node(node.id).is_some_and(|n| {
                    slate_doc::agent_chat::agent(n).is_some_and(|a| a.session == relay.recipient)
                }) {
                    return Err("Conversation endpoint changed".into());
                }
            }
        }
        Ok(())
    }
    pub(in super::super) fn finish_contacted_presence_relay(
        &mut self,
        a: &mut Collaborator,
        work: &witness::WitnessedCommand,
        now: Instant,
    ) {
        let witness::WitnessAction::Relay(relay) = &work.action else {
            return;
        };
        let outcome = self.validate_presence_work(a, work).and_then(|()| {
            if work.index != work.total_steps() {
                return Err("Conversation contact is incomplete".into());
            }
            self.finish_presence_relay(a, relay)
        });
        let reply = match outcome {
            Ok(value) => {
                a.status = "Message submitted to conversation".into();
                a.activity = "exchanging".into();
                BoardReply {
                    id: work.call.id.clone(),
                    session_id: a.binding.session_id.clone(),
                    request_id: a.request.clone(),
                    success: true,
                    result: value,
                }
            }
            Err(error) => {
                a.note_helper_outcome("blocked", &error);
                a.status = error.clone();
                a.activity = "blocked".into();
                BoardReply::error(&work.call, &error)
            }
        };
        let _ = self.presence.links.reply(&a.dir, reply);
        a.last_active = now;
    }
    pub(in super::super) fn finish_presence_relay(
        &mut self,
        from: &mut Collaborator,
        relay: &PreparedRelay,
    ) -> Result<Value, String> {
        // Called only by completed real contact in the existing work owner.
        match &relay.endpoint {
            RelayEndpoint::Agent { .. } => {
                self.send_presence_message_from(
                    &relay.recipient,
                    &relay.text,
                    Some(&from.profile.name),
                )?;
                if let Some(a) = self
                    .presence
                    .agents
                    .iter_mut()
                    .find(|a| a.profile.id == relay.recipient)
                {
                    a.relays += 1;
                }
            }
            RelayEndpoint::Portal { node, inputs } => {
                relay.expected_matches(&self.doc().scene)?;
                let current = self.agent_input_snapshot(node.id)?;
                check_inputs(from.grant.as_ref().ok_or("Grant revoked")?, &current)?;
                if current != *inputs {
                    return Err(
                        "Conversation inputs changed during approach; message was not sent".into(),
                    );
                }
                self.send_scoped_collaborator_relay(node.id, &from.profile.name, &relay.text)?;
            }
        }
        from.relays += 1;
        self.push_history_as(
            atlas_commands::CommandId("presence.send"),
            Some(format!("to {}", relay.recipient)),
            atlas_commands::CmdAuthor::Agent(from.profile.name.clone()),
        );
        Ok(json!({"status":"submitted","recipient":relay.recipient}))
    }
}
