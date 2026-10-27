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

impl SlateApp {
    pub(super) fn key_saved(&mut self) {
        self.agents.refresh_catalog("openai-image");
        self.agents.refresh_catalog("openai-text");
    }

    /// Model-only room for the ordinary chat action in the Robot starter.
    /// The two apron adapters share this condition and the canvas-space height.
    pub(crate) fn model_chat_starter_extra_height(&self, id: NodeId) -> f32 {
        if self.model_node_info(id).is_some() && !agent_inputs::is_flow_node(&self.doc().scene, id)
        {
            atlas_shell::selection_tools::CORNER_HEIGHT + 8.0
        } else {
            0.0
        }
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
        if self.is_artifact_node(source) {
            return self.artifact_editor_body(ui, rect, source, z, theme);
        }
        if agent_inputs::is_flow_node(&self.doc().scene, source) {
            return self.attached_editor_body(ui, rect, source, z, theme);
        }
        let extra_height = self.model_chat_starter_extra_height(source) * z;
        let editor_rect = Rect::from_min_max(
            rect.min,
            Pos2::new(rect.right(), rect.bottom() - extra_height),
        );
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
            editor_rect,
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
                seed_locked: draft.seed.is_some(),
                live: draft.live,
                take_focus: std::mem::take(&mut draft.take_focus),
            },
            z,
            theme,
        );
        if extra_height > 0.0 {
            let action = Rect::from_min_max(
                Pos2::new(rect.left() + 8.0 * z, editor_rect.bottom() + 4.0 * z),
                Pos2::new(rect.right() - 8.0 * z, rect.bottom() - 4.0 * z),
            );
            if atlas_shell::tabs::portal_action_button(
                ui,
                &theme,
                action,
                ui.id().with(("model-chat-agent", source.0)),
                AGENT_CHOICE.menu,
                z,
            ) {
                // This is an ordinary chat spawn, not a generator run or grant.
                self.agents.flow.drafts.insert(source, draft);
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.spawn"),
                    Some(
                        serde_json::json!({"source": source.0, "kind": AGENT_CHOICE.id})
                            .to_string(),
                    ),
                );
                return out.popups;
            }
        }
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
