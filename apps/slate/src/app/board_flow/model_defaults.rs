//! One persisted model default per generator kind.
use super::*;

impl SlateApp {
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
                SpawnKind::Artifact => atlas_ai::runtime::linear_provider(provider),
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
