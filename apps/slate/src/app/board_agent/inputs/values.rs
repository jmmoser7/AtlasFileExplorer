//! Per-source projection for authored wire snapshots. No persistent cache.
use super::*;

#[cfg(test)]
thread_local! {
    static LEGACY_PROJECTION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static PROJECTED_NODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl SlateApp {
    #[cfg(test)]
    pub(crate) fn legacy_input_projection_for_test(&self, legacy: bool) {
        LEGACY_PROJECTION.with(|flag| flag.set(legacy));
    }

    pub(super) fn projected_agent_input_snapshot(
        &self,
        id: NodeId,
        check_source: impl FnMut(NodeId) -> Result<(), String>,
    ) -> Result<atlas_ai::agent::InputSnapshot, String> {
        // Equivalent eager projection is retained only as a measurement/control
        // path in tests. It uses this same value owner and pure wire interpreter.
        #[cfg(test)]
        if LEGACY_PROJECTION.with(std::cell::Cell::get) {
            let outputs = self
                .doc()
                .scene
                .nodes
                .iter()
                .filter_map(|node| {
                    self.projected_agent_input_value(node)
                        .map(|value| (node.id, value))
                })
                .collect();
            return slate_doc::agent_inputs::snapshot_with_source_check(
                self.doc(),
                id,
                AgentContextScope::Selection,
                &[],
                &outputs,
                check_source,
            );
        }
        slate_doc::agent_inputs::snapshot_with_value_source(
            self.doc(),
            id,
            AgentContextScope::Selection,
            &[],
            |source| {
                self.doc()
                    .scene
                    .node(source)
                    .and_then(|node| self.projected_agent_input_value(node))
            },
            check_source,
        )
    }

    fn projected_agent_input_value(
        &self,
        node: &slate_doc::Node,
    ) -> Option<atlas_ai::agent::ContextItem> {
        #[cfg(test)]
        PROJECTED_NODES.with(|count| count.set(count.get() + 1));
        // A model supplies its displayed geometry, even when an attached Chat
        // agent has no image album. The existing capture pass renders its view
        // and depth after the request freezes this current source reference.
        if let Some(model) = self.model_node_info(node.id) {
            return Some(atlas_ai::agent::ContextItem {
                node: node.id.0,
                text: String::new(),
                images: vec![model.path.to_string_lossy().into_owned()],
                outputs: Default::default(),
                active: None,
                depth: None,
                slot: None,
            });
        }
        let mut value = None;
        let mut selected_blocks_document = false;
        if slate_doc::agent_chat::is_agent_node(node) {
            let selected = self.resolved_selected_text_output(node.id);
            let images = self
                .agent_images(node.id)
                .iter()
                .map(|i| {
                    resolve_source(self.tab().path.as_deref(), &i.source)
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            let latest = || {
                self.agents
                    .sessions
                    .get(&node.id)
                    .filter(|s| s.status == AgentStatus::Idle)
                    .and_then(|s| s.turns.iter().rev().find(|t| t.role == "assistant"))
                    .map(|t| t.text.clone())
                    .unwrap_or_default()
            };
            let text = if self.text_response_only(node.id) {
                self.text_answer_source(node.id)
                    .map(|(text, _)| text)
                    .unwrap_or_default()
            } else {
                self.active_authored_text(node.id)
                    .map(str::to_string)
                    .unwrap_or_else(|| match &selected {
                        Ok(Some(text)) => text.clone(),
                        Ok(None) => latest(),
                        Err(_) => String::new(),
                    })
            };
            selected_blocks_document = matches!(selected, Ok(Some(_)) | Err(_));
            let image_outputs = self
                .agent_images(node.id)
                .iter()
                .map(|i| {
                    (
                        i.id.clone(),
                        resolve_source(self.tab().path.as_deref(), &i.source)
                            .to_string_lossy()
                            .into_owned(),
                    )
                })
                .collect();
            let active = self.agent_active_output(node.id);
            value = Some(atlas_ai::agent::ContextItem {
                node: node.id.0,
                text,
                images,
                outputs: image_outputs,
                active,
                depth: None,
                slot: None,
            });
        }
        // A placed text document supplies the words its card shows. Read
        // once by the card or a queued run (`snippet_for`), never here.
        if let NodeKind::Image(image) = &node.kind {
            let text_doc = self.doc().item(image.item).is_some_and(|item| {
                slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Text
            });
            if text_doc && !selected_blocks_document {
                let text = self
                    .active_authored_text(node.id)
                    .map(str::to_string)
                    .or_else(|| image.text_override.clone())
                    .or_else(|| self.cached_snippet(image.item).map(str::to_string))
                    .unwrap_or_default();
                value = Some(atlas_ai::agent::ContextItem {
                    node: node.id.0,
                    text,
                    images: vec![],
                    outputs: Default::default(),
                    active: None,
                    depth: None,
                    slot: None,
                });
            }
        }
        value
    }
}

#[cfg(test)]
mod tests;
