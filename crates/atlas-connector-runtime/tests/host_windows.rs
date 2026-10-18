#![cfg(windows)]

use atlas_connector::*;
use atlas_connector_runtime::{
    host::{HostChannel, HostErrorKind},
    transport::*,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(3);
const PACK: &str = "org.example.host-sdk-fixture";

fn identity() -> ConnectorIdentity {
    ConnectorIdentity {
        pack_id: PACK.into(),
        application_id: PACK.into(),
        application_version: "1.0".into(),
        connector_version: "0.1".into(),
    }
}

fn source() -> SourceRef {
    SourceRef {
        source_id: "fixture-source".into(),
        revision: "revision-1".into(),
        locator: SourceLocator {
            root_id: Some("fixture-root".into()),
            relative: Some("models/generated.fixture".into()),
            absolute: None,
        },
    }
}

fn human(id: &str) -> CallerAuthority {
    CallerAuthority::Human {
        action_id: id.into(),
        target_source: None,
    }
}

fn bind_operation() -> Operation {
    Operation::BindSource {
        source: source(),
        mode: SourceMode::Linked,
        selection_id: "human-selection".into(),
    }
}

fn inspect() -> Operation {
    Operation::InvokeCommand {
        command_id: MOCK_INSPECT_COMMAND.into(),
        parameters: serde_json::json!({}),
    }
}

fn request(pipe: &mut PipeConnection) -> RequestEnvelope {
    match pipe
        .recv::<WireFrame>(WAIT, &Cancellation::default())
        .unwrap()
        .unwrap()
    {
        WireFrame::Request { request } => request,
        other => panic!("expected request, got {other:?}"),
    }
}

fn reply(mock: &mut MockConnector, request: RequestEnvelope) -> ReplyEnvelope {
    // Fixture plugin trusts the independently pinned host. HostChannel itself
    // derives this wire claim from its own trusted action dispatch.
    let host_claim = request.caller.clone();
    mock.handle(request, &host_claim)
}

fn start(
    script: impl FnOnce(&mut PipeConnection, &mut MockConnector) + Send + 'static,
) -> (HostChannel, thread::JoinHandle<()>) {
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
    let peer = thread::spawn(move || {
        let cancel = Cancellation::default();
        let mut pipe = PipeConnection::connect(address, std::process::id(), WAIT, &cancel).unwrap();
        let mut mock = MockConnector::new(identity());
        pipe.send(
            &WireFrame::Hello {
                hello: mock.hello(),
            },
            WAIT,
            &cancel,
        )
        .unwrap();
        match pipe.recv::<WireFrame>(WAIT, &cancel).unwrap().unwrap() {
            WireFrame::Welcome { welcome } => mock.accept_welcome(&welcome).unwrap(),
            _ => panic!("expected welcome"),
        }
        script(&mut pipe, &mut mock);
    });
    let host = HostChannel::accept(
        listener,
        PACK,
        PACK,
        &HostPolicy::default(),
        "host-sdk-session".into(),
        1,
        WAIT,
        &Cancellation::default(),
    )
    .unwrap();
    (host, peer)
}

fn assert_peer_closed(pipe: &mut PipeConnection) {
    assert!(pipe
        .recv::<WireFrame>(WAIT, &Cancellation::default())
        .unwrap()
        .is_none());
}

#[test]
fn validated_calls_replay_locally_and_use_trusted_host_authority() {
    let count = Arc::new(AtomicUsize::new(0));
    let received = Arc::clone(&count);
    let (mut host, peer) = start(move |pipe, mock| {
        for _ in 0..2 {
            let request = request(pipe);
            received.fetch_add(1, Ordering::SeqCst);
            let reply = reply(mock, request);
            pipe.send(&WireFrame::Reply { reply }, WAIT, &Cancellation::default())
                .unwrap();
        }
        assert_peer_closed(pipe);
    });
    // A rejected agent binding never crosses the wire and leaves the healthy
    // channel available for the independently initiated human action.
    let denied = host
        .call(
            "agent-bind",
            None,
            bind_operation(),
            CallerAuthority::Agent {
                agent_id: "fixture-agent".into(),
                workspace_autonomy: true,
            },
            WAIT,
            &Cancellation::default(),
        )
        .unwrap_err();
    assert!(
        matches!(denied.kind(), HostErrorKind::Protocol(error) if error.code == ErrorCode::AuthorityDenied)
    );
    assert!(denied.uncertain_receipt().is_none());
    assert!(host.session().state("agent-bind").is_none());
    host.call(
        "bind",
        None,
        bind_operation(),
        human("bind"),
        WAIT,
        &Cancellation::default(),
    )
    .unwrap();
    let first = host
        .call(
            "inspect",
            Some(source().scope()),
            inspect(),
            human("inspect"),
            WAIT,
            &Cancellation::default(),
        )
        .unwrap();
    let replay = host
        .call(
            "inspect",
            Some(source().scope()),
            inspect(),
            human("inspect"),
            WAIT,
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(first, replay);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    host.close();
    peer.join().unwrap();
}

#[test]
fn invalid_provider_results_close_channel_and_retain_uncertain_receipts() {
    for fault in ["source", "generation", "correlation", "malformed"] {
        let (mut host, peer) = start(move |pipe, mock| {
            let bind = reply(mock, request(pipe));
            pipe.send(
                &WireFrame::Reply { reply: bind },
                WAIT,
                &Cancellation::default(),
            )
            .unwrap();
            let mut inspected = reply(mock, request(pipe));
            match fault {
                "source" => inspected.source.as_mut().unwrap().source_id = "wrong-source".into(),
                "generation" => inspected.generation += 1,
                "correlation" => inspected.request_id = "wrong-request".into(),
                "malformed" => (),
                _ => unreachable!(),
            }
            if fault == "malformed" {
                // Deliberately inject invalid JSON with a valid raw length prefix.
                pipe.send_bytes(b"{ malformed JSON", WAIT, &Cancellation::default())
                    .unwrap();
            } else {
                pipe.send(
                    &WireFrame::Reply { reply: inspected },
                    WAIT,
                    &Cancellation::default(),
                )
                .unwrap();
            }
            assert_peer_closed(pipe);
        });
        host.call(
            "bind",
            None,
            bind_operation(),
            human("bind"),
            WAIT,
            &Cancellation::default(),
        )
        .unwrap();
        let error = host
            .call(
                "inspect",
                Some(source().scope()),
                inspect(),
                human("inspect"),
                WAIT,
                &Cancellation::default(),
            )
            .unwrap_err();
        assert!(matches!(
            &error.uncertain_receipt().unwrap().outcome,
            ReplyBody::OutcomeUnknown { .. }
        ));
        assert_eq!(
            host.session().state("inspect"),
            Some(OperationState::OutcomeUnknown)
        );
        // A repeat queries the tombstone locally. It never executes again.
        let replay = host
            .call(
                "inspect",
                Some(source().scope()),
                inspect(),
                human("inspect"),
                WAIT,
                &Cancellation::default(),
            )
            .unwrap();
        assert_eq!(Some(&replay), error.uncertain_receipt());
        let closed = host
            .call(
                "fresh",
                None,
                Operation::DescribeCommands,
                human("fresh"),
                WAIT,
                &Cancellation::default(),
            )
            .unwrap_err();
        assert!(matches!(closed.kind(), HostErrorKind::Closed));
        assert_eq!(host.session().state("fresh"), None);
        peer.join().unwrap();
    }
}

#[test]
fn timed_out_dispatch_stays_uncertain_and_cannot_automatically_retry() {
    let (mut host, peer) = start(|pipe, _| {
        let _request = request(pipe);
        // Deliberately provide no reply; wait only for the bounded host timeout.
        assert_peer_closed(pipe);
    });
    let started = Instant::now();
    let error = host
        .call(
            "catalog",
            None,
            Operation::DescribeCommands,
            human("catalog"),
            Duration::from_millis(30),
            &Cancellation::default(),
        )
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(matches!(
        &error.uncertain_receipt().unwrap().outcome,
        ReplyBody::OutcomeUnknown { .. }
    ));
    assert_eq!(
        host.session().state("catalog"),
        Some(OperationState::OutcomeUnknown)
    );
    peer.join().unwrap();
}

#[test]
fn selected_pack_and_application_claims_must_both_match() {
    for expected in [("wrong-pack", PACK), (PACK, "wrong-application")] {
        let address = PipeAddress::fresh().unwrap();
        let listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
        let peer = thread::spawn(move || {
            let mut pipe = PipeConnection::connect(
                address,
                std::process::id(),
                WAIT,
                &Cancellation::default(),
            )
            .unwrap();
            pipe.send(
                &WireFrame::Hello {
                    hello: MockConnector::new(identity()).hello(),
                },
                WAIT,
                &Cancellation::default(),
            )
            .unwrap();
            assert_peer_closed(&mut pipe);
        });
        let result = HostChannel::accept(
            listener,
            expected.0,
            expected.1,
            &HostPolicy::default(),
            "host-sdk-session".into(),
            1,
            WAIT,
            &Cancellation::default(),
        );
        assert!(
            matches!(result, Err(error) if matches!(error.kind(), HostErrorKind::IdentityMismatch))
        );
        peer.join().unwrap();
    }
}

#[test]
fn provider_uncertain_outcome_also_requires_explicit_reconciliation() {
    let (mut host, peer) = start(|pipe, _| {
        let request = request(pipe);
        let reply = ReplyEnvelope {
            session_id: request.session_id,
            generation: request.generation,
            request_id: request.request_id,
            source: request.source,
            outcome: ReplyBody::OutcomeUnknown {
                reason: "native outcome cannot be established".into(),
            },
        };
        pipe.send(&WireFrame::Reply { reply }, WAIT, &Cancellation::default())
            .unwrap();
        assert_peer_closed(pipe);
    });
    let receipt = host
        .call(
            "catalog",
            None,
            Operation::DescribeCommands,
            human("catalog"),
            WAIT,
            &Cancellation::default(),
        )
        .unwrap();
    assert!(matches!(receipt.outcome, ReplyBody::OutcomeUnknown { .. }));
    let closed = host
        .call(
            "new-action",
            None,
            Operation::DescribeCommands,
            human("new-action"),
            WAIT,
            &Cancellation::default(),
        )
        .unwrap_err();
    assert!(matches!(closed.kind(), HostErrorKind::Closed));
    assert_eq!(
        host.session().state("catalog"),
        Some(OperationState::OutcomeUnknown)
    );
    peer.join().unwrap();
}
