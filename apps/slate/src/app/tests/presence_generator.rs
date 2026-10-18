//! Provider runs are witnessed effects, never empty journal patches.
use super::*;
use serde_json::json;

#[test]
fn provider_run_requires_autonomy_without_changing_the_human_draft() {
    let (mut h, generator) = board_flow::tests::generator_with_output("presence-provider-approval");
    *h.app.agents.prompt_mut(generator) = "Human draft stays here".into();
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let prepared = h
        .app
        .prepare_targeted_command(
            "portal.agent.run",
            &[generator.0.to_string()],
            &json!({"prompt":"A brighter hall"}),
        )
        .unwrap();
    assert_eq!(
        prepared
            .witness_nodes(&h.app.doc().scene)
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        vec![generator]
    );
    let error = h
        .app
        .finish_targeted_command(prepared, "Dot", false, "presence-job")
        .unwrap_err();
    assert!(error.contains("autonomous board edits"));
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(
        h.app.agents.prompt(generator),
        Some("Human draft stays here")
    );
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn provider_run_uses_existing_mock_dispatch_with_agent_authorship() {
    let (mut h, generator) = board_flow::tests::generator_with_output("presence-provider-dispatch");
    *h.app.agents.prompt_mut(generator) = "Human draft stays here".into();
    let prepared = h
        .app
        .prepare_targeted_command(
            "portal.agent.run",
            &[generator.0.to_string()],
            &json!({"prompt":"A brighter hall"}),
        )
        .unwrap();
    assert!(
        prepared.cmds.is_empty(),
        "this existing generator needs no scene preparation"
    );
    let depth = h.app.tab().journal.undo_depth();
    let status = h
        .app
        .finish_targeted_command(prepared, "Dot", true, "presence-job")
        .unwrap();
    assert!(status.starts_with("queued:"));
    let deadline = Instant::now() + Duration::from_secs(2);
    while h.app.agents.dispatched.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        h.app.pump_comfy_queue();
    }
    let (target, request) = h
        .app
        .agents
        .dispatched
        .last()
        .expect("mock generator received its request");
    assert_eq!(*target, generator);
    assert_eq!(request.prompt, "A brighter hall");
    assert_eq!(
        h.app.agents.prompt(generator),
        Some("Human draft stays here")
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth,
        "queueing alone must not add an identity patch"
    );
    assert_eq!(
        h.app.cmd_history.iter().last().unwrap().author,
        atlas_commands::CmdAuthor::Agent("Dot".into())
    );
}

#[test]
fn rejected_provider_arguments_never_start_a_job() {
    let (h, generator) = board_flow::tests::generator_with_output("presence-provider-invalid");
    for arguments in [
        json!({"prompt":""}),
        json!({"prompt":42}),
        json!({"path":"secret.png"}),
    ] {
        assert!(h
            .app
            .prepare_targeted_command("portal.agent.run", &[generator.0.to_string()], &arguments)
            .is_err());
    }
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn generator_source_consent_distinguishes_file_inputs_from_a_fresh_text_prompt() {
    let (mut h, generator) =
        board_flow::tests::generator_with_output("presence-provider-source-scope");
    let with_image = h
        .app
        .prepare_targeted_command(
            "portal.agent.run",
            &[generator.0.to_string()],
            &json!({"prompt":"A brighter hall"}),
        )
        .unwrap();
    assert!(with_image.requires_linked_inputs());
    h.app.patch_nodes(&[generator], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().seed = None;
        if let NodeKind::Image(image) = &mut node.kind {
            image.item = slate_doc::ItemId::NONE;
        }
    });
    let text_only = h
        .app
        .prepare_targeted_command(
            "portal.agent.run",
            &[generator.0.to_string()],
            &json!({"prompt":"A new blue circle"}),
        )
        .unwrap();
    assert!(!text_only.requires_linked_inputs());
}
