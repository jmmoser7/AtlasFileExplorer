//! Separate-process protocol exercise. No source path is opened or edited.
use atlas_connector::*;
use atlas_connector_runtime::{artifact::*, host::*, transport::*};
use std::{
    error::Error,
    io,
    process::{Child, Command, Stdio},
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const WAIT: Duration = Duration::from_secs(10);
const MOCK_ID: &str = "org.slate.mock-editor";
const POSTER: &[u8] =
    include_bytes!("../../../docs/application-connectors/v0.1/fixtures/mock-poster.png");

pub fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("self-test") => self_test(),
        Some("mock") if arguments.len() == 3 => mock(
            PipeAddress::from_token(&arguments[1])?, arguments[2].parse()?),
        Some("serve") if arguments.len() == 3 => host(
            PipeAddress::from_token(&arguments[1])?, arguments[2].parse()?),
        _ => Err("Usage: connector-conformance self-test | mock TOKEN HOST_PID | serve TOKEN CONNECTOR_PID".into()),
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn self_test() -> Result<()> {
    let address = PipeAddress::fresh()?;
    // Only this disposable fixture is launched or terminated, never a user app.
    let mut child = ChildGuard(
        Command::new(std::env::current_exe()?)
            .args(["mock", address.token(), &std::process::id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    host(address, child.0.id())?;
    let until = std::time::Instant::now() + WAIT;
    loop {
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                return Err(format!("Mock exited with {status}").into());
            }
            break;
        }
        if std::time::Instant::now() >= until {
            return Err("Mock failed to detach before deadline".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("PASS: separate-process identity, host SDK negotiation and reply validation, binding, inspection, local replay, stale-source and agent-write refusal, capture bytes, view restore and preserve detach.");
    Ok(())
}

fn host(address: PipeAddress, peer_pid: u32) -> Result<()> {
    let cancel = Cancellation::default();
    let listener = PipeListener::bind(address.clone(), peer_pid)?;
    let mut channel = HostChannel::accept(
        listener,
        MOCK_ID,
        MOCK_ID,
        &HostPolicy::default(),
        address.token().into(),
        1,
        WAIT,
        &cancel,
    )?;
    let source = SourceRef {
        source_id: "fixture-model".into(),
        revision: "fixture-revision-1".into(),
        locator: SourceLocator {
            root_id: Some("fixture-root".into()),
            relative: Some("models/generated.fixture".into()),
            absolute: None,
        },
    };
    let bound = channel.call(
        "bind",
        None,
        Operation::BindSource {
            source: source.clone(),
            mode: SourceMode::Linked,
            selection_id: "human-fixture-selection".into(),
        },
        human("bind"),
        WAIT,
        &cancel,
    )?;
    require(
        matches!(bound.outcome, ReplyBody::SourceBound { .. }),
        "source bind",
    )?;
    let inspect = Operation::InvokeCommand {
        command_id: MOCK_INSPECT_COMMAND.into(),
        parameters: serde_json::json!({}),
    };
    let first = channel.call(
        "inspect",
        Some(source.scope()),
        inspect.clone(),
        human("inspect"),
        WAIT,
        &cancel,
    )?;
    require(
        matches!(&first.outcome, ReplyBody::CommandResult { .. }),
        "inspection",
    )?;
    let replay = channel.call(
        "inspect",
        Some(source.scope()),
        inspect,
        human("inspect"),
        WAIT,
        &cancel,
    )?;
    require(
        first == replay,
        "identical request replays locally without dispatch",
    )?;
    refused(
        channel.call(
            "stale",
            Some(SourceScope {
                source_id: source.source_id.clone(),
                revision: "wrong-revision".into(),
            }),
            Operation::InvokeCommand {
                command_id: MOCK_INSPECT_COMMAND.into(),
                parameters: serde_json::json!({}),
            },
            human("stale"),
            WAIT,
            &cancel,
        ),
        ErrorCode::StaleRevision,
    )?;
    let mutation = channel
        .session()
        .commands()
        .find(|command| command.effect == CommandEffect::SourceWriteBack)
        .ok_or("Mock must advertise adversarial write command for the refusal test")?
        .id
        .clone();
    refused(
        channel.call(
            "agent-write",
            Some(source.scope()),
            Operation::InvokeCommand {
                command_id: mutation,
                parameters: serde_json::json!({}),
            },
            CallerAuthority::Agent {
                agent_id: "fixture-agent".into(),
                workspace_autonomy: true,
            },
            WAIT,
            &cancel,
        ),
        ErrorCode::AuthorityDenied,
    )?;
    let schema = channel
        .session()
        .view_schemas()
        .first()
        .ok_or("Missing view schema")?
        .clone();
    let snapshot = channel.call(
        "view",
        Some(source.scope()),
        Operation::SnapshotView {
            schema: schema.clone(),
        },
        human("view"),
        WAIT,
        &cancel,
    )?;
    let view = match snapshot.outcome {
        ReplyBody::View { view } => view,
        _ => return Err("Missing view snapshot".into()),
    };
    let restored = channel.call(
        "restore",
        Some(source.scope()),
        Operation::RestoreView { view },
        human("restore"),
        WAIT,
        &cancel,
    )?;
    require(
        matches!(restored.outcome, ReplyBody::Ack),
        "view restoration",
    )?;
    let captured = channel.call(
        "capture",
        Some(source.scope()),
        Operation::Capture { schema },
        human("capture"),
        WAIT,
        &cancel,
    )?;
    let capture = match captured.outcome {
        ReplyBody::Captured { capture } => capture,
        _ => return Err("Missing captured artifact".into()),
    };
    require(
        capture.metadata.source == source.scope() && capture.metadata.view.source == source.scope(),
        "capture source/view consistency",
    )?;
    let bytes = receive_artifact(
        channel.address(),
        &capture.artifact,
        channel.peer().process_id,
        MAX_ARTIFACT_BYTES as usize,
        WAIT,
        &cancel,
    )?;
    require(bytes == POSTER, "binary capture bytes and hash")?;
    let detached = channel.call(
        "detach",
        None,
        Operation::Detach {
            policy: DetachPolicy::Preserve,
        },
        human("detach"),
        WAIT,
        &cancel,
    )?;
    require(
        matches!(
            detached.outcome,
            ReplyBody::Detached {
                preserved_unsaved: true,
                application_terminated: false
            }
        ),
        "non-destructive detach",
    )?;
    Ok(())
}

fn mock(address: PipeAddress, host_pid: u32) -> Result<()> {
    let cancel = Cancellation::default();
    let mut store = ArtifactStore::new(MAX_ARTIFACT_BYTES as usize)?;
    let artifact = store.insert("fixture-poster", "image/png", POSTER.to_vec())?;
    let mut mock = MockConnector::new(ConnectorIdentity {
        pack_id: MOCK_ID.into(),
        application_id: MOCK_ID.into(),
        application_version: "0.1.0".into(),
        connector_version: "0.1.0".into(),
    });
    mock.enable_adversarial_write_offer()?;
    mock.set_artifact(artifact)?;
    let mut connection = PipeConnection::connect(address.clone(), host_pid, WAIT, &cancel)?;
    connection.send(
        &WireFrame::Hello {
            hello: mock.hello(),
        },
        WAIT,
        &cancel,
    )?;
    match receive(&mut connection, &cancel)? {
        WireFrame::Welcome { welcome } => mock.accept_welcome(&welcome)?,
        _ => return Err("Expected host welcome".into()),
    }
    loop {
        let request = match receive(&mut connection, &cancel)? {
            WireFrame::Request { request } => request,
            _ => return Err("Only host requests are valid after negotiation".into()),
        };
        // This fixture trusts its OS-pinned host. Production HOST dispatch derives
        // authority from its own human/agent action; never from incoming JSON.
        let trusted = request.caller.clone();
        let reply = mock.handle(request, &trusted);
        let sender = match &reply.outcome {
            ReplyBody::Captured { capture } => Some(ArtifactSender::bind(
                &address,
                capture.artifact.clone(),
                store
                    .get(&capture.artifact.transfer_id)
                    .ok_or("Unknown transfer")?,
                host_pid,
            )?),
            _ => None,
        };
        let done = matches!(reply.outcome, ReplyBody::Detached { .. });
        connection.send(&WireFrame::Reply { reply }, WAIT, &cancel)?;
        if let Some(sender) = sender {
            sender.send(WAIT, &cancel)?;
        }
        if done {
            break;
        }
    }
    Ok(())
}

fn receive(connection: &mut PipeConnection, cancel: &Cancellation) -> Result<WireFrame> {
    Ok(connection
        .recv(WAIT, cancel)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "peer disconnected"))?)
}
fn human(id: &str) -> CallerAuthority {
    CallerAuthority::Human {
        action_id: id.into(),
        target_source: None,
    }
}
fn refused(result: std::result::Result<ReplyEnvelope, HostError>, code: ErrorCode) -> Result<()> {
    let error = result.expect_err("host must refuse this action before dispatch");
    require(
        matches!(error.kind(), HostErrorKind::Protocol(problem) if problem.code == code)
            && error.uncertain_receipt().is_none(),
        "host refusal before dispatch",
    )
}
fn require(condition: bool, label: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(format!("Failed conformance assertion: {label}").into())
    }
}
