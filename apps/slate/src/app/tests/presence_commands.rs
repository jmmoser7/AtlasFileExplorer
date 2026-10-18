//! Collaborators use the same typed board commands with explicit authorship.
use super::*;
use serde_json::json;
use slate_doc::scene::{CmdAuthor, ConnectorEnd, SceneCmd};

fn setup(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h
}

fn prepare(
    h: &Harness,
    command: &str,
    ids: &[NodeId],
    args: serde_json::Value,
) -> super::super::dispatch::PreparedBoardCommand {
    h.app
        .prepare_targeted_command(
            command,
            &ids.iter().map(|id| id.0.to_string()).collect::<Vec<_>>(),
            &args,
        )
        .unwrap()
}

#[test]
fn witnessed_commit_keeps_agent_authorship_and_one_undo_step() {
    let mut h = setup("presence-command-author");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let b = add_filled_rect(&mut h.app, 180.0, 0.0, 100.0, 80.0);
    h.app.board_sel.insert(b);
    let before = h.app.doc().scene.node(a).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let prepared = prepare(&h, "board.objects.move", &[a], json!({"dx":70,"dy":25}));
    assert_eq!(h.app.doc().scene.node(a), Some(&before));
    assert_eq!(h.app.board_sel.iter().copied().collect::<Vec<_>>(), vec![b]);
    assert!(h
        .app
        .finish_targeted_command(prepared, "Dot", true, "session-a")
        .unwrap()
        .starts_with("applied:"));
    assert_eq!(h.app.doc().scene.node(a).unwrap().rect.x, 70.0);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene.node(a), Some(&before));
}

#[test]
fn synthetic_presence_session_without_live_prepared_context_fails_closed_on_accept() {
    let mut h = setup("presence-command-stage");
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let depth = h.app.tab().journal.undo_depth();
    let prepared = prepare(&h, "board.objects.move", &[a], json!({"dx":10,"dy":20}));
    let status = h
        .app
        .finish_targeted_command(prepared, "Dot", false, "session-a")
        .unwrap();
    let id = status.strip_prefix("staged:").unwrap();
    assert_eq!(h.app.doc().scene.node(a).unwrap().rect.x, 0.0);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let proposal = h
        .app
        .pending_stage_proposals()
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .clone();
    h.app.accept_stage_proposal(id);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.doc().scene.node(a).unwrap().rect.x, 0.0);
    assert!(h
        .app
        .toasts
        .last()
        .is_some_and(|(text, _)| text.contains("no live prepared action")));
    assert_eq!(slate_doc::stage::read_result(&h.base, id).unwrap(), None);
    h.app.accept_stage_proposal_data(&proposal);
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth,
        "synthetic session cannot bypass the missing live execution context"
    );
}

#[test]
fn human_edit_or_tab_switch_invalidates_prepared_work_without_partial_application() {
    let mut h = setup("presence-command-conflict");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let b = add_filled_rect(&mut h.app, 200.0, 0.0, 100.0, 80.0);
    let prepared = prepare(&h, "board.objects.move", &[a, b], json!({"dx":20,"dy":0}));
    h.app.patch_nodes(&[b], |node| node.rect.y += 5.0);
    assert!(prepared.expected_matches(&h.app.doc().scene).is_err());
    assert!(h
        .app
        .finish_targeted_command(prepared, "Dot", true, "session-a")
        .is_err());
    assert_eq!(h.app.doc().scene.node(a).unwrap().rect.x, 0.0);
    let prepared = prepare(&h, "board.objects.move", &[a], json!({"dx":20,"dy":0}));
    h.app.tab_mut().id += 1000;
    assert!(
        h.app
            .finish_targeted_command(prepared, "Dot", true, "session-a")
            .is_err(),
        "distinct unsaved tabs are distinct identities"
    );
}

#[test]
fn frame_move_witnesses_members_and_refuses_locked_children() {
    let mut h = setup("presence-command-frame");
    let frame = deck_frame(&mut h, WorldRect::new(0.0, 0.0, 960.0, 540.0), 0);
    let member = add_filled_rect(&mut h.app, 20.0, 20.0, 100.0, 80.0);
    let prepared = prepare(
        &h,
        "board.objects.move",
        &[frame],
        json!({"dx":100,"dy":50}),
    );
    assert!(prepared.targets.contains(&member));
    assert_eq!(prepared.cmds.len(), 2);
    h.app.patch_nodes(&[member], |node| node.locked = true);
    assert!(h
        .app
        .prepare_targeted_command(
            "board.objects.move",
            &[frame.0.to_string()],
            &json!({"dx":100,"dy":50})
        )
        .is_err());
}

#[test]
fn typed_human_dispatch_uses_same_translation_and_human_journal() {
    let mut h = setup("presence-command-human");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let detail =
        json!({"targets":[id.0.to_string()],"arguments":{"width":150,"height":90}}).to_string();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.objects.resize"),
        Some(detail)
    ));
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.w, 150.0);
    assert_eq!(h.app.tab().journal.last_author(), Some(&CmdAuthor::Human));
}

#[test]
fn creation_uses_board_recipes_without_reserving_ids_or_selection() {
    let mut h = setup("presence-command-create");
    for (command, extra) in [
        ("board.objects.create_frame", json!({"title":"Opening"})),
        ("board.objects.create_shape", json!({"shape":"ellipse"})),
        ("board.objects.create_text", json!({"text":"Hello, board"})),
    ] {
        let mut args = json!({"x":10,"y":20,"width":120,"height":90});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let prepared = prepare(&h, command, &[], args.clone());
        assert_eq!(h.app.doc().scene.nodes.len(), 0);
        assert_eq!(prepared.targets.len(), 1);
        let same = prepare(&h, command, &[], args);
        assert_eq!(prepared.targets, same.targets);
        let SceneCmd::Add { node, .. } = &prepared.cmds[0] else {
            panic!("creation must be an Add");
        };
        match &node.kind {
            NodeKind::Frame(frame) => assert_eq!(frame.title, "Opening"),
            NodeKind::Shape(shape) => assert_eq!(shape.shape, slate_doc::scene::ShapeKind::Ellipse),
            NodeKind::Text(text) => assert_eq!(text.text, "Hello, board"),
            _ => panic!("unexpected recipe result"),
        }
    }
    h.app.board_sel.clear();
}

#[test]
fn deletion_reuses_connector_degradation_and_undo_restores_it() {
    let mut h = setup("presence-command-delete");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let b = add_filled_rect(&mut h.app, 200.0, 0.0, 100.0, 80.0);
    let prepared = prepare(&h, "board.objects.wire", &[a, b], json!({}));
    let wire = *prepared.targets.last().unwrap();
    h.app
        .finish_targeted_command(prepared, "Dot", true, "session-a")
        .unwrap();
    let original = h.app.doc().scene.node(wire).unwrap().clone();
    let prepared = prepare(&h, "board.objects.delete", &[a], json!({}));
    assert!(
        prepared.targets.contains(&wire),
        "wire change must be witnessed too"
    );
    h.app
        .finish_targeted_command(prepared, "Dot", true, "session-a")
        .unwrap();
    assert!(h.app.doc().scene.node(a).is_none());
    let NodeKind::Connector(connector) = &h.app.doc().scene.node(wire).unwrap().kind else {
        panic!();
    };
    assert!(matches!(connector.a, ConnectorEnd::Free { .. }));
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene.node(wire), Some(&original));
    assert!(h.app.doc().scene.node(a).is_some());
}

#[test]
fn source_files_bad_geometry_and_read_only_are_rejected() {
    let mut h = setup("presence-command-boundaries");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    for (targets, args) in [
        (
            vec![r"C:\private\photo.png".to_owned()],
            json!({"dx":1,"dy":0}),
        ),
        (
            vec![id.0.to_string()],
            json!({"dx":1,"dy":0,"path":"source.png"}),
        ),
        (vec![id.0.to_string()], json!({"dx":1e30,"dy":0})),
        (vec![], json!({"dx":1,"dy":0})),
    ] {
        assert!(h
            .app
            .prepare_targeted_command("board.objects.move", &targets, &args)
            .is_err());
    }
    h.app.tab_mut().read_only = true;
    assert!(h
        .app
        .prepare_targeted_command(
            "board.objects.move",
            &[id.0.to_string()],
            &json!({"dx":1,"dy":0})
        )
        .is_err());
}

#[test]
fn staged_z_order_preserves_requested_position() {
    let mut h = setup("presence-command-z-order");
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let b = add_filled_rect(&mut h.app, 200.0, 0.0, 100.0, 80.0);
    let c = add_filled_rect(&mut h.app, 400.0, 0.0, 100.0, 80.0);
    let prepared = prepare(&h, "board.objects.order", &[c], json!({"z":0}));
    let status = h
        .app
        .finish_targeted_command(prepared, "Dot", false, "session-a")
        .unwrap();
    h.app
        .accept_stage_proposal(status.strip_prefix("staged:").unwrap());
    assert_eq!(
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        vec![c, a, b]
    );
}

#[test]
fn public_command_schema_and_argument_validation_share_one_catalogue() {
    let h = setup("presence-command-schema");
    let schema = SlateApp::targeted_command_schema();
    let entries = schema.as_object().unwrap();
    let approved = std::collections::BTreeSet::from([
        "board.objects.create_frame",
        "board.objects.create_generator",
        "board.objects.create_shape",
        "board.objects.create_text",
        "board.objects.create_train",
        "board.objects.crop",
        "board.objects.delete",
        "board.objects.move",
        "board.objects.order",
        "board.objects.place_images",
        "board.objects.resize",
        "board.objects.wire",
        "portal.agent.run",
    ]);
    assert_eq!(
        entries
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        approved
    );
    for (command, definition) in entries {
        assert!(commands::SPECS.iter().any(|spec| spec.id.0 == command));
        assert!(definition["arguments"].is_object());
        assert!(definition["targets"].is_string());
    }
    let frame_args = &schema["board.objects.create_frame"]["arguments"];
    assert!(frame_args.get("title").is_some());
    assert!(frame_args.get("text").is_none());
    let crop_args = &schema["board.objects.crop"]["arguments"];
    for field in ["mode", "handle", "x", "y", "dx", "dy"] {
        assert!(crop_args.get(field).is_some(), "missing crop field {field}");
    }
    assert_eq!(crop_args.as_object().unwrap().len(), 6);
    let good = json!({"x":0,"y":0,"width":960,"height":540,"title":"Intro"});
    assert!(h
        .app
        .prepare_targeted_command("board.objects.create_frame", &[], &good)
        .is_ok());
    let bad = json!({"x":0,"y":0,"width":960,"height":540,"text":"Intro"});
    assert!(h
        .app
        .prepare_targeted_command("board.objects.create_frame", &[], &bad)
        .is_err());
    assert!(schema["board.objects.resize"]["arguments"]
        .get("x")
        .is_some());
    assert!(schema["board.objects.order"]["arguments"]
        .get("order")
        .is_some());
    assert!(schema["board.objects.create_shape"]["arguments"]
        .get("shape")
        .is_some());
    assert!(schema["board.objects.wire"]["arguments"]
        .get("source_side")
        .is_some());
}

#[test]
fn revocation_fences_cloned_proposals_without_any_disk_write() {
    let mut h = setup("presence-command-revocation");
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let prepared = prepare(&h, "board.objects.move", &[id], json!({"dx":100,"dy":0}));
    let status = h
        .app
        .finish_targeted_command(prepared, "Dot", false, "session-revoked")
        .unwrap();
    let proposal_id = status.strip_prefix("staged:").unwrap();
    let proposal = h
        .app
        .pending_stage_proposals()
        .iter()
        .find(|proposal| proposal.id == proposal_id)
        .unwrap()
        .clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app.cancel_presence_proposals("session-revoked");
    assert!(h
        .app
        .pending_stage_proposals()
        .iter()
        .all(|pending| pending.id != proposal_id));
    h.app.accept_stage_proposal_data(&proposal);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 0.0);
    assert_eq!(
        slate_doc::stage::read_result(&h.base, proposal_id).unwrap(),
        None
    );
}

#[test]
fn failed_rejection_persistence_still_revokes_the_in_memory_mutation_permit() {
    let mut h = setup("presence-command-reject-failure");
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let prepared = prepare(&h, "board.objects.move", &[id], json!({"dx":100,"dy":0}));
    let status = h
        .app
        .finish_targeted_command(prepared, "Dot", false, "session-rejected")
        .unwrap();
    let proposal_id = status.strip_prefix("staged:").unwrap();
    let proposal = h
        .app
        .pending_stage_proposals()
        .iter()
        .find(|proposal| proposal.id == proposal_id)
        .unwrap()
        .clone();
    let depth = h.app.tab().journal.undo_depth();
    // Losing the workspace prevents recording rejection. It must not prevent
    // local revocation, nor permit accepting a retained proposal clone later.
    h.app.ai.config.workspace_dir = None;
    h.app.reject_stage_proposal(proposal_id);
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.accept_stage_proposal_data(&proposal);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 0.0);
    assert!(h
        .app
        .pending_stage_proposals()
        .iter()
        .all(|pending| pending.id != proposal_id));
}

#[test]
fn a_stage_file_cannot_substitute_commands_under_a_live_presence_proposal_id() {
    let mut h = setup("presence-command-payload-fence");
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let prepared = prepare(&h, "board.objects.move", &[id], json!({"dx":10,"dy":0}));
    let status = h
        .app
        .finish_targeted_command(prepared, "Dot", false, "session-live")
        .unwrap();
    let proposal_id = status.strip_prefix("staged:").unwrap();
    let mut substituted = h
        .app
        .pending_stage_proposals()
        .iter()
        .find(|proposal| proposal.id == proposal_id)
        .unwrap()
        .clone();
    let depth = h.app.tab().journal.undo_depth();
    let SceneCmd::Patch { after, .. } = &mut substituted.cmds[0] else {
        panic!();
    };
    after.rect.x = 500.0;
    h.app.accept_stage_proposal_data(&substituted);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 0.0);
    assert!(h
        .app
        .pending_stage_proposals()
        .iter()
        .all(|pending| pending.id != proposal_id));
}

#[test]
fn generator_patches_coalesce_with_their_own_author_only() {
    let mut h = setup("presence-patch-author-boundary");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    h.app.last_board_edit = None;
    h.app.patch_nodes(&[id], |node| node.rect.x = 10.0);
    let depth = h.app.tab().journal.undo_depth();
    h.app
        .patch_nodes_as(&[id], CmdAuthor::Agent("Dot".into()), |node| {
            node.rect.x = 20.0
        });
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    h.app
        .patch_nodes_as(&[id], CmdAuthor::Agent("Dot".into()), |node| {
            node.rect.x = 30.0
        });
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.patch_nodes(&[id], |node| node.rect.x = 40.0);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);
    assert_eq!(h.app.tab().journal.last_author(), Some(&CmdAuthor::Human));
}
