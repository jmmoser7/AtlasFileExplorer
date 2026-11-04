//! Durable response options review source text; they are never authored words.
use super::*;
use slate_doc::scene::SceneCmd;
use slate_doc::text_answers::{TextAnswerRef, TextOutputRef, TextReview};
use std::ops::Range;

impl SlateApp {
    pub(crate) fn activate_text_review_mode(&mut self, id: NodeId) {
        if self.text_response_only(id) || self.tab().read_only {
            return;
        }
        let selected = self
            .resolved_selected_text_output(id)
            .ok()
            .flatten()
            .or_else(|| {
                self.text_answer_source(id)
                    .filter(|(text, _)| text.is_empty())
                    .and_then(|_| self.agent_note_reply(id))
            });
        self.patch_nodes(&[id], |node| {
            if let Some(text) = &selected {
                match &mut node.kind {
                    NodeKind::Text(t) => t.text.clone_from(text),
                    NodeKind::Image(i) => i.text_override = Some(text.clone()),
                    _ => {}
                }
            }
            if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                agent.generator.text_response_only = true;
                agent.generator.preserve_history =
                    Some(agent.generator.preserve_history.unwrap_or(true));
                agent.generator.shown_text = None;
                agent.generator.text_context.clear();
            }
        });
    }
    pub(crate) fn dispatch_text_review_action(
        &mut self,
        ctx: &egui::Context,
        detail: Option<&str>,
        apply: bool,
    ) -> bool {
        #[derive(serde::Deserialize)]
        struct Target {
            node: NodeId,
        }
        let Some(target) = detail.and_then(|d| serde_json::from_str::<Target>(d).ok()) else {
            return false;
        };
        if apply {
            self.apply_text_review(ctx, target.node)
        } else {
            self.dismiss_text_review(target.node)
        }
    }
    pub(crate) fn text_response_only(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.generator.text_response_only)
    }

    pub(crate) fn text_review(&self, id: NodeId) -> Option<TextReview> {
        self.text_review_view(id).cloned()
    }
    pub(crate) fn text_review_view(&self, id: NodeId) -> Option<&TextReview> {
        let settings = &slate_doc::agent_chat::agent(self.doc().scene.node(id)?)?.generator;
        settings.text_reviews.get(settings.shown_review?)
    }

    pub(crate) fn record_text_review(
        &mut self,
        id: NodeId,
        request: &str,
        group: &str,
        context_epoch: u64,
        source: String,
        source_revision: Option<String>,
        selection: Option<Range<usize>>,
        question: String,
        text: String,
        response: TextAnswerRef,
    ) -> bool {
        let Some(origin) = self.text_output_source(id) else {
            return false;
        };
        let words = [
            "rewrite", "rephrase", "shorten", "concise", "simplify", "improve", "edit", "clarify",
            "tighter", "clearer",
        ];
        let edit = question.split(|c: char| !c.is_alphabetic()).any(|w| {
            words
                .iter()
                .any(|candidate| w.eq_ignore_ascii_case(candidate))
        });
        let review = TextReview {
            id: request.to_owned(),
            group: group.to_owned(),
            source,
            source_revision,
            selection,
            question,
            text,
            edit,
            output: TextOutputRef {
                review_id: Some(request.to_owned()),
                source: origin,
                response,
            },
        };
        let first = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_none_or(|a| !a.generator.text_reviews.iter().any(|r| r.group == group));
        self.patch_nodes_as(
            &[id],
            slate_doc::scene::CmdAuthor::Agent("text response".into()),
            |node| {
                let Some(agent) = slate_doc::agent_chat::agent_mut(node) else {
                    return;
                };
                if agent
                    .generator
                    .text_reviews
                    .iter()
                    .any(|old| old.output == review.output)
                {
                    return;
                }
                let index = agent.generator.text_reviews.len();
                agent.generator.text_reviews.push(review.clone());
                if first {
                    agent.generator.shown_review = Some(index);
                    if agent.generator.preserve_history == Some(true)
                        && context_epoch != u64::MAX
                        && agent.generator.text_context_epoch.unwrap_or(0) == context_epoch
                    {
                        agent.generator.text_context.extend([
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
                        ]);
                    } else if agent.generator.preserve_history != Some(true) {
                        agent.generator.text_context.clear();
                    }
                }
                agent.generator.shown_text = None;
            },
        );
        first
    }

    pub(crate) fn link_text_review(&mut self, id: NodeId, request: &str, range: &Range<usize>) {
        self.patch_nodes_as(
            &[id],
            slate_doc::scene::CmdAuthor::Agent("text response".into()),
            |node| {
                let answers = match &mut node.kind {
                    NodeKind::Text(t) => &mut t.answers,
                    NodeKind::Image(i) => &mut i.answers,
                    _ => return,
                };
                if let Some(answer) = answers
                    .iter_mut()
                    .find(|a| a.start == range.start && a.end == range.end)
                {
                    answer.review_id = Some(request.to_owned());
                }
            },
        );
    }

    pub(crate) fn prepare_text_review_selection(
        &self,
        id: NodeId,
        index: Option<usize>,
    ) -> Result<SceneCmd, String> {
        let before = self.doc().scene.node(id).ok_or("Text source is missing")?;
        if self.tab().read_only || before.locked {
            return Err("Text review is read-only or locked".into());
        }
        let mut after = before.clone();
        let agent =
            slate_doc::agent_chat::agent_mut(&mut after).ok_or("Text source has no responses")?;
        if index.is_some_and(|index| index >= agent.generator.text_reviews.len()) {
            return Err("Text response is unavailable".into());
        }
        agent.generator.shown_review = index;
        agent.generator.shown_text = None;
        let review = index
            .and_then(|i| agent.generator.text_reviews.get(i))
            .cloned();
        if let Some(review) = review {
            if let Some(range) = review.selection.clone() {
                if self
                    .text_answer_source(id)
                    .is_some_and(|(source, revision)| {
                        review.is_current(&source, revision.as_deref())
                    })
                {
                    if let Some(mut link) = slate_doc::text_answers::TextAnswer::new(
                        &review.source,
                        range,
                        review.output.response.clone(),
                        review.question,
                        review.text,
                    ) {
                        link.review_id = Some(review.id);
                        link.origin = Some(review.output.source);
                        link.source_revision = review.source_revision;
                        match &mut after.kind {
                            NodeKind::Text(t) => {
                                slate_doc::text_answers::upsert(&mut t.answers, &t.text, link);
                            }
                            NodeKind::Image(i) => {
                                slate_doc::text_answers::upsert(
                                    &mut i.answers,
                                    &review.source,
                                    link,
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if before == &after {
            return Err("This response is already selected".into());
        }
        Ok(SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(after),
        })
    }

    pub(crate) fn dismiss_text_review(&mut self, id: NodeId) -> bool {
        let Ok(command) = self.prepare_text_review_selection(id, None) else {
            return false;
        };
        self.commit_scene(vec![command])
    }

    /// Only an explicit human Replace reaches the ordinary authored-text helper.
    /// The immutable source/revision/range is rechecked immediately before commit.
    pub(crate) fn apply_text_review(&mut self, _ctx: &egui::Context, id: NodeId) -> bool {
        let Some(review) = self.text_review(id) else {
            return false;
        };
        let Some((source, revision)) = self.text_answer_source(id) else {
            return false;
        };
        let Some(replacement) = review.replacement(&source, revision.as_deref()) else {
            self.toast("The source text changed. Ask for a new revision before replacing it.");
            return false;
        };
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        if self.tab().read_only || before.locked {
            return false;
        }
        let mut after = before.clone();
        if matches!(&after.kind, NodeKind::Image(_)) {
            slate_doc::authored_text::apply_document_text_from(
                &mut after,
                &source,
                &replacement,
                before.rect,
            );
        } else {
            slate_doc::authored_text::apply_inline_edit(&mut after, &replacement, None);
        }
        if let Some(agent) = slate_doc::agent_chat::agent_mut(&mut after) {
            agent.generator.shown_review = None;
        }
        if before == after {
            return false;
        }
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }])
    }
}
