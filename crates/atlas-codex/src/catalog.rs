use super::*;
impl Client {
    pub fn models(executable: &Path, cwd: &Path) -> Result<Vec<atlas_agent::AgentModel>, String> {
        let mut client = Self::connect(executable, cwd)?;
        client.model_catalog()
    }

    /// Read the installed model catalog within a worker-owned time budget.
    pub fn models_bounded(
        executable: &Path,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<Vec<atlas_agent::AgentModel>, String> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("Invalid Codex time budget")?;
        let mut client = Self::connect_before(executable, cwd, Some(deadline))?;
        client.model_catalog()
    }

    pub(super) fn model_catalog(&mut self) -> Result<Vec<atlas_agent::AgentModel>, String> {
        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let result = self.rpc(
                "model/list",
                json!({"limit":100,"includeHidden":false,"cursor":cursor}),
            )?;
            for model in result["data"].as_array().into_iter().flatten() {
                if let Some(model) = catalog_model(model) {
                    models.push(model);
                }
            }
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                return Ok(models);
            }
        }
    }
}

/// Fields verified against the installed app-server Model/TurnStartParams schema.
fn catalog_model(model: &Value) -> Option<atlas_agent::AgentModel> {
    use atlas_agent::{ModelParameterDefinition, ModelParameterValue};
    let id = model["model"].as_str()?;
    let mut parameters = Vec::new();
    for (key, name, values, value_key, label_key, default_key) in [
        (
            "effort",
            "Thought level",
            "supportedReasoningEfforts",
            "reasoningEffort",
            "reasoningEffort",
            "defaultReasoningEffort",
        ),
        (
            "serviceTier",
            "Speed",
            "serviceTiers",
            "id",
            "name",
            "defaultServiceTier",
        ),
    ] {
        let choices: Vec<_> = model[values]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|choice| {
                let value = choice[value_key].as_str()?;
                Some(ModelParameterValue {
                    value: value.into(),
                    name: choice[label_key].as_str().unwrap_or(value).into(),
                })
            })
            .collect();
        if !choices.is_empty() {
            let default = model[default_key]
                .as_str()
                .filter(|value| choices.iter().any(|c| c.value == *value))
                .map(str::to_owned);
            parameters.push(ModelParameterDefinition {
                id: key.into(),
                name: name.into(),
                values: choices,
                default,
            });
        }
    }
    Some(atlas_agent::AgentModel {
        id: id.into(),
        name: model["displayName"].as_str().unwrap_or(id).into(),
        parameters,
        is_default: model["isDefault"] == true,
    })
}

impl Client {
    pub(super) fn apply_model_parameters(
        &mut self,
        request: &AgentRequest,
        params: &mut Value,
    ) -> Result<(), String> {
        if request.model_parameters.is_empty() {
            return Ok(());
        }
        let models = self.model_catalog()?;
        let model = models
            .iter()
            .find(|m| {
                request
                    .model
                    .as_deref()
                    .map_or(m.is_default, |id| m.id == id)
            })
            .ok_or("The selected Codex model is no longer available. Choose a model again.")?;
        model.validate_parameters(&request.model_parameters)?;
        for (id, value) in &request.model_parameters {
            params[id.as_str()] = json!(value);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reasoning_and_speed_choices_come_only_from_the_catalog() {
        let model = catalog_model(&json!({"model":"actual","displayName":"Actual","isDefault":true,
            "supportedReasoningEfforts":[{"reasoningEffort":"high"}],"defaultReasoningEffort":"high",
            "serviceTiers":[{"id":"priority","name":"Fast"}],"defaultServiceTier":"priority"})).unwrap();
        assert!(model.is_default);
        assert_eq!(model.parameters[0].id, "effort");
        assert_eq!(model.parameters[1].values[0].value, "priority");
        assert_eq!(model.parameters[1].default.as_deref(), Some("priority"));
        let absent =
            catalog_model(&json!({"model":"old","additionalSpeedTiers":["fast"]})).unwrap();
        assert!(
            absent.parameters.is_empty(),
            "deprecated speed metadata is not a supported turn choice"
        );
    }
}
