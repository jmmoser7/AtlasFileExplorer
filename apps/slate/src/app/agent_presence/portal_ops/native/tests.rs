use super::*;
use crate::app::application_connector::native::{test_support, NativeCaller};
use crate::app::tests::Harness;
use atlas_connector::SourceMode;
mod cancellation;

pub(super) fn setup(
    mode: SourceMode,
    autonomy: bool,
) -> (Harness, slate_doc::NodeId, test_support::Peer, String) {
    let mut h = crate::app::agent_presence::tests::host();
    let (node, peer) = test_support::install(&mut h, mode);
    let who = crate::app::agent_presence::tests::add(&mut h, "Native modeler");
    let actor = h
        .app
        .presence
        .agents
        .iter_mut()
        .find(|a| a.profile.id == who)
        .unwrap();
    actor.request = atlas_agent::request_id();
    actor
        .grant
        .as_mut()
        .unwrap()
        .readable_sources
        .push(SOURCE_INPUTS.into());
    actor.grant.as_mut().unwrap().mutation_autonomy = autonomy;
    actor.grant.as_mut().unwrap().max_actions = 64;
    h.app
        .presence
        .roster
        .session_mut(&actor.motion)
        .unwrap()
        .begin_request(id(&actor.request).unwrap())
        .unwrap();
    (h, node, peer, who)
}
fn begin(
    h: &mut Harness,
    who: &str,
    node: slate_doc::NodeId,
    command: &str,
    arguments: Value,
) -> (BoardCall, Option<BoardReply>) {
    let index = h
        .app
        .presence
        .agents
        .iter()
        .position(|a| a.profile.id == who)
        .unwrap();
    let mut actor = h.app.presence.agents.remove(index);
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: actor.binding.clone(),
        request_id: actor.request.clone(),
        sequence: actor.calls as u64,
        operation: BoardOperation::Invoke {
            command: command.into(),
            targets: vec![node.0.to_string()],
            arguments,
        },
    };
    // The canonical reply writer accepts only calls it actually read from this
    // request's mailbox. Prime that real path before exercising app admission.
    h.app
        .presence
        .links
        .set_session(
            &actor.dir,
            actor.binding.clone(),
            actor.grant.clone().unwrap(),
            actor.request.clone(),
        )
        .unwrap();
    let path = actor
        .dir
        .join("board-calls")
        .join(format!("{}.json", call.id));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    atlas_ai::agent::atomic_write_json(&path, &call).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(observed) = h.app.presence.links.poll_limit(&actor.dir, 1).pop() {
            assert_eq!(observed, call);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "canonical mailbox did not admit the call"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let reply = h.app.presence_call_with_context(&h.ctx, &mut actor, &call);
    h.app.presence.agents.insert(index, actor);
    (call, reply)
}
fn tick(h: &mut Harness) {
    crate::app::agent_presence::tests::proposal_support::tick(h);
}
fn dispatched(
    h: &mut Harness,
    peer: &test_support::Peer,
) -> crate::app::application_connector::native::NativeRequest {
    for _ in 0..160 {
        tick(h);
        if let Some(request) = peer.next() {
            return request;
        }
    }
    panic!("canonical visible action did not reach the native worker");
}
fn reply(h: &mut Harness, who: &str, call: &BoardCall) -> BoardReply {
    let directory = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == who)
        .unwrap()
        .dir
        .clone();
    let path = directory
        .join("board-replies")
        .join(format!("{}.json", call.id));
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        tick(h);
        if let Ok(bytes) = std::fs::read(&path) {
            return serde_json::from_slice(&bytes).unwrap();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("canonical BoardReply was not published");
}

#[test]
fn native_linked_inspect_is_witnessed_and_waits_for_its_actual_caller_receipt() {
    let (mut h, node, peer, who) = setup(SourceMode::Linked, false);
    let (call, immediate) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.inspect",
        json!({}),
    );
    assert!(immediate.is_none());
    let request = dispatched(&mut h, &peer);
    match &request.caller {
        NativeCaller::Agent {
            identity,
            autonomy,
            permit,
        } => {
            assert_eq!(identity.agent, who);
            assert!(!autonomy);
            assert!(permit.is_none());
        }
        _ => panic!("provider call became Human"),
    }
    let folder = h.app.presence.agents[0].dir.join("board-replies");
    assert!(!folder.join(format!("{}.json", call.id)).exists());
    peer.complete(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Value(json!({"status":"command_result","value":{"objects":455}})),
        },
        "selected-revision",
    );
    let result = reply(&mut h, &who, &call);
    assert!(result.success && result.matches(&call));
    assert_eq!(result.result["native_result"]["value"]["objects"], 455);
}

#[test]
fn native_draft_box_requires_isolation_and_actual_actor_autonomy() {
    for (mode, autonomy) in [
        (SourceMode::Linked, true),
        (SourceMode::IsolatedDraft, false),
    ] {
        let (mut h, node, peer, who) = setup(mode, autonomy);
        let (_, result) = begin(
            &mut h,
            &who,
            node,
            "board.model.connector.invoke",
            json!({"command_id":"org.slate.rhino.draft_box","parameters":{"x":4}}),
        );
        assert!(!result.unwrap().success);
        assert!(peer.next().is_none());
    }
    let (mut h, node, peer, who) = setup(SourceMode::IsolatedDraft, true);
    let (call, immediate) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.invoke",
        json!({"command_id":"org.slate.rhino.draft_box","parameters":{"x":4}}),
    );
    assert!(immediate.is_none());
    let request = dispatched(&mut h, &peer);
    match &request.caller {
        NativeCaller::Agent {
            identity,
            permit: Some(permit),
            ..
        } => assert!(permit.valid_for(identity, &request.key, "org.slate.rhino.draft_box")),
        _ => panic!("no actual-actor draft permit"),
    }
    peer.complete(request, NativeOutcome::Completed { result: NativeResult::Value(json!({"status":"command_result","value":{"object":"box-1"},"revision":"after-box"})) }, "after-box");
    let result = reply(&mut h, &who, &call);
    assert!(result.success);
    assert_eq!(result.result["source"]["revision"], "after-box");
}

#[test]
fn native_capture_returns_owned_png_without_draft_or_canvas_mutation() {
    let (mut h, node, peer, who) = setup(SourceMode::Linked, false);
    let before = h.app.doc().scene.nodes.clone();
    let (call, immediate) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.capture",
        json!({}),
    );
    assert!(immediate.is_none());
    let request = dispatched(&mut h, &peer);
    let NativeOperation::Capture(artifact) = &request.operation else {
        panic!("expected protocol capture")
    };
    let path = artifact.path().to_path_buf();
    assert_eq!(
        path.parent(),
        Some(h.base.join("portal-captures").as_path())
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(4, 2, image::Rgba([20, 80, 120, 255]))
        .save(&path)
        .unwrap();
    peer.complete(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Capture {
                path: path.clone(),
                size: [4, 2],
            },
        },
        "selected-revision",
    );
    let result = reply(&mut h, &who, &call);
    assert!(result.success);
    assert_eq!(
        result.result["image_source"],
        path.to_string_lossy().as_ref()
    );
    assert!(
        path.is_file(),
        "published file must survive until provider transport reads it"
    );
    assert_eq!(h.app.doc().scene.nodes, before);
}

#[test]
fn native_pause_revokes_queued_dispatch_and_checks_while_paused() {
    let (mut h, node, peer, who) = setup(SourceMode::IsolatedDraft, true);
    let (call, _) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.invoke",
        json!({"command_id":"org.slate.rhino.draft_box","parameters":{"x":4}}),
    );
    let request = dispatched(&mut h, &peer);
    h.app.presence.agents[0].paused = true;
    let result = reply(&mut h, &who, &call);
    assert!(!result.success);
    assert!(!request.lease.claim());
}

#[test]
fn native_unknown_is_not_a_success_or_an_automatic_replay() {
    let (mut h, node, peer, who) = setup(SourceMode::IsolatedDraft, true);
    let (call, _) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.invoke",
        json!({"command_id":"org.slate.rhino.draft_box","parameters":{"x":4}}),
    );
    let request = dispatched(&mut h, &peer);
    peer.complete(
        request,
        NativeOutcome::OutcomeUnknown {
            receipt: None,
            reason: "native transport ended after dispatch".into(),
        },
        "selected-revision",
    );
    let result = reply(&mut h, &who, &call);
    assert!(!result.success);
    assert_eq!(result.result["status"], "outcome_unknown");
    assert_eq!(result.result["automatic_replay"], false);
    for _ in 0..5 {
        tick(&mut h);
    }
    assert!(peer.next().is_none());
}

#[test]
fn native_rejects_source_writeback_missing_read_grant_and_inactive_sources() {
    let (mut h, node, peer, who) = setup(SourceMode::IsolatedDraft, true);
    let (_, result) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.invoke",
        json!({"command_id":"org.slate.rhino.source_save","parameters":{}}),
    );
    assert!(!result.unwrap().success);
    assert!(peer.next().is_none());
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_sources
        .clear();
    let (_, result) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.inspect",
        json!({}),
    );
    assert!(!result.unwrap().success);
    assert!(peer.next().is_none());
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_sources
        .push(SOURCE_INPUTS.into());
    h.app.connector_deactivate(node);
    let (_, result) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.inspect",
        json!({}),
    );
    assert!(!result.unwrap().success);
    assert!(
        peer.next().is_none(),
        "no untrusted call can start a sidecar"
    );
}

#[test]
fn native_epoch_change_cancels_even_while_actor_is_paused() {
    let (mut h, node, peer, who) = setup(SourceMode::Linked, false);
    let (call, _) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.inspect",
        json!({}),
    );
    h.app.presence.agents[0].control_epoch += 1;
    h.app.presence.agents[0].paused = true;
    let result = reply(&mut h, &who, &call);
    assert!(!result.success);
    assert!(peer.next().is_none());
}

#[test]
fn native_busy_rejection_is_known_and_does_not_become_unknown() {
    let (mut h, node, peer, who) = setup(SourceMode::IsolatedDraft, true);
    let (call, _) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.invoke",
        json!({"command_id":"org.slate.rhino.draft_box","parameters":{"x":4}}),
    );
    let request = dispatched(&mut h, &peer);
    peer.complete(
        request,
        NativeOutcome::Rejected {
            error: atlas_connector::ProtocolError::new(
                atlas_connector::ErrorCode::InvalidRequest,
                "Finish the active native command",
            ),
        },
        "selected-revision",
    );
    let result = reply(&mut h, &who, &call);
    assert!(!result.success);
    assert_eq!(result.result["status"], "native_rejected");
}

#[test]
fn native_rebind_suppresses_late_capture_and_cleans_only_owned_output() {
    let (mut h, node, peer, who) = setup(SourceMode::Linked, false);
    let (call, _) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.capture",
        json!({}),
    );
    let request = dispatched(&mut h, &peer);
    assert!(request.lease.claim());
    let NativeOperation::Capture(artifact) = &request.operation else {
        panic!("capture request")
    };
    let output = artifact.path().to_path_buf();
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(4, 2, image::Rgba([20, 80, 120, 255]))
        .save(&output)
        .unwrap();
    let user_file = h.base.join("user.png");
    std::fs::write(&user_file, b"preserve").unwrap();
    let slate_doc::scene::NodeKind::Image(image) = &h.app.doc().scene.node(node).unwrap().kind
    else {
        unreachable!()
    };
    let item = image.item;
    h.app.doc_mut().relink(item, h.base.join("different.3dm"));
    let result = reply(&mut h, &who, &call);
    assert!(!result.success);
    peer.finish_admitted(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Capture {
                path: output.clone(),
                size: [4, 2],
            },
        },
        "selected-revision",
    );
    h.app.maintain_application_connectors(&h.ctx);
    for _ in 0..100 {
        if !output.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.exists());
    assert_eq!(std::fs::read(user_file).unwrap(), b"preserve");
}
