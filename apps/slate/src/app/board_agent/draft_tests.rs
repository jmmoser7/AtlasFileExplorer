//! Drafts across presentation switches.

use super::test_support::*;
use super::*;

const DRAFT_TEXT: &str = "And the benches?";

/// The unsent draft sits at the tail of the main line in `shape`: its own
/// draft card in line with the train, or the composer of the tail card.
fn assert_draft_joined(
    h: &super::super::tests::Harness,
    root: NodeId,
    draft: NodeId,
    shape: Shape,
    as_card: bool,
) {
    use slate_doc::agent_chat::{agent, visible_parent, Detail};
    let scene = &h.app.doc().scene;
    let tail = main_tail(h, root);
    if !as_card {
        assert!(
            scene.node(draft).is_none(),
            "{shape:?}: the draft card joined the tail"
        );
        assert_eq!(
            h.app.agents.prompts.get(&tail).map(String::as_str),
            Some(DRAFT_TEXT),
            "{shape:?}: the tail's composer holds the unsent text"
        );
        assert!(
            !h.app.agent_has_child(tail),
            "{shape:?}: the tail shows its composer"
        );
        return;
    }
    let n = scene.node(draft).expect("the draft card stays");
    let a = agent(n).unwrap();
    assert!(
        a.chat.draft && !n.hidden,
        "{shape:?}: still an unsent draft"
    );
    assert_eq!(
        visible_parent(scene, n),
        Some(tail),
        "{shape:?}: hangs from the tail"
    );
    let want = match shape {
        Shape::Pairs => (true, Detail::Pair),
        Shape::Train => (true, Detail::Summary),
        Shape::Window => (false, Detail::Full),
    };
    assert_eq!(
        (a.chat.train, a.chat.detail),
        want,
        "{shape:?}: drawn in the new form"
    );
    assert_eq!(
        h.app.agents.prompts.get(&draft).map(String::as_str),
        Some(DRAFT_TEXT),
        "{shape:?}: the unsent text is kept"
    );
}

#[test]
fn an_unsent_draft_joins_every_presentation_switch() {
    let (mut h, ids) = branched_pairs("mode_draft_joins");
    let root = ids[0];
    h.app.board_sel = std::iter::once(ids[2]).collect();
    assert!(h.app.agent_spawn_command(None));
    let draft = *h.app.board_sel.iter().next().unwrap();
    *h.app.agents.prompt_mut(draft) = DRAFT_TEXT.into();
    settle(&mut h);
    assert_projected_with(&h, root, Shape::Pairs, Some(draft));
    assert_draft_joined(&h, root, draft, Shape::Pairs, true);

    let card = main_tail(&h, root);
    switch(&mut h, card, Shape::Train);
    assert_projected_with(&h, root, Shape::Train, Some(draft));
    assert_draft_joined(&h, root, draft, Shape::Train, true);

    let before = h.app.doc().scene.nodes.clone();
    let card = main_tail(&h, root);
    switch(&mut h, card, Shape::Window);
    assert_projected(&h, root, Shape::Window);
    assert_draft_joined(&h, root, draft, Shape::Window, false);
    h.app.board_undo();
    assert_eq!(
        h.app.doc().scene.nodes,
        before,
        "one Undo restores the train and the draft's place"
    );
    settle(&mut h);
    assert_draft_joined(&h, root, draft, Shape::Train, true);
    h.app.board_redo();
    settle(&mut h);
    assert_draft_joined(&h, root, draft, Shape::Window, false);

    let before = h.app.doc().scene.nodes.clone();
    let card = main_tail(&h, root);
    switch(&mut h, card, Shape::Pairs);
    assert_projected(&h, root, Shape::Pairs);
    assert_draft_joined(&h, root, draft, Shape::Pairs, false);
    h.app.board_undo();
    assert_eq!(
        h.app.doc().scene.nodes,
        before,
        "one Undo restores the single window"
    );
    assert!(
        h.app.agents.dispatched.is_empty(),
        "the draft was never sent"
    );
}

/// Two unsent drafts in flight, one on each branch, through two
/// presentation switches and Undo/Redo across both: each draft's text
/// is kept exactly once, on its own draft card or on its own branch's
/// window, and nothing is sent.
#[test]
fn two_drafts_keep_their_text_through_switches_and_undo_redo() {
    const FORK_TEXT: &str = "And the fountain?";
    type H = super::super::tests::Harness;
    let (mut h, ids) = branched_pairs("mode_two_drafts");
    let root = ids[0];
    let spawn = |h: &mut H, from: NodeId, text: &str| {
        h.app.board_sel = std::iter::once(from).collect();
        assert!(h.app.agent_spawn_command(None));
        let draft = *h.app.board_sel.iter().next().unwrap();
        *h.app.agents.prompt_mut(draft) = text.into();
        draft
    };
    let main = spawn(&mut h, ids[2], DRAFT_TEXT);
    let fork = spawn(&mut h, ids[4], FORK_TEXT);
    settle(&mut h);
    let check = |h: &H, as_cards: bool, when: &str| {
        for (draft, text, tail) in [(main, DRAFT_TEXT, MAIN[5]), (fork, FORK_TEXT, FORK[5])] {
            let held: Vec<NodeId> = h
                .app
                .agents
                .prompts
                .iter()
                .filter(|(_, t)| t.as_str() == text)
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(held.len(), 1, "{when}: {text:?} is kept once: {held:?}");
            let at = held[0];
            let scene = &h.app.doc().scene;
            assert!(
                scene.node(at).is_some_and(|n| !n.hidden),
                "{when}: {text:?} is on a card on the board"
            );
            if as_cards {
                assert_eq!(at, draft, "{when}: the draft card keeps {text:?}");
            } else {
                assert!(scene.node(draft).is_none(), "{when}: the draft joined");
                assert_eq!(
                    shown(h, at).last().map(String::as_str),
                    Some(tail),
                    "{when}: {text:?} is on its own branch's tail"
                );
            }
        }
        assert!(h.app.agents.dispatched.is_empty(), "{when}: nothing sent");
    };
    let undo = |h: &mut H| {
        h.app.board_undo();
        settle(h);
    };
    let redo = |h: &mut H| {
        h.app.board_redo();
        settle(h);
    };
    check(&h, true, "pairs");
    let card = main_tail(&h, root);
    switch(&mut h, card, Shape::Train);
    check(&h, true, "train");
    let card = main_tail(&h, root);
    switch(&mut h, card, Shape::Window);
    check(&h, false, "window");
    undo(&mut h);
    check(&h, true, "undo to train");
    redo(&mut h);
    check(&h, false, "redo to window");
    let card = main_tail(&h, root);
    switch(&mut h, card, Shape::Pairs);
    check(&h, false, "window to pairs");
    undo(&mut h);
    check(&h, false, "undo to window");
    undo(&mut h);
    check(&h, true, "undo to train again");
    undo(&mut h);
    check(&h, true, "undo to the first pairs");
    redo(&mut h);
    check(&h, true, "redo to train");
    redo(&mut h);
    check(&h, false, "redo to window");
    redo(&mut h);
    check(&h, false, "redo to pairs");
}

#[test]
fn a_draft_card_switches_presentation_from_its_own_selection() {
    let (mut h, ids) = branched_pairs("mode_draft_selected");
    let root = ids[0];
    h.app.board_sel = std::iter::once(ids[2]).collect();
    assert!(h.app.agent_spawn_command(None));
    let draft = *h.app.board_sel.iter().next().unwrap();
    *h.app.agents.prompt_mut(draft) = DRAFT_TEXT.into();
    settle(&mut h);
    switch(&mut h, draft, Shape::Window);
    assert_projected(&h, root, Shape::Window);
    assert_draft_joined(&h, root, draft, Shape::Window, false);
}

#[test]
fn a_wire_on_a_merged_message_follows_the_card_that_shows_it() {
    use slate_doc::scene::ConnectorEnd;
    let (mut h, ids) = branched_pairs("mode_wire_follows");
    let root = ids[0];
    switch(&mut h, ids[2], Shape::Train);
    let asks = slate_doc::agent_chat::conversation(&h.app.doc().scene, root)
        .into_iter()
        .find(|id| shown(&h, *id) == ["Which trees?"])
        .unwrap();
    let noted = note(&mut h, Pos2::new(0.0, 900.0), "Trees");
    let noted = h.app.doc().scene.node(noted).unwrap().clone();
    let wire = h.app.provenance_wire(asks, &noted, true);
    let wire = h.app.add_nodes(vec![wire])[0];
    settle(&mut h);
    let anchored = |h: &super::super::tests::Harness| {
        let NodeKind::Connector(c) = &h.app.doc().scene.node(wire).unwrap().kind else {
            panic!("the wire stays a wire");
        };
        match c.a {
            ConnectorEnd::Anchored { node, .. } => node,
            ConnectorEnd::Free { .. } => panic!("the wire stays anchored"),
        }
    };
    switch(&mut h, ids[2], Shape::Pairs);
    assert_projected(&h, root, Shape::Pairs);
    assert!(
        h.app.doc().scene.node(asks).is_none(),
        "the one-message card merged"
    );
    assert_eq!(
        shown(&h, anchored(&h)),
        ["Which trees?", "Two plane trees."],
        "the wire follows the card that shows its message"
    );
    h.app.board_undo();
    assert_eq!(anchored(&h), asks, "one Undo restores the wire's card");
}

#[test]
fn the_menu_offers_every_other_presentation_from_each_one() {
    use slate_doc::agent_chat::{ChatView, Detail};
    let pairs = ChatView {
        train: true,
        detail: Detail::Pair,
        ..Default::default()
    };
    let train = ChatView {
        train: true,
        detail: Detail::Summary,
        ..Default::default()
    };
    let window = ChatView {
        train: false,
        detail: Detail::Full,
        ..Default::default()
    };
    let offered = |chat: &ChatView| -> Vec<&'static str> {
        agent_presentations(chat, false)
            .into_iter()
            .filter(|(_, shown)| *shown)
            .map(|(command, _)| command)
            .collect()
    };
    assert_eq!(offered(&pairs), ["portal.agent.chat", "portal.agent.train"]);
    assert_eq!(offered(&train), ["portal.agent.chat", "portal.agent.pairs"]);
    assert_eq!(
        offered(&window),
        ["portal.agent.train", "portal.agent.pairs"]
    );
    assert!(
        agent_presentations(&pairs, true)
            .iter()
            .all(|(_, shown)| !shown),
        "never while a reply streams"
    );
}
