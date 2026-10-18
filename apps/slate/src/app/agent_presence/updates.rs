//! Typed follow-ups are accepted during work; only provider acknowledgements mean delivery.
use super::*;

impl SlateApp {
    pub(super) fn presence_resume_chat(&mut self, who: &str) -> Result<(), String> {
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .ok_or("Unknown collaborator")?;
        if a.grant.is_none() {
            return Err(
                "Board access was revoked. Restore the agent to grant a fresh session.".into(),
            );
        }
        let continue_task = !self.presence.task_busy(who)
            && (a.resume_task
                || self.presence_conversation(who).is_some_and(|c| {
                    c.turns
                        .iter()
                        .rev()
                        .find(|t| {
                            t.role == "user" && !matches!(t.status.as_str(), "queued" | "not-sent")
                        })
                        .is_some_and(|t| t.status == "stopped")
                }));
        self.set_presence_paused(who, false, controls::PauseCause::Human)?;
        if let Some(a) = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
        {
            a.resume_task = false;
        }
        if continue_task {
            self.send_presence_message(who, "Continue from the previous task. Check the current board state before making further changes.")?;
        }
        Ok(())
    }
    pub(super) fn presence_send_update(&mut self, who: &str, text: &str) -> Result<(), String> {
        submission::validate_message_text(text)?;
        self.ai
            .config
            .valid_workspace()
            .ok_or("The AI workspace is unavailable")?;
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .ok_or("Collaborator is not present")?;
        let provider = a.profile.provider.clone();
        let c = self.presence.conversations.edit(who)?;
        if c.turns
            .iter()
            .filter(|t| t.status == "queued" && t.board_origin.is_none())
            .count()
            >= 8
        {
            return Err(
                "Eight updates are queued. Resume the agent or wait for the current task.".into(),
            );
        }
        let images = c.draft_images.clone();
        let steer = if a.inflight && provider == "codex" && images.is_empty() {
            a.runtime
                .as_ref()
                .map(|r| r.steer(&a.request, text))
                .transpose()?
        } else {
            None
        };
        let status = if steer.is_some() {
            "steering"
        } else {
            "queued"
        };
        let request = steer.unwrap_or_else(atlas_agent::request_id);
        let author = self.settings.presence_identity.name.clone();
        c.push(
            "user",
            text.trim(),
            &author,
            Some(&provider),
            &request,
            status,
        );
        c.set_message_images(&request, images);
        c.set_draft_images(Vec::new());
        c.set_draft("");
        self.presence.conversations.save(who);
        Ok(())
    }
    pub(super) fn presence_pump_updates(&mut self) {
        // One queued turn per frame. No task is restarted merely by opening Slate.
        let next = self
            .presence
            .agents
            .iter()
            .filter(|a| {
                a.tab == self.tab().id
                    && a.departure.is_none()
                    && !a.paused
                    && !self.presence.task_busy(&a.profile.id)
            })
            .find_map(|a| {
                self.presence_conversation(&a.profile.id).and_then(|c| {
                    c.turns
                        .iter()
                        .find(|t| {
                            t.role == "user" && t.status == "queued" && t.board_origin.is_none()
                        })
                        .map(|t| (a.profile.id.clone(), t.request.clone(), t.text.clone()))
                })
            });
        if let Some((who, request, text)) = next {
            if let Ok(c) = self.presence.conversations.edit(&who) {
                if let Some(index) = c.request_index(&request) {
                    c.set_turn_status(index, "dispatching");
                }
            }
            if let Err(error) = self.submit_presence_message(&who, &text, None, Some(&request)) {
                if let Ok(c) = self.presence.conversations.edit(&who) {
                    if let Some(index) = c.request_index(&request) {
                        c.set_turn_status(index, "failed");
                    }
                    c.push("system", &error, "Slate", None, "", "complete");
                }
            }
            self.presence.conversations.save(&who);
        }
    }
}

pub(super) fn acknowledge_updates(c: &mut conversation::Conversation, snapshot: &AgentSession) {
    for event in &snapshot.activity {
        let Some(key) = event.id.strip_prefix("codex-steer:") else {
            continue;
        };
        let Some((id, result)) = key.rsplit_once(':') else {
            continue;
        };
        if let Some(index) = c.request_index(id) {
            if c.turns[index].status == "steering" {
                c.set_turn_status(
                    index,
                    if result == "accepted" {
                        "complete"
                    } else {
                        "failed"
                    },
                );
            }
        }
    }
    if !matches!(snapshot.status, atlas_agent::AgentStatus::Thinking) {
        let pending: Vec<_> = c
            .turns
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status == "steering")
            .map(|(i, _)| i)
            .collect();
        for index in pending {
            c.set_turn_status(index, "unconfirmed");
        }
    }
}

pub(super) fn stop_updates(c: &mut conversation::Conversation) -> bool {
    let indices: Vec<_> = c
        .turns
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.board_origin.is_none()
                && matches!(t.status.as_str(), "queued" | "dispatching" | "steering")
        })
        .map(|(i, _)| i)
        .collect();
    let changed = !indices.is_empty();
    for index in indices {
        c.set_turn_status(index, "not-sent");
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::super::super::tests::Harness;
    use super::*;
    fn host() -> (Harness, String) {
        let mut h = Harness::new("chat_updates");
        h.app.at_home = false;
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.ai.config.workspace_dir = Some(h.base.clone());
        let who = atlas_agent::request_id();
        h.app
            .join_presence(
                StoredAgent {
                    id: who.clone(),
                    name: "Dot".into(),
                    hat: "cap".into(),
                    provider: "local".into(),
                    retired: false,
                },
                None,
            )
            .unwrap();
        (h, who)
    }
    #[test]
    fn immediate_second_send_cannot_replace_a_live_request_binding() {
        let (mut h, who) = host();
        h.app.send_presence_message(&who, "First task").unwrap();
        let request = h.app.presence.agents[0].request.clone();
        let binding = h.app.presence.agents[0].binding.clone();
        h.app.send_presence_message(&who, "Second task").unwrap();
        assert_eq!(h.app.presence.agents[0].request, request);
        assert_eq!(h.app.presence.agents[0].binding, binding);
        assert_eq!(
            h.app
                .presence
                .conversations
                .get(&who)
                .unwrap()
                .turns
                .iter()
                .filter(|turn| {
                    turn.role == "user" && turn.text == "Second task" && turn.status == "queued"
                })
                .count(),
            1
        );
        h.app.stop_presence(&who, true).unwrap();
        assert!(h.app.presence.agents[0].grant.is_none());
        assert!(!h.app.presence.agents[0].inflight);
    }
    #[test]
    fn update_while_busy_preserves_active_request_and_dispatches_in_order() {
        let (mut h, who) = host();
        h.app.presence_send_chat(&who, "First task").unwrap();
        let first = h.app.presence.agents[0].request.clone();
        h.app
            .presence_send_chat(&who, "Please focus on the captions")
            .unwrap();
        assert_eq!(h.app.presence.agents[0].request, first);
        let c = h.app.presence_conversation(&who).unwrap();
        assert_eq!(c.turns.last().unwrap().status, "queued");
        assert_eq!(
            c.replay().len(),
            1,
            "undelivered updates are not provider history"
        );
        h.app.presence.agents[0].inflight = false;
        h.app.presence_pump_updates();
        assert_ne!(h.app.presence.agents[0].request, first);
        let c = h.app.presence_conversation(&who).unwrap();
        assert_eq!(
            c.turns
                .iter()
                .filter(|t| t.text == "Please focus on the captions")
                .count(),
            1
        );
        assert_eq!(c.turns.last().unwrap().status, "sent");
    }
    #[test]
    fn paused_updates_wait_for_resume_and_stop_cancels_without_replay() {
        let (mut h, who) = host();
        h.app.presence_send_chat(&who, "Read the brief").unwrap();
        h.app.presence.agents[0].paused = true;
        h.app
            .presence_send_chat(&who, "Then check the labels")
            .unwrap();
        h.app.presence.agents[0].inflight = false;
        h.app.presence_pump_updates();
        assert_eq!(
            h.app
                .presence_conversation(&who)
                .unwrap()
                .turns
                .last()
                .unwrap()
                .status,
            "queued"
        );
        h.app.stop_presence(&who, false).unwrap();
        assert_eq!(
            h.app
                .presence_conversation(&who)
                .unwrap()
                .turns
                .last()
                .unwrap()
                .status,
            "not-sent"
        );
    }
    #[test]
    fn a_new_human_message_after_stop_starts_work_without_discarding_history() {
        let (mut h, who) = host();
        h.app.presence_send_chat(&who, "First task").unwrap();
        h.app
            .apply_presence_action(&h.ctx, chrome::PresenceAction::Cancel { id: who.clone() })
            .unwrap();
        assert!(h.app.presence.agents[0].paused);
        h.app
            .presence_send_chat(&who, "Try this new task instead")
            .unwrap();
        assert!(!h.app.presence.agents[0].paused);
        assert!(h.app.presence.agents[0].inflight);
        assert!(h
            .app
            .presence_conversation(&who)
            .unwrap()
            .turns
            .iter()
            .any(|t| t.text == "First task"));
    }
    #[test]
    fn a_provider_error_cannot_turn_a_listening_message_into_board_resume() {
        let (mut h, who) = host();
        h.app.presence_send_chat(&who, "First task").unwrap();
        h.app
            .set_presence_paused(&who, true, controls::PauseCause::Human)
            .unwrap();
        let snapshot = AgentSession {
            request: h.app.presence.agents[0].request.clone(),
            status: atlas_agent::AgentStatus::Error("Declared provider failure".into()),
            ..Default::default()
        };
        super::super::lifecycle_tests::session_support::publish_and_observe(
            &mut h, &who, &snapshot,
        );
        assert_eq!(h.app.presence.agents[0].activity, "error");
        assert!(h.app.presence.agents[0].paused);
        h.app
            .presence_send_chat(&who, "Discuss the failure first")
            .unwrap();
        assert!(h.app.presence.agents[0].paused);
        assert!(h.app.presence.agents[0].chat_only);
    }
    #[test]
    fn explicit_resume_restarts_a_task_that_ended_while_board_work_was_paused() {
        let (mut h, who) = host();
        h.app
            .presence_send_chat(&who, "Arrange the captions")
            .unwrap();
        let first = h.app.presence.agents[0].request.clone();
        h.app
            .set_presence_paused(&who, true, controls::PauseCause::Human)
            .unwrap();
        h.app.presence.agents[0].inflight = false;
        let c = h.app.presence.conversations.edit(&who).unwrap();
        c.set_turn_status(0, "complete");
        h.app.presence_resume_chat(&who).unwrap();
        assert!(!h.app.presence.agents[0].paused);
        assert_ne!(h.app.presence.agents[0].request, first);
        assert!(h
            .app
            .presence_conversation(&who)
            .unwrap()
            .turns
            .last()
            .unwrap()
            .text
            .starts_with("Continue from"));
    }
    #[test]
    fn steering_is_delivered_only_after_matching_provider_acknowledgement() {
        let mut c = conversation::Conversation::default();
        c.push(
            "user",
            "Keep the title",
            "You",
            Some("codex"),
            "update-1",
            "steering",
        );
        c.push(
            "user",
            "Check the footer too",
            "You",
            Some("codex"),
            "update-unacknowledged",
            "steering",
        );
        let mut snapshot = AgentSession {
            request: "task".into(),
            ..Default::default()
        };
        snapshot.record_activity(atlas_agent::AgentActivity {
            id: "codex-steer:update-1:accepted".into(),
            request: "task".into(),
            kind: atlas_agent::AgentActivityKind::Commentary,
            text: "Update accepted".into(),
            at: 1,
            semantics: None,
        });
        acknowledge_updates(&mut c, &snapshot);
        assert_eq!(c.turns[0].status, "complete");
        assert_eq!(
            c.turns[1].status, "unconfirmed",
            "a finished provider must not leave an update spinning forever"
        );
        assert_eq!(c.replay().len(), 1);
    }
}
