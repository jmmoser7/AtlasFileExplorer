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
    /// Shared generator choices. Absent values preserve older workbook behavior.
    #[serde(default, skip_serializing_if = "GeneratorSettings::is_default")]
    pub generator: GeneratorSettings,
}

impl AgentPortalRef {
    /// Authored graph settings before a program is configured. This reference
    /// carries no executable provider, session, transcript or runtime grant.
    pub fn unbound(view: atlas_agent::PortalView) -> Self {
        Self {
            provider: String::new(),
            session: String::new(),
            context: Default::default(),
            channel: None,
            model: None,
            model_parameters: Default::default(),
            bundle: None,
            seed: None,
            view,
            chat: Default::default(),
            live: false,
            instruction: String::new(),
            image: Default::default(),
            generator: Default::default(),
        }
    }
}

/// Provider-neutral, journaled generator input and history choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphInputKind {
    Image,
    Text,
    Style,
}

/// Provider-neutral, journaled generator input and history choices.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratorSettings {
    /// A plain authored input source, not a generator or a configured program.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_source_kind: Option<GraphInputKind>,
    /// Last explicit inherited-history source, retained after a terminal cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_source: Option<NodeId>,
    /// Prefix actually replayed into the current public image transcript.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherited_turn_count: Option<u32>,
    /// Agent-panel Text responses review the original document, unlike legacy
    /// explicit output nodes whose body is the generated text.
    #[serde(skip_serializing_if = "is_false")]
    pub text_response_only: bool,
    /// Provenance of a Text output placeholder created by the output owner.
    /// Ordinary linked/authored Text never acquires output ownership by being empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_text_output: Option<SourceUri>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text_context: Vec<atlas_agent::AgentTurn>,
    /// Explicit Preserve OFF boundary; pending outcomes from older epochs never
    /// repopulate the retained context when Preserve is enabled again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_context_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text_reviews: Vec<crate::text_answers::TextReview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shown_review: Option<usize>,
    /// Immutable native model outputs; independent from provider transcript memory.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub model_outputs: Vec<ModelOutputRef>,
    /// Removed presentation entries. Immutable outputs, sources and memory remain linked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden_outputs: Vec<HiddenOutputRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shown_model: Option<String>,
    /// A terminal cut archives prior model context independently of outputs.
    #[serde(skip_serializing_if = "is_false")]
    pub history_cut: bool,
    /// Existing transcript manifests retained for inspection after a context cut.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub archived_history: Vec<SourceUri>,
    /// Selected text output is presentation state, independent of active memory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shown_text: Option<crate::text_answers::TextOutputRef>,
    /// None retains the legacy image count; adapters clamp authored counts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u8>,
    /// None retains the pre-existing checkpoint behavior on old documents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserve_history: Option<bool>,
    /// Existing InputSlot IDs; no geometry or wire endpoint identity is stored.
    #[serde(skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub grafted_slots: std::collections::BTreeSet<String>,
}

/// Actual immutable output identity, independent of the host's current agent context.
/// Image requests survive seed/unbundle/history cuts; model tickets and artifact
/// family/revision IDs are native producer identities, never path aliases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HiddenOutputRef {
    Image {
        request: String,
        id: String,
    },
    Model {
        ticket: String,
    },
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        review_id: Option<String>,
        response: crate::text_answers::TextAnswerRef,
    },
    Artifact {
        family: String,
        revision: String,
    },
}

impl HiddenOutputRef {
    /// Legacy images with no immutable request origin cannot be removed safely.
    pub fn image(output: &atlas_agent::ImageOutput) -> Option<Self> {
        (!output.request.is_empty() && !output.id.is_empty()).then(|| Self::Image {
            request: output.request.clone(),
            id: output.id.clone(),
        })
    }
}

impl GeneratorSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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
