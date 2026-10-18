//! One app adapter for shared generator chrome; provider and lifecycle stay here.
use super::*;
use atlas_shell::selection_tools::generator::{self as chrome, activity};
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;

pub(crate) struct GeneratorChromeData<'a> {
    model: Option<&'a atlas_agent::AgentModel>,
    selected: Option<&'a BTreeMap<String, String>>,
    session: Option<&'a AgentSession>,
    request: &'a str,
    access: Option<chrome::access::AccessView>,
}

impl chrome::ModelParameters for GeneratorChromeData<'_> {
    fn len(&self) -> usize {
        self.model.map_or(0, |model| model.parameters.len())
    }
    fn parameter(&self, index: usize) -> Option<chrome::ModelParameterHeader<'_>> {
        let p = self.model?.parameters.get(index)?;
        Some(chrome::ModelParameterHeader {
            id: &p.id,
            name: &p.name,
            selected: self
                .selected
                .and_then(|selected| selected.get(&p.id))
                .map(String::as_str)
                .or(p.default.as_deref())
                .unwrap_or(""),
        })
    }
    fn value_count(&self, index: usize) -> usize {
        self.model
            .and_then(|m| m.parameters.get(index))
            .map_or(0, |p| p.values.len())
    }
    fn value_at(&self, index: usize, value: usize) -> Option<chrome::ModelParameterValue<'_>> {
        let value = self.model?.parameters.get(index)?.values.get(value)?;
        Some(chrome::ModelParameterValue {
            value: &value.value,
            name: &value.name,
        })
    }
}

impl activity::ActivityEntries for GeneratorChromeData<'_> {
    fn len(&self) -> usize {
        self.session.map_or(0, |session| {
            session
                .activity
                .iter()
                .filter(|event| event.request == self.request)
                .count()
        })
    }
    fn get(&self, index: usize) -> Option<activity::ActivityEntry<'_>> {
        let event = self
            .session?
            .activity
            .iter()
            .filter(|event| event.request == self.request)
            .nth(index)?;
        Some(activity::ActivityEntry {
            id: &event.id,
            label: match event.kind {
                atlas_agent::AgentActivityKind::Commentary => "Agent",
                atlas_agent::AgentActivityKind::ToolStarted => "Working",
                atlas_agent::AgentActivityKind::ToolFinished => "Completed",
            },
            text: &event.text,
        })
    }
}

impl GeneratorChromeData<'_> {
    #[cfg(test)]
    pub(crate) fn entries(&self) -> &dyn activity::ActivityEntries {
        self
    }
    pub(crate) fn view<'a>(
        &'a self,
        state: &'a mut activity::ActivityState,
        running: bool,
    ) -> chrome::GeneratorChrome<'a> {
        chrome::GeneratorChrome {
            picker_page: chrome::PickerPage::Root,
            parameters: self,
            full_access: self.access,
            activity: (!activity::ActivityEntries::is_empty(self)).then_some(
                activity::ActivityView {
                    entries: self,
                    state,
                    running,
                },
            ),
        }
    }
}

impl SlateApp {
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
    fn generator_activity_data(&self, id: NodeId) -> GeneratorChromeData<'_> {
        let session = self.agents.session(id);
        GeneratorChromeData {
            model: None,
            selected: None,
            session,
            request: self
                .agents
                .request_of(id)
                .or_else(|| session.map(|s| s.request.as_str()))
                .unwrap_or(""),
            access: None,
        }
    }
    pub(crate) fn generator_chrome_data(&self, id: NodeId) -> GeneratorChromeData<'_> {
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent);
        let mut data = self.generator_activity_data(id);
        data.model = agent.and_then(|a| {
            self.agents
                .catalog(atlas_ai::agent::model_catalog(&a.provider))
                .iter()
                .find(|model| {
                    a.model
                        .as_ref()
                        .map_or(model.is_default, |id| id == &model.id)
                })
        });
        data.selected = agent.map(|a| &a.model_parameters);
        data.access = self
            .is_artifact_node(id)
            .then_some(chrome::access::AccessView {
                enabled: self.dashboard_full_access(),
                effective: agent.is_some_and(|agent| {
                    self.agents.access_snapshot.artifact_granted(&agent.session)
                        && self.crosstalk_policy(&agent.session)
                            != atlas_agent::TurnPolicy::ReadOnly
                }),
                interactive: !self.tab().read_only,
            });
        data
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
        struct Choice {
            node: NodeId,
            parameter: String,
            value: String,
        }
        let Some(choice) = detail.and_then(|d| serde_json::from_str::<Choice>(d).ok()) else {
            return false;
        };
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
        selected.insert(choice.parameter, choice.value);
        if let Err(error) = model.validate_parameters(&selected) {
            self.toast(error);
            return false;
        }
        self.patch_nodes(&[choice.node], |node| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(node) {
                a.model_parameters.clone_from(&selected);
            }
        });
        true
    }
    pub(crate) fn dispatch_generator_chrome(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        parameter: Option<(String, String)>,
        access: Option<bool>,
    ) {
        if let Some((parameter, value)) = parameter {
            self.dispatch(
                ctx,
                atlas_commands::CommandId("portal.agent.model_parameter"),
                Some(
                    serde_json::json!({"node":id,"parameter":parameter,"value":value}).to_string(),
                ),
            );
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
        self.agents.dashboard_full_access
    }
    pub(crate) fn dashboard_set_full_access(&mut self, detail: Option<&str>) -> bool {
        let on = detail
            .and_then(|d| serde_json::from_str::<bool>(d).ok())
            .unwrap_or_else(|| !self.dashboard_full_access());
        let Some(path) = self.agents.access_path.as_deref() else {
            return false;
        };
        if let Err(error) = atlas_ai::access::set_artifact_default_full_access_in(path, on) {
            self.toast(error);
            return false;
        }
        self.agents.access_snapshot = atlas_ai::access::load_snapshot_in(path);
        self.agents.dashboard_full_access = on;
        self.toast(if on { "Full access on for all dashboard builders on this machine, from their next run. Explicit restrictions still apply." }
            else { "Full access off for dashboard builders. Explicit conversation grants still apply." });
        true
    }
}
