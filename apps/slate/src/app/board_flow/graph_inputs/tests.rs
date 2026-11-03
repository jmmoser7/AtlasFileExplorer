use super::*;
use crate::app::tests::Harness;

#[test]
fn plain_sources_have_reference_sizes_and_no_program_or_implicit_wire() {
    let h = Harness::new("plain_graph_sources");
    for kind in ["image", "text", "style"] {
        let commands = h
            .app
            .prepare_graph_input_source(kind, [31.0, 47.0])
            .unwrap();
        assert_eq!(commands.len(), 1);
        let SceneCmd::Add { node, .. } = &commands[0] else {
            panic!("one authored source")
        };
        let expected = if kind == "text" {
            [260.0, 130.0]
        } else {
            [220.0, 160.0]
        };
        assert_eq!([node.rect.x, node.rect.y], [31.0, 47.0]);
        assert_eq!([node.rect.w, node.rect.h], expected);
        let metadata = slate_doc::agent_chat::agent(node).unwrap();
        assert!(metadata.provider.is_empty() && metadata.session.is_empty());
        assert!(metadata.generator.input_source_kind.is_some());
        assert!(slate_doc::agent_inputs::input_ports_of(node, false).is_empty());
        match &node.kind {
            NodeKind::Image(image) => assert_eq!(image.item, slate_doc::ItemId::NONE),
            NodeKind::Text(text) => assert!(text.text.is_empty()),
            _ => panic!("a plain input, never a program portal"),
        }
    }
    assert!(h
        .app
        .prepare_graph_input_source("video", [0.0, 0.0])
        .is_err());
    assert!(h
        .app
        .prepare_graph_input_source("text", [f32::NAN, 0.0])
        .is_err());
}

#[test]
fn unbound_graft_is_journaled_persistent_and_carries_no_execution_identity() {
    let mut h = Harness::new("unbound_graph_graft");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 220.0, 160.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(slate_doc::ItemId::NONE)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let patch = h.app.prepare_input_graft(id, "style", true).unwrap();
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert!(h.app.commit_scene(vec![patch]));
    let after = h.app.doc().scene.node(id).unwrap();
    assert_eq!(after.rect, before.rect);
    let agent = slate_doc::agent_chat::agent(after).unwrap();
    assert!(agent.provider.is_empty() && agent.session.is_empty());
    assert!(agent.channel.is_none() && agent.bundle.is_none());
    assert!(agent.generator.grafted_slots.contains("style"));
    let saved = serde_json::to_vec(after).unwrap();
    let restored: slate_doc::scene::Node = serde_json::from_slice(&saved).unwrap();
    assert_eq!(after, &restored);
    assert!(h.app.pending_generator_host(id).is_some());
    assert!(h.app.prepare_input_graft(id, "view", true).is_err());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}
