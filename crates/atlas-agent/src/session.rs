use crate::*;
/// Resolve the exact prefix independently of presentation mode. Never silently
/// accept a partially loaded checkpoint (including the first message).
pub fn checkpoint(
    turns: &[AgentTurn],
    through: Option<usize>,
) -> Result<&[AgentTurn], &'static str> {
    let end = through.unwrap_or(turns.len());
    turns
        .get(..end)
        .ok_or("The checkpoint history has not loaded yet.")
}

/// A background reload of the provider's copy of a conversation Slate is
/// already showing. The provider's transcript carries no Slate timestamps,
/// notes or artifacts, and can lag, so it only appends messages beyond the
/// shown ones and never shrinks or rewrites them. `None` keeps `shown` as is.
pub fn reloaded(shown: &AgentSession, fetched: AgentSession) -> Option<AgentSession> {
    let other_stream = !shown.conversation.is_empty() && shown.conversation != fetched.conversation;
    if shown.turns.is_empty() || other_stream {
        return (*shown != fetched).then_some(fetched);
    }
    let said = |t: &&AgentTurn| t.role != "system";
    let known: Vec<_> = shown.turns.iter().filter(said).collect();
    let told: Vec<_> = fetched.turns.iter().filter(said).collect();
    let continues =
        told.len() > known.len() && known.iter().zip(&told).all(|(a, b)| a.role == b.role);
    if !continues && shown.conversation == fetched.conversation {
        return None;
    }
    let mut next = shown.clone();
    next.conversation = fetched.conversation;
    if continues {
        next.turns
            .extend(told[known.len()..].iter().map(|t| (*t).clone()));
        next.updated_at = next.updated_at.max(fetched.updated_at);
    }
    Some(next)
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentSession {
    #[serde(default)]
    pub approval: Option<AgentApproval>,
    /// Provider-owned identity. Slate caches history; it does not own it.
    #[serde(default)]
    pub conversation: String,
    #[serde(default)]
    pub artifacts: Vec<AgentArtifact>,
    #[serde(default)]
    pub status: AgentStatus,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub turns: Vec<AgentTurn>,
    /// Bounded public progress, never a replayable conversation checkpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activity: Vec<AgentActivity>,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub bundle: ImageBundle,
    #[serde(default)]
    pub request: String,
    /// Provider-reported token total, when the session file carries one
    /// (`usage` as a number, or `usage.total_tokens` / input + output).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_usage"
    )]
    pub usage: Option<u64>,
}

fn de_usage<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(usage_number(&value))
}

/// A token total written by a provider: a bare number, or an object with
/// `total_tokens` or input plus output.
pub fn usage_number(value: &serde_json::Value) -> Option<u64> {
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    if let Some(n) = value.as_i64().filter(|n| *n >= 0) {
        return Some(n as u64);
    }
    if let Some(n) = value.as_f64().filter(|n| n.is_finite() && *n >= 0.0) {
        return Some(n as u64);
    }
    let obj = value.as_object()?;
    if let Some(n) = obj.get("total_tokens").and_then(usage_number) {
        return Some(n);
    }
    let input = obj
        .get("input_tokens")
        .or_else(|| obj.get("prompt_tokens"))
        .and_then(usage_number)
        .unwrap_or(0);
    let output = obj
        .get("output_tokens")
        .or_else(|| obj.get("completion_tokens"))
        .and_then(usage_number)
        .unwrap_or(0);
    (input + output > 0).then_some(input + output)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentApproval {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Read,
    Web,
    Created,
    Modified,
    Deleted,
}
impl ArtifactKind {
    pub fn is_output(self) -> bool {
        matches!(self, Self::Created | Self::Modified | Self::Deleted)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Web => "Source",
            Self::Created => "Created",
            Self::Modified => "Modified",
            Self::Deleted => "Deleted",
        }
    }
}

/// One note an agent leaves about using Slate. No file contents or secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackNote {
    #[serde(default)]
    pub id: String,
    /// What was hard, in one sentence.
    pub what: String,
    #[serde(default)]
    pub tried: String,
    /// An action the agent could not reach. Empty when the note is a suggestion.
    #[serde(default)]
    pub missing: String,
}

/// An observed tool result, never a path guessed from assistant prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentArtifact {
    pub id: String,
    /// Index of the associated assistant message in the normalized history.
    pub turn: usize,
    pub kind: ArtifactKind,
    pub source: String,
    pub title: String,
}
impl AgentSession {
    pub fn record_artifact(&mut self, artifact: AgentArtifact) {
        if artifact.source.is_empty() {
            return;
        }
        if let Some(old) = self.artifacts.iter_mut().find(|a| a.id == artifact.id) {
            *old = artifact;
        } else {
            self.artifacts.push(artifact);
        }
    }
}
