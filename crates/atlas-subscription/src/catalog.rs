//! Provider-advertised identities and controls; no guessed model lists.
use atlas_agent::{
    AgentModel, AgentRequest, ModelCapabilities, ModelParameterDefinition, ModelParameterValue,
};
use serde_json::Value;

pub(crate) fn claude(init: &Value) -> Vec<AgentModel> {
    init["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model["value"].as_str()?.to_owned();
            // The authenticated provider's current resolved catalog is covered by
            // its documented all-current-model vision/tool contract. A missing
            // resolved identity is unknown; model-name branding is not a capability.
            let current_claude = model["resolvedModel"]
                .as_str()
                .is_some_and(|id| !id.is_empty());
            let mut parameters = Vec::new();
            if model["supportsEffort"] == true {
                let values: Vec<_> = model["supportedEffortLevels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str())
                    .map(|value| ModelParameterValue {
                        value: value.into(),
                        name: value.into(),
                    })
                    .collect();
                if !values.is_empty() {
                    parameters.push(ModelParameterDefinition {
                        id: "effort".into(),
                        name: "Thought level".into(),
                        values,
                        default: None,
                    });
                }
            }
            // supportsFastMode is not billing consent or account entitlement. Fast
            // is deliberately absent until an explicit host opt-in is implemented.
            Some(AgentModel {
                id: id.clone(),
                name: model["displayName"].as_str().unwrap_or(&id).into(),
                is_default: id == "default",
                parameters,
                capabilities: Some(ModelCapabilities {
                    text: true,
                    vision: current_claude,
                    image_generation: false,
                    artifact: current_claude,
                    model_tools: current_claude,
                }),
            })
        })
        .collect()
}

pub(crate) fn cursor(created: &Value, image_input: bool) -> Vec<AgentModel> {
    created["models"]["availableModels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model["modelId"].as_str()?.to_owned();
            let automatic = model["name"] == "Auto" && id.starts_with("default[");
            Some(AgentModel {
                id: id.clone(),
                name: model["name"].as_str().unwrap_or(&id).into(),
                parameters: vec![],
                is_default: created["models"]["currentModelId"] == id,
                capabilities: Some(ModelCapabilities {
                    text: true,
                    vision: automatic && image_input,
                    image_generation: false,
                    artifact: true,
                    model_tools: true,
                }),
            })
        })
        .collect()
}

pub(crate) fn validate(
    models: &[AgentModel],
    request: &AgentRequest,
    vision: bool,
    tools: bool,
) -> Result<AgentModel, String> {
    let model = match request.model.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => models.iter().find(|model| model.id == id),
        None => models.iter().find(|model| model.is_default),
    }
    .ok_or("The selected model is no longer advertised. Refresh and choose a model again.")?;
    model.validate_parameters(&request.model_parameters)?;
    let caps = model
        .capabilities
        .ok_or("This model has no verified task capabilities.")?;
    if !caps.text
        || (vision && !caps.vision)
        || (tools && request.artifact.is_none() && !caps.model_tools)
        || (request.artifact.is_some() && !caps.artifact)
    {
        return Err("This model and transport do not advertise the required text, image-input or native-tool capability.".into());
    }
    Ok(model.clone())
}
