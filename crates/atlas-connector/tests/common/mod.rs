#![allow(dead_code)]

use atlas_connector::*;

pub fn identity(vendor: &str) -> ConnectorIdentity {
    ConnectorIdentity {
        pack_id: format!("org.example.{vendor}"),
        application_id: format!("org.example.{vendor}"),
        application_version: "1.0".into(),
        connector_version: "0.1".into(),
    }
}

pub fn source() -> SourceRef {
    SourceRef {
        source_id: "source-1".into(),
        revision: "rev-a".into(),
        locator: SourceLocator {
            root_id: Some("root-1".into()),
            relative: Some("models/example.dat".into()),
            absolute: Some("C:/models/example.dat".into()),
        },
    }
}

pub fn session() -> Session {
    let mock = MockConnector::new(identity("alpha"));
    Session::negotiate(
        &HostPolicy::default(),
        &mock.hello(),
        "test-session".into(),
        1,
    )
    .unwrap()
    .0
}

pub fn request(id: &str, operation: Operation, scope: Option<SourceScope>) -> RequestEnvelope {
    RequestEnvelope {
        session_id: "test-session".into(),
        generation: 1,
        request_id: id.into(),
        caller: CallerAuthority::Human {
            action_id: id.into(),
            target_source: None,
        },
        source: scope,
        operation,
    }
}

pub fn begin(session: &mut Session, request: RequestEnvelope) -> Result<Begin, ProtocolError> {
    let authority = request.caller.clone();
    session.begin(request, &authority)
}

pub fn bind(session: &mut Session) {
    let source = source();
    begin(
        session,
        request(
            "bind",
            Operation::BindSource {
                source: source.clone(),
                mode: SourceMode::Linked,
                selection_id: "selection-1".into(),
            },
            None,
        ),
    )
    .unwrap();
    session
        .complete(
            "bind",
            ReplyBody::SourceBound {
                binding: SourceBinding {
                    source,
                    document_id: "native-doc".into(),
                    mode: SourceMode::Linked,
                    dirty: false,
                },
            },
        )
        .unwrap();
}

pub fn inspect(id: &str) -> RequestEnvelope {
    request(
        id,
        Operation::InvokeCommand {
            command_id: MOCK_INSPECT_COMMAND.into(),
            parameters: serde_json::json!({}),
        },
        Some(source().scope()),
    )
}

pub fn expect_error<T: std::fmt::Debug>(result: Result<T, ProtocolError>, code: ErrorCode) {
    assert_eq!(result.unwrap_err().code, code);
}
