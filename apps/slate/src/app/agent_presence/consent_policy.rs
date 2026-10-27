//! Durable human consent preferences. Live board grants remain session-local.
use super::StoredAgent;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConsentScopeKind {
    Workbook,
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConsentScope {
    pub(crate) kind: ConsentScopeKind,
    pub(crate) canonical_path: PathBuf,
}

impl ConsentScope {
    pub(crate) fn workbook_or_workspace(workbook: Option<&Path>, workspace: &Path) -> Option<Self> {
        if let Some(workbook) = workbook {
            return Some(Self {
                kind: ConsentScopeKind::Workbook,
                canonical_path: workbook.canonicalize().ok()?,
            });
        }
        workspace.canonicalize().ok().map(|canonical_path| Self {
            kind: ConsentScopeKind::Workspace,
            canonical_path,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConsentCapabilities {
    /// Permission for independent collaborators to commit witnessed board edits.
    pub(crate) mutation_autonomy: bool,
    /// Permission to read explicitly linked board-file inputs.
    pub(crate) board_file_inputs: bool,
    /// Permission to read conversation endpoints created by this collaborator.
    pub(crate) own_chats: bool,
    /// Provider-controlled Codex command and filesystem access for this identity.
    pub(crate) provider_full_access: bool,
}

/// Explicit, durable consent. This record is never accepted as an active grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentConsentPolicy {
    pub(crate) identity_id: String,
    pub(crate) identity_name: String,
    pub(crate) provider: String,
    pub(crate) scope: ConsentScope,
    pub(crate) capabilities: ConsentCapabilities,
}

impl AgentConsentPolicy {
    pub(crate) fn matches(&self, profile: &StoredAgent, scope: &ConsentScope) -> bool {
        self.identity_id == profile.id
            && self.identity_name == profile.name
            && self.provider == profile.provider
            && &self.scope == scope
            && !profile.retired
    }

    pub(crate) fn new(profile: &StoredAgent, scope: ConsentScope, mutation_autonomy: bool) -> Self {
        Self {
            identity_id: profile.id.clone(),
            identity_name: profile.name.clone(),
            provider: profile.provider.clone(),
            scope,
            capabilities: ConsentCapabilities {
                mutation_autonomy,
                ..ConsentCapabilities::default()
            },
        }
    }
}

impl super::SlateApp {
    pub(crate) fn presence_full_access_for(&self, who: &str) -> bool {
        if self.is_chat_train_identity(who) {
            return self.agent_full_access(who);
        }
        self.presence
            .agents
            .iter()
            .find(|a| {
                a.profile.id == who
                    && a.parent.is_none()
                    && a.profile.provider == "codex"
                    && a.departure.is_none()
            })
            .is_some_and(|a| self.agent_session_access_granted(&a.binding.session_id))
    }

    pub(super) fn presence_consent_scope(&self) -> Result<ConsentScope, String> {
        let workspace = self
            .ai
            .config
            .valid_workspace()
            .ok_or("The AI workspace is unavailable")?;
        ConsentScope::workbook_or_workspace(self.tab().path.as_deref(), workspace)
            .ok_or_else(|| "The workbook and AI workspace do not have a canonical path".into())
    }

    pub(super) fn persist_consent_preference(
        &mut self,
        who: &str,
        preference: ConsentPreference,
        enabled: bool,
    ) -> Result<(), String> {
        let profile = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who && a.parent.is_none())
            .map(|a| a.profile.clone())
            .ok_or("Only a primary collaborator has persistent consent preferences")?;
        let scope = self.presence_consent_scope()?;
        let policy = self
            .settings
            .agent_consent_policies
            .iter_mut()
            .find(|p| p.matches(&profile, &scope));
        if let Some(policy) = policy {
            preference.set(&mut policy.capabilities, enabled);
        } else {
            let mut policy = AgentConsentPolicy::new(&profile, scope, false);
            preference.set(&mut policy.capabilities, enabled);
            self.settings.agent_consent_policies.push(policy);
        }
        self.settings.save();
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ConsentPreference {
    MutationAutonomy,
    BoardFileInputs,
    OwnChats,
    ProviderFullAccess,
}

impl ConsentPreference {
    fn set(self, capabilities: &mut ConsentCapabilities, enabled: bool) {
        match self {
            Self::MutationAutonomy => capabilities.mutation_autonomy = enabled,
            Self::BoardFileInputs => capabilities.board_file_inputs = enabled,
            Self::OwnChats => capabilities.own_chats = enabled,
            Self::ProviderFullAccess => capabilities.provider_full_access = enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> StoredAgent {
        StoredAgent {
            id: "agent-1".into(),
            name: "Mira".into(),
            hat: "beanie".into(),
            provider: "codex".into(),
            retired: false,
        }
    }

    fn scope(kind: ConsentScopeKind, path: &str) -> ConsentScope {
        ConsentScope {
            kind,
            canonical_path: PathBuf::from(path),
        }
    }

    #[test]
    fn consent_rebind_requires_exact_identity_provider_and_scope() {
        let profile = profile();
        let saved_scope = scope(ConsentScopeKind::Workbook, r"C:\books\work.slate");
        let policy = AgentConsentPolicy::new(&profile, saved_scope.clone(), true);
        assert!(policy.matches(&profile, &saved_scope));

        let mut renamed = profile.clone();
        renamed.name.push_str(" copy");
        assert!(!policy.matches(&renamed, &saved_scope));
        let mut another_provider = profile.clone();
        another_provider.provider = "cursor".into();
        assert!(!policy.matches(&another_provider, &saved_scope));
        assert!(!policy.matches(
            &profile,
            &scope(ConsentScopeKind::Workbook, r"C:\books\other.slate")
        ));
        assert!(!policy.matches(&profile, &scope(ConsentScopeKind::Workspace, r"C:\books")));
        let mut retired = profile;
        retired.retired = true;
        assert!(!policy.matches(&retired, &saved_scope));
    }

    #[test]
    fn persisted_consent_round_trips_without_session_authority() {
        let profile = profile();
        let mut policy = AgentConsentPolicy::new(
            &profile,
            scope(ConsentScopeKind::Workspace, r"C:\workspace"),
            true,
        );
        policy.capabilities.board_file_inputs = true;
        policy.capabilities.own_chats = true;
        policy.capabilities.provider_full_access = true;
        let encoded = serde_json::to_string(&policy).unwrap();
        let decoded: AgentConsentPolicy = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, policy);
        assert!(encoded.contains("mutation_autonomy"));
        assert!(encoded.contains("board_file_inputs"));
        assert!(encoded.contains("own_chats"));
        assert!(encoded.contains("provider_full_access"));
        assert!(!encoded.contains("grant_id"));
        assert!(!encoded.contains("session_id"));
        assert!(!encoded.contains("secret"));
    }
}
