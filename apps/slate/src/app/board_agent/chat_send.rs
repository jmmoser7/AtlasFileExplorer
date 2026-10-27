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
        if let Err(error) = self.validate_chat_train_send(portal) {
            self.fail_agent_await(portal, error);
            return None;
        }
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
        let request_id = atlas_ai::agent::request_id();
        let provider_workspace = self.agent_folder_for(portal).unwrap_or_else(|| ws.clone());
        let collaborator =
            match self.bind_chat_train_request(portal, &provider_workspace, &request_id) {
                Ok(binding) => binding,
                Err(error) => {
                    self.fail_agent_await(portal, error);
                    return None;
                }
            };
        let req = AgentRequest {
            collaborator,
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
            id: request_id,
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
            self.revoke_chat_train_request(portal, &req.id);
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

#[cfg(test)]
mod native_tools_tests {
    use super::*;
    use slate_doc::scene::{ConnectorEnd, Side};

    #[test]
    fn dispatched_chat_train_request_carries_real_board_membership() {
        let mut h = crate::app::tests::Harness::new("chat_native_membership");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.ai.config.workspace_dir = Some(h.base.clone());
        let (model, _peer) = crate::app::application_connector::native::test_support::install(
            &mut h,
            atlas_connector::SourceMode::Linked,
        );
        let chat = super::super::train_setup::train(&mut h, Pos2::new(800.0, 0.0), "codex");
        let wire = h.app.build_connector_with(
            ConnectorEnd::Anchored {
                node: model,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: chat,
                side: Side::Left,
                t: 0.5,
            },
            &[],
        );
        h.app.add_nodes(vec![wire]);
        h.app.board_sel = std::iter::once(chat).collect();
        assert!(h.app.open_chat_train_board_access());
        let workspace = h.base.clone();
        let sent = h.app.send_agent_prompt_prepared(
            chat,
            chat,
            "Inspect the wired Rhino model".into(),
            workspace,
            super::super::runtime::ChatSendOrigin {
                typing_here: false,
                relayed_from: None,
                preserve_composer: false,
            },
            Default::default(),
        );
        assert!(sent.is_some());
        let (_, request) = h
            .app
            .agents
            .dispatched
            .last()
            .expect("real outbound request captured");
        assert!(request
            .collaborator
            .as_ref()
            .is_some_and(|binding| binding.validate().is_ok()));
        assert_eq!(request.prompt, "Inspect the wired Rhino model");
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
