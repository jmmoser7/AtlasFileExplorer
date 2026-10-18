use super::*;
use std::collections::HashMap;

/// One board's ephemeral membership. Persist only `profiles()`, never this roster.
#[derive(Debug, Default)]
pub struct PresenceRoster {
    profiles: HashMap<PresenceId, AgentProfile>,
    sessions: HashMap<PresenceId, PresenceSession>,
}

impl PresenceRoster {
    /// Choose only the new avatar's world position; never move existing members.
    /// The host supplies displayed positions for its board, including departures.
    pub fn vacant_spawn(
        preferred: WorldPoint,
        occupied: &[WorldPoint],
        clearance: f64,
    ) -> Result<WorldPoint, PresenceError> {
        preferred.validate()?;
        if !clearance.is_finite() || clearance <= 0.0 {
            return Err(PresenceError::InvalidPoint);
        }
        if occupied.len() > PROFILE_LIMIT {
            return Err(PresenceError::ProfileLimit);
        }
        for point in occupied {
            point.validate()?;
        }
        let vacant = |candidate: WorldPoint| {
            candidate.validate().is_ok()
                && occupied
                    .iter()
                    .all(|point| (candidate.x - point.x).hypot(candidate.y - point.y) >= clearance)
        };
        if vacant(preferred) {
            return Ok(preferred);
        }
        // A point excludes at most four lattice sites. The extra rings also
        // leave room at world-coordinate edges; at most 2,209 candidates/128 sites.
        let rings = ((occupied.len() * 4 + 1) as f64).sqrt().ceil() as i32;
        for ring in 1..=rings {
            for y in -ring..=ring {
                for x in -ring..=ring {
                    if x.abs() != ring && y.abs() != ring {
                        continue;
                    }
                    let candidate = WorldPoint {
                        x: preferred.x + f64::from(x) * clearance,
                        y: preferred.y + f64::from(y) * clearance,
                    };
                    if vacant(candidate) {
                        return Ok(candidate);
                    }
                }
            }
        }
        Err(PresenceError::InvalidPoint)
    }

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

#[cfg(test)]
mod spawn_tests {
    use super::*;

    #[test]
    fn vacancy_is_bounded_deterministic_and_preserves_existing_world_positions() {
        let preferred = WorldPoint::new(320.0, 180.0).unwrap();
        let mut occupied = Vec::new();
        for _ in 0..PROFILE_LIMIT {
            let before = occupied.clone();
            let spawn = PresenceRoster::vacant_spawn(preferred, &occupied, 64.0).unwrap();
            assert_eq!(occupied, before);
            assert!(occupied
                .iter()
                .all(|p: &WorldPoint| { (p.x - spawn.x).hypot(p.y - spawn.y) >= 64.0 }));
            let mut reversed = occupied.clone();
            reversed.reverse();
            assert_eq!(
                PresenceRoster::vacant_spawn(preferred, &reversed, 64.0),
                Ok(spawn)
            );
            occupied.push(spawn);
        }
        assert_eq!(occupied[0], preferred);
    }

    #[test]
    fn vacancy_validates_inputs_and_fails_without_an_overlapping_fallback() {
        let p = WorldPoint::new(0.0, 0.0).unwrap();
        for gap in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                PresenceRoster::vacant_spawn(p, &[], gap),
                Err(PresenceError::InvalidPoint)
            );
        }
        let bad = WorldPoint {
            x: f64::NAN,
            y: 0.0,
        };
        for (preferred, occupied) in [(bad, vec![]), (p, vec![bad])] {
            assert_eq!(
                PresenceRoster::vacant_spawn(preferred, &occupied, 64.0),
                Err(PresenceError::InvalidPoint)
            );
        }
        assert_eq!(
            PresenceRoster::vacant_spawn(p, &vec![p; PROFILE_LIMIT + 1], 64.0),
            Err(PresenceError::ProfileLimit)
        );
        assert_eq!(
            PresenceRoster::vacant_spawn(p, &[p], WORLD_COORD_LIMIT * 4.0),
            Err(PresenceError::InvalidPoint)
        );
        let edge = WorldPoint::new(WORLD_COORD_LIMIT, WORLD_COORD_LIMIT).unwrap();
        let spawn = PresenceRoster::vacant_spawn(edge, &[edge], 64.0).unwrap();
        assert!(spawn.validate().is_ok());
        assert!((spawn.x - edge.x).hypot(spawn.y - edge.y) >= 64.0);
    }
}
