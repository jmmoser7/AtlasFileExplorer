//! Same-host generation and upstream wire creation on the existing flow owner.
use super::*;
use slate_doc::image_paint::{ImageResultId, ImageResultLayers};
use slate_doc::scene::{ImageSettings, Node};

#[cfg(test)]
mod tests;

impl SlateApp {
    pub(crate) fn dispatch_image_selection(&mut self, lasso: bool) -> bool {
        let Some(image) = self.selected_image_nodes().first().copied() else {
            return false;
        };
        if lasso {
            self.start_image_lasso(image);
            true
        } else {
            self.create_image_layer_from_selection(image)
        }
    }

    /// Export receives the shown result's authored stack, just as board paint
    /// does. This changes only the export snapshot, never the workbook.
    pub(crate) fn resolve_export_image_layers(&self, doc: &mut slate_doc::SlateDoc) {
        for node in &mut doc.scene.nodes {
            let Some(result) = self.image_result_id(node.id) else {
                continue;
            };
            let NodeKind::Image(image) = &mut node.kind else {
                continue;
            };
            image.paint_layers = slate_doc::image_paint::layers_for_result(image, &result).to_vec();
        }
    }

    pub(super) fn flow_is_upstream(&self, id: NodeId, grip: Option<(Side, f32)>) -> bool {
        if !matches!(grip, Some((Side::Left, _))) {
            return false;
        }
        self.doc().scene.node(id).is_some_and(|node| {
            agent_inputs::is_flow_node(&self.doc().scene, id)
                || matches!(&node.kind, NodeKind::Image(image)
                    if self.doc().item(image.item).is_some_and(|item|
                        slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Image))
        })
    }

    pub(super) fn flow_input_label(&self, id: NodeId, input: &str) -> String {
        let target = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map_or("image", |a| {
                if a.view == PortalView::Text {
                    "text"
                } else {
                    "image"
                }
            });
        format!("{input} → {target}")
    }

    /// Plan a capability change without changing image geometry or item identity.
    fn image_generator_plan(&self, id: NodeId, provider: &str) -> Option<Node> {
        let mut node = self.doc().scene.node(id)?.clone();
        let NodeKind::Image(original) = &node.kind else {
            return None;
        };
        if original.agent.is_some() {
            return Some(node);
        }
        let item = self.doc().item(original.item)?;
        if slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Image {
            return None;
        }
        let program = atlas_ai::agent::provider_by_id(provider);
        let mut binding_node = node.clone();
        binding_node.kind = NodeKind::Portal(PortalNode::unbound_agent("", program.id.clone()));
        bind_program(&mut binding_node, &program, &self.program_binding(Some(id)));
        let mut agent = slate_doc::agent_chat::agent(&binding_node)?.clone();
        let seed_id = format!("placed-{}-{}", id.0, original.item.0);
        agent.seed = Some(atlas_ai::agent::ImageOutput {
            id: seed_id.clone(),
            source: super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &slate_doc::scene::resolve_source(
                    self.tab().path.as_deref(),
                    &item.path.to_string_lossy(),
                ),
            ),
            request: String::new(),
            prompt: String::new(),
            model: String::new(),
            task: String::new(),
            live: String::new(),
        });
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        let item_result = ImageResultId::Item(image.item);
        let output_result = ImageResultId::Output(seed_id);
        if let Some(stack) = image
            .result_layers
            .iter_mut()
            .find(|s| s.result == item_result)
        {
            stack.result = output_result;
        } else if !image.paint_layers.is_empty() {
            let mut stack = ImageResultLayers::new(output_result);
            stack.layers = std::mem::take(&mut image.paint_layers);
            stack.normalize_metadata();
            image.result_layers.push(stack);
        }
        image.agent = Some(agent);
        Some(node)
    }

    pub(super) fn submit_image_in_place(
        &mut self,
        id: NodeId,
        draft: &AgentDraft,
        provider: &str,
        model: &str,
        aspect: atlas_ai::agent::Aspect,
    ) -> bool {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(mut after) = self.image_generator_plan(id, provider) else {
            return false;
        };
        let Some(agent) = slate_doc::agent_chat::agent_mut(&mut after) else {
            return false;
        };
        agent.provider = provider.to_string();
        agent.model = (!model.is_empty()).then(|| model.to_string());
        agent.instruction = draft.prompt.trim().to_string();
        agent.image = ImageSettings {
            count: draft.count,
            aspect,
            seed: draft.seed,
        };
        agent.live = draft.live;
        if let NodeKind::Image(image) = &mut after.kind {
            image.item = slate_doc::ItemId::NONE;
        }
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return true;
        }
        *self.agents.prompt_mut(id) = draft.prompt.trim().to_string();
        self.board_sel = std::iter::once(id).collect();
        if !draft.live {
            self.queue_generation(id);
        }
        true
    }

    /// An input drag leaves the receiving image in place. The new card's
    /// output lands at the drop point; adding it and binding it is one undo.
    pub(super) fn spawn_flow_input(
        &mut self,
        target: NodeId,
        kind: SpawnKind,
        at: Option<[f32; 2]>,
        provider: Option<String>,
        model: Option<String>,
        settings: ImageSettings,
    ) -> bool {
        let Some(before) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let target_provider = self
            .default_model(true)
            .map(|m| m.0)
            .unwrap_or_else(|| "comfy".into());
        let after = self
            .image_generator_plan(target, &target_provider)
            .unwrap_or_else(|| before.clone());
        let Some(port) = agent_inputs::input_ports_of(&after, false)
            .iter()
            .find(|p| p.slot.takes_text() == (kind == SpawnKind::Text))
            .copied()
        else {
            self.toast("This image has no compatible input.");
            return false;
        };
        let provider = provider
            .as_deref()
            .filter(|p| match kind {
                SpawnKind::Image => atlas_ai::agent::local_image_engine(p),
                SpawnKind::Text => TEXT_ENGINES.contains(p),
            })
            .unwrap_or(if kind == SpawnKind::Image {
                "comfy"
            } else {
                "ollama"
            });
        let mut program = atlas_ai::agent::provider_by_id(provider);
        program.view = if kind == SpawnKind::Image {
            PortalView::Images
        } else {
            PortalView::Text
        };
        let size = if kind == SpawnKind::Image {
            let ratio = settings.aspect.ratio().unwrap_or(1.0).clamp(0.25, 4.0);
            egui::vec2(AGENT_PICTURE_W, AGENT_PICTURE_W / ratio)
        } else {
            program_card_size(program.view)
        };
        let mut input = self.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(0.0, 0.0, size.x, size.y),
            NodeKind::Portal(PortalNode::unbound_agent("", program.id.clone())),
        );
        bind_program(&mut input, &program, &self.program_binding(None));
        let drop = at.map(|[x, y]| Pos2::new(x - size.x, y - OUTPUT_T * size.y));
        let Some(rect) = self.agent_spawn_rect(target, drop, size, self.tab().cam.z) else {
            return false;
        };
        input.rect = rect;
        if let Some(a) = slate_doc::agent_chat::agent_mut(&mut input) {
            a.model = model.filter(|m| !m.is_empty());
            a.image = settings;
        }
        if kind == SpawnKind::Text {
            self.seed_agent_text_output_on_node(&mut input);
        }
        let id = input.id;
        let wire = self.build_connector_with(
            ConnectorEnd::Anchored {
                node: id,
                side: Side::Right,
                t: OUTPUT_T,
            },
            ConnectorEnd::Anchored {
                node: target,
                side: Side::Left,
                t: port.t,
            },
            &[input.clone(), after.clone()],
        );
        let index = self.doc().scene.nodes.len();
        let mut commands = Vec::new();
        if before != after {
            commands.push(SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        commands.push(SceneCmd::Add { index, node: input });
        commands.push(SceneCmd::Add {
            index: index + 1,
            node: wire,
        });
        if !self.commit_scene(commands) {
            return false;
        }
        self.board_sel = std::iter::once(id).collect();
        self.agents.flow.focus_prompt = Some(id);
        true
    }
}
