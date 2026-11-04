//! Generator UI commands cross the ordinary scoped witness/stage/journal path.
use super::*;
use slate_doc::scene::{CmdAuthor, ModelOutputRef, NodeId, SceneCmd, SourceUri};

fn setup() -> (Harness, String, NodeId) {
    let (mut h, node) =
        crate::app::board_flow::tests::generator_with_output("generator-command-parity");
    h.app.tab_mut().path = Some(h.base.join("generators.slate"));
    let path = h.base.join("prior.3dm");
    h.app.item_for_ready_path(&path, 17, 0);
    h.app.patch_nodes(&[node], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.bundle = Some(SourceUri {
            locator: "history/session.json".into(),
        });
        agent.generator.count = Some(1);
        agent.generator.preserve_history = Some(false);
        agent.generator.history_cut = true;
        agent.generator.model_outputs = vec![ModelOutputRef {
            id: "prior-model".into(),
            source: SourceUri {
                locator: path.to_string_lossy().into(),
            },
            size: 17,
            mtime: 0,
        }];
    });
    h.app.agents.sessions.insert(
        node,
        Arc::new(AgentSession {
            turns: [
                ("user", "private question"),
                ("assistant", "private old answer"),
                ("user", "next question"),
                ("assistant", "latest answer"),
            ]
            .into_iter()
            .map(|(role, text)| atlas_agent::AgentTurn {
                role: role.into(),
                text: text.into(),
                at: 0,
            })
            .collect(),
            ..Default::default()
        }),
    );
    let conversation = slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap())
        .unwrap()
        .session
        .clone();
    h.app.board_sel.clear();
    let who = add(&mut h, "Dot");
    let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
    grant.readable_sources.push("board-file-inputs".into());
    grant.conversation_ids.push(conversation);
    (h, who, node)
}

fn commands() -> [(&'static str, serde_json::Value); 4] {
    [
        (
            "portal.agent.generator_settings",
            json!({"count":3,"preserve_history":true}),
        ),
        ("board.input.graft", json!({"slot":"style","grafted":true})),
        ("portal.agent.text_output", json!({"through":2})),
        ("portal.agent.model_output", json!({"output":"prior-model"})),
    ]
}

#[test]
fn generator_commands_share_human_patches_with_witnessed_approved_agent_journals() {
    for (command, arguments) in commands() {
        let (mut h, who, node) = setup();
        let before = h.app.doc().scene.node(node).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let items = h.app.doc().items.clone();
        let prepared = h
            .app
            .prepare_targeted_command(command, &[node.0.to_string()], &arguments)
            .unwrap();
        assert_eq!(prepared.targets, [node]);
        assert_eq!(prepared.witness_nodes(&h.app.doc().scene), [before.clone()]);
        let SceneCmd::Patch { after, .. } = &prepared.cmds[0] else {
            panic!("selection/settings must be an ordinary patch");
        };
        let expected = after.as_ref().clone();
        assert_eq!(h.app.doc().scene.node(node), Some(&before));
        assert_eq!(
            h.app.doc().items,
            items,
            "preparation cannot register model items"
        );
        let mut detail = arguments.clone();
        detail["node"] = json!(node);
        assert!(
            h.app.dispatch(
                &h.ctx,
                atlas_commands::CommandId(command),
                Some(detail.to_string())
            ),
            "human {command}"
        );
        assert_eq!(h.app.doc().scene.node(node), Some(&expected));
        assert_eq!(h.app.tab().journal.last_author(), Some(&CmdAuthor::Human));
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(node), Some(&before));
        let proposal = proposal_support::stage_command(&mut h, &who, command, &[node], arguments);
        assert_eq!(h.app.doc().scene.node(node), Some(&before));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        h.app.accept_stage_proposal(&proposal);
        assert_eq!(
            h.app.doc().scene.node(node),
            Some(&before),
            "approval still waits for the real witness"
        );
        proposal_support::finish(&mut h, &who);
        assert_eq!(
            h.app.doc().scene.node(node),
            Some(&expected),
            "agent {command}"
        );
        assert_eq!(
            h.app.tab().journal.last_author(),
            Some(&CmdAuthor::Agent("Dot".into()))
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(node), Some(&before));
        h.app.board_redo();
        assert_eq!(h.app.doc().scene.node(node), Some(&expected));
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap()).unwrap();
        let original = slate_doc::agent_chat::agent(&before).unwrap();
        assert_eq!(agent.session, original.session);
        assert_eq!(agent.channel, original.channel);
        assert!(
            agent.generator.history_cut,
            "history cannot be resurrected by preservation"
        );
        assert_eq!(
            agent.generator.archived_history,
            original.generator.archived_history
        );
    }
}

#[test]
fn generator_command_schema_and_target_validation_fail_without_mutation() {
    let (mut h, _, node) = setup();
    let scene = h.app.doc().scene.clone();
    let items = h.app.doc().items.clone();
    let depth = h.app.tab().journal.undo_depth();
    for (command, valid) in commands() {
        assert!(BOARD_COMMANDS.contains(&command));
        assert!(h.app.presence_command_schema()[command].is_object());
        for targets in [
            vec![],
            vec![node.0.to_string(), node.0.to_string()],
            vec!["999999".into()],
            vec!["C:/private.3dm".into()],
            vec!["0".into()],
        ] {
            assert!(
                h.app
                    .prepare_targeted_command(command, &targets, &valid)
                    .is_err(),
                "{command} {targets:?}"
            );
        }
        let mut foreign = valid.clone();
        foreign["source"] = json!("C:/private.3dm");
        assert!(h
            .app
            .prepare_targeted_command(command, &[node.0.to_string()], &foreign)
            .is_err());
        h.app.tab_mut().read_only = true;
        assert!(h
            .app
            .prepare_targeted_command(command, &[node.0.to_string()], &valid)
            .is_err());
        h.app.tab_mut().read_only = false;
    }
    for (command, args) in [
        ("portal.agent.generator_settings", json!({"count":0})),
        ("portal.agent.generator_settings", json!({"count":9})),
        ("portal.agent.generator_settings", json!({"count":1.5})),
        (
            "portal.agent.generator_settings",
            json!({"preserve_history":"true"}),
        ),
        ("board.input.graft", json!({"slot":"view","grafted":true})),
        (
            "board.input.graft",
            json!({"slot":"unknown","grafted":true}),
        ),
        ("board.input.graft", json!({"slot":"style","grafted":1})),
        ("portal.agent.text_output", json!({"through":0})),
        ("portal.agent.text_output", json!({"through":1})),
        ("portal.agent.text_output", json!({"through":99})),
        ("portal.agent.model_output", json!({"output":"latest"})),
        (
            "portal.agent.model_output",
            json!({"output":"C:/private.3dm"}),
        ),
    ] {
        assert!(
            h.app
                .prepare_targeted_command(command, &[node.0.to_string()], &args)
                .is_err(),
            "{command}: {args}"
        );
    }
    assert_eq!(h.app.doc().scene, scene);
    assert_eq!(h.app.doc().items, items);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.patch_nodes(&[node], |node| node.locked = true);
    for (command, args) in commands() {
        assert!(h
            .app
            .prepare_targeted_command(command, &[node.0.to_string()], &args)
            .is_err());
    }
}

#[test]
fn generator_command_grant_revocation_blocks_the_accepted_patch() {
    for (command, args) in commands() {
        let (mut h, who, node) = setup();
        let before = h.app.doc().scene.node(node).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let proposal = proposal_support::stage_command(&mut h, &who, command, &[node], args);
        let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
        match command {
            "portal.agent.text_output" => grant.conversation_ids.clear(),
            "portal.agent.model_output" => grant.readable_sources.clear(),
            _ => grant.commands.retain(|allowed| allowed != command),
        }
        h.app.accept_stage_proposal(&proposal);
        for _ in 0..10 {
            proposal_support::tick(&mut h);
        }
        assert_eq!(h.app.doc().scene.node(node), Some(&before));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(h.app.presence.agents[0].work.is_none());
    }
}

#[test]
fn text_output_scope_is_checked_before_private_checkpoint_validation() {
    let (mut h, _, node) = setup();
    let before = h.app.doc().scene.clone();
    let mut actor = h.app.presence.agents.remove(0);
    actor.grant.as_mut().unwrap().conversation_ids.clear();
    actor.request = atlas_agent::request_id();
    h.app
        .presence
        .roster
        .session_mut(&actor.motion)
        .unwrap()
        .begin_request(id(&actor.request).unwrap())
        .unwrap();
    for (sequence, through) in [1, 2, 99999].into_iter().enumerate() {
        let call = BoardCall {
            id: atlas_agent::request_id(),
            session: actor.binding.clone(),
            request_id: actor.request.clone(),
            sequence: sequence as u64,
            operation: BoardOperation::Invoke {
                command: "portal.agent.text_output".into(),
                targets: vec![node.0.to_string()],
                arguments: json!({"through":through}),
            },
        };
        let reply = h.app.presence_call(&mut actor, &call).unwrap();
        assert!(!reply.success);
        assert!(
            reply.result["error"]
                .as_str()
                .unwrap()
                .contains("conversation endpoint"),
            "checkpoint existence/role must remain private"
        );
        assert!(actor.work.is_none());
    }
    assert_eq!(h.app.doc().scene, before);
}

#[test]
fn generator_command_descriptors_preserve_conversation_and_source_boundaries() {
    let (mut h, _, node) = setup();
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap().clone();
    let descriptor = h
        .app
        .presence_object_descriptor(h.app.doc().scene.node(node).unwrap(), &grant);
    assert_eq!(
        descriptor["program"]["generator"]["model_outputs"][0]["id"],
        "prior-model"
    );
    assert_eq!(
        descriptor["program"]["generator"]["text_outputs"],
        json!([{"through":2},{"through":4}])
    );
    let mut limited = grant;
    limited.conversation_ids.clear();
    let descriptor = h
        .app
        .presence_object_descriptor(h.app.doc().scene.node(node).unwrap(), &limited);
    assert!(descriptor["program"]["generator"]
        .get("text_outputs")
        .is_none());
    let encoded = serde_json::to_string(&descriptor).unwrap();
    assert!(!encoded.contains("private old answer") && !encoded.contains("private question"));
    assert!(!encoded.contains("prior.3dm") && !encoded.contains("session.json"));
    let turns = (0..300)
        .map(|_| atlas_agent::AgentTurn {
            role: "assistant".into(),
            text: "private".into(),
            at: 0,
        })
        .collect();
    h.app.agents.sessions.insert(
        node,
        Arc::new(AgentSession {
            turns,
            ..Default::default()
        }),
    );
    let descriptor = h.app.presence_object_descriptor(
        h.app.doc().scene.node(node).unwrap(),
        h.app.presence.agents[0].grant.as_ref().unwrap(),
    );
    assert_eq!(
        descriptor["program"]["generator"]["text_outputs"]
            .as_array()
            .unwrap()
            .len(),
        128
    );
    assert_eq!(
        descriptor["program"]["generator"]["text_outputs_total"],
        300
    );
    assert_eq!(
        descriptor["program"]["generator"]["text_outputs_truncated"],
        true
    );
    let oversized = "x".repeat(4000);
    h.app.patch_nodes(&[node], |node| {
        let generator = &mut slate_doc::agent_chat::agent_mut(node).unwrap().generator;
        let mut invalid = generator.model_outputs[0].clone();
        invalid.id = oversized.clone();
        generator.model_outputs.insert(0, invalid);
        generator.shown_model = Some(oversized.clone());
        generator.grafted_slots.insert(oversized.clone());
        generator.grafted_slots.insert("style".into());
        generator.grafted_slots.insert("view".into());
    });
    let descriptor = h.app.presence_object_descriptor(
        h.app.doc().scene.node(node).unwrap(),
        h.app.presence.agents[0].grant.as_ref().unwrap(),
    );
    assert_eq!(
        descriptor["program"]["generator"]["model_outputs"],
        json!([{"id":"prior-model"}])
    );
    assert_eq!(
        descriptor["program"]["generator"]["model_outputs_unavailable"],
        1
    );
    assert!(descriptor["program"]["generator"]["shown_model"].is_null());
    assert!(!serde_json::to_string(&descriptor)
        .unwrap()
        .contains(&oversized));
    assert_eq!(
        descriptor["program"]["generator"]["grafted_slots"],
        json!(["style"])
    );
}

#[test]
fn missing_model_registration_and_stale_preparation_never_fallback_or_mutate() {
    let (mut h, _, node) = setup();
    let args = json!({"output":"prior-model"});
    let prepared = h
        .app
        .prepare_targeted_command("portal.agent.model_output", &[node.0.to_string()], &args)
        .unwrap();
    let item = h.app.doc().items.last().unwrap().id;
    assert!(h.app.doc_mut().remove_item(item));
    let before = h.app.doc().scene.clone();
    let items = h.app.doc().items.clone();
    let depth = h.app.tab().journal.undo_depth();
    let error = h
        .app
        .prepare_targeted_command("portal.agent.model_output", &[node.0.to_string()], &args)
        .unwrap_err();
    assert!(error.contains("human preview control"));
    assert!(h
        .app
        .finish_targeted_command(prepared, "Dot", true, "missing-link")
        .is_err());
    assert_eq!(h.app.doc().scene, before);
    assert_eq!(h.app.doc().items, items);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(
        h.app
            .select_model_generator_output(node, "prior-model")
            .is_ok(),
        "human link recovery remains available"
    );
    assert_eq!(h.app.doc().items.len(), items.len() + 1);
}
