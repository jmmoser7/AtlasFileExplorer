//! One app adapter for shared generator chrome; provider and lifecycle stay here.
use super::*;
use atlas_shell::selection_tools::generator::{self as chrome, activity};
mod data;
mod editor;
mod model_activity;
mod model_outputs;
#[cfg(test)]
mod parameter_default_tests;
#[cfg(test)]
mod tests;
mod text_outputs;
mod text_reviews;

impl SlateApp {
    pub(super) fn generator_activity_state(
        &self,
        id: NodeId,
        stored: Option<activity::ActivityState>,
    ) -> activity::ActivityState {
        let mut state = stored.unwrap_or_else(|| {
            let mut state = activity::ActivityState::default();
            let preserve = self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .map(|agent| agent.generator.preserve_history.unwrap_or(true))
                .or_else(|| self.pending_generator_preserve(id));
            if preserve == Some(true) {
                state.fold = activity::CardFold::Partial;
            }
            state
        });
        self.disclose_model_activity(id, &mut state);
        state
    }

    pub(crate) fn set_generator_settings(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        struct Choice {
            node: NodeId,
            count: Option<u8>,
            preserve_history: Option<bool>,
        }
        let Some(choice) = detail.and_then(|value| serde_json::from_str::<Choice>(value).ok())
        else {
            return false;
        };
        let Ok(command) =
            self.prepare_generator_settings(choice.node, choice.count, choice.preserve_history)
        else {
            return false;
        };
        self.commit_scene(vec![command])
    }
    pub(crate) fn prepare_generator_settings(
        &self,
        id: NodeId,
        count: Option<u8>,
        preserve_history: Option<bool>,
    ) -> Result<slate_doc::scene::SceneCmd, String> {
        if self.tab().read_only || count.is_some() && self.agent_is_running(id) {
            return Err("Generator settings require a writable host; Count changes require an idle generator".into());
        }
        if count.is_some_and(|count| !(1..=8).contains(&count)) {
            return Err("Count must be an integer from 1 to 8".into());
        }
        let before = self.doc().scene.node(id).ok_or("Generator is missing")?;
        if before.locked {
            return Err("Generator is locked".into());
        }
        let mut after = before.clone();
        let agent =
            slate_doc::agent_chat::agent_mut(&mut after).ok_or("Target is not a generator")?;
        if let Some(count) = count {
            agent.generator.count = Some(count);
            agent.image.count = count;
        }
        if let Some(preserve) = preserve_history {
            let cut = !preserve && agent.generator.preserve_history.unwrap_or(true);
            agent.generator.preserve_history = Some(preserve);
            if !preserve && agent.generator.text_response_only {
                if cut {
                    agent.generator.text_context_epoch = Some(
                        agent
                            .generator
                            .text_context_epoch
                            .unwrap_or(0)
                            .saturating_add(1),
                    );
                }
                agent.generator.text_context.clear();
            }
        }
        if before == &after {
            return Err("Generator settings are unchanged".into());
        }
        Ok(slate_doc::scene::SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(after),
        })
    }
    pub(crate) fn dispatch_generator_settings(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        count: Option<usize>,
        preserve: Option<bool>,
    ) {
        if preserve == Some(true) {
            let mut presentation = self.generator_presentation(id);
            let mut state = self.generator_activity_state(id, presentation.activity);
            state.fold = activity::CardFold::Partial;
            presentation.activity = Some(state);
            self.restore_generator_presentation(id, presentation);
        }
        if self.set_pending_generator_settings(id, count, preserve) {
            return;
        }
        if count.is_some() || preserve.is_some() {
            self.dispatch(
                ctx,
                atlas_commands::CommandId("portal.agent.generator_settings"),
                Some(
                    serde_json::json!({"node":id,"count":count,"preserve_history":preserve})
                        .to_string(),
                ),
            );
        }
    }
    pub(crate) fn dispatch_generator_provider(
        &mut self,
        id: NodeId,
        provider: Option<String>,
        edit_key: Option<String>,
    ) {
        if let Some(provider) = provider {
            if self.pending_generator_host(id).is_some() {
                self.set_pending_generator_provider(id, &provider);
            } else {
                if self.tab().read_only {
                    return;
                }
                let image = self.image_palette_eligible(id);
                let allowed = if self.is_artifact_node(id) {
                    self.pending_provider_options(id)
                        .iter()
                        .any(|choice| choice.id == provider)
                } else if self.model_node_info(id).is_some() {
                    Self::scoped_model_harness(&provider)
                        && self.agents.programs.iter().any(|p| p.id == provider)
                } else if self.bound_chat_generator_host(id) {
                    self.agents.programs.iter().any(|choice| {
                        choice.id == provider && choice.view == atlas_agent::PortalView::Chat
                    })
                } else if self.doc().scene.node(id).is_some_and(
                    |node| matches!(&node.kind, NodeKind::Portal(portal) if portal.agent.is_some()),
                ) {
                    self.agents
                        .programs
                        .iter()
                        .any(|choice| choice.id == provider)
                } else {
                    self.agent_models(image)
                        .iter()
                        .any(|choice| choice.0 == provider)
                };
                if allowed {
                    let model = if self.is_artifact_node(id)
                        && atlas_ai::runtime::subscription_provider(&provider).is_some()
                    {
                        self.subscription_default_model(id, &provider)
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    self.set_flow_model(id, image, &provider, &model);
                }
            }
        }
        if let Some(provider) = edit_key {
            match provider.as_str() {
                "openai-image" | "openai-text" => {
                    self.ai.packs.key_draft.clear();
                    self.ai.packs.key_entry = true;
                }
                "cursor" => {
                    self.agents.flow.picker_key_entry = Some((self.tab().id, id));
                    self.agents.key_draft.clear();
                }
                _ => {}
            }
        }
    }
    pub(crate) fn apply_text_answer_navigation(&mut self) {
        let Some((workbook, id, target)) = self.agents.flow.text_answer_navigation.clone() else {
            return;
        };
        if workbook != self.tab().id {
            self.agents.flow.text_answer_navigation = None;
            return;
        }
        if let Some(review_id) = target.review_id.as_deref() {
            let found = self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.generator.text_reviews.iter().any(|r| r.id == review_id));
            self.agents.flow.text_answer_navigation = None;
            if found {
                self.agents
                    .flow
                    .viewed_dialogues
                    .insert((workbook, id), target);
                self.board_sel = [id].into_iter().collect();
                self.open_object_panel(super::super::board_properties::Panel::Agent);
                let mut presentation = self.generator_presentation(id);
                let mut state = presentation.activity.unwrap_or_default();
                state.fold = activity::CardFold::Open;
                state.scroll.follow = false;
                state.focus_index = Some(1);
                presentation.activity = Some(state);
                self.restore_generator_presentation(id, presentation);
            }
            return;
        }
        let path =
            slate_doc::scene::resolve_source(self.tab().path.as_deref(), &target.source.locator);
        let current = self.text_output_is_current(id, &target);
        let session = if current {
            self.agents.sessions.get(&id).cloned()
        } else {
            path.parent()
                .and_then(|dir| self.agents.sources.poll(dir, None))
        };
        let Some(session) = session else {
            return;
        };
        let row = session
            .turns
            .get(..target.response.through)
            .filter(|turns| turns.last().is_some_and(|turn| turn.role == "assistant"))
            .map(|turns| turns.iter().filter(|turn| turn.role != "system").count() - 1);
        self.agents.flow.text_answer_navigation = None;
        let Some(row) = row else {
            self.toast("The recorded response checkpoint is unavailable. Its saved answer remains on the passage.");
            return;
        };
        self.agents
            .flow
            .viewed_dialogues
            .insert((workbook, id), target);
        self.board_sel = [id].into_iter().collect();
        self.open_object_panel(super::super::board_properties::Panel::Agent);
        let mut presentation = self.generator_presentation(id);
        let mut state = presentation.activity.unwrap_or_default();
        state.fold = activity::CardFold::Open;
        state.scroll.follow = false;
        state.focus_index = Some(row);
        presentation.activity = Some(state);
        self.restore_generator_presentation(id, presentation);
    }
    /// Read immediately after painting this editor, never from a skipped/cached body.
    pub(crate) fn generator_menu_dismissal_owns_press(
        &self,
        ctx: &egui::Context,
        id: NodeId,
    ) -> bool {
        self.generator_presentation(id).dismissed_menu.is_some()
            && ctx.input(|input| input.pointer.any_pressed())
    }

    /// The current popup dismisses before parent editors or board shortcuts.
    pub(crate) fn dismiss_generator_popup_for_escape(&mut self, ctx: &egui::Context) -> bool {
        let host = self
            .shape_properties
            .ids
            .first()
            .copied()
            .filter(|_| {
                self.shape_properties.ids.len() == 1
                    && self.shape_properties.panel
                        == Some(super::super::board_properties::Panel::Agent)
            })
            .filter(|id| {
                let state = self.generator_presentation(*id);
                state.model_open || state.settings_open || state.aspect_open || state.live_open
            })
            .or_else(|| {
                self.agents
                    .flow
                    .presentations
                    .iter()
                    .find_map(|((tab, id), state)| {
                        (*tab == self.tab().id
                            && (state.model_open
                                || state.settings_open
                                || state.aspect_open
                                || state.live_open))
                            .then_some(*id)
                    })
            });
        let presentation = host.map(|id| (id, self.generator_presentation(id)));
        let generator_open = presentation.as_ref().is_some_and(|(_, state)| {
            state.model_open || state.settings_open || state.aspect_open || state.live_open
        });
        if !(generator_open || self.agents.model_menu.is_some())
            || !ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            return false;
        }
        self.agents.model_menu = None;
        self.agents.model_menu_rect = None;
        if let Some((id, mut state)) = presentation.filter(|_| generator_open) {
            state.model_open = false;
            state.picker_page = chrome::PickerPage::Root;
            state.settings_open = false;
            state.aspect_open = false;
            state.live_open = false;
            state.dismissed_menu = None;
            self.restore_generator_presentation(id, state);
            self.close_pending_generator_menus(id);
        }
        true
    }

    pub(crate) fn generator_presentation(&self, id: NodeId) -> chrome::GeneratorPresentation {
        self.agents
            .flow
            .presentations
            .get(&(self.tab().id, id))
            .copied()
            .unwrap_or_default()
    }
    pub(crate) fn restore_generator_presentation(
        &mut self,
        id: NodeId,
        state: chrome::GeneratorPresentation,
    ) {
        self.agents
            .flow
            .presentations
            .insert((self.tab().id, id), state);
    }
    pub(crate) fn forget_generator_presentation(&mut self, id: NodeId) {
        self.agents.flow.presentations.remove(&(self.tab().id, id));
    }
    pub(crate) fn generator_activity_headline(&self, id: NodeId) -> Option<&str> {
        let session = self.agents.sessions.get(&id)?;
        let request = self
            .agents
            .requests
            .get(&id)
            .map(String::as_str)
            .unwrap_or(&session.request);
        session
            .activity
            .iter()
            .rev()
            .find(|a| a.request == request && !a.text.is_empty())
            .map(|a| a.text.as_str())
    }
    pub(crate) fn generator_set_parameter(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Choice {
            node: NodeId,
            parameter: String,
            #[serde(deserialize_with = "serde::Deserialize::deserialize")]
            value: Option<String>,
        }
        let Some(choice) = detail.and_then(|d| serde_json::from_str::<Choice>(d).ok()) else {
            return false;
        };
        #[cfg(test)]
        eprintln!(
            "generator parameter writer reached: node={:?}, parameter={}, value={:?}",
            choice.node, choice.parameter, choice.value
        );
        if self.refuse_read_only_edit() {
            return false;
        }
        if self.agent_is_running(choice.node) {
            self.toast("Stop this run before changing its model choices. The running request keeps its settings.");
            return false;
        }
        let data = self.generator_chrome_data(choice.node);
        let Some(model) = data.model else {
            self.toast("Wait for this model's available choices to load.");
            return false;
        };
        let mut selected = data.selected.cloned().unwrap_or_default();
        if !model.parameters.iter().any(|p| p.id == choice.parameter) {
            self.toast("This model does not offer that parameter.");
            return false;
        }
        if let Some(value) = choice.value {
            selected.insert(choice.parameter, value);
        } else {
            selected.remove(&choice.parameter);
        }
        if let Err(error) = model.validate_parameters(&selected) {
            self.toast(error);
            return false;
        }
        let unchanged = data.selected.is_some_and(|values| values == &selected);
        if self.set_pending_generator_parameters(choice.node, selected.clone()) {
            return true;
        }
        if unchanged {
            return true;
        }
        // A menu choice is a discrete action, like Count, not a continuous inspector scrub.
        self.last_board_edit = None;
        self.patch_nodes(&[choice.node], |node| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(node) {
                a.model_parameters.clone_from(&selected);
            }
        });
        if let Some(agent) = self
            .doc()
            .scene
            .node(choice.node)
            .and_then(slate_doc::agent_chat::agent)
        {
            let (provider, model) = (
                agent.provider.clone(),
                agent.model.clone().unwrap_or_default(),
            );
            self.remember_source_model(choice.node, &provider, &model, &selected);
        }
        self.last_board_edit = None;
        true
    }
    pub(crate) fn dispatch_generator_chrome(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        parameter: Option<(String, Option<String>)>,
        access: Option<bool>,
    ) {
        if let Some((parameter, value)) = parameter {
            let _ran = self.dispatch(
                ctx,
                atlas_commands::CommandId("portal.agent.model_parameter"),
                Some(
                    serde_json::json!({"node":id,"parameter":parameter,"value":value}).to_string(),
                ),
            );
            #[cfg(test)]
            eprintln!("registered generator parameter dispatch: node={id:?}, ran={_ran}");
        }
        if let Some(enabled) = access {
            self.dispatch(
                ctx,
                atlas_commands::CommandId("portal.artifact.full_access"),
                Some(enabled.to_string()),
            );
        }
    }
    pub(crate) fn dashboard_full_access(&self) -> bool {
        self.agents.access_snapshot.global_default_full_access()
    }
    pub(crate) fn dashboard_set_full_access(&mut self, detail: Option<&str>) -> bool {
        let on = detail
            .and_then(|d| serde_json::from_str::<bool>(d).ok())
            .unwrap_or_else(|| !self.dashboard_full_access());
        let Some(path) = self.agents.access_path.as_deref() else {
            return false;
        };
        if let Err(error) = atlas_ai::access::set_global_default_full_access_in(path, on) {
            self.toast(error);
            return false;
        }
        self.agents.access_snapshot = atlas_ai::access::load_snapshot_in(path);
        self.agents.dashboard_full_access = on;
        self.toast(if on { "Full access on for all agents for this user, from their next run. Explicit restrictions and Board access still apply." }
            else { "Full access off globally. Explicit conversation grants still apply." });
        true
    }
}
