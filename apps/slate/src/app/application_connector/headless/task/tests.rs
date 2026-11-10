use super::*;
mod background;
mod capture_route;
mod history;
mod subscription;
use crate::app::{
    application_connector::native::{test_support, NativeCaller},
    tests::Harness,
};
use atlas_agent::collaborator::{BoardCall, BoardOperation};
use atlas_connector::{ProcessOwnership, SourceMode, SurfaceDescriptor};
use serde_json::json;

fn fixture(name: &str) -> (Harness, NodeId, test_support::Peer) {
    let mut h = Harness::new(name);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.set_agent_programs_for_test(&["codex"]);
    h.app.set_agent_catalog_for_test(
        "codex",
        vec![atlas_agent::AgentModel {
            id: "native-task-model".into(),
            name: "Native task model".into(),
            is_default: true,
            ..Default::default()
        }],
    );
    h.app.settings.agent_artifact_model = Some(("codex".into(), "native-task-model".into()));
    let (node, mut peer) = test_support::install(&mut h, SourceMode::IsolatedDraft);
    test_support::set_surface_descriptor(
        &mut h,
        node,
        SurfaceDescriptor {
            surface_id: "headless".into(),
            mode: SurfaceMode::Headless,
            process_ownership: ProcessOwnership::LaunchedDedicated,
            native_handle: None,
            can_detach: true,
        },
    );
    test_support::add_command(
        &mut h,
        node,
        atlas_connector_runtime::headless_rhino::command_descriptor(),
    );
    peer.binding
        .commands
        .push(atlas_connector_runtime::headless_rhino::command_descriptor());
    (h, node, peer)
}

fn wait_for_task(h: &mut Harness, node: NodeId) {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        h.app.maintain_application_connectors(&h.ctx);
        if h.app
            .connectors
            .headless
            .tasks
            .active
            .contains_key(&(h.app.tab().id, node))
        {
            return;
        }
        std::thread::yield_now();
    }
    panic!(
        "human model task did not reach the ordinary provider submission: {:?}",
        h.app.agent_failure_reason(node)
    );
}

fn acknowledge_task_stop(h: &mut Harness, node: NodeId, request: &str) {
    // The test backend has no provider process to send its terminal Stop ACK.
    // Keep the same current-request public-session transition as the runtime.
    let (_, provider) = h.app.agent_session_for(node).unwrap();
    h.app.set_agent_session_for_test(
        node,
        atlas_agent::AgentSession {
            request: request.into(),
            provider,
            status: atlas_agent::AgentStatus::Idle,
            ..Default::default()
        },
    );
    h.app.agent_pump(&h.ctx);
    assert!(!h.app.agent_is_running(node));
}

fn submit(h: &mut Harness, node: NodeId) {
    h.app.submit_pending_model_task_for_test(
        &h.ctx,
        node,
        "Create one box using the declared inputs.",
    );
    wait_for_task(h, node);
}

#[test]
fn human_first_send_ensures_headless_setup_without_a_board_grant() {
    let (mut h, node, peer) = fixture("model-task-automatic-setup");
    let key = (h.app.tab().id, node);
    h.app.retire_connector_session(key);
    let (actions, _requests) = crossbeam_channel::bounded(8);
    let (events, receive) = crossbeam_channel::bounded(8);
    super::super::HEADLESS_START_FOR_TEST
        .with(|slot| *slot.borrow_mut() = Some((actions, receive)));
    assert!(!h.app.model_task_is_running(node));
    h.app
        .submit_pending_model_task_for_test(&h.ctx, node, "Create a box");
    assert!(h.app.connectors.headless.setup_sends.contains_key(&key));
    assert!(h.app.agents.dispatched.is_empty());
    assert!(h.app.presence.agents_empty());
    events
        .send(worker::Event::Ready {
            surface: SurfaceDescriptor {
                surface_id: "headless".into(),
                mode: SurfaceMode::Headless,
                process_ownership: ProcessOwnership::LaunchedDedicated,
                native_handle: None,
                can_detach: true,
            },
            process_id: 0,
            binding: peer.binding.clone(),
        })
        .unwrap();
    h.app.maintain_application_connectors(&h.ctx);
    wait_for_task(&mut h, node);
    assert_eq!(
        h.app.agents.dispatched.last().unwrap().1.prompt,
        "Create a box"
    );
    assert!(h.app.presence.agents_empty());
    assert_eq!(h.app.attached_model_native_access(node), Some(false));
}

#[test]
fn declared_images_are_immutable_private_copies_and_next_draft_is_not_consumed() {
    let (mut h, node, _peer) = fixture("model-task-frozen-wires");
    let source = h.base.join("declared.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 40, 60, 255]))
        .save(&source)
        .unwrap();
    let original = std::fs::read(&source).unwrap();
    let item = h.app.doc_mut().add_item(
        source.clone(),
        "declared.png",
        original.len() as u64,
        0,
        "task-source",
    );
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(-200.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let source_node = h.app.add_nodes(vec![image])[0];
    let t = h
        .app
        .wire_input_ports(h.app.doc().scene.node(node).unwrap())
        .iter()
        .find(|port| port.slot == atlas_agent::InputSlot::Media)
        .unwrap()
        .t;
    let wire = h.app.build_connector(
        slate_doc::scene::ConnectorEnd::Anchored {
            node: source_node,
            side: slate_doc::scene::Side::Right,
            t: 0.5,
        },
        slate_doc::scene::ConnectorEnd::Anchored {
            node,
            side: slate_doc::scene::Side::Left,
            t,
        },
    );
    h.app.add_nodes(vec![wire]);
    h.app
        .submit_pending_model_task_for_test(&h.ctx, node, "Use this declared image");
    assert!(h.app.model_task_is_running(node));
    *h.app.agents.prompt_mut(node) = "Unsubmitted next task".into();
    wait_for_task(&mut h, node);
    let request = &h.app.agents.dispatched.last().unwrap().1;
    assert_eq!(request.prompt, "Use this declared image");
    assert_eq!(h.app.agents.prompt(node), Some("Unsubmitted next task"));
    let path = std::path::Path::new(
        &request
            .inputs
            .wired
            .iter()
            .find(|item| item.node == source_node.0)
            .unwrap()
            .images[0],
    );
    assert_ne!(path, source.as_path());
    assert_eq!(std::fs::read(path).unwrap(), original);
    std::fs::write(&source, b"changed after acceptance").unwrap();
    assert_eq!(
        std::fs::read(path).unwrap(),
        original,
        "request retains exact submitted bytes"
    );
    assert!(h.app.presence.agents_empty());
}

fn call(
    task: &ModelTaskAuthority,
    command: &str,
    targets: Vec<String>,
    arguments: serde_json::Value,
) -> BoardCall {
    BoardCall {
        id: atlas_agent::request_id(),
        session: task.permit.identity.membership.clone(),
        request_id: task.permit.identity.request.clone(),
        sequence: task.next_sequence,
        operation: BoardOperation::Invoke {
            command: command.into(),
            targets,
            arguments,
        },
    }
}

#[test]
fn explicit_first_model_send_creates_task_without_general_board_membership_then_repeat_and_reload()
{
    let (mut h, node, _peer) = fixture("model-task-human-entry");
    assert!(!h.app.presence.has_live_agents(h.app.tab().id));
    assert!(
        !h.app.model_task_is_running(node),
        "preview/setup and model selection grant no task"
    );
    let source_path = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::scene::NodeKind::Image(image) => &h.app.doc().item(image.item).unwrap().path,
        _ => panic!("native fixture source"),
    };
    assert!(source_path.to_string_lossy().contains("model-task-"));
    let before = serde_json::to_string(h.app.doc()).unwrap();
    assert!(before.contains(&serde_json::to_string(source_path).unwrap()));
    submit(&mut h, node);
    let first = h.app.agents.dispatched.last().unwrap().1.clone();
    assert_eq!(first.prompt, "Create one box using the declared inputs.");
    assert_eq!(first.model.as_deref(), Some("native-task-model"));
    assert!(first.collaborator.as_ref().unwrap().is_model_task());
    assert!(!h.app.presence.has_live_agents(h.app.tab().id));
    assert_eq!(h.app.attached_model_native_access(node), Some(false));
    assert!(
        h.app
            .validate_chat_train_send(node)
            .unwrap_err()
            .contains("Resume"),
        "general collaborator policy remains required outside Human task"
    );
    let key = (h.app.tab().id, node);
    let task = &h.app.connectors.headless.tasks.active[&key];
    assert!(!task.grant.readable_board && !task.grant.mutation_autonomy);
    assert!(task.grant.readable_sources.is_empty() && task.grant.conversation_ids.is_empty());
    assert_eq!(task.grant.helper_limit, 0);
    let saved = serde_json::to_vec(h.app.doc()).unwrap();
    let saved_text = String::from_utf8_lossy(&saved);
    let membership = first.collaborator.as_ref().unwrap();
    let private_dir = task.dir.to_string_lossy();
    for (field, value) in [
        ("ephemeral session", membership.session_id.as_str()),
        ("ephemeral grant", membership.grant_id.as_str()),
        ("private transport folder", private_dir.as_ref()),
    ] {
        let exact_json_string = serde_json::to_string(value).unwrap();
        assert!(
            !saved_text.contains(&exact_json_string),
            "live task {field} must never be workbook state"
        );
    }
    assert!(h.app.stop_agent_request_at(node, &first.id));
    assert!(!h.app.model_task_is_running(node));
    acknowledge_task_stop(&mut h, node, &first.id);
    for reopen in [false, true] {
        if reopen {
            h.app.tab_mut().doc = serde_json::from_slice(&saved).unwrap();
        }
        *h.app.agents.prompt_mut(node) = "A new explicit model task".into();
        h.app.board_sel = [node].into_iter().collect();
        assert!(h.app.send_selected_agent_prompt());
        wait_for_task(&mut h, node);
        let request = h.app.agents.dispatched.last().unwrap().1.clone();
        assert_eq!(request.prompt, "A new explicit model task");
        assert_ne!(request.collaborator, first.collaborator);
        assert!(!h.app.presence.has_live_agents(h.app.tab().id));
        assert!(h.app.stop_agent_request_at(node, &request.id));
        acknowledge_task_stop(&mut h, node, &request.id);
    }
}

#[test]
fn model_task_admits_only_own_closed_generation_and_carries_revocable_worker_purpose() {
    let (mut h, node, peer) = fixture("model-task-command-scope");
    submit(&mut h, node);
    let key = (h.app.tab().id, node);
    let mut task = h.app.connectors.headless.tasks.active.remove(&key).unwrap();
    for wrong_doc in [true, false] {
        let mut permit = task.permit.clone();
        if wrong_doc {
            permit.doc += 1;
        } else {
            permit.node = NodeId(node.0 + 1000);
        }
        let mut identity = permit.identity.clone();
        identity.call = atlas_agent::request_id();
        assert!(h.app.native_submit_model_task_in((h.app.tab().id,node),&permit.binding,identity,&permit,crate::app::application_connector::native::NativeOperation::Inspect,Lease::new()).is_err(),"lower native boundary binds the exact Slate doc and node even if native bindings are cloned");
        assert!(peer.next().is_none());
    }
    for wrong_request in [false, true] {
        let mut foreign = call(
            &task,
            "board.model.connector.inspect",
            vec![node.0.to_string()],
            json!({}),
        );
        if wrong_request {
            foreign.request_id = atlas_agent::request_id();
        } else {
            foreign.session.grant_id = atlas_agent::request_id();
        }
        assert!(calls::start(&mut h.app, &mut task, &foreign).is_err());
    }
    task.doc += 1;
    let foreign = call(
        &task,
        "board.model.connector.inspect",
        vec![node.0.to_string()],
        json!({}),
    );
    assert!(
        calls::start(&mut h.app, &mut task, &foreign).is_err(),
        "other workbook denied"
    );
    task.doc -= 1;
    for (command, target, args) in [
        (
            "presence.read_image",
            node,
            json!({"path":"C:/outside.png"}),
        ),
        ("presence.helper", node, json!({})),
        ("presence.send", node, json!({})),
        ("board.objects.delete", node, json!({})),
        (
            "board.model.connector.invoke",
            node,
            json!({"command_id":"org.slate.rhino.source_save","parameters":{}}),
        ),
        (
            "board.model.connector.invoke",
            NodeId(node.0 + 1000),
            json!({"command_id":atlas_connector_runtime::headless_rhino::GENERATE_COMMAND,"parameters":{}}),
        ),
    ] {
        let call = call(&task, command, vec![target.0.to_string()], args);
        assert!(
            calls::start(&mut h.app, &mut task, &call).is_err(),
            "deny {command}"
        );
        assert!(peer.next().is_none());
    }
    let plan = json!({"units":"meters","operations":[{"kind":"box","id":"box","origin":[0,0,0],"size":[2,3,4]}]});
    let request = call(
        &task,
        "board.model.connector.invoke",
        vec![node.0.to_string()],
        json!({"command_id":atlas_connector_runtime::headless_rhino::GENERATE_COMMAND,"parameters":{"plan":plan.to_string()}}),
    );
    assert!(calls::start(&mut h.app, &mut task, &request)
        .unwrap()
        .is_none());
    assert!(
        calls::start(&mut h.app, &mut task, &request).is_err(),
        "replay denied"
    );
    let native = peer.next().unwrap();
    assert!(native.expires <= task.permit.expires);
    match &native.caller {
        NativeCaller::Agent {
            autonomy,
            permit: Some(permit),
            ..
        } => {
            assert!(!autonomy);
            assert!(permit.model_task() && permit.live());
            assert_eq!(
                permit.commands(),
                &[atlas_connector_runtime::headless_rhino::GENERATE_COMMAND.to_string()]
            );
            task.permit.lease.revoke();
            assert!(
                !permit.live(),
                "worker retains same revocation capability, independent of frame pump"
            );
        }
        _ => panic!("task must retain Agent authorship, never pretend to be Human"),
    }
    native.lease.revoke();
    task.permit.expires = Instant::now() - Duration::from_secs(1);
    let mut identity = task.permit.identity.clone();
    identity.call = atlas_agent::request_id();
    assert!(h
        .app
        .native_submit_model_task_in(
            (h.app.tab().id, node),
            &task.permit.binding,
            identity,
            &task.permit,
            crate::app::application_connector::native::NativeOperation::Inspect,
            Lease::new()
        )
        .is_err());
    assert!(
        peer.next().is_none(),
        "expired task cannot enqueue another native operation"
    );
}

#[test]
fn generic_send_cannot_mint_task_and_stale_or_readonly_human_task_never_dispatches() {
    let (mut h, node, peer) = fixture("model-task-origin-and-stale");
    h.app.patch_nodes(&[node], |n| {
        let slate_doc::scene::NodeKind::Image(image) = &mut n.kind else {
            panic!()
        };
        image.agent = slate_doc::scene::PortalNode::unbound_agent("Model", "codex").agent;
        let agent = image.agent.as_mut().unwrap();
        agent.model = Some("native-task-model".into());
        agent.session = atlas_agent::request_id();
    });
    *h.app.agents.prompt_mut(node) = "Generic caller is not a human task".into();
    let before = h.app.doc().scene.node(node).unwrap().clone();
    h.app.send_agent_prompt(node);
    assert!(!h.app.model_task_is_running(node));
    assert_eq!(
        h.app.doc().scene.node(node),
        Some(&before),
        "generic/agent preflight retains command witnesses"
    );
    assert!(h.app.agents.dispatched.is_empty());
    h.app.tab_mut().read_only = true;
    h.app.board_sel = [node].into_iter().collect();
    assert!(!h.app.send_selected_agent_prompt());
    assert!(h.app.agents.dispatched.is_empty() && peer.next().is_none());
    h.app.tab_mut().read_only = false;
    *h.app.agents.prompt_mut(node) = "Human task".into();
    assert!(h.app.send_selected_agent_prompt());
    assert!(
        h.app.model_task_is_running(node),
        "a valid Human Run actually reached task staging before the source edit"
    );
    h.app.patch_nodes(&[node], |n| {
        slate_doc::agent_chat::agent_mut(n).unwrap().model = Some("changed-model".into())
    });
    h.app.pump_model_tasks(&h.ctx);
    assert!(!h.app.model_task_is_running(node));
    assert!(h.app.agents.dispatched.is_empty() && peer.next().is_none());
}
