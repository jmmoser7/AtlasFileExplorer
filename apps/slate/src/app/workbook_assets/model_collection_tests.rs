use super::*;
use crate::app::tests::Harness;
use slate_doc::scene::{ImageNode, ModelOutputRef, NodeKind, PortalNode, SourceUri, WorldRect};

#[test]
fn generated_model_catalog_collects_rewrites_undoes_and_moves_without_private_files() {
    let mut h = Harness::new("portable-generated-models");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let data = h.base.join("data");
    let root = data.join("headless-rhino/models");
    std::fs::create_dir_all(&root).unwrap();
    let workbook = h.base.join("first/Board.slate");
    std::fs::create_dir_all(workbook.parent().unwrap()).unwrap();
    h.app.tab_mut().path = Some(workbook.clone());
    let mut outputs = Vec::new();
    let mut sources = Vec::new();
    let mut items = Vec::new();
    for (id, bytes) in [
        ("empty-model", b"empty".as_slice()),
        ("native-a", b"model-a".as_slice()),
        ("native-b", b"model-b".as_slice()),
    ] {
        let path = root.join(format!("{id}.3dm"));
        std::fs::write(&path, bytes).unwrap();
        atlas_core::workbook_assets::write_headless_model_provenance(&path, id).unwrap();
        items.push(h.app.item_for_ready_path(&path, bytes.len() as u64, 0));
        outputs.push(ModelOutputRef {
            id: id.into(),
            source: SourceUri {
                // The locator rewrite journal uses forward slashes, including
                // when undoing a Windows absolute source path.
                locator: slate_doc::scene::source_locator(Some(&workbook), &path)
                    .replace('\\', "/"),
            },
            size: bytes.len() as u64,
            mtime: 0,
        });
        sources.push(path);
    }
    let private = data.join("headless-rhino/jobs/private/input.3dm");
    std::fs::create_dir_all(private.parent().unwrap()).unwrap();
    std::fs::write(&private, b"snapshot").unwrap();
    let mut image = ImageNode::new(items[2]);
    image.agent = PortalNode::unbound_agent("3d", "codex").agent;
    image.agent.as_mut().unwrap().generator.model_outputs = outputs.clone();
    let hidden = slate_doc::scene::HiddenOutputRef::Model {
        ticket: "native-a".into(),
    };
    image
        .agent
        .as_mut()
        .unwrap()
        .generator
        .hidden_outputs
        .push(hidden.clone());
    let node = h
        .app
        .doc()
        .scene
        .node_preparation()
        .build_node(WorldRect::new(0., 0., 640., 480.), NodeKind::Image(image));
    let node = h.app.add_nodes(vec![node])[0];
    let outcome =
        atlas_core::workbook_assets::collect_data_dir_assets(&data, &workbook, &sources, 0);
    assert_eq!(outcome.collected.len(), 3);
    assert_eq!(
        h.app
            .apply_collected_on_tab(h.app.active_tab, Some(&workbook), &outcome),
        1
    );
    let saved = h
        .app
        .doc()
        .scene
        .node(node)
        .and_then(slate_doc::agent_chat::agent)
        .unwrap()
        .generator
        .model_outputs
        .clone();
    assert!(saved.iter().all(|o| o
        .source
        .locator
        .starts_with("assets/generated/headless-rhino/")));
    h.app.board_undo();
    assert_eq!(
        h.app
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .unwrap()
            .generator
            .model_outputs,
        outputs
    );
    h.app.board_redo();
    h.app.doc().save_to(&workbook).unwrap();
    let moved = h.base.join("second/Board.slate");
    let copied = atlas_core::workbook_assets::copy_referenced_assets(
        &workbook,
        &moved,
        &outcome
            .collected
            .iter()
            .map(|a| a.to.clone())
            .collect::<Vec<_>>(),
    );
    assert_eq!(copied.collected.len(), 3);
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    h.app.doc().save_to(&moved).unwrap();
    for path in sources {
        std::fs::remove_file(path).unwrap();
    }
    let reopened = slate_doc::SlateDoc::load_from(&moved).unwrap();
    assert_eq!(
        slate_doc::agent_chat::agent(reopened.scene.node(node).unwrap())
            .unwrap()
            .generator
            .hidden_outputs,
        [hidden]
    );
    let refs = &reopened
        .scene
        .node(node)
        .and_then(slate_doc::agent_chat::agent)
        .unwrap()
        .generator
        .model_outputs;
    for (index, output) in refs.iter().enumerate() {
        assert_eq!(
            std::fs::read(resolve_source(Some(&moved), &output.source.locator)).unwrap(),
            [
                b"empty".as_slice(),
                b"model-a".as_slice(),
                b"model-b".as_slice()
            ][index]
        );
    }
    assert!(private.exists());
    assert!(!moved.parent().unwrap().join("jobs").exists());
    h.app.tab_mut().doc = reopened;
    h.app.tab_mut().path = Some(moved);
    let items = h.app.doc().items.clone();
    let depth = h.app.tab().journal.undo_depth();
    let prepared = h
        .app
        .prepare_targeted_command(
            "portal.agent.model_output",
            &[node.0.to_string()],
            &serde_json::json!({"output":"native-a"}),
        )
        .expect("Collect/save/reopen preserves each catalog output's registered link");
    assert_eq!(
        h.app.doc().items,
        items,
        "preparation must not register a replacement link"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app
        .finish_targeted_command(prepared, "Dot", true, "reopened-model-selection")
        .unwrap();
    let shown = h
        .app
        .doc()
        .scene
        .node(node)
        .and_then(slate_doc::agent_chat::agent)
        .unwrap();
    assert_eq!(shown.generator.shown_model.as_deref(), Some("native-a"));
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&slate_doc::scene::CmdAuthor::Agent("Dot".into()))
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}
