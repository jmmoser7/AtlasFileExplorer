//! A run must not turn a pending linked document into an empty wire value.

use crate::app::SlateApp;
use slate_doc::{scene::NodeKind, NodeId};

impl SlateApp {
    pub(super) fn check_document_input_preview(&self, source: NodeId) -> Result<(), String> {
        let Some(NodeKind::Image(image)) = self.doc().scene.node(source).map(|node| &node.kind)
        else {
            return Ok(());
        };
        let Some(item) = self.doc().item(image.item) else {
            return Ok(());
        };
        if image.text_override.is_some()
            || slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Text
        {
            return Ok(());
        }
        if !self.prepare_document_card(image.item) {
            return Err(format!(
                "Linked text '{}' is still loading. Run again when its preview appears.",
                item.file_name
            ));
        }
        if self.cached_snippet(image.item).is_none() {
            return Err(format!(
                "Linked text '{}' has no readable local excerpt.",
                item.file_name
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{ConnectorEnd, ImageNode, Side};

    #[test]
    fn pending_wired_text_refuses_the_snapshot_then_uses_completed_words() {
        let (mut h, generator, _) = super::super::super::test_support::generator_with_note(
            "document-input-wait",
            "existing instruction",
        );
        let item =
            h.app
                .doc_mut()
                .add_item(h.base.join("reference.txt"), "reference.txt", 100, 0, "txt");
        let source = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(-600.0, 300.0, 120.0, 120.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        let source_id = source.id;
        let prompt = h
            .app
            .wire_input_ports(h.app.doc().scene.node(generator).unwrap())
            .iter()
            .find(|port| port.slot == atlas_agent::InputSlot::Prompt)
            .expect("the generator exposes a text prompt input")
            .t;
        let wire = h.app.build_connector_with(
            ConnectorEnd::Anchored {
                node: source_id,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: generator,
                side: Side::Left,
                t: prompt,
            },
            std::slice::from_ref(&source),
        );
        h.app.add_nodes(vec![source, wire]);
        let error = h.app.agent_input_snapshot(generator).unwrap_err();
        assert!(error.contains("still loading"), "{error}");
        assert!(error.contains("reference.txt"));
        h.app
            .complete_document_preview_for_test(item, "prepared reference words");
        let inputs = h.app.agent_input_snapshot(generator).unwrap();
        assert!(inputs
            .wired
            .iter()
            .any(|input| input.node == source_id.0 && input.text == "prepared reference words"));
    }

    #[test]
    fn readiness_checks_only_active_sources_and_preserves_empty_text_rejection() {
        let (mut h, generator, note) = super::super::super::test_support::generator_with_note(
            "document-input-scope",
            "existing instruction",
        );
        let item =
            h.app
                .doc_mut()
                .add_item(h.base.join("unwired.txt"), "unwired.txt", 100, 0, "unwired");
        let source = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(-600.0, 300.0, 120.0, 120.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        let unwired = h.app.add_nodes(vec![source])[0];
        let mut checked = Vec::new();
        let inputs = slate_doc::agent_inputs::snapshot_with_source_check(
            h.app.doc(),
            generator,
            slate_doc::scene::AgentContextScope::Selection,
            &[unwired],
            &Default::default(),
            |source| {
                checked.push(source);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(checked, [note]);
        assert!(inputs.context.is_empty());
        assert_eq!(inputs.wired[0].text, "existing instruction");
        assert!(h.app.agent_input_snapshot(generator).is_ok());

        h.app.patch_nodes(&[note], |node| {
            if let NodeKind::Text(text) = &mut node.kind {
                text.text.clear();
            }
        });
        assert_eq!(
            h.app.agent_input_snapshot(generator).unwrap_err(),
            "Connected text is empty."
        );
    }
}
