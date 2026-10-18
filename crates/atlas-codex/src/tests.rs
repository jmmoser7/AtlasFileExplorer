use super::*;

#[test]
fn stale_rollout_recovery_matches_only_the_documented_missing_rollout_error() {
    assert!(missing_rollout(
        r#"{"code":-32600,"message":"No rollout found for thread id abc"}"#
    ));
    assert!(!missing_rollout(
        r#"{"code":-32600,"message":"Invalid thread parameters"}"#
    ));
    assert!(!missing_rollout(
        r#"{"code":-32000,"message":"Rollout not found"}"#
    ));
    assert!(!missing_rollout("connection closed"));
}

#[test]
fn stale_rollout_resume_starts_a_fresh_thread_and_preserves_unrelated_errors() {
    let mut methods = Vec::new();
    let (thread, replay) = start_or_resume(Path::new("C:/project"), Some("stale"), |method, _| {
        methods.push(method.to_string());
        match method {
            "thread/resume" => {
                Err(r#"{"code":-32600,"message":"no rollout found for thread id stale"}"#.into())
            }
            "thread/start" => Ok(json!({"thread":{"id":"fresh"}})),
            _ => unreachable!(),
        }
    })
    .unwrap();
    assert_eq!(methods, ["thread/resume", "thread/start"]);
    assert_eq!(
        thread.pointer("/thread/id").and_then(Value::as_str),
        Some("fresh")
    );
    assert!(replay);

    let mut methods = Vec::new();
    let failure = start_or_resume(Path::new("C:/project"), Some("still-valid"), |method, _| {
        methods.push(method.to_string());
        Err(r#"{"code":-32600,"message":"Invalid resume parameters"}"#.into())
    });
    assert!(failure.is_err());
    assert_eq!(methods, ["thread/resume"]);
}

#[test]
fn steer_protocol_targets_the_expected_active_turn_and_only_text_input() {
    assert_eq!(
        steer_params("thread-7", "turn-9", "Keep the selected layout."),
        json!({
            "threadId":"thread-7",
            "expectedTurnId":"turn-9",
            "input":[{"type":"text","text":"Keep the selected layout."}]
        })
    );
}

#[test]
fn steer_response_uses_official_turn_id_and_rejects_another_turn() {
    assert_eq!(
        accepted_steer_turn(&json!({"turnId":"turn_456"}), "turn_456").unwrap(),
        "turn_456"
    );
    assert!(accepted_steer_turn(&json!({"turnId":"turn_other"}), "turn_456").is_err());
    assert!(accepted_steer_turn(&json!({"acceptedTurnId":"turn_456"}), "turn_456").is_ok());
}

#[test]
fn steering_feedback_is_recorded_under_the_active_request() {
    let mut session = AgentSession {
        request: "request-1".into(),
        ..Default::default()
    };
    session.record_activity(AgentActivity {
        id: "codex-steer:codex-steer-1:accepted".into(),
        request: "request-1".into(),
        kind: AgentActivityKind::Commentary,
        text: "Steering message accepted".into(),
        at: 1,
        semantics: Some(AgentActivitySemantics::new(AgentActivityAction::Chat)),
    });
    assert_eq!(session.activity.len(), 1);
    assert_eq!(session.activity[0].request, "request-1");
}

#[test]
fn history_artifacts_require_successful_structured_results() {
    let state = session_from_thread(&json!({"id":"existing","turns":[{"id":"t1","items":[
      {"type":"userMessage","content":[{"text":"Fix the file"}]},
      {"type":"fileChange","id":"bad","status":"failed","changes":[{"path":"bad.rs","kind":{"type":"update"}}]},
      {"type":"fileChange","id":"good","status":"completed","changes":[{"path":"good.rs","kind":{"type":"update"}}]},
      {"type":"commandExecution","id":"read","exitCode":0,"commandActions":[{"type":"read","path":"reference.rs"}]},
      {"type":"agentMessage","text":"Maybe also edit imaginary.rs"}
    ]}]}));
    assert_eq!(state.conversation, "existing");
    assert_eq!(state.turns.len(), 2);
    assert_eq!(state.artifacts.len(), 2);
    assert!(state.artifacts.iter().all(|a| a.turn == 1));
    assert_eq!(state.artifacts[0].kind, ArtifactKind::Modified);
    assert_eq!(state.artifacts[1].kind, ArtifactKind::Read);
}

#[test]
fn full_access_is_per_turn_and_absent_without_a_grant() {
    let asks = turn_params(
        "t",
        vec![],
        "r",
        Some("gpt"),
        TurnPolicy::Default,
        false,
        None,
    );
    assert!(asks.get("approvalPolicy").is_none());
    assert!(asks.get("sandboxPolicy").is_none());
    let full = turn_params(
        "t",
        vec![],
        "r",
        Some("gpt"),
        TurnPolicy::Default,
        true,
        None,
    );
    assert_eq!(full["approvalPolicy"], "never");
    assert_eq!(full["sandboxPolicy"]["type"], "dangerFullAccess");
    assert_eq!(full["threadId"], "t");
}

/// First hands-on use, 28 September 2026: Codex overrides persist on the
/// thread, so a turn after a read-only or full-access turn must put the
/// thread's own settings back, or it silently keeps the last override.
#[test]
fn a_turn_after_an_override_restores_the_threads_own_settings() {
    let own = Permissions::from_thread_response(&json!({
        "thread": {"id": "t"},
        "approvalPolicy": "on-request",
        "sandbox": {"type": "workspaceWrite", "networkAccess": false}
    }))
    .expect("the thread reports its settings");
    let mut turns = TurnPermissions::default();
    turns.own = Some(own.clone());
    let send = |turns: &mut TurnPermissions, policy, full| {
        let restore = turns.next(policy, full);
        turn_params("t", vec![], "r", None, policy, full, restore.as_ref())
    };
    let first = send(&mut turns, TurnPolicy::Default, false);
    assert!(
        first.get("sandboxPolicy").is_none(),
        "never overridden: nothing sent"
    );
    let review = send(&mut turns, TurnPolicy::ReadOnly, false);
    assert_eq!(review["sandboxPolicy"]["type"], "readOnly");
    let after_review = send(&mut turns, TurnPolicy::Default, false);
    assert_eq!(after_review["approvalPolicy"], "on-request");
    assert_eq!(after_review["sandboxPolicy"], own.sandbox);
    let steady = send(&mut turns, TurnPolicy::Default, false);
    assert!(steady.get("sandboxPolicy").is_none(), "restored once");
    let full = send(&mut turns, TurnPolicy::Default, true);
    assert_eq!(full["sandboxPolicy"]["type"], "dangerFullAccess");
    let withdrawn = send(&mut turns, TurnPolicy::Default, false);
    assert_eq!(
        withdrawn["approvalPolicy"], "on-request",
        "withdrawing Full access applies from the next message"
    );

    let mut unknown = TurnPermissions::default();
    unknown.next(TurnPolicy::Default, true);
    assert_eq!(
        unknown.next(TurnPolicy::Default, false),
        Some(Permissions::provider_default()),
        "a thread that did not report its settings falls back to asking"
    );
    assert_eq!(
        Permissions::from_thread_response(&json!({
            "approvalPolicy": "never",
            "sandbox": "read-only"
        }))
        .map(|p| p.sandbox),
        Some(json!({"type": "readOnly"}))
    );
    assert_eq!(
        Permissions::from_thread_response(&json!({"thread": {}})),
        None
    );
}

#[test]
fn a_read_only_turn_is_sandboxed_and_outranks_full_access() {
    for full_access in [false, true] {
        let own = Permissions::provider_default();
        let p = turn_params(
            "t",
            vec![],
            "r",
            None,
            TurnPolicy::ReadOnly,
            full_access,
            Some(&own),
        );
        assert_eq!(p["approvalPolicy"], "never");
        assert_eq!(
            p["sandboxPolicy"],
            json!({"type": "readOnly"}),
            "{full_access}"
        );
        assert_eq!(p["clientUserMessageId"], "r");
    }
}

#[test]
fn thread_configuration_preserves_provider_permissions() {
    for id in [None, Some("portal-owned")] {
        let p = thread_params(Path::new("C:/project"), id);
        assert!(p.get("sandbox").is_none());
        assert!(p.get("approvalPolicy").is_none());
        assert!(p.get("developerInstructions").is_none());
        assert_eq!(p.get("threadId").and_then(Value::as_str), id);
    }
}

#[test]
#[ignore = "spends one ChatGPT image through the installed, signed-in Codex; set CODEX_BIN"]
fn live_image_run_collects_a_chatgpt_picture() {
    let exe = std::path::PathBuf::from(std::env::var_os("CODEX_BIN").expect("CODEX_BIN"));
    let dir = std::env::temp_dir().join(format!("atlas-codex-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut client = Client::start(&exe, &dir, None).unwrap();
    client.set_image_dir(dir.join("out"));
    let request: AgentRequest = serde_json::from_value(json!({
        "id": "live-image", "prompt": "a small blue sphere on a plain white background", "at": 0,
        "image": {"count": 1, "aspect": "square"},
        "inputs": {"revision": "1", "context": [], "wired": []}
    }))
    .unwrap();
    let mut session = session_from_thread(&json!({}));
    let started = Instant::now();
    client
        .run(&request, &mut session, &AtomicBool::new(false), |_| {})
        .unwrap();
    eprintln!("{:?} in {:?}", session.status, started.elapsed());
    assert_eq!(session.status, AgentStatus::Idle);
    assert_eq!(session.bundle.images.len(), 1);
    assert!(Path::new(&session.bundle.images[0].source).is_file());
    eprintln!("{}", session.bundle.images[0].source);
}

#[test]
fn an_image_run_names_count_shape_and_roles_and_collects_finished_files() {
    let request: AgentRequest = serde_json::from_value(json!({
        "id": "run", "prompt": "a timber pavilion at dusk", "at": 0,
        "image": {"count": 3, "aspect": "wide"},
        "inputs": {"revision": "1", "context": [], "wired": [
            {"node": 1, "text": "", "images": ["hall.png"], "slot": "media"},
            {"node": 2, "text": "", "images": ["monet.jpg"], "slot": "style"}
        ]}
    }))
    .unwrap();
    let text = image_turn(&request);
    assert!(text.contains("Create 3 images"));
    assert!(text.contains("16:9"));
    assert!(text.contains("Image 2 is a style reference"));
    assert!(text.ends_with("a timber pavilion at dusk"));

    let root = std::env::temp_dir().join(format!("atlas-codex-pictures-{}", std::process::id()));
    let (watch, dest) = (root.join("thread"), root.join("out"));
    std::fs::create_dir_all(&watch).unwrap();
    std::fs::write(watch.join("old.png"), b"old").unwrap();
    let mut pictures = Pictures {
        watch: Some(watch.clone()),
        dest: dest.clone(),
        seen: picture_files(&watch).into_iter().collect(),
        sizes: BTreeMap::new(),
        found: 0,
        last_scan: None,
        settled: false,
    };
    let mut session = session_from_thread(&json!({}));
    std::fs::write(watch.join("new.png"), b"picture").unwrap();
    assert!(
        !pictures.collect(&request, &mut session).unwrap(),
        "waits one scan"
    );
    pictures.last_scan = None;
    assert!(pictures.collect(&request, &mut session).unwrap());
    assert_eq!(
        session.bundle.images.len(),
        1,
        "files from before the run stay out"
    );
    assert_eq!(session.bundle.images[0].task, "vary");
    assert!(dest.join("run-0.png").is_file());
    std::fs::write(watch.join("last.png"), b"late").unwrap();
    pictures.settle();
    assert!(pictures.collect(&request, &mut session).unwrap());
    assert_eq!(session.bundle.images.len(), 2);
    let _ = std::fs::remove_dir_all(root);
}
