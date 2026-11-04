//! Scoped native requests share the existing application worker and source owner.
use super::*;
mod retention;
mod types;
pub(crate) use types::*;
#[cfg(test)]
pub(crate) mod test_support;

pub(crate) const MAX_NATIVE_TICKETS: usize = 8;
const RECEIPT_SLOT_BYTES: usize = atlas_connector::MAX_FRAME_BYTES + 64 * 1024;
const RETAINED_RECEIPT_BYTES: usize = MAX_NATIVE_TICKETS * RECEIPT_SLOT_BYTES;
pub(crate) const NATIVE_TICKET_LIFETIME: Duration = Duration::from_secs(300);

impl SlateApp {
    pub(crate) fn native_binding(&self, node: NodeId) -> Result<BindingSnapshot, String> {
        let session = self
            .connectors
            .sessions
            .get(&(self.tab().id, node))
            .ok_or("Activate Rhino for this model first")?;
        if !session.enabled || session.ended {
            return Err("Activate a current Rhino source before calling native tools".into());
        }
        if self
            .model_node_info(node)
            .is_none_or(|info| info.path != session.path)
        {
            return Err("The linked model changed since Rhino was bound".into());
        }
        session
            .binding
            .clone()
            .ok_or("Rhino is still starting; inspect again after it is ready".into())
    }

    pub(crate) fn native_submit(
        &mut self,
        node: NodeId,
        expected: &BindingSnapshot,
        identity: AgentIdentity,
        autonomy: bool,
        operation: NativeOperation,
        lease: Lease,
    ) -> Result<NativeTicket, NativeOutcome> {
        self.native_submit_caller(node, expected, Some((identity, autonomy)), operation, lease)
    }

    /// Human control adapters use the same source-bound ticket and receipt owner.
    /// The adapter must establish a current human gesture and explicit target wire.
    pub(crate) fn native_submit_control(
        &mut self,
        node: NodeId,
        expected: &BindingSnapshot,
        operation: NativeOperation,
        lease: Lease,
    ) -> Result<NativeTicket, NativeOutcome> {
        self.native_submit_caller(node, expected, None, operation, lease)
    }

    fn native_submit_caller(
        &mut self,
        node: NodeId,
        expected: &BindingSnapshot,
        identity: Option<(AgentIdentity, bool)>,
        operation: NativeOperation,
        lease: Lease,
    ) -> Result<NativeTicket, NativeOutcome> {
        let current = self
            .native_binding(node)
            .map_err(NativeOutcome::not_dispatched)?;
        if current.key() != expected.key()
            || current.binding.source.scope() != expected.binding.source.scope()
        {
            return Err(NativeOutcome::not_dispatched(
                "Native source changed during the witnessed approach",
            ));
        }
        let occupied: usize = self
            .connectors
            .sessions
            .values()
            .map(|s| s.tickets.len())
            .sum();
        let reserved = occupied + self.connectors.uncertain.len() + self.connectors.retired.len();
        if reserved >= MAX_NATIVE_TICKETS
            || (reserved + 1) * RECEIPT_SLOT_BYTES > RETAINED_RECEIPT_BYTES
        {
            return Err(NativeOutcome::not_dispatched(
                "Native receipt capacity is full; retain and reconcile existing outcomes",
            ));
        }
        let session_key = (self.tab().id, node);
        let session = self.connectors.sessions.get_mut(&session_key).unwrap();
        if session.pending > 0 || !session.tickets.is_empty() {
            return Err(NativeOutcome::not_dispatched(
                "Another native operation is still pending on this model",
            ));
        }
        let selected = match &operation {
            NativeOperation::Inspect => Some("org.slate.rhino.inspect"),
            NativeOperation::Capture(_) => None,
            NativeOperation::Invoke { command, .. } => Some(command.as_str()),
        };
        let mut permit = None;
        if let Some(command) = selected {
            let descriptor = current
                .commands
                .iter()
                .find(|c| c.id == command)
                .ok_or_else(|| {
                    NativeOutcome::not_dispatched(
                        "Choose a command from this model's native capabilities",
                    )
                })?;
            if (identity.is_some() && !descriptor.agent_allowed)
                || descriptor.effect == atlas_connector::CommandEffect::SourceWriteBack
            {
                return Err(NativeOutcome::not_dispatched(
                    "This native command is not available to board agents",
                ));
            }
            if descriptor.effect == atlas_connector::CommandEffect::DraftMutation {
                if identity.as_ref().is_some_and(|(_, autonomy)| !autonomy)
                    || current.binding.mode != atlas_connector::SourceMode::IsolatedDraft
                {
                    return Err(NativeOutcome::not_dispatched("Open an isolated Rhino copy and grant this collaborator mutation autonomy first"));
                }
                let commands = current
                    .commands
                    .iter()
                    .filter(|c| {
                        c.agent_allowed && c.effect == atlas_connector::CommandEffect::DraftMutation
                    })
                    .map(|c| c.id.clone())
                    .collect();
                if let Some((identity, _)) = &identity {
                    permit = Some(DraftPermit {
                        identity: identity.clone(),
                        key: current.key(),
                        commands,
                        grant_id: format!("native-{}", atlas_agent::request_id()),
                    });
                }
            }
        }
        let id = format!("native-{}", atlas_agent::request_id());
        let expires = std::time::Instant::now() + NATIVE_TICKET_LIFETIME;
        let (reply, result) = crossbeam_channel::bounded(1);
        let ticket = NativeTicket {
            id: id.clone(),
            key: current.key(),
            lease: lease.clone(),
        };
        let actor = identity.as_ref().map(|(identity, _)| identity.clone());
        let request = NativeRequest {
            ticket: id.clone(),
            key: current.key(),
            expected: current.binding.source.scope(),
            caller: match identity {
                Some((identity, autonomy)) => NativeCaller::Agent {
                    identity,
                    autonomy,
                    permit,
                },
                None => NativeCaller::Human,
            },
            operation,
            lease: lease.clone(),
            expires,
            reply,
        };
        session
            .tx
            .try_send(worker::Action::Native(request))
            .map_err(|_| {
                NativeOutcome::not_dispatched("Native operation queue is full or disconnected")
            })?;
        session.tickets.insert(
            id.clone(),
            TicketEntry {
                id,
                key: ticket.key.clone(),
                lease,
                expires,
                reply: result,
                terminal: None,
                actor,
            },
        );
        Ok(ticket)
    }

    pub(crate) fn native_poll(
        &mut self,
        node: NodeId,
        ticket: &NativeTicket,
    ) -> Result<Option<Completion>, String> {
        let key = (self.tab().id, node);
        let session = self
            .connectors
            .sessions
            .get_mut(&key)
            .ok_or("The native source was replaced or closed")?;
        if !session.tickets.contains_key(&ticket.id) {
            return self
                .connectors
                .uncertain
                .iter()
                .chain(self.connectors.recent.iter())
                .find(|entry| entry.ticket == ticket.id && entry.key == ticket.key)
                .cloned()
                .map(Some)
                .ok_or("Unknown or consumed native receipt".into());
        }
        let entry = session
            .tickets
            .get_mut(&ticket.id)
            .ok_or("Unknown or consumed native receipt")?;
        if entry.key != ticket.key || !entry.lease.same(&ticket.lease) {
            return Err("Native receipt belongs to another caller".into());
        }
        if entry.terminal.is_none() {
            if std::time::Instant::now() >= entry.expires {
                entry.lease.revoke();
            }
            match entry.reply.try_recv() {
                Ok(completion) => entry.terminal = Some(completion),
                Err(crossbeam_channel::TryRecvError::Empty)
                    if std::time::Instant::now() < entry.expires =>
                {
                    return Ok(None)
                }
                Err(_) => {
                    entry.lease.revoke();
                    let outcome = if entry.lease.dispatched() {
                        NativeOutcome::OutcomeUnknown { receipt: None, reason: "Native completion became unavailable after admission; inspect the native document before a fresh edit".into() }
                    } else {
                        NativeOutcome::not_dispatched(
                            "Native request expired or disconnected before admission",
                        )
                    };
                    entry.terminal = Some(Completion {
                        ticket: ticket.id.clone(),
                        key: ticket.key.clone(),
                        binding_after: session.binding.as_ref().map(|s| s.binding.clone()),
                        observation: session.binding.as_ref().map_or(0, |s| s.observation),
                        outcome,
                        actor: entry.actor.clone(),
                    });
                }
            }
        }
        let mut completion = entry.terminal.take().unwrap();
        if !entry.matches(&completion) {
            entry.lease.revoke();
            session.ended = true;
            session.failure = Some("Native completion identity did not match its ticket".into());
            session.tickets.remove(&ticket.id);
            completion.binding_after = None;
            self.native_abandon(completion);
            return Err("Native completion identity did not match its ticket".into());
        }
        session.tickets.remove(&ticket.id);
        self.apply_native_binding(&completion);
        self.audit_native(&completion);
        Ok(Some(completion))
    }

    pub(crate) fn native_artifact(&self, workspace: PathBuf) -> OwnedArtifact {
        let path = workspace
            .join("portal-captures")
            .join(format!("native-{}.png", atlas_agent::request_id()));
        OwnedArtifact { workspace, path }
    }

    pub(crate) fn connector_activate_isolated(&mut self, node: NodeId) {
        if !self.application_connector_eligible(node) {
            self.toast("Choose a Rhino .3dm model first.");
            return;
        }
        let key = (self.tab().id, node);
        let reusable = self.connectors.sessions.get(&key).is_some_and(|s| {
            !s.ended
                && s.binding
                    .as_ref()
                    .is_some_and(|b| b.binding.mode == atlas_connector::SourceMode::IsolatedDraft)
        });
        if !reusable {
            self.connector_leave_contents();
            self.retire_connector_session(key);
        }
        self.activate_connector_mode(
            node,
            Some(worker::DraftGrant {
                grant_id: format!("host-{}", atlas_agent::request_id()),
                agent_id: "explicit-isolated-setup".into(),
                commands: Vec::new(),
            }),
        );
    }
}
