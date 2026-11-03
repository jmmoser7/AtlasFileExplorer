//! Display references are independent from provider history and authored note text.
use super::*;
use slate_doc::text_answers::{TextAnswerRef, TextOutputRef};
impl SlateApp {
    pub(crate) fn text_output_source(&self, id: NodeId) -> Option<slate_doc::scene::SourceUri> {
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)?;
        agent.bundle.clone().or_else(|| {
            let workspace = self.ai.config.workspace_dir.as_deref()?;
            let path = self.agent_link_dir(id, workspace)?.join("session.json");
            Some(slate_doc::scene::SourceUri {
                locator: super::super::super::board_portal::source_locator(
                    self.tab().path.as_deref(),
                    &path,
                ),
            })
        })
    }
    pub(crate) fn select_text_output(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        struct Choice {
            node: NodeId,
            through: usize,
            #[serde(default)]
            review_id: Option<String>,
        }
        let Some(choice) = detail.and_then(|detail| serde_json::from_str::<Choice>(detail).ok())
        else {
            return false;
        };
        let Ok(command) = self.prepare_text_output_review(
            choice.node,
            choice.through,
            choice.review_id.as_deref(),
        ) else {
            return false;
        };
        self.commit_scene(vec![command])
    }
    pub(crate) fn prepare_text_output_review(
        &self,
        id: NodeId,
        through: usize,
        review_id: Option<&str>,
    ) -> Result<slate_doc::scene::SceneCmd, String> {
        if self.text_response_only(id) {
            let settings = &slate_doc::agent_chat::agent(
                self.doc().scene.node(id).ok_or("Text source is missing")?,
            )
            .ok_or("Text responses are missing")?
            .generator;
            let mut matches = settings
                .text_reviews
                .iter()
                .enumerate()
                .filter(|(_, review)| {
                    review.output.response.through == through
                        && review_id.is_none_or(|identity| review.id == identity)
                });
            let index = matches
                .next()
                .map(|(index, _)| index)
                .ok_or("Choose an existing text response")?;
            if matches.next().is_some() {
                return Err("Choose the saved response identity; this checkpoint belongs to multiple responses".into());
            }
            return self.prepare_text_review_selection(id, Some(index));
        }
        let before = self.doc().scene.node(id).ok_or("Generator is missing")?;
        if self.tab().read_only || before.locked {
            return Err("Text output is read-only or locked".into());
        }
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .ok_or("Target has no text response history")?;
        let turn = through
            .checked_sub(1)
            .and_then(|index| self.agents.session(id)?.turns.get(index))
            .ok_or("Text response checkpoint is unavailable; open its saved history first")?;
        if turn.role != "assistant" {
            return Err("Choose an existing assistant response checkpoint".into());
        }
        let source = self
            .text_output_source(id)
            .ok_or("Text response history source is unavailable")?;
        let reference = TextOutputRef {
            review_id: None,
            source,
            response: TextAnswerRef {
                session: agent.session.clone(),
                through,
            },
        };
        let mut after = before.clone();
        slate_doc::agent_chat::agent_mut(&mut after)
            .unwrap()
            .generator
            .shown_text = Some(reference);
        if before == &after {
            return Err("This text output is already selected".into());
        }
        Ok(slate_doc::scene::SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(after),
        })
    }
    /// The existing authored editor takes precedence without changing durable selection.
    pub(crate) fn active_authored_text(&self, id: NodeId) -> Option<&str> {
        self.text_edit
            .as_ref()
            .filter(|(node, _)| *node == id)
            .map(|(_, text)| text.as_str())
            .or_else(|| {
                self.text_doc_edit
                    .as_ref()
                    .filter(|edit| edit.node == id)
                    .map(|edit| edit.buffer.as_str())
            })
    }
    pub(crate) fn text_output_is_current(&self, id: NodeId, reference: &TextOutputRef) -> bool {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|agent| agent.session == reference.response.session)
            && self.text_output_source(id).is_some_and(|source| {
                slate_doc::scene::resolve_source(self.tab().path.as_deref(), &source.locator)
                    == slate_doc::scene::resolve_source(
                        self.tab().path.as_deref(),
                        &reference.source.locator,
                    )
            })
    }
    /// Exact displayed reference for authored wire snapshots. A missing source is an error.
    pub(crate) fn resolved_selected_text_output(
        &self,
        id: NodeId,
    ) -> Result<Option<String>, String> {
        if self.text_response_only(id) {
            return Ok(None);
        }
        // The existing editor's live buffer is the active authored content.
        // Keep the display reference durable until a real edit is committed,
        // so Escape/no-op restores it without a second undo transaction.
        if self.active_authored_text(id).is_some() {
            return Ok(None);
        }
        let Some(agent) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return Ok(None);
        };
        let Some(reference) = agent.generator.shown_text.as_ref() else {
            return Ok(None);
        };
        let source_path =
            slate_doc::scene::resolve_source(self.tab().path.as_deref(), &reference.source.locator);
        let current = self.text_output_is_current(id, reference);
        let session = if current {
            self.agents.sessions.get(&id).cloned()
        } else {
            source_path
                .parent()
                .and_then(|dir| self.agents.sources.snapshot(dir))
        }
        .ok_or("The selected text output source has not loaded yet.")?;
        let turn = reference
            .response
            .through
            .checked_sub(1)
            .and_then(|index| session.turns.get(index))
            .filter(|turn| turn.role == "assistant")
            .ok_or("The selected text output checkpoint is unavailable.")?;
        Ok(Some(turn.text.clone()))
    }
    pub(crate) fn displayed_text_output(&self, id: NodeId) -> Option<String> {
        match self.resolved_selected_text_output(id) {
            Ok(Some(text)) => Some(text),
            Ok(None) => self.agent_reply(id),
            Err(_) => None,
        }
    }
    pub(crate) fn choose_text_output_index(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        index: usize,
    ) {
        let Some(output) = self.result_outputs(id).into_iter().nth(index) else {
            return;
        };
        let slate_doc::scene::HiddenOutputRef::Text {
            review_id,
            response,
        } = output.reference
        else {
            return;
        };
        self.dispatch(
            ctx,
            atlas_commands::CommandId("portal.agent.text_output"),
            Some(
                serde_json::json!({"node":id,"through":response.through,"review_id":review_id})
                    .to_string(),
            ),
        );
    }
}
