#[path = "work_advance.rs"]
mod advance;
use super::*;
use std::time::Duration;

pub(super) fn command_activity(command: &str) -> &'static str {
    match command {
        "board.objects.move" => "moving",
        "board.objects.resize" => "resizing",
        "board.objects.crop" => "resizing",
        "board.objects.create_frame" => "frames",
        "board.objects.create_text" | "board.objects.set_text" => "writing",
        "board.objects.wire" => "wiring",
        "board.objects.delete" | "board.shape.edit" | "board.wire.edit" => "editing",
        "board.objects.order" => "organizing",
        "portal.agent.run" => "creating-image",
        "presence.send" => "exchanging",
        _ => "working",
    }
}

impl SlateApp {
    pub(crate) fn presence_pump(&mut self, ctx: &egui::Context) {
        self.presence_release_avatar_hold(ctx, None, false);
        self.presence.conversations.poll();
        self.pump_presence_people(ctx);
        let now = Instant::now();
        let elapsed = self
            .presence
            .last_tick
            .replace(now)
            .map_or(Duration::ZERO, |t| now.duration_since(t));
        let pointer_press = ctx.input(|i| i.pointer.primary_pressed());
        let active_tab = self.tab().id;
        let native_view_active = self.enscape_shown_node().is_some();
        let board_visible = !self.at_home
            && self.doc().view.active_view == slate_doc::ViewKind::Board
            && self.presenting.is_none()
            && self.portal_chrome.maximized.is_none()
            // A live foreign HWND can cover egui presence. Give its human
            // interaction priority until the native view is parked.
            && !native_view_active
            && !ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let vanished: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| a.departure.is_none() && !self.tabs.iter().any(|t| t.id == a.tab))
            .map(|a| a.profile.id.clone())
            .collect();
        for who in vanished {
            let _ = self.retire_presence(&who, "bow");
        }
        let mut calls = Vec::new();
        let mut ended = Vec::new();
        let pending: std::collections::HashSet<_> = self
            .pending_stage_proposals()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let vanished_proposals: Vec<_> = self
            .presence
            .proposals
            .keys()
            .filter(|id| !pending.contains(*id))
            .cloned()
            .collect();
        for id in vanished_proposals {
            self.presence_end_proposal_motion(&id, true);
        }
        let pending_agents: std::collections::HashSet<_> = self
            .presence
            .proposals
            .values()
            .map(|p| p.participant.clone())
            .collect();
        let listening_actions: std::collections::HashSet<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| self.presence.approved_while_listening(a))
            .map(|a| a.profile.id.clone())
            .collect();
        for a in &mut self.presence.agents {
            if a.departure.is_some() {
                continue;
            }
            if let Some(error) = a.cursor.as_ref().and_then(|c| c.take_error()) {
                ended.push((a.profile.id.clone(), error));
            }
            if let Some(snapshot) = self.presence.sources.poll(&a.dir, None) {
                if snapshot.request == a.request
                    && !a.request.is_empty()
                    && !a
                        .snapshot
                        .as_ref()
                        .is_some_and(|previous| Arc::ptr_eq(previous, &snapshot))
                {
                    let mut current_turn = false;
                    if let Ok(thread) = self.presence.conversations.edit(&a.profile.id) {
                        current_turn = thread.reply_start(a, &snapshot).is_some();
                        if thread.absorb(a, &snapshot) {
                            self.presence.conversations.dirty(&a.profile.id);
                        }
                    }
                    if a.inflight
                        && matches!(
                            snapshot.status,
                            atlas_agent::AgentStatus::Error(_) | atlas_agent::AgentStatus::Offline
                        )
                    {
                        ended.push((
                            a.profile.id.clone(),
                            match &snapshot.status {
                                atlas_agent::AgentStatus::Error(e) => e.clone(),
                                _ => "Sidecar offline".into(),
                            },
                        ));
                    }
                    if matches!(snapshot.status, atlas_agent::AgentStatus::Idle)
                        && (a.work.is_some() || a.read.is_some())
                        && a.seen_running
                        && !a.work.as_ref().is_some_and(|w| {
                            matches!(w.purpose, witness::WitnessPurpose::ApprovedProposal(_))
                        })
                    {
                        ended.push((
                            a.profile.id.clone(),
                            "Provider ended the task before its board action completed".into(),
                        ));
                    }
                    // Public commentary can grow under one event ID. Compare
                    // its bounded observation, not a snapshot heartbeat or ID.
                    if let Some(event) = snapshot
                        .activity
                        .last()
                        .filter(|event| a.last_event.as_ref() != Some(*event))
                    {
                        a.last_event = Some(event.clone());
                        a.last_active = now;
                        a.activity = event
                            .semantics
                            .as_ref()
                            .map(|s| activity_id(s.action))
                            .unwrap_or_else(|| "working".into());
                        a.observed_target = event
                            .semantics
                            .as_ref()
                            .and_then(|s| s.target.as_ref())
                            .and_then(|t| t.node_id.parse::<u64>().ok())
                            .map(slate_doc::NodeId);
                        if matches!(event.kind, atlas_agent::AgentActivityKind::ToolFinished)
                            && !matches!(a.activity.as_str(), "error" | "cancel" | "blocked")
                        {
                            a.activity = "working".into();
                        }
                    }
                    a.status = match &snapshot.status {
                        atlas_agent::AgentStatus::Thinking => "Working".into(),
                        atlas_agent::AgentStatus::Idle => "Ready".into(),
                        atlas_agent::AgentStatus::Offline => "Sidecar offline".into(),
                        atlas_agent::AgentStatus::Error(e) => e.chars().take(160).collect(),
                    };
                    if matches!(snapshot.status, atlas_agent::AgentStatus::Thinking) {
                        a.seen_running = true;
                    }
                    if snapshot.approval.is_some() {
                        a.activity = "approval".into();
                        a.status = "Provider approval required; open its chat".into();
                    } else if let atlas_agent::AgentStatus::Error(reason) = &snapshot.status {
                        a.note_helper_outcome("error", reason);
                        a.activity = "error".into();
                        a.inflight = false;
                    } else if matches!(snapshot.status, atlas_agent::AgentStatus::Offline) {
                        a.note_helper_outcome("offline", "Sidecar offline");
                        a.activity = "offline".into();
                        a.inflight = false;
                    } else if matches!(snapshot.status, atlas_agent::AgentStatus::Idle)
                        && !a.had_response
                        && a.inflight
                        && current_turn
                        && (a.seen_running
                            || !snapshot.activity.is_empty()
                            || snapshot.turns.last().is_some_and(|t| t.role == "assistant"))
                    {
                        a.had_response = true;
                        a.inflight = false;
                        a.activity = "success".into();
                        a.note_helper_outcome("success", "Ready");
                        a.last_active = now;
                    }
                    a.snapshot = Some(snapshot);
                }
            }
            for (target, request) in &a.launched {
                if let Some(session) = self
                    .agents
                    .session(*target)
                    .filter(|s| s.request == *request)
                {
                    if matches!(
                        session.status,
                        atlas_agent::AgentStatus::Error(_) | atlas_agent::AgentStatus::Offline
                    ) {
                        ended.push((a.profile.id.clone(), "This task's image provider failed; the queued harness change was canceled".into()));
                    }
                }
            }
            a.launched.retain(|(target, request)| {
                self.agents.request_of(*target) == Some(request.as_str())
                    && self.agents.session(*target).is_none_or(|s| {
                        s.request != *request
                            || matches!(s.status, atlas_agent::AgentStatus::Thinking)
                    })
            });
            if a.work.is_none() && matches!(a.activity.as_str(), "error" | "offline" | "blocked") {
                // Keep the real diagnostic visible until another action or
                // an explicit human control changes it.
            } else if a.paused && !listening_actions.contains(&a.profile.id) {
                a.activity = "listening".into();
                a.status = "Listening; task suspended".into();
            } else if let Some(work) = &a.work {
                a.activity = command_activity(work.command()).into();
                a.status = if native_view_active {
                    "Waiting · park the native view to continue"
                } else if work.purpose == witness::WitnessPurpose::ProposalPreview {
                    "Proposed preview · board unchanged"
                } else if matches!(work.purpose, witness::WitnessPurpose::ApprovedProposal(_)) {
                    "Applying approved change visibly"
                } else {
                    "Working on the board"
                }
                .into();
            } else if pending_agents.contains(&a.profile.id) {
                a.activity = "approval".into();
                a.status = "Proposal ready · accept or reject in chat".into();
            } else if a.read.is_some() {
                a.activity = "reading".into();
            } else if !a.launched.is_empty() {
                a.activity = "tool-waiting".into();
                a.status = "Waiting for the image provider".into();
            }
            for call in self.presence.links.poll_limit(
                &a.dir,
                if !pointer_press && (!a.paused || (a.chat_only && a.work.is_none())) {
                    4usize.saturating_sub(calls.len())
                } else {
                    0
                },
            ) {
                calls.push((a.profile.id.clone(), call));
            }
            // A human listening pause outlives provider-observation freshness.
            if !a.paused
                && a.work.is_none()
                && a.read.is_none()
                && a.launched.is_empty()
                && a.last_active.elapsed().as_secs() > 8
                && !matches!(
                    a.activity.as_str(),
                    "error" | "approval" | "offline" | "blocked" | "question"
                )
            {
                a.activity = if a.inflight { "reasoning" } else { "resting" }.into();
            }
        }
        for (who, reason) in ended {
            if let Some(a) = self
                .presence
                .agents
                .iter_mut()
                .find(|a| a.profile.id == who)
            {
                a.note_helper_outcome("error", &reason);
            }
            let _ = self.stop_presence(&who, false);
            if let Some(a) = self
                .presence
                .agents
                .iter_mut()
                .find(|a| a.profile.id == who)
            {
                a.status = reason;
                a.activity = "error".into();
            }
        }
        for (who, call) in calls {
            let Some(index) = self
                .presence
                .agents
                .iter()
                .position(|a| a.profile.id == who)
            else {
                continue;
            };
            let mut a = self.presence.agents.remove(index);
            let result = self.presence_call_with_context(ctx, &mut a, &call);
            if let Some(reply) = result {
                let _ = self.presence.links.reply(&a.dir, reply);
            }
            self.presence.agents.insert(index, a);
        }
        self.advance_presence_actions(ctx, board_visible, active_tab, pointer_press, elapsed, now);
        let helpers: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| {
                a.parent.is_some()
                    && a.departure.is_none()
                    && ((a.had_response
                        && !a.inflight
                        && a.work.is_none()
                        && !self.presence.has_pending_for(a)
                        && a.read.is_none()
                        && a.launched.is_empty())
                        || a.joined_at.elapsed().as_secs() > helper_scope::TASK_LIMIT_SECS)
            })
            .map(|a| a.profile.id.clone())
            .collect();
        for who in helpers {
            let _ = self.retire_completed_helper(&who);
        }
        self.finish_presence_handoffs();
        self.pump_presence_mentions(board_visible && !pointer_press);
        let forgotten: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| {
                a.parent.is_some()
                    && a.departure.as_ref().is_some_and(|(t, style, _)| {
                        t.elapsed().as_secs_f64() >= chrome::departure_duration(style) + 0.1
                    })
            })
            .map(|a| a.profile.id.clone())
            .collect();
        for who in forgotten {
            if let Ok(id) = id(&who) {
                let _ = self.presence.roster.forget_retired_profile(&id);
            }
        }
        self.presence.agents.retain(|a| {
            a.departure.as_ref().is_none_or(|(t, style, _)| {
                t.elapsed().as_secs_f64() < chrome::departure_duration(style) + 0.1
            })
        });
        let dirs = self.presence.agents.iter().map(|a| a.dir.clone()).collect();
        self.presence.links.retain(&dirs);
        self.presence.sources.retain(&dirs);
        let ids: Vec<_> = self
            .presence
            .agents
            .iter()
            .flat_map(|a| [a.motion.as_str(), a.profile.id.as_str()])
            .collect();
        self.presence.painter.retain(&ids);
        self.presence.refresh_held(active_tab);
        self.presence
            .helper_results
            .retain(|r| r.retired_at.elapsed().as_secs() < 900);
        self.presence.refresh_helper_scope();
        self.pump_presence_schedules(ctx);
        if self
            .presence
            .agents
            .iter()
            .any(|a| a.read.is_some() || a.departure.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if self
            .presence
            .agents
            .iter()
            .any(|a| a.inflight || !a.launched.is_empty())
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if !self.presence.agents.is_empty() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        if self.presence.conversations.status().is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
