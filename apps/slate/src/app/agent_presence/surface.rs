//! Application data adapter for the shared participant/conversation surface.
use super::*;

impl SlateApp {
    /// Borrow display metadata only. The staging owner performs the real
    /// applicability, replay, permission and journal checks when a human decides.
    pub(super) fn presence_proposal_rows(&self, who: &str) -> Vec<chrome::ProposalDecision<'_>> {
        let Some(agent) = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who && a.tab == self.tab().id && a.departure.is_none())
        else {
            return Vec::new();
        };
        self.pending_stage_proposals()
            .iter()
            .filter(|proposal| {
                proposal.session.as_deref() == Some(agent.binding.session_id.as_str())
                    && proposal.target.workbook.as_deref() == self.tab().path.as_deref()
            })
            .map(|proposal| chrome::ProposalDecision {
                id: &proposal.id,
                label: self
                    .registry
                    .iter()
                    .find(|spec| spec.id.0 == proposal.title)
                    .map_or(proposal.title.as_str(), |spec| spec.name),
                recovery_required: proposal.status == slate_doc::ProposalStatus::RecoveryRequired,
                applying: self.presence.proposals.get(&proposal.id).is_some_and(|p| {
                    p.approval.is_some()
                        && agent.work.as_ref().is_some_and(|w| {
                            w.purpose
                                == witness::WitnessPurpose::ApprovedProposal(proposal.id.clone())
                        })
                }),
            })
            .collect()
    }

    pub(crate) fn presence_capsules(&mut self, ctx: &egui::Context) -> bool {
        self.ensure_agent_programs();
        self.presence_poll_photo(ctx);
        for provider in ["codex", "cursor"] {
            self.agents.want_catalog(provider);
        }
        let avatar_capture = self.presence_avatar_input(ctx);
        let palette = self.palette();
        let now = ctx.input(|i| i.time);
        let reduced = self.settings.presence_reduced_motion || chrome::system_reduced_motion(ctx);
        let tab = self.tab().id;
        let mut state = std::mem::take(&mut self.presence.chrome);
        if state.font_family().is_none() {
            state.set_font_family(
                slate_doc::scene::Typeface::Segoe
                    .egui_family()
                    .map(|name| egui::FontFamily::Name(name.into())),
            );
        }
        let mut painter = std::mem::take(&mut self.presence.painter);
        let live: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| a.tab == tab && a.departure.is_none())
            .collect();
        let proposals: Vec<_> = live
            .iter()
            .map(|a| {
                if state.active_chat() == Some(a.profile.id.as_str()) {
                    self.presence_proposal_rows(&a.profile.id)
                } else {
                    Vec::new()
                }
            })
            .collect();
        let human_ids: Vec<_> = self
            .settings
            .presence_contacts
            .iter()
            .map(|p| format!("human:{}", p.id))
            .collect();
        let messages: Vec<(Vec<_>, usize)> = std::iter::once("self")
            .chain(live.iter().map(|a| a.profile.id.as_str()))
            .chain(human_ids.iter().map(String::as_str))
            .map(|id| {
                if state.active_chat() != Some(id) {
                    return (Vec::new(), 0);
                }
                self.presence_conversation(id)
                    .map(|c| {
                        let total = c.turns.len();
                        let range = state.message_range(id, total);
                        let rows = c.turns[range]
                            .iter()
                            .map(|t| chrome::ConversationMessage {
                                id: &t.id,
                                text: &t.text,
                                outgoing: t.role == "user",
                                system: t.role == "system",
                                status: &t.status,
                            })
                            .collect();
                        (rows, total)
                    })
                    .unwrap_or_default()
            })
            .collect();
        let choices: Vec<_> = live
            .iter()
            .map(|a| self.presence_model_choice(&a.profile.id, &a.profile.provider))
            .collect();
        let models: Vec<Vec<_>> = live
            .iter()
            .map(|a| {
                self.agents
                    .catalog(&a.profile.provider)
                    .iter()
                    .map(|m| chrome::ProviderOption {
                        id: &m.id,
                        label: &m.name,
                    })
                    .collect()
            })
            .collect();
        let history_status = self.presence_history_status();
        let statuses: Vec<_> = live
            .iter()
            .map(|a| presence_mode_status(a, history_status.as_deref()))
            .collect();
        let self_thread = self.presence_conversation("self");
        let mut participants = vec![chrome::Participant {
            id: "self",
            name: &self.settings.presence_identity.name,
            hat: "none",
            status: history_status.as_deref().unwrap_or("Your private notes"),
            provider: "",
            human: true,
            paused: false,
            autonomous: false,
            file_inputs: false,
            own_chats: false,
            parent_name: None,
            chats: &[],
            local_profile: true,
            photo_texture: self.presence.photo.texture(),
            busy: false,
            messages: &messages[0].0,
            message_total: messages[0].1,
            proposals: &[],
            draft: self_thread.map_or("", |c| c.draft.as_str()),
            queued_provider: None,
            model: "",
            models: &[],
        }];
        for (index, a) in live.iter().enumerate() {
            participants.push(chrome::Participant {
                id: &a.profile.id,
                name: &a.profile.name,
                hat: &a.profile.hat,
                status: &statuses[index],
                provider: &a.profile.provider,
                human: false,
                paused: a.paused,
                autonomous: a.grant.as_ref().is_some_and(|g| g.mutation_autonomy),
                file_inputs: a
                    .grant
                    .as_ref()
                    .is_some_and(|g| g.readable_sources.iter().any(|s| s == "board-file-inputs")),
                own_chats: a.grant.as_ref().is_some_and(|g| {
                    g.readable_sources
                        .iter()
                        .any(|s| s == "board-owned-conversations")
                }),
                parent_name: a
                    .parent
                    .as_ref()
                    .and_then(|id| live.iter().find(|p| &p.profile.id == id))
                    .map(|p| p.profile.name.as_str()),
                chats: &[],
                local_profile: false,
                photo_texture: None,
                busy: self.presence.task_busy(&a.profile.id),
                messages: &messages[index + 1].0,
                message_total: messages[index + 1].1,
                proposals: &proposals[index],
                draft: self
                    .presence_conversation(&a.profile.id)
                    .map_or("", |c| c.draft.as_str()),
                queued_provider: a.pending_harness.as_ref().map(|p| p.0.as_str()),
                model: choices[index].model.as_deref().unwrap_or(""),
                models: &models[index],
            });
        }
        for (index, person) in self.settings.presence_contacts.iter().enumerate() {
            let who = human_ids[index].as_str();
            participants.push(chrome::Participant {
                id: who,
                name: &person.name,
                hat: "none",
                provider: "",
                human: true,
                local_profile: false,
                status: if let Some(problem) = history_status.as_deref() {
                    problem
                } else if self.presence.human_chat.online.contains(&person.id) {
                    "Online · direct messages"
                } else {
                    "Offline · saved conversation"
                },
                paused: false,
                autonomous: false,
                file_inputs: false,
                own_chats: false,
                parent_name: None,
                chats: &[],
                photo_texture: self.presence.human_chat.photo(&person.id),
                busy: false,
                messages: &messages[1 + live.len() + index].0,
                message_total: messages[1 + live.len() + index].1,
                proposals: &[],
                draft: self
                    .presence_conversation(who)
                    .map_or("", |c| c.draft.as_str()),
                queued_provider: None,
                model: "",
                models: &[],
            });
        }
        let providers = [
            chrome::ProviderOption {
                id: "codex",
                label: "Codex",
            },
            chrome::ProviderOption {
                id: "cursor",
                label: "Cursor",
            },
            chrome::ProviderOption {
                id: "local",
                label: "Dot / local",
            },
        ];
        let top = self.canvas_rect.right_top() + egui::vec2(-12.0, 12.0);
        let mut actions = Vec::new();
        let response = egui::Area::new(egui::Id::new("slate-participants"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(top)
            .show(ctx, |ui| {
                ui.set_max_width((self.canvas_rect.width() - 24.0).clamp(120.0, 620.0));
                actions = chrome::participant_capsules(
                    ui,
                    &palette,
                    &mut painter,
                    &mut state,
                    &participants,
                    &providers,
                    now,
                    reduced,
                );
            });
        let capture =
            avatar_capture || response.response.contains_pointer() || state.captures_pointer(ctx);
        self.presence.chrome = state;
        self.presence.painter = painter;
        for action in actions {
            self.dispatch_presence_action(ctx, action);
        }
        self.presence_people_setup(ctx);
        self.presence_schedule_surface(ctx);
        self.presence_activity_window(ctx);
        capture
            || ctx.pointer_latest_pos().is_some_and(|p| {
                ctx.layer_id_at(p).is_some_and(|l| {
                    [
                        "presence-people-setup",
                        "presence-schedule-editor",
                        "collaborator-conversation",
                    ]
                    .iter()
                    .any(|id| l.id == egui::Id::new(id))
                })
            })
    }

    pub(super) fn dispatch_presence_action(
        &mut self,
        ctx: &egui::Context,
        action: chrome::PresenceAction,
    ) {
        use chrome::PresenceAction::*;
        if let AcceptProposal { participant, id } | RejectProposal { participant, id } = &action {
            if !self
                .presence_proposal_rows(participant)
                .iter()
                .any(|p| p.id == id)
            {
                self.toast("This proposal is no longer available in this conversation.");
                return;
            }
        }
        let (command, detail) = match action {
            AcceptProposal { id, .. } => ("presence.accept", serde_json::json!({"id":id})),
            RejectProposal { id, .. } => ("presence.reject", serde_json::json!({"id":id})),
            Add {
                name,
                hat,
                provider,
            } => (
                "presence.add",
                serde_json::json!({"name":name,"hat":hat,"provider":provider}),
            ),
            Rename { id, name } => ("presence.name", serde_json::json!({"id":id,"name":name})),
            SetHat { id, hat } => ("presence.hat", serde_json::json!({"id":id,"hat":hat})),
            SetProvider { id, provider } => (
                "presence.provider",
                serde_json::json!({"id":id,"provider":provider}),
            ),
            SwitchProvider {
                id,
                provider,
                after_task,
            } => (
                "presence.provider.switch",
                serde_json::json!({"id":id,"provider":provider,"after_task":after_task}),
            ),
            CancelProviderSwitch { id } => {
                ("presence.provider.cancel", serde_json::json!({"id":id}))
            }
            SetModel { id, model } => {
                ("presence.model", serde_json::json!({"id":id,"model":model}))
            }
            Draft { id, text } => ("presence.draft", serde_json::json!({"id":id,"text":text})),
            SetPhoto { id } => ("presence.photo", serde_json::json!({"id":id})),
            RemovePhoto { id } => ("presence.photo.remove", serde_json::json!({"id":id})),
            ShowConversation { id } | OpenChat { id, .. } => {
                ("presence.chat", serde_json::json!({"id":id}))
            }
            Send { id, text } => ("presence.send", serde_json::json!({"id":id,"text":text})),
            Retry { id, message } => (
                "presence.retry",
                serde_json::json!({"id":id,"message":message}),
            ),
            ConnectPeople => ("presence.people", serde_json::json!({})),
            Pause { id, paused } => (
                "presence.pause",
                serde_json::json!({"id":id,"paused":paused}),
            ),
            Cancel { id } => ("presence.cancel", serde_json::json!({"id":id})),
            Retire { id, style } => (
                "presence.retire",
                serde_json::json!({"id":id,"style":style}),
            ),
            Restore { id } => ("presence.restore", serde_json::json!({"id":id})),
            Locate { id } => ("presence.locate", serde_json::json!({"id":id})),
            ShowActivity { id } => ("presence.activity", serde_json::json!({"id":id})),
            SetAutonomy { id, enabled } => (
                "presence.grant",
                serde_json::json!({"id":id,"enabled":enabled}),
            ),
            SetFileInputs { id, enabled } => (
                "presence.file_inputs",
                serde_json::json!({"id":id,"enabled":enabled}),
            ),
            SetOwnChats { id, enabled } => (
                "presence.own_chats",
                serde_json::json!({"id":id,"enabled":enabled}),
            ),
            Revoke { id } => ("presence.revoke", serde_json::json!({"id":id})),
            SpawnHelper { id } => ("presence.helper", serde_json::json!({"id":id})),
            Clone { id } => ("presence.clone", serde_json::json!({"id":id})),
            Schedule { id } => ("presence.schedule", serde_json::json!({"id":id})),
            CopyIdentity { id } => ("presence.identity", serde_json::json!({"id":id})),
            CopyLink { id } => ("presence.link", serde_json::json!({"id":id})),
        };
        if let Some(spec) = self.registry.iter().find(|s| s.id.0 == command) {
            self.dispatch(ctx, spec.id, Some(detail.to_string()));
        }
    }
}

pub(super) fn presence_mode_status(a: &Collaborator, history: Option<&str>) -> String {
    let status = history.unwrap_or(&a.status);
    let status = status.strip_suffix(" · proposal mode").unwrap_or(status);
    let mode = match a.grant.as_ref() {
        None => "no board access",
        Some(grant) if grant.mutation_autonomy => "autonomous board edits",
        Some(_) => "proposal mode",
    };
    format!("{status} · {mode}")
}
