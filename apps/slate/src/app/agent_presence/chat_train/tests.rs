use super::*;
use crate::app::application_connector::native::test_support;
use crate::app::tests::Harness;
use slate_doc::scene::{ConnectorEnd, PortalNode, Side, WorldRect};

fn setup() -> (Harness, NodeId, NodeId, NodeId, String) {
    let mut h = crate::app::agent_presence::tests::host();
    let (source, _peer) = test_support::install(&mut h, atlas_connector::SourceMode::Linked);
    let (unrelated, _other) = test_support::install(&mut h, atlas_connector::SourceMode::Linked);
    let mut portal = PortalNode::unbound_agent("Actual Codex chat", "codex");
    portal.agent.as_mut().unwrap().session = atlas_agent::request_id();
    portal.agent.as_mut().unwrap().chat.train = true;
    let session = portal.agent.as_ref().unwrap().session.clone();
    let node = h.app.doc().scene.node_preparation().build_node(
        WorldRect::new(1000.0, 0.0, 600.0, 500.0),
        NodeKind::Portal(portal),
    );
    let chat = h.app.add_nodes(vec![node])[0];
    let wire = h.app.build_connector_with(
        ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: chat,
            side: Side::Left,
            t: 0.5,
        },
        &[],
    );
    h.app.add_nodes(vec![wire]);
    (h, source, unrelated, chat, session)
}

#[test]
fn chat_train_uses_actual_session_and_narrow_wires_without_auto_permissions() {
    let (mut h, source, unrelated, chat, session) = setup();
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
        .unwrap();
    let request = atlas_agent::request_id();
    let binding = h
        .app
        .bind_chat_train_request(chat, &workspace, &request)
        .unwrap()
        .unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == session)
        .unwrap();
    assert_eq!(actor.profile.provider, "codex");
    assert_eq!(actor.request, request);
    assert_eq!(actor.binding, binding);
    assert_eq!(actor.dir, h.app.agent_link_dir(chat, &workspace).unwrap());
    let grant = actor.grant.as_ref().unwrap();
    assert!(!grant.readable_board && !grant.mutation_autonomy);
    assert!(grant.readable_sources.is_empty());
    assert!(grant.allows_node(&source.0.to_string()));
    assert!(!grant.allows_node(&unrelated.0.to_string()));
    assert_eq!(
        grant.commands,
        [
            "board.model.connector.inspect",
            "board.model.connector.capture",
            "board.model.connector.invoke",
            "presence.read_image",
            "presence.read_text",
        ]
    );
    assert_eq!(grant.helper_limit, 0);
}

#[test]
fn chat_train_binding_preserves_explicit_autonomy_and_input_grants() {
    let (mut h, _, _, chat, session) = setup();
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
        .unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter_mut()
        .find(|a| a.profile.id == session)
        .unwrap();
    actor
        .grant
        .as_mut()
        .unwrap()
        .readable_sources
        .push("board-file-inputs".into());
    actor.grant.as_mut().unwrap().mutation_autonomy = true;
    h.app
        .bind_chat_train_request(chat, &workspace, &atlas_agent::request_id())
        .unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == session)
        .unwrap();
    assert!(actor.grant.as_ref().unwrap().mutation_autonomy);
    assert_eq!(
        actor.grant.as_ref().unwrap().readable_sources,
        ["board-file-inputs"]
    );
}

#[test]
fn deleting_explicit_wire_revokes_chat_membership_request() {
    let (mut h, _, _, chat, session) = setup();
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
        .unwrap();
    h.app
        .bind_chat_train_request(chat, &workspace, &atlas_agent::request_id())
        .unwrap();
    let wire = h.app.presence.chat_trains[&session].wires[0].wire;
    h.app.delete_board_nodes_now(&[wire]);
    h.app.maintain_chat_train_memberships();
    let actor = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == session)
        .unwrap();
    assert!(actor.request.is_empty() && actor.paused);
    assert!(!h.app.presence.chat_trains.contains_key(&session));
}

#[test]
fn mere_selected_context_never_grants_native_board_tools() {
    let (mut h, source, _, chat, session) = setup();
    let wire = h.app.chat_train_wires(&session)[0].wire;
    h.app.delete_board_nodes_now(&[wire]);
    h.app.board_sel = std::iter::once(source).collect();
    let workspace = h.base.clone();
    assert!(h
        .app
        .bind_chat_train_request(chat, &workspace, &atlas_agent::request_id())
        .unwrap()
        .is_none());
    assert!(h.app.presence.agents.is_empty());
}

#[test]
fn sending_cannot_create_resume_or_restore_human_revoked_membership() {
    let (mut h, _, _, chat, session) = setup();
    let workspace = h.base.clone();
    assert!(h
        .app
        .bind_chat_train_request(chat, &workspace, &atlas_agent::request_id())
        .is_err());
    assert!(h.app.presence.agents.is_empty());
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
        .unwrap();
    let original = h.app.presence.agents[0].binding.clone();
    h.app.presence.agents[0].paused = true;
    assert!(h
        .app
        .bind_chat_train_request(chat, &workspace, &atlas_agent::request_id())
        .is_err());
    assert!(h.app.presence.agents[0].paused);
    assert_eq!(h.app.presence.agents[0].binding, original);
    h.app.presence.agents[0].paused = false;
    h.app.presence.agents[0].grant = None;
    assert!(h
        .app
        .bind_chat_train_request(chat, &workspace, &atlas_agent::request_id())
        .is_err());
    assert!(h.app.presence.agents[0].grant.is_none());
    assert_eq!(h.app.presence.agents[0].profile.id, session);
    assert_eq!(h.app.presence.agents.len(), 1);
}

#[test]
fn explicit_reference_image_stays_readable_but_is_not_a_native_target() {
    use slate_doc::scene::ImageNode;
    let (mut h, source, unrelated, chat, session) = setup();
    let item = h.app.doc_mut().add_item(
        h.base.join("reference.png"),
        "reference.png",
        1,
        0,
        "reference",
    );
    let node = h.app.doc().scene.node_preparation().build_node(
        WorldRect::new(0.0, 0.0, 50.0, 50.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let reference = h.app.add_nodes(vec![node])[0];
    let wire = h.app.build_connector_with(
        ConnectorEnd::Anchored {
            node: reference,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: chat,
            side: Side::Left,
            t: 0.5,
        },
        &[],
    );
    h.app.add_nodes(vec![wire]);
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
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
        .readable_sources
        .push("board-file-inputs".into());
    let request = atlas_agent::request_id();
    let binding = h
        .app
        .bind_chat_train_request(chat, &workspace, &request)
        .unwrap()
        .unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == session)
        .unwrap();
    assert!(actor
        .grant
        .as_ref()
        .unwrap()
        .allows_node(&reference.0.to_string()));
    assert!(h.app.native_binding(reference).is_err());
    let grant = actor.grant.as_ref().unwrap();
    let call = |command: &str, target: NodeId| BoardCall {
        id: atlas_agent::request_id(),
        session: binding.clone(),
        request_id: request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: command.into(),
            targets: vec![target.0.to_string()],
            arguments: serde_json::json!({}),
        },
    };
    // This is the actual authorize owner that rejected the live wired PNG.
    assert!(grant
        .authorize(&call("presence.read_image", reference), &binding, &request)
        .is_ok());
    assert!(grant.commands.iter().any(|c| c == "presence.read_text"));
    assert!(grant
        .authorize(
            &call("board.model.connector.inspect", source),
            &binding,
            &request
        )
        .is_ok());
    assert!(grant
        .authorize(&call("presence.read_image", unrelated), &binding, &request)
        .is_err());
    assert!(grant
        .authorize(
            &call("board.model.connector.activate_draft", source),
            &binding,
            &request
        )
        .is_err());
    assert!(grant
        .authorize(&call("board.objects.remove", reference), &binding, &request)
        .is_err());
    assert_eq!(grant.readable_sources, ["board-file-inputs"]);
}

#[test]
fn queued_and_relayed_messages_cannot_launch_a_second_chat_train_runtime() {
    let (mut h, _, _, chat, session) = setup();
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
        .unwrap();
    let binding = h.app.presence.agents[0].binding.clone();
    h.app.presence.agents[0].paused = true;
    let request = atlas_agent::request_id();
    for (from, queued) in [
        (None, None),
        (Some("other-agent"), None),
        (None, Some(request.as_str())),
    ] {
        assert!(h
            .app
            .submit_presence_message(&session, "Inspect the model", from, queued)
            .is_err());
    }
    let actor = &h.app.presence.agents[0];
    assert!(actor.runtime.is_none() && actor.request.is_empty() && actor.paused);
    assert_eq!(actor.binding, binding);
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn explicit_csv_file_wire_is_readable_without_becoming_a_native_target() {
    use slate_doc::scene::ImageNode;
    let (mut h, _, _, chat, session) = setup();
    let path = h.base.join("layers.csv");
    std::fs::write(&path, b"path,red,green,blue\nTest,1,2,3\n").unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(path, "layers.csv", 32, 0, "csv-reference");
    let node = h.app.doc().scene.node_preparation().build_node(
        WorldRect::new(0.0, 0.0, 100.0, 80.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let reference = h.app.add_nodes(vec![node])[0];
    let wire = h.app.build_connector_with(
        ConnectorEnd::Anchored {
            node: reference,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: chat,
            side: Side::Left,
            t: 0.5,
        },
        &[],
    );
    h.app.add_nodes(vec![wire]);
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
        .unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == session)
        .unwrap();
    assert!(actor
        .grant
        .as_ref()
        .unwrap()
        .allows_node(&reference.0.to_string()));
    assert!(h
        .app
        .native_explicit_inputs(&session, chat)
        .iter()
        .any(|(_, node, _)| *node == reference));
    assert!(h.app.native_binding(reference).is_err());
}

#[test]
fn tools_thread_locator_sync_requires_exact_request_and_keeps_local_identity_and_grant() {
    let (mut h, _, _, chat, session) = setup();
    h.app.patch_nodes(&[chat], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().channel = Some("prior-thread".into());
    });
    let workspace = h.base.clone();
    h.app
        .ensure_chat_train_membership(chat, &workspace, true)
        .unwrap();
    let request = atlas_agent::request_id();
    h.app
        .bind_chat_train_request(chat, &workspace, &request)
        .unwrap();
    let binding = h.app.presence.agents[0].binding.clone();
    let grant = h.app.presence.agents[0].grant.clone();
    let mut state = atlas_agent::AgentSession {
        provider: "codex".into(),
        conversation: "tools-thread".into(),
        request: "unrelated-request".into(),
        ..Default::default()
    };
    h.app.sync_chat_train_provider_channel(chat, &state);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(chat).unwrap())
            .unwrap()
            .channel
            .as_deref(),
        Some("prior-thread")
    );
    state.request = request;
    h.app.sync_chat_train_provider_channel(chat, &state);
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(chat).unwrap()).unwrap();
    assert_eq!(agent.channel.as_deref(), Some("tools-thread"));
    assert_eq!(agent.session, session);
    assert_eq!(h.app.presence.agents[0].binding, binding);
    assert_eq!(h.app.presence.agents[0].grant, grant);
}
