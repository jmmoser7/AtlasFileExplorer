//! Ordinary real app dispatch and pump; no provider is launched by these tests.
use super::*;
use std::time::Duration;

pub(super) fn tick(h: &mut Harness) {
    h.app.presence.last_tick = Some(Instant::now() - Duration::from_millis(50));
    h.app.presence_pump(&h.ctx);
}

pub(super) fn begin_command(
    h: &mut Harness,
    who: &str,
    command: &str,
    targets: &[slate_doc::NodeId],
    arguments: serde_json::Value,
) -> BoardCall {
    let index = h
        .app
        .presence
        .agents
        .iter()
        .position(|a| a.profile.id == who)
        .unwrap();
    let mut a = h.app.presence.agents.remove(index);
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
    assert!(!a.paused && !a.grant.as_ref().unwrap().mutation_autonomy);
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: a.calls as u64,
        operation: BoardOperation::Invoke {
            command: command.into(),
            targets: targets.iter().map(|id| id.0.to_string()).collect(),
            arguments,
        },
    };
    assert!(
        h.app.presence_call(&mut a, &call).is_none(),
        "real call admitted"
    );
    h.app.presence.agents.insert(index, a);
    call
}

pub(super) fn stage_command(
    h: &mut Harness,
    who: &str,
    command: &str,
    targets: &[slate_doc::NodeId],
    arguments: serde_json::Value,
) -> String {
    let call = begin_command(h, who, command, targets, arguments);
    let index = h
        .app
        .presence
        .agents
        .iter()
        .position(|a| a.profile.id == who)
        .unwrap();
    for _ in 0..160 {
        tick(h);
        if h.app.presence.agents[index].work.is_none() {
            break;
        }
    }
    assert!(h.app.presence.agents[index].work.is_none());
    h.app
        .presence
        .proposals
        .iter()
        .find(|(_, p)| p.call.id == call.id)
        .expect("exact prepared context survives staging")
        .0
        .clone()
}

pub(super) fn finish(h: &mut Harness, who: &str) {
    for _ in 0..160 {
        tick(h);
        if h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap()
            .work
            .is_none()
        {
            return;
        }
    }
    panic!("bounded approved witness did not finish");
}
