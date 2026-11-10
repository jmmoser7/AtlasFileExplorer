use super::*;
use atlas_agent::collaborator::BoardOperation;
use base64::Engine as _;
use sha2::{Digest, Sha256};

#[test]
fn helper_delivery_receipt_distinguishes_output_prepared_from_flushed() {
    let dir = std::env::temp_dir().join(format!("slate-delivery-{}", atlas_agent::request_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let call = atlas_agent::collaborator::BoardCall {
        id: "call-delivery".into(),
        session: BoardLinkSession {
            session_id: "model-task-delivery".into(),
            grant_id: "grant-delivery".into(),
            workspace_id: "workspace-delivery".into(),
        },
        request_id: "request-delivery".into(),
        sequence: 0,
        operation: BoardOperation::Read {
            node_ids: vec![],
            offset: 0,
            limit: 32,
        },
    };
    let result = json!({"content":[{"type":"text","text":"PRIVATE_SOURCE_VALUE"}],"isError":false});
    let mut audit = delivery::Audit::new(&dir);
    let mut output = Vec::new();
    audit
        .send(
            &mut output,
            &dir,
            |_| false,
            &call,
            json!(9),
            result.clone(),
            None,
        )
        .unwrap();
    let lines = std::fs::read_to_string(dir.join("helper-delivery.jsonl")).unwrap();
    assert!(!lines.contains("PRIVATE_SOURCE_VALUE"));
    let events = lines
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events[0]["phase"], "output_prepared");
    assert_eq!(events[1]["phase"], "stdout_written_and_flushed");
    let payload = serde_json::to_vec(&result).unwrap();
    assert_eq!(
        events[1]["result_sha256"],
        format!("{:x}", Sha256::digest(&payload))
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["result"],
        result
    );
    struct BrokenFlush;
    impl std::io::Write for BrokenFlush {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("fixture flush failure"))
        }
    }
    assert!(audit
        .send(
            &mut BrokenFlush,
            &dir,
            |_| false,
            &call,
            json!(10),
            result,
            None
        )
        .is_err());
    let events = std::fs::read_to_string(dir.join("helper-delivery.jsonl")).unwrap();
    assert!(events.contains("stdout_write_failed"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn task_aliases_are_bounded_exact_and_never_accept_a_stale_or_canonical_name() {
    let session = BoardLinkSession {
        session_id: "model-task-alias".into(),
        grant_id: "grant-alias".into(),
        workspace_id: "workspace-alias".into(),
    };
    let names = tool_aliases(&session, "request-one");
    assert!(names.iter().all(
        |name| name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    ));
    assert_ne!(names, tool_aliases(&session, "request-two"));
    let mut changed = session.clone();
    changed.grant_id = "grant-other".into();
    assert_ne!(names, tool_aliases(&changed, "request-one"));
    assert!(matches!(
        parse_task_operation(&names[0], &json!({}), &session, "request-one").unwrap(),
        BoardOperation::Read { .. }
    ));
    for name in [
        "slate_board_read".to_owned(),
        "slate_board_command".into(),
        "slate_participants_read".into(),
        format!("ambient.{}", names[0]),
        tool_aliases(&session, "request-two")[0].clone(),
    ] {
        assert!(parse_task_operation(&name, &json!({}), &session, "request-one").is_err());
    }
    let schemas = request_tool_schemas(&session, "request-one");
    assert_eq!(schemas[0]["name"], names[0]);
    assert_eq!(schemas[1]["name"], names[1]);
}

#[test]
fn mcp_capture_image_contains_the_exact_validated_png_bytes() {
    let workspace =
        std::env::temp_dir().join(format!("slate-mcp-image-{}", atlas_agent::request_id()));
    std::fs::create_dir_all(workspace.join("portal-captures")).unwrap();
    let png=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPQqDjxHwAEnAJobhvlygAAAABJRU5ErkJggg==").unwrap();
    let path = workspace.join("portal-captures/capture.png");
    std::fs::write(&path, &png).unwrap();
    let operation = BoardOperation::Invoke {
        command: "board.model.connector.capture".into(),
        targets: vec!["1".into()],
        arguments: json!({}),
    };
    let content = reply_content(
        &workspace,
        |_| false,
        &operation,
        true,
        &json!({"image_source":path}),
    )
    .unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    let actual = base64::engine::general_purpose::STANDARD
        .decode(content[1]["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(Sha256::digest(&actual), Sha256::digest(&png));
    assert_eq!(actual, png);
    assert!(reply_content(
        &workspace,
        |_| true,
        &operation,
        true,
        &json!({"image_source":path})
    )
    .is_err());
    let outside = workspace.join("outside.png");
    std::fs::write(&outside, &png).unwrap();
    assert!(reply_content(
        &workspace,
        |_| false,
        &operation,
        true,
        &json!({"image_source":outside})
    )
    .is_err());
    assert_eq!(
        reply_content(&workspace, |_| false, &operation, false, &json!({}))
            .unwrap()
            .len(),
        1
    );
    std::fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn mcp_native_inspection_never_turns_an_arbitrary_path_into_image_content() {
    let operation = BoardOperation::Invoke {
        command: "board.model.connector.inspect".into(),
        targets: vec!["1".into()],
        arguments: json!({}),
    };
    let content = reply_content(
        Path::new("."),
        |_| true,
        &operation,
        true,
        &json!({"image_source":"private.png"}),
    )
    .unwrap();
    assert_eq!(content.len(), 1);
}

#[test]
fn mcp_image_encoding_enforces_the_provider_limit_without_a_filename_fallback() {
    assert!(mcp_image(&vec![0; 7_500_000]).is_ok());
    assert!(mcp_image(&vec![0; 7_500_001])
        .unwrap_err()
        .contains("smaller view"));
}

#[test]
fn artifact_aliases_have_no_model_operations_and_readiness_requires_listing_and_same_deadline() {
    use atlas_agent::{
        helper::{ArtifactToolsLaunch, HelperLaunch},
        AgentRequest, ArtifactRun,
    };
    let root = std::env::temp_dir().join(format!("artifact-ready-{}", atlas_agent::request_id()));
    std::fs::create_dir_all(&root).unwrap();
    let run = ArtifactRun {
        family_id: "family".into(),
        run_id: "run".into(),
        source_root: root.clone(),
        parent: None,
        staging_output: root.join(".drafts/run/output"),
    };
    let request = AgentRequest {
        id: run.run_id.clone(),
        output_dir: Some(run.staging_output.to_string_lossy().into_owned()),
        artifact: Some(run),
        ..Default::default()
    };
    let launch = ArtifactToolsLaunch::artifact(root.join("slate.exe"), &root, &request).unwrap();
    let dir = launch.transport_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("artifact-session.json"),
        serde_json::to_vec(&launch.identity).unwrap(),
    )
    .unwrap();
    let route = route::Route::Artifact(launch.identity.clone());
    let helper = HelperLaunch::Artifact(launch.clone());
    let names = helper_tool_aliases(&helper);
    assert_eq!(names.len(), 3);
    assert!(names.iter().all(|name| name.len() <= 64));
    assert!(artifact::parse(&names[0], &json!({}), &launch.identity).is_ok());
    for name in [
        "slate_board_command",
        "artifact_write_file",
        "slate_board_read",
    ] {
        assert!(artifact::parse(name, &json!({}), &launch.identity).is_err());
    }
    assert!(artifact::parse(
        &names[2],
        &json!({"path":"index.html","content":"ok","extra":true}),
        &launch.identity
    )
    .is_err());
    write_ready(
        &dir.join("helper-ready.json"),
        &route,
        launch.deadline_unix_ms,
        false,
    )
    .unwrap();
    assert!(helper_initialized(&helper));
    assert!(!helper_ready(&helper));
    write_ready(
        &dir.join("helper-ready.json"),
        &route,
        launch.deadline_unix_ms,
        true,
    )
    .unwrap();
    assert!(helper_ready(&helper));
    write_ready(
        &dir.join("helper-ready.json"),
        &route,
        launch.deadline_unix_ms + 1,
        true,
    )
    .unwrap();
    assert!(!helper_ready(&helper));
    write_ready(
        &dir.join("helper-ready.json"),
        &route,
        launch.deadline_unix_ms,
        true,
    )
    .unwrap();
    let mut changed_payload = launch.identity.clone();
    changed_payload.request_fingerprint = "0".repeat(64);
    std::fs::write(
        dir.join("artifact-session.json"),
        serde_json::to_vec(&changed_payload).unwrap(),
    )
    .unwrap();
    assert!(
        !helper_initialized(&helper),
        "same IDs with a different prepared payload are not a live helper binding"
    );
    let mut wrong = launch.identity.clone();
    wrong.run_id = "another-run".into();
    std::fs::write(
        dir.join("artifact-session.json"),
        serde_json::to_vec(&wrong).unwrap(),
    )
    .unwrap();
    assert!(!helper_initialized(&helper));
    let mut max_request = request.clone();
    max_request
        .model_parameters
        .insert("effort".into(), "max".into());
    let max_launch =
        ArtifactToolsLaunch::artifact(root.join("slate.exe"), &root, &max_request).unwrap();
    assert_eq!(max_launch.identity.budget_seconds, 1800);
    std::fs::write(
        dir.join("artifact-session.json"),
        serde_json::to_vec(&max_launch.identity).unwrap(),
    )
    .unwrap();
    write_ready(
        &dir.join("helper-ready.json"),
        &route::Route::Artifact(max_launch.identity.clone()),
        max_launch.deadline_unix_ms,
        true,
    )
    .unwrap();
    assert!(helper_ready(&HelperLaunch::Artifact(max_launch)));
    assert!(
        !helper_initialized(&helper),
        "a default request cannot borrow a MAX request's live budget"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn artifact_image_reply_emits_the_exact_frozen_png_only_when_its_parent_hash_matches() {
    use atlas_agent::artifact_tools::{ArtifactFileReply, ArtifactImage};
    let root = std::env::temp_dir().join(format!("artifact-png-{}", atlas_agent::request_id()));
    std::fs::create_dir(&root).unwrap();
    let png=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPQqDjxHwAEnAJobhvlygAAAABJRU5ErkJggg==").unwrap();
    let path = root.join("frozen.png");
    std::fs::write(&path, &png).unwrap();
    let mut reply = ArtifactFileReply {
        result: json!({"path":path}),
        image: Some(ArtifactImage {
            path: path.clone(),
            sha256: format!("{:x}", Sha256::digest(&png)),
        }),
    };
    let content = artifact::content(&reply, true, |_| false).unwrap();
    assert_eq!(content[1]["type"], "image");
    let actual = base64::engine::general_purpose::STANDARD
        .decode(content[1]["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(actual, png);
    reply.image.as_mut().unwrap().sha256 = "0".repeat(64);
    assert!(artifact::content(&reply, true, |_| false)
        .unwrap_err()
        .contains("changed"));
    assert_eq!(
        artifact::content(&reply, false, |_| false).unwrap().len(),
        1
    );
    assert!(artifact::content(&reply, true, |_| true).is_err());
    // The existing Model reader still rejects this arbitrary frozen directory.
    assert!(
        atlas_agent::collaborator::images::read_workspace_png(&root, &path, Some(|_| false))
            .is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
}
