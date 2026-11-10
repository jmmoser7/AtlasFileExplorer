use super::*;
use serde_json::json;

#[test]
fn installed_cursor_mcp_permission_shape_is_exactly_bound() {
    use atlas_agent::collaborator::{BoardLinkSession, HostToolsLaunch};
    let root = std::env::temp_dir().join(format!("slate-permission-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let request = AgentRequest {
        id: "permission-request".into(),
        collaborator: Some(BoardLinkSession {
            session_id: "model-task-permission".into(),
            grant_id: "grant-permission".into(),
            workspace_id: "workspace-permission".into(),
        }),
        ..Default::default()
    };
    let model =
        HostToolsLaunch::model_task(root.join("helper.exe"), &root, &root, &request).unwrap();
    let dir = model.session.transport_dir(&root);
    let launch = atlas_agent::helper::HelperLaunch::Model(model.clone());
    std::fs::create_dir_all(&dir).unwrap();
    let ready = dir.join("helper-ready.json");
    std::fs::write(
        dir.join("board-session.json"),
        serde_json::to_vec(&atlas_agent::collaborator::transport::ready_value(
            &model.session,
            &request.id,
        ))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&ready,serde_json::to_vec(&json!({"session":model.session,"request_id":request.id,"initialized":true,"tools_listed":true,"tool_names":host_tools::tool_aliases(&model.session,&request.id),"deadline_unix_ms":model.deadline_unix_ms})).unwrap()).unwrap();
    let mut initializing: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ready).unwrap()).unwrap();
    initializing["tools_listed"] = json!(false);
    std::fs::write(&ready, serde_json::to_vec(&initializing).unwrap()).unwrap();
    assert!(tool_client::initialized(&launch));
    assert!(!tool_client::ready(&launch));
    tool_client::wait_initialized(Some(&launch), &AtomicBool::new(false)).unwrap();
    let denied = json!({"params":{"sessionId":"actual-acp-session","toolCall":{"kind":"other","title":format!("{}-{}: {}",tool_client::cursor_server_name(&launch),host_tools::helper_tool_aliases(&launch)[0],host_tools::helper_tool_aliases(&launch)[0])}}});
    assert!(!tool_client::allowed_cursor_tool(
        &denied,
        Some(&launch),
        Some("actual-acp-session")
    ));
    initializing["tools_listed"] = json!(true);
    std::fs::write(&ready, serde_json::to_vec(&initializing).unwrap()).unwrap();
    let server = tool_client::cursor_server_name(&launch);
    let aliases = host_tools::tool_aliases(&model.session, &request.id);
    let valid = json!({"params":{"sessionId":"actual-acp-session","toolCall":{"kind":"other","title":format!("{server}-{}: {}",aliases[0],aliases[0])}}});
    assert!(tool_client::allowed_cursor_tool(
        &valid,
        Some(&launch),
        Some("actual-acp-session")
    ));
    assert!(!tool_client::allowed_cursor_tool(
        &valid,
        Some(&launch),
        Some("other-session")
    ));
    for title in [
        "slate: slate_board_read".into(),
        format!("{server}: {}", aliases[0]),
        format!("unregistered-spoof: {}", aliases[0]),
        format!("{server}-{}: {}", aliases[0], aliases[1]),
        format!("{server}: shell"),
        format!("{server}: slate_board_read extra"),
    ] {
        let mut invalid = valid.clone();
        invalid["params"]["toolCall"]["title"] = json!(title);
        assert!(!tool_client::allowed_cursor_tool(
            &invalid,
            Some(&launch),
            Some("actual-acp-session")
        ));
    }
    let mut invalid = valid.clone();
    invalid["params"]["toolCall"]["kind"] = json!("mcp");
    assert!(!tool_client::allowed_cursor_tool(
        &invalid,
        Some(&launch),
        Some("actual-acp-session")
    ));
    std::fs::remove_file(ready).unwrap();
    assert!(!tool_client::allowed_cursor_tool(
        &valid,
        Some(&launch),
        Some("actual-acp-session")
    ));
}

#[test]
fn model_tool_aliasing_changes_the_guide_without_rewriting_supplied_input() {
    let root = std::env::temp_dir();
    let request = AgentRequest {
        id: "guide-alias-request".into(),
        prompt: "Source says slate_board_read and slate_board_command literally.".into(),
        model: Some("fable".into()),
        model_parameters: [("effort".into(), "max".into())].into(),
        collaborator: Some(atlas_agent::collaborator::BoardLinkSession {
            session_id: "model-task-guide-alias".into(),
            grant_id: "grant-guide-alias".into(),
            workspace_id: "workspace-guide-alias".into(),
        }),
        ..Default::default()
    };
    let aliases = host_tools::tool_aliases(request.collaborator.as_ref().unwrap(), &request.id);
    for provider in [Provider::ClaudeCode, Provider::Cursor] {
        let payload = payload::TextPayload::new(&request, provider, &root);
        assert!(payload.text.starts_with(&request.prompt));
        assert!(payload
            .text
            .contains(&format!("Use {} with empty node_ids", aliases[0])));
        assert_eq!(payload.text.matches(&request.prompt).count(), 1);
        assert!(!payload.text.contains("answer only the final text"));
        let wire: serde_json::Value = serde_json::from_str(&payload.claude_wire()).unwrap();
        assert_eq!(wire["message"]["content"][0]["text"], payload.text);
    }
}

#[test]
fn actual_model_payload_projects_source_once_and_keeps_authored_repeats_and_history() {
    let source="Three upright cobalt-blue rectangular blocks on a white rectangular base. Left to right, the blocks have heights 1, 2, and 3. Call the study Three-step study.";
    let mut request = AgentRequest {
        id: "model-source-once".into(),
        prompt: format!("{source}\n\nCreate a 3D model from the connected source."),
        prompt_inputs_projected: true,
        collaborator: Some(atlas_agent::collaborator::BoardLinkSession {
            session_id: "model-task-source-once".into(),
            grant_id: "grant-source-once".into(),
            workspace_id: "workspace-source-once".into(),
        }),
        ..Default::default()
    };
    request.inputs.wired.push(
        serde_json::from_value(
            json!({"node":1,"text":source,"images":[],"outputs":{},"active":null,"slot":"prompt"}),
        )
        .unwrap(),
    );
    for (prompt_count, history_count) in [(1, 0), (2, 0), (1, 1)] {
        request.prompt = std::iter::repeat_n(source, prompt_count)
            .collect::<Vec<_>>()
            .join("\n\n");
        request.history = if history_count > 0 {
            vec![AgentTurn {
                role: "user".into(),
                text: source.into(),
                at: 1,
            }]
        } else {
            vec![]
        };
        let payload =
            payload::TextPayload::new(&request, Provider::ClaudeCode, &std::env::temp_dir());
        let expected = prompt_count + history_count;
        let claude: serde_json::Value = serde_json::from_str(&payload.claude_wire()).unwrap();
        let cursor: serde_json::Value =
            serde_json::from_str(&payload.cursor_wire("session-current")).unwrap();
        assert_eq!(
            claude["message"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .matches(source)
                .count(),
            expected
        );
        assert_eq!(
            cursor["params"]["prompt"][0]["text"]
                .as_str()
                .unwrap()
                .matches(source)
                .count(),
            expected
        );
        assert_eq!(
            request.inputs.wired[0].text, source,
            "Frozen input remains intact for ReadContext"
        );
    }
}

#[test]
fn artifact_helper_is_distinct_and_uses_the_existing_canonical_payload() {
    let root = std::env::temp_dir().join(atlas_agent::request_id());
    let output = root.join(".drafts/actual-artifact/output");
    let request = AgentRequest {
        id: "actual-artifact".into(),
        prompt: "Build a dashboard from the submitted data.".into(),
        artifact: Some(atlas_agent::ArtifactRun {
            family_id: "family-artifact".into(),
            run_id: "actual-artifact".into(),
            source_root: root.clone(),
            parent: None,
            staging_output: output.clone(),
        }),
        output_dir: Some(output.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let launch = atlas_agent::helper::HelperLaunch::Artifact(
        atlas_agent::helper::ArtifactToolsLaunch::artifact(root.join("slate.exe"), &root, &request)
            .unwrap(),
    );
    assert!(validate_request_shape(&request, Some(&launch)).is_ok());
    assert!(validate_request_shape(&request, None).is_err());
    let mut stale = request.clone();
    stale.id = "another-request".into();
    assert!(validate_request_shape(&stale, Some(&launch)).is_err());
    let payload = payload::TextPayload::new(&request, Provider::ClaudeCode, &root);
    assert_eq!(payload.text, request.input_text_in(Some(&root)));
    assert!(!payload
        .text
        .contains("Slate explicit model generation task:"));
    assert!(!payload
        .text
        .contains("Slate submitted input (JSON-quoted text):"));
    let parsed: serde_json::Value = serde_json::from_str(&payload.claude_wire()).unwrap();
    assert_eq!(
        parsed["message"]["content"][0]["text"],
        request.input_text_in(Some(&root))
    );
    assert!(host_tools::helper_tool_aliases(&launch).len() >= 3);
}

#[test]
fn artifact_vision_uses_frozen_wires_once_while_text_context_policy_is_preserved() {
    use base64::Engine as _;
    let root = std::env::temp_dir().join(atlas_agent::request_id());
    std::fs::create_dir_all(&root).unwrap();
    let image = root.join("frozen.png");
    let bytes=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPQqDjxHwAEnAJobhvlygAAAABJRU5ErkJggg==").unwrap();
    std::fs::write(&image, bytes).unwrap();
    let mut request = AgentRequest::default();
    request.inputs.wired.push(
        serde_json::from_value(
            json!({"node":1,"text":"source","images":[image],"outputs":{},"active":null}),
        )
        .unwrap(),
    );
    request.inputs.context = request.inputs.wired.clone();
    assert_eq!(
        vision::load(&request, Some(|_| false)).unwrap().len(),
        2,
        "Text's explicit context policy remains unchanged"
    );
    request.artifact = Some(atlas_agent::ArtifactRun {
        family_id: "family".into(),
        run_id: "request".into(),
        source_root: root.clone(),
        parent: None,
        staging_output: root.join("output"),
    });
    assert_eq!(
        vision::load(&request, Some(|_| false)).unwrap().len(),
        1,
        "Artifact context mirrors its same frozen wires"
    );
    std::fs::remove_file(image).unwrap();
    std::fs::remove_dir(root).unwrap();
}
