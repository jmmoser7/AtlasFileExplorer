//! Agent chat-train forks, checkpoints, and the composer card.

use super::*;

#[test]
fn agent_conversation_projection_rename_and_undo_preserve_forks() {
    let mut h = agent_board("train_projection");
    let mut ids = Vec::new();
    for (i, parent) in [None, Some(0), Some(1), Some(2), Some(1), Some(4)]
        .into_iter()
        .enumerate()
    {
        let mut p = slate_doc::PortalNode::unbound_agent("Courtyard", "codex");
        let a = p.agent.as_mut().unwrap();
        a.session = "projection-test".into();
        a.chat = slate_doc::agent_chat::ChatView {
            train: true,
            parent: parent.map(|j| ids[j]),
            start: i,
            end: Some(i + 1),
            ..Default::default()
        };
        let n = h.app.doc_mut().scene.build_node(
            WorldRect::new(i as f32 * 416.0, 0.0, 320.0, 200.0),
            NodeKind::Portal(p),
        );
        ids.push(n.id);
        h.app.add_nodes(vec![n]);
    }
    h.app.board_sel.clear();
    h.app.board_sel.insert(ids[3]);
    assert!(h.app.agent_rename("Garden study"));
    for id in &ids {
        let NodeKind::Portal(p) = &h.app.doc().scene.node(*id).unwrap().kind else {
            panic!()
        };
        assert_eq!(
            p.title,
            if [ids[2], ids[3]].contains(id) {
                "Garden study"
            } else {
                "Courtyard"
            }
        );
    }
    assert!(h.app.agent_show_chat());
    assert!(
        !h.app.agent_expand_bundle(),
        "collapsed windows are not bundles"
    );
    let visible: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| !n.hidden)
        .map(|n| n.id)
        .collect();
    assert_eq!(visible, vec![ids[1], ids[3], ids[5]]);
    let upper = h.app.doc().scene.node(ids[3]).unwrap();
    let lower = h.app.doc().scene.node(ids[5]).unwrap();
    assert!(lower.rect.y - upper.rect.y >= upper.rect.h);
    assert_eq!(
        slate_doc::agent_chat::visible_parent(&h.app.doc().scene, upper),
        Some(ids[1])
    );
    h.app.agent_show_train();
    assert_eq!(h.app.doc().scene.nodes.len(), 6);
    assert!(h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .all(|n| !n.hidden && slate_doc::agent_chat::agent(n).unwrap().chat.train));
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(ids[4]).unwrap())
            .unwrap()
            .chat
            .parent,
        Some(ids[1])
    );
    h.app.board_undo();
    assert_eq!(
        h.app.doc().scene.nodes.iter().filter(|n| !n.hidden).count(),
        3
    );
}

#[test]
fn agent_expanding_window_retains_existing_branch_checkpoint() {
    let mut h = agent_board("window_subdivision");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "codex");
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.chat.end = Some(3);
            a.bundle = None;
        }
    });
    let original = h.app.doc().scene.node(root).unwrap().clone();
    let mut child = h.app.doc_mut().scene.build_duplicate(&original, 416.0, 0.0);
    let child_id = child.id;
    if let NodeKind::Portal(p) = &mut child.kind {
        let a = p.agent.as_mut().unwrap();
        a.chat.parent = Some(root);
        a.chat.start = 3;
        a.chat.end = Some(5);
    }
    h.app.add_nodes(vec![child]);
    h.app.board_sel.clear();
    h.app.board_sel.insert(child_id);
    h.app.agent_show_train();
    assert_eq!(h.app.doc().scene.nodes.len(), 5);
    assert_eq!(
        slate_doc::agent_chat::conversation(&h.app.doc().scene, child_id).len(),
        5
    );
    let branch = slate_doc::agent_chat::agent(h.app.doc().scene.node(child_id).unwrap()).unwrap();
    assert_eq!(branch.chat.start, 4);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(branch.chat.parent.unwrap()).unwrap())
            .unwrap()
            .chat
            .parent,
        Some(root)
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
}

#[test]
fn agent_output_draft_is_consumed_without_an_extra_empty_car() {
    let mut h = agent_board("agent_draft");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "local");
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.chat.train = true;
            a.chat.detail = slate_doc::agent_chat::Detail::Summary;
            a.bundle = None;
        }
    });
    h.app.board_sel.clear();
    h.app.board_sel.insert(root);
    h.app.agent_spawn_command(Some("[480,120]"));
    let draft = *h.app.board_sel.iter().next().unwrap();
    assert_ne!(draft, root);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let (reply, history) = h
        .app
        .prepare_agent_train_send(draft, &std::env::temp_dir())
        .unwrap();
    assert!(history.is_empty());
    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    let user = h.app.doc().scene.node(draft).unwrap();
    assert_eq!([user.rect.x, user.rect.y], [480.0, 120.0]);
    let a = slate_doc::agent_chat::agent(user).unwrap();
    assert!(!a.chat.draft);
    assert_eq!(a.chat.parent, Some(root));
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(reply).unwrap())
            .unwrap()
            .chat
            .parent,
        Some(draft)
    );
    h.app.board_undo();
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(draft).unwrap())
            .unwrap()
            .chat
            .draft
    );
}

#[test]
fn pasted_chat_train_forks_source_and_undo_removes_cards() {
    let mut h = agent_board("train_fork_paste");
    h.app.ai.config.workspace_dir = Some(h.base.join("ai-ws"));
    std::fs::create_dir_all(h.app.ai.config.workspace_dir.as_ref().unwrap()).unwrap();
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "local");
    let session = slate_doc::scene::new_agent_session_id();
    let dir = atlas_ai::agent::agent_dir(h.app.ai.config.workspace_dir.as_ref().unwrap(), &session);
    std::fs::create_dir_all(&dir).unwrap();
    let turns = vec![
        atlas_ai::agent::AgentTurn {
            role: "user".into(),
            text: "hello".into(),
            at: 0,
        },
        atlas_ai::agent::AgentTurn {
            role: "assistant".into(),
            text: "hi".into(),
            at: 1,
        },
    ];
    let state = atlas_ai::agent::AgentSession {
        usage: None,
        approval: None,
        conversation: "provider-1".into(),
        artifacts: vec![],
        status: atlas_ai::agent::AgentStatus::Idle,
        provider: "local".into(),
        turns: turns.clone(),
        updated_at: 0,
        bundle: Default::default(),
        request: String::new(),
    };
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &state).unwrap();
    let manifest = slate_doc::SourceUri {
        locator: super::super::super::board_portal::source_locator(None, &dir.join("session.json")),
    };
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.session = session.clone();
            a.channel = Some("provider-1".into());
            a.bundle = Some(manifest.clone());
            a.chat.train = true;
            a.chat.detail = slate_doc::agent_chat::Detail::Pair;
            p.title = "Courtyard".into();
        }
    });
    let original_session = session;
    h.app.board_sel.clear();
    h.app.board_sel.insert(root);
    assert_eq!(h.app.board_copy(&h.ctx), 1);
    h.app.board_sel.clear();
    let before = h.app.doc().scene.nodes.len();
    assert_eq!(
        h.app.board_paste(None, Some(egui::Pos2::new(900.0, 0.0))),
        1
    );
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
    let pasted = *h.app.board_sel.iter().next().unwrap();
    let forked = slate_doc::agent_chat::agent(h.app.doc().scene.node(pasted).unwrap()).unwrap();
    assert_ne!(forked.session, original_session);
    assert!(forked.channel.is_none());
    assert!(forked
        .bundle
        .as_ref()
        .is_some_and(|b| b.locator.contains("session.json")));
    let NodeKind::Portal(p) = &h.app.doc().scene.node(pasted).unwrap().kind else {
        panic!("portal")
    };
    assert!(p.title.contains("Forked from Courtyard"));
    let original_turns = std::fs::read_to_string(dir.join("session.json")).unwrap();
    assert!(original_turns.contains("provider-1"));
    assert_ne!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(root).unwrap())
            .unwrap()
            .session,
        forked.session
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), before);
}

#[test]
fn train_composer_grows_with_the_message_instead_of_a_tall_empty_card() {
    let mut h = agent_board("train_composer_fit");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    *h.app.agents.prompt_mut(id) = "Hello".into();
    h.app.agents.prompt_epoch = h.app.agents.prompt_epoch.wrapping_add(1);
    h.app.fit_agent_cards(&h.ctx);
    let short = h.app.doc().scene.node(id).unwrap().rect.h;
    assert!(
        short < 120.0,
        "a short train message should sit in a short card, height {short}"
    );
    *h.app.agents.prompt_mut(id) = (0..24)
        .map(|i| format!("line {i} wraps across the card"))
        .collect::<Vec<_>>()
        .join("\n");
    h.app.agents.prompt_epoch = h.app.agents.prompt_epoch.wrapping_add(1);
    h.app.fit_agent_cards(&h.ctx);
    let long = h.app.doc().scene.node(id).unwrap().rect.h;
    assert!(
        long > short + 80.0,
        "the card should grow with the message ({short} -> {long})"
    );
}
