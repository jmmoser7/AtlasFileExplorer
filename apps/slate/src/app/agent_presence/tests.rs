use super::super::tests::Harness;
use super::*;
use serde_json::json;

#[path = "perf_tests.rs"]
mod perf;

#[path = "motion_render_tests.rs"]
mod motion_render;

#[path = "chrome_render_tests.rs"]
mod chrome_render;

#[path = "proposal_chat_tests.rs"]
mod proposal_chat;

#[path = "crop_tests.rs"]
mod crop;

#[test]
fn mentions_resolve_exact_names_and_disambiguate_by_identity() {
    let roster = [("one", "Dot"), ("two", "Dot helper")];
    assert_eq!(
        messaging::resolve_mention("@Dot helper: read this", &roster).unwrap(),
        Some(("two", "read this"))
    );
    assert_eq!(
        messaging::resolve_mention("@Dotted read this", &roster).unwrap(),
        None
    );
    assert!(messaging::resolve_mention("@Dot read", &[("one", "Dot"), ("two", "Dot")]).is_err());
    assert_eq!(
        messaging::resolve_mention("@two: read", &[("one", "Dot"), ("two", "Dot")]).unwrap(),
        Some(("two", "read"))
    );
}

pub(super) fn host() -> Harness {
    let mut h = Harness::new("native_presence");
    h.app.at_home = false;
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h
}
pub(super) fn add(h: &mut Harness, name: &str) -> String {
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: name.into(),
        hat: "none".into(),
        provider: "local".into(),
        retired: false,
    };
    let who = profile.id.clone();
    h.app.join_presence(profile, None).unwrap();
    who
}

#[test]
fn retirement_revokes_work_before_the_departure_and_restore_is_fresh() {
    let mut h = host();
    let who = add(&mut h, "Dot");
    let old = h.app.presence.agents[0].binding.clone();
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Retire {
                id: who.clone(),
                style: "bow".into(),
            },
        )
        .unwrap();
    assert!(h.app.presence.agents[0].grant.is_none());
    assert!(h.app.presence.agents[0].work.is_none());
    assert!(h
        .app
        .presence
        .roster
        .session(&id(&old.session_id).unwrap())
        .is_none());
    h.app
        .apply_presence_action(&h.ctx, chrome::PresenceAction::Restore { id: who })
        .unwrap();
    let fresh = &h.app.presence.agents[0];
    assert_ne!(old.session_id, fresh.binding.session_id);
    assert_ne!(old.grant_id, fresh.binding.grant_id);
    assert!(fresh.request.is_empty());
    assert!(!fresh.grant.as_ref().unwrap().mutation_autonomy);
}

#[test]
fn scopes_do_not_include_source_bytes_and_inactive_boards_cannot_apply() {
    let mut h = host();
    let who = add(&mut h, "Dot");
    let mut a = h.app.presence.agents.remove(0);
    assert!(a.grant.as_ref().unwrap().readable_sources.is_empty());
    assert!(!a.grant.as_ref().unwrap().mutation_autonomy);
    a.request = "request-1".into();
    a.tab += 1;
    let call = BoardCall {
        id: "call-1".into(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "board.objects.create_shape".into(),
            targets: vec![],
            arguments: json!({"x":0,"y":0,"width":40,"height":40}),
        },
    };
    let reply = h.app.presence_call(&mut a, &call).unwrap();
    assert!(!reply.success);
    assert_eq!(a.activity, "blocked");
    assert!(a.status.contains("inactive"));
    assert!(h.app.doc().scene.nodes.is_empty());
    assert!(!who.is_empty());
}

#[test]
fn native_preview_moves_before_commit_and_human_conflict_cancels_it() {
    let mut h = host();
    let who = add(&mut h, "Dot");
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_shape",
            &[],
            &json!({"x":0,"y":0,"width":80,"height":60}),
        )
        .unwrap();
    h.app.commit_scene(prepared.cmds);
    let node = h.app.doc().scene.nodes[0].clone();
    let mut a = h.app.presence.agents.remove(0);
    a.request = "task-1".into();
    a.grant.as_mut().unwrap().mutation_autonomy = true;
    h.app
        .presence
        .roster
        .session_mut(&a.motion)
        .unwrap()
        .begin_request(id(&a.request).unwrap())
        .unwrap();
    let call = BoardCall {
        id: "call-1".into(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "board.objects.move".into(),
            targets: vec![node.id.0.to_string()],
            arguments: json!({"dx":220,"dy":0}),
        },
    };
    assert!(h.app.presence_call(&mut a, &call).is_none());
    h.app.presence.agents.push(a);
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Grid;
    for _ in 0..25 {
        h.app.presence.last_tick = Some(Instant::now() - std::time::Duration::from_millis(50));
        h.app.presence_pump(&h.ctx);
    }
    assert!(
        h.app.presence_preview(node.id).is_none(),
        "a hidden board cannot advance an unwitnessed edit"
    );
    assert_eq!(h.app.doc().scene.node(node.id), Some(&node));
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let mut observed = false;
    for _ in 0..25 {
        h.app.presence.last_tick = Some(Instant::now() - std::time::Duration::from_millis(50));
        h.app.presence_pump(&h.ctx);
        if let Some(p) = h.app.presence_preview(node.id) {
            if p.rect.x > 0.0 && p.rect.x < 220.0 {
                observed = true;
                assert_eq!(h.app.doc().scene.node(node.id).unwrap().rect, node.rect);
                break;
            }
        }
    }
    assert!(
        observed,
        "the object visibly moves while authored state remains unchanged"
    );
    assert!(h.app.presence.holds_node(node.id));
    let board = h.app.active_tab;
    h.app.new_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.presence_pump(&h.ctx);
    assert!(
        !h.app.presence.holds_node(node.id),
        "IDs held on another workbook cannot suppress this board's picks"
    );
    h.app.switch_tab(board);
    h.app.presence_pump(&h.ctx);
    assert!(h.app.presence.holds_node(node.id));
    let mut after = node.clone();
    after.rect.y = 90.0;
    h.app.commit_scene(vec![slate_doc::SceneCmd::Patch {
        before: Box::new(node.clone()),
        after: Box::new(after.clone()),
    }]);
    h.app.presence_pump(&h.ctx);
    assert!(h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == who)
        .unwrap()
        .work
        .is_none());
    assert_eq!(h.app.doc().scene.node(node.id), Some(&after));
}

#[test]
fn incremental_reads_keep_a_global_node_budget_and_reject_stale_pages() {
    let mut h = host();
    add(&mut h, "Dot");
    for x in 0..8 {
        let p = h
            .app
            .prepare_targeted_command(
                "board.objects.create_shape",
                &[],
                &json!({"x":x*50,"y":0,"width":40,"height":40}),
            )
            .unwrap();
        h.app.commit_scene(p.cmds);
    }
    let mut a = h.app.presence.agents.remove(0);
    a.request = "read-task".into();
    h.app
        .presence
        .roster
        .session_mut(&a.motion)
        .unwrap()
        .begin_request(id(&a.request).unwrap())
        .unwrap();
    let call = BoardCall {
        id: "read-one".into(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 4,
        operation: BoardOperation::Read {
            node_ids: vec![],
            offset: 0,
            limit: 4,
        },
    };
    assert!(h.app.presence_call(&mut a, &call).is_none());
    let mut budget = 2;
    assert!(h
        .app
        .advance_presence_read(&mut a, &mut budget, std::time::Duration::ZERO)
        .is_none());
    assert_eq!(budget, 0);
    budget = 2;
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut budget, std::time::Duration::ZERO)
        .unwrap();
    assert_eq!(reply.result["objects"].as_array().unwrap().len(), 4);
    assert_eq!(reply.result["next_offset"], 4);
    let mut call = call;
    call.id = "read-two".into();
    call.sequence = 1;
    assert!(
        h.app.presence_call(&mut a, &call).is_none(),
        "out-of-order unique parallel sequence is allowed"
    );
    let p = h
        .app
        .prepare_targeted_command(
            "board.objects.create_shape",
            &[],
            &json!({"x":500,"y":0,"width":40,"height":40}),
        )
        .unwrap();
    h.app.commit_scene(p.cmds);
    budget = 64;
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut budget, std::time::Duration::ZERO)
        .unwrap();
    assert!(!reply.success);
}

#[test]
fn repeated_helpers_can_exit_without_exhausting_saved_identity_capacity() {
    let mut roster = PresenceRoster::default();
    let parent = AgentProfile {
        id: id("parent").unwrap(),
        name: "Parent".into(),
        hat: None,
        retired: false,
    };
    roster.register(parent).unwrap();
    let parent = roster
        .join(&id("parent").unwrap(), None, point(0.0, 0.0))
        .unwrap();
    for number in 0..140 {
        let who = id(&format!("helper-{number}")).unwrap();
        roster
            .register(AgentProfile {
                id: who.clone(),
                name: "Helper".into(),
                hat: None,
                retired: false,
            })
            .unwrap();
        roster.join(&who, Some(&parent), point(0.0, 0.0)).unwrap();
        roster.retire(&who).unwrap();
        roster.forget_retired_profile(&who).unwrap();
    }
    assert_eq!(roster.profiles().count(), 1);
    assert!(roster.session(&parent).is_some());
}

#[test]
fn immediate_second_send_cannot_replace_a_live_request_binding() {
    let mut h = host();
    let who = add(&mut h, "Dot");
    h.app.send_presence_message(&who, "First task").unwrap();
    let request = h.app.presence.agents[0].request.clone();
    let binding = h.app.presence.agents[0].binding.clone();
    assert!(h.app.send_presence_message(&who, "Second task").is_err());
    assert_eq!(h.app.presence.agents[0].request, request);
    assert_eq!(h.app.presence.agents[0].binding, binding);
    h.app.stop_presence(&who, true).unwrap();
    assert!(h.app.presence.agents[0].grant.is_none());
    assert!(!h.app.presence.agents[0].inflight);
}

#[test]
fn a_carried_frame_keeps_its_contents_at_the_same_visible_offset() {
    let mut h = host();
    let who = add(&mut h, "Dot");
    for (command, args) in [
        (
            "board.objects.create_frame",
            json!({"x":0,"y":0,"width":300,"height":240,"title":"Slide"}),
        ),
        (
            "board.objects.create_shape",
            json!({"x":50,"y":60,"width":40,"height":40}),
        ),
    ] {
        let p = h.app.prepare_targeted_command(command, &[], &args).unwrap();
        h.app.commit_scene(p.cmds);
    }
    let frame = h.app.doc().scene.nodes[0].clone();
    let child = h.app.doc().scene.nodes[1].clone();
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.move",
            &[frame.id.0.to_string()],
            &json!({"dx":200,"dy":100}),
        )
        .unwrap();
    let call = BoardCall {
        id: "carry-frame".into(),
        session: h.app.presence.agents[0].binding.clone(),
        request_id: "frame-task".into(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "board.objects.move".into(),
            targets: vec![frame.id.0.to_string()],
            arguments: json!({"dx":200,"dy":100}),
        },
    };
    let mut work = witness::WitnessedCommand::new(
        call,
        prepared,
        &h.app.doc().scene,
        h.app.board_wire_routing,
    )
    .unwrap();
    work.preview(MotionSample {
        position: point(250.0, 170.0),
        phase: MotionPhase::Work,
        progress: 0.5,
    })
    .unwrap();
    let visible_frame = &work.previews[&frame.id];
    let visible_child = &work.previews[&child.id];
    assert_eq!(visible_frame.rect.x, 100.0);
    assert_eq!(visible_frame.rect.y, 50.0);
    assert_eq!(
        visible_child.rect.x - visible_frame.rect.x,
        child.rect.x - frame.rect.x
    );
    assert_eq!(
        visible_child.rect.y - visible_frame.rect.y,
        child.rect.y - frame.rect.y
    );
    assert_eq!(h.app.doc().scene.node(child.id), Some(&child));
    work.complete_step();
    assert_eq!(
        work.index,
        work.steps.len(),
        "contents travel with their frame rather than as a second action"
    );
    h.app.stop_presence(&who, true).unwrap();
}

#[test]
fn a_stale_targeted_read_releases_motion_but_keeps_the_task_binding() {
    let mut h = host();
    add(&mut h, "Dot");
    let p = h
        .app
        .prepare_targeted_command(
            "board.objects.create_shape",
            &[],
            &json!({"x":300,"y":0,"width":40,"height":40}),
        )
        .unwrap();
    h.app.commit_scene(p.cmds);
    let target = h.app.doc().scene.nodes[0].id;
    let mut a = h.app.presence.agents.remove(0);
    a.request = "read-again".into();
    h.app
        .presence
        .roster
        .session_mut(&a.motion)
        .unwrap()
        .begin_request(id(&a.request).unwrap())
        .unwrap();
    let mut call = BoardCall {
        id: "stale-read".into(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Read {
            node_ids: vec![target.0.to_string()],
            offset: 0,
            limit: 1,
        },
    };
    assert!(h.app.presence_call(&mut a, &call).is_none());
    let p = h
        .app
        .prepare_targeted_command(
            "board.objects.move",
            &[target.0.to_string()],
            &json!({"dx":10,"dy":0}),
        )
        .unwrap();
    h.app.commit_scene(p.cmds);
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut 64, std::time::Duration::ZERO)
        .unwrap();
    assert!(!reply.success);
    let motion = h.app.presence.roster.session(&a.motion).unwrap();
    assert!(motion.current_plan().is_none());
    assert_eq!(motion.queued_len(), 0);
    assert_eq!(motion.request_id(), Some(&id(&a.request).unwrap()));
    call.id = "fresh-read".into();
    call.sequence = 1;
    assert!(h.app.presence_call(&mut a, &call).is_none());
    let mut completed = false;
    for _ in 0..20 {
        if let Some(reply) =
            h.app
                .advance_presence_read(&mut a, &mut 64, std::time::Duration::from_millis(50))
        {
            assert!(reply.success);
            completed = true;
            break;
        }
    }
    assert!(completed, "the same task can read again after a stale page");
    call.id = "missing-read".into();
    call.sequence = 2;
    call.operation = BoardOperation::Read {
        node_ids: vec!["99999999".into()],
        offset: 0,
        limit: 1,
    };
    assert!(h.app.presence_call(&mut a, &call).is_none());
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut 64, std::time::Duration::ZERO)
        .unwrap();
    assert!(
        !reply.success,
        "a missing explicit ID must not return an empty page with the same next offset"
    );
    assert!(reply.result.get("next_offset").is_none());
}
