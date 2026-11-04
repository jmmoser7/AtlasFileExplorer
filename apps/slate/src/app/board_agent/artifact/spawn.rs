use super::*;
use slate_doc::agent_inputs::{ArtifactMemoryPin, InputKind, WireBinding};
use slate_doc::scene::{ConnectorEnd, Side};

impl SlateApp {
    pub(crate) fn build_generator_memory_wire(
        &mut self,
        source: NodeId,
        target: &Node,
    ) -> Option<Node> {
        let source_node = self.doc().scene.node(source)?;
        let source_agent = slate_doc::agent_chat::agent(source_node)?;
        if slate_doc::agent_chat::is_chat_host(source_node) {
            return None;
        }
        let turns = self
            .agents
            .sessions
            .get(&source)
            .map(|s| s.turns.clone())
            .unwrap_or_default();
        let pin = slate_doc::agent_inputs::GeneratorMemoryPin {
            source_revision: source_agent.session.clone(),
            turns,
        };
        self.build_generator_memory_pin_wire(source, target, pin)
    }

    pub(crate) fn build_generator_memory_pin_wire(
        &mut self,
        source: NodeId,
        target: &Node,
        pin: slate_doc::agent_inputs::GeneratorMemoryPin,
    ) -> Option<Node> {
        let template = self.generator_memory_wire_template(source, target, pin)?;
        Some(
            self.doc_mut()
                .scene
                .build_node(template.rect, template.kind),
        )
    }

    pub(crate) fn prepare_generator_memory_pin_wire(
        &self,
        scratch: &mut slate_doc::Scene,
        source: NodeId,
        target: &Node,
        pin: slate_doc::agent_inputs::GeneratorMemoryPin,
    ) -> Option<Node> {
        let template = self.generator_memory_wire_template(source, target, pin)?;
        Some(scratch.build_node(template.rect, template.kind))
    }

    fn generator_memory_wire_template(
        &self,
        source: NodeId,
        target: &Node,
        pin: slate_doc::agent_inputs::GeneratorMemoryPin,
    ) -> Option<Node> {
        Some(self.connector_template_bound(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Top,
                t: 1.0,
            },
            ConnectorEnd::Anchored {
                node: target.id,
                side: Side::Top,
                t: 0.0,
            },
            std::slice::from_ref(target),
            Some(WireBinding {
                input_b: true,
                kind: InputKind::Memory,
                output: Some(serde_json::to_string(&pin).ok()?),
                order: vec![],
                all_images: false,
                consumed: false,
                slot: Some("generator_memory".into()),
            }),
        ))
    }

    /// Archive active context by moving future requests to a fresh session.
    /// Completed results remain linked while the old transcript is archived.
    pub(crate) fn generator_history_cut_patch(&self, id: NodeId) -> Option<SceneCmd> {
        let before = self.doc().scene.node(id)?.clone();
        let incoming_source = self.doc().scene.nodes.iter().find_map(|node| {
            let NodeKind::Connector(conn) = &node.kind else {
                return None;
            };
            let binding = conn.binding.as_ref()?;
            (binding.kind == InputKind::Memory
                && slate_doc::agent_inputs::endpoint_node(binding.target_end(conn)) == Some(id))
            .then(|| slate_doc::agent_inputs::endpoint_node(binding.source_end(conn)))
            .flatten()
        });
        let mut after = before.clone();
        let agent = slate_doc::agent_chat::agent_mut(&mut after)?;
        agent.generator.history_source = agent.generator.history_source.or(incoming_source);
        agent.generator.inherited_turn_count = Some(0);
        if let Some(bundle) = &agent.bundle {
            if !agent.generator.archived_history.contains(bundle) {
                agent.generator.archived_history.push(bundle.clone());
            }
        }
        agent.generator.preserve_history = Some(false);
        agent.generator.history_cut = true;
        agent.live = false;
        agent.session = slate_doc::scene::new_agent_session_id();
        agent.channel = None;
        let root = self
            .ai
            .config
            .valid_workspace()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| atlas_core::index::data_dir().join("archived-generator-history"));
        let dir = atlas_ai::agent::agent_dir(&root, &agent.session);
        std::fs::create_dir_all(&dir).ok()?;
        let mut retained = self.agent_images(id).as_ref().clone();
        for image in &mut retained {
            image.source = resolve_source(self.tab().path.as_deref(), &image.source)
                .to_string_lossy()
                .into_owned();
        }
        let session = atlas_agent::AgentSession {
            provider: agent.provider.clone(),
            bundle: atlas_agent::ImageBundle { images: retained },
            ..Default::default()
        };
        let manifest = dir.join("session.json");
        atlas_ai::agent::atomic_write_json(&manifest, &session).ok()?;
        agent.bundle = Some(slate_doc::SourceUri {
            locator: super::super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &manifest,
            ),
        });
        if let Some(binding) = artifact_mut(&mut after) {
            binding.active_memory = None;
        }
        Some(SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        })
    }
    pub(crate) fn build_artifact_memory_wire(
        &mut self,
        source: NodeId,
        target: &Node,
    ) -> Option<Node> {
        let source_node = self.doc().scene.node(source)?;
        let a = artifact(source_node)?;
        let agent = slate_doc::agent_chat::agent(source_node)?;
        if agent.generator.history_cut {
            return None;
        }
        let shown = ArtifactMemoryPin {
            revision: a.revision.clone()?,
            through: a.checkpoint,
        };
        let pin = if agent.generator.preserve_history.is_none() {
            shown
        } else {
            a.active_memory.clone().unwrap_or(shown)
        };
        let binding = WireBinding {
            input_b: true,
            kind: InputKind::Memory,
            output: Some(serde_json::to_string(&pin).ok()?),
            order: vec![],
            all_images: false,
            consumed: false,
            slot: Some("memory".into()),
        };
        let mut wire = self.build_connector_bound(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Top,
                t: 1.0,
            },
            ConnectorEnd::Anchored {
                node: target.id,
                side: Side::Top,
                t: 0.0,
            },
            std::slice::from_ref(target),
            Some(binding),
        );
        if let NodeKind::Connector(conn) = &mut wire.kind {
            conn.label = Some("Memory".into());
        }
        Some(wire)
    }
    pub(crate) fn prepare_generator_history_cut(
        &self,
        id: NodeId,
    ) -> Result<Vec<SceneCmd>, String> {
        if self.tab().read_only {
            return Err("This workbook is read-only.".into());
        }
        if self.doc().scene.node(id).is_some_and(|node| node.locked) {
            return Err("Unlock this generator before disconnecting its history.".into());
        }
        if !slate_doc::agent_inputs::generator_history_is_terminal(&self.doc().scene, id)
            || self
                .doc()
                .scene
                .node(id)
                .is_none_or(slate_doc::agent_chat::is_chat_host)
        {
            return Err("Only the final generator's incoming history can be disconnected.".into());
        }
        let mut cmds = self
            .doc()
            .scene
            .nodes
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, node)| {
                let NodeKind::Connector(conn) = &node.kind else {
                    return None;
                };
                let binding = conn.binding.as_ref()?;
                (binding.kind == InputKind::Memory
                    && slate_doc::agent_inputs::endpoint_node(binding.target_end(conn)) == Some(id))
                .then(|| SceneCmd::Remove {
                    index,
                    node: node.clone(),
                })
            })
            .collect::<Vec<_>>();
        if cmds.is_empty() {
            return Err(
                "No inherited Memory is connected. The next Play already starts fresh.".into(),
            );
        }
        let Some(patch) = self.generator_history_cut_patch(id) else {
            return Err(
                "The fresh history manifest could not be prepared. History remains connected."
                    .into(),
            );
        };
        cmds.push(patch);
        Ok(cmds)
    }

    pub(crate) fn finish_generator_history_cut(&mut self, id: NodeId) {
        self.agents.sessions.remove(&id);
        self.agents.bindings.remove(&id);
        self.agents.artifact.inspect.remove(&id);
        if self.image_history_host(id) {
            self.sync_pending_generator_preserve(id, false);
        }
    }

    pub(crate) fn artifact_disconnect_memory(&mut self, id: NodeId) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let cmds = match self.prepare_generator_history_cut(id) {
            Ok(cmds) => cmds,
            Err(error) => {
                self.toast(error);
                return false;
            }
        };
        // Stop uses the original runtime directory/session, before the one
        // journal transaction switches this target to its fresh context.
        self.cancel_generator_history_work(id);
        let changed = self.commit_scene(cmds);
        if changed {
            self.finish_generator_history_cut(id);
            self.toast("Memory disconnected. The next Play starts a fresh conversation.");
        }
        changed
    }
    pub(crate) fn artifact_fork(&mut self, id: NodeId) -> bool {
        if !self
            .doc()
            .scene
            .node(id)
            .and_then(artifact)
            .is_some_and(|a| a.revision.is_some())
        {
            self.toast("Apply a completed artifact revision before creating a fork.");
            return false;
        }
        let forked = self.spawn_flow_node(Some(
            &serde_json::json!({"source":id.0,"kind":"artifact"}).to_string(),
        ));
        if forked {
            self.toast("Artifact fork created with the selected revision and Memory.");
        }
        forked
    }
}
