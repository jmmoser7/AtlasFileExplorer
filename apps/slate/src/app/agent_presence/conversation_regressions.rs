use super::*;

#[test]
fn an_explicit_visual_read_requires_source_permission_and_rechecks_it_after_preparation() {
    let mut h = host();
    let who = add(&mut h);
    let source = h.base.join("inspect.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([20, 80, 160, 255]))
        .save(&source)
        .unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(source, "inspect.png", 64, 0, "inspect-key");
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0., 0., 40., 40.),
        slate_doc::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let node = image.id;
    h.app.add_nodes(vec![image]);
    h.app
        .presence_send_chat(&who, "Describe the picture")
        .unwrap();
    let mut a = h.app.presence.agents.remove(0);
    let call = BoardCall {
        id: "visual-read".into(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "presence.read_image".into(),
            targets: vec![node.0.to_string()],
            arguments: serde_json::json!({}),
        },
    };
    let error = h.app.begin_presence_read(&mut a, &call).unwrap_err();
    assert!(error.contains("wired inputs"), "{error}");
    assert!(a.read.is_none());
    a.grant
        .as_mut()
        .unwrap()
        .readable_sources
        .push("board-file-inputs".into());
    h.app.begin_presence_read(&mut a, &call).unwrap();
    a.grant.as_mut().unwrap().readable_sources.clear();
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut 256, std::time::Duration::from_secs(1))
        .unwrap();
    assert!(!reply.success);
    assert!(reply.result.to_string().contains("revoked"));
    assert!(reply.result.get("image_source").is_none());
}

fn program(h: &mut Harness, title: &str, view: atlas_agent::PortalView) -> slate_doc::NodeId {
    let mut portal = slate_doc::scene::PortalNode::unbound_agent(title, "cursor");
    let agent = portal.agent.as_mut().unwrap();
    agent.view = view;
    agent.instruction = "A small blue landscape".into();
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0., 0., 200., 200.),
        slate_doc::NodeKind::Portal(portal),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    id
}

#[test]
fn readable_board_descriptors_include_names_and_authored_programs_without_source_paths_or_ungranted_history(
) {
    let mut h = host();
    let who = add(&mut h);
    let item = h.app.doc_mut().add_item(
        h.base.join("private-folder/sunset.png"),
        "sunset.png",
        123,
        456,
        "image-key",
    );
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0., 0., 40., 40.),
        slate_doc::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    let value = h.app.presence_object_descriptor(&image, grant);
    assert_eq!(value["item"]["name"], "sunset.png");
    assert_eq!(value["item"]["source_inputs_granted"], false);
    assert!(!value.to_string().contains("private-folder"));
    let portal = program(&mut h, "Image workshop", atlas_agent::PortalView::Images);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    let value = h
        .app
        .presence_object_descriptor(h.app.doc().scene.node(portal).unwrap(), grant);
    assert_eq!(value["title"], "Image workshop");
    assert_eq!(value["program"]["instruction"], "A small blue landscape");
    assert!(value["program"].get("conversation").is_none());
    assert!(h.app.presence_conversation(&who).is_none());
}

#[test]
fn created_chat_scope_requires_a_fresh_human_opt_in_and_does_not_include_other_chats() {
    let mut h = host();
    let who = add(&mut h);
    let created = program(&mut h, "Agent's new chat", atlas_agent::PortalView::Chat);
    let unrelated = program(
        &mut h,
        "Another person's chat",
        atlas_agent::PortalView::Chat,
    );
    h.app
        .presence_send_chat(&who, "Create a planning chat")
        .unwrap();
    let mut a = h.app.presence.agents.remove(0);
    assert!(!h
        .app
        .presence_register_created_chats(&mut a, &[created])
        .unwrap());
    h.app.presence.agents.push(a);
    h.app.presence_set_own_chats(&who, true).unwrap();
    let mut a = h.app.presence.agents.remove(0);
    assert!(h
        .app
        .presence_register_created_chats(&mut a, &[created])
        .unwrap());
    let created_session = slate_doc::agent_chat::agent(h.app.doc().scene.node(created).unwrap())
        .unwrap()
        .session
        .clone();
    let unrelated_session =
        &slate_doc::agent_chat::agent(h.app.doc().scene.node(unrelated).unwrap())
            .unwrap()
            .session;
    assert!(a
        .grant
        .as_ref()
        .unwrap()
        .conversation_ids
        .contains(&created_session));
    assert!(!a
        .grant
        .as_ref()
        .unwrap()
        .conversation_ids
        .contains(unrelated_session));
    assert!(!a.grant.as_ref().unwrap().mutation_autonomy);
    assert!(!a
        .grant
        .as_ref()
        .unwrap()
        .readable_sources
        .iter()
        .any(|s| s == "board-file-inputs"));
    h.app.presence.agents.push(a);
    h.app.presence_set_own_chats(&who, false).unwrap();
    assert!(!h.app.presence.agents[0]
        .grant
        .as_ref()
        .unwrap()
        .conversation_ids
        .contains(&created_session));
    assert!(h.app.presence.agents[0].request.is_empty());
    assert!(h.app.presence.agents[0].owned_conversations.is_empty());
}

#[test]
fn scoped_reads_for_a_paused_conversation_do_not_move_the_listening_avatar() {
    let mut h = host();
    let who = add(&mut h);
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_text",
            &[],
            &serde_json::json!({"x":1000,"y":1000,"width":120,"height":80,"text":"Read this"}),
        )
        .unwrap();
    let target = prepared.targets[0];
    h.app.commit_scene(prepared.cmds);
    h.app.presence.agents[0].paused = true;
    h.app.presence_send_chat(&who, "What does it say?").unwrap();
    let mut a = h.app.presence.agents.remove(0);
    let before = h
        .app
        .presence
        .roster
        .session(&a.motion)
        .unwrap()
        .sample()
        .position;
    let call = BoardCall {
        id: "read-while-listening".into(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Read {
            node_ids: vec![target.0.to_string()],
            offset: 0,
            limit: 8,
        },
    };
    h.app.begin_presence_read(&mut a, &call).unwrap();
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut 256, std::time::Duration::from_secs(1))
        .unwrap();
    assert!(reply.success);
    assert_eq!(
        h.app
            .presence
            .roster
            .session(&a.motion)
            .unwrap()
            .sample()
            .position,
        before
    );
    assert_eq!(reply.result["objects"][0]["text"], "Read this");
    h.app.presence.agents.push(a);
}

#[test]
fn successful_helper_departure_preserves_its_parents_queued_harness_change() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "Prepare the deck").unwrap();
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::SpawnHelper { id: who.clone() },
        )
        .unwrap();
    let helper = h.app.presence.agents[1].profile.id.clone();
    h.app.presence_send_chat(&helper, "Read the notes").unwrap();
    h.app
        .pin_pack_for_test("codex", atlas_ai::packs::PackHealth::Ok);
    h.app.agents.set_catalog_for_test("codex", vec![]);
    h.app
        .presence_set_harness(&who, "codex", "after-task")
        .unwrap();
    for a in &mut h.app.presence.agents {
        a.had_response = true;
        a.inflight = false;
        a.activity = "success".into();
        a.note_helper_outcome("success", "Ready");
    }
    h.app.retire_completed_helper(&helper).unwrap();
    assert!(h.app.presence.agents[0].pending_harness.is_some());
    h.app.finish_presence_handoffs();
    assert_eq!(h.app.presence.agents[0].profile.provider, "codex");
}
