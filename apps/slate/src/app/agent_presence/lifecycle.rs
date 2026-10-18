use super::*;

const SPAWN_CLEARANCE: f64 = 64.0;

impl SlateApp {
    pub(super) fn join_presence(
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
    pub(super) fn join_scoped_presence(
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
        if self.at_home || self.tabs.is_empty() {
            return Err("Open a board workbook before adding a collaborator".into());
        }
        if self
            .presence
            .agents
            .iter()
            .any(|a| a.profile.id == profile.id && a.departure.is_none())
        {
            return Err("This collaborator is already present".into());
        }
        if !matches!(profile.provider.as_str(), "codex" | "local" | "cursor") {
            return Err("Choose Codex, Cursor, or a local sidecar".into());
        }
        if let Some((who, _, grant)) = &parent {
            let count = self
                .presence
                .agents
                .iter()
                .filter(|a| a.parent.as_deref() == Some(who) && a.departure.is_none())
                .count();
            if count >= usize::from(grant.helper_limit) {
                return Err("Helper limit reached".into());
            }
        }
        let conversations = parent.as_ref().map_or_else(
            || self.presence_conversations(),
            |p| p.2.conversation_ids.clone(),
        );
        if conversations.len() >= 128
            || self
                .presence
                .agents
                .iter()
                .filter(|a| a.tab == self.tab().id && a.departure.is_none())
                .any(|a| {
                    a.grant
                        .as_ref()
                        .is_some_and(|g| g.conversation_ids.len() >= 128)
                })
        {
            return Err("The board conversation scope is full (128 endpoints). Retire an unused collaborator or remove an unused chat before adding another.".into());
        }
        let profile_id = id(&profile.id)?;
        let position = self.board_xf().s2w(self.canvas_rect.center());
        let parent_position = parent
            .as_ref()
            .and_then(|p| self.presence.agents.iter().find(|a| a.profile.id == p.0))
            .and_then(|a| a.position(&self.presence.roster))
            // presence_call temporarily owns detached parents; helper admission
            // rejects active work, so their settled session position is exact.
            .or_else(|| {
                parent
                    .as_ref()
                    .and_then(|p| self.presence.roster.session(&p.1))
                    .map(|session| session.sample().position)
            });
        let preferred = parent_position.map_or_else(
            || point(position.x, position.y),
            |p| WorldPoint {
                x: p.x + SPAWN_CLEARANCE,
                y: p.y,
            },
        );
        let mut occupied: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id && a.profile.id != profile.id)
            .filter_map(|a| {
                a.position(&self.presence.roster)
                    .or_else(|| a.departure.as_ref().map(|d| d.2))
            })
            .collect();
        if let Some(p) = parent_position.filter(|p| !occupied.contains(p)) {
            occupied.push(p);
        }
        let spawn = PresenceRoster::vacant_spawn(preferred, &occupied, SPAWN_CLEARANCE)
            .map_err(|e| e.to_string())?;
        let parent_motion = parent.as_ref().map(|p| p.1.clone());
        let stored = AgentProfile {
            id: profile_id.clone(),
            name: profile.name.clone(),
            hat: Some(profile.hat.clone()),
            retired: false,
        };
        let motion = if self.presence.roster.profiles().any(|p| p.id == profile_id) {
            self.presence.roster.restore(&profile_id, spawn)
        } else {
            self.presence
                .roster
                .register(stored)
                .map_err(|e| e.to_string())?;
            self.presence
                .roster
                .join(&profile_id, parent_motion.as_ref(), spawn)
        }
        .map_err(|e| e.to_string())?;
        let workspace_id = format!("slate-{}", self.tab().id);
        let binding = BoardLinkSession {
            session_id: motion.as_str().into(),
            grant_id: atlas_agent::request_id(),
            workspace_id: workspace_id.clone(),
        };
        let mut grant = CollaboratorGrant {
            id: binding.grant_id.clone(),
            workspace_id,
            readable_board: true,
            readable_nodes: vec![],
            readable_sources: vec![],
            commands: BOARD_COMMANDS.iter().map(|s| (*s).into()).collect(),
            conversation_ids: conversations,
            mutation_autonomy: false,
            helper_limit: 4,
            max_actions: 128,
        };
        let mut provider_full_access = false;
        if parent.is_none() {
            if let Some(scope) =
                consent_policy::ConsentScope::workbook_or_workspace(self.tab().path.as_deref(), &ws)
            {
                if let Some(policy) = self
                    .settings
                    .agent_consent_policies
                    .iter()
                    .find(|policy| policy.matches(&profile, &scope))
                {
                    grant.mutation_autonomy = policy.capabilities.mutation_autonomy;
                    provider_full_access = policy.capabilities.provider_full_access;
                    if policy.capabilities.board_file_inputs {
                        grant.readable_sources.push("board-file-inputs".into());
                    }
                    if policy.capabilities.own_chats {
                        grant.readable_sources.push(own_chats::OWN_CHATS.into());
                    }
                }
            }
        }
        if let Some((_, _, pg)) = &parent {
            grant.commands = pg
                .commands
                .iter()
                .filter(|c| c.as_str() != "presence.helper")
                .cloned()
                .collect();
            grant.readable_board = pg.readable_board;
            grant.readable_nodes = pg.readable_nodes.clone();
            grant.readable_sources = pg.readable_sources.clone();
            grant.readable_sources.retain(|s| s != own_chats::OWN_CHATS);
            grant.conversation_ids = pg.conversation_ids.clone();
            grant.helper_limit = 0;
            grant.max_actions = pg.max_actions.min(32);
            if !pg.allows_helper(&grant) {
                let _ = self.presence.roster.leave(&motion);
                return Err("Helper exceeds parent scope".into());
            }
        }
        let dir = atlas_ai::agent::agent_dir(&ws, motion.as_str());
        if parent.is_none() && profile.provider == "codex" {
            if self.agent_session_access_available() {
                if let Err(error) =
                    self.set_agent_session_access(&binding.session_id, provider_full_access)
                {
                    let _ = self.presence.roster.leave(&motion);
                    return Err(error);
                }
            } else if provider_full_access {
                let _ = self.presence.roster.leave(&motion);
                return Err("Machine provider-access storage is unavailable".into());
            }
        }
        self.presence.agents.retain(|a| a.profile.id != profile.id);
        // A human join explicitly names this endpoint to current collaborators.
        let new_id = profile.id.clone();
        if !grant.conversation_ids.contains(&new_id) {
            grant.conversation_ids.push(new_id.clone());
        }
        if parent.is_some() {
            self.presence.helper_endpoints.insert(new_id.clone());
        }
        let tab_id = self.tab().id;
        for existing in self
            .presence
            .agents
            .iter_mut()
            .filter(|a| a.tab == tab_id && a.departure.is_none())
        {
            if let Some(g) = existing.grant.as_mut() {
                if g.conversation_ids.len() < 128 && !g.conversation_ids.contains(&new_id) {
                    g.conversation_ids.push(new_id.clone());
                }
            }
        }
        self.presence.agents.push(Collaborator {
            profile,
            tab: self.tab().id,
            motion,
            binding,
            grant: Some(grant),
            dir,
            request: String::new(),
            sequences: 0,
            calls: 0,
            paused: false,
            resume_task: false,
            parent: parent.map(|p| p.0),
            helper_outcome: None,
            runtime: None,
            cursor: None,
            snapshot: None,
            status: "Ready · proposal mode".into(),
            activity: "idle".into(),
            last_event: None,
            last_active: Instant::now(),
            work: None,
            read: None,
            control_epoch: 0,
            owned_conversations: Vec::new(),
            departure: None,
            had_response: false,
            relays: 0,
            observed_target: None,
            joined_at: Instant::now(),
            inflight: false,
            seen_running: false,
            launched: Vec::new(),
            pending_harness: None,
            switch_offer: None,
            chat_only: false,
            reply_from: 0,
        });
        Ok(())
    }

    pub(super) fn stop_presence(&mut self, who: &str, revoke: bool) -> Result<(), String> {
        self.stop_presence_with(who, revoke, revoke, true)
    }
    fn stop_presence_with(
        &mut self,
        who: &str,
        revoke: bool,
        forget_preferences: bool,
        cancel_ancestors: bool,
    ) -> Result<(), String> {
        if cancel_ancestors {
            self.presence.cancel_pending_ancestors(who);
        }
        let ids = self.presence.descendants(who);
        self.presence.scheduling.disarm(&ids);
        if !self.presence.agents.iter().any(|a| a.profile.id == who) {
            return Err("Unknown collaborator".into());
        }
        if forget_preferences {
            self.settings
                .agent_consent_policies
                .retain(|policy| !ids.contains(&policy.identity_id));
            self.settings.save();
        }
        if revoke && self.agent_session_access_available() {
            let sessions: Vec<_> = ids
                .iter()
                .filter_map(|id| {
                    self.presence
                        .agents
                        .iter()
                        .find(|a| a.profile.id == *id && a.profile.provider == "codex")
                        .map(|a| a.binding.session_id.clone())
                })
                .collect();
            for session in sessions {
                if let Err(error) = self.set_agent_session_access(&session, false) {
                    self.toast(error);
                }
            }
        }
        for who in ids {
            let busy = self
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == who)
                .is_some_and(|a| {
                    a.inflight
                        || a.work.is_some()
                        || a.read.is_some()
                        || self.presence.has_pending_for(a)
                        || !a.launched.is_empty()
                });
            // Fence bridge authority before requesting provider cancellation.
            if let Some(a) = self.presence.agents.iter().find(|a| a.profile.id == who) {
                self.presence.links.revoke(&a.dir);
                if self.presence.conversations.get(&who).is_some() {
                    let request = a.request.clone();
                    let thread = self.presence.conversations.edit(&who)?;
                    let updates_stopped = updates::stop_updates(thread);
                    let changed = thread.stop(&request);
                    if busy && !request.is_empty() {
                        thread.push(
                            "system",
                            "Task stopped. Completed changes and results were kept.",
                            "Slate",
                            None,
                            &request,
                            "stopped",
                        );
                    }
                    if changed || updates_stopped || busy {
                        self.presence.conversations.save(&who);
                    }
                }
            }
            let launched = self
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == who)
                .map(|a| a.launched.clone())
                .unwrap_or_default();
            for (target, request) in launched {
                self.stop_agent_request_at(target, &request);
            }
            let session = self
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == who)
                .map(|a| a.binding.session_id.clone())
                .unwrap_or_default();
            self.cancel_presence_proposals(&session);
            let a = self
                .presence
                .agents
                .iter_mut()
                .find(|a| a.profile.id == who)
                .unwrap();
            a.control_epoch = a.control_epoch.wrapping_add(1);
            if let Some(runtime) = &a.runtime {
                runtime.stop();
            }
            a.runtime = None;
            if let Some(cursor) = a.cursor.take() {
                cursor.stop();
            }
            a.read = None;
            a.launched.clear();
            a.cancel_motion(&mut self.presence.roster)?;
            if let Some(work) = a.work.take() {
                let _ = self.presence.links.reply(
                    &a.dir,
                    BoardReply::error(&work.call, "Work cancelled by the human"),
                );
            }
            self.presence.links.revoke(&a.dir);
            if let Some(m) = self.presence.roster.session_mut(&a.motion) {
                m.cancel_request();
            }
            a.inflight = false;
            a.paused = true;
            a.pending_harness = None;
            a.switch_offer = None;
            a.chat_only = false;
            a.status = if revoke {
                "Board grant revoked"
            } else {
                "Cancelled; ready for a new task"
            }
            .into();
            a.activity = "cancel".into();
            if busy
                && !a
                    .helper_outcome
                    .as_ref()
                    .is_some_and(|o| o.provider_failed())
            {
                a.note_helper_outcome("cancel", &a.status.clone());
            }
            a.request.clear();
            if revoke {
                a.grant = None;
            }
        }
        let tab = self.tab().id;
        self.presence.refresh_held(tab);
        Ok(())
    }

    pub(super) fn retire_presence(&mut self, who: &str, style: &str) -> Result<(), String> {
        self.depart_presence(who, style, false)
    }
    pub(super) fn depart_presence(
        &mut self,
        who: &str,
        style: &str,
        completed_helper: bool,
    ) -> Result<(), String> {
        self.stop_presence_with(who, true, false, !completed_helper)?;
        let descendants = self.presence.descendants(who);
        let ids: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| descendants.contains(&a.profile.id))
            .map(|a| {
                (
                    a.profile.id.clone(),
                    a.position(&self.presence.roster).unwrap_or(point(0.0, 0.0)),
                )
            })
            .collect();
        for (who, p) in ids {
            let a = self
                .presence
                .agents
                .iter_mut()
                .find(|a| a.profile.id == who)
                .unwrap();
            a.departure = Some((Instant::now(), style.into(), p));
            a.profile.retired = true;
            a.activity = "retire".into();
            let _ = self.presence.roster.retire(&id(&who)?);
        }
        Ok(())
    }
}
