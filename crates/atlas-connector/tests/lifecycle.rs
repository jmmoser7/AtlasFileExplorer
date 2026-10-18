mod common;
use atlas_connector::*;
use common::*;

#[test]
fn cancellation_requests_stop_without_claiming_rollback() {
    let mut session = session();
    bind(&mut session);
    begin(&mut session, inspect("work")).unwrap();
    let cancel = request(
        "cancel",
        Operation::Cancel {
            request_id: "work".into(),
        },
        None,
    );
    begin(&mut session, cancel).unwrap();
    expect_error(
        session.complete(
            "cancel",
            ReplyBody::CancellationRequested {
                target_request_id: "work".into(),
                rollback: true,
            },
        ),
        ErrorCode::InvalidResult,
    );
    session
        .complete(
            "cancel",
            ReplyBody::CancellationRequested {
                target_request_id: "work".into(),
                rollback: false,
            },
        )
        .unwrap();
    assert_eq!(
        session.state("work"),
        Some(OperationState::CancellationRequested)
    );
    // Work may already have committed when cancellation arrives.
    session
        .complete(
            "work",
            ReplyBody::CommandResult {
                value: serde_json::json!({"finished":true}),
                revision: None,
            },
        )
        .unwrap();
    expect_error(
        begin(
            &mut session,
            request(
                "late-cancel",
                Operation::Cancel {
                    request_id: "work".into(),
                },
                None,
            ),
        ),
        ErrorCode::CancellationUnsupported,
    );
}

#[test]
fn dirty_detach_preserves_application_and_never_implicitly_saves_or_discards() {
    let mut session = session();
    bind(&mut session);
    session
        .observe_source_state(&source().scope(), true)
        .unwrap();
    expect_error(
        begin(
            &mut session,
            request(
                "close",
                Operation::Detach {
                    policy: DetachPolicy::CloseIfClean,
                },
                None,
            ),
        ),
        ErrorCode::DirtyDocument,
    );
    begin(
        &mut session,
        request(
            "detach",
            Operation::Detach {
                policy: DetachPolicy::Preserve,
            },
            None,
        ),
    )
    .unwrap();
    expect_error(
        session.complete(
            "detach",
            ReplyBody::Detached {
                preserved_unsaved: true,
                application_terminated: true,
            },
        ),
        ErrorCode::InvalidResult,
    );
    session
        .complete(
            "detach",
            ReplyBody::Detached {
                preserved_unsaved: true,
                application_terminated: false,
            },
        )
        .unwrap();
    assert!(session.binding().unwrap().dirty);
    assert!(session.surface().is_none());
    expect_error(
        begin(
            &mut session,
            request("new", Operation::DescribeCommands, None),
        ),
        ErrorCode::Detached,
    );
}

fn surface_request(id: &str, action: SurfaceAction) -> RequestEnvelope {
    request(id, Operation::Surface { action }, Some(source().scope()))
}

#[test]
fn hidden_surfaces_cannot_capture_input_and_geometry_has_explicit_dpi() {
    let mut session = session();
    bind(&mut session);
    let descriptor = SurfaceDescriptor {
        surface_id: "surface-1".into(),
        mode: SurfaceMode::Headless,
        process_ownership: ProcessOwnership::AttachedExisting,
        native_handle: None,
        can_detach: true,
    };
    begin(
        &mut session,
        surface_request(
            "attach",
            SurfaceAction::Attach {
                surface: descriptor,
            },
        ),
    )
    .unwrap();
    session.complete("attach", ReplyBody::Ack).unwrap();
    expect_error(
        begin(
            &mut session,
            surface_request("hidden-focus", SurfaceAction::Focus),
        ),
        ErrorCode::AuthorityDenied,
    );
    begin(&mut session, surface_request("show", SurfaceAction::Show)).unwrap();
    session.complete("show", ReplyBody::Ack).unwrap();
    begin(&mut session, surface_request("focus", SurfaceAction::Focus)).unwrap();
    session.complete("focus", ReplyBody::Ack).unwrap();
    assert!(session.focused());
    begin(&mut session, surface_request("hide", SurfaceAction::Hide)).unwrap();
    session.complete("hide", ReplyBody::Ack).unwrap();
    assert!(!session.focused());
    assert!(!session.visible());
    let invalid = SurfaceGeometry {
        x_px: -100,
        y_px: 20,
        width_px: 800,
        height_px: 600,
        dpi: 0,
    };
    expect_error(
        begin(
            &mut session,
            surface_request("geometry", SurfaceAction::Place { geometry: invalid }),
        ),
        ErrorCode::InvalidRequest,
    );
}

#[test]
fn lifecycle_transitions_are_serialized_so_delayed_focus_cannot_override_hide() {
    let mut session = session();
    bind(&mut session);
    let descriptor = SurfaceDescriptor {
        surface_id: "surface-1".into(),
        mode: SurfaceMode::Headless,
        process_ownership: ProcessOwnership::AttachedExisting,
        native_handle: None,
        can_detach: true,
    };
    begin(
        &mut session,
        surface_request(
            "attach",
            SurfaceAction::Attach {
                surface: descriptor,
            },
        ),
    )
    .unwrap();
    expect_error(
        begin(
            &mut session,
            surface_request(
                "second-attach",
                SurfaceAction::Attach {
                    surface: SurfaceDescriptor {
                        surface_id: "surface-2".into(),
                        mode: SurfaceMode::Headless,
                        process_ownership: ProcessOwnership::AttachedExisting,
                        native_handle: None,
                        can_detach: true,
                    },
                },
            ),
        ),
        ErrorCode::RequestPending,
    );
    session.complete("attach", ReplyBody::Ack).unwrap();
    begin(&mut session, surface_request("show", SurfaceAction::Show)).unwrap();
    session.complete("show", ReplyBody::Ack).unwrap();
    begin(&mut session, surface_request("focus", SurfaceAction::Focus)).unwrap();
    expect_error(
        begin(&mut session, surface_request("hide", SurfaceAction::Hide)),
        ErrorCode::RequestPending,
    );
    session.complete("focus", ReplyBody::Ack).unwrap();
    begin(&mut session, surface_request("hide", SurfaceAction::Hide)).unwrap();
    session.complete("hide", ReplyBody::Ack).unwrap();
    assert!(!session.focused());
}

#[test]
fn relative_locators_cannot_escape_roots_and_rebinding_waits_for_inflight_work() {
    let mut session = session();
    let mut escaping = source();
    escaping.locator.relative = Some("../secret.dat".into());
    expect_error(
        begin(
            &mut session,
            request(
                "bad",
                Operation::BindSource {
                    source: escaping,
                    mode: SourceMode::Linked,
                    selection_id: "selection".into(),
                },
                None,
            ),
        ),
        ErrorCode::InvalidRequest,
    );
    bind(&mut session);
    begin(&mut session, inspect("work")).unwrap();
    expect_error(
        begin(
            &mut session,
            request(
                "rebind",
                Operation::BindSource {
                    source: source(),
                    mode: SourceMode::Linked,
                    selection_id: "new-selection".into(),
                },
                None,
            ),
        ),
        ErrorCode::RequestPending,
    );
}

#[test]
fn source_work_and_binding_changes_are_symmetric_barriers() {
    let mut session = session();
    bind(&mut session);
    begin(&mut session, inspect("pending-inspect")).unwrap();
    expect_error(
        begin(
            &mut session,
            request(
                "unbind-early",
                Operation::UnbindSource,
                Some(source().scope()),
            ),
        ),
        ErrorCode::RequestPending,
    );
    expect_error(
        begin(
            &mut session,
            request(
                "detach-early",
                Operation::Detach {
                    policy: DetachPolicy::Preserve,
                },
                None,
            ),
        ),
        ErrorCode::RequestPending,
    );
    session
        .complete(
            "pending-inspect",
            ReplyBody::CommandResult {
                value: serde_json::json!({}),
                revision: None,
            },
        )
        .unwrap();
    begin(
        &mut session,
        request("unbind", Operation::UnbindSource, Some(source().scope())),
    )
    .unwrap();
    expect_error(
        begin(&mut session, inspect("inspect-during-unbind")),
        ErrorCode::RequestPending,
    );
    session.complete("unbind", ReplyBody::Ack).unwrap();
    assert!(session.binding().is_none());
}

#[test]
fn rebind_and_detach_block_new_work_until_their_completion() {
    let mut session = session();
    bind(&mut session);
    begin(
        &mut session,
        request(
            "rebind",
            Operation::BindSource {
                source: source(),
                mode: SourceMode::Linked,
                selection_id: "new-selection".into(),
            },
            None,
        ),
    )
    .unwrap();
    expect_error(
        begin(&mut session, inspect("during-rebind")),
        ErrorCode::RequestPending,
    );
    session
        .complete(
            "rebind",
            ReplyBody::SourceBound {
                binding: SourceBinding {
                    source: source(),
                    document_id: "replacement-doc".into(),
                    mode: SourceMode::Linked,
                    dirty: false,
                },
            },
        )
        .unwrap();
    begin(
        &mut session,
        request(
            "detach",
            Operation::Detach {
                policy: DetachPolicy::Preserve,
            },
            None,
        ),
    )
    .unwrap();
    expect_error(
        begin(&mut session, inspect("during-detach")),
        ErrorCode::RequestPending,
    );
    session
        .complete(
            "detach",
            ReplyBody::Detached {
                preserved_unsaved: true,
                application_terminated: false,
            },
        )
        .unwrap();
    assert_eq!(session.binding().unwrap().document_id, "replacement-doc");
}

#[test]
fn pending_native_edit_cannot_lose_receipt_to_unbind() {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.commands[0].effect = CommandEffect::SourceWriteBack;
    hello.commands[0].undo = UndoOwner::NativeApplication;
    let (mut session, _) =
        Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
    bind(&mut session);
    let mut req = inspect("native-edit");
    req.caller = CallerAuthority::Human {
        action_id: "native-edit".into(),
        target_source: Some("source-1".into()),
    };
    begin(&mut session, req).unwrap();
    expect_error(
        begin(
            &mut session,
            request(
                "unbind-before-edit-result",
                Operation::UnbindSource,
                Some(source().scope()),
            ),
        ),
        ErrorCode::RequestPending,
    );
    session
        .complete(
            "native-edit",
            ReplyBody::CommandResult {
                value: serde_json::json!({"native_undo":"receipt-1"}),
                revision: Some("rev-b".into()),
            },
        )
        .unwrap();
    let scope = session.binding().unwrap().source.scope();
    begin(
        &mut session,
        request(
            "unbind-after-edit-result",
            Operation::UnbindSource,
            Some(scope),
        ),
    )
    .unwrap();
    session
        .complete("unbind-after-edit-result", ReplyBody::Ack)
        .unwrap();
    assert!(session.binding().is_none());
    assert_eq!(
        session.state("native-edit"),
        Some(OperationState::Completed)
    );
}
