use super::*;
use atlas_shell::selection_tools::generator as chrome;

struct ArtifactHistory<'a, T>(&'a [T]);

impl<T> chrome::GeneratorHistory for ArtifactHistory<'_, T> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&self, index: usize) -> Option<chrome::GeneratorHistoryItem<'_>> {
        self.0.get(index)?;
        Some(chrome::GeneratorHistoryItem {
            label: format!("Revision {}", index + 1).into(),
            thumb: None,
        })
    }
}

fn model_label(provider: &str, model: Option<&str>) -> String {
    let provider = match provider {
        "codex" => "Codex",
        "cursor" => "Cursor",
        provider => provider,
    };
    format!("{provider} · {}", model.unwrap_or("default"))
}

fn current_model_label(
    provider: &str,
    model: Option<&str>,
    catalog: &[atlas_ai::agent::AgentModel],
) -> String {
    let name = model.map(|id| {
        catalog
            .iter()
            .find(|model| model.id == id)
            .map_or(id, |model| model.name.as_str())
    });
    model_label(provider, name)
}

impl SlateApp {
    pub(crate) fn artifact_editor_height(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        width: f32,
    ) -> f32 {
        let prompt = self
            .agents
            .prompt(id)
            .or_else(|| {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(slate_doc::agent_chat::agent)
                    .map(|agent| agent.instruction.as_str())
            })
            .unwrap_or("");
        let lines = atlas_shell::selection_tools::image_prompt_line_count(painter, prompt, width);
        let history_count = self
            .doc()
            .scene
            .node(id)
            .and_then(artifact)
            .map_or(0, |binding| binding.revisions.len());
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent);
        let label = agent.map_or_else(
            || model_label("codex", None),
            |agent| {
                current_model_label(
                    &agent.provider,
                    agent.model.as_deref(),
                    self.agents.catalog(&agent.provider),
                )
            },
        );
        let approval = self.agent_approval_description(id);
        let data = self.generator_chrome_data(id);
        let mut state = self.generator_presentation(id).activity.unwrap_or_default();
        let view = data.view(&mut state, self.artifact_is_busy(id));
        chrome::generator_layout(
            painter.ctx(),
            width,
            &label,
            Some("Settings"),
            chrome::GeneratorContent::new(
                lines,
                history_count,
                self.artifact_generator_status(id),
                approval.as_deref(),
                &view,
            ),
        )
        .height
    }
    fn artifact_generator_status(&self, id: NodeId) -> Option<&str> {
        if let Some(error) = self.agent_failure_reason(id) {
            Some(error)
        } else if self.agent_approval_description(id).is_some() {
            Some("Waiting for your approval…")
        } else if self.agents.artifact.preparing.contains_key(&id) {
            Some("Preparing explicit inputs…")
        } else if self.artifact_revision_loading(id) {
            Some("Loading selected revision…")
        } else if let Some(running) = self.agents.artifact.running.get(&id) {
            if !running.publishing {
                if let Some(activity) = self.generator_activity_headline(id) {
                    return Some(activity);
                }
            }
            let session = self
                .agents
                .sessions
                .get(&id)
                .filter(|session| session.request == running.prepared.plan.request.id);
            Some(if running.publishing {
                "Preparing artifact preview…"
            } else if session.is_some_and(|session| session.approval.is_some()) {
                "Waiting for your approval…"
            } else if session.is_some() {
                "Building artifact…"
            } else {
                "Starting artifact harness…"
            })
        } else if self.agents.artifact.pending.contains_key(&id) {
            Some("Preview ready · Settings: Apply or Discard")
        } else if self.agents.artifact.inspect.contains(&id) {
            Some("Memory checkpoint is open in its source file. Fresh eyes disconnects inherited history.")
        } else {
            None
        }
    }
    pub(crate) fn artifact_editor_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Vec<Rect> {
        let Some(instruction) = self
            .doc()
            .scene
            .node(id)
            .filter(|node| artifact(node).is_some())
            .and_then(slate_doc::agent_chat::agent)
            .map(|agent| agent.instruction.clone())
        else {
            return vec![];
        };
        self.sync_instruction(id, &instruction);
        self.agents.want_catalog("codex");
        self.agents.want_catalog("cursor");
        self.remember_object_panel_open(id, super::super::super::board_properties::Panel::Agent);
        let take_focus = self.take_flow_prompt_focus(id);
        let node = self.doc().scene.node(id).unwrap();
        let agent = slate_doc::agent_chat::agent(node).unwrap();
        let binding = artifact(node).unwrap();
        let mut models = Vec::new();
        for provider in ["codex", "cursor"] {
            let available =
                self.agents.programs.iter().any(|p| p.id == provider) || agent.provider == provider;
            if !available {
                continue;
            }
            models.push((provider.to_owned(), None, model_label(provider, None)));
            for model in self.agents.catalog(provider) {
                models.push((
                    provider.to_owned(),
                    Some(model.id.clone()),
                    model_label(provider, Some(&model.name)),
                ));
            }
        }
        let current = models
            .iter()
            .position(|(p, m, _)| p == &agent.provider && m == &agent.model)
            .unwrap_or_else(|| {
                // Keep the authored choice visible while its catalog is loading
                // (or no longer advertises it), without changing that choice.
                models.push((
                    agent.provider.clone(),
                    agent.model.clone(),
                    current_model_label(
                        &agent.provider,
                        agent.model.as_deref(),
                        self.agents.catalog(&agent.provider),
                    ),
                ));
                models.len() - 1
            });
        let labels = models.iter().map(|m| m.2.as_str()).collect::<Vec<_>>();
        let mut prompt = self
            .agents
            .prompt(id)
            .unwrap_or(&agent.instruction)
            .to_owned();
        let presentation = self.generator_presentation(id);
        let mut model_open = presentation.model_open;
        let mut settings_open = presentation.settings_open;
        let mut activity_state = presentation.activity.unwrap_or_default();
        let data = self.generator_chrome_data(id);
        let pending = self.agents.artifact.pending.contains_key(&id);
        let actions: &[&str] = if pending {
            &[
                "Apply preview",
                "Discard preview",
                "Fork",
                "Fresh eyes",
                "Inspect memory",
                "Open source",
            ]
        } else {
            &["Fork", "Fresh eyes", "Inspect memory", "Open source"]
        };
        let history = ArtifactHistory(&binding.revisions);
        let status = self.artifact_generator_status(id);
        let approval = self.agent_approval_description(id);
        let mut chrome_view = data.view(&mut activity_state, self.artifact_is_busy(id));
        chrome_view.picker_page = presentation.picker_page;
        let out = chrome::generator_editor(
            ui,
            rect,
            chrome::GeneratorOptions {
                prompt: &mut prompt,
                hint: "Describe the dashboard or the next change",
                take_focus,
                models: &labels,
                model: current,
                model_open: &mut model_open,
                settings: Some(chrome::GeneratorSettings {
                    label: "Settings",
                    choices: actions,
                    selected: 0,
                    open: &mut settings_open,
                }),
                history: &history,
                selected_history: binding
                    .revision
                    .as_ref()
                    .and_then(|r| binding.revisions.iter().position(|v| v == r)),
                running: self.artifact_is_busy(id),
                status,
                approval: approval.as_deref(),
                approval_interactive: self.agent_approval_interactive(id),
                chrome: chrome_view,
            },
            z,
            theme,
        );
        let chosen_model = out
            .model
            .filter(|_| !self.artifact_is_busy(id))
            .and_then(|index| models.get(index))
            .map(|(provider, model, _)| (provider.clone(), model.clone()));
        let busy_model_choice = out.model.is_some() && self.artifact_is_busy(id);
        let chosen_revision = out
            .clicked_history
            .and_then(|index| binding.revisions.get(index))
            .cloned();
        if let Some(allow) = out.approval {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.approval"),
                Some(serde_json::to_string(&(id.0, allow)).unwrap()),
            );
        }
        self.track_flow_prompt_focus(id, out.prompt.focused);
        self.restore_generator_presentation(id, out.presentation);
        self.dispatch_generator_chrome(ui.ctx(), id, out.model_parameter, out.full_access);
        if busy_model_choice {
            self.toast("Stop this run before changing the harness or model. The running request keeps its settings.");
        }
        if out.prompt.changed {
            *self.agents.prompt_mut(id) = prompt.clone();
        }
        if let Some((provider, model)) = chosen_model {
            self.patch_nodes(&[id], |node| {
                if let Some(a) = slate_doc::agent_chat::agent_mut(node) {
                    a.provider.clone_from(&provider);
                    a.model.clone_from(&model);
                    a.model_parameters.clear();
                }
            });
            self.remember_flow_model(
                crate::app::board_flow::SpawnKind::Artifact,
                &provider,
                model.as_deref().unwrap_or(""),
            );
        }
        if let Some(revision) = chosen_revision {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.artifact.revision"),
                Some(serde_json::json!({"node":id,"revision":revision}).to_string()),
            );
        }
        if out.submit {
            self.board_sel = [id].into_iter().collect();
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(if self.artifact_is_busy(id) {
                    "portal.agent.stop"
                } else {
                    "portal.agent.run"
                }),
                Some(serde_json::json!({"node":id}).to_string()),
            );
        }
        if let Some(action) = out.settings.and_then(|i| actions.get(i)) {
            let command = match *action {
                "Apply preview" => "portal.artifact.accept",
                "Discard preview" => "portal.artifact.reject",
                "Fork" => "portal.artifact.fork",
                "Fresh eyes" => "portal.artifact.memory_disconnect",
                "Inspect memory" => "portal.artifact.memory_inspect",
                _ => "portal.artifact.source_open",
            };
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(command),
                Some(serde_json::json!({"node":id}).to_string()),
            );
        }
        out.popups
    }
    pub(crate) fn artifact_toggle_memory_inspect(&mut self, id: NodeId) -> bool {
        let memory = slate_doc::agent_inputs::artifact_memory_source(&self.doc().scene, id);
        let source = memory
            .map(|(source, pin)| (source, pin.revision))
            .or_else(|| {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(artifact)
                    .and_then(|a| a.revision.clone())
                    .map(|r| (id, r))
            });
        let Some((source, revision)) = source else {
            self.toast("This artifact has no completed memory checkpoint.");
            return false;
        };
        let reference = match self.artifact_reference(source, &revision) {
            Ok(r) => r,
            Err(e) => {
                self.toast(e);
                return false;
            }
        };
        let path = reference
            .source_root
            .join("revisions")
            .join(&reference.revision_id)
            .join("revision.json");
        match super::super::super::board_web::open_externally(&path.to_string_lossy()) {
            Ok(()) => {
                self.agents.artifact.inspect.insert(id);
                self.toast(format!("Opened memory checkpoint: {}", path.display()));
                true
            }
            Err(error) => {
                self.toast(format!(
                    "Could not open memory checkpoint {}: {error}",
                    path.display()
                ));
                false
            }
        }
    }
    pub(crate) fn artifact_open_source(&mut self, id: NodeId) -> bool {
        let available = self.artifact_preview_source(id).is_some()
            || self.doc().scene.node(id).is_some_and(
                |node| matches!(&node.kind, NodeKind::Portal(portal) if portal.source.is_some()),
            );
        if !available {
            self.toast("This artifact has no source yet. Play, then Apply a completed preview.");
            return false;
        }
        if self.web_open_external_for(id) {
            self.toast("Opened artifact source in its system browser.");
            true
        } else {
            false
        }
    }
}
