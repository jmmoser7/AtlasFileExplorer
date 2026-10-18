//! Deferred chat sends after image input preparation.

use super::*;

impl SlateApp {
    pub(super) fn send_agent_prompt_prepared(
        &mut self,
        portal: NodeId,
        composer: NodeId,
        prompt: String,
        ws: PathBuf,
        origin: super::runtime::ChatSendOrigin,
        inputs: atlas_agent::InputSnapshot,
    ) -> Option<NodeId> {
        let human = origin.preserve_composer.then(|| {
            (
                self.board_sel.clone(),
                self.agents.composer_editing,
                self.agents.composer_focus,
            )
        });
        let sent =
            self.send_agent_prompt_prepared_inner(portal, composer, prompt, ws, origin, inputs);
        if let Some((selection, editing, focus)) = human {
            self.board_sel = selection;
            self.agents.composer_editing = editing;
            self.agents.composer_focus = focus;
        }
        sent
    }
    fn send_agent_prompt_prepared_inner(
        &mut self,
        portal: NodeId,
        composer: NodeId,
        prompt: String,
        ws: PathBuf,
        origin: super::runtime::ChatSendOrigin,
        inputs: atlas_agent::InputSnapshot,
    ) -> Option<NodeId> {
        let (portal, history) = self.prepare_agent_train_send(portal, &ws)?;
        if !origin.preserve_composer {
            self.consume_agent_context(composer);
        }
        // The next message is typed on the new tail. A send that waited to
        // connect leaves the caret wherever the person has moved since.
        if origin.typing_here && portal != composer {
            self.agents.composer_editing = None;
            self.agents.composer_focus = Some(portal);
            self.agent_focus(portal);
        }
        let (session, _) = self.agent_session_for(portal)?;
        let req = AgentRequest {
            collaborator: None,
            model: self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.model.clone()),
            model_parameters: self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .map(|a| a.model_parameters.clone())
                .unwrap_or_default(),
            history,
            inputs,
            id: atlas_ai::agent::request_id(),
            prompt,
            at: atlas_ai::context::now_secs(),
            image: None,
            artifact: None,
            output_dir: self.agent_output_dir(portal, &ws, &session),
            oneshot: false,
            policy: self.crosstalk_policy(&session),
            relayed_from: origin.relayed_from,
            goal: self.crosstalk_goal(&session),
        };
        if !self.submit_prepared_agent_request(portal, req.clone(), &ws) {
            return None;
        }
        let turn = AgentTurn {
            role: "user".into(),
            text: req.prompt.clone(),
            at: req.at,
        };
        if !self.agents.local_turns.contains_key(&portal) {
            // A linear provider's request carries no history, and a new
            // tail has no session yet: the stream the composer shows is
            // what the whole train keeps displaying.
            let same_stream = self
                .doc()
                .scene
                .node(composer)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.session == session);
            let seed = self
                .agents
                .sessions
                .get(&portal)
                .map(|s| s.turns.clone())
                .or_else(|| {
                    same_stream
                        .then(|| self.agent_all_turns(composer))
                        .filter(|t| !t.is_empty())
                })
                .unwrap_or_else(|| req.history.clone());
            self.agents.local_turns.insert(portal, seed);
        }
        let turns = self.agents.local_turns.get_mut(&portal).unwrap();
        if turns
            .last()
            .is_none_or(|t| t.role != "user" || t.text != turn.text)
        {
            turns.push(turn);
        }
        let pending_turns = turns.clone();
        let siblings: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| {
                slate_doc::agent_chat::agent(n)
                    .filter(|a| a.session == session && n.id != portal)
                    .map(|_| n.id)
            })
            .collect();
        for id in siblings {
            self.agents.bindings.insert(id, session.clone());
            self.agents.local_turns.insert(id, pending_turns.clone());
        }
        if !origin.preserve_composer {
            self.agents.prompt_mut(portal).clear();
            self.agents.prompt_mut(composer).clear();
        }
        Some(portal)
    }
}

impl SlateApp {
    pub(super) fn pump_pending_chat_sends(&mut self, ctx: &egui::Context) {
        let ids: Vec<_> = self.agents.chat_preparing.keys().copied().collect();
        for id in ids {
            let Some(mut pending) = self.agents.chat_preparing.get(&id).cloned() else {
                continue;
            };
            if pending.doc != self.tab().id {
                self.agents.chat_preparing.remove(&id);
                self.toast(
                    "The pending send belongs to a different open tab; send it again there.",
                );
                continue;
            }
            match self.poll_agent_input_preparation(&mut pending.inputs) {
                Ok(true) => {
                    self.agents.chat_preparing.remove(&id);
                    self.send_agent_prompt_prepared(
                        pending.portal,
                        pending.composer,
                        pending.prompt,
                        pending.workspace,
                        pending.origin,
                        pending.inputs.inputs,
                    );
                }
                Ok(false) => ctx.request_repaint_after(Duration::from_millis(30)),
                Err(error) => {
                    self.agents.chat_preparing.remove(&id);
                    self.fail_agent_await(id, error);
                }
            }
        }
    }

    pub(super) fn cancel_preparing_agent_send(&mut self, id: NodeId) -> bool {
        self.agents.chat_preparing.remove(&id).is_some()
    }
}
