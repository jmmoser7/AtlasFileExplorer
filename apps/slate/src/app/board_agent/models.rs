//! Model list and the live toggle.

use super::*;

impl SlateApp {
    pub(super) fn pump_agent_models(&mut self) {
        if let Some((provider, result)) = self
            .agents
            .models_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.agents.models_rx = None;
            match result {
                Ok(models) => {
                    self.agents.models.insert(provider.clone(), models);
                    self.agents.models_error.remove(&provider);
                }
                Err(error) => {
                    self.agents.models_error.insert(provider, error);
                }
            }
            self.agents.fit_revision = None;
        }
        if self.agents.models_rx.is_some() {
            return;
        }
        let provider = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(slate_doc::agent_chat::agent)
            .map(|a| atlas_ai::agent::model_catalog(&a.provider))
            .chain(self.agents.models_wanted.iter().map(String::as_str))
            .find(|p| {
                matches!(
                    *p,
                    "codex" | "ollama" | "cursor" | "comfy" | "openai-image" | "openai-text"
                ) && !self.agents.models_started.contains(*p)
            })
            .map(str::to_owned);
        if let Some(provider) = provider {
            self.agents.models_started.insert(provider.clone());
            let (tx, rx) = unbounded();
            self.agents.models_rx = Some(rx);
            std::thread::spawn(move || {
                let result = match provider.as_str() {
                    "codex" => atlas_ai::runtime::codex_models(),
                    "cursor" => atlas_ai::runtime::cursor_models(),
                    "comfy" => atlas_ai::runtime::comfy_models(),
                    "openai-image" => atlas_ai::runtime::openai_image_models(),
                    "openai-text" => atlas_ai::runtime::openai_text_models(),
                    _ => atlas_ai::runtime::local_models(),
                };
                let _ = tx.send((provider, result));
            });
        }
    }
    pub(crate) fn agent_toggle_live(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self
            .agent_session_for(id)
            .is_none_or(|(_, provider)| !atlas_ai::agent::local_image_engine(&provider))
        {
            return false;
        }
        let enable = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| !a.live);
        self.set_generator_live(id, enable);
        true
    }
    /// Live is journaled; its run identity and last input signature are not.
    pub(crate) fn set_generator_live(&mut self, id: NodeId, live: bool) {
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.live = live;
            }
            // Live shows every new frame, so a picked result lets go.
            if let NodeKind::Image(i) = &mut n.kind {
                if live {
                    i.item = slate_doc::ItemId::NONE;
                }
            }
        });
        self.agents.live_sent.remove(&id);
        if live {
            self.agents
                .live_run
                .insert(id, atlas_ai::agent::request_id());
        } else {
            self.agents.live_run.remove(&id);
            if let Some(model) = self.agents.steering.remove(&id) {
                if self.model3d.live.contains_key(&model) {
                    self.lock_model(model);
                }
            }
        }
    }
    /// The shown live frame stays in the album; the next frame starts a new slot.
    pub(crate) fn agent_keep_live(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if !self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| atlas_ai::agent::local_image_engine(&a.provider) && a.live)
        {
            self.toast("Keep applies while Live is on.");
            return false;
        }
        self.agents
            .live_run
            .insert(id, atlas_ai::agent::request_id());
        true
    }
    pub(crate) fn agent_set_model(&mut self, value: &str) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        // A sent card records the model that answered it; only the tail chooses.
        if self.agent_is_running(id) || self.agent_has_child(id) {
            return false;
        }
        let Some((_, provider)) = self.agent_session_for(id) else {
            return false;
        };
        let local = provider.starts_with("ollama");
        let key = atlas_ai::agent::model_catalog(&provider);
        // An empty choice is the provider's default (Auto for ComfyUI); a local
        // chat model is always explicit.
        if value.is_empty() && local && !self.is_text_block(id) {
            return false;
        }
        if !value.is_empty()
            && !self
                .agents
                .models
                .get(key)
                .is_some_and(|models| models.iter().any(|m| m.id == value))
        {
            return false;
        }
        let model = (!value.is_empty()).then(|| value.to_owned());
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.model = model.clone();
                if local {
                    a.provider = "ollama".into();
                }
            }
        });
        true
    }
    pub(super) fn refresh_agent_connection(&mut self) {
        if self.agents.connection_rx.is_some()
            || self
                .agents
                .connection_tick
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
        {
            return;
        }
        let Some(id) = self.selected_agent_portal() else {
            return;
        };
        if !self.agent_linear(id)
            || self.agent_is_running(id)
            || self.agents.project_picker.is_some()
            || self.agents.chat_picker.is_some()
        {
            return;
        }
        let Some(state) = self.agents.session(id) else {
            return;
        };
        if state.conversation.is_empty()
            || (state.provider != "codex" && state.provider != "cursor")
        {
            return;
        }
        let channel = state.conversation.clone();
        if let Some((session, provider)) = self.agent_session_for(id) {
            if provider == "codex" {
                self.agents.codex.remove(&session);
            } else if self.agents.sidecar_child.contains_key(&session)
                || self.agents.sidecar_booting.contains(&session)
            {
                return;
            }
        }
        self.load_agent_connection(id, channel, true);
    }
}
