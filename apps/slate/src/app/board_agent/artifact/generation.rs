use super::*;
use slate_doc::agent_inputs::{self, InputKind};

const MAX_IN_FLIGHT: usize = 2;

impl SlateApp {
    pub(crate) fn queue_artifact_generation(&mut self, id: NodeId) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        if !self.is_artifact_node(id) {
            self.toast("Select an artifact before pressing Play.");
            return false;
        }
        if self.doc().scene.node(id).is_some_and(|node| node.locked) {
            self.toast("Unlock this artifact before pressing Play.");
            return false;
        }
        if self.artifact_is_busy(id)
            || self.agents.artifact.active_workers >= MAX_IN_FLIGHT
            || self.agents.artifact.preparing.len() + self.agents.artifact.running.len()
                >= MAX_IN_FLIGHT
        {
            self.toast("Wait for the current artifact generation or stop it first.");
            return false;
        }
        match self.artifact_plan(id) {
            Ok((plan, inputs)) => {
                self.agents.awaiting.remove(&id);
                self.agents.requests.insert(id, plan.request.id.clone());
                self.agents.artifact.pending.remove(&id);
                self.agents.artifact.preparing.insert(
                    id,
                    Preparing {
                        plan,
                        inputs,
                        started: false,
                    },
                );
                true
            }
            Err(error) => {
                self.fail_agent_await(id, error.clone());
                self.toast(error);
                false
            }
        }
    }
    fn artifact_plan(
        &mut self,
        id: NodeId,
    ) -> Result<(Plan, super::super::input_prep::InputPreparation), String> {
        let node = self
            .doc()
            .scene
            .node(id)
            .cloned()
            .ok_or("Artifact is missing")?;
        let binding = artifact(&node).ok_or("Select an artifact")?;
        let agent = slate_doc::agent_chat::agent(&node).ok_or("Choose a harness")?;
        if !atlas_ai::runtime::linear_provider(&agent.provider) {
            return Err("Choose an installed Cursor or Codex harness for the artifact.".into());
        }
        let workspace = self
            .ai
            .config
            .valid_workspace()
            .map(std::path::Path::to_path_buf)
            .ok_or("Set an AI workspace before generating an artifact.")?;
        let graph = &self.doc().scene;
        let edge_count = |kind| {
            graph
                .nodes
                .iter()
                .filter(|n| {
                    let NodeKind::Connector(c) = &n.kind else {
                        return false;
                    };
                    c.binding.as_ref().is_some_and(|b| {
                        b.kind == kind && agent_inputs::endpoint_node(b.target_end(c)) == Some(id)
                    })
                })
                .count()
        };
        let content = agent_inputs::artifact_content_parent(graph, id);
        let memory = agent_inputs::artifact_memory_source(graph, id);
        if edge_count(InputKind::Artifact) > usize::from(content.is_some())
            || edge_count(InputKind::Memory) > usize::from(memory.is_some())
        {
            return Err(
                "The artifact has an invalid or ambiguous content/memory connection.".into(),
            );
        }
        let parent = content
            .as_ref()
            .map(|(source, rev)| self.artifact_reference(*source, rev))
            .transpose()?;
        let memory_ref = memory
            .as_ref()
            .map(|(source, pin)| {
                self.artifact_reference(*source, &pin.revision)
                    .map(|reference| (reference, pin.through))
            })
            .transpose()?;
        let source_agent = memory.as_ref().and_then(|(source, _)| {
            self.doc()
                .scene
                .node(*source)
                .and_then(slate_doc::agent_chat::agent)
        });
        let resume = memory.as_ref().is_some_and(|(source, pin)| {
            source_agent.is_some_and(|a| a.provider == agent.provider)
                && self.agents.sessions.get(source).is_some_and(|s| {
                    s.status == AgentStatus::Idle && s.turns.len() == pin.through && pin.through > 0
                })
                && self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .filter(|node| {
                        slate_doc::agent_chat::agent(node).is_some_and(|a| {
                            source_agent.is_some_and(|source| {
                                a.session == source.session && a.provider == source.provider
                            })
                        })
                    })
                    .filter_map(|node| {
                        self.agents
                            .sessions
                            .get(&node.id)
                            .map(|session| session.turns.len())
                    })
                    .max()
                    == Some(pin.through)
                && !self
                    .agents
                    .artifact
                    .running
                    .values()
                    .any(|r| source_agent.is_some_and(|a| a.session == r.prepared.plan.session))
                && !self
                    .agents
                    .artifact
                    .preparing
                    .values()
                    .any(|p| source_agent.is_some_and(|a| a.session == p.plan.session))
        });
        let session = if resume {
            source_agent.unwrap().session.clone()
        } else {
            slate_doc::scene::new_agent_session_id()
        };
        let link_dir = if resume {
            self.agent_link_dir(memory.as_ref().unwrap().0, &workspace)
                .unwrap_or_else(|| atlas_ai::agent::agent_dir(&workspace, &session))
        } else {
            atlas_ai::agent::agent_dir(&workspace, &session)
        };
        let family = binding.family.clone();
        let history_dir = workspace.join(".atlas-ai/artifact-sources");
        let history_root = binding
            .history_root
            .as_ref()
            .map(|uri| resolve_source(self.tab().path.as_deref(), &uri.locator));
        let inputs = self.agent_input_snapshot(id)?;
        let mut prompt = self
            .agents
            .prompt(id)
            .unwrap_or(&agent.instruction)
            .trim()
            .to_owned();
        if prompt.is_empty() {
            prompt = inputs
                .wired
                .iter()
                .filter(|v| v.port().takes_text())
                .map(|v| v.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
        }
        if prompt.trim().is_empty() {
            return Err("Type feedback or connect a prompt first.".into());
        }
        let tables = inputs
            .wired
            .iter()
            .filter_map(|value| {
                let NodeKind::Image(image) = &self.doc().scene.node(NodeId(value.node))?.kind
                else {
                    return None;
                };
                let path = self.item_path(image.item)?;
                slate_doc::media::is_table_source(&path).then_some((value.node, path))
            })
            .collect();
        let request = AgentRequest {
            id: atlas_ai::agent::request_id(),
            prompt,
            model: agent.model.clone(),
            model_parameters: agent.model_parameters.clone(),
            at: atlas_ai::context::now_secs(),
            ..Default::default()
        };
        let previous = binding.revision.clone();
        let provider = agent.provider.clone();
        let inputs = self.begin_agent_input_preparation(inputs)?;
        Ok((
            Plan {
                tab: self.tab().id,
                node: id,
                request,
                workspace,
                link_dir,
                history_dir,
                history_root,
                session,
                provider,
                family,
                parent,
                memory: memory_ref,
                resume,
                previous,
                tables,
            },
            inputs,
        ))
    }
    pub(crate) fn cancel_artifact_generation(&mut self, id: NodeId) -> bool {
        let preparing = self.agents.artifact.preparing.remove(&id).is_some();
        let running = self.agents.artifact.running.remove(&id).is_some();
        let picking = self.agents.artifact.picks.remove(&id).is_some();
        preparing || running || picking
    }
}

pub(super) fn prepare(mut plan: Plan) -> Result<Prepared, String> {
    if let Some((reference, through)) = &plan.memory {
        let previous = atlas_ai::versions::load_artifact_revision(reference).map_err(|error| {
            format!(
                "Could not load Memory revision {} from {}: {error}",
                reference.revision_id,
                reference.source_root.display()
            )
        })?;
        let checkpoint = &previous.provenance.checkpoint;
        if checkpoint.through != *through || checkpoint.turns.len() != *through {
            return Err("The pinned memory checkpoint is incomplete.".into());
        }
        if plan.resume
            && !atlas_ai::versions::artifact_checkpoint_is_head(
                &plan.link_dir,
                &plan.provider,
                checkpoint,
            )
            .map_err(|error| {
                format!(
                    "Could not inspect the Memory session in {}: {error}",
                    plan.link_dir.display()
                )
            })?
        {
            plan.resume = false;
            plan.session = slate_doc::scene::new_agent_session_id();
            plan.link_dir = atlas_ai::agent::agent_dir(&plan.workspace, &plan.session);
        }
        if !plan.resume {
            plan.request.history = atlas_agent::checkpoint(&checkpoint.turns, Some(*through))
                .map_err(|e| e.to_string())?
                .to_vec();
        }
    }
    let run = if let Some(root) = &plan.history_root {
        atlas_ai::versions::prepare_artifact_run_in_family(
            root,
            &plan.family,
            &plan.request.id,
            plan.parent.clone(),
        )
    } else {
        atlas_ai::versions::prepare_artifact_run(
            &plan.history_dir,
            &plan.family,
            &plan.request.id,
            plan.parent.clone(),
        )
    }
    .map_err(|error| {
        let root = plan.history_root.as_deref().unwrap_or(&plan.history_dir);
        format!(
            "Could not prepare artifact history in {}: {error}",
            root.display()
        )
    })?;
    let outcome = (|| {
        let work = run.source_root.join("work").join(&plan.session);
        std::fs::create_dir_all(&work).map_err(|error| {
            format!(
                "Could not create artifact working directory {}: {error}",
                work.display()
            )
        })?;
        let (inputs, provenance) =
            atlas_ai::versions::freeze_artifact_inputs(&run, &plan.request.inputs, &plan.tables)
                .map_err(|error| {
                    format!(
                        "Could not freeze explicit inputs for {}: {error}",
                        run.staging_output.display()
                    )
                })?;
        plan.request.inputs = inputs;
        plan.request.output_dir = Some(run.staging_output.to_string_lossy().into_owned());
        plan.request.artifact = Some(run.clone());
        Ok(Prepared {
            plan,
            run: run.clone(),
            input_provenance: provenance,
        })
    })();
    if outcome.is_err() {
        let _ = atlas_ai::versions::discard_artifact_run(&run);
    }
    outcome
}
