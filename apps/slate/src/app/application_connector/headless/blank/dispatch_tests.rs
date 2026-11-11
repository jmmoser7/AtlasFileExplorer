#[path = "model_preferences_tests.rs"]
mod model_preferences_tests;

use super::*;
use atlas_commands::CommandId;

#[test]
fn actual_native_activation_without_ai_program_keeps_native_session_and_unbound_history() {
    let mut h = crate::app::tests::Harness::new("native-activation-without-ai");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_agent_programs_for_test(&[]);
    let node = crate::app::board_flow::tests::place(&mut h, "local.obj", 0.);
    let (tx, _actions) = crossbeam_channel::bounded(8);
    let (_events, rx) = crossbeam_channel::bounded(32);
    super::super::HEADLESS_START_FOR_TEST.with(|fixture| *fixture.borrow_mut() = Some((tx, rx)));
    assert!(h.app.connector_activate_headless(node).is_ok());
    assert!(
        super::super::HEADLESS_START_FOR_TEST.with(|fixture| fixture.borrow().is_none()),
        "actual native activation reached its existing worker boundary"
    );
    let session = &h.app.connectors.sessions[&(h.app.tab().id, node)];
    assert!(session.enabled && !session.ended);
    assert_eq!(
        session.descriptor.as_ref().unwrap().mode,
        atlas_connector::SurfaceMode::Headless
    );
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap()).unwrap();
    assert!(agent.provider.is_empty() && agent.model.is_none());
    assert!(!h.app.source_agent_inputs_visible(node));
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn ordinary_text_wire_creates_a_closed_native_model_without_a_cached_provider() {
    let mut h = crate::app::tests::Harness::new("downstream-model-cold-catalog");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.set_agent_programs_for_test(&[]);
    let source = crate::app::board_flow::tests::place(&mut h, "notes.txt", 0.);
    let before = h.app.doc().scene.nodes.len();
    let (reply, rx) = crossbeam_channel::bounded(1);
    let (accepted, admission) = crossbeam_channel::bounded(1);
    EMPTY_PREPARATION_FOR_TEST
        .with(|fixture| *fixture.borrow_mut() = Some((rx, Cancellation::default(), accepted)));
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("portal.agent.spawn"),
        Some(
            serde_json::json!({
                "source": source.0,
                "kind": "model",
                "chooser": true,
                "at": [1400., 200.],
            })
            .to_string()
        )
    ));
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/empty-meters.3dm"
    ));
    let path = h.base.join("native-empty.3dm");
    std::fs::write(&path, bytes).unwrap();
    assert!(matches!(
        model_preview::load_preview(&path, bytes),
        Err(model_preview::LoadError::Empty(reason)) if reason.starts_with("Empty model.")
    ));
    reply
        .send(Ok(super::super::super::super::items::ReadyFile {
            path,
            size: bytes.len() as u64,
            mtime: 0,
        }))
        .unwrap();
    h.app.pump_blank_models(&h.ctx);
    assert_eq!(admission.try_recv(), Ok(true));
    let [node, wire] = &h.app.doc().scene.nodes[before..] else {
        panic!("the blank model and data wire are committed together");
    };
    let NodeKind::Connector(wire) = &wire.kind else {
        panic!("typed connector");
    };
    let binding = wire.binding.as_ref().unwrap();
    assert_eq!(binding.slot.as_deref(), Some("prompt"));
    assert_eq!(
        h.app.wire_end_point(binding.target_end(wire)),
        Some(egui::pos2(1400., 200.))
    );
    assert!(h.app.model_node_info(node.id).is_some());
    assert!(h.app.source_agent_inputs_visible(node.id));
    assert!(!h.app.source_agent_dialogue_open(node.id));
    assert!(!h.app.board_sel.contains(&node.id));
    assert!(h.app.connectors.sessions.is_empty());
    assert!(h.app.agents.dispatched.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), before);
}

fn native_chooser_fixture(name: &str) -> (crate::app::tests::Harness, NodeId) {
    let (mut h, source) = crate::app::board_flow::tests::generator_with_output(name);
    h.app.set_agent_programs_for_test(&["comfy", "codex"]);
    h.app.set_agent_catalog_for_test(
        "codex",
        vec![atlas_agent::AgentModel {
            id: "native-codex-model".into(),
            name: "Native".into(),
            is_default: true,
            ..Default::default()
        }],
    );
    h.app.patch_nodes(&[source], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(false)
    });
    (h, source)
}

#[test]
fn downstream_model_creation_retains_all_inputs_across_undo_redo_and_reload() {
    let (mut h, source) = native_chooser_fixture("downstream-model-input-activation");
    let (_reply, rx) = crossbeam_channel::bounded(1);
    EMPTY_PREPARATION_FOR_TEST.with(|fixture| {
        *fixture.borrow_mut() = Some((rx, Cancellation::default(), crossbeam_channel::bounded(1).0))
    });
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({"source": source.0, "kind": "model", "chooser": true}).to_string()
    )));
    let job = h.app.connectors.headless.blanks.pop().unwrap();
    let path = h.base.join("downstream-empty.3dm");
    std::fs::write(&path, b"prepared native model").unwrap();
    let file = super::super::super::super::items::ReadyFile {
        path,
        size: 21,
        mtime: 0,
    };
    let nodes = h.app.prepare_blank_model_nodes(&job, &file).unwrap();
    let child = h
        .app
        .link_prepared_ready_media(file, nodes, slate_doc::scene::CmdAuthor::Human)
        .unwrap();
    let check = |h: &crate::app::tests::Harness| {
        let node = h.app.doc().scene.node(child).unwrap();
        let ports = h.app.wire_input_ports(node);
        assert!(!ports.is_empty());
        for port in ports {
            assert!(h.app.source_agent_input_visible(child, port.slot));
            assert!(h.app.presented_wire_ports(node).iter().any(|shown| {
                shown.side == slate_doc::scene::Side::Left && (shown.t - port.t).abs() < 0.001
            }));
        }
        assert!(!h.app.source_agent_dialogue_open(child));
        assert!(!h.app.source_agent_inputs_visible(source));
        assert!(h.app.agents.dispatched.is_empty());
    };
    check(&h);
    h.app.board_undo();
    assert!(h.app.doc().scene.node(child).is_none());
    assert!(!h.app.source_agent_inputs_visible(child));
    h.app.board_redo();
    check(&h);
    let saved = serde_json::to_vec(h.app.doc()).unwrap();
    h.app.tab_mut().doc = serde_json::from_slice(&saved).unwrap();
    check(&h);
}

#[test]
fn completed_owned_blank_rejects_late_admission_without_any_partial_document_or_artifact() {
    for failure in ["provider", "catalog", "source", "source-kind"] {
        let (mut h, source) = native_chooser_fixture(&format!("blank-late-{failure}"));
        let path = h.base.join("empty-owned.3dm");
        let marker = atlas_core::workbook_assets::sidecar_path(&path);
        std::fs::write(&path, b"owned blank model").unwrap();
        std::fs::write(&marker, b"owned provenance").unwrap();
        let (rx, cancel, accepted, done) = worker::owned_empty_for_test(path.clone());
        EMPTY_PREPARATION_FOR_TEST
            .with(|fixture| *fixture.borrow_mut() = Some((rx, cancel, accepted)));
        assert!(h.app.spawn_flow_node(Some(
            &serde_json::json!({ "source": source.0, "kind": "model", "chooser": true })
                .to_string()
        )));
        match failure {
            "provider" => h.app.set_agent_programs_for_test(&[]),
            "catalog" => h.app.set_agent_catalog_for_test("codex", vec![]),
            "source" => h.app.patch_nodes(&[source], |node| node.locked = true),
            _ => h.app.patch_nodes(&[source], |node| {
                let NodeKind::Image(image) = &mut node.kind else {
                    panic!()
                };
                image.agent.as_mut().unwrap().view = atlas_agent::PortalView::Text;
            }),
        }
        let document = serde_json::to_vec(h.app.doc()).unwrap();
        let depth = h.app.tab().journal.undo_depth();
        h.app.pump_blank_models(&h.ctx);
        done.recv_timeout(Duration::from_secs(2))
            .expect("dedicated owner finishes cleanup after rejection");
        assert_eq!(
            serde_json::to_vec(h.app.doc()).unwrap(),
            document,
            "{failure} leaves no node, item or allocator mutation"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(
            !path.exists() && !marker.exists(),
            "only the worker's own generated artifacts are retired"
        );
        assert!(h.app.connectors.headless.blanks.is_empty());
        assert!(h.app.agents.dispatched.is_empty());
    }
}

#[test]
fn canceled_owned_blank_cleans_its_artifact_and_root_local_creation_needs_no_ai() {
    let mut h = crate::app::tests::Harness::new("root-local-blank-no-ai");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_agent_programs_for_test(&[]);
    let path = h.base.join("empty-owned.3dm");
    let marker = atlas_core::workbook_assets::sidecar_path(&path);
    std::fs::write(&path, b"owned blank model").unwrap();
    std::fs::write(&marker, b"owned provenance").unwrap();
    let (rx, cancel, accepted, done) = worker::owned_empty_for_test(path.clone());
    EMPTY_PREPARATION_FOR_TEST
        .with(|fixture| *fixture.borrow_mut() = Some((rx, cancel.clone(), accepted)));
    let before = serde_json::to_vec(h.app.doc()).unwrap();
    h.app.connector_new_headless().unwrap();
    let job = &h.app.connectors.headless.blanks[0];
    let file = super::super::super::super::items::ReadyFile {
        path: path.clone(),
        size: 17,
        mtime: 0,
    };
    let prepared = h.app.prepare_blank_model_nodes(job, &file).unwrap();
    assert!(
        slate_doc::agent_chat::agent(&prepared[0]).is_none(),
        "local blank can remain unbound without an AI account"
    );
    h.app.connector_stop_empty_headless();
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(cancel.is_cancelled());
    assert!(!path.exists() && !marker.exists());
    assert_eq!(serde_json::to_vec(h.app.doc()).unwrap(), before);
}

#[test]
fn rejecting_a_generic_ready_file_never_deletes_the_callers_file() {
    let (mut h, source) = native_chooser_fixture("blank-generic-file-is-not-owned");
    let path = h.base.join("user-model.3dm");
    std::fs::write(&path, b"user file").unwrap();
    let (reply, rx) = crossbeam_channel::bounded(1);
    EMPTY_PREPARATION_FOR_TEST.with(|fixture| {
        *fixture.borrow_mut() = Some((rx, Cancellation::default(), crossbeam_channel::bounded(1).0))
    });
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({ "source": source.0, "kind": "model", "chooser": true }).to_string()
    )));
    h.app.set_agent_programs_for_test(&[]);
    reply
        .send(Ok(super::super::super::super::items::ReadyFile {
            path: path.clone(),
            size: 9,
            mtime: 0,
        }))
        .unwrap();
    let before = serde_json::to_vec(h.app.doc()).unwrap();
    h.app.pump_blank_models(&h.ctx);
    assert_eq!(serde_json::to_vec(h.app.doc()).unwrap(), before);
    assert_eq!(std::fs::read(path).unwrap(), b"user file");
}
#[test]
fn blank_model_binding_uses_native_catalog_even_when_text_and_image_defaults_differ() {
    let mut h = crate::app::tests::Harness::new("blank-model-native-catalog");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app
        .set_agent_programs_for_test(&["comfy", "openai-image", "codex"]);
    h.app.set_agent_catalog_for_test(
        "codex",
        vec![atlas_agent::AgentModel {
            id: "native-codex-model".into(),
            name: "Native model".into(),
            is_default: true,
            ..Default::default()
        }],
    );
    h.app.settings.agent_text_model = Some(("openai-text".into(), "text-default".into()));
    h.app.settings.agent_image_model = Some(("comfy".into(), "image-checkpoint".into()));
    let node = crate::app::board_flow::tests::place(&mut h, "native.obj", 0.);
    h.app.bind_model_agent(node).unwrap();
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap()).unwrap();
    assert_eq!(agent.provider, "codex");
    assert_eq!(agent.model.as_deref(), Some("native-codex-model"));
    assert_eq!(agent.view, atlas_agent::PortalView::Chat);
    assert!(h.app.agents.dispatched.is_empty());
}
#[test]
fn output_model_choice_uses_the_existing_cancellable_blank_job_without_sending() {
    let (mut h, source) =
        crate::app::board_flow::tests::generator_with_output("chooser-native-model-job");
    h.app.set_agent_programs_for_test(&["comfy", "codex"]);
    h.app.set_agent_catalog_for_test(
        "codex",
        vec![atlas_agent::AgentModel {
            id: "native-codex-model".into(),
            name: "Native".into(),
            is_default: true,
            ..Default::default()
        }],
    );
    h.app.patch_nodes(&[source], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(false);
    });
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let (reply, rx) = crossbeam_channel::bounded(1);
    let cancel = Cancellation::default();
    EMPTY_PREPARATION_FOR_TEST.with(|fixture| {
        *fixture.borrow_mut() = Some((rx, cancel.clone(), crossbeam_channel::bounded(1).0))
    });
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({
            "source": source.0, "kind": "model", "chooser": true, "at": [1400., 200.],
        })
        .to_string()
    )));
    assert_eq!(h.app.connectors.headless.blanks.len(), 1);
    assert_eq!(
        h.app.connectors.headless.blanks[0]
            .downstream
            .as_ref()
            .unwrap()
            .source,
        source
    );
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.agents.dispatched.is_empty());
    h.app.connector_stop_empty_headless();
    assert!(cancel.is_cancelled());
    drop(reply);
    h.app.pump_blank_models(&h.ctx);
    assert_eq!(h.app.doc().scene.nodes, before);

    let (reply, rx) = crossbeam_channel::bounded(1);
    EMPTY_PREPARATION_FOR_TEST.with(|fixture| {
        *fixture.borrow_mut() = Some((rx, Cancellation::default(), crossbeam_channel::bounded(1).0))
    });
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({ "source": source.0, "kind": "model", "chooser": true }).to_string()
    )));
    reply
        .send(Err("Native model worker failed".into()))
        .unwrap();
    h.app.pump_blank_models(&h.ctx);
    assert_eq!(
        h.app.doc().scene.nodes,
        before,
        "worker failure creates no partial model or connector"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.connectors.headless.blanks.is_empty());
    assert!(h
        .app
        .toasts
        .last()
        .unwrap()
        .0
        .contains("Native model worker failed"));
}

#[test]
fn registered_root_dispatch_routes_activate_generate_and_stop_to_the_bound_job_owner() {
    let mut h = crate::app::tests::Harness::new("headless-root-existing-route");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let (node, peer) = crate::app::application_connector::native::test_support::install(
        &mut h,
        atlas_connector::SourceMode::IsolatedDraft,
    );
    crate::app::application_connector::native::test_support::set_surface_descriptor(
        &mut h,
        node,
        atlas_connector::SurfaceDescriptor {
            surface_id: "headless".into(),
            mode: atlas_connector::SurfaceMode::Headless,
            process_ownership: atlas_connector::ProcessOwnership::LaunchedDedicated,
            native_handle: None,
            can_detach: true,
        },
    );
    let detail = serde_json::json!({"node":node.0}).to_string();
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("board.model.headless.activate"),
        Some(detail.clone())
    ));
    let plan = atlas_connector_runtime::headless_rhino::GeometryPlan {
        units: atlas_connector_runtime::headless_rhino::Units::Meters,
        operations: vec![],
    };
    let detail=serde_json::json!({"node":node.0,"parameters":{"plan":serde_json::to_string(&plan).unwrap()}}).to_string();
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("board.model.headless.generate"),
        Some(detail)
    ));
    let key = (h.app.tab().id, node);
    assert!(h.app.connectors.headless.jobs.contains_key(&key));
    assert!(
        peer.next().is_none(),
        "dispatch queues preparation; it does not skip native admission"
    );
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("board.model.headless.stop"),
        Some(serde_json::json!({"node":node.0}).to_string())
    ));
    assert!(!h.app.connectors.headless.jobs.contains_key(&key));
    assert_eq!(
        h.app.native_binding(node).unwrap().binding,
        peer.binding.binding
    );
}

#[test]
fn registered_root_dispatch_schedules_blank_without_target_and_stop_cancels_it() {
    let mut h = crate::app::tests::Harness::new("headless-root-new-route");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.board_sel.clear();
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let (_reply, rx) = crossbeam_channel::bounded(1);
    let cancel = Cancellation::default();
    EMPTY_PREPARATION_FOR_TEST.with(|fixture| {
        *fixture.borrow_mut() = Some((rx, cancel.clone(), crossbeam_channel::bounded(1).0))
    });
    assert!(!h
        .app
        .dispatch(&h.ctx, CommandId("board.model.headless.unsupported"), None));
    assert!(h.app.connectors.headless.blanks.is_empty());
    assert!(
        h.app
            .prepare_targeted_command("board.model.headless.new", &[], &serde_json::json!({}))
            .is_err(),
        "human setup must not become an agent targeted mutation"
    );
    h.app.tab_mut().read_only = true;
    assert!(h
        .app
        .dispatch(&h.ctx, CommandId("board.model.headless.new"), None));
    assert!(h.app.connectors.headless.blanks.is_empty());
    assert!(h.app.toasts.last().unwrap().0.contains("editable board"));
    h.app.tab_mut().read_only = false;
    assert!(h
        .app
        .dispatch(&h.ctx, CommandId("board.model.headless.new"), None));
    assert_eq!(h.app.connectors.headless.blanks.len(), 1);
    assert!(EMPTY_PREPARATION_FOR_TEST.with(|fixture| fixture.borrow().is_none()));
    assert!(h
        .app
        .toasts
        .last()
        .unwrap()
        .0
        .contains("Creating an empty Rhino model"));
    assert!(h
        .app
        .dispatch(&h.ctx, CommandId("board.model.headless.stop"), None));
    assert!(h.app.connectors.headless.blanks.is_empty());
    assert!(cancel.is_cancelled());
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
