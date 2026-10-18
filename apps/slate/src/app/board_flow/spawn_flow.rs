//! One journaled spawn and its explicit content/memory connections.
use super::*;
use slate_doc::scene::Node;

impl SlateApp {
    /// Pure media preparation shared by the wire-drop UI and scoped commands.
    /// Binding creates fresh session data; preparation never starts a provider,
    /// registers an item, writes a file, changes selection, or commits a node.
    pub(crate) fn prepare_flow_media_node(
        scene: &mut slate_doc::scene::Scene,
        kind: SpawnKind,
        rect: slate_doc::WorldRect,
        program: &atlas_ai::agent::AgentProvider,
        binding: &super::super::board_agent::ProgramBinding,
    ) -> Node {
        let mut portal = PortalNode::unbound_agent("", program.id.clone());
        if kind == SpawnKind::Artifact {
            let agent = portal.agent.take();
            portal = PortalNode::unbound_web("Artifact");
            portal.agent = agent;
            portal.web.as_mut().unwrap().artifact = Some(Default::default());
        }
        let mut node = scene.build_node(rect, NodeKind::Portal(portal));
        bind_program(&mut node, program, binding);
        node.rect = rect;
        node
    }

    pub(crate) fn spawn_flow_node(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        struct Spawn {
            source: u64,
            kind: String,
            #[serde(default)]
            at: Option<[f32; 2]>,
            #[serde(default)]
            prompt: String,
            /// Engine for an image agent (`comfy`, `codex-image`, `openai-image`).
            #[serde(default)]
            provider: Option<String>,
            #[serde(default)]
            model: Option<String>,
            #[serde(default)]
            image: slate_doc::scene::ImageSettings,
            #[serde(default)]
            live: bool,
            /// The source grip a dropped wire left from.
            #[serde(default)]
            from: Option<(Side, f32)>,
        }
        let Some(spawn) = detail.and_then(|d| serde_json::from_str::<Spawn>(d).ok()) else {
            return false;
        };
        let source = NodeId(spawn.source);
        if !self.is_agent_media(source) {
            return false;
        }
        if spawn.kind == AGENT_CHOICE.id {
            return self.spawn_agent_train(source, spawn.at, spawn.from);
        }
        let Some(kind) = SpawnKind::from_id(&spawn.kind) else {
            return false;
        };
        if kind == SpawnKind::Artifact && self.artifact_revision_loading(source) {
            self.toast("Wait for the selected artifact revision to load before creating a fork.");
            return false;
        }
        let doc = self.doc();
        let Some(carries) = agent_inputs::wire_kind(&doc.scene, source, &|id| {
            doc.item(id).map(|item| item.path.as_path())
        }) else {
            return false;
        };
        // Without an explicit choice the new frame takes the model the person
        // last chose for this kind of agent.
        let (provider, model) = match spawn.provider.clone() {
            Some(provider)
                if kind == SpawnKind::Artifact
                    && !atlas_ai::runtime::linear_provider(&provider) =>
            {
                self.toast("Artifact generation requires Cursor or Codex.");
                return false;
            }
            Some(provider) => (Some(provider), spawn.model.clone()),
            None if kind == SpawnKind::Artifact => self
                .doc()
                .scene
                .node(source)
                .and_then(slate_doc::agent_chat::agent)
                .filter(|a| atlas_ai::runtime::linear_provider(&a.provider))
                .map(|a| (Some(a.provider.clone()), a.model.clone()))
                .or_else(|| {
                    self.default_flow_model(SpawnKind::Artifact)
                        .filter(|(provider, _)| atlas_ai::runtime::linear_provider(provider))
                        .map(|(provider, model)| {
                            (Some(provider), (!model.is_empty()).then_some(model))
                        })
                })
                .unwrap_or_else(|| (Some("codex".into()), None)),
            None => match self.default_flow_model(kind) {
                Some((provider, model)) => (Some(provider), Some(model)),
                None => (None, None),
            },
        };
        if self.flow_is_upstream(source, spawn.from) {
            return self.spawn_flow_input(source, kind, spawn.at, provider, model, spawn.image);
        }
        let program = match kind {
            SpawnKind::Artifact => atlas_ai::agent::provider_by_id(
                provider
                    .as_deref()
                    .filter(|p| atlas_ai::runtime::linear_provider(p))
                    .unwrap_or("codex"),
            ),
            SpawnKind::Image => atlas_ai::agent::provider_by_id(
                provider
                    .as_deref()
                    .filter(|p| atlas_ai::agent::local_image_engine(p))
                    .unwrap_or("comfy"),
            ),
            SpawnKind::Text => atlas_ai::agent::AgentProvider {
                view: PortalView::Text,
                display_name: "Text".into(),
                ..atlas_ai::agent::provider_by_id(
                    provider
                        .as_deref()
                        .filter(|p| TEXT_ENGINES.contains(p))
                        .unwrap_or("ollama"),
                )
            },
        };
        // The agent makes media: a picture shaped like the one it will make,
        // or a sticky it writes into.
        let size = match kind {
            SpawnKind::Image => {
                let source_ratio = self
                    .doc()
                    .scene
                    .node(source)
                    .map(|n| n.rect.w / n.rect.h.max(1.0))
                    .filter(|r| r.is_finite() && *r > 0.0);
                let ratio = spawn
                    .image
                    .aspect
                    .ratio()
                    .or(source_ratio)
                    .unwrap_or(1.0)
                    .clamp(0.25, 4.0);
                egui::vec2(AGENT_PICTURE_W, AGENT_PICTURE_W / ratio)
            }
            SpawnKind::Text => program_card_size(program.view),
            SpawnKind::Artifact => egui::vec2(960.0, 540.0),
        };
        let binding = self.program_binding(None);
        let mut node = Self::prepare_flow_media_node(
            &mut self.doc_mut().scene,
            kind,
            slate_doc::WorldRect::new(0.0, 0.0, size.x, size.y),
            &program,
            &binding,
        );
        // A web page gives the image agent its picture and the text agent its
        // visible text (read as the run starts).
        let doc = self.doc();
        let reads_text = if kind == SpawnKind::Artifact && carries == InputKind::Artifact {
            false
        } else if agent_inputs::is_web_page(&doc.scene, source) {
            kind == SpawnKind::Text
        } else {
            !agent_inputs::feeds_picture(&doc.scene, source, &|id| {
                doc.item(id).map(|item| item.path.as_path())
            })
        };
        let Some(port) = agent_inputs::input_ports_of(&node, self.model_has_viewport(node.id))
            .iter()
            .find(|p| p.slot.takes_text() == reads_text)
            .copied()
        else {
            self.toast("This node has no input for that output.");
            return false;
        };
        // A dropped wire's end becomes the new node's port.
        let site = self.wire_input_host(&node).anchor(Side::Left, port.t);
        let drop = spawn
            .at
            .map(|[x, y]| Pos2::new(x, y - (site[1] - node.rect.y)));
        let zoom = self.tab().cam.z;
        let Some(rect) = self.agent_spawn_rect(source, drop, size, zoom) else {
            return false;
        };
        node.rect = rect;
        let prompt = spawn.prompt.trim().to_string();
        let run = !prompt.is_empty();
        let inherited_parameters = self
            .doc()
            .scene
            .node(source)
            .and_then(slate_doc::agent_chat::agent)
            .filter(|a| Some(&a.provider) == provider.as_ref() && a.model == model)
            .map(|a| a.model_parameters.clone())
            .unwrap_or_default();
        if let Some(a) = slate_doc::agent_chat::agent_mut(&mut node) {
            a.instruction = if run {
                prompt
            } else if kind == SpawnKind::Text && carries == InputKind::Images {
                DESCRIBE_STYLE.into()
            } else {
                String::new()
            };
            a.model = model.clone().filter(|m| !m.is_empty());
            a.model_parameters = inherited_parameters;
            a.image = spawn.image;
            a.live = spawn.live && kind == SpawnKind::Image;
        }
        let id = node.id;
        // Both wires bind and route against the planned node before one commit.
        let (side, t) = spawn.from.unwrap_or((Side::Right, OUTPUT_T));
        let wire = self.build_connector_with(
            ConnectorEnd::Anchored {
                node: source,
                side,
                t,
            },
            ConnectorEnd::Anchored {
                node: id,
                side: Side::Left,
                t: port.t,
            },
            std::slice::from_ref(&node),
        );
        if kind == SpawnKind::Text {
            self.seed_agent_text_output_on_node(&mut node);
        }
        let memory = (kind == SpawnKind::Artifact)
            .then(|| self.build_artifact_memory_wire(source, &node))
            .flatten();
        let mut nodes = vec![node, wire];
        nodes.extend(memory);
        if self.add_nodes(nodes).is_empty() {
            return false;
        }
        self.board_sel = std::iter::once(id).collect();
        if kind == SpawnKind::Artifact {
            let instruction = self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .map(|agent| agent.instruction.clone())
                .unwrap_or_default();
            self.sync_instruction(id, &instruction);
            self.open_object_panel(super::super::board_properties::Panel::Agent);
        }
        if !run {
            self.agents.flow.focus_prompt = Some(id);
        }
        if run {
            match kind {
                SpawnKind::Image if spawn.live => {}
                SpawnKind::Image => self.queue_generation(id),
                SpawnKind::Text => self.queue_text_block(id),
                SpawnKind::Artifact => {
                    self.queue_artifact_generation(id);
                }
            }
        }
        true
    }
}
