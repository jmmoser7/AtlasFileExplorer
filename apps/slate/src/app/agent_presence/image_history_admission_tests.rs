//! Strict argument and autonomous admission coverage on the registered route.
use super::*;

#[test]
fn collaborator_downstream_preserves_human_selected_source_and_open_panel() {
    for autonomous in [false, true] {
        let (mut h, who, source, _) = setup();
        h.app.board_sel = [source].into_iter().collect();
        h.app.shape_properties.ids = vec![source];
        h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Agent);
        let selected = h.app.board_sel.clone();
        let old_ids = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        let runs = h.app.agents.dispatched.len();
        let args = json!({"action":"downstream","x":1800,"y":30});
        if autonomous {
            h.app.presence.agents[0]
                .grant
                .as_mut()
                .unwrap()
                .mutation_autonomy = true;
            assert!(invoke(&mut h, source, args).is_none());
        } else {
            let proposal = proposal_support::stage_command(
                &mut h,
                &who,
                "board.image.history",
                &[source],
                args,
            );
            assert_eq!(h.app.board_sel, selected);
            assert_eq!(
                h.app.shape_properties.panel,
                Some(crate::app::board_properties::Panel::Agent)
            );
            h.app.accept_stage_proposal(&proposal);
        }
        proposal_support::finish(&mut h, &who);
        assert_eq!(
            h.app.board_sel, selected,
            "collaborator creation preserves human selection"
        );
        assert_eq!(h.app.shape_properties.ids, vec![source]);
        assert_eq!(
            h.app.shape_properties.panel,
            Some(crate::app::board_properties::Panel::Agent),
            "collaborator creation preserves the human panel"
        );
        let child = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find(|node| {
                !old_ids.contains(&node.id) && matches!(node.kind, slate_doc::NodeKind::Image(_))
            })
            .expect("actual collaborator downstream creation committed");
        assert!(!h.app.board_sel.contains(&child.id));
        assert_eq!((child.rect.x, child.rect.y), (1800.0, 30.0));
        assert_eq!(
            h.app.agents.dispatched.len(),
            runs,
            "creation never submits inference"
        );
    }
    let (mut h, _, source, _) = setup();
    h.app.board_sel = [source].into_iter().collect();
    h.app.shape_properties.ids = vec![source];
    h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Agent);
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.image.history"),
        Some(json!({"node":source.0,"action":"downstream"}).to_string())
    ));
    assert!(!h.app.board_sel.contains(&source));
    assert!(
        h.app.shape_properties.panel.is_none(),
        "human downstream retains its explicit panel cleanup"
    );
}

#[test]
fn history_mutation_schema_rejects_forged_payloads_and_duplicate_targets_without_effect() {
    let (mut h, _, source, child) = setup();
    let scene = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(BOARD_COMMANDS.contains(&"board.image.history"));
    assert!(h.app.presence_command_schema()["board.image.history"].is_object());
    for args in [
        json!({"action":"view"}),
        json!({"action":"refresh","turns":[]}),
        json!({"action":"downstream","provider":"comfy"}),
        json!({"action":"cut","pin":{}}),
        json!({"action":"downstream","x":1}),
        json!({"action":"refresh","x":1,"y":2}),
        json!({"action":"downstream","x":1e30,"y":0}),
    ] {
        assert!(
            h.app
                .prepare_targeted_command("board.image.history", &[child.0.to_string()], &args)
                .is_err(),
            "{args}"
        );
    }
    assert!(h
        .app
        .prepare_targeted_command(
            "board.image.history",
            &[child.0.to_string(), child.0.to_string()],
            &json!({"action":"cut"})
        )
        .is_err());
    assert!(
        h.app
            .prepare_targeted_command(
                "board.image.history",
                &[source.0.to_string()],
                &json!({"action":"cut"})
            )
            .is_err(),
        "interior source cannot cut"
    );
    assert_eq!(h.app.doc().scene, scene);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.tab_mut().read_only = true;
    assert!(h
        .app
        .prepare_targeted_command(
            "board.image.history",
            &[child.0.to_string()],
            &json!({"action":"refresh"})
        )
        .is_err());
}

#[test]
fn history_autonomous_cut_uses_same_cancel_finish_and_single_agent_undo() {
    let (mut h, who, _, child) = setup();
    h.app.set_agent_session_for_test(
        child,
        AgentSession {
            status: atlas_agent::AgentStatus::Thinking,
            ..Default::default()
        },
    );
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .mutation_autonomy = true;
    assert!(invoke(&mut h, child, json!({"action":"cut"})).is_none());
    proposal_support::finish(&mut h, &who);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&slate_doc::scene::CmdAuthor::Agent(
            "History collaborator".into()
        ))
    );
    assert!(!h.app.agent_generation_dispatched(child));
    assert!(h.app.agents.session(child).is_none());
    assert_ne!(h.app.doc().scene, before);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene, before);
    assert!(!h.app.agent_generation_dispatched(child));
}

#[test]
fn history_saved_pin_endpoint_is_not_replaced_by_current_parent_permission() {
    let (mut h, _, source, child) = setup();
    let old = slate_doc::agent_chat::agent(h.app.doc().scene.node(source).unwrap())
        .unwrap()
        .session
        .clone();
    h.app.patch_nodes(&[source], |n| {
        slate_doc::agent_chat::agent_mut(n).unwrap().session = "fresh-parent-endpoint".into()
    });
    let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
    grant.conversation_ids.retain(|endpoint| endpoint != &old);
    grant.conversation_ids.push("fresh-parent-endpoint".into());
    let reply = invoke(&mut h, child, json!({"action":"view"}))
        .expect("saved endpoint denied before public turn read");
    assert!(!reply.success);
    assert!(reply.result.to_string().contains("endpoint"));
    assert!(h.app.presence.agents[0].read.is_none());
}
