use super::*;

fn request() -> AgentRequest {
    let root = std::env::temp_dir().join("artifact-profile-tests");
    let output = root.join(".drafts/request-1/output");
    AgentRequest {
        id: "request-1".into(),
        prompt: "Make this pie chart interactive.".into(),
        artifact: Some(ArtifactRun {
            family_id: "family-1".into(),
            run_id: "request-1".into(),
            source_root: root,
            parent: None,
            staging_output: output.clone(),
        }),
        output_dir: Some(output.to_string_lossy().into_owned()),
        ..Default::default()
    }
}

#[test]
fn artifact_profile_replaces_conflicting_generic_guides() {
    let request = request();
    request.validate_artifact_request().unwrap();
    let text = request.input_text_in(Some(std::path::Path::new("link")));
    for instruction in [
        "Artifact generation profile v1",
        "self-contained offline HTML",
        "Never edit an upstream revision",
        "Identify estimated",
        "Do not install packages",
        "freeze",
        "source input",
    ] {
        assert!(
            text.to_lowercase().contains(&instruction.to_lowercase()),
            "{instruction}"
        );
    }
    assert!(!text.contains("Edit existing files where they are"));
    assert!(!text.contains(SLATE_LINK_GUIDE));
    assert!(!text.contains(FILE_ATLAS_PLACE_GUIDE));
    assert!(!text.contains("{{request_id}}"));
    assert!(text.contains("\"id\":\"request-1\""));
    assert_eq!(display_prompt(&text), request.prompt);
}

#[test]
fn artifact_profile_carries_only_wired_inputs_and_quoted_complete_history() {
    let mut request = request();
    request.inputs = serde_json::from_value(serde_json::json!({
        "revision":"private-metadata",
        "context":[{"node":1,"text":"ambient-not-allowed","images":[]}],
        "wired":[{"node":2,"text":"explicit-source-labels","images":[]}]
    }))
    .unwrap();
    request.history.push(AgentTurn {
        role: "assistant".into(),
        text: "chosen-prefix-only".into(),
        at: 0,
    });
    let text = request.input_text();
    assert!(text.contains("explicit-source-labels"));
    assert!(text.contains("chosen-prefix-only"));
    assert!(text.contains("replayed into a fresh session"));
    assert!(!text.contains("ambient-not-allowed"));
    assert!(!text.contains("private-metadata"));
}

#[test]
fn artifact_submission_cannot_write_to_a_reference_or_belong_to_another_run() {
    let mut request = request();
    request.output_dir = Some(std::env::temp_dir().to_string_lossy().into_owned());
    assert_eq!(
        request.validate_artifact_request().unwrap_err(),
        "Artifact output must be its owned staging directory."
    );
    request.output_dir = Some(
        request
            .artifact
            .as_ref()
            .unwrap()
            .staging_output
            .to_string_lossy()
            .into_owned(),
    );
    request.artifact.as_mut().unwrap().run_id = "another-request".into();
    assert_eq!(
        request.validate_artifact_request().unwrap_err(),
        "Artifact staging belongs to a different request."
    );
    request.artifact.as_mut().unwrap().run_id = request.id.clone();
    request.oneshot = true;
    assert!(request
        .validate_artifact_request()
        .unwrap_err()
        .contains("request modes"));
}

#[test]
fn paths_with_template_braces_are_preserved_as_literal_json_data() {
    let mut request = request();
    let output = request
        .artifact
        .as_mut()
        .unwrap()
        .staging_output
        .join("{{request_id}}");
    request.artifact.as_mut().unwrap().staging_output = output.clone();
    request.output_dir = Some(output.to_string_lossy().into_owned());
    let guide = artifact_generation_guide_in(&request, Some("link"));
    assert!(guide.contains(&serde_json::to_string(&output).unwrap()));
}

#[test]
fn artifact_display_accepts_lf_and_crlf_headers_without_changing_user_text() {
    let prompt = "  Preserve this\r\nuser text.\n\u{1f916}\r\n ";
    for ending in ["\n", "\r\n"] {
        let text = format!(
            "{prompt}{ARTIFACT_GENERATION_MARKER}Artifact generation profile v1{ending}Instructions."
        );
        let displayed = display_prompt(&text);
        assert_eq!(displayed, prompt);
        assert_eq!(displayed.as_ptr(), text.as_ptr());
    }
}

#[test]
fn artifact_display_retains_partial_or_lookalike_profile_headers() {
    for header in [
        "Artifact generation profile v1",
        "Artifact generation profile v1\r",
        "Artifact generation profile v1\r\r\n",
        "Artifact generation profile v10\n",
        "Artifact generation profile v1 extra\n",
        " Artifact generation profile v1\n",
        "Artifact generation profile v\n",
    ] {
        let text = format!("Keep\r\nthis.{ARTIFACT_GENERATION_MARKER}{header}");
        assert_eq!(display_prompt(&text), text);
    }
    let changed_marker =
        "Keep\r\n\r\nSlate artifact generation:\r\nArtifact generation profile v1\r\n";
    assert_eq!(display_prompt(changed_marker), changed_marker);
}
