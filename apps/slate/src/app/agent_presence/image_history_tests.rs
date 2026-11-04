//! Registered routes supplement the ordinary pointer/Send history regression.
use super::*;
use serde_json::Value;
#[path = "image_history_admission_tests.rs"]
mod admission;
use slate_doc::{NodeId, SceneCmd};

fn setup() -> (Harness, String, NodeId, NodeId) {
    let mut h = host();
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("history.slate"));
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_generator",
            &[],
            &json!({"provider":"comfy","x":0,"y":0,"width":420,"height":390}),
        )
        .unwrap();
    h.app.commit_scene(prepared.cmds);
    let source = h.app.doc().scene.nodes[0].id;
    h.app.patch_nodes(&[source], |n| {
        let a = slate_doc::agent_chat::agent_mut(n).unwrap();
        a.generator.preserve_history = Some(true);
        a.generator.inherited_turn_count = Some(0);
    });
    h.app.set_agent_session_for_test(
        source,
        AgentSession {
            turns: vec![
                atlas_agent::AgentTurn {
                    role: "user".into(),
                    text: "A🦊é界B".into(),
                    at: 1,
                },
                atlas_agent::AgentTurn {
                    role: "assistant".into(),
                    text: "actual provider reply".into(),
                    at: 2,
                },
            ],
            ..Default::default()
        },
    );
    let before = h.app.doc().scene.clone();
    let planned = h
        .app
        .prepare_targeted_command(
            "board.image.history",
            &[source.0.to_string()],
            &json!({"action":"downstream","x":700,"y":30}),
        )
        .unwrap();
    assert_eq!(h.app.doc().scene, before, "scratch planning is inert");
    let child=planned.created_ids().into_iter().find(|id|planned.cmds.iter().any(|cmd|matches!(cmd,SceneCmd::Add{node,..} if node.id==*id && matches!(node.kind,slate_doc::NodeKind::Image(_))))).unwrap();
    assert!(h.app.dispatch_targeted_human("board.image.history",Some(&json!({"targets":[source.0.to_string()],"arguments":{"action":"downstream","x":700,"y":30}}).to_string())));
    let mut session = h.app.agents.session(source).unwrap().clone();
    session.turns.push(atlas_agent::AgentTurn {
        role: "user".into(),
        text: "A later actual public request".into(),
        at: 3,
    });
    session.turns.push(atlas_agent::AgentTurn {
        role: "tool".into(),
        text: "PRIVATE TOOL CONTENT".into(),
        at: 4,
    });
    h.app.set_agent_session_for_test(source, session);
    let who = add(&mut h, "History collaborator");
    let endpoints = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter_map(slate_doc::agent_chat::agent)
        .map(|a| a.session.clone())
        .collect();
    let nodes = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .map(|n| n.id.0.to_string())
        .collect();
    let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
    grant.conversation_ids = endpoints;
    grant.readable_board = false;
    grant.readable_nodes = nodes;
    (h, who, source, child)
}
fn invoke(h: &mut Harness, target: NodeId, args: Value) -> Option<BoardReply> {
    let mut a = h.app.presence.agents.remove(0);
    if a.request.is_empty() {
        a.request = atlas_agent::request_id();
        h.app
            .presence
            .roster
            .session_mut(&a.motion)
            .unwrap()
            .begin_request(id(&a.request).unwrap())
            .unwrap();
    }
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: a.calls as u64,
        operation: BoardOperation::Invoke {
            command: "board.image.history".into(),
            targets: vec![target.0.to_string()],
            arguments: args,
        },
    };
    let reply = h.app.presence_call(&mut a, &call);
    h.app.presence.agents.insert(0, a);
    reply
}
fn read(h: &mut Harness, target: NodeId, args: Value) -> BoardReply {
    if let Some(reply) = invoke(h, target, args) {
        return reply;
    }
    for _ in 0..40 {
        let mut a = h.app.presence.agents.remove(0);
        let reply =
            h.app
                .advance_presence_read(&mut a, &mut 4, std::time::Duration::from_millis(50));
        h.app.presence.agents.insert(0, a);
        if let Some(reply) = reply {
            return reply;
        }
    }
    panic!("bounded public read did not finish");
}
#[test]
fn history_registered_scope_precedes_reads_and_rechecks_revocation_and_stale_sources() {
    for mode in ["endpoint", "source", "locked"] {
        let (mut h, _, source, child) = setup();
        let before = h.app.doc().scene.clone();
        match mode {
            "endpoint" => h.app.presence.agents[0]
                .grant
                .as_mut()
                .unwrap()
                .conversation_ids
                .clear(),
            "source" => h.app.presence.agents[0]
                .grant
                .as_mut()
                .unwrap()
                .readable_nodes
                .retain(|id| id != &source.0.to_string()),
            _ => {
                h.app.patch_nodes(&[source], |n| n.locked = true);
            }
        }
        let reply = invoke(&mut h, child, json!({"action":"refresh"}))
            .expect("denial is immediate, before history preparation");
        assert!(!reply.success);
        assert!(h.app.presence.agents[0].work.is_none());
        if mode != "locked" {
            assert_eq!(h.app.doc().scene, before);
        }
    }
    let (mut h, who, _source, child) = setup();
    let old_pin =
        slate_doc::agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child).unwrap();
    let proposal = proposal_support::stage_command(
        &mut h,
        &who,
        "board.image.history",
        &[child],
        json!({"action":"refresh"}),
    );
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .conversation_ids
        .clear();
    h.app.accept_stage_proposal(&proposal);
    proposal_support::finish(&mut h, &who);
    assert_eq!(
        slate_doc::agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child).unwrap(),
        old_pin
    );
    let (mut h, who, source, child) = setup();
    let proposal = proposal_support::stage_command(
        &mut h,
        &who,
        "board.image.history",
        &[child],
        json!({"action":"refresh"}),
    );
    h.app.patch_nodes(&[source], |n| n.rect.x += 1.0);
    let before = h.app.doc().scene.clone();
    h.app.accept_stage_proposal(&proposal);
    proposal_support::finish(&mut h, &who);
    assert_eq!(h.app.doc().scene, before);
}
#[test]
fn history_staged_cut_is_inert_rejection_keeps_work_acceptance_is_one_undo_and_fences_old_session()
{
    let (mut h, who, _, child) = setup();
    let history = slate_doc::agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child)
        .unwrap()
        .turns;
    let request:atlas_agent::AgentRequest=serde_json::from_value(json!({"id":"actual-pending-cut-request","prompt":"Pending question","at":5,"image":{},"image_history":true,"image_history_prefix":history.len(),"history":history})).unwrap();
    h.app.enqueue_generation(child, request.clone());
    for _ in 0..8 {
        h.frame();
    }
    assert_eq!(h.app.agents.request_of(child), Some(request.id.as_str()));
    assert!(h
        .app
        .agents
        .dispatched
        .iter()
        .any(|(id, r)| *id == child && r.id == request.id));
    h.app.set_agent_session_for_test(
        child,
        AgentSession {
            request: request.id.clone(),
            status: atlas_agent::AgentStatus::Thinking,
            ..Default::default()
        },
    );
    let before = h.app.doc().scene.clone();
    let old = slate_doc::agent_chat::agent(before.node(child).unwrap())
        .unwrap()
        .session
        .clone();
    let depth = h.app.tab().journal.undo_depth();
    let proposal = proposal_support::stage_command(
        &mut h,
        &who,
        "board.image.history",
        &[child],
        json!({"action":"cut"}),
    );
    assert_eq!(h.app.doc().scene, before);
    assert!(
        h.app.agent_generation_dispatched(child),
        "staging does not cancel work"
    );
    assert!(!h.app.history_request_cancelled_for_test(child, &request.id));
    h.app.reject_stage_proposal(&proposal);
    assert_eq!(h.app.doc().scene, before);
    assert!(h.app.agent_generation_dispatched(child));
    let proposal = proposal_support::stage_command(
        &mut h,
        &who,
        "board.image.history",
        &[child],
        json!({"action":"cut"}),
    );
    h.app.accept_stage_proposal(&proposal);
    proposal_support::finish(&mut h, &who);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(!h.app.agent_generation_dispatched(child));
    assert!(
        h.app.history_request_cancelled_for_test(child, &request.id),
        "accepted cut fences the actually admitted old request"
    );
    let new = slate_doc::agent_chat::agent(h.app.doc().scene.node(child).unwrap()).unwrap();
    assert_ne!(new.session, old);
    assert_eq!(new.generator.inherited_turn_count, Some(0));
    assert!(h.app.agents.session(child).is_none());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene, before);
    assert!(
        !h.app.agent_generation_dispatched(child),
        "Undo restores intent, never inference"
    );
}
#[test]
fn history_view_is_public_bounded_unicode_and_revocation_fenced_without_ui_or_doc_mutation() {
    let (mut h, _, source, _) = setup();
    let before = h.app.doc().scene.clone();
    let selected = h.app.board_sel.clone();
    let depth = h.app.tab().journal.undo_depth();
    let runs = h.app.agents.dispatched.len();
    let first = read(&mut h, source, json!({"action":"view","limit":4}));
    assert!(first.success);
    assert_eq!(first.result["messages"][0]["text"], "A");
    assert!(first.result["public_only"].as_bool().unwrap());
    let next = read(
        &mut h,
        source,
        json!({"action":"view","cursor":first.result["next_cursor"],"text_cursor":first.result["next_text_cursor"],"generation":first.result["generation"],"limit":4}),
    );
    assert!(next.success);
    assert_eq!(next.result["messages"][0]["text"], "🦊");
    assert_eq!(h.app.doc().scene, before);
    assert_eq!(h.app.board_sel, selected);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.agents.dispatched.len(), runs);
    assert!(invoke(&mut h, source, json!({"action":"view"})).is_none());
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .conversation_ids
        .clear();
    let mut actor = h.app.presence.agents.remove(0);
    let reply = h
        .app
        .advance_presence_read(&mut actor, &mut 4, std::time::Duration::from_millis(50))
        .unwrap();
    assert!(!reply.success);
    h.app.presence.agents.insert(0, actor);
}

#[test]
fn history_accepted_refresh_keeps_captured_pin_reconnects_and_creates_closed_downstream_without_run(
) {
    let (mut h, who, source, child) = setup();
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let proposal = proposal_support::stage_command(
        &mut h,
        &who,
        "board.image.history",
        &[child],
        json!({"action":"refresh"}),
    );
    let mut session = h.app.agents.session(source).unwrap().clone();
    session.turns.push(atlas_agent::AgentTurn {
        role: "user".into(),
        text: "Parent completed later".into(),
        at: 5,
    });
    h.app.set_agent_session_for_test(source, session);
    h.app.accept_stage_proposal(&proposal);
    proposal_support::finish(&mut h, &who);
    let pin =
        slate_doc::agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child).unwrap();
    assert_eq!(pin.turns.len(), 3);
    assert!(!serde_json::to_string(&pin)
        .unwrap()
        .contains("PRIVATE TOOL"));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene, before);
    h.app.board_redo();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.image.history"),
        Some(json!({"node":child.0,"action":"cut","incoming":true}).to_string())
    ));
    assert!(
        slate_doc::agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child).is_none()
    );
    // Cut creates a new endpoint; explicit human endpoint permission is needed
    // for a subsequent collaborator context operation, never inferred from Pin.
    let fresh = slate_doc::agent_chat::agent(h.app.doc().scene.node(child).unwrap())
        .unwrap()
        .session
        .clone();
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .conversation_ids
        .push(fresh);
    let proposal = proposal_support::stage_command(
        &mut h,
        &who,
        "board.image.history",
        &[child],
        json!({"action":"reconnect"}),
    );
    h.app.accept_stage_proposal(&proposal);
    proposal_support::finish(&mut h, &who);
    assert_eq!(
        slate_doc::agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child)
            .unwrap()
            .turns
            .len(),
        4
    );
    let before_ids = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .map(|n| n.id)
        .collect::<Vec<_>>();
    let runs = h.app.agents.dispatched.len();
    let proposal = proposal_support::stage_command(
        &mut h,
        &who,
        "board.image.history",
        &[source],
        json!({"action":"downstream","x":1200,"y":40}),
    );
    h.app.accept_stage_proposal(&proposal);
    proposal_support::finish(&mut h, &who);
    let new = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| !before_ids.contains(&n.id) && matches!(n.kind, slate_doc::NodeKind::Image(_)))
        .unwrap();
    assert_eq!((new.rect.x, new.rect.y), (1200.0, 40.0));
    assert!(!h.app.board_sel.contains(&new.id));
    let agent = slate_doc::agent_chat::agent(new).unwrap();
    assert!(agent.seed.is_none() && agent.bundle.is_none());
    assert_eq!(agent.generator.inherited_turn_count, Some(0));
    assert_eq!(h.app.agents.dispatched.len(), runs);
}
