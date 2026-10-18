use super::*;
use crate::app::tests::Harness;

#[test]
fn artifact_web_frames_keep_their_authored_size_when_agent_text_arrives() {
    let mut h = Harness::new("artifact-web-fit");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let mut portal = PortalNode::unbound_web("dashboard");
    portal.web.as_mut().unwrap().artifact = Some(Default::default());
    portal.agent = PortalNode::unbound_agent("builder", "codex").agent;
    let rect = slate_doc::WorldRect::new(30.0, 40.0, 1280.0, 720.0);
    let node = h
        .app
        .doc_mut()
        .scene
        .build_node(rect, NodeKind::Portal(portal));
    let id = h.app.add_nodes(vec![node])[0];
    *h.app.agents.prompt_mut(id) = "Keep this dashboard's composition and viewport.".into();
    h.app.fit_agent_cards(&h.ctx);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect);
    let mut resized = h.app.doc().scene.node(id).unwrap().clone();
    resized.rect.w = 900.0;
    assert!(!record_agent_resize(&mut resized));
    assert!(slate_doc::agent_chat::agent(&resized)
        .unwrap()
        .chat
        .size
        .is_none());
}
