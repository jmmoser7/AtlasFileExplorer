use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PauseCause {
    Human,
    InputHold,
}

impl SlateApp {
    pub(super) fn apply_presence_action(
        &mut self,
        ctx: &egui::Context,
        action: chrome::PresenceAction,
    ) -> Result<(), String> {
        use chrome::PresenceAction::*;
        let durable = matches!(
            &action,
            Add { .. }
                | Restore { .. }
                | Rename { .. }
                | SetHat { .. }
                | SetProvider { .. }
                | Retire { .. }
                | Clone { .. }
        );
        match action {
            decision @ (AcceptProposal { .. } | RejectProposal { .. }) => {
                self.dispatch_presence_action(ctx, decision);
            }
            SetOwnChats { id, enabled } => {
                let was_paused = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id)
                    .is_some_and(|a| a.paused);
                self.presence_set_own_chats(&id, enabled)?;
                if !enabled {
                    self.set_presence_paused(&id, was_paused, PauseCause::InputHold)?;
                }
                self.persist_consent_preference(
                    &id,
                    consent_policy::ConsentPreference::OwnChats,
                    enabled,
                )?;
            }
            Retry { id, message } => self.presence_retry_message(&id, &message)?,
            ConnectPeople => self.presence.human_chat.setup.open = true,
            SetupWorkspace => self.ai.pick_workspace(),
            Draft { id, text } => self.presence_save_draft(&id, &text)?,
            SetPhoto { id } => self.presence_choose_photo(ctx, &id)?,
            RemovePhoto { id } => self.presence_remove_photo(&id)?,
            SetModel { id, model } => self.presence_set_model(
                &id,
                (!model.is_empty()).then_some(model),
                Default::default(),
            )?,
            SwitchProvider {
                id,
                provider,
                after_task,
            } => self.presence_set_harness(
                &id,
                &provider,
                if after_task { "after-task" } else { "stop" },
            )?,
            CancelProviderSwitch { id } => self.presence_set_harness(&id, "", "cancel")?,
            Clone { id } => {
                let source = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id)
                    .map(|a| a.profile.clone())
                    .or_else(|| {
                        self.settings
                            .collaborators
                            .iter()
                            .find(|p| p.id == id)
                            .cloned()
                    })
                    .ok_or("Unknown collaborator")?;
                self.join_presence(
                    StoredAgent {
                        id: atlas_agent::request_id(),
                        name: format!("{} copy", source.name),
                        hat: source.hat,
                        provider: source.provider,
                        retired: false,
                    },
                    None,
                )?;
            }
            Schedule { id } => self.presence_open_schedule(&id)?,
            SetFileInputs { id, enabled } => {
                let primary = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id)
                    .is_some_and(|a| a.parent.is_none());
                if enabled {
                    if let Some(parent) = self
                        .presence
                        .agents
                        .iter()
                        .find(|a| a.profile.id == id)
                        .and_then(|a| a.parent.as_ref())
                    {
                        if !self
                            .presence
                            .agents
                            .iter()
                            .find(|a| &a.profile.id == parent)
                            .and_then(|a| a.grant.as_ref())
                            .is_some_and(|g| {
                                g.readable_sources.iter().any(|s| s == "board-file-inputs")
                            })
                        {
                            return Err(
                                "A temporary helper cannot exceed its parent's input scope".into(),
                            );
                        }
                    }
                }
                if !enabled {
                    let was_paused = self
                        .presence
                        .agents
                        .iter()
                        .find(|a| a.profile.id == id)
                        .is_some_and(|a| a.paused);
                    self.stop_presence(&id, false)?;
                    self.set_presence_paused(&id, was_paused, PauseCause::InputHold)?;
                }
                let a = self
                    .presence
                    .agents
                    .iter_mut()
                    .find(|a| a.profile.id == id)
                    .ok_or("Unknown collaborator")?;
                let grant = a.grant.as_mut().ok_or("Board grant revoked")?;
                grant.readable_sources.retain(|s| s != "board-file-inputs");
                if enabled {
                    grant.readable_sources.push("board-file-inputs".into());
                }
                if !a.request.is_empty() {
                    self.presence
                        .links
                        .set_session(&a.dir, a.binding.clone(), grant.clone(), a.request.clone())
                        .map_err(str::to_owned)?;
                }
                if primary {
                    self.persist_consent_preference(
                        &id,
                        consent_policy::ConsentPreference::BoardFileInputs,
                        enabled,
                    )?;
                }
            }
            CopyIdentity { id } => ctx.copy_text(format!("@{id}")),
            CopyLink { id } => {
                let a = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id)
                    .ok_or("Unknown collaborator")?;
                ctx.copy_text(a.dir.to_string_lossy().into_owned());
            }
            Add {
                name,
                hat,
                provider,
            } => {
                let id = atlas_agent::request_id();
                self.join_presence(
                    StoredAgent {
                        id: id.clone(),
                        name,
                        hat,
                        provider,
                        retired: false,
                    },
                    None,
                )?;
                self.presence.chrome.open_chat(&id);
            }
            Restore { id } => {
                let mut profile = self
                    .settings
                    .collaborators
                    .iter()
                    .find(|p| p.id == id)
                    .cloned()
                    .ok_or("Unknown saved profile")?;
                profile.retired = false;
                self.join_presence(profile, None)?;
            }
            SpawnHelper { id } => {
                let parent = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id && a.grant.is_some() && !a.paused)
                    .ok_or("The parent needs an active board grant")?;
                if parent.parent.is_some() {
                    return Err("Temporary helpers cannot spawn helpers".into());
                }
                let profile = StoredAgent {
                    id: atlas_agent::request_id(),
                    name: self.presence.next_helper_name(&id, &parent.profile.name),
                    hat: "beanie".into(),
                    provider: parent.profile.provider.clone(),
                    retired: false,
                };
                self.join_presence(profile, Some(id))?;
            }
            Send { id, text } => self.presence_send_chat(&id, &text)?,
            Locate { id } => {
                let a = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id && a.tab == self.tab().id)
                    .ok_or("This collaborator is on another board")?;
                let p = a
                    .position(&self.presence.roster)
                    .ok_or("Collaborator has retired")?;
                self.tab_mut().cam.offset = egui::vec2(p.x as f32, p.y as f32);
                ctx.request_repaint();
            }
            ShowActivity { id } => self.presence.chrome.open_chat(&id),
            ShowConversation { id } | OpenChat { id, .. } => self.presence.chrome.open_chat(&id),
            Attach { id } => self.presence_attach_image(ctx, &id)?,
            RemoveAttachment { id, index } => self.presence_remove_attachment(&id, index)?,
            Cancel { id } => {
                self.stop_presence(&id, false)?;
                self.set_presence_paused(&id, true, PauseCause::InputHold)?;
                if let Some(a) = self.presence.agents.iter_mut().find(|a| a.profile.id == id) {
                    a.activity = "cancel".into();
                    a.status = "Stopped. Resume or send a new task.".into();
                }
            }
            Revoke { id } => self.stop_presence(&id, true)?,
            Pause { id, paused } => {
                if paused {
                    self.set_presence_paused(&id, true, PauseCause::Human)?;
                } else if !self.presence.agents.iter().any(|agent| {
                    agent.profile.id == id
                        && agent.tab == self.tab().id
                        && agent.departure.is_none()
                }) {
                    self.ensure_saved_presence_joined(&id)?;
                } else {
                    self.presence_resume_chat(&id)?;
                }
            }
            Retire { id, style } => self.retire_presence(&id, &style)?,
            SetAutonomy { id, enabled } => {
                if enabled
                    && self
                        .presence
                        .agents
                        .iter()
                        .any(|a| a.profile.id == id && a.parent.is_some())
                {
                    return Err("Temporary helpers use proposal mode; add an independent collaborator for autonomous edits".into());
                }
                if !enabled {
                    let was_paused = self
                        .presence
                        .agents
                        .iter()
                        .find(|a| a.profile.id == id)
                        .is_some_and(|a| a.paused);
                    self.stop_presence(&id, false)?;
                    self.set_presence_paused(&id, was_paused, PauseCause::InputHold)?;
                }
                let a = self
                    .presence
                    .agents
                    .iter_mut()
                    .find(|a| a.profile.id == id)
                    .ok_or("Unknown collaborator")?;
                let grant = a
                    .grant
                    .as_mut()
                    .ok_or("Grant was revoked; restore a fresh session")?;
                grant.mutation_autonomy = enabled;
                if !a.request.is_empty() {
                    self.presence
                        .links
                        .set_session(&a.dir, a.binding.clone(), grant.clone(), a.request.clone())
                        .map_err(str::to_owned)?;
                }
                self.persist_consent_preference(
                    &id,
                    consent_policy::ConsentPreference::MutationAutonomy,
                    enabled,
                )?;
            }
            SetFullAccess { id, enabled } => {
                let (session, provider, primary) = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id)
                    .map(|a| {
                        (
                            a.binding.session_id.clone(),
                            a.profile.provider.clone(),
                            a.parent.is_none(),
                        )
                    })
                    .ok_or("Unknown collaborator")?;
                if !primary || provider != "codex" {
                    return Err("Provider full access is available only for independent Codex collaborators".into());
                }
                self.presence_consent_scope()?;
                self.set_agent_session_access(&session, enabled)?;
                self.persist_consent_preference(
                    &id,
                    consent_policy::ConsentPreference::ProviderFullAccess,
                    enabled,
                )?;
            }
            Rename { id, name } => {
                if id == "self" {
                    return self.presence_update_identity(
                        &name,
                        self.settings.presence_identity.photo.clone(),
                    );
                }
                let old_name = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.profile.id == id)
                    .map(|a| a.profile.name.clone())
                    .ok_or("Unknown collaborator")?;
                let name = name.trim().chars().take(80).collect::<String>();
                if name.is_empty() || name.chars().any(char::is_control) {
                    return Err("Choose a readable name".into());
                }
                for policy in self
                    .settings
                    .agent_consent_policies
                    .iter_mut()
                    .filter(|policy| policy.identity_id == id && policy.identity_name == old_name)
                {
                    policy.identity_name = name.clone();
                }
                let a = self
                    .presence
                    .agents
                    .iter_mut()
                    .find(|a| a.profile.id == id)
                    .ok_or("Unknown collaborator")?;
                a.profile.name = name;
            }
            SetHat { id, hat } => {
                let a = self
                    .presence
                    .agents
                    .iter_mut()
                    .find(|a| a.profile.id == id)
                    .ok_or("Unknown collaborator")?;
                a.profile.hat = hat;
            }
            SetProvider { id, provider } => {
                self.presence_set_harness(&id, &provider, "choose")?;
            }
        }
        if durable {
            self.persist_presence_profiles();
        }
        Ok(())
    }
}

impl SlateApp {
    pub(super) fn set_presence_paused(
        &mut self,
        who: &str,
        paused: bool,
        cause: PauseCause,
    ) -> Result<(), String> {
        if cause == PauseCause::Human {
            self.presence_forget_avatar_hold(who);
        }
        let busy = self.presence.task_busy(who);
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
            .ok_or("Unknown collaborator")?;
        if paused && !a.paused {
            a.resume_task = busy;
        }
        if !paused && cause == PauseCause::InputHold {
            a.resume_task = false;
        }
        a.paused = paused;
        if cause == PauseCause::Human {
            a.control_epoch = a.control_epoch.wrapping_add(1);
        }
        a.chat_only = false;
        a.status = if paused {
            "Listening; task suspended"
        } else {
            "Resumed"
        }
        .into();
        a.activity = if paused {
            "listening"
        } else if a.inflight {
            "working"
        } else {
            "idle"
        }
        .into();
        Ok(())
    }
}
