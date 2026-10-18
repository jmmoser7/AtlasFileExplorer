//! One bounded synthetic session write/expect/observe mechanism for app tests.
use crate::app::tests::Harness;
use atlas_agent::AgentSession;
use std::time::Instant;
pub(in crate::app::agent_presence) fn publish_and_observe(
    h: &mut Harness,
    who: &str,
    session: &AgentSession,
) {
    publish_and_observe_until(h, who, session, |h| {
        h.app
            .presence
            .agents
            .iter()
            .any(|a| a.profile.id == who && a.snapshot.as_deref().is_some_and(|s| s == session))
    });
}

pub(in crate::app::agent_presence) fn publish_and_observe_until(
    h: &mut Harness,
    who: &str,
    session: &AgentSession,
    observed: impl Fn(&Harness) -> bool,
) {
    let dir = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == who)
        .unwrap()
        .dir
        .clone();
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &session).unwrap();
    h.app.presence.sources.expect(&dir, &session.request);
    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    loop {
        h.app.presence_pump(&h.ctx);
        if observed(h) {
            break;
        }
        assert!(Instant::now() < deadline, "helper session was not observed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
