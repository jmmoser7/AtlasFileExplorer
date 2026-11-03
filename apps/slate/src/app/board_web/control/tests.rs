use super::*;
use crate::app::application_connector::native::{test_support, NativeCaller};
use crate::app::tests::{web_board, with_fake_host, Harness};
use atlas_agent::control::ValueSpec;
use atlas_connector::SourceMode;
use slate_doc::scene::{ConnectorEnd, Side, WorldRect};

fn declaration() -> Control {
    Control {
        id: "height".into(),
        input: "model".into(),
        command: "board.model.connector.invoke".into(),
        arguments: json!({"command_id":"org.slate.rhino.draft_building_floors","parameters":{"building_id":"B01"}}),
        value: ValueSpec::Integer {
            path: vec!["parameters".into(), "floors".into()],
            min: 1,
            max: 80,
        },
    }
}
fn intent() -> Intent {
    Intent {
        version: 1,
        control_id: "height".into(),
        sequence: 1,
        value: json!(12),
    }
}
fn setup(
    mode: SourceMode,
) -> (
    Harness,
    NodeId,
    NodeId,
    test_support::Peer,
    crate::app::tests::FakeWebHost,
) {
    let mut h = web_board("declared_control");
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    let (model, peer) = test_support::install(&mut h, mode);
    let mut floor = peer
        .binding
        .commands
        .iter()
        .find(|c| c.id == "org.slate.rhino.draft_box")
        .unwrap()
        .clone();
    floor.id = "org.slate.rhino.draft_building_floors".into();
    floor.parameters = json!({"type":"object","properties":{"building_id":{"type":"string"},"floors":{"type":"integer","minimum":1,"maximum":80}},"required":["building_id","floors"],"additionalProperties":false});
    test_support::add_command(&mut h, model, floor);
    let path = h.base.join("dashboard.html");
    let page = h
        .app
        .build_bound_web(WorldRect::new(0.0, 0.0, 600.0, 500.0), &path);
    let page = h.app.add_nodes(vec![page])[0];
    let host = with_fake_host(&mut h);
    host.set_current_url(page, &path.to_string_lossy());
    let mut slot = Slot::new(h.app.control_source(page).unwrap(), 0);
    slot.manifest = Some(Manifest {
        version: 1,
        controls: vec![declaration()],
    });
    h.app.web.controls.slots.insert(page, slot);
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: model,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: page,
            side: Side::Left,
            t: 0.5,
        },
    );
    h.app.add_nodes(vec![wire]);
    h.app.web.focused = Some(page);
    h.app.web.last_page_input = Some(Instant::now());
    (h, page, model, peer, host)
}
#[test]
fn declared_control_uses_same_human_ticket_and_native_receipt() {
    let (mut h, page, _, peer, _) = setup(SourceMode::IsolatedDraft);
    let pending = h
        .app
        .submit_control(page, &declaration(), &intent())
        .unwrap();
    let request = peer.next().unwrap();
    assert!(matches!(request.caller, NativeCaller::Human));
    let NativeOperation::Invoke { parameters, .. } = &request.operation else {
        panic!("typed invocation expected")
    };
    assert_eq!(parameters["floors"], 12);
    peer.complete(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Value(
                json!({"status":"command_result","value":{"complete":true}}),
            ),
        },
        "after-control",
    );
    let mut slot = h.app.web.controls.slots.remove(&page).unwrap();
    slot.pending = Some(pending);
    h.app.finish_control(page, &mut slot);
    assert!(slot.pending.is_none());
    assert_eq!(slot.state["replies"][0]["status"], "completed");
}
#[test]
fn decorative_wire_navigation_and_agent_focus_never_grant_control_authority() {
    let (mut h, page, _, peer, host) = setup(SourceMode::IsolatedDraft);
    h.app.web.agent_focus_lease = Some((page, "agent-request".into()));
    assert!(h
        .app
        .submit_control(page, &declaration(), &intent())
        .is_err());
    h.app.web.agent_focus_lease = None;
    host.set_current_url(page, "https://example.com/");
    assert!(h
        .app
        .submit_control(page, &declaration(), &intent())
        .is_err());
    host.set_current_url(page, &h.base.join("dashboard.html").to_string_lossy());
    for node in &mut h.app.doc_mut().scene.nodes {
        if let NodeKind::Connector(c) = &mut node.kind {
            c.binding = None;
        }
    }
    assert!(h
        .app
        .submit_control(page, &declaration(), &intent())
        .is_err());
    assert!(peer.next().is_none());
}
#[test]
fn linked_sources_bad_values_and_unadvertised_commands_are_rejected() {
    let (mut h, page, _, peer, _) = setup(SourceMode::Linked);
    assert!(h
        .app
        .submit_control(page, &declaration(), &intent())
        .is_err());
    let mut bad = intent();
    bad.value = json!(81);
    assert!(h.app.submit_control(page, &declaration(), &bad).is_err());
    let mut bad = declaration();
    bad.arguments["command_id"] = json!("org.slate.rhino.source_save");
    assert!(h.app.submit_control(page, &bad, &intent()).is_err());
    assert!(peer.next().is_none());
}
#[test]
fn lost_wire_suppresses_a_completed_native_result() {
    let (mut h, page, _, peer, _) = setup(SourceMode::IsolatedDraft);
    let pending = h
        .app
        .submit_control(page, &declaration(), &intent())
        .unwrap();
    let request = peer.next().unwrap();
    peer.complete(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Value(
                json!({"status":"command_result","value":{"complete":true}}),
            ),
        },
        "after-control",
    );
    for node in &mut h.app.doc_mut().scene.nodes {
        if let NodeKind::Connector(c) = &mut node.kind {
            c.binding = None;
        }
    }
    let mut slot = h.app.web.controls.slots.remove(&page).unwrap();
    slot.pending = Some(pending);
    h.app.finish_control(page, &mut slot);
    assert_eq!(slot.state["replies"][0]["status"], "rejected");
}

#[test]
fn changed_declaration_suppresses_an_in_flight_result() {
    let (mut h, page, _, peer, _) = setup(SourceMode::IsolatedDraft);
    let pending = h
        .app
        .submit_control(page, &declaration(), &intent())
        .unwrap();
    let request = peer.next().unwrap();
    peer.complete(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Value(
                json!({"status":"command_result","value":{"complete":true}}),
            ),
        },
        "after-control",
    );
    let mut slot = h.app.web.controls.slots.remove(&page).unwrap();
    slot.pending = Some(pending);
    slot.invalidated = true;
    h.app.finish_control(page, &mut slot);
    assert_eq!(slot.state["replies"][0]["status"], "rejected");
}

#[test]
fn an_advertised_but_unregistered_native_action_is_not_page_authority() {
    let (mut h, page, _, peer, _) = setup(SourceMode::IsolatedDraft);
    let mut control = declaration();
    control.arguments = json!({"command_id":"org.slate.rhino.draft_box","parameters":{}});
    let error = h
        .app
        .submit_control(page, &control, &intent())
        .err()
        .unwrap();
    assert!(error.contains("not registered"));
    assert!(peer.next().is_none());
}

#[test]
fn pump_observes_withdrawn_or_changed_declarations_before_completion() {
    for changed in [false, true] {
        let (mut h, page, _, peer, host) = setup(SourceMode::IsolatedDraft);
        let pending = h
            .app
            .submit_control(page, &declaration(), &intent())
            .unwrap();
        let request = peer.next().unwrap();
        peer.complete(
            request,
            NativeOutcome::Completed {
                result: NativeResult::Value(
                    json!({"status":"command_result","value":{"complete":true}}),
                ),
            },
            "after-control",
        );
        h.app.web.controls.slots.get_mut(&page).unwrap().pending = Some(pending);
        let text = if changed {
            let mut control = declaration();
            control.arguments["parameters"]["building_id"] = json!("B02");
            json!({"manifest":{"version":1,"controls":[control]},"intents":[]}).to_string()
        } else {
            String::new()
        };
        host.0.borrow_mut().controls.insert(page, Ok(text));
        h.app.web.host_open.insert(page);
        h.app.pump_dashboard_controls(&h.ctx);
        let state = &h.app.web.controls.slots[&page].state;
        assert_eq!(state["ready"], false);
        assert_eq!(state["replies"][0]["status"], "rejected");
    }
}
