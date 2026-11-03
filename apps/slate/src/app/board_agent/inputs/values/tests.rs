use super::*;
use crate::app::board_agent::test_support::{generator_with_note, note};

fn equivalent(app: &SlateApp, target: NodeId) {
    app.legacy_input_projection_for_test(true);
    let eager = app.agent_input_snapshot(target);
    app.legacy_input_projection_for_test(false);
    let lazy = app.agent_input_snapshot(target);
    assert_eq!(
        serde_json::to_value(eager).unwrap(),
        serde_json::to_value(lazy).unwrap()
    );
}

#[test]
fn unwired_nodes_are_never_projected_and_snapshot_matches_eager_control() {
    let (mut h, target, _) = generator_with_note("lazy-projection-scope", "Wired instruction");
    for i in 0..80 {
        note(
            &mut h,
            Pos2::new(5000.0, i as f32 * 200.0),
            "Unrelated authored content",
        );
    }
    equivalent(&h.app, target);
    PROJECTED_NODES.with(|count| count.set(0));
    h.app.agent_input_snapshot(target).unwrap();
    assert_eq!(
        PROJECTED_NODES.with(std::cell::Cell::get),
        1,
        "only the actual wired source is projected"
    );
    h.app.legacy_input_projection_for_test(true);
    PROJECTED_NODES.with(|count| count.set(0));
    h.app.agent_input_snapshot(target).unwrap();
    let eager_nodes = PROJECTED_NODES.with(std::cell::Cell::get);
    h.app.legacy_input_projection_for_test(false);
    assert!(eager_nodes > 80, "control exercises whole-board projection");
}

#[test]
fn live_edits_and_journal_changes_are_read_without_cached_projection() {
    let (mut h, target, source) = generator_with_note("lazy-projection-edits", "Original source");
    let captured = h.app.agent_input_snapshot(target).unwrap();
    h.app.text_edit = Some((source, "Live Unicode source 🦀".into()));
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Live Unicode source 🦀"
    );
    h.app.text_edit = None;
    h.app.patch_nodes(&[source], |node| {
        if let NodeKind::Text(text) = &mut node.kind {
            text.text = "Committed change".into();
        }
    });
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Committed change"
    );
    assert_eq!(
        captured.wired[0].text, "Original source",
        "an earlier run's immutable input stays intact"
    );
}

#[test]
fn new_session_output_is_visible_without_a_scene_edit() {
    let (mut h, target, source) = generator_with_note("lazy-projection-session", "");
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
        .unwrap()
        .clone();
    h.app.patch_nodes(&[source], |node| {
        if let NodeKind::Text(text) = &mut node.kind {
            text.agent = Some(Box::new(agent.clone()));
        }
    });
    let mut session = AgentSession {
        turns: vec![AgentTurn {
            role: "assistant".into(),
            text: "First response".into(),
            at: 1,
        }],
        ..Default::default()
    };
    h.app.set_agent_session_for_test(source, session.clone());
    let scene = h.app.scene_gen;
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "First response"
    );
    session.turns[0].text = "Updated response".into();
    h.app.set_agent_session_for_test(source, session);
    assert_eq!(h.app.scene_gen, scene);
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Updated response"
    );
}
