//! Host-local authority for new, isolated native documents. Never deserialized
//! from a workbook or peer. Existing linked originals remain human-only.
use crate::{validation::*, *};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftAuthorityGrant {
    pub grant_id: String,
    pub agent_id: String,
    pub source_id: String,
    pub document_id: String,
    pub command_ids: Vec<String>,
    purpose: DraftAuthorityPurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DraftAuthorityPurpose {
    Collaborator,
    ModelTask,
}

impl Session {
    /// Call only from trusted dispatch after an explicit user autonomy grant.
    /// The current revision is checked when granting; subsequent native edits
    /// can advance it without broadening the source/document/command scope.
    pub fn grant_draft_authority(
        &mut self,
        grant_id: &str,
        agent_id: &str,
        source: &SourceScope,
        command_ids: &[String],
    ) -> Result<(), ProtocolError> {
        self.grant_draft_authority_for(
            grant_id,
            agent_id,
            source,
            command_ids,
            DraftAuthorityPurpose::Collaborator,
        )
    }

    /// Trusted host dispatch only: one explicitly submitted model-generation task.
    /// This never grants workspace or Board mutation autonomy. The host must
    /// revoke the permit when its request/lease ends; peers cannot install it.
    pub fn grant_model_task_authority(
        &mut self,
        grant_id: &str,
        agent_id: &str,
        source: &SourceScope,
        command_ids: &[String],
    ) -> Result<(), ProtocolError> {
        if command_ids.len() != 1 || command_ids[0] != "org.slate.rhino.headless.generate" {
            return Err(error(
                ErrorCode::AuthorityDenied,
                "model task admits only the closed headless generator",
            ));
        }
        self.grant_draft_authority_for(
            grant_id,
            agent_id,
            source,
            command_ids,
            DraftAuthorityPurpose::ModelTask,
        )
    }

    fn grant_draft_authority_for(
        &mut self,
        grant_id: &str,
        agent_id: &str,
        source: &SourceScope,
        command_ids: &[String],
        purpose: DraftAuthorityPurpose,
    ) -> Result<(), ProtocolError> {
        self.check_scope(Some(source))?;
        let binding = self.binding.as_ref().expect("scope checked");
        if self.welcome.version != V0_2
            || self.detached
            || binding.mode != SourceMode::IsolatedDraft
            || !valid_id(grant_id)
            || !valid_id(agent_id)
            || command_ids.is_empty()
            || command_ids.len() > 64
            || command_ids.iter().any(|id| {
                self.commands
                    .get(id)
                    .is_none_or(|c| !c.agent_allowed || c.effect != CommandEffect::DraftMutation)
            })
        {
            return Err(error(
                ErrorCode::AuthorityDenied,
                "draft grant requires v0.2, a bound isolated document and eligible closed commands",
            ));
        }
        let mut command_ids = command_ids.to_vec();
        command_ids.sort();
        command_ids.dedup();
        let grant = DraftAuthorityGrant {
            grant_id: grant_id.into(),
            agent_id: agent_id.into(),
            source_id: source.source_id.clone(),
            document_id: binding.document_id.clone(),
            command_ids,
            purpose,
        };
        if let Some(prior) = self.draft_grants.get(grant_id) {
            return if prior == &grant {
                Ok(())
            } else {
                Err(error(
                    ErrorCode::RequestIdConflict,
                    "grant identity already has a different scope",
                ))
            };
        }
        if self.draft_grants.len() >= 32 {
            return Err(error(
                ErrorCode::AuthorityDenied,
                "bounded draft authority registry is full",
            ));
        }
        self.draft_grants.insert(grant_id.into(), grant);
        Ok(())
    }

    /// Revocation blocks future admission, but does not claim native rollback.
    pub fn revoke_draft_authority(&mut self, grant_id: &str) -> bool {
        self.draft_grants.remove(grant_id).is_some()
    }

    pub(crate) fn agent_command_allowed(
        &self,
        caller: &CallerAuthority,
        command: &CommandDescriptor,
    ) -> bool {
        let CallerAuthority::Agent {
            agent_id,
            workspace_autonomy,
        } = caller
        else {
            return false;
        };
        if !command.agent_allowed {
            return false;
        }
        if command.effect == CommandEffect::Inspect {
            return true;
        }
        if self.welcome.version != V0_2 {
            return false;
        }
        if command.effect == CommandEffect::EphemeralView {
            return true;
        }
        if command.effect != CommandEffect::DraftMutation {
            return false;
        }
        let Some(binding) = &self.binding else {
            return false;
        };
        binding.mode == SourceMode::IsolatedDraft
            && self.draft_grants.values().any(|grant| {
                grant.agent_id == *agent_id
                    && grant.source_id == binding.source.source_id
                    && grant.document_id == binding.document_id
                    && grant.command_ids.contains(&command.id)
                    && match grant.purpose {
                        DraftAuthorityPurpose::Collaborator => *workspace_autonomy,
                        DraftAuthorityPurpose::ModelTask => true,
                    }
            })
    }
}
