use super::*;
#[test]
fn artifact_descriptor_cannot_be_rebound_to_model_another_run_or_another_deadline() {
    let root = std::env::temp_dir();
    let mut request = AgentRequest {
        id: "run-test".into(),
        artifact: Some(crate::ArtifactRun {
            family_id: "family-test".into(),
            run_id: "run-test".into(),
            source_root: root.clone(),
            parent: None,
            staging_output: root.join(".drafts/run-test/output"),
        }),
        output_dir: Some(
            root.join(".drafts/run-test/output")
                .to_string_lossy()
                .into_owned(),
        ),
        ..Default::default()
    };
    let launch = ArtifactToolsLaunch::artifact(root.join("slate.exe"), &root, &request).unwrap();
    assert!(launch.validate_for(&request).is_ok());
    let roundtrip: AgentRequest =
        serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
    assert!(launch.validate_for(&roundtrip).is_ok());
    let mut changed_request = request.clone();
    changed_request.prompt.push_str(" changed question");
    assert!(launch.validate_for(&changed_request).is_err());
    changed_request = request.clone();
    changed_request.inputs.wired.push(crate::ContextItem {
        node: 1,
        text: String::new(),
        images: vec![root.join("external.png").to_string_lossy().into_owned()],
        outputs: Default::default(),
        active: None,
        depth: None,
        slot: Some(crate::InputSlot::Media),
    });
    assert!(launch.validate_for(&changed_request).is_err());
    changed_request = request.clone();
    changed_request
        .model_parameters
        .insert("effort".into(), "max".into());
    assert!(launch.validate_for(&changed_request).is_err());
    assert!(launch.remaining_time().unwrap() <= Duration::from_secs(600));
    let mut too_long = launch.clone();
    too_long.deadline_unix_ms = crate::collaborator::host_tools::deadline_after(601).unwrap();
    assert!(too_long.remaining_time().is_err());
    let mut max_request = request.clone();
    max_request
        .model_parameters
        .insert("effort".into(), "max".into());
    let max = ArtifactToolsLaunch::artifact(root.join("slate.exe"), &root, &max_request).unwrap();
    assert_eq!(max.identity.budget_seconds, 1800);
    assert!(max.remaining_time().unwrap() <= Duration::from_secs(1800));
    assert!(max.remaining_time().unwrap() > Duration::from_secs(1790));
    let captured = max.expires;
    let first = max.remaining_time().unwrap();
    let second = max.remaining_time().unwrap();
    assert_eq!(max.expires, captured);
    assert!(second <= first);
    let mut forged = launch.clone();
    forged.identity.budget_seconds = 1800;
    forged.arguments[2] = serde_json::to_string(&forged.identity).unwrap();
    assert!(
        forged.validate_for(&request).is_err(),
        "a default request cannot forge the MAX budget"
    );
    let mut expired = max.clone();
    expired.expires = Instant::now() - Duration::from_secs(1);
    assert!(expired.validate_for(&max_request).is_err());
    let mut excess = max.clone();
    excess.deadline_unix_ms = crate::collaborator::host_tools::deadline_after(1801).unwrap();
    assert!(excess.remaining_time().is_err());
    let mut changed = launch.clone();
    changed.deadline_unix_ms += 1;
    assert!(changed.validate_for(&request).is_err());
    changed = launch.clone();
    changed.identity.run_id = "another-run".into();
    assert!(changed.validate_for(&request).is_err());
    request.collaborator = Some(crate::collaborator::BoardLinkSession {
        session_id: "model-task-test".into(),
        grant_id: "grant".into(),
        workspace_id: "workspace".into(),
    });
    assert!(launch.validate_for(&request).is_err());
}

#[test]
fn artifact_write_checks_both_decoded_and_actual_encoded_bounds() {
    use crate::artifact_tools::*;
    let mut call = ArtifactCall {
        id: "call-test".into(),
        identity: ArtifactIdentity {
            request_id: "run-test".into(),
            run_id: "run-test".into(),
            family_id: "family-test".into(),
            request_fingerprint: "0".repeat(64),
            budget_seconds: 600,
        },
        sequence: 0,
        operation: ArtifactOperation::WriteFile {
            path: "index.html".into(),
            content: "x".repeat(MAX_WRITE_BYTES),
        },
    };
    assert!(call.validate().is_ok());
    call.operation = ArtifactOperation::WriteFile {
        path: "index.html".into(),
        content: "x".repeat(MAX_WRITE_BYTES + 1),
    };
    assert!(call.validate().unwrap_err().contains("32 KiB"));
    call.operation = ArtifactOperation::WriteFile {
        path: "index.html".into(),
        content: "\0".repeat(MAX_WRITE_BYTES),
    };
    assert!(call.validate().unwrap_err().contains("64 KiB"));
}
