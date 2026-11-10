//! First binding must retain the tools owned by the original source host.
use super::*;

#[test]
fn first_agent_binding_preserves_text_web_and_model_native_toolbars() {
    for kind in [PendingHost::Text, PendingHost::Artifact, PendingHost::Model] {
        let mut h = prepare(&format!("first-bind-toolbar-{kind:?}"));
        let id = match kind {
            PendingHost::Text => {
                let mut text = slate_doc::scene::TextNode::agent_note(
                    slate_doc::scene::AgentPortalRef::unbound(PortalView::Text),
                );
                text.agent = None;
                text.text = "Original authored words".into();
                let node = h.app.doc_mut().scene.build_node(
                    slate_doc::WorldRect::new(0.0, 0.0, 760.0, 420.0),
                    NodeKind::Text(text),
                );
                h.app.add_nodes(vec![node])[0]
            }
            PendingHost::Artifact => {
                let node = h.app.doc_mut().scene.build_node(
                    slate_doc::WorldRect::new(0.0, 0.0, 760.0, 420.0),
                    NodeKind::Portal(PortalNode::unbound_web("Web portal")),
                );
                h.app.add_nodes(vec![node])[0]
            }
            _ => {
                let id = super::super::super::tests::place(&mut h, "native.obj", 0.0);
                std::fs::write(
                    h.base.join("native.obj"),
                    "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
                )
                .unwrap();
                id
            }
        };
        let original = h.app.doc().scene.node(id).unwrap().clone();
        let before = h.app.property_strip_signature_for_test(&[id]);
        let required: &[&str] = match kind {
            PendingHost::Text => &["Panel(Text)"],
            PendingHost::Artifact => &["Panel(Corners)", "ModelScreenshot"],
            _ => &[
                "Panel(ModelDisplay)",
                "ModelMeasure",
                "ModelScreenshot",
                "ModelMaximize",
                "Panel(Filter)",
            ],
        };
        for tool in required {
            assert!(
                before.contains(tool),
                "the native fixture offers {tool}: {before}"
            );
        }
        let draft = AgentDraft::new();
        assert!(h
            .app
            .bind_pending_generator(id, kind, &draft, "codex", "discovered-native-id"));
        let bound = h.app.doc().scene.node(id).unwrap();
        assert!(slate_doc::agent_chat::agent(bound).is_some());
        assert_eq!(bound.rect, original.rect);
        let after = h.app.property_strip_signature_for_test(&[id]);
        assert_eq!(after, before, "binding retains native tools: {kind:?}");
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &original);
        assert_eq!(h.app.property_strip_signature_for_test(&[id]), before);
        h.app.board_redo();
        assert_eq!(h.app.property_strip_signature_for_test(&[id]), after);
    }
}

#[test]
fn image_binding_keeps_the_image_palette_and_chat_bundle_keeps_its_affordance() {
    let mut h = prepare("first-bind-image-and-chat-tools");
    let image = super::super::super::tests::place(&mut h, "native.png", 0.0);
    let before = h.app.property_strip_signature_for_test(&[image]);
    for tool in ["Panel(Brush)", "Panel(Layers)", "Panel(Filter)"] {
        assert!(before.contains(tool), "image owns {tool}: {before}");
    }
    // Exercise the real first-image binding while Live defers input capture
    // and dispatch; this check owns toolbar retention, not provider execution.
    let mut draft = AgentDraft::new();
    draft.live = true;
    assert!(h
        .app
        .submit_image_in_place(image, &draft, "comfy", "", draft.image_aspect()));
    assert!(slate_doc::agent_chat::agent(h.app.doc().scene.node(image).unwrap()).is_some());
    assert_eq!(h.app.property_strip_signature_for_test(&[image]), before);

    h.app.place_agent_portal_at(Pos2::ZERO);
    let chat = h.app.doc().scene.nodes.last().unwrap().id;
    let chat_before = h.app.property_strip_signature_for_test(&[chat]);
    h.app.set_agent_program(chat, "codex");
    assert_eq!(
        h.app.property_strip_signature_for_test(&[chat]),
        chat_before
    );
    h.app.patch_nodes(&[chat], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.chat.train = true;
        agent.chat.bundled = vec![NodeId(999)];
    });
    assert_eq!(
        h.app.property_strip_signature_for_test(&[chat]),
        "[Panel(Fill), Panel(Stroke), Agent(true), ModelScreenshot]"
    );
    h.app.patch_nodes(&[chat], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .chat
            .bundled
            .clear();
    });
    h.app.place_agent_portal_at(Pos2::new(1000.0, 0.0));
    let child = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.patch_nodes(&[child], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.chat.train = true;
        agent.chat.parent = Some(chat);
    });
    assert_eq!(
        h.app.property_strip_signature_for_test(&[chat, child]),
        "[Panel(Fill), Panel(Stroke), Agent(false)]"
    );
}
