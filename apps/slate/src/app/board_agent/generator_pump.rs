//! Comfy queue, live pump, and model steer.

use super::*;

impl SlateApp {
    pub(crate) fn pump_comfy_queue(&mut self) {
        let ready: Vec<NodeId> = self
            .agents
            .comfy_queue
            .iter()
            .filter(|(id, queue)| !queue.is_empty() && !self.agent_is_running(**id))
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
            let since = self
                .agents
                .capture_wait
                .get(&id)
                .copied()
                .unwrap_or_else(Instant::now);
            let allow_stale = since.elapsed() > CAPTURE_TIMEOUT;
            match self.capture_generator_inputs(id, &mut request, allow_stale) {
                Ok(true) => {}
                Ok(false) => {
                    let since = *self
                        .agents
                        .capture_wait
                        .entry(id)
                        .or_insert_with(Instant::now);
                    if since.elapsed() > CAPTURE_TIMEOUT {
                        self.agents.capture_wait.remove(&id);
                        self.agents.comfy_queue.remove(&id);
                        self.fail_agent_await(
                            id,
                            "The wired 3D model, video or web page did not finish loading.".into(),
                        );
                    }
                    continue;
                }
                Err(error) => {
                    self.agents.capture_wait.remove(&id);
                    self.agents.comfy_queue.remove(&id);
                    self.fail_agent_await(id, error);
                    continue;
                }
            }
            self.agents.capture_wait.remove(&id);
            if let Some(queue) = self.agents.comfy_queue.get_mut(&id) {
                queue.pop_front();
            }
            self.dispatch_generation(id, request);
        }
    }
    /// A wired 3D model is captured when its run starts, from its current camera.
    fn capture_generator_inputs(
        &mut self,
        id: NodeId,
        request: &mut AgentRequest,
        allow_stale: bool,
    ) -> Result<bool, String> {
        let session = self
            .agent_session_for(id)
            .map(|(s, _)| s)
            .unwrap_or_default();
        let dir = atlas_core::index::data_dir()
            .join("comfy-inputs")
            .join(&session);
        for item in &mut request.inputs.wired {
            let node = NodeId(item.node);
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
                continue;
            }
            // A web page on a picture port feeds what it shows, as pixels.
            if item.images.is_empty()
                && !item.port().takes_text()
                && slate_doc::agent_inputs::is_web_page(&self.doc().scene, node)
            {
                match self.capture_web_page(node, &dir)? {
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
                continue;
            }
            // A video feeds the frame it shows, the way a model feeds its view.
            if self.node_is_video(node) {
                match self.capture_video_frame(node, &dir)? {
                    Some(frame) => item.images = vec![frame.to_string_lossy().into_owned()],
                    None => return Ok(false),
                }
                continue;
            }
            if self.model_node_info(node).is_none() {
                continue;
            }
            match self.capture_model_inputs(node, &dir)? {
                Some(capture) => {
                    item.images = vec![capture.view.to_string_lossy().into_owned()];
                    item.depth = capture
                        .depth
                        .map(|path| path.to_string_lossy().into_owned());
                }
                None => return Ok(false),
            }
        }
        // A text block reads what was captured (a page's text), not what its
        // wires held when it was queued.
        if request.oneshot {
            request.prompt = self.text_block_prompt(id, &request.inputs);
        }
        Ok(true)
    }
    fn dispatch_generation(&mut self, id: NodeId, request: AgentRequest) {
        let Some((session, provider)) = self.agent_session_for(id) else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            return;
        };
        let dir = self
            .agent_link_dir(id, &ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &session));
        let manifest = slate_doc::SourceUri {
            locator: super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &dir.join("session.json"),
            ),
        };
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.bundle.is_none())
        {
            self.patch_nodes(&[id], |n| {
                if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                    a.bundle.get_or_insert_with(|| manifest.clone());
                }
            });
        }
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
        #[cfg(test)]
        {
            let _ = (provider, ws);
            self.agents.dispatched.push((id, request));
        }
        #[cfg(not(test))]
        {
            let cwd = self.agent_folder_for(id).unwrap_or_else(|| ws.clone());
            let workbook = self.tab().path.clone();
            let runtime = self.agents.codex.entry(session).or_insert_with(|| {
                atlas_ai::runtime::CodexLink::start_beside(dir, cwd, provider, workbook)
            });
            if let Err(error) = runtime.send(request) {
                self.fail_agent_await(id, error);
            }
        }
    }
    fn live_generators(&self) -> Vec<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                slate_doc::agent_chat::agent(n)
                    .is_some_and(|a| atlas_ai::agent::local_image_engine(&a.provider) && a.live)
            })
            .map(|n| n.id)
            .collect()
    }
    /// Live re-renders whenever a wired input changes: the camera of a wired
    /// model, a prompt, a source image, or the checkpoint. Latest wins.
    pub(super) fn pump_live_generators(&mut self) {
        use std::hash::{Hash, Hasher};
        self.release_steering();
        for id in self.live_generators() {
            if self.agent_is_running(id) || self.generations_waiting(id) > 0 {
                continue;
            }
            let view = self.generator_view(id);
            if view.inputs.is_empty() {
                continue;
            }
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            view.signature.hash(&mut hasher);
            self.agents.prompt_mut(id).trim().hash(&mut hasher);
            self.doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .map(|a| a.model.clone())
                .hash(&mut hasher);
            if let Some(model) = view.geometry() {
                let Some(pose) = self.model_pose_hash(model) else {
                    continue;
                };
                pose.hash(&mut hasher);
            }
            let signature = hasher.finish();
            if self.agents.live_sent.get(&id) == Some(&signature) {
                self.agents.live_settle.remove(&id);
                continue;
            }
            let typing = self.text_edit.as_ref().is_some_and(|(node, _)| {
                view.inputs
                    .iter()
                    .any(|i| i.node == *node && i.role == InputRole::Prompt)
            });
            if typing {
                let since = match self.agents.live_settle.get(&id) {
                    Some((pending, at)) if *pending == signature => *at,
                    _ => {
                        self.agents
                            .live_settle
                            .insert(id, (signature, Instant::now()));
                        continue;
                    }
                };
                if since.elapsed() < LIVE_TYPING_SETTLE {
                    continue;
                }
            }
            self.agents.live_settle.remove(&id);
            self.agents.live_sent.insert(id, signature);
            match self.generation_request(id, true) {
                Ok(request) => self.enqueue_generation(id, request),
                Err(error) => self.fail_agent_await(id, error),
            }
        }
    }
    pub(super) fn model_pose_hash(&self, id: NodeId) -> Option<u64> {
        let info = self.model_node_info(id)?;
        if self.model3d.external.contains(&info.cache_key) {
            let stamp = self
                .model3d
                .enscape_stamps
                .get(&info.cache_key)
                .copied()
                .unwrap_or(0);
            return Some(
                stamp
                    ^ (info.rect.w.to_bits() as u64).rotate_left(17)
                    ^ (info.rect.h.to_bits() as u64).rotate_left(41),
            );
        }
        let cam = self
            .model3d
            .live
            .get(&id)
            .map(|vp| vp.cam)
            .unwrap_or(info.cam);
        Some(
            cam.cache_hash()
                ^ (info.rect.w.to_bits() as u64).rotate_left(17)
                ^ (info.rect.h.to_bits() as u64).rotate_left(41),
        )
    }
    /// A focused live generator flies its wired model. Leaving focus locks the
    /// model at that pose, one journaled camera change.
    fn release_steering(&mut self) {
        let done: Vec<(NodeId, NodeId)> = self
            .agents
            .steering
            .iter()
            .filter(|(generator, _)| self.contents_focused() != Some(**generator))
            .map(|(g, m)| (*g, *m))
            .collect();
        for (generator, model) in done {
            self.agents.steering.remove(&generator);
            if self.model3d.live.contains_key(&model) {
                self.lock_model(model);
            }
        }
    }
    pub(super) fn steer_generator_model(&mut self, ui: &egui::Ui, generator: NodeId, body: Rect) {
        let Some(model) = self.generator_view(generator).geometry() else {
            return;
        };
        let response = ui.interact(
            body,
            Id::new(("generator-steer", generator.0)),
            Sense::drag(),
        );
        let scroll = if response.hovered() {
            ui.input(|i| i.smooth_scroll_delta.y)
        } else {
            0.0
        };
        if !(response.dragged() || scroll != 0.0) {
            return;
        }
        if self
            .model_node_info(model)
            .is_some_and(|info| self.model3d.external.contains(&info.cache_key))
        {
            if self.enscape_shown_node() != Some(model) {
                self.open_enscape_node(model);
            }
            return;
        }
        if !self.model3d.live.contains_key(&model) {
            self.unlock_model(model);
            self.agents.steering.insert(generator, model);
        }
        if response.dragged() {
            let delta = response.drag_delta();
            let pan = ui.input(|i| i.modifiers.shift);
            self.model_drag(model, delta.x, delta.y, pan, body.height());
        }
        if scroll != 0.0 {
            self.model_scroll(model, scroll);
        }
        ui.ctx().request_repaint();
    }
    /// Fit the card to the newest image once, so browsing never resizes it.
    pub(super) fn fit_generator_frame(&mut self, id: NodeId, image: &str, size: egui::Vec2) {
        if self.agents.fitted_image.get(&id).map(String::as_str) == Some(image)
            || size.x < 2.0
            || size.y < 2.0
            || self.tab().read_only
            || self.board_drag.is_some()
        {
            return;
        }
        self.agents.fitted_image.insert(id, image.to_string());
        let Some(width) = self.doc().scene.node(id).map(|n| n.rect.w) else {
            return;
        };
        let height = fit_frame_height(width, size.x as u32, size.y as u32);
        if self
            .doc()
            .scene
            .node(id)
            .is_some_and(|n| (n.rect.h - height).abs() > 2.0)
        {
            self.patch_nodes(&[id], |n| n.rect.h = height);
        }
    }
}
