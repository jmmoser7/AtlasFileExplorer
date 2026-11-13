//! Bound text/model adapters pass existing data into the one generator shell.
use super::*;
use atlas_shell::selection_tools::{self as tools, generator as chrome};

struct TextHistory<'a> {
    entries: &'a [(String, String, String)],
    selected: atlas_shell::result_actions::ResultSelection,
    active: Option<String>,
}
impl chrome::GeneratorHistory for TextHistory<'_> {
    fn image_actions(&self) -> bool {
        true
    }
    fn len(&self) -> usize {
        self.entries.len()
    }
    fn is_selected(&self, index: usize) -> bool {
        self.entries.get(index).is_some_and(|entry| {
            self.selected.contains(&entry.0) || self.active.as_ref() == Some(&entry.0)
        })
    }
    fn get(&self, index: usize) -> Option<chrome::GeneratorHistoryItem<'_>> {
        Some(chrome::GeneratorHistoryItem {
            label: std::borrow::Cow::Borrowed(&self.entries.get(index)?.1),
            thumb: None,
        })
    }
    fn text(&self, index: usize) -> Option<&str> {
        Some(&self.entries.get(index)?.2)
    }
}
impl SlateApp {
    fn bound_generator_layout(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        width: f32,
    ) -> chrome::GeneratorLayout {
        let prompt = self.agent_prompt_text(id);
        let lines = tools::image_prompt_line_count(painter, &prompt, width);
        let data = self.generator_chrome_data(id);
        let mut state = self.generator_activity_state(id, self.generator_presentation(id).activity);
        let view = data.view(&mut state, self.agent_is_running(id));
        let count = self.result_outputs(id).len();
        chrome::generator_layout(
            painter.ctx(),
            width,
            "Model",
            Some("Settings"),
            chrome::GeneratorContent::new(lines, count, self.agent_failure_reason(id), None, &view),
        )
    }
    pub(crate) fn bound_generator_height(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        width: f32,
    ) -> f32 {
        self.bound_generator_layout(painter, id, width).height
    }

    pub(crate) fn paint_bound_generator_editor(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: NodeId,
        edit: tools::AgentEdit<'_>,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> tools::AgentEditResponse {
        let model_mode = self.grasshopper_viewer(id) || self.model_node_info(id).is_some();
        let outputs = self.model_generator_outputs(id);
        let selected_model = outputs.iter().position(|output| output.current);
        let visible = self
            .bound_generator_layout(ui.painter(), id, rect.width() / z)
            .visible_history_range(outputs.len(), selected_model);
        let model_history = self.model_output_history(ui.ctx(), id, visible);
        let text_entries = self.result_text_entries(id);
        let active = self.active_result_reference(id);
        let text_history = TextHistory {
            entries: &text_entries,
            selected: self.result_selection(id),
            active: self
                .result_outputs(id)
                .into_iter()
                .find(|output| Some(&output.reference) == active.as_ref())
                .map(|output| output.id),
        };
        let history: &dyn chrome::GeneratorHistory = if model_mode {
            &model_history
        } else {
            &text_history
        };
        let mut presentation = self.generator_presentation(id);
        let mut state = self.generator_activity_state(id, presentation.activity);
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent);
        let preserve = agent
            .map(|agent| agent.generator.preserve_history.unwrap_or(true))
            .or_else(|| self.pending_generator_preserve(id));
        let data = self.generator_chrome_data(id);
        let mut view = data.view(&mut state, self.agent_is_running(id));
        view.picker_page = presentation.picker_page;
        let mut out = chrome::generator_editor(
            ui,
            rect,
            chrome::GeneratorOptions {
                prompt: &mut *edit.prompt,
                hint: edit.hint,
                take_focus: edit.take_focus,
                count: (!self.grasshopper_viewer(id)).then_some(edit.count.max(1) as usize),
                preserve_history: preserve,
                models: edit.models,
                model: edit.model,
                model_open: edit.model_open,
                settings: Some(chrome::GeneratorSettings {
                    label: "Settings",
                    choices: &[],
                    selected: 0,
                    open: &mut presentation.settings_open,
                }),
                history,
                selected_history: selected_model,
                running: self.agent_is_running(id),
                status: self.agent_failure_reason(id),
                approval: None,
                approval_interactive: false,
                chrome: view,
            },
            z,
            theme,
        );
        #[cfg(test)]
        if let Some(response) = ui.ctx().read_response(ui.id().with("generator-model")) {
            let transform = ui
                .ctx()
                .layer_transform_to_global(response.layer_id)
                .unwrap_or_default();
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    Id::new(("painted-generator-model-hit", id.0)),
                    transform * response.rect,
                )
            });
        }
        #[cfg(test)]
        if let Some(response) = ui.ctx().read_response(ui.id().with("generator-settings")) {
            let transform = ui
                .ctx()
                .layer_transform_to_global(response.layer_id)
                .unwrap_or_default();
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    Id::new("painted-generator-options-hit"),
                    transform * response.rect,
                )
            });
        }
        if out.activity_fold.is_some() {
            self.note_model_activity_fold(id);
        }
        self.restore_generator_presentation(id, out.presentation);
        if out.board_access {
            self.board_sel = [id].into_iter().collect();
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.board_access"),
                None,
            );
        }
        self.dispatch_generator_chrome(ui.ctx(), id, out.model_parameter, out.full_access);
        self.dispatch_generator_provider(id, out.provider, out.edit_key);
        self.dispatch_generator_settings(ui.ctx(), id, out.count, out.preserve_history);
        if let Some(index) = out.clicked_history {
            if model_mode {
                let identities = model_history
                    .entries
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<Vec<_>>();
                if let Some(identity) = identities.get(index) {
                    self.select_generator_result(
                        id,
                        &identities,
                        index,
                        out.result_interaction.shift,
                    );
                    self.choose_model_output(ui.ctx(), id, identity);
                }
            } else {
                let identities = text_entries
                    .iter()
                    .map(|entry| entry.0.clone())
                    .collect::<Vec<_>>();
                self.select_generator_result(id, &identities, index, out.result_interaction.shift);
                self.choose_text_output_index(ui.ctx(), id, index);
            }
        }
        self.open_result_context(ui, id, out.result_interaction);
        if out.submit {
            self.agents
                .flow
                .viewed_dialogues
                .remove(&(self.tab().id, id));
        }
        let submit = if out.submit && self.agent_is_running(id) {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.stop"),
                Some(serde_json::json!({"node":id}).to_string()),
            );
            false
        } else if out.submit
            && (self.grasshopper_viewer(id) || self.model_node_info(id).is_some())
            && self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some()
        {
            self.edit_agent_prompt_text(id, edit.prompt.clone());
            out.prompt.changed = false;
            self.commit_instruction(id);
            self.board_sel = [id].into_iter().collect();
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.send"),
                Some(serde_json::json!({"node":id}).to_string()),
            );
            false
        } else {
            out.submit
        };
        tools::AgentEditResponse {
            model: out.model,
            count: out.count.map(|count| count as u8),
            submit,
            prompt: out.prompt,
            popups: out.popups,
            ..Default::default()
        }
    }
}
