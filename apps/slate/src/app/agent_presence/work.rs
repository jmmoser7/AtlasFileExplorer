use super::*;
use serde_json::json;
use std::time::Duration;

pub(super) fn command_activity(command: &str) -> &'static str {
    match command {
        "board.objects.move" => "moving",
        "board.objects.resize" => "resizing",
        "board.objects.create_frame" => "frames",
        "board.objects.create_text" => "writing",
        "board.objects.wire" => "wiring",
        "board.objects.delete" => "editing",
        "board.objects.order" => "organizing",
        "portal.agent.run" => "creating-image",
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
        for a in &mut self.presence.agents {
            if a.departure.is_some() {
                continue;
            }
            if a.proposed
                .as_ref()
                .is_some_and(|(id, _)| !pending.contains(id))
            {
                a.proposed = None;
                a.pending_created = None;
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
                    {
                        ended.push((
                            a.profile.id.clone(),
                            "Provider ended the task before its board action completed".into(),
                        ));
                    }
                    if let Some(event) = snapshot.activity.last().filter(|e| e.id != a.last_event) {
                        a.last_event = event.id.clone();
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
            if a.proposed.is_some() {
                a.activity = "approval".into();
                a.status = "Proposal ready · accept or reject in chat".into();
            } else if let Some(work) = &a.work {
                a.activity = command_activity(&work.prepared.command).into();
                a.status = if native_view_active {
                    "Waiting · park the native view to continue"
                } else {
                    "Working on the board"
                }
                .into();
            } else if a.read.is_some() {
                a.activity = "reading".into();
            } else if !a.launched.is_empty() {
                a.activity = "tool-waiting".into();
                a.status = "Waiting for the image provider".into();
            }
            for call in self.presence.links.poll_limit(
                &a.dir,
                if !a.paused || a.chat_only {
                    4usize.saturating_sub(calls.len())
                } else {
                    0
                },
            ) {
                calls.push((a.profile.id.clone(), call));
            }
            if a.work.is_none()
                && a.read.is_none()
                && a.launched.is_empty()
                && a.last_active.elapsed().as_secs() > 8
                && !matches!(
                    a.activity.as_str(),
                    "error" | "approval" | "offline" | "blocked" | "question"
                )
            {
                a.activity = if a.paused {
                    "resting"
                } else if a.inflight {
                    "reasoning"
                } else {
                    "resting"
                }
                .into();
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
            let result = self.presence_call(&mut a, &call);
            if let Some(reply) = result {
                let _ = self.presence.links.reply(&a.dir, reply);
            }
            self.presence.agents.insert(index, a);
        }
        let mut read_budget = 256usize;
        for index in 0..self.presence.agents.len() {
            let mut a = self.presence.agents.remove(index);
            if board_visible && a.tab == active_tab && a.departure.is_none() {
                if let Some(reply) = if !a.paused || a.chat_only {
                    self.advance_presence_read(&mut a, &mut read_budget, elapsed)
                } else {
                    None
                } {
                    let _ = self.presence.links.reply(&a.dir, reply);
                }
                if let Some(mut work) = (!a.paused).then(|| a.work.take()).flatten() {
                    let generation = self.doc().scene.scene_gen();
                    let applicable = !self.human_owns_presence_targets(&work.prepared.targets)
                        && work.prepared.tab_id() == active_tab
                        && (work.checked_generation == Some(generation)
                            || work.prepared.expected_matches(&self.doc().scene).is_ok());
                    if applicable {
                        work.checked_generation = Some(generation);
                    }
                    let result = if !applicable {
                        Err("A human changed an action target; preview cancelled".to_owned())
                    } else if let Some(motion) = self.presence.roster.session_mut(&a.motion) {
                        let start = if !work.started {
                            work.begin_step(motion)
                        } else {
                            Ok(())
                        };
                        start.map(|()| {
                            let sample = motion.advance(elapsed);
                            work.preview(sample);
                            if sample.phase == MotionPhase::ReadyToCommit {
                                let _ = motion.resolve_ready(|_| true);
                                work.complete_step();
                            }
                            work.index == work.steps.len()
                        })
                    } else {
                        Err("Session ended".into())
                    };
                    match result {
                        Ok(true) => {
                            let created = work.prepared.created_ids();
                            let created_prepared =
                                (!created.is_empty()).then(|| work.prepared.clone());
                            let generator_target = if work.prepared.command == "portal.agent.run" {
                                work.prepared
                                    .witness_nodes(&self.doc().scene)
                                    .first()
                                    .map(|n| n.id)
                            } else {
                                None
                            };
                            let autonomy = a.grant.as_ref().is_some_and(|g| g.mutation_autonomy);
                            let authorization = a
                                .grant
                                .as_ref()
                                .ok_or("Grant revoked")
                                .and_then(|g| g.authorize(&work.call, &a.binding, &a.request));
                            let outcome = authorization.map_err(str::to_owned).and_then(|()| {
                                self.finish_targeted_command(
                                    work.prepared,
                                    &a.profile.name,
                                    autonomy,
                                    &a.binding.session_id,
                                )
                            });
                            if let Ok(status) = &outcome {
                                if let Some(id) = status.strip_prefix("staged:") {
                                    a.proposed = Some((id.into(), work.previews));
                                    if !created.is_empty() {
                                        a.pending_created = Some(own_chats::CreatedProposal {
                                            id: id.into(),
                                            grant: a.binding.grant_id.clone(),
                                            call: work.call.clone(),
                                            nodes: created.clone(),
                                            prepared: created_prepared.unwrap(),
                                        });
                                    }
                                } else if !created.is_empty() {
                                    if let Err(error) =
                                        self.presence_register_created_chats(&mut a, &created)
                                    {
                                        a.status = error;
                                    }
                                }
                                if let (Some(target), Some(request)) =
                                    (generator_target, status.strip_prefix("queued:"))
                                {
                                    a.launched.push((target, request.into()));
                                }
                            }
                            let reply = match outcome {
                                Ok(status) => {
                                    a.status = if autonomy {
                                        "Action completed"
                                    } else {
                                        "Proposal ready · accept or reject in chat"
                                    }
                                    .into();
                                    a.activity =
                                        if autonomy { "success" } else { "approval" }.into();
                                    BoardReply {
                                        id: work.call.id.clone(),
                                        session_id: a.binding.session_id.clone(),
                                        request_id: a.request.clone(),
                                        success: true,
                                        result: json!({"status":status,"created_ids":created.iter()
                                            .map(|id|id.0.to_string()).collect::<Vec<_>>()}),
                                    }
                                }
                                Err(e) => {
                                    a.note_helper_outcome("blocked", &e);
                                    a.status = e.clone();
                                    a.activity = "blocked".into();
                                    BoardReply::error(&work.call, &e)
                                }
                            };
                            let _ = self.presence.links.reply(&a.dir, reply);
                            a.last_active = now;
                        }
                        Ok(false) => {
                            a.work = Some(work);
                            ctx.request_repaint();
                        }
                        Err(error) => {
                            a.note_helper_outcome("blocked", &error);
                            let _ = self
                                .presence
                                .links
                                .reply(&a.dir, BoardReply::error(&work.call, &error));
                            if let Some(m) = self.presence.roster.session_mut(&a.motion) {
                                m.cancel_motion();
                            }
                            a.status = error;
                            a.activity = "blocked".into();
                        }
                    }
                }
            }
            self.presence.agents.insert(index, a);
        }
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
                        && a.proposed.is_none()
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
