use crate::app::tests::Harness;
use slate_doc::scene::{NodeKind, PortalNode, WorldRect};
use slate_doc::{SceneCmd, SlateDoc};
mod read_only;

#[test]
fn recovery_opens_a_new_dirty_copy_and_pauses_live_generation() {
    let mut harness = Harness::new("recovery_install");
    harness.seed();
    let original_tab = harness.app.tab().id;
    let original_doc = harness.app.doc().clone();
    let mut source = SlateDoc::new("Never saved");
    let mut portal = PortalNode::unbound_agent("Generator", "example");
    portal.agent.as_mut().unwrap().live = true;
    let node = source.scene.build_node(
        WorldRect::new(10.0, 20.0, 300.0, 200.0),
        NodeKind::Portal(portal),
    );
    assert!(source.scene.apply(&SceneCmd::Add { index: 0, node }));
    let snapshot = source.clone();

    harness.app.install_recovered_doc(source);

    let tab = harness.app.tab();
    assert_ne!(tab.id, original_tab);
    assert!(tab.path.is_none() && tab.lease.is_none() && tab.dirty && tab.recovered);
    let restored = slate_doc::agent_chat::agent(&tab.doc.scene.nodes[0]).unwrap();
    assert!(!restored.live);
    assert!(!tab.journal.can_undo());
    assert!(
        slate_doc::agent_chat::agent(&snapshot.scene.nodes[0])
            .unwrap()
            .live
    );
    let previous = harness
        .app
        .tabs
        .iter()
        .find(|tab| tab.id == original_tab)
        .unwrap();
    assert_eq!(previous.doc, original_doc);
    assert_eq!(tab.doc.scene.nodes[0].rect, snapshot.scene.nodes[0].rect);
}

#[test]
fn selecting_rectangle_alone_leaves_authored_board_and_history_unchanged() {
    let mut harness = Harness::new("rectangle_arm_safety");
    let before = harness.app.doc().clone();
    let depth = harness.app.tab().journal.undo_depth();
    for _ in 0..100 {
        harness
            .app
            .set_board_tool(crate::app::board::BoardTool::RectShape);
        harness
            .app
            .set_board_tool(crate::app::board::BoardTool::Select);
    }
    assert_eq!(*harness.app.doc(), before);
    assert_eq!(harness.app.tab().journal.undo_depth(), depth);
}

#[test]
fn opening_a_workbook_preserves_a_never_saved_text_draft_in_its_own_tab() {
    let mut harness = Harness::new("recovery_open_draft");
    harness.app.leave_home();
    harness.app.ensure_work_tab();
    let first_tab = harness.app.tab().id;
    harness.app.place_text_at(eframe::egui::pos2(10.0, 20.0));
    harness.app.text_box_draft.as_mut().unwrap().buffer = "Keep my draft".into();
    assert!(harness.app.tab().is_blank());
    let other = harness.base.join("other.slate");
    SlateDoc::new("Other workbook").save_to(&other).unwrap();

    harness.app.open_doc_at(other);

    assert_ne!(harness.app.tab().id, first_tab);
    let first = harness
        .app
        .tabs
        .iter()
        .find(|tab| tab.id == first_tab)
        .unwrap();
    assert!(first.dirty && first.path.is_none());
    let NodeKind::Text(text) = &first.doc.scene.nodes[0].kind else {
        panic!()
    };
    assert_eq!(text.text, "Keep my draft");
    assert!(harness.app.doc().scene.nodes.is_empty());
    assert!(harness.app.text_box_draft.is_none());
}
