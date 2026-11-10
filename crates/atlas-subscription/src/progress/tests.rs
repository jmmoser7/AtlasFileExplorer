use super::*;
use atlas_agent::{
    collaborator::{BoardLinkSession, HostToolsLaunch},
    AgentRequest,
};
fn fixture() -> (Progress, HelperLaunch, AgentSession) {
    let root = std::env::temp_dir().join(format!("slate-progress-{}", atlas_agent::request_id()));
    std::fs::create_dir_all(&root).unwrap();
    let request = AgentRequest {
        id: "progress-request".into(),
        collaborator: Some(BoardLinkSession {
            session_id: "model-task-progress".into(),
            grant_id: "grant-progress".into(),
            workspace_id: "workspace-progress".into(),
        }),
        ..Default::default()
    };
    let tools =
        HostToolsLaunch::model_task(root.join("helper.exe"), &root, &root, &request).unwrap();
    let session = AgentSession {
        request: request.id.clone(),
        ..Default::default()
    };
    (
        Progress::new(&root, &request.id),
        HelperLaunch::Model(tools),
        session,
    )
}
#[test]
fn typed_phase_tool_ack_and_result_are_public_but_private_content_is_never_recorded() {
    let (mut progress, tools, mut session) = fixture();
    let private = "PRIVATE_REASONING_OR_TOOL_PAYLOAD_DO_NOT_LOG";
    let thinking = json!({"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"thinking","thinking":private}}});
    assert!(progress.claude(&thinking, Some(&tools), &mut session));
    assert!(!progress.claude(&json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":private}}}),Some(&tools),&mut session));
    let alias = crate::host_tools::helper_tool_aliases(&tools)[0].clone();
    let use_message = json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu-progress","name":format!("mcp__slate__{alias}"),"input":{"private":private}}]}});
    assert!(progress.claude(&use_message, Some(&tools), &mut session));
    assert!(!progress.claude(&use_message, Some(&tools), &mut session));
    assert!(progress.claude(&json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu-progress","content":private,"is_error":false}]}}),Some(&tools),&mut session));
    assert!(session.turns.is_empty());
    assert_eq!(session.activity.len(), 3);
    assert_eq!(session.activity[1].kind, Kind::ToolStarted);
    assert_eq!(session.activity[2].kind, Kind::ToolFinished);
    let public = serde_json::to_string(&session).unwrap();
    let trace = std::fs::read_to_string(&progress.path).unwrap();
    assert!(trace.contains("tool_result_received"));
    assert!(!trace.contains(private) && !public.contains(private));
    assert!(!trace.contains(&format!("{:x}", Sha256::digest(private.as_bytes()))));
}
#[test]
fn metadata_and_activity_are_bounded_and_acp_thought_text_is_ignored() {
    let (mut progress, tools, mut session) = fixture();
    let update = json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"PRIVATE_COT"}});
    progress.acp(&update, Some(&tools), &mut session);
    for _ in 0..1000 {
        progress.note(
            "control_request_received",
            "unsafe id with text",
            "can_use_tool",
            Some(false),
        );
    }
    let trace = std::fs::read_to_string(&progress.path).unwrap();
    assert_eq!(trace.lines().count(), 256);
    assert!(!trace.contains("unsafe id with text") && !trace.contains("PRIVATE_COT"));
    assert_eq!(session.activity.len(), 1);
    assert!(session.activity[0].at < now());
}

#[test]
fn system_flood_is_aggregated_and_preserves_tool_terminal_evidence_without_private_fields() {
    let (mut progress, tools, mut session) = fixture();
    let private = "PRIVATE_STATUS_BODY_AUTH_PATH_OR_THOUGHT";
    for _ in 0..1000 {
        progress.claude(&json!({"type":"system","subtype":"thinking_tokens","text":private,"thinking":private,"thinking_tokens":1234}),Some(&tools),&mut session);
    }
    assert_eq!(
        progress.count, 0,
        "System heartbeat metadata does not consume ordered event budget"
    );
    progress.claude(&json!({"type":"system","subtype":"api_retry","status":"api_retry","attempt":2,"retry_delay_ms":1000,"error_status":429,"message":private}),Some(&tools),&mut session);
    progress.claude(
        &json!({"type":"result","is_error":false,"result":"DONE"}),
        Some(&tools),
        &mut session,
    );
    let trace = std::fs::read_to_string(&progress.path).unwrap();
    let summary = std::fs::read_to_string(progress.path.with_extension("summary.json")).unwrap();
    assert!(trace.contains("api_retry") && trace.contains("provider_terminal_received"));
    let summary_value: serde_json::Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(
        summary_value["public_category_counts"]["thinking_tokens"],
        1000
    );
    assert!(summary_value["latest_whitelisted_status"]
        .get("thinking_tokens")
        .is_none());
    for content in [&trace, &summary] {
        assert!(!content.contains(private));

        assert!(!content.contains(&format!("{:x}", Sha256::digest(private.as_bytes()))));
    }
}
