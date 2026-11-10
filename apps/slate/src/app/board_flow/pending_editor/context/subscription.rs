//! Subscription eligibility is shared by pickers and Send admission.
use super::*;
impl SlateApp {
    pub(crate) fn subscription_task_requirements(
        &self,
        id: NodeId,
    ) -> Result<(atlas_agent::OutputTask, bool), String> {
        let task = match self.generator_source_kind(id) {
            Some(PendingHost::Text) => atlas_agent::OutputTask::Text,
            Some(PendingHost::Model) => atlas_agent::OutputTask::Model,
            Some(PendingHost::Artifact) => atlas_agent::OutputTask::Artifact,
            _ => {
                return Err("This subscription transport does not support this output kind.".into())
            }
        };
        let inputs = self.agent_input_snapshot(id)?;
        // Model task capture intentionally excludes ambient Board context.
        let items = inputs.wired.iter().chain(
            inputs
                .context
                .iter()
                .filter(|_| task == atlas_agent::OutputTask::Text),
        );
        let vision = items
            .into_iter()
            .any(|item| !item.images.is_empty() || item.depth.is_some());
        Ok((task, vision))
    }
    pub(crate) fn subscription_provider_inputs_available(
        &self,
        id: NodeId,
        provider: &str,
    ) -> bool {
        let models = self
            .agents
            .catalog(atlas_ai::agent::model_catalog(provider));
        let Ok((task, vision)) = self.subscription_task_requirements(id) else {
            return false;
        };
        // Discovery can still load a catalog. No unknown model can pass Send.
        models.is_empty()
            || models.iter().any(|model| {
                model
                    .capabilities
                    .is_some_and(|caps| caps.supports(task, vision))
            })
    }
    pub(crate) fn subscription_default_model(&self, id: NodeId, provider: &str) -> Option<String> {
        let catalog = self
            .agents
            .catalog(atlas_ai::agent::model_catalog(provider));
        let (task, vision) = self.subscription_task_requirements(id).ok()?;
        let compatible = |model: &&atlas_agent::AgentModel| {
            model
                .capabilities
                .is_some_and(|caps| caps.supports(task, vision))
        };
        catalog
            .iter()
            .filter(compatible)
            .find(|model| model.is_default)
            .or_else(|| catalog.iter().filter(compatible).next())
            .map(|model| model.id.clone())
    }
    pub(crate) fn validate_subscription_source_choice(&self, id: NodeId) -> Result<(), String> {
        let Some(agent) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return Ok(());
        };
        if atlas_ai::runtime::subscription_provider(&agent.provider).is_none() {
            return Ok(());
        }
        let (task, vision) = self.subscription_task_requirements(id)?;
        let model = self
            .agents
            .catalog(atlas_ai::agent::model_catalog(&agent.provider))
            .iter()
            .find(|model| {
                agent
                    .model
                    .as_ref()
                    .map_or(model.is_default, |id| &model.id == id)
            })
            .ok_or_else(|| {
                self.agents
                    .catalog_error(&agent.provider)
                    .unwrap_or("Refresh this provider's model options before sending.")
                    .to_owned()
            })?;
        if !model
            .capabilities
            .is_some_and(|caps| caps.supports(task, vision))
        {
            return Err("This model and transport do not advertise the requested output and visual inputs. Choose a compatible model.".into());
        }
        model.validate_parameters(&agent.model_parameters)
    }
}
