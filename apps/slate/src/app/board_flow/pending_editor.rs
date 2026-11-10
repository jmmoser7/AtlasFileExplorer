//! The first explicit Agent Send adapts the original source into the common generator.
use super::*;
mod context;
pub(crate) use context::PendingHost;
#[cfg(test)]
mod tests;
impl SlateApp {
    pub(super) fn pending_editor_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Vec<Rect> {
        let Some(kind) = self.pending_generator_host(id) else {
            return Vec::new();
        };
        self.prepare_pending_chat_choice(id);
        let providers = self.pending_provider_options(id);
        let mut draft = self
            .agents
            .flow
            .drafts
            .get(&id)
            .cloned()
            .unwrap_or_else(|| AgentDraft::from_node(self.doc().scene.node(id).unwrap()));
        draft.kind = match kind {
            PendingHost::Image => SpawnKind::Image,
            PendingHost::Artifact | PendingHost::Model | PendingHost::Chat => SpawnKind::Artifact,
            PendingHost::Text => SpawnKind::Text,
        };
        let provider = draft.choice.as_ref().map_or("", |(p, _)| p.as_str());
        let models = self.source_model_options(id, provider);
        let index = model_index(&models, draft.choice.as_ref());
        let presentation = self.generator_presentation(id);
        draft.model_open = presentation.model_open;
        draft.aspect_open = presentation.aspect_open;
        self.agents.flow.drafts.insert(id, draft.clone());
        let labels: Vec<_> = models.iter().map(|m| m.2.as_str()).collect();
        let aspects: Vec<_> = atlas_ai::agent::Aspect::ALL
            .iter()
            .map(|a| a.label())
            .collect();
        let hint = if draft.preserve_history {
            "Message…"
        } else {
            match kind {
                PendingHost::Image => "Describe the picture. Submit makes it again.",
                PendingHost::Text
                    if self
                        .agents
                        .flow
                        .text_selections
                        .contains_key(&(self.tab().id, id)) =>
                {
                    "Ask about the selected text…"
                }
                PendingHost::Text => "Describe the text or the next change",
                PendingHost::Model => {
                    if providers.is_empty() {
                        "Set up a supported native model agent"
                    } else {
                        "Describe the model or the next change"
                    }
                }
                PendingHost::Artifact => "Describe the dashboard or the next change",
                PendingHost::Chat => "Message",
            }
        };
        let mut projected = self.project_agent_prompt(id, &draft.prompt);
        let out = self.paint_image_agent_editor(
            ui,
            rect,
            id,
            atlas_shell::selection_tools::AgentEdit {
                modes: &[],
                mode: 0,
                image: kind == PendingHost::Image,
                prompt: &mut projected,
                hint,
                models: &labels,
                model: index,
                model_open: &mut draft.model_open,
                count: draft.count,
                aspects: &aspects,
                aspect: draft.aspect,
                aspect_open: &mut draft.aspect_open,
                custom_aspect: draft.custom_aspect,
                custom_aspect_supported: self.custom_aspect_available(
                    id,
                    draft
                        .choice
                        .as_ref()
                        .map_or("", |(provider, _)| provider.as_str()),
                ),
                seed_locked: draft.seed.is_some(),
                seed_supported: atlas_ai::runtime::image_seed_supported(
                    draft
                        .choice
                        .as_ref()
                        .map_or("", |(provider, _)| provider.as_str()),
                ),
                live: draft.live,
                take_focus: std::mem::take(&mut draft.take_focus),
            },
            z,
            theme,
        );
        if out.prompt.changed {
            draft.prompt = self.authored_agent_prompt_edit(id, &projected, &draft.prompt);
            self.sync_projected_prompt_cursor(ui.ctx(), id, &projected, &draft.prompt);
        }
        // Shared picker actions update the same pending record while the prompt is borrowed.
        if let Some(updated) = self.agents.flow.drafts.get(&id) {
            draft.choice.clone_from(&updated.choice);
            draft.model_parameters.clone_from(&updated.model_parameters);
            draft.preserve_history = updated.preserve_history;
            draft.aspect = updated.aspect;
            draft.custom_aspect = updated.custom_aspect;
            draft.seed = updated.seed;
        }
        if let Some((provider, model, _)) = out.model.and_then(|i| models.get(i)) {
            draft.choice = Some((provider.clone(), model.clone()));
            draft.model_parameters.clear();
            self.remember_source_model(id, provider, model, &Default::default());
        }
        if let Some(count) = out.count {
            draft.count = count.clamp(1, 8);
        }
        if let Some(aspect) = out.aspect {
            draft.aspect = aspect;
        }
        if let Some(ratio) = out.custom_aspect {
            draft.custom_aspect = Some(ratio);
            draft.aspect = atlas_ai::agent::Aspect::ALL.len();
        }
        if let Some(lock) = out.seed_locked {
            draft.seed = lock.then(fresh_seed);
        }
        if let Some(live) = out.live {
            draft.live = live;
        }
        if out.prompt.focused {
            self.web_release_keyboard();
        }
        if out.submit {
            let previous_request = self.agents.request_of(id).map(str::to_owned);
            self.submit_pending_generator(ui.ctx(), id, &mut draft);
            if self.text_response_only(id)
                || (kind == PendingHost::Image
                    && draft.preserve_history
                    && self.agents.request_of(id) != previous_request.as_deref())
            {
                draft.prompt.clear();
            }
        }
        self.agents.flow.drafts.insert(id, draft);
        out.popups
    }

    /// Collapsed Run and composer Send share the original first-bind owner.
    pub(crate) fn submit_pending_image(&mut self, id: NodeId) -> bool {
        if self.pending_generator_host(id) != Some(PendingHost::Image) {
            return false;
        }
        self.submit_pending_media(id)
    }

    /// Explicit human Send resolves the pending modality's own saved choice.
    pub(crate) fn submit_pending_media(&mut self, id: NodeId) -> bool {
        let Some(kind @ (PendingHost::Image | PendingHost::Text)) = self.pending_generator_host(id)
        else {
            return false;
        };
        self.prepare_pending_chat_choice(id);
        let mut draft = self
            .agents
            .flow
            .drafts
            .get(&id)
            .cloned()
            .unwrap_or_else(|| AgentDraft::from_node(self.doc().scene.node(id).unwrap()));
        let previous_request = self.agents.request_of(id).map(str::to_owned);
        if let Some((_, provider, model)) = self.pending_submission_choice(id, &draft) {
            if kind == PendingHost::Image {
                let aspect = draft.image_aspect();
                self.submit_image_in_place(id, &draft, &provider, &model, aspect);
            } else if self.bind_pending_generator(id, kind, &draft, &provider, &model) {
                *self.agents.prompt_mut(id) = draft.prompt.clone();
                self.send_agent_prompt(id);
            }
        }
        if (kind == PendingHost::Text && self.text_response_only(id))
            || (draft.preserve_history && self.agents.request_of(id) != previous_request.as_deref())
        {
            draft.prompt.clear();
        }
        self.agents.flow.drafts.insert(id, draft);
        true
    }

    fn pending_submission_choice(
        &mut self,
        id: NodeId,
        draft: &AgentDraft,
    ) -> Option<(PendingHost, String, String)> {
        let kind = self.pending_generator_host(id)?;
        if self.tab().read_only
            || self.doc().scene.node(id).is_none_or(|n| n.locked)
            || (self
                .project_agent_prompt(id, &draft.prompt)
                .trim()
                .is_empty()
                && !self.has_dynamic_prompt_source(id))
        {
            return None;
        }
        let (provider, model) = draft.choice.clone()?;
        if !self
            .pending_provider_options(id)
            .iter()
            .any(|p| p.id == provider)
        {
            return None;
        }
        if self.ai.packs.credential_present(&provider) == Some(false) {
            self.dispatch_generator_provider(id, None, Some(provider));
            return None;
        }
        if atlas_ai::runtime::subscription_provider(&provider).is_some() {
            if !self
                .source_model_options(id, &provider)
                .iter()
                .any(|(_, available, _)| available == &model)
            {
                let message = self
                    .agents
                    .catalog_error(&provider)
                    .unwrap_or("Refresh this provider's model options before sending.")
                    .to_owned();
                self.toast(message);
                return None;
            }
            let selected = self
                .agents
                .catalog(&provider)
                .iter()
                .find(|m| m.id == model)?;
            if let Err(error) = selected.validate_parameters(&draft.model_parameters) {
                self.toast(error);
                return None;
            }
        }
        Some((kind, provider, model))
    }

    fn submit_pending_generator(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        draft: &mut AgentDraft,
    ) {
        let Some((kind, provider, model)) = self.pending_submission_choice(id, draft) else {
            return;
        };
        if kind == PendingHost::Image {
            let aspect = draft.image_aspect();
            self.submit_image_in_place(id, draft, &provider, &model, aspect);
            return;
        }
        if kind == PendingHost::Model
            && (!Self::scoped_model_harness(&provider)
                || !self.agents.catalog(&provider).iter().any(|m| m.id == model))
        {
            self.toast("Choose an available model from the native agent catalog.");
            return;
        }
        if !self.bind_pending_generator(id, kind, draft, &provider, &model) {
            return;
        }
        *self.agents.prompt_mut(id) = draft.prompt.clone();
        self.board_sel = [id].into_iter().collect();
        self.dispatch(
            ctx,
            atlas_commands::CommandId("portal.agent.send"),
            Some(serde_json::json!({"node":id}).to_string()),
        );
    }

    /// Explicit Chat configuration/Send attaches the prepared reference to the
    /// original portal. The temporary program binding never owns its geometry.
    pub(crate) fn bind_pending_chat(&mut self, id: NodeId) -> bool {
        if self.pending_generator_host(id) != Some(PendingHost::Chat) {
            return false;
        }
        if self.tab().read_only || self.doc().scene.node(id).is_none_or(|n| n.locked) {
            return false;
        }
        self.prepare_pending_chat_choice(id);
        let Some(draft) = self.agents.flow.drafts.get(&id).cloned() else {
            self.toast("Set up an available chat program in Settings.");
            return false;
        };
        let Some((provider, model)) = draft.choice.as_ref() else {
            self.toast("Set up an available chat program in Settings.");
            return false;
        };
        if !self
            .pending_provider_options(id)
            .iter()
            .any(|p| p.id == *provider)
            || !self
                .agents
                .catalog(atlas_ai::agent::model_catalog(provider))
                .iter()
                .any(|m| m.id == *model)
        {
            self.toast("Choose an available chat program and model in the header.");
            return false;
        }
        if let Some(selected) = self
            .agents
            .catalog(atlas_ai::agent::model_catalog(provider))
            .iter()
            .find(|m| m.id == *model)
        {
            if let Err(error) = selected.validate_parameters(&draft.model_parameters) {
                self.toast(error);
                return false;
            }
        }
        if self.ai.packs.credential_present(provider) == Some(false) {
            self.dispatch_generator_provider(id, None, Some(provider.clone()));
            return false;
        }
        self.bind_pending_generator(id, PendingHost::Chat, &draft, provider, model)
    }

    fn bind_pending_generator(
        &mut self,
        id: NodeId,
        kind: PendingHost,
        draft: &AgentDraft,
        provider: &str,
        model: &str,
    ) -> bool {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let program = atlas_ai::agent::AgentProvider {
            view: if kind == PendingHost::Text {
                PortalView::Text
            } else {
                PortalView::Chat
            },
            ..atlas_ai::agent::provider_by_id(provider)
        };
        let mut temporary = before.clone();
        temporary.kind = NodeKind::Portal(PortalNode::unbound_agent("", provider));
        bind_program(&mut temporary, &program, &self.program_binding(Some(id)));
        let Some(mut agent) = slate_doc::agent_chat::agent(&temporary).cloned() else {
            return false;
        };
        agent.instruction = if kind == PendingHost::Chat {
            String::new()
        } else {
            draft.prompt.clone()
        };
        agent.model = (!model.is_empty()).then(|| model.into());
        agent.model_parameters.clone_from(&draft.model_parameters);
        if kind == PendingHost::Chat {
            if let Some(previous) = slate_doc::agent_chat::agent(&before) {
                agent.chat = previous.chat.clone();
                if !agent.chat.train && agent.chat.detail == slate_doc::agent_chat::Detail::Summary
                {
                    agent.chat.train = true;
                    agent.chat.detail = slate_doc::agent_chat::Detail::Pair;
                }
            }
            agent.chat.linear = atlas_ai::runtime::linear_provider(provider);
            // Retain authored dimensions when the normal fit owner sees the new binding.
            agent.chat.size = Some([before.rect.w, before.rect.h]);
        } else {
            agent.chat.train = false;
        }
        if let Some(previous) = slate_doc::agent_chat::agent(&before) {
            agent.generator = previous.generator.clone();
        }
        // Explicit Send promotes an input source; merely opening Agent retains
        // its output-only graph identity and authored dimensions.
        agent.generator.input_source_kind = None;
        agent.generator.preserve_history = Some(draft.preserve_history);
        agent.generator.count = Some(draft.count);
        agent.image.count = draft.count;
        let owned_text_output = self.is_owned_text_output(id);
        agent.generator.text_response_only = kind == PendingHost::Text && !owned_text_output;
        let mut after = before.clone();
        match &mut after.kind {
            NodeKind::Text(text) => text.agent = Some(Box::new(agent)),
            NodeKind::Image(image) => image.agent = Some(Box::new(agent)),
            NodeKind::Portal(portal) if kind == PendingHost::Chat => {
                portal.agent = Some(Box::new(agent));
            }
            NodeKind::Portal(portal) if kind == PendingHost::Artifact => {
                portal.agent = Some(Box::new(agent));
                let Some(web) = portal.web.as_mut() else {
                    return false;
                };
                web.artifact = Some(Default::default());
            }
            _ => return false,
        }
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }])
    }
}
