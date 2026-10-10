//! Hover chips, trains, and bundles.

use super::test_support::*;
use super::*;

#[test]
fn hovering_a_generator_shows_what_it_reads_and_can_do() {
    let (mut h, id, _) = generator_with_note("gen_hover", "a nice public park");
    wire_model(&mut h, id);
    h.frame();
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    let body = h.app.board_xf().rect_w2s(r);
    // Choosing a program focuses the portal, which reveals its controls.
    h.app.agent_blur();
    h.app.board_sel.clear();
    let rest = painted_text(&mut h, None);
    assert!(
        !rest
            .iter()
            .any(|t| t.starts_with("Prompt ·") || t == "Render"),
        "quiet at rest: {rest:?}"
    );
    assert!(
        !rest.iter().any(|t| t.contains("Connect")),
        "a wired generator asks for nothing: {rest:?}"
    );
    let hover = painted_text(&mut h, Some(body.center()));
    for expected in [
        "Prompt · a nice public park",
        "Geometry · pavilion.3dm",
        "Render",
    ] {
        assert!(
            hover.iter().any(|t| t == expected),
            "missing {expected:?} in {hover:?}"
        );
    }
    // Model and Live moved to the Agent squircle.
    assert!(
        !hover.iter().any(|t| t == "ComfyUI · Auto" || t == "Live"),
        "no model chip or Live toggle on the picture: {hover:?}"
    );

    // Live swaps the primary action for Keep.
    h.app.set_generator_live(id, true);
    let live = painted_text(&mut h, Some(body.center()));
    assert!(live.iter().any(|t| t == "Keep"), "{live:?}");
}

#[test]
fn live_follows_a_prompt_while_it_is_typed() {
    let (mut h, id, note) = generator_with_note("gen_typing", "morning light");
    h.app.set_generator_live(id, true);
    h.app.pump_live_generators();
    h.app.agents.awaiting.remove(&id);
    h.app.text_edit = Some((note, "storm clouds".into()));
    h.app.pump_live_generators();
    assert_eq!(h.app.agents.dispatched.len(), 1, "typing waits for a pause");
    if let Some((_, at)) = h.app.agents.live_settle.get_mut(&id) {
        *at -= LIVE_TYPING_SETTLE;
    }
    h.app.pump_live_generators();
    assert_eq!(
        h.app.agents.dispatched.len(),
        2,
        "a pause renders the draft"
    );
    assert_eq!(h.app.agents.dispatched[1].1.prompt, "storm clouds");
}

#[test]
fn live_rerenders_when_a_wired_input_changes() {
    let (mut h, id, note) = generator_with_note("gen_stream", "morning light");
    h.app.set_generator_live(id, true);
    h.app.pump_live_generators();
    assert_eq!(h.app.agents.dispatched.len(), 1, "live renders at once");
    assert_eq!(h.app.agents.dispatched[0].1.prompt, "morning light");
    h.app.agents.awaiting.remove(&id);
    h.app.pump_live_generators();
    assert_eq!(
        h.app.agents.dispatched.len(),
        1,
        "unchanged inputs do not rerun"
    );
    h.app.patch_nodes(&[note], |n| {
        if let NodeKind::Text(t) = &mut n.kind {
            t.text = "evening light".into();
        }
    });
    h.app.pump_live_generators();
    assert_eq!(h.app.agents.dispatched.len(), 2, "an edited prompt reruns");
    assert_eq!(h.app.agents.dispatched[1].1.prompt, "evening light");
    let (a, b) = (&h.app.agents.dispatched[0].1, &h.app.agents.dispatched[1].1);
    assert_eq!(
        a.image.as_ref().and_then(|i| i.seed),
        b.image.as_ref().and_then(|i| i.seed)
    );

    // A camera move on a wired model changes the signature live watches.
    let model = wire_model(&mut h, id);
    let before = h.app.model_pose_hash(model).unwrap();
    h.app.patch_nodes(&[model], |n| {
        if let NodeKind::Image(img) = &mut n.kind {
            img.model.yaw += 0.4;
        }
    });
    assert_ne!(h.app.model_pose_hash(model), Some(before));
    h.app.agents.awaiting.remove(&id);
    assert!(!h.app.pointer_over_image_album(&h.app.board_xf(), None));
}

#[test]
fn a_wired_model_renders_and_names_a_missing_gpu() {
    let (mut h, id, _) = generator_with_note("gen_model", "night, warm lights");
    let model_id = wire_model(&mut h, id);
    let view = h.app.generator_view(id);
    assert_eq!(view.task().action(), "Render");
    assert_eq!(view.geometry(), Some(model_id));
    assert!(view.inputs.iter().any(|i| i.role == InputRole::Prompt));
    h.app.queue_generation(id);
    assert!(
        h.app.agents.dispatched.is_empty(),
        "no run starts without a captured view"
    );
    assert!(matches!(
        h.app.agents.awaiting.get(&id),
        Some(AgentAwait::Failed { .. })
    ));
}

#[test]
fn cursor_dropdown_journals_a_model_without_replacing_the_session() {
    let mut h = super::super::tests::Harness::new("cursor_model");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "cursor");
    h.app.board_sel = [id].into_iter().collect();
    let session = h.app.agent_session_for(id).unwrap().0;
    h.app.agents.models.insert(
        "cursor".into(),
        vec![atlas_ai::agent::AgentModel {
            id: "composer-2.5".into(),
            name: "Composer 2.5".into(),
        }],
    );
    assert!(h.app.agent_set_model("composer-2.5"));
    let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(a.provider, "cursor");
    assert_eq!(a.model.as_deref(), Some("composer-2.5"));
    assert_eq!(a.session, session);
    assert!(h.app.agent_set_model(""));
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .model
            .is_none()
    );
}

#[test]
fn full_access_is_a_local_grant_per_conversation_and_never_journaled() {
    let mut h = super::super::tests::Harness::new("agent_full_access");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "cursor");
    h.app.board_sel = [id].into_iter().collect();
    let session = h.app.agent_session_for(id).unwrap().0;
    let scene = h.app.doc().scene.clone();
    let store = std::env::temp_dir().join(format!(
        "slate_full_access_{}_{}.json",
        std::process::id(),
        session
    ));
    h.app.agents.access_path = Some(store.clone());
    assert!(!h.app.agent_full_access(&session));
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.agent.full_access"),
        None
    ));
    assert!(h.app.agent_full_access(&session));
    assert!(atlas_ai::access::granted_in(&store, &session));
    assert_eq!(
        h.app.doc().scene,
        scene,
        "the grant never enters the workbook"
    );
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.agent.full_access"),
        None
    ));
    assert!(!h.app.agent_full_access(&session));
    assert!(!atlas_ai::access::granted_in(&store, &session));
    let _ = std::fs::remove_file(store);
}

#[test]
fn background_refresh_keeps_history_and_model_choice_is_journaled() {
    let mut h = super::super::tests::Harness::new("agent_refresh");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "codex");
    h.app.agents.project_picker = None;
    h.app.agents.models_started.insert("codex".into());
    h.app.agents.models.insert(
        "codex".into(),
        vec![atlas_ai::agent::AgentModel {
            id: "gpt-6-astra".into(),
            name: "Astra".into(),
        }],
    );
    h.app.board_sel = [id].into_iter().collect();
    assert!(h.app.agent_set_model("gpt-6-astra"));
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .model
            .as_deref(),
        Some("gpt-6-astra")
    );
    let state:AgentSession=serde_json::from_value(serde_json::json!({"conversation":"test","provider":"codex","turns":[{"role":"assistant","text":"Keep this answer","at":0}]})).unwrap();
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(state.clone()));
    let session = h.app.agent_session_for(id).unwrap().0;
    for result in [Ok(state), Err("offline".into())] {
        let (tx, rx) = unbounded();
        tx.send((id, session.clone(), result)).unwrap();
        h.app.agents.connection_rx = Some(rx);
        h.app.agents.connection_pending = Some(id);
        h.app.agents.connection_background = true;
        let epoch = h.app.agents.output_epoch;
        h.app.pump_agent_connection(&h.ctx);
        assert_eq!(h.app.visible_agent_turns(id)[0].text, "Keep this answer");
        assert_eq!(h.app.agents.output_epoch, epoch);
        assert!(h.app.agents.awaiting.is_empty());
    }
    let r = h.app.doc().scene.node(id).unwrap().rect;
    let xf = h.app.board_xf();
    let mid = xf.w2s(Pos2::new(r.x, r.y + r.h * 0.5));
    let right = xf.w2s(Pos2::new(r.x + r.w, r.y + r.h * 0.5));
    assert_eq!(
        h.app.agent_artifact_at(mid, &xf),
        None,
        "a train card with no referenced files has no left input dot"
    );
    assert_eq!(
        h.app.agent_artifact_at(right, &xf),
        None,
        "an untouched train card has no edited-document handle"
    );
    h.app.agents.project_picker = Some(id);
    assert_eq!(h.app.agent_artifact_at(mid, &xf), None);
    h.app.agents.project_picker = None;
    assert_eq!(h.app.agent_output_at(mid, &xf), None);
    let snapshot = h.app.agent_input_snapshot(id).unwrap();
    assert!(snapshot.context.is_empty());
    assert!(snapshot.wired.is_empty());
}

#[test]
fn coding_train_keeps_identity_and_artifacts_open_only_on_request() {
    let mut h = super::super::tests::Harness::new("coding_artifacts");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "codex");
    h.app.agents.project_picker = None;
    h.app
        .set_agent_channel(root, Some("provider-conversation".into()));
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.chat.train = true;
            a.chat.linear = false; // Older workbooks did not persist the coding-stream flag.
            a.chat.detail = slate_doc::agent_chat::Detail::Full;
            a.bundle = None;
        }
    });
    h.app.board_sel = [root].into_iter().collect();
    let session = h.app.agent_session_for(root).unwrap();
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.agent_spawn_command(Some("[400,0]")));
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
    let draft = *h.app.board_sel.iter().next().unwrap();
    assert_eq!(h.app.agent_session_for(draft).unwrap(), session);
    h.app.board_sel = [root].into_iter().collect();
    assert!(
        !h.app.agent_spawn_command(Some("[400,200]")),
        "coding handles cannot fork"
    );
    let (reply, replay) = h
        .app
        .prepare_agent_train_send(draft, std::path::Path::new("test-workspace"))
        .unwrap();
    assert!(replay.is_empty());
    assert_eq!(h.app.agent_session_for(reply).unwrap(), session);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(reply).unwrap())
            .unwrap()
            .channel
            .as_deref(),
        Some("provider-conversation")
    );
    assert!(h
        .app
        .prepare_agent_train_send(root, std::path::Path::new("test-workspace"))
        .is_none());
    let state:AgentSession=serde_json::from_value(serde_json::json!({"provider":"codex","conversation":"provider-conversation","artifacts":[
        {"id":"read","turn":1,"kind":"web","source":"https://example.com/reference","title":"Reference"},
        {"id":"edit","turn":1,"kind":"modified","source":"C:/fixture/main.rs","title":"main.rs"}
    ]})).unwrap();
    h.app
        .agents
        .sessions
        .insert(reply, std::sync::Arc::new(state));
    assert_eq!(h.app.agent_artifacts(reply, false).len(), 1);
    assert_eq!(h.app.agent_artifacts(reply, true).len(), 1);
    let before = h.app.doc().scene.nodes.len();
    h.app
        .toggle_agent_artifacts(Some(&format!("[{},true]", reply.0)));
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before,
        "hover/list does not spawn portals"
    );
    assert!(h
        .app
        .open_agent_artifact(Some(&serde_json::to_string(&(reply.0, "edit")).unwrap())));
    assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
    let atlas = &h.app.doc().scene.nodes[before];
    let NodeKind::Portal(p) = &atlas.kind else {
        panic!("expected atlas portal")
    };
    assert_eq!(p.kind, PortalKind::FileAtlas);
    assert_eq!(p.atlas.files, vec!["main.rs"]);
    let NodeKind::Connector(wire) = &h.app.doc().scene.nodes[before + 1].kind else {
        panic!("expected wire")
    };
    assert!(
        wire.binding.is_none(),
        "provenance is not implicit model input"
    );
}

#[test]
fn agent_nested_bundles_restore_one_level_with_horizontal_spacing() {
    let mut h = super::super::tests::Harness::new("nested_chat_bundle");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let mut ids = Vec::new();
    for i in 0..4 {
        let mut portal = PortalNode::unbound_agent("Nested", "codex");
        let a = portal.agent.as_mut().unwrap();
        a.chat.train = true;
        a.chat.start = i;
        a.chat.end = Some(i + 1);
        a.chat.parent = ids.last().copied();
        let n = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(0.0, 0.0, 200.0, 100.0),
            NodeKind::Portal(portal),
        );
        ids.push(n.id);
        h.app.add_nodes(vec![n]);
    }
    for pair in [&ids[..2], &ids[2..]] {
        h.app.board_sel = pair.iter().copied().collect();
        assert!(h.app.agent_bundle_selection());
    }
    h.app.board_sel = [ids[1], ids[3]].into_iter().collect();
    assert!(h.app.agent_bundle_selection());
    assert_eq!(
        h.app.doc().scene.nodes.iter().filter(|n| !n.hidden).count(),
        1
    );
    let encoded = serde_json::to_string(&h.app.doc().scene).unwrap();
    let decoded: slate_doc::scene::Scene = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, h.app.doc().scene);
    assert!(h.app.agent_expand_bundle());
    for i in [1, 3] {
        let n = h.app.doc().scene.node(ids[i]).unwrap();
        assert!(!n.hidden);
        assert_eq!(
            slate_doc::agent_chat::agent(n).unwrap().chat.bundled,
            vec![ids[i - 1]]
        );
        assert!(h.app.doc().scene.node(ids[i - 1]).unwrap().hidden);
    }
    let left = h.app.doc().scene.node(ids[1]).unwrap().rect;
    let right = h.app.doc().scene.node(ids[3]).unwrap().rect;
    assert!(right.x >= left.x + left.w + slate_doc::agent_chat::CARD_GAP);
    assert!(h.app.agent_expand_bundle());
    let left = h.app.doc().scene.node(ids[2]).unwrap().rect;
    let right = h.app.doc().scene.node(ids[3]).unwrap().rect;
    assert!(right.x >= left.x + left.w + slate_doc::agent_chat::CARD_GAP);
    assert!(h.app.doc().scene.node(ids[0]).unwrap().hidden);
}

#[test]
fn agent_full_window_output_creates_two_branches_and_sends_in_place() {
    for train in [false, true] {
        let mut h = super::super::tests::Harness::new("full_chat_fork");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let root = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(root, "local");
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.bundle = None;
                a.chat.train = train;
                a.chat.detail = slate_doc::agent_chat::Detail::Full;
            }
        });
        h.app.board_sel = [root].into_iter().collect();
        assert!(h.app.agent_spawn_command(Some("[400,0]")));
        let children: Vec<_> = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(root))
            })
            .map(|n| n.id)
            .collect();
        assert_eq!(children.len(), 2);
        let a = h.app.doc().scene.node(children[0]).unwrap().rect;
        let b = h.app.doc().scene.node(children[1]).unwrap().rect;
        assert!(b.y >= a.y + a.h);
        let count = h.app.doc().scene.nodes.len();
        let (recipient, history) = h
            .app
            .prepare_agent_train_send(children[0], std::path::Path::new("C:/sample"))
            .unwrap();
        assert_eq!(recipient, children[0]);
        assert!(history.is_empty());
        assert_eq!(h.app.doc().scene.nodes.len(), count);
        assert!(
            !slate_doc::agent_chat::agent(h.app.doc().scene.node(recipient).unwrap())
                .unwrap()
                .chat
                .draft
        );
    }
}
