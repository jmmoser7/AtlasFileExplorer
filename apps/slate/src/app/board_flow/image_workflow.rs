//! Same-host generation and upstream wire creation on the existing flow owner.
use super::*;
use slate_doc::image_paint::ImageResultId;
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

    pub(crate) fn dispatch_image_selection_copy(&mut self, ctx: &egui::Context) -> bool {
        let Some(image) = self.selected_image_nodes().first().copied() else {
            return false;
        };
        let Some(mask) = self.image_selection_clip(image) else {
            return false;
        };
        let Some(item) = self.image_selection_item(image) else {
            return false;
        };
        let Some(cutout) = self.image_selection_cutout_node(image, item, &mask) else {
            return false;
        };
        let size = egui::vec2(cutout.rect.w, cutout.rect.h);
        let screen = ctx
            .pointer_latest_pos()
            .filter(|point| self.canvas_rect.contains(*point))
            .unwrap_or(self.canvas_rect.center());
        let world = self.board_xf().s2w(screen);
        let center = self.resolve_point_snap(world, &[image], None, false, false);
        self.copy_image_selection_to_canvas(image, cutout, center - size * 0.5, ctx.clone())
            .is_some()
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
            image.paint_layers = Self::renderer_layers(image, &result);
            image.result_layers.clear();
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
        if original
            .agent
            .as_ref()
            .is_some_and(|a| !a.provider.is_empty())
        {
            if !original.paint_layers.is_empty() {
                if let Some(result) = self.image_result_id(id) {
                    let NodeKind::Image(image) = &mut node.kind else {
                        unreachable!()
                    };
                    slate_doc::image_paint::seed_result_from_legacy(image, result);
                }
            }
            return Some(node);
        }
        let item = self.doc().item(original.item);
        if item.is_none() && original.item != slate_doc::ItemId::NONE {
            return None;
        }
        if item.is_some_and(|item| slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Image)
        {
            return None;
        }
        let program = atlas_ai::agent::provider_by_id(provider);
        let mut binding_node = node.clone();
        binding_node.kind = NodeKind::Portal(PortalNode::unbound_agent("", program.id.clone()));
        bind_program(&mut binding_node, &program, &self.program_binding(Some(id)));
        let mut agent = slate_doc::agent_chat::agent(&binding_node)?.clone();
        // A new prepared session has no saved transcript to wait for.
        // First dispatch installs its manifest through the existing owner.
        agent.bundle = None;
        if let Some(previous) = original.agent.as_ref() {
            agent.generator = previous.generator.clone();
        }
        if let Some(item) = item {
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
                slate_doc::image_paint::seed_result_from_legacy(image, output_result);
            }
        }
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.agent = Some(Box::new(agent));
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
        agent.generator.input_source_kind = None;
        agent.generator.preserve_history = Some(draft.preserve_history);
        agent.generator.count = Some(draft.count);
        agent.model = (!model.is_empty()).then(|| model.to_string());
        agent.model_parameters.clone_from(&draft.model_parameters);
        agent.instruction = draft.prompt.trim().to_string();
        agent.image = ImageSettings {
            count: draft.count,
            aspect,
            seed: draft.seed,
        };
        agent.live = draft.live;
        // Capture the image the person invoked, including its crop and layers,
        // before releasing an album pick to follow the next generated result.
        let prepared = if draft.live {
            None
        } else {
            match self
                .generator_input_refs(id, true)
                .and_then(|inputs| self.begin_agent_input_preparation(inputs))
            {
                Ok(inputs) => Some(inputs),
                Err(error) => {
                    self.toast(error);
                    return true;
                }
            }
        };
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
        if let Some(inputs) = prepared {
            match self.generation_request(id, false) {
                Ok(mut request) => {
                    request.inputs = inputs.inputs.clone();
                    self.enqueue_prepared_generation(id, request, inputs);
                }
                Err(error) => self.toast(error),
            }
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
                SpawnKind::Artifact => false,
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

impl SlateApp {
    /// Run the agent a picture or note was summoned with, once.
    pub(crate) fn run_agent_media(&mut self, id: NodeId) {
        if self.submit_pending_image(id) {
            return;
        }
        if self.is_text_block(id) {
            self.queue_text_block(id);
        } else {
            // Capture the shown result and its authored crop/layers before the
            // card releases an album pick to follow the next result.
            let prepared = self.generation_request(id, false).and_then(|request| {
                self.begin_agent_input_preparation(request.inputs.clone())
                    .map(|inputs| (request, inputs))
            });
            let (request, inputs) = match prepared {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.toast(error.clone());
                    self.fail_agent_await(id, error);
                    return;
                }
            };
            let manifest = self.ai.config.valid_workspace().and_then(|workspace| {
                let (session, _) = self.agent_session_for(id)?;
                let dir = self
                    .agent_link_dir(id, workspace)
                    .unwrap_or_else(|| atlas_ai::agent::agent_dir(workspace, &session));
                Some(self.generation_bundle_manifest(&dir))
            });
            let result = self.image_result_id(id);
            let Some(before) = self.doc().scene.node(id).cloned() else {
                return;
            };
            let count = request
                .image
                .as_ref()
                .filter(|_| {
                    self.generator_source_kind(id) == Some(PendingHost::Image)
                        && !before.locked
                        && !self.tab().read_only
                })
                .map(|image| image.count);
            let mut after = before.clone();
            // The first durable bundle reference completes this same Send,
            // rather than adding a later automatic undo step at dispatch.
            if let Some(agent) = slate_doc::agent_chat::agent_mut(&mut after) {
                if let Some(count) = count {
                    agent.image.count = count;
                    agent.generator.count = Some(count);
                }
                if agent.bundle.is_none() {
                    agent.bundle.clone_from(&manifest);
                }
            }
            if let NodeKind::Image(image) = &mut after.kind {
                if !image.paint_layers.is_empty() {
                    if let Some(result) = result {
                        slate_doc::image_paint::seed_result_from_legacy(image, result);
                    }
                }
                image.item = slate_doc::ItemId::NONE;
            }
            self.last_board_edit = None;
            if before != after
                && !self.commit_scene(vec![SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                }])
            {
                return;
            }
            if let Some(count) = count {
                if let Some(draft) = self.agents.flow.drafts.get_mut(&id) {
                    draft.count = count;
                }
            }
            self.enqueue_prepared_generation(id, request, inputs);
        }
    }
}
