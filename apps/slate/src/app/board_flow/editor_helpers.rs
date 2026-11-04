//! Starter editor plus shared model choice, key editing and seed helpers.
use super::*;

/// Where `choice` sits in a model list: the exact entry, else that provider's
/// default, else the first (local) entry.
pub(super) fn model_index(
    models: &[(String, String, String)],
    choice: Option<&(String, String)>,
) -> usize {
    let Some((provider, model)) = choice else {
        return 0;
    };
    models
        .iter()
        .position(|m| &m.0 == provider && &m.1 == model)
        .or_else(|| {
            models
                .iter()
                .position(|m| &m.0 == provider && m.1.is_empty())
        })
        .or_else(|| models.iter().position(|m| &m.0 == provider))
        .unwrap_or(0)
}

/// The key field shows one bullet per key character: the bullets left are
/// the characters kept, anything else was typed or pasted.
pub(super) fn absorb_key(draft: &mut AgentDraft, shown: &str) {
    let kept = shown.chars().filter(|c| *c == '•').count();
    let typed: String = shown.chars().filter(|c| *c != '•').collect();
    let key = draft.key.get_or_insert_with(String::new);
    *key = key.chars().take(kept).chain(typed.chars()).collect();
}

/// A seed to lock, from a fresh request id.
pub(super) fn fresh_seed() -> u64 {
    let id = atlas_ai::agent::request_id();
    id.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    }) >> 1
}

mod source_visibility;

impl SlateApp {
    pub(super) fn key_saved(&mut self) {
        self.agents.refresh_catalog("openai-image");
        self.agents.refresh_catalog("openai-text");
    }

    /// Chat-bound media uses the same provider/catalog owner as Chat Train.
    /// This is editor eligibility only; it never changes flow-port semantics.
    pub(crate) fn bound_chat_generator_host(&self, id: NodeId) -> bool {
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        if !slate_doc::agent_chat::agent(node).is_some_and(|agent| agent.view == PortalView::Chat) {
            return false;
        }
        match &node.kind {
            NodeKind::Text(_) => true,
            NodeKind::Image(image) => {
                self.model_node_info(id).is_some()
                    || self.doc().item(image.item).is_some_and(|item| {
                        slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Text
                    })
            }
            _ => false,
        }
    }

    pub(crate) fn has_attached_generator_editor(&self, id: NodeId) -> bool {
        agent_inputs::is_flow_node(&self.doc().scene, id) || self.bound_chat_generator_host(id)
    }

    /// Bind the original source through the existing program owner, retaining
    /// its authored content, style, geometry and selection identity.
    pub(super) fn submit_unbound_text_query(
        &mut self,
        ctx: &egui::Context,
        source: NodeId,
        draft: &AgentDraft,
        provider: &str,
        model: &str,
    ) -> bool {
        let Some(before) = self.doc().scene.node(source).cloned() else {
            return false;
        };
        if self.tab().read_only
            || before.locked
            || self.agent_is_running(source)
            || slate_doc::agent_chat::agent(&before).is_some()
            || self.text_answer_selection(ctx, source).is_none()
            || !TEXT_ENGINES.contains(&provider)
        {
            return false;
        }
        let program = atlas_ai::agent::AgentProvider {
            view: PortalView::Text,
            ..atlas_ai::agent::provider_by_id(provider)
        };
        let mut bound = before.clone();
        bound.kind = NodeKind::Portal(PortalNode::unbound_agent("", provider));
        bind_program(&mut bound, &program, &self.program_binding(Some(source)));
        let Some(mut agent) = slate_doc::agent_chat::agent(&bound).cloned() else {
            return false;
        };
        agent.instruction = draft.prompt.trim().to_owned();
        agent.model = (!model.is_empty()).then(|| model.to_owned());
        agent.generator.preserve_history = Some(false);
        let mut after = before.clone();
        match &mut after.kind {
            NodeKind::Text(text) => text.agent = Some(Box::new(agent)),
            NodeKind::Image(image) => image.agent = Some(Box::new(agent)),
            _ => return false,
        }
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return false;
        }
        self.sync_instruction(source, draft.prompt.trim());
        self.board_sel = [source].into_iter().collect();
        self.open_object_panel(super::super::board_properties::Panel::Agent);
        self.queue_text_block(source);
        true
    }

    /// The Agent squircle's editor for `source`. Returns open popup rects.
    pub(crate) fn agent_editor_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        source: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Vec<Rect> {
        if self.pending_generator_host(source).is_some() {
            return self.pending_editor_body(ui, rect, source, z, theme);
        }
        if self.is_artifact_node(source) {
            return self.artifact_editor_body(ui, rect, source, z, theme);
        }
        if self.has_attached_generator_editor(source) {
            return self.attached_editor_body(ui, rect, source, z, theme);
        }
        let mut draft = self
            .agents
            .flow
            .drafts
            .remove(&source)
            .unwrap_or_else(AgentDraft::new);
        let models = self.agent_models(draft.kind == SpawnKind::Image);
        let modes: Vec<&str> = MODALITIES.iter().map(|m| m.label).collect();
        let mode = if draft.kind == SpawnKind::Image {
            SpawnKind::Image
        } else {
            SpawnKind::Text
        }
        .index();
        let current = draft
            .choice
            .clone()
            .or_else(|| self.default_model(draft.kind == SpawnKind::Image));
        let index = model_index(&models, current.as_ref());
        let labels: Vec<&str> = models.iter().map(|m| m.2.as_str()).collect();
        let aspects: Vec<&str> = atlas_ai::agent::Aspect::ALL
            .iter()
            .map(|a| a.label())
            .collect();
        let pasting = draft.key.is_some();
        let hint = if pasting {
            "Paste your OpenAI API key, then Submit. It stays in Windows Credential Manager."
        } else if draft.kind == SpawnKind::Image {
            "Describe the picture to make, or the look to give this one"
        } else {
            "Ask for a description, a caption, or a rewrite"
        };
        let mut hidden = String::new();
        let key_masked = draft.key.as_ref().map(|k| "•".repeat(k.chars().count()));
        let prompt_text = match &key_masked {
            Some(masked) => {
                hidden.clone_from(masked);
                &mut hidden
            }
            None => &mut draft.prompt,
        };
        let out = self.paint_image_agent_editor(
            ui,
            rect,
            source,
            atlas_shell::selection_tools::AgentEdit {
                modes: &modes,
                mode,
                image: draft.kind == SpawnKind::Image,
                prompt: prompt_text,
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
                    source,
                    models.get(index).map_or("", |model| model.0.as_str()),
                ),
                seed_locked: draft.seed.is_some(),
                seed_supported: atlas_ai::runtime::image_seed_supported(
                    models.get(index).map_or("", |model| model.0.as_str()),
                ),
                live: draft.live,
                take_focus: std::mem::take(&mut draft.take_focus),
            },
            z,
            theme,
        );
        if out.prompt.focused {
            self.web_release_keyboard();
        }
        if pasting && out.prompt.changed {
            absorb_key(&mut draft, &hidden);
        }
        if let Some(mode) = out.mode {
            draft.kind = MODALITIES
                .get(mode)
                .and_then(|m| m.kind)
                .unwrap_or(SpawnKind::Image);
            draft.choice = None;
            draft.key = None;
        }
        if let Some((provider, model, _)) = out.model.and_then(|i| models.get(i)) {
            draft.choice = Some((provider.clone(), model.clone()));
            self.remember_model(draft.kind == SpawnKind::Image, provider, model);
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
        if out.submit {
            let chosen = draft
                .choice
                .clone()
                .or_else(|| models.get(index).map(|m| (m.0.clone(), m.1.clone())));
            self.submit_agent_draft(ui.ctx(), source, &mut draft, chosen);
        }
        self.agents.flow.drafts.insert(source, draft);
        out.popups
    }
}

#[cfg(test)]
#[path = "editor_helpers/tests.rs"]
mod starter_tests;

impl SlateApp {
    /// Consume a pasted key through PackSession; true even if saving fails.
    pub(super) fn save_pasted_key(&mut self, draft: &mut AgentDraft) -> bool {
        let Some(key) = draft.key.take() else {
            return false;
        };
        self.ai.packs.key_draft = key;
        match self
            .ai
            .packs
            .save_openai_key(self.ai.config.workspace_dir.clone())
        {
            Ok(()) => {
                self.key_saved();
                self.toast("OpenAI API key saved for this Windows user.");
            }
            Err(error) => {
                self.toast(error);
                draft.key = Some(String::new());
            }
        }
        true
    }

    /// The squircle on a picture or note an agent makes edits that agent:
    /// model, count, aspect, seed and Live apply at once (journaled), and
    /// Submit runs it again with the prompt shown here and docked on the card.
    pub(super) fn attached_editor_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Vec<Rect> {
        let Some((view, provider, selected_model, instruction, image_settings, live)) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|agent| {
                (
                    agent.view,
                    agent.provider.clone(),
                    agent.model.clone(),
                    agent.instruction.clone(),
                    agent.image,
                    agent.live,
                )
            })
        else {
            return Vec::new();
        };
        let image = view == PortalView::Images;
        let kind = if image {
            SpawnKind::Image
        } else {
            SpawnKind::Text
        };
        self.sync_instruction(id, &instruction);
        let mut draft = self
            .agents
            .flow
            .drafts
            .remove(&id)
            .unwrap_or_else(AgentDraft::new);
        let presentation = self.generator_presentation(id);
        draft.key = None;
        draft.model_open = presentation.model_open;
        draft.aspect_open = presentation.aspect_open;
        let chat_host = self.bound_chat_generator_host(id);
        let models = if self.generator_source_kind(id).is_some() {
            self.source_model_options(id, &provider)
        } else {
            self.agent_models(image)
        };
        let current = (provider.clone(), selected_model.clone().unwrap_or_default());
        let index = model_index(&models, Some(&current));
        let labels: Vec<&str> = models.iter().map(|m| m.2.as_str()).collect();
        let aspects: Vec<&str> = atlas_ai::agent::Aspect::ALL
            .iter()
            .map(|a| a.label())
            .collect();
        let aspect = atlas_ai::agent::Aspect::ALL
            .iter()
            .position(|a| *a == image_settings.aspect)
            .unwrap_or(atlas_ai::agent::Aspect::ALL.len());
        let pasting = draft.key.is_some();
        let hint = if pasting {
            "Paste your OpenAI API key, then Submit. It stays in Windows Credential Manager."
        } else if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|agent| agent.generator.preserve_history.unwrap_or(true))
        {
            "Message…"
        } else if image {
            "Describe the picture. Submit makes it again."
        } else {
            match self.generator_source_kind(id) {
                Some(PendingHost::Text)
                    if self
                        .agents
                        .flow
                        .text_selections
                        .contains_key(&(self.tab().id, id)) =>
                {
                    "Ask about the selected text…"
                }
                Some(PendingHost::Text) => "Describe the text or the next change",
                Some(PendingHost::Model) => "Describe the model or the next change",
                Some(PendingHost::Artifact) => "Describe the dashboard or the next change",
                _ => "Tell the model what to write. Submit writes it again.",
            }
        };
        let mut text = match &draft.key {
            Some(key) => "•".repeat(key.chars().count()),
            None => self.agents.prompt_mut(id).clone(),
        };
        let modes = [kind.modality().label];
        let out = self.paint_image_agent_editor(
            ui,
            rect,
            id,
            atlas_shell::selection_tools::AgentEdit {
                modes: &modes,
                mode: 0,
                image,
                prompt: &mut text,
                hint,
                models: &labels,
                model: index,
                model_open: &mut draft.model_open,
                count: image_settings.count.max(1),
                aspects: &aspects,
                aspect,
                aspect_open: &mut draft.aspect_open,
                custom_aspect: image_settings.aspect.custom(),
                custom_aspect_supported: self.custom_aspect_available(id, &provider),
                seed_locked: image_settings.seed.is_some(),
                seed_supported: atlas_ai::runtime::image_seed_supported(&provider),
                live: live,
                take_focus: std::mem::take(&mut draft.take_focus),
            },
            z,
            theme,
        );
        if out.prompt.focused {
            self.web_release_keyboard();
        }
        if out.prompt.changed {
            if pasting {
                absorb_key(&mut draft, &text);
            } else {
                *self.agents.prompt_mut(id) = text;
            }
        }
        if let Some((provider, model, _)) = out.model.and_then(|i| models.get(i)) {
            if chat_host {
                self.board_sel = [id].into_iter().collect();
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.selected_model"),
                    Some(model.clone()),
                );
            } else {
                self.set_flow_model(id, image, provider, model);
            }
        }
        let settings = {
            let mut s = image_settings;
            if let Some(count) = out.count {
                s.count = count.clamp(1, 8);
            }
            if let Some(i) = out.aspect {
                s.aspect = atlas_ai::agent::Aspect::ALL
                    .get(i)
                    .copied()
                    .unwrap_or_default();
            }
            if let Some([width, height]) = out.custom_aspect {
                s.aspect = atlas_ai::agent::Aspect::Custom { width, height };
            }
            if let Some(lock) = out.seed_locked {
                s.seed = lock.then(fresh_seed);
            }
            s
        };
        if settings != image_settings && !self.tab().read_only {
            self.patch_nodes(&[id], |n| {
                if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                    a.image = settings;
                }
            });
        }
        if let Some(live) = out.live {
            self.set_generator_live(id, live);
        }
        if out.submit && !self.save_pasted_key(&mut draft) {
            if self.generator_source_kind(id) == Some(PendingHost::Text) {
                self.activate_text_review_mode(id);
            }
            self.commit_instruction(id);
            self.board_sel = std::iter::once(id).collect();
            if !live {
                self.run_agent_media(id);
            }
        }
        self.agents.flow.drafts.insert(id, draft);
        out.popups
    }
}
