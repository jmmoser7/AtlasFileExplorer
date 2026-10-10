//! Generator view and generation request.

use super::*;

impl SlateApp {
    /// The inputs an image generator reads now, in wire order. Cached until the
    /// scene or an upstream output changes.
    pub(crate) fn generator_view(&self, id: NodeId) -> std::rc::Rc<GeneratorView> {
        let editing = self.text_edit.as_ref().map(|(node, text)| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (node, text).hash(&mut hasher);
            hasher.finish()
        });
        let key = (self.scene_gen, self.agents.output_epoch, editing);
        if let Some((cached, view)) = self.agents.generator_views.borrow().get(&id) {
            if *cached == key {
                return view.clone();
            }
        }
        let view = std::rc::Rc::new(self.resolve_generator(id));
        self.agents
            .generator_views
            .borrow_mut()
            .insert(id, (key, view.clone()));
        view
    }
    fn resolve_generator(&self, id: NodeId) -> GeneratorView {
        use std::hash::{Hash, Hasher};
        let inputs = match self.agent_input_snapshot(id) {
            Ok(inputs) => inputs,
            Err(error) => {
                return GeneratorView {
                    error: Some(error),
                    ..Default::default()
                }
            }
        };
        let mut view = GeneratorView::default();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut prompts = Vec::new();
        let short = |text: &str| -> String {
            let line = text.lines().next().unwrap_or("").trim();
            let mut out: String = line.chars().take(36).collect();
            if line.chars().count() > 36 {
                out.push('…');
            }
            out
        };
        for item in &inputs.wired {
            let node = NodeId(item.node);
            let slot = item.port();
            if let Some(info) = self
                .model_node_info(node)
                .filter(|_| slot == atlas_agent::InputSlot::Media)
            {
                node.0.hash(&mut hasher);
                view.inputs.push(GeneratorInput {
                    node,
                    role: InputRole::Geometry,
                    label: info
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "3D model".into()),
                });
                continue;
            }
            // A web page's picture is captured when the run starts.
            if item.images.is_empty()
                && !slot.takes_text()
                && slate_doc::agent_inputs::is_web_page(&self.doc().scene, node)
            {
                node.0.hash(&mut hasher);
                view.inputs.push(GeneratorInput {
                    node,
                    role: InputRole::of_slot(slot),
                    label: "Web page".into(),
                });
                continue;
            }
            if let Some(first) = item.images.first().filter(|_| !slot.takes_text()) {
                (slot, &item.images).hash(&mut hasher);
                let label = if matches!(
                    self.doc().scene.node(node).map(|n| &n.kind),
                    Some(NodeKind::Portal(_))
                ) {
                    "Generator output".to_string()
                } else {
                    std::path::Path::new(first)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Image".into())
                };
                view.inputs.push(GeneratorInput {
                    node,
                    role: InputRole::of_slot(slot),
                    label,
                });
                self.append_image_region_prompts(node, slot, &mut hasher, &mut prompts);
                continue;
            }
            let text = item.text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            text.hash(&mut hasher);
            view.inputs.push(GeneratorInput {
                node,
                role: InputRole::Prompt,
                label: short(&text),
            });
            prompts.push(text);
        }
        view.prompt = prompts.join(", ");
        view.signature = hasher.finish();
        view
    }
    /// Wired notes are the prompt. Typed text is added after them.
    fn generator_prompt(&mut self, id: NodeId) -> String {
        let view = self.generator_view(id);
        // The journaled prompt an image agent was submitted with, else a draft.
        let authored = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.instruction.trim().to_string())
            .unwrap_or_default();
        let typed = if authored.is_empty() {
            self.agents.prompt_mut(id).trim().to_string()
        } else {
            authored
        };
        let prompt = [view.prompt.as_str(), typed.as_str()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        if !prompt.is_empty() {
            return prompt;
        }
        self.agent_images(id)
            .last()
            .map(|i| i.prompt.clone())
            .unwrap_or_default()
    }
    pub(super) fn generation_request(
        &mut self,
        id: NodeId,
        live: bool,
    ) -> Result<AgentRequest, String> {
        if let Some(error) = self.generator_view(id).error.clone() {
            return Err(error);
        }
        let prompt = self.generator_prompt(id);
        if prompt.is_empty() {
            return Err("Connect a note with a prompt to this generator.".into());
        }
        let inputs = self.agent_input_snapshot(id)?;
        let (model, session, settings) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| (a.model.clone(), a.session.clone(), a.image))
            .unwrap_or_default();
        let live_run = live.then(|| {
            self.agents
                .live_run
                .entry(id)
                .or_insert_with(atlas_ai::agent::request_id)
                .clone()
        });
        // Live repeats one seed per generator; otherwise a locked seed repeats.
        let image = Some(atlas_ai::agent::ImageParams {
            seed: if live {
                Some(settings.seed.unwrap_or_else(|| stable_seed(&session)))
            } else {
                settings.seed
            },
            live: live_run,
            aspect: settings.aspect,
            count: settings.count,
        });
        Ok(AgentRequest {
            id: atlas_ai::agent::request_id(),
            prompt,
            model,
            at: atlas_ai::context::now_secs(),
            inputs,
            history: vec![],
            image,
            output_dir: None,
            oneshot: false,
            ..Default::default()
        })
    }
    /// Every press is accepted. One generation runs per note at a time.
    pub(crate) fn queue_generation(&mut self, id: NodeId) {
        match self.generation_request(id, false) {
            Ok(request) => self.enqueue_generation(id, request),
            Err(error) => {
                self.toast(error.clone());
                self.fail_agent_await(id, error);
            }
        }
    }
    pub(crate) fn enqueue_generation(&mut self, id: NodeId, request: AgentRequest) {
        if self.ai.config.valid_workspace().is_none() {
            self.fail_agent_await(
                id,
                "Set an AI workspace before generating — the agent link has nowhere to write."
                    .into(),
            );
            self.toast("Set an AI workspace before generating.");
            #[cfg(not(test))]
            self.ai.pick_workspace();
            return;
        }
        let queue = self.agents.comfy_queue.entry(id).or_default();
        if queue.len() >= GENERATION_QUEUE {
            self.toast("Eight generations are already waiting on this generator.");
            return;
        }
        queue.push_back(request);
        if matches!(
            self.agents.awaiting.get(&id),
            Some(AgentAwait::Failed { .. })
        ) {
            self.agents.awaiting.remove(&id);
        }
        self.pump_comfy_queue();
    }
    pub(crate) fn generations_waiting(&self, id: NodeId) -> usize {
        self.agents.comfy_queue.get(&id).map_or(0, |q| q.len())
    }
}
