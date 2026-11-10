use super::*;

#[test]
fn artifact_projected_prompt_is_not_requoted_in_metadata_and_nonprojected_data_survives() {
    let root = std::env::temp_dir();
    let mut request = AgentRequest {
        id: "artifact-projection".into(),
        prompt: "ARTIFACT_FROZEN_SENTINEL\n\nHuman suffix".into(),
        prompt_inputs_projected: true,
        artifact: Some(ArtifactRun {
            family_id: "family".into(),
            run_id: "artifact-projection".into(),
            source_root: root.clone(),
            parent: None,
            staging_output: root.join("output"),
        }),
        ..Default::default()
    };
    request.inputs.wired.push(ContextItem {
        node: 1,
        text: "ARTIFACT_FROZEN_SENTINEL".into(),
        slot: Some(InputSlot::Prompt),
        images: vec![],
        outputs: Default::default(),
        active: None,
        depth: None,
    });
    let frozen = request.inputs.clone();
    let projected = request.input_text();
    assert!(projected.contains("\"text_supplied_inline\":true"));
    assert!(projected.contains("manifest lists FILES ONLY"));
    let metadata: serde_json::Value = serde_json::from_str(
        projected
            .split("Slate wired attachments (data):\n")
            .nth(1)
            .unwrap(),
    )
    .unwrap();
    assert!(
        metadata[0].get("text").is_none(),
        "projected inline text is not mislabeled as an empty attachment body"
    );
    assert_eq!(metadata[0]["text_supplied_inline"], true);
    assert_eq!(
        request
            .input_text()
            .matches("ARTIFACT_FROZEN_SENTINEL")
            .count(),
        1
    );
    request.prompt.push_str("\nARTIFACT_FROZEN_SENTINEL");
    request.history.push(AgentTurn {
        role: "user".into(),
        text: "ARTIFACT_FROZEN_SENTINEL".into(),
        at: 0,
    });
    assert_eq!(
        request
            .input_text()
            .matches("ARTIFACT_FROZEN_SENTINEL")
            .count(),
        3,
        "intentional authored and history repetitions survive"
    );
    assert_eq!(
        request.inputs, frozen,
        "immutable ToolContext remains complete"
    );
    request.prompt_inputs_projected = false;
    request.prompt = "Human suffix".into();
    request.history.clear();
    assert!(!request
        .input_text()
        .contains("\"text_supplied_inline\":true"));
    assert_eq!(
        request
            .input_text()
            .matches("ARTIFACT_FROZEN_SENTINEL")
            .count(),
        1,
        "nonprojected source remains in metadata"
    );
    assert_eq!(request.inputs, frozen);
}

#[test]
fn model_task_quotes_projected_prompt_once_and_keeps_immutable_tool_inputs() {
    let mut request = AgentRequest {
        prompt: "FROZEN_SOURCE\n\nHuman suffix".into(),
        prompt_inputs_projected: true,
        collaborator: Some(collaborator::BoardLinkSession {
            session_id: "model-task-once".into(),
            grant_id: "grant-once".into(),
            workspace_id: "workspace-once".into(),
        }),
        ..Default::default()
    };
    request.inputs.wired.push(ContextItem {
        node: 1,
        text: "FROZEN_SOURCE".into(),
        images: vec![],
        outputs: Default::default(),
        active: None,
        depth: None,
        slot: Some(InputSlot::Prompt),
    });
    assert_eq!(request.input_text().matches("FROZEN_SOURCE").count(), 1);
    assert_eq!(request.inputs.wired[0].text, "FROZEN_SOURCE");
    request.prompt.push_str("\n\nFROZEN_SOURCE");
    assert_eq!(
        request.input_text().matches("FROZEN_SOURCE").count(),
        2,
        "intentional authored repeats survive"
    );
    request.collaborator.as_mut().unwrap().session_id = "ordinary-board".into();
    assert_eq!(
        request.input_text().matches("FROZEN_SOURCE").count(),
        3,
        "ordinary Board serialization is retained"
    );
}
#[test]
fn pending_projection_resolves_frozen_inputs_without_changing_policy_or_repetitions() {
    for policy in [None, Some(false), Some(true)] {
        let mut request = AgentRequest {
            prompt: "STALE WORDS, submitted suffix".into(),
            prompt_inputs_projected: true,
            pending_prompt_projection: Some(PromptProjection {
                authored: "FROZEN_WORDS".into(),
                separator: ", ".into(),
                static_instructions: "Retain region annotation".into(),
            }),
            image_history: policy,
            ..Default::default()
        };
        request.inputs.wired.push(ContextItem {
            node: 1,
            text: "FROZEN_WORDS".into(),
            images: vec![],
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: Some(InputSlot::Prompt),
        });
        let encoded = serde_json::to_value(&request).unwrap();
        let mut request: AgentRequest = serde_json::from_value(encoded).unwrap();
        request.resolve_pending_prompt_projection();
        assert_eq!(
            request.prompt,
            "FROZEN_WORDS, Retain region annotation, FROZEN_WORDS"
        );
        assert_eq!(
            request
                .prompt_with_history()
                .matches("FROZEN_WORDS")
                .count(),
            2
        );
        assert_eq!(request.image_history, policy);
        assert!(!request.oneshot);
        assert!(request.pending_prompt_projection.is_none());
        request.resolve_pending_prompt_projection();
        assert_eq!(
            request.prompt,
            "FROZEN_WORDS, Retain region annotation, FROZEN_WORDS"
        );
    }
}
#[test]
fn projected_prompts_do_not_requote_wires_and_legacy_requests_keep_them() {
    let legacy: AgentRequest = serde_json::from_value(serde_json::json!({
            "id":"r", "at":0, "prompt":"Authored suffix",
            "text_source":"Exact source document",
            "inputs":{"revision":"captured","context":[],"wired":[{"node":1,"text":"LINKED_SENTINEL","images":[],"slot":"prompt"}]}
        })).unwrap();
    assert!(!legacy.prompt_inputs_projected);
    assert!(!serde_json::to_value(&legacy)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("prompt_inputs_projected"));
    assert_eq!(
        legacy
            .prompt_with_history()
            .matches("LINKED_SENTINEL")
            .count(),
        1
    );
    let mut projected = legacy.clone();
    projected.prompt = "LINKED_SENTINEL\n\nAuthored suffix".into();
    projected.prompt_inputs_projected = true;
    let roundtrip: AgentRequest =
        serde_json::from_value(serde_json::to_value(&projected).unwrap()).unwrap();
    assert!(roundtrip.prompt_inputs_projected);
    assert_eq!(
        roundtrip
            .prompt_with_history()
            .matches("LINKED_SENTINEL")
            .count(),
        1
    );
    assert_eq!(
        roundtrip
            .prompt_with_history()
            .matches("Exact source document")
            .count(),
        1
    );
    projected.prompt.push_str("\n\nLINKED_SENTINEL");
    assert_eq!(
        projected
            .prompt_with_history()
            .matches("LINKED_SENTINEL")
            .count(),
        2,
        "an intentional repetition in the authored message must survive"
    );
}
#[test]
fn explicit_text_source_is_quoted_separately_and_only_active_history_is_replayed() {
    let mut request = AgentRequest {
        prompt: "Explain this passage".into(),
        text_source: Some("A café & tea.".into()),
        ..Default::default()
    };
    let off = request.prompt_with_history();
    assert_eq!(request.prompt, "Explain this passage");
    assert!(off.contains("Source text (quoted document data):\nA café & tea."));
    assert!(!off.contains("Earlier private turn"));
    request.inputs.wired.push(ContextItem {
        node: 7,
        text: "WIRED_DATA".into(),
        slot: Some(InputSlot::Prompt),
        images: Vec::new(),
        outputs: Default::default(),
        active: None,
        depth: None,
    });
    let wired = request.prompt_with_history();
    assert_eq!(wired.matches("WIRED_DATA").count(), 1);
    assert_eq!(wired.matches("A café & tea.").count(), 1);
    assert_eq!(request.prompt, "Explain this passage");
    request.history.push(AgentTurn {
        role: "user".into(),
        text: "Earlier private turn".into(),
        at: 0,
    });
    assert!(request
        .prompt_with_history()
        .contains("Earlier private turn"));
    request.history.clear();
    request.text_source = None;
    assert_eq!(
        request.prompt_with_history(),
        "Explain this passage",
        "legacy empty-source/history payload is unchanged"
    );
}

#[test]
fn greetings_do_not_receive_file_or_dashboard_task_instructions() {
    for prompt in ["Hi!", "hello", " Good evening. ", "thanks"] {
        let request = AgentRequest {
            prompt: prompt.into(),
            ..Default::default()
        };
        let text = request.input_text();
        assert!(text.contains("answer directly once without progress updates"));
        assert!(!text.contains("return.json"));
        assert!(!text.contains("application/slate-link+json"));
        assert!(!text.contains("place.json"));
        assert_eq!(display_prompt(&text), prompt);
    }
}

#[test]
fn a_greeting_that_also_requests_file_work_retains_conditional_capabilities() {
    let request = AgentRequest {
        prompt: "Hi! Please make a dashboard.".into(),
        ..Default::default()
    };
    let text = request.input_text();
    assert!(text.contains("return.json"));
    assert!(text.contains("guidance below applies only"));
    assert_eq!(display_prompt(&text), request.prompt);
}

#[test]
fn wired_context_and_goal_turns_keep_their_task_guides_even_with_a_short_message() {
    let mut request = AgentRequest {
        prompt: "hello".into(),
        ..Default::default()
    };
    request.inputs.wired.push(ContextItem {
        node: 1,
        text: "inspect this".into(),
        images: vec![],
        outputs: Default::default(),
        active: None,
        depth: None,
        slot: None,
    });
    assert!(request.input_text().contains("return.json"));
    request.inputs.wired.clear();
    request.goal = Some(GoalTurn {
        goal: "check work".into(),
        claims: false,
        in_reply: false,
    });
    assert!(request.input_text().contains(GOAL_MARKER));
}
