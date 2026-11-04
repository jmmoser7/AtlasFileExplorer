// Agent-produced media exports preserve the displayed result.

#[test]
fn image_portal_export_contains_bundle_pixels_without_live_controls() {
    let mut doc = SlateDoc::new("Bundle");
    let node = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 400.0, 300.0),
        NodeKind::Portal(slate_doc::PortalNode::unbound_agent("Images", "image-link")),
    );
    let id = node.id;
    doc.scene.apply(&SceneCmd::Add { index: 0, node });
    let mut assets = AssetMap::default();
    assets.insert_agent_images(
        id,
        vec!["assets/first.png".into(), "assets/second.png".into()],
    );
    let html = render_html(&doc, &assets);
    assert!(html.contains("agent-image-bundle"));
    assert!(html.contains("assets/first.png"));
    assert!(html.contains("assets/second.png"));
    assert!(!html.contains(">Generate<"));
    assert!(html.contains("data-provider=\"image-link\""));
    assert!(!html.contains("live agent state is not exported"));
}

/// Media an agent makes exports as the media: the picture's shown result
/// as a plain img, and a note's reply as its words (Art. IV).
#[test]
fn agent_media_exports_as_the_media_it_shows() {
    let mut doc = SlateDoc::new("Made");
    let agent = slate_doc::PortalNode::unbound_agent("Images", "comfy")
        .agent
        .map(|a| *a)
        .unwrap();
    let picture = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 400.0, 300.0),
        NodeKind::Image(slate_doc::scene::ImageNode::generated(agent.clone())),
    );
    let note = doc.scene.build_node(
        WorldRect::new(500.0, 0.0, 300.0, 200.0),
        NodeKind::Text(slate_doc::scene::TextNode::agent_note(agent)),
    );
    let (picture_id, note_id) = (picture.id, note.id);
    doc.scene.apply(&SceneCmd::Add {
        index: 0,
        node: picture,
    });
    doc.scene.apply(&SceneCmd::Add {
        index: 1,
        node: note,
    });
    let mut assets = AssetMap::default();
    assets.insert_agent_images(
        picture_id,
        vec!["assets/newest.png".into(), "assets/older.png".into()],
    );
    if let Some(node) = doc.scene.node_mut(note_id) {
        if let NodeKind::Text(text) = &mut node.kind {
            text.text = "Authored source stays intact".into();
        }
    }
    assets.insert_agent_reply(note_id, "A calm harbour at dusk.".into());
    let html = render_html(&doc, &assets);
    assert!(html.contains("<img src=\"assets/newest.png\""));
    assert!(
        !html.contains("assets/older.png"),
        "one picture, not a bundle"
    );
    assert!(!html.contains("agent-image-bundle"));
    assert!(html.contains("A calm harbour at dusk."));
    assert!(!html.contains("Authored source stays intact"));
}
