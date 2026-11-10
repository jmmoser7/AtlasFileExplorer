//! Provider-advertised choices. IDs and values are opaque to shared chrome.
use crate::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentModel {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ModelParameterDefinition>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_default: bool,
    /// Verified model and transport features. Missing legacy metadata makes no
    /// claim about a new task; adapters may supply only features they expose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ModelCapabilities>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTask {
    Text,
    Image,
    Artifact,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelCapabilities {
    pub text: bool,
    pub vision: bool,
    pub image_generation: bool,
    pub artifact: bool,
    pub model_tools: bool,
}

impl ModelCapabilities {
    pub const fn text_only() -> Self {
        Self {
            text: true,
            vision: false,
            image_generation: false,
            artifact: false,
            model_tools: false,
        }
    }

    pub fn supports(self, task: OutputTask, requires_vision: bool) -> bool {
        (!requires_vision || self.vision)
            && match task {
                OutputTask::Text => self.text,
                OutputTask::Image => self.image_generation,
                OutputTask::Artifact => self.artifact,
                OutputTask::Model => self.model_tools,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelParameterDefinition {
    pub id: String,
    pub name: String,
    pub values: Vec<ModelParameterValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelParameterValue {
    pub value: String,
    pub name: String,
}

impl AgentModel {
    /// Reject stale or fabricated parameters before a paid provider turn.
    pub fn validate_parameters(&self, selected: &BTreeMap<String, String>) -> Result<(), String> {
        for (id, value) in selected {
            let definition = self
                .parameters
                .iter()
                .find(|p| p.id == *id)
                .ok_or_else(|| {
                    format!(
                        "{} does not offer the model parameter '{id}'. Choose the model again.",
                        self.name
                    )
                })?;
            if !definition
                .values
                .iter()
                .any(|choice| choice.value == *value)
            {
                return Err(format!(
                    "{} no longer offers '{}' for {}. Choose it again.",
                    self.name, value, definition.name
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vision_input_never_implies_image_generation_or_native_tools() {
        let text = ModelCapabilities::text_only();
        assert!(text.supports(OutputTask::Text, false));
        assert!(!text.supports(OutputTask::Text, true));
        for task in [OutputTask::Image, OutputTask::Artifact, OutputTask::Model] {
            assert!(!text.supports(task, false));
        }
        let vision = ModelCapabilities {
            vision: true,
            ..text
        };
        assert!(vision.supports(OutputTask::Text, true));
        assert!(!vision.supports(OutputTask::Image, false));
        assert!(!vision.supports(OutputTask::Model, false));
        let legacy: AgentModel = serde_json::from_str(r#"{"id":"old","name":"Old"}"#).unwrap();
        assert!(legacy.capabilities.is_none());
    }
    #[test]
    fn only_advertised_choices_are_accepted() {
        let model: AgentModel = serde_json::from_value(serde_json::json!({
            "id":"example","name":"Example","parameters":[{"id":"vendor-effort","name":"Thought level","values":[{"value":"deep","name":"Deep"}]}]
        })).unwrap();
        assert!(model
            .validate_parameters(&BTreeMap::from([("vendor-effort".into(), "deep".into())]))
            .is_ok());
        assert!(model
            .validate_parameters(&BTreeMap::from([(
                "vendor-effort".into(),
                "made-up".into()
            )]))
            .is_err());
        assert!(model
            .validate_parameters(&BTreeMap::from([("serviceTier".into(), "fast".into())]))
            .is_err());
        let legacy: AgentModel = serde_json::from_str(r#"{"id":"old","name":"Old"}"#).unwrap();
        assert!(legacy.parameters.is_empty());
    }
}
