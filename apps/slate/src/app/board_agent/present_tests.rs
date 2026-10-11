//! Stop folder and presentation switches.

use super::test_support::*;
use super::*;

#[test]
fn stop_never_recreates_a_missing_bundle_folder() {
    let (mut h, tail, cancel) = streaming_tail("stop_missing_bundle");
    let at = output_circle(&h, tail);
    let gone = cancel.parent().unwrap().with_file_name("moved-bundle");
    let _ = std::fs::remove_dir_all(&gone);
    bundle_at(&mut h, tail, &gone);
    press(&mut h, at);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !gone.exists(),
        "Stop does not recreate a bundle folder that moved"
    );
    assert!(
        h.app
            .toasts
            .iter()
            .any(|(m, _)| m.contains("folder has moved")),
        "the user is told the run was not stopped"
    );
}

#[test]
fn stop_writes_into_a_bundled_card_folder() {
    let (mut h, tail, cancel) = streaming_tail("stop_bundle");
    let at = output_circle(&h, tail);
    let bundle = cancel.parent().unwrap().with_file_name("bundle");
    let _ = std::fs::remove_dir_all(&bundle);
    std::fs::create_dir_all(&bundle).unwrap();
    bundle_at(&mut h, tail, &bundle);
    press(&mut h, at);
    assert!(
        stop_requested(&bundle.join("cancel.json")),
        "Stop reaches a bundled card's own folder"
    );
}

#[test]
fn after_the_reply_the_output_circle_continues_the_train_again() {
    let (mut h, tail, cancel) = streaming_tail("stop_then_output");
    h.app.agents.awaiting.remove(&tail);
    h.frame();
    let at = output_circle(&h, tail);
    let out = h.frame_output(|_| {});
    assert!(
        stop_square_at(&out, at).is_none(),
        "idle cards show no Stop"
    );
    assert_eq!(h.app.agent_output_at(at, &h.app.board_xf()), Some(tail));
    press(&mut h, at);
    assert!(!cancel.exists(), "an idle output circle never stops");
    let draft = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| {
            slate_doc::agent_chat::agent(n)
                .is_some_and(|a| a.chat.draft && a.chat.parent == Some(tail))
        })
        .map(|n| n.id);
    assert!(draft.is_some(), "the output circle placed the next draft");
}

#[test]
fn a_streaming_output_circle_is_not_a_continuation_grip() {
    let (h, tail, _) = streaming_tail("stop_not_grip");
    let at = output_circle(&h, tail);
    assert_eq!(h.app.agent_output_at(at, &h.app.board_xf()), None);
}

/// Switch from the conversation's current shape (reached through `path`)
/// to `to`, check it, then check that one Undo restores the scene.
fn check_transition(tag: &str, path: &[Shape], to: Shape) {
    let (mut h, ids) = branched_pairs(tag);
    let root = ids[0];
    assert_projected(&h, root, Shape::Pairs);
    let tail = |h: &super::super::tests::Harness| {
        slate_doc::agent_chat::conversation(&h.app.doc().scene, root)
            .into_iter()
            .rev()
            .find(|id| !h.app.doc().scene.node(*id).unwrap().hidden)
            .unwrap()
    };
    for step in path {
        let card = tail(&h);
        switch(&mut h, card, *step);
        assert_projected(&h, root, *step);
    }
    let before = h.app.doc().scene.nodes.clone();
    let card = tail(&h);
    switch(&mut h, card, to);
    assert_projected(&h, root, to);
    h.app.board_undo();
    assert_eq!(
        h.app.doc().scene.nodes,
        before,
        "one Undo restores the previous presentation"
    );
}

#[test]
fn pairs_become_one_message_per_card_mid_conversation() {
    check_transition("mode_pairs_train", &[], Shape::Train);
}

#[test]
fn one_message_per_card_becomes_pairs_mid_conversation() {
    check_transition("mode_train_pairs", &[Shape::Train], Shape::Pairs);
}

#[test]
fn pairs_become_single_chat_windows_mid_conversation() {
    check_transition("mode_pairs_window", &[], Shape::Window);
}

#[test]
fn single_chat_windows_become_pairs_mid_conversation() {
    check_transition("mode_window_pairs", &[Shape::Window], Shape::Pairs);
}

#[test]
fn single_chat_windows_become_one_message_per_card_mid_conversation() {
    check_transition("mode_window_train", &[Shape::Window], Shape::Train);
}

#[test]
fn one_message_per_card_becomes_single_chat_windows_mid_conversation() {
    check_transition("mode_train_window", &[Shape::Train], Shape::Window);
}

#[test]
fn a_long_round_trip_through_every_mode_keeps_the_whole_conversation() {
    check_transition(
        "mode_round_trip",
        &[
            Shape::Train,
            Shape::Window,
            Shape::Pairs,
            Shape::Window,
            Shape::Train,
        ],
        Shape::Pairs,
    );
}

fn ws(h: &mut super::super::tests::Harness) -> std::path::PathBuf {
    let dir = h.base.join("ai-ws");
    std::fs::create_dir_all(&dir).unwrap();
    h.app.ai.config.workspace_dir = Some(dir.clone());
    dir
}

fn sid(h: &super::super::tests::Harness, id: NodeId) -> String {
    h.app.agent_session_for(id).unwrap().0
}

fn kids(h: &super::super::tests::Harness, parent: NodeId) -> Vec<NodeId> {
    h.app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| chat(h, n.id).parent == Some(parent))
        .map(|n| n.id)
        .collect()
}

fn idle(turns: Vec<AgentTurn>) -> AgentSession {
    AgentSession {
        usage: None,
        approval: None,
        conversation: String::new(),
        artifacts: vec![],
        status: AgentStatus::Idle,
        provider: "cursor".into(),
        turns,
        updated_at: 1,
        bundle: Default::default(),
        request: String::new(),
    }
}

fn read_req(path: &std::path::Path) -> AgentRequest {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !path.is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "missing {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Ancestor pair plus a sibling line, sharing one Cursor session.
fn branched(tag: &str) -> (super::super::tests::Harness, NodeId, NodeId) {
    let mut h = board(tag);
    ws(&mut h);
    let root = train(&mut h, Pos2::ZERO, "cursor");
    let mut all = turns_of(&["Plan the courtyard?", "Start with the trees."]);
    all.extend(turns_of(&["Pave it instead?", "Granite setts."]));
    h.app.agents.local_turns.insert(root, all.clone());
    h.app.patch_nodes(&[root], |n| {
        slate_doc::agent_chat::agent_mut(n).unwrap().chat.end = Some(2);
    });
    let sibling = next_card(&mut h, root);
    h.app.agents.local_turns.insert(sibling, all);
    (h, root, sibling)
}

fn has(h: &super::super::tests::Harness, id: NodeId, text: &str) -> bool {
    h.app.agent_all_turns(id).iter().any(|t| t.text == text)
}

#[test]
fn forking_a_linear_chat_yields_a_distinct_session() {
    let mut h = board("linear_fork_session");
    ws(&mut h);
    let root = train(&mut h, Pos2::ZERO, "cursor");
    let original = sid(&h, root);
    h.app.board_sel.insert(root);
    assert!(h.app.agent_fork_selected(None));
    let fork = kids(&h, root)[0];
    assert_ne!(sid(&h, fork), original);
    assert_eq!(sid(&h, root), original);
    assert!(chat(&h, fork).linear && chat(&h, fork).draft);
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(fork).unwrap()).unwrap();
    assert!(agent.channel.is_none());
    h.app.board_undo();
    assert!(kids(&h, root).is_empty());
}

#[test]
fn continuing_the_unbranched_tail_keeps_the_linear_session() {
    let mut h = board("linear_tail_session");
    let dir = ws(&mut h);
    let root = train(&mut h, Pos2::ZERO, "cursor");
    h.app.patch_nodes(&[root], |n| {
        slate_doc::agent_chat::agent_mut(n).unwrap().chat.detail =
            slate_doc::agent_chat::Detail::Summary;
    });
    let original = sid(&h, root);
    *h.app.agents.prompt_mut(root) = "Where?".into();
    let sent = h.app.send_agent_prompt(root).unwrap();
    assert_ne!(sent, root);
    assert_eq!(sid(&h, root), original);
    assert_eq!(sid(&h, sent), original);
    let req = read_req(&atlas_ai::agent::agent_dir(&dir, &original).join("request.json"));
    assert!(req.history.is_empty(), "the tail continues the live stream");
}

#[test]
fn a_reply_on_one_linear_branch_never_lands_on_its_sibling() {
    let (mut h, root, sibling) = branched("linear_reply_routing");
    let dir = h.app.ai.config.workspace_dir.clone().unwrap();
    h.app.board_sel.insert(root);
    assert!(h.app.agent_fork_selected(None));
    let fork = kids(&h, root)
        .into_iter()
        .find(|id| *id != sibling)
        .unwrap();
    assert_ne!(
        h.app.agent_link_dir(fork, &dir).unwrap(),
        h.app.agent_link_dir(sibling, &dir).unwrap()
    );
    *h.app.agents.prompt_mut(fork) = "branch question".into();
    let sent = h.app.send_agent_prompt(fork).unwrap();
    let sent_dir = h.app.agent_link_dir(sent, &dir).unwrap();
    assert_eq!(sent_dir, atlas_ai::agent::agent_dir(&dir, &sid(&h, sent)));
    assert!(!has(&h, sibling, "branch question"));
    let mut turns = h.app.agent_all_turns(sent);
    turns.push(AgentTurn {
        role: "assistant".into(),
        text: "only-on-fork".into(),
        at: 9,
    });
    atlas_ai::agent::atomic_write_json(&sent_dir.join("session.json"), &idle(turns)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !has(&h, sent, "only-on-fork") {
        assert!(
            std::time::Instant::now() < deadline,
            "the reply never arrived"
        );
        h.frame();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!has(&h, sibling, "only-on-fork"));
    assert!(!has(&h, root, "only-on-fork"));
}

#[test]
fn a_linear_fork_is_seeded_with_ancestors_and_not_its_sibling() {
    let (mut h, root, sibling) = branched("linear_fork_seed");
    let dir = h.app.ai.config.workspace_dir.clone().unwrap();
    *h.app.agents.prompt_mut(root) = "Use plane trees?".into();
    let sent = h
        .app
        .send_agent_prompt(root)
        .expect("an earlier card forks");
    assert_ne!(sid(&h, sent), sid(&h, sibling));
    let req = read_req(
        &h.app
            .agent_link_dir(sent, &dir)
            .unwrap()
            .join("request.json"),
    );
    let seeded: Vec<_> = req.history.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(seeded, ["Plan the courtyard?", "Start with the trees."]);
}

#[test]
fn pasting_a_linear_card_forks_a_session_seeded_to_its_checkpoint() {
    let (mut h, root, sibling) = branched("linear_paste_fork");
    let dir = h.app.ai.config.workspace_dir.clone().unwrap();
    let original = sid(&h, root);
    let folder = atlas_ai::agent::agent_dir(&dir, &original);
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("session.json");
    atlas_ai::agent::atomic_write_json(&file, &idle(h.app.agent_all_turns(root))).unwrap();
    let locator = super::super::board_portal::source_locator(None, &file);
    for id in [root, sibling] {
        h.app.patch_nodes(&[id], |n| {
            slate_doc::agent_chat::agent_mut(n).unwrap().bundle = Some(slate_doc::SourceUri {
                locator: locator.clone(),
            });
        });
    }
    h.app.board_sel = [sibling].into_iter().collect();
    assert_eq!(h.app.board_copy(&h.ctx), 1);
    h.app.board_sel.clear();
    assert_eq!(h.app.board_paste(None, Some(Pos2::new(900.0, 240.0))), 1);
    let pasted = *h.app.board_sel.iter().next().unwrap();
    assert_ne!(sid(&h, pasted), original);
    let saved: AgentSession = serde_json::from_str(
        &std::fs::read_to_string(
            h.app
                .agent_link_dir(pasted, &dir)
                .unwrap()
                .join("session.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved
            .turns
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>(),
        ["Plan the courtyard?", "Start with the trees."]
    );
}
