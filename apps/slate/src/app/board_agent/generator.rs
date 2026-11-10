//! Generator view and generation request.
mod request;

use super::runtime::QueuedGeneration;
use super::*;

#[cfg(test)]
#[path = "generator_tests.rs"]
mod tests;

impl SlateApp {
    /// A direct blister run is ready once its authored primary and prompt wires
    /// have completed values. Style-only references cannot stand in for Media.
    pub(crate) fn image_agent_has_wired_image_and_prompt(&self, id: NodeId) -> bool {
        self.generator_view(id).wired_image_and_prompt
    }

    /// The inputs an image generator reads now, in wire order. Cached until the
    /// scene or an upstream output changes.
    pub(crate) fn generator_view(&self, id: NodeId) -> std::rc::Rc<GeneratorView> {
        let editing = if self.text_edit.is_some() || self.text_doc_edit.is_some() {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.text_edit.hash(&mut hasher);
            self.text_doc_edit
                .as_ref()
                .map(|edit| (edit.node, edit.buffer.as_str()))
                .hash(&mut hasher);
            Some(hasher.finish())
        } else {
            None
        };
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
    pub(crate) fn invalidate_generator_inputs(&self) {
        self.agents.generator_views.borrow_mut().clear();
    }

    fn resolve_generator(&self, id: NodeId) -> GeneratorView {
        use std::hash::{Hash, Hasher};
        let live = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|agent| agent.live);
        let inputs = match self.generator_input_refs(id, !live) {
            Ok(inputs) => inputs,
            Err(error) => {
                return GeneratorView {
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        let mut view = GeneratorView {
            wired_prompt: Self::prompt_with_inputs(&inputs, ""),
            wired_image_and_prompt: inputs
                .on(atlas_agent::InputSlot::Media)
                .any(|item| item.node != id.0 && !item.images.is_empty())
                && inputs
                    .on(atlas_agent::InputSlot::Prompt)
                    .any(|item| !item.text.trim().is_empty()),
            ..Default::default()
        };
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
                let label = if node == id {
                    "Current image".to_string()
                } else if matches!(
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
        let prompt = self.agent_prompt_text(id).trim().to_owned();
        if !prompt.is_empty() {
            return prompt;
        }
        self.agent_images(id)
            .last()
            .map(|i| i.prompt.clone())
            .unwrap_or_default()
    }
    pub(crate) fn generation_request(
        &mut self,
        id: NodeId,
        live: bool,
    ) -> Result<AgentRequest, String> {
        self.generation_request_with_prompt(id, live, None)
    }

    /// Every press is accepted. One generation runs per note at a time.
    pub(crate) fn queue_generation(&mut self, id: NodeId) {
        self.queue_generation_in_step(id, None);
    }
    pub(crate) fn queue_spawned_generation(
        &mut self,
        id: NodeId,
        step: slate_doc::scene::GroupToken,
    ) {
        self.queue_generation_in_step(id, Some(step));
    }
    fn queue_generation_in_step(&mut self, id: NodeId, step: Option<slate_doc::scene::GroupToken>) {
        match self.generation_request(id, false) {
            Ok(request) => {
                self.enqueue_generation_in_step(
                    id,
                    request,
                    slate_doc::scene::CmdAuthor::Human,
                    step,
                );
            }
            Err(error) => {
                self.toast(error.clone());
                self.fail_agent_await(id, error);
            }
        }
    }
    pub(crate) fn enqueue_generation(&mut self, id: NodeId, request: AgentRequest) {
        self.enqueue_generation_as(id, request, slate_doc::scene::CmdAuthor::Human);
    }

    pub(crate) fn enqueue_generation_as(
        &mut self,
        id: NodeId,
        request: AgentRequest,
        author: slate_doc::scene::CmdAuthor,
    ) -> bool {
        self.enqueue_generation_in_step(id, request, author, None)
    }
    fn enqueue_generation_in_step(
        &mut self,
        id: NodeId,
        request: AgentRequest,
        author: slate_doc::scene::CmdAuthor,
        step: Option<slate_doc::scene::GroupToken>,
    ) -> bool {
        let inputs = match self.begin_agent_input_preparation(request.inputs.clone()) {
            Ok(inputs) => inputs,
            Err(error) => {
                self.fail_agent_await(id, error.clone());
                self.toast(error);
                return false;
            }
        };
        self.enqueue_prepared_generation_in_step(id, request, inputs, author, step)
    }
    pub(crate) fn enqueue_prepared_generation(
        &mut self,
        id: NodeId,
        request: AgentRequest,
        inputs: super::input_prep::InputPreparation,
    ) {
        self.enqueue_prepared_generation_as(
            id,
            request,
            inputs,
            slate_doc::scene::CmdAuthor::Human,
        );
    }

    pub(crate) fn check_generation_queue(&self, id: NodeId) -> Result<(), String> {
        if self.ai.config.valid_workspace().is_none() {
            return Err("Set an AI workspace before generating".into());
        }
        if self
            .agents
            .comfy_queue
            .get(&id)
            .map_or(0, |queue| queue.len())
            >= GENERATION_QUEUE
        {
            return Err("Eight generations are already waiting on this generator".into());
        }
        Ok(())
    }

    pub(crate) fn enqueue_prepared_generation_as(
        &mut self,
        id: NodeId,
        request: AgentRequest,
        inputs: super::input_prep::InputPreparation,
        author: slate_doc::scene::CmdAuthor,
    ) -> bool {
        self.enqueue_prepared_generation_in_step(id, request, inputs, author, None)
    }
    fn enqueue_prepared_generation_in_step(
        &mut self,
        id: NodeId,
        mut request: AgentRequest,
        mut inputs: super::input_prep::InputPreparation,
        author: slate_doc::scene::CmdAuthor,
        step: Option<slate_doc::scene::GroupToken>,
    ) -> bool {
        if let Err(error) = self.check_generation_queue(id) {
            self.fail_agent_await(id, error.clone());
            self.toast(error);
            #[cfg(not(test))]
            if self.ai.config.valid_workspace().is_none() {
                self.ai.pick_workspace();
            }
            return false;
        }
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .cloned();
        let settings = agent
            .as_ref()
            .map(|a| a.generator.clone())
            .unwrap_or_default();
        let count = request
            .image
            .as_ref()
            .map(|image| image.count)
            .or(settings.count)
            .unwrap_or(1)
            .max(1);
        let shared = slate_doc::agent_inputs::shared_generator_sources(&self.doc().scene, id);
        let jobs = match slate_doc::agent_inputs::plan_grafted_inputs_with_shared(
            &inputs.inputs,
            &settings.grafted_slots,
            count,
            &shared,
        ) {
            Ok(jobs) => jobs,
            Err(error) => {
                self.fail_agent_await(id, error.clone());
                self.toast(error);
                return false;
            }
        };
        let waiting = self
            .agents
            .comfy_queue
            .get(&id)
            .map_or(0, |queue| queue.len());
        let total: usize = jobs
            .iter()
            .map(|job| {
                if request.image.is_none() {
                    usize::from(job.count)
                } else {
                    1
                }
            })
            .sum();
        if waiting + total > GENERATION_QUEUE {
            let error = format!(
                "{} graft branches exceed the generator's eight-job queue; reduce the inputs or wait.",
                jobs.len()
            );
            self.fail_agent_await(id, error.clone());
            self.toast(error);
            return false;
        }
        // Explicit new settings opt into immutable replay. None preserves the
        // legacy adapter's oneshot/session behavior on existing documents.
        if request.image.is_some() {
            if request.image_history.is_none() {
                if let Err(error) =
                    self.capture_image_history_request(id, &mut request, settings.preserve_history)
                {
                    self.fail_agent_await(id, error.clone());
                    self.toast(error);
                    return false;
                }
            }
            if request.image_history == Some(false) {
                request.history.clear();
                request.image_history_prefix = None;
            }
            if request.image_history == Some(true)
                && request
                    .image_history_prefix
                    .is_none_or(|prefix| prefix as usize > request.history.len())
            {
                self.fail_agent_await(
                    id,
                    "Capture a valid inherited Image history boundary before sending".into(),
                );
                return false;
            }
        } else if let Some(preserve) = settings.preserve_history {
            request.oneshot = !preserve || settings.text_response_only;
            request.history = if settings.text_response_only {
                if preserve {
                    settings.text_context.clone()
                } else {
                    Vec::new()
                }
            } else if preserve {
                self.agents
                    .sessions
                    .get(&id)
                    .map(|s| s.turns.clone())
                    .filter(|turns| !turns.is_empty())
                    .or_else(|| {
                        slate_doc::agent_inputs::generator_memory_snapshot(&self.doc().scene, id)
                            .map(|pin| pin.turns)
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
        }
        let query = self.agents.flow.text_answer_requests.contains_key(&(
            self.tab().id,
            id,
            request.id.clone(),
        ));
        if self.is_text_block(id) && !query {
            self.patch_nodes_as(&[id], author.clone(), |node| {
                if let NodeKind::Image(image) = &mut node.kind {
                    image.text_override = None;
                }
            });
        }
        let doc = self.tab().id;
        inputs.freeze_capture(&request);
        let queue = self.agents.comfy_queue.entry(id).or_default();
        let original_id = request.id.clone();
        let answer = self
            .agents
            .flow
            .text_answer_requests
            .get(&(doc, id, original_id.clone()))
            .cloned();
        let branching = total > 1;
        for job in jobs {
            let copies = if request.image.is_none() {
                job.count
            } else {
                1
            };
            for result in 0..copies {
                let mut branch_request = request.clone();
                if branching {
                    branch_request.id = format!("{}-branch-{}-{result}", original_id, job.branch);
                    branch_request.oneshot = true;
                    if branch_request.prompt_inputs_projected
                        && branch_request.pending_prompt_projection.is_none()
                    {
                        branch_request.prompt = slate_doc::agent_inputs::grafted_prompt(
                            &branch_request.prompt,
                            &inputs.inputs,
                            &job.inputs,
                            if request.image.is_none() {
                                "\n\n"
                            } else {
                                ", "
                            },
                        );
                    }
                }
                if let Some(image) = &mut branch_request.image {
                    image.count = job.count;
                }
                branch_request.inputs = job.inputs.clone();
                let mut branch_inputs = inputs.clone();
                branch_inputs.inputs = job.inputs.clone();
                if let Some(answer) = &answer {
                    self.agents
                        .flow
                        .text_answer_requests
                        .insert((doc, id, branch_request.id.clone()), answer.clone());
                }
                queue.push_back(QueuedGeneration {
                    doc,
                    creation_step: step,
                    author: author.clone(),
                    request: branch_request,
                    inputs: branch_inputs,
                });
            }
        }
        if branching {
            self.agents
                .flow
                .text_answer_requests
                .remove(&(doc, id, original_id));
        }
        if matches!(
            self.agents.awaiting.get(&id),
            Some(AgentAwait::Failed { .. })
        ) {
            self.agents.awaiting.remove(&id);
        }
        self.pump_comfy_queue();
        true
    }
    pub(crate) fn generations_waiting(&self, id: NodeId) -> usize {
        self.agents.comfy_queue.get(&id).map_or(0, |q| q.len())
    }
}
