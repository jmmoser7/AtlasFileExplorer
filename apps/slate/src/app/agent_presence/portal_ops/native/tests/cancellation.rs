use super::*;

#[test]
fn revoke_after_admission_retains_real_scope_without_a_binding_event() {
    let (mut h, node, peer, who) = setup(SourceMode::IsolatedDraft, true);
    let (call, _) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.invoke",
        json!({"command_id":"org.slate.rhino.draft_box","parameters":{"x":4}}),
    );
    let request = dispatched(&mut h, &peer);
    assert!(request.lease.claim());
    h.app.presence.agents[0].paused = true;
    let result = reply(&mut h, &who, &call);
    assert!(!result.success);
    assert_eq!(
        result.result["status"],
        "native_action_stopped_after_admission"
    );
    peer.finish_admitted(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Value(
                json!({"status":"command_result","value":{"object":"actual-box"}}),
            ),
        },
        "actual-after-box",
    );
    h.app.maintain_application_connectors(&h.ctx);
    assert_eq!(
        h.app.native_binding(node).unwrap().binding.source.revision,
        "actual-after-box"
    );
    assert!(
        peer.next().is_none(),
        "revocation cannot replay an admitted effect"
    );
}

#[test]
fn revoke_after_capture_admission_suppresses_import_and_cleans_owned_pixels() {
    let (mut h, node, peer, who) = setup(SourceMode::Linked, false);
    let before = h.app.doc().scene.nodes.clone();
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
    h.app.presence.agents[0].paused = true;
    assert!(!reply(&mut h, &who, &call).success);
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
    assert_eq!(h.app.doc().scene.nodes, before);
}

#[test]
fn unresolved_receipt_reservations_refuse_new_dispatch_without_eviction() {
    let (mut h, first, first_peer, who) = setup(SourceMode::Linked, false);
    for index in 0..8 {
        let (node, peer) = if index == 0 {
            (first, None)
        } else {
            let (node, peer) = test_support::install(&mut h, SourceMode::Linked);
            (node, Some(peer))
        };
        let peer = peer.as_ref().unwrap_or(&first_peer);
        let (call, immediate) = begin(
            &mut h,
            &who,
            node,
            "board.model.connector.inspect",
            json!({}),
        );
        assert!(immediate.is_none());
        let request = dispatched(&mut h, peer);
        peer.complete(
            request,
            NativeOutcome::OutcomeUnknown {
                receipt: None,
                reason: "Retained native outcome".into(),
            },
            "selected-revision",
        );
        assert_eq!(
            reply(&mut h, &who, &call).result["status"],
            "outcome_unknown"
        );
    }
    let (node, peer) = test_support::install(&mut h, SourceMode::Linked);
    let (call, immediate) = begin(
        &mut h,
        &who,
        node,
        "board.model.connector.inspect",
        json!({}),
    );
    assert!(immediate.is_none());
    let result = reply(&mut h, &who, &call);
    assert!(!result.success);
    assert_eq!(result.result["status"], "not_dispatched");
    assert!(peer.next().is_none());
}
