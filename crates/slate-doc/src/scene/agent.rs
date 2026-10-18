//! Authored agent configuration; execution and grants remain machine-local.
use super::*;

/// Scope of board context an agent portal publishes to its linked local agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentContextScope {
    #[default]
    Selection,
    Frame,
    Board,
}

/// Journaled agent-portal binding. The provider is resolved by name from user
/// settings so durable scene data never depends on a vendor protocol (Art. I.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentPortalRef {
    pub provider: String,
    pub session: String,
    #[serde(default)]
    pub context: AgentContextScope,
    /// Opaque provider conversation id. The adapter resumes supported saved
    /// conversations; the provider retains history and execution ownership.
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Opaque provider-advertised choices, captured with the model on submission.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub model_parameters: std::collections::BTreeMap<String, String>,
    /// Image output manifest, resolved by the shared SourceUri owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<SourceUri>,
    /// A child created by Unbundle begins with this immutable output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<atlas_agent::ImageOutput>,
    #[serde(default)]
    pub view: atlas_agent::PortalView,
    /// Authored presentation of linked history. Transcript bytes stay in the source.
    #[serde(default)]
    pub chat: crate::agent_chat::ChatView,
    /// When set, a connected 3D model view is restyled as its camera moves.
    #[serde(default, skip_serializing_if = "is_false")]
    pub live: bool,
    /// The authored prompt: a text block's instruction, or the style guide an
    /// image agent was submitted with. Wired prompts are read before it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instruction: String,
    /// How a generator's runs are shaped: pictures per run, aspect, seed lock.
    #[serde(default, skip_serializing_if = "ImageSettings::is_default")]
    pub image: ImageSettings,
}

/// Journaled image-agent settings (D20). A locked seed makes runs repeatable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImageSettings {
    /// Pictures per run; 0 and 1 both mean one.
    #[serde(default)]
    pub count: u8,
    #[serde(default)]
    pub aspect: atlas_agent::Aspect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl ImageSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_programs_use_provider_defaults_and_authored_choices_round_trip() {
        let mut program: AgentPortalRef =
            serde_json::from_str(r#"{"provider":"example","session":"session"}"#).unwrap();
        assert!(program.model_parameters.is_empty());
        let legacy = serde_json::to_value(&program).unwrap();
        assert!(legacy.get("model_parameters").is_none());
        program.model = Some("actual-provider-model".into());
        program
            .model_parameters
            .insert("provider-effort".into(), "deep".into());
        let encoded = serde_json::to_value(&program).unwrap();
        assert_eq!(encoded["model_parameters"]["provider-effort"], "deep");
        assert_eq!(
            serde_json::from_value::<AgentPortalRef>(encoded).unwrap(),
            program
        );
        let fork = program.clone();
        assert_eq!(fork.model_parameters, program.model_parameters);
    }
}
