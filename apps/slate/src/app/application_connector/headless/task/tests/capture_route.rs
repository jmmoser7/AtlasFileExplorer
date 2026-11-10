use super::*;
use crate::app::application_connector::native::{Completion, NativeOperation};

#[test]
fn own_not_dispatched_capture_advances_only_observation_and_keeps_task_valid() {
    for change in 0..6 {
        let (mut h, node, peer) = fixture(&format!("capture-refusal-observation-{change}"));
        submit(&mut h, node);
        let key = (h.app.tab().id, node);
        let mut task = h.app.connectors.headless.tasks.active.remove(&key).unwrap();
        let expected = task.permit.binding.clone();
        let call = call(
            &task,
            "board.model.connector.capture",
            vec![node.0.to_string()],
            json!({}),
        );
        let mut identity = task.permit.identity.clone();
        identity.call = call.id.clone();
        // Reproduce EB19's former route: the headless worker advances serial
        // then refuses Capture without claiming a native dispatch lease.
        let ticket = h
            .app
            .native_submit_model_task_in(
                (key.0, node),
                &expected,
                identity,
                &task.permit,
                NativeOperation::Capture(h.app.native_artifact(h.base.clone())),
                Lease::new(),
            )
            .unwrap_or_else(|_| panic!("trusted fixture admission"));
        task.pending = Some(calls::Pending { call, ticket });
        let request = peer.next().unwrap();
        assert!(matches!(request.operation, NativeOperation::Capture(_)));
        let mut actor = match &request.caller {
            NativeCaller::Agent { identity, .. } => identity.clone(),
            _ => panic!(),
        };
        let mut after = expected.binding.clone();
        let mut outcome =
            NativeOutcome::not_dispatched("Capture this model through the native Slate viewport");
        if change == 1 {
            actor.call = "wrong-call".into();
        }
        if change == 2 {
            after.source.revision = "unrelated-source-revision".into();
        }
        if change == 3 {
            outcome = NativeOutcome::OutcomeUnknown {
                receipt: None,
                reason: "unknown native effect".into(),
            };
        }
        if change == 4 {
            let other = h.app.doc_mut().add_item(
                h.base.join("different.3dm"),
                "different.3dm",
                1,
                0,
                "different-file",
            );
            h.app.patch_nodes(&[node], |node| {
                if let slate_doc::scene::NodeKind::Image(image) = &mut node.kind {
                    image.item = other;
                }
            });
        }
        request
            .reply
            .try_send(Completion {
                ticket: request.ticket,
                key: request.key,
                binding_after: Some(after),
                observation: expected.observation + if change == 5 { 3 } else { 1 },
                actor: Some(actor),
                outcome,
            })
            .ok()
            .unwrap();
        calls::pump(&mut h.app, &mut task);
        if change == 0 {
            assert_eq!(task.permit.binding.observation, expected.observation + 1);
            assert_eq!(task.permit.binding.binding, expected.binding);
            assert!(
                task.validate(&h.app, true).is_ok(),
                "own no-effect refusal must not impersonate an external source change"
            );
            assert!(!task.model_accepted && task.mutations == 0);
        } else {
            assert_eq!(
                task.permit.binding.observation, expected.observation,
                "unsafe receipt must not refresh authority"
            );
            assert!(
                task.validate(&h.app, true).is_err(),
                "wrong actor/source/file, unknown effect or unexplained serial jump remains fenced"
            );
        }
    }
}

#[test]
fn advertised_model_capture_routes_to_scoped_viewport_and_defers_parked_work_without_focus_change()
{
    let (mut h, node, peer) = fixture("model-capture-viewport-route");
    submit(&mut h, node);
    let key = (h.app.tab().id, node);
    let mut task = h.app.connectors.headless.tasks.active.remove(&key).unwrap();
    let wrong = call(
        &task,
        "board.model.connector.capture",
        vec![(node.0 + 100).to_string()],
        json!({}),
    );
    assert!(calls::start(&mut h.app, &mut task, &wrong).is_err());
    assert!(task.capture.is_none());
    let request = call(
        &task,
        "board.model.connector.capture",
        vec![node.0.to_string()],
        json!({}),
    );
    assert!(calls::start(&mut h.app, &mut task, &request)
        .unwrap()
        .is_none());
    assert!(task.capture.is_some() && task.pending.is_none());
    assert!(
        peer.next().is_none(),
        "capture must not enter the SDK worker"
    );
    h.app.sync_agent_doc();
    let mut other = crate::app::SlateTab::empty();
    other.doc = h.app.doc().clone();
    h.app.tabs.push(other);
    h.app.active_tab = h.app.tabs.len() - 1;
    h.app.sync_agent_doc();
    let visible = h.app.tab().id;
    let camera = h.app.tab().cam;
    let selected = h.app.board_sel.clone();
    let mut pending = task.capture.take().unwrap();
    assert!(super::super::capture::poll(&mut h.app, &task, &mut pending)
        .unwrap()
        .is_none());
    assert_eq!(h.app.tab().id, visible);
    assert_eq!(h.app.tab().cam.offset, camera.offset);
    assert_eq!(h.app.tab().cam.z, camera.z);
    assert_eq!(h.app.board_sel, selected);
    h.app.active_tab = h.app.tabs.iter().position(|tab| tab.id == key.0).unwrap();
    h.app.sync_agent_doc();
    let error = super::super::capture::poll(&mut h.app, &task, &mut pending).unwrap_err();
    assert!(
        error.contains("GPU rendering"),
        "the ordinary viewport owner is reached in a GL-free harness: {error}"
    );
    assert!(peer.next().is_none());
    assert!(task.validate(&h.app, true).is_ok());
}

#[test]
fn scoped_viewport_png_finalization_is_real_hashed_unique_and_revocable() {
    let (mut h, node, _peer) = fixture("model-capture-png-finalize");
    submit(&mut h, node);
    let key = (h.app.tab().id, node);
    let task = &h.app.connectors.headless.tasks.active[&key];
    let source = h.base.join("captured-view.png");
    image::RgbImage::from_pixel(2, 1, image::Rgb([17, 33, 91]))
        .save(&source)
        .unwrap();
    let destination = h.base.join("portal-captures/scoped.png");
    let value =
        super::super::capture::finalize(&source, &destination, &h.base, &task.permit).unwrap();
    let bytes = std::fs::read(&destination).unwrap();
    assert_eq!(bytes, std::fs::read(&source).unwrap());
    assert_eq!(
        value["image_sha256"],
        model_preview::view_meta::hash_bytes(&bytes)
    );
    assert_eq!(value["width"], 2);
    assert_eq!(value["height"], 1);
    assert_eq!(value["capture_owner"], "slate_native_viewport");
    assert!(super::super::capture::finalize(&source, &destination, &h.base, &task.permit).is_err());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        bytes,
        "a filename collision cannot overwrite or delete an existing capture"
    );
    task.permit.lease.revoke();
    let cancelled = h.base.join("portal-captures/cancelled.png");
    assert!(super::super::capture::finalize(&source, &cancelled, &h.base, &task.permit).is_err());
    assert!(!cancelled.exists());
}

#[test]
fn validated_native_rejection_preserves_its_actual_terminal_cause_without_retry_or_counter_adoption(
) {
    let (mut h, node, peer) = fixture("model-rejected-cause");
    submit(&mut h, node);
    let key = (h.app.tab().id, node);
    let mut task = h.app.connectors.headless.tasks.active.remove(&key).unwrap();
    let before = task.permit.binding.clone();
    h.app.set_agent_session_for_test(
        node,
        atlas_agent::AgentSession {
            provider: task.target.provider.clone(),
            request: task.permit.identity.request.clone(),
            status: atlas_agent::AgentStatus::Thinking,
            turns: vec![atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "Previous successful output is not this failed request".into(),
                at: 1,
            }],
            ..Default::default()
        },
    );
    let request = call(
        &task,
        "board.model.connector.invoke",
        vec![node.0.to_string()],
        json!({"command_id":atlas_connector_runtime::headless_rhino::GENERATE_COMMAND,"parameters":{"plan":json!({"units":"meters","operations":[{"kind":"box","id":"marker","origin":[0,0,0],"size":[1,1,1]}]}).to_string()}}),
    );
    assert!(calls::start(&mut h.app, &mut task, &request)
        .unwrap()
        .is_none());
    let native = peer.next().unwrap();
    let reason = "Specific SDK rejection: original model retained";
    peer.complete(
        native,
        NativeOutcome::Rejected {
            error: atlas_connector::ProtocolError::new(
                atlas_connector::ErrorCode::InvalidResult,
                reason,
            ),
        },
        &before.binding.source.revision,
    );
    calls::pump(&mut h.app, &mut task);
    assert_eq!(task.failure.as_deref(), Some(reason));
    assert_eq!(
        task.permit.binding.observation, before.observation,
        "rejection never refreshes source authority"
    );
    assert_eq!(task.validate(&h.app, true).unwrap_err(), reason);
    let retry = call(
        &task,
        "board.model.connector.invoke",
        vec![node.0.to_string()],
        json!({}),
    );
    assert_eq!(
        calls::start(&mut h.app, &mut task, &retry).unwrap_err(),
        reason
    );
    assert!(peer.next().is_none());
    assert_eq!(task.mutations, 1);
    h.app.connectors.headless.tasks.active.insert(key, task);
    h.app.pump_model_tasks(&h.ctx);
    assert!(!h.app.model_task_is_running(node));
    assert_eq!(h.app.agent_failure_reason(node), Some(reason));
}
