//! Text draft materialization shared by normal commits and recovery clones.
use super::*;

impl TextBoxDraft {
    pub(in crate::app::board) fn build_node(&self, scene: &mut slate_doc::Scene) -> Option<Node> {
        if self.buffer.is_empty() {
            return None;
        }
        Some(scene.build_node(
            self.rect,
            NodeKind::Text(TextNode {
                text: self.buffer.clone(),
                style: Default::default(),
                family: self.family,
                size: self.size,
                color: self.color,
                align: self.align,
                fill: None,
                stroke: Default::default(),
                agent: None,
            }),
        ))
    }
}

pub(in crate::app::board) fn apply_document_edit(node: &mut Node, edit: &TextDocEdit) {
    slate_doc::authored_text::apply_document_text(node, &edit.buffer, edit.rect);
}

pub(in crate::app::board) use slate_doc::authored_text::apply_inline_edit;

pub(super) fn inline_edit_differs(node: &Node, text: &str, rect: Option<WorldRect>) -> bool {
    let words = match &node.kind {
        NodeKind::Text(text) => text.text.as_str(),
        NodeKind::Shape(shape) if slate_doc::scene::shape_hosts_text(shape) => {
            shape.text.as_ref().map_or("", |text| text.body.as_str())
        }
        _ => return false,
    };
    words != text || rect.is_some_and(|rect| node.rect != rect)
}

pub(super) fn document_edit_differs(node: &Node, edit: &TextDocEdit) -> bool {
    edit.buffer != edit.original
        && matches!(&node.kind, NodeKind::Image(image)
            if image.text_override.as_deref() != Some(edit.buffer.as_str()) || node.rect != edit.rect)
}
