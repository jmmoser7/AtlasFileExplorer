//! Authored board words and their pure mutations; linked sources are never read.
use crate::scene::{Node, NodeKind, TextAlign, WorldRect};

/// Borrow only authored words. An explicit empty override is not a missing override.
pub fn text(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Text(text) => Some(&text.text),
        NodeKind::Shape(shape) if crate::scene::shape_hosts_text(shape) => {
            shape.text.as_ref().map(|text| text.body.as_str())
        }
        NodeKind::Image(image) => image.text_override.as_deref(),
        _ => None,
    }
}

pub fn apply_inline_edit(node: &mut Node, text: &str, rect: Option<WorldRect>) {
    if let Some(rect) = rect {
        node.rect = rect;
    }
    match &mut node.kind {
        NodeKind::Text(t) => t.text = text.to_owned(),
        NodeKind::Shape(s) if crate::scene::shape_hosts_text(s) => {
            let ink = crate::scene::shape_text_ink(s.fill);
            let block = s
                .text
                .get_or_insert_with(|| crate::scene::ShapeText::new(ink));
            block.body = text.to_owned();
            if block.body.is_empty()
                && block.style == crate::scene::TextStyle::default()
                && block.family == crate::scene::Typeface::Sans
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

pub fn apply_document_text(node: &mut Node, text: &str, rect: WorldRect) {
    if let NodeKind::Image(image) = &mut node.kind {
        image.text_override = Some(text.to_owned());
        node.rect = rect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImageNode, Scene, ShapeKind, ShapeNode, ShapeText, WorldRect};

    #[test]
    fn authored_accessor_borrows_and_distinguishes_empty_override_from_absence() {
        let mut scene = Scene::default();
        let mut node = scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 50.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: None,
                stroke: crate::scene::Stroke::none(),
                corner: crate::scene::Corner::Square,
                sides: crate::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            }),
        );
        assert_eq!(text(&node), None);
        if let NodeKind::Shape(shape) = &mut node.kind {
            shape.text = Some(ShapeText::new(crate::scene::Rgba::BLACK));
        }
        assert_eq!(text(&node), Some(""));
        apply_inline_edit(&mut node, "Unicode 🦊", None);
        let NodeKind::Shape(shape) = &node.kind else {
            unreachable!()
        };
        assert_eq!(
            text(&node).unwrap().as_ptr(),
            shape.text.as_ref().unwrap().body.as_ptr()
        );
        let mut image = ImageNode::new(crate::ItemId(1));
        node.kind = NodeKind::Image(image.clone());
        assert_eq!(text(&node), None);
        image.text_override = Some(String::new());
        node.kind = NodeKind::Image(image);
        assert_eq!(text(&node), Some(""));
    }
}
