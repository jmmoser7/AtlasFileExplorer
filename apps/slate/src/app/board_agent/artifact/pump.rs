use super::*;

impl SlateApp {
    pub(crate) fn pump_artifact_generations(&mut self, ctx: &egui::Context) {
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
                    self.agents.artifact.running.remove(&id);
                    match result {
                        Ok(pending) if self.artifact_target_current(tab, id, &pending.previous) => {
                            self.agents.artifact.pending.insert(id, pending);
                            self.toast("Artifact preview ready. Apply keeps this revision; Discard retains the accepted source.");
                        }
                        Ok(_) => {}
                        Err(error) => self.fail_agent_await(id, error),
                    }
                }
                WorkDone::Picked(tab, id, token, previous, result) => {
                    if self.agents.artifact.picks.get(&id) != Some(&token) {
                        continue;
                    }
                    self.agents.artifact.picks.remove(&id);
                    if !self.artifact_target_current(tab, id, &previous) {
                        continue;
                    }
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
                            self.accept_artifact_output(id);
                        }
                        Err(error) => self.fail_agent_await(id, error),
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
        if !self.agents.artifact.preparing.is_empty() || !self.agents.artifact.running.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
    fn artifact_target_current(&self, tab: u64, id: NodeId, previous: &Option<String>) -> bool {
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
            .unwrap_or(&plan.request.prompt)
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
    pub(crate) fn accept_artifact_output(&mut self, id: NodeId) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(pending) = self.agents.artifact.pending.get(&id) else {
            self.toast("There is no artifact preview to apply. Play to build one first.");
            return false;
        };
        if !self.artifact_target_current(pending.tab, id, &pending.previous) {
            self.toast(
                "This preview belongs to an earlier artifact state. Discard it and Play again.",
            );
            return false;
        }
        let pending = self.agents.artifact.pending.remove(&id).unwrap();
        let before = self.doc().scene.node(id).unwrap().clone();
        let mut after = before.clone();
        let base = self.tab().path.as_deref();
        let source = SourceUri {
            locator: super::super::super::board_portal::source_locator(
                base,
                &pending.reference.materialized_entry,
            ),
        };
        let root = SourceUri {
            locator: super::super::super::board_portal::source_locator(
                base,
                &pending.reference.source_root,
            ),
        };
        if let NodeKind::Portal(p) = &mut after.kind {
            p.source = Some(source);
        }
        let a = artifact_mut(&mut after).unwrap();
        a.family = pending.revision.family_id.clone();
        a.history_root = Some(root);
        a.revision = Some(pending.reference.revision_id.clone());
        if !a.revisions.contains(&pending.reference.revision_id) {
            a.revisions.push(pending.reference.revision_id);
        }
        a.checkpoint = pending.revision.provenance.checkpoint.through;
        a.packaged = None;
        let cmds = vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }];
        let applied = self.commit_scene_as(
            cmds,
            slate_doc::scene::CmdAuthor::Agent(pending.revision.provenance.provider),
        );
        if applied {
            self.toast(
                "Artifact preview applied. This revision is now available to downstream artifacts.",
            );
        }
        applied
    }
    pub(crate) fn reject_artifact_output(&mut self, id: NodeId) -> bool {
        if self.agents.artifact.pending.remove(&id).is_some() {
            self.toast("Artifact preview discarded. The accepted revision is unchanged.");
            true
        } else {
            self.toast("There is no artifact preview to discard.");
            false
        }
    }
    pub(crate) fn artifact_pick_revision(&mut self, id: NodeId, revision: &str) -> bool {
        if self.tab().read_only
            || self.agents.artifact.active_workers >= 2
            || self.agents.artifact.preparing.contains_key(&id)
            || self.agents.artifact.running.contains_key(&id)
        {
            self.toast("Wait for artifact preparation or generation to finish before selecting a revision.");
            return false;
        }
        let reference = match self.artifact_reference(id, revision) {
            Ok(r) => r,
            Err(e) => {
                self.toast(e);
                return false;
            }
        };
        let tab = self.tab().id;
        let previous = self
            .doc()
            .scene
            .node(id)
            .and_then(artifact)
            .and_then(|a| a.revision.clone());
        let tx = self.artifact_work_tx();
        let token = atlas_ai::agent::request_id();
        self.agents.artifact.picks.insert(id, token.clone());
        self.agents.artifact.active_workers += 1;
        std::thread::spawn(move || {
            let result = (|| {
                atlas_ai::versions::materialize_artifact_revision(&reference)
                    .map_err(|e| e.to_string())?;
                let manifest = atlas_ai::versions::load_artifact_revision(&reference)
                    .map_err(|e| e.to_string())?;
                Ok((reference, manifest))
            })();
            let _ = tx.send(WorkDone::Picked(tab, id, token, previous, result));
        });
        true
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
