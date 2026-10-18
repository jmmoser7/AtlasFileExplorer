mod common;
use atlas_connector::*;
use common::*;

fn configured_session() -> Session {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.capabilities.offered.push("capture".into());
    Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1)
        .unwrap()
        .0
}

fn schema() -> ViewSchema {
    ViewSchema {
        namespace: "org.example.alpha.view".into(),
        version: 1,
    }
}

fn view() -> ViewSnapshot {
    ViewSnapshot {
        source: source().scope(),
        schema: schema().namespace,
        schema_version: 1,
        fidelity: ViewFidelity::ExactNative,
        state: serde_json::json!({"camera":"front"}),
    }
}

fn capture() -> CaptureResult {
    CaptureResult {
        metadata: CaptureMetadata {
            source: source().scope(),
            view: view(),
            captured_at: "2026-10-16T12:00:00Z".into(),
            width_px: 1,
            height_px: 1,
            consistency: CaptureConsistency::BestEffort,
        },
        // Descriptor validation alone cannot verify content hash; runtime does.
        artifact: ArtifactTransfer {
            transfer_id: "transfer-1".into(),
            byte_length: 123,
            media_type: "image/png".into(),
            sha256: "a".repeat(64),
        },
    }
}

#[test]
fn capture_and_view_provenance_must_match_admitted_source_and_revision() {
    let mut session = configured_session();
    bind(&mut session);
    begin(
        &mut session,
        request(
            "capture",
            Operation::Capture { schema: schema() },
            Some(source().scope()),
        ),
    )
    .unwrap();
    let mut result = capture();
    result.metadata.view.source.revision = "another-revision".into();
    expect_error(
        session.complete("capture", ReplyBody::Captured { capture: result }),
        ErrorCode::SourceMismatch,
    );
    let mut result = capture();
    result.metadata.source.source_id = "another-source".into();
    expect_error(
        session.complete("capture", ReplyBody::Captured { capture: result }),
        ErrorCode::SourceMismatch,
    );
    let reply = session
        .complete("capture", ReplyBody::Captured { capture: capture() })
        .unwrap();
    let ReplyBody::Captured { capture } = reply.outcome else {
        panic!("capture")
    };
    assert_eq!(capture.metadata.consistency, CaptureConsistency::BestEffort);
}

#[test]
fn unknown_schema_and_bad_transfer_bounds_are_explicit_failures() {
    let mut session = configured_session();
    bind(&mut session);
    let unsupported = ViewSchema {
        namespace: "org.unknown.view".into(),
        version: 1,
    };
    expect_error(
        begin(
            &mut session,
            request(
                "unknown",
                Operation::SnapshotView {
                    schema: unsupported,
                },
                Some(source().scope()),
            ),
        ),
        ErrorCode::UnsupportedSchema,
    );
    let mut result = capture().artifact;
    result.byte_length = MAX_ARTIFACT_BYTES + 1;
    expect_error(result.validate(), ErrorCode::TransferTooLarge);
    result.byte_length = 123;
    result.media_type = "image/png\r\nHeader:bad".into();
    expect_error(result.validate(), ErrorCode::InvalidResult);
    result.media_type = "image/png".into();
    result.sha256 = "A".repeat(64);
    expect_error(result.validate(), ErrorCode::InvalidResult);
}

#[test]
fn captures_require_valid_utc_calendar_timestamps() {
    let mut session = configured_session();
    bind(&mut session);
    begin(
        &mut session,
        request(
            "capture-time",
            Operation::Capture { schema: schema() },
            Some(source().scope()),
        ),
    )
    .unwrap();
    for timestamp in [
        "bogusZ",
        "2026-02-29T12:00:00Z",
        "2026-10-16T24:00:00Z",
        "2026-10-16T12:00:00.Z",
    ] {
        let mut result = capture();
        result.metadata.captured_at = timestamp.into();
        expect_error(
            session.complete("capture-time", ReplyBody::Captured { capture: result }),
            ErrorCode::InvalidResult,
        );
    }
    let mut result = capture();
    result.metadata.captured_at = "2026-10-16T12:00:00.123456789Z".into();
    session
        .complete("capture-time", ReplyBody::Captured { capture: result })
        .unwrap();
}

#[test]
fn late_reply_scope_cannot_be_relabelled_before_validation() {
    let mut session = configured_session();
    bind(&mut session);
    let req = request(
        "view",
        Operation::SnapshotView { schema: schema() },
        Some(source().scope()),
    );
    begin(&mut session, req.clone()).unwrap();
    let reply = ReplyEnvelope {
        session_id: req.session_id,
        generation: 1,
        request_id: req.request_id,
        source: Some(SourceScope {
            source_id: "other-source".into(),
            revision: "rev-a".into(),
        }),
        outcome: ReplyBody::View { view: view() },
    };
    expect_error(session.accept_reply(reply), ErrorCode::SourceMismatch);
}
