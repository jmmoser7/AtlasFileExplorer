//! Images carried-history actions adapt existing immutable Memory pins and cuts.
use super::*;
#[cfg(test)]
mod tests;
mod ui;

pub(super) struct HistoryView {
    tab: u64,
    source: NodeId,
    turns: Vec<atlas_agent::AgentTurn>,
}

impl SlateApp {
    /// Freeze public context before a downstream child is prepared.
    pub(crate) fn downstream_generator_history(
        &self,
        source: NodeId,
    ) -> Result<Option<agent_inputs::GeneratorMemoryPin>, String> {
        if !self.image_history_preserved(source) {
            return Ok(None);
        }
        let parent = self
            .doc()
            .scene
            .node(source)
            .ok_or("The source is missing")?;
        let agent = slate_doc::agent_chat::agent(parent);
        let turns = match agent {
            Some(agent) if agent.generator.text_response_only => agent
                .generator
                .text_context
                .iter()
                .filter(|turn| matches!(turn.role.as_str(), "user" | "assistant"))
                .cloned()
                .collect(),
            _ => self.public_image_history(source)?,
        };
        Ok(Some(agent_inputs::GeneratorMemoryPin {
            source_revision: agent.map_or(String::new(), |agent| agent.session.clone()),
            turns,
        }))
    }

    pub(crate) fn image_history_preserved(&self, id: NodeId) -> bool {
        self.pending_generator_preserve(id).unwrap_or_else(|| {
            self.doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.generator.preserve_history.unwrap_or(true))
        })
    }
    /// Metadata-only dependency discovery: safe before endpoint admission.
    pub(crate) fn image_history_dependencies(
        &self,
        id: NodeId,
        action: &str,
    ) -> Result<Vec<NodeId>, String> {
        if !self.image_history_host(id) {
            return Err("Choose an Images history host".into());
        }
        let mut ids = vec![id];
        let incoming = self.image_history_edge(id, true);
        match action {
            "refresh" if incoming.is_none() => {
                return Err("No inherited history is connected".into())
            }
            "reconnect" if incoming.is_some() => {
                return Err("History is already connected; use refresh".into())
            }
            "cut" if !agent_inputs::generator_history_is_terminal(&self.doc().scene, id) => {
                return Err("Only terminal inherited history can be cut".into())
            }
            "refresh" | "reconnect" | "cut" | "view" | "downstream" => (),
            _ => return Err("Unknown Images history action".into()),
        }
        if action != "downstream" || self.image_history_preserved(id) {
            if let Some((wire, source, _)) = incoming {
                ids.extend([source, wire]);
            } else if action == "reconnect" {
                ids.push(
                    self.doc()
                        .scene
                        .node(id)
                        .and_then(slate_doc::agent_chat::agent)
                        .and_then(|a| a.generator.history_source)
                        .ok_or("No remembered history source")?,
                );
            } else if action == "cut" {
                return Err("No inherited history is connected".into());
            }
        }
        if action == "cut" {
            for node in &self.doc().scene.nodes {
                let NodeKind::Connector(conn) = &node.kind else {
                    continue;
                };
                let Some(binding) = conn.binding.as_ref() else {
                    continue;
                };
                if binding.kind == InputKind::Memory
                    && agent_inputs::endpoint_node(binding.target_end(conn)) == Some(id)
                {
                    for dependency in [
                        Some(node.id),
                        agent_inputs::endpoint_node(binding.source_end(conn)),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if !ids.contains(&dependency) {
                            ids.push(dependency);
                        }
                    }
                }
            }
        }
        for dependency in &ids {
            let node = self
                .doc()
                .scene
                .node(*dependency)
                .ok_or("An Images history dependency is missing")?;
            if action != "view" && node.locked {
                return Err("An Images history dependency is locked".into());
            }
        }
        Ok(ids)
    }

    pub(crate) fn image_history_required_endpoints(
        &self,
        id: NodeId,
        action: &str,
    ) -> Result<Vec<String>, String> {
        if action == "downstream" && !self.image_history_preserved(id) {
            return Ok(Vec::new());
        }
        let mut endpoints = Vec::new();
        for dependency in self.image_history_dependencies(id, action)? {
            let node = self
                .doc()
                .scene
                .node(dependency)
                .ok_or("History dependency disappeared")?;
            if let Some(agent) = slate_doc::agent_chat::agent(node) {
                if !agent.session.is_empty() && !endpoints.contains(&agent.session) {
                    endpoints.push(agent.session.clone());
                }
            }
            if let NodeKind::Connector(conn) = &node.kind {
                if let Some(binding) = conn.binding.as_ref().filter(|b| {
                    b.kind == InputKind::Memory && b.slot.as_deref() == Some("generator_memory")
                }) {
                    let endpoint = agent_inputs::generator_memory_endpoint(binding)?;
                    if !endpoint.is_empty() && !endpoints.contains(&endpoint) {
                        endpoints.push(endpoint);
                    }
                }
            }
        }
        Ok(endpoints)
    }

    pub(crate) fn capture_image_history_request(
        &self,
        id: NodeId,
        request: &mut AgentRequest,
        policy: Option<bool>,
    ) -> Result<(), String> {
        if request.image.is_none() {
            return Ok(());
        }
        let Some(preserve) = policy else {
            return Ok(());
        };
        request.image_history = Some(preserve);
        request.image_history_prefix = None;
        request.oneshot = true;
        request.history = if preserve {
            self.image_history_ready(id)?;
            let inherited = agent_inputs::generator_memory_snapshot_checked(&self.doc().scene, id)?
                .map(|pin| pin.turns)
                .unwrap_or_default();
            let visible = self
                .agents
                .session(id)
                .map_or(&[][..], |s| s.turns.as_slice());
            let previous = self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.generator.inherited_turn_count);
            let (history, prefix) =
                agent_inputs::compose_image_history(&inherited, visible, previous)?;
            request.image_history_prefix = Some(prefix);
            history
        } else {
            Vec::new()
        };
        Ok(())
    }
    pub(crate) fn image_history_host(&self, id: NodeId) -> bool {
        self.generator_source_kind(id) == Some(PendingHost::Image)
            && self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_none_or(|a| a.generator.input_source_kind.is_none())
    }
    /// Public session snapshots only: never activity, tools, reasoning or execution.
    pub(crate) fn image_history_ready(&self, id: NodeId) -> Result<(), String> {
        if self.doc().scene.node(id).is_none() {
            return Err("The image source is missing".into());
        }
        if self.agents.session(id).is_none()
            && self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.bundle.is_some())
        {
            return Err("Wait for this saved conversation to load".into());
        }
        Ok(())
    }
    pub(crate) fn public_image_history(
        &self,
        id: NodeId,
    ) -> Result<Vec<atlas_agent::AgentTurn>, String> {
        self.image_history_ready(id)?;
        if let Some(session) = self.agents.session(id) {
            return Ok(session
                .turns
                .iter()
                .filter(|t| matches!(t.role.as_str(), "user" | "assistant"))
                .cloned()
                .collect());
        }
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.bundle.is_some())
        {
            return Err("Wait for this saved conversation to load".into());
        }
        Ok(
            agent_inputs::generator_memory_snapshot_checked(&self.doc().scene, id)?
                .map(|pin| pin.turns)
                .unwrap_or_default()
                .into_iter()
                .filter(|t| matches!(t.role.as_str(), "user" | "assistant"))
                .collect(),
        )
    }
    /// The JS handle chooses the first history edge on that side; typed data is separate.
    pub(crate) fn image_history_edge(
        &self,
        id: NodeId,
        incoming: bool,
    ) -> Option<(NodeId, NodeId, NodeId)> {
        self.doc().scene.nodes.iter().find_map(|node| {
            let NodeKind::Connector(conn) = &node.kind else {
                return None;
            };
            let binding = conn.binding.as_ref()?;
            if binding.kind != InputKind::Memory
                || binding.slot.as_deref() != Some("generator_memory")
            {
                return None;
            }
            let source = agent_inputs::endpoint_node(binding.source_end(conn))?;
            let target = agent_inputs::endpoint_node(binding.target_end(conn))?;
            ((if incoming { target } else { source }) == id).then_some((node.id, source, target))
        })
    }
    pub(crate) fn image_history_handle(
        &self,
        id: NodeId,
        incoming: bool,
        body: Rect,
        z: f32,
    ) -> Option<Rect> {
        if !self.image_history_host(id) {
            return None;
        }
        let preserve = self.pending_generator_preserve(id).unwrap_or_else(|| {
            self.doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.generator.preserve_history.unwrap_or(true))
        });
        if !preserve {
            return None;
        }
        let visible = if incoming {
            self.image_history_edge(id, true).is_some()
        } else {
            self.image_history_edge(id, false).is_some()
                || self.image_history_edge(id, true).is_some()
                || self.agents.session(id).is_some_and(|s| !s.turns.is_empty())
        };
        visible.then(|| {
            Rect::from_center_size(
                Pos2::new(
                    if incoming {
                        body.left() + 6.0 * z
                    } else {
                        body.right() - 6.0 * z
                    },
                    body.top() + 10.0 * z,
                ),
                egui::vec2(7.0, 7.0) * z,
            )
        })
    }
    pub(crate) fn prepare_refresh_image_history(
        &self,
        id: NodeId,
    ) -> Result<Vec<SceneCmd>, String> {
        if !self.image_history_host(id) {
            return Err("Choose an image generator".into());
        }
        let before = self.doc().scene.node(id).ok_or("Image is missing")?.clone();
        let edge = self.image_history_edge(id, true);
        let source = edge
            .map(|(_, source, _)| source)
            .or_else(|| {
                slate_doc::agent_chat::agent(&before)?
                    .generator
                    .history_source
            })
            .ok_or("No saved history source can be reconnected")?;
        if source == id || !self.image_history_host(source) {
            return Err("The saved history source is no longer available".into());
        }
        let turns = self.public_image_history(source)?;
        let previous = self
            .agents
            .session(id)
            .map_or(&[][..], |s| s.turns.as_slice());
        let mut after = self.graph_settings_node(id)?;
        let agent = slate_doc::agent_chat::agent_mut(&mut after).unwrap();
        // An existing legacy pin plus loaded turns has an unknown inherited
        // boundary. Refuse instead of guessing a prefix or reviving cut context.
        if agent.generator.inherited_turn_count.is_none() && edge.is_some() && !previous.is_empty()
        {
            return Err("Cut this legacy inherited history before reconnecting it; its saved boundary is unknown".into());
        }
        if edge.is_none() && agent.generator.inherited_turn_count.is_none() {
            agent.generator.inherited_turn_count = Some(0);
        }
        agent.generator.history_source = Some(source);
        agent.generator.preserve_history = Some(true);
        agent.generator.history_cut = false;
        let revision = self
            .doc()
            .scene
            .node(source)
            .and_then(slate_doc::agent_chat::agent)
            .map_or(String::new(), |a| a.session.clone());
        let pin = agent_inputs::GeneratorMemoryPin {
            source_revision: revision,
            turns,
        };
        let mut commands = vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after.clone()),
        }];
        if let Some((wire, _, _)) = edge {
            let before = self.doc().scene.node(wire).unwrap().clone();
            let mut after = before.clone();
            if let NodeKind::Connector(conn) = &mut after.kind {
                conn.binding.as_mut().unwrap().output =
                    Some(serde_json::to_string(&pin).map_err(|e| e.to_string())?);
            }
            commands.push(SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        } else {
            let wire = self
                .prepare_generator_memory_pin_wire(
                    &mut self.doc().scene.node_preparation(),
                    source,
                    &after,
                    pin,
                )
                .ok_or("The history connection could not be prepared")?;
            commands.push(SceneCmd::Add {
                index: self.doc().scene.nodes.len(),
                node: wire,
            });
        }
        let source_before = self.doc().scene.node(source).unwrap().clone();
        let mut source_after = self.graph_settings_node(source)?;
        slate_doc::agent_chat::agent_mut(&mut source_after)
            .unwrap()
            .generator
            .preserve_history = Some(true);
        if source_before != source_after {
            commands.push(SceneCmd::Patch {
                before: Box::new(source_before),
                after: Box::new(source_after),
            });
        }
        Ok(commands)
    }
    pub(crate) fn image_history_command(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Action {
            node: u64,
            action: String,
            #[serde(default)]
            incoming: bool,
        }
        let Some(action) = detail.and_then(|s| serde_json::from_str::<Action>(s).ok()) else {
            return false;
        };
        let id = NodeId(action.node);
        if !self.image_history_host(id) {
            return false;
        }
        match action.action.as_str() {
            "view" => match self.public_image_history(id) {
                Ok(turns) => {
                    self.agents.flow.history_view = Some(HistoryView {
                        tab: self.tab().id,
                        source: id,
                        turns,
                    });
                    true
                }
                Err(error) => {
                    self.toast(error);
                    false
                }
            },
            "cut" | "refresh" | "reconnect" | "downstream" => {
                let target = if action.action == "cut" {
                    let Some((_, _, target)) = self.image_history_edge(id, action.incoming) else {
                        return false;
                    };
                    target
                } else {
                    id
                };
                let detail = serde_json::json!({"targets":[target.0.to_string()],"arguments":{"action":action.action}}).to_string();
                self.dispatch_targeted_human("board.image.history", Some(&detail))
            }
            _ => false,
        }
    }
    pub(crate) fn paint_carried_history_view(&mut self, ctx: &egui::Context) {
        let Some(view) = self.agents.flow.history_view.take() else {
            return;
        };
        if view.tab != self.tab().id || self.doc().scene.node(view.source).is_none() {
            return;
        }
        let turns = view
            .turns
            .iter()
            .map(|t| (t.role.as_str(), t.text.as_str()))
            .collect::<Vec<_>>();
        if !atlas_shell::selection_tools::generator::carried_history_dialog(
            ctx,
            Id::new(("carried-image-history", view.tab, view.source.0)),
            &turns,
            self.dark_mode,
        ) {
            self.agents.flow.history_view = Some(view);
        }
    }
}
