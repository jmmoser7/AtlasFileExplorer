//! Join entry points share the one explicit-workspace lifecycle implementation.
use super::*;

impl SlateApp {
    pub(in crate::app::agent_presence) fn ensure_saved_presence_joined(
        &mut self,
        who: &str,
    ) -> Result<(), String> {
        if self.presence.agents.iter().any(|agent| {
            agent.profile.id == who && agent.tab == self.tab().id && agent.departure.is_none()
        }) {
            return Ok(());
        }
        let profile = self
            .settings
            .collaborators
            .iter()
            .find(|profile| profile.id == who && !profile.retired)
            .cloned()
            .ok_or("This collaborator is retired; restore it from Advanced first")?;
        self.join_presence(profile, None)
    }
    pub(in crate::app::agent_presence) fn join_presence(
        &mut self,
        profile: StoredAgent,
        parent: Option<String>,
    ) -> Result<(), String> {
        let scope = parent
            .map(|who| {
                self.presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == who)
                    .and_then(|a| a.grant.clone().map(|g| (who, a.motion.clone(), g)))
                    .ok_or_else(|| "Parent grant is unavailable".to_owned())
            })
            .transpose()?;
        self.join_scoped_presence(profile, scope)
    }
    pub(in crate::app::agent_presence) fn join_scoped_presence(
        &mut self,
        profile: StoredAgent,
        parent: Option<(String, PresenceId, CollaboratorGrant)>,
    ) -> Result<(), String> {
        let ws = self
            .ai
            .config
            .valid_workspace()
            .ok_or("Set the AI workspace before adding a collaborator")?
            .to_path_buf();
        self.join_scoped_presence_at(profile, parent, ws)
    }
}
