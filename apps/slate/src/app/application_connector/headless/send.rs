//! Explicit human Send prepares selected geometry, independently of model memory.
use super::*;

pub(super) struct PendingSend {
    expected: slate_doc::scene::Node,
    prompt: String,
    path: PathBuf,
    key: super::super::native::BindingKey,
    lease: Lease,
    expires: std::time::Instant,
}
pub(super) struct SetupSend {
    expected: slate_doc::scene::Node,
    prompt: String,
    expires: std::time::Instant,
}
impl PendingSend {
    pub(super) fn cancel(self) {
        self.lease.revoke();
    }
}

impl SlateApp {
    /// Called only by the registered human Send action. Preview selection alone
    /// never reaches this path, and no membership/grant/checkpoint is created.
    pub(crate) fn prepare_headless_model_send(&mut self, node: NodeId) -> Result<bool, String> {
        if self.grasshopper_viewer(node) {
            return Err("Bake the Grasshopper preview to edit it with the 3D agent".into());
        }
        self.validate_model_generator_count(node)?;
        let key = (self.tab().id, node);
        if self.model_node_info(node).is_none() {
            return Ok(false);
        }
        if self.tab().read_only || self.doc().scene.node(node).is_none_or(|n| n.locked) {
            return Err("Model is read-only or locked".into());
        }
        if !self.connectors.sessions.get(&key).is_some_and(|s| {
            s.enabled
                && !s.ended
                && s.descriptor
                    .as_ref()
                    .is_some_and(|d| d.mode == SurfaceMode::Headless)
        }) {
            if !self.application_connector_eligible(node) {
                return Err("Native model generation requires a supported Rhino model".into());
            }
            self.connector_activate_headless(node)?;
        }
        if self.connectors.sessions.get(&key).is_some_and(|s| {
            s.path == self.model_node_info(node).unwrap().path && s.binding.is_none()
        }) {
            self.connectors.headless.setup_sends.insert(
                key,
                SetupSend {
                    expected: self.doc().scene.node(node).unwrap().clone(),
                    prompt: self.agents.prompt(node).unwrap_or_default().into(),
                    expires: std::time::Instant::now() + Duration::from_secs(30),
                },
            );
            return Ok(true);
        }
        let Some(session) = self.connectors.sessions.get(&key).filter(|s| {
            s.descriptor
                .as_ref()
                .is_some_and(|d| d.mode == SurfaceMode::Headless)
        }) else {
            return Ok(false);
        };
        let info = self
            .model_node_info(node)
            .ok_or("Selected model was removed")?;
        if info.path == session.path {
            return Ok(false);
        }
        if self.tab().read_only || self.doc().scene.node(node).is_none_or(|n| n.locked) {
            return Err("Historical model source is read-only or locked".into());
        }
        if !self
            .model_generator_outputs(node)
            .iter()
            .any(|o| o.current && o.source == info.path)
            || !owned_output_path(self.tab().path.as_deref(), &info.path)
        {
            return Err(
                "Explicitly activate this external model source before sending native work".into(),
            );
        }
        if self.connectors.headless.sends.contains_key(&key) {
            return Err("Selected model source is already preparing for Send".into());
        }
        if session.pending > 0 || !session.tickets.is_empty() {
            return Err("Finish or Stop the current native operation before sending from another model revision".into());
        }
        let binding = session
            .binding
            .as_ref()
            .ok_or("Headless source is still preparing")?;
        if binding.binding.mode != atlas_connector::SourceMode::IsolatedDraft {
            return Err("Historical Send requires the existing isolated draft scope".into());
        }
        let lease = Lease::new();
        let binding_key = binding.key();
        let pending = PendingSend {
            expected: self.doc().scene.node(node).unwrap().clone(),
            prompt: self.agents.prompt(node).unwrap_or_default().into(),
            path: info.path.clone(),
            key: binding_key.clone(),
            lease: lease.clone(),
            expires: std::time::Instant::now() + Duration::from_secs(20),
        };
        session
            .tx
            .try_send(worker::Action::PrepareHeadless {
                key: binding_key,
                path: info.path,
                revision: binding.binding.source.revision.clone(),
                lease,
            })
            .map_err(|_| "Headless source preparation queue is unavailable")?;
        self.connectors.headless.sends.insert(key, pending);
        Ok(true)
    }

    pub(super) fn pump_headless_sends(&mut self, ctx: &egui::Context) {
        let setups = std::mem::take(&mut self.connectors.headless.setup_sends);
        for (key, setup) in setups {
            if self.at_home
                || self.tab().id != key.0
                || std::time::Instant::now() >= setup.expires
                || self.doc().scene.node(key.1) != Some(&setup.expected)
                || self.agents.prompt(key.1).unwrap_or_default() != setup.prompt
            {
                continue;
            }
            if self.native_binding(key.1).is_ok() {
                self.send_human_model_prompt(key.1);
            } else if let Some(error) = self
                .connectors
                .sessions
                .get(&key)
                .and_then(|s| s.failure.clone())
            {
                self.fail_agent_await(key.1, error);
            } else {
                self.connectors.headless.setup_sends.insert(key, setup);
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        }
        let pending = std::mem::take(&mut self.connectors.headless.sends);
        for (key, send) in pending {
            if self.at_home || self.tab().id != key.0 {
                send.cancel();
                continue;
            }
            let valid = !self.tab().read_only
                && self.doc().scene.node(key.1) == Some(&send.expected)
                && self.agents.prompt(key.1).unwrap_or_default() == send.prompt
                && send.lease.live()
                && std::time::Instant::now() < send.expires;
            if !valid {
                send.cancel();
                continue;
            }
            let Some(session) = self.connectors.sessions.get_mut(&key) else {
                send.cancel();
                continue;
            };
            if session.ended
                || session.failure.is_some()
                || session
                    .binding
                    .as_ref()
                    .is_some_and(|b| b.key() != send.key)
            {
                send.cancel();
                continue;
            }
            let ready = session.binding.as_ref().is_some_and(|b| {
                b.binding.source.locator.absolute.as_deref()
                    == Some(send.path.to_string_lossy().as_ref())
            });
            if ready && send.lease.publish() {
                session.path = send.path; // Binding event already verified source/hash.
                                          // Ordinary send rechecks current provider/membership and memory cut.
                self.send_human_model_prompt(key.1);
            } else {
                self.connectors.headless.sends.insert(key, send);
            }
        }
        if !self.connectors.headless.sends.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

fn owned_output_path(workbook: Option<&std::path::Path>, path: &std::path::Path) -> bool {
    let local = atlas_core::index::data_dir().join("headless-rhino/models");
    if path.parent() == Some(local.as_path()) {
        return true;
    }
    let Some(workbook) = workbook else {
        return false;
    };
    let root = atlas_core::workbook_assets::assets_root(workbook).join("generated/headless-rhino");
    path.strip_prefix(root).is_ok_and(|relative| {
        relative.components().count() == 2
            && relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{application_connector::native::test_support, tests::Harness};
    fn fixture() -> (Harness, NodeId, test_support::Peer, PathBuf) {
        let mut h = Harness::new("historical-model-send");
        h.app.ensure_work_tab();
        h.app.leave_home();
        let (node, peer) =
            test_support::install(&mut h, atlas_connector::SourceMode::IsolatedDraft);
        h.app
            .connectors
            .sessions
            .get_mut(&(h.app.tab().id, node))
            .unwrap()
            .descriptor = Some(atlas_connector::SurfaceDescriptor {
            surface_id: "headless".into(),
            mode: SurfaceMode::Headless,
            process_ownership: atlas_connector::ProcessOwnership::LaunchedDedicated,
            native_handle: None,
            can_detach: true,
        });
        let path = atlas_core::index::data_dir().join("headless-rhino/models/native-prior.3dm");
        h.app.patch_nodes(&[node], |n| {
            if let slate_doc::scene::NodeKind::Image(image) = &mut n.kind {
                image.agent = slate_doc::scene::PortalNode::unbound_agent("model", "codex").agent;
                let agent = image.agent.as_mut().unwrap();
                agent.generator.history_cut = true;
                agent.generator.model_outputs = vec![slate_doc::scene::ModelOutputRef {
                    id: "prior".into(),
                    source: slate_doc::SourceUri {
                        locator: path.to_string_lossy().into(),
                    },
                    size: 1,
                    mtime: 0,
                }];
            }
        });
        h.app.select_model_generator_output(node, "prior").unwrap();
        (h, node, peer, path)
    }
    #[test]
    fn historical_preview_only_then_explicit_send_prepares_without_memory_or_authority_changes() {
        let (mut h, node, peer, path) = fixture();
        let original = h.app.connectors.sessions[&(h.app.tab().id, node)]
            .binding
            .clone()
            .unwrap();
        assert!(peer.next_preparation().is_none());
        assert!(
            h.app.native_binding(node).is_err(),
            "viewing does not silently rebind"
        );
        h.app.board_sel.clear();
        h.app.board_sel.insert(node);
        assert!(
            h.app.send_selected_agent_prompt(),
            "registered Send queues geometry preparation before provider work"
        );
        let (key, selected, revision, lease) = peer.next_preparation().unwrap();
        assert!(key == original.key());
        assert_eq!(selected, path);
        assert_eq!(revision, original.binding.source.revision);
        assert!(lease.live());
        assert!(
            h.app
                .doc()
                .scene
                .node(node)
                .and_then(slate_doc::agent_chat::agent)
                .unwrap()
                .generator
                .history_cut
        );
        assert!(
            h.app.presence.agents_empty(),
            "Send preparation cannot create membership or grants"
        );
        h.app.patch_nodes(&[node], |n| n.rect.x += 1.);
        h.app.pump_headless_sends(&egui::Context::default());
        assert!(!lease.live(), "changing the target cancels pending Send");
    }
    #[test]
    fn read_only_or_external_selected_model_cannot_prepare_send() {
        let (mut h, node, peer, _) = fixture();
        h.app.tab_mut().read_only = true;
        assert!(h.app.prepare_headless_model_send(node).is_err());
        assert!(peer.next_preparation().is_none());
        h.app.tab_mut().read_only = false;
        let external = h.base.join("external.3dm");
        let item = h.app.item_for_ready_path(&external, 1, 0);
        h.app.patch_nodes(&[node], |n| {
            if let slate_doc::scene::NodeKind::Image(image) = &mut n.kind {
                image.item = item;
            }
        });
        assert!(h.app.prepare_headless_model_send(node).is_err());
        assert!(peer.next_preparation().is_none());
    }
}
