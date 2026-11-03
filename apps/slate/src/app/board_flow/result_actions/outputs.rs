//! Typed presentation identities over the existing immutable output owners.
use super::*;
use slate_doc::scene::HiddenOutputRef;

pub(crate) struct Output {
    pub(crate) id: String,
    pub(crate) reference: HiddenOutputRef,
}

impl SlateApp {
    pub(crate) fn output_is_hidden(&self, host: NodeId, reference: &HiddenOutputRef) -> bool {
        self.doc()
            .scene
            .node(host)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|agent| agent.generator.hidden_outputs.contains(reference))
    }

    pub(crate) fn result_outputs(&self, host: NodeId) -> Vec<Output> {
        let Some(node) = self.doc().scene.node(host) else {
            return Vec::new();
        };
        let Some(agent) = slate_doc::agent_chat::agent(node) else {
            return Vec::new();
        };
        let outputs = if let Some(artifact) = slate_doc::scene::artifact(node) {
            artifact
                .revisions
                .iter()
                .map(|revision| Output {
                    id: revision.clone(),
                    reference: HiddenOutputRef::Artifact {
                        family: artifact.family.clone(),
                        revision: revision.clone(),
                    },
                })
                .collect::<Vec<_>>()
        } else if !agent.generator.model_outputs.is_empty() {
            agent
                .generator
                .model_outputs
                .iter()
                .filter(|output| Self::valid_model_output_identity(&output.id))
                .map(|output| Output {
                    id: output.id.clone(),
                    reference: HiddenOutputRef::Model {
                        ticket: output.id.clone(),
                    },
                })
                .collect()
        } else if agent.generator.text_response_only {
            agent
                .generator
                .text_reviews
                .iter()
                .map(|review| Output {
                    id: review.id.clone(),
                    reference: HiddenOutputRef::Text {
                        review_id: Some(review.id.clone()),
                        response: review.output.response.clone(),
                    },
                })
                .collect()
        } else if matches!(&node.kind, NodeKind::Text(_)) || agent.view == PortalView::Text {
            self.agents.session(host).map_or_else(Vec::new, |session| {
                session
                    .turns
                    .iter()
                    .enumerate()
                    .filter(|(_, turn)| turn.role == "assistant")
                    .map(|(raw, _)| Output {
                        id: format!("{}:{}", agent.session, raw + 1),
                        reference: HiddenOutputRef::Text {
                            review_id: None,
                            response: slate_doc::text_answers::TextAnswerRef {
                                session: agent.session.clone(),
                                through: raw + 1,
                            },
                        },
                    })
                    .collect()
            })
        } else {
            self.agent_images(host)
                .iter()
                .map(|image| Output {
                    id: image.id.clone(),
                    reference: HiddenOutputRef::Image {
                        request: image.request.clone(),
                        id: image.id.clone(),
                    },
                })
                .collect()
        };
        outputs
            .into_iter()
            .filter(|output| !agent.generator.hidden_outputs.contains(&output.reference))
            .collect()
    }

    /// Stable visible identity, label and actual saved preview wording. A review
    /// previews its captured source; selecting it never authors its proposed text.
    pub(crate) fn result_text_entries(&self, host: NodeId) -> Vec<(String, String, String)> {
        let Some(agent) = self
            .doc()
            .scene
            .node(host)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return Vec::new();
        };
        self.result_outputs(host)
            .into_iter()
            .filter_map(|output| {
                let HiddenOutputRef::Text {
                    review_id,
                    response,
                } = output.reference
                else {
                    return None;
                };
                if let Some(id) = review_id {
                    let review = agent
                        .generator
                        .text_reviews
                        .iter()
                        .find(|review| review.id == id && review.output.response == response)?;
                    Some((output.id, review.question.clone(), review.source.clone()))
                } else {
                    if agent.session != response.session {
                        return None;
                    }
                    let turn = self
                        .agents
                        .session(host)?
                        .turns
                        .get(response.through.checked_sub(1)?)?;
                    (turn.role == "assistant")
                        .then(|| (output.id, turn.text.clone(), turn.text.clone()))
                }
            })
            .collect()
    }

    pub(crate) fn open_result_context(
        &mut self,
        ui: &egui::Ui,
        host: NodeId,
        interaction: ResultInteraction,
    ) {
        let Some((index, at)) = interaction.menu else {
            return;
        };
        let Some(binding) = self.result_binding(host) else {
            return;
        };
        let available = self.result_outputs(host);
        let Some(target) = available.get(index).map(|output| output.id.clone()) else {
            return;
        };
        let ordered = available
            .iter()
            .map(|output| output.id.clone())
            .collect::<Vec<_>>();
        let tab = self.tab().id;
        let selection = self
            .agents
            .flow
            .results
            .selections
            .entry((tab, host, binding.clone()))
            .or_default();
        selection.select(&ordered, index, interaction.shift, true);
        let outputs = selection
            .identities(&ordered)
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let references = available
            .into_iter()
            .filter(|output| outputs.contains(&output.id))
            .map(|output| output.reference)
            .collect();
        self.agents.flow.results.menu = Some(ResultMenu {
            address: ResultAddress {
                tab,
                host,
                binding,
                outputs,
                references,
                target: Some(target),
                witness: self.doc().scene.node(host).cloned().map(Box::new),
            },
            at,
            opener: ui.layer_id(),
        });
        self.agents.flow.set_hud_hidden(host, false);
        ui.ctx().request_repaint();
    }

    /// A square chooses output content; circles retain the source's Agent band.
    pub(crate) fn select_square_result(&mut self, host: NodeId) {
        self.board_sel.remove(&host);
        if self.shape_properties.ids == [host] {
            self.shape_properties.panel = None;
            self.forget_object_panel(host);
        }
        if self.agents.focused == Some(host) {
            self.agent_blur();
        }
    }
}
