use super::*;

impl SlateApp {
    pub(super) fn artifact_batch_guard(&self, id: NodeId) -> Option<String> {
        let node = self.doc().scene.node(id)?;
        let agent = slate_doc::agent_chat::agent(node)?;
        let source = match &node.kind {
            NodeKind::Portal(portal) => &portal.source,
            _ => return None,
        };
        let inputs: Vec<_> =
            self.doc()
                .scene
                .nodes
                .iter()
                .filter_map(|node| {
                    let NodeKind::Connector(conn) = &node.kind else {
                        return None;
                    };
                    let binding = conn.binding.as_ref()?;
                    (slate_doc::agent_inputs::endpoint_node(binding.target_end(conn)) == Some(id))
                        .then(|| {
                            (
                                node.id,
                                (
                                    binding.kind,
                                    &binding.output,
                                    &binding.order,
                                    binding.all_images,
                                    &binding.slot,
                                ),
                                slate_doc::agent_inputs::endpoint_node(binding.source_end(conn)),
                            )
                        })
                })
                .collect();
        let prompt = self.agents.prompt(id).unwrap_or(&agent.instruction).trim();
        let captured = self.agent_input_snapshot(id).ok()?;
        let values: Vec<_> = captured
            .wired
            .iter()
            .map(|item| (item.node, &item.text, &item.images, &item.depth, item.slot))
            .collect();
        serde_json::to_string(&(
            prompt,
            inputs,
            values,
            source,
            &artifact(node)?.revision,
            &agent.provider,
            &agent.model,
            &agent.model_parameters,
            (
                agent.generator.count,
                agent.generator.preserve_history,
                &agent.generator.grafted_slots,
                agent.generator.history_cut,
            ),
            node.locked,
        ))
        .ok()
    }

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
                || self.artifact_batch_guard(id).as_deref() != Some(plan.guard.as_str())
            {
                self.agents.artifact.queued.remove(&id);
                continue;
            }
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
                            discard_prepared(prepared);
                        }
                        Err(error) => self.fail_agent_await(id, error),
                    }
                }
                WorkDone::Published(tab, id, request, result) => {
                    let matches = self.agents.artifact.running.get(&id).is_some_and(|r| {
                        r.prepared.plan.request.id == request && r.prepared.plan.tab == tab
                    });
                    if !matches {
                        continue;
                    }
                    let guard = self
                        .agents
                        .artifact
                        .running
                        .get(&id)
                        .map(|r| r.prepared.plan.guard.clone());
                    let current = self.artifact_batch_guard(id);
                    self.agents.artifact.running.remove(&id);
                    if guard != current {
                        self.agents.artifact.queued.remove(&id);
                        continue;
                    }
                    match result {
                        Ok(pending) if self.artifact_target_current(tab, id, &pending.previous) => {
                            self.agents.artifact.pending.insert(id, pending);
                            if self.accept_artifact_output_from(id, true, None) {
                                let next_guard = self.artifact_batch_guard(id).unwrap_or_default();
                                let next_revision = self
                                    .doc()
                                    .scene
                                    .node(id)
                                    .and_then(artifact)
                                    .and_then(|a| a.revision.clone());
                                if let Some(queue) = self.agents.artifact.queued.get_mut(&id) {
                                    for (plan, _) in queue {
                                        plan.previous = next_revision.clone();
                                        plan.guard = next_guard.clone();
                                    }
                                }
                            } else {
                                self.agents.artifact.queued.remove(&id);
                            }
                        }
                        Ok(_) => {}
                        Err(error) => self.fail_agent_await(id, error),
                    }
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
            if !self.artifact_target_current(
                running.prepared.plan.tab,
                id,
                &running.prepared.plan.previous,
            ) {
                self.agents.artifact.running.remove(&id);
                continue;
            }
            if let Some(error) = self.agent_failure_reason(id).map(str::to_owned) {
                if let Some(running) = self.agents.artifact.running.remove(&id) {
                    discard_prepared(running.prepared);
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
            let running = self.agents.artifact.running.get_mut(&id).unwrap();
            running.publishing = true;
            let run = running.prepared.run.clone();
            let plan = running.prepared.plan.clone();
            let provenance = ArtifactProvenance {
                prompt: plan.request.prompt.clone(),
                provider: plan.provider.clone(),
                model: plan
                    .request
                    .model
                    .clone()
                    .unwrap_or_else(|| "provider-default".into()),
                checkpoint: ArtifactCheckpoint {
                    conversation: plan.session.clone(),
                    through: session.turns.len(),
                    turns: session.turns.clone(),
                },
                inputs: running.prepared.input_provenance.clone(),
            };
            let tx = self.artifact_work_tx();
            self.agents.artifact.active_workers += 1;
            std::thread::spawn(move || {
                let result = (|| {
                    validate_return(&plan, &run)?;
                    let (reference, revision) =
                        atlas_ai::versions::publish_artifact_revision(&run, provenance)
                            .map_err(|e| e.to_string())?;
                    Ok(Pending {
                        tab: plan.tab,
                        previous: plan.previous.clone(),
                        reference,
                        revision,
                    })
                })();
                let _ = tx.send(WorkDone::Published(plan.tab, plan.node, request, result));
            });
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
    fn start_prepared_artifact(&mut self, prepared: Prepared) {
        let plan = &prepared.plan;
        let id = plan.node;
        if self.artifact_batch_guard(id).as_deref() != Some(plan.guard.as_str()) {
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
        let instruction = plan.authored_prompt.clone();
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
        if self.submit_prepared_agent_request(id, request, &ws) {
            self.agents.artifact.running.insert(
                id,
                Running {
                    prepared,
                    publishing: false,
                },
            );
        } else {
            discard_prepared(prepared);
        }
    }
}

fn validate_return(plan: &Plan, run: &ArtifactRun) -> Result<(), String> {
    atlas_ai::versions::validate_artifact_return(run, &plan.link_dir)
        .map_err(|error| error.to_string())
}

fn discard_prepared(prepared: Prepared) {
    std::thread::spawn(move || {
        let _ = atlas_ai::versions::discard_artifact_run(&prepared.run);
    });
}
