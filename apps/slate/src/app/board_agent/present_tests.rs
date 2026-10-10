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
