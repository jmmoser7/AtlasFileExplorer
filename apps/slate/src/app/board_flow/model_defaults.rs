//! One persisted model default per generator kind.
use super::*;

impl SlateApp {
    pub(crate) fn default_source_model(&self, id: NodeId) -> Option<(String, String)> {
        match self.generator_source_kind(id)? {
            PendingHost::Image => self.default_flow_model(SpawnKind::Image),
            PendingHost::Text => self.default_flow_model(SpawnKind::Text),
            PendingHost::Model => self.default_native_model(),
            PendingHost::Artifact => self.default_flow_model(SpawnKind::Artifact),
            PendingHost::Chat => self
                .settings
                .agent_chat_model
                .clone()
                .or_else(|| self.default_flow_model(SpawnKind::Artifact)),
        }
    }

    pub(crate) fn default_native_model(&self) -> Option<(String, String)> {
        self.settings
            .agent_artifact_model
            .as_ref()
            .filter(|(provider, _)| Self::scoped_model_harness(provider))
            .cloned()
    }

    pub(crate) fn default_model_parameters(
        &self,
        provider: &str,
        model: &str,
    ) -> std::collections::BTreeMap<String, String> {
        let selected = self
            .settings
            .agent_model_parameters
            .get(provider)
            .and_then(|models| models.get(model))
            .cloned()
            .unwrap_or_default();
        self.compatible_model_parameters(provider, model, selected)
    }

    pub(crate) fn compatible_model_parameters(
        &self,
        provider: &str,
        model: &str,
        mut selected: std::collections::BTreeMap<String, String>,
    ) -> std::collections::BTreeMap<String, String> {
        if let Some(advertised) = self
            .agents
            .catalog(atlas_ai::agent::model_catalog(provider))
            .iter()
            .find(|m| {
                if model.is_empty() {
                    m.is_default
                } else {
                    m.id == model
                }
            })
        {
            selected.retain(|id, value| {
                advertised
                    .validate_parameters(&std::collections::BTreeMap::from([(
                        id.clone(),
                        value.clone(),
                    )]))
                    .is_ok()
            });
        } else if !self
            .agents
            .catalog(atlas_ai::agent::model_catalog(provider))
            .is_empty()
        {
            selected.clear();
        }
        selected
    }

    /// Machine-local choices never change an existing element's explicit configuration.
    pub(crate) fn remember_source_model(
        &mut self,
        id: NodeId,
        provider: &str,
        model: &str,
        parameters: &std::collections::BTreeMap<String, String>,
    ) {
        let Some(kind) = self.generator_source_kind(id) else {
            return;
        };
        let choice = Some((provider.to_owned(), model.to_owned()));
        let slot = match kind {
            PendingHost::Image => &mut self.settings.agent_image_model,
            PendingHost::Text => &mut self.settings.agent_text_model,
            PendingHost::Model | PendingHost::Artifact => &mut self.settings.agent_artifact_model,
            PendingHost::Chat => &mut self.settings.agent_chat_model,
        };
        let changed = *slot != choice;
        *slot = choice;
        let saved = self
            .settings
            .agent_model_parameters
            .entry(provider.into())
            .or_default()
            .entry(model.into())
            .or_default();
        if changed || saved != parameters {
            saved.clone_from(parameters);
            self.settings.save();
        }
    }

    pub(crate) fn default_flow_model(&self, kind: SpawnKind) -> Option<(String, String)> {
        let choice = match kind {
            SpawnKind::Image => &self.settings.agent_image_model,
            SpawnKind::Text => &self.settings.agent_text_model,
            SpawnKind::Artifact => &self.settings.agent_artifact_model,
        };
        choice
            .as_ref()
            .filter(|(provider, _)| match kind {
                SpawnKind::Image => atlas_ai::agent::local_image_engine(provider),
                SpawnKind::Text => TEXT_ENGINES.contains(&provider.as_str()),
                SpawnKind::Artifact => atlas_ai::runtime::artifact_provider(provider),
            })
            .cloned()
    }

    pub(crate) fn remember_flow_model(&mut self, kind: SpawnKind, provider: &str, model: &str) {
        let choice = Some((provider.to_string(), model.to_string()));
        let slot = match kind {
            SpawnKind::Image => &mut self.settings.agent_image_model,
            SpawnKind::Text => &mut self.settings.agent_text_model,
            SpawnKind::Artifact => &mut self.settings.agent_artifact_model,
        };
        if *slot != choice {
            *slot = choice;
            self.settings.save();
        }
    }

    pub(crate) fn default_model(&self, image: bool) -> Option<(String, String)> {
        self.default_flow_model(if image {
            SpawnKind::Image
        } else {
            SpawnKind::Text
        })
    }
    pub(crate) fn remember_model(&mut self, image: bool, provider: &str, model: &str) {
        self.remember_flow_model(
            if image {
                SpawnKind::Image
            } else {
                SpawnKind::Text
            },
            provider,
            model,
        );
    }
}
