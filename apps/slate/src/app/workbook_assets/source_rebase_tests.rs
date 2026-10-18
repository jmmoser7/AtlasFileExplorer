use super::*;
use crate::app::tests::Harness;
use slate_doc::scene::{
    artifact, ArtifactGeneratorRef, NodeId, NodeKind, PortalNode, SceneCmd, SourceUri, WorldRect,
};

fn fixture(tag: &str) -> (Harness, NodeId, PathBuf, PathBuf) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    let original = h.base.join("original");
    std::fs::create_dir_all(original.join("history/revisions/r1")).unwrap();
    std::fs::create_dir_all(original.join("history/work/session")).unwrap();
    std::fs::create_dir_all(original.join("private-agent")).unwrap();
    std::fs::write(
        original.join("history/revisions/r1/index.html"),
        b"<html>accepted pie</html>",
    )
    .unwrap();
    std::fs::write(
        original.join("private-agent/session.json"),
        b"{\"fixture\":true}",
    )
    .unwrap();
    let old = original.join("board.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, old.clone());
    let mut portal = PortalNode::bound_web("pie", "history/revisions/r1/index.html");
    portal.web.as_mut().unwrap().artifact = Some(Box::new(ArtifactGeneratorRef {
        history_root: Some(SourceUri {
            locator: "history".into(),
        }),
        workspace: Some(SourceUri {
            locator: "history/work/session".into(),
        }),
        revision: Some("r1".into()),
        revisions: vec!["r1".into()],
        ..Default::default()
    }));
    portal.agent = PortalNode::unbound_agent("builder", "codex").agent;
    portal.agent.as_mut().unwrap().bundle = Some(SourceUri {
        locator: "private-agent/session.json".into(),
    });
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 960.0, 540.0),
        NodeKind::Portal(portal),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let mut after = before.clone();
    if let NodeKind::Portal(portal) = &mut after.kind {
        portal.title = "edited pie".into();
    }
    assert!(h.app.commit_scene(vec![SceneCmd::Patch {
        before: Box::new(before),
        after: Box::new(after)
    }]));
    let destination = h.base.join("moved");
    std::fs::create_dir_all(&destination).unwrap();
    (h, id, old, destination.join("copy.slate"))
}

fn targets(doc: &slate_doc::SlateDoc, id: NodeId, workbook: &Path) -> Vec<PathBuf> {
    let node = doc.scene.node(id).unwrap();
    let NodeKind::Portal(portal) = &node.kind else {
        panic!()
    };
    let generation = artifact(node).unwrap();
    [
        portal.source.as_ref(),
        generation.history_root.as_ref(),
        generation.workspace.as_ref(),
        portal.agent.as_ref().unwrap().bundle.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|source| resolve_source(Some(workbook), &source.locator))
    .collect()
}

#[test]
fn save_as_and_reopen_preserve_artifact_targets_without_collecting_history_or_sessions() {
    let (mut h, id, old, destination) = fixture("artifact-save-as");
    h.app.board_undo();
    let expected = targets(h.app.doc(), id, &old);
    let depth = h.app.tab().journal.undo_depth();
    let redo = h.app.tab().edit_redo.len();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination.clone());
    assert_eq!(h.app.tab().path.as_ref(), Some(&destination));
    assert_eq!(targets(h.app.doc(), id, &destination), expected);
    let reopened = slate_doc::SlateDoc::load_from(&destination).unwrap();
    assert_eq!(targets(&reopened, id, &destination), expected);
    assert_eq!(
        artifact(reopened.scene.node(id).unwrap())
            .unwrap()
            .revision
            .as_deref(),
        Some("r1")
    );
    assert_eq!(
        std::fs::read(&expected[0]).unwrap(),
        b"<html>accepted pie</html>"
    );
    assert!(!destination.parent().unwrap().join("history").exists());
    assert!(!destination.parent().unwrap().join("private-agent").exists());
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth,
        "rebasing adds no authored edit"
    );
    assert_eq!(h.app.tab().edit_redo.len(), redo);
    h.app.board_redo();
    assert_eq!(targets(h.app.doc(), id, &destination), expected);
    h.app.board_undo();
    assert_eq!(targets(h.app.doc(), id, &destination), expected);
}

#[test]
fn failed_save_as_preserves_relative_sources_and_redo_snapshots() {
    let (mut h, id, old, destination) = fixture("artifact-save-as-failed");
    h.app.board_undo();
    let before = h.app.doc().clone();
    let expected = targets(&before, id, &old);
    let depth = h.app.tab().journal.undo_depth();
    let redo = h.app.tab().edit_redo.len();
    std::fs::create_dir(&destination).unwrap();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination);
    assert_eq!(h.app.doc(), &before);
    assert_eq!(h.app.tab().path.as_ref(), Some(&old));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.tab().edit_redo.len(), redo);
    h.app.board_redo();
    assert_eq!(targets(h.app.doc(), id, &old), expected);
}
