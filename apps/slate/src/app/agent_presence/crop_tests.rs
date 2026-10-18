//! The scoped adapter retains ordinary image eligibility and journal fences.
use super::*;
use crate::app::tests::picture_from;
use slate_doc::scene::{CmdAuthor, NodeId, SceneCmd};

fn setup() -> (Harness, String, NodeId) {
    let mut h = host();
    h.app.set_agent_programs_for_test(&[]);
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.tab_mut().path = Some(h.base.join("crop-board.slate"));
    let image = picture_from(&mut h, "crop-command.png", 0.0);
    h.app.board_sel.clear();
    let who = add(&mut h, "Dot");
    (h, who, image)
}

fn handle() -> serde_json::Value {
    json!({"mode":"handle","handle":4,"x":120,"y":90})
}

#[test]
fn crop_schema_rejects_mixed_modes_bad_keys_bounds_and_non_single_image_targets() {
    let (mut h, _, image) = setup();
    let target = [image.0.to_string()];
    assert!(SlateApp::targeted_command_schema()
        .get("board.objects.crop")
        .is_some());
    assert!(BOARD_COMMANDS.contains(&"board.objects.crop"));
    assert_eq!(
        super::super::work::command_activity("board.objects.crop"),
        "resizing"
    );
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    for args in [
        json!({"mode":"handle","handle":4,"x":120,"y":90,"dx":1}),
        json!({"mode":"handle","handle":4,"x":120,"y":90,"source":"private.png"}),
        json!({"mode":"handle","handle":8,"x":120,"y":90}),
        json!({"mode":"handle","handle":-1,"x":120,"y":90}),
        json!({"mode":"handle","handle":1.5,"x":120,"y":90}),
        json!({"mode":"handle","handle":"4","x":120,"y":90}),
        json!({"mode":"handle","handle":4,"x":1e99,"y":90}),
        json!({"mode":"handle","handle":4,"x":null,"y":90}),
        json!({"mode":"handle","handle":4,"x":120}),
        json!({"mode":"pan","dx":10,"dy":0,"handle":4}),
        json!({"mode":"pan","dx":1e10,"dy":0}),
        json!({"mode":"pan","dx":10}),
        json!({"mode":"uv","x":0.2,"y":0.2}),
        json!({"dx":10,"dy":0}),
        serde_json::Value::Null,
    ] {
        assert!(
            h.app
                .prepare_targeted_command("board.objects.crop", &target, &args)
                .is_err(),
            "{args}"
        );
    }
    for targets in [
        vec![],
        vec![target[0].clone(), target[0].clone()],
        vec!["999999".into()],
        vec!["C:/source.png".into()],
    ] {
        assert!(h
            .app
            .prepare_targeted_command("board.objects.crop", &targets, &handle())
            .is_err());
    }
    assert_eq!(h.app.doc().scene, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let shape = h
        .app
        .prepare_targeted_command(
            "board.objects.create_shape",
            &[],
            &json!({"x":220,"y":0,"width":40,"height":40}),
        )
        .unwrap();
    let shape_id = shape.created_ids()[0];
    h.app.commit_scene(shape.cmds);
    assert!(h
        .app
        .prepare_targeted_command("board.objects.crop", &[shape_id.0.to_string()], &handle())
        .is_err());
    h.app.tab_mut().read_only = true;
    assert!(h
        .app
        .prepare_targeted_command("board.objects.crop", &target, &handle())
        .is_err());
    h.app.tab_mut().read_only = false;
    h.app.patch_nodes(&[image], |node| node.locked = true);
    assert!(h
        .app
        .prepare_targeted_command("board.objects.crop", &target, &handle())
        .is_err());
}

#[test]
fn crop_complete_node_patch_is_one_agent_undo_redo_without_camera_selection_or_tool_changes() {
    let (mut h, _, image) = setup();
    h.app.patch_nodes(&[image], |node| {
        node.rotation_deg = 19.0;
        node.opacity = 0.7;
        if let slate_doc::NodeKind::Image(image) = &mut node.kind {
            image.flip_x = true;
        }
    });
    let before = h.app.doc().scene.node(image).unwrap().clone();
    let contact = crate::app::board_crop::gesture_contact(
        &before,
        crate::app::board_crop::CropGesture::Handle {
            handle: 4,
            world: (0.0, 0.0),
        },
    )
    .unwrap();
    let (cx, cy) = before.rect.center();
    let args =
        json!({"mode":"handle","handle":4,"x":cx+(contact.0-cx)*0.75,"y":cy+(contact.1-cy)*0.75});
    let prepared = h
        .app
        .prepare_targeted_command("board.objects.crop", &[image.0.to_string()], &args)
        .unwrap();
    assert!(prepared.crop_gesture().is_some());
    assert!(
        !prepared.requires_linked_inputs(),
        "crop metadata grants no source access"
    );
    let expected = match &prepared.cmds[0] {
        SceneCmd::Patch { before: old, after } => {
            assert_eq!(old.as_ref(), &before);
            after.as_ref().clone()
        }
        _ => panic!("one complete Node patch"),
    };
    assert_eq!(prepared.cmds.len(), 1);
    let depth = h.app.tab().journal.undo_depth();
    let camera = (h.app.tab().cam.offset, h.app.tab().cam.z);
    let selection = h.app.board_sel.clone();
    let tool = h.app.board_tool;
    let items = h.app.doc().items.clone();
    h.app
        .finish_targeted_command(prepared, "Dot", true, "crop-undo-session")
        .unwrap();
    assert_eq!(h.app.doc().scene.node(image), Some(&expected));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    assert_eq!((h.app.tab().cam.offset, h.app.tab().cam.z), camera);
    assert_eq!(h.app.board_sel, selection);
    assert_eq!(h.app.board_tool, tool);
    assert_eq!(h.app.doc().items, items);
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(tab.doc.scene.node(image), Some(&before));
    assert!(tab.journal.redo(&mut tab.doc.scene));
    assert_eq!(tab.doc.scene.node(image), Some(&expected));
}

#[test]
fn crop_stale_and_read_only_prepared_actions_leave_the_live_node_unchanged() {
    let (mut h, _, image) = setup();
    let targets = [image.0.to_string()];
    let prepared = h
        .app
        .prepare_targeted_command("board.objects.crop", &targets, &handle())
        .unwrap();
    h.app.patch_nodes(&[image], |node| node.opacity = 0.5);
    let live = h.app.doc().scene.node(image).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h
        .app
        .finish_targeted_command(prepared, "Dot", true, "stale")
        .is_err());
    assert_eq!(h.app.doc().scene.node(image), Some(&live));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let prepared = h
        .app
        .prepare_targeted_command("board.objects.crop", &targets, &handle())
        .unwrap();
    h.app.tab_mut().read_only = true;
    assert!(h
        .app
        .finish_targeted_command(prepared, "Dot", true, "read-only")
        .is_err());
    assert_eq!(h.app.doc().scene.node(image), Some(&live));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

fn stage(h: &mut Harness, image: NodeId) -> String {
    let who = h.app.presence.agents[0].profile.id.clone();
    let rect = h.app.doc().scene.node(image).unwrap().rect;
    let args = json!({"mode":"handle","handle":4,"x":rect.x+rect.w*0.9,"y":rect.y+rect.h*0.9});
    super::proposal_support::stage_command(h, &who, "board.objects.crop", &[image], args)
}

#[test]
fn crop_proposals_use_the_existing_accept_replay_recovery_and_revocation_fences() {
    let (mut h, who, image) = setup();
    let before = h.app.doc().scene.node(image).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let proposal = stage(&mut h, image);
    assert_eq!(h.app.doc().scene.node(image), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let mut recovered = h.app.pending_stage_proposals()[0].clone();
    recovered.status = slate_doc::ProposalStatus::RecoveryRequired;
    h.app.accept_stage_proposal_data(&recovered);
    assert_eq!(h.app.doc().scene.node(image), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    // Recovery cannot resurrect a live permit; a fresh proposal is required.
    assert!(!h
        .app
        .pending_stage_proposals()
        .iter()
        .any(|p| p.id == proposal));
    let proposal = stage(&mut h, image);
    h.app.accept_stage_proposal(&proposal);
    assert_eq!(h.app.doc().scene.node(image), Some(&before));
    super::proposal_support::finish(&mut h, &who);
    let accepted = h.app.doc().scene.node(image).unwrap().clone();
    assert_ne!(accepted, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.accept_stage_proposal(&proposal);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let proposal = stage(&mut h, image);
    h.app
        .apply_presence_action(&h.ctx, chrome::PresenceAction::Revoke { id: who.clone() })
        .unwrap();
    h.app.accept_stage_proposal(&proposal);
    assert_eq!(h.app.doc().scene.node(image), Some(&accepted));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let mut agent = h.app.presence.agents.remove(0);
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: agent.binding.clone(),
        request_id: agent.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "board.objects.crop".into(),
            targets: vec![image.0.to_string()],
            arguments: handle(),
        },
    };
    assert!(!h.app.presence_call(&mut agent, &call).unwrap().success);
    assert!(agent.work.is_none());
    h.app.presence.agents.push(agent);
}

#[test]
fn image_read_descriptor_exposes_bounded_crop_geometry_without_source_paths() {
    let (h, _, image) = setup();
    let node = h.app.doc().scene.node(image).unwrap();
    let value = h
        .app
        .presence_object_descriptor(node, h.app.presence.agents[0].grant.as_ref().unwrap());
    assert_eq!(value["rotation_deg"], json!(node.rotation_deg));
    assert_eq!(value["croppable"], true);
    assert!(value["crop"].is_object() && value["mirror"].is_object());
    assert_eq!(value["mask"]["present"], false);
    assert_eq!(value["item"]["name"], "crop-command.png");
    assert!(value["item"].get("path").is_none());
    let encoded_base = serde_json::to_string(&h.base).unwrap();
    assert!(!serde_json::to_string(&value)
        .unwrap()
        .contains(encoded_base.trim_matches('"')));
}
