use std::collections::BTreeMap;

use schemars::{schema_for, Schema};
use serde_json::Value;

use crate::*;

pub fn protocol_schema() -> Schema {
    schema_for!(WireFrame)
}

/// Generated from the public Rust models, never a second handwritten contract.
pub fn schema_documents() -> BTreeMap<String, Value> {
    [
        ("wire-frame", schema_for!(WireFrame)),
        ("connector-descriptor", schema_for!(ConnectorDescriptor)),
        ("hello", schema_for!(Hello)),
        ("welcome", schema_for!(Welcome)),
        ("request", schema_for!(RequestEnvelope)),
        ("reply", schema_for!(ReplyEnvelope)),
        ("view-snapshot", schema_for!(ViewSnapshot)),
        ("capture-metadata", schema_for!(CaptureMetadata)),
        ("artifact-transfer", schema_for!(ArtifactTransfer)),
        ("artifact-completion", schema_for!(ArtifactCompletion)),
        ("artifact-receipt", schema_for!(ArtifactReceipt)),
    ]
    .into_iter()
    .map(|(name, schema)| {
        (
            format!("{name}.schema.json"),
            serde_json::to_value(schema).expect("schema is serializable"),
        )
    })
    .collect()
}

/// Binary-pipe profile fixtures. The transfer describes the five ASCII bytes
/// `hello`; its digest is SHA-256 of those actual bytes, not a placeholder.
pub fn artifact_fixture_documents() -> BTreeMap<String, Value> {
    let transfer = ArtifactTransfer {
        transfer_id: "fixture-hello".into(),
        byte_length: 5,
        media_type: "application/octet-stream".into(),
        sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
    };
    [
        (
            "artifact-transfer.json".into(),
            serde_json::to_value(&transfer).expect("serializable transfer"),
        ),
        (
            "artifact-completion.json".into(),
            serde_json::to_value(ArtifactCompletion::new(&transfer))
                .expect("serializable completion"),
        ),
        (
            "artifact-receipt.json".into(),
            serde_json::to_value(ArtifactReceipt::new(&transfer)).expect("serializable receipt"),
        ),
    ]
    .into_iter()
    .collect()
}

/// Wire fixtures are built by the same adapter and admission policy that the
/// public harness exercises. Both identities follow exactly the same path.
pub fn fixture_frames() -> BTreeMap<String, WireFrame> {
    let mut fixtures = BTreeMap::new();
    for vendor in ["alpha", "beta"] {
        let mut mock = MockConnector::new(ConnectorIdentity {
            pack_id: format!("org.example.{vendor}"),
            application_id: format!("org.example.{vendor}"),
            application_version: "1.0".into(),
            connector_version: "0.1".into(),
        });
        fixtures.insert(
            format!("{vendor}-hello.json"),
            WireFrame::Hello {
                hello: mock.hello(),
            },
        );
        let welcome = mock
            .negotiate(&HostPolicy::default(), format!("fixture-{vendor}"), 1)
            .expect("valid mock");
        fixtures.insert(
            format!("{vendor}-welcome.json"),
            WireFrame::Welcome {
                welcome: welcome.clone(),
            },
        );
        let source = SourceRef {
            source_id: "example-source".into(),
            revision: "revision-1".into(),
            locator: SourceLocator {
                root_id: Some("example-root".into()),
                relative: Some("models/example.dat".into()),
                absolute: Some("C:/example/models/example.dat".into()),
            },
        };
        let caller = CallerAuthority::Human {
            action_id: "bind-1".into(),
            target_source: None,
        };
        let request = RequestEnvelope {
            session_id: welcome.session_id.clone(),
            generation: 1,
            request_id: "bind-1".into(),
            caller: caller.clone(),
            source: None,
            operation: Operation::BindSource {
                source: source.clone(),
                mode: SourceMode::Linked,
                selection_id: "human-selection-1".into(),
            },
        };
        fixtures.insert(
            format!("{vendor}-bind-request.json"),
            WireFrame::Request {
                request: request.clone(),
            },
        );
        fixtures.insert(
            format!("{vendor}-bind-reply.json"),
            WireFrame::Reply {
                reply: mock.handle(request, &caller),
            },
        );
        let caller = CallerAuthority::Agent {
            agent_id: "example-agent".into(),
            workspace_autonomy: false,
        };
        let request = RequestEnvelope {
            session_id: welcome.session_id,
            generation: 1,
            request_id: "inspect-1".into(),
            caller: caller.clone(),
            source: Some(source.scope()),
            operation: Operation::InvokeCommand {
                command_id: MOCK_INSPECT_COMMAND.into(),
                parameters: serde_json::json!({}),
            },
        };
        fixtures.insert(
            format!("{vendor}-inspect-request.json"),
            WireFrame::Request {
                request: request.clone(),
            },
        );
        fixtures.insert(
            format!("{vendor}-inspect-reply.json"),
            WireFrame::Reply {
                reply: mock.handle(request, &caller),
            },
        );
    }
    fixtures
}
