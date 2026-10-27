//! Bounded terminal retention and owned-artifact cleanup outlive caller revocation.
use super::*;

impl SlateApp {
    pub(super) fn apply_native_binding(&mut self, completion: &Completion) {
        let Some(binding) = completion.binding_after.as_ref() else {
            return;
        };
        if binding.document_id != completion.key.document
            || binding.source.source_id != completion.key.source
        {
            return;
        }
        let keys: Vec<_> = self
            .connectors
            .sessions
            .iter()
            .filter(|(key, session)| {
                session.binding.as_ref().is_some_and(|old| {
                    old.key() == completion.key && old.observation <= completion.observation
                }) && self
                    .tabs
                    .iter()
                    .find(|tab| tab.id == key.0)
                    .and_then(|tab| {
                        let slate_doc::scene::NodeKind::Image(image) =
                            &tab.doc.scene.node(key.1)?.kind
                        else {
                            return None;
                        };
                        let item = tab.doc.item(image.item)?;
                        Some(crate::app::board_portal::resolve_source(
                            tab.path.as_deref(),
                            &item.path.to_string_lossy(),
                        ))
                    })
                    .as_ref()
                    == Some(&session.path)
            })
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            if let Some(snapshot) = self
                .connectors
                .sessions
                .get_mut(&key)
                .and_then(|session| session.binding.as_mut())
            {
                snapshot.binding = binding.clone();
                snapshot.observation = completion.observation;
            }
        }
    }
    pub(crate) fn native_source_matches(&self, node: NodeId, key: &BindingKey) -> bool {
        self.connectors
            .sessions
            .get(&(self.tab().id, node))
            .is_some_and(|session| {
                session.enabled
                    && session
                        .binding
                        .as_ref()
                        .is_some_and(|binding| &binding.key() == key)
                    && self
                        .model_node_info(node)
                        .is_some_and(|info| info.path == session.path)
            })
    }
    pub(in crate::app::application_connector) fn retire_connector_session(
        &mut self,
        key: (u64, NodeId),
    ) {
        if let Some(mut session) = self.connectors.sessions.remove(&key) {
            for (_, entry) in session.tickets.drain() {
                entry.lease.revoke();
                self.connectors.retired.push(entry);
            }
        }
    }
    pub(super) fn audit_native(&mut self, completion: &Completion) {
        let kind = match &completion.outcome {
            NativeOutcome::NotDispatched { .. } => "not_dispatched",
            NativeOutcome::RevokedBeforeAdmission => "revoked_before_admission",
            NativeOutcome::Rejected { .. } => "native_rejected",
            NativeOutcome::Completed { .. } => "completed",
            NativeOutcome::OutcomeUnknown { .. } => "outcome_unknown",
        };
        atlas_core::session_log::event(
            "slate.native.receipt",
            &format!("{} {} {}", completion.ticket, completion.key.worker, kind),
        );
        if completion.outcome.unknown() {
            if !self
                .connectors
                .uncertain
                .iter()
                .any(|prior| prior.ticket == completion.ticket)
            {
                // Admission reserves a full framed receipt slot (<=4 MiB plus
                // envelope) before dispatch. Unknown receipts never expire or
                // evict another unknown during this application lifetime.
                self.connectors.uncertain.push(completion.clone());
            }
        } else {
            self.connectors.recent.push_back(completion.clone());
            while self.connectors.recent.len() > 16 {
                self.connectors.recent.pop_front();
            }
        }
    }
    pub(crate) fn native_abandon(&mut self, completion: Completion) {
        self.apply_native_binding(&completion);
        self.audit_native(&completion);
        cleanup_owned(completion);
    }
    pub(in crate::app::application_connector) fn maintain_native_receipts(&mut self) {
        let mut abandoned = Vec::new();
        for session in self.connectors.sessions.values_mut() {
            for entry in session.tickets.values() {
                if std::time::Instant::now() >= entry.expires {
                    entry.lease.revoke();
                }
            }
            let canceled: Vec<_> = session
                .tickets
                .iter()
                .filter(|(_, entry)| !entry.lease.live())
                .map(|(id, _)| id.clone())
                .collect();
            for id in canceled {
                if let Some(entry) = session.tickets.get_mut(&id) {
                    if let Some(completion) = terminal(entry) {
                        abandoned.push(completion);
                        session.tickets.remove(&id);
                    }
                }
            }
        }
        let mut pending = Vec::new();
        for mut entry in std::mem::take(&mut self.connectors.retired) {
            if let Some(completion) = terminal(&mut entry) {
                abandoned.push(completion);
            } else {
                pending.push(entry);
            }
        }
        self.connectors.retired = pending;
        for completion in abandoned {
            self.native_abandon(completion);
        }
    }
}

fn terminal(entry: &mut TicketEntry) -> Option<Completion> {
    if let Some(complete) = entry.terminal.take() {
        return Some(complete);
    }
    match entry.reply.try_recv() {
        Ok(mut completion) => {
            if !entry.matches(&completion) {
                entry.lease.revoke();
                completion.binding_after = None;
            }
            Some(completion)
        }
        Err(crossbeam_channel::TryRecvError::Empty)
            if std::time::Instant::now() < entry.expires =>
        {
            None
        }
        Err(_) => Some(Completion {
            ticket: entry.id.clone(),
            key: entry.key.clone(),
            binding_after: None,
            observation: 0,
            outcome: if entry.lease.dispatched() {
                NativeOutcome::OutcomeUnknown { receipt: None, reason: "Caller left after native admission; inspect the preserved document before a fresh edit".into() }
            } else {
                NativeOutcome::not_dispatched("Native action was revoked before dispatch")
            },
            actor: entry.actor.clone(),
        }),
    }
}

fn cleanup_owned(completion: Completion) {
    if let NativeOutcome::Completed {
        result: NativeResult::Capture { path, .. },
    } = completion.outcome
    {
        std::thread::spawn(move || {
            let _ = std::fs::remove_file(path);
        });
    }
}
