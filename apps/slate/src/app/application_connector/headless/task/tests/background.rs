//! Controller regressions: accepted work belongs to its workbook, not the visible tab.
use super::*;
use crate::app::application_connector::native::NativeOperation;

fn other_tab(h: &mut Harness, node: NodeId) -> (u64, u64) {
    h.app.sync_agent_doc();
    let original = h.app.tab().id;
    let mut other = crate::app::SlateTab::empty();
    other.doc = h.app.doc().clone(); // Deliberate same NodeId in a different workbook.
    slate_doc::agent_chat::agent_mut(other.doc.scene.node_mut(node).unwrap())
        .unwrap()
        .session = "unrelated-document-session".into();
    let visible = other.id;
    h.app.tabs.push(other);
    h.app.active_tab = h.app.tabs.len() - 1;
    h.app.sync_agent_doc();
    h.app.board_sel = [node].into_iter().collect();
    (original, visible)
}
fn generate_call(h: &Harness, doc: u64, node: NodeId) -> BoardCall {
    call(
        &h.app.connectors.headless.tasks.active[&(doc, node)],
        "board.model.connector.invoke",
        vec![node.0.to_string()],
        json!({"command_id":atlas_connector_runtime::headless_rhino::GENERATE_COMMAND,
            "parameters":{"plan":json!({"units":"meters","operations":[{"kind":"box","id":"box","origin":[0,0,0],"size":[1,1,2]}]}).to_string()}}),
    )
}
fn enqueue(h: &Harness, doc: u64, node: NodeId, call: &BoardCall) {
    let dir = &h.app.connectors.headless.tasks.active[&(doc, node)].dir;
    std::fs::create_dir_all(dir.join("board-calls")).unwrap();
    atlas_ai::agent::atomic_write_json(
        &dir.join("board-calls").join(format!("{}.json", call.id)),
        call,
    )
    .unwrap();
}
fn wait(h: &mut Harness, mut ready: impl FnMut(&mut Harness) -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        h.app.pump_model_tasks(&h.ctx);
        if ready(h) {
            return;
        }
        assert!(Instant::now() < end, "background task did not progress");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn inactive_workbook_admits_call_and_publishes_after_provider_idle_without_touching_visible_alias()
{
    let (mut h, node, peer) = fixture("model-task-background-idle-receipt");
    submit(&mut h, node);
    let (doc, visible) = other_tab(&mut h, node);
    let visible_doc = serde_json::to_vec(h.app.doc()).unwrap();
    let selection = h.app.board_sel.clone();
    let camera = h.app.tab().cam;
    let depth = h.app.tab().journal.undo_depth();
    let request = generate_call(&h, doc, node);
    let permit = h.app.connectors.headless.tasks.active[&(doc, node)]
        .permit
        .clone();
    let mut identity = permit.identity.clone();
    identity.call = request.id.clone();
    assert!(
        h.app
            .native_submit_model_task(
                node,
                &permit.binding,
                identity,
                &permit,
                NativeOperation::Inspect,
                Lease::new()
            )
            .is_err(),
        "visible alias has no scoped permit/session"
    );
    assert!(peer.next().is_none());
    enqueue(&h, doc, node, &request);
    let mut native = None;
    wait(&mut h, |_| {
        native = peer.next();
        native.is_some()
    });
    let native = native.unwrap();
    assert!(native.lease.claim());
    assert!(
        matches!(&native.caller,NativeCaller::Agent{autonomy:false,permit:Some(permit),..} if permit.model_task())
    );
    let provider_dir = h.app.connectors.headless.tasks.active[&(doc, node)]
        .provider_dir
        .clone();
    atlas_ai::agent::atomic_write_json(
        &provider_dir.join("session.json"),
        &atlas_agent::AgentSession {
            request: permit.identity.request.clone(),
            status: atlas_agent::AgentStatus::Idle,
            provider: "codex".into(),
            ..Default::default()
        },
    )
    .unwrap();
    wait(&mut h, |h| {
        h.app.connectors.headless.tasks.active[&(doc, node)].provider_terminal
    });
    assert!(
        permit.live(),
        "provider Idle cannot revoke an already admitted native operation"
    );
    assert!(native.lease.live());
    let forbidden_new = generate_call(&h, doc, node);
    let mut task = h
        .app
        .connectors
        .headless
        .tasks
        .active
        .remove(&(doc, node))
        .unwrap();
    assert!(
        calls::start(&mut h.app, &mut task, &forbidden_new).is_err(),
        "no new commands after provider turn ends"
    );
    h.app
        .connectors
        .headless
        .tasks
        .active
        .insert((doc, node), task);
    let output = atlas_core::index::data_dir()
        .join("headless-rhino/models")
        .join(format!("background-{}.3dm", atlas_agent::request_id()));
    peer.finish_admitted(
        native,
        NativeOutcome::Completed {
            result: NativeResult::Model {
                path: output.clone(),
                size: 32,
                mtime: 1,
                report: json!({"objects":1}),
            },
        },
        "background-result",
    );
    wait(&mut h, |h| {
        h.app
            .tabs
            .iter()
            .find(|tab| tab.id == doc)
            .unwrap()
            .doc
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|agent| agent.generator.model_outputs.len() == 1)
    });
    assert_eq!(h.app.tab().id, visible);
    assert_eq!(serde_json::to_vec(h.app.doc()).unwrap(), visible_doc);
    assert_eq!(h.app.board_sel, selection);
    assert_eq!(h.app.tab().cam.offset, camera.offset);
    assert_eq!(h.app.tab().cam.z, camera.z);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let target = h.app.tabs.iter().find(|tab| tab.id == doc).unwrap();
    assert_eq!(
        target.journal.last_author(),
        Some(&slate_doc::scene::CmdAuthor::Agent(
            permit.identity.agent.clone()
        ))
    );
    assert_eq!(h.app.model_node_info_in(doc, node).unwrap().path, output);
    wait(&mut h, |h| {
        !h.app
            .connectors
            .headless
            .tasks
            .active
            .contains_key(&(doc, node))
    });
    assert!(!permit.live());
    assert!(h.app.presence.agents_empty());
}

#[test]
fn inactive_task_close_or_stop_revokes_pending_native_lease_and_never_publishes_to_alias() {
    for close in [false, true] {
        let (mut h, node, peer) = fixture(if close {
            "model-task-background-close"
        } else {
            "model-task-background-stop"
        });
        submit(&mut h, node);
        let (doc, visible) = other_tab(&mut h, node);
        let request = generate_call(&h, doc, node);
        enqueue(&h, doc, node, &request);
        let mut native = None;
        wait(&mut h, |_| {
            native = peer.next();
            native.is_some()
        });
        let native = native.unwrap();
        let permit = h.app.connectors.headless.tasks.active[&(doc, node)]
            .permit
            .clone();
        if close {
            let index = h.app.tabs.iter().position(|tab| tab.id == doc).unwrap();
            h.app.force_close_tab(index);
        } else {
            h.app.cancel_model_task((doc, node));
        }
        assert!(
            !permit.live() && !native.lease.live(),
            "Stop and close revoke synchronously"
        );
        h.app.pump_model_tasks(&h.ctx);
        assert_eq!(h.app.tab().id, visible);
        assert!(h
            .app
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .unwrap()
            .generator
            .model_outputs
            .is_empty());
    }
}

#[test]
fn inactive_source_words_output_identity_binding_or_readonly_change_rejects_before_native_dispatch()
{
    use slate_doc::scene::*;
    for change in 0..4 {
        let (mut h, node, peer) = fixture(&format!("model-task-background-stale-{change}"));
        let note = h.app.doc_mut().scene.build_node(
            WorldRect::new(-200., 0., 160., 100.),
            NodeKind::Text(TextNode {
                answers: Vec::new(),
                text: "Frozen declared source".into(),
                style: Default::default(),
                family: Typeface::Sans,
                size: 18.,
                color: Rgba::opaque(20, 20, 20),
                align: TextAlign::Left,
                fill: None,
                stroke: Default::default(),
                agent: PortalNode::unbound_agent("Text", "codex-text").agent,
            }),
        );
        let source = h.app.add_nodes(vec![note])[0];
        let t = h
            .app
            .wire_input_ports(h.app.doc().scene.node(node).unwrap())
            .iter()
            .find(|port| port.slot == atlas_agent::InputSlot::Prompt)
            .unwrap()
            .t;
        let wire = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node,
                side: Side::Left,
                t,
            },
        );
        h.app.add_nodes(vec![wire]);
        submit(&mut h, node);
        let (doc, visible) = other_tab(&mut h, node);
        let request = generate_call(&h, doc, node);
        let permit = h.app.connectors.headless.tasks.active[&(doc, node)]
            .permit
            .clone();
        let visible_doc = serde_json::to_vec(h.app.doc()).unwrap();
        let original = h.app.tabs.iter_mut().find(|tab| tab.id == doc).unwrap();
        match change {
            0 => {
                if let NodeKind::Text(text) = &mut original.doc.scene.node_mut(source).unwrap().kind
                {
                    text.text = "Changed source while inactive".into();
                }
            }
            1 => {
                slate_doc::agent_chat::agent_mut(original.doc.scene.node_mut(source).unwrap())
                    .unwrap()
                    .generator
                    .shown_text = Some(slate_doc::text_answers::TextOutputRef {
                    review_id: None,
                    source: SourceUri {
                        locator: "changed-checkpoint/session.json".into(),
                    },
                    response: slate_doc::text_answers::TextAnswerRef {
                        session: "different-output-session".into(),
                        through: 2,
                    },
                });
            }
            2 => {
                h.app
                    .connectors
                    .sessions
                    .get_mut(&(doc, node))
                    .unwrap()
                    .binding
                    .as_mut()
                    .unwrap()
                    .observation += 1
            }
            _ => original.read_only = true,
        }
        enqueue(&h, doc, node, &request);
        h.app.pump_model_tasks(&h.ctx);
        assert!(
            !h.app
                .connectors
                .headless
                .tasks
                .active
                .contains_key(&(doc, node)),
            "stale variant {change}"
        );
        assert!(!permit.live());
        assert!(peer.next().is_none());
        assert_eq!(h.app.tab().id, visible);
        assert_eq!(serde_json::to_vec(h.app.doc()).unwrap(), visible_doc);
        assert!(h
            .app
            .tabs
            .iter()
            .find(|tab| tab.id == doc)
            .unwrap()
            .doc
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .unwrap()
            .generator
            .model_outputs
            .is_empty());
    }
}
