use super::*;
use crate::app::tests::Harness;
use serde_json::json;
use slate_doc::scene::{CmdAuthor, ImageNode};

fn host(name: &str) -> Harness {
    let mut h = Harness::new(name);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h
}

fn args(provider: &str) -> Value {
    json!({"x":30,"y":40,"width":480,"height":320,"provider":provider,"prompt":"A calm board"})
}

#[test]
fn train_preparation_is_pure_and_commit_is_attributed_and_reversible() {
    let mut h = host("scoped-train-pure");
    let scene = serde_json::to_value(&h.app.doc().scene).unwrap();
    let selection = h.app.board_sel.clone();
    let depth = h.app.tab().journal.undo_depth();
    let files = std::fs::read_dir(&h.base).unwrap().count();
    let prepared = h
        .app
        .prepare_targeted_command("board.objects.create_train", &[], &args("codex"))
        .unwrap();
    let ids = prepared.created_ids();
    assert_eq!(ids.len(), 1);
    assert_eq!(serde_json::to_value(&h.app.doc().scene).unwrap(), scene);
    assert_eq!(h.app.board_sel, selection);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.doc().items.is_empty());
    assert_eq!(std::fs::read_dir(&h.base).unwrap().count(), files);
    h.app
        .finish_targeted_command(prepared, "Dot", true, "fresh")
        .unwrap();
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(ids[0]).unwrap()).unwrap();
    assert!(agent.chat.train);
    assert_eq!(agent.provider, "codex");
    assert!(agent.channel.is_none());
    assert!(!agent.live);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn two_sequential_trains_get_distinct_nodes_and_sessions() {
    let mut h = host("scoped-trains-distinct");
    let first = h
        .app
        .prepare_targeted_command("board.objects.create_train", &[], &args("codex"))
        .unwrap();
    let a = first.created_ids()[0];
    h.app
        .finish_targeted_command(first, "Dot", true, "owner")
        .unwrap();
    let second = h
        .app
        .prepare_targeted_command("board.objects.create_train", &[], &args("cursor"))
        .unwrap();
    let b = second.created_ids()[0];
    h.app
        .finish_targeted_command(second, "Dot", true, "owner")
        .unwrap();
    assert_ne!(a, b);
    let agent = |id| slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_ne!(agent(a).session, agent(b).session);
    assert!(agent(a).channel.is_none() && agent(b).channel.is_none());
}

#[test]
fn generator_is_existing_media_kind_without_an_automatic_run() {
    let h = host("scoped-generator-pure");
    let prepared = h
        .app
        .prepare_targeted_command("board.objects.create_generator", &[], &args("openai-image"))
        .unwrap();
    assert_eq!(prepared.cmds.len(), 1);
    let SceneCmd::Add { node, .. } = &prepared.cmds[0] else {
        panic!("expected addition")
    };
    let NodeKind::Image(image) = &node.kind else {
        panic!("generator must share image media owner")
    };
    assert!(image.item.is_none());
    let agent = image.agent.as_ref().unwrap();
    assert_eq!(agent.provider, "openai-image");
    assert_eq!(agent.instruction, "A calm board");
    assert!(!agent.live);
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn image_placement_uses_granted_node_and_checks_item_rebinding_before_commit() {
    let mut h = host("scoped-image-item");
    let item = h
        .app
        .doc_mut()
        .add_item(h.base.join("existing.png"), "existing.png", 1, 2, "png");
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 300.0, 150.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let source = node.id;
    h.app.add_nodes(vec![node]);
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.place_images",
            &[source.0.to_string()],
            &json!({"x":700,"y":400}),
        )
        .unwrap();
    assert!(prepared.requires_linked_inputs());
    assert!(prepared.targets.contains(&source));
    assert_eq!(prepared.created_ids().len(), 1);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.doc_mut().items[0].path = h.base.join("rebound.png");
    assert!(h
        .app
        .finish_targeted_command(prepared, "Dot", true, "owner")
        .unwrap_err()
        .contains("linked image changed"));
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn creation_rejects_unknown_parameters_paths_wrong_engines_and_readonly() {
    let mut h = host("scoped-create-rejects");
    assert!(h
        .app
        .prepare_targeted_command("board.objects.create_generator", &[], &args("cursor"))
        .is_err());
    let mut arbitrary = args("codex");
    arbitrary["path"] = json!("C:/private");
    assert!(h
        .app
        .prepare_targeted_command("board.objects.create_train", &[], &arbitrary)
        .is_err());
    assert!(h
        .app
        .prepare_targeted_command(
            "board.objects.place_images",
            &["C:/private.png".into()],
            &json!({"x":0,"y":0})
        )
        .is_err());
    h.app.tab_mut().read_only = true;
    assert!(h
        .app
        .prepare_targeted_command("board.objects.create_train", &[], &args("codex"))
        .is_err());
}

#[test]
fn tagged_frame_refuses_missing_assignments_without_mutating_items_or_scene() {
    let mut h = host("scoped-image-frame-tags");
    let item = h
        .app
        .doc_mut()
        .add_item(h.base.join("existing.png"), "existing.png", 1, 2, "png");
    let source = h.app.doc_mut().scene.build_node(
        WorldRect::new(-500.0, -500.0, 200.0, 100.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let source_id = source.id;
    h.app.add_nodes(vec![source]);
    let frame = h
        .app
        .prepare_targeted_command(
            "board.objects.create_frame",
            &[],
            &json!({"x":0,"y":0,"width":1000,"height":800}),
        )
        .unwrap();
    let frame_id = frame.created_ids()[0];
    h.app
        .finish_targeted_command(frame, "Dot", true, "owner")
        .unwrap();
    let NodeKind::Frame(frame) = &mut h.app.doc_mut().scene.node_mut(frame_id).unwrap().kind else {
        panic!("expected frame")
    };
    frame
        .assignments
        .insert(slate_doc::GroupId(3), slate_doc::TagId(7));
    let before = serde_json::to_value(h.app.doc()).unwrap();
    assert!(h
        .app
        .prepare_targeted_command(
            "board.objects.place_images",
            &[source_id.0.to_string()],
            &json!({"x":500,"y":400})
        )
        .unwrap_err()
        .contains("frame's tags"));
    assert_eq!(serde_json::to_value(h.app.doc()).unwrap(), before);
}

#[test]
fn image_placement_is_journaled_inside_an_untagged_slide() {
    let mut h = host("scoped-image-slide");
    let item = h
        .app
        .doc_mut()
        .add_item(h.base.join("existing.png"), "existing.png", 1, 2, "png");
    let source = h.app.doc_mut().scene.build_node(
        WorldRect::new(-500.0, -500.0, 300.0, 150.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let source_id = source.id;
    h.app.add_nodes(vec![source]);
    let frame = h
        .app
        .prepare_targeted_command(
            "board.objects.create_frame",
            &[],
            &json!({"x":0,"y":0,"width":1000,"height":800}),
        )
        .unwrap();
    let frame_id = frame.created_ids()[0];
    h.app
        .finish_targeted_command(frame, "Dot", true, "owner")
        .unwrap();
    let place = h
        .app
        .prepare_targeted_command(
            "board.objects.place_images",
            &[source_id.0.to_string()],
            &json!({"x":500,"y":400}),
        )
        .unwrap();
    let placed = place.created_ids()[0];
    h.app
        .finish_targeted_command(place, "Dot", true, "owner")
        .unwrap();
    assert_eq!(h.app.doc().scene.frame_of(placed), Some(frame_id));
    let rect = h.app.doc().scene.node(placed).unwrap().rect;
    assert!((rect.w / rect.h - 2.0).abs() < 0.001);
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert!(h.app.doc().scene.node(placed).is_none());
    assert!(h.app.doc().scene.node(source_id).is_some());
}
