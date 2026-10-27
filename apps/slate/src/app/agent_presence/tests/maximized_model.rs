//! Real presence-pump regression for maximized native model cards.
use super::*;

#[test]
fn maximized_native_model_pump_advances_board_read_and_scoped_inspection() {
    use crate::app::application_connector::native::test_support;
    let mut h = web_board("maximized_native_read");
    let (model, peer) = test_support::install(&mut h, atlas_connector::SourceMode::Linked);
    assert!(matches!(
        h.app.doc().scene.node(model).unwrap().kind,
        slate_doc::scene::NodeKind::Image(_)
    ));
    assert!(h.app.portal_maximize(model));
    let who = join(&mut h);
    let mut actor = h.app.presence.agents.remove(0);
    assert_eq!(actor.profile.id, who);
    begin_request(&mut actor);
    actor
        .grant
        .as_mut()
        .unwrap()
        .readable_sources
        .push("board-file-inputs".into());
    h.app
        .presence
        .roster
        .session_mut(&actor.motion)
        .unwrap()
        .begin_request(id(&actor.request).unwrap())
        .unwrap();
    let read = call(
        &actor,
        0,
        BoardOperation::Read {
            node_ids: vec![model.0.to_string()],
            offset: 0,
            limit: 1,
        },
    );
    h.app.begin_presence_read(&mut actor, &read).unwrap();
    h.app.presence.agents.insert(0, actor);
    let pump = |h: &mut crate::app::tests::Harness| {
        h.app.presence.last_tick = Some(Instant::now() - Duration::from_secs(1));
        h.app.presence_pump(&h.ctx);
    };
    for _ in 0..8 {
        pump(&mut h);
    }
    assert!(
        h.app.presence.agents[0].read.is_none(),
        "maximized Image-model read must finish through the real presence pump"
    );
    assert_ne!(h.app.presence.agents[0].activity, "error");
    let mut actor = h.app.presence.agents.remove(0);
    let inspect = call(
        &actor,
        1,
        BoardOperation::Invoke {
            command: "board.model.connector.inspect".into(),
            targets: vec![model.0.to_string()],
            arguments: json!({}),
        },
    );
    assert!(h
        .app
        .begin_presence_portal_call(Some(&h.ctx), &mut actor, &inspect)
        .unwrap()
        .is_none());
    h.app.presence.agents.insert(0, actor);
    let mut dispatched = None;
    for _ in 0..8 {
        pump(&mut h);
        if let Some(request) = peer.next() {
            dispatched = Some(request);
            break;
        }
    }
    assert!(dispatched.is_some(), "same-source native inspection must reach its existing authenticated worker while maximized");
    assert_eq!(h.app.portal_chrome.maximized, Some(model));
}

#[test]
fn maximized_native_model_keeps_ordinary_board_witness_waiting_until_restore() {
    use crate::app::agent_presence::tests::proposal_support::{begin_command, tick};
    use crate::app::application_connector::native::test_support;
    let mut h = web_board("maximized_native_board_witness");
    let (model, _peer) = test_support::install(&mut h, atlas_connector::SourceMode::Linked);
    let who = join(&mut h);
    let before = h.app.doc().scene.node(model).unwrap().rect;
    begin_command(
        &mut h,
        &who,
        "board.objects.move",
        &[model],
        json!({"dx":100,"dy":0}),
    );
    assert!(h.app.presence.agents[0].work.is_some());
    assert!(h.app.portal_maximize(model));
    for _ in 0..32 {
        tick(&mut h);
    }
    let work = h.app.presence.agents[0]
        .work
        .as_ref()
        .expect("covered board work must remain pending");
    assert!(!work.started && work.index == 0);
    assert_eq!(h.app.doc().scene.node(model).unwrap().rect, before);
    assert!(h.app.presence.proposals.is_empty());
    assert!(h.app.portal_restore());
    for _ in 0..80 {
        tick(&mut h);
        if h.app.presence.agents[0].work.is_none() {
            break;
        }
    }
    assert!(
        h.app.presence.agents[0].work.is_none(),
        "the same board witness resumes after restore"
    );
    assert!(!h.app.presence.proposals.is_empty());
}
