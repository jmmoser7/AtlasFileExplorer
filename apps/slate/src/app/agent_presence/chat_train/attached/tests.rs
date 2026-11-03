use super::*;
use crate::app::application_connector::native::{
    test_support, AgentIdentity, Lease, NativeCaller, NativeOperation, NativeOutcome, NativeResult,
};
use crate::app::tests::Harness;
use atlas_connector::{ProcessOwnership, SourceMode, SurfaceDescriptor, SurfaceMode};

fn fixture() -> (Harness, NodeId, test_support::Peer, String) {
    let mut h = crate::app::agent_presence::tests::host();
    let (node, peer) = test_support::install(&mut h, SourceMode::IsolatedDraft);
    test_support::set_surface_descriptor(
        &mut h,
        node,
        SurfaceDescriptor {
            surface_id: "headless".into(),
            mode: SurfaceMode::Headless,
            process_ownership: ProcessOwnership::LaunchedDedicated,
            native_handle: None,
            can_detach: true,
        },
    );
    test_support::add_command(
        &mut h,
        node,
        atlas_connector_runtime::headless_rhino::command_descriptor(),
    );
    let session = atlas_agent::request_id();
    h.app.patch_nodes(&[node], |n| {
        if let NodeKind::Image(image) = &mut n.kind {
            image.agent =
                slate_doc::scene::PortalNode::unbound_agent("Own 3D builder", "codex").agent;
            let agent = image.agent.as_mut().unwrap();
            agent.session = session.clone();
            agent.chat.train = false;
            agent.generator.history_cut = true;
        }
    });
    (h, node, peer, session)
}

#[test]
fn attached_model_send_requires_explicit_access_and_preserves_revocation() {
    let (mut h, node, _peer, session) = fixture();
    let workspace = h.base.clone();
    assert!(h.app.chat_train_wires(&session).is_empty());
    assert_eq!(h.app.attached_model_native_access(node), Some(false));
    assert!(h.app.validate_chat_train_send(node).is_err());
    assert!(h
        .app
        .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
        .is_err());
    assert!(h.app.presence.agents.is_empty());
    h.app
        .ensure_chat_train_membership(node, &workspace, true)
        .unwrap();
    assert_eq!(h.app.attached_model_native_access(node), Some(true));
    let request = atlas_agent::request_id();
    assert!(h
        .app
        .bind_chat_train_request(node, &workspace, &request)
        .unwrap()
        .is_some());
    let actor = h
        .app
        .presence
        .agents
        .iter_mut()
        .find(|a| a.profile.id == session)
        .unwrap();
    let membership = actor.binding.clone();
    assert!(actor
        .grant
        .as_ref()
        .unwrap()
        .allows_node(&node.0.to_string()));
    assert!(!actor.grant.as_ref().unwrap().mutation_autonomy);
    actor
        .grant
        .as_mut()
        .unwrap()
        .commands
        .retain(|command| command != "board.model.connector.invoke");
    h.app
        .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
        .unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter_mut()
        .find(|a| a.profile.id == session)
        .unwrap();
    assert!(
        !actor
            .grant
            .as_ref()
            .unwrap()
            .commands
            .iter()
            .any(|command| command == "board.model.connector.invoke"),
        "Send cannot restore a removed command scope"
    );
    actor.grant.as_mut().unwrap().readable_nodes.clear();
    assert_eq!(h.app.attached_model_native_access(node), Some(false));
    assert!(h
        .app
        .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
        .is_err());
    let actor = h
        .app
        .presence
        .agents
        .iter_mut()
        .find(|a| a.profile.id == session)
        .unwrap();
    actor.grant = None;
    assert!(h.app.validate_chat_train_send(node).is_err());
    assert!(h
        .app
        .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
        .is_err());
    assert_eq!(h.app.presence.agents.len(), 1);
    assert_eq!(h.app.presence.agents[0].binding, membership);
    assert!(h.app.presence.agents[0].grant.is_none());
    assert!(
        h.app
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .unwrap()
            .generator
            .history_cut,
        "native context cannot restore cut memory"
    );
}

#[test]
fn attached_model_mutation_uses_existing_scoped_permit_and_revision_does_not_nag() {
    let (mut h, node, peer, session) = fixture();
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(node, &workspace, true)
        .unwrap();
    let request = atlas_agent::request_id();
    h.app
        .bind_chat_train_request(node, &workspace, &request)
        .unwrap()
        .unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter_mut()
        .find(|a| a.profile.id == session)
        .unwrap();
    actor.grant.as_mut().unwrap().mutation_autonomy = true; // explicit test-owned consent
    let identity = AgentIdentity {
        agent: session.clone(),
        membership: actor.binding.clone(),
        request: request.clone(),
        call: atlas_agent::request_id(),
        epoch: actor.control_epoch,
    };
    let binding = h.app.native_binding(node).unwrap();
    let lease = Lease::new();
    let command = atlas_connector_runtime::headless_rhino::GENERATE_COMMAND;
    let operation = || NativeOperation::Invoke {
        command: command.into(),
        parameters: serde_json::json!({"plan":"{\"units\":\"source\",\"operations\":[]}"}),
    };
    assert!(h
        .app
        .native_submit(
            node,
            &binding,
            identity.clone(),
            false,
            operation(),
            Lease::new()
        )
        .is_err());
    let ticket = h
        .app
        .native_submit(
            node,
            &binding,
            identity.clone(),
            true,
            operation(),
            lease.clone(),
        )
        .map_err(|_| "scoped invocation refused")
        .unwrap();
    let native = peer.next().unwrap();
    match &native.caller {
        NativeCaller::Agent {
            permit: Some(Box::new(permit)),
            ..
        } => assert!(permit.valid_for(&identity, &binding.key(), command)),
        _ => panic!("scoped draft permit missing"),
    };
    let path = atlas_core::index::data_dir().join(format!(
        "headless-rhino/models/native-{}.3dm",
        atlas_agent::request_id()
    ));
    peer.complete(
        native,
        NativeOutcome::Completed {
            result: NativeResult::Model {
                path: path.clone(),
                size: 1,
                mtime: 0,
                report: serde_json::json!({}),
            },
        },
        "accepted-revision",
    );
    let completion = h.app.native_poll(node, &ticket).unwrap().unwrap();
    assert!(lease.publish());
    h.app
        .native_publish_headless_model(node, &completion)
        .unwrap();
    let advanced = h.app.headless_model_agent_context(node).unwrap();
    assert!(advanced.key() == binding.key());
    assert_eq!(advanced.binding.source.revision, "accepted-revision");
    h.app.maintain_chat_train_memberships();
    assert_eq!(h.app.attached_model_native_access(node), Some(true));
    assert!(h
        .app
        .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
        .unwrap()
        .is_some());
    assert_eq!(h.app.presence.agents.len(), 1);
    assert!(
        h.app.presence.agents[0]
            .grant
            .as_ref()
            .unwrap()
            .mutation_autonomy
    );
}

#[test]
fn attached_model_read_only_stale_and_external_sources_cannot_gain_mutation_scope() {
    let (mut h, node, _peer, session) = fixture();
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(node, &workspace, true)
        .unwrap();
    h.app
        .presence
        .agents
        .iter_mut()
        .find(|a| a.profile.id == session)
        .unwrap()
        .grant
        .as_mut()
        .unwrap()
        .mutation_autonomy = true;
    h.app.tab_mut().read_only = true;
    h.app
        .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
        .unwrap();
    assert!(
        !h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == session)
            .unwrap()
            .grant
            .as_ref()
            .unwrap()
            .mutation_autonomy
    );
    h.app.tab_mut().read_only = false;
    let item = h
        .app
        .item_for_ready_path(&h.base.join("external.3dm"), 1, 0);
    h.app.patch_nodes(&[node], |n| {
        if let NodeKind::Image(image) = &mut n.kind {
            image.item = item;
        }
    });
    assert!(h.app.native_binding(node).is_err());
    assert_eq!(h.app.attached_model_native_access(node), None);
    h.app.maintain_chat_train_memberships();
    assert!(h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == session)
        .is_none_or(|a| a.paused || a.grant.is_none()));
    assert!(h
        .app
        .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
        .unwrap()
        .is_none());
}

#[test]
fn attached_identity_events_without_scene_edits_fence_send_until_explicit_resume() {
    for changed in ["worker", "source", "document"] {
        for maintain_first in [false, true] {
            let (mut h, node, peer, session) = fixture();
            let workspace = h.base.clone();
            h.app
                .ensure_chat_train_membership(node, &workspace, true)
                .unwrap();
            h.app
                .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
                .unwrap()
                .unwrap();
            h.app.maintain_chat_train_memberships();
            let generation = (h.app.scene_gen, h.app.doc().scene.scene_gen());
            let old_epoch = h
                .app
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == session)
                .unwrap()
                .control_epoch;
            let mut observed = peer.binding.clone();
            observed.observation += 1;
            match changed {
                "worker" => observed.worker = atlas_agent::request_id(),
                "source" => observed.binding.source.source_id = "external-source".into(),
                "document" => observed.binding.document_id = "external-document".into(),
                _ => unreachable!(),
            }
            // Same path and unchanged scene; the production connector event pump
            // receives the replacement binding from another thread.
            h.app
                .test_deliver_headless_identity(node, observed.clone(), changed == "worker");
            assert_eq!((h.app.scene_gen, h.app.doc().scene.scene_gen()), generation);
            assert!(h.app.headless_model_agent_context(node).unwrap().key() == observed.key());
            assert_eq!(h.app.attached_model_native_access(node), Some(false));
            assert!(h.app.validate_chat_train_send(node).is_err());
            if maintain_first {
                h.app.maintain_chat_train_memberships();
            }
            assert!(h
                .app
                .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
                .is_err());
            assert!(!h.app.presence.chat_trains.contains_key(&session));
            let actor = h
                .app
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == session)
                .unwrap();
            assert!(actor.paused);
            assert!(actor.control_epoch > old_epoch);
            assert!(actor.request.is_empty());
            // Neither repeated Send nor the event pump itself installs a grant
            // binding for the new native identity.
            assert!(h
                .app
                .ensure_chat_train_membership(node, &workspace, false)
                .is_err());
            h.app
                .ensure_chat_train_membership(node, &workspace, true)
                .unwrap();
            assert_eq!(h.app.attached_model_native_access(node), Some(false));
            assert!(h
                .app
                .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
                .is_err());
            h.app
                .apply_presence_action(
                    &h.ctx,
                    crate::app::agent_presence::chrome::PresenceAction::Pause {
                        id: session.clone(),
                        paused: false,
                    },
                )
                .unwrap();
            assert_eq!(h.app.attached_model_native_access(node), Some(true));
            assert!(h
                .app
                .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
                .unwrap()
                .is_some());
            assert!(
                h.app
                    .presence
                    .chat_trains
                    .get(&session)
                    .unwrap()
                    .attached
                    .as_ref()
                    .unwrap()
                    .key
                    == observed.key()
            );
            h.app.stop_presence(&session, true).unwrap();
            assert_eq!(h.app.attached_model_native_access(node), Some(false));
            assert!(h
                .app
                .bind_chat_train_request(node, &workspace, &atlas_agent::request_id())
                .is_err());
            assert!(h
                .app
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == session)
                .unwrap()
                .grant
                .is_none());
        }
    }
}
