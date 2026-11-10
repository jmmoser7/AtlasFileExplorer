use super::*;
use serde_json::json;

#[test]
fn only_text_shapes_are_admitted_before_provider_execution() {
    let request = AgentRequest {
        oneshot: true,
        ..Default::default()
    };
    assert!(validate_request_shape(&request, None).is_ok());
    for rejected in [AgentRequest {
        image: Some(Default::default()),
        ..request.clone()
    }] {
        assert!(validate_request_shape(&rejected, None).is_err());
    }
    assert!(
        validate_request_shape(
            &AgentRequest {
                oneshot: false,
                history: vec![AgentTurn {
                    role: "user".into(),
                    text: "public history".into(),
                    at: 0
                }],
                ..request.clone()
            },
            None
        )
        .is_ok(),
        "Memory ON remains a valid Text shape"
    );
    let mut visual = request;
    visual.inputs.wired.push(
        serde_json::from_value(json!({"node":1,"text":"caption","images":["source.png"]})).unwrap(),
    );
    assert!(
        catalog::validate(
            &[AgentModel {
                id: String::new(),
                name: "Text only".into(),
                is_default: true,
                capabilities: Some(atlas_agent::ModelCapabilities::text_only()),
                ..Default::default()
            }],
            &visual,
            true,
            false
        )
        .is_err(),
        "image understanding is not silently inferred from text support"
    );
}

#[test]
fn claude_public_text_skips_thinking_and_deduplicates_final_flush() {
    let mut stream = claude_cli::TextStream::default();
    for private in ["thinking_delta", "signature_delta"] {
        assert!(!stream.apply(&json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":private,"thinking":"private","signature":"private"}}})));
    }
    assert!(stream.apply(&json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello Ω"}}})));
    assert!(stream.apply(&json!({"type":"assistant","message":{"content":[{"type":"thinking","thinking":"private"},{"type":"redacted_thinking","data":"private"},{"type":"text","text":"Hello Ω"}]}})));
    assert_eq!(stream.text, "Hello Ω");
}

#[test]
fn cursor_public_text_ignores_reasoning_and_nontext_content() {
    assert_eq!(
        cursor_acp::public_text(
            &json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"private"}})
        ),
        None
    );
    assert_eq!(
        cursor_acp::public_text(
            &json!({"sessionUpdate":"agent_message_chunk","content":{"type":"image","data":"private"}})
        ),
        None
    );
    let update =
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"public Ω"}});
    assert_eq!(cursor_acp::public_text(&update), Some("public Ω"));
}

#[test]
fn provider_identity_keeps_legacy_sdk_registration_separate() {
    assert_eq!(provider("cursor-cli"), Some(Provider::Cursor));
    assert_eq!(
        provider("cursor"),
        None,
        "the SDK/API-key route remains separate"
    );
    assert_eq!(Provider::Cursor.id(), "cursor-cli");
    assert_eq!(provider("claude-code"), Some(Provider::ClaudeCode));
    assert_eq!(provider("arbitrary"), None);
}

#[test]
fn text_prompt_replays_only_captured_source_and_active_public_history_once() {
    let request = AgentRequest {
        prompt: "Revise this".into(),
        text_source: Some("SOURCE_Ω_731".into()),
        history: vec![AgentTurn {
            role: "assistant".into(),
            text: "HISTORY_π_294".into(),
            at: 0,
        }],
        oneshot: false,
        ..Default::default()
    };
    assert!(validate_request_shape(&request, None).is_ok());
    let prompt = request.prompt_with_history();
    assert_eq!(prompt.matches("SOURCE_Ω_731").count(), 1);
    assert_eq!(prompt.matches("HISTORY_π_294").count(), 1);
    assert_eq!(prompt.matches("Revise this").count(), 1);
    assert!(!prompt.contains(atlas_agent::ARTIFACT_MARKER));
    let off = AgentRequest {
        history: Vec::new(),
        oneshot: true,
        ..request
    }
    .prompt_with_history();
    assert!(!off.contains("HISTORY_π_294"));
    assert_eq!(off.matches("SOURCE_Ω_731").count(), 1);
}

#[test]
fn actual_subscription_payload_frames_inline_source_in_both_memory_modes() {
    for memory in [false, true] {
        let mut request = AgentRequest {
            id: "text-framing".into(),
            prompt: "SOURCE_158_Ω\n\nDescribe only the supplied source.".into(),
            prompt_inputs_projected: true,
            oneshot: !memory,
            history: if memory {
                vec![AgentTurn {
                    role: "assistant".into(),
                    text: "HISTORY_π".into(),
                    at: 0,
                }]
            } else {
                vec![]
            },
            ..Default::default()
        };
        request.inputs.wired.push(
            serde_json::from_value(
                json!({"node":158,"text":"SOURCE_158_Ω","images":[],"slot":"prompt"}),
            )
            .unwrap(),
        );
        // This constructor and envelope are the exact ones used by Client::run
        // and Cursor session/prompt, rather than a second test-only renderer.
        let payload =
            payload::TextPayload::new(&request, Provider::Cursor, Path::new("unused-receipt"));
        let wire: serde_json::Value =
            serde_json::from_str(&payload.cursor_wire("actual-session")).unwrap();
        let text = wire["params"]["prompt"][0]["text"].as_str().unwrap();
        assert!(text.contains("Do not search the workspace"));
        let quoted = text
            .split_once("Slate submitted input (JSON-quoted text):\n")
            .unwrap()
            .1;
        let decoded: String = serde_json::from_str(quoted).unwrap();
        assert_eq!(decoded, request.prompt_with_history());
        assert_eq!(decoded.matches("SOURCE_158_Ω").count(), 1);
        assert_eq!(decoded.matches("HISTORY_π").count(), usize::from(memory));
        assert_eq!(text, payload.text);
    }
}

#[test]
fn live_catalog_controls_are_validated_without_fabricated_speed_or_generation() {
    let models = catalog::claude(
        &json!({"models":[{"value":"default","resolvedModel":"resolved-current","displayName":"Current","supportsEffort":true,"supportedEffortLevels":["medium"],"supportsFastMode":true}]}),
    );
    assert_eq!(models.len(), 1);
    assert!(models[0].capabilities.unwrap().vision);
    assert!(!models[0].capabilities.unwrap().image_generation);
    assert!(!models[0]
        .parameters
        .iter()
        .any(|parameter| parameter.id == "speed"));
    let mut request = AgentRequest::default();
    request
        .model_parameters
        .insert("effort".into(), "medium".into());
    assert!(catalog::validate(&models, &request, true, true).is_ok());
    request
        .model_parameters
        .insert("effort".into(), "invented".into());
    assert!(catalog::validate(&models, &request, false, false).is_err());
    request.model_parameters.clear();
    request.model = Some("removed-model".into());
    assert!(catalog::validate(&models, &request, false, false).is_err());
    let unknown = catalog::claude(&json!({"models":[{"value":"custom","displayName":"Unknown"}]}));
    assert!(!unknown[0].capabilities.unwrap().vision);
    assert!(!unknown[0].capabilities.unwrap().model_tools);
}

#[test]
fn cursor_model_ids_are_opaque_and_agent_vision_is_not_every_models_capability() {
    let current = json!({"models":{"currentModelId":"default[]","availableModels":[{"modelId":"default[]","name":"Auto"},{"modelId":"opaque[effort=vendor-value]","name":"Concrete"}]}});
    let models = catalog::cursor(&current, true);
    assert!(models[0].capabilities.unwrap().vision);
    assert!(!models[1].capabilities.unwrap().vision);
    assert_eq!(models[1].id, "opaque[effort=vendor-value]");
    assert!(models[1].parameters.is_empty());
    assert!(models
        .iter()
        .all(|model| !model.capabilities.unwrap().image_generation));
}

#[test]
#[ignore = "explicit owned image constructor proof; no model request"]
fn actual_authorized_png_bytes_are_in_both_vendor_payloads() {
    use base64::Engine;
    let path = std::env::var("ATLAS_SUBSCRIPTION_VISION_FIXTURE")
        .expect("explicit authorized image fixture");
    let source = std::fs::read(&path).unwrap();
    let mut request = AgentRequest {
        prompt: "Describe the explicitly supplied image.".into(),
        oneshot: true,
        ..Default::default()
    };
    request.inputs.wired.push(
        serde_json::from_value(json!({"node":1370,"text":"","images":[path],"slot":"media"}))
            .unwrap(),
    );
    let mut payload =
        payload::TextPayload::new(&request, Provider::ClaudeCode, Path::new("unused-receipt"));
    payload.capture_images(&request, Some(|_| false)).unwrap();
    let claude: serde_json::Value = serde_json::from_str(&payload.claude_wire()).unwrap();
    let cursor: serde_json::Value =
        serde_json::from_str(&payload.cursor_wire("constructor-proof")).unwrap();
    let first = base64::engine::general_purpose::STANDARD
        .decode(
            claude["message"]["content"][1]["source"]["data"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
    let second = base64::engine::general_purpose::STANDARD
        .decode(cursor["params"]["prompt"][1]["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(first, source);
    assert_eq!(second, source);
    assert_eq!(
        claude["message"]["content"][1]["source"]["media_type"],
        "image/png"
    );
    assert_eq!(cursor["params"]["prompt"][1]["mimeType"], "image/png");
    let report = json!({"scope":"Actual production payload constructor; no model call","raw_bytes":source.len(),"raw_sha256":payload::hash(&source),"claude_packet_bytes":payload.claude_wire().len(),"cursor_packet_bytes":payload.cursor_wire("constructor-proof").len(),"decoded_claude_equals_source":true,"decoded_cursor_equals_source":true});
    if let Ok(path) = std::env::var("ATLAS_SUBSCRIPTION_VISION_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    eprintln!("{report}");
}

#[cfg(windows)]
#[test]
fn legitimate_image_event_above_old_limit_is_accepted_and_oversize_is_private() {
    let stop = AtomicBool::new(false);
    let mut allowed = fixed_child(r#"[Console]::Out.WriteLine('{"data":"'+('A'*10000000)+'"}')"#);
    let message = allowed
        .receive(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(20),
        )
        .unwrap();
    assert_eq!(message["data"].as_str().unwrap().len(), 10_000_000);
    allowed
        .finish(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        )
        .unwrap();
    let script = format!(
        "[Console]::Out.Write(('PRIVATE_DATA'+'A'*{})); Start-Sleep -Seconds 30",
        process::MAX_EVENT_BYTES
    );
    let mut oversized = fixed_child(&script);
    let error = oversized
        .receive(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(20),
        )
        .unwrap_err();
    assert!(error.contains("message limit"));
    assert!(!error.contains("PRIVATE_DATA"));
    drop(oversized);
    let mut malformed = fixed_child(r#"[Console]::Out.Write('{"PRIVATE_DATA":'); exit 0"#);
    let error = malformed
        .receive(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        )
        .unwrap_err();
    assert!(!error.contains("PRIVATE_DATA"));
}

#[cfg(windows)]
#[test]
fn actual_stdin_bytes_match_private_content_free_dispatch_receipt() {
    let dir = std::env::temp_dir().join(format!("slate-receipt-{}", atlas_agent::request_id()));
    let source = "SOURCE_PRIVATE_Ω";
    let mut request = AgentRequest {
        id: "../../untrusted-request-id".into(),
        prompt: format!("{source}\n\nPRIVATE_INSTRUCTION"),
        prompt_inputs_projected: true,
        oneshot: true,
        ..Default::default()
    };
    request.inputs.wired.push(
        serde_json::from_value(json!({"node":158,"text":source,"images":[],"slot":"prompt"}))
            .unwrap(),
    );
    let mut payload = payload::TextPayload::new(&request, Provider::Cursor, &dir);
    let wire = payload.cursor_wire("owned-session");
    let script = r#"[Console]::InputEncoding=[Text.UTF8Encoding]::new($false); $data=[Console]::In.ReadToEnd(); $bytes=[Text.Encoding]::UTF8.GetBytes($data); $hash=[BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes)).Replace('-',''); [Console]::Out.WriteLine('{"sha256":"'+$hash+'","bytes":'+$bytes.Length+'}')"#;
    let mut child = fixed_child(script);
    payload
        .send(&mut child, &wire, "cursor_acp_session_prompt_jsonl")
        .unwrap();
    child.close_input();
    let stop = AtomicBool::new(false);
    let observed = child
        .receive(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        )
        .unwrap();
    child
        .finish(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        )
        .unwrap();
    payload.observe_model("observed-model", "test child observation");
    payload.terminal(true);
    let paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 2);
    for path in &paths {
        assert_eq!(path.parent(), Some(dir.as_path()));
    }
    let receipt_path = paths
        .iter()
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("subscription-dispatch-")
        })
        .unwrap();
    let stored = std::fs::read_to_string(receipt_path).unwrap();
    let receipt: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(receipt["wire_sha256"], observed["sha256"]);
    assert_eq!(receipt["wire_utf8_bytes"], observed["bytes"]);
    assert_eq!(receipt["wire_sha256"], payload::hash(wire.as_bytes()));
    assert_eq!(receipt["write_status"], "stdin_written_and_flushed");
    assert_eq!(receipt["terminal_result"], "completed");
    assert_eq!(receipt["source_metadata"][0]["node"], 158);
    assert_eq!(
        receipt["source_metadata"][0]["current_prompt_occurrences"],
        1
    );
    assert_eq!(
        receipt["source_metadata"][0]["projected_input_occurrences"],
        1
    );
    assert_eq!(receipt["history_turn_count"], 0);
    assert_eq!(receipt["effective_model"], "observed-model");
    for private in [source, "PRIVATE_INSTRUCTION"] {
        assert!(!stored.contains(private));
    }
    for path in paths {
        let content = std::fs::read_to_string(&path).unwrap();
        for private in [source, "PRIVATE_INSTRUCTION"] {
            assert!(!content.contains(private));
        }
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
}

#[cfg(windows)]
fn fixed_child(script: &str) -> process::Process {
    let executable = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = Provider::ClaudeCode.command(&executable);
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    process::Process::spawn(command).unwrap()
}

#[cfg(windows)]
#[test]
fn nonzero_exit_cannot_promote_a_success_shaped_json_message() {
    let mut child =
        fixed_child(r#"[Console]::Out.WriteLine('{"type":"result","subtype":"success"}'); exit 7"#);
    let stop = AtomicBool::new(false);
    let message = child
        .receive(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        )
        .unwrap();
    assert_eq!(message["subtype"], "success");
    assert!(child
        .finish(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(10)
        )
        .is_err());
}

#[cfg(windows)]
#[test]
fn missing_completion_and_active_cancellation_are_failures() {
    let stop = AtomicBool::new(false);
    let mut empty = fixed_child("exit 0");
    assert!(empty
        .receive(
            &stop,
            std::time::Instant::now() + std::time::Duration::from_secs(10)
        )
        .is_err());
    let mut active = fixed_child("Start-Sleep -Seconds 30");
    stop.store(true, Ordering::Relaxed);
    let started = std::time::Instant::now();
    assert_eq!(
        active
            .receive(
                &stop,
                std::time::Instant::now() + std::time::Duration::from_secs(10)
            )
            .unwrap_err(),
        "Response stopped."
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    // Drop stops only this owned process tree; no background child is retained.
    drop(active);
}
