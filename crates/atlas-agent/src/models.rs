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
