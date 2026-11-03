//! Unsaved configuration for the original source, before its first explicit Send.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingHost {
    Image,
    Text,
    Model,
    Artifact,
    Chat,
}

impl SlateApp {
    /// Cached scene facts only; request validation repeats the capability check
    /// against the captured Media snapshot before executing an adapter.
    pub(crate) fn custom_aspect_available(&self, id: NodeId, provider: &str) -> bool {
        let source_transform = self.image_item_path(id).is_some()
            || self.doc().scene.nodes.iter().any(|node| {
                let NodeKind::Connector(connector) = &node.kind else {
                    return false;
                };
                if let Some(binding) = &connector.binding {
                    slate_doc::agent_inputs::endpoint_node(binding.target_end(connector))
                        == Some(id)
                        && binding.kind == slate_doc::agent_inputs::InputKind::Images
                        && binding.slot.as_deref() != Some("style")
                } else {
                    slate_doc::agent_inputs::endpoint_node(&connector.a) == Some(id)
                        || slate_doc::agent_inputs::endpoint_node(&connector.b) == Some(id)
                }
            });
        atlas_ai::runtime::custom_image_aspect_supported(provider, source_transform)
    }
    pub(crate) fn pending_generator_prompt(&self, id: NodeId) -> Option<&str> {
        self.pending_generator_host(id)?;
        self.agents.flow.drafts.get(&id).map(|d| d.prompt.as_str())
    }
    pub(crate) fn pending_generator_preserve(&self, id: NodeId) -> Option<bool> {
        self.pending_generator_host(id)?;
        Some(
            self.agents
                .flow
                .drafts
                .get(&id)
                .map(|d| d.preserve_history)
                .unwrap_or_else(|| {
                    self.doc()
                        .scene
                        .node(id)
                        .and_then(slate_doc::agent_chat::agent)
                        .and_then(|a| a.generator.preserve_history)
                        .unwrap_or(false)
                }),
        )
    }
    pub(crate) fn set_pending_generator_settings(
        &mut self,
        id: NodeId,
        count: Option<usize>,
        preserve: Option<bool>,
    ) -> bool {
        if self.pending_generator_host(id).is_none() {
            return false;
        }
        let draft = self
            .agents
            .flow
            .drafts
            .entry(id)
            .or_insert_with(AgentDraft::new);
        if let Some(count) = count {
            draft.count = count.clamp(1, 8) as u8;
        }
        if let Some(preserve) = preserve {
            draft.preserve_history = preserve;
        }
        true
    }
    pub(crate) fn pending_generator_host(&self, id: NodeId) -> Option<PendingHost> {
        let node = self.doc().scene.node(id)?;
        if slate_doc::agent_chat::agent(node).is_some_and(|a| !a.provider.is_empty()) {
            return None;
        }
        self.generator_source_kind(id)
    }

    pub(crate) fn generator_source_kind(&self, id: NodeId) -> Option<PendingHost> {
        let node = self.doc().scene.node(id)?;
        if self.model_node_info(id).is_some() {
            return Some(PendingHost::Model);
        }
        match &node.kind {
            NodeKind::Portal(portal) if portal.kind == slate_doc::scene::PortalKind::Agent => {
                Some(PendingHost::Chat)
            }
            NodeKind::Text(_) => Some(PendingHost::Text),
            NodeKind::Image(image) => {
                match self
                    .doc()
                    .item(image.item)
                    .map(|item| slate_doc::media_kind(&item.path))
                {
                    Some(slate_doc::MediaKind::Text) => Some(PendingHost::Text),
                    Some(slate_doc::MediaKind::Image) => Some(PendingHost::Image),
                    None if image.item == slate_doc::ItemId::NONE && image.agent.is_none() => {
                        Some(PendingHost::Image)
                    }
                    _ => image.agent.as_ref().and_then(|a| match a.view {
                        PortalView::Images => Some(PendingHost::Image),
                        PortalView::Text => Some(PendingHost::Text),
                        _ => None,
                    }),
                }
            }
            NodeKind::Portal(portal) if portal.kind == slate_doc::scene::PortalKind::Web => {
                Some(PendingHost::Artifact)
            }
            _ => None,
        }
    }

    /// Cached registry/capability data only: discovery and credentials stay on workers.
    pub(crate) fn pending_provider_options(
        &self,
        id: NodeId,
    ) -> Vec<atlas_ai::agent::AgentProvider> {
        let Some(kind) = self.generator_source_kind(id) else {
            return Vec::new();
        };
        let has_codex = self
            .registered_agent_programs()
            .iter()
            .any(|p| p.id == "codex");
        let ids: Vec<&str> = match kind {
            PendingHost::Chat => self
                .registered_agent_programs()
                .iter()
                .filter(|p| p.view == PortalView::Chat)
                .map(|p| p.id.as_str())
                .collect(),
            PendingHost::Model => self
                .registered_agent_programs()
                .iter()
                .filter(|p| Self::native_model_harness(&p.id))
                .map(|p| p.id.as_str())
                .collect(),
            PendingHost::Artifact => self
                .registered_agent_programs()
                .iter()
                .filter(|p| atlas_ai::runtime::linear_provider(&p.id))
                .map(|p| p.id.as_str())
                .collect(),
            PendingHost::Image => [
                Some("comfy"),
                has_codex.then_some("codex-image"),
                Some("openai-image"),
            ]
            .into_iter()
            .flatten()
            .collect(),
            PendingHost::Text => [
                Some("ollama"),
                has_codex.then_some("codex-text"),
                Some("openai-text"),
            ]
            .into_iter()
            .flatten()
            .collect(),
        };
        ids.into_iter()
            .map(atlas_ai::agent::provider_by_id)
            .collect()
    }

    /// Initialize only the existing ephemeral draft, never a document binding.
    pub(crate) fn prepare_pending_chat_choice(&mut self, id: NodeId) {
        if self.pending_generator_host(id) != Some(PendingHost::Chat) {
            return;
        }
        let providers = self.pending_provider_options(id);
        let current = self
            .agents
            .flow
            .drafts
            .get(&id)
            .and_then(|d| d.choice.clone());
        let provider = current
            .as_ref()
            .map(|(p, _)| p.as_str())
            .filter(|p| providers.iter().any(|entry| entry.id == *p))
            .or_else(|| providers.first().map(|p| p.id.as_str()));
        let Some(provider) = provider.map(str::to_owned) else {
            if let Some(draft) = self.agents.flow.drafts.get_mut(&id) {
                draft.choice = None;
                draft.model_parameters.clear();
            }
            return;
        };
        let models = self.source_model_options(id, &provider);
        let selected = current
            .as_ref()
            .filter(|(p, m)| p == &provider && models.iter().any(|(_, choice, _)| choice == m));
        let model = selected
            .map(|(_, m)| m.clone())
            .or_else(|| {
                let catalog = self
                    .agents
                    .catalog(atlas_ai::agent::model_catalog(&provider));
                catalog
                    .iter()
                    .find(|m| m.is_default)
                    .or_else(|| catalog.first())
                    .map(|m| m.id.clone())
            })
            .unwrap_or_default();
        let draft = self
            .agents
            .flow
            .drafts
            .entry(id)
            .or_insert_with(AgentDraft::new);
        if draft.choice.as_ref() != Some(&(provider.clone(), model.clone())) {
            draft.choice = Some((provider, model));
            draft.model_parameters.clear();
        }
    }

    pub(crate) fn pending_generator_choice(
        &self,
        id: NodeId,
    ) -> Option<(&str, &str, &BTreeMap<String, String>)> {
        self.pending_generator_host(id)?;
        let draft = self.agents.flow.drafts.get(&id)?;
        let (provider, model) = draft.choice.as_ref()?;
        Some((provider, model, &draft.model_parameters))
    }

    pub(crate) fn set_pending_generator_provider(&mut self, id: NodeId, provider: &str) -> bool {
        if self.tab().read_only
            || self.doc().scene.node(id).is_none_or(|n| n.locked)
            || !self
                .pending_provider_options(id)
                .iter()
                .any(|p| p.id == provider)
        {
            return false;
        }
        let custom_supported = self.custom_aspect_available(id, provider);
        let draft = self
            .agents
            .flow
            .drafts
            .entry(id)
            .or_insert_with(AgentDraft::new);
        draft.choice = Some((provider.into(), String::new()));
        draft.model_parameters.clear();
        if !atlas_ai::runtime::image_seed_supported(provider) {
            draft.seed = None;
        }
        if draft.aspect == atlas_ai::agent::Aspect::ALL.len() && !custom_supported {
            draft.aspect = 0;
        }
        true
    }

    pub(crate) fn set_pending_generator_model(&mut self, id: NodeId, model: &str) -> bool {
        if self.tab().read_only || self.doc().scene.node(id).is_none_or(|n| n.locked) {
            return false;
        }
        let Some((provider, _, _)) = self.pending_generator_choice(id) else {
            return false;
        };
        let provider = provider.to_owned();
        if !self
            .source_model_options(id, &provider)
            .iter()
            .any(|(_, id, _)| id == model)
        {
            return false;
        }
        let Some(draft) = self.agents.flow.drafts.get_mut(&id) else {
            return false;
        };
        if let Some((_, selected)) = &mut draft.choice {
            *selected = model.into();
        }
        draft.model_parameters.clear();
        true
    }

    pub(crate) fn clear_pending_generator_prompt(&mut self, id: NodeId) {
        if let Some(draft) = self.agents.flow.drafts.get_mut(&id) {
            draft.prompt.clear();
        }
    }
    pub(crate) fn close_pending_generator_menus(&mut self, id: NodeId) {
        if let Some(draft) = self.agents.flow.drafts.get_mut(&id) {
            draft.model_open = false;
            draft.aspect_open = false;
        }
    }

    pub(crate) fn sync_pending_generator_preserve(&mut self, id: NodeId, preserve: bool) {
        if let Some(draft) = self.agents.flow.drafts.get_mut(&id) {
            draft.preserve_history = preserve;
        }
    }

    pub(crate) fn set_pending_generator_parameters(
        &mut self,
        id: NodeId,
        selected: BTreeMap<String, String>,
    ) -> bool {
        if self.pending_generator_host(id).is_none() {
            return false;
        }
        self.agents
            .flow
            .drafts
            .entry(id)
            .or_insert_with(AgentDraft::new)
            .model_parameters = selected;
        true
    }

    /// Model labels contain identity only. Credential state has its own action.
    pub(crate) fn source_model_options(
        &mut self,
        id: NodeId,
        provider: &str,
    ) -> Vec<(String, String, String)> {
        let Some(kind) = self.generator_source_kind(id) else {
            return Vec::new();
        };
        if provider.is_empty() && kind == PendingHost::Chat {
            return Vec::new();
        }
        let catalog = atlas_ai::agent::model_catalog(provider);
        self.agents.want_catalog(catalog);
        let models = self.agents.catalog(catalog);
        if matches!(kind, PendingHost::Model | PendingHost::Chat) {
            // Existing Codex dynamic-tool harness contract; never fixture IDs or an image fallback.
            return models
                .iter()
                .map(|m| (provider.into(), m.id.clone(), m.name.clone()))
                .collect();
        }
        let mut out = vec![(provider.into(), String::new(), "Automatic".into())];
        out.extend(
            models
                .iter()
                .map(|m| (provider.into(), m.id.clone(), m.name.clone())),
        );
        if models.is_empty() && provider == "openai-text" {
            out.push((
                provider.into(),
                atlas_ai::runtime::OPENAI_TEXT_MODEL.into(),
                atlas_ai::runtime::OPENAI_TEXT_MODEL.into(),
            ));
        }
        if models.is_empty() && provider == "openai-image" {
            out.extend(
                atlas_ai::runtime::OPENAI_IMAGE_MODELS
                    .iter()
                    .map(|(id, name)| (provider.into(), (*id).into(), (*name).into())),
            );
        }
        out
    }
}
