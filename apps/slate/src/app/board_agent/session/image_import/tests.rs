use super::*;
use crate::app::tests::Harness;
use serde_json::json;

fn host(tag: &str) -> (Harness, NodeId, PathBuf) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    let prepared = h.app.prepare_targeted_command(
        "board.objects.create_generator", &[],
        &json!({"x":0,"y":0,"width":480,"height":320,"provider":"openai-image","prompt":"image"}),
    ).unwrap();
    let node = prepared.created_ids()[0];
    h.app
        .finish_targeted_command(prepared, "Dot", true, "owner")
        .unwrap();
    let path = atlas_core::workbook_assets::generated_dir(
        h.app.tab().path.as_deref().unwrap(),
        "openai-image",
        "2026-10",
    )
    .join("fresh-output.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"known metadata fixture").unwrap();
    let mut session = AgentSession::default();
    session.bundle.images.push(atlas_agent::ImageOutput {
        id: "fresh-output".into(),
        source: path.to_string_lossy().into_owned(),
        request: "generation-1".into(),
        ..Default::default()
    });
    h.app
        .agents
        .sessions
        .insert(node, std::sync::Arc::new(session));
    h.app.agents.output_epoch += 1;
    (h, node, path)
}

fn poll(h: &mut Harness, node: NodeId) {
    for _ in 0..200 {
        h.app.agents.image_imports.note_candidate(node);
        h.app.pump_known_agent_image_imports(&h.ctx);
        if h.app.registered_agent_image_item(node).is_some()
            || h.app.agent_image_import_problem(node).is_some()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("known output metadata did not complete");
}

#[test]
fn fresh_generation_registers_without_a_human_pick_then_places_with_one_undo() {
    let (mut h, node, path) = host("fresh-output-place");
    let scene = h.app.doc().scene.clone();
    let selection = h.app.board_sel.clone();
    let depth = h.app.tab().journal.undo_depth();
    atlas_core::fs_probe::reset();
    poll(&mut h, node);
    assert_eq!(
        atlas_core::fs_probe::count(),
        0,
        "metadata must stay off the frame thread"
    );
    assert_eq!(
        h.app.doc().scene,
        scene,
        "registration does not author a pick or alter preview geometry"
    );
    assert_eq!(h.app.board_sel, selection);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let item = h.app.registered_agent_image_item(node).unwrap();
    assert_eq!(h.app.item_path(item), Some(path));
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.place_images",
            &[node.0.to_string()],
            &json!({"x":700,"y":400}),
        )
        .unwrap();
    let placed = prepared.created_ids()[0];
    h.app
        .finish_targeted_command(prepared, "Dot", true, "owner")
        .unwrap();
    assert!(h.app.doc().scene.node(placed).is_some());
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert!(h.app.doc().scene.node(placed).is_none());
    assert!(h.app.doc().scene.node(node).is_some());
}

#[test]
fn deleted_or_rebound_generator_cannot_register_a_late_result() {
    let (mut h, node, _) = host("fresh-output-stale");
    h.app.agents.image_imports.note_candidate(node);
    h.app.pump_known_agent_image_imports(&h.ctx);
    let before = h.app.doc().scene.node(node).unwrap().clone();
    let mut after = before.clone();
    slate_doc::agent_chat::agent_mut(&mut after)
        .unwrap()
        .session = "replacement-session".into();
    h.app.commit_scene(vec![slate_doc::scene::SceneCmd::Patch {
        before: Box::new(before),
        after: Box::new(after),
    }]);
    for _ in 0..100 {
        h.app.pump_known_agent_image_imports(&h.ctx);
        if h.app.agents.image_imports.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.app.doc().items.is_empty());
}

#[test]
fn provider_manifest_cannot_register_a_path_outside_generated_outputs() {
    let (mut h, node, _) = host("fresh-output-scope");
    let private = h.base.join("private.png");
    std::fs::write(&private, b"not a provider result").unwrap();
    let snapshot = std::sync::Arc::make_mut(h.app.agents.sessions.get_mut(&node).unwrap());
    snapshot.bundle.images[0].source = private.to_string_lossy().into_owned();
    h.app.agents.output_epoch += 1;
    poll(&mut h, node);
    assert!(h.app.doc().items.is_empty());
    assert!(h
        .app
        .agent_image_import_problem(node)
        .unwrap()
        .contains("outside"));
}

#[test]
fn metadata_admission_and_candidates_remain_bounded() {
    let mut imports = KnownImageImports::default();
    for id in (1..=10_000).rev() {
        imports.note_candidate(NodeId(id));
    }
    assert_eq!(imports.candidates.len(), PENDING);
    assert_eq!(imports.candidates.first(), Some(&NodeId(1)));
    assert_eq!(imports.candidates.last(), Some(&NodeId(PENDING as u64)));
    imports.cursor = 10_000;
    imports.candidates.clear();
    for id in 1..=10_000 {
        imports.note_candidate(NodeId(id));
    }
    assert!(imports.candidates.is_empty());
}

#[test]
fn read_only_board_never_registers_worker_output() {
    let (mut h, node, _) = host("fresh-output-readonly");
    h.app.agents.image_imports.note_candidate(node);
    h.app.pump_known_agent_image_imports(&h.ctx);
    h.app.tab_mut().read_only = true;
    for _ in 0..100 {
        h.app.pump_known_agent_image_imports(&h.ctx);
        if h.app.agents.image_imports.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.app.doc().items.is_empty());
}

#[test]
fn missing_file_recovers_after_backoff_without_changing_result_identity() {
    let (mut h, node, path) = host("fresh-output-retry");
    let identity = h.app.known_image_import(node).unwrap().identity;
    std::fs::remove_file(&path).unwrap();
    poll(&mut h, node);
    assert!(h.app.agent_image_import_problem(node).is_some());
    assert!(h.app.registered_agent_image_item(node).is_none());
    let deadline = h
        .app
        .agents
        .image_imports
        .completed
        .get(&identity)
        .unwrap()
        .retry_at;
    assert!(h
        .app
        .agents
        .image_imports
        .blocks_retry(&identity, deadline - Duration::from_millis(1)));
    std::fs::write(&path, b"same result is now ready").unwrap();
    assert_eq!(h.app.known_image_import(node).unwrap().identity, identity);
    for _ in 0..200 {
        h.app.agents.image_imports.note_candidate(node);
        h.app.pump_known_agent_image_imports_at(&h.ctx, deadline);
        if h.app.registered_agent_image_item(node).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.app.registered_agent_image_item(node).is_some());
    assert!(h.app.agent_image_import_problem(node).is_none());
    assert!(h
        .app
        .agents
        .image_imports
        .blocks_retry(&identity, deadline + Duration::from_secs(60)));
}

#[test]
fn failed_import_backoff_is_capped_and_never_permanently_blocks_retry() {
    let (h, node, _) = host("fresh-output-backoff");
    let identity = h.app.known_image_import(node).unwrap().identity;
    let mut imports = KnownImageImports::default();
    let now = Instant::now();
    for _ in 0..100 {
        imports.remember(
            identity.clone(),
            Some("temporarily unavailable".into()),
            now,
        );
    }
    let record = imports.completed.get(&identity).unwrap();
    assert_eq!(record.retry_at.duration_since(now), Duration::from_secs(30));
    assert!(!imports.blocks_retry(&identity, now + Duration::from_secs(30)));
    assert_eq!(imports.completed.len(), 1);
}
