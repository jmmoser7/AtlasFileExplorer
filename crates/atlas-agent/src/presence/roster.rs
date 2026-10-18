use super::*;
use std::collections::HashMap;

/// One board's ephemeral membership. Persist only `profiles()`, never this roster.
#[derive(Debug, Default)]
pub struct PresenceRoster {
    profiles: HashMap<PresenceId, AgentProfile>,
    sessions: HashMap<PresenceId, PresenceSession>,
}

impl PresenceRoster {
    pub fn profiles(&self) -> impl Iterator<Item = &AgentProfile> {
        self.profiles.values()
    }
    /// A completed temporary identity can be discarded after its visible exit.
    /// Live sessions and their replay fences are never evicted by this operation.
    pub fn forget_retired_profile(&mut self, id: &PresenceId) -> Result<(), PresenceError> {
        let profile = self.profiles.get(id).ok_or(PresenceError::UnknownProfile)?;
        if !profile.retired || self.sessions.values().any(|s| s.profile_id() == id) {
            return Err(PresenceError::AlreadyPresent);
        }
        self.profiles.remove(id);
        Ok(())
    }
    pub fn session(&self, id: &PresenceId) -> Option<&PresenceSession> {
        self.sessions.get(id)
    }
    pub fn session_mut(&mut self, id: &PresenceId) -> Option<&mut PresenceSession> {
        self.sessions.get_mut(id)
    }
    pub fn sessions(&self) -> impl Iterator<Item = &PresenceSession> {
        self.sessions.values()
    }

    pub fn register(&mut self, profile: AgentProfile) -> Result<(), PresenceError> {
        profile.validate()?;
        if self.profiles.contains_key(&profile.id) {
            return Err(PresenceError::ProfileExists);
        }
        if self.profiles.len() == PROFILE_LIMIT {
            return Err(PresenceError::ProfileLimit);
        }
        self.profiles.insert(profile.id.clone(), profile);
        Ok(())
    }

    pub fn join(
        &mut self,
        profile_id: &PresenceId,
        parent_id: Option<&PresenceId>,
        position: WorldPoint,
    ) -> Result<PresenceId, PresenceError> {
        position.validate()?;
        let profile = self
            .profiles
            .get(profile_id)
            .ok_or(PresenceError::UnknownProfile)?;
        if profile.retired {
            return Err(PresenceError::RetiredProfile);
        }
        if self.sessions.values().any(|s| s.profile_id() == profile_id) {
            return Err(PresenceError::AlreadyPresent);
        }
        if self.sessions.len() == SESSION_LIMIT {
            return Err(PresenceError::SessionLimit);
        }
        if let Some(parent) = parent_id {
            self.validate_parent(parent)?;
        }
        let session = PresenceSession::new(profile_id.clone(), parent_id.cloned(), position)?;
        let id = session.id().clone();
        self.sessions.insert(id.clone(), session);
        Ok(id)
    }

    fn validate_parent(&self, parent_id: &PresenceId) -> Result<(), PresenceError> {
        let mut parent = self
            .sessions
            .get(parent_id)
            .ok_or(PresenceError::UnknownSession)?;
        let mut depth = 1;
        while let Some(id) = parent.parent_id() {
            depth += 1;
            parent = self.sessions.get(id).ok_or(PresenceError::UnknownSession)?;
        }
        let helpers = self
            .sessions
            .values()
            .filter(|s| s.parent_id().is_some())
            .count();
        let children = self
            .sessions
            .values()
            .filter(|s| s.parent_id() == Some(parent_id))
            .count();
        if depth > HELPER_DEPTH_LIMIT || helpers == HELPER_LIMIT || children == HELPERS_PER_PARENT {
            return Err(PresenceError::HelperLimit);
        }
        Ok(())
    }

    /// Retiring a parent also cancels its temporary helper sessions. Their profile
    /// definitions survive, but all runtime jobs, cursor positions and hints drop.
    pub fn retire(
        &mut self,
        profile_id: &PresenceId,
    ) -> Result<Vec<PresenceSignal>, PresenceError> {
        let profile = self
            .profiles
            .get_mut(profile_id)
            .ok_or(PresenceError::UnknownProfile)?;
        profile.retired = true;
        let ids: Vec<_> = self
            .sessions
            .values()
            .filter(|s| s.profile_id() == profile_id)
            .map(|s| s.id().clone())
            .collect();
        let mut cancelled = Vec::new();
        for id in ids {
            self.remove_tree(&id, &mut cancelled);
        }
        Ok(cancelled)
    }

    fn remove_tree(&mut self, id: &PresenceId, cancelled: &mut Vec<PresenceSignal>) {
        let children: Vec<_> = self
            .sessions
            .values()
            .filter(|s| s.parent_id() == Some(id))
            .map(|s| s.id().clone())
            .collect();
        for child in children {
            self.remove_tree(&child, cancelled);
        }
        if let Some(mut session) = self.sessions.remove(id) {
            cancelled.extend(session.cancel_pending());
        }
    }

    pub fn leave(&mut self, id: &PresenceId) -> Result<Vec<PresenceSignal>, PresenceError> {
        if !self.sessions.contains_key(id) {
            return Err(PresenceError::UnknownSession);
        }
        let mut cancelled = Vec::new();
        self.remove_tree(id, &mut cancelled);
        Ok(cancelled)
    }

    pub fn restore(
        &mut self,
        profile_id: &PresenceId,
        spawn: WorldPoint,
    ) -> Result<PresenceId, PresenceError> {
        spawn.validate()?;
        if !self
            .profiles
            .get(profile_id)
            .ok_or(PresenceError::UnknownProfile)?
            .retired
        {
            return Err(PresenceError::NotRetired);
        }
        self.profiles
            .get_mut(profile_id)
            .expect("checked profile")
            .retired = false;
        match self.join(profile_id, None, spawn) {
            Ok(id) => Ok(id),
            Err(error) => {
                self.profiles
                    .get_mut(profile_id)
                    .expect("checked profile")
                    .retired = true;
                Err(error)
            }
        }
    }
}
