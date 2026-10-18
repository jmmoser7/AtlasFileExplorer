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
    if let NodeKind::Image(image) = &mut node.kind {
        image.text_override = Some(edit.buffer.clone());
        node.rect = edit.rect;
    }
}

pub(in crate::app::board) fn apply_inline_edit(
    node: &mut Node,
    text: &str,
    rect: Option<WorldRect>,
) {
    if let Some(rect) = rect {
        node.rect = rect;
    }
    match &mut node.kind {
        NodeKind::Text(t) => t.text = text.to_owned(),
        NodeKind::Shape(s) if slate_doc::scene::shape_hosts_text(s) => {
            let ink = slate_doc::scene::shape_text_ink(s.fill);
            let block = s
                .text
                .get_or_insert_with(|| slate_doc::scene::ShapeText::new(ink));
            block.body = text.to_owned();
            if block.body.is_empty()
                && block.style == slate_doc::scene::TextStyle::default()
                && block.family == slate_doc::scene::Typeface::Sans
                && block.size == 24.0
                && block.align == TextAlign::Center
                && block.color == ink
            {
                s.text = None;
            }
        }
        _ => {}
    }
}

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
