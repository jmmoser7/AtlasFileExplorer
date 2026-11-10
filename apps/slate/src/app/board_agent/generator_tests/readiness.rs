//! Generate's paint-time readiness shares the resolved generator view.
use super::*;
use crate::app::board_agent::runtime::INPUT_RESOLVES_ON_THIS_THREAD;
use crate::app::board_agent::test_support::note;
use atlas_agent::{InputSlot, PortalView};

pub(super) fn wire(
    h: &mut crate::app::tests::Harness,
    source: NodeId,
    target: NodeId,
    slot: InputSlot,
) {
    let t = slate_doc::agent_inputs::input_ports(&h.app.doc().scene, target)
        .iter()
        .find(|port| port.slot == slot)
        .unwrap()
        .t;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: target,
            side: Side::Left,
            t,
        },
    );
    h.app.add_nodes(vec![wire]);
}

pub(super) fn load_image_history(h: &mut crate::app::tests::Harness, id: NodeId) {
    h.app.set_agent_session_for_test(
        id,
        AgentSession {
            provider: "comfy".into(),
            status: AgentStatus::Idle,
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: "Earlier image instruction".into(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "Earlier image response".into(),
                    at: 2,
                },
            ],
            ..Default::default()
        },
    );
}

#[test]
fn generate_readiness_requires_explicit_primary_and_prompt_wires() {
    let (mut h, id) = generator_with_output("readiness-explicit-wires");
    let prompt = note(&mut h, Pos2::new(-400.0, 0.0), "evening light");
    wire(&mut h, prompt, id, InputSlot::Prompt);
    assert!(h
        .app
        .generator_input_refs(id, true)
        .unwrap()
        .on(InputSlot::Media)
        .next()
        .is_some());
    assert!(
        !h.app.image_agent_has_wired_image_and_prompt(id),
        "current-image fallback is not an authored primary wire"
    );
    let style = place(&mut h, "style.png", -600.0);
    wire(&mut h, style, id, InputSlot::Style);
    assert!(
        !h.app.image_agent_has_wired_image_and_prompt(id),
        "style cannot replace Media"
    );
    let primary = place(&mut h, "primary.png", -800.0);
    wire(&mut h, primary, id, InputSlot::Media);
    assert!(h.app.image_agent_has_wired_image_and_prompt(id));
    h.app.text_edit = Some((prompt, "  ".into()));
    assert!(
        !h.app.image_agent_has_wired_image_and_prompt(id),
        "an empty live prompt disables the direct run"
    );
}

#[test]
fn generate_readiness_reuses_the_view_until_scene_output_or_live_edit_changes() {
    let (mut h, id) = generator_with_output("readiness-revision-cache");
    let prompt = note(&mut h, Pos2::new(-400.0, 0.0), "evening light");
    wire(&mut h, prompt, id, InputSlot::Prompt);
    let primary = place(&mut h, "primary.png", -800.0);
    wire(&mut h, primary, id, InputSlot::Media);
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| count.set(0));
    for _ in 0..20 {
        assert!(h.app.image_agent_has_wired_image_and_prompt(id));
        assert!(h.app.generator_view(id).wired_image_and_prompt);
    }
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| {
        assert_eq!(
            count.get(),
            1,
            "paint and layout share one input resolution"
        )
    });
    h.app.text_edit = Some((prompt, String::new()));
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 2));
    h.app.agents.output_epoch += 1;
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 3));
    h.app.text_edit = None;
    h.app.patch_nodes(&[prompt], |node| {
        if let NodeKind::Text(text) = &mut node.kind {
            text.text.clear();
        }
    });
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 4));
}

#[test]
fn wired_composer_and_image_request_keep_live_text_separate_from_authored_instruction() {
    for preserve in [false, true] {
        let (mut h, id) = generator_with_output("wired-composer-image");
        h.app.patch_nodes(&[id], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.instruction = "Keep the framing".into();
            agent.generator.preserve_history = Some(preserve);
        });
        load_image_history(&mut h, id);
        h.app.sync_instruction(id, "Keep the framing");
        let source = note(&mut h, Pos2::new(-400.0, 0.0), "Evening light");
        wire(&mut h, source, id, InputSlot::Prompt);
        let body = h.app.doc().scene.node(source).unwrap().clone();
        assert_eq!(
            h.app.agent_prompt_text(id),
            "Evening light, Keep the framing"
        );
        let request = h.app.generation_request(id, false).unwrap();
        assert_eq!(request.prompt, h.app.agent_prompt_text(id));
        h.app
            .edit_agent_prompt_text(id, "Evening light, Keep the framing and colors".into());
        assert_eq!(h.app.agents.prompt(id), Some("Keep the framing and colors"));
        // An edit wholly inside linked words leaves the authored suffix intact.
        h.app
            .edit_agent_prompt_text(id, "Evening glow, Keep the framing and colors".into());
        assert_eq!(h.app.agents.prompt(id), Some("Keep the framing and colors"));
        // Select-all replacement authors the new instruction, never a source copy.
        h.app.edit_agent_prompt_text(id, "Use a wider lens".into());
        assert_eq!(
            h.app.agent_prompt_text(id),
            "Evening light, Use a wider lens"
        );
        assert_eq!(h.app.doc().scene.node(source), Some(&body));
        h.app.patch_nodes(&[source], |node| {
            if let NodeKind::Text(text) = &mut node.kind {
                text.text = "Morning light".into();
            }
        });
        assert_eq!(
            h.app.agent_prompt_text(id),
            "Morning light, Use a wider lens"
        );
        assert_eq!(
            h.app.generation_request(id, false).unwrap().prompt,
            "Morning light, Use a wider lens"
        );
        // Moving the wire's endpoint away disconnects the derived instruction.
        let connector = h.app.doc().scene.nodes.iter().find(|node| {
            matches!(&node.kind, NodeKind::Connector(c) if matches!(&c.a, ConnectorEnd::Anchored { node, .. } if *node == source))
        }).unwrap().id;
        h.app.patch_nodes(&[connector], |node| {
            if let NodeKind::Connector(c) = &mut node.kind {
                c.b = ConnectorEnd::Free { point: [0.0, 0.0] };
                c.binding = None;
            }
        });
        assert_eq!(h.app.agent_prompt_text(id), "Use a wider lens");
        assert_eq!(h.app.agents.prompt(id), Some("Use a wider lens"));
    }
}

#[test]
fn text_dialogue_dispatch_keeps_wires_with_both_history_modes_and_node_isolation() {
    for preserve in [false, true] {
        let (mut h, id) = generator_with_output("wired-composer-text");
        h.app.patch_nodes(&[id], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.view = PortalView::Text;
            agent.instruction = "Summarize concisely".into();
            agent.generator.preserve_history = Some(preserve);
        });
        h.app.sync_instruction(id, "Summarize concisely");
        let source = note(&mut h, Pos2::new(-400.0, 0.0), "A café by the sea");
        wire(&mut h, source, id, InputSlot::Prompt);
        let unrelated = note(&mut h, Pos2::new(-400.0, 200.0), "Unrelated text");
        let inputs = h.app.agent_input_snapshot(id).unwrap();
        assert_eq!(
            h.app.text_block_prompt(id, &inputs),
            "A café by the sea\n\nSummarize concisely"
        );
        assert_eq!(
            h.app.agent_prompt_text(id),
            h.app.text_block_prompt(id, &inputs)
        );
        assert_eq!(h.app.agent_prompt_text(unrelated), "");
        h.app
            .edit_agent_prompt_text(id, "A café by the sea\n\nSummarize in French".into());
        assert_eq!(h.app.agents.prompt(id), Some("Summarize in French"));
        assert_eq!(
            h.app.doc().scene.node(source).and_then(|n| match &n.kind {
                NodeKind::Text(t) => Some(t.text.as_str()),
                _ => None,
            }),
            Some("A café by the sea")
        );
    }
}

#[test]
fn typing_after_a_wired_only_prompt_authors_the_suffix_and_keeps_source_live() {
    let (mut h, id) = generator_with_output("wired-only-typing");
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .instruction
            .clear();
    });
    h.app.sync_instruction(id, "");
    let source = note(&mut h, Pos2::new(-400.0, 0.0), "foo");
    wire(&mut h, source, id, InputSlot::Prompt);
    assert_eq!(h.app.agent_prompt_text(id), "foo");
    h.app.edit_agent_prompt_text(id, "foox".into());
    assert_eq!(h.app.agents.prompt(id), Some("x"));
    assert_eq!(h.app.agent_prompt_text(id), "foo, x");
    *h.app.agents.prompt_mut(id) = String::new();
    h.app.edit_agent_prompt_text(id, "foo\nMake it blue".into());
    assert_eq!(h.app.agents.prompt(id), Some("\nMake it blue"));
    assert_eq!(h.app.agent_prompt_text(id), "foo, \nMake it blue");
    // Replacement sharing an initial character with the wire still keeps it.
    h.app.edit_agent_prompt_text(id, "fresh instruction".into());
    assert_eq!(h.app.agents.prompt(id), Some("fresh instruction"));
    assert_eq!(h.app.agent_prompt_text(id), "foo, fresh instruction");
}

#[test]
fn composer_projection_preserves_spaces_between_typed_words() {
    let (mut h, id) = generator_with_output("composer-typing-spaces");
    *h.app.agents.prompt_mut(id) = "hello ".into();
    assert_eq!(h.app.agent_prompt_text(id), "hello ");
    h.app.edit_agent_prompt_text(id, "hello world ".into());
    assert_eq!(h.app.agent_prompt_text(id), "hello world ");
    let source = note(&mut h, Pos2::new(-400.0, 0.0), "foo");
    wire(&mut h, source, id, InputSlot::Prompt);
    assert_eq!(h.app.agent_prompt_text(id), "foo, hello world ");
    h.app
        .edit_agent_prompt_text(id, "foo, hello world again ".into());
    assert_eq!(h.app.agents.prompt(id), Some("hello world again "));
}

#[test]
fn first_wired_suffix_insertion_keeps_the_editor_cursor_after_the_typed_text() {
    let (mut h, id) = generator_with_output("wired-composer-cursor");
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .instruction
            .clear();
    });
    h.app.sync_instruction(id, "");
    let source = note(&mut h, Pos2::new(-400.0, 0.0), "foo");
    wire(&mut h, source, id, InputSlot::Prompt);
    let ctx = egui::Context::default();
    let field = Id::new("wired-typing-cursor");
    ctx.memory_mut(|memory| memory.request_focus(field));
    let mut state = egui::text_edit::TextEditState::default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(4),
        )));
    state.store(&ctx, field);
    h.app.edit_agent_prompt_text(id, "foox".into());
    h.app.sync_projected_prompt_cursor(&ctx, id, "foox", "x");
    let state = egui::text_edit::TextEditState::load(&ctx, field).unwrap();
    assert_eq!(state.cursor.char_range().unwrap().primary.index, 6);
    h.app.edit_agent_prompt_text(id, "foo, xy".into());
    assert_eq!(h.app.agent_prompt_text(id), "foo, xy");
}

#[test]
fn non_image_composer_projection_reuses_input_cache_and_invalidates_live_text_and_wires() {
    let (mut h, id) = generator_with_output("cached-text-composer");
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.view = PortalView::Text;
        agent.instruction.clear();
    });
    h.app.sync_instruction(id, "");
    let source = note(&mut h, Pos2::new(-400.0, 0.0), "Original source");
    wire(&mut h, source, id, InputSlot::Prompt);
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| count.set(0));
    for _ in 0..20 {
        assert_eq!(h.app.agent_prompt_text(id), "Original source");
    }
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 1));
    h.app.text_edit = Some((source, "Live source edit".into()));
    assert_eq!(h.app.agent_prompt_text(id), "Live source edit");
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 2));
    h.app.agents.output_epoch += 1;
    assert_eq!(h.app.agent_prompt_text(id), "Live source edit");
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 3));
    h.app.text_edit = None;
    let connector = h.app.doc().scene.nodes.iter().find(|node| {
        matches!(&node.kind, NodeKind::Connector(c) if matches!(&c.a, ConnectorEnd::Anchored { node, .. } if *node == source))
    }).unwrap().id;
    h.app.patch_nodes(&[connector], |node| {
        if let NodeKind::Connector(c) = &mut node.kind {
            c.b = ConnectorEnd::Free { point: [0.0, 0.0] };
            c.binding = None;
        }
    });
    assert_eq!(h.app.agent_prompt_text(id), "");
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 4));
}

#[test]
fn linked_document_prompt_cache_refreshes_after_preview_completion_and_live_edit() {
    let (mut h, id) = generator_with_output("cached-linked-document-prompt");
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.view = PortalView::Text;
        agent.instruction.clear();
    });
    h.app.sync_instruction(id, "");
    let path = h.base.join("reference.txt");
    let item = h
        .app
        .doc_mut()
        .add_item(path.clone(), "reference.txt", 100, 0, "txt");
    let rect = slate_doc::WorldRect::new(-400.0, 0.0, 200.0, 160.0);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let source = node.id;
    h.app.add_nodes(vec![node]);
    wire(&mut h, source, id, InputSlot::Prompt);
    assert_eq!(h.app.agent_prompt_text(id), "");
    assert!(h.app.generator_view(id).error.is_some());
    h.app
        .complete_document_preview_for_test(item, "Completed source");
    assert_eq!(h.app.agent_prompt_text(id), "Completed source");
    assert!(h.app.generator_view(id).error.is_none());
    h.app.text_doc_edit = Some(crate::app::board::TextDocEdit {
        node: source,
        item,
        path,
        rect,
        buffer: "Live document words".into(),
        original: "Completed source".into(),
        claim_focus: false,
        auto_size: false,
    });
    assert_eq!(h.app.agent_prompt_text(id), "Live document words");
    h.app.text_doc_edit.as_mut().unwrap().buffer = "Updated document words".into();
    assert_eq!(h.app.agent_prompt_text(id), "Updated document words");
    h.app.text_doc_edit = None;
    assert_eq!(h.app.agent_prompt_text(id), "Completed source");
    h.app.cache_snippet(item, Some("Replaced excerpt".into()));
    assert_eq!(h.app.agent_prompt_text(id), "Replaced excerpt");
}
