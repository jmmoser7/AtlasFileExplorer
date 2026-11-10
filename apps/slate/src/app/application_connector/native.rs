//! Scoped native requests share the existing application worker and source owner.
use super::*;
mod outcome;
mod retention;
mod types;
pub(crate) use outcome::outcome_value;
pub(crate) use types::*;
#[cfg(test)]
pub(crate) mod test_support;

pub(crate) const MAX_NATIVE_TICKETS: usize = 8;
const RECEIPT_SLOT_BYTES: usize = atlas_connector::MAX_FRAME_BYTES + 64 * 1024;
const RETAINED_RECEIPT_BYTES: usize = MAX_NATIVE_TICKETS * RECEIPT_SLOT_BYTES;
pub(crate) const NATIVE_TICKET_LIFETIME: Duration = Duration::from_secs(300);

impl SlateApp {
    pub(crate) fn native_binding(&self, node: NodeId) -> Result<BindingSnapshot, String> {
        self.native_binding_in(self.tab().id, node)
    }

    pub(in crate::app::application_connector) fn native_binding_in(
        &self,
        doc: u64,
        node: NodeId,
    ) -> Result<BindingSnapshot, String> {
        let session = self
            .connectors
            .sessions
            .get(&(doc, node))
            .ok_or("Activate Rhino for this model first")?;
        if !session.enabled || session.ended {
            return Err("Activate a current Rhino source before calling native tools".into());
        }
        if self
            .model_node_info_in(doc, node)
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
        self.native_submit_caller(
            (self.tab().id, node),
            expected,
            Some((identity, autonomy)),
            None,
            operation,
            lease,
        )
    }

    pub(in crate::app::application_connector) fn native_submit_model_task_in(
        &mut self,
        (doc, node): (u64, NodeId),
        expected: &BindingSnapshot,
        identity: AgentIdentity,
        task: &ModelTaskPermit,
        operation: NativeOperation,
        lease: Lease,
    ) -> Result<NativeTicket, NativeOutcome> {
        if self.connectors.sessions.get(&(doc, node)).is_none_or(|s| {
            s.descriptor
                .as_ref()
                .is_none_or(|d| d.mode != SurfaceMode::Headless)
        }) {
            return Err(NativeOutcome::not_dispatched(
                "Model task requires its headless isolated worker",
            ));
        }
        self.native_submit_caller(
            (doc, node),
            expected,
            Some((identity, false)),
            Some(task),
            operation,
            lease,
        )
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
        self.native_submit_caller(
            (self.tab().id, node),
            expected,
            None,
            None,
            operation,
            lease,
        )
    }

    fn native_submit_caller(
        &mut self,
        (doc, node): (u64, NodeId),
        expected: &BindingSnapshot,
        identity: Option<(AgentIdentity, bool)>,
        task: Option<&ModelTaskPermit>,
        operation: NativeOperation,
        lease: Lease,
    ) -> Result<NativeTicket, NativeOutcome> {
        let current = self
            .native_binding_in(doc, node)
            .map_err(NativeOutcome::not_dispatched)?;
        if let Some(task) = task {
            let allowed = identity
                .as_ref()
                .is_some_and(|(identity, _)| task.authorizes(doc, node, identity, &current))
                && match &operation {
                    NativeOperation::Inspect | NativeOperation::Capture(_) => true,
                    NativeOperation::Invoke { command, .. } => {
                        command == atlas_connector_runtime::headless_rhino::GENERATE_COMMAND
                    }
                };
            if !allowed {
                return Err(NativeOutcome::not_dispatched("Model generation task expired, changed binding, or requested a different command"));
            }
        }
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
        let session_key = (doc, node);
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
        let mut permit = task.map(|task| {
            Box::new(DraftPermit {
                identity: identity.as_ref().unwrap().0.clone(),
                key: current.key(),
                commands: vec![atlas_connector_runtime::headless_rhino::GENERATE_COMMAND.into()],
                grant_id: format!("native-{}", atlas_agent::request_id()),
                purpose: types::DraftPurpose::ModelTask,
                task: Some(task.clone()),
            })
        });
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
                if identity.as_ref().is_some_and(|(_, autonomy)| !autonomy) && task.is_none()
                    || current.binding.mode != atlas_connector::SourceMode::IsolatedDraft
                {
                    return Err(NativeOutcome::not_dispatched("Open an isolated Rhino copy and grant this collaborator mutation autonomy first"));
                }
                let commands = current
                    .commands
                    .iter()
                    .filter(|c| {
                        c.agent_allowed
                            && c.effect == atlas_connector::CommandEffect::DraftMutation
                            && (task.is_none()
                                || c.id
                                    == atlas_connector_runtime::headless_rhino::GENERATE_COMMAND)
                    })
                    .map(|c| c.id.clone())
                    .collect();
                if let Some((identity, _)) = &identity {
                    permit = Some(Box::new(DraftPermit {
                        identity: identity.clone(),
                        key: current.key(),
                        commands,
                        grant_id: format!("native-{}", atlas_agent::request_id()),
                        purpose: if task.is_some() {
                            types::DraftPurpose::ModelTask
                        } else {
                            types::DraftPurpose::Collaborator
                        },
                        task: task.cloned(),
                    }));
                }
            }
        }
        let id = format!("native-{}", atlas_agent::request_id());
        let expires = task.map_or(std::time::Instant::now() + NATIVE_TICKET_LIFETIME, |task| {
            task.expires
                .min(std::time::Instant::now() + NATIVE_TICKET_LIFETIME)
        });
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
            .try_send(worker::Action::Native(Box::new(request)))
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
        self.native_poll_in(self.tab().id, node, ticket)
    }

    pub(in crate::app::application_connector) fn native_poll_in(
        &mut self,
        doc: u64,
        node: NodeId,
        ticket: &NativeTicket,
    ) -> Result<Option<Completion>, String> {
        let key = (doc, node);
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
