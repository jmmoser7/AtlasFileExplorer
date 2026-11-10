//! Frozen declared-source fence for accepted tasks serviced outside the visible tab.
use super::*;

#[derive(PartialEq)]
pub(super) struct Witness(serde_json::Value);
impl Witness {
    pub(super) fn capture(
        app: &SlateApp,
        doc: u64,
        target: NodeId,
        inputs: &InputSnapshot,
    ) -> Result<Self, String> {
        let tab = app
            .tabs
            .iter()
            .find(|tab| tab.id == doc)
            .ok_or("Model task workbook was closed")?;
        let runtime = app.agent_runtime_in(doc);
        let sources = inputs.wired.iter().map(|input| {
            let id = NodeId(input.node);
            let node = tab.doc.scene.node(id);
            let item = node.and_then(|node| match &node.kind {
                slate_doc::scene::NodeKind::Image(image) => tab.doc.item(image.item),
                _ => None,
            });
            let session = runtime.and_then(|runtime| runtime.session(id));
            serde_json::json!({"node":node,"item":item,
                "authored":runtime.and_then(|runtime| runtime.prompt(id)),
                "output":session.map(|session| (&session.turns, &session.artifacts, &session.bundle))})
        }).collect::<Vec<_>>();
        let wires = tab
            .doc
            .scene
            .nodes
            .iter()
            .filter(|node| match &node.kind {
                slate_doc::scene::NodeKind::Connector(connector) => {
                    connector.binding.as_ref().is_some_and(|binding| {
                        slate_doc::agent_inputs::endpoint_node(binding.target_end(connector))
                            == Some(target)
                    })
                }
                _ => false,
            })
            .collect::<Vec<_>>();
        Ok(Self(
            serde_json::json!({"workbook":tab.path,"sources":sources,"wires":wires}),
        ))
    }
}
