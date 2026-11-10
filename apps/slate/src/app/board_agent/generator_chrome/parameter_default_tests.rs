use super::*;
use crate::app::board_flow::tests::generator_with_output;

#[test]
fn registered_parameter_reset_omits_only_override_and_rejects_missing_or_invalid_values() {
    let (mut h, id) = generator_with_output("parameter-default-reset");
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.provider = "codex-image".into();
        agent.model = Some("advertised".into());
        agent
            .model_parameters
            .insert("effort".into(), "high".into());
    });
    h.app.agents.set_catalog_for_test("codex-image", vec![serde_json::from_value(serde_json::json!({
        "id":"advertised", "name":"Actual discovered label", "parameters":[
            {"id":"effort","name":"Thought level","values":[{"value":"high","name":"Vendor intense"}]},
            {"id":"serviceTier","name":"Speed","values":[{"value":"priority","name":"Fast"},{"value":"instant","name":"Ultrafast"}]}
        ]
    })).unwrap()]);
    let command = atlas_commands::CommandId("portal.agent.model_parameter");
    let change = |value: serde_json::Value| {
        serde_json::json!({"node":id,"parameter":"serviceTier","value":value}).to_string()
    };
    h.app
        .dispatch(&h.ctx, command, Some(change(serde_json::json!("instant"))));
    let captured = h.app.generation_request(id, false).unwrap();
    assert_eq!(
        captured
            .model_parameters
            .get("serviceTier")
            .map(String::as_str),
        Some("instant")
    );
    let source = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    for detail in [
        serde_json::json!({"node":id,"parameter":"serviceTier"}),
        serde_json::json!({"node":id,"parameter":"serviceTier","value":false}),
        serde_json::json!({"node":id,"parameter":"unknown","value":null}),
        serde_json::json!({"node":id,"parameter":"serviceTier","value":"fabricated"}),
    ] {
        assert!(!h.app.generator_set_parameter(Some(&detail.to_string())));
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &source);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
    h.app.tab_mut().read_only = true;
    assert!(!h
        .app
        .generator_set_parameter(Some(&change(serde_json::Value::Null))));
    h.app.tab_mut().read_only = false;
    h.app
        .dispatch(&h.ctx, command, Some(change(serde_json::Value::Null)));
    let reset = h.app.generation_request(id, false).unwrap();
    assert!(!reset.model_parameters.contains_key("serviceTier"));
    assert_eq!(
        reset.model_parameters.get("effort").map(String::as_str),
        Some("high")
    );
    assert_eq!(
        captured
            .model_parameters
            .get("serviceTier")
            .map(String::as_str),
        Some("instant")
    );
    let after = h.app.doc().scene.node(id).unwrap();
    let mut expected = source.clone();
    slate_doc::agent_chat::agent_mut(&mut expected)
        .unwrap()
        .model_parameters
        .remove("serviceTier");
    assert_eq!(
        after, &expected,
        "source/model/history and every other setting unchanged"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&source),
        "Undo restores the exact prior instant override and all other source/model/history data"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(
        captured
            .model_parameters
            .get("serviceTier")
            .map(String::as_str),
        Some("instant"),
        "Undo cannot rewrite an immutable captured request"
    );
}

#[test]
fn ordinary_unbound_web_parameter_reset_keeps_authored_source_and_binding_absent() {
    let mut h = crate::app::tests::Harness::new("ordinary-web-default-reset");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_agent_programs_for_test(&["codex"]);
    h.app.agents.set_catalog_for_test(
        "codex",
        vec![serde_json::from_value(serde_json::json!({
        "id":"available", "name":"Discovered", "is_default":true, "parameters":[
            {"id":"serviceTier","name":"Speed","values":[{"value":"instant","name":"Ultrafast"}]}
        ]
    })).unwrap()],
    );
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20.0, 20.0, 640.0, 360.0),
        NodeKind::Portal(slate_doc::scene::PortalNode::bound_web(
            "Original",
            "https://example.invalid/",
        )),
    );
    let id = node.id;
    let original = node.clone();
    h.app.add_nodes(vec![node]);
    assert!(h.app.set_pending_generator_provider(id, "codex"));
    assert!(h.app.set_pending_generator_model(id, "available"));
    let set =
        serde_json::json!({"node":id,"parameter":"serviceTier","value":"instant"}).to_string();
    let reset = serde_json::json!({"node":id,"parameter":"serviceTier","value":null}).to_string();
    assert!(h.app.generator_set_parameter(Some(&set)));
    assert_eq!(
        h.app
            .pending_generator_choice(id)
            .unwrap()
            .2
            .get("serviceTier")
            .map(String::as_str),
        Some("instant")
    );
    assert!(h.app.generator_set_parameter(Some(&reset)));
    assert!(h.app.pending_generator_choice(id).unwrap().2.is_empty());
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &original);
    assert!(slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).is_none());
    assert!(h.app.agents.request_of(id).is_none());
}
