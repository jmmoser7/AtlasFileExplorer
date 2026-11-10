//! Borrowed provider/configuration/activity data for the one generator chrome.
use super::*;
use std::collections::BTreeMap;
pub(crate) struct GeneratorChromeData<'a> {
    pub(super) model: Option<&'a atlas_agent::AgentModel>,
    pub(super) selected_model_id: Option<&'a str>,
    pub(super) providers: Vec<atlas_ai::agent::AgentProvider>,
    pub(super) credentials: &'a atlas_ai::packs::PackSession,
    pub(super) provider: &'a str,
    pub(super) chat_host: bool,
    pub(super) source_kind: Option<crate::app::board_flow::PendingHost>,
    pub(super) selected: Option<&'a BTreeMap<String, String>>,
    pub(super) session: Option<&'a AgentSession>,
    failure: Option<&'a str>,
    pub(super) review_dialogue: Option<AgentSession>,
    pub(super) request: &'a str,
    pub(super) access: Option<chrome::access::AccessView>,
    pub(super) board_access: Option<chrome::access::BoardAccessView>,
}

impl chrome::ModelParameters for GeneratorChromeData<'_> {
    fn header_provider_prefix(&self) -> Option<&str> {
        use crate::app::board_flow::PendingHost;
        match self.source_kind {
            None | Some(PendingHost::Chat) => None,
            Some(PendingHost::Image) if self.provider == "openai-image" => None,
            _ => self
                .providers
                .iter()
                .find(|p| p.id == self.provider)
                .map(|p| p.display_name.as_str()),
        }
    }
    fn generator_parameter_hover(&self) -> bool {
        self.source_kind
            .is_some_and(|kind| kind != crate::app::board_flow::PendingHost::Chat)
    }
    fn parameter_is_overridden(&self, index: usize) -> Option<bool> {
        let parameter = self.model?.parameters.get(index)?;
        Some(
            self.selected
                .is_some_and(|selected| selected.contains_key(&parameter.id)),
        )
    }
    fn model_label(&self) -> Option<&str> {
        self.model
            .map(|m| m.name.as_str())
            .or(self.selected_model_id.filter(|id| !id.is_empty()))
    }
    fn model_is_selected(&self, label: &str) -> Option<bool> {
        self.model_label().map(|selected| selected == label)
    }
    fn board_access(&self) -> Option<chrome::access::BoardAccessView> {
        self.board_access
    }

    fn provider_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|p| !self.chat_host || p.view == atlas_agent::PortalView::Chat)
            .count()
    }
    fn provider(&self, index: usize) -> Option<chrome::ModelProvider<'_>> {
        let provider = self
            .providers
            .iter()
            .filter(|p| !self.chat_host || p.view == atlas_agent::PortalView::Chat)
            .nth(index)?;
        Some(chrome::ModelProvider {
            id: &provider.id,
            name: &provider.display_name,
            icon: match provider.id.as_str() {
                "comfy" => Some(atlas_shell::icons::Icon::Display),
                "cursor" => Some(atlas_shell::icons::Icon::ProviderCursor),
                "codex" | "codex-text" | "codex-image" | "openai-text" | "openai-image" => {
                    Some(atlas_shell::icons::Icon::ProviderCodex)
                }
                "ollama" | "local" => Some(atlas_shell::icons::Icon::ProviderOllama),
                _ => None,
            },
            selected: provider.id == self.provider,
            key_present: self.credentials.credential_present(&provider.id),
        })
    }
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
        self.session().map_or(0, |session| {
            session
                .turns
                .iter()
                .filter(|turn| turn.role != "system")
                .count()
                + session
                    .activity
                    .iter()
                    .filter(|event| event.request == self.request)
                    .count()
        }) + usize::from(self.failure.is_some())
    }
    fn is_user(&self, index: usize) -> bool {
        self.session()
            .and_then(|session| {
                session
                    .turns
                    .iter()
                    .filter(|turn| turn.role != "system")
                    .nth(index)
            })
            .is_some_and(|turn| turn.role == "user")
    }
    fn get(&self, index: usize) -> Option<activity::ActivityEntry<'_>> {
        if index == activity::ActivityEntries::len(self).saturating_sub(1) {
            if let Some(failure) = self.failure {
                return Some(activity::ActivityEntry {
                    id: "request-failure",
                    label: "Error",
                    text: failure,
                });
            }
        }
        let session = self.session()?;
        let turns = session.turns.iter().filter(|turn| turn.role != "system");
        let count = turns.clone().count();
        if let Some(turn) = turns.clone().nth(index) {
            return Some(activity::ActivityEntry {
                id: &turn.role,
                label: if turn.role == "user" { "You" } else { "Agent" },
                text: &turn.text,
            });
        }
        let event = session
            .activity
            .iter()
            .filter(|event| event.request == self.request)
            .nth(index.checked_sub(count)?)?;
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
    fn session(&self) -> Option<&AgentSession> {
        self.review_dialogue.as_ref().or(self.session)
    }
    #[cfg(test)]
    pub(crate) fn entries(&self) -> &dyn activity::ActivityEntries {
        self
    }
    pub(crate) fn view<'a>(
        &'a self,
        state: &'a mut activity::ActivityState,
        running: bool,
    ) -> chrome::GeneratorChrome<'a> {
        self.view_with_dialogue_policy(state, running, None)
    }
    pub(crate) fn view_with_dialogue_policy<'a>(
        &'a self,
        state: &'a mut activity::ActivityState,
        running: bool,
        dialogue: Option<bool>,
    ) -> chrome::GeneratorChrome<'a> {
        chrome::GeneratorChrome {
            picker_page: chrome::PickerPage::Root,
            parameters: self,
            full_access: self.access,
            activity: dialogue
                .unwrap_or_else(|| !activity::ActivityEntries::is_empty(self))
                .then_some(activity::ActivityView {
                    entries: self,
                    state,
                    running,
                }),
        }
    }
}

impl SlateApp {
    fn generator_activity_data(&self, id: NodeId) -> GeneratorChromeData<'_> {
        let active = self.agents.session(id);
        let viewed = self.agents.flow.viewed_dialogues.get(&(self.tab().id, id));
        let mut review_dialogue = viewed
            .and_then(|v| v.review_id.as_ref())
            .and_then(|review_id| {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(slate_doc::agent_chat::agent)
                    .and_then(|a| a.generator.text_reviews.iter().find(|r| &r.id == review_id))
            })
            .map(|review| AgentSession {
                request: review.id.clone(),
                turns: vec![
                    atlas_agent::AgentTurn {
                        role: "user".into(),
                        text: review.question.clone(),
                        at: 0,
                    },
                    atlas_agent::AgentTurn {
                        role: "assistant".into(),
                        text: review.text.clone(),
                        at: 1,
                    },
                ],
                ..Default::default()
            });
        let mut session = if let Some(reference) = viewed {
            if reference.review_id.is_some() {
                None
            } else {
                let current = self.text_output_is_current(id, reference);
                if current {
                    active
                } else {
                    let path = slate_doc::scene::resolve_source(
                        self.tab().path.as_deref(),
                        &reference.source.locator,
                    );
                    path.parent().and_then(|dir| self.agents.sources.peek(dir))
                }
            }
        } else {
            active
        };
        if self.text_response_only(id) && !self.agent_is_running(id) && viewed.is_none() {
            session = None;
            let context = self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .filter(|a| a.generator.preserve_history == Some(true))
                .map(|a| a.generator.text_context.clone())
                .unwrap_or_default();
            if !context.is_empty() {
                review_dialogue = Some(AgentSession {
                    turns: context,
                    ..Default::default()
                });
            }
        }
        GeneratorChromeData {
            model: None,
            selected_model_id: None,
            providers: self.agents.programs.clone(),
            credentials: &self.ai.packs,
            provider: "",
            chat_host: false,
            source_kind: self.generator_source_kind(id),
            selected: None,
            session,
            failure: self.agent_failure_reason(id),
            review_dialogue,
            request: self
                .agents
                .request_of(id)
                .or_else(|| session.map(|s| s.request.as_str()))
                .unwrap_or(""),
            access: None,
            board_access: None,
        }
    }
    pub(crate) fn generator_chrome_data(&self, id: NodeId) -> GeneratorChromeData<'_> {
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent);
        let mut data = self.generator_activity_data(id);
        if self.generator_source_kind(id).is_some() {
            data.providers = self.pending_provider_options(id);
        }
        if self.pending_generator_host(id).is_some() {
            data.providers = self.pending_provider_options(id);
            if let Some((provider, model, selected)) = self.pending_generator_choice(id) {
                data.selected_model_id = Some(model);
                data.provider = provider;
                data.selected = Some(selected);
                data.model = self
                    .agents
                    .catalog(atlas_ai::agent::model_catalog(provider))
                    .iter()
                    .find(|m| {
                        if model.is_empty() {
                            m.is_default
                        } else {
                            m.id == model
                        }
                    });
            }
            data.access = Some(chrome::access::AccessView {
                enabled: self.dashboard_full_access(),
                effective: false,
                interactive: !self.tab().read_only,
            });
            return data;
        }
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
        data.selected_model_id = agent.and_then(|a| a.model.as_deref());
        data.board_access =
            self.attached_model_native_access(id)
                .map(|active| chrome::access::BoardAccessView {
                    active,
                    interactive: !self.tab().read_only,
                });
        data.chat_host = self.bound_chat_generator_host(id);
        data.provider = agent.map_or("", |agent| agent.provider.as_str());
        data.selected = agent.map(|a| &a.model_parameters);
        data.access = agent.map(|agent| chrome::access::AccessView {
            enabled: self.dashboard_full_access(),
            effective: !self.tab().read_only
                && self
                    .agents
                    .sidecar_policies
                    .get(&agent.session)
                    .filter(|_| self.agent_is_running(id))
                    .and_then(|policy| policy.full_access)
                    .unwrap_or_else(|| {
                        (if self.is_artifact_node(id) {
                            self.agents.access_snapshot.artifact_granted(&agent.session)
                        } else {
                            self.agent_full_access(&agent.session)
                        }) && self.crosstalk_policy(&agent.session)
                            != atlas_agent::TurnPolicy::ReadOnly
                    }),
            interactive: !self.tab().read_only,
        });
        data
    }
}
