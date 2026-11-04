use super::*;

#[test]
fn reconnect_requires_source_loss_not_an_ordinary_command_denial() {
    assert!(source_lost(ErrorCode::SourceNotBound));
    assert!(source_lost(ErrorCode::Detached));
    assert!(!source_lost(ErrorCode::AuthorityDenied));
    assert!(!source_lost(ErrorCode::UnsupportedOperation));
    assert!(!source_lost(ErrorCode::DirtyDocument));
}

#[test]
fn cold_binding_has_its_own_bound_without_extending_commands_or_capture() {
    let bind = Operation::BindSource {
        source: SourceRef {
            source_id: "selected-model".into(),
            revision: "selected-revision".into(),
            locator: SourceLocator {
                root_id: None,
                relative: None,
                absolute: Some("model.3dm".into()),
            },
        },
        mode: SourceMode::Linked,
        selection_id: "explicit-activation".into(),
    };
    assert_eq!(request_deadline(&bind), Duration::from_secs(180));
    assert_eq!(
        request_deadline(&Operation::InvokeCommand {
            command_id: "inspect".into(),
            parameters: serde_json::json!({})
        }),
        Duration::from_secs(20)
    );
    assert_eq!(
        request_deadline(&Operation::Capture {
            schema: ViewSchema {
                namespace: SCHEMA.into(),
                version: 1
            }
        }),
        Duration::from_secs(20)
    );
    assert_eq!(
        request_deadline(&Operation::Detach {
            policy: DetachPolicy::Preserve
        }),
        Duration::from_secs(20)
    );
}

#[test]
fn native_preview_refresh_is_reserved_for_view_or_draft_effects() {
    use super::super::native::NativeOperation;
    let operation = NativeOperation::Invoke {
        command: "org.slate.rhino.test".into(),
        parameters: serde_json::json!({}),
    };
    for (effect, expected) in [
        (CommandEffect::Inspect, false),
        (CommandEffect::SourceWriteBack, false),
        (CommandEffect::DraftMutation, true),
        (CommandEffect::EphemeralView, true),
    ] {
        let descriptor = CommandDescriptor {
            id: "org.slate.rhino.test".into(),
            title: "test".into(),
            effect,
            undo: UndoOwner::NativeApplication,
            cancellable: false,
            agent_allowed: true,
            parameters: serde_json::json!({"type":"object"}),
        };
        assert_eq!(
            native::refreshes_preview(&operation, std::iter::once(&descriptor)),
            expected
        );
        assert!(!native::refreshes_preview(
            &NativeOperation::Inspect,
            std::iter::once(&descriptor)
        ));
    }
}
