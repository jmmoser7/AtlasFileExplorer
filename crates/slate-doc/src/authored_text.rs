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
        NodeKind::Text(t) => {
            if t.text != text {
                if let Some(agent) = t.agent.as_mut() {
                    agent.generator.shown_text = None;
                    agent.generator.shown_review = None;
                }
            }
            crate::text_answers::rebase(&mut t.answers, &t.text, text);
            t.text = text.to_owned();
        }
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
    let previous = match &node.kind {
        NodeKind::Image(image) => image.text_override.clone().unwrap_or_default(),
        _ => String::new(),
    };
    apply_document_text_from(node, &previous, text, rect);
}

/// The existing document editor supplies its cached original; no linked-file I/O.
pub fn apply_document_text_from(node: &mut Node, previous: &str, text: &str, rect: WorldRect) {
    if let NodeKind::Image(image) = &mut node.kind {
        if previous != text {
            if let Some(agent) = image.agent.as_mut() {
                agent.generator.shown_text = None;
                agent.generator.shown_review = None;
            }
        }
        crate::text_answers::rebase(&mut image.answers, previous, text);
        for answer in &mut image.answers {
            answer.source_revision = None;
        }
        image.text_override = Some(text.to_owned());
        node.rect = rect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImageNode, Scene, ShapeKind, ShapeNode, ShapeText, WorldRect};

    #[test]
    fn text_edit_rebases_answers_in_the_same_node_mutation() {
        let mut node: Node = serde_json::from_str(
            r#"{"id":1,"rect":{"x":0.0,"y":0.0,"w":200.0,"h":80.0},"kind":{"text":{"text":"Shade 🌳 trees.","size":14.0,"color":[0,0,0,255]}}}"#,
        )
        .unwrap();
        let NodeKind::Text(text) = &mut node.kind else {
            unreachable!()
        };
        assert!(text.answers.is_empty());
        text.answers.push(
            crate::text_answers::TextAnswer::new(
                &text.text,
                6..10,
                crate::text_answers::TextAnswerRef {
                    session: "answer".into(),
                    through: 2,
                },
                "Which tree?".into(),
                "The tree in the brief.".into(),
            )
            .unwrap(),
        );
        apply_inline_edit(&mut node, "More shade. Shade 🌳 trees.", None);
        let NodeKind::Text(text) = &node.kind else {
            unreachable!()
        };
        assert_eq!(text.answers[0].quote, "🌳");
        assert!(text.answers[0].is_valid_for(&text.text));
        apply_inline_edit(&mut node, "More shade. Shade flowers trees.", None);
        let NodeKind::Text(text) = &node.kind else {
            unreachable!()
        };
        assert!(text.answers.is_empty());
    }

    #[test]
    fn actual_authored_edit_clears_display_checkpoint_but_noop_preserves_it() {
        let mut node:Node=serde_json::from_value(serde_json::json!({
            "id":1,"rect":{"x":0.0,"y":0.0,"w":200.0,"h":80.0},
            "kind":{"text":{"text":"Authored words","size":14.0,"color":[0,0,0,255],
                "agent":{"provider":"local","session":"current","generator":{"shown_text":{"source":{"locator":"history/session.json"},"response":{"session":"archive","through":2}}}}}}
        })).unwrap();
        let before = node.clone();
        apply_inline_edit(&mut node, "Authored words", None);
        assert_eq!(node, before);
        apply_inline_edit(&mut node, "Edited words", None);
        let agent = crate::agent_chat::agent(&node).unwrap();
        assert!(agent.generator.shown_text.is_none());
        assert_eq!(agent.session, "current");
        let restored: Node = serde_json::from_value(serde_json::to_value(before).unwrap()).unwrap();
        assert!(crate::agent_chat::agent(&restored)
            .unwrap()
            .generator
            .shown_text
            .is_some());
    }

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
    #[test]
    fn first_linked_override_rebases_existing_answers_from_cached_original() {
        let mut scene = Scene::default();
        let mut image = ImageNode::new(crate::ItemId(1));
        let mut answer = crate::text_answers::TextAnswer::new(
            "A café & tea.",
            2..7,
            crate::text_answers::TextAnswerRef {
                session: "old".into(),
                through: 2,
            },
            "why".into(),
            "coffee".into(),
        )
        .unwrap();
        answer.source_revision = Some("file-v1".into());
        image.answers.push(answer);
        let mut node = scene.build_node(
            WorldRect::new(0.0, 0.0, 200.0, 80.0),
            NodeKind::Image(image),
        );
        apply_document_text_from(
            &mut node,
            "A café & tea.",
            "More. A café & tea.",
            WorldRect::new(0.0, 0.0, 200.0, 80.0),
        );
        let NodeKind::Image(image) = node.kind else {
            panic!()
        };
        assert_eq!(image.text_override.as_deref(), Some("More. A café & tea."));
        assert_eq!(image.answers.len(), 1);
        assert_eq!(image.answers[0].quote, "café");
        assert!(image.answers[0].source_revision.is_none());
        assert!(image.answers[0].is_valid_for(image.text_override.as_deref().unwrap()));
    }
}
