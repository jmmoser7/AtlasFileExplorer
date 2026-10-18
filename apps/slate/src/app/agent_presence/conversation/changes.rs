//! The conversation mutation owner emits worker deltas, never history snapshots.
use super::*;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(crate) enum ConversationChange {
    Append(ConversationTurn),
    Span {
        start: usize,
        end: usize,
        turns: Vec<ConversationTurn>,
    },
    Status {
        index: usize,
        status: String,
    },
    Draft(String),
    Binding {
        index: usize,
        value: ProviderBinding,
    },
    Choice {
        provider: String,
        value: ModelChoice,
    },
    Artifact {
        index: usize,
        value: ConversationArtifact,
    },
    Schedule(Option<atlas_ai::schedule::AgentSchedule>),
}
#[derive(Clone, Debug, Default)]
pub(crate) struct ConversationRuntime {
    requests: HashMap<String, Vec<usize>>,
    artifacts: HashMap<(PathBuf, String), usize>,
    pub(crate) changes: Vec<ConversationChange>,
}
impl PartialEq for Conversation {
    fn eq(&self, other: &Self) -> bool {
        self.turns == other.turns
            && self.draft == other.draft
            && self.bindings == other.bindings
            && self.choices == other.choices
            && self.artifacts == other.artifacts
            && self.schedule == other.schedule
    }
}
impl Eq for Conversation {}
impl Conversation {
    /// Called only on the persistence worker after deserialization/recovery.
    pub(crate) fn rebuild_runtime(&mut self) {
        self.runtime = ConversationRuntime::default();
        for (index, turn) in self.turns.iter().enumerate() {
            self.runtime
                .requests
                .entry(turn.request.clone())
                .or_default()
                .push(index);
        }
        for (index, artifact) in self.artifacts.iter().enumerate() {
            self.runtime
                .artifacts
                .insert((artifact.dir.clone(), artifact.artifact.id.clone()), index);
        }
    }
    pub(crate) fn request_index(&self, request: &str) -> Option<usize> {
        self.runtime
            .requests
            .get(request)
            .and_then(|indices| indices.first().copied())
    }
    pub(crate) fn request_indices(&self, request: &str) -> &[usize] {
        self.runtime
            .requests
            .get(request)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub(crate) fn append_turn(&mut self, turn: ConversationTurn) {
        self.runtime
            .requests
            .entry(turn.request.clone())
            .or_default()
            .push(self.turns.len());
        self.runtime
            .changes
            .push(ConversationChange::Append(turn.clone()));
        self.turns.push(turn);
    }
    pub(crate) fn set_turn_status(&mut self, index: usize, status: &str) {
        if let Some(turn) = self.turns.get_mut(index) {
            if turn.status != status {
                turn.status = status.into();
                self.runtime.changes.push(ConversationChange::Status {
                    index,
                    status: status.into(),
                });
            }
        }
    }
    pub(crate) fn set_draft(&mut self, text: &str) {
        if self.draft != text {
            self.draft = text.into();
            self.runtime
                .changes
                .push(ConversationChange::Draft(text.into()));
        }
    }
    pub(crate) fn set_schedule(&mut self, value: Option<atlas_ai::schedule::AgentSchedule>) {
        self.schedule = value.clone();
        self.runtime
            .changes
            .push(ConversationChange::Schedule(value));
    }
    pub(crate) fn set_choice(&mut self, provider: String, value: ModelChoice) {
        self.choices.insert(provider.clone(), value.clone());
        self.runtime
            .changes
            .push(ConversationChange::Choice { provider, value });
    }
    pub(crate) fn set_binding(&mut self, index: usize, value: ProviderBinding) {
        if let Some(binding) = self.bindings.get_mut(index) {
            *binding = value.clone();
        } else {
            self.bindings.push(value.clone());
        }
        self.runtime
            .changes
            .push(ConversationChange::Binding { index, value });
    }
    pub(crate) fn set_artifact(&mut self, value: ConversationArtifact) -> bool {
        let key = (value.dir.clone(), value.artifact.id.clone());
        let index = self
            .runtime
            .artifacts
            .get(&key)
            .copied()
            .unwrap_or(self.artifacts.len());
        if self.artifacts.get(index) == Some(&value) {
            return false;
        }
        if let Some(old) = self.artifacts.get_mut(index) {
            *old = value.clone();
        } else {
            self.artifacts.push(value.clone());
        }
        self.runtime.artifacts.insert(key, index);
        self.runtime
            .changes
            .push(ConversationChange::Artifact { index, value });
        true
    }
    pub(crate) fn replace_turn_span(
        &mut self,
        start: usize,
        end: usize,
        turns: Vec<ConversationTurn>,
    ) {
        // Only the changed request's suffix participates; the retained prefix is untouched.
        let keys: std::collections::HashSet<_> = self.turns[start..]
            .iter()
            .map(|turn| turn.request.clone())
            .chain(turns.iter().map(|turn| turn.request.clone()))
            .collect();
        self.turns.splice(start..end, turns.clone());
        for key in keys {
            self.runtime
                .requests
                .entry(key)
                .or_default()
                .retain(|index| *index < start);
        }
        for (offset, turn) in self.turns[start..].iter().enumerate() {
            self.runtime
                .requests
                .entry(turn.request.clone())
                .or_default()
                .push(start + offset);
        }
        self.runtime
            .changes
            .push(ConversationChange::Span { start, end, turns });
    }
    pub(crate) fn take_changes(&mut self) -> Vec<ConversationChange> {
        std::mem::take(&mut self.runtime.changes)
    }
    /// Worker-side mirror only. Rebuild no frame-side indices and emit no journal.
    pub(crate) fn apply_changes(&mut self, changes: Vec<ConversationChange>) {
        for change in changes {
            match change {
                ConversationChange::Append(turn) => self.turns.push(turn),
                ConversationChange::Span { start, end, turns } => {
                    self.turns.splice(start..end, turns);
                }
                ConversationChange::Status { index, status } => {
                    if let Some(turn) = self.turns.get_mut(index) {
                        turn.status = status;
                    }
                }
                ConversationChange::Draft(value) => self.draft = value,
                ConversationChange::Binding { index, value } => {
                    if let Some(old) = self.bindings.get_mut(index) {
                        *old = value;
                    } else {
                        self.bindings.push(value);
                    }
                }
                ConversationChange::Choice { provider, value } => {
                    self.choices.insert(provider, value);
                }
                ConversationChange::Artifact { index, value } => {
                    if let Some(old) = self.artifacts.get_mut(index) {
                        *old = value;
                    } else {
                        self.artifacts.push(value);
                    }
                }
                ConversationChange::Schedule(value) => self.schedule = value,
            }
        }
    }
}
