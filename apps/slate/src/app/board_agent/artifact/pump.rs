use super::completion::{guard_delta, trace};
use super::*;

impl SlateApp {
    pub(crate) fn pump_artifact_generations(&mut self, ctx: &egui::Context) {
        let queued: Vec<_> = self.agents.artifact.queued.keys().copied().collect();
        for id in queued {
            if self.artifact_is_busy(id) || self.agents.artifact.pending.contains_key(&id) {
                continue;
            }
            if self.agents.artifact.preparing.len() + self.agents.artifact.running.len() >= 2 {
                break;
            }
            let Some((plan, inputs)) = self
                .agents
                .artifact
                .queued
                .get_mut(&id)
                .and_then(|queue| queue.pop_front())
            else {
                self.agents.artifact.queued.remove(&id);
                continue;
            };
            if !self.artifact_target_current(plan.tab, id, &plan.previous)
                || self.artifact_plan_guard(&plan).as_deref() != Some(plan.guard.as_str())
            {
                trace(
                    "drop.prepare",
                    plan.tab,
                    id,
                    &plan.request.id,
                    "source-or-target-changed",
                );
                self.agents.artifact.queued.remove(&id);
                self.fail_current_artifact_phase(plan.tab, id, &plan.request.id,
                    "The artifact inputs or target changed during preparation. Run again with the current inputs.");
                continue;
            }
            trace(
                "prepare.accepted",
                plan.tab,
                id,
                &plan.request.id,
                "queued-inputs",
            );
            self.agents.requests.insert(id, plan.request.id.clone());
            self.agents.artifact.preparing.insert(
                id,
                Preparing {
                    plan,
                    inputs,
                    started: false,
                },
            );
        }
        let ids = self
            .agents
            .artifact
            .preparing
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in ids {
            let Some(mut pending) = self.agents.artifact.preparing.remove(&id) else {
                continue;
            };
            if pending.plan.tab != self.tab().id
                || !self.is_artifact_node(id)
                || self.tab().read_only
            {
                trace(
                    "drop.prepare",
                    pending.plan.tab,
                    id,
                    &pending.plan.request.id,
                    "inactive-or-unavailable-target",
                );
                self.fail_current_artifact_phase(pending.plan.tab, id, &pending.plan.request.id,
                    "The artifact target changed during preparation. Run again with the current target.");
                continue;
            }
            if !pending.started && self.agents.artifact.active_workers < 2 {
                match self.poll_agent_input_preparation(&mut pending.inputs) {
                    Ok(true) => {
                        pending.plan.request.inputs = pending.inputs.inputs.clone();
                        let since = self
                            .agents
                            .capture_wait
                            .get(&id)
                            .copied()
                            .unwrap_or_else(Instant::now);
                        match self.capture_prepared_generator_inputs(
                            id,
                            &mut pending.inputs,
                            &mut pending.plan.request,
                            since.elapsed() > CAPTURE_TIMEOUT,
                        ) {
                            Ok(true) => {}
                            Ok(false) => {
                                let since = *self
                                    .agents
                                    .capture_wait
                                    .entry(id)
                                    .or_insert_with(Instant::now);
                                if since.elapsed() > CAPTURE_TIMEOUT {
                                    self.agents.capture_wait.remove(&id);
                                    self.agents.artifact.queued.remove(&id);
                                    self.fail_agent_await(id, "The connected model, video or web page did not finish loading.".into());
                                    continue;
                                }
                                self.agents.artifact.preparing.insert(id, pending);
                                continue;
                            }
                            Err(error) => {
                                self.fail_agent_await(id, error);
                                continue;
                            }
                        }
                        self.agents.capture_wait.remove(&id);
                        let plan = pending.plan.clone();
                        let tx = self.artifact_work_tx();
                        self.agents.artifact.active_workers += 1;
                        std::thread::spawn(move || {
                            let tag = (plan.tab, plan.node, plan.request.id.clone());
                            let result = super::generation::prepare(plan);
                            let _ =
                                tx.send(WorkDone::Prepared(tag.0, tag.1, tag.2, Box::new(result)));
                        });
                        pending.started = true;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.fail_agent_await(id, error);
                        continue;
                    }
                }
            }
            self.agents.artifact.preparing.insert(id, pending);
        }
        for _ in 0..8 {
            let Some(done) = self
                .agents
                .artifact
                .work
                .as_ref()
                .and_then(|(_, rx)| rx.try_recv().ok())
            else {
                break;
            };
            self.agents.artifact.active_workers =
                self.agents.artifact.active_workers.saturating_sub(1);
            match done {
                WorkDone::Prepared(tab, id, request, result) => {
                    let result = *result;
                    trace(
                        "prepare.result",
                        tab,
                        id,
                        &request,
                        if result.is_ok() { "ok" } else { "error" },
                    );
                    let expected = self
                        .agents
                        .artifact
                        .preparing
                        .get(&id)
                        .is_some_and(|p| p.plan.tab == tab && p.plan.request.id == request);
                    if !expected {
                        if let Ok(prepared) = result {
                            discard_prepared(prepared);
                        }
                        continue;
                    }
                    self.agents.artifact.preparing.remove(&id);
                    match result {
                        Ok(prepared)
                            if self.artifact_target_current(tab, id, &prepared.plan.previous) =>
                        {
                            self.start_prepared_artifact(prepared)
                        }
                        Ok(prepared) => {
                            trace("drop.target", tab, id, &request, "prepared-target-changed");
                            self.fail_current_artifact_phase(tab, id, &request,
                                "The artifact target changed before dispatch. Run again with the current target.");
                            discard_prepared(prepared);
                        }
                        Err(error) => self.fail_current_artifact_phase(tab, id, &request, &error),
                    }
                }
                WorkDone::Published(tab, id, request, result) => {
                    self.finish_artifact_publication(tab, id, request, result);
                }
                WorkDone::Picked(tab, id, token, expected, remove, lease, result) => {
                    let current = self.agents.artifact.picks.get(&id) == Some(&token);
                    if current {
                        self.agents.artifact.picks.remove(&id);
                    }
                    let valid = current
                        && self.tab().id == tab
                        && !self.tab().read_only
                        && !expected.locked
                        && self.doc().scene.node(id) == Some(expected.as_ref());
                    if let Some(lease) = lease {
                        let command = if valid {
                            result.and_then(|(reference, revision)| {
                                let pending = Pending {
                                    tab,
                                    previous: artifact(&expected).and_then(|a| a.revision.clone()),
                                    reference,
                                    revision,
                                };
                                self.prepare_artifact_output_patch(id, &pending, false, remove)
                            })
                        } else {
                            Err("Saved output target changed or preparation was cancelled".into())
                        };
                        self.complete_presence_artifact_pick(*lease, command);
                    } else if valid {
                        let previous =
                            artifact(&expected).and_then(|binding| binding.revision.clone());
                        match result {
                            Ok((reference, revision)) => {
                                self.agents.artifact.pending.insert(
                                    id,
                                    Pending {
                                        tab,
                                        previous,
                                        reference,
                                        revision,
                                    },
                                );
                                self.accept_artifact_output_from(id, false, remove);
                            }
                            Err(error) => self.fail_agent_await(id, error),
                        }
                    }
                }
            }
            ctx.request_repaint();
        }
        let ids = self
            .agents
            .artifact
            .running
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in ids {
            let Some(running) = self.agents.artifact.running.get(&id) else {
                continue;
            };
            let subscription =
                atlas_ai::runtime::subscription_provider(&running.prepared.plan.provider).is_some();
            let target_current = if subscription {
                self.subscription_artifact_target_current(&running.prepared.plan)
            } else {
                self.artifact_target_current(
                    running.prepared.plan.tab,
                    id,
                    &running.prepared.plan.previous,
                )
            };
            if !target_current {
                let plan = &running.prepared.plan;
                if subscription
                    && self
                        .agent_runtime_in(plan.tab)
                        .and_then(|runtime| runtime.request_of(id))
                        == Some(plan.request.id.as_str())
                {
                    // Fence the exact session before removing its app witness.
                    // Already-admitted writes may finish, but cannot publish.
                    if let Some(runtime) = self.agents.codex.get(&plan.session) {
                        runtime.stop_with_reason(
                            "The artifact target changed; output remains staged.".into(),
                        );
                    }
                }
                trace(
                    "drop.target",
                    plan.tab,
                    id,
                    &plan.request.id,
                    "running-target-changed",
                );
                let (tab, request) = (plan.tab, plan.request.id.clone());
                self.agents.artifact.running.remove(&id);
                let reason = if subscription {
                    "The artifact target or model changed during generation. Run again with the current target."
                } else {
                    "The artifact target changed during generation. Run again with the current target."
                };
                self.fail_current_artifact_phase(tab, id, &request, reason);
                continue;
            }
            if let Some(error) = self.agent_failure_reason(id).map(str::to_owned) {
                if let Some(running) = self.agents.artifact.running.remove(&id) {
                    let plan = &running.prepared.plan;
                    trace(
                        "drop.failed",
                        plan.tab,
                        id,
                        &plan.request.id,
                        "existing-await-failure",
                    );
                    if !subscription {
                        discard_prepared(running.prepared);
                    }
                }
                self.toast(error);
                ctx.request_repaint();
                continue;
            }
            if running.publishing || self.agents.artifact.active_workers >= 2 {
                continue;
            }
            let request = running.prepared.plan.request.id.clone();
            let Some(session) = self
                .agents
                .sessions
                .get(&id)
                .filter(|s| s.request == request)
                .cloned()
            else {
                continue;
            };
            if let AgentStatus::Error(error) = &session.status {
                self.agents.artifact.running.remove(&id);
                self.fail_agent_await(id, error.clone());
                continue;
            }
            if session.status != AgentStatus::Idle {
                continue;
            }
            self.start_artifact_publication(id, &session);
        }
        if !self.agents.artifact.preparing.is_empty()
            || !self.agents.artifact.running.is_empty()
            || !self.agents.artifact.queued.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
    pub(super) fn artifact_target_current(
        &self,
        tab: u64,
        id: NodeId,
        previous: &Option<String>,
    ) -> bool {
        self.tab().id == tab
            && !self.tab().read_only
            && self
                .doc()
                .scene
                .node(id)
                .is_some_and(|n| !n.locked && artifact(n).is_some_and(|a| &a.revision == previous))
    }
    fn start_prepared_artifact(&mut self, mut prepared: Prepared) {
        let plan = &prepared.plan;
        let id = plan.node;
        if self.artifact_plan_guard(plan).as_deref() != Some(plan.guard.as_str()) {
            trace(
                "drop.dispatch",
                plan.tab,
                id,
                &plan.request.id,
                &guard_delta(&plan.guard, self.artifact_plan_guard(plan).as_deref()),
            );
            self.agents.artifact.queued.remove(&id);
            discard_prepared(prepared);
            self.fail_agent_await(id, "The artifact prompt or connected sources changed during preparation. Send the updated request again.".into());
            return;
        }
        if !self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|agent| {
                agent.provider == plan.provider
                    && agent.model == plan.request.model
                    && agent.model_parameters == plan.request.model_parameters
            })
        {
            discard_prepared(prepared);
            self.fail_agent_await(id, "The artifact harness or model changed during preparation. Run again with the selected model.".into());
            return;
        }
        let base = self.tab().path.as_deref();
        let workspace = SourceUri {
            locator: super::super::super::board_portal::source_locator(
                base,
                &prepared.run.source_root.join("work").join(&plan.session),
            ),
        };
        let bundle = SourceUri {
            locator: super::super::super::board_portal::source_locator(
                base,
                &plan.link_dir.join("session.json"),
            ),
        };
        // The submitted request owns its immutable prompt. The editor may
        // already hold feedback for the next run while preparation finishes.
        let instruction = self
            .agents
            .prompt(id)
            .or_else(|| {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(slate_doc::agent_chat::agent)
                    .map(|agent| agent.instruction.as_str())
            })
            .unwrap_or("")
            .to_owned();
        self.patch_nodes(&[id], |node| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(node) {
                a.session.clone_from(&plan.session);
                a.channel = None;
                a.bundle = Some(bundle.clone());
                a.instruction.clone_from(&instruction);
                a.chat = Default::default();
            }
            if let Some(a) = artifact_mut(node) {
                a.workspace = Some(workspace.clone());
            }
        });
        let request = prepared.plan.request.clone();
        let ws = prepared.plan.workspace.clone();
        let submitted = prepared.plan.authored_prompt.clone();
        let clear_composer = !prepared.plan.accepted_batch
            && self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|agent| {
                    agent.generator.preserve_history.unwrap_or(true)
                        && self.agents.prompt(id).unwrap_or(&agent.instruction) == submitted
                });
        let running_guard = self
            .artifact_batch_guard_with_prompt(id, Some(&submitted))
            .unwrap_or_default();
        if self.submit_prepared_agent_request(id, request, &ws) {
            trace(
                "dispatch.accepted",
                plan.tab,
                id,
                &plan.request.id,
                "native-request",
            );
            prepared.plan.guard = running_guard;
            prepared.plan.accepted_batch = true;
            if let Some(queue) = self.agents.artifact.queued.get_mut(&id) {
                for (sibling, _) in queue {
                    sibling.accepted_batch = true;
                }
            }
            self.agents.artifact.running.insert(
                id,
                Running {
                    prepared,
                    publishing: false,
                },
            );
            if clear_composer {
                self.agents.prompt_mut(id).clear();
                self.clear_pending_generator_prompt(id);
                self.patch_nodes(&[id], |node| {
                    if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                        agent.instruction.clear();
                    }
                });
                self.sync_instruction(id, "");
            }
        } else {
            trace(
                "dispatch.failed",
                plan.tab,
                id,
                &plan.request.id,
                "submission-refused",
            );
            discard_prepared(prepared);
        }
    }
}

fn discard_prepared(prepared: Prepared) {
    std::thread::spawn(move || {
        let _ = atlas_ai::versions::discard_artifact_run(&prepared.run);
    });
}
