//! Native Slate acceptance for the headless adapter, sharing native ticket leases.
use super::native::{
    Completion, Lease, NativeOperation, NativeOutcome, NativeResult, NativeTicket,
};
use super::*;
use atlas_connector_runtime::headless_rhino::{parse_parameters, GENERATE_COMMAND};
mod blank;
mod catalog;
mod send;
pub(crate) use catalog::ModelGeneratorOutput;

#[cfg(test)]
thread_local! {
    static HEADLESS_START_FOR_TEST: std::cell::RefCell<Option<(
        crossbeam_channel::Sender<worker::Action>, crossbeam_channel::Receiver<worker::Event>,
    )>> = const { std::cell::RefCell::new(None) };
}

#[derive(Default)]
pub(super) struct HumanJobs {
    jobs: HashMap<(u64, NodeId), HumanJob>,
    blanks: Vec<blank::BlankJob>,
    sends: HashMap<(u64, NodeId), send::PendingSend>,
}
struct HumanJob {
    parameters: serde_json::Value,
    expected: PathBuf,
    ticket: Option<NativeTicket>,
}

impl SlateApp {
    /// Scoped host discovery only. Exact native invocation still calls
    /// native_binding after explicit Send has prepared the selected revision.
    pub(crate) fn headless_model_agent_context(
        &self,
        node: NodeId,
    ) -> Option<super::native::BindingSnapshot> {
        let key = (self.tab().id, node);
        let session = self.connectors.sessions.get(&key)?;
        if !session.enabled
            || session.ended
            || session
                .descriptor
                .as_ref()
                .is_none_or(|d| d.mode != SurfaceMode::Headless)
        {
            return None;
        }
        let info = self.model_node_info(node)?;
        if info.path != session.path && !self.headless_history_preview(key, &info.path) {
            return None;
        }
        let binding = session.binding.clone()?;
        (binding.binding.mode == atlas_connector::SourceMode::IsolatedDraft).then_some(binding)
    }

    /// Explicit human setup; agent sending never creates its own draft authority.
    pub(crate) fn connector_activate_headless(&mut self, node: NodeId) -> Result<(), String> {
        let info = self
            .model_node_info(node)
            .ok_or("Choose a placed 3D model for the headless builder")?;
        if self.tab().read_only {
            return Err("Workbook is read-only".into());
        }
        let key = (self.tab().id, node);
        if self.connectors.sessions.get(&key).is_some_and(|s| {
            !s.ended
                && s.path == info.path
                && s.descriptor
                    .as_ref()
                    .is_some_and(|d| d.mode == SurfaceMode::Headless)
        }) {
            return Ok(());
        }
        self.connector_leave_contents();
        self.retire_connector_session(key);
        self.bind_model_agent(node)?;
        #[cfg(not(test))]
        let (tx, rx) = worker::start_headless(info.path.clone());
        #[cfg(test)]
        let (tx, rx) = HEADLESS_START_FOR_TEST
            .with(|fixture| fixture.borrow_mut().take())
            .unwrap_or_else(|| worker::start_headless(info.path.clone()));
        self.connectors.sessions.insert(
            key,
            Session {
                enabled: true,
                pending: 0,
                ended: false,
                failure: None,
                path: info.path,
                tx,
                rx,
                surface: None,
                descriptor: Some(atlas_connector::SurfaceDescriptor {
                    surface_id: "headless".into(),
                    mode: SurfaceMode::Headless,
                    process_ownership: atlas_connector::ProcessOwnership::LaunchedDedicated,
                    native_handle: None,
                    can_detach: true,
                }),
                process_id: 0,
                binding: None,
                tickets: HashMap::new(),
                status: "Preparing isolated headless Rhino builder".into(),
                preview_error_status: None,
                focused: false,
                last_geometry: None,
                host: Default::default(),
                poster: None,
                input_captures: HashMap::new(),
                invocation_receipt: None,
            },
        );
        Ok(())
    }

    pub(crate) fn connector_generate_headless(
        &mut self,
        node: NodeId,
        parameters: serde_json::Value,
    ) -> Result<(), String> {
        parse_parameters(&parameters)?;
        let key = (self.tab().id, node);
        if self.connectors.headless.jobs.contains_key(&key) {
            return Err("Headless model generation is already pending; Stop it first".into());
        }
        self.connector_activate_headless(node)?;
        let expected = self.model_node_info(node).ok_or("Model was removed")?.path;
        self.connectors.headless.jobs.insert(
            key,
            HumanJob {
                parameters,
                expected,
                ticket: None,
            },
        );
        Ok(())
    }

    pub(crate) fn connector_stop_headless(&mut self, node: NodeId) {
        let key = (self.tab().id, node);
        if let Some(send) = self.connectors.headless.sends.remove(&key) {
            send.cancel();
        }
        if let Some(job) = self.connectors.headless.jobs.remove(&key) {
            if let Some(ticket) = job.ticket {
                ticket.lease.revoke();
            }
        }
        if let Some(session) = self.connectors.sessions.get_mut(&key) {
            for ticket in session.tickets.values() {
                ticket.lease.revoke();
            }
            session.status = "Stopping owned headless work; previous model retained".into();
        }
    }

    pub(super) fn pump_headless_models(&mut self, ctx: &egui::Context) {
        self.pump_headless_sends(ctx);
        self.pump_blank_models(ctx);
        let mut pending = std::mem::take(&mut self.connectors.headless.jobs);
        for (key, mut job) in pending.drain() {
            if !self.tabs.iter().any(|t| t.id == key.0) {
                if let Some(ticket) = job.ticket {
                    ticket.lease.revoke();
                }
                continue;
            }
            if self.at_home || self.tab().id != key.0 {
                self.connectors.headless.jobs.insert(key, job);
                continue;
            }
            if self.tab().read_only
                || self
                    .model_node_info(key.1)
                    .is_none_or(|i| i.path != job.expected)
            {
                if let Some(ticket) = job.ticket {
                    ticket.lease.revoke();
                }
                continue;
            }
            if job.ticket.is_none() {
                match self.native_binding(key.1) {
                    Ok(binding) => match self.native_submit_control(
                        key.1,
                        &binding,
                        NativeOperation::Invoke {
                            command: GENERATE_COMMAND.into(),
                            parameters: job.parameters.clone(),
                        },
                        Lease::new(),
                    ) {
                        Ok(ticket) => job.ticket = Some(ticket),
                        Err(outcome) => {
                            self.toast(format!(
                                "Headless generation refused: {}",
                                outcome_reason(&outcome)
                            ));
                            continue;
                        }
                    },
                    Err(_)
                        if self
                            .connectors
                            .sessions
                            .get(&key)
                            .is_some_and(|s| !s.ended && s.failure.is_none()) =>
                    {
                        self.connectors.headless.jobs.insert(key, job);
                        continue;
                    }
                    Err(error) => {
                        self.toast(error);
                        continue;
                    }
                }
            }
            let ticket = job.ticket.as_ref().unwrap();
            match self.native_poll(key.1, ticket) {
                Ok(Some(completion)) => {
                    if matches!(
                        completion.outcome,
                        NativeOutcome::Completed {
                            result: NativeResult::Model { .. }
                        }
                    ) {
                        if ticket.lease.publish() {
                            if let Err(error) =
                                self.native_publish_headless_model(key.1, &completion)
                            {
                                self.native_abandon(completion);
                                self.toast(error);
                            }
                        } else {
                            self.native_abandon(completion);
                        }
                    } else {
                        self.toast(format!(
                            "Headless generation: {}",
                            outcome_reason(&completion.outcome)
                        ));
                    }
                }
                Ok(None) => {
                    self.connectors.headless.jobs.insert(key, job);
                }
                Err(error) => self.toast(error),
            }
        }
        if !self.connectors.headless.jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    /// Called only after the existing caller validation and Lease::publish win.
    pub(crate) fn native_publish_headless_model(
        &mut self,
        node: NodeId,
        completion: &Completion,
    ) -> Result<(), String> {
        let NativeOutcome::Completed {
            result: NativeResult::Model {
                path, size, mtime, ..
            },
        } = &completion.outcome
        else {
            return Ok(());
        };
        if self.tab().read_only || self.doc().scene.node(node).is_none_or(|n| n.locked) {
            return Err("Model is read-only, locked or removed; prior output retained".into());
        }
        let binding = self.native_binding(node)?;
        if binding.key() != completion.key
            || completion.binding_after.as_ref() != Some(&binding.binding)
            || !path.starts_with(atlas_core::index::data_dir().join("headless-rhino/models"))
        {
            return Err("Headless model source/generation changed before acceptance".into());
        }
        if self.connectors.sessions[&(self.tab().id, node)]
            .descriptor
            .as_ref()
            .is_none_or(|s| s.mode != SurfaceMode::Headless)
        {
            return Err("This result is not from the headless adapter".into());
        }
        let item = self.item_for_ready_path(path, *size, *mtime);
        let output = slate_doc::scene::ModelOutputRef {
            id: completion.ticket.clone(),
            source: slate_doc::SourceUri {
                locator: slate_doc::scene::source_locator(self.tab().path.as_deref(), path),
            },
            size: *size,
            mtime: *mtime,
        };
        if self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.generator.model_outputs.len() >= 128)
        {
            return Err("Model revision catalog is full; create a new builder branch before generating more outputs".into());
        }
        let author = completion
            .actor
            .as_ref()
            .map_or(slate_doc::scene::CmdAuthor::Human, |a| {
                slate_doc::scene::CmdAuthor::Agent(a.agent.clone())
            });
        self.connectors.sessions[&(self.tab().id, node)]
            .tx
            .try_send(worker::Action::AcceptHeadless {
                key: completion.key.clone(),
                path: path.clone(),
                revision: binding.binding.source.revision.clone(),
            })
            .map_err(|_| {
                "Accepted model could not be queued for headless source synchronization"
            })?;
        self.patch_nodes_as(&[node], author, |n| {
            if let slate_doc::scene::NodeKind::Image(image) = &mut n.kind {
                image.item = item;
            }
            if let Some(agent) = slate_doc::agent_chat::agent_mut(n) {
                agent.generator.model_outputs.push(output.clone());
                agent.generator.shown_model = Some(output.id.clone());
            }
        });
        self.model3d.live.remove(&node);
        if let Some(session) = self.connectors.sessions.get_mut(&(self.tab().id, node)) {
            session.path = path.clone();
            session.status = "Verified headless model accepted".into();
        }
        self.unlock_model(node);
        Ok(())
    }
}

fn outcome_reason(outcome: &NativeOutcome) -> String {
    match outcome {
        NativeOutcome::NotDispatched { reason, .. }
        | NativeOutcome::OutcomeUnknown { reason, .. } => reason.clone(),
        NativeOutcome::Rejected { error } => error.message.clone(),
        NativeOutcome::RevokedBeforeAdmission => "Cancelled before admission".into(),
        NativeOutcome::Completed { .. } => "Completed".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::native::test_support;
    use super::*;
    use crate::app::tests::Harness;
    use atlas_connector::SourceMode;

    #[test]
    fn headless_acceptance_preserves_frame_camera_and_has_one_undo() {
        let mut h = Harness::new("headless_model_acceptance");
        h.app.ensure_work_tab();
        h.app.leave_home();
        let (node, peer) = test_support::install(&mut h, SourceMode::IsolatedDraft);
        let key = (h.app.tab().id, node);
        h.app.connectors.sessions.get_mut(&key).unwrap().descriptor =
            Some(atlas_connector::SurfaceDescriptor {
                surface_id: "headless".into(),
                mode: SurfaceMode::Headless,
                process_ownership: atlas_connector::ProcessOwnership::LaunchedDedicated,
                native_handle: None,
                can_detach: true,
            });
        let before = h.app.doc().scene.node(node).unwrap().clone();
        let completion = Completion {
            ticket: "fixture".into(),
            key: peer.binding.key(),
            binding_after: Some(peer.binding.binding.clone()),
            observation: 1,
            actor: None,
            outcome: NativeOutcome::Completed {
                result: NativeResult::Model {
                    path: atlas_core::index::data_dir()
                        .join("headless-rhino/models/acceptance-fixture.3dm"),
                    size: 1,
                    mtime: 0,
                    report: serde_json::json!({}),
                },
            },
        };
        h.app
            .native_publish_headless_model(node, &completion)
            .unwrap();
        let after = h.app.doc().scene.node(node).unwrap().clone();
        assert_eq!(before.rect, after.rect);
        assert_eq!(before.rotation_deg, after.rotation_deg);
        let slate_doc::scene::NodeKind::Image(old) = &before.kind else {
            panic!()
        };
        let slate_doc::scene::NodeKind::Image(new) = &after.kind else {
            panic!()
        };
        assert_eq!(old.model, new.model);
        assert_ne!(old.item, new.item);
        let tab = h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert_eq!(tab.doc.scene.node(node), Some(&before));
    }

    #[test]
    fn changed_native_key_refuses_model_acceptance() {
        let mut h = Harness::new("headless_stale_acceptance");
        h.app.ensure_work_tab();
        h.app.leave_home();
        let (node, peer) = test_support::install(&mut h, SourceMode::IsolatedDraft);
        let before = h.app.doc().scene.node(node).unwrap().clone();
        let mut key = peer.binding.key();
        key.worker = "another-generation".into();
        let completion = Completion {
            ticket: "fixture".into(),
            key,
            binding_after: Some(peer.binding.binding),
            observation: 1,
            actor: None,
            outcome: NativeOutcome::Completed {
                result: NativeResult::Model {
                    path: atlas_core::index::data_dir()
                        .join("headless-rhino/models/stale-fixture.3dm"),
                    size: 1,
                    mtime: 0,
                    report: serde_json::json!({}),
                },
            },
        };
        assert!(h
            .app
            .native_publish_headless_model(node, &completion)
            .is_err());
        assert_eq!(h.app.doc().scene.node(node), Some(&before));
    }
}

// Deliver an asynchronous connector observation through the production event pump.
// This fixture deliberately does not touch the document or scene generations.
#[cfg(test)]
impl SlateApp {
    pub(crate) fn test_deliver_headless_identity(
        &mut self,
        node: NodeId,
        binding: super::native::BindingSnapshot,
        restarted: bool,
    ) {
        let key = (self.tab().id, node);
        let session = self.connectors.sessions.get_mut(&key).unwrap();
        let surface = session.descriptor.clone().unwrap();
        let (tx, rx) = crossbeam_channel::bounded(1);
        session.rx = rx;
        std::thread::spawn(move || {
            let event = if restarted {
                worker::Event::Ready {
                    surface,
                    process_id: 0,
                    binding,
                }
            } else {
                worker::Event::Binding(binding)
            };
            tx.send(event).unwrap();
        })
        .join()
        .unwrap();
        self.maintain_application_connectors(&egui::Context::default());
    }
}
