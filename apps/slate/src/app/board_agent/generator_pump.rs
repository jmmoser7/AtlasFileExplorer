//! Comfy queue, live pump, and model steer.
mod history;
mod live;
#[cfg(test)]
mod undo_tests;

use super::*;

impl SlateApp {
    /// Undo cancels only the automatic work tagged by that exact creation.
    pub(crate) fn discard_undone_generator_creation(
        &mut self,
        doc: u64,
        step: slate_doc::scene::GroupToken,
    ) {
        let removed: Vec<_> = self
            .agents
            .comfy_queue
            .iter()
            .flat_map(|(id, queue)| {
                queue
                    .iter()
                    .filter(move |entry| entry.doc == doc && entry.creation_step == Some(step))
                    .map(move |entry| (*id, entry.request.id.clone()))
            })
            .collect();
        for (id, request) in removed {
            self.discard_generation_matching(id, &request, Some((doc, step)));
        }
    }
    pub(super) fn discard_generation_request(&mut self, id: NodeId, request_id: &str) {
        self.discard_generation_matching(id, request_id, None);
    }
    fn discard_generation_matching(
        &mut self,
        id: NodeId,
        request_id: &str,
        creation: Option<(u64, slate_doc::scene::GroupToken)>,
    ) {
        let matches = |entry: &super::runtime::QueuedGeneration| {
            entry.request.id == request_id
                && creation
                    .is_none_or(|(doc, step)| entry.doc == doc && entry.creation_step == Some(step))
        };
        let removed_front = self
            .agents
            .comfy_queue
            .get(&id)
            .is_none_or(|queue| queue.front().is_none_or(matches));
        if let Some(queue) = self.agents.comfy_queue.get_mut(&id) {
            queue.retain(|entry| !matches(entry));
            if queue.is_empty() {
                self.agents.comfy_queue.remove(&id);
            }
        }
        if removed_front {
            self.agents.capture_wait.remove(&id);
        }
        self.agents
            .flow
            .text_answer_requests
            .remove(&(self.tab().id, id, request_id.to_owned()));
    }

    pub(crate) fn pump_comfy_queue(&mut self) {
        let ready: Vec<NodeId> = self
            .agents
            .comfy_queue
            .iter()
            .filter(|(id, queue)| !queue.is_empty() && !self.agent_generation_dispatched(**id))
            .map(|(id, _)| *id)
            .collect();
        for id in ready {
            let Some(mut request) = self
                .agents
                .comfy_queue
                .get(&id)
                .and_then(|queue| queue.front().cloned())
            else {
                continue;
            };
            if request.doc != self.tab().id {
                self.discard_generation_request(id, &request.request.id);
                self.toast(
                    "The queued generation belongs to a different open tab; send it again there.",
                );
                continue;
            }
            match self.poll_agent_input_preparation(&mut request.inputs) {
                Ok(true) => request.request.inputs = request.inputs.inputs.clone(),
                Ok(false) => continue,
                Err(error) => {
                    self.discard_generation_request(id, &request.request.id);
                    self.fail_agent_await(id, error);
                    continue;
                }
            }
            let since = self
                .agents
                .capture_wait
                .get(&id)
                .copied()
                .unwrap_or_else(Instant::now);
            let allow_stale = since.elapsed() > CAPTURE_TIMEOUT;
            match self.capture_prepared_generator_inputs(
                id,
                &mut request.inputs,
                &mut request.request,
                allow_stale,
            ) {
                Ok(true) => {}
                Ok(false) => {
                    let since = *self
                        .agents
                        .capture_wait
                        .entry(id)
                        .or_insert_with(Instant::now);
                    if since.elapsed() > CAPTURE_TIMEOUT {
                        self.discard_generation_request(id, &request.request.id);
                        self.fail_agent_await(
                            id,
                            "The wired 3D model, video or web page did not finish loading.".into(),
                        );
                    }
                    continue;
                }
                Err(error) => {
                    self.discard_generation_request(id, &request.request.id);
                    self.fail_agent_await(id, error);
                    continue;
                }
            }
            self.agents.capture_wait.remove(&id);
            let queue_empty = if let Some(queue) = self.agents.comfy_queue.get_mut(&id) {
                queue.pop_front();
                queue.is_empty()
            } else {
                false
            };
            if queue_empty {
                self.agents.comfy_queue.remove(&id);
            }
            let query = self.agents.flow.text_answer_requests.contains_key(&(
                self.tab().id,
                id,
                request.request.id.clone(),
            ));
            if query {
                self.agents
                    .flow
                    .text_query_runs
                    .insert((self.tab().id, id), request.request.id.clone());
            } else {
                self.agents
                    .flow
                    .text_query_runs
                    .remove(&(self.tab().id, id));
            }
            if !query
                && !self.text_response_only(id)
                && self.is_text_block(id)
                && self.agent_text_window_owned(id)
                && !self.tab().read_only
            {
                self.clear_agent_text_output_file(id);
            }
            let image_prefix = request.request.image_history_prefix;
            self.dispatch_generation(
                id,
                request.request,
                request.author,
                image_prefix,
                request.creation_step,
            );
        }
    }
    /// All branches share one frozen set of dynamic source values. Successful
    /// captures remain frozen while another source is still loading.
    pub(crate) fn capture_prepared_generator_inputs(
        &mut self,
        id: NodeId,
        prep: &mut super::input_prep::InputPreparation,
        request: &mut AgentRequest,
        allow_stale: bool,
    ) -> Result<bool, String> {
        if prep.capture.is_none() {
            prep.freeze_capture(request);
        }
        let shared = prep.capture.as_ref().unwrap().clone();
        let mut capture = shared.borrow().clone();
        if !capture.ready {
            let ready = self.capture_generator_inputs(
                id,
                &mut capture.request,
                allow_stale,
                &mut capture.completed,
            )?;
            capture.ready = ready;
            *shared.borrow_mut() = capture.clone();
            if !ready {
                return Ok(false);
            }
        }
        let old_prompt = request
            .inputs
            .on(atlas_agent::InputSlot::Prompt)
            .map(|value| value.text.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        for item in &mut request.inputs.wired {
            if capture.completed.contains(&(item.node, item.port())) {
                if let Some(frozen) = capture
                    .request
                    .inputs
                    .wired
                    .iter()
                    .find(|value| value.node == item.node && value.port() == item.port())
                {
                    item.text.clone_from(&frozen.text);
                    item.images.clone_from(&frozen.images);
                    item.depth.clone_from(&frozen.depth);
                }
            }
        }
        if self.is_text_block(id)
            && request.pending_prompt_projection.is_none()
            && !old_prompt.is_empty()
            && request.prompt.starts_with(&old_prompt)
        {
            let new_prompt = request
                .inputs
                .on(atlas_agent::InputSlot::Prompt)
                .map(|value| value.text.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            request.prompt = format!("{new_prompt}{}", &request.prompt[old_prompt.len()..]);
        }
        // This pass skips frozen sources and retains per-request passage checks.
        self.capture_generator_inputs(id, request, allow_stale, &mut capture.completed)
    }

    fn capture_generator_inputs(
        &mut self,
        id: NodeId,
        request: &mut AgentRequest,
        allow_stale: bool,
        completed: &mut HashSet<(u64, atlas_agent::InputSlot)>,
    ) -> Result<bool, String> {
        let session = self
            .agent_session_for(id)
            .map(|(s, _)| s)
            .unwrap_or_default();
        let dir = atlas_core::index::data_dir()
            .join("comfy-inputs")
            .join(&session)
            .join(&request.id);
        let capture_agent = format!(
            "slate-generator-{}",
            model_preview::view_meta::hash_bytes(session.as_bytes())
        );
        for item in &mut request.inputs.wired {
            let node = NodeId(item.node);
            let key = (item.node, item.port());
            if completed.contains(&key) {
                continue;
            }
            // A web page on Prompt feeds its visible text, read once as the
            // run starts (D15 / D27 amendment).
            if item.port().takes_text()
                && slate_doc::agent_inputs::is_web_page(&self.doc().scene, node)
            {
                match self.read_web_text(node)? {
                    Some(text) => item.text = text,
                    None if allow_stale => {
                        if let Some(text) = self.stale_web_text(node) {
                            item.text = text;
                        } else {
                            return Ok(false);
                        }
                    }
                    None => return Ok(false),
                }
                completed.insert(key);
                continue;
            }
            // A web page on a picture port feeds what it shows, as pixels.
            if item.images.is_empty()
                && !item.port().takes_text()
                && slate_doc::agent_inputs::is_web_page(&self.doc().scene, node)
            {
                match self.capture_web_page(node, &dir, None)? {
                    Some(page) => item.images = vec![page.to_string_lossy().into_owned()],
                    None if allow_stale => {
                        if let Some(page) = self.stale_web_image(node, &dir)? {
                            item.images = vec![page.to_string_lossy().into_owned()];
                        } else {
                            return Ok(false);
                        }
                    }
                    None => return Ok(false),
                }
                completed.insert(key);
                continue;
            }
            // A video feeds the frame it shows, the way a model feeds its view.
            if self.node_is_video(node) {
                match self.capture_video_frame(node, &dir)? {
                    Some(frame) => item.images = vec![frame.to_string_lossy().into_owned()],
                    None => return Ok(false),
                }
                completed.insert(key);
                continue;
            }
            if self.model_node_info(node).is_none() {
                continue;
            }
            match self.capture_model_inputs(node, &dir, &capture_agent)? {
                Some(capture) => {
                    item.images = vec![capture.view.to_string_lossy().into_owned()];
                    item.depth = capture
                        .depth
                        .map(|path| path.to_string_lossy().into_owned());
                }
                None => return Ok(false),
            }
            completed.insert(key);
        }
        // A text block reads what was captured (a page's text), not what its
        // wires held when it was queued.
        let projected = request.pending_prompt_projection.is_some();
        request.resolve_pending_prompt_projection();
        if projected {
            let tab = self.tab().id;
            if let Some(snapshot) =
                self.agents
                    .flow
                    .text_answer_requests
                    .get_mut(&(tab, id, request.id.clone()))
            {
                snapshot.question.clone_from(&request.prompt);
            }
        }
        if self.is_text_block(id) {
            if let Some(snapshot) =
                self.agents
                    .flow
                    .text_answer_requests
                    .get(&(self.tab().id, id, request.id.clone()))
            {
                if !self.text_answer_source(id).is_some_and(|(text, revision)| {
                    text == snapshot.source && revision == snapshot.source_revision
                }) {
                    return Err(
                        "The selected passage changed. Select it again and resend your question."
                            .into(),
                    );
                }
                if self
                    .agent_session_for(id)
                    .is_none_or(|(name, _)| name != snapshot.session)
                {
                    return Err("The selected passage belongs to another conversation. Send the question again.".into());
                }
                // Keep the question and passage submitted together, even if the composer changes while queued.
                if snapshot.review {
                    request.prompt.clone_from(&snapshot.question);
                    request.text_source = Some(snapshot.source[snapshot.range.clone()].to_owned());
                } else {
                    request.prompt = format!(
                        "{}\n\nSelected passage:\n{}",
                        snapshot.question,
                        &snapshot.source[snapshot.range.clone()]
                    );
                }
            }
        }
        Ok(true)
    }
    fn dispatch_generation(
        &mut self,
        id: NodeId,
        request: AgentRequest,
        author: slate_doc::scene::CmdAuthor,
        image_prefix: Option<u32>,
        creation_step: Option<slate_doc::scene::GroupToken>,
    ) {
        if request.image.is_some() {
            if let Err(error) = self.image_generation_binding(id) {
                self.fail_agent_await(id, error);
                return;
            }
        }
        let Some((session, provider)) = self.agent_session_for(id) else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            return;
        };
        let dir = self
            .agent_link_dir(id, &ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &session));
        let manifest = self.generation_bundle_manifest(&dir);
        if let Some(mut after) = self
            .doc()
            .scene
            .node(id)
            .filter(|node| {
                slate_doc::agent_chat::agent(node).is_some_and(|agent| agent.bundle.is_none())
            })
            .cloned()
        {
            slate_doc::agent_chat::agent_mut(&mut after).unwrap().bundle = Some(manifest);
            // Only an auto-run carrying this creation's token completes its
            // original gesture. A later manual Send is always a separate step.
            let folded = creation_step.is_some_and(|step| {
                self.tab().journal.top_token() == Some(step)
                    && self.tab().journal.top_creates_node(id, &author)
                    && self.fold_into_last_step(&after)
            });
            if !folded {
                self.patch_nodes_as(&[id], author.clone(), |node| *node = after.clone());
            }
        }
        self.agents.cancelled_generations.remove(&id);
        self.agents.generation_authors.insert(id, author.clone());
        self.agents.requests.insert(id, request.id.clone());
        self.agents.bindings.insert(id, session.clone());
        self.agents.awaiting.insert(
            id,
            AgentAwait::Sent {
                at: Instant::now(),
                req_at: request.at,
            },
        );
        self.agents.sources.expect(&dir, &request.id);
        let image_dialogue = request.image.is_some() && request.image_history == Some(true);
        let admitted;
        #[cfg(test)]
        {
            let _ = (provider, ws);
            self.agents.dispatched.push((id, request));
            admitted = true;
        }
        #[cfg(not(test))]
        {
            let cwd = self.agent_folder_for(id).unwrap_or_else(|| ws.clone());
            let workbook = self.tab().path.clone();
            let runtime = self.agents.codex.entry(session).or_insert_with(|| {
                atlas_ai::runtime::CodexLink::start_beside(
                    dir,
                    cwd,
                    provider,
                    workbook,
                    Some(ws.clone()),
                )
            });
            admitted = match runtime.send(request) {
                Ok(()) => true,
                Err(error) => {
                    self.fail_agent_await(id, error);
                    false
                }
            };
        }
        if admitted {
            if image_dialogue {
                self.agents.prompt_mut(id).clear();
                self.clear_pending_generator_prompt(id);
            }
            if let Some(prefix) = image_prefix {
                self.patch_nodes_as(&[id], author, |node| {
                    if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                        agent.generator.inherited_turn_count = Some(prefix);
                        if image_dialogue {
                            agent.instruction.clear();
                        }
                    }
                });
            }
        }
    }
}
