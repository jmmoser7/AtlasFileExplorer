//! Saved output references adapt to the existing save snapshot and worker owner.
use super::*;
use slate_doc::scene::HiddenOutputRef;

impl SlateApp {
    pub(super) fn non_image_result_payload(
        &mut self,
        address: &ResultAddress,
    ) -> (slate_doc::SlateDoc, Vec<Node>) {
        let mut snapshot = slate_doc::SlateDoc::new("saved results");
        let Some(host) = self.doc().scene.node(address.host).cloned() else {
            return (snapshot, Vec::new());
        };
        let mut nodes = Vec::new();
        for reference in &address.references {
            let mut node = host.clone();
            match reference {
                HiddenOutputRef::Model { ticket } => {
                    if self.grasshopper_model_output(address.host, ticket)
                        && self.active_result_reference(address.host).as_ref() != Some(reference)
                    {
                        self.toast(
                            "Run this Grasshopper revision before copying it onto the canvas.",
                        );
                        return (snapshot, Vec::new());
                    }
                    let output = self
                        .doc()
                        .scene
                        .node(address.host)
                        .and_then(slate_doc::agent_chat::agent)
                        .and_then(|agent| {
                            agent
                                .generator
                                .model_outputs
                                .iter()
                                .find(|output| &output.id == ticket)
                        })
                        .cloned();
                    let Some(output) = output else {
                        return (snapshot, Vec::new());
                    };
                    let path = slate_doc::scene::resolve_source(
                        self.tab().path.as_deref(),
                        &output.source.locator,
                    );
                    let item = self.item_for_ready_path(&path, output.size, output.mtime);
                    let NodeKind::Image(image) = &mut node.kind else {
                        return (snapshot, Vec::new());
                    };
                    image.item = item;
                    image.agent = None;
                }
                HiddenOutputRef::Text {
                    review_id,
                    response,
                } => {
                    if let NodeKind::Image(image) = &node.kind {
                        let Some(agent) = slate_doc::agent_chat::agent(&host).cloned() else {
                            return (snapshot, Vec::new());
                        };
                        let Some(source) = self
                            .active_authored_text(address.host)
                            .map(str::to_owned)
                            .or_else(|| {
                                self.text_answer_source(address.host).map(|(text, _)| text)
                            })
                        else {
                            return (snapshot, Vec::new());
                        };
                        let mut text = slate_doc::scene::TextNode::agent_note(agent);
                        text.text = source;
                        text.answers = image.answers.clone();
                        node.kind = NodeKind::Text(text);
                    }
                    let NodeKind::Text(text) = &mut node.kind else {
                        return (snapshot, Vec::new());
                    };
                    let authored = self.active_authored_text(address.host).map(str::to_owned);
                    if let Some(authored) = authored {
                        text.text = authored;
                    } else if review_id.is_none() && !self.text_response_only(address.host) {
                        let Some(turn) = self
                            .agents
                            .session(address.host)
                            .filter(|_| {
                                self.doc()
                                    .scene
                                    .node(address.host)
                                    .and_then(slate_doc::agent_chat::agent)
                                    .is_some_and(|agent| agent.session == response.session)
                            })
                            .and_then(|session| {
                                response
                                    .through
                                    .checked_sub(1)
                                    .and_then(|index| session.turns.get(index))
                            })
                            .filter(|turn| turn.role == "assistant")
                        else {
                            return (snapshot, Vec::new());
                        };
                        text.text = turn.text.clone();
                    }
                    // Reviews are private suggestions. Only existing authored
                    // words/intentional links survive; no review reply is copied.
                    text.agent = None;
                }
                HiddenOutputRef::Artifact { family, revision } => {
                    let Some(binding) = slate_doc::scene::artifact_mut(&mut node) else {
                        return (snapshot, Vec::new());
                    };
                    if &binding.family != family {
                        return (snapshot, Vec::new());
                    }
                    binding.revision = Some(revision.clone());
                    let Ok(reference) = self.artifact_reference(address.host, revision) else {
                        return (snapshot, Vec::new());
                    };
                    let NodeKind::Portal(portal) = &mut node.kind else {
                        return (snapshot, Vec::new());
                    };
                    portal.source = Some(slate_doc::SourceUri {
                        locator: super::super::super::board_portal::source_locator(
                            self.tab().path.as_deref(),
                            &reference.materialized_entry,
                        ),
                    });
                    portal.agent = None;
                }
                HiddenOutputRef::Image { .. } => return (snapshot, Vec::new()),
            }
            node.id = NodeId(u64::MAX - nodes.len() as u64);
            nodes.push(node);
        }
        snapshot.items = self.doc().items.clone();
        (snapshot, nodes)
    }
}
