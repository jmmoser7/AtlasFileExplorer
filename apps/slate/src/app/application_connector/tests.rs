use super::*;
use crate::app::tests::Harness;
use slate_doc::scene::{ImageNode, NodeKind, WorldRect};
fn model(tag: &str) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    let item =
        h.app
            .doc_mut()
            .add_item(h.base.join("test.3dm"), "test.3dm", 1, 0, "connector-model");
    let node = h.app.doc().scene.node_preparation().build_node(
        WorldRect::new(0.0, 0.0, 640.0, 480.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    (h, id)
}
fn native_session(
    h: &mut Harness,
    id: NodeId,
) -> (
    crossbeam_channel::Receiver<worker::Action>,
    crossbeam_channel::Sender<worker::Event>,
) {
    let key = (h.app.tab().id, id);
    let (tx, actions) = crossbeam_channel::bounded(8);
    let (events, rx) = crossbeam_channel::bounded(16);
    h.app.connectors.sessions.insert(
        key,
        Session {
            enabled: true,
            pending: 0,
            ended: false,
            failure: None,
            path: h.base.join("test.3dm"),
            tx,
            rx,
            surface: None,
            descriptor: None,
            process_id: 0,
            binding: None,
            tickets: HashMap::new(),
            status: "ready".into(),
            preview_error_status: None,
            focused: false,
            last_geometry: None,
            host: Default::default(),
            poster: None,
            input_captures: HashMap::new(),
            invocation_receipt: None,
        },
    );
    (actions, events)
}
#[test]
fn image_backed_model_uses_shared_fullscreen_and_keeps_authored_rect() {
    let (mut h, id) = model("connector-fullscreen");
    let before = h.app.doc().scene.node(id).unwrap().clone();
    assert!(h.app.portal_maximize(id));
    assert!(h.app.portal_is_maximized(id));
    assert_eq!(h.app.contents_focused(), Some(id));
    assert!(h.app.portal_restore());
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}
#[test]
fn rhino_activation_without_installed_offer_does_not_launch_or_change_source() {
    let (mut h, id) = model("connector-unavailable");
    let before = h.app.doc().scene.node(id).unwrap().clone();
    assert!(h.app.application_connector_eligible(id));
    h.app.connector_activate(id);
    assert!(h.app.connectors.sessions.is_empty());
    assert!(!h.app.application_connector_active(id));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}
#[test]
fn parking_native_mode_retains_session_and_exclusive_focus_peels() {
    let (mut h, id) = model("connector-parking");
    let key = (h.app.tab().id, id);
    let (_actions, _events) = native_session(&mut h, id);
    h.app.portal_enter_interactive(id);
    assert!(h.app.connectors.sessions[&key].focused);
    assert!(h.app.contents_blur());
    assert!(!h.app.connectors.sessions[&key].focused);
    h.app.connector_deactivate(id);
    assert!(!h.app.application_connector_active(id));
    assert_eq!(h.app.connectors.sessions.len(), 1);
    // The ordinary preview can be entered while the same native document is parked.
    h.app.model3d.headless_live = true;
    h.app.unlock_model(id);
    assert!(h.app.model3d.live.contains_key(&id));
    h.app.connectors.sessions.get_mut(&key).unwrap().process_id = 4242;
    h.app.connector_activate(id);
    assert!(h.app.application_connector_active(id));
    assert!(h.app.connectors.sessions[&key].focused);
    assert!(!h.app.model3d.live.contains_key(&id));
    assert_eq!(h.app.connectors.sessions[&key].process_id, 4242);
    assert_eq!(h.app.connectors.sessions.len(), 1);
}

#[test]
fn accepted_preview_clears_only_its_diagnostic_without_claiming_native_focus() {
    let (mut h, id) = model("connector-preview-diagnostic");
    let key = (h.app.tab().id, id);
    let (_actions, events) = native_session(&mut h, id);
    let preview = || worker::Event::Preview {
        rgba: vec![32, 38, 41, 255],
        width: 1,
        height: 1,
    };
    events
        .send(worker::Event::PreviewUnavailable("cold hidden view".into()))
        .unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    assert!(h.app.connectors.sessions[&key]
        .status
        .contains("cold hidden view"));
    events.send(preview()).unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    let session = &h.app.connectors.sessions[&key];
    assert!(session.poster.is_some());
    assert_eq!(session.status, "Rhino.Inside");
    assert!(!session.focused && !session.host.ready_for_child_launch());

    events
        .send(worker::Event::PreviewUnavailable(
            "older preview error".into(),
        ))
        .unwrap();
    events
        .send(worker::Event::Status(
            "newer native operation failed".into(),
        ))
        .unwrap();
    events.send(preview()).unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    assert_eq!(
        h.app.connectors.sessions[&key].status,
        "newer native operation failed"
    );

    events
        .send(worker::Event::PreviewUnavailable(
            "inactive preview error".into(),
        ))
        .unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    h.app.at_home = true;
    events.send(preview()).unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    assert!(h.app.connectors.sessions[&key]
        .status
        .contains("inactive preview error"));
}

#[test]
fn native_back_to_slate_control_is_visible_and_preserves_bound_document() {
    let (mut h, id) = model("connector-visible-exit");
    let (_actions, _events) = native_session(&mut h, id);
    h.app.patch_nodes(&[id], |node| {
        node.rect = WorldRect::new(-320.0, -240.0, 640.0, 480.0)
    });
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
    let before = h.app.doc().scene.node(id).unwrap().clone();
    h.frame();
    let rect = h.app.board_xf().rect_w2s(before.rect);
    let at = egui::pos2(
        rect.left() + 55.0 * h.app.board_xf().z,
        rect.bottom() - 63.0 * h.app.board_xf().z,
    );
    let output = h.frame_output(|input| input.events.push(egui::Event::PointerMoved(at)));
    fn has_label(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Text(t) => t.galley.text() == "Slate 3D",
            egui::Shape::Vec(shapes) => shapes.iter().any(has_label),
            _ => false,
        }
    }
    assert!(output.shapes.iter().any(|shape| has_label(&shape.shape)));
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    assert!(!h.app.application_connector_active(id));
    assert_eq!(h.app.connectors.sessions.len(), 1);
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}

#[test]
fn tiny_native_card_retains_document_and_intent_without_show_or_placement() {
    let (mut h, id) = model("connector-readable-size");
    let (actions, _events) = native_session(&mut h, id);
    h.app.patch_nodes(&[id], |node| {
        node.rect = WorldRect::new(-120.0, -90.0, 240.0, 180.0)
    });
    h.app.tab_mut().cam.z = 0.39;
    h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
    let before = h.app.doc().scene.node(id).unwrap().clone();
    h.app.portal_enter_interactive(id);
    h.frame();
    assert!(h.app.connectors.sessions[&(h.app.tab().id, id)].focused);
    assert!(h.app.application_connector_active(id));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    while let Ok(action) = actions.try_recv() {
        assert!(!matches!(action, worker::Action::Focus(request) if request.focused));
        assert!(!matches!(action, worker::Action::Place(_)));
    }
}

#[test]
fn native_wired_capture_waits_for_real_pixels_and_has_no_slate_depth() {
    let (mut h, id) = model("connector-native-input");
    let (actions, events) = native_session(&mut h, id);
    let directory = h.base.join("generation-request-1");
    assert!(h
        .app
        .capture_model_inputs(id, &directory, "generator-test")
        .unwrap()
        .is_none());
    let worker::Action::InputCapture {
        directory: queued,
        path,
        agent_id,
    } = actions.try_recv().unwrap()
    else {
        panic!("native input must use the native worker")
    };
    assert_eq!(queued, directory);
    assert_eq!(agent_id, "generator-test");
    assert!(actions.try_recv().is_err());
    assert!(h
        .app
        .capture_model_inputs(id, &directory, "generator-test")
        .unwrap()
        .is_none());
    assert!(
        actions.try_recv().is_err(),
        "polling cannot duplicate a capture"
    );
    events
        .send(worker::Event::InputCapture {
            directory: directory.clone(),
            result: Ok(path.clone()),
        })
        .unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    let capture = h
        .app
        .capture_model_inputs(id, &directory, "generator-test")
        .unwrap()
        .unwrap();
    assert_eq!(capture.view, path);
    assert_eq!(capture.depth, None);
    assert!(h
        .app
        .capture_model_inputs(id, &h.base.join("generation-request-2"), "generator-test")
        .unwrap()
        .is_none());
    assert!(matches!(
        actions.try_recv(),
        Ok(worker::Action::InputCapture { .. })
    ));
}

#[test]
fn native_capture_failure_never_substitutes_slate_cached_geometry() {
    let (mut h, id) = model("connector-native-input-failure");
    let (_actions, _events) = native_session(&mut h, id);
    let key = (h.app.tab().id, id);
    h.app.connectors.sessions.get_mut(&key).unwrap().failure =
        Some("Native viewport unavailable".into());
    assert_eq!(
        h.app
            .capture_model_inputs(id, &h.base, "generator-test")
            .err()
            .unwrap(),
        "Native viewport unavailable"
    );
}

#[test]
fn rebind_removes_old_native_session_instead_of_parking_wrong_source() {
    let (mut h, id) = model("connector-source-rebind");
    let (_actions, _events) = native_session(&mut h, id);
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    let item = image.item;
    assert!(h.app.doc_mut().relink(item, h.base.join("different.3dm")));
    h.app.maintain_application_connectors(&h.ctx);
    assert!(h.app.connectors.sessions.is_empty());
}

#[test]
fn native_escape_peels_contents_then_shared_maximize() {
    let (mut h, id) = model("connector-escape-layers");
    let (_actions, _events) = native_session(&mut h, id);
    h.app.portal_maximize(id);
    assert_eq!(h.app.contents_focused(), Some(id));
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None);
    assert_eq!(h.app.contents_focused(), None);
    assert!(h.app.portal_is_maximized(id));
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None);
    assert!(!h.app.portal_is_maximized(id));
}

#[test]
fn explicit_invoke_routes_preserve_real_caller_authority() {
    let (mut h, id) = model("connector-caller-route");
    let (actions, _events) = native_session(&mut h, id);
    h.app
        .connectors
        .sessions
        .get_mut(&(h.app.tab().id, id))
        .unwrap()
        .invocation_receipt = Some(serde_json::json!({"command":"old"}));
    h.app
        .connector_invoke_scoped(id, "org.slate.rhino.inspect".into(), serde_json::json!({}))
        .unwrap();
    assert!(h.app.connector_invocation_receipt(id).is_none());
    assert!(matches!(
        actions.try_recv(),
        Ok(worker::Action::Invoke { agent: true, .. })
    ));
    h.app.dispatch_application_connector(
        "board.model.connector.invoke",
        Some(&format!(
            "{{\"node\":{},\"command\":\"org.slate.rhino.inspect\",\"parameters\":{{}}}}",
            id.0
        )),
    );
    assert!(matches!(
        actions.try_recv(),
        Ok(worker::Action::Invoke { agent: false, .. })
    ));
}

#[test]
fn native_capture_keeps_agent_author_and_maximized_source_controls() {
    use slate_doc::scene::CmdAuthor;
    let (mut h, id) = model("connector-native-capture-author");
    let (actions, events) = native_session(&mut h, id);
    h.app.portal_maximize(id);
    while actions.try_recv().is_ok() {}
    h.app.sync_shape_properties();
    assert!(!h
        .app
        .open_object_panel(crate::app::board_properties::Panel::ModelDisplay));
    assert!(!h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Filter));
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Capture));
    let author = CmdAuthor::Agent("Design lead".into());
    h.app.connector_capture_as(id, None, author.clone());
    let worker::Action::Capture {
        path,
        canvas,
        author: queued_author,
    } = actions.try_recv().unwrap()
    else {
        panic!("expected native still capture")
    };
    assert!(canvas);
    assert_eq!(queued_author, author);
    // This fixture substitutes only the already verified worker's file event.
    let path = h.base.join(path.file_name().unwrap());
    std::fs::write(&path, b"native pixels").unwrap();
    events
        .send(worker::Event::Capture {
            file: crate::app::items::ReadyFile::read(path).unwrap(),
            size: [1200, 800],
            canvas,
            author,
        })
        .unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Design lead".into()))
    );
    assert!(
        !h.app.board_sel.contains(&id),
        "normal capture selection is preserved"
    );
    let captured_selection = h.app.board_sel.clone();
    h.app.sync_shape_properties();
    assert_eq!(h.app.shape_properties.ids, vec![id]);
    assert!(h.app.portal_is_maximized(id));
    assert_eq!(
        h.app.board_sel, captured_selection,
        "chrome cannot retarget selection"
    );
    h.app.open_portal_capture_menu(&h.ctx, id);
    assert_eq!(h.app.portal_capture_launch_source(), Some(id));
    assert_eq!(h.app.board_sel, captured_selection);
    h.app.portal_restore();
    h.app.sync_shape_properties();
    assert_eq!(
        h.app
            .shape_properties
            .ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>(),
        captured_selection
    );
}

#[test]
fn explicit_reactivation_discards_ended_session_but_preserves_source() {
    let (mut h, id) = model("connector-ended-reconnect");
    let (_actions, events) = native_session(&mut h, id);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    events.send(worker::Event::Ended).unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    assert!(h.app.connectors.sessions[&(h.app.tab().id, id)].ended);
    h.app.connector_deactivate(id);
    h.app.connector_activate(id);
    // This harness has no installed offer: reconnect removes the ended peer,
    // then refuses a new launch instead of presenting the dead window again.
    assert!(h.app.connectors.sessions.is_empty());
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}
