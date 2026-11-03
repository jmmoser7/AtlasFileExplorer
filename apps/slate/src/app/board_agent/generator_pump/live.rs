//! Existing live rendering and model steering on the generator owner.
use super::*;
impl SlateApp {
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
    pub(in crate::app::board_agent) fn pump_live_generators(&mut self) {
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
    pub(in crate::app::board_agent) fn model_pose_hash(&self, id: NodeId) -> Option<u64> {
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
    pub(in crate::app::board_agent) fn steer_generator_model(
        &mut self,
        ui: &egui::Ui,
        generator: NodeId,
        body: Rect,
    ) {
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
    pub(in crate::app::board_agent) fn fit_generator_frame(
        &mut self,
        id: NodeId,
        image: &str,
        image_request: &str,
        size: egui::Vec2,
    ) {
        // Old output A may arrive after replacement B. Only B can automatically
        // resize this frame, and its recorded caller must own that journal edit.
        if self
            .agents
            .requests
            .get(&id)
            .is_some_and(|current| current != image_request)
        {
            return;
        }
        if self
            .agents
            .cancelled_generations
            .get(&id)
            .is_some_and(|cancelled| cancelled == image_request)
        {
            return;
        }
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
            let author = if self
                .agents
                .requests
                .get(&id)
                .is_some_and(|request| request == image_request)
            {
                self.agents
                    .generation_authors
                    .get(&id)
                    .cloned()
                    .unwrap_or(slate_doc::scene::CmdAuthor::Human)
            } else {
                slate_doc::scene::CmdAuthor::Human
            };
            self.patch_nodes_as(&[id], author, |node| node.rect.h = height);
        }
    }
}
